//! The time-domain view and the time gate.

use super::*;

/// The TDR of the shown sweep and what it was computed for. A failure is
/// cached like a result, so a sweep TDR cannot handle (a non-uniform grid, a
/// span too narrow for its top frequency) is tried once, not every frame.
pub(super) struct TdrCache {
    pub(super) shown_rev: u64,
    pub(super) vf: f64,
    pub(super) window: Window,
    pub(super) result: Result<TdrResult, String>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct GateKey {
    pub(super) shown_rev: u64,
    pub(super) vf: f64,
    pub(super) window: Window,
    pub(super) start_m: f64,
    pub(super) stop_m: f64,
    pub(super) notch: bool,
}

/// Pointer interaction with the gate lines in one TDR plot.
pub(super) struct GateEvent {
    pub(super) clicked: bool,
    pub(super) pointer: Option<egui_plot::PlotPoint>,
    /// Gate edge under the pointer (0 = start, 1 = stop).
    pub(super) hover: Option<usize>,
    pub(super) drag_started: bool,
    pub(super) dragged: bool,
    pub(super) drag_stopped: bool,
}

impl GateEvent {
    pub(super) fn read(pui: &egui_plot::PlotUi<'_>, gate_x: Option<(f64, f64)>) -> Self {
        let resp = pui.response();
        let pointer = pui.pointer_coordinate();
        let near = pui.plot_bounds().width() * 0.015;
        let hover = match (pointer, gate_x) {
            (Some(p), Some((a, b))) if resp.hovered() || resp.dragged() => {
                if (p.x - a).abs() < near {
                    Some(0)
                } else if (p.x - b).abs() < near {
                    Some(1)
                } else {
                    None
                }
            }
            _ => None,
        };
        GateEvent {
            clicked: resp.clicked(),
            pointer,
            hover,
            drag_started: resp.drag_started(),
            dragged: resp.dragged(),
            drag_stopped: resp.drag_stopped(),
        }
    }
}

/// Paint the gate on top of a plot in screen space, so it never feeds back
/// into the plot's auto-bounds (a data-space polygon sized to the current
/// bounds would grow the bounds every frame).
pub(super) fn draw_gate(ui: &egui::Ui, transform: &egui_plot::PlotTransform, rect: egui::Rect, gate_x: Option<(f64, f64)>) {
    let Some((a, b)) = gate_x else { return };
    let x_of = |x: f64| transform.position_from_point(&egui_plot::PlotPoint::new(x, 0.0)).x;
    let (xa, xb) = (x_of(a), x_of(b));
    let painter = ui.painter().with_clip_rect(rect);
    let band = egui::Rect::from_min_max(egui::pos2(xa.min(xb), rect.top()), egui::pos2(xa.max(xb), rect.bottom()));
    painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(255, 150, 80, 22));
    let stroke = egui::Stroke::new(1.5, GATE_COLOR);
    painter.vline(xa, rect.y_range(), stroke);
    painter.vline(xb, rect.y_range(), stroke);
}

impl App {
    /// Point range of the segment TDR and gating work on: the widest one
    /// (TDR needs a uniform grid). The whole sweep when there is one segment.
    pub(super) fn tdr_range(&self) -> std::ops::Range<usize> {
        let total = self.shown.as_ref().map_or(0, |s| s.freqs_hz.len());
        SweepResult::widest_segment(&self.last_plan, total)
    }

    /// Gated S11 of the shown sweep (cached), with the unreliable band-edge width in Hz.
    pub(super) fn gated_s11(&mut self) -> Option<(Vec<Complex64>, f64)> {
        if !self.gate_on {
            return None;
        }
        let range = self.tdr_range();
        let sweep = self.shown.as_ref()?;
        let key = GateKey {
            shown_rev: self.shown_rev,
            vf: self.tdr_vf,
            window: self.tdr_window,
            start_m: self.gate_start_m,
            stop_m: self.gate_stop_m,
            notch: self.gate_notch,
        };
        if !matches!(&self.gated_cache, Some((k, _, _)) if *k == key) {
            let cfg = TdrConfig {
                window: self.tdr_window,
                velocity_factor: self.tdr_vf,
                fft_size: 0,
            };
            let kind = if self.gate_notch { GateKind::Notch } else { GateKind::BandPass };
            let g = Gate::from_distance(self.gate_start_m, self.gate_stop_m, self.tdr_vf, kind);
            match gate(&sweep.freqs_hz[range.clone()], &sweep.s11[range.clone()], &cfg, &g) {
                Ok(r) => {
                    // Splice the gated segment back into the full sweep.
                    let mut full = sweep.s11.clone();
                    full[range].copy_from_slice(&r.s11);
                    self.gated_cache = Some((key, full, r.edge_hz));
                    self.gate_error = None;
                }
                Err(e) => {
                    self.gated_cache = None;
                    self.gate_error = Some(e.to_string());
                    return None;
                }
            }
        }
        self.gated_cache.as_ref().map(|(_, s, e)| (s.clone(), *e))
    }

    pub(super) fn gate_tag(&self, ui: &mut egui::Ui) {
        if self.gate_on && self.gated_cache.is_some() {
            ui.label(RichText::new("GATED").color(GATE_COLOR).strong())
                .on_hover_text("S11 is time-gated; see the TDR panel");
        }
    }

    pub(super) fn tdr_result(&mut self) -> Option<&TdrResult> {
        let range = self.tdr_range();
        let sweep = self.shown.as_ref()?;
        let fresh =
            matches!(&self.tdr_cache, Some(c) if c.shown_rev == self.shown_rev && c.vf == self.tdr_vf && c.window == self.tdr_window);
        if !fresh {
            let cfg = TdrConfig {
                window: self.tdr_window,
                velocity_factor: self.tdr_vf,
                fft_size: 0,
            };
            let result = tdr(&sweep.freqs_hz[range.clone()], &sweep.s11[range], &cfg).map_err(|e| match e {
                litevna::Error::InvalidArgument(msg) => msg,
                e => e.to_string(),
            });
            self.tdr_cache = Some(TdrCache {
                shown_rev: self.shown_rev,
                vf: self.tdr_vf,
                window: self.tdr_window,
                result,
            });
        }
        self.tdr_cached()
    }

    /// The TDR last computed, if that worked (it may be of an older sweep;
    /// `tdr_result` is what brings it up to date).
    pub(super) fn tdr_cached(&self) -> Option<&TdrResult> {
        self.tdr_cache.as_ref().and_then(|c| c.result.as_ref().ok())
    }

    pub(super) fn central_tdr(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.tdr_result().cloned() else {
            // No sweep yet, or a sweep TDR cannot work with: say which.
            let why = self.tdr_cache.as_ref().and_then(|c| c.result.as_ref().err().cloned());
            ui.centered_and_justified(|ui| match (&self.shown, why) {
                (Some(_), Some(why)) => ui.colored_label(
                    Color32::from_rgb(230, 150, 80),
                    format!("No TDR for this sweep: {why}.\nTDR needs evenly spaced points from a low start frequency to a high stop."),
                ),
                _ => ui.label("Connect and press Run; TDR is computed from S11"),
            });
            return;
        };
        // readout
        ui.horizontal(|ui| {
            match self.tdr_marker {
                Some(i) if i < t.distance_m.len() => {
                    ui.label(
                        RichText::new(format!("M  {:.3} m  {:.2} ns", t.distance_m[i], t.time_s[i] * 1e9))
                            .strong()
                            .color(MARKER_COLORS[0]),
                    );
                    ui.separator();
                    ui.label(RichText::new(format!("|Γ| {:.1} dB", db(t.impulse_mag[i]))).color(S11_COLOR));
                    ui.label(format!("Z {:.1} Ω", t.impedance_ohm[i]));
                    if ui.small_button("clear").clicked() {
                        self.tdr_marker = None;
                    }
                }
                _ => {
                    ui.small("Click in a plot to place a marker. Distances are one-way, using the velocity factor.");
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let partial = self.is_partially_calibrated();
                let tag = ui.label(self.cal_tag().strong());
                if partial {
                    tag.on_hover_text("some segments have no usable calibration and are shown raw");
                }
                self.gate_tag(ui);
                if self.gate_on {
                    ui.small("drag the orange lines to move the gate");
                }
            });
        });

        let avail = ui.available_size();
        let h = ((avail.y - 10.0) / 2.0).max(150.0);
        let limit = self.tdr_max_m.min(t.max_distance_m());
        let gate_x = if self.gate_on {
            Some((self.gate_start_m, self.gate_stop_m))
        } else {
            None
        };
        let plot_drag_ok = self.gate_hover.is_none() && self.gate_drag.is_none();
        self.gate_hover = None;
        let mut gate_events: Vec<GateEvent> = Vec::new();
        let n = t.distance_m.iter().position(|d| *d > limit).unwrap_or(t.distance_m.len());
        let dist = &t.distance_m[..n];
        let marker_x = self.tdr_marker.and_then(|i| dist.get(i).copied());
        let mut clicked_x: Option<f64> = None;

        let r = Plot::new("tdr-impulse")
            .height(h)
            .x_axis_label("m")
            .y_axis_label("|Γ| dB")
            .include_y(0.0)
            .include_y(-60.0)
            .include_x(0.0)
            .include_x(limit)
            .allow_drag(plot_drag_ok)
            .show(ui, |pui| {
                pui.line(Line::new("impulse", series(dist, t.impulse_mag[..n].iter().map(|m| db(*m).max(-100.0)))).color(S11_COLOR));
                if let Some(x) = marker_x {
                    pui.vline(VLine::new("marker", x).color(MARKER_COLORS[0]));
                }
                GateEvent::read(pui, gate_x)
            });
        draw_gate(ui, &r.transform, r.response.rect, gate_x);
        gate_events.push(r.inner);
        let r = Plot::new("tdr-z")
            .height(h)
            .x_axis_label("m")
            .y_axis_label("Ω")
            .include_y(0.0)
            .include_y(100.0)
            .include_x(0.0)
            .include_x(limit)
            .allow_drag(plot_drag_ok)
            .show(ui, |pui| {
                pui.line(Line::new("Z", series(dist, t.impedance_ohm[..n].iter().map(|z| z.min(500.0)))).color(S21_COLOR));
                if let Some(x) = marker_x {
                    pui.vline(VLine::new("marker", x).color(MARKER_COLORS[0]));
                }
                GateEvent::read(pui, gate_x)
            });
        draw_gate(ui, &r.transform, r.response.rect, gate_x);
        gate_events.push(r.inner);

        for ev in gate_events {
            if ev.hover.is_some() {
                self.gate_hover = ev.hover;
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if ev.drag_started && ev.hover.is_some() {
                self.gate_drag = ev.hover;
            }
            if ev.dragged {
                if let (Some(edge), Some(x)) = (self.gate_drag, ev.pointer.map(|p| p.x)) {
                    let min_gap = t.resolution_m() * 0.5;
                    match edge {
                        0 => self.gate_start_m = x.min(self.gate_stop_m - min_gap),
                        _ => self.gate_stop_m = x.max(self.gate_start_m + min_gap),
                    }
                }
            }
            if ev.drag_stopped {
                self.gate_drag = None;
            }
            if ev.clicked && ev.hover.is_none() && self.gate_drag.is_none() {
                clicked_x = ev.pointer.map(|p| p.x);
            }
        }
        if let Some(x) = clicked_x {
            let best = dist
                .iter()
                .enumerate()
                .map(|(i, d)| (i, (d - x).abs()))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            self.tdr_marker = best.map(|(i, _)| i);
        }
    }
}
