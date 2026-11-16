//! A session with a vna-server: the [`Cmd`]s of the GUI go over a WebSocket
//! as [`ClientMsg`]s, and what the server says comes back as the [`Event`]s
//! a local worker would send, so that the GUI cannot tell the difference.
//!
//! The session is a state machine without threads or clocks of its own: it
//! is told the time, and whoever drives it calls [`Session::poll`] when a
//! socket has news or [`Session::deadline`] has come (a thread on native, the
//! GUI's frames and a browser timer in the browser; see `handle.rs`).
//!
//! What the GUI sees, against what happens:
//!
//! | Situation | Events for the GUI |
//! |---|---|
//! | socket open, hello, the server's analyser | `Connected` (port: the address the GUI gave; the server in `via`) |
//! | the server speaks another protocol | `Error` (stops the sweep), `Disconnected` |
//! | the first socket fails, or no hello comes | `Error` "connecting to …", as a failed serial port |
//! | the server has no analyser (yet) | `LinkLost` (`NoAnalyser`, or `AnalyserGone` live), then `Connected` when it has one |
//! | the server's analyser drops off USB | `LinkLost` (`AnalyserLost`), then `Connected` |
//! | the socket drops | `LinkLost` (`Server`), reconnect with a growing pause; `Disconnected` and `Error` after [`RemoteTiming::give_up_after`] |
//! | `Cmd::Disconnect`, also while waiting | `Disconnected` |
//!
//! While the GUI is told the link is lost, commands are kept rather than
//! sent, like the worker does: continuous sweeping is asked for again once
//! there is a link and an analyser (unless the server already sweeps), a
//! display sweep that was under way is asked once more, a calibration
//! standard never.

use std::collections::VecDeque;

use web_time::{Duration, Instant};

use crate::wire::{self, ClientMsg, Hello, ServerMsg};
use crate::{server_label, Cmd, Event, Loss, Purpose, Via};

/// How a session with a server waits. The defaults are for a Raspberry Pi
/// that restarts (about a minute); tests make them short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteTiming {
    /// The pause before the first try after the socket dropped; it doubles
    /// with every failed try, up to `max_retry`.
    pub first_retry: Duration,
    pub max_retry: Duration,
    /// How long a dropped server is waited for.
    pub give_up_after: Duration,
    /// How long an opening socket may take to bring the hello.
    pub greeting_within: Duration,
}

impl Default for RemoteTiming {
    fn default() -> Self {
        RemoteTiming {
            first_retry: Duration::from_secs(1),
            max_retry: Duration::from_secs(10),
            give_up_after: Duration::from_secs(120),
            greeting_within: Duration::from_secs(10),
        }
    }
}

/// One WebSocket, as the session needs it: `ewebsock` in the program, a
/// script in the tests. Dropping it closes it.
pub(crate) trait Socket {
    fn send(&mut self, text: String);
    fn try_recv(&mut self) -> Option<ewebsock::WsEvent>;
}

/// Opens a socket to a URL; an error when that cannot even be tried.
pub(crate) type Opener = Box<dyn FnMut(&str) -> Result<Box<dyn Socket>, String>>;

/// The answer of a worker without a device: what a session that has ended
/// (or never began) says to a command.
pub(crate) fn not_connected(cmd: &Cmd) -> Option<Event> {
    let what = match cmd {
        Cmd::Sweep { .. } => "sweep",
        Cmd::Screenshot => "screenshot",
        _ => return None,
    };
    Some(Event::Error {
        msg: format!("{what}: not connected"),
        stops_sweep: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Link {
    /// A socket is there (opening, or open).
    Open,
    /// The socket dropped; the next try is at this time.
    Down {
        retry_at: Instant,
    },
    Over,
}

/// What the server last said about its analyser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Analyser {
    Unknown,
    There,
    /// Lost on USB; the server's worker waits for it and resumes by itself.
    Lost,
    /// None at all.
    Absent,
}

pub(crate) struct Session {
    /// The port as the GUI knows it: what it put in `Cmd::Connect`.
    port: String,
    url: String,
    open: Opener,
    timing: RemoteTiming,
    /// Added to the error when the first socket fails (the https case in a browser).
    hint: Option<String>,
    socket: Option<Box<dyn Socket>>,
    link: Link,
    /// The hello arrived on the socket there is now.
    greeted: bool,
    /// A hello ever arrived: from then on a dropped socket is a lost link,
    /// before that it is a failed connect.
    ever_greeted: bool,
    opened_at: Instant,
    attempt: u32,
    /// When the socket dropped, for giving up.
    lost_since: Option<Instant>,
    hello: Option<Hello>,
    analyser: Analyser,
    /// The messages after a hello are the server's catch-up until its
    /// `Continuous`, which says whether it is sweeping.
    catching_up: bool,
    /// The server's notice that it has no analyser comes next in the catch-up.
    skip_notice: bool,
    /// Continuous sweeping the GUI asked for and did not stop.
    continuous: Option<ClientMsg>,
    /// A single display sweep that was sent and has not been answered.
    in_flight: Option<ClientMsg>,
    /// A single display sweep to ask for once the link is back.
    once: Option<ClientMsg>,
    out: VecDeque<Event>,
}

impl Session {
    /// Open the first socket.
    pub(crate) fn start(port: String, url: String, open: Opener, timing: RemoteTiming, hint: Option<String>, now: Instant) -> Session {
        let mut s = Session {
            port,
            url,
            open,
            timing,
            hint,
            socket: None,
            link: Link::Open,
            greeted: false,
            ever_greeted: false,
            opened_at: now,
            attempt: 0,
            lost_since: None,
            hello: None,
            analyser: Analyser::Unknown,
            catching_up: false,
            skip_notice: false,
            continuous: None,
            in_flight: None,
            once: None,
            out: VecDeque::new(),
        };
        s.open_socket(now);
        s
    }

    pub(crate) fn next_event(&mut self) -> Option<Event> {
        self.out.pop_front()
    }

    #[cfg(test)]
    fn is_over(&self) -> bool {
        self.link == Link::Over
    }

    /// When [`poll`](Self::poll) has something to do even if no socket has news.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        match self.link {
            Link::Open if !self.greeted => Some(self.opened_at + self.timing.greeting_within),
            Link::Down { retry_at } => Some(retry_at),
            _ => None,
        }
    }

    /// Read what the socket brought, and act on the clock.
    pub(crate) fn poll(&mut self, now: Instant) {
        while let Some(ev) = self.socket.as_mut().and_then(|s| s.try_recv()) {
            self.on_socket(ev, now);
        }
        match self.link {
            Link::Open if !self.greeted && now >= self.opened_at + self.timing.greeting_within => {
                let secs = self.timing.greeting_within.as_secs_f32();
                self.socket_down(&format!("no answer within {secs:.0} s; is it a vna-server?"), now);
            }
            Link::Down { retry_at } if now >= retry_at => self.open_socket(now),
            _ => {}
        }
    }

    pub(crate) fn command(&mut self, cmd: Cmd) {
        if let Cmd::Disconnect = cmd {
            self.socket = None;
            if self.link != Link::Over {
                self.link = Link::Over;
                self.out.push_back(Event::Disconnected);
            }
            return;
        }
        if self.link == Link::Over {
            self.out.extend(not_connected(&cmd));
            return;
        }
        let Ok(msg) = ClientMsg::try_from(cmd) else {
            // A Connect: the handle opens a new session for it.
            return;
        };
        let live = self.link == Link::Open && self.greeted && self.analyser != Analyser::Absent;
        if live {
            self.track(&msg);
            self.send(msg);
        } else {
            self.hold(msg);
        }
    }

    /// Remember what has to be picked up again after a lost link.
    fn track(&mut self, msg: &ClientMsg) {
        match msg {
            ClientMsg::Sweep { continuous: true, .. } => {
                self.continuous = Some(msg.clone());
                self.once = None;
            }
            ClientMsg::Sweep {
                continuous: false,
                purpose: Purpose::Display,
                ..
            } => self.in_flight = Some(msg.clone()),
            ClientMsg::Stop => {
                self.continuous = None;
                self.in_flight = None;
                self.once = None;
            }
            _ => {}
        }
    }

    /// A command while the GUI is told the link is lost: what the worker
    /// does with it while it waits for its port.
    fn hold(&mut self, msg: ClientMsg) {
        match msg {
            ClientMsg::Sweep { continuous: true, .. } => {
                self.continuous = Some(msg);
                self.once = None;
            }
            ClientMsg::Sweep {
                purpose: Purpose::Display, ..
            } => self.once = Some(msg),
            ClientMsg::Sweep { .. } => self.error("sweep: not connected: waiting for the device to come back", true),
            ClientMsg::Stop => {
                self.continuous = None;
                self.once = None;
            }
            ClientMsg::Screenshot => self.error("screenshot: not connected", true),
            ClientMsg::Battery | ClientMsg::SetAverage(_) => {}
        }
    }

    fn send(&mut self, msg: ClientMsg) {
        if let Some(s) = &mut self.socket {
            s.send(msg.encode());
        }
    }

    fn error(&mut self, msg: impl Into<String>, stops_sweep: bool) {
        self.out.push_back(Event::Error {
            msg: msg.into(),
            stops_sweep,
        });
    }

    fn open_socket(&mut self, now: Instant) {
        self.greeted = false;
        self.opened_at = now;
        self.link = Link::Open;
        match (self.open)(&self.url) {
            Ok(s) => self.socket = Some(s),
            Err(e) => self.socket_down(&e, now),
        }
    }

    fn on_socket(&mut self, ev: ewebsock::WsEvent, now: Instant) {
        use ewebsock::{WsEvent, WsMessage};
        match ev {
            WsEvent::Opened => {}
            WsEvent::Message(WsMessage::Text(t)) => self.on_text(&t),
            WsEvent::Message(WsMessage::Binary(_) | WsMessage::Unknown(_)) => {
                self.on_bad(&wire::Error::Malformed("a binary message".into()))
            }
            WsEvent::Message(WsMessage::Ping(_) | WsMessage::Pong(_)) => {}
            WsEvent::Error(e) => self.socket_down(&e, now),
            WsEvent::Closed => self.socket_down("closed by the server", now),
        }
    }

    fn on_text(&mut self, text: &str) {
        match ServerMsg::decode(text) {
            Ok(ServerMsg::Hello(h)) if !self.greeted => {
                self.greeted = true;
                self.ever_greeted = true;
                self.attempt = 0;
                self.catching_up = true;
                self.hello = Some(h);
            }
            Ok(ServerMsg::Hello(_)) => {}
            Ok(ServerMsg::Event(e)) if self.greeted => self.on_event(e),
            Ok(ServerMsg::Event(_)) => self.not_a_server("it spoke before it said hello"),
            Err(e) => self.on_bad(&e),
        }
    }

    fn on_bad(&mut self, e: &wire::Error) {
        match e {
            wire::Error::Version { .. } => {
                self.error(e.to_string(), true);
                self.end();
            }
            _ if !self.greeted => self.not_a_server(&e.to_string()),
            _ => self.error(format!("a message from the server could not be read: {e}"), false),
        }
    }

    fn not_a_server(&mut self, why: &str) {
        self.error(
            format!(
                "connecting to {}: the answer is not that of a vna-server ({why})",
                server_label(&self.port)
            ),
            true,
        );
        self.end();
    }

    /// The session is over for good: close, and let a GUI that thought it
    /// connected know.
    fn end(&mut self) {
        self.socket = None;
        self.link = Link::Over;
        if self.ever_greeted {
            self.out.push_back(Event::Disconnected);
        }
    }

    fn on_event(&mut self, e: Event) {
        match e {
            Event::Connected {
                port, info, battery_mv, ..
            } => {
                let was = self.analyser;
                self.analyser = Analyser::There;
                self.lost_since = None;
                let via = self.hello.as_ref().map(|h| Via {
                    server: h.name.clone(),
                    version: h.server.clone(),
                    device_port: port,
                });
                self.out.push_back(Event::Connected {
                    port: self.port.clone(),
                    info,
                    battery_mv,
                    via,
                });
                // After the catch-up its `Continuous` decides; live, only an
                // analyser that was not there at all needs it (a server
                // whose analyser came back on USB resumes by itself).
                if !self.catching_up && was == Analyser::Absent {
                    self.resume();
                }
            }
            Event::LinkLost { port, msg, .. } => {
                self.analyser = Analyser::Lost;
                self.lost_since = None;
                self.out.push_back(Event::LinkLost {
                    port: self.port.clone(),
                    msg: format!("{port}: {msg}"),
                    what: Loss::AnalyserLost,
                });
            }
            Event::Disconnected => {
                self.analyser = Analyser::Absent;
                self.lost_since = None;
                if self.once.is_none() {
                    self.once = self.in_flight.take();
                }
                let (msg, what) = if self.catching_up {
                    // The server's own notice about it follows; the waiting
                    // state says it already, without the red of an error.
                    self.skip_notice = true;
                    ("the server has no analyser connected yet", Loss::NoAnalyser)
                } else {
                    ("the server lost its analyser", Loss::AnalyserGone)
                };
                self.out.push_back(Event::LinkLost {
                    port: self.port.clone(),
                    msg: msg.into(),
                    what,
                });
            }
            Event::Error { stops_sweep: false, .. } if self.catching_up && self.skip_notice => {
                self.skip_notice = false;
            }
            Event::Continuous(on) if self.catching_up => {
                self.catching_up = false;
                let has = matches!(self.analyser, Analyser::There | Analyser::Lost);
                if on {
                    // Sweeping already, for us or for another client.
                    self.once = None;
                    self.out.push_back(Event::Continuous(true));
                } else if self.continuous.is_some() {
                    // Asked for again (now, or once there is an analyser);
                    // the GUI keeps showing it as running meanwhile.
                    if has {
                        self.resume();
                    }
                } else {
                    if has {
                        self.resume();
                    }
                    self.out.push_back(Event::Continuous(false));
                }
            }
            Event::Continuous(on) => {
                if !on {
                    // Stopped, by us or by another client.
                    self.continuous = None;
                }
                self.out.push_back(Event::Continuous(on));
            }
            Event::Error { stops_sweep: true, .. } => {
                self.continuous = None;
                self.in_flight = None;
                self.out.push_back(e);
            }
            Event::SweepDone {
                purpose: Purpose::Display, ..
            }
            | Event::Cancelled { purpose: Purpose::Display } => {
                self.in_flight = None;
                self.out.push_back(e);
            }
            other => self.out.push_back(other),
        }
    }

    /// Ask again for what the GUI expects to go on.
    fn resume(&mut self) {
        if let Some(m) = self.continuous.clone() {
            self.once = None;
            self.send(m);
        } else if let Some(m) = self.once.take() {
            self.in_flight = Some(m.clone());
            self.send(m);
        }
    }

    fn socket_down(&mut self, reason: &str, now: Instant) {
        self.socket = None;
        self.greeted = false;
        if self.link == Link::Over {
            return;
        }
        if !self.ever_greeted {
            let hint = self.hint.clone().unwrap_or_default();
            self.error(format!("connecting to {}: {reason}{hint}", server_label(&self.port)), true);
            self.link = Link::Over;
            return;
        }
        let since = *self.lost_since.get_or_insert_with(|| {
            // First failure of this loss: tell the GUI.
            self.out.push_back(Event::LinkLost {
                port: self.port.clone(),
                msg: reason.to_string(),
                what: Loss::Server,
            });
            now
        });
        if self.once.is_none() {
            self.once = self.in_flight.take();
        }
        self.analyser = Analyser::Unknown;
        if now.duration_since(since) >= self.timing.give_up_after {
            self.out.push_back(Event::Disconnected);
            self.error(
                format!(
                    "the server {} did not come back within {} s",
                    server_label(&self.port),
                    self.timing.give_up_after.as_secs()
                ),
                true,
            );
            self.link = Link::Over;
            return;
        }
        self.attempt += 1;
        let pause = self
            .timing
            .first_retry
            .saturating_mul(1 << (self.attempt - 1).min(16))
            .min(self.timing.max_retry);
        log::debug!("{}: {reason}; trying again in {:.1} s", self.port, pause.as_secs_f32());
        self.link = Link::Down { retry_at: now + pause };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use ewebsock::{WsEvent, WsMessage};
    use litevna::discovery::Kind;
    use litevna::{Limits, SweepConfig, VnaInfo};

    /// Both ends of a scripted socket: what the "server" sends, what the
    /// session sent, and whether it was closed.
    #[derive(Default)]
    struct Wire {
        incoming: VecDeque<WsEvent>,
        sent: Vec<String>,
        closed: bool,
    }

    struct Fake(Rc<RefCell<Wire>>);

    impl Socket for Fake {
        fn send(&mut self, text: String) {
            self.0.borrow_mut().sent.push(text);
        }
        fn try_recv(&mut self) -> Option<WsEvent> {
            self.0.borrow_mut().incoming.pop_front()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.0.borrow_mut().closed = true;
        }
    }

    /// A test bench: the session, the sockets it opened, and a clock.
    struct Bench {
        s: Session,
        sockets: Rc<RefCell<Vec<Rc<RefCell<Wire>>>>>,
        /// Whether the next open succeeds.
        reachable: Rc<RefCell<bool>>,
        now: Instant,
    }

    const T: RemoteTiming = RemoteTiming {
        first_retry: Duration::from_secs(1),
        max_retry: Duration::from_secs(10),
        give_up_after: Duration::from_secs(120),
        greeting_within: Duration::from_secs(10),
    };

    impl Bench {
        fn new() -> Bench {
            let sockets: Rc<RefCell<Vec<Rc<RefCell<Wire>>>>> = Rc::default();
            let reachable = Rc::new(RefCell::new(true));
            let (s2, r2) = (sockets.clone(), reachable.clone());
            let open: Opener = Box::new(move |_url| {
                if !*r2.borrow() {
                    return Err("connection refused".into());
                }
                let w = Rc::new(RefCell::new(Wire::default()));
                s2.borrow_mut().push(w.clone());
                Ok(Box::new(Fake(w)))
            });
            let now = Instant::now();
            let s = Session::start("vna.local:8080".into(), "ws://vna.local:8080/ws".into(), open, T, None, now);
            Bench {
                s,
                sockets,
                reachable,
                now,
            }
        }

        fn wire(&self) -> Rc<RefCell<Wire>> {
            self.sockets.borrow().last().expect("a socket").clone()
        }

        fn server(&mut self, m: ServerMsg) {
            self.wire()
                .borrow_mut()
                .incoming
                .push_back(WsEvent::Message(WsMessage::Text(m.encode())));
        }

        fn event(&mut self, e: Event) {
            self.server(ServerMsg::Event(e));
        }

        fn hello(&mut self, protocol: u32) {
            let mut h = Hello::new("0.4.0", "shack");
            h.protocol = protocol;
            self.server(ServerMsg::Hello(h));
        }

        fn drop_socket(&mut self) {
            self.wire()
                .borrow_mut()
                .incoming
                .push_back(WsEvent::Error("read: Connection reset by peer".into()));
        }

        fn after(&mut self, d: Duration) {
            self.now += d;
        }

        /// Poll and return what the GUI gets.
        fn run(&mut self) -> Vec<Event> {
            self.s.poll(self.now);
            std::iter::from_fn(|| self.s.next_event()).collect()
        }

        fn cmd(&mut self, c: Cmd) -> Vec<Event> {
            self.s.command(c);
            self.run()
        }

        fn sent(&self) -> Vec<ClientMsg> {
            self.wire().borrow().sent.iter().map(|t| ClientMsg::decode(t).unwrap()).collect()
        }

        /// Hello, an analyser, not sweeping: what a server with a device says.
        fn greet(&mut self) -> Vec<Event> {
            self.hello(wire::PROTOCOL_VERSION);
            self.event(connected());
            self.event(Event::Continuous(false));
            self.run()
        }
    }

    fn connected() -> Event {
        Event::Connected {
            port: "/dev/ttyACM0".into(),
            info: VnaInfo {
                kind: Kind::LiteVna,
                model: "LiteVNA".into(),
                firmware: "2.2".into(),
                detail: "hw 2".into(),
                limits: Limits {
                    min_hz: 50_000,
                    max_hz: 6_300_000_000,
                    max_points_per_scan: None,
                    has_device_average: true,
                },
                lite: None,
                shell: None,
            },
            battery_mv: Some(4000),
            via: None,
        }
    }

    fn sweep(continuous: bool, purpose: Purpose) -> Cmd {
        Cmd::Sweep {
            cfgs: vec![SweepConfig::linear(1_000_000, 30_000_000, 11).unwrap()],
            purpose,
            continuous,
            repeat: 1,
        }
    }

    fn msg(c: Cmd) -> ClientMsg {
        ClientMsg::try_from(c).unwrap()
    }

    #[test]
    fn connecting_gives_connected_with_the_address_and_the_server_in_the_detail() {
        let mut b = Bench::new();
        let got = b.greet();
        assert_eq!(got.len(), 2, "{got:?}");
        let Event::Connected {
            port,
            info,
            battery_mv,
            via,
        } = &got[0]
        else {
            panic!("{got:?}")
        };
        assert_eq!(port, "vna.local:8080");
        assert_eq!(info.detail, "hw 2");
        assert_eq!(
            via.as_ref(),
            Some(&Via {
                server: "shack".into(),
                version: "0.4.0".into(),
                device_port: "/dev/ttyACM0".into()
            })
        );
        assert_eq!(*battery_mv, Some(4000));
        assert!(matches!(got[1], Event::Continuous(false)));
        b.cmd(sweep(false, Purpose::Display));
        assert_eq!(b.sent(), [msg(sweep(false, Purpose::Display))]);
    }

    #[test]
    fn another_protocol_version_ends_the_session() {
        let mut b = Bench::new();
        b.hello(wire::PROTOCOL_VERSION + 1);
        let got = b.run();
        assert!(
            matches!(&got[0], Event::Error { msg, stops_sweep: true } if msg.contains("server is newer")),
            "{got:?}"
        );
        assert!(b.s.is_over() && b.wire().borrow().closed);
        // the GUI was still connecting: no Disconnected needed, nothing more comes
        assert_eq!(got.len(), 1);
        assert!(matches!(&b.cmd(sweep(false, Purpose::Display))[..], [Event::Error { .. }]));
    }

    #[test]
    fn a_first_socket_that_fails_is_a_failed_connect() {
        let mut b = Bench::new();
        b.drop_socket();
        let got = b.run();
        assert!(
            matches!(&got[..], [Event::Error { msg, stops_sweep: true }] if msg.starts_with("connecting to vna.local:8080: read")),
            "{got:?}"
        );
        assert!(b.s.is_over());
        // and so is silence
        let mut b = Bench::new();
        b.after(T.greeting_within);
        let got = b.run();
        assert!(
            matches!(&got[..], [Event::Error { msg, .. }] if msg.contains("is it a vna-server")),
            "{got:?}"
        );
        // and something that is not a vna-server
        let mut b = Bench::new();
        b.wire()
            .borrow_mut()
            .incoming
            .push_back(WsEvent::Message(WsMessage::Text("{\"x\":1}".into())));
        assert!(matches!(&b.run()[..], [Event::Error { msg, .. }] if msg.contains("not that of a vna-server")));
    }

    #[test]
    fn a_server_without_an_analyser_is_a_link_that_is_waited_for() {
        let mut b = Bench::new();
        b.hello(wire::PROTOCOL_VERSION);
        b.event(Event::Disconnected);
        b.event(Event::Error {
            msg: "the server has no analyser connected".into(),
            stops_sweep: false,
        });
        b.event(Event::Continuous(false));
        let got = b.run();
        assert!(
            matches!(&got[..], [Event::LinkLost { port, what: Loss::NoAnalyser, .. }, Event::Continuous(false)] if port == "vna.local:8080"),
            "{got:?}"
        );
        // Run is kept, not sent: there is nothing to sweep with
        assert!(b.cmd(sweep(true, Purpose::Display)).is_empty());
        assert!(b.sent().is_empty());
        // the analyser appears: Connected, and the sweeping is asked for
        b.event(connected());
        let got = b.run();
        assert!(matches!(&got[..], [Event::Connected { .. }]), "{got:?}");
        assert_eq!(b.sent(), [msg(sweep(true, Purpose::Display))]);
        // Cancel while waiting ends it at once
        // live, it is the analyser that went
        b.event(Event::Disconnected);
        assert!(matches!(
            &b.run()[..],
            [Event::LinkLost {
                what: Loss::AnalyserGone,
                ..
            }]
        ));
        assert!(matches!(&b.cmd(Cmd::Disconnect)[..], [Event::Disconnected]));
        assert!(b.wire().borrow().closed);
    }

    #[test]
    fn the_analyser_of_the_server_dropping_off_usb_is_a_lost_link() {
        let mut b = Bench::new();
        b.greet();
        b.cmd(sweep(true, Purpose::Display));
        b.event(Event::LinkLost {
            port: "/dev/ttyACM0".into(),
            msg: "sweep: I/O error: Broken pipe".into(),
            what: Loss::Device,
        });
        let got = b.run();
        assert!(
            matches!(&got[..], [Event::LinkLost { port, msg, what: Loss::AnalyserLost }] if port == "vna.local:8080" && msg.contains("/dev/ttyACM0")),
            "{got:?}"
        );
        // the server's worker resumes by itself: nothing is sent again
        b.event(connected());
        assert!(matches!(&b.run()[..], [Event::Connected { .. }]));
        assert_eq!(b.sent().len(), 1);
    }

    #[test]
    fn a_dropped_socket_is_reconnected_and_the_sweeping_resumed() {
        let mut b = Bench::new();
        b.greet();
        b.cmd(sweep(true, Purpose::Display));
        b.event(Event::Continuous(true));
        b.run();
        b.drop_socket();
        let got = b.run();
        assert!(
            matches!(&got[..], [Event::LinkLost { msg, what: Loss::Server, .. }] if msg.contains("reset by peer")),
            "{got:?}"
        );
        assert_eq!(b.sockets.borrow().len(), 1);
        // the server is down for a while: tries at 1, 2, 4 s ... say nothing new
        *b.reachable.borrow_mut() = false;
        for wait in [1, 2, 4] {
            b.after(Duration::from_secs(wait));
            assert!(b.run().is_empty());
        }
        // it is back; a new socket, and the server stopped sweeping when we left
        *b.reachable.borrow_mut() = true;
        b.after(Duration::from_secs(8));
        assert!(b.run().is_empty());
        assert_eq!(b.sockets.borrow().len(), 2);
        let got = b.greet();
        // Continuous(false) of the catch-up is not passed on: it is asked for again
        assert!(matches!(&got[..], [Event::Connected { .. }]), "{got:?}");
        assert_eq!(b.sent(), [msg(sweep(true, Purpose::Display))]);
    }

    #[test]
    fn after_a_reconnect_a_server_that_still_sweeps_is_left_alone() {
        let mut b = Bench::new();
        b.greet();
        b.cmd(sweep(true, Purpose::Display));
        b.drop_socket();
        b.run();
        b.after(Duration::from_secs(1));
        b.run();
        b.hello(wire::PROTOCOL_VERSION);
        b.event(connected());
        b.event(Event::Continuous(true));
        let got = b.run();
        assert!(matches!(&got[..], [Event::Connected { .. }, Event::Continuous(true)]), "{got:?}");
        assert!(b.sent().is_empty());
    }

    #[test]
    fn a_single_sweep_is_asked_again_once_and_a_standard_never() {
        let mut b = Bench::new();
        b.greet();
        b.cmd(sweep(false, Purpose::Display));
        b.drop_socket();
        b.run();
        // a standard while waiting is refused, as by the worker
        let got = b.cmd(sweep(false, Purpose::Standard(litevna::calibration::Standard::Open)));
        assert!(
            matches!(&got[..], [Event::Error { msg, stops_sweep: true }] if msg.contains("waiting")),
            "{got:?}"
        );
        b.after(Duration::from_secs(1));
        b.run();
        b.greet();
        assert_eq!(b.sent(), [msg(sweep(false, Purpose::Display))]);
        // answered: not again after the next drop
        b.event(Event::SweepDone {
            cfgs: vec![SweepConfig::linear(1_000_000, 30_000_000, 11).unwrap()],
            result: litevna::SweepResult {
                freqs_hz: (0..11).map(|i| 1_000_000 + i).collect(),
                s11: vec![Default::default(); 11],
                s21: vec![Default::default(); 11],
                points: vec![],
                checksum_ok: 0,
                checksum_bad: 0,
                duplicates: 0,
                elapsed: Duration::ZERO,
            },
            purpose: Purpose::Display,
            repeat: 1,
        });
        b.run();
        b.drop_socket();
        b.run();
        b.after(Duration::from_secs(1));
        b.run();
        b.greet();
        assert!(b.sent().is_empty());
    }

    #[test]
    fn a_server_that_does_not_come_back_is_given_up() {
        let mut b = Bench::new();
        b.greet();
        b.drop_socket();
        b.run();
        *b.reachable.borrow_mut() = false;
        let mut waited = Duration::ZERO;
        let mut last = Vec::new();
        while !b.s.is_over() {
            let d = b.s.deadline().expect("a next try") - b.now;
            assert!(d <= T.max_retry);
            b.after(d);
            waited += d;
            last = b.run();
        }
        assert!(waited >= T.give_up_after && waited <= T.give_up_after + T.max_retry, "{waited:?}");
        assert!(
            matches!(&last[..], [Event::Disconnected, Event::Error { msg, stops_sweep: true }] if msg.contains("did not come back")),
            "{last:?}"
        );
        // Cancel while waiting: at once
        let mut b = Bench::new();
        b.greet();
        b.drop_socket();
        b.run();
        assert!(matches!(&b.cmd(Cmd::Disconnect)[..], [Event::Disconnected]));
        assert!(b.s.deadline().is_none());
    }

    #[test]
    fn stopped_elsewhere_means_not_resumed() {
        let mut b = Bench::new();
        b.greet();
        b.cmd(sweep(true, Purpose::Display));
        // another client pressed Stop
        b.event(Event::Continuous(false));
        assert!(matches!(&b.run()[..], [Event::Continuous(false)]));
        b.drop_socket();
        b.run();
        b.after(Duration::from_secs(1));
        b.run();
        let got = b.greet();
        assert!(matches!(&got[..], [Event::Connected { .. }, Event::Continuous(false)]), "{got:?}");
        assert!(b.sent().is_empty());
    }

    #[test]
    fn an_unreadable_message_is_an_error_not_the_end() {
        let mut b = Bench::new();
        b.greet();
        b.wire()
            .borrow_mut()
            .incoming
            .push_back(WsEvent::Message(WsMessage::Text("garbage".into())));
        let got = b.run();
        assert!(matches!(&got[..], [Event::Error { stops_sweep: false, .. }]), "{got:?}");
        assert!(!b.s.is_over());
    }
}
