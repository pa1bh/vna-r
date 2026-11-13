//! Touchstone files and device screenshots.

use super::*;

/// The measurement that is built in, for the browser build: a first visitor
/// has no file at hand. What it is stands in the file's comment lines.
const EXAMPLE: &str = include_str!("../../web/example-ocf-antenna.s1p");
const EXAMPLE_NAME: &str = "example-ocf-antenna.s1p";
pub(super) const EXAMPLE_ABOUT: &str = "an off-centre-fed wire antenna for HF, 3–30 MHz, measured with a NanoVNA-F";

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

/// Ask for a calibration file for one sweep segment. Those are kept by path,
/// which a browser does not give (and without a device there are no segments to sweep).
pub(super) fn pick_cal_path() -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    return rfd::FileDialog::new().add_filter("calibration", &["json"]).pick_file();
    #[cfg(target_arch = "wasm32")]
    None
}

impl App {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn open_touchstone(&mut self, as_reference: bool) {
        let Some(path) = rfd::FileDialog::new().add_filter("Touchstone", &["s1p", "s2p"]).pick_file() else {
            return;
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.show_touchstone(name, &path.display().to_string(), read_touchstone_file_full(&path), as_reference);
    }

    /// The browser asks for the file and answers later: see [`App::handle_picked`].
    #[cfg(target_arch = "wasm32")]
    pub(super) fn open_touchstone(&mut self, as_reference: bool) {
        let purpose = if as_reference {
            web::PickFor::Reference
        } else {
            web::PickFor::Trace
        };
        self.web.pick(purpose, "Touchstone", &["s1p", "s2p"]);
    }

    /// Show the built-in example as the trace.
    pub(super) fn open_example(&mut self) {
        let file = litevna::touchstone::read_touchstone_full(EXAMPLE.as_bytes(), Some(1));
        self.show_touchstone(EXAMPLE_NAME.to_string(), &format!("the example ({EXAMPLE_ABOUT})"), file, false);
        // One port: its S21 is a row of zeros, a flat line that only stretches the scales.
        self.show_s21 = false;
    }

    /// Show a Touchstone file that was read (or say why it could not be) as
    /// the trace or as a reference. `shown_as` is how the messages name it:
    /// the path, or in a browser just the file name.
    pub(super) fn show_touchstone(
        &mut self,
        name: String,
        shown_as: &str,
        file: litevna::Result<litevna::touchstone::TouchstoneFile>,
        as_reference: bool,
    ) {
        match file {
            Ok(file) => {
                let note = if file.renormalised() {
                    format!("; {} Ω file: S11 renormalised to 50 Ω, S21 as in the file", file.z0_ohm)
                } else {
                    String::new()
                };
                let sweep = file.sweep;
                self.file_message = Some(format!(
                    "opened {shown_as} as {}{note}",
                    if as_reference { "reference" } else { "trace" }
                ));
                if as_reference {
                    self.references.push(Reference {
                        name,
                        sweep,
                        visible: true,
                    });
                    self.references_changed = true;
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
            Err(e) => self.error = Some(format!("opening {shown_as}: {e}")),
        }
    }

    pub(super) fn save_s2p(&mut self) {
        let Some(sweep) = self.freq_sweep() else { return };
        let file_name = format!(
            "sweep-{}-{}.s2p",
            compact_hz(sweep.freqs_hz[0]),
            compact_hz(*sweep.freqs_hz.last().unwrap())
        );
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
        // A browser saves by downloading under the proposed name: always the two-port file.
        #[cfg(target_arch = "wasm32")]
        {
            let mut bytes = Vec::new();
            match write_s2p(&mut bytes, &sweep, Some(&comment)) {
                Ok(()) => {
                    self.file_message = Some(format!("offered {file_name} as a download"));
                    self.web.save(file_name, bytes);
                }
                Err(e) => self.error = Some(format!("saving s2p: {e}")),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.write_touchstone(&sweep, &comment, file_name);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_touchstone(&mut self, sweep: &SweepResult, comment: &str, file_name: String) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Touchstone", &["s2p", "s1p"])
            .set_file_name(file_name)
            .save_file()
        else {
            return;
        };
        // The extension decides: .s1p is S11 only, anything else the two-port file.
        let one_port = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("s1p"));
        let written = std::fs::File::create(&path).and_then(|f| {
            let f = std::io::BufWriter::new(f);
            if one_port {
                write_s1p(f, sweep, Some(comment))
            } else {
                write_s2p(f, sweep, Some(comment))
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

    /// A screenshot comes from a device, which the browser build does not have.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn save_screenshot(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
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
    fn the_built_in_example_reads_as_a_one_port_sweep() {
        let file = litevna::touchstone::read_touchstone_full(EXAMPLE.as_bytes(), Some(1)).unwrap();
        assert!(!file.renormalised());
        let sweep = file.sweep;
        assert_eq!(sweep.freqs_hz.len(), 1001);
        assert_eq!((sweep.freqs_hz[0], sweep.freqs_hz[1000]), (3_000_000, 30_000_000));
        // A passive antenna: no reflection above 1, and a dip somewhere that is a usable match.
        assert!(sweep.s11.iter().all(|g| g.norm() < 1.0));
        assert!(sweep.s11.iter().any(|g| g.norm() < 0.2));
    }

    #[test]
    fn compact_hz_for_file_names() {
        assert_eq!(compact_hz(50_000), "50kHz");
        assert_eq!(compact_hz(14_200_000), "14.2MHz");
        assert_eq!(compact_hz(6_300_000_000), "6.3GHz");
        assert_eq!(compact_hz(30_000_000), "30MHz");
        assert_eq!(compact_hz(500), "500Hz");
    }
}
