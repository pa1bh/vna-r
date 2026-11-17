//! `vna-server` — the analyser on USB, the vna-r GUI over the network.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;

use clap::Parser;
use vna_server::{Config, DeviceChoice, Server};

/// `vna-server 0.4.0 (abc1234)`: workspace version plus the git revision from build.rs.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("VNA_R_GIT_REV"), ")");

#[derive(Parser)]
#[command(
    name = "vna-server",
    version = VERSION,
    about = "Share a LiteVNA or NanoVNA-F on USB with the vna-r GUI over the network (PA1BH)"
)]
struct Cli {
    /// Address and TCP port for the web page and the WebSocket.
    #[arg(long, value_name = "ADDR", default_value = "0.0.0.0:8080")]
    listen: String,
    /// Serial port of the analyser, e.g. /dev/ttyACM0. Without it, the first
    /// port that looks like an analyser.
    #[arg(long, value_name = "SERIAL PORT", conflicts_with = "simulate")]
    port: Option<String>,
    /// Use a simulated analyser instead of a real one, optionally sweeping at
    /// this many points per second (0: as fast as it goes).
    #[arg(long, value_name = "POINTS PER SECOND", num_args = 0..=1, require_equals = true)]
    simulate: Option<Option<f64>>,
    /// What clients see as the name of this server. Default: the host name.
    #[arg(long)]
    name: Option<String>,
    /// Serve the web client from this folder (index.html, vna-gui.js,
    /// vna-gui_bg.wasm) instead of the copy built into the program.
    #[arg(long, value_name = "DIR")]
    web: Option<PathBuf>,
}

fn main() -> ExitCode {
    let mut log = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    // Under systemd the journal stamps every line itself.
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        log.format_timestamp(None);
    }
    log.init();
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let listen: SocketAddr = cli
        .listen
        .to_socket_addrs()
        .map_err(|e| format!("--listen {}: {e}", cli.listen))?
        .next()
        .ok_or_else(|| format!("--listen {}: no address", cli.listen))?;
    let device = match (cli.simulate, cli.port) {
        (Some(rate), _) => {
            if let Some(r) = rate {
                if !(r >= 0.0) || !r.is_finite() {
                    return Err(format!("--simulate={r}: a number of points per second, 0 or more"));
                }
            }
            DeviceChoice::Simulator(rate)
        }
        (None, Some(p)) => DeviceChoice::Port(p),
        (None, None) => DeviceChoice::Auto,
    };
    let mut config = Config::new(listen, device);
    if let Some(n) = cli.name {
        config.name = n;
    }
    config.web_dir = cli.web;
    log::info!("vna-server {VERSION}, \"{}\"", config.name);
    let mut server = Server::start(config).map_err(|e| format!("starting on {listen}: {e}"))?;

    // Ctrl-C, or SIGTERM from `systemctl stop`: let go of the device first,
    // so that a LiteVNA gets its screen back. A second one does not wait.
    let (tx, rx) = channel::<Option<String>>();
    let asked = AtomicBool::new(false);
    // A server that cannot measure any more stops too, with an error:
    // systemd (`Restart=on-failure`) then starts a fresh one.
    if let Some(fatal) = server.fatal() {
        let tx = tx.clone();
        thread::spawn(move || {
            if let Ok(why) = fatal.recv() {
                let _ = tx.send(Some(why));
            }
        });
    }
    ctrlc::set_handler(move || {
        if asked.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
        let _ = tx.send(None);
    })
    .map_err(|e| format!("installing the signal handler: {e}"))?;
    let failed = rx.recv().ok().flatten();
    server.shutdown();
    log::info!("stopped");
    match failed {
        Some(why) => Err(why),
        None => Ok(()),
    }
}
