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
    /// Open standard at the far end of a line (cable/adapter), for kit fitting.
    LineOpen,
    /// Short standard at the far end of the same line.
    LineShort,
}

impl Standard {
    /// The calibration standards proper.
    pub const ALL: [Standard; 5] = [
        Standard::Open,
        Standard::Short,
        Standard::Load,
        Standard::Through,
        Standard::Isolation,
    ];
    /// Auxiliary measurements used by [`CalibrationSet::fit_open_capacitance`].
    pub const LINE: [Standard; 2] = [Standard::LineOpen, Standard::LineShort];

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "open" | "o" => Standard::Open,
            "short" | "s" => Standard::Short,
            "load" | "l" => Standard::Load,
            "through" | "thru" | "t" => Standard::Through,
            "isolation" | "isol" | "i" => Standard::Isolation,
            "line-open" | "line_open" | "lineopen" => Standard::LineOpen,
            "line-short" | "line_short" | "lineshort" => Standard::LineShort,
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
            Standard::LineOpen => "line-open",
            Standard::LineShort => "line-short",
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
        StandardModel {
            offset_delay_s: 0.0,
            termination: Termination::Open { c: [0.0; 4] },
        }
    }
    pub fn ideal_short() -> Self {
        StandardModel {
            offset_delay_s: 0.0,
            termination: Termination::Short { l: [0.0; 4] },
        }
    }
    pub fn ideal_load() -> Self {
        StandardModel {
            offset_delay_s: 0.0,
            termination: Termination::Load { resistance: Z0 },
        }
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

    /// Constant fringing capacitance of the open (the `c[0]` term), in farad.
    pub fn open_c0(&self) -> f64 {
        match &self.open.termination {
            Termination::Open { c } => c[0],
            _ => 0.0,
        }
    }

    /// Set the open's constant fringing capacitance (farad), keeping higher-order terms.
    pub fn set_open_c0(&mut self, c0: f64) {
        self.open.termination = match &self.open.termination {
            Termination::Open { c } => Termination::Open { c: [c0, c[1], c[2], c[3]] },
            _ => Termination::Open { c: [c0, 0.0, 0.0, 0.0] },
        };
    }

    pub fn short_l0(&self) -> f64 {
        match &self.short.termination {
            Termination::Short { l } => l[0],
            _ => 0.0,
        }
    }

    pub fn set_short_l0(&mut self, l0: f64) {
        self.short.termination = match &self.short.termination {
            Termination::Short { l } => Termination::Short { l: [l0, l[1], l[2], l[3]] },
            _ => Termination::Short { l: [l0, 0.0, 0.0, 0.0] },
        };
    }

    pub fn load_r(&self) -> f64 {
        match &self.load.termination {
            Termination::Load { resistance } => *resistance,
            _ => Z0,
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
    #[serde(default)]
    pub line_open: Option<StandardMeasurement>,
    #[serde(default)]
    pub line_short: Option<StandardMeasurement>,
}

impl CalibrationSet {
    pub fn new(freqs_hz: Vec<u64>, kit: CalKit) -> Self {
        CalibrationSet {
            freqs_hz,
            kit,
            note: None,
            open: None,
            short: None,
            load: None,
            through: None,
            isolation: None,
            line_open: None,
            line_short: None,
        }
    }

    fn slot(&mut self, standard: Standard) -> &mut Option<StandardMeasurement> {
        match standard {
            Standard::Open => &mut self.open,
            Standard::Short => &mut self.short,
            Standard::Load => &mut self.load,
            Standard::Through => &mut self.through,
            Standard::Isolation => &mut self.isolation,
            Standard::LineOpen => &mut self.line_open,
            Standard::LineShort => &mut self.line_short,
        }
    }

    pub fn get(&self, standard: Standard) -> Option<&StandardMeasurement> {
        match standard {
            Standard::Open => self.open.as_ref(),
            Standard::Short => self.short.as_ref(),
            Standard::Load => self.load.as_ref(),
            Standard::Through => self.through.as_ref(),
            Standard::Isolation => self.isolation.as_ref(),
            Standard::LineOpen => self.line_open.as_ref(),
            Standard::LineShort => self.line_short.as_ref(),
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
        *self.slot(standard) = Some(StandardMeasurement {
            s11: sweep.s11.clone(),
            s21: sweep.s21.clone(),
        });
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
            let (e00, e11, delta) =
                solve_one_port(&standards).ok_or_else(|| Error::Calibration(format!("degenerate standards at {} Hz", self.freqs_hz[i])))?;
            let e10e01 = e00 * e11 - delta;

            let e30 = self.isolation.as_ref().map_or(C64::new(0.0, 0.0), |iso| iso.s21[i]);
            let e10e32 = self.through.as_ref().map(|thru| {
                // Through: S11a = 0, S21a = known delay line, so the
                // denominator (1 − e11·S11a) is 1.
                (thru.s21[i] - e30) / self.kit.through_s21(f)
            });
            terms.push(ErrorTerms {
                e00,
                e11,
                e10e01,
                e30,
                e10e32,
            });
        }
        Ok(Calibration {
            freqs_hz: self.freqs_hz.clone(),
            terms,
        })
    }
}

/// Largest phase step (radians) between adjacent points that linear
/// interpolation in re/im can still follow without ambiguity.
pub const MAX_INTERPOLATION_PHASE_STEP: f64 = PI / 2.0;
/// See [`CalibrationSet::coarse_low_end_warning`].
pub const COARSE_LOW_END_RATIO: f64 = 2.0;

impl CalibrationSet {
    /// Resample every stored standard onto `freqs_hz` by linear interpolation
    /// of the complex values (real and imaginary parts separately).
    ///
    /// Fails when `freqs_hz` reaches outside the calibrated span, or when the
    /// calibration grid is so coarse that the open's phase turns more than
    /// 90° between neighbouring points (then the interpolation is ambiguous).
    pub fn interpolate(&self, freqs_hz: &[u64]) -> Result<CalibrationSet> {
        if freqs_hz.is_empty() {
            return Err(Error::Calibration("empty target grid".into()));
        }
        let (lo, hi) = (self.freqs_hz[0], *self.freqs_hz.last().unwrap());
        let (tlo, thi) = (*freqs_hz.iter().min().unwrap(), *freqs_hz.iter().max().unwrap());
        if tlo < lo || thi > hi {
            return Err(Error::Calibration(format!(
                "target grid {}..{} Hz lies outside the calibrated span {}..{} Hz",
                tlo, thi, lo, hi
            )));
        }
        if let Some(open) = &self.open {
            let step = max_phase_step(&open.s11);
            if step > MAX_INTERPOLATION_PHASE_STEP {
                return Err(Error::Calibration(format!(
                    "calibration grid too coarse to interpolate: open phase jumps {:.0}° between points",
                    step.to_degrees()
                )));
            }
        }
        let resample = |m: &StandardMeasurement| StandardMeasurement {
            s11: interpolate_complex(&self.freqs_hz, &m.s11, freqs_hz),
            s21: interpolate_complex(&self.freqs_hz, &m.s21, freqs_hz),
        };
        Ok(CalibrationSet {
            freqs_hz: freqs_hz.to_vec(),
            kit: self.kit.clone(),
            note: self.note.clone(),
            open: self.open.as_ref().map(resample),
            short: self.short.as_ref().map(resample),
            load: self.load.as_ref().map(resample),
            through: self.through.as_ref().map(resample),
            isolation: self.isolation.as_ref().map(resample),
            line_open: self.line_open.as_ref().map(resample),
            line_short: self.line_short.as_ref().map(resample),
        })
    }

    /// Warn when `freqs_hz` has points below this set's second grid point
    /// while that point lies more than [`COARSE_LOW_END_RATIO`] times above
    /// the first. The VNA's error terms change quickly at the low end
    /// (coupling capacitors, bridge), so a linear interpolation across such
    /// a wide relative gap distorts everything below the second point. A
    /// 50 kHz–6.3 GHz calibration in 1001 points, for example, has its second
    /// point at 6.35 MHz and is useless for an HF sweep. Returns `None` when
    /// the grids are equal or the gap is fine.
    pub fn coarse_low_end_warning(&self, freqs_hz: &[u64]) -> Option<String> {
        if self.freqs_hz.len() < 2 || freqs_hz == self.freqs_hz.as_slice() {
            return None;
        }
        let (f0, f1) = (self.freqs_hz[0], self.freqs_hz[1]);
        let below = freqs_hz.iter().filter(|&&f| f > f0 && f < f1).count();
        if below == 0 || (f1 as f64) <= COARSE_LOW_END_RATIO * (f0 as f64) {
            return None;
        }
        Some(format!(
            "{} sweep points lie between the first two calibration points ({} and {}); the error terms are \
             interpolated over that gap and results below {} are unreliable. Calibrate on this grid instead.",
            below,
            crate::freq::format_hz(f0),
            crate::freq::format_hz(f1),
            crate::freq::format_hz(f1)
        ))
    }

    /// `solve()`, interpolating first when `freqs_hz` differs from this set's grid.
    pub fn solve_for(&self, freqs_hz: &[u64]) -> Result<Calibration> {
        if freqs_hz == self.freqs_hz.as_slice() {
            self.solve()
        } else {
            self.interpolate(freqs_hz)?.solve()
        }
    }
}

/// Largest absolute phase difference between neighbouring values.
fn max_phase_step(values: &[C64]) -> f64 {
    values
        .windows(2)
        .map(|w| {
            let d = (w[1].arg() - w[0].arg()).abs();
            d.min(2.0 * PI - d)
        })
        .fold(0.0, f64::max)
}

/// Piecewise-linear interpolation of complex samples `ys` at `xs` (sorted,
/// strictly increasing) onto `targets` (each within `xs`' range).
fn interpolate_complex(xs: &[u64], ys: &[C64], targets: &[u64]) -> Vec<C64> {
    targets
        .iter()
        .map(|&t| {
            // first index with xs[i] >= t
            let i = xs.partition_point(|&x| x < t);
            if i == 0 {
                return ys[0];
            }
            if i >= xs.len() {
                return ys[xs.len() - 1];
            }
            if xs[i] == t {
                return ys[i];
            }
            let (x0, x1) = (xs[i - 1] as f64, xs[i] as f64);
            let w = (t as f64 - x0) / (x1 - x0);
            ys[i - 1] * (1.0 - w) + ys[i] * w
        })
        .collect()
}

/// How to express the open/short asymmetry found by [`CalibrationSet::fit_open`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenModel {
    /// Fringing capacitance `c[0]` of the open (NanoVNA convention).
    Capacitance,
    /// One-way offset delay of the open (recessed pin, e.g. N female).
    Delay,
}

/// Result of [`CalibrationSet::fit_open_capacitance`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenFit {
    /// Capacitance to **add** to the kit's open `c[0]` (farad). Negative means
    /// the short is "more inductive" than the open is capacitive.
    pub delta_c_farad: f64,
    /// The same asymmetry expressed as extra one-way offset delay of the open
    /// (seconds): `ε = ω·τ` instead of `ε ≈ ω·C·Z0`. Physically more apt for
    /// connectors whose open pin sits recessed (N female). Only one of the two
    /// should be applied.
    pub delta_delay_s: f64,
    pub points_used: usize,
    /// RMS of the open/short magnitude asymmetry before and after the fit
    /// (dimensionless, ≈ radians of phase asymmetry).
    pub ripple_rms_before: f64,
    pub ripple_rms_after: f64,
}

impl CalibrationSet {
    /// Estimate the open standard's fringing capacitance from the line-open
    /// and line-short measurements, relative to the kit currently in this set.
    ///
    /// Theory: a one-port calibration can only fix the standards *relative* to
    /// each other. If the real open lags by φ (capacitance) and the real short
    /// by ψ (inductance), calibrating with ideal definitions leaves a residual
    /// bilinear distortion that is invisible at the port but shows up on a
    /// line: the corrected magnitude of an open-ended line ripples as
    /// `1 − ε·sin(u)` with the line phase `u`, a short-ended line as
    /// `1 + ε·sin(u)`, where `ε = (φ − ψ)/2 ≈ ω·(C·Z0 − L/Z0)`. The ratio of
    /// the two cancels cable loss, so `(1 − |Γo|/|Γs|)/2 ≈ ε·sin(u)` and a
    /// least-squares fit of that against `ω·sin(arg Γo)` yields
    /// `k = C·Z0 − L/Z0`. Attributing everything to the open (the usual
    /// convention, L = 0) gives `C = k / Z0`.
    ///
    /// Only points at or above `f_min_hz` are used; below ~300 MHz the ripple
    /// drowns in noise.
    pub fn fit_open_capacitance(&self, f_min_hz: u64) -> Result<OpenFit> {
        self.fit_open(f_min_hz, OpenModel::Capacitance)
    }

    /// Like [`Self::fit_open_capacitance`], iterating in the chosen model so the
    /// returned correction is exact for that model (the other field is the
    /// small-angle equivalent).
    pub fn fit_open(&self, f_min_hz: u64, model: OpenModel) -> Result<OpenFit> {
        if self.line_open.is_none() || self.line_short.is_none() {
            return Err(Error::Calibration("need line-open and line-short measurements".into()));
        }
        // The first-order estimate is ~2 % low at large asymmetry; iterating
        // (apply, re-solve, re-fit) converges on the exact value in a few steps.
        let mut work = self.clone();
        let mut total_k = 0.0;
        let mut first: Option<(f64, usize)> = None;
        let mut last_rms = 0.0;
        for _ in 0..10 {
            let (k, n, rms_before, rms_after) = work.fit_step(f_min_hz)?;
            if first.is_none() {
                first = Some((rms_before, n));
            }
            last_rms = rms_after;
            total_k += k;
            match model {
                OpenModel::Capacitance => work.kit.set_open_c0(work.kit.open_c0() + k / Z0),
                OpenModel::Delay => work.kit.open.offset_delay_s += k,
            }
            if k.abs() < 1e-15 {
                break;
            }
        }
        let (rms_before, n) = first.unwrap_or((0.0, 0));
        Ok(OpenFit {
            delta_c_farad: total_k / Z0,
            delta_delay_s: total_k,
            points_used: n,
            ripple_rms_before: rms_before,
            ripple_rms_after: last_rms,
        })
    }

    /// One least-squares pass: returns `(k, points, rms_before, rms_after)`.
    fn fit_step(&self, f_min_hz: u64) -> Result<(f64, usize, f64, f64)> {
        let (lo, ls) = (self.line_open.as_ref().unwrap(), self.line_short.as_ref().unwrap());
        let cal = self.solve()?;
        let (mut sxy, mut sxx, mut syy, mut n) = (0.0f64, 0.0f64, 0.0f64, 0usize);
        let mut samples: Vec<(f64, f64)> = Vec::new();
        for (i, &f) in self.freqs_hz.iter().enumerate() {
            if f < f_min_hz {
                continue;
            }
            let go = cal.terms[i].correct_s11(lo.s11[i]);
            let gs = cal.terms[i].correct_s11(ls.s11[i]);
            let r = (go.norm() + gs.norm()) / 2.0; // line loss, ripple cancels to first order
            if r < 0.05 {
                continue;
            }
            // Ripple amplitude scales with |Γ| of the lossy line: normalise it out.
            let y = (1.0 - go.norm() / gs.norm()) / (2.0 * r);
            let x = 2.0 * PI * f as f64 * go.arg().sin();
            sxy += x * y;
            sxx += x * x;
            syy += y * y;
            n += 1;
            samples.push((x, y));
        }
        if n < 10 || sxx <= 0.0 {
            return Err(Error::Calibration(format!("not enough usable points above {f_min_hz} Hz ({n})")));
        }
        let k = sxy / sxx;
        let after: f64 = samples.iter().map(|(x, y)| (y - k * x).powi(2)).sum::<f64>() / n as f64;
        Ok((k, n, (syy / n as f64).sqrt(), after.sqrt()))
    }
}

impl CalibrationSet {
    /// Store the asymmetry found by [`Self::fit_open`] in this set's kit, in
    /// the model the fit was made for. A kit still called "ideal" is renamed
    /// "fitted".
    pub fn apply_open_fit(&mut self, fit: &OpenFit, model: OpenModel) {
        match model {
            OpenModel::Capacitance => self.kit.set_open_c0(self.kit.open_c0() + fit.delta_c_farad),
            OpenModel::Delay => self.kit.open.offset_delay_s += fit.delta_delay_s,
        }
        if self.kit.name == "ideal" {
            self.kit.name = "fitted".into();
        }
    }
}

/// Sanity check of a raw standard measurement before it is stored: a loose
/// open or a forgotten load produces a calibration that still "solves" but
/// gives nonsense. Returns a human-readable warning, or `None` if the
/// measurement looks like the standard it claims to be.
pub fn plausibility_warning(standard: Standard, sweep: &SweepResult) -> Option<String> {
    let median = |mut v: Vec<f64>| -> f64 {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    if sweep.s11.is_empty() {
        return None;
    }
    let db = crate::touchstone::db;
    let s11_db = median(sweep.s11.iter().map(|s| db(s.norm())).collect());
    let s21_db = median(sweep.s21.iter().map(|s| db(s.norm())).collect());
    match standard {
        Standard::Open | Standard::Short if s11_db < -3.0 => Some(format!(
            "median raw |S11| is {s11_db:.1} dB; an {} should reflect nearly everything (≈ 0 dB). Loose connector?",
            standard.name()
        )),
        Standard::Load if s11_db > -10.0 => Some(format!(
            "median raw |S11| is {s11_db:.1} dB; a load should be well below -10 dB. Is the load connected?"
        )),
        Standard::Through if s21_db < -6.0 => Some(format!(
            "median raw |S21| is {s21_db:.1} dB; a through should pass nearly everything. Cable connected to port 2?"
        )),
        Standard::Isolation if s21_db > -40.0 => Some(format!("median raw |S21| is {s21_db:.1} dB; isolation needs loads on both ports.")),
        Standard::LineOpen | Standard::LineShort if s11_db < -6.0 => Some(format!(
            "median raw |S11| is {s11_db:.1} dB; the far-end {} should still reflect most of the signal.",
            if standard == Standard::LineOpen { "open" } else { "short" }
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Line diagnostics (roadmap step 4)
// ---------------------------------------------------------------------------

/// Statistics of the corrected line-open / line-short over one frequency band.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBand {
    pub f_lo_hz: f64,
    pub f_hi_hz: f64,
    /// RMS ripple of linear |Γ| around a quadratic trend, open- and short-ended line.
    pub ripple_open: f64,
    pub ripple_short: f64,
    /// The same after a band-pass gate around the far-end reflection: what the
    /// calibration leaves on the line itself (≈ 0 when the error terms are smooth).
    pub gated_open: f64,
    pub gated_short: f64,
    /// Mean |X| of the near-end term in this band (linear).
    pub near_end_mag: f64,
}

/// Result of [`CalibrationSet::check_lines`].
#[derive(Debug, Clone, PartialEq)]
pub struct LineCheck {
    pub bands: Vec<LineBand>,
    /// Round-trip time of the far-end reflection found on the line-short (s).
    pub far_end_s: f64,
    /// Near-end term X(f) (gated around t = 0, averaged over open and short
    /// lines) on the set's grid: the sum of residual directivity (a load
    /// definition error) and any physical discontinuity within the gate
    /// (the cable's own connector). The two are indistinguishable here.
    pub near_end: Vec<C64>,
    /// RMS difference between the open- and short-line near-end terms; the
    /// model predicts the same X for both, so this is a consistency check.
    pub near_end_open_short_rms: f64,
    /// Open/short asymmetry re-fitted on the far-end-gated lines, as ΔC
    /// (farad): ≈ 0 means the kit fit is consistent with the gated data.
    pub gated_refit_delta_c_farad: f64,
    /// Band-edge width the gates blur (Hz); bands are kept inside it.
    pub edge_hz: f64,
}

/// RMS deviation of `|g|` from a quadratic trend over the points with `f_min ≤ f ≤ f_max`.
fn ripple_quadratic(f: &[f64], g: &[C64], f_min: f64, f_max: f64) -> f64 {
    let pts: Vec<(f64, f64)> = f
        .iter()
        .zip(g)
        .filter(|(x, _)| **x >= f_min && **x <= f_max)
        .map(|(x, c)| ((x - f_min) / (f_max - f_min), c.norm()))
        .collect();
    if pts.len() < 6 {
        return 0.0;
    }
    let n = pts.len() as f64;
    let (mut sx, mut sx2, mut sx3, mut sx4, mut sy, mut sxy, mut sx2y) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (x, y) in &pts {
        sx += x;
        sx2 += x * x;
        sx3 += x * x * x;
        sx4 += x * x * x * x;
        sy += y;
        sxy += x * y;
        sx2y += x * x * y;
    }
    let m = [[n, sx, sx2], [sx, sx2, sx3], [sx2, sx3, sx4]];
    let v = [sy, sxy, sx2y];
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-30 {
        return 0.0;
    }
    let coef = |k: usize| {
        let mut mm = m;
        for (r, row) in mm.iter_mut().enumerate() {
            row[k] = v[r];
        }
        det(mm) / d
    };
    let (a, b, c) = (coef(0), coef(1), coef(2));
    let ss: f64 = pts.iter().map(|(x, y)| (y - a - b * x - c * x * x).powi(2)).sum();
    (ss / n).sqrt()
}

impl CalibrationSet {
    /// Diagnose what the calibration leaves on the line-open / line-short
    /// measurements, using time gating (see docs/03-calibratie.md §9).
    ///
    /// * `bands`: ripple per `band_hz`-wide band, raw and with everything but
    ///   the far-end reflection gated away;
    /// * `near_end`: the term at t ≈ 0 of the corrected lines. A load
    ///   definition error and the cable's input connector both land here and
    ///   cannot be told apart from these measurements alone; flipping the
    ///   cable changes the connector part but not the load part.
    ///
    /// Needs line-open and line-short on a uniform grid of ≥ 8 points.
    pub fn check_lines(&self, band_hz: f64) -> Result<LineCheck> {
        use crate::timedomain::{gate, tdr, Gate, GateKind, TdrConfig};
        let (lo_m, ls_m) = match (&self.line_open, &self.line_short) {
            (Some(o), Some(s)) => (o, s),
            _ => return Err(Error::Calibration("need line-open and line-short measurements".into())),
        };
        let cal = self.solve()?;
        let n = self.freqs_hz.len();
        let f: Vec<f64> = self.freqs_hz.iter().map(|&x| x as f64).collect();
        let correct = |m: &[C64]| -> Vec<C64> { (0..n).map(|i| cal.terms[i].correct_s11(m[i])).collect() };
        let (lo, ls) = (correct(&lo_m.s11), correct(&ls_m.s11));

        let cfg = TdrConfig::default();
        let t = tdr(&self.freqs_hz, &ls, &cfg)?;
        let dt = t.time_s[1] - t.time_s[0];
        let min_sep = ((t.resolution_s / dt).ceil() as usize).max(2);
        // Strongest reflection beyond one resolution from the plane = the far end.
        let far_bin = t
            .peaks(8, min_sep)
            .into_iter()
            .find(|(i, _)| t.time_s[*i] > 2.0 * t.resolution_s)
            .map(|(i, _)| i)
            .ok_or_else(|| Error::Calibration("no far-end reflection found on the line-short".into()))?;
        let far_s = t.time_s[far_bin];
        let res = t.resolution_s;
        let near = Gate {
            start_s: -2.5 * res,
            stop_s: (far_s / 3.0).min(10.0 * res),
            kind: GateKind::BandPass,
        };
        let far = Gate {
            start_s: far_s - 8.0 * res,
            stop_s: far_s + 8.0 * res,
            kind: GateKind::BandPass,
        };
        let (xo, xs) = (gate(&self.freqs_hz, &lo, &cfg, &near)?, gate(&self.freqs_hz, &ls, &cfg, &near)?);
        let (go, gs) = (gate(&self.freqs_hz, &lo, &cfg, &far)?, gate(&self.freqs_hz, &ls, &cfg, &far)?);
        let edge_hz = xo.edge_hz.max(go.edge_hz);
        let near_end: Vec<C64> = (0..n).map(|i| (xo.s11[i] + xs.s11[i]) / 2.0).collect();

        // Stay clear of the gate blur and of the band edges where the window
        // compensation is clamped (there the gated trace is not trustworthy).
        let clamped = xo.clamped_points.max(go.clamped_points).min(n / 2 - 1);
        let f_first = (f[0] + edge_hz).max(f[clamped]);
        let f_last = (f[n - 1] - edge_hz).min(f[n - 1 - clamped]);
        let mut bands = Vec::new();
        let mut lo_f = (f_first / band_hz).ceil() * band_hz;
        if lo_f - f_first > band_hz / 2.0 {
            lo_f -= band_hz; // start half a band earlier if that still clears the edge
            lo_f = lo_f.max(f_first);
        }
        while lo_f + band_hz / 2.0 < f_last {
            let hi_f = (lo_f + band_hz).min(f_last);
            let mag: Vec<f64> = f
                .iter()
                .zip(&near_end)
                .filter(|(x, _)| **x >= lo_f && **x <= hi_f)
                .map(|(_, c)| c.norm())
                .collect();
            let near_mag = if mag.is_empty() {
                0.0
            } else {
                mag.iter().sum::<f64>() / mag.len() as f64
            };
            bands.push(LineBand {
                f_lo_hz: lo_f,
                f_hi_hz: hi_f,
                ripple_open: ripple_quadratic(&f, &lo, lo_f, hi_f),
                ripple_short: ripple_quadratic(&f, &ls, lo_f, hi_f),
                gated_open: ripple_quadratic(&f, &go.s11, lo_f, hi_f),
                gated_short: ripple_quadratic(&f, &gs.s11, lo_f, hi_f),
                near_end_mag: near_mag,
            });
            lo_f += band_hz;
        }

        let (mut d, mut c) = (0.0, 0usize);
        for i in 0..n {
            if f[i] >= f_first && f[i] <= f_last {
                d += (xo.s11[i] - xs.s11[i]).norm_sqr();
                c += 1;
            }
        }
        let near_end_open_short_rms = if c > 0 { (d / c as f64).sqrt() } else { 0.0 };

        // Asymmetry re-fit on the gated lines (same estimator as fit_step).
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for i in 0..n {
            if f[i] < f_first.max(300e6) || f[i] > f_last {
                continue;
            }
            let (o, s_) = (go.s11[i], gs.s11[i]);
            let r = (o.norm() + s_.norm()) / 2.0;
            if r < 0.05 {
                continue;
            }
            let y = (1.0 - o.norm() / s_.norm()) / (2.0 * r);
            let x = 2.0 * PI * f[i] * o.arg().sin();
            sxy += x * y;
            sxx += x * x;
        }
        let k = if sxx > 0.0 { sxy / sxx } else { 0.0 };

        Ok(LineCheck {
            bands,
            far_end_s: far_s,
            near_end,
            near_end_open_short_rms,
            gated_refit_delta_c_farad: k / Z0,
            edge_hz,
        })
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
    let base = [
        [rows[0][0], rows[0][1], rows[0][2]],
        [rows[1][0], rows[1][1], rows[1][2]],
        [rows[2][0], rows[2][1], rows[2][2]],
    ];
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

    /// Synthetic 50 kHz–6.3 GHz set: perfect instrument, ideal standards, and
    /// line measurements through a connector D = 0.03j in front of a lossy
    /// 5 ns (round trip) line. check_lines must find the far end, the
    /// connector as near-end term, ripple that the far-end gate removes, and
    /// no asymmetry.
    #[test]
    fn check_lines_finds_connector_and_clean_far_end() {
        let freqs: Vec<u64> = (0..1001).map(|i| 50_000 + i * 6_299_950).collect();
        let mut set = CalibrationSet::new(freqs.clone(), CalKit::ideal());
        let n = freqs.len();
        let meas = |g: Vec<C64>| StandardMeasurement {
            s21: vec![C64::new(0.0, 0.0); n],
            s11: g,
        };
        set.open = Some(meas(vec![C64::new(1.0, 0.0); n]));
        set.short = Some(meas(vec![C64::new(-1.0, 0.0); n]));
        set.load = Some(meas(vec![C64::new(0.0, 0.0); n]));
        let d = C64::new(0.0, 0.03);
        let line = |sign: f64| -> Vec<C64> {
            freqs
                .iter()
                .map(|&f| {
                    let e = sign * 0.85 * C64::from_polar(1.0, -2.0 * PI * f as f64 * 5e-9);
                    (d + e) / (C64::new(1.0, 0.0) + d * e)
                })
                .collect()
        };
        set.line_open = Some(meas(line(1.0)));
        set.line_short = Some(meas(line(-1.0)));
        let chk = set.check_lines(1e9).unwrap();
        assert!((chk.far_end_s - 5e-9).abs() < 0.2e-9, "far end {}", chk.far_end_s);
        assert!(chk.bands.len() >= 4);
        for b in &chk.bands {
            assert!((b.near_end_mag - 0.03).abs() < 0.002, "near end {}", b.near_end_mag);
            // imaginary D on a 0.85 line: ripple ≈ 0.03·1.72/√2 = 0.036
            assert!(b.ripple_short > 0.025 && b.ripple_short < 0.045, "ripple {}", b.ripple_short);
            assert!(
                b.gated_short < 0.003 && b.gated_open < 0.003,
                "gated {} {}",
                b.gated_open,
                b.gated_short
            );
        }
        assert!(chk.near_end_open_short_rms < 0.002);
        assert!(chk.gated_refit_delta_c_farad.abs() < 1e-15);
    }

    #[test]
    fn coarse_low_end_is_flagged() {
        let wide: Vec<u64> = (0..1001).map(|i| 50_000 + i * 6_300_000).collect();
        let set = CalibrationSet::new(wide, CalKit::ideal());
        let hf: Vec<u64> = (0..1001).map(|i| 100_000 + i * 49_900).collect();
        assert!(set.coarse_low_end_warning(&hf).is_some());
        let uhf: Vec<u64> = (0..101).map(|i| 400_000_000 + i * 1_000_000).collect();
        assert!(set.coarse_low_end_warning(&uhf).is_none());
        assert!(set.coarse_low_end_warning(&set.freqs_hz.clone()).is_none());
        // 1 MHz..1 GHz in 201 points: second point 6 MHz, ratio 6 → flagged for an HF sweep too
        let g1: Vec<u64> = (0..201).map(|i| 1_000_000 + i * 4_995_000).collect();
        let set1 = CalibrationSet::new(g1, CalKit::ideal());
        assert!(set1.coarse_low_end_warning(&hf[20..]).is_some());
        // a dense HF calibration is fine
        let dense: Vec<u64> = (0..1001).map(|i| 100_000 + i * 49_900).collect();
        assert!(CalibrationSet::new(dense, CalKit::ideal())
            .coarse_low_end_warning(&hf[1..500])
            .is_none());
    }
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
                Standard::Isolation | Standard::LineOpen | Standard::LineShort => (kit.load.gamma(f), C64::new(0.0, 0.0)),
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
        kit.open.termination = Termination::Open {
            c: [50e-15, 0.0, 0.0, 0.0],
        };
        kit.open.offset_delay_s = 30e-12;
        kit.short.termination = Termination::Short { l: [2e-12, 0.0, 0.0, 0.0] };
        kit.short.offset_delay_s = 30e-12;
        kit.load.termination = Termination::Load { resistance: 50.3 };
        kit.through_delay_s = 40e-12;
        check_roundtrip(kit);
    }

    #[test]
    fn standard_models_behave() {
        let open = StandardModel {
            offset_delay_s: 0.0,
            termination: Termination::Open {
                c: [50e-15, 0.0, 0.0, 0.0],
            },
        };
        let g = open.gamma(1e9);
        assert!((g.norm() - 1.0).abs() < 1e-12);
        assert!(
            g.arg() < 0.0 && g.arg() > -0.1,
            "open with C rotates slightly clockwise: {}",
            g.arg()
        );
        let short = StandardModel {
            offset_delay_s: 0.0,
            termination: Termination::Short { l: [1e-9, 0.0, 0.0, 0.0] },
        };
        let g = short.gamma(1e9);
        assert!((g.norm() - 1.0).abs() < 1e-12 && g.re < 0.0);
        let load = StandardModel::ideal_load();
        assert!(load.gamma(1e9).norm() < 1e-15);
        // 30 ps one-way offset at 1 GHz: round trip 2·2π·1e9·30e-12 ≈ 0.377 rad
        let delayed = StandardModel {
            offset_delay_s: 30e-12,
            termination: Termination::Open { c: [0.0; 4] },
        };
        assert!((delayed.gamma(1e9).arg() + 0.37699).abs() < 1e-4);
    }

    /// Smooth, frequency-dependent error terms (for interpolation tests).
    fn terms_at_freq(f: f64) -> ErrorTerms {
        let g = f / 1e9;
        ErrorTerms {
            e00: C64::new(0.02 + 0.01 * g, -0.015 + 0.005 * g * g),
            e11: C64::new(-0.05 + 0.02 * g, 0.03 - 0.01 * g),
            e10e01: C64::from_polar(0.8 - 0.1 * g, -1.5 * g),
            e30: C64::new(1e-4, -2e-4),
            e10e32: Some(C64::from_polar(0.9 - 0.05 * g, -1.2 * g)),
        }
    }

    fn standards_on(freqs: &[u64], kit: &CalKit) -> CalibrationSet {
        let mut set = CalibrationSet::new(freqs.to_vec(), kit.clone());
        for std in Standard::ALL {
            let (mut s11, mut s21) = (Vec::new(), Vec::new());
            for &f in freqs {
                let fh = f as f64;
                let (a11, a21) = match std {
                    Standard::Open => (kit.open.gamma(fh), C64::new(0.0, 0.0)),
                    Standard::Short => (kit.short.gamma(fh), C64::new(0.0, 0.0)),
                    Standard::Load => (kit.load.gamma(fh), C64::new(0.0, 0.0)),
                    Standard::Through => (C64::new(0.0, 0.0), kit.through_s21(fh)),
                    Standard::Isolation | Standard::LineOpen | Standard::LineShort => (kit.load.gamma(fh), C64::new(0.0, 0.0)),
                };
                let (m11, m21) = measure(&terms_at_freq(fh), a11, a21);
                s11.push(m11);
                s21.push(m21);
            }
            set.set(std, &sweep_from(freqs, s11, s21)).unwrap();
        }
        set
    }

    #[test]
    fn interpolation_tracks_smooth_terms() {
        let kit = CalKit::ideal();
        let coarse: Vec<u64> = (0..=100).map(|i| 10_000_000 + i * 9_900_000).collect(); // 10 MHz..1 GHz
        let set = standards_on(&coarse, &kit);
        // Fine grid, offset so no point coincides with the coarse grid.
        let fine: Vec<u64> = (0..=400).map(|i| 123_456_789 + i * 1_234_567).collect();
        let cal = set.solve_for(&fine).unwrap();
        assert_eq!(cal.freqs_hz, fine);

        let dut11 = C64::from_polar(0.4, 1.0);
        let dut21 = C64::from_polar(0.6, -2.0);
        let mut worst = 0.0f64;
        for (i, &f) in fine.iter().enumerate() {
            let (m11, m21) = measure(&terms_at_freq(f as f64), dut11, dut21);
            let (a11, a21) = cal.correct(i, m11, m21);
            worst = worst.max((a11 - dut11).norm()).max((a21 - dut21).norm());
        }
        assert!(worst < 2e-4, "interpolation error {worst}");

        // Exact grid points reproduce the direct solution.
        let direct = set.solve().unwrap();
        let via = set.solve_for(&coarse).unwrap();
        assert_eq!(direct, via);
    }

    #[test]
    fn interpolation_rejects_out_of_range_and_coarse_grids() {
        let kit = CalKit::ideal();
        let coarse: Vec<u64> = (0..=10).map(|i| 100_000_000 + i * 90_000_000).collect();
        let set = standards_on(&coarse, &kit);
        assert!(set.interpolate(&[50_000_000]).is_err());
        assert!(set.interpolate(&[1_500_000_000]).is_err());
        // Open phase turns 1.5 rad/GHz · 0.09 GHz ≈ 7.7° per step here: fine.
        assert!(set.interpolate(&[150_000_000, 500_000_000]).is_ok());
        // A grid whose open phase jumps >90° per step must be refused.
        let mut bad = set.clone();
        if let Some(open) = &mut bad.open {
            for (i, v) in open.s11.iter_mut().enumerate() {
                *v = C64::from_polar(1.0, 2.0 * i as f64); // 115° per step
            }
        }
        let err = bad.interpolate(&[150_000_000]).unwrap_err().to_string();
        assert!(err.contains("too coarse"), "{err}");
    }

    #[test]
    fn open_capacitance_fit_recovers_kit_asymmetry() {
        // Real kit: open with 60 fF, short ideal. Calibrated as if ideal.
        let mut real = CalKit::ideal();
        real.set_open_c0(60e-15);
        let freqs: Vec<u64> = (0..1001).map(|i| 50_000 + i * 6_300_000).collect(); // 50 kHz .. 6.3 GHz
        let one = C64::new(0.0, 0.0);
        let mut set = CalibrationSet::new(freqs.clone(), CalKit::ideal());
        // port standards (real kit) measured through the synthetic instrument
        for std in [Standard::Open, Standard::Short, Standard::Load] {
            let (mut s11, mut s21) = (Vec::new(), Vec::new());
            for &f in &freqs {
                let fh = f as f64;
                let a = match std {
                    Standard::Open => real.open.gamma(fh),
                    Standard::Short => real.short.gamma(fh),
                    _ => real.load.gamma(fh),
                };
                let (m11, m21) = measure(&terms_at_freq(fh), a, one);
                s11.push(m11);
                s21.push(m21);
            }
            set.set(std, &sweep_from(&freqs, s11, s21)).unwrap();
        }
        // the same standards at the end of a 2.5 ns lossy line
        let tau = 2.5e-9;
        for (std, term) in [(Standard::LineOpen, &real.open), (Standard::LineShort, &real.short)] {
            let (mut s11, mut s21) = (Vec::new(), Vec::new());
            for &f in &freqs {
                let fh = f as f64;
                let loss = 10f64.powf(-0.5 * (fh / 1e9).sqrt() / 20.0); // 0.5 dB·√GHz round trip
                let a = term.gamma(fh) * C64::from_polar(loss, -2.0 * 2.0 * PI * fh * tau);
                let (m11, m21) = measure(&terms_at_freq(fh), a, one);
                s11.push(m11);
                s21.push(m21);
            }
            set.set(std, &sweep_from(&freqs, s11, s21)).unwrap();
        }
        let fit = set.fit_open_capacitance(500_000_000).unwrap();
        let c_ff = fit.delta_c_farad * 1e15;
        assert!((c_ff - 60.0).abs() < 1.0, "fitted {c_ff} fF, expected ~60 fF; {fit:?}");
        assert!(fit.ripple_rms_after < fit.ripple_rms_before / 5.0, "{fit:?}");

        // Apply and refit: the asymmetry should be gone.
        set.kit.set_open_c0(set.kit.open_c0() + fit.delta_c_farad);
        let again = set.fit_open_capacitance(500_000_000).unwrap();
        assert!(again.delta_c_farad.abs() * 1e15 < 1.0, "residual {} fF", again.delta_c_farad * 1e15);
    }

    #[test]
    fn json_roundtrip_and_missing_standard() {
        let freqs = vec![1_000_000, 2_000_000];
        let mut set = CalibrationSet::new(freqs.clone(), CalKit::ideal());
        set.set(
            Standard::Open,
            &sweep_from(&freqs, vec![C64::new(1.0, 0.0); 2], vec![C64::new(0.0, 0.0); 2]),
        )
        .unwrap();
        let json = set.to_json().unwrap();
        let back = CalibrationSet::from_json(&json).unwrap();
        assert_eq!(back, set);
        let err = back.solve().unwrap_err().to_string();
        assert!(err.contains("short") && err.contains("load"), "{err}");
        // wrong grid rejected
        assert!(set
            .set(
                Standard::Short,
                &sweep_from(&[1, 2], vec![C64::new(0.0, 0.0); 2], vec![C64::new(0.0, 0.0); 2])
            )
            .is_err());
    }
}
