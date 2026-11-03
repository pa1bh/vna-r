//! Touchstone v1 `.s2p` export (Hz, real/imaginary, 50 Ω).
//!
//! The LiteVNA measures S11 and S21 only; S12 and S22 are written as zero so
//! the file stays a valid two-port file that every tool accepts.

use std::io::Write;

use crate::device::SweepResult;

pub fn write_s2p<W: Write>(mut w: W, sweep: &SweepResult, comment: Option<&str>) -> std::io::Result<()> {
    writeln!(w, "! Exported by vna-r (litevna crate)")?;
    if let Some(c) = comment {
        for line in c.lines() {
            writeln!(w, "! {line}")?;
        }
    }
    writeln!(w, "! S11 and S21 measured; S12/S22 not available on a 1.5-port VNA (written as 0).")?;
    writeln!(w, "# Hz S RI R 50")?;
    for i in 0..sweep.freqs_hz.len() {
        let s11 = sweep.s11[i];
        let s21 = sweep.s21[i];
        writeln!(
            w,
            "{} {:.9e} {:.9e} {:.9e} {:.9e} 0 0 0 0",
            sweep.freqs_hz[i], s11.re, s11.im, s21.re, s21.im
        )?;
    }
    Ok(())
}

pub fn write_csv<W: Write>(mut w: W, sweep: &SweepResult) -> std::io::Result<()> {
    writeln!(w, "freq_hz,s11_re,s11_im,s11_db,s11_deg,swr,s21_re,s21_im,s21_db,s21_deg")?;
    for i in 0..sweep.freqs_hz.len() {
        let s11 = sweep.s11[i];
        let s21 = sweep.s21[i];
        writeln!(
            w,
            "{},{:.9e},{:.9e},{:.3},{:.2},{:.3},{:.9e},{:.9e},{:.3},{:.2}",
            sweep.freqs_hz[i],
            s11.re,
            s11.im,
            db(s11.norm()),
            s11.arg().to_degrees(),
            swr(s11.norm()),
            s21.re,
            s21.im,
            db(s21.norm()),
            s21.arg().to_degrees(),
        )?;
    }
    Ok(())
}

/// 20·log10 of a linear magnitude; `-inf` guarded to a floor.
pub fn db(mag: f64) -> f64 {
    if mag <= 0.0 {
        -200.0
    } else {
        20.0 * mag.log10()
    }
}

/// Standing wave ratio from |Γ|. Values ≥ 1 clamp to 999.
pub fn swr(gamma_mag: f64) -> f64 {
    if gamma_mag >= 1.0 {
        999.0
    } else {
        (1.0 + gamma_mag) / (1.0 - gamma_mag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex64;
    use std::time::Duration;

    #[test]
    fn writes_valid_touchstone() {
        let sweep = SweepResult {
            freqs_hz: vec![1_000_000, 2_000_000],
            s11: vec![Complex64::new(0.5, 0.0), Complex64::new(0.0, -0.5)],
            s21: vec![Complex64::new(0.1, 0.0), Complex64::new(0.0, 0.1)],
            points: vec![None, None],
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        };
        let mut buf = Vec::new();
        write_s2p(&mut buf, &sweep, Some("test")).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("# Hz S RI R 50"));
        assert!(text.lines().filter(|l| !l.starts_with('!') && !l.starts_with('#')).count() == 2);
        assert!((swr(0.5) - 3.0).abs() < 1e-12);
        assert!((db(0.1) + 20.0).abs() < 1e-9);
    }
}
