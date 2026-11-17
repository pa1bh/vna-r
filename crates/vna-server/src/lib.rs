//! `vna-server` — an analyser on USB, shared over the network: the vna-r GUI
//! (in a browser, or the program on a computer) connects to it over a
//! WebSocket. Design: docs/10-client-server.md.
//!
//! Threads and channels, no async runtime:
//!
//! * the **worker** of `vna-link`, the same code that drives the device for
//!   the GUI, owns the device;
//! * the **hub** (`hub`) owns the worker's command channel and the list of
//!   clients. It finds and connects the device, routes the worker's events
//!   to the clients and the clients' commands to the worker, and never waits
//!   for a client;
//! * a **forwarder** puts the worker's events on the hub's channel, so the
//!   hub has one queue to wait on;
//! * the **listener** accepts connections, one thread each (`conn`): a
//!   plain HTTP request for the web client is answered and closed, a
//!   WebSocket stays and becomes a client of the hub.

mod conn;
mod hub;
mod web;

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hub::ToHub;

/// The program version, as the hello reports it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the WebSocket. The page the server serves names it in
/// `<meta name="vna-r-server" content="/ws">`.
pub(crate) const WS_PATH: &str = "/ws";

/// Whether the web client was built in (else the placeholder page is served).
pub const BUILT_IN_WEB: bool = web::BUILT_IN;

/// Connections served at the same time (page requests and clients); more are closed at once.
const MAX_CONNECTIONS: usize = 64;
/// How long a shutdown waits for the connections to say goodbye and close.
const CLOSE_WITHIN: Duration = Duration::from_secs(2);

/// Which analyser the server drives.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceChoice {
    /// The first serial port whose USB ids are those of an analyser.
    Auto,
    /// This serial port, whenever it is there.
    Port(String),
    /// The simulator of `vna-link`, at this many points per second
    /// (`None`: its default; 0: as fast as it goes).
    Simulator(Option<f64>),
}

/// Whether a serial port is there now; see [`Config::port_present`].
pub type PortCheck = Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub struct Config {
    pub listen: SocketAddr,
    pub device: DeviceChoice,
    /// What the hello calls this server.
    pub name: String,
    /// Serve the web client from this folder instead of the built-in copy.
    pub web_dir: Option<PathBuf>,
    /// How often to look for the device while there is none.
    pub search_every: Duration,
    /// Replaces the look at the system's serial ports for
    /// [`DeviceChoice::Port`]: lets a test decide when "the device is
    /// plugged in". Not something to set otherwise.
    #[doc(hidden)]
    pub port_present: Option<PortCheck>,
    /// How often a client is pinged, and how long it may be silent before
    /// it is let go (20 s and 60 s). Settable for tests.
    #[doc(hidden)]
    pub ping_every: Duration,
    #[doc(hidden)]
    pub silent_for: Duration,
}

impl Config {
    pub fn new(listen: SocketAddr, device: DeviceChoice) -> Self {
        Config {
            listen,
            device,
            name: host_name(),
            web_dir: None,
            search_every: Duration::from_secs(2),
            port_present: None,
            ping_every: Duration::from_secs(20),
            silent_for: Duration::from_secs(60),
        }
    }
}

/// The host name, the default name of a server.
fn host_name() -> String {
    let name = gethostname::gethostname().to_string_lossy().into_owned();
    if name.is_empty() {
        "vna-server".into()
    } else {
        name
    }
}

/// Shared by the connection threads.
pub(crate) struct Shared {
    pub(crate) hub: Sender<ToHub>,
    pub(crate) web: web::Web,
    pub(crate) next_id: AtomicU64,
    pub(crate) active: AtomicUsize,
    pub(crate) ping_every: Duration,
    pub(crate) silent_for: Duration,
}

/// A running server.
pub struct Server {
    addr: SocketAddr,
    hub: Sender<ToHub>,
    clients: Arc<AtomicUsize>,
    stopping: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    fatal: Option<Receiver<String>>,
    shared: Arc<Shared>,
}

impl Server {
    /// Listen, and start looking for the device.
    pub fn start(config: Config) -> io::Result<Server> {
        let web = web::Web::new(config.web_dir.clone())?;
        let listener = TcpListener::bind(config.listen)?;
        let addr = listener.local_addr()?;
        log::info!("listening on http://{addr}; web client {}", web.describe());
        let (fatal_tx, fatal) = channel();
        let (hub, clients) = hub::start(&config, fatal_tx);
        let shared = Arc::new(Shared {
            hub: hub.clone(),
            web,
            next_id: AtomicU64::new(1),
            active: AtomicUsize::new(0),
            ping_every: config.ping_every,
            silent_for: config.silent_for,
        });
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let for_listener = shared.clone();
        let listener = thread::Builder::new()
            .name("listener".into())
            .spawn(move || accept(listener, for_listener, stop))?;
        Ok(Server {
            addr,
            hub,
            clients,
            stopping,
            listener: Some(listener),
            fatal: Some(fatal),
            shared,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Hears it, once, when the server can no longer do its work (the thread
    /// that drives the analyser died): shut it down and exit with an error,
    /// so that whatever keeps the service running starts a new one. Can be
    /// taken once.
    pub fn fatal(&mut self) -> Option<Receiver<String>> {
        self.fatal.take()
    }

    /// WebSocket clients connected now.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }

    /// Stop listening, let go of the device, so that a LiteVNA leaves USB
    /// mode and gets its own screen back, and tell the clients. Returns when
    /// that is done, after a few seconds at most.
    pub fn shutdown(mut self) {
        // First, so that nobody new comes in while the hub says goodbye.
        self.stop_listening();
        let (done, wait) = channel();
        if self.hub.send(ToHub::Shutdown(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_secs(10));
        }
        // The hub has put the goodbye in every client's outbox; the threads
        // of the connections still have to send it and close. A program that
        // exits right after this call would cut them off before they do.
        let deadline = Instant::now() + CLOSE_WITHIN;
        while self.shared.active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn stop_listening(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        // `accept` blocks; a connection of our own wakes it.
        let ip = match self.addr.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        let _ = TcpStream::connect_timeout(&SocketAddr::new(ip, self.addr.port()), Duration::from_secs(1));
        if let Some(t) = self.listener.take() {
            let _ = t.join();
        }
    }
}

fn accept(listener: TcpListener, shared: Arc<Shared>, stopping: Arc<AtomicBool>) {
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            break;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                // Out of file descriptors, say: do not spin on it.
                log::warn!("accepting a connection: {e}");
                thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        if shared.active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            shared.active.fetch_sub(1, Ordering::SeqCst);
            log::warn!(
                "{MAX_CONNECTIONS} connections open; refusing one from {:?}",
                stream.peer_addr().ok()
            );
            continue;
        }
        let shared2 = shared.clone();
        let spawned = thread::Builder::new().name("connection".into()).spawn(move || {
            // Given back when the thread ends, also when it ends in a panic:
            // a slot lost each time would in the end refuse everybody.
            struct Slot(Arc<Shared>);
            impl Drop for Slot {
                fn drop(&mut self) {
                    self.0.active.fetch_sub(1, Ordering::SeqCst);
                }
            }
            let slot = Slot(shared2);
            conn::serve(stream, &slot.0);
        });
        if let Err(e) = spawned {
            shared.active.fetch_sub(1, Ordering::SeqCst);
            log::warn!("no thread for a connection: {e}");
        }
    }
}
