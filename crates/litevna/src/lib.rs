//! `litevna` — a small, dependency-light driver for the LiteVNA family
//! (LiteVNA 62 / 64 / 67) and other devices speaking the NanoVNA V2
//! ("S-A-A-2") binary register protocol, plus the text shell of the
//! NanoVNA-H / NanoVNA-F behind the same [`Vna`] handle.
//!
//! The crate is split in layers so the protocol can be unit-tested without
//! hardware:
//!
//! * [`protocol`] — pure functions: opcode/register constants, command
//!   encoders and the 32-byte FIFO record parser.
//! * [`device`] — [`LiteVna`], which drives a serial port and exposes a
//!   typed API (device info, sweeps, screenshots, ...).
//! * [`shell`] — [`NanoVnaShell`], the text command line of the NanoVNA-H/-F.
//! * [`vna`] — [`Vna`], one handle over both protocols.
//! * [`discovery`] — find candidate USB serial ports and their protocol.
//! * [`calibration`] — SOL(T) error-term calibration applied on the host.
//! * [`timedomain`] — TDR: impulse/step response and impedance along a line from S11.
//! * [`touchstone`] — read Touchstone `.s1p`/`.s2p` files, write `.s2p` and CSV.
//! * [`traces`] — derived quantities for display: dB, phase, SWR, impedance, group delay.
//! * [`freq`] — parse human friendly frequencies such as `"14.2M"`.

/// Reference impedance of the measurement system, ohm. Everything in this
/// crate (calibration kit models, impedance, SWR, TDR) is relative to it.
pub const Z0: f64 = 50.0;

pub mod calibration;
pub mod device;
pub mod discovery;
pub mod error;
#[cfg(test)]
mod fakeport;
pub mod freq;
pub mod protocol;
pub mod shell;
pub mod timedomain;
pub mod touchstone;
pub mod traces;
pub mod vna;

pub use calibration::{CalKit, Calibration, CalibrationSet, Standard};
pub use device::{DeviceInfo, LiteVna, Screenshot, SweepConfig, SweepProgress, SweepResult};
pub use error::{Error, Result};
pub use protocol::FifoPoint;
pub use shell::NanoVnaShell;
pub use vna::{Limits, Vna, VnaInfo};
