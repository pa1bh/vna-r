//! `vna-link` — the link between a front-end and an analyser: a thread that
//! owns the device, takes [`Cmd`]s and answers with [`Event`]s.
//!
//! The GUI uses it today; it has no dependency on egui so that a headless
//! server can run the same thread (docs/10-client-server.md). [`wire`] is
//! what a server and its clients send each other.

pub mod wire;
mod worker;

pub use worker::{spawn, Cmd, Event, Notify, Purpose, WorkerHandle, RECONNECT_FOR};
