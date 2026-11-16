//! Numbers that JSON cannot hold. A sweep can contain NaN (a FIFO record
//! whose reference wave is zero) or an infinity; serde_json writes those as
//! `null` and then refuses to read `null` back as a number, so one bad point
//! would make a whole sweep unreadable. In a human-readable format such a
//! number is written as a string instead; binary formats keep the plain `f64`.

use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeTuple;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An `f64` that may not be finite.
struct Num(f64);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        if x.is_finite() || !s.is_human_readable() {
            s.serialize_f64(x)
        } else if x.is_nan() {
            s.serialize_str("NaN")
        } else if x > 0.0 {
            s.serialize_str("inf")
        } else {
            s.serialize_str("-inf")
        }
    }
}

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Num;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"a number, "NaN", "inf" or "-inf""#)
            }
            fn visit_f64<E: de::Error>(self, x: f64) -> Result<Num, E> {
                Ok(Num(x))
            }
            // JSON writes 1.0 as 1.0, but another writer may not.
            fn visit_i64<E: de::Error>(self, x: i64) -> Result<Num, E> {
                Ok(Num(x as f64))
            }
            fn visit_u64<E: de::Error>(self, x: u64) -> Result<Num, E> {
                Ok(Num(x as f64))
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Num, E> {
                match s {
                    "NaN" => Ok(Num(f64::NAN)),
                    "inf" => Ok(Num(f64::INFINITY)),
                    "-inf" => Ok(Num(f64::NEG_INFINITY)),
                    _ => Err(E::invalid_value(de::Unexpected::Str(s), &self)),
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// `Vec<Complex64>` as a list of `[re, im]`, the layout num-complex's own
/// serde uses, with non-finite parts as strings.
pub(crate) mod complex_vec {
    use super::*;
    use num_complex::Complex64;

    struct Pair(Complex64);

    impl Serialize for Pair {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut t = s.serialize_tuple(2)?;
            t.serialize_element(&Num(self.0.re))?;
            t.serialize_element(&Num(self.0.im))?;
            t.end()
        }
    }

    impl<'de> Deserialize<'de> for Pair {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let (Num(re), Num(im)) = <(Num, Num)>::deserialize(d)?;
            Ok(Pair(Complex64::new(re, im)))
        }
    }

    pub(crate) fn serialize<S: Serializer>(v: &[Complex64], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|&c| Pair(c)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Complex64>, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Vec<Complex64>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of [re, im] pairs")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 16));
                while let Some(Pair(c)) = seq.next_element()? {
                    out.push(c);
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V)
    }
}

#[cfg(test)]
mod tests {
    use crate::device::SweepResult;
    use num_complex::Complex64;
    use std::time::Duration;

    #[test]
    fn a_sweep_with_nan_and_infinities_survives_json() {
        let sweep = SweepResult {
            freqs_hz: vec![1_000_000, 2_000_000, 3_000_000],
            s11: vec![
                Complex64::new(f64::NAN, 0.25),
                Complex64::new(f64::INFINITY, f64::NEG_INFINITY),
                Complex64::new(-0.5, 1e-300),
            ],
            s21: vec![Complex64::new(0.0, -0.0); 3],
            points: Vec::new(),
            checksum_ok: 3,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::from_millis(12),
        };
        let text = serde_json::to_string(&sweep).unwrap();
        assert!(text.contains(r#"["NaN",0.25],["inf","-inf"]"#), "{text}");
        let back: SweepResult = serde_json::from_str(&text).unwrap();
        assert!(back.s11[0].re.is_nan() && back.s11[0].im == 0.25);
        assert_eq!(back.s11[1], Complex64::new(f64::INFINITY, f64::NEG_INFINITY));
        assert_eq!(back.s11[2], Complex64::new(-0.5, 1e-300));
        assert_eq!(back.s21, sweep.s21);
        assert_eq!((back.checksum_ok, back.elapsed), (3, Duration::from_millis(12)));
        // a number written as an integer is read too; any other string is an error
        let ints: SweepResult = serde_json::from_str(&text.replace("[-0.5,", "[-1,")).unwrap();
        assert_eq!(ints.s11[2].re, -1.0);
        assert!(serde_json::from_str::<SweepResult>(&text.replace("\"NaN\"", "\"nan\"")).is_err());
    }
}
