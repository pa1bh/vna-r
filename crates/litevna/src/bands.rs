//! Amateur radio bands, for drawing as a backdrop behind a frequency plot so
//! that it is plain whether an antenna's dip lies in the band.
//!
//! IARU Region 1 (Europe, Africa, the Middle East and northern Asia), the
//! allocations as a Dutch licence has them; a few are wider or narrower
//! elsewhere in the region (4 m is not available in every country). The
//! table is deliberately a constant: the first version draws one region and
//! leaves other regions and personal bands for later (work list 65).

/// One band: its common name and its edges in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub name: &'static str,
    pub start_hz: u64,
    pub stop_hz: u64,
}

const fn band(name: &'static str, start_hz: u64, stop_hz: u64) -> Band {
    Band { name, start_hz, stop_hz }
}

/// IARU Region 1, sorted by frequency, non-overlapping.
pub const REGION_1: &[Band] = &[
    band("2200 m", 135_700, 137_800),
    band("630 m", 472_000, 479_000),
    band("160 m", 1_810_000, 2_000_000),
    band("80 m", 3_500_000, 3_800_000),
    band("60 m", 5_351_500, 5_366_500),
    band("40 m", 7_000_000, 7_200_000),
    band("30 m", 10_100_000, 10_150_000),
    band("20 m", 14_000_000, 14_350_000),
    band("17 m", 18_068_000, 18_168_000),
    band("15 m", 21_000_000, 21_450_000),
    band("12 m", 24_890_000, 24_990_000),
    band("10 m", 28_000_000, 29_700_000),
    band("6 m", 50_000_000, 52_000_000),
    band("4 m", 70_000_000, 70_500_000),
    band("2 m", 144_000_000, 146_000_000),
    band("70 cm", 430_000_000, 440_000_000),
    band("23 cm", 1_240_000_000, 1_300_000_000),
    band("13 cm", 2_320_000_000, 2_450_000_000),
    band("9 cm", 3_400_000_000, 3_410_000_000),
    band("6 cm", 5_650_000_000, 5_850_000_000),
];

/// The bands of `table` that overlap `start_hz..=stop_hz`, in order.
pub fn bands_within(table: &'static [Band], start_hz: u64, stop_hz: u64) -> impl Iterator<Item = &'static Band> {
    table.iter().filter(move |b| b.stop_hz >= start_hz && b.start_hz <= stop_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_1_is_sorted_and_disjoint() {
        for w in REGION_1.windows(2) {
            assert!(w[0].stop_hz < w[1].start_hz, "{:?} overlaps {:?}", w[0], w[1]);
        }
        for b in REGION_1 {
            assert!(b.start_hz < b.stop_hz, "{b:?}");
        }
    }

    #[test]
    fn selection_by_sweep_range() {
        let hf: Vec<&str> = bands_within(REGION_1, 3_000_000, 30_000_000).map(|b| b.name).collect();
        assert_eq!(hf, ["80 m", "60 m", "40 m", "30 m", "20 m", "17 m", "15 m", "12 m", "10 m"]);
        // A sweep that ends inside a band still gets it; one between bands gets none.
        assert_eq!(bands_within(REGION_1, 1_000_000, 1_900_000).count(), 1);
        assert_eq!(bands_within(REGION_1, 8_000_000, 10_000_000).count(), 0);
    }
}
