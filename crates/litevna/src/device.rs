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

    /// Parse one segment written as `START:STOP:POINTS`, e.g. `100k:50M:1001`
    /// (frequencies as [`crate::freq::parse_hz`] takes them).
    pub fn parse_segment(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split(':').collect();
        let [start, stop, points] = parts.as_slice() else {
            return Err(Error::InvalidArgument(format!("a segment is START:STOP:POINTS, got {spec:?}")));
        };
        let points: u16 = points
            .trim()
            .parse()
            .map_err(|_| Error::InvalidArgument(format!("segment {spec:?}: {points:?} is not a number of points")))?;
        SweepConfig::linear(crate::freq::parse_hz(start)?, crate::freq::parse_hz(stop)?, points)
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

    /// The last frequency; `start_hz` for a (meaningless) zero-point config
    /// rather than an underflow.
    pub fn stop_hz(&self) -> u64 {
        self.start_hz + self.step_hz * u64::from(self.points).saturating_sub(1)
    }

    pub fn frequencies(&self) -> Vec<u64> {
        (0..self.points as u64).map(|i| self.start_hz + i * self.step_hz).collect()
    }
}

/// Put the segments of a multi-segment sweep in sweep order (by start
/// frequency) and check that none overlaps the next. Segments may touch: the
/// shared point is measured twice and written once on export. `T` is whatever
/// travels with a segment, such as its calibration file.
pub fn plan_segments<T>(mut segments: Vec<(SweepConfig, T)>) -> Result<Vec<(SweepConfig, T)>> {
    segments.sort_by_key(|(c, _)| c.start_hz);
    for w in segments.windows(2) {
        let (a, b) = (&w[0].0, &w[1].0);
        if b.start_hz < a.stop_hz() {
            return Err(Error::InvalidArgument(format!(
                "segments overlap: {}–{} and {}–{}",
                crate::freq::format_hz(a.start_hz),
                crate::freq::format_hz(a.stop_hz()),
                crate::freq::format_hz(b.start_hz),
                crate::freq::format_hz(b.stop_hz())
            )));
        }
    }
    Ok(segments)
}

/// Segment edges that divide `start_hz..=stop_hz` into `segments` ranges of
/// equal ratio (each spans the same factor), touching end to start: with the
/// same number of points per segment the grid is then dense at the low end
/// and coarse at the top, which is what 50 kHz to 6 GHz in one picture needs.
/// Edges are rounded to whole hertz; a kilohertz start over a gigahertz stop
/// in four segments gives about 1:18 per segment.
pub fn log_segment_edges(start_hz: u64, stop_hz: u64, segments: usize) -> Result<Vec<(u64, u64)>> {
    if start_hz == 0 || stop_hz <= start_hz || segments == 0 {
        return Err(Error::InvalidArgument(
            "logarithmic spacing needs a start above 0 Hz, a stop above the start and at least one segment".into(),
        ));
    }
    let ratio = (stop_hz as f64 / start_hz as f64).powf(1.0 / segments as f64);
    let edge = |k: usize| -> u64 {
        if k == segments {
            stop_hz
        } else {
            (start_hz as f64 * ratio.powi(k as i32)).round() as u64
        }
    };
    let edges: Vec<(u64, u64)> = (0..segments).map(|k| (edge(k), edge(k + 1))).collect();
    if edges.iter().any(|(a, b)| b <= a) {
        return Err(Error::InvalidArgument(
            "too many segments for this span: segments would be empty".into(),
        ));
    }
    Ok(edges)
}

/// What a sweep calls while it runs: after every block of data, with the
/// number of points measured so far and the total. Returning
/// `ControlFlow::Break(())` stops the sweep, which then fails with
/// [`Error::Cancelled`]. A block is one READFIFO chunk of at most 255 records
/// on a LiteVNA and one `scan_bin` of at most 301 points on a NanoVNA shell,
/// so that is how soon a sweep can be stopped.
pub type SweepProgress<'a> = &'a mut dyn FnMut(usize, usize) -> std::ops::ControlFlow<()>;

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

    /// Port extension: move the reference plane of port 1 outward by a
    /// one-way delay (seconds; negative moves it inward), as if the
    /// calibration had been done that much further along a lossless line.
    /// S11 is turned by twice the delay (the wave goes there and back), S21
    /// by once. For a fixture or an adapter behind the calibration plane
    /// whose delay you know or read off the TDR.
    pub fn extend_port(&mut self, delay_s: f64) {
        for (i, &f) in self.freqs_hz.iter().enumerate() {
            let turn = 2.0 * std::f64::consts::PI * f as f64 * delay_s;
            self.s11[i] *= Complex64::from_polar(1.0, 2.0 * turn);
            self.s21[i] *= Complex64::from_polar(1.0, turn);
        }
    }

    /// The complex mean of several sweeps on the same grid, per point, for
    /// both S11 and S21. Averaging the complex values lowers noise without the
    /// bias that averaging magnitudes in dB has. Counts and times add up; the
    /// raw records are those of the newest sweep. `None` for no sweeps or
    /// sweeps on different grids.
    pub fn average<'a>(sweeps: impl IntoIterator<Item = &'a SweepResult>) -> Option<SweepResult> {
        Self::average_trimmed(sweeps, 0)
    }

    /// [`average`](Self::average) that first leaves out, per point and per
    /// parameter, the `drop` values farthest from the plain mean: a glitch in
    /// one sweep (a USB hiccup, a hand near the antenna) then does not pull
    /// the mean. At least one value always remains, so with fewer than
    /// `drop + 1` sweeps (a buffer still filling) fewer are dropped.
    pub fn average_trimmed<'a>(sweeps: impl IntoIterator<Item = &'a SweepResult>, drop: usize) -> Option<SweepResult> {
        let sweeps: Vec<&SweepResult> = sweeps.into_iter().collect();
        let newest = *sweeps.last()?;
        if sweeps.iter().any(|s| s.freqs_hz != newest.freqs_hz) {
            return None;
        }
        let drop = drop.min(sweeps.len() - 1);
        let mut out = newest.clone();
        let mut values: Vec<Complex64> = Vec::with_capacity(sweeps.len());
        let mut trimmed_mean = |pick: &dyn Fn(&SweepResult) -> Complex64| -> Complex64 {
            values.clear();
            values.extend(sweeps.iter().map(|s| pick(s)));
            let mean = values.iter().sum::<Complex64>() / values.len() as f64;
            if drop > 0 {
                values.sort_by(|a, b| (a - mean).norm().total_cmp(&(b - mean).norm()));
                values.truncate(values.len() - drop);
            }
            values.iter().sum::<Complex64>() / values.len() as f64
        };
        for i in 0..out.freqs_hz.len() {
            out.s11[i] = trimmed_mean(&|s| s.s11[i]);
            out.s21[i] = trimmed_mean(&|s| s.s21[i]);
        }
        out.checksum_ok = sweeps.iter().map(|s| s.checksum_ok).sum();
        out.checksum_bad = sweeps.iter().map(|s| s.checksum_bad).sum();
        out.duplicates = sweeps.iter().map(|s| s.duplicates).sum();
        out.elapsed = sweeps.iter().map(|s| s.elapsed).sum();
        Some(out)
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

    /// Point range of the segment with the widest span, of a sweep of
    /// `total_points` made of `cfgs` in sweep order: the part TDR and gating
    /// work on, because they need one uniform grid and resolution goes with
    /// span. The whole sweep when there is one segment, or when `cfgs` does
    /// not describe a sweep of this size (a file, an older plan).
    pub fn widest_segment(cfgs: &[SweepConfig], total_points: usize) -> std::ops::Range<usize> {
        let ranges = Self::segment_ranges(cfgs);
        if ranges.len() <= 1 || ranges.last().map_or(0, |r| r.end) != total_points {
            return 0..total_points;
        }
        ranges
            .into_iter()
            .zip(cfgs)
            .max_by_key(|(_, c)| c.stop_hz() - c.start_hz)
            .map(|(r, _)| r)
            .unwrap_or(0..total_points)
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
        for px in self.data.as_chunks::<2>().0 {
            let v = if big_endian {
                u16::from_be_bytes(*px)
            } else {
                u16::from_le_bytes(*px)
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
/// measured 330 to 1100 points/s depending on channel selection and averaging
/// (docs/01 §7 and §8), NanoVNA-Saver budgets 35 ms/point for the old V2.
const PER_POINT_BUDGET: Duration = Duration::from_millis(20);
// The fixed waits are short under `cargo test`: the fake port answers at once.
const MIN_FIFO_TIMEOUT: Duration = Duration::from_millis(if cfg!(test) { 100 } else { 3000 });
const REGISTER_TIMEOUT: Duration = Duration::from_millis(if cfg!(test) { 100 } else { 2000 });
/// Small pause after writes, mirroring every reference implementation.
const WRITE_SETTLE: Duration = Duration::from_millis(if cfg!(test) { 0 } else { 50 });

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
        Self::from_port(port)
    }

    /// Drive a device over a port that is already open: another transport
    /// than a local serial device, or the fake port of the tests. Brings the
    /// protocol parser into a known state like [`LiteVna::open`].
    pub fn from_port(port: Box<dyn SerialPort>) -> Result<Self> {
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
        buf.as_chunks::<FIFO_POINT_SIZE>()
            .0
            .iter()
            .map(|record| FifoPoint::parse(record))
            .collect()
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
        self.sweep_with(cfg, fifo_all, &mut |_, _| std::ops::ControlFlow::Continue(()))
    }

    /// [`LiteVna::sweep`] that reports progress and can be stopped, see [`SweepProgress`].
    pub fn sweep_with(&mut self, cfg: &SweepConfig, fifo_all: bool, progress: SweepProgress<'_>) -> Result<SweepResult> {
        let result = self.sweep_inner(cfg, fifo_all, progress);
        if result.is_err() {
            // After a failure or a cancel the reply stream may be out of step
            // (a READFIFO answered late, half a record read). Forget the
            // configuration so the next sweep starts with the protocol reset
            // in `configure_sweep`.
            self.configured = None;
        }
        result
    }

    fn sweep_inner(&mut self, cfg: &SweepConfig, fifo_all: bool, progress: SweepProgress<'_>) -> Result<SweepResult> {
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
            if progress(filled, n).is_break() && filled < n {
                return Err(Error::Cancelled);
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

    /// Send arbitrary bytes and read `reply_len` bytes back: an escape hatch
    /// for exploring opcodes this driver has no method for (build the bytes
    /// with the encoders in [`crate::protocol`]). Not used by the CLI or GUI.
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
    use crate::fakeport::{FakeDevice, FakePort};
    use crate::protocol::{cmd, fifo_checksum};
    use std::collections::VecDeque;

    /// A LiteVNA as the driver sees it: registers, and a sweep that runs
    /// continuously and delivers 32-byte records on READFIFO.
    ///
    /// Every record carries where it came from: `rev0 = (index, pass)` with
    /// `fwd0 = (1000, 0)`, so S11 = (index + j·pass) / 1000. `pass` counts how
    /// often the sweep has wrapped; records queued before a FIFO clear have
    /// pass −1.
    struct FakeLite {
        pending: Vec<u8>,
        mem: [u8; 256],
        /// Records still in flight when the host clears the FIFO (the real
        /// device leaves up to 32, docs/01 §7): delivered first after a clear.
        in_flight_after_clear: Vec<u16>,
        queued: VecDeque<[u8; 32]>,
        /// Where the running sweep is when the host clears the FIFO (the
        /// sweep runs on between the configuration and the clear).
        position_at_clear: Option<u16>,
        next_index: u16,
        pass: i32,
        generated: usize,
        /// Ordinals (0-based, counting generated records) to send with a bad checksum.
        corrupt: Vec<usize>,
        /// Ordinal → frequency index to send instead of the real one.
        wrong_index: Option<(usize, u16)>,
        /// The sweep never advances beyond this index.
        stuck_at: Option<u16>,
        /// Reply to a screenshot request.
        screenshot: Vec<u8>,
    }

    impl FakeLite {
        fn new() -> Self {
            let mut mem = [0u8; 256];
            mem[reg::DEVICE_VARIANT as usize] = 2;
            mem[reg::PROTOCOL_VERSION as usize] = 1;
            mem[reg::HARDWARE_REVISION as usize] = 0x40;
            mem[reg::FW_MAJOR as usize] = 2;
            mem[reg::FW_MINOR as usize] = 2;
            mem[reg::BATTERY_MV as usize..reg::BATTERY_MV as usize + 2].copy_from_slice(&4123u16.to_le_bytes());
            FakeLite {
                pending: Vec::new(),
                mem,
                in_flight_after_clear: Vec::new(),
                queued: VecDeque::new(),
                position_at_clear: None,
                next_index: 0,
                pass: 0,
                generated: 0,
                corrupt: Vec::new(),
                wrong_index: None,
                stuck_at: None,
                screenshot: Vec::new(),
            }
        }

        fn points(&self) -> u16 {
            let a = reg::SWEEP_POINTS as usize;
            u16::from_le_bytes([self.mem[a], self.mem[a + 1]]).max(1)
        }

        fn record(index: u16, pass: i32) -> [u8; 32] {
            let mut r = [0u8; 32];
            r[0x00..0x04].copy_from_slice(&1000i32.to_le_bytes());
            r[0x08..0x0C].copy_from_slice(&(index as i32).to_le_bytes());
            r[0x0C..0x10].copy_from_slice(&pass.to_le_bytes());
            r[0x10..0x14].copy_from_slice(&pass.to_le_bytes());
            r[0x14..0x18].copy_from_slice(&(index as i32).to_le_bytes());
            r[0x18..0x1A].copy_from_slice(&index.to_le_bytes());
            r[0x1F] = fifo_checksum(&r);
            r
        }

        fn next_record(&mut self) -> [u8; 32] {
            if let Some(r) = self.queued.pop_front() {
                return r;
            }
            let ordinal = self.generated;
            self.generated += 1;
            let index = self.stuck_at.unwrap_or(self.next_index);
            let mut r = match self.wrong_index {
                Some((at, wrong)) if at == ordinal => Self::record(wrong, self.pass),
                _ => Self::record(index, self.pass),
            };
            if self.corrupt.contains(&ordinal) {
                r[0x1F] ^= 0xFF;
            }
            self.next_index += 1;
            if self.next_index >= self.points() {
                self.next_index = 0;
                self.pass += 1;
            }
            r
        }
    }

    impl FakeDevice for FakeLite {
        fn on_write(&mut self, bytes: &[u8], rx: &mut VecDeque<u8>) {
            self.pending.extend_from_slice(bytes);
            while let Some(&op) = self.pending.first() {
                let len = match op {
                    cmd::NOP | cmd::INDICATE => 1,
                    cmd::READ | cmd::READ2 | cmd::READ4 => 2,
                    cmd::READFIFO | cmd::WRITE => 3,
                    cmd::WRITE2 => 4,
                    cmd::WRITE4 => 6,
                    cmd::WRITE8 => 10,
                    other => panic!("fake LiteVNA: unknown opcode {other:#04x}"),
                };
                if self.pending.len() < len {
                    break;
                }
                let c: Vec<u8> = self.pending.drain(..len).collect();
                match op {
                    cmd::NOP => {}
                    cmd::INDICATE => rx.push_back(proto::INDICATE_ACK),
                    cmd::READ | cmd::READ2 | cmd::READ4 => {
                        let n = match op {
                            cmd::READ => 1,
                            cmd::READ2 => 2,
                            _ => 4,
                        };
                        let a = c[1] as usize;
                        rx.extend(&self.mem[a..a + n]);
                    }
                    cmd::READFIFO => {
                        let n = if c[2] == 0 { self.points() as usize } else { c[2] as usize };
                        for _ in 0..n {
                            let r = self.next_record();
                            rx.extend(r);
                        }
                    }
                    cmd::WRITE if c[1] == reg::VALUES_FIFO => {
                        self.queued = self.in_flight_after_clear.iter().map(|&i| Self::record(i, -1)).collect();
                        if let Some(at) = self.position_at_clear {
                            self.next_index = at;
                        }
                    }
                    cmd::WRITE if c[1] == reg::SCREENSHOT => rx.extend(self.screenshot.iter()),
                    _ => {
                        let a = c[1] as usize;
                        self.mem[a..a + len - 2].copy_from_slice(&c[2..]);
                        // A new sweep definition starts the sweep from its first point.
                        if [reg::SWEEP_START_HZ, reg::SWEEP_STEP_HZ, reg::SWEEP_POINTS].contains(&c[1]) {
                            self.next_index = 0;
                        }
                    }
                }
            }
        }
    }

    fn lite(dev: FakeLite) -> (LiteVna, FakePort) {
        let port = FakePort::new(dev);
        let mut vna = LiteVna::from_port(port.boxed()).unwrap();
        vna.release_on_drop = false;
        (vna, port)
    }

    fn cfg(points: u16) -> SweepConfig {
        SweepConfig::linear(1_000_000, 1_000_000 + (points as u64 - 1) * 1000, points).unwrap()
    }

    /// Does `haystack` contain the bytes `needle`?
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn identity_and_battery_registers() {
        let (mut vna, _) = lite(FakeLite::new());
        vna.indicate().unwrap();
        let info = vna.device_info().unwrap();
        assert_eq!((info.variant, info.protocol_version, info.fw_major, info.fw_minor), (2, 1, 2, 2));
        assert_eq!(vna.battery_millivolts().unwrap(), 4123);
    }

    #[test]
    fn sweep_collects_every_point_once() {
        let (mut vna, port) = lite(FakeLite::new());
        // 600 points: more than two READFIFO chunks of 255.
        let c = cfg(600);
        let r = vna.sweep(&c, false).unwrap();
        assert_eq!(r.freqs_hz, c.frequencies());
        assert_eq!((r.checksum_ok, r.checksum_bad, r.duplicates, r.missing_points()), (600, 0, 0, 0));
        for (i, g) in r.s11.iter().enumerate() {
            assert_eq!((g.re, g.im), (i as f64 / 1000.0, 0.0), "point {i}");
        }
        // The sweep registers were written as one block: mode, start, step, points, values per frequency.
        let tx = port.written();
        assert!(contains(&tx, &proto::encode_write8(reg::SWEEP_START_HZ, c.start_hz)));
        assert!(contains(&tx, &proto::encode_write8(reg::SWEEP_STEP_HZ, c.step_hz)));
        assert!(contains(&tx, &proto::encode_write2(reg::SWEEP_POINTS, 600)));
        // Chunks of at most 255, never more than still missing.
        assert!(contains(&tx, &proto::encode_read_fifo(reg::VALUES_FIFO, 255)));
        assert!(contains(&tx, &proto::encode_read_fifo(reg::VALUES_FIFO, 90)));
    }

    #[test]
    fn fifo_all_asks_for_the_whole_sweep_at_once() {
        let (mut vna, port) = lite(FakeLite::new());
        let r = vna.sweep(&cfg(300), true).unwrap();
        assert_eq!(r.missing_points(), 0);
        assert!(contains(&port.written(), &proto::encode_read_fifo(reg::VALUES_FIFO, 0)));
    }

    #[test]
    fn unchanged_config_is_not_written_again() {
        let (mut vna, port) = lite(FakeLite::new());
        let c = cfg(50);
        vna.sweep(&c, false).unwrap();
        port.clear_written();
        vna.sweep(&c, false).unwrap();
        let tx = port.written();
        assert!(!contains(&tx, &proto::encode_write8(reg::SWEEP_START_HZ, c.start_hz)));
        assert!(
            contains(&tx, &proto::encode_write(reg::VALUES_FIFO, 0)),
            "the FIFO is cleared before every sweep"
        );
        // Another grid is written.
        port.clear_written();
        let other = cfg(51);
        vna.sweep(&other, false).unwrap();
        assert!(contains(&port.written(), &proto::encode_write2(reg::SWEEP_POINTS, 51)));
    }

    #[test]
    fn a_sweep_that_starts_mid_way_still_fills_every_point() {
        // The device sweeps continuously; after the clear it carries on where it was.
        let mut dev = FakeLite::new();
        dev.position_at_clear = Some(37);
        let (mut vna, _) = lite(dev);
        let r = vna.sweep(&cfg(100), false).unwrap();
        assert_eq!((r.missing_points(), r.duplicates), (0, 0));
        // Points 37..99 are from the pass that was running, 0..36 from the next.
        assert_eq!(r.s11[37].im, 0.0);
        assert_eq!(r.s11[36].im, 1.0 / 1000.0);
    }

    #[test]
    fn records_in_flight_after_the_clear_fill_their_points_and_later_ones_count_as_duplicates() {
        // docs/01 §7: clearing the FIFO leaves up to 32 queued records. They
        // arrive first and fill their points like any other record; the sweep
        // is complete as soon as every point has one.
        let mut dev = FakeLite::new();
        dev.in_flight_after_clear = (68..100).collect();
        let (mut vna, _) = lite(dev);
        let r = vna.sweep(&cfg(100), false).unwrap();
        assert_eq!(r.missing_points(), 0);
        // The tail came from the in-flight records (pass −1) ...
        assert_eq!(r.s11[99].im, -1.0 / 1000.0);
        // ... and the head from the sweep after the clear.
        assert_eq!(r.s11[0].im, 0.0);
        // 32 + 68 records were enough; nothing was read twice.
        assert_eq!((r.checksum_ok, r.duplicates), (100, 0));
    }

    #[test]
    fn a_record_with_a_bad_checksum_is_skipped_and_the_point_comes_again() {
        let mut dev = FakeLite::new();
        dev.corrupt = vec![5];
        let (mut vna, _) = lite(dev);
        let r = vna.sweep(&cfg(20), false).unwrap();
        assert_eq!((r.checksum_bad, r.missing_points()), (1, 0));
        // Point 5 was taken from the next pass of the sweep, not from the corrupt record.
        assert_eq!(r.s11[5], Complex64::new(5.0 / 1000.0, 1.0 / 1000.0));
        // The device sweeps on from point 0, so points 0..4 arrive a second
        // time before point 5 does: counted as duplicates, and the newer
        // record replaces the older one. Points after 5 are from the first pass.
        assert_eq!(r.duplicates, 5);
        assert_eq!(r.s11[4].im, 1.0 / 1000.0);
        assert_eq!(r.s11[6].im, 0.0);
        assert_eq!(r.checksum_ok, 19 + 6);
    }

    #[test]
    fn an_index_beyond_the_sweep_is_a_protocol_error() {
        let mut dev = FakeLite::new();
        dev.wrong_index = Some((3, 500));
        let (mut vna, _) = lite(dev);
        let err = vna.sweep(&cfg(20), false).unwrap_err();
        assert!(err.to_string().contains("freq_index 500 out of range"), "{err}");
    }

    #[test]
    fn a_sweep_that_never_completes_gives_up() {
        let mut dev = FakeLite::new();
        dev.stuck_at = Some(0);
        let (mut vna, _) = lite(dev);
        let err = vna.sweep(&cfg(5), false).unwrap_err();
        assert!(err.to_string().contains("sweep did not complete: 1/5"), "{err}");
    }

    #[test]
    fn after_a_failed_sweep_the_next_one_reconfigures() {
        let mut dev = FakeLite::new();
        dev.wrong_index = Some((3, 500));
        let (mut vna, port) = lite(dev);
        let c = cfg(20);
        assert!(vna.sweep(&c, false).is_err());
        port.clear_written();
        // Same config, but the protocol is reset and the registers are written again.
        let r = vna.sweep(&c, false).unwrap();
        assert_eq!(r.missing_points(), 0);
        let tx = port.written();
        assert!(tx.starts_with(&proto::RESET_SEQUENCE));
        assert!(contains(&tx, &proto::encode_write8(reg::SWEEP_START_HZ, c.start_hz)));
    }

    #[test]
    fn progress_is_reported_per_chunk_and_can_cancel() {
        use std::ops::ControlFlow;
        let (mut vna, port) = lite(FakeLite::new());
        let c = cfg(600);
        let mut seen = Vec::new();
        let r = vna
            .sweep_with(&c, false, &mut |done, total| {
                seen.push((done, total));
                ControlFlow::Continue(())
            })
            .unwrap();
        assert_eq!(r.missing_points(), 0);
        assert_eq!(seen, [(255, 600), (510, 600), (600, 600)]);

        // Stop after the first chunk: no further READFIFO goes out.
        port.clear_written();
        let err = vna.sweep_with(&c, false, &mut |_, _| ControlFlow::Break(())).unwrap_err();
        assert!(matches!(err, Error::Cancelled), "{err:?}");
        assert!(!err.is_link_failure());
        let fifo_reads = port
            .written()
            .windows(2)
            .filter(|w| w == &[cmd::READFIFO, reg::VALUES_FIFO])
            .count();
        assert_eq!(fifo_reads, 1);
        // Breaking on the last block is not a cancel: the sweep is complete.
        let r = vna.sweep_with(&cfg(100), false, &mut |_, _| ControlFlow::Break(())).unwrap();
        assert_eq!(r.missing_points(), 0);
    }

    #[test]
    fn a_pulled_cable_is_a_link_failure() {
        let (mut vna, port) = lite(FakeLite::new());
        port.unplug();
        let err = vna.sweep(&cfg(20), false).unwrap_err();
        assert!(err.is_link_failure(), "{err:?}");
    }

    #[test]
    fn a_silent_device_times_out() {
        let mut dev = FakeLite::new();
        // No records at all: the device answers nothing to READFIFO.
        dev.mem[reg::SWEEP_POINTS as usize] = 0;
        struct Mute;
        impl FakeDevice for Mute {
            fn on_write(&mut self, _: &[u8], _: &mut VecDeque<u8>) {}
        }
        let port = FakePort::new(Mute);
        let mut vna = LiteVna::from_port(port.boxed()).unwrap();
        vna.release_on_drop = false;
        let err = vna.read_u8(reg::DEVICE_VARIANT).unwrap_err();
        assert!(matches!(err, Error::Timeout { expected: 1, got: 0 }), "{err:?}");
        assert!(!err.is_link_failure());
    }

    #[test]
    fn screenshot_reads_header_and_pixels() {
        let mut dev = FakeLite::new();
        dev.screenshot.extend(4u16.to_le_bytes());
        dev.screenshot.extend(2u16.to_le_bytes());
        dev.screenshot.push(16);
        // 8 pixels RGB565 big-endian: pure red 0xF800.
        dev.screenshot.extend([0xF8, 0x00].repeat(8));
        let (mut vna, _) = lite(dev);
        let shot = vna.screenshot().unwrap();
        assert_eq!((shot.width, shot.height, shot.bits_per_pixel, shot.data.len()), (4, 2, 16, 16));
        let rgb = shot.to_rgb888();
        assert_eq!(&rgb[..3], &[255, 0, 0]);
        // The other byte order reads the same bytes as a dark green-blue.
        assert_ne!(&shot.to_rgb888_with(false)[..3], &[255, 0, 0]);
    }

    #[test]
    fn screenshot_refuses_an_implausible_header() {
        for header in [
            [0xFF, 0xFF, 0xFF, 0xFF, 32u8], // 65535 × 65535 × 4 bytes
            [0x20, 0x03, 0xE0, 0x01, 24],   // 800 × 480 but 24 bpp
            [0x00, 0x00, 0xE0, 0x01, 16],   // zero width
        ] {
            let mut dev = FakeLite::new();
            dev.screenshot.extend(header);
            let (mut vna, _) = lite(dev);
            let err = vna.screenshot().unwrap_err();
            assert!(err.to_string().contains("implausible screenshot header"), "{header:?}: {err}");
        }
    }

    #[test]
    fn sweep_config_rounds_to_the_device_grid() {
        let c = SweepConfig::linear(1_000_000, 30_000_000, 201).unwrap();
        assert_eq!((c.start_hz, c.step_hz, c.points), (1_000_000, 145_000, 201));
        assert_eq!(c.stop_hz(), 30_000_000);
        assert!(SweepConfig::linear(1_000_000, 30_000_000, 1).is_err());
        assert!(SweepConfig::linear(30_000_000, 1_000_000, 201).is_err());
        let back = SweepConfig::from_frequencies(&c.frequencies()).unwrap();
        assert_eq!((back.start_hz, back.step_hz, back.points), (c.start_hz, c.step_hz, c.points));
        assert!(SweepConfig::from_frequencies(&[1, 2, 4]).is_err());
    }

    #[test]
    fn port_extension_moves_the_reference_plane() {
        // A short behind 100 ps of line, and a through of the same line.
        let tau = 100e-12;
        let freqs: Vec<u64> = (1..=50).map(|i| i * 100_000_000).collect();
        let w = |f: u64| 2.0 * std::f64::consts::PI * f as f64 * tau;
        let mut sweep = SweepResult {
            s11: freqs.iter().map(|&f| -Complex64::from_polar(1.0, -2.0 * w(f))).collect(),
            s21: freqs.iter().map(|&f| Complex64::from_polar(0.9, -w(f))).collect(),
            freqs_hz: freqs,
            points: Vec::new(),
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        };
        let before = sweep.clone();
        sweep.extend_port(tau);
        for (a, b) in sweep.s11.iter().zip(&sweep.s21) {
            assert!((a - Complex64::new(-1.0, 0.0)).norm() < 1e-9, "S11 {a}");
            assert!((b - Complex64::new(0.9, 0.0)).norm() < 1e-9, "S21 {b}");
        }
        // The opposite extension undoes it; zero does nothing.
        sweep.extend_port(-tau);
        sweep.extend_port(0.0);
        assert!(sweep.s11.iter().zip(&before.s11).all(|(a, b)| (a - b).norm() < 1e-9));
    }

    #[test]
    fn trimmed_average_leaves_out_the_glitch() {
        let freqs: Vec<u64> = vec![1_000_000, 2_000_000, 3_000_000];
        let sweep = |v: f64| SweepResult {
            s11: vec![Complex64::new(v, 0.0); 3],
            s21: vec![Complex64::new(0.0, v); 3],
            freqs_hz: freqs.clone(),
            points: Vec::new(),
            checksum_ok: 1,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::from_millis(10),
        };
        // Four quiet sweeps and one glitch of 1.0 at every point.
        let sweeps = vec![sweep(0.10), sweep(0.11), sweep(1.0), sweep(0.09), sweep(0.10)];
        let plain = SweepResult::average(&sweeps).unwrap();
        assert!((plain.s11[1].re - 0.28).abs() < 1e-12);
        let trimmed = SweepResult::average_trimmed(&sweeps, 1).unwrap();
        assert!((trimmed.s11[1].re - 0.10).abs() < 1e-12, "{}", trimmed.s11[1].re);
        assert!((trimmed.s21[1].im - 0.10).abs() < 1e-12);
        // Counts still add up over all five sweeps; dropping more than there
        // are sweeps keeps one; a lone sweep is returned as it is.
        assert_eq!(trimmed.checksum_ok, 5);
        assert_eq!(trimmed.elapsed, Duration::from_millis(50));
        let one_left = SweepResult::average_trimmed(&sweeps, 9).unwrap();
        assert!(one_left.s11[0].re.is_finite());
        let alone = SweepResult::average_trimmed(&sweeps[2..3], 2).unwrap();
        assert_eq!(alone.s11[0].re, 1.0);
        assert!(SweepResult::average_trimmed(&sweeps[..0], 1).is_none());
    }

    #[test]
    fn log_segment_edges_share_one_ratio_and_touch() {
        let edges = log_segment_edges(50_000, 6_300_000_000, 4).unwrap();
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0].0, 50_000);
        assert_eq!(edges[3].1, 6_300_000_000);
        for w in edges.windows(2) {
            assert_eq!(w[0].1, w[1].0, "segments must touch");
        }
        let ratios: Vec<f64> = edges.iter().map(|(a, b)| *b as f64 / *a as f64).collect();
        for r in &ratios {
            assert!((r - ratios[0]).abs() / ratios[0] < 1e-6, "{ratios:?}");
        }
        assert!((ratios[0] - (6_300_000_000.0_f64 / 50_000.0).powf(0.25)).abs() < 1e-3);
        assert_eq!(log_segment_edges(1_000_000, 30_000_000, 1).unwrap(), vec![(1_000_000, 30_000_000)]);
        assert!(log_segment_edges(0, 1_000, 2).is_err());
        assert!(log_segment_edges(2_000, 1_000, 2).is_err());
        assert!(log_segment_edges(1_000, 1_000, 1).is_err());
        assert!(log_segment_edges(1_000, 1_001, 5).is_err());
    }

    #[test]
    fn segments_are_parsed_ordered_and_checked() {
        let hf = SweepConfig::parse_segment("100k:50M:1001").unwrap();
        assert_eq!((hf.start_hz, hf.points), (100_000, 1001));
        let wide = SweepConfig::parse_segment(" 50M : 6.3G : 501 ").unwrap();
        for bad in ["1M:30M", "1M:30M:x", "1M:30M:201:9", "30M:1M:201", "abc:30M:201"] {
            assert!(SweepConfig::parse_segment(bad).is_err(), "{bad}");
        }
        // Given in the wrong order: sorted by start, and what travels along stays with its segment.
        let plan = plan_segments(vec![(wide, "wide"), (hf, "hf")]).unwrap();
        assert_eq!(plan.iter().map(|(_, n)| *n).collect::<Vec<_>>(), ["hf", "wide"]);
        // Touching is fine, overlapping is not.
        let overlapping = SweepConfig::parse_segment("40M:1G:101").unwrap();
        let err = plan_segments(vec![(hf, ()), (overlapping, ())]).unwrap_err().to_string();
        assert!(err.contains("segments overlap: 100.000 kHz"), "{err}");
        // TDR takes the widest segment; a plan that does not fit the data means the whole sweep.
        assert_eq!(SweepResult::widest_segment(&[hf, wide], 1502), 1001..1502);
        assert_eq!(SweepResult::widest_segment(&[hf], 1001), 0..1001);
        assert_eq!(SweepResult::widest_segment(&[hf, wide], 300), 0..300);
        assert_eq!(SweepResult::widest_segment(&[], 300), 0..300);
    }

    #[test]
    fn concat_and_segment_ranges() {
        let (mut vna, _) = lite(FakeLite::new());
        let (a, b) = (cfg(10), SweepConfig::linear(2_000_000, 2_004_000, 5).unwrap());
        let parts = [vna.sweep(&a, false).unwrap(), vna.sweep(&b, false).unwrap()];
        let all = SweepResult::concat(&parts);
        assert_eq!(all.freqs_hz.len(), 15);
        assert_eq!(all.checksum_ok, 15);
        let ranges = SweepResult::segment_ranges(&[a, b]);
        assert_eq!(ranges, vec![0..10, 10..15]);
        let second = all.slice(ranges[1].clone());
        assert_eq!(second.freqs_hz, b.frequencies());
        assert_eq!(second.missing_points(), 0);
    }

    #[test]
    fn zero_point_config_does_not_underflow() {
        let cfg = SweepConfig {
            start_hz: 1_000_000,
            step_hz: 1000,
            points: 0,
            values_per_freq: 1,
            calibrated: false,
        };
        assert_eq!(cfg.stop_hz(), 1_000_000);
        assert!(cfg.frequencies().is_empty());
    }

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
