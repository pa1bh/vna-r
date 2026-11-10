//! Starting and stopping sweeps: the sweep fields, segments, software averaging.

use super::*;

impl App {
    pub(super) fn current_config(&mut self) -> Option<SweepConfig> {
        let cfg = parse_hz(&self.start)
            .and_then(|s| parse_hz(&self.stop).map(|e| (s, e)))
            .and_then(|(s, e)| SweepConfig::linear(s, e, self.points));
        match cfg {
            Ok(c) => {
                self.sweep_error = None;
                Some(c)
            }
            Err(e) => {
                self.sweep_error = Some(e.to_string());
                None
            }
        }
    }

    /// All segments (primary first, then extras), sorted by start, with the
    /// calibration file each should use. Sets `sweep_error` on a bad entry.
    pub(super) fn segment_plan(&mut self) -> Option<Vec<(SweepConfig, Option<PathBuf>)>> {
        let primary = self.current_config()?;
        let mut plan = vec![(primary, None)];
        for (k, seg) in self.extra_segments.iter().enumerate() {
            let cfg = parse_hz(&seg.start)
                .and_then(|a| parse_hz(&seg.stop).map(|b| (a, b)))
                .and_then(|(a, b)| SweepConfig::linear(a, b, seg.points));
            match cfg {
                Ok(c) => plan.push((c, seg.cal_file.clone())),
                Err(e) => {
                    self.sweep_error = Some(format!("segment {}: {e}", k + 2));
                    return None;
                }
            }
        }
        // Sorted by start; touching (stop == next start) is fine, overlap is not.
        match plan_segments(plan) {
            Ok(plan) => {
                self.sweep_error = None;
                Some(plan)
            }
            Err(litevna::Error::InvalidArgument(msg)) => {
                self.sweep_error = Some(msg);
                None
            }
            Err(e) => {
                self.sweep_error = Some(e.to_string());
                None
            }
        }
    }

    pub(super) fn start_sweep(&mut self, continuous: bool) {
        if let Some(plan) = self.segment_plan() {
            self.running = continuous;
            // The primary segment is the one without its own calibration file
            // that came from the Sweep panel; `segment_plan` sorts, so find it.
            self.applied_primary = self.current_config();
            self.sent_plan_cals = plan.iter().map(|(_, c)| c.clone()).collect();
            let cfgs = plan.into_iter().map(|(c, _)| c).collect();
            self.send(Cmd::Sweep {
                cfgs,
                purpose: Purpose::Display,
                continuous,
                repeat: 1,
            });
        }
    }

    /// Something is being measured or waiting for its turn: the continuous
    /// sweep, a calibration standard, or a single sweep that has reported
    /// progress. This is when the Stop button has something to stop.
    pub(super) fn sweeping(&self) -> bool {
        self.running || self.cal.pending.is_some() || self.progress.is_some()
    }

    pub(super) fn stop_sweep(&mut self) {
        self.running = false;
        self.progress = None;
        self.send(Cmd::Stop);
    }

    /// Add a raw sweep to the ring buffer and return the complex mean of the
    /// last `sweep_avg` sweeps (the newest alone when averaging is off). The
    /// buffer resets when the grid changes. Averaging complex values shrinks
    /// noise without biasing the magnitude, unlike averaging dB.
    pub(super) fn push_average(&mut self, raw: SweepResult) -> SweepResult {
        if self.avg_buf.front().is_some_and(|b| b.freqs_hz != raw.freqs_hz) {
            self.avg_buf.clear();
        }
        self.avg_buf.push_back(raw);
        while self.avg_buf.len() > self.sweep_avg.max(1) {
            self.avg_buf.pop_front();
        }
        SweepResult::average(self.avg_buf.iter()).expect("buffer holds one grid")
    }

    /// Extra segments with their own calibration files.
    pub(super) fn segments_panel(&mut self, ui: &mut egui::Ui) {
        let open = !self.extra_segments.is_empty();
        egui::CollapsingHeader::new(format!("Segments ({})", 1 + self.extra_segments.len())).default_open(open).show(ui, |ui| {
            ui.small("Extra ranges swept after the primary one and shown in one trace, e.g. a dense HF segment with its own calibration next to a wide one. Segments may touch but not overlap.");
            if self.cal.apply && self.seg_calibrated.len() == self.last_plan.len() && self.seg_calibrated.len() > 1 {
                let raw: Vec<String> = self
                    .last_plan
                    .iter()
                    .zip(&self.seg_calibrated)
                    .filter(|(_, ok)| !**ok)
                    .map(|(c, _)| format!("{} → {}", format_hz(c.start_hz), format_hz(c.stop_hz())))
                    .collect();
                if !raw.is_empty() {
                    ui.small(RichText::new(format!("⚠ shown raw (no usable calibration): {}", raw.join(", "))).color(Color32::from_rgb(255, 150, 80)));
                }
            }
            let mut remove: Option<usize> = None;
            for k in 0..self.extra_segments.len() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", k + 2));
                    ui.add(egui::TextEdit::singleline(&mut self.extra_segments[k].start).desired_width(52.0));
                    ui.label("→");
                    ui.add(egui::TextEdit::singleline(&mut self.extra_segments[k].stop).desired_width(52.0));
                    egui::ComboBox::from_id_salt(("seg-points", k)).selected_text(self.extra_segments[k].points.to_string()).width(60.0).show_ui(ui, |ui| {
                        for p in POINT_CHOICES {
                            ui.selectable_value(&mut self.extra_segments[k].points, p, p.to_string());
                        }
                    });
                    if ui.small_button("×").on_hover_text("remove segment").clicked() {
                        remove = Some(k);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    let name = self.extra_segments[k].cal_file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
                    let label = match &name {
                        Some(n) => format!("cal: {n}"),
                        None => "cal: panel (interpolated)".to_string(),
                    };
                    if ui.small_button(label).on_hover_text("calibration file for this segment; without one the calibration panel's set is interpolated").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("calibration", &["json"]).pick_file() {
                            self.seg_cals.remove(&path);
                            self.extra_segments[k].cal_file = Some(path);
                        }
                    }
                    if self.extra_segments[k].cal_file.is_some() && ui.small_button("none").clicked() {
                        self.extra_segments[k].cal_file = None;
                    }
                });
                if let Some(e) = self.extra_segments[k].cal_file.as_ref().and_then(|p| self.seg_cals.get(p)).and_then(|c| c.error.as_ref()) {
                    ui.small(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(255, 150, 80)));
                }
            }
            if let Some(k) = remove {
                self.extra_segments.remove(k);
            }
            if ui.small_button("+ segment").clicked() {
                let start = self.extra_segments.last().map(|s| s.stop.clone()).unwrap_or_else(|| self.stop.clone());
                let stop = format_hz(self.device_max_hz()).replace(' ', "");
                self.extra_segments.push(SegmentSettings { start, stop, points: 1001, cal_file: None });
            }
        });
    }
}
