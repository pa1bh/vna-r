//! The GUI's handle (`vna_link::WorkerHandle`) against a real server with
//! the simulator, over loopback: what the GUI sees when its device is
//! behind a server. The client's pauses are made short; the one real wait
//! is the keep-alive test, which has to let time pass.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use litevna::SweepConfig;
use tungstenite::Message;
use vna_link::wire::{Hello, ServerMsg};
use vna_link::{spawn_with, Cmd, Event, Loss, Options, Purpose, RemoteTiming, Via, WorkerHandle};
use vna_server::{Config, DeviceChoice, Server};

const TIMEOUT: Duration = Duration::from_secs(10);

fn server_on(addr: SocketAddr, rate: f64) -> Server {
    let config = || {
        let mut c = Config::new(addr, DeviceChoice::Simulator(Some(rate)));
        c.name = "test bench".into();
        c.search_every = Duration::from_millis(20);
        c
    };
    // The port of a server that just stopped may take a moment to be free.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match Server::start(config()) {
            Ok(s) => return s,
            Err(e) if Instant::now() < deadline => {
                eprintln!("binding {addr}: {e}; again");
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("{e}"),
        }
    }
}

fn server(rate: f64) -> Server {
    server_on("127.0.0.1:0".parse().unwrap(), rate)
}

fn handle() -> WorkerHandle {
    spawn_with(
        Arc::new(|| {}),
        Options {
            remote: RemoteTiming {
                first_retry: Duration::from_millis(20),
                max_retry: Duration::from_millis(100),
                give_up_after: Duration::from_secs(5),
                greeting_within: Duration::from_secs(2),
            },
            ..Options::default()
        },
    )
}

fn next(h: &WorkerHandle) -> Event {
    h.recv_timeout(TIMEOUT).expect("an event within the timeout")
}

fn until(h: &WorkerHandle, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let e = next(h);
        let done = stop(&e);
        seen.push(e);
        if done {
            return seen;
        }
    }
}

fn url(addr: SocketAddr) -> String {
    format!("ws://{addr}/ws")
}

fn sweep(points: u16, continuous: bool) -> Cmd {
    Cmd::Sweep {
        cfgs: vec![SweepConfig::linear(1_000_000, 30_000_000, points).unwrap()],
        purpose: Purpose::Display,
        continuous,
        repeat: 1,
    }
}

fn is_done(e: &Event) -> bool {
    matches!(e, Event::SweepDone { .. })
}

/// Wait until `cond` holds, for what the server does on its own threads.
fn eventually(what: &str, cond: impl Fn() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !cond() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn connect_sweep_and_disconnect_through_the_handle() {
    let s = server(0.0);
    let h = handle();
    let port = url(s.local_addr());
    h.send(Cmd::Connect(port.clone()));
    match until(&h, |e| matches!(e, Event::Connected { .. })).pop() {
        Some(Event::Connected { port: p, info, via, .. }) => {
            assert_eq!(p, port);
            assert_eq!(info.model, "vna-r simulator");
            assert_eq!(
                via,
                Some(Via {
                    server: "test bench".into(),
                    version: vna_server::VERSION.into(),
                    device_port: "sim:0".into()
                })
            );
        }
        other => panic!("{other:?}"),
    }
    h.send(sweep(51, false));
    match until(&h, is_done).pop() {
        Some(Event::SweepDone { result, .. }) => assert_eq!(result.freqs_hz.len(), 51),
        other => panic!("{other:?}"),
    }
    assert_eq!(s.clients(), 1);
    h.send(Cmd::Disconnect);
    until(&h, |e| matches!(e, Event::Disconnected));
    eventually("the server sees the client leave", || s.clients() == 0);
    // and after that, as with a serial port that is gone
    h.send(sweep(11, false));
    assert!(matches!(next(&h), Event::Error { stops_sweep: true, .. }));
    s.shutdown();
}

#[test]
fn a_server_that_restarts_is_reconnected_and_sweeping_resumes() {
    let s = server(20_000.0);
    let addr = s.local_addr();
    let h = handle();
    h.send(Cmd::Connect(url(addr)));
    until(&h, |e| matches!(e, Event::Connected { .. }));
    h.send(sweep(101, true));
    until(&h, |e| matches!(e, Event::Continuous(true)));
    until(&h, is_done);

    s.shutdown();
    let lost = until(&h, |e| matches!(e, Event::LinkLost { what: Loss::Server, .. }));
    assert!(
        lost.iter().all(|e| !matches!(e, Event::Disconnected | Event::Error { .. })),
        "{lost:?}"
    );

    let s = server_on(addr, 20_000.0);
    match until(&h, |e| matches!(e, Event::Connected { .. })).pop() {
        Some(Event::Connected { port, .. }) => assert_eq!(port, url(addr)),
        other => panic!("{other:?}"),
    }
    // The new server does not sweep by itself; the session asks again.
    until(&h, |e| matches!(e, Event::Continuous(true)));
    until(&h, is_done);
    s.shutdown();
}

#[test]
fn a_server_on_another_protocol_version_is_refused() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let fake = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let mut hello = Hello::new("9.0.0", "future");
        hello.protocol += 1;
        ws.send(Message::Text(ServerMsg::Hello(hello).encode())).unwrap();
        // until the client closes
        while ws.read().is_ok() {}
    });
    let h = handle();
    h.send(Cmd::Connect(url(addr)));
    match next(&h) {
        Event::Error { msg, stops_sweep: true } => assert!(msg.contains("server is newer"), "{msg}"),
        other => panic!("{other:?}"),
    }
    fake.join().unwrap();
}

#[test]
fn a_client_that_says_nothing_is_kept_alive_by_the_pings() {
    let mut c = Config::new("127.0.0.1:0".parse().unwrap(), DeviceChoice::Simulator(Some(0.0)));
    c.ping_every = Duration::from_millis(50);
    c.silent_for = Duration::from_millis(300);
    let s = Server::start(c).unwrap();
    let h = handle();
    h.send(Cmd::Connect(url(s.local_addr())));
    until(&h, |e| matches!(e, Event::Connected { .. }));
    // For comparison a client that does not read, and so does not answer.
    let mute = TcpStream::connect(s.local_addr()).unwrap();
    let (_mute, _) = tungstenite::client(url(s.local_addr()), mute).unwrap();
    eventually("both are there", || s.clients() == 2);
    // Several times the limit: only time can tell.
    thread::sleep(Duration::from_millis(1200));
    assert_eq!(s.clients(), 1, "the mute client is let go, the handle is not");
    let mut seen = Vec::new();
    while let Some(e) = h.try_recv() {
        seen.push(e);
    }
    assert!(!seen.iter().any(|e| matches!(e, Event::LinkLost { .. })), "{seen:?}");
    h.send(Cmd::Battery);
    until(&h, |e| matches!(e, Event::Battery(_)));
    s.shutdown();
}

#[test]
fn what_cannot_be_reached_is_a_failed_connect() {
    let h = handle();
    // nothing listens on port 1
    h.send(Cmd::Connect("ws://127.0.0.1:1/ws".into()));
    match next(&h) {
        Event::Error { msg, stops_sweep: true } => assert!(msg.starts_with("connecting to 127.0.0.1:1:"), "{msg}"),
        other => panic!("{other:?}"),
    }
    h.send(Cmd::Connect("wss://vna.example.org/ws".into()));
    match next(&h) {
        Event::Error { msg, .. } => assert!(msg.contains("TLS"), "{msg}"),
        other => panic!("{other:?}"),
    }
    h.send(Cmd::Connect("ws://not an address".into()));
    assert!(matches!(next(&h), Event::Error { stops_sweep: true, .. }));
}

#[test]
fn one_device_at_a_time_across_the_two_backends() {
    let s = server(0.0);
    let h = handle();
    h.send(Cmd::Connect("sim:0".into()));
    match next(&h) {
        Event::Connected { port, .. } => assert_eq!(port, "sim:0"),
        other => panic!("{other:?}"),
    }
    // to the server: the local device is let go, and its Disconnected is not news
    h.send(Cmd::Connect(url(s.local_addr())));
    let seen = until(&h, |e| matches!(e, Event::Connected { .. }));
    assert!(!seen.iter().any(|e| matches!(e, Event::Disconnected)), "{seen:?}");
    eventually("the server has the client", || s.clients() == 1);
    // and back
    h.send(Cmd::Connect("sim:0".into()));
    match until(&h, |e| matches!(e, Event::Connected { .. })).pop() {
        Some(Event::Connected { port, .. }) => assert_eq!(port, "sim:0"),
        other => panic!("{other:?}"),
    }
    eventually("the server sees the client leave", || s.clients() == 0);
    h.send(sweep(11, false));
    until(&h, is_done);
    s.shutdown();
}
