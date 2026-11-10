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

/// A Touchstone file as read: the sweep, referred to 50 Ω, and the reference
/// impedance the file itself used.
#[derive(Debug, Clone)]
pub struct TouchstoneFile {
    pub sweep: SweepResult,
    /// The `R` value of the option line (50 when absent). S11 has been
    /// renormalised to 50 Ω when this differs; S21 is left as in the file.
    pub z0_ohm: f64,
}

impl TouchstoneFile {
    /// True when the file used a reference other than 50 Ω.
    pub fn renormalised(&self) -> bool {
        (self.z0_ohm - 50.0).abs() > 1e-9
    }
}

/// Read a Touchstone v1 file (`.s1p` or `.s2p`), see [`read_touchstone_full`].
pub fn read_touchstone<R: BufRead>(r: R, ports: Option<usize>) -> Result<SweepResult> {
    Ok(read_touchstone_full(r, ports)?.sweep)
}

/// Read a Touchstone v1 file (`.s1p` or `.s2p`). Handles the option line
/// (`# <Hz|kHz|MHz|GHz> S <RI|MA|DB> R <z0>`, defaults GHz/MA/50), comments,
/// values wrapped over several lines, and both column orders. S12/S22 of a
/// two-port are ignored (the result only carries S11 and S21); a one-port
/// gets S21 = 0. A reference impedance other than 50 Ω is honoured for S11
/// (renormalised to 50 Ω, so impedance and SWR come out right); S21 is kept
/// as written. `ports` is a hint from the file name (1 or 2); with `None`
/// the number of values per frequency decides.
pub fn read_touchstone_full<R: BufRead>(r: R, ports: Option<usize>) -> Result<TouchstoneFile> {
    let mut unit = 1e9;
    let mut format = Format::Ma;
    let mut z0 = 50.0;
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
                    "Y" | "Z" | "G" | "H" => {
                        return Err(Error::parse(
                            Some(lineno + 1),
                            format!("Touchstone parameter type {w} not supported (only S)"),
                        ))
                    }
                    "RI" => format = Format::Ri,
                    "MA" => format = Format::Ma,
                    "DB" => format = Format::Db,
                    "R" => {
                        z0 = match words.next().map(|w| w.parse::<f64>()) {
                            Some(Ok(v)) if v.is_finite() && v > 0.0 => v,
                            _ => return Err(Error::parse(Some(lineno + 1), "bad reference impedance".into())),
                        };
                    }
                    other => return Err(Error::parse(Some(lineno + 1), format!("unknown option {other:?}"))),
                }
            }
            continue;
        }
        if body.starts_with('[') {
            return Err(Error::parse(
                Some(lineno + 1),
                "Touchstone v2 ([Version] keywords) is not supported".into(),
            ));
        }
        for t in body.split_whitespace() {
            let v: f64 = t
                .parse()
                .map_err(|_| Error::parse(Some(lineno + 1), format!("not a number: {t:?}")))?;
            // Rust reads "nan" and "inf" as numbers; a measurement file has neither.
            if !v.is_finite() {
                return Err(Error::parse(Some(lineno + 1), format!("not a finite number: {t:?}")));
            }
            tokens.push(v);
        }
    }
    if tokens.is_empty() {
        return Err(Error::parse(None, "no data in Touchstone file".into()));
    }
    let per = match ports {
        Some(1) => 3,
        Some(2) => 9,
        Some(n) => return Err(Error::parse(None, format!("{n}-port files are not supported"))),
        None => {
            if tokens.len().is_multiple_of(9) && tokens.len().is_multiple_of(3) && looks_like_two_port(&tokens) {
                9
            } else if tokens.len().is_multiple_of(3) {
                3
            } else {
                return Err(Error::parse(
                    None,
                    format!("{} values do not form 1-port (3) or 2-port (9) records", tokens.len()),
                ));
            }
        }
    };
    if !tokens.len().is_multiple_of(per) {
        return Err(Error::parse(None, format!("{} values do not form records of {per}", tokens.len())));
    }
    let n = tokens.len() / per;
    let mut freqs_hz = Vec::with_capacity(n);
    let mut s11 = Vec::with_capacity(n);
    let mut s21 = Vec::with_capacity(n);
    for rec in tokens.chunks(per) {
        let f = rec[0] * unit;
        if !(f >= 0.0) || f > 1e13 {
            return Err(Error::parse(None, format!("frequency out of range: {}", rec[0])));
        }
        freqs_hz.push(f.round() as u64);
        s11.push(format.to_complex(rec[1], rec[2]));
        // two-port column order is S11 S21 S12 S22
        s21.push(if per == 9 {
            format.to_complex(rec[3], rec[4])
        } else {
            Complex64::new(0.0, 0.0)
        });
    }
    if freqs_hz.windows(2).any(|w| w[1] < w[0]) {
        return Err(Error::parse(None, "frequencies are not increasing".into()));
    }
    if let Some(w) = freqs_hz.windows(2).find(|w| w[1] == w[0]) {
        // Two rows for one frequency: which of the two is the measurement?
        return Err(Error::parse(None, format!("frequency {} Hz appears more than once", w[0])));
    }
    if (z0 - 50.0).abs() > 1e-9 {
        for g in &mut s11 {
            *g = renormalise_gamma(*g, z0, 50.0);
        }
    }
    Ok(TouchstoneFile {
        sweep: SweepResult {
            points: Vec::new(),
            freqs_hz,
            s11,
            s21,
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        },
        z0_ohm: z0,
    })
}

/// A one-port reflection coefficient referred to `from` ohm, expressed against
/// `to` ohm: the impedance Z = from·(1+Γ)/(1−Γ) is the same, so
/// Γ' = (Z − to)/(Z + to), written without the division so Γ = 1 stays 1.
pub fn renormalise_gamma(gamma: Complex64, from: f64, to: f64) -> Complex64 {
    let one = Complex64::new(1.0, 0.0);
    let num = from * (one + gamma) - to * (one - gamma);
    let den = from * (one + gamma) + to * (one - gamma);
    num / den
}

/// Read a Touchstone file by path; the extension (`.s1p` / `.s2p`) sets the port count.
pub fn read_touchstone_file(path: &std::path::Path) -> Result<SweepResult> {
    Ok(read_touchstone_file_full(path)?.sweep)
}

/// Like [`read_touchstone_file`], also returning the file's reference impedance.
pub fn read_touchstone_file_full(path: &std::path::Path) -> Result<TouchstoneFile> {
    let ports = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("s1p") => Some(1),
        Some("s2p") => Some(2),
        _ => None,
    };
    let f = std::fs::File::open(path)?;
    read_touchstone_full(std::io::BufReader::new(f), ports)
}

/// With an ambiguous token count (a multiple of 9 is also a multiple of 3),
/// prefer the reading whose frequencies increase.
fn looks_like_two_port(tokens: &[f64]) -> bool {
    let inc = |step: usize| {
        tokens
            .chunks(step)
            .map(|c| c[0])
            .collect::<Vec<_>>()
            .windows(2)
            .all(|w| w[1] > w[0])
    };
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

// `db` and `swr` live in [`crate::traces`] with the other derived quantities;
// re-exported here because the CSV writer uses them and callers imported them from this module.
pub use crate::traces::{db, swr};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_values_and_repeated_frequencies_are_refused() {
        let err = read_touchstone("# MHz S RI R 50\n1 0.5 0.0\n2 nan 0.0\n".as_bytes(), Some(1)).unwrap_err();
        assert_eq!(err.to_string(), "line 3: not a finite number: \"nan\"");
        let err = read_touchstone("# MHz S DB R 50\n1 inf 0.0\n".as_bytes(), Some(1)).unwrap_err();
        assert!(err.to_string().contains("not a finite number"), "{err}");
        let err = read_touchstone("# MHz S RI R 50\n1 0.5 0.0\n1 0.4 0.0\n2 0.3 0.0\n".as_bytes(), Some(1)).unwrap_err();
        assert_eq!(err.to_string(), "frequency 1000000 Hz appears more than once");
    }

    #[test]
    fn tabs_crlf_and_trailing_comments_are_fine() {
        let text = "! made by something else\r\n#\tMHZ\tS\tMA\tR\t50\r\n1\t0.5\t90 ! first point\r\n2\t0.25\t-90\r\n";
        let sweep = read_touchstone(text.as_bytes(), Some(1)).unwrap();
        assert_eq!(sweep.freqs_hz, vec![1_000_000, 2_000_000]);
        assert!((sweep.s11[0] - Complex64::new(0.0, 0.5)).norm() < 1e-12);
        assert!((sweep.s11[1] - Complex64::new(0.0, -0.25)).norm() < 1e-12);
    }

    #[test]
    fn parse_errors_name_the_line() {
        let text = "# MHz S RI R 50\n1 0.5 0.0\n2 0.5 oops\n";
        let err = read_touchstone(text.as_bytes(), Some(1)).unwrap_err();
        assert!(matches!(err, Error::Parse { line: Some(3), .. }), "{err:?}");
        assert_eq!(err.to_string(), "line 3: not a number: \"oops\"");
        let err = read_touchstone("[Version] 2.0\n".as_bytes(), None).unwrap_err();
        assert!(matches!(err, Error::Parse { line: Some(1), .. }), "{err:?}");
        let err = read_touchstone("! only a comment\n".as_bytes(), None).unwrap_err();
        assert!(matches!(err, Error::Parse { line: None, .. }), "{err:?}");
    }

    fn sample() -> SweepResult {
        let freqs_hz: Vec<u64> = (1..=5).map(|k| k * 10_000_000).collect();
        let s11: Vec<Complex64> = freqs_hz.iter().map(|&f| Complex64::from_polar(0.3, -(f as f64) * 1e-8)).collect();
        let s21: Vec<Complex64> = freqs_hz.iter().map(|&f| Complex64::from_polar(0.9, -(f as f64) * 3e-8)).collect();
        SweepResult {
            points: Vec::new(),
            freqs_hz,
            s11,
            s21,
            checksum_ok: 0,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::ZERO,
        }
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
    fn reference_impedance_is_honoured_for_s11() {
        // Γ = 0 in a 75 Ω file is a 75 Ω load: against 50 Ω that is Γ = 0.2.
        let s1p = "# MHz S RI R 75\n1 0 0\n2 1 0\n";
        let f = read_touchstone_full(std::io::Cursor::new(s1p), Some(1)).unwrap();
        assert!(f.renormalised());
        assert!((f.sweep.s11[0].re - 0.2).abs() < 1e-12 && f.sweep.s11[0].im.abs() < 1e-12);
        // An open stays an open.
        assert!((f.sweep.s11[1] - Complex64::new(1.0, 0.0)).norm() < 1e-12);
        let plain = read_touchstone_full(std::io::Cursor::new("# MHz S RI R 50\n1 0 0\n"), Some(1)).unwrap();
        assert!(!plain.renormalised());
        assert!(read_touchstone(std::io::Cursor::new("# MHz S RI R abc\n1 0 0\n"), Some(1)).is_err());
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
            points: Vec::new(),
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
