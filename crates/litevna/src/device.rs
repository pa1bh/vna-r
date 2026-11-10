//! [`LiteVna`]: drive a LiteVNA / NanoVNA V2 over its USB CDC serial port.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use num_complex::Complex64;
use serialport::{ClearBuffer, SerialPort};

use crate::error::{Error, Result};
use crate::protocol::{self as proto, reg, Channel, DataMode, FifoPoint, FIFO_MAX_CHUNK, FIFO_POINT_SIZE};

/// Identity registers `0xF0..=0xF4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    pub variant: u8,
    pub protocol_version: u8,
    pub hardware_revision: u8,
    pub fw_major: u8,
    pub fw_minor: u8,
}

impl DeviceInfo {
    /// The bootloader reports firmware major `0xFF`.
    pub fn in_dfu_mode(&self) -> bool {
        self.fw_major == proto::FW_MAJOR_DFU
    }
}

/// Sweep parameters as the device wants them: start + step + points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepConfig {
    pub start_hz: u64,
    pub step_hz: u64,
    pub points: u16,
    pub values_per_freq: u16,
    /// Ask the device for data with its own on-screen calibration applied
    /// (LiteVNA extension, `rawSamplesMode = 3`). Default is raw data.
    pub calibrated: bool,
}

impl SweepConfig {
    /// Linear sweep from `start_hz` to (approximately) `stop_hz`. The device
    /// only knows integer steps, so the real stop is `start + step·(points-1)`.
    pub fn linear(start_hz: u64, stop_hz: u64, points: u16) -> Result<Self> {
        if points < 2 {
            return Err(Error::InvalidArgument("a sweep needs at least 2 points".into()));
        }
        if stop_hz <= start_hz {
            return Err(Error::InvalidArgument("stop must be above start".into()));
        }
        let step_hz = (stop_hz - start_hz) / (points as u64 - 1);
        if step_hz == 0 {
            return Err(Error::InvalidArgument("span too small for this many points".into()));
        }
        Ok(SweepConfig {
            start_hz,
            step_hz,
            points,
            values_per_freq: 1,
            calibrated: false,
        })
    }

    /// Rebuild a config from a uniformly spaced frequency list (e.g. from a
    /// saved calibration).
    pub fn from_frequencies(freqs: &[u64]) -> Result<Self> {
        if freqs.len() < 2 || freqs.len() > u16::MAX as usize {
            return Err(Error::InvalidArgument(format!("{} frequencies: need 2..=65535", freqs.len())));
        }
        let step = freqs[1]
            .checked_sub(freqs[0])
            .filter(|s| *s > 0)
            .ok_or_else(|| Error::InvalidArgument("frequencies must be strictly increasing".into()))?;
        for w in freqs.windows(2) {
            if w[1].wrapping_sub(w[0]) != step {
                return Err(Error::InvalidArgument("frequencies are not uniformly spaced".into()));
            }
        }
        Ok(SweepConfig {
            start_hz: freqs[0],
            step_hz: step,
            points: freqs.len() as u16,
            values_per_freq: 1,
            calibrated: false,
        })
    }

    pub fn stop_hz(&self) -> u64 {
        self.start_hz + self.step_hz * (self.points as u64 - 1)
    }

    pub fn frequencies(&self) -> Vec<u64> {
        (0..self.points as u64).map(|i| self.start_hz + i * self.step_hz).collect()
    }
}

/// A completed sweep, indexed by frequency point.
#[derive(Debug, Clone)]
pub struct SweepResult {
    pub freqs_hz: Vec<u64>,
    pub s11: Vec<Complex64>,
    pub s21: Vec<Complex64>,
    /// The raw LiteVNA FIFO records, one per frequency point (`None` where a
    /// point never arrived). **Empty** for a sweep without raw records: a
    /// NanoVNA shell sweep, a Touchstone file, an average. `missing_points`
    /// is then 0; only a vector as long as `freqs_hz` says something about
    /// completeness.
    pub points: Vec<Option<FifoPoint>>,
    pub checksum_ok: usize,
    pub checksum_bad: usize,
    /// Records whose `freq_index` was already filled (the sweep wrapped or restarted).
    pub duplicates: usize,
    pub elapsed: Duration,
}

impl SweepResult {
    /// Points that never arrived; 0 for a sweep that carries no raw records.
    pub fn missing_points(&self) -> usize {
        if self.points.len() != self.freqs_hz.len() {
            return 0;
        }
        self.points.iter().filter(|p| p.is_none()).count()
    }

    /// Join the results of several segments into one sweep (multi-segment
    /// sweeps: e.g. HF fine plus UHF coarse). Counts and times add up; the
    /// caller keeps the segment boundaries if it needs them.
    pub fn concat(parts: &[SweepResult]) -> SweepResult {
        let mut out = SweepResult {
            freqs_hz: Vec::new(),
            s11: Vec::new(),
            s21: Vec::new(),
            points: Vec::new(),
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        };
        for p in parts {
            out.freqs_hz.extend_from_slice(&p.freqs_hz);
            out.s11.extend_from_slice(&p.s11);
            out.s21.extend_from_slice(&p.s21);
            out.points.extend_from_slice(&p.points);
            out.checksum_ok += p.checksum_ok;
            out.checksum_bad += p.checksum_bad;
            out.duplicates += p.duplicates;
            out.elapsed += p.elapsed;
        }
        out
    }

    /// The points `range` of this sweep as a sweep of its own. Sweeps from a
    /// NanoVNA shell or a Touchstone file carry no raw FIFO records; the slice
    /// then has none either.
    pub fn slice(&self, range: std::ops::Range<usize>) -> SweepResult {
        SweepResult {
            freqs_hz: self.freqs_hz[range.clone()].to_vec(),
            s11: self.s11[range.clone()].to_vec(),
            s21: self.s21[range.clone()].to_vec(),
            points: if self.points.len() == self.freqs_hz.len() {
                self.points[range].to_vec()
            } else {
                Vec::new()
            },
            checksum_ok: self.checksum_ok,
            checksum_bad: self.checksum_bad,
            duplicates: self.duplicates,
            elapsed: self.elapsed,
        }
    }

    /// Point ranges of the segments that make up this sweep, given the
    /// segment configs in sweep order.
    pub fn segment_ranges(cfgs: &[SweepConfig]) -> Vec<std::ops::Range<usize>> {
        let mut at = 0usize;
        cfgs.iter()
            .map(|c| {
                let r = at..at + c.points as usize;
                at += c.points as usize;
                r
            })
            .collect()
    }

    pub fn missing_indexes(&self) -> Vec<usize> {
        self.points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_none())
            .map(|(i, _)| i)
            .collect()
    }
}

/// Screen capture as sent by the device: RGB565 pixels.
#[derive(Debug, Clone)]
pub struct Screenshot {
    pub width: u16,
    pub height: u16,
    pub bits_per_pixel: u8,
    /// Byte order of each 16-bit pixel as this device sends it: big-endian on
    /// the LiteVNA, little-endian on the NanoVNA-F.
    pub big_endian: bool,
    pub data: Vec<u8>,
}

impl Screenshot {
    /// Convert to packed 8-bit RGB with the device's own byte order.
    pub fn to_rgb888(&self) -> Vec<u8> {
        self.to_rgb888_with(self.big_endian)
    }

    /// Convert to packed 8-bit RGB. `big_endian` selects the byte order of
    /// each 16-bit pixel (NanoVNA-Saver treats the LiteVNA 64 as big-endian).
    pub fn to_rgb888_with(&self, big_endian: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() / 2 * 3);
        for px in self.data.chunks_exact(2) {
            let v = if big_endian {
                u16::from_be_bytes([px[0], px[1]])
            } else {
                u16::from_le_bytes([px[0], px[1]])
            };
            let r = ((v >> 11) & 0x1F) as u32;
            let g = ((v >> 5) & 0x3F) as u32;
            let b = (v & 0x1F) as u32;
            out.push(((r * 527 + 23) >> 6) as u8);
            out.push(((g * 259 + 33) >> 6) as u8);
            out.push(((b * 527 + 23) >> 6) as u8);
        }
        out
    }
}

/// Time to allow per requested FIFO point, generous on purpose: the LiteVNA
/// does >1600 points/s, NanoVNA-Saver budgets 35 ms/point for the old V2.
const PER_POINT_BUDGET: Duration = Duration::from_millis(20);
const MIN_FIFO_TIMEOUT: Duration = Duration::from_secs(3);
const REGISTER_TIMEOUT: Duration = Duration::from_secs(2);
/// Small pause after writes, mirroring every reference implementation.
const WRITE_SETTLE: Duration = Duration::from_millis(50);

/// A connected device.
pub struct LiteVna {
    port: Box<dyn SerialPort>,
    /// Print every byte sent/received to stderr.
    pub trace: bool,
    /// Hand the screen back to the device UI when this handle is dropped (default `true`).
    pub release_on_drop: bool,
    /// Sweep the device is currently programmed with (while in USB data mode).
    configured: Option<SweepConfig>,
}

impl LiteVna {
    /// Open the serial port and bring the protocol parser into a known state.
    /// Does not touch the device UI yet (no sweep registers are written).
    pub fn open(path: &str) -> Result<Self> {
        // Baud rate is meaningless for USB CDC but must be set.
        let port = serialport::new(path, 115_200).timeout(Duration::from_millis(250)).open()?;
        let mut dev = LiteVna {
            port,
            trace: false,
            release_on_drop: true,
            configured: None,
        };
        dev.reset_protocol()?;
        Ok(dev)
    }

    // ----- low level -------------------------------------------------------

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        if self.trace {
            eprintln!("tx: {}", hex(bytes));
        }
        self.port.write_all(bytes)?;
        self.port.flush()?;
        Ok(())
    }

    /// Read exactly `buf.len()` bytes or fail with [`Error::Timeout`].
    fn recv_exact(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut filled = 0;
        while filled < buf.len() {
            let now = Instant::now();
            if now >= deadline {
                if self.trace && filled > 0 {
                    eprintln!("rx (partial): {}", hex(&buf[..filled]));
                }
                return Err(Error::Timeout {
                    expected: buf.len(),
                    got: filled,
                });
            }
            let slice = (deadline - now).min(Duration::from_millis(250));
            self.port.set_timeout(slice)?;
            match self.port.read(&mut buf[filled..]) {
                Ok(0) => {}
                Ok(n) => filled += n,
                Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if self.trace {
            let shown = buf.len().min(64);
            eprintln!("rx: {}{}", hex(&buf[..shown]), if buf.len() > shown { " …" } else { "" });
        }
        Ok(())
    }

    fn discard_input(&mut self) -> Result<()> {
        self.port.clear(ClearBuffer::Input)?;
        Ok(())
    }

    /// Send eight NOPs so a half-received command can never confuse the parser.
    pub fn reset_protocol(&mut self) -> Result<()> {
        self.send(&proto::RESET_SEQUENCE)?;
        std::thread::sleep(WRITE_SETTLE);
        self.discard_input()
    }

    /// INDICATE: the device must answer `'2'`. The cheapest "are you there?".
    pub fn indicate(&mut self) -> Result<()> {
        self.discard_input()?;
        self.send(&proto::encode_indicate())?;
        let mut b = [0u8; 1];
        self.recv_exact(&mut b, REGISTER_TIMEOUT)?;
        if b[0] != proto::INDICATE_ACK {
            return Err(Error::protocol(format!("INDICATE answered 0x{:02X}, expected 0x32", b[0])));
        }
        Ok(())
    }

    pub fn read_u8(&mut self, addr: u8) -> Result<u8> {
        self.discard_input()?;
        self.send(&proto::encode_read(addr))?;
        let mut b = [0u8; 1];
        self.recv_exact(&mut b, REGISTER_TIMEOUT)?;
        Ok(b[0])
    }

    pub fn read_u16(&mut self, addr: u8) -> Result<u16> {
        self.discard_input()?;
        self.send(&proto::encode_read2(addr))?;
        let mut b = [0u8; 2];
        self.recv_exact(&mut b, REGISTER_TIMEOUT)?;
        Ok(u16::from_le_bytes(b))
    }

    pub fn read_u32(&mut self, addr: u8) -> Result<u32> {
        self.discard_input()?;
        self.send(&proto::encode_read4(addr))?;
        let mut b = [0u8; 4];
        self.recv_exact(&mut b, REGISTER_TIMEOUT)?;
        Ok(u32::from_le_bytes(b))
    }

    pub fn write_u8(&mut self, addr: u8, value: u8) -> Result<()> {
        self.send(&proto::encode_write(addr, value))
    }

    pub fn write_u16(&mut self, addr: u8, value: u16) -> Result<()> {
        self.send(&proto::encode_write2(addr, value))
    }

    pub fn write_u32(&mut self, addr: u8, value: u32) -> Result<()> {
        self.send(&proto::encode_write4(addr, value))
    }

    pub fn write_u64(&mut self, addr: u8, value: u64) -> Result<()> {
        self.send(&proto::encode_write8(addr, value))
    }

    // ----- identity / housekeeping ---------------------------------------

    pub fn device_info(&mut self) -> Result<DeviceInfo> {
        Ok(DeviceInfo {
            variant: self.read_u8(reg::DEVICE_VARIANT)?,
            protocol_version: self.read_u8(reg::PROTOCOL_VERSION)?,
            hardware_revision: self.read_u8(reg::HARDWARE_REVISION)?,
            fw_major: self.read_u8(reg::FW_MAJOR)?,
            fw_minor: self.read_u8(reg::FW_MINOR)?,
        })
    }

    /// Battery voltage in millivolts (LiteVNA register, absent on a plain V2).
    pub fn battery_millivolts(&mut self) -> Result<u16> {
        self.read_u16(reg::BATTERY_MV)
    }

    pub fn average(&mut self) -> Result<u8> {
        self.read_u8(reg::AVERAGE)
    }

    pub fn channel(&mut self) -> Result<u8> {
        self.read_u8(reg::CHANNEL_SELECT)
    }

    pub fn set_data_mode(&mut self, mode: DataMode) -> Result<()> {
        self.write_u8(reg::RAW_SAMPLES_MODE, mode as u8)?;
        std::thread::sleep(WRITE_SETTLE);
        if mode != DataMode::Usb && mode != DataMode::Calibrated {
            self.configured = None;
        }
        Ok(())
    }

    /// Give the screen back to the on-device UI.
    pub fn exit_usb_mode(&mut self) -> Result<()> {
        self.set_data_mode(DataMode::Normal)
    }

    /// 1..=80; larger is slower and quieter (narrower IF bandwidth).
    pub fn set_average(&mut self, value: u8) -> Result<()> {
        if !(1..=80).contains(&value) {
            return Err(Error::InvalidArgument("average must be 1..=80".into()));
        }
        self.write_u8(reg::AVERAGE, value)
    }

    /// Output power of the low (MS5351) and high (MAX2871) frequency sources, 1..=3 each.
    pub fn set_power(&mut self, lf: u8, hf: u8) -> Result<()> {
        if !(1..=3).contains(&lf) || !(1..=3).contains(&hf) {
            return Err(Error::InvalidArgument("power levels must be 1..=3".into()));
        }
        self.write_u8(reg::LF_POWER, lf)?;
        self.write_u8(reg::HF_POWER, hf)
    }

    pub fn set_channel(&mut self, channel: Channel) -> Result<()> {
        self.write_u8(reg::CHANNEL_SELECT, channel as u8)
    }

    pub fn set_unix_time(&mut self, secs: u32) -> Result<()> {
        self.write_u32(reg::UNIX_TIME, secs)
    }

    // ----- sweeping -------------------------------------------------------

    /// Program the sweep. Writing these registers takes the device out of its
    /// own UI and into USB data mode; the sweep starts running immediately.
    pub fn configure_sweep(&mut self, cfg: &SweepConfig) -> Result<()> {
        if cfg.points == 0 {
            return Err(Error::InvalidArgument("points must be > 0".into()));
        }
        self.reset_protocol()?;
        let mode = if cfg.calibrated { DataMode::Calibrated } else { DataMode::Usb };
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&proto::encode_write(reg::RAW_SAMPLES_MODE, mode as u8));
        bytes.extend_from_slice(&proto::encode_write8(reg::SWEEP_START_HZ, cfg.start_hz));
        bytes.extend_from_slice(&proto::encode_write8(reg::SWEEP_STEP_HZ, cfg.step_hz));
        bytes.extend_from_slice(&proto::encode_write2(reg::SWEEP_POINTS, cfg.points));
        bytes.extend_from_slice(&proto::encode_write2(reg::VALUES_PER_FREQ, cfg.values_per_freq.max(1)));
        self.send(&bytes)?;
        std::thread::sleep(WRITE_SETTLE);
        self.configured = Some(*cfg);
        Ok(())
    }

    /// Drop whatever the continuously running sweep has already queued.
    pub fn clear_fifo(&mut self) -> Result<()> {
        self.write_u8(reg::VALUES_FIFO, 0)?;
        std::thread::sleep(WRITE_SETTLE);
        self.discard_input()
    }

    /// Read `count` records from the FIFO in chunks of at most 255.
    pub fn read_fifo_points(&mut self, count: usize) -> Result<Vec<FifoPoint>> {
        let mut out = Vec::with_capacity(count);
        let mut remaining = count;
        while remaining > 0 {
            let n = remaining.min(FIFO_MAX_CHUNK);
            self.send(&proto::encode_read_fifo(reg::VALUES_FIFO, n as u8))?;
            out.extend(self.recv_fifo_records(n)?);
            remaining -= n;
        }
        Ok(out)
    }

    /// LiteVNA extension: `READFIFO 0x30 0` returns every point of the sweep.
    pub fn read_fifo_all(&mut self, expected: usize) -> Result<Vec<FifoPoint>> {
        self.send(&proto::encode_read_fifo(reg::VALUES_FIFO, 0))?;
        self.recv_fifo_records(expected)
    }

    fn recv_fifo_records(&mut self, n: usize) -> Result<Vec<FifoPoint>> {
        let mut buf = vec![0u8; n * FIFO_POINT_SIZE];
        let timeout = (PER_POINT_BUDGET * n as u32).max(MIN_FIFO_TIMEOUT);
        self.recv_exact(&mut buf, timeout)?;
        buf.chunks_exact(FIFO_POINT_SIZE).map(FifoPoint::parse).collect()
    }

    /// Configure, clear the FIFO and collect one complete sweep.
    ///
    /// Records are read until every frequency index has arrived. Measured on
    /// a LiteVNA 67: clearing the FIFO leaves up to 32 already-queued records
    /// in flight, so reading exactly `points` records can miss the tail of the
    /// sweep; the duplicates are counted in [`SweepResult::duplicates`].
    ///
    /// With `fifo_all` the LiteVNA-only "send everything" request is used for
    /// the first read, otherwise points are fetched in chunks of 255 like
    /// NanoVNA-Saver does.
    pub fn sweep(&mut self, cfg: &SweepConfig, fifo_all: bool) -> Result<SweepResult> {
        if self.configured.as_ref() != Some(cfg) {
            self.configure_sweep(cfg)?;
        }
        self.clear_fifo()?;
        let started = Instant::now();
        let n = cfg.points as usize;

        let mut points: Vec<Option<FifoPoint>> = vec![None; n];
        let (mut ok, mut bad, mut dup, mut filled, mut received) = (0usize, 0usize, 0usize, 0usize, 0usize);
        let max_records = n * 3 + 2 * FIFO_MAX_CHUNK;
        let mut first = true;
        while filled < n {
            if received >= max_records {
                return Err(Error::protocol(format!(
                    "sweep did not complete: {filled}/{n} points after {received} records"
                )));
            }
            let records = if fifo_all && first {
                self.read_fifo_all(n)?
            } else {
                self.read_fifo_points((n - filled).min(FIFO_MAX_CHUNK))?
            };
            first = false;
            for r in records {
                received += 1;
                if !r.checksum_ok {
                    // Count it and leave the point empty: the device sweeps on and
                    // delivers this index again on its next pass, so a corrupt record
                    // never ends up in a measurement or a calibration.
                    bad += 1;
                    continue;
                }
                ok += 1;
                let idx = r.freq_index as usize;
                if idx >= n {
                    return Err(Error::protocol(format!("freq_index {idx} out of range for {n} points")));
                }
                if points[idx].is_some() {
                    dup += 1;
                } else {
                    filled += 1;
                }
                points[idx] = Some(r);
            }
        }
        let elapsed = started.elapsed();

        let zero = Complex64::new(0.0, 0.0);
        Ok(SweepResult {
            freqs_hz: cfg.frequencies(),
            s11: points.iter().map(|p| p.map_or(zero, |p| p.s11())).collect(),
            s21: points.iter().map(|p| p.map_or(zero, |p| p.s21())).collect(),
            points,
            checksum_ok: ok,
            checksum_bad: bad,
            duplicates: dup,
            elapsed,
        })
    }

    // ----- screenshot -----------------------------------------------------

    /// Ask the device for its framebuffer. The LiteVNA 67 has a 1024×600
    /// display, i.e. about 1.2 MB over full-speed USB, so allow some seconds.
    pub fn screenshot(&mut self) -> Result<Screenshot> {
        self.reset_protocol()?;
        self.configured = None; // the device redraws its UI for the capture
        self.send(&proto::encode_write(reg::SCREENSHOT, 0))?;
        let mut header = [0u8; 5];
        self.recv_exact(&mut header, Duration::from_secs(5))?;
        let width = u16::from_le_bytes([header[0], header[1]]);
        let height = u16::from_le_bytes([header[2], header[3]]);
        let bpp = header[4];
        // The LiteVNA family sends RGB565 (16 bpp) at 320×240 to 800×480; the
        // conversion assumes 16 bpp. Bound the allocation by the header too: a
        // garbage header (a non-LiteVNA answering the probe, a broken link)
        // could otherwise ask for u16 × u16 × 4 bytes, 17 GB.
        const MAX_SIDE: u16 = 4096;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || bpp != 16 {
            return Err(Error::protocol(format!("implausible screenshot header {header:02X?}")));
        }
        let size = width as usize * height as usize * (bpp as usize / 8);
        let mut data = vec![0u8; size];
        self.recv_exact(&mut data, Duration::from_secs(30))?;
        Ok(Screenshot {
            width,
            height,
            bits_per_pixel: bpp,
            big_endian: true,
            data,
        })
    }

    /// Opcode constants re-exported for callers that want to poke registers by hand.
    pub fn raw_command(&mut self, bytes: &[u8], reply_len: usize) -> Result<Vec<u8>> {
        self.discard_input()?;
        self.send(bytes)?;
        let mut buf = vec![0u8; reply_len];
        if reply_len > 0 {
            self.recv_exact(&mut buf, REGISTER_TIMEOUT)?;
        }
        Ok(buf)
    }
}

impl Drop for LiteVna {
    fn drop(&mut self) {
        // Best effort: never leave the device stuck in USB data mode.
        if self.release_on_drop {
            let _ = self.write_u8(reg::RAW_SAMPLES_MODE, DataMode::Normal as u8);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sweep_without_records(n: usize) -> SweepResult {
        SweepResult {
            freqs_hz: (0..n as u64).map(|i| 1_000_000 + i * 1_000).collect(),
            s11: vec![Complex64::new(0.1, 0.0); n],
            s21: vec![Complex64::new(0.0, 0.0); n],
            points: Vec::new(),
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn slice_works_without_raw_records() {
        // A NanoVNA shell sweep has no FIFO records; slicing it per segment must not panic.
        let s = sweep_without_records(10);
        let part = s.slice(4..10);
        assert_eq!(part.freqs_hz.len(), 6);
        assert_eq!(part.s11.len(), 6);
        assert!(part.points.is_empty());
        assert_eq!(part.freqs_hz[0], 1_004_000);
    }
}
