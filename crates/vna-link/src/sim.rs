//! A device that exists only in software, for tests, demonstrations and
//! working on the server and the GUI without an analyser.
//!
//! The real worker drives it: [`Cmd::Connect`](crate::Cmd::Connect) with a
//! port name starting with [`SIMULATOR`] opens it instead of a serial port,
//! and from then on everything above the device (progress per block,
//! cancelling, the backlog, continuous sweeping, Stop) is the code that runs
//! with hardware.
//!
//! What it measures, on the exact grid asked for:
//!
//! * a display sweep: on port 1 a fan dipole for 80, 40, 20, 15 and 10 m
//!   behind 12 m of coax, on port 2 a band-pass filter around 14.2 MHz;
//! * a calibration standard: that standard, ideal (see [`Device::measuring`]).
//!
//! Both with a little noise that is different in every sweep, so that
//! averaging and a live trace visibly do something. It also hands out FIFO
//! records consistent with the data, like a LiteVNA, so that the raw-record
//! path is exercised too.

#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::f64::consts::PI;
use std::thread;
use std::time::{Duration, Instant};

use litevna::calibration::Standard;
use litevna::device::SweepProgress;
use litevna::discovery::Kind;
use litevna::protocol::{fifo_checksum, FifoPoint, FIFO_MAX_CHUNK, FIFO_POINT_SIZE};
use litevna::{Error, Limits, LiteVna, Result, Screenshot, SweepConfig, SweepResult, VnaInfo};
use num_complex::Complex64;

use crate::device::Device;
use crate::Purpose;

/// Port name of the simulator. `sim:` alone sweeps at [`DEFAULT_RATE`];
/// `sim:<points per second>` at another rate, and `sim:0` as fast as it can
/// (for tests). It is not in any port list: a server, a test or a developer
/// asks for it by name.
pub const SIMULATOR: &str = "sim:";

/// Points per second: a LiteVNA 67 with `average = 1` sweeps 1001 points
/// over 50 kHz–6.3 GHz in 1.6 s (docs/01, the measurements of 2026-11-03).
pub const DEFAULT_RATE: f64 = 625.0;

/// Points per progress report: a LiteVNA delivers this many records per READFIFO.
const BLOCK: usize = FIFO_MAX_CHUNK;

/// The battery of a LiteVNA that is mostly charged.
const BATTERY_MV: u16 = 3_950;

pub(crate) fn is_simulator(port: &str) -> bool {
    port.starts_with(SIMULATOR)
}

/// Open the simulator: what [`Vna::open`](litevna::Vna::open), `info` and
/// the battery reading are for a real device.
pub(crate) fn open(port: &str) -> Result<(Box<dyn Device>, VnaInfo, Option<u16>)> {
    let spec = &port[SIMULATOR.len()..];
    let rate = match spec {
        "" => Some(DEFAULT_RATE),
        "0" => None,
        s => match s.parse::<f64>() {
            Ok(r) if r.is_finite() && r > 0.0 => Some(r),
            _ => {
                return Err(Error::InvalidArgument(format!(
                    "{port}: after {SIMULATOR} comes nothing or a sweep rate in points per second"
                )))
            }
        },
    };
    let info = VnaInfo {
        // Behaves like a LiteVNA (FIFO records, any number of points) but has
        // no identity registers, so `lite` stays empty. Nothing in the GUI
        // decides on `kind`.
        kind: Kind::LiteVna,
        model: "vna-r simulator".into(),
        firmware: env!("CARGO_PKG_VERSION").into(),
        detail: match rate {
            Some(r) => format!("no hardware, {r:.0} points/s"),
            None => "no hardware, unpaced".into(),
        },
        // The range of a LiteVNA, without the averaging in the device.
        limits: Limits {
            has_device_average: false,
            ..Limits::LITEVNA
        },
        lite: None,
        shell: None,
    };
    let sim = Simulator {
        rate,
        on_port: Purpose::Display,
        sweeps: 0,
    };
    Ok((Box::new(sim), info, Some(BATTERY_MV)))
}

struct Simulator {
    /// Points per second; `None` is as fast as it goes.
    rate: Option<f64>,
    on_port: Purpose,
    /// Sweeps done, which seeds the noise of the next one.
    sweeps: u64,
}

impl Device for Simulator {
    fn sweep_with(&mut self, cfg: &SweepConfig, progress: SweepProgress<'_>) -> Result<SweepResult> {
        let n = cfg.points as usize;
        if n == 0 {
            return Err(Error::InvalidArgument("a sweep needs at least one point".into()));
        }
        let started = Instant::now();
        let mut filled = 0;
        while filled < n {
            filled += (n - filled).min(BLOCK);
            if let Some(rate) = self.rate {
                // A rate so low that the time does not fit a `Duration` (or an
                // `Instant`) is a sweep that never ends, not a panic.
                let after = Duration::try_from_secs_f64(filled as f64 / rate).unwrap_or(Duration::MAX);
                let wait = match started.checked_add(after) {
                    Some(due) => due.saturating_duration_since(Instant::now()),
                    None => after,
                };
                thread::sleep(wait);
            }
            // Like the drivers: a break after the last block changes nothing.
            if progress(filled, n).is_break() && filled < n {
                return Err(Error::Cancelled);
            }
        }
        self.sweeps += 1;
        let mut noise = Noise::new(self.sweeps);
        let freqs_hz = cfg.frequencies();
        let points: Vec<Option<FifoPoint>> = freqs_hz
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                let (s11, s21) = response(self.on_port, f as f64);
                let s11 = s11 + noise.complex(2e-3);
                let s21 = s21 * (Complex64::new(1.0, 0.0) + noise.complex(2e-3)) + noise.complex(3e-4);
                Some(record(i as u16, f as f64, s11, s21))
            })
            .collect();
        Ok(SweepResult {
            freqs_hz,
            s11: points.iter().map(|p| p.expect("every point").s11()).collect(),
            s21: points.iter().map(|p| p.expect("every point").s21()).collect(),
            checksum_ok: n,
            points,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: started.elapsed(),
        })
    }

    fn screenshot(&mut self) -> Result<Screenshot> {
        Ok(test_pattern())
    }

    fn battery_millivolts(&mut self) -> Result<u16> {
        Ok(BATTERY_MV)
    }

    fn release(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_lite_mut(&mut self) -> Option<&mut LiteVna> {
        None
    }

    fn measuring(&mut self, purpose: Purpose) {
        self.on_port = purpose;
    }
}

/// What is on the ports at `f` Hz: (S11, S21) without noise.
fn response(on_port: Purpose, f: f64) -> (Complex64, Complex64) {
    let zero = Complex64::new(0.0, 0.0);
    let one = Complex64::new(1.0, 0.0);
    match on_port {
        Purpose::Display => (antenna(f), band_pass(f)),
        Purpose::Standard(s) => match s {
            Standard::Open => (one, zero),
            Standard::Short => (-one, zero),
            Standard::Load | Standard::Isolation => (zero, zero),
            Standard::Through => (zero, one),
            // An ideal open or short at the end of 10 cm of lossless line.
            Standard::LineOpen => (line(f, 0.1, 0.0), zero),
            Standard::LineShort => (-line(f, 0.1, 0.0), zero),
        },
    }
}

/// Round trip over `len` m of coax with velocity factor 0.66 and `loss_db`
/// per metre at 30 MHz (rising with the root of the frequency).
fn line(f: f64, len: f64, loss_db: f64) -> Complex64 {
    let beta = 2.0 * PI * f / (0.66 * 299_792_458.0);
    let alpha = loss_db / 8.686 * (f / 30e6).sqrt();
    Complex64::new(-2.0 * alpha * len, -2.0 * beta * len).exp()
}

/// A fan dipole: one series RLC per band, all in parallel at the feed
/// point, seen through 12 m of RG-58-like coax.
fn antenna(f: f64) -> Complex64 {
    // (resonance Hz, radiation resistance ohm, Q)
    const BANDS: [(f64, f64, f64); 5] = [
        (3.65e6, 55.0, 14.0),
        (7.1e6, 62.0, 16.0),
        (14.2e6, 70.0, 18.0),
        (21.25e6, 80.0, 20.0),
        (28.6e6, 95.0, 22.0),
    ];
    let w = 2.0 * PI * f;
    let y: Complex64 = BANDS
        .iter()
        .map(|&(f0, r, q)| {
            let l = q * r / (2.0 * PI * f0);
            let c = 1.0 / ((2.0 * PI * f0).powi(2) * l);
            Complex64::new(1.0, 0.0) / Complex64::new(r, w * l - 1.0 / (w * c))
        })
        .sum();
    let z = Complex64::new(1.0, 0.0) / y;
    let z0 = litevna::Z0;
    (z - z0) / (z + z0) * line(f, 12.0, 0.045)
}

/// A third-order Butterworth band-pass, 12.2–16.2 MHz, with 0.6 dB of
/// insertion loss and 5 ns of extra delay.
fn band_pass(f: f64) -> Complex64 {
    let w = 2.0 * PI * f;
    let w0 = 2.0 * PI * 14.2e6;
    let bw = 2.0 * PI * 4e6;
    // The low-pass prototype at p = (s² + ω0²) / (s·B), s = jω.
    let p = Complex64::new(0.0, (w * w - w0 * w0) / (w * bw));
    let one = Complex64::new(1.0, 0.0);
    let h = one / ((p + one) * (p * p + p + one));
    h * 10f64.powf(-0.6 / 20.0) * Complex64::new(0.0, -w * 5e-9).exp()
}

/// The FIFO record a LiteVNA would send for this point: a reference wave of
/// the size the device reports, with a phase that turns with frequency.
fn record(index: u16, f: f64, s11: Complex64, s21: Complex64) -> FifoPoint {
    let fwd = Complex64::from_polar(4_194_304.0, 2.0 * PI * f * 3.7e-9);
    let (rev0, rev1) = (s11 * fwd, s21 * fwd);
    let mut r = [0u8; FIFO_POINT_SIZE];
    let words = [fwd.re, fwd.im, rev0.re, rev0.im, rev1.re, rev1.im].map(|x| x.round() as i32);
    for (k, w) in words.iter().enumerate() {
        r[4 * k..4 * k + 4].copy_from_slice(&w.to_le_bytes());
    }
    r[0x18..0x1A].copy_from_slice(&index.to_le_bytes());
    r[0x1F] = fifo_checksum(&r);
    FifoPoint::parse(&r).expect("a whole record")
}

/// splitmix64: small, deterministic, good enough to look like noise.
struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        Noise(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// Uniform in [-1, 1).
    fn uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }

    /// A complex value with parts of roughly `size`: each the sum of two
    /// uniforms, which bunches towards zero as noise does.
    fn complex(&mut self, size: f64) -> Complex64 {
        let re = (self.uniform() + self.uniform()) * size;
        let im = (self.uniform() + self.uniform()) * size;
        Complex64::new(re, im)
    }
}

/// 480×320 RGB565, big-endian like a LiteVNA 64: eight colour bars over a
/// grey ramp, with a white frame, so a wrong size, byte order or channel
/// order is plain to see.
fn test_pattern() -> Screenshot {
    let (w, h) = (480usize, 320usize);
    // white, yellow, cyan, green, magenta, red, blue, black
    const BARS: [(u8, u8, u8); 8] = [
        (255, 255, 255),
        (255, 255, 0),
        (0, 255, 255),
        (0, 255, 0),
        (255, 0, 255),
        (255, 0, 0),
        (0, 0, 255),
        (0, 0, 0),
    ];
    let rgb565 = |(r, g, b): (u8, u8, u8)| ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
    let mut data = Vec::with_capacity(w * h * 2);
    for y in 0..h {
        for x in 0..w {
            let c = if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                (255, 255, 255)
            } else if y < h * 2 / 3 {
                BARS[x * BARS.len() / w]
            } else {
                let v = (x * 255 / (w - 1)) as u8;
                (v, v, v)
            };
            data.extend_from_slice(&rgb565(c).to_be_bytes());
        }
    }
    Screenshot {
        width: w as u16,
        height: h as u16,
        bits_per_pixel: 16,
        big_endian: true,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litevna::traces::swr;

    #[test]
    fn the_antenna_has_its_bands_and_the_filter_its_pass_band() {
        // A dip near every band centre, a poor match between the bands.
        for f in [3.65e6, 7.1e6, 14.2e6, 21.25e6, 28.6e6] {
            let s = swr(antenna(f).norm());
            assert!(s < 2.0, "{f}: SWR {s}");
        }
        assert!(swr(antenna(10.0e6).norm()) > 5.0);
        assert!(band_pass(14.2e6).norm() > 0.9);
        assert!(band_pass(7.0e6).norm() < 0.01 && band_pass(30.0e6).norm() < 0.01);
    }

    #[test]
    fn records_carry_the_data_and_noise_differs_per_sweep() {
        let p = record(7, 14.2e6, Complex64::new(0.25, -0.5), Complex64::new(0.0, 0.75));
        assert!(p.checksum_ok && p.freq_index == 7);
        assert!((p.s11() - Complex64::new(0.25, -0.5)).norm() < 1e-6);
        assert!((p.s21() - Complex64::new(0.0, 0.75)).norm() < 1e-6);
        let (mut a, mut b) = (Noise::new(1), Noise::new(2));
        let (x, y) = (a.complex(1.0), b.complex(1.0));
        assert_ne!(x, y);
        assert!(x.re.abs() <= 2.0 && x.im.abs() <= 2.0);
    }

    #[test]
    fn the_rate_is_in_the_port_name() {
        let detail = |port: &str| open(port).map(|(_, info, _)| info.detail);
        assert!(detail("sim:").unwrap().contains("625 points/s"));
        assert!(detail("sim:2000").unwrap().contains("2000 points/s"));
        assert!(detail("sim:0").unwrap().contains("unpaced"));
        for bad in ["sim:fast", "sim:-5", "sim:NaN"] {
            assert!(matches!(open(bad), Err(Error::InvalidArgument(_))), "{bad}");
        }
    }

    #[test]
    fn the_test_pattern_has_the_size_of_a_screen() {
        let shot = test_pattern();
        assert_eq!(shot.data.len(), 480 * 320 * 2);
        let rgb = shot.to_rgb888();
        // yellow bar, a little in from the frame
        let px = |x: usize, y: usize| &rgb[3 * (y * 480 + x)..3 * (y * 480 + x) + 3];
        assert_eq!(px(90, 100), [255, 255, 0]);
        assert_eq!(px(0, 0), [255, 255, 255]);
    }
}
