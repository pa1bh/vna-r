//! The hub: the one place that owns the worker's command channel and the
//! clients. Everything arrives on one channel ([`ToHub`]) and is handled in
//! order, so a client that joins sees the state as it was at that moment and
//! then every event after it.
//!
//! It never waits for a client: a message for a client goes into that
//! client's [`Outbox`], which its own thread empties onto the socket.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use vna_link::wire::{self, ClientMsg, Hello, ServerMsg};
use vna_link::{Cmd, Event, Options, Purpose, SIMULATOR};

use crate::{Config, DeviceChoice, PortCheck, VERSION};

/// What the hub is told.
// One at a time through a channel, each handled at once: boxing the event
// would only cost an allocation per message.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ToHub {
    Join {
        id: u64,
        peer: SocketAddr,
        origin: Option<String>,
        outbox: Arc<Outbox>,
    },
    /// The client is gone. `ack` is answered once the hub has forgotten it.
    Leave {
        id: u64,
        why: String,
        ack: Option<Sender<()>>,
    },
    Message {
        id: u64,
        msg: Result<ClientMsg, wire::Error>,
    },
    Event(Event),
    /// Let go of the device, then answer.
    Shutdown(Sender<()>),
}

/// Messages queued for a client beyond this, or this many bytes, and it is
/// dropped: it does not read, and only its own messages could pile up.
const MAX_QUEUED: usize = 256;
const MAX_QUEUED_BYTES: usize = 16 << 20;

/// How long a failed connect waits before the next try, at first and at most.
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// How long shutting down waits for the worker to let go of the device.
const RELEASE_FOR: Duration = Duration::from_secs(5);

/// The prefix of the worker's error when `Cmd::Connect` fails
/// (`fail(&format!("connecting to {port}"), e)` in the worker).
const CONNECT_FAILED: &str = "connecting to ";

/// How a queued message is treated when a client falls behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Only the newest one matters: a newer one replaces it.
    Progress,
    /// A display sweep: likewise.
    Display,
    /// Kept, in order.
    Other,
}

#[derive(Default)]
struct Queue {
    items: VecDeque<(Kind, Arc<str>)>,
    bytes: usize,
    closed: bool,
}

/// The messages waiting for one client.
#[derive(Default)]
pub(crate) struct Outbox {
    queue: Mutex<Queue>,
}

impl Outbox {
    /// Queue a message; `false` when the client is too far behind, which
    /// closes the outbox.
    pub(crate) fn push(&self, kind: Kind, text: Arc<str>) -> bool {
        let mut q = self.queue.lock().expect("outbox");
        if q.closed {
            return true;
        }
        if kind != Kind::Other {
            // Appended at the end rather than put in the old one's place,
            // so the order with the other messages stays right: a sweep that
            // is done comes before the progress of the next one.
            if let Some(i) = q.items.iter().position(|(k, _)| *k == kind) {
                let (_, old) = q.items.remove(i).expect("found");
                q.bytes -= old.len();
            }
        }
        q.bytes += text.len();
        q.items.push_back((kind, text));
        if q.items.len() > MAX_QUEUED || q.bytes > MAX_QUEUED_BYTES {
            q.items.clear();
            q.bytes = 0;
            q.closed = true;
            return false;
        }
        true
    }

    /// What is waiting, and whether the hub has closed the outbox (after
    /// which nothing more comes).
    pub(crate) fn take(&self) -> (Vec<Arc<str>>, bool) {
        let mut q = self.queue.lock().expect("outbox");
        q.bytes = 0;
        let items = q.items.drain(..).map(|(_, t)| t).collect();
        (items, q.closed)
    }

    /// No more messages; those queued are still delivered.
    pub(crate) fn close(&self) {
        self.queue.lock().expect("outbox").closed = true;
    }
}

struct Client {
    id: u64,
    peer: SocketAddr,
    outbox: Arc<Outbox>,
}

#[derive(Debug, Clone, PartialEq)]
enum DeviceState {
    /// No device; looking again at `next_search`.
    Searching,
    /// `Cmd::Connect` sent for this port.
    Connecting(String),
    Connected(String),
    /// The worker waits for the port to come back.
    Lost(String),
}

/// Start the worker, the forwarder and the hub.
/// The number is that of the clients connected, kept up to date by the hub.
pub(crate) fn start(config: &Config) -> (Sender<ToHub>, Arc<AtomicUsize>) {
    let (tx, rx) = channel();
    let (commands, events) = vna_link::spawn_worker(
        // The forwarder below blocks on the events, it needs no wake-up.
        Arc::new(|| {}),
        Options {
            announce_continuous: true,
            ..Options::default()
        },
    );
    let count = Arc::new(AtomicUsize::new(0));
    let forward = tx.clone();
    thread::Builder::new()
        .name("forwarder".into())
        .spawn(move || {
            for e in events {
                if forward.send(ToHub::Event(e)).is_err() {
                    break;
                }
            }
        })
        .expect("spawn forwarder thread");
    let disconnected: Arc<str> = ServerMsg::Event(Event::Disconnected).encode().into();
    let hub = Hub {
        worker: commands,
        clients: Vec::new(),
        count: count.clone(),
        choice: config.device.clone(),
        port_present: config.port_present.clone(),
        search_every: config.search_every,
        state: DeviceState::Searching,
        next_search: Instant::now(),
        said_searching: false,
        failures: 0,
        last_failure: None,
        standard_asker: None,
        screenshot_asker: None,
        hello: ServerMsg::Hello(Hello::new(VERSION, config.name.clone())).encode().into(),
        no_device: notice("the server has no analyser connected; it connects to one as soon as it is plugged in"),
        device_line: disconnected,
        continuous: false,
        last_sweep: None,
    };
    thread::Builder::new()
        .name("hub".into())
        .spawn(move || hub.run(rx))
        .expect("spawn hub thread");
    (tx, count)
}

fn notice(msg: &str) -> Arc<str> {
    ServerMsg::Event(Event::Error {
        msg: msg.into(),
        stops_sweep: false,
    })
    .encode()
    .into()
}

struct Hub {
    worker: Sender<Cmd>,
    clients: Vec<Client>,
    /// `clients.len()`, for whoever holds the server.
    count: Arc<AtomicUsize>,
    choice: DeviceChoice,
    port_present: Option<PortCheck>,
    search_every: Duration,
    state: DeviceState,
    next_search: Instant,
    /// "Looking for an analyser" is logged once per search, not every round.
    said_searching: bool,
    /// Failed connects in a row, and the last message (logged once).
    failures: u32,
    last_failure: Option<String>,
    standard_asker: Option<u64>,
    screenshot_asker: Option<u64>,
    // What a client that joins is sent, as encoded text.
    hello: Arc<str>,
    no_device: Arc<str>,
    /// The last `Connected`, `LinkLost` or `Disconnected`.
    device_line: Arc<str>,
    continuous: bool,
    /// The last display sweep.
    last_sweep: Option<Arc<str>>,
}

impl Hub {
    fn run(mut self, rx: Receiver<ToHub>) {
        loop {
            if self.state == DeviceState::Searching && Instant::now() >= self.next_search {
                self.search();
            }
            let wait = match self.state {
                DeviceState::Searching => self.next_search.saturating_duration_since(Instant::now()),
                _ => Duration::from_secs(1),
            };
            match rx.recv_timeout(wait) {
                Ok(ToHub::Shutdown(done)) => {
                    self.shut_down(&rx);
                    let _ = done.send(());
                    return;
                }
                Ok(m) => self.handle(m),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn handle(&mut self, m: ToHub) {
        match m {
            ToHub::Join { id, peer, origin, outbox } => self.join(id, peer, origin, outbox),
            ToHub::Leave { id, why, ack } => {
                if let Some(i) = self.clients.iter().position(|c| c.id == id) {
                    let c = self.clients.remove(i);
                    self.count.store(self.clients.len(), Ordering::SeqCst);
                    log::info!("client {id} ({}) left: {why}; {} connected", c.peer, self.clients.len());
                    self.forget(id);
                }
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
            }
            ToHub::Message { id, msg } => self.message(id, msg),
            ToHub::Event(e) => self.event(e),
            ToHub::Shutdown(_) => unreachable!("handled in run"),
        }
    }

    fn join(&mut self, id: u64, peer: SocketAddr, origin: Option<String>, outbox: Arc<Outbox>) {
        log::info!(
            "client {id} connected from {peer}, Origin {}; {} connected",
            origin.as_deref().unwrap_or("none"),
            self.clients.len() + 1
        );
        // Bring it up to date with ordinary events: the device, whether it
        // sweeps, the last sweep. A few messages, so they always fit.
        // `Continuous` comes either way, so that a client that reconnects
        // knows after it whether it must start its sweeping again.
        outbox.push(Kind::Other, self.hello.clone());
        outbox.push(Kind::Other, self.device_line.clone());
        if !self.has_device() {
            outbox.push(Kind::Other, self.no_device.clone());
        }
        outbox.push(Kind::Other, encode(Event::Continuous(self.continuous)));
        if let Some(s) = &self.last_sweep {
            outbox.push(Kind::Display, s.clone());
        }
        self.clients.push(Client { id, peer, outbox });
        self.count.store(self.clients.len(), Ordering::SeqCst);
    }

    /// A client is gone: forget what it asked for, and stop the device when
    /// nobody is watching any more.
    fn forget(&mut self, id: u64) {
        if self.standard_asker == Some(id) {
            self.standard_asker = None;
        }
        if self.screenshot_asker == Some(id) {
            self.screenshot_asker = None;
        }
        if self.clients.is_empty() && self.has_device() {
            log::info!("no clients left: stopping, the device gets its screen back");
            let _ = self.worker.send(Cmd::Stop);
            // The worker will say so too; a client that joins before that
            // must not be told the device is sweeping.
            self.continuous = false;
        }
    }

    fn has_device(&self) -> bool {
        matches!(self.state, DeviceState::Connected(_) | DeviceState::Lost(_))
    }

    fn message(&mut self, id: u64, msg: Result<ClientMsg, wire::Error>) {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("client {id}: {e}");
                self.send_to(
                    Some(id),
                    encode(Event::Error {
                        msg: format!("the server could not read a message: {e}"),
                        stops_sweep: false,
                    }),
                );
                return;
            }
        };
        if !self.has_device() {
            // Only what asks for an answer gets one.
            if matches!(msg, ClientMsg::Sweep { .. } | ClientMsg::Screenshot) {
                self.send_to(
                    Some(id),
                    encode(Event::Error {
                        msg: "the server has no analyser connected".into(),
                        stops_sweep: true,
                    }),
                );
            }
            return;
        }
        match &msg {
            ClientMsg::Sweep {
                purpose: Purpose::Standard(_),
                ..
            } => self.standard_asker = Some(id),
            ClientMsg::Screenshot => self.screenshot_asker = Some(id),
            _ => {}
        }
        let _ = self.worker.send(Cmd::from(msg));
    }

    fn event(&mut self, e: Event) {
        match &e {
            Event::Connected { port, info, .. } => {
                if matches!(self.state, DeviceState::Lost(_)) {
                    log::info!("the device on {port} is back");
                } else {
                    log::info!("connected to {} on {port}", info.summary());
                }
                self.state = DeviceState::Connected(port.clone());
                self.failures = 0;
                self.last_failure = None;
                self.said_searching = false;
            }
            Event::LinkLost { port, msg, .. } => {
                log::warn!("lost the device on {port} ({msg}); waiting for it to come back");
                self.state = DeviceState::Lost(port.clone());
            }
            Event::Disconnected => {
                log::info!("the device is gone; looking for one again");
                self.state = DeviceState::Searching;
                self.next_search = Instant::now() + self.search_every;
                self.last_sweep = None;
                self.continuous = false;
            }
            Event::Error { msg, .. } if msg.starts_with(CONNECT_FAILED) => {
                if let DeviceState::Connecting(_) = self.state {
                    // A try of our own, not something a client asked for.
                    self.connect_failed(msg);
                    return;
                }
            }
            Event::Continuous(on) => self.continuous = *on,
            _ => {}
        }
        let (kind, only) = match &e {
            Event::Progress {
                purpose: Purpose::Display, ..
            } => (Kind::Progress, None),
            // Like the result: another client would wait for an end it never hears of.
            Event::Progress {
                purpose: Purpose::Standard(_),
                ..
            } => (Kind::Progress, Some(self.standard_asker)),
            Event::SweepDone {
                purpose: Purpose::Display, ..
            } => (Kind::Display, None),
            Event::SweepDone {
                purpose: Purpose::Standard(_),
                ..
            }
            | Event::Cancelled {
                purpose: Purpose::Standard(_),
            } => (Kind::Other, Some(self.standard_asker)),
            Event::Screenshot(_) => (Kind::Other, Some(self.screenshot_asker)),
            _ => (Kind::Other, None),
        };
        let is_device_line = matches!(e, Event::Connected { .. } | Event::LinkLost { .. } | Event::Disconnected);
        let text = encode(e);
        if is_device_line {
            self.device_line = text.clone();
        }
        if kind == Kind::Display {
            self.last_sweep = Some(text.clone());
        }
        match only {
            Some(asker) => self.send_kind_to(asker, kind, text),
            None => self.broadcast(kind, text),
        }
    }

    fn broadcast(&mut self, kind: Kind, text: Arc<str>) {
        let behind: Vec<u64> = self
            .clients
            .iter()
            .filter(|c| !c.outbox.push(kind, text.clone()))
            .map(|c| c.id)
            .collect();
        for id in behind {
            self.drop_client(id);
        }
    }

    fn send_to(&mut self, id: Option<u64>, text: Arc<str>) {
        self.send_kind_to(id, Kind::Other, text);
    }

    fn send_kind_to(&mut self, id: Option<u64>, kind: Kind, text: Arc<str>) {
        let Some(c) = id.and_then(|id| self.clients.iter().find(|c| c.id == id)) else {
            return;
        };
        if !c.outbox.push(kind, text) {
            let id = c.id;
            self.drop_client(id);
        }
    }

    fn drop_client(&mut self, id: u64) {
        if let Some(i) = self.clients.iter().position(|c| c.id == id) {
            let c = self.clients.remove(i);
            self.count.store(self.clients.len(), Ordering::SeqCst);
            log::warn!("client {id} ({}) cannot keep up; dropped", c.peer);
            self.forget(id);
        }
    }

    /// Look for the device and connect to it when it is there.
    fn search(&mut self) {
        let port = match &self.choice {
            DeviceChoice::Simulator(rate) => Some(match rate {
                Some(r) => format!("{SIMULATOR}{r}"),
                None => SIMULATOR.to_string(),
            }),
            DeviceChoice::Port(p) => self.present(p).then(|| p.clone()),
            DeviceChoice::Auto => match litevna::discovery::list_ports() {
                Ok(ports) => ports.into_iter().find(|p| p.looks_like_vna()).map(|p| p.path),
                Err(e) => {
                    log::debug!("listing serial ports: {e}");
                    None
                }
            },
        };
        match port {
            Some(port) => {
                if self.failures == 0 {
                    log::info!("connecting to {port}");
                }
                let _ = self.worker.send(Cmd::Connect(port.clone()));
                self.state = DeviceState::Connecting(port);
            }
            None => {
                if !self.said_searching {
                    self.said_searching = true;
                    match &self.choice {
                        DeviceChoice::Port(p) => log::info!("{p} is not there; looking for it every {:?}", self.search_every),
                        _ => log::info!("no analyser found; looking for one every {:?}", self.search_every),
                    }
                }
                self.next_search = Instant::now() + self.search_every;
            }
        }
    }

    fn present(&self, port: &str) -> bool {
        if let Some(check) = &self.port_present {
            return check(port);
        }
        if port.starts_with(SIMULATOR) {
            return true;
        }
        // A name the system lists, or a path such as /dev/serial/by-id/… that it does not.
        let listed = litevna::discovery::list_ports().is_ok_and(|ports| ports.iter().any(|p| p.path == port));
        listed || Path::new(port).exists()
    }

    fn connect_failed(&mut self, msg: &str) {
        self.failures += 1;
        let pause = RETRY_FIRST
            .saturating_mul(1 << (self.failures - 1).min(4))
            .min(RETRY_MAX)
            .max(self.search_every);
        // Once per kind of failure; a port that stays busy says so once.
        if self.last_failure.as_deref() != Some(msg) {
            log::warn!("{msg}; trying again every so often (next in {} s)", pause.as_secs());
            self.last_failure = Some(msg.to_string());
        } else {
            log::debug!("{msg}; next try in {} s", pause.as_secs());
        }
        self.state = DeviceState::Searching;
        self.next_search = Instant::now() + pause;
    }

    /// Disconnect the device (dropping it hands a LiteVNA its screen back),
    /// tell the clients, and close their outboxes.
    fn shut_down(&mut self, rx: &Receiver<ToHub>) {
        log::info!("shutting down: letting go of the device");
        let _ = self.worker.send(Cmd::Disconnect);
        let deadline = Instant::now() + RELEASE_FOR;
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(ToHub::Event(Event::Disconnected)) => {
                    self.broadcast(Kind::Other, encode(Event::Disconnected));
                    break;
                }
                Ok(ToHub::Event(e)) => self.event(e),
                Ok(ToHub::Leave { ack: Some(ack), .. }) => {
                    let _ = ack.send(());
                }
                Ok(_) => {}
                Err(_) => {
                    log::warn!("the worker did not let go of the device within {} s", RELEASE_FOR.as_secs());
                    break;
                }
            }
        }
        for c in &self.clients {
            c.outbox.close();
        }
    }
}

fn encode(e: Event) -> Arc<str> {
    ServerMsg::Event(e).encode().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(o: &Outbox) -> Vec<String> {
        o.take().0.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn progress_and_display_sweeps_keep_only_the_newest_in_order() {
        let o = Outbox::default();
        for (k, t) in [
            (Kind::Progress, "p1"),
            (Kind::Other, "connected"),
            (Kind::Progress, "p2"),
            (Kind::Display, "d1"),
            (Kind::Progress, "p3"),
            (Kind::Other, "battery"),
            (Kind::Display, "d2"),
        ] {
            assert!(o.push(k, t.into()));
        }
        assert_eq!(texts(&o), ["connected", "p3", "battery", "d2"].map(String::from));
        assert!(texts(&o).is_empty());
    }

    #[test]
    fn a_client_that_does_not_read_is_dropped() {
        let o = Outbox::default();
        for _ in 0..MAX_QUEUED - 2 {
            assert!(o.push(Kind::Other, "x".into()));
        }
        // coalesced messages never fill it
        for _ in 0..10 * MAX_QUEUED {
            assert!(o.push(Kind::Progress, "p".into()));
            assert!(o.push(Kind::Display, "d".into()));
        }
        assert!(!o.push(Kind::Other, "one too many".into()));
        let (items, closed) = o.take();
        assert!(items.is_empty() && closed);
        // and by size
        let o = Outbox::default();
        let big: Arc<str> = "x".repeat(MAX_QUEUED_BYTES / 2 + 1).into();
        assert!(o.push(Kind::Other, big.clone()));
        assert!(!o.push(Kind::Other, big));
    }

    #[test]
    fn closing_delivers_what_is_queued() {
        let o = Outbox::default();
        o.push(Kind::Other, "bye".into());
        o.close();
        assert!(o.push(Kind::Other, "after".into()));
        let (items, closed) = o.take();
        assert_eq!((items.len(), closed), (1, true));
    }
}
