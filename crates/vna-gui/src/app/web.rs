//! The browser build: file dialogs that answer later.
//!
//! A browser gives no path and does not block: picking a file is a future
//! that ends with the file's name and bytes, saving is a download under a
//! proposed name. The futures run on the browser's own event loop; what they
//! picked comes back over a channel and is taken over in the next frame
//! ([`App::handle_picked`]).

use std::sync::mpsc::{channel, Receiver, Sender};

use litevna::touchstone::read_touchstone_full;

use super::*;

/// What a picked file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickFor {
    Trace,
    Reference,
    Calibration,
}

pub(super) struct Picked {
    purpose: PickFor,
    name: String,
    bytes: Vec<u8>,
}

pub(super) struct WebFiles {
    ctx: egui::Context,
    tx: Sender<Picked>,
    rx: Receiver<Picked>,
}

impl WebFiles {
    pub(super) fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        WebFiles { ctx, tx, rx }
    }

    /// Ask for a file; it arrives through [`App::handle_picked`], or never when the user cancels.
    pub(super) fn pick(&self, purpose: PickFor, filter: &'static str, extensions: &'static [&'static str]) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(file) = rfd::AsyncFileDialog::new().add_filter(filter, extensions).pick_file().await else {
                return;
            };
            let picked = Picked {
                purpose,
                name: file.file_name(),
                bytes: file.read().await,
            };
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
    }

    /// Offer `bytes` as a download called `name`.
    pub(super) fn save(&self, name: String, bytes: Vec<u8>) {
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(file) = rfd::AsyncFileDialog::new().set_file_name(name.as_str()).save_file().await {
                if let Err(e) = file.write(&bytes).await {
                    log::warn!("saving {name}: {e}");
                }
            }
        });
    }
}

impl App {
    /// Take over the files the browser dialogs delivered since the last frame.
    pub(super) fn handle_picked(&mut self) {
        while let Ok(picked) = self.web.rx.try_recv() {
            let Picked { purpose, name, bytes } = picked;
            match purpose {
                PickFor::Trace | PickFor::Reference => {
                    // As with a path: the extension sets the port count.
                    let ports = match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
                        Some("s1p") => Some(1),
                        Some("s2p") => Some(2),
                        _ => None,
                    };
                    let file = read_touchstone_full(bytes.as_slice(), ports);
                    self.show_touchstone(name.clone(), &name, file, purpose == PickFor::Reference);
                }
                PickFor::Calibration => {
                    let text = String::from_utf8(bytes).map_err(|e| e.to_string());
                    self.load_cal_text(text, Path::new(&name));
                }
            }
        }
    }
}
