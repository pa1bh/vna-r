//! The device lives on its own thread: serial I/O blocks, the UI must not.
//!
//! The UI sends [`Cmd`]s and receives [`Event`]s over channels. After every
//! event the worker asks egui to repaint, so the UI never has to poll.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;

use litevna::calibration::Standard;
use litevna::{DeviceInfo, LiteVna, Screenshot, SweepConfig, SweepResult};

/// Why a sweep was requested; decides what the UI does with the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Display,
    Standard(Standard),
}

#[derive(Debug)]
pub enum Cmd {
    Connect(String),
    Disconnect,
    /// One sweep, or start sweeping continuously with this config.
    Sweep { cfg: SweepConfig, purpose: Purpose, continuous: bool },
    /// Stop continuous sweeping and hand the screen back to the device.
    Stop,
    SetAverage(u8),
    Screenshot,
}

#[derive(Debug)]
pub enum Event {
    Connected { port: String, info: DeviceInfo, battery_mv: Option<u16> },
    Disconnected,
    SweepDone { cfg: SweepConfig, result: SweepResult, purpose: Purpose },
    Screenshot(Screenshot),
    Error(String),
}

pub struct WorkerHandle {
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Event>,
}

pub fn spawn(ctx: egui::Context) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (evt_tx, evt_rx) = channel::<Event>();
    thread::Builder::new()
        .name("litevna-worker".into())
        .spawn(move || run(cmd_rx, evt_tx, ctx))
        .expect("spawn worker thread");
    WorkerHandle { tx: cmd_tx, rx: evt_rx }
}

struct Worker {
    dev: Option<LiteVna>,
    continuous: Option<(SweepConfig, Purpose)>,
    events: Sender<Event>,
    ctx: egui::Context,
}

fn run(cmds: Receiver<Cmd>, events: Sender<Event>, ctx: egui::Context) {
    let mut w = Worker { dev: None, continuous: None, events, ctx };
    loop {
        // Block for a command when idle; only peek when sweeping continuously.
        let cmd = if w.continuous.is_some() {
            match cmds.try_recv() {
                Ok(c) => Some(c),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match cmds.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match cmd {
            Some(cmd) => w.handle(cmd),
            None => {
                if let Some((cfg, purpose)) = w.continuous {
                    w.sweep_once(cfg, purpose);
                }
            }
        }
    }
    // Dropping the device hands the screen back to the on-device UI.
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
        self.ctx.request_repaint();
    }

    fn fail(&mut self, context: &str, err: impl std::fmt::Display) {
        self.continuous = None;
        self.emit(Event::Error(format!("{context}: {err}")));
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Connect(port) => {
                self.dev = None;
                self.continuous = None;
                match LiteVna::open(&port).and_then(|mut d| {
                    d.indicate()?;
                    let info = d.device_info()?;
                    let battery_mv = d.battery_millivolts().ok();
                    Ok((d, info, battery_mv))
                }) {
                    Ok((d, info, battery_mv)) => {
                        self.dev = Some(d);
                        self.emit(Event::Connected { port, info, battery_mv });
                    }
                    Err(e) => self.fail(&format!("connecting to {port}"), e),
                }
            }
            Cmd::Disconnect => {
                self.continuous = None;
                self.dev = None;
                self.emit(Event::Disconnected);
            }
            Cmd::Sweep { cfg, purpose, continuous } => {
                if self.dev.is_none() {
                    self.fail("sweep", "not connected");
                    return;
                }
                if continuous {
                    self.continuous = Some((cfg, purpose));
                } else {
                    self.sweep_once(cfg, purpose);
                }
            }
            Cmd::Stop => {
                self.continuous = None;
                if let Some(d) = &mut self.dev {
                    if let Err(e) = d.exit_usb_mode() {
                        self.fail("releasing device UI", e);
                    }
                }
            }
            Cmd::SetAverage(avg) => {
                if let Some(d) = &mut self.dev {
                    if let Err(e) = d.set_average(avg) {
                        self.fail("setting average", e);
                    }
                }
            }
            Cmd::Screenshot => {
                let Some(d) = &mut self.dev else {
                    self.fail("screenshot", "not connected");
                    return;
                };
                match d.screenshot() {
                    Ok(shot) => self.emit(Event::Screenshot(shot)),
                    Err(e) => self.fail("screenshot", e),
                }
            }
        }
    }

    fn sweep_once(&mut self, cfg: SweepConfig, purpose: Purpose) {
        let Some(d) = &mut self.dev else {
            self.fail("sweep", "not connected");
            return;
        };
        match d.sweep(&cfg, false) {
            Ok(result) => self.emit(Event::SweepDone { cfg, result, purpose }),
            Err(e) => {
                let msg = e.to_string();
                // A broken pipe means the cable went; forget the device.
                if matches!(e, litevna::Error::Io(_) | litevna::Error::Serial(_)) {
                    self.dev = None;
                    self.continuous = None;
                    self.emit(Event::Disconnected);
                }
                self.fail("sweep", msg);
            }
        }
    }
}
