//! The NanoVNA V2 / LiteVNA wire protocol, as pure functions.
//!
//! Source: LiteVNA Product Manual (v1.4 firmware), appendix "USB data
//! interface", and the NanoVNA V2 user manual. All multi-byte integers are
//! little-endian. The host always initiates; the device only ever replies to
//! READ / READFIFO / INDICATE commands.

use num_complex::Complex64;

/// Command opcodes (byte 0 of every host-to-device command).
pub mod cmd {
    pub const NOP: u8 = 0x00;
    /// Device replies with ASCII `'2'` ([`super::INDICATE_ACK`]).
    pub const INDICATE: u8 = 0x0D;
    pub const READ: u8 = 0x10;
    pub const READ2: u8 = 0x11;
    pub const READ4: u8 = 0x12;
    pub const READFIFO: u8 = 0x18;
    pub const WRITE: u8 = 0x20;
    pub const WRITE2: u8 = 0x21;
    pub const WRITE4: u8 = 0x22;
    pub const WRITE8: u8 = 0x23;
    pub const WRITEFIFO: u8 = 0x28;
}

/// Register addresses in normal (non-DFU) operation.
pub mod reg {
    /// u64, sweep start frequency in Hz.
    pub const SWEEP_START_HZ: u8 = 0x00;
    /// u64, sweep step in Hz.
    pub const SWEEP_STEP_HZ: u8 = 0x10;
    /// u16, number of points.
    pub const SWEEP_POINTS: u8 = 0x20;
    /// u16, number of measurements emitted per frequency point.
    pub const VALUES_PER_FREQ: u8 = 0x22;
    /// u8, see [`super::DataMode`].
    pub const RAW_SAMPLES_MODE: u8 = 0x26;
    /// FIFO with 32-byte [`super::FifoPoint`] records. Writing any value clears it.
    pub const VALUES_FIFO: u8 = 0x30;
    /// u8, 1..=80 — FFT sample multiplier, effectively the IF bandwidth.
    pub const AVERAGE: u8 = 0x40;
    /// u8, 1..=3 — MS5351 (low frequency source) output power.
    pub const LF_POWER: u8 = 0x41;
    /// u8, 1..=3 — MAX2871 (high frequency source) output power.
    pub const HF_POWER: u8 = 0x42;
    /// u8, 0 = S11+S21, 1 = S11 only, 2 = S21 only.
    pub const CHANNEL_SELECT: u8 = 0x44;
    /// u32, unix timestamp.
    pub const UNIX_TIME: u8 = 0x58;
    /// u16, battery voltage in millivolts (LiteVNA only).
    pub const BATTERY_MV: u8 = 0x5C;
    /// u8, writing any value makes the device send a screenshot.
    pub const SCREENSHOT: u8 = 0xEE;
    /// u8, always 0x02.
    pub const DEVICE_VARIANT: u8 = 0xF0;
    /// u8, always 0x01.
    pub const PROTOCOL_VERSION: u8 = 0xF1;
    pub const HARDWARE_REVISION: u8 = 0xF2;
    /// u8, 0xFF when the device is in DFU (bootloader) mode.
    pub const FW_MAJOR: u8 = 0xF3;
    pub const FW_MINOR: u8 = 0xF4;
}

/// Reply byte to [`cmd::INDICATE`].
pub const INDICATE_ACK: u8 = 0x32;
/// Size of one record in the values FIFO.
pub const FIFO_POINT_SIZE: usize = 32;
/// Largest `NN` a single READFIFO can request. LiteVNA additionally
/// accepts `0` meaning "all points of the sweep".
pub const FIFO_MAX_CHUNK: usize = 255;
/// Value of `FW_MAJOR` when the bootloader is running.
pub const FW_MAJOR_DFU: u8 = 0xFF;

/// Values for [`reg::RAW_SAMPLES_MODE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataMode {
    /// PC controls the sweep, FIFO delivers raw (uncalibrated) I/Q data.
    Usb = 0,
    /// Raw ADC samples; leaves this protocol entirely. Not used here.
    RawSamples = 1,
    /// Hand control back to the on-device UI.
    Normal = 2,
    /// LiteVNA extension: FIFO delivers data with the on-device calibration applied.
    Calibrated = 3,
}

/// Values for [`reg::CHANNEL_SELECT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Channel {
    Both = 0,
    S11Only = 1,
    S21Only = 2,
}

/// Eight NOPs: enough to flush any half-received command (WRITE8 is 10 bytes,
/// but every existing host implementation uses 8 and the firmware tolerates it).
pub const RESET_SEQUENCE: [u8; 8] = [cmd::NOP; 8];

pub fn encode_indicate() -> [u8; 1] {
    [cmd::INDICATE]
}

pub fn encode_read(addr: u8) -> [u8; 2] {
    [cmd::READ, addr]
}

pub fn encode_read2(addr: u8) -> [u8; 2] {
    [cmd::READ2, addr]
}

pub fn encode_read4(addr: u8) -> [u8; 2] {
    [cmd::READ4, addr]
}

/// `count` 0 means "all points" on LiteVNA firmware.
pub fn encode_read_fifo(addr: u8, count: u8) -> [u8; 3] {
    [cmd::READFIFO, addr, count]
}

pub fn encode_write(addr: u8, value: u8) -> [u8; 3] {
    [cmd::WRITE, addr, value]
}

pub fn encode_write2(addr: u8, value: u16) -> [u8; 4] {
    let b = value.to_le_bytes();
    [cmd::WRITE2, addr, b[0], b[1]]
}

pub fn encode_write4(addr: u8, value: u32) -> [u8; 6] {
    let b = value.to_le_bytes();
    [cmd::WRITE4, addr, b[0], b[1], b[2], b[3]]
}

pub fn encode_write8(addr: u8, value: u64) -> [u8; 10] {
    let b = value.to_le_bytes();
    [cmd::WRITE8, addr, b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]
}

/// Checksum over the first 31 bytes of a FIFO record as implemented by
/// NanoVNA-App (DiSlord). Not part of the official documentation, where byte
/// 0x1F is "reserved"; whether the LiteVNA 67 fills it is something to verify.
pub fn fifo_checksum(record: &[u8]) -> u8 {
    let mut c: u8 = 0x46;
    for &b in &record[..FIFO_POINT_SIZE - 1] {
        c = (c ^ ((c << 1) | 1)) ^ b;
    }
    c
}

/// One 32-byte record from the values FIFO.
///
/// `fwd0` is the reference (outgoing) wave, `rev0` the wave coming back into
/// port 1 (reflection), `rev1` the wave arriving at port 2 (transmission).
/// All three have an arbitrary common phase, so only the ratios are meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FifoPoint {
    pub fwd0_re: i32,
    pub fwd0_im: i32,
    pub rev0_re: i32,
    pub rev0_im: i32,
    pub rev1_re: i32,
    pub rev1_im: i32,
    pub freq_index: u16,
    pub reserved: [u8; 5],
    pub checksum: u8,
    /// `true` when [`fifo_checksum`] matches byte 0x1F.
    pub checksum_ok: bool,
}

impl FifoPoint {
    pub fn parse(record: &[u8]) -> crate::Result<Self> {
        if record.len() < FIFO_POINT_SIZE {
            return Err(crate::Error::protocol(format!(
                "FIFO record too short: {} bytes",
                record.len()
            )));
        }
        let i32_at = |o: usize| i32::from_le_bytes([record[o], record[o + 1], record[o + 2], record[o + 3]]);
        let checksum = record[0x1F];
        Ok(FifoPoint {
            fwd0_re: i32_at(0x00),
            fwd0_im: i32_at(0x04),
            rev0_re: i32_at(0x08),
            rev0_im: i32_at(0x0C),
            rev1_re: i32_at(0x10),
            rev1_im: i32_at(0x14),
            freq_index: u16::from_le_bytes([record[0x18], record[0x19]]),
            reserved: [record[0x1A], record[0x1B], record[0x1C], record[0x1D], record[0x1E]],
            checksum,
            checksum_ok: fifo_checksum(record) == checksum,
        })
    }

    pub fn fwd0(&self) -> Complex64 {
        Complex64::new(self.fwd0_re as f64, self.fwd0_im as f64)
    }

    pub fn rev0(&self) -> Complex64 {
        Complex64::new(self.rev0_re as f64, self.rev0_im as f64)
    }

    pub fn rev1(&self) -> Complex64 {
        Complex64::new(self.rev1_re as f64, self.rev1_im as f64)
    }

    /// Raw (uncalibrated) reflection coefficient Γ at port 1.
    pub fn s11(&self) -> Complex64 {
        self.rev0() / self.fwd0()
    }

    /// Raw (uncalibrated) transmission coefficient port 1 → port 2.
    pub fn s21(&self) -> Complex64 {
        self.rev1() / self.fwd0()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoders_match_manual_examples() {
        // Examples straight from the LiteVNA manual appendix.
        assert_eq!(encode_write(reg::AVERAGE, 5), [0x20, 0x40, 0x05]);
        assert_eq!(encode_write(reg::VALUES_FIFO, 0), [0x20, 0x30, 0x00]);
        assert_eq!(
            encode_write8(reg::SWEEP_START_HZ, 10_000),
            [0x23, 0x00, 0x10, 0x27, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            encode_write8(reg::SWEEP_STEP_HZ, 1_000_000),
            [0x23, 0x10, 0x40, 0x42, 0x0F, 0, 0, 0, 0, 0]
        );
        assert_eq!(encode_write2(reg::SWEEP_POINTS, 0x400), [0x21, 0x20, 0x00, 0x04]);
        assert_eq!(encode_write2(reg::VALUES_PER_FREQ, 1), [0x21, 0x22, 0x01, 0x00]);
        assert_eq!(encode_read_fifo(reg::VALUES_FIFO, 255), [0x18, 0x30, 0xFF]);
        assert_eq!(encode_read(reg::DEVICE_VARIANT), [0x10, 0xF0]);
        assert_eq!(encode_write4(reg::UNIX_TIME, 0x0102_0304), [0x22, 0x58, 0x04, 0x03, 0x02, 0x01]);
    }

    fn sample_record() -> [u8; 32] {
        let mut r = [0u8; 32];
        r[0x00..0x04].copy_from_slice(&1000i32.to_le_bytes()); // fwd re
        r[0x04..0x08].copy_from_slice(&0i32.to_le_bytes()); // fwd im
        r[0x08..0x0C].copy_from_slice(&(-500i32).to_le_bytes()); // rev0 re
        r[0x0C..0x10].copy_from_slice(&500i32.to_le_bytes()); // rev0 im
        r[0x10..0x14].copy_from_slice(&0i32.to_le_bytes()); // rev1 re
        r[0x14..0x18].copy_from_slice(&100i32.to_le_bytes()); // rev1 im
        r[0x18..0x1A].copy_from_slice(&42u16.to_le_bytes());
        r[0x1F] = fifo_checksum(&r);
        r
    }

    #[test]
    fn parses_fifo_record_and_ratios() {
        let p = FifoPoint::parse(&sample_record()).unwrap();
        assert_eq!(p.freq_index, 42);
        assert!(p.checksum_ok);
        let s11 = p.s11();
        assert!((s11.re - -0.5).abs() < 1e-12 && (s11.im - 0.5).abs() < 1e-12);
        let s21 = p.s21();
        assert!((s21.re).abs() < 1e-12 && (s21.im - 0.1).abs() < 1e-12);
    }

    #[test]
    fn detects_bad_checksum_and_short_record() {
        let mut r = sample_record();
        r[0x1F] ^= 0xFF;
        assert!(!FifoPoint::parse(&r).unwrap().checksum_ok);
        assert!(FifoPoint::parse(&r[..10]).is_err());
    }
}
