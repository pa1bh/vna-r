//! Touchstone files and device screenshots.

use super::*;

/// A frequency for a file name: `50kHz`, `14.2MHz`, `6.3GHz` (no space, no trailing zeros).
pub(super) fn compact_hz(hz: u64) -> String {
    let text = format_hz(hz).replace(' ', "");
    let split = text.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number = if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    };
    format!("{number}{unit}")
}

impl App {
    pub(super) fn open_touchstone(&mut self, as_reference: bool) {
        let Some(path) = rfd::FileDialog::new().add_filter("Touchstone", &["s1p", "s2p"]).pick_file() else {
            return;
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match read_touchstone_file_full(&path) {
            Ok(file) => {
                let note = if file.renormalised() {
                    format!("; {} Ω file: S11 renormalised to 50 Ω, S21 as in the file", file.z0_ohm)
                } else {
                    String::new()
                };
                let sweep = file.sweep;
                self.file_message = Some(format!(
                    "opened {} as {}{note}",
                    path.display(),
                    if as_reference { "reference" } else { "trace" }
                ));
                if as_reference {
                    self.references.push(Reference {
                        name,
                        sweep,
                        visible: true,
                    });
                } else {
                    if self.running {
                        self.stop_sweep();
                    }
                    self.last_plan.clear();
                    self.last_plan_cals.clear();
                    self.avg_buf.clear();
                    self.file_trace = Some(name);
                    if let Some(old) = self.raw.as_ref().map(|r| r.freqs_hz.clone()) {
                        self.remap_markers(&old, &sweep.freqs_hz);
                    }
                    self.raw = Some(sweep.clone());
                    self.set_shown(sweep);
                }
            }
            Err(e) => self.error = Some(format!("opening {}: {e}", path.display())),
        }
    }

    pub(super) fn save_s2p(&mut self) {
        let Some(sweep) = self.freq_sweep() else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Touchstone", &["s2p", "s1p"])
            .set_file_name(format!(
                "sweep-{}-{}.s2p",
                compact_hz(sweep.freqs_hz[0]),
                compact_hz(*sweep.freqs_hz.last().unwrap())
            ))
            .save_file()
        else {
            return;
        };
        let gated = if self.gate_on && self.gated_cache.is_some() {
            format!(
                ", S11 time-gated {:.2}..{:.2} m {}",
                self.gate_start_m,
                self.gate_stop_m,
                if self.gate_notch { "notch" } else { "band pass" }
            )
        } else {
            String::new()
        };
        let gated = if self.port_ext_ps != 0.0 {
            format!("{gated}, port extension {:.1} ps", self.port_ext_ps)
        } else {
            gated
        };
        let comment = format!(
            "vna-r, {} points, {}{}",
            sweep.freqs_hz.len(),
            match &self.file_trace {
                Some(name) => format!("from file {name}"),
                None if self.is_calibrated() => "host calibrated".to_string(),
                None if self.is_partially_calibrated() => "partly host calibrated (see segments)".to_string(),
                None => "uncalibrated raw".to_string(),
            },
            gated
        );
        // The extension decides: .s1p is S11 only, anything else the two-port file.
        let one_port = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("s1p"));
        let written = std::fs::File::create(&path).and_then(|f| {
            let f = std::io::BufWriter::new(f);
            if one_port {
                write_s1p(f, &sweep, Some(&comment))
            } else {
                write_s2p(f, &sweep, Some(&comment))
            }
        });
        match written {
            Ok(()) => {
                self.log(format!("wrote {}", path.display()));
                self.file_message = Some(format!("wrote {}", path.display()));
            }
            Err(e) => self.error = Some(format!("saving s2p: {e}")),
        }
    }

    pub(super) fn screenshot_window(&mut self, ctx: &egui::Context) {
        if !self.show_screenshot {
            return;
        }
        let mut open = self.show_screenshot;
        let mut save = false;
        egui::Window::new("Device screenshot")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some((tex, shot)) = &self.screenshot {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}×{}", shot.width, shot.height));
                        if ui.button("Save PNG…").clicked() {
                            save = true;
                        }
                    });
                    ui.add(egui::Image::new(tex).max_width(ui.available_width().min(1024.0)));
                }
            });
        self.show_screenshot = open;
        if save {
            self.save_screenshot();
        }
    }

    pub(super) fn save_screenshot(&mut self) {
        let Some((_, shot)) = &self.screenshot else { return };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("screenshot.png")
            .save_file()
        else {
            return;
        };
        let rgb = shot.to_rgb888();
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let file = std::io::BufWriter::new(std::fs::File::create(&path)?);
            let mut enc = png::Encoder::new(file, shot.width as u32, shot.height as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header()?;
            w.write_image_data(&rgb)?;
            w.finish()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.error = Some(format!("saving screenshot: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_hz_for_file_names() {
        assert_eq!(compact_hz(50_000), "50kHz");
        assert_eq!(compact_hz(14_200_000), "14.2MHz");
        assert_eq!(compact_hz(6_300_000_000), "6.3GHz");
        assert_eq!(compact_hz(30_000_000), "30MHz");
        assert_eq!(compact_hz(500), "500Hz");
    }
}
