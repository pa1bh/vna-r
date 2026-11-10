//! About window, developer log and the diagnostics text for bug reports.

use super::*;

/// What draws the window: wgpu's backend and adapter, e.g. "Vulkan, Intel UHD Graphics
/// (IntegratedGpu; Mesa 24.1)". Software rendering shows up here as "llvmpipe".
pub(super) fn renderer_description(cc: &eframe::CreationContext<'_>) -> String {
    match &cc.wgpu_render_state {
        Some(rs) => {
            let i = rs.adapter.get_info();
            let driver = [i.driver.as_str(), i.driver_info.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            if driver.is_empty() {
                format!("{:?}, {} ({:?})", i.backend, i.name, i.device_type)
            } else {
                format!("{:?}, {} ({:?}; {driver})", i.backend, i.name, i.device_type)
            }
        }
        None => "OpenGL (glow)".to_string(),
    }
}

/// ", on Wayland" / ", on X11" on Linux (from the environment winit uses to choose), empty elsewhere.
pub(super) fn display_server() -> &'static str {
    if !cfg!(target_os = "linux") {
        return "";
    }
    match std::env::var_os("WAYLAND_DISPLAY") {
        Some(v) if !v.is_empty() => ", on Wayland",
        _ => ", on X11",
    }
}

impl App {
    /// Append a line to the developer log, stamped with seconds since start.
    pub(super) fn log(&mut self, msg: impl Into<String>) {
        let t = self.started.elapsed().as_secs_f64();
        self.log.push_back(format!("{t:8.1}  {}", msg.into()));
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }

    /// Everything a bug report needs, as plain text for the clipboard.
    pub(super) fn diagnostics(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "vna-r {VERSION} ({GIT_REV}), {} {}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        out.push_str(&format!("renderer: {}{}\n", self.renderer, display_server()));
        match &self.connection {
            Connection::Connected { port, info, battery_mv } => {
                out.push_str(&format!("device: {} on {port}", info.summary()));
                if let Some(mv) = battery_mv {
                    out.push_str(&format!(", battery {:.2} V", *mv as f64 / 1000.0));
                }
                out.push_str(&format!("\n{}\n", info.detail));
            }
            Connection::Connecting(p) => out.push_str(&format!("device: connecting to {p}\n")),
            Connection::Disconnected => out.push_str("device: not connected\n"),
        }
        out.push_str(&format!(
            "sweep: {} – {}, {} points, average {}, sweep avg {}, {} extra segment(s)\n",
            self.start,
            self.stop,
            self.points,
            self.average,
            self.sweep_avg,
            self.extra_segments.len()
        ));
        if let Some(raw) = &self.raw {
            out.push_str(&format!(
                "last sweep: {} points in {} ms, #{}\n",
                raw.freqs_hz.len(),
                raw.elapsed.as_millis(),
                self.sweep_count
            ));
        }
        out.push_str(&format!(
            "calibration: {}{}\n",
            match &self.cal.set {
                Some(set) => format!(
                    "{} points, {} – {}",
                    set.freqs_hz.len(),
                    format_hz(set.freqs_hz[0]),
                    format_hz(*set.freqs_hz.last().unwrap())
                ),
                None => "none".to_string(),
            },
            self.cal
                .file
                .as_ref()
                .map(|f| format!(", file {}", f.display()))
                .unwrap_or_default()
        ));
        if let Some(e) = &self.error {
            out.push_str(&format!("last error: {e}\n"));
        }
        out.push_str("\nlog:\n");
        for line in &self.log {
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = self.show_about;
        egui::Window::new("About vna-r")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("vna-r").size(28.0).strong());
                    ui.label(format!("version {VERSION}"));
                    ui.weak(format!("build {GIT_REV}"));
                    ui.weak(format!("renderer: {}{}", self.renderer, display_server()));
                    ui.add_space(10.0);
                    ui.label("Desktop software for LiteVNA and NanoVNA vector network analysers:");
                    ui.label("sweeps, calibration, Smith chart, TDR, gating, references and Touchstone files.");
                    ui.add_space(10.0);
                    ui.label(RichText::new("Bas Hendriks, PA1BH").strong());
                    ui.hyperlink_to(REPO_URL, REPO_URL);
                    ui.add_space(10.0);
                    ui.weak("MIT licence. Written in Rust with egui.");
                    ui.add_space(6.0);
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                    ui.add_space(4.0);
                });
            });
        self.show_about &= open;
    }

    pub(super) fn log_window(&mut self, ctx: &egui::Context) {
        if !self.show_log {
            return;
        }
        let mut open = self.show_log;
        let mut copy = false;
        egui::Window::new("Developer log")
            .open(&mut open)
            .resizable(true)
            .default_size([640.0, 320.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(name) = &self.file_trace {
                        if let Some(raw) = &self.raw {
                            ui.small(format!(
                                "file {name}: {} → {} in {} points",
                                format_hz(raw.freqs_hz[0]),
                                format_hz(*raw.freqs_hz.last().unwrap()),
                                raw.freqs_hz.len()
                            ));
                        }
                    } else if let (Some(raw), false) = (&self.raw, self.last_plan.is_empty()) {
                        let segs = if self.last_plan.len() > 1 {
                            format!(" in {} segments", self.last_plan.len())
                        } else {
                            String::new()
                        };
                        ui.small(format!(
                            "last sweep: {} → {} in {} points{}, {} ms, sweep #{}{}",
                            format_hz(raw.freqs_hz[0]),
                            format_hz(*raw.freqs_hz.last().unwrap()),
                            raw.freqs_hz.len(),
                            segs,
                            raw.elapsed.as_millis(),
                            self.sweep_count,
                            if self.sweep_avg > 1 {
                                format!(", avg {}/{}", self.avg_buf.len().min(self.sweep_avg), self.sweep_avg)
                            } else {
                                String::new()
                            }
                        ));
                    } else {
                        ui.small("no sweep yet");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("clear").clicked() {
                            self.log.clear();
                        }
                        if ui.small_button("copy diagnostics").clicked() {
                            copy = true;
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log {
                        ui.monospace(line);
                    }
                });
            });
        if copy {
            ctx.copy_text(self.diagnostics());
            self.log("diagnostics copied to the clipboard");
            self.notify("diagnostics copied to the clipboard");
        }
        self.show_log = open;
    }
}
