//! Derived quantities for display: everything a VNA screen shows is a
//! function of the complex S-parameters per frequency. This module keeps
//! those functions UI-free so the GUI, the CLI export and tests share them.
//!
//! * log magnitude `20·log10|S|` (dB), linear magnitude `|S|`
//! * phase `arg S` (degrees, wrapped to ±180)
//! * SWR `(1+|Γ|)/(1−|Γ|)`
//! * impedance `Z = Z0·(1+Γ)/(1−Γ)` as `R + jX` or `|Z|` (ohm)
//! * group delay `τg = −dφ/dω` (seconds), from the unwrapped phase with a
//!   central difference over ±`aperture` points

use num_complex::Complex64 as C64;
use std::f64::consts::PI;

use crate::Z0;

/// Which S-parameter a trace is taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    S11,
    S21,
}

impl Param {
    pub fn name(&self) -> &'static str {
        match self {
            Param::S11 => "S11",
            Param::S21 => "S21",
        }
    }
}

/// What to plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    LogMag,
    LinMag,
    Phase,
    Swr,
    /// Resistance and reactance of the port impedance (two series).
    Impedance,
    ImpedanceMag,
    GroupDelay,
}

impl TraceKind {
    pub const ALL: [TraceKind; 7] = [
        TraceKind::LogMag,
        TraceKind::LinMag,
        TraceKind::Phase,
        TraceKind::Swr,
        TraceKind::Impedance,
        TraceKind::ImpedanceMag,
        TraceKind::GroupDelay,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            TraceKind::LogMag => "log mag",
            TraceKind::LinMag => "lin mag",
            TraceKind::Phase => "phase",
            TraceKind::Swr => "SWR",
            TraceKind::Impedance => "R + jX",
            TraceKind::ImpedanceMag => "|Z|",
            TraceKind::GroupDelay => "group delay",
        }
    }

    /// Short identifier for settings files and CLI flags.
    pub fn id(&self) -> &'static str {
        match self {
            TraceKind::LogMag => "logmag",
            TraceKind::LinMag => "linmag",
            TraceKind::Phase => "phase",
            TraceKind::Swr => "swr",
            TraceKind::Impedance => "rx",
            TraceKind::ImpedanceMag => "zmag",
            TraceKind::GroupDelay => "groupdelay",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim().to_ascii_lowercase();
        Self::ALL.iter().copied().find(|k| k.id() == t || k.name().eq_ignore_ascii_case(&t))
    }

    /// Axis unit label.
    pub fn unit(&self) -> &'static str {
        match self {
            TraceKind::LogMag => "dB",
            TraceKind::LinMag => "",
            TraceKind::Phase => "°",
            TraceKind::Swr => "SWR",
            TraceKind::Impedance | TraceKind::ImpedanceMag => "Ω",
            TraceKind::GroupDelay => "ns",
        }
    }

    /// Does this quantity make sense for the given parameter? Reflection-only
    /// quantities (SWR, impedance) are not defined for S21.
    pub fn applies_to(&self, param: Param) -> bool {
        match self {
            TraceKind::Swr | TraceKind::Impedance | TraceKind::ImpedanceMag => param == Param::S11,
            _ => true,
        }
    }

    /// Y-range that should always be visible, so an empty or flat trace still
    /// gets a sensible axis.
    pub fn default_range(&self) -> (f64, f64) {
        match self {
            TraceKind::LogMag => (-40.0, 0.0),
            TraceKind::LinMag => (0.0, 1.0),
            TraceKind::Phase => (-180.0, 180.0),
            TraceKind::Swr => (1.0, 3.0),
            TraceKind::Impedance => (-50.0, 100.0),
            TraceKind::ImpedanceMag => (0.0, 100.0),
            TraceKind::GroupDelay => (0.0, 0.0),
        }
    }
}

/// One plotted curve.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceSeries {
    /// Legend label, e.g. "S11 dB", "R", "X".
    pub label: String,
    pub values: Vec<f64>,
}

/// Port impedance from a reflection coefficient: `Z = Z0·(1+Γ)/(1−Γ)`.
/// Returns `(inf, 0)` for Γ = 1 (ideal open).
pub fn impedance(gamma: C64) -> C64 {
    let one = C64::new(1.0, 0.0);
    let d = one - gamma;
    if d.norm() < 1e-12 {
        C64::new(f64::INFINITY, 0.0)
    } else {
        Z0 * (one + gamma) / d
    }
}

/// Lowest value [`db`] returns: the magnitude 0 has no logarithm.
pub const DB_FLOOR: f64 = -200.0;
/// What [`swr`] returns for total reflection (|Γ| ≥ 1), where SWR is infinite.
pub const SWR_MAX: f64 = 999.0;

/// 20·log10 of a linear magnitude, not below [`DB_FLOOR`].
pub fn db(mag: f64) -> f64 {
    if mag <= 0.0 {
        DB_FLOOR
    } else {
        (20.0 * mag.log10()).max(DB_FLOOR)
    }
}

/// Standing-wave ratio from |Γ| for tables and files; [`SWR_MAX`] at total
/// reflection. Plots use [`swr_clamped`] with the lower [`SWR_CLAMP`].
pub fn swr(gamma_mag: f64) -> f64 {
    swr_clamped(gamma_mag, SWR_MAX)
}

/// Standing-wave ratio from |Γ|; saturates at `max` (|Γ| ≥ 1 is not physical
/// but happens on a raw or badly calibrated open).
pub fn swr_clamped(mag: f64, max: f64) -> f64 {
    if mag >= 1.0 {
        max
    } else {
        ((1.0 + mag) / (1.0 - mag)).min(max)
    }
}

/// Unwrap a phase sequence (radians) so consecutive samples never jump by
/// more than π.
pub fn unwrap_phase(phase_rad: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(phase_rad.len());
    let mut offset = 0.0;
    let mut prev = None;
    for &p in phase_rad {
        if let Some(q) = prev {
            let d: f64 = p - q;
            if d > PI {
                offset -= 2.0 * PI;
            } else if d < -PI {
                offset += 2.0 * PI;
            }
        }
        out.push(p + offset);
        prev = Some(p);
    }
    out
}

/// Group delay `τg = −dφ/dω` in seconds, per point, from the unwrapped phase
/// with a central difference between points `i−aperture` and `i+aperture`
/// (clamped at the ends). A larger aperture averages out noise at the cost
/// of frequency resolution. The sweep must be monotonic in frequency.
pub fn group_delay(freqs_hz: &[u64], s: &[C64], aperture: usize) -> Vec<f64> {
    let n = freqs_hz.len().min(s.len());
    if n < 2 {
        return vec![0.0; n];
    }
    let a = aperture.max(1);
    let phase = unwrap_phase(&s[..n].iter().map(|c| c.arg()).collect::<Vec<_>>());
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(a);
            let hi = (i + a).min(n - 1);
            let df = freqs_hz[hi] as f64 - freqs_hz[lo] as f64;
            if df <= 0.0 {
                0.0
            } else {
                -(phase[hi] - phase[lo]) / (2.0 * PI * df)
            }
        })
        .collect()
}

/// Compute the series for `kind` from parameter `param`. Returns nothing
/// when the quantity does not apply (see [`TraceKind::applies_to`]).
/// Impedance values are clamped to ±`Z_CLAMP` so an open does not blow up
/// the plot scale.
pub fn compute(kind: TraceKind, param: Param, freqs_hz: &[u64], s: &[C64], aperture: usize) -> Vec<TraceSeries> {
    if !kind.applies_to(param) {
        return Vec::new();
    }
    let name = param.name();
    let one = |label: String, values: Vec<f64>| vec![TraceSeries { label, values }];
    match kind {
        TraceKind::LogMag => one(format!("{name} dB"), s.iter().map(|c| db(c.norm())).collect()),
        TraceKind::LinMag => one(format!("|{name}|"), s.iter().map(|c| c.norm()).collect()),
        TraceKind::Phase => one(format!("{name} °"), s.iter().map(|c| c.arg().to_degrees()).collect()),
        TraceKind::Swr => one("SWR".into(), s.iter().map(|c| swr_clamped(c.norm(), SWR_CLAMP)).collect()),
        TraceKind::Impedance => {
            let z: Vec<C64> = s.iter().map(|c| impedance(*c)).collect();
            vec![
                TraceSeries {
                    label: "R".into(),
                    values: z.iter().map(|z| clamp(z.re)).collect(),
                },
                TraceSeries {
                    label: "X".into(),
                    values: z.iter().map(|z| clamp(z.im)).collect(),
                },
            ]
        }
        TraceKind::ImpedanceMag => one("|Z|".into(), s.iter().map(|c| clamp(impedance(*c).norm())).collect()),
        TraceKind::GroupDelay => one(
            format!("τg {name}"),
            group_delay(freqs_hz, s, aperture).iter().map(|t| t * 1e9).collect(),
        ),
    }
}

/// Largest impedance magnitude shown (ohm).
pub const Z_CLAMP: f64 = 1000.0;
/// Largest SWR shown.
pub const SWR_CLAMP: f64 = 20.0;

fn clamp(v: f64) -> f64 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(-Z_CLAMP, Z_CLAMP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, start: f64, step: f64) -> Vec<u64> {
        (0..n).map(|i| (start + i as f64 * step) as u64).collect()
    }

    #[test]
    fn group_delay_of_a_delay_line() {
        // S21 = exp(−j·2π·f·τ) with τ = 5 ns (≈ 1 m of cable at VF 0.67).
        let tau = 5e-9;
        let freqs = grid(401, 1e6, 2.5e6); // 1 MHz .. 1 GHz, 2.5 MHz steps → 4.5° per step
        let s: Vec<C64> = freqs.iter().map(|&f| C64::from_polar(0.9, -2.0 * PI * f as f64 * tau)).collect();
        for a in [1, 3, 10] {
            let gd = group_delay(&freqs, &s, a);
            for t in gd {
                assert!((t - tau).abs() < 1e-13, "aperture {a}: {t}");
            }
        }
        // Phase steps well beyond ±180° per point are unwrapped too (τ = 60 ns → 54°/step): fine.
        let tau2 = 60e-9;
        let s2: Vec<C64> = freqs.iter().map(|&f| C64::from_polar(1.0, -2.0 * PI * f as f64 * tau2)).collect();
        let gd = group_delay(&freqs, &s2, 2);
        assert!((gd[200] - tau2).abs() < 1e-13);
    }

    #[test]
    fn unwrap_removes_jumps() {
        let raw = [3.0, -3.0, 2.9, -3.1, 3.1];
        let u = unwrap_phase(&raw);
        for w in u.windows(2) {
            assert!((w[1] - w[0]).abs() < PI);
        }
        assert!((u[1] - (2.0 * PI - 3.0)).abs() < 1e-12);
    }

    #[test]
    fn db_and_swr_conventions() {
        assert_eq!(db(1.0), 0.0);
        assert!((db(0.1) + 20.0).abs() < 1e-12);
        assert_eq!(db(0.0), DB_FLOOR);
        assert_eq!(db(1e-30), DB_FLOOR);
        assert!((swr(1.0 / 3.0) - 2.0).abs() < 1e-12);
        assert_eq!(swr(1.0), SWR_MAX);
        assert_eq!(swr(1.2), SWR_MAX);
        assert_eq!(swr_clamped(0.99, SWR_CLAMP), SWR_CLAMP);
    }

    #[test]
    fn impedance_and_swr() {
        let z = impedance(C64::new(0.0, 0.0));
        assert!((z.re - 50.0).abs() < 1e-12 && z.im.abs() < 1e-12);
        // Γ for 100 Ω: (100−50)/(100+50) = 1/3
        let z = impedance(C64::new(1.0 / 3.0, 0.0));
        assert!((z.re - 100.0).abs() < 1e-9);
        assert!(impedance(C64::new(1.0, 0.0)).re.is_infinite());
        assert!((swr_clamped(1.0 / 3.0, 20.0) - 2.0).abs() < 1e-12);
        assert_eq!(swr_clamped(1.2, 20.0), 20.0);
    }

    #[test]
    fn compute_respects_applicability_and_clamps() {
        let freqs = grid(3, 1e6, 1e6);
        let s = vec![C64::new(1.0, 0.0); 3]; // ideal open
        assert!(compute(TraceKind::Swr, Param::S21, &freqs, &s, 1).is_empty());
        let rx = compute(TraceKind::Impedance, Param::S11, &freqs, &s, 1);
        assert_eq!(rx.len(), 2);
        assert_eq!(rx[0].values[0], Z_CLAMP);
        assert_eq!(rx[1].values[0], 0.0);
        let db = compute(TraceKind::LogMag, Param::S11, &freqs, &s, 1);
        assert!(db[0].values[0].abs() < 1e-9);
        assert_eq!(TraceKind::parse("R + jX"), Some(TraceKind::Impedance));
        assert_eq!(TraceKind::parse("groupdelay"), Some(TraceKind::GroupDelay));
    }
}
