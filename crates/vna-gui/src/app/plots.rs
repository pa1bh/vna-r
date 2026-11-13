//! The frequency view: Smith chart, the three selectable plots, markers and references.

use super::*;

/// A held trace: what was shown at the time (calibrated, gated as it was).
pub(super) struct Reference {
    pub(super) name: String,
    pub(super) sweep: SweepResult,
    pub(super) visible: bool,
}

/// Subdirectory of the settings directory that holds the references.
#[cfg(not(target_arch = "wasm32"))]
pub(super) const REFERENCE_DIR: &str = "references";

/// Where the references are kept between sessions: next to eframe's settings file.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn references_dir() -> Option<PathBuf> {
    eframe::storage_dir("vna-r").map(|d| d.join(REFERENCE_DIR))
}

/// A browser has no directory to keep them in: the references last as long as the page.
#[cfg(target_arch = "wasm32")]
pub(super) fn references_dir() -> Option<PathBuf> {
    None
}

fn reference_file(dir: &Path, position: usize) -> PathBuf {
    dir.join(format!("ref-{position}.s2p"))
}

/// Write the references as `ref-0.s2p`, `ref-1.s2p`, … into `dir`, replacing
/// what was there. Names and visibility are in the settings.
pub(super) fn write_references(dir: &Path, references: &[Reference]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for k in 0..MAX_REFS.max(references.len()) {
        let path = reference_file(dir, k);
        match references.get(k) {
            Some(r) => {
                let f = std::io::BufWriter::new(std::fs::File::create(&path)?);
                write_s2p(f, &r.sweep, Some(&format!("vna-r reference: {}", r.name)))?;
            }
            None if path.exists() => std::fs::remove_file(&path)?,
            None => {}
        }
    }
    Ok(())
}

/// Read back the references the settings list. One that cannot be read (file
/// gone or damaged) ends the list there, so names and files stay paired.
pub(super) fn read_references(dir: &Path, entries: &[ReferenceSettings]) -> Vec<Reference> {
    let mut out = Vec::new();
    for (k, entry) in entries.iter().enumerate().take(MAX_REFS) {
        match litevna::touchstone::read_touchstone_file(&reference_file(dir, k)) {
            Ok(sweep) => out.push(Reference {
                name: entry.name.clone(),
                sweep,
                visible: entry.visible,
            }),
            Err(e) => {
                log::warn!("reference {k} ({}) not restored: {e}", entry.name);
                break;
            }
        }
    }
    out
}

/// Interpolation of a reference onto another grid (held at the ends outside
/// the reference span), polar like the calibration (see
/// `litevna::calibration::interpolate_complex`).
pub(super) fn resample_complex(src_f: &[u64], s11: &[Complex64], s21: &[Complex64], dst_f: &[u64]) -> (Vec<Complex64>, Vec<Complex64>) {
    if src_f.is_empty() {
        let zeros = vec![Complex64::new(0.0, 0.0); dst_f.len()];
        return (zeros.clone(), zeros);
    }
    (interpolate_complex(src_f, s11, dst_f), interpolate_complex(src_f, s21, dst_f))
}

/// Paint the amateur bands that fall inside the plot's x range as a grey
/// backdrop with their name, in screen space after the plot has drawn (a
/// data-space polygon would feed back into the auto-bounds; see `draw_gate`).
pub(super) fn draw_bands(ui: &egui::Ui, transform: &egui_plot::PlotTransform, rect: egui::Rect) {
    let bounds = transform.bounds();
    let (lo_hz, hi_hz) = (bounds.min()[0].max(0.0) * 1e6, bounds.max()[0].max(0.0) * 1e6);
    let x_of = |hz: u64| transform.position_from_point(&egui_plot::PlotPoint::new(hz as f64 / 1e6, 0.0)).x;
    let painter = ui.painter().with_clip_rect(rect);
    for b in bands_within(REGION_1, lo_hz as u64, hi_hz as u64) {
        let (xa, xb) = (x_of(b.start_hz).max(rect.left()), x_of(b.stop_hz).min(rect.right()));
        let band = egui::Rect::from_min_max(egui::pos2(xa, rect.top()), egui::pos2(xb, rect.bottom()));
        painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(160, 160, 160, 28));
        painter.text(
            egui::pos2((xa + xb) / 2.0, rect.top() + 4.0),
            egui::Align2::CENTER_TOP,
            b.name,
            egui::FontId::proportional(10.0),
            Color32::from_gray(150),
        );
    }
}

pub(super) fn series(x: &[f64], y: impl Iterator<Item = f64>) -> PlotPoints<'static> {
    x.iter().zip(y).map(|(x, y)| [*x, y]).collect()
}

impl App {
    /// Markers are point indices. When the grid changes, keep each marker at
    /// the frequency it stood on (nearest new point); a marker whose
    /// frequency lies outside the new grid is dropped.
    pub(super) fn remap_markers(&mut self, old: &[u64], new: &[u64]) {
        if old == new || new.is_empty() {
            return;
        }
        let (lo, hi) = (*new.iter().min().unwrap(), *new.iter().max().unwrap());
        for m in &mut self.markers {
            *m = m
                .and_then(|i| old.get(i).copied())
                .filter(|f| (lo..=hi).contains(f))
                .map(|f| nearest_index(new, f));
        }
    }

    /// The sweep the frequency view works on: `shown`, with S11 gated when the gate is on.
    /// What the frequency view shows: the shown sweep, with S11 time-gated
    /// when the gate is on. A copy without the raw FIFO records, which no
    /// view or export needs.
    pub(super) fn freq_sweep(&mut self) -> Option<SweepResult> {
        let gated = self.gated_s11().map(|(s11, _)| s11);
        let shown = self.shown.as_ref()?;
        Some(SweepResult {
            freqs_hz: shown.freqs_hz.clone(),
            s11: gated.unwrap_or_else(|| shown.s11.clone()),
            s21: shown.s21.clone(),
            points: Vec::new(),
            checksum_ok: shown.checksum_ok,
            checksum_bad: shown.checksum_bad,
            duplicates: shown.duplicates,
            elapsed: shown.elapsed,
        })
    }

    pub(super) fn hold_reference(&mut self) {
        let Some(sweep) = self.freq_sweep() else { return };
        let n = self.references.len() + 1;
        let name = format!(
            "ref {n}: {}–{}{}",
            format_hz(sweep.freqs_hz[0]).replace(' ', ""),
            format_hz(*sweep.freqs_hz.last().unwrap()).replace(' ', ""),
            // A file trace is what it is; the calibration flags describe the last device sweep.
            if self.file_trace.is_some() || self.is_calibrated() {
                ""
            } else {
                " raw"
            }
        );
        self.references.push(Reference {
            name,
            sweep,
            visible: true,
        });
        self.references_changed = true;
    }

    /// Write the references to disk if they changed since the last time
    /// (called when eframe saves the settings: every 30 s and at exit).
    pub(super) fn store_references(&mut self) {
        if !self.references_changed {
            return;
        }
        let Some(dir) = references_dir() else { return };
        match write_references(&dir, &self.references) {
            Ok(()) => self.references_changed = false,
            Err(e) => self.error = Some(format!("saving references to {}: {e}", dir.display())),
        }
    }

    pub(super) fn references_panel(&mut self, ui: &mut egui::Ui) {
        if self.references.is_empty() {
            ui.small("Hold (in the marker bar above the charts) keeps the current trace as a reference to compare against.");
            return;
        }
        ui.label("References");
        let mut remove: Option<usize> = None;
        for k in 0..self.references.len() {
            let color = REF_COLORS[k % REF_COLORS.len()];
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.references[k].visible, "");
                ui.add(
                    egui::TextEdit::singleline(&mut self.references[k].name)
                        .desired_width(120.0)
                        .text_color(color),
                );
                let is_diff = self.diff_ref == Some(k);
                if ui
                    .selectable_label(is_diff, "Δ")
                    .on_hover_text("show live − this reference (dB / degrees) in log mag and phase plots")
                    .clicked()
                {
                    self.diff_ref = if is_diff { None } else { Some(k) };
                }
                if ui.small_button("×").clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            self.references.remove(k);
            self.references_changed = true;
            self.diff_ref = match self.diff_ref {
                Some(d) if d == k => None,
                Some(d) if d > k => Some(d - 1),
                other => other,
            };
        }
        ui.small("References are kept between sessions. To keep one for good, save it while it is the trace, or copy it from the references folder next to the settings.");
    }

    pub(super) fn central(&mut self, ui: &mut egui::Ui) {
        if self.view == View::Tdr {
            self.central_tdr(ui);
            return;
        }
        // One copy of the shown sweep per frame, for the readout and the plots.
        let sweep = self.freq_sweep();
        self.marker_readout(ui, sweep.as_ref());
        let avail = ui.available_size();
        let h = ((avail.y - 10.0) / 2.0).max(150.0);
        let Some(sweep) = sweep else {
            ui.centered_and_justified(|ui| ui.label(EMPTY_HINT));
            return;
        };
        // Grey lines where a gated trace stops being trustworthy.
        let edge_lines: Vec<f64> = match self.gated_cache.as_ref().filter(|_| self.gate_on) {
            Some((_, _, edge_hz)) => {
                let r = self.tdr_range();
                let (f0, f1) = (sweep.freqs_hz[r.start] as f64, sweep.freqs_hz[r.end - 1] as f64);
                vec![(f0 + edge_hz) / 1e6, (f1 - edge_hz) / 1e6]
            }
            None => Vec::new(),
        };
        let mhz: Vec<f64> = sweep.freqs_hz.iter().map(|f| *f as f64 / 1e6).collect();
        // Arrow keys nudge the active marker (shift: 10 points), unless a text
        // field has the keyboard: `key_pressed` sees the raw events, so without
        // this check editing Start/Stop would move the marker too.
        let n_pts = sweep.freqs_hz.len();
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (left, right, shift) = ui.input(|i| {
            (
                !typing && i.key_pressed(egui::Key::ArrowLeft),
                !typing && i.key_pressed(egui::Key::ArrowRight),
                i.modifiers.shift,
            )
        });
        if left || right {
            if let Some(i) = self.markers[self.active_marker] {
                let step = if shift { 10 } else { 1 };
                let j = if left { i.saturating_sub(step) } else { (i + step).min(n_pts - 1) };
                self.markers[self.active_marker] = Some(j);
            }
        }
        let marker_lines: Vec<(f64, Color32)> = self
            .markers
            .iter()
            .enumerate()
            .filter_map(|(k, m)| m.and_then(|i| mhz.get(i).map(|x| (*x, MARKER_COLORS[k]))))
            .collect();
        let marker_points: Vec<(Complex64, Color32)> = self
            .markers
            .iter()
            .enumerate()
            .filter_map(|(k, m)| m.and_then(|i| sweep.s11.get(i).map(|g| (*g, MARKER_COLORS[k]))))
            .collect();
        let mut clicked_x: Option<f64> = None;

        ui.columns(2, |cols| {
            // Smith chart
            let w = cols[0].available_width();
            let extra: Vec<(&[Complex64], Color32)> = self
                .references
                .iter()
                .enumerate()
                .filter(|(_, r)| r.visible)
                .map(|(k, r)| (r.sweep.s11.as_slice(), REF_COLORS[k % REF_COLORS.len()]))
                .collect();
            let resp = SmithChart {
                gamma: &sweep.s11,
                markers: &marker_points,
                trace_color: S11_COLOR,
                extra: &extra,
            }
            .show(&mut cols[0], Vec2::new(w, h));
            if resp.response.clicked() {
                if let Some(p) = resp.response.interact_pointer_pos() {
                    // nearest trace point on screen
                    let best = sweep
                        .s11
                        .iter()
                        .enumerate()
                        .map(|(i, g)| (i, resp.pos_of(*g).distance(p)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((i, _)) = best {
                        self.markers[self.active_marker] = Some(i);
                    }
                }
            }
            if let Some(x) = self.kind_plot(&mut cols[1], 0, h, &mhz, &sweep, &marker_lines, &edge_lines) {
                clicked_x = Some(x);
            }
        });
        ui.columns(2, |cols| {
            for (col, k) in [(0usize, 1usize), (1, 2)] {
                if let Some(x) = self.kind_plot(&mut cols[col], k, h, &mhz, &sweep, &marker_lines, &edge_lines) {
                    clicked_x = Some(x);
                }
            }
        });

        if let Some(x) = clicked_x {
            // nearest frequency index
            let best = mhz
                .iter()
                .enumerate()
                .map(|(i, f)| (i, (f - x).abs()))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            self.markers[self.active_marker] = best.map(|(i, _)| i);
        }
    }

    /// One selectable plot: a header with the trace-kind combo box, then the
    /// plot itself. Returns the clicked x (MHz) if the user clicked in it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn kind_plot(
        &mut self,
        ui: &mut egui::Ui,
        slot: usize,
        h: f32,
        mhz: &[f64],
        sweep: &SweepResult,
        marker_lines: &[(f64, Color32)],
        edge_lines: &[f64],
    ) -> Option<f64> {
        let header_h = 22.0;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("plot_kind", slot))
                .selected_text(self.plot_kinds[slot].name())
                .show_ui(ui, |ui| {
                    for k in TraceKind::ALL {
                        ui.selectable_value(&mut self.plot_kinds[slot], k, k.name());
                    }
                });
            if self.plot_kinds[slot] == TraceKind::GroupDelay {
                ui.small(format!("aperture ±{} pts", self.gd_aperture));
            }
        });
        let kind = self.plot_kinds[slot];
        let (lo, hi) = kind.default_range();
        let mut lines: Vec<(String, Vec<f64>, Color32, bool)> = Vec::new();
        for (param, s, color, show) in [
            (Param::S11, &sweep.s11, S11_COLOR, self.show_s11),
            (Param::S21, &sweep.s21, S21_COLOR, self.show_s21),
        ] {
            if !show {
                continue;
            }
            for (i, series) in traces::compute(kind, param, &sweep.freqs_hz, s, self.gd_aperture)
                .into_iter()
                .enumerate()
            {
                // second series of a pair (X of R+jX) dashed
                lines.push((series.label, series.values, color, i == 1));
            }
        }
        // Reference traces (thin, own colour) and the optional difference trace.
        let mut ref_lines: Vec<(String, Vec<f64>, Vec<f64>, Color32)> = Vec::new();
        for (k, r) in self.references.iter().enumerate() {
            if !r.visible {
                continue;
            }
            let color = REF_COLORS[k % REF_COLORS.len()];
            let rmhz: Vec<f64> = r.sweep.freqs_hz.iter().map(|f| *f as f64 / 1e6).collect();
            for (param, s, show) in [(Param::S11, &r.sweep.s11, self.show_s11), (Param::S21, &r.sweep.s21, self.show_s21)] {
                if !show {
                    continue;
                }
                for ser in traces::compute(kind, param, &r.sweep.freqs_hz, s, self.gd_aperture) {
                    ref_lines.push((format!("{} {}", r.name, ser.label), rmhz.clone(), ser.values, color));
                }
            }
        }
        // Difference trace: live minus reference, in the unit of this view. The
        // reference is resampled (complex, linear) onto the live grid first, then
        // the same derived quantity is computed for both and subtracted; phase
        // differences are wrapped to ±180°.
        let mut diff_lines: Vec<(String, Vec<f64>)> = Vec::new();
        if let Some(k) = self.diff_ref.filter(|k| *k < self.references.len()) {
            let r = &self.references[k];
            let (rs11, rs21) = resample_complex(&r.sweep.freqs_hz, &r.sweep.s11, &r.sweep.s21, &sweep.freqs_hz);
            for (param, live, refv, show) in [
                (Param::S11, &sweep.s11, &rs11, self.show_s11),
                (Param::S21, &sweep.s21, &rs21, self.show_s21),
            ] {
                if !show {
                    continue;
                }
                let live_series = traces::compute(kind, param, &sweep.freqs_hz, live, self.gd_aperture);
                let ref_series = traces::compute(kind, param, &sweep.freqs_hz, refv, self.gd_aperture);
                for (ls, rs) in live_series.into_iter().zip(ref_series) {
                    let vals: Vec<f64> = ls
                        .values
                        .iter()
                        .zip(&rs.values)
                        .map(|(a, b)| {
                            let d = a - b;
                            if kind == TraceKind::Phase {
                                ((d + 180.0).rem_euclid(360.0)) - 180.0
                            } else {
                                d
                            }
                        })
                        .collect();
                    diff_lines.push((format!("Δ {} vs {}", ls.label, r.name), vals));
                }
            }
        }
        let r = Plot::new(("plot", slot, kind.id()))
            .height((h - header_h).max(100.0))
            .legend(Legend::default())
            .x_axis_label("MHz")
            .y_axis_label(kind.unit())
            .include_y(lo)
            .include_y(hi)
            .show(ui, |pui| {
                for (label, x, values, color) in &ref_lines {
                    pui.line(Line::new(label.clone(), series(x, values.iter().copied())).color(*color).width(1.0));
                }
                for (label, values) in &diff_lines {
                    pui.line(
                        Line::new(label.clone(), series(mhz, values.iter().copied()))
                            .color(Color32::from_rgb(255, 120, 120))
                            .style(egui_plot::LineStyle::dotted_dense()),
                    );
                }
                for (label, values, color, dashed) in &lines {
                    let mut line = Line::new(label.clone(), series(mhz, values.iter().copied())).color(*color);
                    if *dashed {
                        line = line.style(egui_plot::LineStyle::dashed_loose());
                    }
                    pui.line(line);
                }
                for (k, (x, c)) in marker_lines.iter().enumerate() {
                    pui.vline(VLine::new(format!("M{}", k + 1), *x).color(*c));
                }
                for x in edge_lines {
                    pui.vline(
                        VLine::new("gate edge", *x)
                            .color(Color32::from_gray(110))
                            .style(egui_plot::LineStyle::dashed_dense()),
                    );
                }
                (pui.response().clicked(), pui.pointer_coordinate())
            });
        if self.show_bands {
            draw_bands(ui, &r.transform, r.response.rect);
        }
        match r.inner {
            (true, Some(p)) => Some(p.x),
            _ => None,
        }
    }

    pub(super) fn marker_readout(&mut self, ui: &mut egui::Ui, sweep: Option<&SweepResult>) {
        ui.horizontal(|ui| {
            ui.label("Markers");
            for (k, color) in MARKER_COLORS.iter().enumerate() {
                let placed = self.markers[k].is_some();
                let text = RichText::new(format!("M{}", k + 1)).color(*color);
                let text = if placed { text.strong() } else { text.weak() };
                if ui.selectable_label(self.active_marker == k, text).clicked() {
                    self.active_marker = k;
                }
            }
            ui.small("click in a plot places the active one (keys 1–4 choose it); ← → nudge, shift for ×10")
                .on_hover_text("other keys: space Run/Stop, S single sweep, H hold, G gate on/off, T frequency/time domain");
            if ui.small_button("clear").clicked() {
                self.markers[self.active_marker] = None;
            }
            if ui.small_button("clear all").clicked() {
                self.markers = [None; MARKERS];
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.shown.is_some() && self.references.len() < MAX_REFS,
                    egui::Button::new("Hold").small(),
                )
                .on_hover_text("keep the current trace as a reference (see Traces)")
                .clicked()
            {
                self.hold_reference();
            }
            ui.separator();
            if let Some(sweep) = sweep {
                let n = sweep.freqs_hz.len();
                let argmin = |f: &dyn Fn(usize) -> f64| (0..n).min_by(|a, b| f(*a).total_cmp(&f(*b)));
                if ui
                    .small_button("min SWR")
                    .on_hover_text("move the active marker to the lowest |S11|")
                    .clicked()
                {
                    self.markers[self.active_marker] = argmin(&|i| sweep.s11[i].norm());
                }
                if ui.small_button("max S21").clicked() {
                    self.markers[self.active_marker] = argmin(&|i| -sweep.s21[i].norm());
                }
                if ui.small_button("min S21").clicked() {
                    self.markers[self.active_marker] = argmin(&|i| sweep.s21[i].norm());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let partial = self.is_partially_calibrated();
                let tag = ui.label(self.cal_tag().strong());
                if partial {
                    tag.on_hover_text("some segments have no usable calibration and are shown raw");
                }
                self.gate_tag(ui);
            });
        });

        let Some(sweep) = sweep else { return };
        let placed: Vec<(usize, usize)> = self
            .markers
            .iter()
            .enumerate()
            .filter_map(|(k, m)| m.map(|i| (k, i)))
            .filter(|(_, i)| *i < sweep.freqs_hz.len())
            .collect();
        if placed.is_empty() {
            return;
        }
        egui::Grid::new("marker-table").striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
            for h in ["", "frequency", "S11 dB", "S11 °", "Z", "L / C", "SWR", "S21 dB", "S21 °"] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for (k, i) in &placed {
                let s11 = sweep.s11[*i];
                let s21 = sweep.s21[*i];
                let z = impedance(s11);
                let f_hz = sweep.freqs_hz[*i];
                ui.label(RichText::new(format!("M{}", k + 1)).color(MARKER_COLORS[*k]).strong());
                ui.label(format_hz(f_hz));
                ui.label(RichText::new(format!("{:.2}", db(s11.norm()))).color(S11_COLOR));
                ui.label(format!("{:.1}", s11.arg().to_degrees()));
                ui.label(format!("{:.1} {} j{:.1} Ω", z.re, if z.im < 0.0 { "−" } else { "+" }, z.im.abs()));
                // The reactance as the one coil or capacitor it is at this
                // frequency; Q and the parallel form on hover, so the table
                // stays one line per marker.
                let lc = match series_equivalent(z, f_hz as f64) {
                    Some(Reactance::Inductance(l)) => format_eng(l, "H"),
                    Some(Reactance::Capacitance(c)) => format_eng(c, "F"),
                    None => "–".to_string(),
                };
                let q = q_factor(z).map_or("–".to_string(), |q| format!("{q:.1}"));
                let p = parallel_equivalent(z);
                let p_x = match series_equivalent(Complex64::new(0.0, p.im), f_hz as f64) {
                    Some(Reactance::Inductance(l)) => format_eng(l, "H"),
                    Some(Reactance::Capacitance(c)) => format_eng(c, "F"),
                    None => "–".to_string(),
                };
                ui.label(lc).on_hover_text(format!(
                    "series equivalent of X at this frequency\nQ = |X|/R = {q}\nparallel equivalent: {} ∥ {p_x}",
                    format_eng(p.re, "Ω")
                ));
                ui.label(format!("{:.2}", swr(s11.norm())));
                ui.label(RichText::new(format!("{:.2}", db(s21.norm()))).color(S21_COLOR));
                ui.label(format!("{:.1}", s21.arg().to_degrees()));
                ui.end_row();
            }
            // Delta between the first two placed markers.
            if placed.len() >= 2 {
                let (ka, a) = placed[0];
                let (kb, b) = placed[1];
                let df = sweep.freqs_hz[b] as i64 - sweep.freqs_hz[a] as i64;
                ui.label(RichText::new(format!("M{}−M{}", kb + 1, ka + 1)).weak());
                ui.label(format!("{}{}", if df < 0 { "−" } else { "" }, format_hz(df.unsigned_abs())));
                ui.label(format!("{:+.2}", db(sweep.s11[b].norm()) - db(sweep.s11[a].norm())));
                ui.label(format!("{:+.1}", sweep.s11[b].arg().to_degrees() - sweep.s11[a].arg().to_degrees()));
                ui.label("");
                ui.label("");
                ui.label("");
                ui.label(format!("{:+.2}", db(sweep.s21[b].norm()) - db(sweep.s21[a].norm())));
                ui.label(format!("{:+.1}", sweep.s21[b].arg().to_degrees() - sweep.s21[a].arg().to_degrees()));
                ui.end_row();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(name: &str, points: u64) -> Reference {
        let freqs: Vec<u64> = (0..points).map(|i| 1_000_000 + i * 500_000).collect();
        Reference {
            name: name.into(),
            sweep: SweepResult {
                s11: freqs.iter().map(|&f| Complex64::from_polar(0.5, f as f64 * 1e-7)).collect(),
                s21: freqs.iter().map(|&f| Complex64::new(1e-3, f as f64 * 1e-9)).collect(),
                freqs_hz: freqs,
                points: Vec::new(),
                checksum_ok: 0,
                checksum_bad: 0,
                duplicates: 0,
                elapsed: Duration::ZERO,
            },
            visible: true,
        }
    }

    #[test]
    fn references_survive_a_round_trip_through_their_directory() {
        let dir = std::env::temp_dir().join(format!("vna-r-test-references-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let refs = vec![reference("before tuning", 11), reference("after", 21)];
        write_references(&dir, &refs).unwrap();
        let entries: Vec<ReferenceSettings> = refs
            .iter()
            .map(|r| ReferenceSettings {
                name: r.name.clone(),
                visible: false,
            })
            .collect();
        let back = read_references(&dir, &entries);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].name, "after");
        assert!(!back[1].visible);
        assert_eq!(back[1].sweep.freqs_hz, refs[1].sweep.freqs_hz);
        for (a, b) in back[0].sweep.s11.iter().zip(&refs[0].sweep.s11) {
            assert!((a - b).norm() < 1e-9);
        }
        // One reference fewer: its file goes, and a settings entry without a file ends the list.
        write_references(&dir, &refs[..1]).unwrap();
        assert!(!reference_file(&dir, 1).exists());
        assert_eq!(read_references(&dir, &entries).len(), 1);
        assert!(read_references(&dir.join("missing"), &entries).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
