//! The device lives on its own thread: serial I/O blocks, the UI must not.
//!
//! The UI sends [`Cmd`]s and receives [`Event`]s over channels. After every
//! event the worker asks egui to repaint, so the UI never has to poll.
//!
//! A sweep can take long (a NanoVNA-F does about 115 points/s). While one
//! runs, the worker looks at the command queue after every block of data the
//! device delivers: a command that makes the sweep pointless (Stop,
//! Disconnect, a new sweep) cancels it there, anything else waits until the
//! sweep is done.

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;

use litevna::calibration::Standard;
use litevna::{Screenshot, SweepConfig, SweepResult, Vna, VnaInfo};

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
    /// One sweep over these segments (in order), or start sweeping them continuously.
    /// `repeat` > 1 sweeps that many times and reports the complex mean (used
    /// for calibration standards, so they get the same averaging as the display).
    Sweep {
        cfgs: Vec<SweepConfig>,
        purpose: Purpose,
        continuous: bool,
        repeat: usize,
    },
    /// Stop continuous sweeping and hand the screen back to the device.
    Stop,
    /// LiteVNA averaging register; ignored on devices without it.
    SetAverage(u8),
    /// Read the battery voltage again (it is read once at connect).
    Battery,
    Screenshot,
}

#[derive(Debug)]
pub enum Event {
    Connected {
        port: String,
        info: VnaInfo,
        battery_mv: Option<u16>,
    },
    Disconnected,
    /// `result` is the concatenation of the segments in `cfgs`, averaged over
    /// `repeat` sweeps.
    SweepDone {
        cfgs: Vec<SweepConfig>,
        result: SweepResult,
        purpose: Purpose,
        repeat: usize,
    },
    /// A sweep is under way: `done` of `total` points, over all segments and repeats.
    Progress {
        done: usize,
        total: usize,
    },
    /// A sweep was cut short by a later command (Stop, Disconnect, a new sweep); no result follows.
    Cancelled {
        purpose: Purpose,
    },
    Screenshot(Screenshot),
    /// The battery voltage, in answer to [`Cmd::Battery`].
    Battery(u16),
    /// Something failed. `stops_sweep` says whether continuous sweeping ended
    /// with it (a sweep or connection failure) or goes on (a screenshot, a
    /// register write).
    Error {
        msg: String,
        stops_sweep: bool,
    },
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
    dev: Option<Vna>,
    /// Path of the open port, to check whether it still exists after an error.
    port: Option<String>,
    continuous: Option<(Vec<SweepConfig>, Purpose, usize)>,
    cmds: Receiver<Cmd>,
    /// Commands read from `cmds` while a sweep ran, to be handled in order after it.
    backlog: VecDeque<Cmd>,
    events: Sender<Event>,
    ctx: egui::Context,
}

/// A command after which the result of a running sweep is no longer wanted.
/// A request to measure a calibration standard is not one: it waits for the
/// sweep under way, so the display has its data on the grid the standard is
/// about to be measured on.
fn interrupts(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Stop
            | Cmd::Disconnect
            | Cmd::Connect(_)
            | Cmd::Sweep {
                purpose: Purpose::Display,
                ..
            }
    )
}

fn run(cmds: Receiver<Cmd>, events: Sender<Event>, ctx: egui::Context) {
    let mut w = Worker {
        dev: None,
        port: None,
        continuous: None,
        cmds,
        backlog: VecDeque::new(),
        events,
        ctx,
    };
    loop {
        // First what arrived during the last sweep; then block for a command
        // when idle, and only peek when sweeping continuously.
        let cmd = if let Some(c) = w.backlog.pop_front() {
            Some(c)
        } else if w.continuous.is_some() {
            match w.cmds.try_recv() {
                Ok(c) => Some(c),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match w.cmds.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match cmd {
            Some(cmd) => w.handle(cmd),
            None => {
                if let Some((cfgs, purpose, repeat)) = w.continuous.clone() {
                    w.sweep_once(cfgs, purpose, repeat);
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

    /// A failure that ends continuous sweeping (sweep, connect).
    fn fail(&mut self, context: &str, err: impl std::fmt::Display) {
        self.continuous = None;
        self.emit(Event::Error {
            msg: format!("{context}: {err}"),
            stops_sweep: true,
        });
    }

    /// The port we opened is no longer among the serial ports of the system.
    fn port_vanished(&self) -> bool {
        match (&self.port, litevna::discovery::list_ports()) {
            (Some(path), Ok(ports)) => !ports.iter().any(|p| &p.path == path),
            _ => false,
        }
    }

    /// A failure of a side action; a running sweep carries on.
    fn warn(&mut self, context: &str, err: impl std::fmt::Display) {
        self.emit(Event::Error {
            msg: format!("{context}: {err}"),
            stops_sweep: false,
        });
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Connect(port) => {
                log::info!("connect {port}");
                self.dev = None;
                self.continuous = None;
                match Vna::open(&port).and_then(|mut d| {
                    let info = d.info()?;
                    let battery_mv = d.battery_millivolts().ok();
                    Ok((d, info, battery_mv))
                }) {
                    Ok((d, info, battery_mv)) => {
                        self.dev = Some(d);
                        self.port = Some(port.clone());
                        self.emit(Event::Connected { port, info, battery_mv });
                    }
                    Err(e) => self.fail(&format!("connecting to {port}"), e),
                }
            }
            Cmd::Disconnect => {
                self.continuous = None;
                self.dev = None;
                self.port = None;
                self.emit(Event::Disconnected);
            }
            Cmd::Sweep {
                cfgs,
                purpose,
                continuous,
                repeat,
            } => {
                if self.dev.is_none() {
                    self.fail("sweep", "not connected");
                    return;
                }
                if continuous {
                    self.continuous = Some((cfgs, purpose, repeat));
                } else {
                    self.sweep_once(cfgs, purpose, repeat);
                }
            }
            Cmd::Stop => {
                self.continuous = None;
                if let Some(d) = &mut self.dev {
                    if let Err(e) = d.release() {
                        self.warn("releasing device UI", e);
                    }
                }
            }
            Cmd::SetAverage(avg) => {
                if let Some(d) = self.dev.as_mut().and_then(|d| d.as_lite_mut()) {
                    if let Err(e) = d.set_average(avg) {
                        self.warn("setting average", e);
                    }
                }
            }
            Cmd::Battery => {
                // A device without a readable battery just gives no answer.
                if let Some(Ok(mv)) = self.dev.as_mut().map(|d| d.battery_millivolts()) {
                    self.emit(Event::Battery(mv));
                }
            }
            Cmd::Screenshot => {
                let Some(d) = &mut self.dev else {
                    self.fail("screenshot", "not connected");
                    return;
                };
                match d.screenshot() {
                    Ok(shot) => self.emit(Event::Screenshot(shot)),
                    Err(e) => self.warn("screenshot", e),
                }
            }
        }
    }

    fn sweep_once(&mut self, cfgs: Vec<SweepConfig>, purpose: Purpose, repeat: usize) {
        let repeat = repeat.max(1);
        let total = cfgs.iter().map(|c| c.points as usize).sum::<usize>() * repeat;
        // Points of the segments and passes that are complete.
        let mut before = 0usize;
        let mut passes: Vec<SweepResult> = Vec::with_capacity(repeat);
        for _ in 0..repeat {
            let mut parts = Vec::with_capacity(cfgs.len());
            for cfg in &cfgs {
                if self.dev.is_none() {
                    self.fail("sweep", "not connected");
                    return;
                }
                if self.backlog.iter().any(interrupts) {
                    self.emit(Event::Cancelled { purpose });
                    return;
                }
                let Worker {
                    dev,
                    cmds,
                    backlog,
                    events,
                    ctx,
                    ..
                } = self;
                let d = dev.as_mut().expect("checked above");
                let mut progress = |done: usize, _segment_total: usize| {
                    let _ = events.send(Event::Progress {
                        done: before + done,
                        total,
                    });
                    ctx.request_repaint();
                    while let Ok(c) = cmds.try_recv() {
                        let stop = interrupts(&c);
                        backlog.push_back(c);
                        if stop {
                            return ControlFlow::Break(());
                        }
                    }
                    ControlFlow::Continue(())
                };
                match d.sweep_with(cfg, false, &mut progress) {
                    Ok(r) => {
                        before += cfg.points as usize;
                        parts.push(r);
                    }
                    Err(litevna::Error::Cancelled) => {
                        // The command in the backlog says what happens next.
                        self.emit(Event::Cancelled { purpose });
                        return;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        // The cable went: the port reports an error, or (a pulled
                        // cable can also look like a timeout) the port is no
                        // longer there. Forget the device.
                        if e.is_link_failure() || self.port_vanished() {
                            self.dev = None;
                            self.port = None;
                            self.continuous = None;
                            self.emit(Event::Disconnected);
                        }
                        self.fail("sweep", msg);
                        return;
                    }
                }
            }
            passes.push(if parts.len() == 1 {
                parts.pop().unwrap()
            } else {
                SweepResult::concat(&parts)
            });
        }
        let result = SweepResult::average(&passes).expect("same grid every pass");
        self.emit(Event::SweepDone {
            cfgs,
            result,
            purpose,
            repeat,
        });
    }
}
