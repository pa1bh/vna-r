//! A real server on a port the system picks, with the simulator, and real
//! WebSocket and HTTP clients over loopback. Every wait is on a message; the
//! read timeout of 10 s only ends a test that is broken.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use litevna::calibration::Standard;
use litevna::SweepConfig;
use tungstenite::{Message, WebSocket};
use vna_link::wire::{ClientMsg, Hello, ServerMsg, PROTOCOL_VERSION};
use vna_link::{Event, Purpose};
use vna_server::{Config, DeviceChoice, Server};

const TIMEOUT: Duration = Duration::from_secs(10);

fn config(device: DeviceChoice) -> Config {
    let mut c = Config::new("127.0.0.1:0".parse().unwrap(), device);
    c.name = "test bench".into();
    c.search_every = Duration::from_millis(20);
    c
}

/// A server with the simulator at `rate` points per second (0: unpaced).
fn server(rate: f64) -> Server {
    Server::start(config(DeviceChoice::Simulator(Some(rate)))).unwrap()
}

struct Client {
    ws: WebSocket<TcpStream>,
}

impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();
        let (ws, _) = tungstenite::client(format!("ws://{addr}/ws"), stream).unwrap();
        Client { ws }
    }

    /// Connected, past the hello and the catch-up, and the device is there.
    fn ready(addr: SocketAddr) -> Client {
        let mut c = Client::connect(addr);
        c.hello();
        let seen = c.until(|e| matches!(e, Event::Connected { .. }));
        // The catch-up ends with `Continuous`; unless the device only came
        // after it, it follows `Connected`.
        if !seen.iter().any(|e| matches!(e, Event::Continuous(_))) {
            assert!(matches!(c.event(), Event::Continuous(_)));
        }
        c
    }

    fn msg(&mut self) -> ServerMsg {
        loop {
            match self.ws.read().expect("a message within the timeout") {
                Message::Text(t) => return ServerMsg::decode(&t).unwrap(),
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("{other:?}"),
            }
        }
    }

    fn hello(&mut self) -> Hello {
        match self.msg() {
            ServerMsg::Hello(h) => h,
            other => panic!("expected the hello, got {other:?}"),
        }
    }

    fn event(&mut self) -> Event {
        match self.msg() {
            ServerMsg::Event(e) => e,
            other => panic!("{other:?}"),
        }
    }

    /// Events up to and including the first that `stop` accepts.
    fn until(&mut self, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut seen = Vec::new();
        loop {
            let e = self.event();
            let done = stop(&e);
            seen.push(e);
            if done {
                return seen;
            }
        }
    }

    fn send(&mut self, m: ClientMsg) {
        self.ws.send(Message::Text(m.encode())).unwrap();
    }

    /// Ask for the battery and return everything up to its answer: the
    /// server handles messages in order, so whatever was going to arrive
    /// before that has arrived.
    fn sync(&mut self) -> Vec<Event> {
        self.send(ClientMsg::Battery);
        self.until(|e| matches!(e, Event::Battery(_)))
    }

    /// Close and wait for the server's reply: by then the server has let go of this client.
    fn close(mut self) {
        self.ws.close(None).unwrap();
        loop {
            match self.ws.read() {
                Ok(_) => {}
                Err(tungstenite::Error::ConnectionClosed) => return,
                Err(e) => panic!("{e}"),
            }
        }
    }
}

fn sweep(points: u16, purpose: Purpose, continuous: bool) -> ClientMsg {
    ClientMsg::Sweep {
        cfgs: vec![SweepConfig::linear(1_000_000, 30_000_000, points).unwrap()],
        purpose,
        continuous,
        repeat: 1,
    }
}

fn is_done(e: &Event) -> bool {
    matches!(e, Event::SweepDone { .. })
}

#[test]
fn the_hello_comes_first_and_shutdown_says_goodbye() {
    let s = server(0.0);
    let mut c = Client::connect(s.local_addr());
    let h = c.hello();
    assert_eq!(h.protocol, PROTOCOL_VERSION);
    assert_eq!((h.server.as_str(), h.name.as_str()), (vna_server::VERSION, "test bench"));
    c.until(|e| matches!(e, Event::Connected { .. }));
    s.shutdown();
    c.until(|e| matches!(e, Event::Disconnected));
}

#[test]
fn a_late_joiner_is_brought_up_to_date() {
    let s = server(20_000.0);
    let addr = s.local_addr();
    let mut a = Client::ready(addr);
    a.send(sweep(51, Purpose::Display, false));
    a.until(is_done);

    let mut b = Client::connect(addr);
    b.hello();
    assert!(matches!(b.event(), Event::Connected { .. }));
    assert!(matches!(b.event(), Event::Continuous(false)));
    match b.event() {
        Event::SweepDone { result, purpose, .. } => assert_eq!((result.freqs_hz.len(), purpose), (51, Purpose::Display)),
        other => panic!("{other:?}"),
    }

    a.send(sweep(101, Purpose::Display, true));
    a.until(|e| matches!(e, Event::Continuous(true)));
    let mut c = Client::connect(addr);
    c.hello();
    assert!(matches!(c.event(), Event::Connected { .. }));
    assert!(matches!(c.event(), Event::Continuous(true)));
    // The last sweep, or a newer one that took its place in the queue.
    let rest = c.until(is_done);
    assert!(
        rest.iter().all(|e| matches!(e, Event::Progress { .. } | Event::SweepDone { .. })),
        "{rest:?}"
    );
    s.shutdown();
}

#[test]
fn standards_and_screenshots_go_only_to_who_asked() {
    let s = server(0.0);
    let addr = s.local_addr();
    let (mut a, mut b) = (Client::ready(addr), Client::ready(addr));

    a.send(sweep(31, Purpose::Display, false));
    a.until(is_done);
    b.until(is_done);

    a.send(sweep(31, Purpose::Standard(Standard::Open), false));
    let got = a.until(is_done);
    assert!(matches!(
        got.last(),
        Some(Event::SweepDone {
            purpose: Purpose::Standard(Standard::Open),
            ..
        })
    ));
    a.send(ClientMsg::Screenshot);
    a.until(|e| matches!(e, Event::Screenshot(_)));

    let seen = b.sync();
    assert!(
        !seen.iter().any(|e| matches!(
            e,
            Event::SweepDone {
                purpose: Purpose::Standard(_),
                ..
            }
        )),
        "{seen:?}"
    );
    assert!(!seen.iter().any(|e| matches!(e, Event::Screenshot(_))));
    // nor its progress, whose end b would never hear of
    assert!(!seen.iter().any(|e| matches!(
        e,
        Event::Progress {
            purpose: Purpose::Standard(_),
            ..
        }
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        Event::Progress {
            purpose: Purpose::Standard(_),
            ..
        }
    )));
    s.shutdown();
}

#[test]
fn one_starts_continuous_sweeping_the_other_stops_it() {
    let s = server(20_000.0);
    let addr = s.local_addr();
    let (mut a, mut b) = (Client::ready(addr), Client::ready(addr));
    a.send(sweep(101, Purpose::Display, true));
    a.until(|e| matches!(e, Event::Continuous(true)));
    b.until(|e| matches!(e, Event::Continuous(true)));
    b.until(is_done);
    b.send(ClientMsg::Stop);
    a.until(|e| matches!(e, Event::Continuous(false)));
    b.until(|e| matches!(e, Event::Continuous(false)));
    // Stop cut the running sweep short: nothing sweeps after it.
    for c in [&mut a, &mut b] {
        let rest = c.sync();
        assert!(!rest.iter().any(|e| is_done(e) || matches!(e, Event::Progress { .. })), "{rest:?}");
    }
    s.shutdown();
}

#[test]
fn the_last_client_leaving_stops_the_device() {
    let s = server(20_000.0);
    let addr = s.local_addr();
    let mut a = Client::ready(addr);
    a.send(sweep(101, Purpose::Display, true));
    a.until(|e| matches!(e, Event::Continuous(true)));
    a.close();

    let mut b = Client::connect(addr);
    b.hello();
    let seen = b.sync();
    assert!(!seen.iter().any(|e| matches!(e, Event::Continuous(true))), "{seen:?}");
    s.shutdown();
}

#[test]
fn a_message_that_does_not_read_gets_an_error_and_the_connection_lives_on() {
    let s = server(0.0);
    let mut a = Client::ready(s.local_addr());
    a.ws.send(Message::Text("not json".into())).unwrap();
    match a.event() {
        Event::Error { msg, stops_sweep } => {
            assert!(msg.contains("could not read"), "{msg}");
            assert!(!stops_sweep);
        }
        other => panic!("{other:?}"),
    }
    a.ws.send(Message::Binary(vec![1, 2, 3])).unwrap();
    assert!(matches!(a.event(), Event::Error { stops_sweep: false, .. }));
    // `connect` is not something a client can send
    a.ws.send(Message::Text(r#"{"connect":"/dev/ttyACM0"}"#.into())).unwrap();
    assert!(matches!(a.event(), Event::Error { stops_sweep: false, .. }));
    a.sync();
    s.shutdown();
}

#[test]
fn without_a_device_a_client_is_told_and_hears_when_it_comes() {
    let plugged = Arc::new(AtomicBool::new(false));
    let looked = Arc::new(AtomicUsize::new(0));
    let mut cfg = config(DeviceChoice::Port("sim:0".into()));
    let (p, l) = (plugged.clone(), looked.clone());
    cfg.port_present = Some(Arc::new(move |_| {
        l.fetch_add(1, Ordering::SeqCst);
        p.load(Ordering::SeqCst)
    }));
    let s = Server::start(cfg).unwrap();
    let mut c = Client::connect(s.local_addr());
    c.hello();
    assert!(matches!(c.event(), Event::Disconnected));
    match c.event() {
        Event::Error { msg, stops_sweep: false } => assert!(msg.contains("no analyser"), "{msg}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(c.event(), Event::Continuous(false)));
    c.send(sweep(11, Purpose::Display, false));
    assert!(matches!(c.event(), Event::Error { stops_sweep: true, .. }));
    // the device is plugged in
    plugged.store(true, Ordering::SeqCst);
    match c.event() {
        Event::Connected { port, .. } => assert_eq!(port, "sim:0"),
        other => panic!("{other:?}"),
    }
    assert!(looked.load(Ordering::SeqCst) >= 1);
    c.send(sweep(11, Purpose::Display, false));
    c.until(is_done);
    s.shutdown();
}

// HTTP

/// The status line, the headers and the body of one request.
fn get(addr: SocketAddr, request: &[u8]) -> (String, String, Vec<u8>) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(TIMEOUT)).unwrap();
    s.write_all(request).unwrap();
    let mut all = Vec::new();
    s.read_to_end(&mut all).unwrap();
    let split = all.windows(4).position(|w| w == b"\r\n\r\n").expect("a response head");
    let head = String::from_utf8(all[..split].to_vec()).unwrap();
    let (status, headers) = head.split_once("\r\n").unwrap_or((&head, ""));
    (status.to_string(), headers.to_string(), all[split + 4..].to_vec())
}

fn path(addr: SocketAddr, p: &str) -> (String, String, Vec<u8>) {
    get(addr, format!("GET {p} HTTP/1.1\r\nHost: test\r\n\r\n").as_bytes())
}

#[test]
fn the_page_says_it_comes_from_a_server() {
    let s = server(0.0);
    let addr = s.local_addr();
    let (status, headers, body) = path(addr, "/");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert!(headers.contains("Cache-Control: no-cache"), "{headers}");
    assert!(headers.contains("Content-Type: text/html"), "{headers}");
    let page = String::from_utf8(body).unwrap();
    assert!(page.contains(r#"<meta name="vna-r-server" content="/ws">"#), "{page}");
    assert_eq!(path(addr, "/index.html").2, page.as_bytes());
    assert!(path(addr, "/nothing.html").0.starts_with("HTTP/1.1 404"));
    assert!(path(addr, "/ws").0.starts_with("HTTP/1.1 400"));
    let (status, _, body) = get(addr, b"HEAD / HTTP/1.1\r\nHost: test\r\n\r\n");
    assert!(status.ends_with("200 OK") && body.is_empty());
    assert!(get(addr, b"POST / HTTP/1.1\r\nHost: test\r\n\r\n").0.starts_with("HTTP/1.1 405"));
    if vna_server::BUILT_IN_WEB {
        let (status, headers, gz) = get(
            addr,
            b"GET /vna-gui_bg.wasm?v=42 HTTP/1.1\r\nHost: test\r\nAccept-Encoding: gzip, br\r\n\r\n",
        );
        assert!(status.ends_with("200 OK"));
        assert!(headers.contains("Content-Type: application/wasm"), "{headers}");
        assert!(
            headers.contains("Content-Encoding: gzip") && headers.contains("immutable"),
            "{headers}"
        );
        let (_, headers, raw) = path(addr, "/vna-gui_bg.wasm");
        assert!(!headers.contains("Content-Encoding") && headers.contains("no-cache"), "{headers}");
        assert_eq!(&raw[..4], b"\0asm");
        assert!(gz.len() < raw.len());
    } else {
        assert!(path(addr, "/vna-gui_bg.wasm").0.starts_with("HTTP/1.1 404"));
    }
    s.shutdown();
}

/// A folder of its own under the system's temporary folder.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vna-server-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("web")).unwrap();
    d
}

#[test]
fn a_web_folder_serves_plain_names_only() {
    let d = scratch("web");
    std::fs::write(d.join("web/index.html"), "<html><head><title>t</title></head></html>").unwrap();
    std::fs::write(d.join("web/vna-gui_bg.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(d.join("secret.txt"), "the secret").unwrap();
    let mut cfg = config(DeviceChoice::Simulator(Some(0.0)));
    cfg.web_dir = Some(d.join("web"));
    let s = Server::start(cfg).unwrap();
    let addr = s.local_addr();
    let (_, _, page) = path(addr, "/");
    assert!(String::from_utf8(page).unwrap().contains("vna-r-server"));
    let (status, headers, body) = path(addr, "/vna-gui_bg.wasm?v=1");
    assert!(
        status.ends_with("200 OK") && headers.contains("Content-Type: application/wasm"),
        "{headers}"
    );
    assert_eq!(body, b"\0asm\x01\0\0\0");
    for bad in [
        "/../secret.txt",
        "/..%2fsecret.txt",
        "/%2e%2e/secret.txt",
        "/web/../../secret.txt",
        "/.hidden",
        "//etc/passwd",
    ] {
        let (status, _, body) = path(addr, bad);
        assert!(status.starts_with("HTTP/1.1 404"), "{bad}: {status}");
        assert!(!String::from_utf8_lossy(&body).contains("the secret"), "{bad}");
    }
    s.shutdown();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn garbage_on_the_socket_does_not_hurt_the_server() {
    let s = server(0.0);
    let addr = s.local_addr();
    // not HTTP
    assert!(get(addr, b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03\r\n\r\n")
        .0
        .starts_with("HTTP/1.1 400"));
    // a head that never ends
    let (status, _, _) = {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(TIMEOUT)).unwrap();
        let _ = s.write_all(&[b'a'; 20 << 10]);
        let mut all = Vec::new();
        let _ = s.read_to_end(&mut all);
        let head = String::from_utf8_lossy(&all).to_string();
        (head.lines().next().unwrap_or("").to_string(), (), ())
    };
    assert!(status.starts_with("HTTP/1.1 431"), "{status}");
    // connect and say nothing, connect and leave half a request
    drop(TcpStream::connect(addr).unwrap());
    let mut half = TcpStream::connect(addr).unwrap();
    half.write_all(b"GET / HT").unwrap();
    drop(half);
    // a WebSocket handshake that is not one
    assert!(get(
        addr,
        b"GET /ws HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n"
    )
    .0
    .starts_with("HTTP/1.1 400"));
    // and it still serves
    assert!(path(addr, "/").0.ends_with("200 OK"));
    let mut c = Client::ready(addr);
    c.sync();
    s.shutdown();
}
