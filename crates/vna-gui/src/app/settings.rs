//! What is remembered between sessions, and how stored settings of older versions are read.

use super::*;

/// Version written into the stored settings; see [`Settings::version`].
pub(super) const SETTINGS_VERSION: u32 = 1;

/// Read a list that older versions stored as a fixed-size array. eframe keeps
/// the settings as RON, where an array is a tuple `(a,b,c)` and a `Vec` is a
/// list `[a,b,c]`; taking either keeps settings from before 0.3 readable.
pub(super) fn list_or_tuple<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Items<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Items<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a list or a tuple")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element()? {
                out.push(item);
            }
            Ok(out)
        }
    }
    d.deserialize_any(Items(std::marker::PhantomData))
}

/// Everything worth remembering between sessions (stored by eframe).
///
/// Compatibility rules: every field has a default (`#[serde(default)]`), so a
/// file from an older version loads with the new fields at their defaults,
/// and unknown fields from a newer version are ignored. Lists are `Vec`s that
/// `apply_settings` cuts or pads to what this build uses, so changing the
/// number of markers or plots does not discard the whole file. A change that
/// these rules cannot absorb raises `SETTINGS_VERSION` and gets a branch in
/// `Settings::migrated`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub(super) struct Settings {
    /// 0 (absent) for settings stored before 0.3.
    pub(super) version: u32,
    pub(super) port: Option<String>,
    pub(super) start: String,
    pub(super) stop: String,
    pub(super) points: u16,
    pub(super) average: u8,
    pub(super) cal_file: Option<PathBuf>,
    pub(super) cal_apply: bool,
    pub(super) show_s11: bool,
    pub(super) show_s21: bool,
    /// The amateur bands drawn behind the frequency plots.
    pub(super) show_bands: bool,
    pub(super) view_tdr: bool,
    pub(super) tdr_vf: f64,
    pub(super) tdr_window: String,
    pub(super) tdr_max_m: f64,
    #[serde(deserialize_with = "list_or_tuple")]
    pub(super) marker_freqs: Vec<Option<u64>>,
    pub(super) fit_f_min: String,
    pub(super) fit_as_delay: bool,
    /// Trace kind per plot (ids from TraceKind::id), top-right, bottom-left, bottom-right.
    #[serde(deserialize_with = "list_or_tuple")]
    pub(super) plot_kinds: Vec<String>,
    pub(super) gd_aperture: usize,
    pub(super) gate_on: bool,
    pub(super) gate_start_m: f64,
    pub(super) gate_stop_m: f64,
    pub(super) gate_notch: bool,
    /// Port extension, one-way delay in picoseconds.
    pub(super) port_ext_ps: f64,
    /// Number of sweeps averaged in software (1 = off).
    pub(super) sweep_avg: usize,
    /// Sweeps left out of the software average per point (outliers), 0 = plain mean.
    pub(super) avg_drop: usize,
    /// Segments beyond the primary one (start/stop/points/calibration file).
    pub(super) extra_segments: Vec<SegmentSettings>,
    /// Developer menu and log window (off for normal use).
    pub(super) dev_tools: bool,
    /// Connect to the VNA at start when one is plugged in (View ▸ Connect at start).
    pub(super) auto_connect: bool,
    /// The held references, in order; their data are files next to the settings (see `plots::REFERENCE_DIR`).
    pub(super) references: Vec<ReferenceSettings>,
    /// The reference the Δ trace is taken against.
    pub(super) diff_ref: Option<usize>,
}

/// Name and visibility of a held reference. The trace itself is the file
/// `ref-<position>.s2p` in the references directory.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(default)]
pub(super) struct ReferenceSettings {
    pub(super) name: String,
    pub(super) visible: bool,
}

/// One extra sweep segment. The primary segment is the Start/Stop/Points of
/// the Sweep panel and uses the calibration panel; an extra segment may carry
/// its own calibration file (e.g. a dense HF calibration next to a 6.3 GHz one).
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(default)]
pub(super) struct SegmentSettings {
    pub(super) start: String,
    pub(super) stop: String,
    pub(super) points: u16,
    pub(super) cal_file: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // The defaults stand in for whatever a stored file lacks, so a
            // file without a version reads as 0; `App::settings` writes the
            // current one.
            version: 0,
            port: None,
            start: "1M".into(),
            stop: "30M".into(),
            points: 201,
            average: 1,
            cal_file: None,
            cal_apply: true,
            show_s11: true,
            show_s21: true,
            show_bands: false,
            view_tdr: false,
            tdr_vf: 0.70,
            tdr_window: "hann".into(),
            tdr_max_m: 2.0,
            marker_freqs: vec![None; MARKERS],
            fit_f_min: "300M".into(),
            fit_as_delay: false,
            plot_kinds: vec!["logmag".into(), "swr".into(), "phase".into()],
            gd_aperture: 2,
            gate_on: false,
            gate_start_m: -0.1,
            gate_stop_m: 0.5,
            gate_notch: false,
            port_ext_ps: 0.0,
            sweep_avg: 1,
            avg_drop: 0,
            extra_segments: Vec::new(),
            dev_tools: false,
            auto_connect: false,
            references: Vec::new(),
            diff_ref: None,
        }
    }
}

impl Settings {
    /// Bring settings stored by an older version up to `SETTINGS_VERSION`.
    /// Versions 0 and 1 differ only in how the two lists are written, which
    /// `list_or_tuple` already reads; a later incompatible change adds its
    /// conversion here.
    pub(super) fn migrated(mut self) -> Settings {
        self.version = SETTINGS_VERSION;
        self
    }
}

impl App {
    pub(super) fn apply_settings(&mut self, s: Settings) {
        let s = s.migrated();
        if let Some(p) = &s.port {
            if self.ports.iter().any(|c| &c.path == p) {
                self.selected_port = Some(p.clone());
            }
        }
        self.start = s.start;
        self.stop = s.stop;
        self.points = s.points;
        self.average = s.average;
        self.cal.apply = s.cal_apply;
        self.show_s11 = s.show_s11;
        self.show_s21 = s.show_s21;
        self.show_bands = s.show_bands;
        self.view = if s.view_tdr { View::Tdr } else { View::Frequency };
        // A hand-edited or future settings file may hold 0, NaN or nonsense;
        // the DragValues only clamp on interaction.
        let finite_or = |v: f64, lo: f64, hi: f64, default: f64| if v.is_finite() { v.clamp(lo, hi) } else { default };
        self.tdr_vf = finite_or(s.tdr_vf, 0.3, 1.0, 0.70);
        self.tdr_window = Window::parse(&s.tdr_window).unwrap_or(Window::Hann);
        self.tdr_max_m = finite_or(s.tdr_max_m, 0.1, 1000.0, 2.0);
        let mut marker_freqs = [None; MARKERS];
        for (slot, f) in marker_freqs.iter_mut().zip(&s.marker_freqs) {
            *slot = *f;
        }
        self.pending_marker_freqs = Some(marker_freqs);
        self.cal.fit_f_min = s.fit_f_min;
        self.cal.fit_as_delay = s.fit_as_delay;
        for (slot, id) in self.plot_kinds.iter_mut().zip(&s.plot_kinds) {
            if let Some(kind) = TraceKind::parse(id) {
                *slot = kind;
            }
        }
        self.gd_aperture = s.gd_aperture.clamp(1, 50);
        self.gate_on = s.gate_on;
        self.gate_start_m = finite_or(s.gate_start_m, -1000.0, 1000.0, -0.1);
        self.gate_stop_m = finite_or(s.gate_stop_m, -1000.0, 1000.0, 0.5);
        if self.gate_stop_m <= self.gate_start_m {
            self.gate_stop_m = self.gate_start_m + 0.01;
        }
        self.gate_notch = s.gate_notch;
        self.port_ext_ps = finite_or(s.port_ext_ps, -PORT_EXT_MAX_PS, PORT_EXT_MAX_PS, 0.0);
        self.sweep_avg = s.sweep_avg.clamp(1, MAX_SWEEP_AVG);
        self.avg_drop = s.avg_drop.min(self.sweep_avg.saturating_sub(2));
        self.extra_segments = s.extra_segments;
        self.dev_tools = s.dev_tools;
        self.auto_connect = s.auto_connect;
        if let Some(dir) = references_dir() {
            self.references = read_references(&dir, &s.references);
            self.diff_ref = s.diff_ref.filter(|k| *k < self.references.len());
        }
        if let Some(path) = s.cal_file {
            if path.exists() {
                self.load_cal_from(&path);
            }
        }
    }

    pub(super) fn settings(&self) -> Settings {
        let mut marker_freqs = [None; MARKERS];
        if let Some(sweep) = &self.shown {
            for (k, m) in self.markers.iter().enumerate() {
                marker_freqs[k] = m.and_then(|i| sweep.freqs_hz.get(i).copied());
            }
        } else if let Some(p) = self.pending_marker_freqs {
            marker_freqs = p;
        }
        Settings {
            version: SETTINGS_VERSION,
            port: self.selected_port.clone(),
            start: self.start.clone(),
            stop: self.stop.clone(),
            points: self.points,
            average: self.average,
            cal_file: self.cal.file.clone(),
            cal_apply: self.cal.apply,
            show_s11: self.show_s11,
            show_s21: self.show_s21,
            show_bands: self.show_bands,
            view_tdr: self.view == View::Tdr,
            tdr_vf: self.tdr_vf,
            tdr_window: self.tdr_window.name().into(),
            tdr_max_m: self.tdr_max_m,
            marker_freqs: marker_freqs.to_vec(),
            fit_f_min: self.cal.fit_f_min.clone(),
            fit_as_delay: self.cal.fit_as_delay,
            plot_kinds: self.plot_kinds.iter().map(|k| k.id().to_string()).collect(),
            gd_aperture: self.gd_aperture,
            gate_on: self.gate_on,
            gate_start_m: self.gate_start_m,
            gate_stop_m: self.gate_stop_m,
            gate_notch: self.gate_notch,
            port_ext_ps: self.port_ext_ps,
            sweep_avg: self.sweep_avg,
            avg_drop: self.avg_drop,
            extra_segments: self.extra_segments.clone(),
            dev_tools: self.dev_tools,
            auto_connect: self.auto_connect,
            references: self
                .references
                .iter()
                .map(|r| ReferenceSettings {
                    name: r.name.clone(),
                    visible: r.visible,
                })
                .collect(),
            diff_ref: self.diff_ref,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings exactly as vna-r 0.2.5 stored them (arrays as RON tuples, no version).
    const SETTINGS_0_2_5: &str = r#"(port:Some("/dev/cu.usbmodemv1_01"),start:"50k",stop:"6,3G",points:101,average:1,cal_file:Some("/tmp/n-6g3.cal.json"),cal_apply:true,show_s11:true,show_s21:false,view_tdr:false,tdr_vf:0.85,tdr_window:"hann",tdr_max_m:7.4,marker_freqs:(Some(6111001500),None,None,None),fit_f_min:"300M",fit_as_delay:false,plot_kinds:("logmag","swr","phase"),gd_aperture:2,gate_on:false,gate_start_m:0.22,gate_stop_m:0.7,gate_notch:false,sweep_avg:1,extra_segments:[],dev_tools:false)"#;

    #[test]
    fn settings_from_0_2_5_load_completely() {
        let s: Settings = ron::from_str(SETTINGS_0_2_5).expect("old settings must stay readable");
        assert_eq!(s.version, 0);
        assert_eq!(s.stop, "6,3G");
        assert_eq!(s.points, 101);
        assert_eq!(s.tdr_vf, 0.85);
        assert_eq!(s.marker_freqs, vec![Some(6_111_001_500), None, None, None]);
        assert_eq!(s.plot_kinds, vec!["logmag", "swr", "phase"]);
        assert_eq!(s.migrated().version, SETTINGS_VERSION);
    }

    #[test]
    fn settings_round_trip_and_tolerate_other_list_lengths() {
        let s = Settings {
            marker_freqs: vec![Some(14_200_000), None, Some(28_000_000), None],
            ..Default::default()
        };
        let text = ron::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
        // A build with more markers or plots wrote longer lists, an older one shorter; neither is an error.
        let longer = text.replace("plot_kinds:[", "plot_kinds:[\"phase\",");
        assert_eq!(ron::from_str::<Settings>(&longer).unwrap().plot_kinds.len(), 4);
        // Unknown fields from a newer version are ignored, missing ones take their default.
        let sparse: Settings = ron::from_str(r#"(version:7,start:"2M",some_future_field:3)"#).unwrap();
        assert_eq!(sparse.start, "2M");
        assert_eq!(sparse.stop, Settings::default().stop);
    }
}
