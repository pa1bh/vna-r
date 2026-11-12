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
//!
//! A link that drops (a cable that moves, a device that restarts: either way
//! the device leaves the USB bus and is usually back within seconds) is not
//! the end of the session. The worker notices it during a sweep, on the
//! battery reading, and while idle by looking whether the port still exists;
//! it then waits for the port to come back, reconnects and carries on with
//! what it was doing. See [`Lost`].

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

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
    /// The link to the device dropped. The worker waits for `port` to come
    /// back and reconnects; [`Event::Connected`] follows when that worked,
    /// [`Event::Disconnected`] and an error when it did not within
    /// [`RECONNECT_FOR`]. A continuous sweep resumes by itself.
    LinkLost {
        port: String,
        msg: String,
    },
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

/// How often an idle worker looks whether the port of its device still exists.
const IDLE_CHECK: Duration = Duration::from_millis(500);
/// How often a worker that lost its link looks whether the port is back.
const RECONNECT_POLL: Duration = Duration::from_millis(500);
/// How long a worker waits for a lost device to come back.
pub const RECONNECT_FOR: Duration = Duration::from_secs(30);
/// Links lost in a row after which the worker stops reconnecting: a device
/// that opens but fails every sweep would otherwise go round for ever. A loss
/// is "in a row" when the link had held for less than [`RELINK_WINDOW`] and no
/// sweep was completed on it.
const MAX_RELINKS: u8 = 3;
const RELINK_WINDOW: Duration = Duration::from_secs(10);

/// What to pick up again once a lost link is back.
#[derive(Debug, Clone, PartialEq)]
enum Resume {
    Nothing,
    /// Continuous sweeping went on.
    Continuous(Vec<SweepConfig>, Purpose, usize),
    /// A single display sweep was under way or asked for; do it once.
    Once(Vec<SweepConfig>, usize),
}

impl Resume {
    /// What a sweep that failed on the link leaves to be done. A calibration
    /// standard is not measured again behind the user's back: the device may
    /// have restarted, and whoever touched the cable may have touched the
    /// standard.
    fn after_failed_sweep(
        continuous: &Option<(Vec<SweepConfig>, Purpose, usize)>,
        cfgs: &[SweepConfig],
        purpose: Purpose,
        repeat: usize,
    ) -> Resume {
        match (continuous, purpose) {
            (Some((c, p, r)), _) => Resume::Continuous(c.clone(), *p, *r),
            (None, Purpose::Display) => Resume::Once(cfgs.to_vec(), repeat),
            (None, Purpose::Standard(_)) => Resume::Nothing,
        }
    }
}

/// A link that dropped and is being waited for.
struct Lost {
    since: Instant,
    resume: Resume,
}

/// What to do about a lost link right now.
#[derive(Debug, PartialEq, Eq)]
enum Retry {
    /// The port is not there (yet).
    Wait,
    /// The port exists: try to open it.
    Open,
    GiveUp,
}

fn retry_step(waited: Duration, port_present: bool) -> Retry {
    if waited > RECONNECT_FOR {
        Retry::GiveUp
    } else if port_present {
        Retry::Open
    } else {
        Retry::Wait
    }
}

/// Open the device on `port` and read what the UI shows about it.
fn open_device(port: &str) -> litevna::Result<(Vna, VnaInfo, Option<u16>)> {
    let mut d = Vna::open(port)?;
    let info = d.info()?;
    let battery_mv = d.battery_millivolts().ok();
    Ok((d, info, battery_mv))
}

struct Worker {
    dev: Option<Vna>,
    /// Path of the open port, to check whether it still exists; kept while
    /// the link is lost, as the port to wait for.
    port: Option<String>,
    /// Set while the link is lost and the worker waits for the port.
    lost: Option<Lost>,
    /// Links lost in a row, see [`MAX_RELINKS`].
    relinks: u8,
    /// When the device was last opened.
    linked_at: Option<Instant>,
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
        lost: None,
        relinks: 0,
        linked_at: None,
        continuous: None,
        cmds,
        backlog: VecDeque::new(),
        events,
        ctx,
    };
    loop {
        // First what arrived during the last sweep. Then: only peek when
        // sweeping continuously; wait a short while when the link is lost or
        // a device sits idle, so the port can be looked at in between; block
        // for a command when there is nothing to watch.
        let cmd = if let Some(c) = w.backlog.pop_front() {
            Some(c)
        } else if w.continuous.is_some() && w.lost.is_none() {
            match w.cmds.try_recv() {
                Ok(c) => Some(c),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else if w.lost.is_some() || w.dev.is_some() {
            let wait = if w.lost.is_some() { RECONNECT_POLL } else { IDLE_CHECK };
            match w.cmds.recv_timeout(wait) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match w.cmds.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match cmd {
            Some(cmd) => w.handle(cmd),
            None if w.lost.is_some() => w.try_reconnect(),
            None => {
                if let Some((cfgs, purpose, repeat)) = w.continuous.clone() {
                    w.sweep_once(cfgs, purpose, repeat);
                } else if w.dev.is_some() && w.port_vanished() {
                    w.lose("connection", "the device left the USB bus", Resume::Nothing);
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

    /// The link dropped: forget the handle, tell the UI and start waiting
    /// for the port. After [`MAX_RELINKS`] in a row it is a disconnect.
    fn lose(&mut self, context: &str, err: impl std::fmt::Display, resume: Resume) {
        let msg = format!("{context}: {err}");
        self.dev = None;
        self.continuous = None;
        if self.linked_at.is_some_and(|t| t.elapsed() > RELINK_WINDOW) {
            self.relinks = 0;
        }
        self.relinks += 1;
        let Some(port) = self.port.clone().filter(|_| self.relinks <= MAX_RELINKS) else {
            self.lost = None;
            self.port = None;
            self.emit(Event::Disconnected);
            self.emit(Event::Error {
                msg: format!("{msg} (the connection keeps dropping; not reconnecting again)"),
                stops_sweep: true,
            });
            return;
        };
        log::warn!("{msg}; waiting for {port} to come back");
        self.lost = Some(Lost {
            since: Instant::now(),
            resume,
        });
        self.emit(Event::LinkLost { port, msg });
    }

    /// One look at a lost link: reconnect when the port is back, give up
    /// after [`RECONNECT_FOR`].
    fn try_reconnect(&mut self) {
        let (Some(lost), Some(port)) = (&self.lost, self.port.clone()) else {
            return;
        };
        let waited = lost.since.elapsed();
        match retry_step(waited, !self.port_vanished()) {
            Retry::Wait => {}
            Retry::GiveUp => {
                self.lost = None;
                self.port = None;
                self.emit(Event::Disconnected);
                self.emit(Event::Error {
                    msg: format!("the device on {port} did not come back within {} s", RECONNECT_FOR.as_secs()),
                    stops_sweep: true,
                });
            }
            // A device that has only just appeared may not answer yet; the
            // next look tries again.
            Retry::Open => match open_device(&port) {
                Ok((d, info, battery_mv)) => {
                    log::info!("reconnected {port} after {:.1} s", waited.as_secs_f32());
                    let resume = self.lost.take().map(|l| l.resume).unwrap_or(Resume::Nothing);
                    self.dev = Some(d);
                    self.linked_at = Some(Instant::now());
                    self.emit(Event::Connected { port, info, battery_mv });
                    match resume {
                        Resume::Nothing => {}
                        Resume::Continuous(cfgs, purpose, repeat) => self.continuous = Some((cfgs, purpose, repeat)),
                        Resume::Once(cfgs, repeat) => self.sweep_once(cfgs, Purpose::Display, repeat),
                    }
                }
                Err(e) => log::debug!("reconnecting {port}: {e}"),
            },
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
                self.lost = None;
                self.relinks = 0;
                match open_device(&port) {
                    Ok((d, info, battery_mv)) => {
                        self.dev = Some(d);
                        self.linked_at = Some(Instant::now());
                        self.port = Some(port.clone());
                        self.emit(Event::Connected { port, info, battery_mv });
                    }
                    Err(e) => self.fail(&format!("connecting to {port}"), e),
                }
            }
            Cmd::Disconnect => {
                self.continuous = None;
                self.lost = None;
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
                if let Some(lost) = &mut self.lost {
                    // Asked for while the link is away: do it when it is back.
                    lost.resume = match (continuous, purpose) {
                        (true, _) => Resume::Continuous(cfgs, purpose, repeat),
                        (false, Purpose::Display) => Resume::Once(cfgs, repeat),
                        (false, Purpose::Standard(_)) => {
                            self.fail("sweep", "not connected: waiting for the device to come back");
                            return;
                        }
                    };
                    return;
                }
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
                if let Some(lost) = &mut self.lost {
                    lost.resume = Resume::Nothing;
                }
                if let Some(Err(e)) = self.dev.as_mut().map(|d| d.release()) {
                    if e.is_link_failure() {
                        self.lose("releasing device UI", e, Resume::Nothing);
                    } else {
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
                // A device without a readable battery just gives no answer;
                // a port that fails is a lost link.
                match self.dev.as_mut().map(|d| d.battery_millivolts()) {
                    Some(Ok(mv)) => self.emit(Event::Battery(mv)),
                    Some(Err(e)) if e.is_link_failure() => {
                        let resume = match self.continuous.clone() {
                            Some((cfgs, purpose, repeat)) => Resume::Continuous(cfgs, purpose, repeat),
                            None => Resume::Nothing,
                        };
                        self.lose("reading the battery", e, resume);
                    }
                    _ => {}
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
                        // The link went: the port reports an error, or (a pulled
                        // cable can also look like a timeout) the port is no
                        // longer there. Wait for it to come back.
                        if e.is_link_failure() || self.port_vanished() {
                            let resume = Resume::after_failed_sweep(&self.continuous, &cfgs, purpose, repeat);
                            self.lose("sweep", e, resume);
                        } else {
                            self.fail("sweep", e);
                        }
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
        self.relinks = 0;
        let result = SweepResult::average(&passes).expect("same grid every pass");
        self.emit(Event::SweepDone {
            cfgs,
            result,
            purpose,
            repeat,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SweepConfig {
        SweepConfig::linear(1_000_000, 30_000_000, 101).unwrap()
    }

    #[test]
    fn a_lost_link_is_waited_for_then_opened_then_given_up() {
        assert_eq!(retry_step(Duration::from_secs(1), false), Retry::Wait);
        assert_eq!(retry_step(Duration::from_secs(1), true), Retry::Open);
        assert_eq!(retry_step(RECONNECT_FOR, true), Retry::Open);
        // past the limit it is over, also when the port happens to be there
        assert_eq!(retry_step(RECONNECT_FOR + Duration::from_millis(1), false), Retry::GiveUp);
        assert_eq!(retry_step(RECONNECT_FOR + Duration::from_millis(1), true), Retry::GiveUp);
    }

    /// With a device: `VNA_R_TEST_PORT=/dev/… cargo test -p vna-gui -- --ignored --nocapture hardware_session`.
    /// Sweeps continuously for `VNA_R_TEST_SECONDS` (default 20), stops, sits
    /// idle for five seconds and does a single sweep, printing every event.
    /// Untouched, the link must hold. With `VNA_R_EXPECT_DROP=1` pull the
    /// cable and put it back while it runs: it must report the lost link,
    /// reconnect and sweep again.
    #[test]
    #[ignore = "needs a device on VNA_R_TEST_PORT"]
    fn hardware_session() {
        let port = std::env::var("VNA_R_TEST_PORT").expect("VNA_R_TEST_PORT");
        let secs: u64 = std::env::var("VNA_R_TEST_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
        let expect_drop = std::env::var("VNA_R_EXPECT_DROP").is_ok();
        let w = spawn(egui::Context::default());
        let t0 = Instant::now();
        let stamp = |t0: Instant| format!("{:6.1}s", t0.elapsed().as_secs_f32());
        let sweep = |continuous| Cmd::Sweep {
            cfgs: vec![SweepConfig::linear(50_000_000, 150_000_000, 801).unwrap()],
            purpose: Purpose::Display,
            continuous,
            repeat: 1,
        };
        w.tx.send(Cmd::Connect(port)).unwrap();
        w.tx.send(sweep(true)).unwrap();
        let (mut sweeps, mut lost, mut connects, mut sweeps_after_reconnect) = (0, 0, 0, 0);
        let mut phase = 0;
        loop {
            let limit = match phase {
                0 => Duration::from_secs(secs),
                1 => Duration::from_secs(secs + 5),
                _ => Duration::from_secs(secs + 15),
            };
            match w.rx.recv_timeout(limit.saturating_sub(t0.elapsed())) {
                Ok(Event::SweepDone { result, .. }) => {
                    sweeps += 1;
                    if connects > 1 {
                        sweeps_after_reconnect += 1;
                    }
                    if phase == 2 {
                        println!("{} single sweep after idle: {} points", stamp(t0), result.freqs_hz.len());
                        break;
                    }
                }
                Ok(Event::Progress { .. }) => {}
                Ok(Event::Connected { info, .. }) => {
                    connects += 1;
                    println!("{} connected: {}", stamp(t0), info.summary());
                }
                Ok(Event::LinkLost { port, msg }) => {
                    lost += 1;
                    println!("{} LINK LOST on {port}: {msg}", stamp(t0));
                }
                Ok(other) => println!("{} {other:?}", stamp(t0)),
                Err(RecvTimeoutError::Timeout) if phase == 0 => {
                    println!("{} {sweeps} sweeps; stop, then idle", stamp(t0));
                    w.tx.send(Cmd::Stop).unwrap();
                    phase = 1;
                }
                Err(RecvTimeoutError::Timeout) if phase == 1 => {
                    w.tx.send(sweep(false)).unwrap();
                    phase = 2;
                }
                Err(e) => panic!("{} no event: {e:?}", stamp(t0)),
            }
        }
        println!("{sweeps} sweeps, {lost} lost links, {connects} connects, {sweeps_after_reconnect} sweeps after a reconnect");
        if expect_drop {
            assert!(lost >= 1, "no lost link seen: was the cable pulled?");
            assert_eq!(connects, lost + 1, "every lost link is followed by a reconnect");
            assert!(sweeps_after_reconnect >= 1, "sweeping resumes after the reconnect");
        } else {
            assert_eq!((lost, connects), (0, 1), "the link must hold when nothing is touched");
            assert!(sweeps >= 2);
        }
    }

    #[test]
    fn what_resumes_after_a_sweep_that_lost_the_link() {
        let cfgs = vec![cfg()];
        // continuous sweeping goes on, whatever the failing sweep was
        let continuous = Some((cfgs.clone(), Purpose::Display, 1));
        assert_eq!(
            Resume::after_failed_sweep(&continuous, &cfgs, Purpose::Display, 1),
            Resume::Continuous(cfgs.clone(), Purpose::Display, 1)
        );
        // a single display sweep is done once more
        assert_eq!(
            Resume::after_failed_sweep(&None, &cfgs, Purpose::Display, 4),
            Resume::Once(cfgs.clone(), 4)
        );
        // a calibration standard is left to the user
        assert_eq!(
            Resume::after_failed_sweep(&None, &cfgs, Purpose::Standard(Standard::Open), 4),
            Resume::Nothing
        );
    }
}
