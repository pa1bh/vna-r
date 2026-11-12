//! The calibration panel and everything that turns a raw sweep into a corrected one:
//! standards, kit fit, calibration files, per-segment calibration.

use super::*;

#[derive(Default)]
pub(super) struct CalState {
    pub(super) set: Option<CalibrationSet>,
    pub(super) file: Option<PathBuf>,
    /// Calibration solved for a specific grid (cache).
    pub(super) solved: Option<(Vec<u64>, Calibration)>,
    pub(super) apply: bool,
    pub(super) pending: Option<Standard>,
    /// What last happened to the set: "measured open", "loaded", "saved", a
    /// fit result. Stays until the next action replaces it.
    pub(super) message: Option<String>,
    /// Why the set cannot be solved for the current grid, if it cannot.
    /// Separate from `message` so a status line survives the re-solve that
    /// follows every action.
    pub(super) solve_error: Option<String>,
    /// Plausibility warning about the last stored standard.
    pub(super) warning: Option<String>,
    /// Warning about interpolating this calibration onto the current sweep grid.
    pub(super) grid_warning: Option<String>,
    /// Lowest frequency used by the kit fit (text, parsed with parse_hz).
    pub(super) fit_f_min: String,
    pub(super) fit_as_delay: bool,
    /// Last fit result, not yet applied to the kit.
    pub(super) fit: Option<(OpenFit, OpenModel)>,
}

impl CalState {
    pub(super) fn fit_model(&self) -> OpenModel {
        if self.fit_as_delay {
            OpenModel::Delay
        } else {
            OpenModel::Capacitance
        }
    }
}

impl CalState {
    pub(super) fn calibration_for(&mut self, freqs: &[u64]) -> Option<&Calibration> {
        let Some(set) = &self.set else { return None };
        let fresh = matches!(&self.solved, Some((g, _)) if g.as_slice() == freqs);
        if !fresh {
            match set.solve_for(freqs) {
                Ok(cal) => {
                    self.solved = Some((freqs.to_vec(), cal));
                    self.solve_error = None;
                    self.grid_warning = join_warnings(set.interpolation_warnings(freqs));
                }
                Err(e) => {
                    self.solved = None;
                    self.grid_warning = None;
                    self.solve_error = Some(e.to_string());
                    return None;
                }
            }
        }
        self.solved.as_ref().map(|(_, c)| c)
    }
}

/// The warnings of [`CalibrationSet::interpolation_warnings`] as one text, or `None`.
fn join_warnings(warnings: Vec<String>) -> Option<String> {
    (!warnings.is_empty()).then(|| warnings.join("\n"))
}

/// A calibration file bound to an extra segment.
#[derive(Default)]
pub(super) struct SegCal {
    pub(super) set: Option<CalibrationSet>,
    pub(super) solved: Option<(Vec<u64>, Calibration)>,
    pub(super) error: Option<String>,
}

impl App {
    /// Build what is shown again from the last raw sweep (or opened file):
    /// after the calibration, its apply switch or the port extension changed.
    pub(super) fn reshow(&mut self) {
        if let Some(raw) = self.raw.clone() {
            let corrected = self.corrected(&raw);
            self.set_shown(corrected);
        }
    }

    /// Correct one segment with its own calibration file (loaded on first use).
    pub(super) fn seg_correct(&mut self, path: &Path, part: &SweepResult) -> SweepResult {
        let entry = self.seg_cals.entry(path.to_path_buf()).or_default();
        if entry.set.is_none() && entry.error.is_none() {
            match std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| CalibrationSet::from_json(&t).map_err(|e| e.to_string()))
            {
                Ok(set) => entry.set = Some(set),
                Err(e) => entry.error = Some(format!("{}: {e}", path.display())),
            }
        }
        let Some(set) = &entry.set else { return part.clone() };
        let fresh = matches!(&entry.solved, Some((g, _)) if g.as_slice() == part.freqs_hz.as_slice());
        if !fresh {
            match set.solve_for(&part.freqs_hz) {
                Ok(cal) => {
                    entry.solved = Some((part.freqs_hz.clone(), cal));
                    entry.error = join_warnings(set.interpolation_warnings(&part.freqs_hz)).map(|w| format!("{}: {w}", path.display()));
                }
                Err(e) => {
                    entry.solved = None;
                    entry.error = Some(format!("{}: {e}", path.display()));
                    return part.clone();
                }
            }
        }
        match &entry.solved {
            Some((_, cal)) => cal.apply(part).unwrap_or_else(|_| part.clone()),
            None => part.clone(),
        }
    }

    /// Is a calibration applied to everything shown?
    pub(super) fn is_calibrated(&self) -> bool {
        self.cal.apply && !self.seg_calibrated.is_empty() && self.seg_calibrated.iter().all(|c| *c)
    }

    /// Calibration applied to some segments but not all.
    pub(super) fn is_partially_calibrated(&self) -> bool {
        self.cal.apply && self.seg_calibrated.iter().any(|c| *c) && !self.seg_calibrated.iter().all(|c| *c)
    }

    /// CAL / CAL* / RAW tag.
    pub(super) fn cal_tag(&self) -> RichText {
        if self.file_trace.is_some() {
            RichText::new("FILE").color(Color32::from_rgb(120, 170, 220))
        } else if self.is_calibrated() {
            RichText::new("CAL").color(Color32::from_rgb(120, 200, 120))
        } else if self.is_partially_calibrated() {
            RichText::new("CAL*").color(Color32::from_rgb(230, 150, 80))
        } else {
            RichText::new("RAW").color(Color32::from_rgb(230, 150, 80))
        }
    }

    pub(super) fn corrected(&mut self, raw: &SweepResult) -> SweepResult {
        if !self.cal.apply || self.file_trace.is_some() {
            return raw.clone();
        }
        let ranges = SweepResult::segment_ranges(&self.last_plan);
        let segmented =
            ranges.len() > 1 && ranges.last().map_or(0, |r| r.end) == raw.freqs_hz.len() && self.last_plan_cals.len() == ranges.len();
        if !segmented {
            return match self.cal.calibration_for(&raw.freqs_hz) {
                Some(cal) => {
                    self.seg_calibrated = vec![true];
                    cal.apply(raw).unwrap_or_else(|_| raw.clone())
                }
                None => {
                    self.seg_calibrated = vec![false];
                    raw.clone()
                }
            };
        }
        let cals = self.last_plan_cals.clone();
        let mut parts = Vec::with_capacity(ranges.len());
        let mut flags = Vec::with_capacity(ranges.len());
        for (range, cal_path) in ranges.into_iter().zip(cals) {
            let part = raw.slice(range);
            let done = match cal_path {
                Some(p) => {
                    let d = self.seg_correct(&p, &part);
                    flags.push(self.seg_cals.get(&p).is_some_and(|c| c.solved.is_some()));
                    d
                }
                None => match self.cal.calibration_for(&part.freqs_hz) {
                    Some(cal) => {
                        flags.push(true);
                        cal.apply(&part).unwrap_or_else(|_| part.clone())
                    }
                    None => {
                        flags.push(false);
                        part.clone()
                    }
                },
            };
            parts.push(done);
        }
        self.seg_calibrated = flags;
        SweepResult::concat(&parts)
    }

    pub(super) fn store_standard(&mut self, std: Standard, sweep: &SweepResult, repeat: usize) {
        self.cal.warning = plausibility_warning(std, sweep);
        self.cal.fit = None;
        let needs_new = match &self.cal.set {
            Some(set) => set.freqs_hz != sweep.freqs_hz,
            None => true,
        };
        if needs_new {
            if self.cal.set.is_some() {
                self.cal.message = Some("grid changed: started a new calibration".into());
            }
            self.cal.set = Some(CalibrationSet::new(sweep.freqs_hz.clone(), CalKit::ideal()));
            self.cal.file = None;
        }
        if let Some(set) = &mut self.cal.set {
            match set.set(std, sweep) {
                Ok(()) => {
                    self.cal.message = Some(if repeat > 1 {
                        format!("measured {} (mean of {repeat} sweeps)", std.name())
                    } else {
                        format!("measured {}", std.name())
                    })
                }
                Err(e) => self.cal.message = Some(e.to_string()),
            }
        }
        self.cal.solved = None;
        self.reshow();
    }

    pub(super) fn calibration_panel(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        match &self.cal.set {
            Some(set) => {
                ui.label(format!(
                    "{} pts, {} → {}",
                    set.freqs_hz.len(),
                    format_hz(set.freqs_hz[0]),
                    format_hz(*set.freqs_hz.last().unwrap())
                ));
                let have: Vec<_> = Standard::ALL
                    .iter()
                    .chain(Standard::LINE.iter())
                    .filter(|s| set.has(**s))
                    .map(|s| s.name())
                    .collect();
                ui.label(format!(
                    "measured: {}",
                    if have.is_empty() { "—".to_string() } else { have.join(", ") }
                ));
                ui.label(format!(
                    "kit: {} (open C0 {:.1} fF, delay {:.2} ps)",
                    set.kit.name,
                    set.kit.open_c0() * 1e15,
                    set.kit.open.offset_delay_s * 1e12
                ));
                // Which standards are there, not a solve: this runs every frame.
                let missing: Vec<&str> = set.missing_standards().iter().map(|s| s.name()).collect();
                let status = if missing.is_empty() {
                    format!("ready for S11{}", if set.has(Standard::Through) { " + S21" } else { "" })
                } else {
                    format!("incomplete: still to measure {}", missing.join(", "))
                };
                ui.label(status);
                if let Some(f) = &self.cal.file {
                    ui.small(f.display().to_string());
                }
            }
            None => {
                ui.label("no calibration — measure standards on the current sweep grid");
            }
        }
        if let Some(m) = &self.cal.message {
            ui.small(RichText::new(m).color(Color32::from_rgb(200, 200, 120)));
        }
        if self.cal.apply && self.cal.set.is_some() {
            if let Some(e) = &self.cal.solve_error {
                ui.small(RichText::new(format!("not applied: {e}")).color(Color32::from_rgb(255, 150, 80)));
            }
        }
        if let Some(w) = &self.cal.warning {
            ui.small(RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(255, 150, 80)));
        }
        if self.cal.apply && self.cal.solved.is_some() {
            if let Some(w) = &self.cal.grid_warning {
                ui.small(RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(255, 150, 80)));
            }
        }

        self.standard_buttons(ui, &Standard::ALL, connected);
        ui.horizontal(|ui| {
            ui.label("Port extension");
            let changed = ui
                .add(
                    egui::DragValue::new(&mut self.port_ext_ps)
                        .range(-PORT_EXT_MAX_PS..=PORT_EXT_MAX_PS)
                        .speed(1.0)
                        .fixed_decimals(1)
                        .suffix(" ps"),
                )
                .on_hover_text(
                    "moves the reference plane outward by this one-way delay (S11 turns by twice, S21 by once): \
                     for an adapter or fixture behind the calibration plane. Read the delay off the TDR, \
                     or tune until a short at the new plane is a dot at the left of the Smith chart. 0 = off.",
                )
                .changed();
            if self.port_ext_ps != 0.0 {
                // One-way length at the TDR velocity factor, as a feel for the size.
                ui.small(format!(
                    "≈ {:.1} mm at VF {:.2}",
                    self.port_ext_ps * 1e-12 * C0 * self.tdr_vf * 1e3,
                    self.tdr_vf
                ));
                if ui.small_button("0").on_hover_text("switch the port extension off").clicked() {
                    self.port_ext_ps = 0.0;
                    self.reshow();
                }
            }
            if changed {
                self.reshow();
            }
        });
        ui.horizontal(|ui| {
            let changed = ui.checkbox(&mut self.cal.apply, "apply").changed();
            if changed {
                self.reshow();
            }
            if ui.button("Load…").clicked() {
                self.load_cal();
            }
            if ui.add_enabled(self.cal.set.is_some(), egui::Button::new("Save…")).clicked() {
                self.save_cal();
            }
            if ui
                .add_enabled(self.cal.set.is_some(), egui::Button::new("Clear"))
                .on_hover_text("forget the standards in this panel; calibration files of extra segments stay in use")
                .clicked()
            {
                self.cal = CalState {
                    apply: self.cal.apply,
                    fit_f_min: self.cal.fit_f_min.clone(),
                    fit_as_delay: self.cal.fit_as_delay,
                    ..Default::default()
                };
                // The shown data is raw again, so the CAL tag must go too.
                self.seg_calibrated.clear();
                if let Some(raw) = self.raw.clone() {
                    self.set_shown(raw);
                }
            }
        });
    }

    pub(super) fn standard_buttons(&mut self, ui: &mut egui::Ui, standards: &[Standard], connected: bool) {
        ui.horizontal_wrapped(|ui| {
            for &std in standards {
                let pending = self.cal.pending == Some(std);
                let label = if pending {
                    format!("{}…", std.name())
                } else {
                    std.name().to_string()
                };
                if ui
                    .add_enabled(connected && self.cal.pending.is_none(), egui::Button::new(label))
                    .clicked()
                {
                    if let Some(cfg) = self.current_config() {
                        // Display and calibration must share one grid. When
                        // Start/Stop/Points were edited but not applied, apply
                        // them first (as Apply sweep would), then measure.
                        if self.applied_primary.is_some_and(|p| p != cfg) {
                            if self.segment_plan().is_none() {
                                return;
                            }
                            self.start_sweep(self.running);
                        }
                        self.cal.pending = Some(std);
                        // The same averaging as the display, so a standard is
                        // not one noisy sweep under an averaged trace.
                        self.send(Cmd::Sweep {
                            cfgs: vec![cfg],
                            purpose: Purpose::Standard(std),
                            continuous: false,
                            repeat: self.sweep_avg.max(1),
                        });
                    }
                }
            }
        });
        if connected && self.grid_edited() {
            ui.small(
                RichText::new("Start/Stop/Points differ from the running sweep; measuring a standard applies them first.")
                    .color(Color32::from_rgb(230, 150, 80)),
            );
        }
    }

    /// The typed Start/Stop/Points differ from the sweep that was last applied.
    pub(super) fn grid_edited(&self) -> bool {
        let typed = parse_hz(&self.start)
            .and_then(|s| parse_hz(&self.stop).map(|e| (s, e)))
            .and_then(|(s, e)| SweepConfig::linear(s, e, self.points))
            .ok();
        matches!((typed, self.applied_primary), (Some(t), Some(a)) if t != a)
    }

    /// Kit fit from line-open / line-short measurements (see docs/03-calibratie.md §8).
    pub(super) fn kit_fit_panel(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        let have_lines = self
            .cal
            .set
            .as_ref()
            .is_some_and(|s| s.has(Standard::LineOpen) && s.has(Standard::LineShort));
        let solvable = self.cal.set.as_ref().is_some_and(|s| s.missing_standards().is_empty());
        ui.small("With open, short and load done: put a cable (≥ 50 cm) on port 1 and measure it with an open and a short at the far end. The ripple difference gives the open/short asymmetry of the kit.");
        self.standard_buttons(ui, &Standard::LINE, connected);
        ui.horizontal(|ui| {
            ui.label("from");
            ui.add(egui::TextEdit::singleline(&mut self.cal.fit_f_min).desired_width(60.0));
            ui.label("as");
            egui::ComboBox::from_id_salt("fit_model")
                .selected_text(if self.cal.fit_as_delay { "delay" } else { "capacitance" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.cal.fit_as_delay, false, "capacitance");
                    ui.selectable_value(&mut self.cal.fit_as_delay, true, "delay");
                });
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(have_lines && solvable, egui::Button::new("Fit")).clicked() {
                self.run_kit_fit();
            }
            if ui.add_enabled(self.cal.fit.is_some(), egui::Button::new("Apply to kit")).clicked() {
                self.apply_kit_fit();
            }
            let not_ideal = self.cal.set.as_ref().is_some_and(|s| s.kit != CalKit::ideal());
            if ui.add_enabled(not_ideal, egui::Button::new("Reset kit")).clicked() {
                if let Some(set) = &mut self.cal.set {
                    set.kit = CalKit::ideal();
                }
                self.cal.fit = None;
                self.cal.solved = None;
                self.cal.message = Some("kit reset to ideal".into());
                self.reshow();
            }
        });
        if let Some((fit, model)) = self.cal.fit {
            let kit = &self.cal.set.as_ref().unwrap().kit;
            let line = match model {
                OpenModel::Capacitance => format!(
                    "ΔC {:+.1} fF → open C0 {:.1} fF",
                    fit.delta_c_farad * 1e15,
                    (kit.open_c0() + fit.delta_c_farad) * 1e15
                ),
                OpenModel::Delay => format!(
                    "Δτ {:+.2} ps → open delay {:.2} ps",
                    fit.delta_delay_s * 1e12,
                    (kit.open.offset_delay_s + fit.delta_delay_s) * 1e12
                ),
            };
            ui.label(line);
            ui.small(format!(
                "{} points, ripple RMS {:.4} → {:.4} (short assumed as in the kit)",
                fit.points_used, fit.ripple_rms_before, fit.ripple_rms_after
            ));
        }
    }

    pub(super) fn run_kit_fit(&mut self) {
        let f_min = match parse_hz(&self.cal.fit_f_min) {
            Ok(f) => f,
            Err(e) => {
                self.cal.message = Some(format!("fit: {e}"));
                return;
            }
        };
        let model = self.cal.fit_model();
        let Some(set) = &self.cal.set else { return };
        match set.fit_open(f_min, model) {
            Ok(fit) => {
                self.cal.fit = Some((fit, model));
                self.cal.message = None;
            }
            Err(e) => {
                self.cal.fit = None;
                self.cal.message = Some(format!("fit: {e}"));
            }
        }
    }

    pub(super) fn apply_kit_fit(&mut self) {
        let Some((fit, model)) = self.cal.fit.take() else { return };
        let Some(set) = &mut self.cal.set else { return };
        set.apply_open_fit(&fit, model);
        self.cal.solved = None;
        self.cal.message = Some(format!(
            "kit updated: open C0 {:.1} fF, delay {:.2} ps — save to keep it",
            set.kit.open_c0() * 1e15,
            set.kit.open.offset_delay_s * 1e12
        ));
        self.reshow();
    }

    pub(super) fn load_cal(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("calibration", &["json"]).pick_file() else {
            return;
        };
        self.load_cal_from(&path);
    }

    pub(super) fn load_cal_from(&mut self, path: &Path) {
        match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| CalibrationSet::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(set) => {
                self.cal.set = Some(set);
                self.cal.file = Some(path.to_path_buf());
                self.cal.solved = None;
                self.cal.fit = None;
                self.cal.warning = None;
                self.cal.message = Some("loaded".into());
                self.log(format!("loaded calibration {}", path.display()));
                self.reshow();
            }
            Err(e) => self.error = Some(format!("loading calibration: {e}")),
        }
    }

    pub(super) fn save_cal(&mut self) {
        let Some(set) = &self.cal.set else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("calibration", &["json"])
            .set_file_name(format!(
                "{}-{}.cal.json",
                compact_hz(set.freqs_hz[0]),
                compact_hz(*set.freqs_hz.last().unwrap())
            ))
            .save_file()
        else {
            return;
        };
        match set
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|j| std::fs::write(&path, j).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.log(format!("saved calibration {}", path.display()));
                self.cal.file = Some(path);
                self.cal.message = Some("saved".into());
            }
            Err(e) => self.error = Some(format!("saving calibration: {e}")),
        }
    }
}
