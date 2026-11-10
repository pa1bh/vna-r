//! [`NanoVnaShell`]: the text command line ("shell") of the NanoVNA family
//! (NanoVNA-H, NanoVNA-F, ...), as opposed to the binary register protocol of
//! the LiteVNA / NanoVNA V2 in [`crate::device`].
//!
//! The device presents a prompt `ch> ` over USB CDC; every command is a line,
//! every reply ends with the next prompt. Measurements come from `scan_bin`,
//! which answers with a small binary block instead of text:
//!
//! ```text
//! <echo of the command line>\r\n
//! u16 mask (0x80 | requested mask), u16 points
//! per point: [u32 freq_hz] [f32 s11_re, f32 s11_im] [f32 s21_re, f32 s21_im]
//! ch>
//! ```
//!
//! All integers are little-endian. The mask bits are `1` frequency, `2` S11,
//! `4` S21 and `8` *uncalibrated*: data before the device's own calibration,
//! which is what the host-side calibration in [`crate::calibration`] wants.
//! One scan is limited to [`MAX_SCAN_POINTS`] points; longer sweeps are done in
//! chunks on the same grid. Measured on a NanoVNA-F 4.3 with firmware 1.0.4:
//! about 230 points/s for S11 alone and 115 points/s for S11 + S21.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use num_complex::Complex64;
use serialport::{ClearBuffer, SerialPort};

use crate::device::{Screenshot, SweepConfig, SweepResult};
use crate::error::{Error, Result};

/// The shell prompt that terminates every reply.
pub const PROMPT: &[u8] = b"ch> ";
/// Largest `points` the firmware accepts in one `scan` ("sweep points exceeds range 301").
pub const MAX_SCAN_POINTS: u16 = 301;

/// `scan` / `scan_bin` output mask bits.
pub mod mask {
    pub const FREQ: u16 = 1;
    pub const S11: u16 = 2;
    pub const S21: u16 = 4;
    /// Return data before the device's own calibration.
    pub const UNCALIBRATED: u16 = 8;
    /// Set by the firmware in the header of a `scan_bin` reply.
    pub const BINARY_REPLY: u16 = 0x80;
}

// Short under `cargo test`: the fake port answers at once, and a test of a
// device that stays silent should not take seconds.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(if cfg!(test) { 150 } else { 3000 });
/// Per point: the device needs ~9 ms for S11 + S21 at the default bandwidth;
/// narrower bandwidths are slower, so be generous.
const PER_POINT_BUDGET: Duration = Duration::from_millis(60);
const MIN_SCAN_TIMEOUT: Duration = Duration::from_secs(5);
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);

/// What `info` reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellInfo {
    /// `Model:` line, e.g. `NanoVNA-F 4.3`.
    pub model: String,
    /// `Version:` line, e.g. `Firmware 1.0.4 by BH5HNU`.
    pub firmware: String,
    /// `Build time:` line.
    pub build: String,
    /// The complete reply, for devices with other line layouts.
    pub raw: String,
}

/// Parse the reply of `info` (`Key: value` lines).
pub fn parse_info(text: &str) -> ShellInfo {
    let mut info = ShellInfo {
        raw: text.trim().to_string(),
        ..Default::default()
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match key.trim().to_ascii_lowercase().as_str() {
            "model" => info.model = value,
            "version" => info.firmware = value,
            "build time" => info.build = value,
            _ => {}
        }
    }
    info
}

/// Screen size of the known models (`capture` sends no header).
pub fn screen_size_for_model(model: &str) -> Option<(u16, u16)> {
    let m = model.to_ascii_lowercase();
    if m.contains("nanovna-f") {
        Some((800, 480))
    } else if m.contains("h4") {
        Some((480, 320))
    } else if m.contains("nanovna") {
        Some((320, 240))
    } else {
        None
    }
}

/// Split a sweep into chunks of at most `max_points` on the same grid.
pub fn scan_chunks(cfg: &SweepConfig, max_points: u16) -> Vec<SweepConfig> {
    let max = max_points.max(1) as u32;
    let n = cfg.points as u32;
    let mut out = Vec::new();
    let mut a = 0u32;
    while a < n {
        let len = (n - a).min(max);
        out.push(SweepConfig {
            start_hz: cfg.start_hz + a as u64 * cfg.step_hz,
            step_hz: cfg.step_hz,
            points: len as u16,
            values_per_freq: cfg.values_per_freq,
            calibrated: cfg.calibrated,
        });
        a += len;
    }
    out
}

/// One point of a decoded `scan_bin` block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanPoint {
    pub freq_hz: Option<u32>,
    pub s11: Option<Complex64>,
    pub s21: Option<Complex64>,
}

/// Bytes per point for a mask.
pub fn point_size(mask: u16) -> usize {
    let mut n = 0;
    if mask & mask::FREQ != 0 {
        n += 4;
    }
    if mask & mask::S11 != 0 {
        n += 8;
    }
    if mask & mask::S21 != 0 {
        n += 8;
    }
    n
}

/// Decode the 4-byte header of a `scan_bin` reply: `(mask, points)`.
pub fn parse_scan_header(header: &[u8; 4]) -> (u16, u16) {
    (
        u16::from_le_bytes([header[0], header[1]]),
        u16::from_le_bytes([header[2], header[3]]),
    )
}

/// Decode the point block of a `scan_bin` reply for `mask` (as sent in the header).
pub fn parse_scan_points(mask: u16, points: u16, data: &[u8]) -> Result<Vec<ScanPoint>> {
    let per = point_size(mask);
    let need = per * points as usize;
    if data.len() != need {
        return Err(Error::protocol(format!(
            "scan_bin: expected {need} bytes for {points} points with mask {mask:#x}, got {}",
            data.len()
        )));
    }
    let f32_at = |b: &[u8]| f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let mut out = Vec::with_capacity(points as usize);
    for rec in data.chunks_exact(per.max(1)).take(points as usize) {
        let mut p = 0;
        let mut pt = ScanPoint {
            freq_hz: None,
            s11: None,
            s21: None,
        };
        if mask & mask::FREQ != 0 {
            pt.freq_hz = Some(u32::from_le_bytes([rec[p], rec[p + 1], rec[p + 2], rec[p + 3]]));
            p += 4;
        }
        if mask & mask::S11 != 0 {
            pt.s11 = Some(Complex64::new(f32_at(&rec[p..]) as f64, f32_at(&rec[p + 4..]) as f64));
            p += 8;
        }
        if mask & mask::S21 != 0 {
            pt.s21 = Some(Complex64::new(f32_at(&rec[p..]) as f64, f32_at(&rec[p + 4..]) as f64));
        }
        out.push(pt);
    }
    Ok(out)
}

/// A connected NanoVNA speaking the text shell.
pub struct NanoVnaShell {
    port: Box<dyn SerialPort>,
    /// Print every command and reply to stderr.
    pub trace: bool,
    /// Put the device's own sweep back (and `resume` it) when this handle is
    /// dropped, if a scan changed it. Default `true`.
    pub restore_on_drop: bool,
    /// The on-screen sweep as it was before our first scan: `(start, stop, points)`.
    saved_sweep: Option<(u64, u64, u16)>,
    /// Whether the device applied its own calibration on screen before our
    /// first raw scan (`cal` reports `cal'ed`); a raw scan can switch that off.
    saved_cal_apply: Option<bool>,
    info: Option<ShellInfo>,
}

impl NanoVnaShell {
    /// Open the port and wait for a prompt. Fails when nothing answers with `ch> `.
    pub fn open(path: &str) -> Result<Self> {
        let port = serialport::new(path, 115_200).timeout(Duration::from_millis(100)).open()?;
        Self::from_port(port)
    }

    /// Drive a device over a port that is already open (another transport,
    /// or the fake port of the tests). Waits for a prompt like [`NanoVnaShell::open`].
    pub fn from_port(port: Box<dyn SerialPort>) -> Result<Self> {
        // `restore_on_drop` stays off until the device has answered: dropping
        // a handle whose `sync` failed must not send `resume` to whatever is
        // on the port and wait another command timeout for its prompt.
        let mut dev = NanoVnaShell {
            port,
            trace: false,
            restore_on_drop: false,
            saved_sweep: None,
            saved_cal_apply: None,
            info: None,
        };
        dev.sync()?;
        dev.restore_on_drop = true;
        Ok(dev)
    }

    /// Send a bare newline and wait for the prompt, discarding anything before it.
    pub fn sync(&mut self) -> Result<()> {
        self.port.clear(ClearBuffer::Input)?;
        self.port.write_all(b"\r")?;
        self.port.flush()?;
        self.read_until_prompt(COMMAND_TIMEOUT).map_err(|e| match e {
            Error::Timeout { .. } => Error::protocol("no NanoVNA shell prompt (`ch> `) on this port"),
            e => e,
        })?;
        Ok(())
    }

    fn read_until_prompt(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    out.extend_from_slice(&buf[..n]);
                    if out.ends_with(PROMPT) {
                        return Ok(out);
                    }
                }
                Ok(_) => {}
                Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout {
                    expected: PROMPT.len(),
                    got: out.len(),
                });
            }
        }
    }

    fn read_exact_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut filled = 0;
        while filled < buf.len() {
            match self.port.read(&mut buf[filled..]) {
                Ok(n) => filled += n,
                Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.into()),
            }
            if filled < buf.len() && Instant::now() >= deadline {
                return Err(Error::Timeout {
                    expected: buf.len(),
                    got: filled,
                });
            }
        }
        Ok(())
    }

    fn send_line(&mut self, cmd: &str) -> Result<()> {
        if self.trace {
            eprintln!("tx: {cmd}");
        }
        self.port.clear(ClearBuffer::Input)?;
        self.port.write_all(cmd.as_bytes())?;
        self.port.write_all(b"\r")?;
        self.port.flush()?;
        Ok(())
    }

    /// Run a text command and return its reply without the echo and the prompt.
    pub fn command(&mut self, cmd: &str, timeout: Duration) -> Result<String> {
        self.send_line(cmd)?;
        let raw = self.read_until_prompt(timeout)?;
        let text = String::from_utf8_lossy(&raw);
        let body = text.strip_suffix("ch> ").unwrap_or(&text);
        // The shell echoes the command line first.
        let body = match body.split_once("\r\n") {
            Some((first, rest)) if first.trim() == cmd.trim() => rest,
            _ => body,
        };
        let body = body.trim_end_matches(['\r', '\n']).to_string();
        if self.trace {
            eprintln!("rx: {body:?}");
        }
        Ok(body)
    }

    /// `info`: model and firmware. Cached after the first call.
    pub fn info(&mut self) -> Result<ShellInfo> {
        if let Some(i) = &self.info {
            return Ok(i.clone());
        }
        let text = self.command("info", COMMAND_TIMEOUT)?;
        let info = parse_info(&text);
        if info.model.is_empty() && info.firmware.is_empty() && text.trim().is_empty() {
            return Err(Error::protocol("`info` gave an empty reply"));
        }
        self.info = Some(info.clone());
        Ok(info)
    }

    /// `vbat`, e.g. `4209 mV`.
    pub fn battery_millivolts(&mut self) -> Result<u16> {
        let text = self.command("vbat", COMMAND_TIMEOUT)?;
        text.split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| Error::protocol(format!("unexpected vbat reply {text:?}")))
    }

    /// The device's own sweep: `(start_hz, stop_hz, points)`.
    pub fn sweep_settings(&mut self) -> Result<(u64, u64, u16)> {
        let text = self.command("sweep", COMMAND_TIMEOUT)?;
        let mut it = text.split_whitespace();
        let parse = |s: Option<&str>| s.and_then(|v| v.parse::<u64>().ok());
        match (parse(it.next()), parse(it.next()), parse(it.next())) {
            (Some(a), Some(b), Some(n)) if n <= u16::MAX as u64 => Ok((a, b, n as u16)),
            _ => Err(Error::protocol(format!("unexpected sweep reply {text:?}"))),
        }
    }

    pub fn set_sweep(&mut self, start_hz: u64, stop_hz: u64, points: u16) -> Result<()> {
        self.command(&format!("sweep {start_hz} {stop_hz} {points}"), COMMAND_TIMEOUT)?;
        Ok(())
    }

    /// Pause / resume the device's own sweeping.
    pub fn pause(&mut self) -> Result<()> {
        self.command("pause", COMMAND_TIMEOUT).map(|_| ())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.command("resume", COMMAND_TIMEOUT).map(|_| ())
    }

    /// `cal`: the calibration status flags, e.g. `load isoln Es Er Et cal'ed`.
    /// `cal'ed` means the device applies its calibration to its own sweep.
    pub fn cal_status(&mut self) -> Result<String> {
        self.command("cal", COMMAND_TIMEOUT)
    }

    /// `cal on` / `cal off`: apply the device's own calibration on screen.
    pub fn set_cal_apply(&mut self, on: bool) -> Result<()> {
        self.command(if on { "cal on" } else { "cal off" }, COMMAND_TIMEOUT).map(|_| ())
    }

    /// `bandwidth n`: 0 = 1 kHz (fastest), 1 = 500 Hz, ... Persists in the device.
    pub fn set_bandwidth(&mut self, index: u16) -> Result<()> {
        self.command(&format!("bandwidth {index}"), COMMAND_TIMEOUT).map(|_| ())
    }

    /// One `scan_bin` of at most [`MAX_SCAN_POINTS`] points on `cfg`'s grid.
    fn scan_bin(&mut self, cfg: &SweepConfig) -> Result<Vec<ScanPoint>> {
        if cfg.points > MAX_SCAN_POINTS {
            return Err(Error::InvalidArgument(format!("scan_bin takes at most {MAX_SCAN_POINTS} points")));
        }
        let mut m = mask::FREQ | mask::S11 | mask::S21;
        if !cfg.calibrated {
            m |= mask::UNCALIBRATED;
        }
        let cmd = format!("scan_bin {} {} {} {}", cfg.start_hz, cfg.stop_hz(), cfg.points, m);
        self.send_line(&cmd)?;
        let timeout = (PER_POINT_BUDGET * cfg.points as u32).max(MIN_SCAN_TIMEOUT);
        // Echo line.
        let mut echo = vec![0u8; cmd.len() + 2];
        self.read_exact_timeout(&mut echo, COMMAND_TIMEOUT)?;
        if &echo[..cmd.len()] != cmd.as_bytes() {
            return Err(Error::protocol(format!(
                "scan_bin: unexpected echo {:?}",
                String::from_utf8_lossy(&echo)
            )));
        }
        let mut header = [0u8; 4];
        self.read_exact_timeout(&mut header, timeout)?;
        let (got_mask, got_points) = parse_scan_header(&header);
        if got_mask & 0x0F != m & 0x0F || got_points != cfg.points {
            // Probably a text error message such as "sweep points exceeds range".
            let mut rest = header.to_vec();
            rest.extend(self.read_until_prompt(COMMAND_TIMEOUT).unwrap_or_default());
            return Err(Error::protocol(format!(
                "scan_bin: device answered {:?}",
                String::from_utf8_lossy(&rest).trim()
            )));
        }
        let mut data = vec![0u8; point_size(got_mask) * got_points as usize];
        self.read_exact_timeout(&mut data, timeout)?;
        self.read_until_prompt(COMMAND_TIMEOUT)?;
        if self.trace {
            eprintln!("rx: scan_bin {} points, mask {got_mask:#x}", got_points);
        }
        parse_scan_points(got_mask, got_points, &data)
    }

    /// Measure `cfg` (any number of points; chunked per [`MAX_SCAN_POINTS`]).
    /// Raw data unless `cfg.calibrated`, in which case the device applies its
    /// own calibration. The device's on-screen sweep is remembered on the
    /// first call and restored when the handle is dropped.
    pub fn sweep(&mut self, cfg: &SweepConfig) -> Result<SweepResult> {
        if self.saved_sweep.is_none() {
            self.saved_sweep = Some(self.sweep_settings()?);
            // After raw scans the NanoVNA-F 1.0.4 showed an uncalibrated trace on screen
            // (its `cal'ed` flag stays set, so it is probably the changed grid); putting the
            // sweep back and re-asserting `cal on` on release costs nothing.
            self.saved_cal_apply = self.cal_status().ok().map(|s| s.contains("cal'ed"));
        }
        let started = Instant::now();
        let freqs = cfg.frequencies();
        let mut s11 = Vec::with_capacity(freqs.len());
        let mut s21 = Vec::with_capacity(freqs.len());
        for chunk in scan_chunks(cfg, MAX_SCAN_POINTS) {
            let pts = self.scan_bin(&chunk)?;
            for (i, p) in pts.iter().enumerate() {
                let expected = chunk.start_hz + i as u64 * chunk.step_hz;
                if let Some(f) = p.freq_hz {
                    if (f as i64 - expected as i64).abs() > 1 {
                        return Err(Error::protocol(format!("scan_bin: point {i} at {f} Hz, expected {expected} Hz")));
                    }
                }
                s11.push(p.s11.unwrap_or_default());
                s21.push(p.s21.unwrap_or_default());
            }
        }
        Ok(SweepResult {
            freqs_hz: freqs,
            s11,
            s21,
            points: Vec::new(),
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: started.elapsed(),
        })
    }

    /// `capture`: the LCD as RGB565. The firmware sends bare pixels, so the
    /// size comes from the model (800×480 on the NanoVNA-F) or, for an
    /// unknown model, from the number of bytes before the prompt.
    pub fn screenshot(&mut self) -> Result<Screenshot> {
        let model = self.info().map(|i| i.model).unwrap_or_default();
        self.send_line("capture")?;
        let mut echo = [0u8; 9]; // "capture\r\n"
        self.read_exact_timeout(&mut echo, COMMAND_TIMEOUT)?;
        let (width, height, data) = match screen_size_for_model(&model) {
            Some((w, h)) => {
                let mut data = vec![0u8; w as usize * h as usize * 2];
                self.read_exact_timeout(&mut data, CAPTURE_TIMEOUT)?;
                self.read_until_prompt(COMMAND_TIMEOUT)?;
                (w, h, data)
            }
            None => {
                let mut all = self.read_until_prompt(CAPTURE_TIMEOUT)?;
                all.truncate(all.len() - PROMPT.len());
                let (w, h) = match all.len() {
                    153_600 => (320, 240),
                    307_200 => (480, 320),
                    768_000 => (800, 480),
                    n => return Err(Error::protocol(format!("capture: {n} bytes, unknown screen size"))),
                };
                (w, h, all)
            }
        };
        Ok(Screenshot {
            width,
            height,
            bits_per_pixel: 16,
            big_endian: false,
            data,
        })
    }

    /// Put the device's own sweep (and its on-screen calibration) back if a
    /// scan changed them, and resume sweeping.
    pub fn restore(&mut self) -> Result<()> {
        if let Some((a, b, n)) = self.saved_sweep.take() {
            self.set_sweep(a, b, n)?;
        }
        if self.saved_cal_apply.take() == Some(true) {
            self.set_cal_apply(true)?;
        }
        self.resume()
    }
}

impl Drop for NanoVnaShell {
    fn drop(&mut self) {
        if self.restore_on_drop {
            let _ = self.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakeport::{FakeDevice, FakePort};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// A NanoVNA-F as the driver sees it: a line shell that echoes each
    /// command, answers and ends with the prompt `ch> `.
    ///
    /// `scan_bin` points carry their own frequency: S11 = (f/1e9, 0.5),
    /// S21 = (0.25, −f/1e9).
    struct FakeShell {
        line: Vec<u8>,
        /// Every command line received, for the test to inspect.
        commands: Arc<Mutex<Vec<String>>>,
        model: &'static str,
        sweep: (u64, u64, u16),
        cal_applied: bool,
        /// Answers nothing at all (not a shell, or a dead device).
        silent: bool,
        /// Text the firmware answers to `scan_bin` instead of data.
        scan_error: Option<&'static str>,
        /// Echo something else than the command.
        garbled_echo: bool,
        /// Size of the `capture` reply in bytes.
        capture_bytes: usize,
    }

    impl FakeShell {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let commands = Arc::new(Mutex::new(Vec::new()));
            let dev = FakeShell {
                line: Vec::new(),
                commands: commands.clone(),
                model: "NanoVNA-F 4.3",
                sweep: (1_000_000, 30_000_000, 101),
                cal_applied: true,
                silent: false,
                scan_error: None,
                garbled_echo: false,
                capture_bytes: 800 * 480 * 2,
            };
            (dev, commands)
        }

        fn reply(&mut self, cmd: &str, rx: &mut VecDeque<u8>) {
            let words: Vec<&str> = cmd.split_whitespace().collect();
            let mut echo = cmd.as_bytes().to_vec();
            if self.garbled_echo {
                echo.fill(b'x');
            }
            rx.extend(echo);
            rx.extend(b"\r\n");
            match words.as_slice() {
                ["info"] => rx.extend(
                    format!(
                        "Model:        {}\r\nVersion:      Firmware 1.0.4 by BH5HNU\r\nBuild time:   Jun  3 2022\r\n",
                        self.model
                    )
                    .bytes(),
                ),
                ["vbat"] => rx.extend(b"4209 mV\r\n"),
                ["sweep"] => rx.extend(format!("{} {} {}\r\n", self.sweep.0, self.sweep.1, self.sweep.2).bytes()),
                ["sweep", a, b, n] => self.sweep = (a.parse().unwrap(), b.parse().unwrap(), n.parse().unwrap()),
                ["cal"] => rx.extend(if self.cal_applied {
                    &b"load isoln Es Er Et cal'ed\r\n"[..]
                } else {
                    &b"load isoln Es Er Et\r\n"[..]
                }),
                ["cal", "on"] => self.cal_applied = true,
                ["scan_bin", a, b, n, m] => {
                    if let Some(text) = self.scan_error {
                        rx.extend(text.bytes());
                        rx.extend(b"\r\n");
                    } else {
                        let (a, b, n, m): (u64, u64, u16, u16) =
                            (a.parse().unwrap(), b.parse().unwrap(), n.parse().unwrap(), m.parse().unwrap());
                        rx.extend((m | mask::BINARY_REPLY).to_le_bytes());
                        rx.extend(n.to_le_bytes());
                        for i in 0..n as u64 {
                            let f = if n > 1 { a + i * (b - a) / (n as u64 - 1) } else { a };
                            let g = f as f32 / 1e9;
                            rx.extend((f as u32).to_le_bytes());
                            for v in [g, 0.5, 0.25, -g] {
                                rx.extend(v.to_le_bytes());
                            }
                        }
                    }
                }
                ["capture"] => rx.extend(std::iter::repeat_n(0x1Fu8, self.capture_bytes)),
                // resume, pause, bandwidth, ...: no output.
                _ => {}
            }
            rx.extend(PROMPT);
        }
    }

    impl FakeDevice for FakeShell {
        fn on_write(&mut self, bytes: &[u8], rx: &mut VecDeque<u8>) {
            for &b in bytes {
                if b != b'\r' {
                    self.line.push(b);
                    continue;
                }
                let cmd = String::from_utf8(std::mem::take(&mut self.line)).unwrap();
                if self.silent {
                    continue;
                }
                if cmd.is_empty() {
                    // A bare return: just a new prompt.
                    rx.extend(b"\r\n");
                    rx.extend(PROMPT);
                    continue;
                }
                self.commands.lock().unwrap().push(cmd.clone());
                self.reply(&cmd, rx);
            }
        }
    }

    fn shell(dev: FakeShell) -> (NanoVnaShell, FakePort) {
        let port = FakePort::new(dev);
        (NanoVnaShell::from_port(port.boxed()).unwrap(), port)
    }

    fn linear(start: u64, stop: u64, points: u16) -> SweepConfig {
        SweepConfig::linear(start, stop, points).unwrap()
    }

    #[test]
    fn open_info_and_battery() {
        let (dev, _) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        let info = sh.info().unwrap();
        assert_eq!(info.model, "NanoVNA-F 4.3");
        assert_eq!(info.firmware, "Firmware 1.0.4 by BH5HNU");
        assert_eq!(sh.battery_millivolts().unwrap(), 4209);
        assert_eq!(sh.sweep_settings().unwrap(), (1_000_000, 30_000_000, 101));
    }

    #[test]
    fn command_strips_echo_and_prompt() {
        let (dev, _) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        assert_eq!(sh.command("vbat", COMMAND_TIMEOUT).unwrap(), "4209 mV");
        assert_eq!(sh.command("resume", COMMAND_TIMEOUT).unwrap(), "");
    }

    #[test]
    fn a_port_without_a_prompt_fails_and_is_left_alone() {
        let (mut dev, _) = FakeShell::new();
        dev.silent = true;
        let port = FakePort::new(dev);
        let err = NanoVnaShell::from_port(port.boxed()).err().expect("no prompt, no shell");
        assert!(err.to_string().contains("no NanoVNA shell prompt"), "{err}");
        // Only the probing return was sent: no `resume` to whatever is on
        // this port when the half-built handle is dropped.
        assert_eq!(port.written(), b"\r");
    }

    #[test]
    fn sweep_returns_the_points_and_restores_the_device_on_drop() {
        let (dev, commands) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        let cfg = linear(2_000_000, 6_000_000, 5);
        let r = sh.sweep(&cfg).unwrap();
        assert_eq!(r.freqs_hz, vec![2_000_000, 3_000_000, 4_000_000, 5_000_000, 6_000_000]);
        assert!(r.points.is_empty(), "a shell sweep has no raw FIFO records");
        assert_eq!(r.missing_points(), 0);
        for (i, f) in r.freqs_hz.iter().enumerate() {
            let g = *f as f32 / 1e9;
            assert_eq!(r.s11[i], Complex64::new(g as f64, 0.5));
            assert_eq!(r.s21[i], Complex64::new(0.25, -g as f64));
        }
        // Raw data: mask = FREQ | S11 | S21 | UNCALIBRATED.
        assert!(commands.lock().unwrap().contains(&"scan_bin 2000000 6000000 5 15".to_string()));
        drop(sh);
        let seen = commands.lock().unwrap().clone();
        let tail: Vec<&str> = seen.iter().rev().take(3).rev().map(String::as_str).collect();
        assert_eq!(tail, ["sweep 1000000 30000000 101", "cal on", "resume"], "all commands: {seen:?}");
    }

    #[test]
    fn a_calibrated_sweep_leaves_out_the_raw_bit() {
        let (dev, commands) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        let mut cfg = linear(2_000_000, 6_000_000, 5);
        cfg.calibrated = true;
        sh.sweep(&cfg).unwrap();
        assert!(commands.lock().unwrap().contains(&"scan_bin 2000000 6000000 5 7".to_string()));
    }

    #[test]
    fn a_long_sweep_is_scanned_in_chunks_of_301() {
        let (dev, commands) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        let cfg = linear(1_000_000, 700_000_000, 700);
        let r = sh.sweep(&cfg).unwrap();
        assert_eq!(r.freqs_hz, cfg.frequencies());
        let scans: Vec<String> = commands
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("scan_bin"))
            .cloned()
            .collect();
        assert_eq!(
            scans,
            [
                "scan_bin 1000000 301000000 301 15",
                "scan_bin 302000000 602000000 301 15",
                "scan_bin 603000000 700000000 98 15",
            ]
        );
        // The chunks join without a gap or a repeated point.
        assert_eq!(r.s11[300].re as f32, 0.301);
        assert_eq!(r.s11[301].re as f32, 0.302);
    }

    #[test]
    fn a_text_answer_to_scan_bin_is_reported() {
        let (mut dev, _) = FakeShell::new();
        dev.scan_error = Some("sweep points exceeds range 301");
        let (mut sh, _) = shell(dev);
        let err = sh.sweep(&linear(2_000_000, 6_000_000, 5)).unwrap_err();
        assert!(err.to_string().contains("sweep points exceeds range 301"), "{err}");
    }

    #[test]
    fn a_wrong_echo_is_a_protocol_error() {
        let (dev, _) = FakeShell::new();
        let port = FakePort::new(dev);
        let mut sh = NanoVnaShell::from_port(port.boxed()).unwrap();
        // The saved sweep and calibration state are read with a good echo;
        // garble only what follows by swapping in a second device state.
        sh.saved_sweep = Some((1_000_000, 30_000_000, 101));
        sh.saved_cal_apply = Some(false);
        let (mut bad, _) = FakeShell::new();
        bad.garbled_echo = true;
        let port = FakePort::new(bad);
        sh.port = port.boxed();
        let err = sh.sweep(&linear(2_000_000, 6_000_000, 5)).unwrap_err();
        assert!(err.to_string().contains("unexpected echo"), "{err}");
        sh.restore_on_drop = false;
    }

    #[test]
    fn screenshot_takes_the_size_from_the_model() {
        let (dev, _) = FakeShell::new();
        let (mut sh, _) = shell(dev);
        let shot = sh.screenshot().unwrap();
        assert_eq!((shot.width, shot.height, shot.bits_per_pixel), (800, 480, 16));
        assert_eq!(shot.data.len(), 800 * 480 * 2);
        assert!(!shot.big_endian);
    }

    #[test]
    fn screenshot_of_an_unknown_model_goes_by_the_byte_count() {
        let (mut dev, _) = FakeShell::new();
        dev.model = "SomethingElse";
        dev.capture_bytes = 480 * 320 * 2;
        let (mut sh, _) = shell(dev);
        let shot = sh.screenshot().unwrap();
        assert_eq!((shot.width, shot.height), (480, 320));
        // A size that matches no known screen is refused.
        let (mut dev, _) = FakeShell::new();
        dev.model = "SomethingElse";
        dev.capture_bytes = 1000;
        let (mut sh, _) = shell(dev);
        assert!(sh.screenshot().unwrap_err().to_string().contains("unknown screen size"));
    }

    #[test]
    fn a_pulled_cable_is_a_link_failure() {
        let (dev, _) = FakeShell::new();
        let (mut sh, port) = shell(dev);
        port.unplug();
        let err = sh.sweep(&linear(2_000_000, 6_000_000, 5)).unwrap_err();
        assert!(err.is_link_failure(), "{err:?}");
    }

    #[test]
    fn info_is_parsed() {
        let text =
            "Model:      NanoVNA-F 4.3\r\nVersion:    Firmware 1.0.4 by BH5HNU\r\nBuild time: Mar 21 2022 - 15:31:22 CST\r\nProject:    x";
        let i = parse_info(text);
        assert_eq!(i.model, "NanoVNA-F 4.3");
        assert_eq!(i.firmware, "Firmware 1.0.4 by BH5HNU");
        assert_eq!(i.build, "Mar 21 2022 - 15:31:22 CST");
        assert_eq!(screen_size_for_model(&i.model), Some((800, 480)));
    }

    #[test]
    fn chunks_keep_the_grid() {
        let cfg = SweepConfig::linear(1_000_000, 100_000_000, 1001).unwrap();
        let chunks = scan_chunks(&cfg, 301);
        assert_eq!(chunks.iter().map(|c| c.points as usize).sum::<usize>(), 1001);
        assert_eq!(chunks.len(), 4);
        let joined: Vec<u64> = chunks.iter().flat_map(|c| c.frequencies()).collect();
        assert_eq!(joined, cfg.frequencies());
        assert_eq!(chunks[3].points, 98);
        assert_eq!(scan_chunks(&cfg, 301).last().unwrap().stop_hz(), cfg.stop_hz());
        let small = SweepConfig::linear(1_000_000, 2_000_000, 11).unwrap();
        assert_eq!(scan_chunks(&small, 301).len(), 1);
    }

    #[test]
    fn scan_bin_block_is_decoded() {
        // Captured from a NanoVNA-F: header for mask 3, 11 points; first record 1 MHz, 1.00091+j0.00011.
        let header = [0x83, 0x00, 0x0b, 0x00];
        let (m, n) = parse_scan_header(&header);
        assert_eq!((m, n), (0x83, 11));
        assert_eq!(point_size(m), 12);
        let mut data = Vec::new();
        for i in 0..11u32 {
            data.extend_from_slice(&(1_000_000 + i * 2_900_000).to_le_bytes());
            data.extend_from_slice(&1.00091f32.to_le_bytes());
            data.extend_from_slice(&0.00011f32.to_le_bytes());
        }
        let pts = parse_scan_points(m, n, &data).unwrap();
        assert_eq!(pts.len(), 11);
        assert_eq!(pts[0].freq_hz, Some(1_000_000));
        assert_eq!(pts[10].freq_hz, Some(30_000_000));
        assert!((pts[0].s11.unwrap().re - 1.00091).abs() < 1e-6);
        assert!(pts[0].s21.is_none());
        assert!(parse_scan_points(m, n, &data[..100]).is_err());
        assert_eq!(point_size(0x8f), 20);
    }
}
