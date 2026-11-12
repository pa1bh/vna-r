//! Application state and the frame: top bar, menu bar, side panel and the events
//! from the device thread. The panels live in the submodules.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Vec2};
use egui_plot::{Legend, Line, Plot, PlotPoints, VLine};
use litevna::calibration::{interpolate_complex, plausibility_warning, CalKit, Calibration, CalibrationSet, OpenFit, OpenModel, Standard};
use litevna::device::plan_segments;
use litevna::discovery::{self, PortCandidate};
use litevna::freq::{format_hz, nearest_index, parse_hz};
use litevna::timedomain::{gate, tdr, Gate, GateKind, TdrConfig, TdrResult, Window, C0};
use litevna::touchstone::{read_touchstone_file_full, write_s1p, write_s2p};
use litevna::traces::{self, db, impedance, swr, Param, TraceKind};
use litevna::{Limits, Screenshot, SweepConfig, SweepResult, VnaInfo};
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::smith::SmithChart;
use crate::worker::{self, Cmd, Event, Purpose, WorkerHandle};

mod calibration;
mod diagnostics;
mod files;
mod plots;
mod settings;
mod sweep;
mod tdr;

// Types and helpers the submodules define; each submodule sees this module through `use super::*`.
use self::calibration::*;
use self::diagnostics::*;
use self::files::*;
use self::plots::*;
use self::settings::*;
use self::tdr::*;

/// Version of the workspace (one number for library, CLI and GUI).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git revision recorded by build.rs ("unknown" in a build without git).
pub const GIT_REV: &str = env!("VNA_R_GIT_REV");

const REPO_URL: &str = "https://github.com/pa1bh/vna-r";

/// The manual as published for this version: `docs/manual` (English) or
/// `docs/handleiding` (Dutch) at the release tag, so the text matches the
/// buttons of the program that is running.
fn manual_url(dir: &str) -> String {
    format!("{REPO_URL}/blob/v{VERSION}/docs/{dir}/README.md")
}

const ISSUES_URL: &str = "https://github.com/pa1bh/vna-r/issues";

/// The sizes offered under View ▸ Zoom, in percent.
const ZOOM_STEPS: [u32; 7] = [75, 100, 125, 150, 175, 200, 250];

/// View ▸ Zoom: the size of everything in the window, text included. egui
/// already zooms on Ctrl/Cmd with plus, minus and zero and eframe keeps the
/// factor with its own state; this entry is there so that it can be found.
fn zoom_menu(ui: &mut egui::Ui) {
    use egui::gui_zoom::kb_shortcuts::{ZOOM_IN, ZOOM_OUT, ZOOM_RESET};
    let ctx = ui.ctx().clone();
    let current = (ctx.zoom_factor() * 100.0).round() as u32;
    ui.menu_button(format!("Zoom ({current}%)"), |ui| {
        for percent in ZOOM_STEPS {
            if ui.radio(current == percent, format!("{percent}%")).clicked() {
                ui.close();
                ctx.set_zoom_factor(percent as f32 / 100.0);
            }
        }
        ui.separator();
        ui.weak(format!(
            "{} and {} step by 10%, {} goes back to 100%",
            ctx.format_shortcut(&ZOOM_IN),
            ctx.format_shortcut(&ZOOM_OUT),
            ctx.format_shortcut(&ZOOM_RESET)
        ));
    })
    .response
    .on_hover_text("the size of everything in the window, text included");
}

/// Lines kept in the developer log.
const LOG_LINES: usize = 400;

const S11_COLOR: Color32 = Color32::from_rgb(255, 204, 0);

const S21_COLOR: Color32 = Color32::from_rgb(80, 220, 120);

const GATE_COLOR: Color32 = Color32::from_rgb(255, 150, 80);

/// Upper bound for the software sweep average (ring buffer of raw sweeps).
const MAX_SWEEP_AVG: usize = 64;

/// Colours for held reference traces.
const REF_COLORS: [Color32; 4] = [
    Color32::from_rgb(150, 150, 150),
    Color32::from_rgb(120, 170, 220),
    Color32::from_rgb(210, 150, 210),
    Color32::from_rgb(160, 210, 150),
];

const MAX_REFS: usize = 4;
/// Largest port extension the field accepts, ps (100 ns, about 20 m of cable).
const PORT_EXT_MAX_PS: f64 = 100_000.0;

const POINT_CHOICES: [u16; 8] = [51, 101, 201, 401, 801, 1001, 1601, 3201];

const MARKERS: usize = 4;

const MARKER_COLORS: [Color32; MARKERS] = [
    Color32::from_rgb(255, 90, 90),
    Color32::from_rgb(90, 200, 255),
    Color32::from_rgb(255, 120, 255),
    Color32::from_rgb(255, 170, 60),
];

// One value for the whole program, replaced on connect and disconnect only.
#[allow(clippy::large_enum_variant)]
enum Connection {
    Disconnected,
    Connecting(String),
    Connected {
        port: String,
        info: VnaInfo,
        battery_mv: Option<u16>,
    },
}

pub struct App {
    worker: WorkerHandle,
    ports: Vec<PortCandidate>,
    /// `ui.input().time` of the last port scan; while disconnected the list is
    /// rescanned every few seconds so a device plugged in later shows up.
    last_port_scan: f64,
    selected_port: Option<String>,
    connection: Connection,

    start: String,
    stop: String,
    points: u16,
    sweep_error: Option<String>,
    average: u8,
    running: bool,
    /// Points done and total of the sweep under way, and when it began; shown
    /// as a bar once a sweep takes long enough to wonder about.
    progress: Option<(usize, usize, Instant)>,

    raw: Option<SweepResult>,
    shown: Option<SweepResult>,
    /// Segment configs of the last display sweep, in sweep order, and the
    /// calibration file each used (None = the calibration panel).
    last_plan: Vec<SweepConfig>,
    last_plan_cals: Vec<Option<PathBuf>>,
    sent_plan_cals: Vec<Option<PathBuf>>,
    /// Primary segment of the last applied sweep: the grid the display runs
    /// on and the grid calibration standards are measured on.
    applied_primary: Option<SweepConfig>,
    extra_segments: Vec<SegmentSettings>,
    /// Per-segment calibration files, loaded on first use.
    seg_cals: HashMap<PathBuf, SegCal>,
    /// Which segments of the last correction actually got a calibration.
    seg_calibrated: Vec<bool>,
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
    /// Port extension: one-way delay (ps) the reference plane is moved outward by.
    port_ext_ps: f64,
    /// Which gate edge the pointer hovers / drags in the TDR plots (0 = start, 1 = stop).
    gate_hover: Option<usize>,
    gate_drag: Option<usize>,
    gate_error: Option<String>,
    gated_cache: Option<(GateKey, Vec<Complex64>, f64)>,
    sweep_avg: usize,
    /// Last raw sweeps on the current grid, newest at the back.
    avg_buf: VecDeque<SweepResult>,
    references: Vec<Reference>,
    /// Reference the difference trace is taken against.
    diff_ref: Option<usize>,
    /// The references differ from what is on disk (see `store_references`).
    references_changed: bool,
    /// Name of the Touchstone file currently shown as the trace (no device data).
    file_trace: Option<String>,
    /// Last file operation (saved/opened), shown under **Files**.
    file_message: Option<String>,
    /// TDR of the shown sweep, or why it cannot be computed; see [`TdrCache`].
    tdr_cache: Option<TdrCache>,
    /// A short confirmation in the top bar ("diagnostics copied"), and when it appeared.
    notice: Option<(String, Instant)>,

    /// Graphics backend and adapter wgpu picked (Vulkan, Metal, GL + driver), for About and bug reports.
    renderer: String,
    show_about: bool,
    /// Developer menu visible (View ▸ Developer tools).
    dev_tools: bool,
    /// Connect at start when a VNA is plugged in (View ▸ Connect at start).
    auto_connect: bool,
    /// The connect-at-start attempt is made once, in the first frame.
    auto_connect_tried: bool,
    /// `ui.input().time` of the last battery reading asked for.
    last_battery_poll: f64,
    show_log: bool,
    /// Developer log: connection events, errors, file operations, sweeps while the window is open.
    log: VecDeque<String>,
    started: Instant,
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
            last_port_scan: f64::NEG_INFINITY,
            selected_port: None,
            connection: Connection::Disconnected,
            start: "1M".into(),
            stop: "30M".into(),
            points: 201,
            sweep_error: None,
            average: 1,
            running: false,
            progress: None,
            raw: None,
            shown: None,
            last_plan: Vec::new(),
            last_plan_cals: Vec::new(),
            sent_plan_cals: Vec::new(),
            applied_primary: None,
            extra_segments: Vec::new(),
            seg_cals: HashMap::new(),
            seg_calibrated: Vec::new(),
            sweep_count: 0,
            cal: CalState {
                apply: true,
                ..Default::default()
            },
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
            port_ext_ps: 0.0,
            gate_hover: None,
            gate_drag: None,
            gate_error: None,
            gated_cache: None,
            sweep_avg: 1,
            avg_buf: VecDeque::new(),
            references: Vec::new(),
            diff_ref: None,
            references_changed: false,
            file_trace: None,
            file_message: None,
            tdr_cache: None,
            notice: None,
            renderer: renderer_description(cc),
            show_about: false,
            dev_tools: false,
            auto_connect: false,
            auto_connect_tried: false,
            last_battery_poll: 0.0,
            show_log: false,
            log: VecDeque::new(),
            started: Instant::now(),
        };
        app.log(format!(
            "vna-r {VERSION} ({GIT_REV}) on {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        app.log(format!("renderer: {}{}", app.renderer, display_server()));
        log::info!("renderer: {}{}", app.renderer, display_server());
        app.refresh_ports();
        if let Some(storage) = cc.storage {
            if let Some(settings) = eframe::get_value::<Settings>(storage, eframe::APP_KEY) {
                app.apply_settings(settings);
            }
        }
        app
    }

    /// Connect to the selected port, if there is one and nothing is connected yet.
    fn connect_selected(&mut self) {
        if !matches!(self.connection, Connection::Disconnected) {
            return;
        }
        if let Some(port) = self.selected_port.clone() {
            self.connection = Connection::Connecting(port.clone());
            self.send(Cmd::Connect(port));
        }
    }

    /// What happens without a click: connect at start when asked to, and a
    /// fresh battery reading every minute while connected.
    fn housekeeping(&mut self, ctx: &egui::Context) {
        const BATTERY_INTERVAL: f64 = 60.0;
        if !self.auto_connect_tried {
            self.auto_connect_tried = true;
            let selected_is_vna = self
                .selected_port
                .as_ref()
                .is_some_and(|sel| self.ports.iter().any(|p| &p.path == sel && p.looks_like_vna()));
            if self.auto_connect && selected_is_vna {
                self.connect_selected();
            }
        }
        if self.connected() {
            let now = ctx.input(|i| i.time);
            if now - self.last_battery_poll > BATTERY_INTERVAL {
                self.last_battery_poll = now;
                self.send(Cmd::Battery);
            }
            ctx.request_repaint_after(Duration::from_secs_f64(BATTERY_INTERVAL));
        }
    }

    /// Keys that work when no text field or other widget has the keyboard:
    /// space Run/Stop, S single sweep, 1–4 active marker, H hold, G gate,
    /// T frequency/time domain. (The arrow keys for the markers are with the plots.)
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let pressed = |key: egui::Key| ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none());
        if self.connected() {
            if pressed(egui::Key::Space) {
                if self.sweeping() {
                    self.stop_sweep();
                } else {
                    self.start_sweep(true);
                }
            }
            if pressed(egui::Key::S) && !self.sweeping() {
                self.start_sweep(false);
            }
        }
        for (k, key) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4]
            .into_iter()
            .enumerate()
        {
            if pressed(key) {
                self.active_marker = k;
            }
        }
        if pressed(egui::Key::H) && self.shown.is_some() && self.references.len() < MAX_REFS {
            self.hold_reference();
        }
        if pressed(egui::Key::G) {
            self.gate_on = !self.gate_on;
        }
        if pressed(egui::Key::T) {
            self.view = if self.view == View::Tdr { View::Frequency } else { View::Tdr };
        }
    }

    fn refresh_ports(&mut self) {
        self.ports = discovery::list_ports().unwrap_or_default();
        // Forget a selection whose port is gone, then default to a VNA if there is one.
        if let Some(sel) = &self.selected_port {
            if !self.ports.iter().any(|p| &p.path == sel) {
                self.selected_port = None;
            }
        }
        if self.selected_port.is_none() {
            self.selected_port = self.ports.iter().find(|p| p.looks_like_vna()).map(|p| p.path.clone());
        }
    }

    fn connected(&self) -> bool {
        matches!(self.connection, Connection::Connected { .. })
    }

    /// Top of the connected device's range; the LiteVNA's 6.3 GHz when nothing is connected.
    fn device_max_hz(&self) -> u64 {
        self.device_limits().map_or(6_300_000_000, |l| l.max_hz)
    }

    fn device_limits(&self) -> Option<Limits> {
        match &self.connection {
            Connection::Connected { info, .. } => Some(info.limits),
            _ => None,
        }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.worker.tx.send(cmd);
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(evt) = self.worker.rx.try_recv() {
            match evt {
                Event::Connected { port, info, battery_mv } => {
                    self.log(format!("connected: {} on {port}", info.summary()));
                    self.connection = Connection::Connected { port, info, battery_mv };
                    self.last_battery_poll = ctx.input(|i| i.time);
                    self.error = None;
                    // Never average sweeps of this device with those of the previous one.
                    self.avg_buf.clear();
                }
                Event::Progress { done, total } => {
                    // A lower count than before means a new sweep began.
                    let since = match self.progress {
                        Some((prev, _, since)) if done >= prev => since,
                        _ => Instant::now(),
                    };
                    self.progress = Some((done, total, since));
                }
                Event::Cancelled { purpose } => {
                    self.progress = None;
                    if let Purpose::Standard(std) = purpose {
                        self.cal.pending = None;
                        self.cal.message = Some(format!("measuring {} was stopped", std.name()));
                    }
                }
                Event::Disconnected => {
                    self.log("disconnected");
                    self.connection = Connection::Disconnected;
                    self.running = false;
                    self.progress = None;
                    self.avg_buf.clear();
                }
                Event::SweepDone {
                    cfgs,
                    result,
                    purpose,
                    repeat,
                } => {
                    self.progress = None;
                    match purpose {
                        Purpose::Display => {
                            self.last_plan_cals = if self.sent_plan_cals.len() == cfgs.len() {
                                self.sent_plan_cals.clone()
                            } else {
                                vec![None; cfgs.len()]
                            };
                            self.last_plan = cfgs;
                            self.file_trace = None;
                            self.sweep_count += 1;
                            if let Some(old) = self.raw.as_ref().map(|r| r.freqs_hz.clone()) {
                                self.remap_markers(&old, &result.freqs_hz);
                            }
                            let result = self.push_average(result);
                            let corrected = self.corrected(&result);
                            self.set_shown(corrected);
                            if let Some(freqs) = self.pending_marker_freqs.take() {
                                for (k, f) in freqs.iter().enumerate() {
                                    self.markers[k] = f.map(|f| nearest_index(&result.freqs_hz, f));
                                }
                            }
                            if self.show_log {
                                self.log(format!(
                                    "sweep #{}: {} points in {} ms",
                                    self.sweep_count,
                                    result.freqs_hz.len(),
                                    result.elapsed.as_millis()
                                ));
                            }
                            self.raw = Some(result);
                        }
                        Purpose::Standard(std) => {
                            self.cal.pending = None;
                            self.store_standard(std, &result, repeat);
                        }
                    }
                }
                Event::Battery(mv) => {
                    if let Connection::Connected { battery_mv, .. } = &mut self.connection {
                        *battery_mv = Some(mv);
                    }
                }
                Event::Screenshot(shot) => {
                    let rgb = shot.to_rgb888();
                    let image = egui::ColorImage::from_rgb([shot.width as usize, shot.height as usize], &rgb);
                    let tex = ctx.load_texture("screenshot", image, egui::TextureOptions::LINEAR);
                    self.screenshot = Some((tex, shot));
                    self.show_screenshot = true;
                }
                Event::Error { msg, stops_sweep } => {
                    log::warn!("worker: {msg}");
                    self.log(format!("error: {msg}"));
                    self.error = Some(msg);
                    if stops_sweep {
                        self.running = false;
                        self.cal.pending = None;
                        self.progress = None;
                    }
                    if let Connection::Connecting(_) = self.connection {
                        self.connection = Connection::Disconnected;
                    }
                }
            }
        }
    }

    /// Everything that is shown goes through here: a corrected device sweep,
    /// a raw one, a file. The port extension is applied at this point, so the
    /// frequency view, the TDR, the gate, references and exports all see it,
    /// while `raw` and the calibration standards never do.
    fn set_shown(&mut self, mut sweep: SweepResult) {
        if self.port_ext_ps != 0.0 {
            sweep.extend_port(self.port_ext_ps * 1e-12);
        }
        self.shown = Some(sweep);
        self.shown_rev += 1;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        const PORT_SCAN_INTERVAL: f64 = 2.0;
        let connected = self.connected();
        if !connected && !matches!(self.connection, Connection::Connecting(_)) {
            let now = ui.input(|i| i.time);
            if now - self.last_port_scan > PORT_SCAN_INTERVAL {
                self.last_port_scan = now;
                self.refresh_ports();
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(PORT_SCAN_INTERVAL));
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!connected, |ui| {
                let selected_text = self.selected_port.clone().unwrap_or_else(|| "select port".into());
                egui::ComboBox::from_id_salt("port")
                    .selected_text(selected_text)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for p in &self.ports {
                            let label = match &p.usb {
                                Some(u) => format!(
                                    "{} {} {}",
                                    if p.looks_like_vna() { "*" } else { " " },
                                    p.path,
                                    u.product.clone().unwrap_or_default()
                                ),
                                None => format!("  {}", p.path),
                            };
                            ui.selectable_value(&mut self.selected_port, Some(p.path.clone()), label);
                        }
                    });
                if ui
                    .button("rescan")
                    .on_hover_text("rescan ports (also happens every 2 s while disconnected)")
                    .clicked()
                {
                    self.refresh_ports();
                }
            });
            match &self.connection {
                Connection::Disconnected => {
                    if ui.add_enabled(self.selected_port.is_some(), egui::Button::new("Connect")).clicked() {
                        self.connect_selected();
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
                            "{}  {}",
                            info.summary(),
                            battery_mv.map(|mv| format!("{:.2} V", mv as f64 / 1000.0)).unwrap_or_default()
                        ))
                        .color(Color32::from_rgb(120, 200, 120)),
                    )
                    .on_hover_text(port);
                }
            }
            ui.separator();
            ui.add_enabled_ui(connected, |ui| {
                // Stop is there for anything that is being measured, also a
                // standard or a single sweep while Run is off.
                let sweeping = self.sweeping();
                if sweeping {
                    if ui.button("Stop").clicked() {
                        self.stop_sweep();
                    }
                } else if ui.button("Run").clicked() {
                    self.start_sweep(true);
                }
                if ui.add_enabled(!sweeping, egui::Button::new("Single")).clicked() {
                    self.start_sweep(false);
                }
                if ui.button("Screenshot").clicked() {
                    self.send(Cmd::Screenshot);
                }
            });
            // A sweep that takes a while (a NanoVNA-F, many points, averaged
            // standards) shows how far it is; a quick one would only flicker.
            if let Some((done, total, since)) = self.progress {
                if since.elapsed() > Duration::from_millis(700) && total > 0 {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total as f32)
                            .desired_width(110.0)
                            .text(format!("{done}/{total}")),
                    )
                    .on_hover_text("points measured in the running sweep; Stop takes effect at the next block of data");
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(err) = self.error.clone() {
                    if ui.small_button("×").on_hover_text("dismiss").clicked() {
                        self.error = None;
                    }
                    // One line, cut off when the bar is too narrow; the full text is in the hover.
                    ui.add(egui::Label::new(RichText::new(&err).color(Color32::from_rgb(255, 110, 110))).truncate())
                        .on_hover_text(&err);
                } else if let Some((text, since)) = &self.notice {
                    const NOTICE_SECONDS: f32 = 6.0;
                    let age = since.elapsed().as_secs_f32();
                    if age < NOTICE_SECONDS {
                        ui.add(egui::Label::new(RichText::new(text).color(Color32::from_rgb(150, 200, 150))).truncate());
                        ui.ctx().request_repaint_after(Duration::from_secs_f32(NOTICE_SECONDS - age + 0.05));
                    } else {
                        self.notice = None;
                    }
                }
            });
        });
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open as trace…").clicked() {
                    ui.close();
                    self.open_touchstone(false);
                }
                if ui
                    .add_enabled(self.references.len() < MAX_REFS, egui::Button::new("Open as reference…"))
                    .clicked()
                {
                    ui.close();
                    self.open_touchstone(true);
                }
                if ui
                    .add_enabled(self.shown.is_some(), egui::Button::new("Save Touchstone (.s2p, .s1p)…"))
                    .clicked()
                {
                    ui.close();
                    self.save_s2p();
                }
                ui.separator();
                if ui.button("Load calibration…").clicked() {
                    ui.close();
                    self.load_cal();
                }
                if ui
                    .add_enabled(self.cal.set.is_some(), egui::Button::new("Save calibration…"))
                    .clicked()
                {
                    ui.close();
                    self.save_cal();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Device", |ui| {
                if connected {
                    if ui.button("Disconnect").clicked() {
                        ui.close();
                        self.running = false;
                        self.send(Cmd::Disconnect);
                    }
                } else if ui
                    .add_enabled(
                        self.selected_port.is_some() && !matches!(self.connection, Connection::Connecting(_)),
                        egui::Button::new("Connect"),
                    )
                    .clicked()
                {
                    ui.close();
                    self.connect_selected();
                }
                if ui.button("Rescan ports").clicked() {
                    ui.close();
                    self.refresh_ports();
                }
                ui.separator();
                ui.add_enabled_ui(connected, |ui| {
                    let sweeping = self.sweeping();
                    if sweeping {
                        if ui.button("Stop").clicked() {
                            ui.close();
                            self.stop_sweep();
                        }
                    } else if ui.button("Run").clicked() {
                        ui.close();
                        self.start_sweep(true);
                    }
                    if ui.add_enabled(!sweeping, egui::Button::new("Single sweep")).clicked() {
                        ui.close();
                        self.start_sweep(false);
                    }
                    if ui.button("Device screenshot").clicked() {
                        ui.close();
                        self.send(Cmd::Screenshot);
                    }
                });
            });
            ui.menu_button("View", |ui| {
                if ui.radio(self.view == View::Frequency, "Frequency domain").clicked() {
                    self.view = View::Frequency;
                }
                if ui.radio(self.view == View::Tdr, "Time domain (TDR)").clicked() {
                    self.view = View::Tdr;
                }
                ui.separator();
                ui.checkbox(&mut self.show_s11, "S11");
                ui.checkbox(&mut self.show_s21, "S21");
                ui.separator();
                zoom_menu(ui);
                ui.separator();
                ui.checkbox(&mut self.auto_connect, "Connect at start")
                    .on_hover_text("connect by itself when the program starts and a VNA is plugged in");
                if ui
                    .checkbox(&mut self.dev_tools, "Developer tools")
                    .on_hover_text("adds a Developer menu with the log window and diagnostics for bug reports")
                    .changed()
                    && !self.dev_tools
                {
                    self.show_log = false;
                }
            });
            if self.dev_tools {
                ui.menu_button("Developer", |ui| {
                    ui.checkbox(&mut self.show_log, "Log window");
                    if ui
                        .button("Copy diagnostics")
                        .on_hover_text("version, device, sweep, calibration and the log, for a bug report")
                        .clicked()
                    {
                        ui.close();
                        ctx.copy_text(self.diagnostics());
                        self.notify("diagnostics copied to the clipboard");
                    }
                    if ui.button("Clear log").clicked() {
                        self.log.clear();
                    }
                });
            }
            ui.menu_button("Help", |ui| {
                if ui.button("Manual (English)").clicked() {
                    ui.close();
                    ctx.open_url(egui::OpenUrl::new_tab(manual_url("manual")));
                }
                if ui.button("Handleiding (Nederlands)").clicked() {
                    ui.close();
                    ctx.open_url(egui::OpenUrl::new_tab(manual_url("handleiding")));
                }
                if ui.button("Report a problem…").clicked() {
                    ui.close();
                    ctx.open_url(egui::OpenUrl::new_tab(ISSUES_URL));
                }
                ui.separator();
                if ui.button("About vna-r").clicked() {
                    ui.close();
                    self.show_about = true;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("vna-r {VERSION}"));
            });
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Sweep");
            let mut apply_by_return = false;
            egui::Grid::new("sweep-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                // Return in Start or Stop applies the sweep, like the button below.
                let entered = |r: egui::Response, ui: &egui::Ui| r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.label("Start");
                let r = ui.text_edit_singleline(&mut self.start);
                apply_by_return |= entered(r, ui);
                ui.end_row();
                ui.label("Stop");
                let r = ui.text_edit_singleline(&mut self.stop);
                apply_by_return |= entered(r, ui);
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
                    let has_avg = self.device_limits().is_none_or(|l| l.has_device_average);
                    ui.add_enabled(has_avg, egui::DragValue::new(&mut self.average).range(1..=80));
                    if ui
                        .add_enabled(self.connected() && has_avg, egui::Button::new("set"))
                        .on_hover_text("LiteVNA register 0x40; also changes the bandwidth in the device menu")
                        .on_disabled_hover_text("only the LiteVNA has this register; a NanoVNA-H/-F sets its bandwidth on the device")
                        .clicked()
                    {
                        self.send(Cmd::SetAverage(self.average));
                    }
                });
                ui.end_row();
                ui.label("Sweep avg");
                ui.horizontal(|ui| {
                    let before = self.sweep_avg;
                    ui.add(egui::DragValue::new(&mut self.sweep_avg).range(1..=MAX_SWEEP_AVG)).on_hover_text("complex mean of the last N sweeps, in software; resets when the grid changes");
                    if self.sweep_avg < before {
                        while self.avg_buf.len() > self.sweep_avg {
                            self.avg_buf.pop_front();
                        }
                    }
                    if self.sweep_avg > 1 {
                        ui.small(format!("{}/{}", self.avg_buf.len().min(self.sweep_avg), self.sweep_avg));
                        if ui.small_button("reset").clicked() {
                            self.avg_buf.clear();
                        }
                    }
                });
                ui.end_row();
            });
            if let Some(e) = &self.sweep_error {
                ui.colored_label(Color32::from_rgb(255, 110, 110), e);
            }
            let apply = ui
                .add_enabled(self.connected(), egui::Button::new("Apply sweep"))
                .on_hover_text("use these settings and the segments below; restarts a running sweep, otherwise does one sweep")
                .clicked();
            if apply || (apply_by_return && self.connected()) {
                self.start_sweep(self.running);
            }
            ui.small("Frequencies: 50k, 14.2M, 1.3G. Return in Start or Stop applies. Applying while running restarts the sweep; segment changes need Apply too.");
            if let Some(l) = self.device_limits() {
                let below = parse_hz(&self.start).is_ok_and(|f| f < l.min_hz);
                let above = parse_hz(&self.stop).is_ok_and(|f| f > l.max_hz);
                let range = format!("Device range {} – {}", format_hz(l.min_hz), format_hz(l.max_hz));
                if below || above {
                    ui.colored_label(Color32::from_rgb(230, 150, 80), format!("{range}; the sweep goes outside it and that part is meaningless."));
                } else {
                    ui.small(range);
                }
            }
            self.segments_panel(ui);

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
            self.references_panel(ui);
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
            if let Some(t) = self.tdr_cached() {
                let (res_m, range_m) = (t.resolution_m(), t.max_distance_m());
                ui.small(format!("resolution {:.3} m, alias-free range {:.1} m", res_m, range_m));
                // Resolution is 1/span: a narrow HF sweep smears a cable into one blob.
                if res_m > self.tdr_max_m / 10.0 {
                    let needed_span = C0 * self.tdr_vf / (2.0 * self.tdr_max_m / 20.0);
                    let stop = needed_span.min(self.device_max_hz() as f64);
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
            if self.last_plan.len() > 1 {
                let r = self.tdr_range();
                if let Some(sw) = &self.shown {
                    if r.end > r.start && r.end <= sw.freqs_hz.len() {
                        ui.small(format!("TDR and gate use the widest segment: {} → {}", format_hz(sw.freqs_hz[r.start]), format_hz(sw.freqs_hz[r.end - 1])));
                    }
                }
            }
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
                let res_m = self.tdr_cached().map(|t| t.resolution_m());
                let marker_d = self.tdr_marker.and_then(|i| self.tdr_cached().and_then(|t| t.distance_m.get(i).copied()));
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
            ui.heading("Files");
            if ui.add_enabled(self.shown.is_some(), egui::Button::new("Save Touchstone…"))
                .on_hover_text("as .s2p (S11 and S21), or give the name the extension .s1p for S11 only")
                .clicked() {
                self.save_s2p();
            }
            ui.horizontal(|ui| {
                if ui.button("Open as trace…").on_hover_text("show an .s1p/.s2p file instead of the device; the next sweep replaces it").clicked() {
                    self.open_touchstone(false);
                }
                if ui.add_enabled(self.references.len() < MAX_REFS, egui::Button::new("Open as reference…")).clicked() {
                    self.open_touchstone(true);
                }
            });
            if let Some(name) = &self.file_trace {
                ui.small(format!("showing file: {name}"));
            }
            if let Some(m) = &self.file_message {
                ui.small(m);
            }

        });
    }

    /// Show a short confirmation in the top bar for a few seconds.
    fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.store_references();
        eframe::set_value(storage, eframe::APP_KEY, &self.settings());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        self.housekeeping(&ctx);
        self.shortcuts(&ctx);
        if self.running {
            // keep the UI alive while the worker streams sweeps
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::left("side")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.central(ui));
        self.screenshot_window(&ctx);
        self.about_window(&ctx);
        self.log_window(&ctx);
    }
}
