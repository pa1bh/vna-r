//! [`Vna`]: one handle for every supported analyzer, whichever protocol it speaks.
//!
//! * LiteVNA 62/64/67 and NanoVNA V2: the binary register protocol ([`crate::device::LiteVna`]).
//! * NanoVNA-F, NanoVNA-H and friends: the text shell ([`crate::shell::NanoVnaShell`]).
//!
//! The CLI and the GUI only need sweeps, screenshots, identity and battery, so
//! that is what this enum offers; protocol-specific extras (registers,
//! averaging, bandwidth) stay on the concrete types behind [`Vna::as_lite_mut`]
//! and [`Vna::as_shell_mut`].

use crate::device::{DeviceInfo, LiteVna, Screenshot, SweepConfig, SweepProgress, SweepResult};
use crate::discovery::{self, Kind};
use crate::error::{Error, Result};
use crate::shell::{NanoVnaShell, ShellInfo, MAX_SCAN_POINTS};

/// What a device can do, for the UI to warn about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub min_hz: u64,
    pub max_hz: u64,
    /// Points the device measures in one go; longer sweeps are chunked by the driver.
    pub max_points_per_scan: Option<u16>,
    /// The LiteVNA averaging register exists (`set_average`).
    pub has_device_average: bool,
}

/// Identity of a connected device, protocol-independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VnaInfo {
    pub kind: Kind,
    /// `LiteVNA` or the model line of the shell, e.g. `NanoVNA-F 4.3`.
    pub model: String,
    /// `2.2` (LiteVNA fw major.minor) or `Firmware 1.0.4 by BH5HNU`.
    pub firmware: String,
    /// Hardware revision or build time.
    pub detail: String,
    pub limits: Limits,
    /// The raw identity registers, LiteVNA only.
    pub lite: Option<DeviceInfo>,
    /// The raw `info` reply, shell only.
    pub shell: Option<ShellInfo>,
}

impl VnaInfo {
    /// One line for a status bar: model, firmware, detail.
    pub fn summary(&self) -> String {
        let mut s = format!("{}  fw {}", self.model, self.firmware);
        if !self.detail.is_empty() {
            s.push_str("  ");
            s.push_str(&self.detail);
        }
        s
    }
}

/// A connected analyzer.
pub enum Vna {
    Lite(LiteVna),
    Shell(NanoVnaShell),
}

impl Vna {
    /// Open `path`, choosing the protocol by the USB ids of the port. For a
    /// port without a recognisable id both protocols are tried, LiteVNA first.
    pub fn open(path: &str) -> Result<Vna> {
        let kind = discovery::list_ports()?.into_iter().find(|p| p.path == path).and_then(|p| p.kind());
        match kind {
            Some(k) => Vna::open_kind(path, k),
            None => match Vna::open_kind(path, Kind::LiteVna) {
                Ok(v) => Ok(v),
                Err(first) => Vna::open_kind(path, Kind::NanoVnaShell)
                    .map_err(|second| Error::protocol(format!("neither protocol answered on {path}: LiteVNA: {first}; shell: {second}"))),
            },
        }
    }

    /// Open `path` with a known protocol.
    pub fn open_kind(path: &str, kind: Kind) -> Result<Vna> {
        match kind {
            Kind::LiteVna => {
                let mut d = LiteVna::open(path)?;
                d.indicate()?;
                Ok(Vna::Lite(d))
            }
            Kind::NanoVnaShell => Ok(Vna::Shell(NanoVnaShell::open(path)?)),
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Vna::Lite(_) => Kind::LiteVna,
            Vna::Shell(_) => Kind::NanoVnaShell,
        }
    }

    pub fn set_trace(&mut self, on: bool) {
        match self {
            Vna::Lite(d) => d.trace = on,
            Vna::Shell(d) => d.trace = on,
        }
    }

    pub fn as_lite_mut(&mut self) -> Option<&mut LiteVna> {
        match self {
            Vna::Lite(d) => Some(d),
            _ => None,
        }
    }

    /// The LiteVNA behind this handle, or [`Error::Unsupported`] naming what
    /// (`peek`, `--average`, …) needs its registers.
    pub fn require_lite(&mut self, what: &str) -> Result<&mut LiteVna> {
        let device = self.kind().name().to_string();
        self.as_lite_mut().ok_or_else(|| Error::Unsupported {
            what: what.to_string(),
            device,
        })
    }

    pub fn as_shell_mut(&mut self) -> Option<&mut NanoVnaShell> {
        match self {
            Vna::Shell(d) => Some(d),
            _ => None,
        }
    }

    pub fn info(&mut self) -> Result<VnaInfo> {
        match self {
            Vna::Lite(d) => {
                let i = d.device_info()?;
                Ok(VnaInfo {
                    kind: Kind::LiteVna,
                    model: "LiteVNA".into(),
                    firmware: if i.in_dfu_mode() {
                        "DFU".into()
                    } else {
                        format!("{}.{}", i.fw_major, i.fw_minor)
                    },
                    detail: format!("hw {}", i.hardware_revision),
                    limits: Limits {
                        min_hz: 50_000,
                        max_hz: 6_300_000_000,
                        max_points_per_scan: None,
                        has_device_average: true,
                    },
                    lite: Some(i),
                    shell: None,
                })
            }
            Vna::Shell(d) => {
                let i = d.info()?;
                let is_f = i.model.to_ascii_lowercase().contains("nanovna-f");
                Ok(VnaInfo {
                    kind: Kind::NanoVnaShell,
                    model: if i.model.is_empty() { "NanoVNA".into() } else { i.model.clone() },
                    firmware: i.firmware.trim_start_matches("Firmware ").to_string(),
                    detail: String::new(),
                    limits: Limits {
                        min_hz: if is_f { 10_000 } else { 50_000 },
                        max_hz: 1_500_000_000,
                        max_points_per_scan: Some(MAX_SCAN_POINTS),
                        has_device_average: false,
                    },
                    lite: None,
                    shell: Some(i),
                })
            }
        }
    }

    pub fn battery_millivolts(&mut self) -> Result<u16> {
        match self {
            Vna::Lite(d) => d.battery_millivolts(),
            Vna::Shell(d) => d.battery_millivolts(),
        }
    }

    /// One sweep of raw data (or device-calibrated data when `cfg.calibrated`).
    /// `fifo_all` only matters for the LiteVNA.
    pub fn sweep(&mut self, cfg: &SweepConfig, fifo_all: bool) -> Result<SweepResult> {
        match self {
            Vna::Lite(d) => d.sweep(cfg, fifo_all),
            Vna::Shell(d) => d.sweep(cfg),
        }
    }

    /// [`Vna::sweep`] that reports progress after every block of data and can
    /// be stopped there, see [`SweepProgress`]: within 255 points on a
    /// LiteVNA, within 301 points (about 3 s) on a NanoVNA shell.
    pub fn sweep_with(&mut self, cfg: &SweepConfig, fifo_all: bool, progress: SweepProgress<'_>) -> Result<SweepResult> {
        match self {
            Vna::Lite(d) => d.sweep_with(cfg, fifo_all, progress),
            Vna::Shell(d) => d.sweep_with(cfg, progress),
        }
    }

    pub fn screenshot(&mut self) -> Result<Screenshot> {
        match self {
            Vna::Lite(d) => d.screenshot(),
            Vna::Shell(d) => d.screenshot(),
        }
    }

    /// Hand the screen back to the device: leave USB data mode (LiteVNA) or
    /// restore and resume its own sweep (shell).
    pub fn release(&mut self) -> Result<()> {
        match self {
            Vna::Lite(d) => d.exit_usb_mode(),
            Vna::Shell(d) => d.restore(),
        }
    }

    /// Whether dropping the handle releases the device (default `true`).
    pub fn set_release_on_drop(&mut self, on: bool) {
        match self {
            Vna::Lite(d) => d.release_on_drop = on,
            Vna::Shell(d) => d.restore_on_drop = on,
        }
    }
}
