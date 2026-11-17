//! What the worker asks of a device, so that it can drive the simulator
//! ([`crate::sim`]) with exactly the code that drives an analyser.
//!
//! The methods are those of [`litevna::Vna`] that the worker calls, with the
//! same signatures; for a `Vna` each one is that method and nothing else.

#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use litevna::device::SweepProgress;
use litevna::{LiteVna, Result, Screenshot, SweepConfig, SweepResult, Vna};

use crate::Purpose;

pub(crate) trait Device: Send {
    fn sweep_with(&mut self, cfg: &SweepConfig, progress: SweepProgress<'_>) -> Result<SweepResult>;
    fn screenshot(&mut self) -> Result<Screenshot>;
    fn battery_millivolts(&mut self) -> Result<u16>;
    fn release(&mut self) -> Result<()>;
    fn as_lite_mut(&mut self) -> Option<&mut LiteVna>;
    /// What the next sweep is for. A real analyser measures whatever is on
    /// its port and has no use for this; the simulator puts the standard
    /// being measured on its port, or the device under test for a display sweep.
    fn measuring(&mut self, _purpose: Purpose) {}
}

impl Device for Vna {
    fn sweep_with(&mut self, cfg: &SweepConfig, progress: SweepProgress<'_>) -> Result<SweepResult> {
        // In the 255-point chunks every LiteVNA firmware answers, not `READFIFO 0`.
        Vna::sweep_with(self, cfg, false, progress)
    }

    fn screenshot(&mut self) -> Result<Screenshot> {
        Vna::screenshot(self)
    }

    fn battery_millivolts(&mut self) -> Result<u16> {
        Vna::battery_millivolts(self)
    }

    fn release(&mut self) -> Result<()> {
        Vna::release(self)
    }

    fn as_lite_mut(&mut self) -> Option<&mut LiteVna> {
        Vna::as_lite_mut(self)
    }
}
