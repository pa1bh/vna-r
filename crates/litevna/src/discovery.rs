//! Find serial ports that look like a LiteVNA / NanoVNA V2.

use serialport::{SerialPortType, UsbPortInfo};

/// USB vendor/product id used by the NanoVNA V2 and LiteVNA firmware
/// (a Cypress CDC-ACM example id).
pub const LITEVNA_VID: u16 = 0x04B4;
pub const LITEVNA_PID: u16 = 0x0008;

#[derive(Debug, Clone)]
pub struct PortCandidate {
    pub path: String,
    pub usb: Option<UsbPortInfo>,
}

impl PortCandidate {
    /// `true` when the USB ids match the LiteVNA / NanoVNA V2 firmware.
    pub fn looks_like_vna(&self) -> bool {
        matches!(&self.usb, Some(u) if u.vid == LITEVNA_VID && u.pid == LITEVNA_PID)
    }
}

/// All serial ports on the system. On macOS the `/dev/tty.*` twins are
/// dropped; `/dev/cu.*` is the one to open for a device we talk to.
pub fn list_ports() -> crate::Result<Vec<PortCandidate>> {
    let mut out = Vec::new();
    for p in serialport::available_ports()? {
        if cfg!(target_os = "macos") && p.port_name.starts_with("/dev/tty.") {
            continue;
        }
        let usb = match p.port_type {
            SerialPortType::UsbPort(info) => Some(info),
            _ => None,
        };
        out.push(PortCandidate { path: p.port_name, usb });
    }
    Ok(out)
}

/// The single port that looks like a VNA, if there is exactly one.
pub fn find_vna_port() -> crate::Result<Option<String>> {
    let vnas: Vec<_> = list_ports()?.into_iter().filter(|p| p.looks_like_vna()).collect();
    match vnas.len() {
        1 => Ok(Some(vnas[0].path.clone())),
        0 => Ok(None),
        n => Err(crate::Error::InvalidArgument(format!(
            "{n} VNA-like ports found, pass --port explicitly: {}",
            vnas.iter().map(|p| p.path.as_str()).collect::<Vec<_>>().join(", ")
        ))),
    }
}
