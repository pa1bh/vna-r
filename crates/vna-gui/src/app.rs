//! Application state and all panels.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Color32, RichText, Vec2};
use egui_plot::{Legend, Line, Plot, PlotPoints, VLine};
use litevna::calibration::{plausibility_warning, CalKit, Calibration, CalibrationSet, OpenFit, OpenModel, Standard};
use litevna::discovery::{self, PortCandidate};
use litevna::freq::{format_hz, parse_hz};
use litevna::timedomain::{gate, tdr, Gate, GateKind, TdrConfig, TdrResult, Window, C0};
use litevna::touchstone::{db, swr, write_s2p};
use litevna::traces::{self, impedance, Param, TraceKind};
use litevna::{DeviceInfo, Screenshot, SweepConfig, SweepResult};
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::smith::SmithChart;
use crate::worker::{self, Cmd, Event, Purpose, WorkerHandle};

const S11_COLOR: Color32 = Color32::from_rgb(255, 204, 0);
const S21_COLOR: Color32 = Color32::from_rgb(80, 220, 120);
const GATE_COLOR: Color32 = Color32::from_rgb(255, 150, 80);
const MARKER_COLOR: Color32 = Color32::from_rgb(255, 90, 90);
const POINT_CHOICES: [u16; 8] = [51, 101, 201, 401, 801, 1001, 1601, 3201];
const MARKERS: usize = 4;
const MARKER_COLORS: [Color32; MARKERS] = [
    Color32::from_rgb(255, 90, 90),
    Color32::from_rgb(90, 200, 255),
    Color32::from_rgb(255, 120, 255),
    Color32::from_rgb(255, 170, 60),
];

/// Everything worth remembering between sessions (stored by eframe).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    port: Option<String>,
    start: String,
    stop: String,
    points: u16,
    average: u8,
    cal_file: Option<PathBuf>,
    cal_apply: bool,
    show_s11: bool,
    show_s21: bool,
    view_tdr: bool,
    tdr_vf: f64,
    tdr_window: String,
    tdr_max_m: f64,
    marker_freqs: [Option<u64>; MARKERS],
    fit_f_min: String,
    fit_as_delay: bool,
    /// Trace kind per plot (ids from TraceKind::id), top-right, bottom-left, bottom-right.
    plot_kinds: [String; 3],
    gd_aperture: usize,
    gate_on: bool,
    gate_start_m: f64,
    gate_stop_m: f64,
    gate_notch: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            port: None,
            start: "1M".into(),
            stop: "30M".into(),
            points: 201,
            average: 1,
            cal_file: None,
            cal_apply: true,
            show_s11: true,
            show_s21: true,
            view_tdr: false,
            tdr_vf: 0.70,
            tdr_window: "hann".into(),
            tdr_max_m: 2.0,
            marker_freqs: [None; MARKERS],
            fit_f_min: "300M".into(),
            fit_as_delay: false,
            plot_kinds: ["logmag".into(), "swr".into(), "phase".into()],
            gd_aperture: 2,
            gate_on: false,
            gate_start_m: -0.1,
            gate_stop_m: 0.5,
            gate_notch: false,
        }
    }
}

enum Connection {
    Disconnected,
    Connecting(String),
    Connected { port: String, info: DeviceInfo, battery_mv: Option<u16> },
}

#[derive(Default)]
struct CalState {
    set: Option<CalibrationSet>,
    file: Option<PathBuf>,
    /// Calibration solved for a specific grid (cache).
    solved: Option<(Vec<u64>, Calibration)>,
    apply: bool,
    pending: Option<Standard>,
    message: Option<String>,
    /// Plausibility warning about the last stored standard.
    warning: Option<String>,
    /// Warning about interpolating this calibration onto the current sweep grid.
    grid_warning: Option<String>,
    /// Lowest frequency used by the kit fit (text, parsed with parse_hz).
    fit_f_min: String,
    fit_as_delay: bool,
    /// Last fit result, not yet applied to the kit.
    fit: Option<(OpenFit, OpenModel)>,
}

impl CalState {
    fn fit_model(&self) -> OpenModel {
        if self.fit_as_delay { OpenModel::Delay } else { OpenModel::Capacitance }
    }
}

impl CalState {
    fn calibration_for(&mut self, freqs: &[u64]) -> Option<&Calibration> {
        let Some(set) = &self.set else { return None };
        let fresh = matches!(&self.solved, Some((g, _)) if g.as_slice() == freqs);
        if !fresh {
            match set.solve_for(freqs) {
                Ok(cal) => {
                    self.solved = Some((freqs.to_vec(), cal));
                    self.message = None;
                    self.grid_warning = set.coarse_low_end_warning(freqs);
                }
                Err(e) => {
                    self.solved = None;
                    self.grid_warning = None;
                    self.message = Some(e.to_string());
                    return None;
                }
            }
        }
        self.solved.as_ref().map(|(_, c)| c)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct GateKey {
    shown_rev: u64,
    vf: f64,
    window: Window,
    start_m: f64,
    stop_m: f64,
    notch: bool,
}

pub struct App {
    worker: WorkerHandle,
    ports: Vec<PortCandidate>,
    selected_port: Option<String>,
    connection: Connection,

    start: String,
    stop: String,
    points: u16,
    sweep_error: Option<String>,
    average: u8,
    running: bool,

    raw: Option<SweepResult>,
    shown: Option<SweepResult>,
    last_cfg: Option<SweepConfig>,
    sweep_count: u64,

    cal: CalState,
    markers: [Option<usize>; MARKERS],
    active_marker: usize,
    /// Marker frequencies from the last session, mapped onto the first sweep.
    pending_marker_freqs: Option<[Option<u64>; MARKERS]>,
    show_s11: bool,
    show_s21: bool,
    plot_kinds: [TraceKind; 3],
    /// Half-width (points) of the group-delay difference.
    gd_aperture: usize,

    error: Option<String>,
    screenshot: Option<(egui::TextureHandle, Screenshot)>,
    show_screenshot: bool,

    view: View,
    tdr_vf: f64,
    tdr_window: Window,
    tdr_max_m: f64,
    tdr_marker: Option<usize>,
    /// Bumped whenever `shown` changes (new sweep, calibration toggled or loaded).
    shown_rev: u64,
    gate_on: bool,
    /// Gate edges as one-way distance (m), like the TDR x axis.
    gate_start_m: f64,
    gate_stop_m: f64,
    gate_notch: bool,
    /// Which gate edge the pointer hovers / drags in the TDR plots (0 = start, 1 = stop).
    gate_hover: Option<usize>,
    gate_drag: Option<usize>,
    gate_error: Option<String>,
    gated_cache: Option<(GateKey, Vec<Complex64>, f64)>,
    /// (sweep number, vf, window) the cached result belongs to.
    /// Keyed on `shown_rev`, VF and window.
    tdr_cache: Option<(u64, f64, Window, TdrResult)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Frequency,
    Tdr,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let worker = worker::spawn(cc.egui_ctx.clone());
        let mut app = App {
            worker,
            ports: Vec::new(),
            selected_port: None,
            connection: Connection::Disconnected,
            start: "1M".into(),
            stop: "30M".into(),
            points: 201,
            sweep_error: None,
            average: 1,
            running: false,
            raw: None,
            shown: None,
            last_cfg: None,
            sweep_count: 0,
            cal: CalState { apply: true, ..Default::default() },
            markers: [None; MARKERS],
            active_marker: 0,
            pending_marker_freqs: None,
            show_s11: true,
            show_s21: true,
            plot_kinds: [TraceKind::LogMag, TraceKind::Swr, TraceKind::Phase],
            gd_aperture: 2,
            error: None,
            screenshot: None,
            show_screenshot: false,
            view: View::Frequency,
            tdr_vf: 0.70,
            tdr_window: Window::Hann,
            tdr_max_m: 2.0,
            tdr_marker: None,
            shown_rev: 0,
            gate_on: false,
            gate_start_m: -0.1,
            gate_stop_m: 0.5,
            gate_notch: false,
            gate_hover: None,
            gate_drag: None,
            gate_error: None,
            gated_cache: None,
            tdr_cache: None,
        };
        app.refresh_ports();
        if let Some(storage) = cc.storage {
            if let Some(settings) = eframe::get_value::<Settings>(storage, eframe::APP_KEY) {
                app.apply_settings(settings);
            }
        }
        app
    }

    fn apply_settings(&mut self, s: Settings) {
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
        self.view = if s.view_tdr { View::Tdr } else { View::Frequency };
        self.tdr_vf = s.tdr_vf;
        self.tdr_window = Window::parse(&s.tdr_window).unwrap_or(Window::Hann);
        self.tdr_max_m = s.tdr_max_m;
        self.pending_marker_freqs = Some(s.marker_freqs);
        self.cal.fit_f_min = s.fit_f_min;
        self.cal.fit_as_delay = s.fit_as_delay;
        for (k, id) in s.plot_kinds.iter().enumerate() {
            if let Some(kind) = TraceKind::parse(id) {
                self.plot_kinds[k] = kind;
            }
        }
        self.gd_aperture = s.gd_aperture.clamp(1, 50);
        self.gate_on = s.gate_on;
        self.gate_start_m = s.gate_start_m;
        self.gate_stop_m = s.gate_stop_m;
        self.gate_notch = s.gate_notch;
        if let Some(path) = s.cal_file {
            if path.exists() {
                self.load_cal_from(&path);
            }
        }
    }

    fn settings(&self) -> Settings {
        let mut marker_freqs = [None; MARKERS];
        if let Some(sweep) = &self.shown {
            for (k, m) in self.markers.iter().enumerate() {
                marker_freqs[k] = m.and_then(|i| sweep.freqs_hz.get(i).copied());
            }
        } else if let Some(p) = self.pending_marker_freqs {
            marker_freqs = p;
        }
        Settings {
            port: self.selected_port.clone(),
            start: self.start.clone(),
            stop: self.stop.clone(),
            points: self.points,
            average: self.average,
            cal_file: self.cal.file.clone(),
            cal_apply: self.cal.apply,
            show_s11: self.show_s11,
            show_s21: self.show_s21,
            view_tdr: self.view == View::Tdr,
            tdr_vf: self.tdr_vf,
            tdr_window: self.tdr_window.name().into(),
            tdr_max_m: self.tdr_max_m,
            marker_freqs,
            fit_f_min: self.cal.fit_f_min.clone(),
            fit_as_delay: self.cal.fit_as_delay,
            plot_kinds: [self.plot_kinds[0].id().into(), self.plot_kinds[1].id().into(), self.plot_kinds[2].id().into()],
            gd_aperture: self.gd_aperture,
            gate_on: self.gate_on,
            gate_start_m: self.gate_start_m,
            gate_stop_m: self.gate_stop_m,
            gate_notch: self.gate_notch,
        }
    }

    fn refresh_ports(&mut self) {
        self.ports = discovery::list_ports().unwrap_or_default();
        if self.selected_port.is_none() {
            self.selected_port = self.ports.iter().find(|p| p.looks_like_vna()).map(|p| p.path.clone());
        }
    }

    fn connected(&self) -> bool {
        matches!(self.connection, Connection::Connected { .. })
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.worker.tx.send(cmd);
    }

    fn current_config(&mut self) -> Option<SweepConfig> {
        let cfg = parse_hz(&self.start)
            .and_then(|s| parse_hz(&self.stop).map(|e| (s, e)))
            .and_then(|(s, e)| SweepConfig::linear(s, e, self.points));
        match cfg {
            Ok(c) => {
                self.sweep_error = None;
                Some(c)
            }
            Err(e) => {
                self.sweep_error = Some(e.to_string());
                None
            }
        }
    }

    fn start_sweep(&mut self, continuous: bool) {
        if let Some(cfg) = self.current_config() {
            self.running = continuous;
            self.send(Cmd::Sweep { cfg, purpose: Purpose::Display, continuous });
        }
    }

    fn stop_sweep(&mut self) {
        self.running = false;
        self.send(Cmd::Stop);
    }

    // ----- events ----------------------------------------------------------

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(evt) = self.worker.rx.try_recv() {
            match evt {
                Event::Connected { port, info, battery_mv } => {
                    self.connection = Connection::Connected { port, info, battery_mv };
                    self.error = None;
                }
                Event::Disconnected => {
                    self.connection = Connection::Disconnected;
                    self.running = false;
                }
                Event::SweepDone { cfg, result, purpose } => match purpose {
                    Purpose::Display => {
                        self.last_cfg = Some(cfg);
                        self.sweep_count += 1;
                        let corrected = self.corrected(&result);
                        self.set_shown(corrected);
                        if let Some(freqs) = self.pending_marker_freqs.take() {
                            for (k, f) in freqs.iter().enumerate() {
                                self.markers[k] = f.map(|f| nearest_index(&result.freqs_hz, f));
                            }
                        }
                        self.raw = Some(result);
                    }
                    Purpose::Standard(std) => {
                        self.cal.pending = None;
                        self.store_standard(std, &result);
                    }
                },
                Event::Screenshot(shot) => {
                    let rgb = shot.to_rgb888(true);
                    let image = egui::ColorImage::from_rgb([shot.width as usize, shot.height as usize], &rgb);
                    let tex = ctx.load_texture("screenshot", image, egui::TextureOptions::LINEAR);
                    self.screenshot = Some((tex, shot));
                    self.show_screenshot = true;
                }
                Event::Error(msg) => {
                    self.error = Some(msg);
                    self.running = false;
                    self.cal.pending = None;
                    if let Connection::Connecting(_) = self.connection {
                        self.connection = Connection::Disconnected;
                    }
                }
            }
        }
    }

    fn set_shown(&mut self, sweep: SweepResult) {
        self.shown = Some(sweep);
        self.shown_rev += 1;
    }

    /// Round-trip seconds for a one-way distance at the current VF.
    fn gate_time(&self, d_m: f64) -> f64 {
        2.0 * d_m / (C0 * self.tdr_vf)
    }

    /// Gated S11 of the shown sweep (cached), with the unreliable band-edge width in Hz.
    fn gated_s11(&mut self) -> Option<(Vec<Complex64>, f64)> {
        if !self.gate_on {
            return None;
        }
        let sweep = self.shown.as_ref()?;
        let key = GateKey { shown_rev: self.shown_rev, vf: self.tdr_vf, window: self.tdr_window, start_m: self.gate_start_m, stop_m: self.gate_stop_m, notch: self.gate_notch };
        if !matches!(&self.gated_cache, Some((k, _, _)) if *k == key) {
            let cfg = TdrConfig { window: self.tdr_window, velocity_factor: self.tdr_vf, fft_size: 0 };
            let g = Gate {
                start_s: self.gate_time(self.gate_start_m),
                stop_s: self.gate_time(self.gate_stop_m),
                kind: if self.gate_notch { GateKind::Notch } else { GateKind::BandPass },
            };
            match gate(&sweep.freqs_hz, &sweep.s11, &cfg, &g) {
                Ok(r) => {
                    self.gated_cache = Some((key, r.s11, r.edge_hz));
                    self.gate_error = None;
                }
                Err(e) => {
                    self.gated_cache = None;
                    self.gate_error = Some(e.to_string());
                    return None;
                }
            }
        }
        self.gated_cache.as_ref().map(|(_, s, e)| (s.clone(), *e))
    }

    /// The sweep the frequency view works on: `shown`, with S11 gated when the gate is on.
    fn freq_sweep(&mut self) -> Option<SweepResult> {
        let mut sweep = self.shown.clone()?;
        if let Some((s11, _)) = self.gated_s11() {
            sweep.s11 = s11;
        }
        Some(sweep)
    }

    fn gate_tag(&self, ui: &mut egui::Ui) {
        if self.gate_on && self.gated_cache.is_some() {
            ui.label(RichText::new("GATED").color(GATE_COLOR).strong()).on_hover_text("S11 is time-gated; see the TDR panel");
        }
    }

    fn corrected(&mut self, raw: &SweepResult) -> SweepResult {
        if !self.cal.apply {
            return raw.clone();
        }
        match self.cal.calibration_for(&raw.freqs_hz) {
            Some(cal) => cal.apply(raw).unwrap_or_else(|_| raw.clone()),
            None => raw.clone(),
        }
    }

    fn store_standard(&mut self, std: Standard, sweep: &SweepResult) {
        self.cal.warning = plausibility_warning(std, sweep);
        self.cal.fit = None;
        let needs_new = match &self.cal.set {
            Some(set) => set.freqs_hz != sweep.freqs_hz,
            None => true,
        };
        if needs_new {
            if self.cal.set.is_some() {
                self.cal.message = Some("grid changed: started a new calibration".into());
            }
            self.cal.set = Some(CalibrationSet::new(sweep.freqs_hz.clone(), CalKit::ideal()));
            self.cal.file = None;
        }
        if let Some(set) = &mut self.cal.set {
            match set.set(std, sweep) {
                Ok(()) => self.cal.message = Some(format!("measured {}", std.name())),
                Err(e) => self.cal.message = Some(e.to_string()),
            }
        }
        self.cal.solved = None;
        if let Some(raw) = self.raw.clone() {
            let corrected = self.corrected(&raw);
            self.set_shown(corrected);
        }
    }

    // ----- panels ----------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("vna-r");
            ui.separator();
            let connected = self.connected();
            ui.add_enabled_ui(!connected, |ui| {
                let selected_text = self.selected_port.clone().unwrap_or_else(|| "select port".into());
                egui::ComboBox::from_id_salt("port").selected_text(selected_text).width(260.0).show_ui(ui, |ui| {
                    for p in &self.ports {
                        let label = match &p.usb {
                            Some(u) => format!("{} {} {}", if p.looks_like_vna() { "★" } else { " " }, p.path, u.product.clone().unwrap_or_default()),
                            None => format!("  {}", p.path),
                        };
                        ui.selectable_value(&mut self.selected_port, Some(p.path.clone()), label);
                    }
                });
                if ui.button("⟳").on_hover_text("rescan ports").clicked() {
                    self.refresh_ports();
                }
            });
            match &self.connection {
                Connection::Disconnected => {
                    if ui.add_enabled(self.selected_port.is_some(), egui::Button::new("Connect")).clicked() {
                        let port = self.selected_port.clone().unwrap();
                        self.connection = Connection::Connecting(port.clone());
                        self.send(Cmd::Connect(port));
                    }
                }
                Connection::Connecting(p) => {
                    ui.spinner();
                    ui.label(format!("connecting {p}…"));
                }
                Connection::Connected { port, info, battery_mv } => {
                    if ui.button("Disconnect").clicked() {
                        self.running = false;
                        self.send(Cmd::Disconnect);
                    }
                    ui.label(
                        RichText::new(format!(
                            "LiteVNA  fw {}.{}  hw {}  {}",
                            info.fw_major,
                            info.fw_minor,
                            info.hardware_revision,
                            battery_mv.map(|mv| format!("{:.2} V", mv as f64 / 1000.0)).unwrap_or_default()
                        ))
                        .color(Color32::from_rgb(120, 200, 120)),
                    )
                    .on_hover_text(port);
                }
            }
            ui.separator();
            ui.add_enabled_ui(connected, |ui| {
                if self.running {
                    if ui.button("⏹ Stop").clicked() {
                        self.stop_sweep();
                    }
                } else if ui.button("▶ Run").clicked() {
                    self.start_sweep(true);
                }
                if ui.add_enabled(!self.running, egui::Button::new("Single")).clicked() {
                    self.start_sweep(false);
                }
                if ui.button("📷 Screenshot").clicked() {
                    self.send(Cmd::Screenshot);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(err) = self.error.clone() {
                    if ui.small_button("✕").clicked() {
                        self.error = None;
                    }
                    ui.colored_label(Color32::from_rgb(255, 110, 110), err);
                }
            });
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Sweep");
            egui::Grid::new("sweep-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Start");
                ui.text_edit_singleline(&mut self.start);
                ui.end_row();
                ui.label("Stop");
                ui.text_edit_singleline(&mut self.stop);
                ui.end_row();
                ui.label("Points");
                egui::ComboBox::from_id_salt("points").selected_text(self.points.to_string()).show_ui(ui, |ui| {
                    for p in POINT_CHOICES {
                        ui.selectable_value(&mut self.points, p, p.to_string());
                    }
                });
                ui.end_row();
                ui.label("Average");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.average).range(1..=80));
                    if ui.add_enabled(self.connected(), egui::Button::new("set")).on_hover_text("writes register 0x40; also changes the bandwidth in the device menu").clicked() {
                        self.send(Cmd::SetAverage(self.average));
                    }
                });
                ui.end_row();
            });
            if let Some(e) = &self.sweep_error {
                ui.colored_label(Color32::from_rgb(255, 110, 110), e);
            }
            if ui.add_enabled(self.connected(), egui::Button::new("Apply sweep")).clicked() {
                self.start_sweep(self.running);
            }
            ui.small("Frequencies: 50k, 14.2M, 1.3G. Applying while running restarts the sweep.");

            ui.add_space(12.0);
            ui.heading("Calibration");
            self.calibration_panel(ui);
            egui::CollapsingHeader::new("Kit fit (line open / line short)").default_open(false).show(ui, |ui| {
                self.kit_fit_panel(ui);
            });

            ui.add_space(12.0);
            ui.heading("Traces");
            ui.checkbox(&mut self.show_s11, RichText::new("S11").color(S11_COLOR));
            ui.checkbox(&mut self.show_s21, RichText::new("S21").color(S21_COLOR));
            ui.horizontal(|ui| {
                ui.label("group delay aperture");
                ui.add(egui::DragValue::new(&mut self.gd_aperture).range(1..=50).suffix(" pts"));
            });
            ui.small("Pick what each plot shows in its header. Group delay = −dφ/dω over ±aperture points.");

            ui.add_space(12.0);
            ui.heading("Time domain (TDR)");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view, View::Frequency, "Frequency");
                ui.selectable_value(&mut self.view, View::Tdr, "TDR");
            });
            egui::Grid::new("tdr-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Velocity factor");
                ui.add(egui::DragValue::new(&mut self.tdr_vf).range(0.3..=1.0).speed(0.005).fixed_decimals(3));
                ui.end_row();
                ui.label("Window");
                egui::ComboBox::from_id_salt("tdr-window").selected_text(self.tdr_window.name()).show_ui(ui, |ui| {
                    for w in [Window::Rectangular, Window::Hann, Window::Blackman] {
                        ui.selectable_value(&mut self.tdr_window, w, w.name());
                    }
                });
                ui.end_row();
                ui.label("Max distance");
                ui.add(egui::DragValue::new(&mut self.tdr_max_m).range(0.1..=1000.0).speed(0.05).suffix(" m"));
                ui.end_row();
            });
            if let Some((_, _, _, t)) = &self.tdr_cache {
                let (res_m, range_m) = (t.resolution_m(), t.max_distance_m());
                ui.small(format!("resolution {:.3} m, alias-free range {:.1} m", res_m, range_m));
                // Resolution is 1/span: a narrow HF sweep smears a cable into one blob.
                if res_m > self.tdr_max_m / 10.0 {
                    let needed_span = litevna::timedomain::C0 * self.tdr_vf / (2.0 * self.tdr_max_m / 20.0);
                    let stop = needed_span.min(6.3e9);
                    ui.colored_label(
                        Color32::from_rgb(230, 150, 80),
                        format!(
                            "Span too small for {:.1} m: pulses are {:.2} m wide. Sweep to ≥ {} for detail.",
                            self.tdr_max_m,
                            res_m,
                            format_hz(stop as u64)
                        ),
                    );
                    if ui.add_enabled(self.connected(), egui::Button::new("Set sweep for TDR")).on_hover_text("50 kHz to the needed stop, 1001 points").clicked() {
                        self.start = "50k".into();
                        self.stop = format_hz(stop as u64).replace(' ', "");
                        self.points = 1001;
                        self.start_sweep(self.running);
                    }
                }
            }

            ui.add_space(6.0);
            ui.checkbox(&mut self.gate_on, "Gate").on_hover_text("Time-gate S11: keep (band pass) or remove (notch) the reflections between start and stop. Applies to the frequency view and exports.");
            if self.gate_on {
                egui::Grid::new("gate-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Start");
                    ui.add(egui::DragValue::new(&mut self.gate_start_m).speed(0.005).fixed_decimals(3).suffix(" m"));
                    ui.end_row();
                    ui.label("Stop");
                    ui.add(egui::DragValue::new(&mut self.gate_stop_m).speed(0.005).fixed_decimals(3).suffix(" m"));
                    ui.end_row();
                    ui.label("Kind");
                    egui::ComboBox::from_id_salt("gate-kind").selected_text(if self.gate_notch { "notch" } else { "band pass" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.gate_notch, false, "band pass");
                        ui.selectable_value(&mut self.gate_notch, true, "notch");
                    });
                    ui.end_row();
                });
                if self.gate_stop_m <= self.gate_start_m {
                    self.gate_stop_m = self.gate_start_m + 0.01;
                }
                let res_m = self.tdr_cache.as_ref().map(|(_, _, _, t)| t.resolution_m());
                let marker_d = self.tdr_marker.and_then(|i| self.tdr_cache.as_ref().and_then(|(_, _, _, t)| t.distance_m.get(i).copied()));
                if let (Some(res), Some(d)) = (res_m, marker_d) {
                    if ui.small_button("Gate around TDR marker").on_hover_text("±3 resolutions around the marker").clicked() {
                        self.gate_start_m = d - 3.0 * res;
                        self.gate_stop_m = d + 3.0 * res;
                    }
                }
                if let Some((_, _, edge_hz)) = &self.gated_cache {
                    ui.small(format!("band edges: outer {} on each side unreliable (gate blur ≈ 1/width)", format_hz(*edge_hz as u64)));
                }
                if let Some(e) = &self.gate_error {
                    ui.colored_label(GATE_COLOR, format!("gate: {e}"));
                }
            }

            ui.add_space(12.0);
            ui.heading("Export");
            if ui.add_enabled(self.shown.is_some(), egui::Button::new("Save Touchstone (.s2p)")).clicked() {
                self.save_s2p();
            }

            ui.add_space(12.0);
            if let Some(cfg) = &self.last_cfg {
                ui.small(format!(
                    "last sweep: {} → {} in {} points, {} ms, sweep #{}",
                    format_hz(cfg.start_hz),
                    format_hz(cfg.stop_hz()),
                    cfg.points,
                    self.raw.as_ref().map(|r| r.elapsed.as_millis()).unwrap_or(0),
                    self.sweep_count
                ));
            }
        });
    }

    fn calibration_panel(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        match &self.cal.set {
            Some(set) => {
                ui.label(format!(
                    "{} pts, {} → {}",
                    set.freqs_hz.len(),
                    format_hz(set.freqs_hz[0]),
                    format_hz(*set.freqs_hz.last().unwrap())
                ));
                let have: Vec<_> = Standard::ALL.iter().chain(Standard::LINE.iter()).filter(|s| set.has(**s)).map(|s| s.name()).collect();
                ui.label(format!("measured: {}", if have.is_empty() { "—".to_string() } else { have.join(", ") }));
                ui.label(format!(
                    "kit: {} (open C0 {:.1} fF, delay {:.2} ps)",
                    set.kit.name,
                    set.kit.open_c0() * 1e15,
                    set.kit.open.offset_delay_s * 1e12
                ));
                let status = match set.solve() {
                    Ok(c) => format!("ready for S11{}", if c.has_through() { " + S21" } else { "" }),
                    Err(_) => "incomplete (need open, short, load)".to_string(),
                };
                ui.label(status);
                if let Some(f) = &self.cal.file {
                    ui.small(f.display().to_string());
                }
            }
            None => {
                ui.label("no calibration — measure standards on the current sweep grid");
            }
        }
        if let Some(m) = &self.cal.message {
            ui.small(RichText::new(m).color(Color32::from_rgb(200, 200, 120)));
        }
        if let Some(w) = &self.cal.warning {
            ui.small(RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(255, 150, 80)));
        }
        if self.cal.apply && self.cal.solved.is_some() {
            if let Some(w) = &self.cal.grid_warning {
                ui.small(RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(255, 150, 80)));
            }
        }

        self.standard_buttons(ui, &Standard::ALL, connected);
        ui.horizontal(|ui| {
            let changed = ui.checkbox(&mut self.cal.apply, "apply").changed();
            if changed {
                if let Some(raw) = self.raw.clone() {
                    let corrected = self.corrected(&raw);
                    self.set_shown(corrected);
                }
            }
            if ui.button("Load…").clicked() {
                self.load_cal();
            }
            if ui.add_enabled(self.cal.set.is_some(), egui::Button::new("Save…")).clicked() {
                self.save_cal();
            }
            if ui.add_enabled(self.cal.set.is_some(), egui::Button::new("Clear")).clicked() {
                self.cal = CalState {
                    apply: self.cal.apply,
                    fit_f_min: self.cal.fit_f_min.clone(),
                    fit_as_delay: self.cal.fit_as_delay,
                    ..Default::default()
                };
                if let Some(raw) = self.raw.clone() {
                    self.set_shown(raw);
                }
            }
        });
    }

    fn standard_buttons(&mut self, ui: &mut egui::Ui, standards: &[Standard], connected: bool) {
        ui.horizontal_wrapped(|ui| {
            for &std in standards {
                let pending = self.cal.pending == Some(std);
                let label = if pending { format!("{}…", std.name()) } else { std.name().to_string() };
                if ui.add_enabled(connected && self.cal.pending.is_none(), egui::Button::new(label)).clicked() {
                    if let Some(cfg) = self.current_config() {
                        self.cal.pending = Some(std);
                        self.send(Cmd::Sweep { cfg, purpose: Purpose::Standard(std), continuous: false });
                    }
                }
            }
        });
    }

    /// Kit fit from line-open / line-short measurements (see docs/03-calibratie.md §8).
    fn kit_fit_panel(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        let have_lines = self.cal.set.as_ref().map_or(false, |s| s.has(Standard::LineOpen) && s.has(Standard::LineShort));
        let solvable = self.cal.set.as_ref().map_or(false, |s| s.solve().is_ok());
        ui.small("With open, short and load done: put a cable (≥ 50 cm) on port 1 and measure it with an open and a short at the far end. The ripple difference gives the open/short asymmetry of the kit.");
        self.standard_buttons(ui, &Standard::LINE, connected);
        ui.horizontal(|ui| {
            ui.label("from");
            ui.add(egui::TextEdit::singleline(&mut self.cal.fit_f_min).desired_width(60.0));
            ui.label("as");
            egui::ComboBox::from_id_salt("fit_model")
                .selected_text(if self.cal.fit_as_delay { "delay" } else { "capacitance" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.cal.fit_as_delay, false, "capacitance");
                    ui.selectable_value(&mut self.cal.fit_as_delay, true, "delay");
                });
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(have_lines && solvable, egui::Button::new("Fit")).clicked() {
                self.run_kit_fit();
            }
            if ui.add_enabled(self.cal.fit.is_some(), egui::Button::new("Apply to kit")).clicked() {
                self.apply_kit_fit();
            }
            let not_ideal = self.cal.set.as_ref().map_or(false, |s| s.kit != CalKit::ideal());
            if ui.add_enabled(not_ideal, egui::Button::new("Reset kit")).clicked() {
                if let Some(set) = &mut self.cal.set {
                    set.kit = CalKit::ideal();
                }
                self.cal.fit = None;
                self.cal.solved = None;
                self.cal.message = Some("kit reset to ideal".into());
                if let Some(raw) = self.raw.clone() {
                    let corrected = self.corrected(&raw);
                    self.set_shown(corrected);
                }
            }
        });
        if let Some((fit, model)) = self.cal.fit {
            let kit = &self.cal.set.as_ref().unwrap().kit;
            let line = match model {
                OpenModel::Capacitance => format!(
                    "ΔC {:+.1} fF → open C0 {:.1} fF",
                    fit.delta_c_farad * 1e15,
                    (kit.open_c0() + fit.delta_c_farad) * 1e15
                ),
                OpenModel::Delay => format!(
                    "Δτ {:+.2} ps → open delay {:.2} ps",
                    fit.delta_delay_s * 1e12,
                    (kit.open.offset_delay_s + fit.delta_delay_s) * 1e12
                ),
            };
            ui.label(line);
            ui.small(format!(
                "{} points, ripple RMS {:.4} → {:.4} (short assumed as in the kit)",
                fit.points_used, fit.ripple_rms_before, fit.ripple_rms_after
            ));
        }
    }

    fn run_kit_fit(&mut self) {
        let f_min = match parse_hz(&self.cal.fit_f_min) {
            Ok(f) => f,
            Err(e) => {
                self.cal.message = Some(format!("fit: {e}"));
                return;
            }
        };
        let model = self.cal.fit_model();
        let Some(set) = &self.cal.set else { return };
        match set.fit_open(f_min, model) {
            Ok(fit) => {
                self.cal.fit = Some((fit, model));
                self.cal.message = None;
            }
            Err(e) => {
                self.cal.fit = None;
                self.cal.message = Some(format!("fit: {e}"));
            }
        }
    }

    fn apply_kit_fit(&mut self) {
        let Some((fit, model)) = self.cal.fit.take() else { return };
        let Some(set) = &mut self.cal.set else { return };
        set.apply_open_fit(&fit, model);
        self.cal.solved = None;
        self.cal.message = Some(format!("kit updated: open C0 {:.1} fF, delay {:.2} ps — save to keep it", set.kit.open_c0() * 1e15, set.kit.open.offset_delay_s * 1e12));
        if let Some(raw) = self.raw.clone() {
            let corrected = self.corrected(&raw);
            self.set_shown(corrected);
        }
    }

    fn load_cal(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("calibration", &["json"]).pick_file() else { return };
        self.load_cal_from(&path);
    }

    fn load_cal_from(&mut self, path: &Path) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| CalibrationSet::from_json(&t).map_err(|e| e.to_string())) {
            Ok(set) => {
                self.cal.set = Some(set);
                self.cal.file = Some(path.to_path_buf());
                self.cal.solved = None;
                self.cal.fit = None;
                self.cal.warning = None;
                self.cal.message = Some("loaded".into());
                if let Some(raw) = self.raw.clone() {
                    let corrected = self.corrected(&raw);
                    self.set_shown(corrected);
                }
            }
            Err(e) => self.error = Some(format!("loading calibration: {e}")),
        }
    }

    fn save_cal(&mut self) {
        let Some(set) = &self.cal.set else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("calibration", &["json"]).set_file_name("litevna.cal.json").save_file() else { return };
        match set.to_json().map_err(|e| e.to_string()).and_then(|j| std::fs::write(&path, j).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.cal.file = Some(path);
                self.cal.message = Some("saved".into());
            }
            Err(e) => self.error = Some(format!("saving calibration: {e}")),
        }
    }

    fn save_s2p(&mut self) {
        let Some(sweep) = self.freq_sweep() else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("Touchstone", &["s2p"]).set_file_name("sweep.s2p").save_file() else { return };
        let gated = if self.gate_on && self.gated_cache.is_some() {
            format!(", S11 time-gated {:.2}..{:.2} m {}", self.gate_start_m, self.gate_stop_m, if self.gate_notch { "notch" } else { "band pass" })
        } else {
            String::new()
        };
        let comment = format!(
            "vna-r, {} points, {}{}",
            sweep.freqs_hz.len(),
            if self.cal.apply && self.cal.solved.is_some() { "host calibrated" } else { "uncalibrated raw" },
            gated
        );
        match std::fs::File::create(&path).and_then(|f| write_s2p(std::io::BufWriter::new(f), &sweep, Some(&comment))) {
            Ok(()) => self.cal.message = Some(format!("wrote {}", path.display())),
            Err(e) => self.error = Some(format!("saving s2p: {e}")),
        }
    }

    // ----- plots -----------------------------------------------------------

    fn central(&mut self, ui: &mut egui::Ui) {
        if self.view == View::Tdr {
            self.central_tdr(ui);
            return;
        }
        self.marker_readout(ui);
        let avail = ui.available_size();
        let h = ((avail.y - 10.0) / 2.0).max(150.0);
        let Some(sweep) = self.freq_sweep() else {
            ui.centered_and_justified(|ui| ui.label("Connect and press Run"));
            return;
        };
        // Grey lines where a gated trace stops being trustworthy.
        let edge_lines: Vec<f64> = match self.gated_cache.as_ref().filter(|_| self.gate_on) {
            Some((_, _, edge_hz)) => {
                let (f0, f1) = (sweep.freqs_hz[0] as f64, *sweep.freqs_hz.last().unwrap() as f64);
                vec![(f0 + edge_hz) / 1e6, (f1 - edge_hz) / 1e6]
            }
            None => Vec::new(),
        };
        let mhz: Vec<f64> = sweep.freqs_hz.iter().map(|f| *f as f64 / 1e6).collect();
        // Arrow keys nudge the active marker (shift: 10 points).
        let n_pts = sweep.freqs_hz.len();
        let (left, right, shift) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.modifiers.shift));
        if left || right {
            if let Some(i) = self.markers[self.active_marker] {
                let step = if shift { 10 } else { 1 };
                let j = if left { i.saturating_sub(step) } else { (i + step).min(n_pts - 1) };
                self.markers[self.active_marker] = Some(j);
            }
        }
        let marker_lines: Vec<(f64, Color32)> = self
            .markers
            .iter()
            .enumerate()
            .filter_map(|(k, m)| m.and_then(|i| mhz.get(i).map(|x| (*x, MARKER_COLORS[k]))))
            .collect();
        let marker_points: Vec<(Complex64, Color32)> = self
            .markers
            .iter()
            .enumerate()
            .filter_map(|(k, m)| m.and_then(|i| sweep.s11.get(i).map(|g| (*g, MARKER_COLORS[k]))))
            .collect();
        let mut clicked_x: Option<f64> = None;

        ui.columns(2, |cols| {
            // Smith chart
            let w = cols[0].available_width();
            let resp = SmithChart { gamma: &sweep.s11, markers: &marker_points, trace_color: S11_COLOR }.show(&mut cols[0], Vec2::new(w, h));
            if resp.response.clicked() {
                if let Some(p) = resp.response.interact_pointer_pos() {
                    // nearest trace point on screen
                    let best = sweep
                        .s11
                        .iter()
                        .enumerate()
                        .map(|(i, g)| (i, resp.pos_of(*g).distance(p)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((i, _)) = best {
                        self.markers[self.active_marker] = Some(i);
                    }
                }
            }
            if let Some(x) = self.kind_plot(&mut cols[1], 0, h, &mhz, &sweep, &marker_lines, &edge_lines) {
                clicked_x = Some(x);
            }
        });
        ui.columns(2, |cols| {
            for (col, k) in [(0usize, 1usize), (1, 2)] {
                if let Some(x) = self.kind_plot(&mut cols[col], k, h, &mhz, &sweep, &marker_lines, &edge_lines) {
                    clicked_x = Some(x);
                }
            }
        });

        if let Some(x) = clicked_x {
            // nearest frequency index
            let best = mhz.iter().enumerate().map(|(i, f)| (i, (f - x).abs())).min_by(|a, b| a.1.total_cmp(&b.1));
            self.markers[self.active_marker] = best.map(|(i, _)| i);
        }
    }

    /// One selectable plot: a header with the trace-kind combo box, then the
    /// plot itself. Returns the clicked x (MHz) if the user clicked in it.
    #[allow(clippy::too_many_arguments)]
    fn kind_plot(&mut self, ui: &mut egui::Ui, slot: usize, h: f32, mhz: &[f64], sweep: &SweepResult, marker_lines: &[(f64, Color32)], edge_lines: &[f64]) -> Option<f64> {
        let header_h = 22.0;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("plot_kind", slot))
                .selected_text(self.plot_kinds[slot].name())
                .show_ui(ui, |ui| {
                    for k in TraceKind::ALL {
                        ui.selectable_value(&mut self.plot_kinds[slot], k, k.name());
                    }
                });
            if self.plot_kinds[slot] == TraceKind::GroupDelay {
                ui.small(format!("aperture ±{} pts", self.gd_aperture));
            }
        });
        let kind = self.plot_kinds[slot];
        let (lo, hi) = kind.default_range();
        let mut lines: Vec<(String, Vec<f64>, Color32, bool)> = Vec::new();
        for (param, s, color, show) in [(Param::S11, &sweep.s11, S11_COLOR, self.show_s11), (Param::S21, &sweep.s21, S21_COLOR, self.show_s21)] {
            if !show {
                continue;
            }
            for (i, series) in traces::compute(kind, param, &sweep.freqs_hz, s, self.gd_aperture).into_iter().enumerate() {
                // second series of a pair (X of R+jX) dashed
                lines.push((series.label, series.values, color, i == 1));
            }
        }
        let r = Plot::new(("plot", slot, kind.id()))
            .height((h - header_h).max(100.0))
            .legend(Legend::default())
            .x_axis_label("MHz")
            .y_axis_label(kind.unit())
            .include_y(lo)
            .include_y(hi)
            .show(ui, |pui| {
                for (label, values, color, dashed) in &lines {
                    let mut line = Line::new(label.clone(), series(mhz, values.iter().copied())).color(*color);
                    if *dashed {
                        line = line.style(egui_plot::LineStyle::dashed_loose());
                    }
                    pui.line(line);
                }
                for (k, (x, c)) in marker_lines.iter().enumerate() {
                    pui.vline(VLine::new(format!("M{}", k + 1), *x).color(*c));
                }
                for x in edge_lines {
                    pui.vline(VLine::new("gate edge", *x).color(Color32::from_gray(110)).style(egui_plot::LineStyle::dashed_dense()));
                }
                (pui.response().clicked(), pui.pointer_coordinate())
            });
        match r.inner {
            (true, Some(p)) => Some(p.x),
            _ => None,
        }
    }

    fn tdr_result(&mut self) -> Option<&TdrResult> {
        let sweep = self.shown.as_ref()?;
        let fresh = matches!(&self.tdr_cache, Some((n, vf, w, _)) if *n == self.shown_rev && *vf == self.tdr_vf && *w == self.tdr_window);
        if !fresh {
            let cfg = TdrConfig { window: self.tdr_window, velocity_factor: self.tdr_vf, fft_size: 0 };
            match tdr(&sweep.freqs_hz, &sweep.s11, &cfg) {
                Ok(t) => self.tdr_cache = Some((self.shown_rev, self.tdr_vf, self.tdr_window, t)),
                Err(e) => {
                    self.error = Some(format!("TDR: {e}"));
                    self.tdr_cache = None;
                }
            }
        }
        self.tdr_cache.as_ref().map(|(_, _, _, t)| t)
    }

    fn central_tdr(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.tdr_result().cloned() else {
            ui.centered_and_justified(|ui| ui.label("Connect and press Run; TDR is computed from S11"));
            return;
        };
        // readout
        ui.horizontal(|ui| {
            match self.tdr_marker {
                Some(i) if i < t.distance_m.len() => {
                    ui.label(RichText::new(format!("M  {:.3} m  {:.2} ns", t.distance_m[i], t.time_s[i] * 1e9)).strong().color(MARKER_COLOR));
                    ui.separator();
                    ui.label(RichText::new(format!("|Γ| {:.1} dB", db(t.impulse_mag[i]))).color(S11_COLOR));
                    ui.label(format!("Z {:.1} Ω", t.impedance_ohm[i]));
                    if ui.small_button("clear").clicked() {
                        self.tdr_marker = None;
                    }
                }
                _ => {
                    ui.small("Click in a plot to place a marker. Distances are one-way, using the velocity factor.");
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if self.cal.apply && self.cal.solved.is_some() {
                    RichText::new("CAL").color(Color32::from_rgb(120, 200, 120))
                } else {
                    RichText::new("RAW").color(Color32::from_rgb(230, 150, 80))
                };
                ui.label(tag.strong());
                self.gate_tag(ui);
                if self.gate_on {
                    ui.small("drag the orange lines to move the gate");
                }
            });
        });

        let avail = ui.available_size();
        let h = ((avail.y - 10.0) / 2.0).max(150.0);
        let limit = self.tdr_max_m.min(t.max_distance_m());
        let gate_x = if self.gate_on { Some((self.gate_start_m, self.gate_stop_m)) } else { None };
        let plot_drag_ok = self.gate_hover.is_none() && self.gate_drag.is_none();
        self.gate_hover = None;
        let mut gate_events: Vec<GateEvent> = Vec::new();
        let n = t.distance_m.iter().position(|d| *d > limit).unwrap_or(t.distance_m.len());
        let dist = &t.distance_m[..n];
        let marker_x = self.tdr_marker.and_then(|i| dist.get(i).copied());
        let mut clicked_x: Option<f64> = None;

        let r = Plot::new("tdr-impulse")
            .height(h)
            .x_axis_label("m")
            .y_axis_label("|Γ| dB")
            .include_y(0.0)
            .include_y(-60.0)
            .include_x(0.0)
            .include_x(limit)
            .allow_drag(plot_drag_ok)
            .show(ui, |pui| {
                pui.line(Line::new("impulse", series(dist, t.impulse_mag[..n].iter().map(|m| db(*m).max(-100.0)))).color(S11_COLOR));
                if let Some(x) = marker_x {
                    pui.vline(VLine::new("marker", x).color(MARKER_COLOR));
                }
                GateEvent::read(pui, gate_x)
            });
        draw_gate(ui, &r.transform, r.response.rect, gate_x);
        gate_events.push(r.inner);
        let r = Plot::new("tdr-z")
            .height(h)
            .x_axis_label("m")
            .y_axis_label("Ω")
            .include_y(0.0)
            .include_y(100.0)
            .include_x(0.0)
            .include_x(limit)
            .allow_drag(plot_drag_ok)
            .show(ui, |pui| {
                pui.line(Line::new("Z", series(dist, t.impedance_ohm[..n].iter().map(|z| z.min(500.0)))).color(S21_COLOR));
                if let Some(x) = marker_x {
                    pui.vline(VLine::new("marker", x).color(MARKER_COLOR));
                }
                GateEvent::read(pui, gate_x)
            });
        draw_gate(ui, &r.transform, r.response.rect, gate_x);
        gate_events.push(r.inner);

        for ev in gate_events {
            if ev.hover.is_some() {
                self.gate_hover = ev.hover;
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if ev.drag_started && ev.hover.is_some() {
                self.gate_drag = ev.hover;
            }
            if ev.dragged {
                if let (Some(edge), Some(x)) = (self.gate_drag, ev.pointer.map(|p| p.x)) {
                    let min_gap = t.resolution_m() * 0.5;
                    match edge {
                        0 => self.gate_start_m = x.min(self.gate_stop_m - min_gap),
                        _ => self.gate_stop_m = x.max(self.gate_start_m + min_gap),
                    }
                }
            }
            if ev.drag_stopped {
                self.gate_drag = None;
            }
            if ev.clicked && ev.hover.is_none() && self.gate_drag.is_none() {
                clicked_x = ev.pointer.map(|p| p.x);
            }
        }
        if let Some(x) = clicked_x {
            let best = dist.iter().enumerate().map(|(i, d)| (i, (d - x).abs())).min_by(|a, b| a.1.total_cmp(&b.1));
            self.tdr_marker = best.map(|(i, _)| i);
        }
    }

    fn marker_readout(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Markers");
            for k in 0..MARKERS {
                let placed = self.markers[k].is_some();
                let text = RichText::new(format!("M{}", k + 1)).color(MARKER_COLORS[k]);
                let text = if placed { text.strong() } else { text.weak() };
                if ui.selectable_label(self.active_marker == k, text).clicked() {
                    self.active_marker = k;
                }
            }
            ui.small("click in a plot places the active one; ← → nudge, shift for ×10");
            if ui.small_button("clear").clicked() {
                self.markers[self.active_marker] = None;
            }
            if ui.small_button("clear all").clicked() {
                self.markers = [None; MARKERS];
            }
            ui.separator();
            if let Some(sweep) = self.freq_sweep() {
                let n = sweep.freqs_hz.len();
                let argmin = |f: &dyn Fn(usize) -> f64| (0..n).min_by(|a, b| f(*a).total_cmp(&f(*b)));
                if ui.small_button("min SWR").on_hover_text("move the active marker to the lowest |S11|").clicked() {
                    self.markers[self.active_marker] = argmin(&|i| sweep.s11[i].norm());
                }
                if ui.small_button("max S21").clicked() {
                    self.markers[self.active_marker] = argmin(&|i| -sweep.s21[i].norm());
                }
                if ui.small_button("min S21").clicked() {
                    self.markers[self.active_marker] = argmin(&|i| sweep.s21[i].norm());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if self.cal.apply && self.cal.solved.is_some() {
                    RichText::new("CAL").color(Color32::from_rgb(120, 200, 120))
                } else {
                    RichText::new("RAW").color(Color32::from_rgb(230, 150, 80))
                };
                ui.label(tag.strong());
                self.gate_tag(ui);
            });
        });

        let Some(sweep_owned) = self.freq_sweep() else { return };
        let sweep = &sweep_owned;
        let placed: Vec<(usize, usize)> = self.markers.iter().enumerate().filter_map(|(k, m)| m.map(|i| (k, i))).filter(|(_, i)| *i < sweep.freqs_hz.len()).collect();
        if placed.is_empty() {
            return;
        }
        egui::Grid::new("marker-table").striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
            for h in ["", "frequency", "S11 dB", "S11 °", "Z", "SWR", "S21 dB", "S21 °"] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for (k, i) in &placed {
                let s11 = sweep.s11[*i];
                let s21 = sweep.s21[*i];
                let z = impedance(s11);
                ui.label(RichText::new(format!("M{}", k + 1)).color(MARKER_COLORS[*k]).strong());
                ui.label(format_hz(sweep.freqs_hz[*i]));
                ui.label(RichText::new(format!("{:.2}", db(s11.norm()))).color(S11_COLOR));
                ui.label(format!("{:.1}", s11.arg().to_degrees()));
                ui.label(format!("{:.1} {} j{:.1} Ω", z.re, if z.im < 0.0 { "−" } else { "+" }, z.im.abs()));
                ui.label(format!("{:.2}", swr(s11.norm())));
                ui.label(RichText::new(format!("{:.2}", db(s21.norm()))).color(S21_COLOR));
                ui.label(format!("{:.1}", s21.arg().to_degrees()));
                ui.end_row();
            }
            // Delta between the first two placed markers.
            if placed.len() >= 2 {
                let (ka, a) = placed[0];
                let (kb, b) = placed[1];
                let df = sweep.freqs_hz[b] as i64 - sweep.freqs_hz[a] as i64;
                ui.label(RichText::new(format!("M{}−M{}", kb + 1, ka + 1)).weak());
                ui.label(format!("{}{}", if df < 0 { "−" } else { "" }, format_hz(df.unsigned_abs())));
                ui.label(format!("{:+.2}", db(sweep.s11[b].norm()) - db(sweep.s11[a].norm())));
                ui.label(format!("{:+.1}", sweep.s11[b].arg().to_degrees() - sweep.s11[a].arg().to_degrees()));
                ui.label("");
                ui.label("");
                ui.label(format!("{:+.2}", db(sweep.s21[b].norm()) - db(sweep.s21[a].norm())));
                ui.label(format!("{:+.1}", sweep.s21[b].arg().to_degrees() - sweep.s21[a].arg().to_degrees()));
                ui.end_row();
            }
        });
    }

    fn screenshot_window(&mut self, ctx: &egui::Context) {
        if !self.show_screenshot {
            return;
        }
        let mut open = self.show_screenshot;
        let mut save = false;
        egui::Window::new("Device screenshot").open(&mut open).resizable(true).show(ctx, |ui| {
            if let Some((tex, shot)) = &self.screenshot {
                ui.horizontal(|ui| {
                    ui.label(format!("{}×{}", shot.width, shot.height));
                    if ui.button("Save PNG…").clicked() {
                        save = true;
                    }
                });
                ui.add(egui::Image::new(tex).max_width(ui.available_width().min(1024.0)));
            }
        });
        self.show_screenshot = open;
        if save {
            self.save_screenshot();
        }
    }

    fn save_screenshot(&mut self) {
        let Some((_, shot)) = &self.screenshot else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("litevna.png").save_file() else { return };
        let rgb = shot.to_rgb888(true);
        let result = (|| -> anyhow::Result<()> {
            let file = std::io::BufWriter::new(std::fs::File::create(&path)?);
            let mut enc = png::Encoder::new(file, shot.width as u32, shot.height as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header()?;
            w.write_image_data(&rgb)?;
            w.finish()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.error = Some(format!("saving screenshot: {e}"));
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        if self.running {
            // keep the UI alive while the worker streams sweeps
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::left("side").resizable(true).default_size(300.0).show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));
        self.screenshot_window(&ctx);
    }
}

/// Pointer interaction with the gate lines in one TDR plot.
struct GateEvent {
    clicked: bool,
    pointer: Option<egui_plot::PlotPoint>,
    /// Gate edge under the pointer (0 = start, 1 = stop).
    hover: Option<usize>,
    drag_started: bool,
    dragged: bool,
    drag_stopped: bool,
}

impl GateEvent {
    fn read(pui: &egui_plot::PlotUi<'_>, gate_x: Option<(f64, f64)>) -> Self {
        let resp = pui.response();
        let pointer = pui.pointer_coordinate();
        let near = pui.plot_bounds().width() * 0.015;
        let hover = match (pointer, gate_x) {
            (Some(p), Some((a, b))) if resp.hovered() || resp.dragged() => {
                if (p.x - a).abs() < near {
                    Some(0)
                } else if (p.x - b).abs() < near {
                    Some(1)
                } else {
                    None
                }
            }
            _ => None,
        };
        GateEvent { clicked: resp.clicked(), pointer, hover, drag_started: resp.drag_started(), dragged: resp.dragged(), drag_stopped: resp.drag_stopped() }
    }
}

/// Paint the gate on top of a plot in screen space, so it never feeds back
/// into the plot's auto-bounds (a data-space polygon sized to the current
/// bounds would grow the bounds every frame).
fn draw_gate(ui: &egui::Ui, transform: &egui_plot::PlotTransform, rect: egui::Rect, gate_x: Option<(f64, f64)>) {
    let Some((a, b)) = gate_x else { return };
    let x_of = |x: f64| transform.position_from_point(&egui_plot::PlotPoint::new(x, 0.0)).x;
    let (xa, xb) = (x_of(a), x_of(b));
    let painter = ui.painter().with_clip_rect(rect);
    let band = egui::Rect::from_min_max(egui::pos2(xa.min(xb), rect.top()), egui::pos2(xa.max(xb), rect.bottom()));
    painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(255, 150, 80, 22));
    let stroke = egui::Stroke::new(1.5, GATE_COLOR);
    painter.vline(xa, rect.y_range(), stroke);
    painter.vline(xb, rect.y_range(), stroke);
}

fn nearest_index(freqs: &[u64], f: u64) -> usize {
    freqs.iter().enumerate().min_by_key(|(_, x)| x.abs_diff(f)).map(|(i, _)| i).unwrap_or(0)
}

fn series(x: &[f64], y: impl Iterator<Item = f64>) -> PlotPoints<'static> {
    x.iter().zip(y).map(|(x, y)| [*x, y]).collect()
}

