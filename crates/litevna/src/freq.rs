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
    // Accept a decimal comma ("6,3G") as well as a point.
    let value: f64 = num
        .trim()
        .replace('_', "")
        .replace(',', ".")
        .parse()
        .map_err(|_| Error::InvalidArgument(format!("not a frequency: {text:?}")))?;
    if !value.is_finite() {
        return Err(Error::InvalidArgument(format!("not a frequency: {text:?}")));
    }
    if value < 0.0 {
        return Err(Error::InvalidArgument(format!("negative frequency: {text:?}")));
    }
    let hz = value * mult;
    if hz > 1e13 {
        return Err(Error::InvalidArgument(format!("frequency out of range: {text:?}")));
    }
    Ok(hz.round() as u64)
}

/// Index of the frequency in `freqs_hz` closest to `f_hz` (0 for an empty list).
pub fn nearest_index(freqs_hz: &[u64], f_hz: u64) -> usize {
    freqs_hz
        .iter()
        .enumerate()
        .min_by_key(|(_, x)| x.abs_diff(f_hz))
        .map(|(i, _)| i)
        .unwrap_or(0)
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

/// A frequency as text for an input field: [`format_hz`] without the space
/// when [`parse_hz`] reads that back as the same number, otherwise with as
/// many decimals as it takes. `format_hz` rounds to the kHz from 1 MHz up,
/// and a grid rebuilt from a rounded stop frequency is another grid.
pub fn format_hz_exact(hz: u64) -> String {
    let reads_back = |text: &str| parse_hz(text).is_ok_and(|v| v == hz);
    let short = format_hz(hz).replace(' ', "");
    if reads_back(&short) {
        return short;
    }
    let (unit, scale, decimals) = if hz >= 1_000_000_000 { ("GHz", 1e9, 9) } else { ("MHz", 1e6, 6) };
    let full = format!("{:.*}", decimals, hz as f64 / scale);
    let text = format!("{}{unit}", full.trim_end_matches('0'));
    if reads_back(&text) {
        text
    } else {
        hz.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_index_picks_the_closest_point() {
        let f = [1_000_000u64, 2_000_000, 3_000_000];
        assert_eq!(nearest_index(&f, 0), 0);
        assert_eq!(nearest_index(&f, 1_400_000), 0);
        assert_eq!(nearest_index(&f, 1_600_000), 1);
        assert_eq!(nearest_index(&f, 9_000_000), 2);
        assert_eq!(nearest_index(&[], 5), 0);
    }

    #[test]
    fn rejects_non_finite_and_absurd_values() {
        for bad in ["nan", "inf", "-inf", "NaN", "1e30", "-1M", "abc"] {
            assert!(parse_hz(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn decimal_comma() {
        assert_eq!(parse_hz("6,3G").unwrap(), 6_300_000_000);
        assert_eq!(parse_hz("14,2M").unwrap(), 14_200_000);
    }

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
    fn a_field_text_reads_back_as_the_same_frequency() {
        // as before where three decimals are exact
        assert_eq!(format_hz_exact(14_200_000), "14.200MHz");
        assert_eq!(format_hz_exact(50_000), "50.000kHz");
        assert_eq!(format_hz_exact(999), "999Hz");
        assert_eq!(format_hz_exact(6_300_000_000), "6.300000GHz");
        // the stop of 100 kHz to 30 MHz in 1601 points
        assert_eq!(format_hz_exact(29_999_200), "29.9992MHz");
        assert_eq!(format_hz_exact(1_000_001), "1.000001MHz");
        assert_eq!(format_hz_exact(6_299_999_999), "6.299999999GHz");
        for hz in [
            0,
            1,
            999,
            1_000,
            1_001,
            999_999,
            1_000_000,
            29_999_200,
            123_456_789,
            999_999_999,
            1_000_000_001,
            6_299_999_999,
            9_999_999_999_999,
        ] {
            assert_eq!(parse_hz(&format_hz_exact(hz)).unwrap(), hz, "{}", format_hz_exact(hz));
        }
        let mut x: u64 = 12345;
        for _ in 0..20_000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let hz = (x >> 20) % 7_000_000_000;
            assert_eq!(parse_hz(&format_hz_exact(hz)).unwrap(), hz, "{}", format_hz_exact(hz));
        }
    }

    #[test]
    fn formats() {
        assert_eq!(format_hz(14_200_000), "14.200 MHz");
        assert_eq!(format_hz(50_000), "50.000 kHz");
    }
}
