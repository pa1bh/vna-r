//! `vna-link` — the link between a front-end and an analyser: the GUI sends
//! [`Cmd`]s and gets [`Event`]s through one [`WorkerHandle`], whether the
//! analyser is on a serial port of this machine (a worker thread that owns
//! the device) or behind a vna-server on the network (a WebSocket session).
//!
//! It has no dependency on egui, so that the server runs the same worker
//! (docs/10-client-server.md). [`wire`] is what a server and its clients send
//! each other. A port named [`SIMULATOR`] opens a device that exists only in
//! software.

mod address;
mod device;
mod handle;
mod remote;
mod sim;
pub mod wire;
mod worker;

// The constants have no caller outside this crate; the documentation of what
// is public names them.
pub use address::{is_server_address, server_label, server_url, DEFAULT_SERVER_PORT};
#[cfg(not(target_arch = "wasm32"))]
pub use handle::spawn_worker;
pub use handle::{spawn, spawn_with, Notify, Options, WorkerHandle};
pub use remote::RemoteTiming;
pub use sim::{DEFAULT_RATE as SIMULATOR_RATE, SIMULATOR};
pub use worker::{Cmd, Event, Loss, Purpose, Via, CONNECT_FAILED, RECONNECT_FOR};
