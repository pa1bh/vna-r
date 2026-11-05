//! Time-domain reflectometry from a frequency sweep.
//!
//! The inverse Fourier transform of S11(f) is the reflection impulse
//! response h(t): every discontinuity along the line shows up as a pulse at
//! its round-trip delay. Two numbers govern everything:
//!
//! * **span** → resolution: two reflections closer than ~1/span merge;
//! * **step Δf** → alias-free range: 1/Δf of round-trip time.
//!
//! Two views are produced:
//!
//! * the **band-pass impulse** `|h_bp(t)|`: the envelope of the complex
//!   IFFT of the measured band. Works for any grid, no DC needed, shows
//!   *where* reflections are;
//! * the **low-pass step** Γ(t): the running sum of the real impulse of a
//!   conjugate-symmetric spectrum. Needs the spectrum from DC, so the sweep
//!   is resampled onto a harmonic grid and extrapolated to DC. Carries sign,
//!   hence impedance along the line via Z = Z0·(1+Γ)/(1−Γ).

use std::f64::consts::PI;

use num_complex::Complex64;
use rustfft::FftPlanner;

use crate::error::{Error, Result};

/// Largest FFT we are willing to run (4 Mi complex values, 64 MiB). The size
/// grows with top frequency / frequency step, so a narrow sweep high up
/// (say 1 MHz wide at 1.4 GHz) would otherwise ask for gigabytes, on the GUI
/// thread. Such a sweep has no useful range resolution anyway.
pub const MAX_FFT_SIZE: usize = 1 << 22;

/// Speed of light in vacuum, m/s.
pub const C0: f64 = 299_792_458.0;
pub const Z0: f64 = 50.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Rectangular,
    Hann,
    Blackman,
}

impl Window {
    /// Symmetric window value at `x ∈ [-1, 1]` (0 = centre).
    fn at(&self, x: f64) -> f64 {
        let x = x.clamp(-1.0, 1.0);
        match self {
            Window::Rectangular => 1.0,
            Window::Hann => 0.5 * (1.0 + (PI * x).cos()),
            Window::Blackman => 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos(),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "rect" | "rectangular" | "none" => Window::Rectangular,
            "hann" | "hanning" => Window::Hann,
            "blackman" => Window::Blackman,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Window::Rectangular => "rectangular",
            Window::Hann => "hann",
            Window::Blackman => "blackman",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdrConfig {
    pub window: Window,
    /// Velocity factor of the line (0.66 RG-58, ~0.70 RG-316, 0.83 foam).
    pub velocity_factor: f64,
    /// FFT length; 0 picks the next power of two ≥ 8× the harmonic grid.
    pub fft_size: usize,
}

impl Default for TdrConfig {
    fn default() -> Self {
        TdrConfig {
            window: Window::Hann,
            velocity_factor: 0.70,
            fft_size: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TdrResult {
    /// Round-trip time per bin, seconds (bin 0 = the reference plane).
    pub time_s: Vec<f64>,
    /// One-way distance per bin, metres, using the velocity factor.
    pub distance_m: Vec<f64>,
    /// Band-pass impulse envelope, linear |Γ| (1 = total reflection).
    pub impulse_mag: Vec<f64>,
    /// Low-pass step response Γ(t).
    pub step: Vec<f64>,
    /// Impedance along the line from the step response, Ω.
    pub impedance_ohm: Vec<f64>,
    /// ≈ 1/span: two reflections closer than this merge.
    pub resolution_s: f64,
    /// 1/Δf: reflections beyond this round-trip time alias back.
    pub max_time_s: f64,
    pub velocity_factor: f64,
}

impl TdrResult {
    pub fn resolution_m(&self) -> f64 {
        self.resolution_s * C0 * self.velocity_factor / 2.0
    }

    pub fn max_distance_m(&self) -> f64 {
        self.max_time_s * C0 * self.velocity_factor / 2.0
    }

    /// Strongest local maxima of the impulse envelope, `(bin, |Γ|)`,
    /// largest first, at most `count`, no two closer than `min_separation` bins.
    pub fn peaks(&self, count: usize, min_separation: usize) -> Vec<(usize, f64)> {
        let m = &self.impulse_mag;
        if m.len() < 2 {
            return Vec::new();
        }
        // Bin 0 (the reference plane) counts as a peak when it is a local maximum.
        let mut cands: Vec<(usize, f64)> = (0..m.len() - 1)
            .filter(|&i| (i == 0 || m[i] >= m[i - 1]) && m[i] > m[i + 1])
            .map(|i| (i, m[i]))
            .collect();
        cands.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut out: Vec<(usize, f64)> = Vec::new();
        for (i, v) in cands {
            if out.iter().all(|(j, _)| i.abs_diff(*j) >= min_separation) {
                out.push((i, v));
                if out.len() == count {
                    break;
                }
            }
        }
        out
    }
}

/// Compute the TDR views of `s11` measured on the uniform grid `freqs_hz`.
pub fn tdr(freqs_hz: &[u64], s11: &[Complex64], cfg: &TdrConfig) -> Result<TdrResult> {
    let Grid {
        n,
        df,
        f0,
        fmax,
        span,
        m,
        nfft,
    } = check_grid(freqs_hz, s11, cfg)?;

    let mut planner = FftPlanner::<f64>::new();
    let ifft = planner.plan_fft_inverse(nfft);

    // --- band-pass impulse: windowed measured band, complex IFFT ---------
    let mut buf = vec![Complex64::new(0.0, 0.0); nfft];
    let mut gain = 0.0;
    for (i, s) in s11.iter().enumerate() {
        let x = if n > 1 { 2.0 * i as f64 / (n - 1) as f64 - 1.0 } else { 0.0 };
        let w = cfg.window.at(x);
        gain += w;
        // Place the band at its true bin so phase slopes map to the right delay;
        // the magnitude is insensitive to the offset f0 anyway.
        let k = ((freqs_hz[i] as f64) / df).round() as usize % nfft;
        buf[k] = s * w;
    }
    ifft.process(&mut buf);
    let impulse_mag: Vec<f64> = buf.iter().take(nfft / 2).map(|c| c.norm() / gain).collect();

    // --- low-pass step: harmonic resampling, DC extrapolation, conj. symmetry
    let mut spec = vec![Complex64::new(0.0, 0.0); nfft];
    let one_sided = |k: usize| -> f64 { cfg.window.at(k as f64 / m as f64) };
    let interp = |f: f64| -> Complex64 {
        if f <= f0 {
            return s11[0];
        }
        if f >= fmax {
            return s11[n - 1];
        }
        let pos = (f - f0) / df;
        let i = pos.floor() as usize;
        let t = pos - i as f64;
        s11[i] * (1.0 - t) + s11[(i + 1).min(n - 1)] * t
    };
    let dc = extrapolate_dc(freqs_hz, s11);
    for k in 0..=m {
        let f = k as f64 * df;
        let v = if k == 0 { Complex64::new(dc, 0.0) } else { interp(f) };
        let w = one_sided(k);
        spec[k] = v * w;
        if k > 0 && k < nfft - k {
            spec[nfft - k] = (v * w).conj();
        }
    }
    ifft.process(&mut spec);
    let scale = 1.0 / nfft as f64;
    let mut step = Vec::with_capacity(nfft / 2);
    // A reflection at the reference plane has half its (windowed) impulse at
    // negative time, which the IFFT wraps to the upper half of the buffer.
    // Start the running sum there so the step is right from t = 0 on.
    let mut acc: f64 = spec.iter().skip(nfft / 2).map(|c| c.re * scale).sum();
    for c in spec.iter().take(nfft / 2) {
        acc += c.re * scale;
        step.push(acc);
    }
    let impedance_ohm: Vec<f64> = step
        .iter()
        .map(|g| {
            let g = g.clamp(-0.999, 0.999);
            Z0 * (1.0 + g) / (1.0 - g)
        })
        .collect();

    let dt = 1.0 / (nfft as f64 * df);
    let time_s: Vec<f64> = (0..nfft / 2).map(|i| i as f64 * dt).collect();
    let distance_m: Vec<f64> = time_s.iter().map(|t| t * C0 * cfg.velocity_factor / 2.0).collect();

    Ok(TdrResult {
        time_s,
        distance_m,
        impulse_mag,
        step,
        impedance_ohm,
        resolution_s: 1.0 / span,
        max_time_s: 1.0 / df,
        velocity_factor: cfg.velocity_factor,
    })
}

/// Γ at DC must be real. Extrapolate the real part linearly from the lowest
/// few points (a single lowest point is often the noisiest of the sweep).
struct Grid {
    n: usize,
    df: f64,
    f0: f64,
    fmax: f64,
    span: f64,
    /// Index of the highest measured frequency on the harmonic grid k·Δf.
    m: usize,
    nfft: usize,
}

fn check_grid(freqs_hz: &[u64], s11: &[Complex64], cfg: &TdrConfig) -> Result<Grid> {
    let n = freqs_hz.len();
    if n < 8 || s11.len() != n {
        return Err(Error::InvalidArgument("TDR needs at least 8 points with matching S11".into()));
    }
    let df = (freqs_hz[1] - freqs_hz[0]) as f64;
    if df <= 0.0 || freqs_hz.windows(2).any(|w| w[1] - w[0] != freqs_hz[1] - freqs_hz[0]) {
        return Err(Error::InvalidArgument("TDR needs a uniformly spaced sweep".into()));
    }
    if !(0.1..=1.0).contains(&cfg.velocity_factor) {
        return Err(Error::InvalidArgument("velocity factor must be within 0.1..=1".into()));
    }
    let f0 = freqs_hz[0] as f64;
    let fmax = *freqs_hz.last().unwrap() as f64;
    // Harmonic grid k·Δf, k = 0..=m, covering the measured band.
    let m = (fmax / df).round() as usize;
    let nfft = if cfg.fft_size > 0 {
        cfg.fft_size.max(2 * m + 2)
    } else {
        (8 * (m + 1)).next_power_of_two()
    };
    if nfft > MAX_FFT_SIZE {
        return Err(Error::InvalidArgument(format!(
            "sweep too narrow for TDR: top frequency / step = {m}, at most {} (widen the span or use fewer points)",
            MAX_FFT_SIZE / 8
        )));
    }
    Ok(Grid {
        n,
        df,
        f0,
        fmax,
        span: fmax - f0,
        m,
        nfft,
    })
}

// ---------------------------------------------------------------------------
// Time gating
// ---------------------------------------------------------------------------

/// Keep or remove the reflections inside a time window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    /// Keep what lies between start and stop (e.g. only the antenna end of a feed line).
    BandPass,
    /// Remove what lies between start and stop (e.g. the connector at the reference plane).
    Notch,
}

impl GateKind {
    pub fn name(&self) -> &'static str {
        match self {
            GateKind::BandPass => "band pass",
            GateKind::Notch => "notch",
        }
    }
}

/// A time gate on the band-pass impulse response. Times are round-trip,
/// seconds, like [`TdrResult::time_s`]; `start_s` may be negative to include
/// the reference plane itself. The flanks are raised-cosine, each
/// [`GATE_EDGE_RESOLUTIONS`] × the TDR resolution wide, centred on start and
/// stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub start_s: f64,
    pub stop_s: f64,
    pub kind: GateKind,
}

/// Width of each gate flank in units of the TDR resolution (1/span).
pub const GATE_EDGE_RESOLUTIONS: f64 = 2.0;
/// Smallest window weight the compensation divides by.
pub const GATE_WINDOW_FLOOR: f64 = 0.1;

/// Result of [`gate`].
#[derive(Debug, Clone, PartialEq)]
pub struct GatedResult {
    /// Gated S11 on the input grid.
    pub s11: Vec<Complex64>,
    /// A gate of width T blurs the frequency response over about 1/T: the
    /// outermost `edge_hz` of the band on each side is unreliable.
    pub edge_hz: f64,
    /// Points at each band edge where the window compensation was clamped to
    /// [`GATE_WINDOW_FLOOR`] (there the gated trace is attenuated and noisier).
    pub clamped_points: usize,
}

/// Time-gate `s11`: transform the (windowed) band to the time domain, multiply
/// by the gate, transform back and undo the window.
///
/// This is what bench analysers do ("gating"). The window keeps the sinc
/// sidelobes of strong reflections from leaking across the gate; dividing it
/// out afterwards restores the band, except at the very edges where the
/// window is small — there the division is clamped and the result tapers.
/// The frequency blur of the gate (≈ 1/gate width) is inherent: a narrow gate
/// cannot resolve fine frequency detail. Use a gate several resolutions wide
/// and judge the result away from the band edges.
pub fn gate(freqs_hz: &[u64], s11: &[Complex64], cfg: &TdrConfig, g: &Gate) -> Result<GatedResult> {
    let Grid { n, df, span, nfft, .. } = check_grid(freqs_hz, s11, cfg)?;
    if !(g.stop_s > g.start_s) || !g.start_s.is_finite() || !g.stop_s.is_finite() {
        return Err(Error::InvalidArgument("gate stop must lie after gate start".into()));
    }
    let max_t = 1.0 / df;
    if g.stop_s > max_t / 2.0 || g.start_s < -max_t / 2.0 {
        return Err(Error::InvalidArgument(format!(
            "gate must lie within ±{:.1} ns (half the alias-free range)",
            max_t / 2.0 * 1e9
        )));
    }
    let mut planner = FftPlanner::<f64>::new();
    let ifft = planner.plan_fft_inverse(nfft);
    let fft = planner.plan_fft_forward(nfft);

    let mut buf = vec![Complex64::new(0.0, 0.0); nfft];
    let mut weights = Vec::with_capacity(n);
    let mut bins = Vec::with_capacity(n);
    for (i, s) in s11.iter().enumerate() {
        let x = 2.0 * i as f64 / (n - 1) as f64 - 1.0;
        let w = cfg.window.at(x);
        let k = ((freqs_hz[i] as f64) / df).round() as usize % nfft;
        buf[k] = s * w;
        weights.push(w);
        bins.push(k);
    }
    ifft.process(&mut buf);

    let dt = 1.0 / (nfft as f64 * df);
    let edge = GATE_EDGE_RESOLUTIONS / span;
    let shape = |t: f64| -> f64 {
        // raised-cosine flanks centred on start and stop
        let rise = |u: f64| -> f64 {
            if u <= -edge / 2.0 {
                0.0
            } else if u >= edge / 2.0 {
                1.0
            } else {
                0.5 * (1.0 + (PI * u / edge).sin())
            }
        };
        rise(t - g.start_s) * (1.0 - rise(t - g.stop_s))
    };
    for (j, c) in buf.iter_mut().enumerate() {
        let t = if j < nfft / 2 {
            j as f64 * dt
        } else {
            (j as f64 - nfft as f64) * dt
        };
        let keep = match g.kind {
            GateKind::BandPass => shape(t),
            GateKind::Notch => 1.0 - shape(t),
        };
        *c *= keep;
    }
    fft.process(&mut buf);

    let scale = 1.0 / nfft as f64;
    let mut clamped = 0;
    let s11g: Vec<Complex64> = bins
        .iter()
        .zip(&weights)
        .map(|(&k, &w)| {
            if w < GATE_WINDOW_FLOOR {
                clamped += 1;
            }
            buf[k] * scale / w.max(GATE_WINDOW_FLOOR)
        })
        .collect();
    Ok(GatedResult {
        s11: s11g,
        edge_hz: 1.0 / (g.stop_s - g.start_s),
        clamped_points: clamped / 2,
    })
}

fn extrapolate_dc(freqs_hz: &[u64], s11: &[Complex64]) -> f64 {
    let k = (s11.len() / 20).clamp(2, 8);
    let xs: Vec<f64> = freqs_hz[..k].iter().map(|f| *f as f64).collect();
    let ys: Vec<f64> = s11[..k].iter().map(|s| s.re).collect();
    let mx = xs.iter().sum::<f64>() / k as f64;
    let my = ys.iter().sum::<f64>() / k as f64;
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let slope = if sxx > 0.0 {
        xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / sxx
    } else {
        0.0
    };
    (my - slope * mx).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two reflections: a small one at the reference plane and a lossy short
    /// 1 m down a VF-0.7 line, on a 1 MHz..1 GHz grid (1 ns resolution).
    fn two_reflections() -> (Vec<u64>, Vec<Complex64>, f64) {
        let freqs: Vec<u64> = (1..=1000).map(|k| k * 1_000_000).collect();
        let tau = 2.0 * 1.0 / (C0 * 0.7); // round trip
        let s11 = freqs
            .iter()
            .map(|&f| {
                let f = f as f64;
                let far = -0.9 * Complex64::from_polar(1.0, -2.0 * PI * f * tau);
                Complex64::new(0.05, 0.0) + far
            })
            .collect();
        (freqs, s11, tau)
    }

    #[test]
    fn gate_isolates_the_far_reflection() {
        let (freqs, s11, tau) = two_reflections();
        let cfg = TdrConfig::default();
        let g = Gate {
            start_s: tau - 3e-9,
            stop_s: tau + 3e-9,
            kind: GateKind::BandPass,
        };
        let out = gate(&freqs, &s11, &cfg, &g).unwrap();
        assert_eq!(out.s11.len(), freqs.len());
        // mid-band (away from the edges by more than edge_hz) only the short remains
        let lo = out.edge_hz;
        for (i, &f) in freqs.iter().enumerate() {
            let f = f as f64;
            if f < 1e6 + lo || f > 1e9 - lo {
                continue;
            }
            let want = -0.9 * Complex64::from_polar(1.0, -2.0 * PI * f * tau);
            let err = (out.s11[i] - want).norm();
            assert!(err < 0.02, "f={f}: err {err}");
        }
        assert!(out.edge_hz < 200e6);
    }

    #[test]
    fn notch_removes_the_reference_plane() {
        let (freqs, s11, tau) = two_reflections();
        let cfg = TdrConfig::default();
        // notch around t = 0: removes the 0.05, keeps the short
        let g = Gate {
            start_s: -3e-9,
            stop_s: 3e-9,
            kind: GateKind::Notch,
        };
        let out = gate(&freqs, &s11, &cfg, &g).unwrap();
        let (mut worst, mut count) = (0.0f64, 0);
        for (i, &f) in freqs.iter().enumerate() {
            let f = f as f64;
            if f < 300e6 || f > 700e6 {
                continue;
            }
            let want = -0.9 * Complex64::from_polar(1.0, -2.0 * PI * f * tau);
            worst = worst.max((out.s11[i] - want).norm());
            count += 1;
        }
        assert!(count > 300 && worst < 0.02, "worst {worst}");
        // band pass around t = 0 keeps only the 0.05
        let g = Gate {
            start_s: -3e-9,
            stop_s: 3e-9,
            kind: GateKind::BandPass,
        };
        let out = gate(&freqs, &s11, &cfg, &g).unwrap();
        for (i, &f) in freqs.iter().enumerate() {
            if (300e6..=700e6).contains(&(f as f64)) {
                assert!((out.s11[i].norm() - 0.05).abs() < 0.01, "f={f}: {}", out.s11[i].norm());
            }
        }
    }

    #[test]
    fn gate_rejects_bad_windows() {
        let (freqs, s11, _) = two_reflections();
        let cfg = TdrConfig::default();
        assert!(gate(
            &freqs,
            &s11,
            &cfg,
            &Gate {
                start_s: 5e-9,
                stop_s: 5e-9,
                kind: GateKind::BandPass
            }
        )
        .is_err());
        assert!(gate(
            &freqs,
            &s11,
            &cfg,
            &Gate {
                start_s: 0.0,
                stop_s: 1.0,
                kind: GateKind::BandPass
            }
        )
        .is_err());
    }

    /// S11 of a lossless line of one-way delay `tau` terminated in `gamma_load`.
    fn line(freqs: &[u64], tau: f64, gamma_load: Complex64) -> Vec<Complex64> {
        freqs
            .iter()
            .map(|&f| gamma_load * Complex64::from_polar(1.0, -2.0 * PI * f as f64 * 2.0 * tau))
            .collect()
    }

    fn grid() -> Vec<u64> {
        (0..1001).map(|i| 50_000 + i * 6_300_000).collect() // the LiteVNA 67 full-span grid
    }

    #[test]
    fn short_at_known_distance() {
        let freqs = grid();
        let tau = 2.5e-9; // ≈ 52 cm at VF 0.70
        let cfg = TdrConfig {
            velocity_factor: 0.70,
            ..Default::default()
        };
        let r = tdr(&freqs, &line(&freqs, tau, Complex64::new(-1.0, 0.0)), &cfg).unwrap();
        let peaks = r.peaks(1, 10);
        let (bin, mag) = peaks[0];
        let expect_m = tau * C0 * 0.70;
        assert!(
            (r.distance_m[bin] - expect_m).abs() < 0.01,
            "peak at {} m, expected {expect_m} m",
            r.distance_m[bin]
        );
        assert!((mag - 1.0).abs() < 0.05, "peak magnitude {mag}");
        // step goes to -1 (short) just after the reflection, impedance ≈ 0
        let after = bin + (0.3e-9 / (r.time_s[1] - r.time_s[0])) as usize;
        assert!(r.step[after] < -0.9, "step after short {}", r.step[after]);
        assert!(r.impedance_ohm[after] < 5.0);
        // before the reflection the line is matched: Γ ≈ 0, Z ≈ 50
        let before = bin / 2;
        assert!(r.step[before].abs() < 0.05, "step before {}", r.step[before]);
        assert!((r.impedance_ohm[before] - 50.0).abs() < 5.0);
        assert!((r.resolution_m() - 0.0167).abs() < 0.002);
        assert!((r.max_distance_m() - 16.66).abs() < 0.1);
    }

    #[test]
    fn open_and_mismatch_steps() {
        let freqs = grid();
        let cfg = TdrConfig::default();
        let r = tdr(&freqs, &line(&freqs, 1.0e-9, Complex64::new(1.0, 0.0)), &cfg).unwrap();
        let (bin, _) = r.peaks(1, 10)[0];
        let after = bin + 20;
        assert!(r.step[after] > 0.9, "open step {}", r.step[after]);
        assert!(r.impedance_ohm[after] > 500.0);
        // a 75 Ω termination: Γ = 0.2
        let r = tdr(&freqs, &line(&freqs, 1.0e-9, Complex64::new(0.2, 0.0)), &cfg).unwrap();
        let (bin, mag) = r.peaks(1, 10)[0];
        assert!((mag - 0.2).abs() < 0.02);
        assert!((r.impedance_ohm[bin + 20] - 75.0).abs() < 3.0, "Z {}", r.impedance_ohm[bin + 20]);
    }

    #[test]
    fn open_at_the_port_is_found_at_zero() {
        let freqs = grid();
        let r = tdr(&freqs, &vec![Complex64::new(1.0, 0.0); freqs.len()], &TdrConfig::default()).unwrap();
        let (bin, mag) = r.peaks(1, 10)[0];
        assert_eq!(bin, 0);
        assert!((mag - 1.0).abs() < 1e-6);
        // the step needs ~1/(2·fmax) to rise; check well after that
        let settled = (0.5e-9 / (r.time_s[1] - r.time_s[0])) as usize;
        assert!(r.step[settled] > 0.98, "step {}", r.step[settled]);
        assert!(r.impedance_ohm[settled] > 1000.0);
    }

    #[test]
    fn rejects_bad_input() {
        let freqs = grid();
        assert!(tdr(&freqs[..4], &vec![Complex64::new(0.0, 0.0); 4], &TdrConfig::default()).is_err());
        let mut uneven = freqs.clone();
        uneven[5] += 1;
        assert!(tdr(&uneven, &vec![Complex64::new(0.0, 0.0); uneven.len()], &TdrConfig::default()).is_err());
        assert!(Window::parse("hann") == Some(Window::Hann));
    }

    #[test]
    fn narrow_sweep_high_up_is_refused_before_allocating() {
        // 1 MHz wide at 1.4 GHz with 3201 points: fmax/Δf ≈ 4.5 million, an FFT of 2^26
        // complex values (1 GiB). Must come back as an error, not an allocation.
        let freqs: Vec<u64> = (0..3201).map(|i| 1_400_000_000 + i * 312).collect();
        let s11 = vec![Complex64::new(0.0, 0.0); freqs.len()];
        let err = tdr(&freqs, &s11, &TdrConfig::default()).unwrap_err().to_string();
        assert!(err.contains("too narrow"), "{err}");
    }
}
