//! Parse and format frequencies the way radio amateurs type them.

use crate::{Error, Result};

/// Parse `"14.2M"`, `"50k"`, `"1.3GHz"`, `"145000000"` into Hz.
/// Suffixes are case-insensitive except that `m`/`M` both mean mega.
pub fn parse_hz(text: &str) -> Result<u64> {
    let s = text.trim();
    let lower = s.to_ascii_lowercase();
    let lower = lower.strip_suffix("hz").unwrap_or(&lower).trim_end();
    let (num, mult): (&str, f64) = match lower.chars().last() {
        Some('k') => (&lower[..lower.len() - 1], 1e3),
        Some('m') => (&lower[..lower.len() - 1], 1e6),
        Some('g') => (&lower[..lower.len() - 1], 1e9),
        _ => (lower, 1.0),
    };
    let value: f64 = num
        .trim()
        .replace('_', "")
        .parse()
        .map_err(|_| Error::InvalidArgument(format!("not a frequency: {text:?}")))?;
    if value < 0.0 {
        return Err(Error::InvalidArgument(format!("negative frequency: {text:?}")));
    }
    Ok((value * mult).round() as u64)
}

/// Format Hz compactly, e.g. `14.200 MHz`.
pub fn format_hz(hz: u64) -> String {
    let f = hz as f64;
    if hz >= 1_000_000_000 {
        format!("{:.6} GHz", f / 1e9)
    } else if hz >= 1_000_000 {
        format!("{:.3} MHz", f / 1e6)
    } else if hz >= 1_000 {
        format!("{:.3} kHz", f / 1e3)
    } else {
        format!("{hz} Hz")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_inputs() {
        assert_eq!(parse_hz("50k").unwrap(), 50_000);
        assert_eq!(parse_hz("14.2M").unwrap(), 14_200_000);
        assert_eq!(parse_hz("1.3GHz").unwrap(), 1_300_000_000);
        assert_eq!(parse_hz("145000000").unwrap(), 145_000_000);
        assert_eq!(parse_hz(" 6.3 g ").unwrap(), 6_300_000_000);
        assert!(parse_hz("abc").is_err());
    }

    #[test]
    fn formats() {
        assert_eq!(format_hz(14_200_000), "14.200 MHz");
        assert_eq!(format_hz(50_000), "50.000 kHz");
    }
}
