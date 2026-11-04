//! Find serial ports that look like a VNA, and tell which protocol to expect.

use serialport::{SerialPortType, UsbPortInfo};

/// USB vendor/product id used by the NanoVNA V2 and LiteVNA firmware
/// (a Cypress CDC-ACM example id).
pub const LITEVNA_VID: u16 = 0x04B4;
pub const LITEVNA_PID: u16 = 0x0008;
/// STMicroelectronics virtual COM port: NanoVNA-H, NanoVNA-F and many other
/// STM32 boards. Identified further by the product string when present.
pub const STM_VCP_VID: u16 = 0x0483;
pub const STM_VCP_PID: u16 = 0x5740;

/// Which protocol a port speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Binary register protocol: LiteVNA 62/64/67, NanoVNA V2.
    LiteVna,
    /// Text shell: NanoVNA-H, NanoVNA-F, ...
    NanoVnaShell,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::LiteVna => "LiteVNA",
            Kind::NanoVnaShell => "NanoVNA",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortCandidate {
    pub path: String,
    pub usb: Option<UsbPortInfo>,
}

impl PortCandidate {
    /// The protocol to expect from the USB ids, if they say anything.
    pub fn kind(&self) -> Option<Kind> {
        let u = self.usb.as_ref()?;
        if u.vid == LITEVNA_VID && u.pid == LITEVNA_PID {
            return Some(Kind::LiteVna);
        }
        if u.vid == STM_VCP_VID && u.pid == STM_VCP_PID {
            // Any STM32 VCP could be here; a NanoVNA names itself when it can.
            let named = u.product.as_deref().map(|p| p.to_ascii_lowercase());
            return match named {
                Some(p) if p.contains("nanovna") || p.contains("vna") => Some(Kind::NanoVnaShell),
                None => Some(Kind::NanoVnaShell),
                Some(_) => None,
            };
        }
        None
    }

    /// `true` when the USB ids match a known analyzer.
    pub fn looks_like_vna(&self) -> bool {
        self.kind().is_some()
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
