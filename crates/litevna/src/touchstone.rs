//! Touchstone v1 `.s2p` export (Hz, real/imaginary, 50 Ω) and import of
//! `.s1p` / `.s2p` files in any of the v1 formats.
//!
//! The LiteVNA measures S11 and S21 only; S12 and S22 are written as zero so
//! the file stays a valid two-port file that every tool accepts.

use std::io::{BufRead, Write};
use std::time::Duration;

use num_complex::Complex64;

use crate::device::SweepResult;
use crate::error::{Error, Result};

/// Read a Touchstone v1 file (`.s1p` or `.s2p`). Handles the option line
/// (`# <Hz|kHz|MHz|GHz> S <RI|MA|DB> R <z0>`, defaults GHz/MA/50), comments,
/// values wrapped over several lines, and both column orders. S12/S22 of a
/// two-port are ignored (the result only carries S11 and S21); a one-port
/// gets S21 = 0. `ports` is a hint from the file name (1 or 2); with `None`
/// the number of values per frequency decides.
pub fn read_touchstone<R: BufRead>(r: R, ports: Option<usize>) -> Result<SweepResult> {
    let mut unit = 1e9;
    let mut format = Format::Ma;
    let mut tokens: Vec<f64> = Vec::new();
    let mut saw_option = false;
    for (lineno, line) in r.lines().enumerate() {
        let line = line?;
        let body = line.split('!').next().unwrap_or("").trim();
        if body.is_empty() {
            continue;
        }
        if let Some(opt) = body.strip_prefix('#') {
            if saw_option {
                continue; // v1 allows only one; ignore repeats
            }
            saw_option = true;
            let mut words = opt.split_whitespace().map(|w| w.to_ascii_uppercase());
            while let Some(w) = words.next() {
                match w.as_str() {
                    "HZ" => unit = 1.0,
                    "KHZ" => unit = 1e3,
                    "MHZ" => unit = 1e6,
                    "GHZ" => unit = 1e9,
                    "S" => {}
                    "Y" | "Z" | "G" | "H" => return Err(Error::InvalidArgument(format!("Touchstone parameter type {w} not supported (only S)"))),
                    "RI" => format = Format::Ri,
                    "MA" => format = Format::Ma,
                    "DB" => format = Format::Db,
                    "R" => {
                        let _z0 = words.next(); // reference impedance; S-parameters are used as-is
                    }
                    other => return Err(Error::InvalidArgument(format!("line {}: unknown option {other:?}", lineno + 1))),
                }
            }
            continue;
        }
        if body.starts_with('[') {
            return Err(Error::InvalidArgument("Touchstone v2 ([Version] keywords) is not supported".into()));
        }
        for t in body.split_whitespace() {
            let v: f64 = t.parse().map_err(|_| Error::InvalidArgument(format!("line {}: not a number: {t:?}", lineno + 1)))?;
            tokens.push(v);
        }
    }
    if tokens.is_empty() {
        return Err(Error::InvalidArgument("no data in Touchstone file".into()));
    }
    let per = match ports {
        Some(1) => 3,
        Some(2) => 9,
        Some(n) => return Err(Error::InvalidArgument(format!("{n}-port files are not supported"))),
        None => {
            if tokens.len() % 9 == 0 && tokens.len() % 3 == 0 && looks_like_two_port(&tokens) {
                9
            } else if tokens.len() % 3 == 0 {
                3
            } else {
                return Err(Error::InvalidArgument(format!("{} values do not form 1-port (3) or 2-port (9) records", tokens.len())));
            }
        }
    };
    if tokens.len() % per != 0 {
        return Err(Error::InvalidArgument(format!("{} values do not form records of {per}", tokens.len())));
    }
    let n = tokens.len() / per;
    let mut freqs_hz = Vec::with_capacity(n);
    let mut s11 = Vec::with_capacity(n);
    let mut s21 = Vec::with_capacity(n);
    for rec in tokens.chunks(per) {
        let f = rec[0] * unit;
        if !(f >= 0.0) || f > 1e13 {
            return Err(Error::InvalidArgument(format!("frequency out of range: {}", rec[0])));
        }
        freqs_hz.push(f.round() as u64);
        s11.push(format.to_complex(rec[1], rec[2]));
        // two-port column order is S11 S21 S12 S22
        s21.push(if per == 9 { format.to_complex(rec[3], rec[4]) } else { Complex64::new(0.0, 0.0) });
    }
    if freqs_hz.windows(2).any(|w| w[1] < w[0]) {
        return Err(Error::InvalidArgument("frequencies are not increasing".into()));
    }
    Ok(SweepResult {
        points: vec![None; n],
        freqs_hz,
        s11,
        s21,
        checksum_ok: 0,
        checksum_bad: 0,
        duplicates: 0,
        elapsed: Duration::ZERO,
    })
}

/// Read a Touchstone file by path; the extension (`.s1p` / `.s2p`) sets the port count.
pub fn read_touchstone_file(path: &std::path::Path) -> Result<SweepResult> {
    let ports = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("s1p") => Some(1),
        Some("s2p") => Some(2),
        _ => None,
    };
    let f = std::fs::File::open(path)?;
    read_touchstone(std::io::BufReader::new(f), ports)
}

/// With an ambiguous token count (a multiple of 9 is also a multiple of 3),
/// prefer the reading whose frequencies increase.
fn looks_like_two_port(tokens: &[f64]) -> bool {
    let inc = |step: usize| tokens.chunks(step).map(|c| c[0]).collect::<Vec<_>>().windows(2).all(|w| w[1] > w[0]);
    inc(9) && !inc(3)
}

#[derive(Clone, Copy)]
enum Format {
    Ri,
    Ma,
    Db,
}

impl Format {
    fn to_complex(self, a: f64, b: f64) -> Complex64 {
        match self {
            Format::Ri => Complex64::new(a, b),
            Format::Ma => Complex64::from_polar(a, b.to_radians()),
            Format::Db => Complex64::from_polar(10f64.powf(a / 20.0), b.to_radians()),
        }
    }
}

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
        // Touchstone wants strictly increasing frequencies; touching sweep
        // segments share their boundary point, so skip exact repeats.
        if i > 0 && sweep.freqs_hz[i] == sweep.freqs_hz[i - 1] {
            continue;
        }
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

    fn sample() -> SweepResult {
        let freqs_hz: Vec<u64> = (1..=5).map(|k| k * 10_000_000).collect();
        let s11: Vec<Complex64> = freqs_hz.iter().map(|&f| Complex64::from_polar(0.3, -(f as f64) * 1e-8)).collect();
        let s21: Vec<Complex64> = freqs_hz.iter().map(|&f| Complex64::from_polar(0.9, -(f as f64) * 3e-8)).collect();
        SweepResult { points: vec![None; 5], freqs_hz, s11, s21, checksum_ok: 0, checksum_bad: 0, duplicates: 0, elapsed: Duration::ZERO }
    }

    #[test]
    fn s2p_round_trip() {
        let sw = sample();
        let mut buf = Vec::new();
        write_s2p(&mut buf, &sw, Some("test")).unwrap();
        let back = read_touchstone(std::io::Cursor::new(buf), Some(2)).unwrap();
        assert_eq!(back.freqs_hz, sw.freqs_hz);
        for i in 0..5 {
            assert!((back.s11[i] - sw.s11[i]).norm() < 1e-9);
            assert!((back.s21[i] - sw.s21[i]).norm() < 1e-9);
        }
        // the same without the extension hint
        let mut buf = Vec::new();
        write_s2p(&mut buf, &sw, None).unwrap();
        let back = read_touchstone(std::io::Cursor::new(buf), None).unwrap();
        assert_eq!(back.s21.len(), 5);
        assert!((back.s21[2] - sw.s21[2]).norm() < 1e-9);
    }

    #[test]
    fn reads_ghz_ma_s1p_and_db_s2p_wrapped() {
        let s1p = "! NanoVNA style\n# GHz S MA R 50\n0.001 0.5 90\n0.002 0.25 -90 ! trailing comment\n";
        let r = read_touchstone(std::io::Cursor::new(s1p), Some(1)).unwrap();
        assert_eq!(r.freqs_hz, vec![1_000_000, 2_000_000]);
        assert!((r.s11[0] - Complex64::new(0.0, 0.5)).norm() < 1e-12);
        assert!((r.s11[1] - Complex64::new(0.0, -0.25)).norm() < 1e-12);
        assert_eq!(r.s21[0], Complex64::new(0.0, 0.0));

        // DB format, MHz, values wrapped over two lines per frequency, default-less option casing
        let s2p = "# mhz s db r 50\n100 -20 0 -6.0206 -90\n  -6.0206 -90 -20 0\n200 -40 180 0 0\n  0 0 -40 180\n";
        let r = read_touchstone(std::io::Cursor::new(s2p), Some(2)).unwrap();
        assert_eq!(r.freqs_hz, vec![100_000_000, 200_000_000]);
        assert!((r.s11[0].norm() - 0.1).abs() < 1e-9);
        assert!((r.s21[0] - Complex64::new(0.0, -0.5)).norm() < 1e-4);
        assert!((r.s11[1] - Complex64::new(-0.01, 0.0)).norm() < 1e-9);
        assert!((r.s21[1].norm() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_touchstone(std::io::Cursor::new("# Hz S RI R 50\n1 2\n"), Some(1)).is_err());
        assert!(read_touchstone(std::io::Cursor::new("# Hz Y RI R 50\n1 2 3\n"), Some(1)).is_err());
        assert!(read_touchstone(std::io::Cursor::new("[Version] 2.0\n"), None).is_err());
        assert!(read_touchstone(std::io::Cursor::new("# Hz S RI R 50\n2 0 0\n1 0 0\n"), Some(1)).is_err());
    }
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
