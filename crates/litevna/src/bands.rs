//! Amateur radio bands, for drawing as a backdrop behind a frequency plot so
//! that it is plain whether an antenna's dip lies in the band.
//!
//! Two fixed tables: IARU Region 1 (Europe, Africa, the Middle East and
//! northern Asia), the allocations as a Dutch licence has them (a few are
//! wider or narrower elsewhere in the region; 4 m is not available in every
//! country), and Region 2 (the Americas), the allocations as the FCC has them
//! (Canada differs in a few edges). Region 3 and personal bands are left for
//! later (work list 65).

/// One band: its common name and its edges in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub name: &'static str,
    pub start_hz: u64,
    pub stop_hz: u64,
}

impl Band {
    /// The band with `fraction` of its width added on each side: a sweep that
    /// shows what happens just beyond the edges, where a multiband antenna's dip
    /// often sits while tuning. `fraction` 0.1 on 7.0–7.3 MHz gives 6.97–7.33 MHz.
    pub fn with_margin(&self, fraction: f64) -> (u64, u64) {
        let margin = (self.stop_hz - self.start_hz) as f64 * fraction.max(0.0);
        let start = (self.start_hz as f64 - margin).max(1.0).round() as u64;
        let stop = (self.stop_hz as f64 + margin).round() as u64;
        (start, stop)
    }
}

/// An IARU region: which table of bands applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    One,
    Two,
}

impl Region {
    pub const ALL: [Region; 2] = [Region::One, Region::Two];

    pub fn bands(self) -> &'static [Band] {
        match self {
            Region::One => REGION_1,
            Region::Two => REGION_2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Region::One => "IARU Region 1",
            Region::Two => "IARU Region 2",
        }
    }

    /// Where the region is, for a menu.
    pub fn where_(self) -> &'static str {
        match self {
            Region::One => "Europe, Africa, Middle East, northern Asia",
            Region::Two => "the Americas",
        }
    }

    /// The region's number, as the settings store it.
    pub fn number(self) -> u8 {
        match self {
            Region::One => 1,
            Region::Two => 2,
        }
    }

    pub fn from_number(n: u8) -> Option<Region> {
        Region::ALL.into_iter().find(|r| r.number() == n)
    }
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

/// IARU Region 2 as the FCC allocates it, sorted by frequency, non-overlapping.
/// 60 m is five channels, given as the span they cover; 13 cm is two pieces.
pub const REGION_2: &[Band] = &[
    band("2200 m", 135_700, 137_800),
    band("630 m", 472_000, 479_000),
    band("160 m", 1_800_000, 2_000_000),
    band("80 m", 3_500_000, 4_000_000),
    band("60 m", 5_330_500, 5_406_500),
    band("40 m", 7_000_000, 7_300_000),
    band("30 m", 10_100_000, 10_150_000),
    band("20 m", 14_000_000, 14_350_000),
    band("17 m", 18_068_000, 18_168_000),
    band("15 m", 21_000_000, 21_450_000),
    band("12 m", 24_890_000, 24_990_000),
    band("10 m", 28_000_000, 29_700_000),
    band("6 m", 50_000_000, 54_000_000),
    band("2 m", 144_000_000, 148_000_000),
    band("1.25 m", 222_000_000, 225_000_000),
    band("70 cm", 420_000_000, 450_000_000),
    band("33 cm", 902_000_000, 928_000_000),
    band("23 cm", 1_240_000_000, 1_300_000_000),
    band("13 cm", 2_300_000_000, 2_310_000_000),
    band("13 cm", 2_390_000_000, 2_450_000_000),
    band("9 cm", 3_300_000_000, 3_500_000_000),
    band("5 cm", 5_650_000_000, 5_925_000_000),
];

/// The bands of `table` that overlap `start_hz..=stop_hz`, in order.
pub fn bands_within(table: &'static [Band], start_hz: u64, stop_hz: u64) -> impl Iterator<Item = &'static Band> {
    table.iter().filter(move |b| b.stop_hz >= start_hz && b.start_hz <= stop_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_sorted_and_disjoint() {
        for region in Region::ALL {
            let table = region.bands();
            for w in table.windows(2) {
                assert!(w[0].stop_hz < w[1].start_hz, "{}: {:?} overlaps {:?}", region.name(), w[0], w[1]);
            }
            for b in table {
                assert!(b.start_hz < b.stop_hz, "{b:?}");
            }
            assert_eq!(Region::from_number(region.number()), Some(region));
        }
        assert_eq!(Region::from_number(3), None);
    }

    #[test]
    fn region_2_is_wider_where_the_americas_are() {
        let forty = |t: &'static [Band]| t.iter().find(|b| b.name == "40 m").unwrap().stop_hz;
        assert_eq!(forty(REGION_1), 7_200_000);
        assert_eq!(forty(REGION_2), 7_300_000);
        let two_m = REGION_2.iter().find(|b| b.name == "2 m").unwrap();
        assert_eq!((two_m.start_hz, two_m.stop_hz), (144_000_000, 148_000_000));
    }

    #[test]
    fn margin_adds_a_share_of_the_width_on_each_side() {
        let forty = REGION_2.iter().find(|b| b.name == "40 m").unwrap();
        assert_eq!(forty.with_margin(0.1), (6_970_000, 7_330_000));
        assert_eq!(forty.with_margin(0.0), (7_000_000, 7_300_000));
        // A negative fraction is taken as none; the start never drops below 1 Hz.
        assert_eq!(forty.with_margin(-1.0), (7_000_000, 7_300_000));
        assert_eq!(REGION_1[0].with_margin(100.0).0, 1);
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
