//! Host-side SOL(T) calibration: the classic one-port 3-term error model
//! (directivity `e00`, source match `e11`, reflection tracking `e10e01`)
//! plus the "enhanced response" transmission terms (isolation `e30`,
//! transmission tracking `e10e32`).
//!
//! Model, per frequency, with `m` = measured and `a` = actual:
//!
//! ```text
//! S11m = e00 + e10e01 · S11a / (1 − e11 · S11a)
//! S21m = e30 + e10e32 · S21a / (1 − e11 · S11a)      (port-2 match assumed ideal)
//! ```
//!
//! Three reflection standards with known `S11a` give three linear equations
//! in `(e00, e11, Δ)` with `Δ = e00·e11 − e10e01`, solved exactly per point.
//! Standards may be non-ideal: see [`StandardModel`].

use std::f64::consts::PI;

use num_complex::Complex64 as C64;
use serde::{Deserialize, Serialize};

use crate::device::SweepResult;
use crate::error::{Error, Result};

/// Reference impedance.
pub const Z0: f64 = 50.0;

/// The five things you can measure during a calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Standard {
    Open,
    Short,
    Load,
    Through,
    Isolation,
}

impl Standard {
    pub const ALL: [Standard; 5] = [Standard::Open, Standard::Short, Standard::Load, Standard::Through, Standard::Isolation];

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "open" | "o" => Standard::Open,
            "short" | "s" => Standard::Short,
            "load" | "l" => Standard::Load,
            "through" | "thru" | "t" => Standard::Through,
            "isolation" | "isol" | "i" => Standard::Isolation,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Standard::Open => "open",
            Standard::Short => "short",
            Standard::Load => "load",
            Standard::Through => "through",
            Standard::Isolation => "isolation",
        }
    }
}

/// What terminates a reflection standard. Polynomials are in Hz:
/// `C(f) = c[0] + c[1]·f + c[2]·f² + c[3]·f³` (farad), likewise `L(f)` (henry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Termination {
    Open { c: [f64; 4] },
    Short { l: [f64; 4] },
    Load { resistance: f64 },
}

/// A reflection standard: a lossless offset line (one-way delay, Z0) in
/// front of a termination. `StandardModel::ideal_*` have zero delay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardModel {
    /// One-way electrical delay of the offset line in seconds.
    pub offset_delay_s: f64,
    pub termination: Termination,
}

impl StandardModel {
    pub fn ideal_open() -> Self {
        StandardModel { offset_delay_s: 0.0, termination: Termination::Open { c: [0.0; 4] } }
    }
    pub fn ideal_short() -> Self {
        StandardModel { offset_delay_s: 0.0, termination: Termination::Short { l: [0.0; 4] } }
    }
    pub fn ideal_load() -> Self {
        StandardModel { offset_delay_s: 0.0, termination: Termination::Load { resistance: Z0 } }
    }

    /// Actual reflection coefficient of this standard at `f_hz`.
    pub fn gamma(&self, f_hz: f64) -> C64 {
        let w = 2.0 * PI * f_hz;
        let poly = |k: &[f64; 4]| k[0] + k[1] * f_hz + k[2] * f_hz * f_hz + k[3] * f_hz * f_hz * f_hz;
        let gamma_t = match &self.termination {
            Termination::Open { c } => {
                let cap = poly(c);
                if cap <= 0.0 || w == 0.0 {
                    C64::new(1.0, 0.0)
                } else {
                    let z = C64::new(0.0, -1.0 / (w * cap));
                    (z - Z0) / (z + Z0)
                }
            }
            Termination::Short { l } => {
                let ind = poly(l);
                if ind <= 0.0 || w == 0.0 {
                    C64::new(-1.0, 0.0)
                } else {
                    let z = C64::new(0.0, w * ind);
                    (z - Z0) / (z + Z0)
                }
            }
            Termination::Load { resistance } => C64::new((resistance - Z0) / (resistance + Z0), 0.0),
        };
        // Round trip through the offset line: Γ = Γt · e^{-j·2·ω·delay}
        gamma_t * C64::from_polar(1.0, -2.0 * w * self.offset_delay_s)
    }
}

/// Definitions of the standards used. [`CalKit::ideal`] is what the
/// NanoVNA/LiteVNA firmware assumes too and is fine up to a few hundred MHz;
/// above that the open's fringing capacitance starts to matter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalKit {
    pub name: String,
    pub open: StandardModel,
    pub short: StandardModel,
    pub load: StandardModel,
    /// One-way delay of the through standard in seconds (0 for a direct connection).
    pub through_delay_s: f64,
}

impl CalKit {
    pub fn ideal() -> Self {
        CalKit {
            name: "ideal".into(),
            open: StandardModel::ideal_open(),
            short: StandardModel::ideal_short(),
            load: StandardModel::ideal_load(),
            through_delay_s: 0.0,
        }
    }

    /// Actual S21 of the through at `f_hz` (lossless delay line).
    pub fn through_s21(&self, f_hz: f64) -> C64 {
        C64::from_polar(1.0, -2.0 * PI * f_hz * self.through_delay_s)
    }
}

/// Raw measurement of one standard, as stored in a calibration file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardMeasurement {
    pub s11: Vec<C64>,
    pub s21: Vec<C64>,
}

/// Everything measured for one calibration, on one frequency grid.
/// Serialises to JSON so a CLI can collect the standards one run at a time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationSet {
    pub freqs_hz: Vec<u64>,
    pub kit: CalKit,
    pub note: Option<String>,
    pub open: Option<StandardMeasurement>,
    pub short: Option<StandardMeasurement>,
    pub load: Option<StandardMeasurement>,
    pub through: Option<StandardMeasurement>,
    pub isolation: Option<StandardMeasurement>,
}

impl CalibrationSet {
    pub fn new(freqs_hz: Vec<u64>, kit: CalKit) -> Self {
        CalibrationSet { freqs_hz, kit, note: None, open: None, short: None, load: None, through: None, isolation: None }
    }

    fn slot(&mut self, standard: Standard) -> &mut Option<StandardMeasurement> {
        match standard {
            Standard::Open => &mut self.open,
            Standard::Short => &mut self.short,
            Standard::Load => &mut self.load,
            Standard::Through => &mut self.through,
            Standard::Isolation => &mut self.isolation,
        }
    }

    pub fn get(&self, standard: Standard) -> Option<&StandardMeasurement> {
        match standard {
            Standard::Open => self.open.as_ref(),
            Standard::Short => self.short.as_ref(),
            Standard::Load => self.load.as_ref(),
            Standard::Through => self.through.as_ref(),
            Standard::Isolation => self.isolation.as_ref(),
        }
    }

    pub fn has(&self, standard: Standard) -> bool {
        self.get(standard).is_some()
    }

    /// Store a sweep taken with `standard` connected. The sweep must be on
    /// exactly this set's frequency grid.
    pub fn set(&mut self, standard: Standard, sweep: &SweepResult) -> Result<()> {
        if sweep.freqs_hz != self.freqs_hz {
            return Err(Error::Calibration(format!(
                "sweep grid ({} pts, {}..{} Hz) differs from calibration grid ({} pts, {}..{} Hz)",
                sweep.freqs_hz.len(),
                sweep.freqs_hz.first().copied().unwrap_or(0),
                sweep.freqs_hz.last().copied().unwrap_or(0),
                self.freqs_hz.len(),
                self.freqs_hz.first().copied().unwrap_or(0),
                self.freqs_hz.last().copied().unwrap_or(0),
            )));
        }
        if sweep.missing_points() > 0 {
            return Err(Error::Calibration(format!("{} points missing in sweep", sweep.missing_points())));
        }
        *self.slot(standard) = Some(StandardMeasurement { s11: sweep.s11.clone(), s21: sweep.s21.clone() });
        Ok(())
    }

    pub fn clear(&mut self, standard: Standard) {
        *self.slot(standard) = None;
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> Result<Self> {
        Ok(serde_json::from_str(text)?)
    }

    /// Solve the error terms. Needs open, short and load; through and
    /// isolation are optional (without a through, S21 is left uncorrected).
    pub fn solve(&self) -> Result<Calibration> {
        let (open, short, load) = match (&self.open, &self.short, &self.load) {
            (Some(o), Some(s), Some(l)) => (o, s, l),
            _ => {
                let missing: Vec<_> = [Standard::Open, Standard::Short, Standard::Load]
                    .iter()
                    .filter(|s| !self.has(**s))
                    .map(|s| s.name())
                    .collect();
                return Err(Error::Calibration(format!("missing standards: {}", missing.join(", "))));
            }
        };
        let n = self.freqs_hz.len();
        let mut terms = Vec::with_capacity(n);
        for i in 0..n {
            let f = self.freqs_hz[i] as f64;
            let standards = [
                (self.kit.open.gamma(f), open.s11[i]),
                (self.kit.short.gamma(f), short.s11[i]),
                (self.kit.load.gamma(f), load.s11[i]),
            ];
            let (e00, e11, delta) = solve_one_port(&standards).ok_or_else(|| {
                Error::Calibration(format!("degenerate standards at {} Hz", self.freqs_hz[i]))
            })?;
            let e10e01 = e00 * e11 - delta;

            let e30 = self.isolation.as_ref().map_or(C64::new(0.0, 0.0), |iso| iso.s21[i]);
            let e10e32 = self.through.as_ref().map(|thru| {
                // Through: S11a = 0, S21a = known delay line, so the
                // denominator (1 − e11·S11a) is 1.
                (thru.s21[i] - e30) / self.kit.through_s21(f)
            });
            terms.push(ErrorTerms { e00, e11, e10e01, e30, e10e32 });
        }
        Ok(Calibration { freqs_hz: self.freqs_hz.clone(), terms })
    }
}

/// Solve `e00 + (Γa·Γm)·e11 − Γa·Δ = Γm` for three standards (Cramer's rule).
fn solve_one_port(standards: &[(C64, C64); 3]) -> Option<(C64, C64, C64)> {
    let one = C64::new(1.0, 0.0);
    let rows: Vec<[C64; 4]> = standards.iter().map(|(ga, gm)| [one, ga * gm, -ga, *gm]).collect();
    let det3 = |m: [[C64; 3]; 3]| -> C64 {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let col = |c: usize| -> [[C64; 3]; 3] {
        let mut m = [[C64::new(0.0, 0.0); 3]; 3];
        for (r, row) in rows.iter().enumerate() {
            for k in 0..3 {
                m[r][k] = if k == c { row[3] } else { row[k] };
            }
        }
        m
    };
    let base = [[rows[0][0], rows[0][1], rows[0][2]], [rows[1][0], rows[1][1], rows[1][2]], [rows[2][0], rows[2][1], rows[2][2]]];
    let d = det3(base);
    if d.norm() < 1e-30 {
        return None;
    }
    Some((det3(col(0)) / d, det3(col(1)) / d, det3(col(2)) / d))
}

/// Error terms for one frequency point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ErrorTerms {
    /// Directivity.
    pub e00: C64,
    /// Source match.
    pub e11: C64,
    /// Reflection tracking.
    pub e10e01: C64,
    /// Isolation (0 when not measured).
    pub e30: C64,
    /// Transmission tracking, `None` when no through was measured.
    pub e10e32: Option<C64>,
}

impl ErrorTerms {
    pub fn correct_s11(&self, s11m: C64) -> C64 {
        let d = s11m - self.e00;
        d / (self.e10e01 + self.e11 * d)
    }

    /// Enhanced-response S21 correction; needs the DUT's corrected S11.
    pub fn correct_s21(&self, s21m: C64, s11a: C64) -> C64 {
        match self.e10e32 {
            Some(e10e32) => (s21m - self.e30) * (C64::new(1.0, 0.0) - self.e11 * s11a) / e10e32,
            None => s21m,
        }
    }
}

/// Solved calibration, ready to apply to sweeps on the same grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub freqs_hz: Vec<u64>,
    pub terms: Vec<ErrorTerms>,
}

impl Calibration {
    pub fn has_through(&self) -> bool {
        self.terms.first().is_some_and(|t| t.e10e32.is_some())
    }

    /// Corrected `(S11, S21)` for point `i`.
    pub fn correct(&self, i: usize, s11m: C64, s21m: C64) -> (C64, C64) {
        let t = &self.terms[i];
        let s11a = t.correct_s11(s11m);
        (s11a, t.correct_s21(s21m, s11a))
    }

    /// A copy of `sweep` with corrected S-parameters. Fails when the grids differ.
    pub fn apply(&self, sweep: &SweepResult) -> Result<SweepResult> {
        if sweep.freqs_hz != self.freqs_hz {
            return Err(Error::Calibration("sweep is not on the calibration's frequency grid".into()));
        }
        let mut out = sweep.clone();
        for i in 0..self.freqs_hz.len() {
            let (s11, s21) = self.correct(i, sweep.s11[i], sweep.s21[i]);
            out.s11[i] = s11;
            out.s21[i] = s21;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sweep_from(freqs: &[u64], s11: Vec<C64>, s21: Vec<C64>) -> SweepResult {
        SweepResult {
            freqs_hz: freqs.to_vec(),
            points: vec![Some(crate::protocol::FifoPoint::parse(&[0u8; 32]).unwrap()); freqs.len()],
            s11,
            s21,
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// Synthetic instrument: fixed error terms that vary a bit with frequency.
    fn terms_at(i: usize) -> ErrorTerms {
        let k = i as f64;
        ErrorTerms {
            e00: C64::new(0.02 + 0.001 * k, -0.015),
            e11: C64::new(-0.05, 0.03 + 0.002 * k),
            e10e01: C64::from_polar(0.8 - 0.01 * k, -0.3 * k),
            e30: C64::new(1e-4, -2e-4),
            e10e32: Some(C64::from_polar(0.9, -0.2 * k)),
        }
    }

    fn measure(t: &ErrorTerms, s11a: C64, s21a: C64) -> (C64, C64) {
        let one = C64::new(1.0, 0.0);
        let denom = one - t.e11 * s11a;
        (t.e00 + t.e10e01 * s11a / denom, t.e30 + t.e10e32.unwrap() * s21a / denom)
    }

    fn measure_standard(freqs: &[u64], kit: &CalKit, std: Standard) -> SweepResult {
        let (mut s11, mut s21) = (Vec::new(), Vec::new());
        for (i, f) in freqs.iter().enumerate() {
            let f = *f as f64;
            let (a11, a21) = match std {
                Standard::Open => (kit.open.gamma(f), C64::new(0.0, 0.0)),
                Standard::Short => (kit.short.gamma(f), C64::new(0.0, 0.0)),
                Standard::Load => (kit.load.gamma(f), C64::new(0.0, 0.0)),
                Standard::Through => (C64::new(0.0, 0.0), kit.through_s21(f)),
                Standard::Isolation => (kit.load.gamma(f), C64::new(0.0, 0.0)),
            };
            let (m11, m21) = measure(&terms_at(i), a11, a21);
            s11.push(m11);
            s21.push(m21);
        }
        sweep_from(freqs, s11, s21)
    }

    fn check_roundtrip(kit: CalKit) {
        let freqs: Vec<u64> = (0..5).map(|i| 100_000_000 + i * 500_000_000).collect();
        let mut set = CalibrationSet::new(freqs.clone(), kit.clone());
        for std in Standard::ALL {
            set.set(std, &measure_standard(&freqs, &kit, std)).unwrap();
        }
        let cal = set.solve().unwrap();
        assert!(cal.has_through());

        // Recovered error terms
        for (i, t) in cal.terms.iter().enumerate() {
            let want = terms_at(i);
            assert!((t.e00 - want.e00).norm() < 1e-9, "e00 at {i}");
            assert!((t.e11 - want.e11).norm() < 1e-9, "e11 at {i}");
            assert!((t.e10e01 - want.e10e01).norm() < 1e-9, "e10e01 at {i}");
        }

        // A DUT: |Γ| 0.3 at 40°, S21 0.5 at -70°
        let dut11 = C64::from_polar(0.3, 40f64.to_radians());
        let dut21 = C64::from_polar(0.5, -70f64.to_radians());
        let (mut m11, mut m21) = (Vec::new(), Vec::new());
        for i in 0..freqs.len() {
            let (a, b) = measure(&terms_at(i), dut11, dut21);
            m11.push(a);
            m21.push(b);
        }
        let corrected = cal.apply(&sweep_from(&freqs, m11, m21)).unwrap();
        for i in 0..freqs.len() {
            assert!((corrected.s11[i] - dut11).norm() < 1e-9, "S11 at {i}: {:?}", corrected.s11[i]);
            assert!((corrected.s21[i] - dut21).norm() < 1e-9, "S21 at {i}: {:?}", corrected.s21[i]);
        }
    }

    #[test]
    fn roundtrip_ideal_kit() {
        check_roundtrip(CalKit::ideal());
    }

    #[test]
    fn roundtrip_non_ideal_kit() {
        let mut kit = CalKit::ideal();
        kit.name = "test kit".into();
        kit.open.termination = Termination::Open { c: [50e-15, 0.0, 0.0, 0.0] };
        kit.open.offset_delay_s = 30e-12;
        kit.short.termination = Termination::Short { l: [2e-12, 0.0, 0.0, 0.0] };
        kit.short.offset_delay_s = 30e-12;
        kit.load.termination = Termination::Load { resistance: 50.3 };
        kit.through_delay_s = 40e-12;
        check_roundtrip(kit);
    }

    #[test]
    fn standard_models_behave() {
        let open = StandardModel { offset_delay_s: 0.0, termination: Termination::Open { c: [50e-15, 0.0, 0.0, 0.0] } };
        let g = open.gamma(1e9);
        assert!((g.norm() - 1.0).abs() < 1e-12);
        assert!(g.arg() < 0.0 && g.arg() > -0.1, "open with C rotates slightly clockwise: {}", g.arg());
        let short = StandardModel { offset_delay_s: 0.0, termination: Termination::Short { l: [1e-9, 0.0, 0.0, 0.0] } };
        let g = short.gamma(1e9);
        assert!((g.norm() - 1.0).abs() < 1e-12 && g.re < 0.0);
        let load = StandardModel::ideal_load();
        assert!(load.gamma(1e9).norm() < 1e-15);
        // 30 ps one-way offset at 1 GHz: round trip 2·2π·1e9·30e-12 ≈ 0.377 rad
        let delayed = StandardModel { offset_delay_s: 30e-12, termination: Termination::Open { c: [0.0; 4] } };
        assert!((delayed.gamma(1e9).arg() + 0.37699).abs() < 1e-4);
    }

    #[test]
    fn json_roundtrip_and_missing_standard() {
        let freqs = vec![1_000_000, 2_000_000];
        let mut set = CalibrationSet::new(freqs.clone(), CalKit::ideal());
        set.set(Standard::Open, &sweep_from(&freqs, vec![C64::new(1.0, 0.0); 2], vec![C64::new(0.0, 0.0); 2])).unwrap();
        let json = set.to_json().unwrap();
        let back = CalibrationSet::from_json(&json).unwrap();
        assert_eq!(back, set);
        let err = back.solve().unwrap_err().to_string();
        assert!(err.contains("short") && err.contains("load"), "{err}");
        // wrong grid rejected
        assert!(set.set(Standard::Short, &sweep_from(&[1, 2], vec![C64::new(0.0, 0.0); 2], vec![C64::new(0.0, 0.0); 2])).is_err());
    }
}
