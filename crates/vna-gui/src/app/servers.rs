//! Servers: a server is a port with another kind of name (docs/10, "Een
//! server is een poort"). Device ▸ Add server…, how a port reads in the
//! interface, and the words for a link that is lost.

use super::*;

/// How a port reads in the interface: a serial port as its path, a server
/// as `server vna.local:8080`.
pub(super) fn port_label(port: &str) -> String {
    if is_server_address(port) {
        format!("server {}", server_label(port))
    } else {
        port.to_string()
    }
}

/// What the program says about a lost link.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LostTexts {
    /// For the log.
    pub(super) log: String,
    /// The red line in the top bar; none for a server that simply has no analyser yet.
    pub(super) error: Option<String>,
    /// Next to the spinner, before the seconds.
    pub(super) waiting: String,
    /// Whether it reads as trouble (orange) rather than as waiting.
    pub(super) trouble: bool,
    pub(super) cancel_hover: &'static str,
}

pub(super) fn lost_texts(port: &str, msg: &str, what: Loss) -> LostTexts {
    let server = server_label(port);
    let (log, error, waiting, trouble, cancel_hover) = match what {
        Loss::Device => (
            format!("connection dropped ({msg}); waiting for {port}"),
            Some(format!(
                "the connection to the device dropped ({msg}): the cable or connector moved, or the device \
                 restarted. Waiting for it to come back."
            )),
            format!("connection dropped, waiting for {port}…"),
            true,
            "stop waiting for the device and disconnect",
        ),
        Loss::Server => (
            format!("connection to the server dropped ({msg}); waiting for {server}"),
            Some(format!(
                "the connection to the server {server} dropped ({msg}): the network, or the server restarted. \
                 Waiting for it to come back."
            )),
            format!("connection to the server dropped, waiting for {server}…"),
            true,
            "stop waiting for the server and disconnect",
        ),
        Loss::NoAnalyser => (
            format!("{server}: the server has no analyser connected yet"),
            None,
            format!("{server}: no analyser on the server yet, waiting for one…"),
            false,
            "stop waiting and disconnect from the server",
        ),
        Loss::AnalyserGone => (
            format!("{server}: the server lost its analyser"),
            Some(format!(
                "the server {server} lost its analyser: it left the server's USB and did not come back. \
                 Waiting for one to be plugged in there."
            )),
            format!("{server}: the server lost its analyser, waiting for one…"),
            true,
            "stop waiting and disconnect from the server",
        ),
        Loss::AnalyserLost => (
            format!("{server}: the analyser dropped off the server's USB ({msg}); the server waits for it"),
            Some(format!(
                "the analyser of the server {server} dropped off its USB ({msg}): the cable or connector moved, \
                 or it restarted. The server waits for it to come back."
            )),
            format!("{server}: the analyser dropped off the server, waiting…"),
            true,
            "stop waiting and disconnect from the server",
        ),
    };
    LostTexts {
        log,
        error,
        waiting,
        trouble,
        cancel_hover,
    }
}

/// The calibration file of each segment of a display sweep that came back
/// on `cfgs`. A server sends every client the sweeps of every client: the
/// per-segment files of this program belong only to the segments it sent.
pub(super) fn plan_cals(cfgs: &[SweepConfig], sent: &[SweepConfig], sent_cals: &[Option<PathBuf>]) -> Vec<Option<PathBuf>> {
    if cfgs == sent && sent_cals.len() == cfgs.len() {
        sent_cals.to_vec()
    } else {
        vec![None; cfgs.len()]
    }
}

/// The port to select at start: the stored one when it is there, a serial
/// port in the list or a server among the known ones.
pub(super) fn restored_port(stored: Option<&str>, ports: &[PortCandidate], servers: &[String]) -> Option<String> {
    let p = stored?;
    let known = if is_server_address(p) {
        servers.iter().any(|s| s == p)
    } else {
        ports.iter().any(|c| c.path == p)
    };
    known.then(|| p.to_string())
}

impl App {
    /// Device ▸ Add server…: one address field, and the servers known, each with **remove**.
    pub(super) fn add_server_window(&mut self, ctx: &egui::Context) {
        if !self.show_add_server {
            return;
        }
        let mut open = true;
        let mut add = false;
        let mut remove: Option<String> = None;
        let in_use = match &self.connection {
            Connection::Disconnected => None,
            Connection::Connecting(p) | Connection::Connected { port: p, .. } | Connection::Reconnecting { port: p, .. } => Some(p.clone()),
        };
        egui::Window::new("Add server")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Address of a computer that runs vna-server with the analyser on its USB:");
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.server_input)
                            .hint_text("vna.local")
                            .desired_width(220.0),
                    );
                    let entered = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if r.changed() {
                        self.server_input_error = None;
                    }
                    add = ui.button("Add").clicked() || entered;
                });
                if let Some(e) = &self.server_input_error {
                    ui.colored_label(Color32::from_rgb(255, 110, 110), e);
                }
                ui.small(if NATIVE {
                    "It then appears in the port list, under the serial ports; select it and press Connect."
                } else {
                    "It then appears in the port list in the top bar; select it and press Connect."
                });
                if !self.servers.is_empty() {
                    ui.separator();
                    for s in &self.servers {
                        ui.horizontal(|ui| {
                            ui.label(server_label(s)).on_hover_text(s);
                            let busy = in_use.as_deref() == Some(s.as_str());
                            if ui
                                .add_enabled(!busy, egui::Button::new("remove").small())
                                .on_hover_text("forget this server")
                                .on_disabled_hover_text("in use: disconnect first")
                                .clicked()
                            {
                                remove = Some(s.clone());
                            }
                        });
                    }
                }
            });
        if add {
            self.add_server();
        }
        if let Some(s) = remove {
            self.servers.retain(|k| k != &s);
            if self.selected_port.as_deref() == Some(s.as_str()) {
                self.selected_port = None;
                self.refresh_ports();
            }
            self.log(format!("removed server {}", server_label(&s)));
        }
        self.show_add_server &= open;
    }

    /// Take what is in the address field: known from now on, and selected.
    fn add_server(&mut self) {
        let Some(url) = server_url(&self.server_input) else {
            self.server_input_error =
                Some("Not an address: type the server's name or IP address, e.g. vna.local or 192.168.1.5:8080.".into());
            return;
        };
        if !self.servers.contains(&url) {
            self.servers.push(url.clone());
        }
        let label = server_label(&url);
        self.log(format!("added server {label}"));
        self.selected_port = Some(url);
        self.server_input.clear();
        self.server_input_error = None;
        self.show_add_server = false;
        self.notify(format!("server {label} added: Connect connects to it"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_for_each_kind_of_wait() {
        let usb = lost_texts("/dev/ttyACM0", "sweep: I/O error: Broken pipe", Loss::Device);
        assert!(usb.error.as_deref().unwrap().contains("the cable or connector moved"));
        assert_eq!(usb.waiting, "connection dropped, waiting for /dev/ttyACM0…");
        let url = "ws://vna.local:8080/ws";
        // right after Connect: nothing dropped, nothing red
        let yet = lost_texts(url, "the server has no analyser connected yet", Loss::NoAnalyser);
        assert_eq!(yet.error, None);
        assert!(!yet.trouble);
        assert_eq!(yet.waiting, "vna.local:8080: no analyser on the server yet, waiting for one…");
        assert!(!yet.waiting.contains("dropped"));
        let net = lost_texts(url, "read: Connection reset by peer", Loss::Server);
        assert!(net.waiting.starts_with("connection to the server dropped"));
        assert!(net.error.as_deref().unwrap().contains("Connection reset by peer"));
        assert!(!net.error.as_deref().unwrap().contains("cable"));
        let usb_there = lost_texts(url, "/dev/ttyACM0: sweep: I/O error: Broken pipe", Loss::AnalyserLost);
        assert!(usb_there
            .error
            .as_deref()
            .unwrap()
            .contains("server vna.local:8080 dropped off its USB (/dev/ttyACM0"));
        let gone = lost_texts(url, "the server lost its analyser", Loss::AnalyserGone);
        assert!(gone.waiting.contains("lost its analyser"));
        assert_eq!(port_label(url), "server vna.local:8080");
        assert_eq!(port_label("/dev/ttyACM0"), "/dev/ttyACM0");
    }

    #[test]
    fn a_sweep_this_program_did_not_send_gets_no_segment_files() {
        let a = SweepConfig::linear(1_000_000, 30_000_000, 101).unwrap();
        let b = SweepConfig::linear(30_000_000, 450_000_000, 51).unwrap();
        let cals = vec![None, Some(PathBuf::from("hf.cal.json"))];
        assert_eq!(plan_cals(&[a, b], &[a, b], &cals), cals);
        // someone else's two segments, the same number but other ranges
        let c = SweepConfig::linear(1_000_000, 2_000_000, 101).unwrap();
        assert_eq!(plan_cals(&[a, c], &[a, b], &cals), vec![None, None]);
        assert_eq!(plan_cals(&[a], &[a, b], &cals), vec![None]);
    }

    #[test]
    fn a_stored_port_is_selected_again_when_it_is_known() {
        let ports = vec![PortCandidate {
            path: "/dev/ttyACM0".into(),
            usb: None,
        }];
        let servers = vec!["ws://vna.local:8080/ws".to_string()];
        assert_eq!(
            restored_port(Some("/dev/ttyACM0"), &ports, &servers).as_deref(),
            Some("/dev/ttyACM0")
        );
        assert_eq!(
            restored_port(Some("ws://vna.local:8080/ws"), &ports, &servers).as_deref(),
            Some("ws://vna.local:8080/ws")
        );
        assert_eq!(restored_port(Some("/dev/ttyACM1"), &ports, &servers), None);
        assert_eq!(restored_port(Some("ws://other:8080/ws"), &ports, &servers), None);
        assert_eq!(restored_port(None, &ports, &servers), None);
    }
}
