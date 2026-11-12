//! `vna-cli` — command line for the LiteVNA (binary protocol) and the NanoVNA-H/-F (text shell).

use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use litevna::calibration::{plausibility_warning, CalKit, Calibration, CalibrationSet, OpenModel, Standard};
use litevna::device::plan_segments;
use litevna::discovery;
use litevna::discovery::Kind;
use litevna::freq::{format_hz, parse_hz};
use litevna::protocol::{reg, Channel};
use litevna::timedomain::{gate as compute_gate, parse_distance_range, tdr as compute_tdr, Gate, GateKind, TdrConfig, Window};
use litevna::touchstone::{write_csv, write_s1p, write_s2p};
use litevna::traces::{db, swr};
use litevna::{SweepConfig, SweepResult, Vna};

/// `vna-cli 0.2.0 (abc1234)`: workspace version plus the git revision from build.rs.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("VNA_R_GIT_REV"), ")");

#[derive(Parser)]
#[command(name = "vna-cli", version = VERSION, about = "LiteVNA / NanoVNA command line (PA1BH)")]
struct Cli {
    /// Serial port, e.g. /dev/cu.usbmodemXXXX. Auto-detected when omitted.
    #[arg(short, long, global = true)]
    port: Option<String>,
    /// Print every byte sent and received.
    #[arg(long, global = true)]
    trace: bool,
    /// Machine-readable output: `list`, `info`, `sweep`, `tdr` and `cal show`
    /// print one JSON document on stdout instead of a table. Progress and
    /// warnings stay on stderr.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List serial ports and mark the ones that look like a VNA.
    List,
    /// Identity (registers or `info`) and battery voltage.
    Info,
    /// Read any LiteVNA register by hand: `peek F0`, `peek --size 2 5C`.
    Peek {
        /// Register address in hex (e.g. F0).
        addr: String,
        /// Register width in bytes: 1, 2 or 4.
        #[arg(long, default_value_t = 1)]
        size: u8,
    },
    /// Run one sweep and print / export S11 and S21.
    Sweep {
        #[command(flatten)]
        grid: GridArgs,
        #[command(flatten)]
        measure: MeasureArgs,
        /// Apply a host-side calibration made with `cal`. Without --start/--stop/
        /// --points the calibrated grid is used; another grid inside the calibrated
        /// span is interpolated.
        #[arg(long)]
        cal: Option<PathBuf>,
        /// Write a Touchstone .s2p file.
        #[arg(long)]
        s2p: Option<PathBuf>,
        /// Write a one-port Touchstone .s1p file (S11 only), e.g. for an antenna.
        #[arg(long)]
        s1p: Option<PathBuf>,
        /// Write a CSV file ("-" for stdout).
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Print at most this many table rows (0 = all).
        #[arg(long, default_value_t = 25)]
        rows: usize,
        /// Leave the device in USB data mode afterwards (screen stays frozen).
        #[arg(long)]
        keep_usb_mode: bool,
        /// Time-gate S11: keep only reflections between these one-way distances
        /// in metres, e.g. `--gate 0.9:1.2` (uses --vf). Needs a uniform grid.
        #[arg(long, value_name = "START:STOP")]
        gate: Option<String>,
        /// With --gate: remove the reflections inside the gate instead of keeping them.
        #[arg(long, requires = "gate")]
        gate_notch: bool,
        /// Velocity factor for --gate distances.
        #[arg(long, default_value_t = 0.70)]
        vf: f64,
        /// Port extension: move the reference plane outward by this one-way delay
        /// in picoseconds (S11 turns by twice the delay, S21 by once), applied
        /// after the calibration and before a gate.
        #[arg(long, value_name = "PS", allow_negative_numbers = true)]
        port_extension: Option<f64>,
        /// Sweep several segments into one trace: `START:STOP:POINTS[@cal.json]`,
        /// repeatable, e.g. `--segment 100k:50M:1001@hf.json --segment 50M:6.3G:1001@n6g3.json`.
        /// Used instead of --start/--stop/--points/--cal; segments must not overlap.
        #[arg(long, value_name = "START:STOP:POINTS[@CAL]", conflicts_with_all = ["start", "stop", "points", "cal"])]
        segment: Vec<String>,
    },
    /// Measure a calibration standard into a calibration file, or inspect one.
    ///
    /// Connect the standard, then run e.g. `cal open --file hf.cal.json`.
    /// The first standard fixes the frequency grid (--start/--stop/--points);
    /// later ones reuse it. Needs open, short and load; through and isolation
    /// are optional. Use `cal show` to see what is in the file.
    Cal {
        /// A standard to measure, or what to do with the file.
        #[arg(value_enum)]
        what: CalAction,
        /// Calibration file (JSON).
        #[arg(long, short)]
        file: PathBuf,
        #[command(flatten)]
        grid: GridArgs,
        #[command(flatten)]
        measure: MeasureArgs,
        /// Free text stored in the file (e.g. "N kit, 50 cm cable").
        #[arg(long)]
        note: Option<String>,
        #[command(flatten)]
        kit: KitArgs,
        /// For `fit`: lowest frequency used (default 300M).
        #[arg(long)]
        f_min: Option<String>,
        /// For `fit`: write the fitted correction into the file's kit.
        #[arg(long)]
        apply: bool,
        /// For `fit`: store the asymmetry as open offset delay instead of capacitance.
        #[arg(long)]
        as_delay: bool,
    },
    /// Time-domain reflectometry: where along the line are the reflections?
    ///
    /// Sweeps (ideally with --cal), inverse-transforms S11 and lists the
    /// strongest reflections with distance, |Γ| and the impedance behind them.
    /// Resolution ≈ 1/span, range ≈ 1/(2·step): sweep wide for detail, fine for reach.
    Tdr {
        #[command(flatten)]
        grid: GridArgs,
        #[command(flatten)]
        measure: MeasureArgs,
        /// Calibration file (strongly recommended: raw S11 above 1 GHz is dominated by the bridge).
        #[arg(long)]
        cal: Option<PathBuf>,
        /// Velocity factor of the cable (0.66 RG-58, 0.70 RG-316, 0.83 foam).
        #[arg(long, default_value_t = 0.70)]
        vf: f64,
        /// Window: rectangular, hann, blackman.
        #[arg(long, default_value = "hann")]
        window: String,
        /// Number of reflections to list.
        #[arg(long, default_value_t = 5)]
        peaks: usize,
        /// Only consider reflections up to this distance in metres (default: alias-free range).
        #[arg(long)]
        max_distance: Option<f64>,
        /// Write distance/time/impulse/step/impedance as CSV ("-" for stdout).
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Port extension in picoseconds (one-way), applied before the transform.
        #[arg(long, value_name = "PS", allow_negative_numbers = true)]
        port_extension: Option<f64>,
        /// Read S11 from a Touchstone file (.s1p/.s2p) instead of sweeping; the
        /// file has its own grid and needs no calibration.
        #[arg(long, value_name = "FILE", conflicts_with_all = ["cal", "start", "stop", "points"])]
        from: Option<PathBuf>,
    },
    /// Grab the device screen as PNG.
    Screenshot {
        #[arg(long, default_value = "screenshot.png")]
        out: PathBuf,
        /// Swap the byte order of the RGB565 pixels (the driver already picks the
        /// right one per device; use this if the colours come out wrong).
        #[arg(long)]
        swap_bytes: bool,
    },
    /// Hand the screen back to the on-device UI (LiteVNA: leave USB mode; NanoVNA: restore its sweep and resume).
    Release,
    /// Print a completion script for your shell, e.g. `vna-cli completions zsh > ~/.zfunc/_vna-cli`.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// The first argument of `cal`: a standard to measure into the file, or an action on the file.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum CalAction {
    /// Measure the open standard.
    #[value(alias = "o")]
    Open,
    /// Measure the short standard.
    #[value(alias = "s")]
    Short,
    /// Measure the load standard.
    #[value(alias = "l")]
    Load,
    /// Measure the through (port 1 to port 2).
    #[value(aliases = ["thru", "t"])]
    Through,
    /// Measure the isolation (loads on both ports).
    #[value(aliases = ["isol", "i"])]
    Isolation,
    /// Measure a cable with an open at its far end, for the kit fit.
    #[value(aliases = ["line_open", "lineopen"])]
    LineOpen,
    /// Measure the same cable with a short at its far end.
    #[value(aliases = ["line_short", "lineshort"])]
    LineShort,
    /// Show what is in the file.
    Show,
    /// Set the kit definition (--open-c, --short-l, --load-r, …).
    Kit,
    /// Fit the open/short asymmetry of the kit from line-open and line-short.
    Fit,
    /// Report what the calibration leaves over on the line measurements.
    Check,
    /// Delete the file.
    Clear,
}

impl CalAction {
    /// The standard this action measures, if it is one.
    fn standard(self) -> Option<Standard> {
        Some(match self {
            CalAction::Open => Standard::Open,
            CalAction::Short => Standard::Short,
            CalAction::Load => Standard::Load,
            CalAction::Through => Standard::Through,
            CalAction::Isolation => Standard::Isolation,
            CalAction::LineOpen => Standard::LineOpen,
            CalAction::LineShort => Standard::LineShort,
            CalAction::Show | CalAction::Kit | CalAction::Fit | CalAction::Check | CalAction::Clear => return None,
        })
    }
}

#[derive(Args, Clone)]
struct GridArgs {
    /// Start frequency, e.g. 1M, 50k, 1.3G.
    #[arg(long)]
    start: Option<String>,
    /// Stop frequency.
    #[arg(long)]
    stop: Option<String>,
    /// Number of points (2..=65535).
    #[arg(long)]
    points: Option<u16>,
}

impl GridArgs {
    fn is_empty(&self) -> bool {
        self.start.is_none() && self.stop.is_none() && self.points.is_none()
    }

    /// Build a config, falling back to `default` (e.g. a calibration grid)
    /// and finally to 1–30 MHz, 101 points.
    fn config(&self, default: Option<&[u64]>) -> Result<SweepConfig> {
        if self.is_empty() {
            if let Some(freqs) = default {
                return Ok(SweepConfig::from_frequencies(freqs)?);
            }
        }
        let start = parse_hz(self.start.as_deref().unwrap_or("1M"))?;
        let stop = parse_hz(self.stop.as_deref().unwrap_or("30M"))?;
        let points = self.points.unwrap_or(101);
        Ok(SweepConfig::linear(start, stop, points)?)
    }
}

#[derive(Args, Clone)]
struct MeasureArgs {
    /// IF bandwidth multiplier 1..=80 (higher = slower, quieter). Note: this also
    /// changes the bandwidth setting in the device menu, and register 0x40 cannot be
    /// read back reliably, so it is not restored exactly.
    #[arg(long)]
    average: Option<u8>,
    /// Only measure one channel: s11 or s21.
    #[arg(long)]
    channel: Option<String>,
    /// Ask for data with the on-device calibration applied (LiteVNA mode 3).
    #[arg(long)]
    calibrated: bool,
    /// Use the LiteVNA "READFIFO 0" request instead of 255-point chunks.
    #[arg(long)]
    fifo_all: bool,
    /// Do not restore the device's previous average/channel settings afterwards.
    #[arg(long)]
    keep_settings: bool,
}

/// Kit definition for `cal kit`. Units chosen for humans: fF, pH, ps, Ω.
#[derive(Args, Clone, Default)]
struct KitArgs {
    /// Open fringing capacitance C0 in fF.
    #[arg(long)]
    open_c: Option<f64>,
    /// Open offset delay (one way) in ps.
    #[arg(long)]
    open_delay: Option<f64>,
    /// Short inductance L0 in pH.
    #[arg(long)]
    short_l: Option<f64>,
    /// Short offset delay (one way) in ps.
    #[arg(long)]
    short_delay: Option<f64>,
    /// Load resistance in Ω.
    #[arg(long)]
    load_r: Option<f64>,
    /// Through delay (one way) in ps.
    #[arg(long)]
    through_delay: Option<f64>,
    /// Kit name.
    #[arg(long)]
    kit_name: Option<String>,
}

impl KitArgs {
    fn is_empty(&self) -> bool {
        self.open_c.is_none()
            && self.open_delay.is_none()
            && self.short_l.is_none()
            && self.short_delay.is_none()
            && self.load_r.is_none()
            && self.through_delay.is_none()
            && self.kit_name.is_none()
    }

    fn apply_to(&self, kit: &mut CalKit) {
        if let Some(c) = self.open_c {
            kit.set_open_c0(c * 1e-15);
        }
        if let Some(d) = self.open_delay {
            kit.open.offset_delay_s = d * 1e-12;
        }
        if let Some(l) = self.short_l {
            kit.set_short_l0(l * 1e-12);
        }
        if let Some(d) = self.short_delay {
            kit.short.offset_delay_s = d * 1e-12;
        }
        if let Some(r) = self.load_r {
            kit.load.termination = litevna::calibration::Termination::Load { resistance: r };
        }
        if let Some(d) = self.through_delay {
            kit.through_delay_s = d * 1e-12;
        }
        if let Some(n) = &self.kit_name {
            kit.name = n.clone();
        }
    }
}

struct Conn {
    port: Option<String>,
    trace: bool,
    json: bool,
}

/// `--port-extension <ps>`: move the reference plane of a sweep.
fn apply_port_extension(sweep: &mut SweepResult, ps: Option<f64>) -> Result<()> {
    if let Some(ps) = ps {
        if !ps.is_finite() {
            bail!("--port-extension wants a delay in picoseconds, got {ps}");
        }
        sweep.extend_port(ps * 1e-12);
        eprintln!("port extension {ps} ps (one-way)");
    }
    Ok(())
}

/// A complex number as JSON: `[re, im]`.
fn complex_json(re: f64, im: f64) -> serde_json::Value {
    serde_json::json!([re, im])
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn main() -> std::process::ExitCode {
    // `vna-cli … | head` closes the pipe while we still print. That is not a
    // failure: stop quietly instead of a panic message (println!) or an
    // "Error: Broken pipe" (the writers that return an error).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let closed_pipe = info
            .payload()
            .downcast_ref::<String>()
            .is_some_and(|m| m.contains("failed printing to stdout") && m.contains("Broken pipe"));
        if closed_pipe {
            std::process::exit(0);
        }
        default_hook(info);
    }));
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) if is_closed_pipe(&e) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:?}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The error is a write to a pipe whose reader has gone.
fn is_closed_pipe(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        let io = cause.downcast_ref::<io::Error>().or(match cause.downcast_ref::<litevna::Error>() {
            Some(litevna::Error::Io(io)) => Some(io),
            _ => None,
        });
        io.is_some_and(|io| io.kind() == io::ErrorKind::BrokenPipe)
    })
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let conn = Conn {
        port: cli.port.clone(),
        trace: cli.trace,
        json: cli.json,
    };
    match cli.cmd {
        Cmd::List => list(conn.json),
        Cmd::Info => info(&mut open(&conn)?, conn.json),
        Cmd::Peek { addr, size } => peek(&mut open(&conn)?, &addr, size),
        Cmd::Sweep {
            grid,
            measure,
            cal,
            s2p,
            s1p,
            csv,
            rows,
            keep_usb_mode,
            gate,
            gate_notch,
            vf,
            port_extension,
            segment,
        } => sweep(
            &conn,
            grid,
            measure,
            cal,
            s2p,
            s1p,
            csv,
            rows,
            keep_usb_mode,
            gate,
            gate_notch,
            vf,
            port_extension,
            segment,
        ),
        Cmd::Cal {
            what,
            file,
            grid,
            measure,
            note,
            kit,
            f_min,
            apply,
            as_delay,
        } => cal(&conn, what, &file, grid, measure, note, kit, f_min, apply, as_delay),
        Cmd::Tdr {
            grid,
            measure,
            cal,
            vf,
            window,
            peaks,
            max_distance,
            csv,
            port_extension,
            from,
        } => tdr_cmd(
            &conn,
            grid,
            measure,
            cal,
            vf,
            &window,
            peaks,
            max_distance,
            csv,
            port_extension,
            from,
        ),
        Cmd::Screenshot { out, swap_bytes } => screenshot(&mut open(&conn)?, &out, swap_bytes),
        Cmd::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(shell, &mut Cli::command(), "vna-cli", &mut io::stdout());
            Ok(())
        }
        Cmd::Release => {
            open(&conn)?.release()?;
            println!("device UI released");
            Ok(())
        }
    }
}

fn open(conn: &Conn) -> Result<Vna> {
    let path = match &conn.port {
        Some(p) => p.clone(),
        None => match discovery::find_vna_port()? {
            Some(p) => p,
            None => bail!(
                "no VNA found (LiteVNA USB {:04X}:{:04X}, NanoVNA-H/-F USB {:04X}:{:04X}). Is it switched on and connected? Try `vna-cli list`.",
                discovery::LITEVNA_VID,
                discovery::LITEVNA_PID,
                discovery::STM_VCP_VID,
                discovery::STM_VCP_PID
            ),
        },
    };
    eprintln!("opening {path}");
    let mut dev = Vna::open(&path).with_context(|| format!("opening {path}"))?;
    dev.set_trace(conn.trace);
    Ok(dev)
}

/// The LiteVNA behind a handle, or a clear error for commands that need its registers.
fn lite<'a>(dev: &'a mut Vna, what: &str) -> Result<&'a mut litevna::LiteVna> {
    Ok(dev.require_lite(what)?)
}

fn list(json: bool) -> Result<()> {
    let ports = discovery::list_ports()?;
    if json {
        let rows: Vec<_> = ports
            .iter()
            .map(|p| {
                serde_json::json!({
                    "path": p.path,
                    "vna": p.kind().map(|k| k.name()),
                    "usb": p.usb.as_ref().map(|u| serde_json::json!({
                        "vid": format!("{:04X}", u.vid),
                        "pid": format!("{:04X}", u.pid),
                        "manufacturer": u.manufacturer,
                        "product": u.product,
                        "serial_number": u.serial_number,
                    })),
                })
            })
            .collect();
        return print_json(&serde_json::Value::Array(rows));
    }
    if ports.is_empty() {
        println!("no serial ports found");
        return Ok(());
    }
    for p in ports {
        let mark = match p.kind() {
            Some(Kind::LiteVna) => "LiteVNA ",
            Some(Kind::NanoVnaShell) => "NanoVNA ",
            None => "        ",
        };
        match &p.usb {
            Some(u) => println!(
                "{mark}{:<32} usb {:04X}:{:04X} {} {} {}",
                p.path,
                u.vid,
                u.pid,
                u.manufacturer.clone().unwrap_or_default(),
                u.product.clone().unwrap_or_default(),
                u.serial_number.clone().map(|s| format!("sn={s}")).unwrap_or_default()
            ),
            None => println!("{mark}{:<32} (not usb)", p.path),
        }
    }
    Ok(())
}

fn info(dev: &mut Vna, json: bool) -> Result<()> {
    let vi = dev.info()?;
    if json {
        return print_json(&serde_json::json!({
            "device": vi.kind.name(),
            "model": vi.model,
            "firmware": vi.firmware,
            "detail": vi.detail,
            "battery_mv": dev.battery_millivolts().ok(),
            "range_hz": [vi.limits.min_hz, vi.limits.max_hz],
            "max_points_per_scan": vi.limits.max_points_per_scan,
            "device_average": vi.limits.has_device_average,
            "registers": vi.lite.as_ref().map(|i| serde_json::json!({
                "variant": i.variant,
                "protocol_version": i.protocol_version,
                "hardware_revision": i.hardware_revision,
                "fw_major": i.fw_major,
                "fw_minor": i.fw_minor,
                "dfu": i.in_dfu_mode(),
            })),
        }));
    }
    let Some(i) = vi.lite else {
        println!("device             : {} (NanoVNA text shell)", vi.model);
        println!("firmware           : {}", vi.firmware);
        if let Some(sh) = &vi.shell {
            if !sh.build.is_empty() {
                println!("build              : {}", sh.build);
            }
        }
        println!(
            "range              : {} – {}, {} points per scan",
            format_hz(vi.limits.min_hz),
            format_hz(vi.limits.max_hz),
            vi.limits.max_points_per_scan.map(|n| n.to_string()).unwrap_or_default()
        );
        match dev.battery_millivolts() {
            Ok(mv) => println!("battery            : {:.3} V", mv as f64 / 1000.0),
            Err(e) => println!("battery            : not readable ({e})"),
        }
        return Ok(());
    };
    println!(
        "device variant     : 0x{:02X}{}",
        i.variant,
        if i.variant == 0x02 { " (NanoVNA V2 / LiteVNA family)" } else { "" }
    );
    println!("protocol version   : 0x{:02X}", i.protocol_version);
    println!("hardware revision  : 0x{:02X}", i.hardware_revision);
    if i.in_dfu_mode() {
        println!("firmware           : DFU / bootloader mode (bootloader minor 0x{:02X})", i.fw_minor);
        println!("the device is waiting for a firmware upload; power-cycle it to run normally.");
        return Ok(());
    }
    println!("firmware           : {}.{}", i.fw_major, i.fw_minor);
    match dev.battery_millivolts() {
        Ok(mv) => println!("battery            : {:.3} V", mv as f64 / 1000.0),
        Err(e) => println!("battery            : not readable ({e})"),
    }
    Ok(())
}

fn peek(dev: &mut Vna, addr: &str, size: u8) -> Result<()> {
    let dev = lite(dev, "peek")?;
    let a = u8::from_str_radix(addr.trim_start_matches("0x"), 16).context("address must be hex, e.g. F0")?;
    let value: u64 = match size {
        1 => dev.read_u8(a)? as u64,
        2 => dev.read_u16(a)? as u64,
        4 => dev.read_u32(a)? as u64,
        _ => bail!("size must be 1, 2 or 4"),
    };
    println!("reg 0x{a:02X} ({size} byte) = 0x{value:0w$X} = {value}", w = size as usize * 2);
    Ok(())
}

/// One sweep with the measurement options applied and device settings restored.
fn measure(dev: &mut Vna, cfg: &SweepConfig, m: &MeasureArgs) -> Result<SweepResult> {
    let mut cfg = *cfg;
    cfg.calibrated = m.calibrated;

    // These registers persist in the device, so remember what was there.
    let prev_average = match m.average {
        Some(avg) => {
            let d = lite(dev, "--average")?;
            let prev = d.average()?;
            d.set_average(avg)?;
            Some(prev)
        }
        None => None,
    };
    let prev_channel = if m.channel.is_some() {
        Some(lite(dev, "--channel")?.channel()?)
    } else {
        None
    };
    if let Some(ch) = &m.channel {
        let ch = match ch.to_ascii_lowercase().as_str() {
            "s11" => Channel::S11Only,
            "s21" => Channel::S21Only,
            "both" => Channel::Both,
            other => bail!("unknown channel {other:?}, use s11, s21 or both"),
        };
        lite(dev, "--channel")?.set_channel(ch)?;
    }

    eprintln!(
        "sweep {} → {} in {} points (step {}){}",
        format_hz(cfg.start_hz),
        format_hz(cfg.stop_hz()),
        cfg.points,
        format_hz(cfg.step_hz),
        if cfg.calibrated { ", device-calibrated data" } else { ", raw data" }
    );
    let result = dev.sweep(&cfg, m.fifo_all);

    if !m.keep_settings {
        if let Some(prev) = prev_average {
            lite(dev, "--average")?.write_u8(reg::AVERAGE, prev)?;
        }
        if let Some(prev) = prev_channel {
            lite(dev, "--channel")?.write_u8(reg::CHANNEL_SELECT, prev)?;
        }
    }
    let result = result?;
    if dev.kind() != Kind::LiteVna {
        eprintln!(
            "got {} points in {:.0} ms ({:.0} pts/s)",
            result.freqs_hz.len(),
            result.elapsed.as_secs_f64() * 1000.0,
            result.freqs_hz.len() as f64 / result.elapsed.as_secs_f64().max(1e-9)
        );
        return Ok(result);
    }
    eprintln!(
        "got {} points in {:.0} ms ({:.0} pts/s), missing {}, duplicates (stale FIFO records) {}, checksum ok {} / bad {}",
        result.freqs_hz.len() - result.missing_points(),
        result.elapsed.as_secs_f64() * 1000.0,
        cfg.points as f64 / result.elapsed.as_secs_f64().max(1e-9),
        result.missing_points(),
        result.duplicates,
        result.checksum_ok,
        result.checksum_bad
    );
    if result.missing_points() > 0 {
        let idx = result.missing_indexes();
        eprintln!(
            "missing indexes: {:?}{}",
            &idx[..idx.len().min(40)],
            if idx.len() > 40 { " …" } else { "" }
        );
    }
    Ok(result)
}

/// What one measurement run produced.
struct Acquired {
    cfg: SweepConfig,
    raw: SweepResult,
    /// Calibrated when a calibration was given, otherwise the raw sweep.
    result: SweepResult,
    calibrated: bool,
    /// Model line of the device that measured it, for file comments.
    model: String,
}

/// The model for a file comment: `LiteVNA`, `NanoVNA-F 4.3`, or the protocol name.
fn model_of(dev: &mut Vna) -> String {
    dev.info().map(|i| i.model).unwrap_or_else(|_| dev.kind().name().to_string())
}

/// Resolve the grid (from args or the calibration file), sweep once and apply
/// the calibration. Shared by `sweep` and `tdr`.
/// Multi-segment sweep: each `START:STOP:POINTS[@cal.json]` is swept in
/// turn (sorted by start) and corrected with its own calibration; the parts
/// are concatenated into one result.
fn acquire_segments(conn: &Conn, specs: &[String], m: &MeasureArgs, keep_usb_mode: bool) -> Result<Acquired> {
    let mut plan: Vec<(SweepConfig, Option<PathBuf>)> = Vec::new();
    for spec in specs {
        let (grid_part, cal_part) = match spec.split_once('@') {
            Some((g, c)) => (g, Some(PathBuf::from(c))),
            None => (spec.as_str(), None),
        };
        let cfg =
            SweepConfig::parse_segment(grid_part).with_context(|| format!("--segment wants START:STOP:POINTS[@cal.json], got {spec:?}"))?;
        plan.push((cfg, cal_part));
    }
    let plan = plan_segments(plan)?;
    // Solve the calibrations first so a bad file fails before touching the device.
    let mut cals: Vec<Option<Calibration>> = Vec::new();
    for (cfg, path) in &plan {
        cals.push(match path {
            Some(p) => {
                let set = load_cal(p)?;
                let freqs = cfg.frequencies();
                let cal = set.solve_for(&freqs).with_context(|| {
                    format!(
                        "cannot use calibration {} on segment {}–{}",
                        p.display(),
                        format_hz(cfg.start_hz),
                        format_hz(cfg.stop_hz())
                    )
                })?;
                for w in set.interpolation_warnings(&freqs) {
                    eprintln!("WARNING: {}: {w}", p.display());
                }
                Some(cal)
            }
            None => None,
        });
    }
    let mut dev = open(conn)?;
    let mut raws = Vec::new();
    let mut results = Vec::new();
    for ((cfg, path), cal) in plan.iter().zip(&cals) {
        eprintln!(
            "segment {} → {} in {} points{}",
            format_hz(cfg.start_hz),
            format_hz(cfg.stop_hz()),
            cfg.points,
            match path {
                Some(p) => format!(", calibration {}", p.display()),
                None => ", raw".to_string(),
            }
        );
        let raw = measure(&mut dev, cfg, m)?;
        results.push(match cal {
            Some(c) => c.apply(&raw)?,
            None => raw.clone(),
        });
        raws.push(raw);
    }
    let model = model_of(&mut dev);
    if keep_usb_mode {
        dev.set_release_on_drop(false);
    } else {
        dev.release()?;
    }
    let calibrated = cals.iter().any(|c| c.is_some());
    Ok(Acquired {
        cfg: plan[0].0,
        raw: SweepResult::concat(&raws),
        calibrated,
        result: SweepResult::concat(&results),
        model,
    })
}

fn acquire(conn: &Conn, grid: &GridArgs, m: &MeasureArgs, cal_file: Option<&Path>, keep_usb_mode: bool) -> Result<Acquired> {
    // With a calibration file the grid defaults to the calibrated one; an
    // explicit --start/--stop/--points inside that span is interpolated.
    let (cfg, calibration): (SweepConfig, Option<Calibration>) = match cal_file {
        Some(path) => {
            let set = load_cal(path)?;
            let cfg = grid.config(Some(&set.freqs_hz))?;
            let freqs = cfg.frequencies();
            let interpolated = freqs != set.freqs_hz;
            let cal = set
                .solve_for(&freqs)
                .with_context(|| format!("cannot use calibration {} on this grid", path.display()))?;
            eprintln!(
                "using calibration {} ({} points {} → {}, kit {}{}){}",
                path.display(),
                set.freqs_hz.len(),
                format_hz(set.freqs_hz[0]),
                format_hz(*set.freqs_hz.last().unwrap()),
                set.kit.name,
                if cal.has_through() { ", with through" } else { ", S21 uncorrected" },
                if interpolated {
                    format!(", interpolated to {} points", freqs.len())
                } else {
                    String::new()
                }
            );
            for w in set.interpolation_warnings(&freqs) {
                eprintln!("WARNING: {w}");
            }
            (cfg, Some(cal))
        }
        None => (grid.config(None)?, None),
    };

    let mut dev = open(conn)?;
    let raw = measure(&mut dev, &cfg, m)?;
    let model = model_of(&mut dev);
    if keep_usb_mode {
        dev.set_release_on_drop(false);
    } else {
        dev.release()?;
    }
    let result = match &calibration {
        Some(cal) => cal.apply(&raw)?,
        None => raw.clone(),
    };
    Ok(Acquired {
        cfg,
        raw,
        calibrated: calibration.is_some(),
        result,
        model,
    })
}

#[allow(clippy::too_many_arguments)]
fn sweep(
    conn: &Conn,
    grid: GridArgs,
    m: MeasureArgs,
    cal_file: Option<PathBuf>,
    s2p: Option<PathBuf>,
    s1p: Option<PathBuf>,
    csv: Option<PathBuf>,
    rows: usize,
    keep_usb_mode: bool,
    gate_arg: Option<String>,
    gate_notch: bool,
    vf: f64,
    port_extension: Option<f64>,
    segments: Vec<String>,
) -> Result<()> {
    if conn.json && csv.as_deref().is_some_and(|p| p.as_os_str() == "-") {
        bail!("--json and --csv - both write to stdout; give the CSV a file name");
    }
    let Acquired {
        cfg,
        raw,
        mut result,
        calibrated,
        model,
    } = if segments.is_empty() {
        acquire(conn, &grid, &m, cal_file.as_deref(), keep_usb_mode)?
    } else {
        acquire_segments(conn, &segments, &m, keep_usb_mode)?
    };

    apply_port_extension(&mut result, port_extension)?;
    // What was done to the data after the calibration, for the file comments.
    let mut processing = String::new();
    if let Some(ps) = port_extension.filter(|ps| *ps != 0.0) {
        processing.push_str(&format!(", port extension {ps:.1} ps"));
    }
    if let Some(spec) = &gate_arg {
        let (start_m, stop_m) = parse_distance_range(spec).context("--gate")?;
        let kind = if gate_notch { GateKind::Notch } else { GateKind::BandPass };
        let g = Gate::from_distance(start_m, stop_m, vf, kind);
        let tcfg = TdrConfig {
            velocity_factor: vf,
            ..Default::default()
        };
        let gated = compute_gate(&result.freqs_hz, &result.s11, &tcfg, &g)?;
        eprintln!(
            "time gate {} {:.3}..{:.3} m (VF {vf}): outer {} of the band on each side is unreliable",
            g.kind.name(),
            start_m,
            stop_m,
            format_hz(gated.edge_hz as u64)
        );
        result.s11 = gated.s11;
        processing.push_str(&format!(", S11 time-gated {start_m:.2}..{stop_m:.2} m {}", g.kind.name()));
    }
    let state = match (calibrated, &cal_file, cfg.calibrated) {
        (true, Some(c), _) => format!("host calibration {}", c.display()),
        (true, None, _) => "host calibration per segment".to_string(),
        (_, _, true) => "device calibrated".to_string(),
        _ => "uncalibrated raw".to_string(),
    };

    if conn.json {
        let points: Vec<_> = (0..result.freqs_hz.len())
            .map(|i| {
                serde_json::json!({
                    "freq_hz": result.freqs_hz[i],
                    "s11": complex_json(result.s11[i].re, result.s11[i].im),
                    "s21": complex_json(result.s21[i].re, result.s21[i].im),
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "device": model,
            "calibrated": calibrated,
            "gated": gate_arg.is_some(),
            "z0_ohm": 50.0,
            "elapsed_ms": raw.elapsed.as_millis() as u64,
            "points": points,
        }))?;
    } else if csv.as_deref().is_none_or(|p| p.as_os_str() != "-") {
        // With `--csv -` stdout carries the CSV and nothing else.
        print_table(&result, rows);
    }
    if let Some(p) = raw.points.iter().flatten().next().filter(|_| conn.trace) {
        eprintln!(
            "first raw record: fwd0=({},{}) rev0=({},{}) rev1=({},{}) reserved={:02X?} checksum=0x{:02X}",
            p.fwd0_re, p.fwd0_im, p.rev0_re, p.rev0_im, p.rev1_re, p.rev1_im, p.reserved, p.checksum
        );
    }

    if let Some(path) = &s2p {
        let f = BufWriter::new(File::create(path).with_context(|| format!("creating {}", path.display()))?);
        let comment = format!("{model} sweep, {} points, {state}{processing}", result.freqs_hz.len());
        write_s2p(f, &result, Some(&comment))?;
        eprintln!("wrote {}", path.display());
    }
    if let Some(path) = &s1p {
        let f = BufWriter::new(File::create(path).with_context(|| format!("creating {}", path.display()))?);
        let comment = format!("{model} sweep, {} points, S11 only, {state}{processing}", result.freqs_hz.len());
        write_s1p(f, &result, Some(&comment))?;
        eprintln!("wrote {}", path.display());
    }
    if let Some(path) = &csv {
        if path.as_os_str() == "-" {
            write_csv(io::stdout().lock(), &result)?;
        } else {
            let f = BufWriter::new(File::create(path).with_context(|| format!("creating {}", path.display()))?);
            write_csv(f, &result)?;
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn tdr_cmd(
    conn: &Conn,
    grid: GridArgs,
    m: MeasureArgs,
    cal_file: Option<PathBuf>,
    vf: f64,
    window: &str,
    n_peaks: usize,
    max_distance: Option<f64>,
    csv: Option<PathBuf>,
    port_extension: Option<f64>,
    from: Option<PathBuf>,
) -> Result<()> {
    let window = Window::parse(window).with_context(|| format!("unknown window {window:?}"))?;
    let mut result = match from {
        Some(path) => {
            let r = litevna::touchstone::read_touchstone_file(&path).with_context(|| format!("reading {}", path.display()))?;
            eprintln!(
                "read {}: {} points {} → {}",
                path.display(),
                r.freqs_hz.len(),
                format_hz(r.freqs_hz[0]),
                format_hz(*r.freqs_hz.last().unwrap())
            );
            r
        }
        None => {
            if cal_file.is_none() {
                eprintln!("note: no --cal given; raw S11 includes the bridge's own reflections");
            }
            acquire(conn, &grid, &m, cal_file.as_deref(), false)?.result
        }
    };
    apply_port_extension(&mut result, port_extension)?;
    let cfg = TdrConfig {
        window,
        velocity_factor: vf,
        fft_size: 0,
    };
    let t = compute_tdr(&result.freqs_hz, &result.s11, &cfg)?;

    let limit_m = max_distance.unwrap_or(t.max_distance_m());
    // With `--csv -` stdout carries the CSV and nothing else; the summary goes to stderr.
    let csv_on_stdout = csv.as_deref().is_some_and(|p| p.as_os_str() == "-");
    if csv_on_stdout {
        eprintln!(
            "TDR: resolution ≈ {:.3} m, alias-free range {:.1} m, VF {:.2}, {} window",
            t.resolution_m(),
            t.max_distance_m(),
            vf,
            window.name()
        );
    } else if !conn.json {
        println!(
            "TDR: resolution ≈ {:.3} m ({:.0} ps), alias-free range {:.1} m ({:.0} ns), VF {:.2}, {} window",
            t.resolution_m(),
            t.resolution_s * 1e12,
            t.max_distance_m(),
            t.max_time_s * 1e9,
            vf,
            window.name()
        );
    }
    let dt = t.time_s[1] - t.time_s[0];
    let min_sep = ((t.resolution_s / dt).ceil() as usize).max(2);
    let peaks: Vec<_> = t
        .peaks(n_peaks * 4, min_sep)
        .into_iter()
        .filter(|(i, _)| t.distance_m[*i] <= limit_m)
        .take(n_peaks)
        .collect();
    if conn.json {
        let peaks: Vec<_> = peaks
            .iter()
            .map(|(i, mag)| {
                let j = (i + min_sep).min(t.impedance_ohm.len() - 1);
                serde_json::json!({
                    "distance_m": t.distance_m[*i],
                    "time_ns": t.time_s[*i] * 1e9,
                    "gamma_db": db(*mag),
                    "z_behind_ohm": t.impedance_ohm[j],
                })
            })
            .collect();
        let trace: Vec<_> = (0..t.time_s.len())
            .take_while(|&i| t.distance_m[i] <= limit_m)
            .map(|i| {
                serde_json::json!({
                    "distance_m": t.distance_m[i],
                    "time_ns": t.time_s[i] * 1e9,
                    "impulse_db": db(t.impulse_mag[i]),
                    "step": t.step[i],
                    "z_ohm": t.impedance_ohm[i],
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "resolution_m": t.resolution_m(),
            "alias_free_range_m": t.max_distance_m(),
            "velocity_factor": vf,
            "window": window.name(),
            "peaks": peaks,
            "trace": trace,
        }))?;
    } else if !csv_on_stdout {
        println!("{:>4} {:>10} {:>10} {:>9} {:>10}", "#", "distance", "time", "|Γ| dB", "Z behind");
    }
    for (k, (i, mag)) in peaks.iter().enumerate().filter(|_| !conn.json && !csv_on_stdout) {
        // impedance a little beyond the edge, where the step has settled
        let j = (i + min_sep).min(t.impedance_ohm.len() - 1);
        println!(
            "{:>4} {:>8.3} m {:>8.2} ns {:>9.1} {:>8.1} Ω",
            k + 1,
            t.distance_m[*i],
            t.time_s[*i] * 1e9,
            db(*mag),
            t.impedance_ohm[j]
        );
    }
    if let Some(path) = &csv {
        use std::io::Write;
        // "-" is stdout, as for `sweep --csv -`.
        let mut f: Box<dyn Write> = if path.as_os_str() == "-" {
            Box::new(io::stdout().lock())
        } else {
            Box::new(BufWriter::new(
                File::create(path).with_context(|| format!("creating {}", path.display()))?,
            ))
        };
        writeln!(f, "distance_m,time_ns,impulse_db,step,z_ohm")?;
        for i in 0..t.time_s.len() {
            if t.distance_m[i] > limit_m {
                break;
            }
            writeln!(
                f,
                "{:.5},{:.4},{:.2},{:.5},{:.2}",
                t.distance_m[i],
                t.time_s[i] * 1e9,
                db(t.impulse_mag[i]),
                t.step[i],
                t.impedance_ohm[i]
            )?;
        }
        f.flush()?;
        if path.as_os_str() != "-" {
            eprintln!("wrote {}", path.display());
        }
    }
    Ok(())
}

fn print_table(result: &SweepResult, rows: usize) {
    let n = result.freqs_hz.len();
    let shown = if rows == 0 { n } else { rows.min(n) };
    let stride = (n + shown - 1) / shown.max(1);
    println!(
        "{:>14} {:>9} {:>8} {:>7} {:>9} {:>8}",
        "freq", "S11 dB", "S11 °", "SWR", "S21 dB", "S21 °"
    );
    for i in (0..n).step_by(stride.max(1)) {
        let s11 = result.s11[i];
        let s21 = result.s21[i];
        println!(
            "{:>14} {:>9.2} {:>8.1} {:>7.2} {:>9.2} {:>8.1}",
            format_hz(result.freqs_hz[i]),
            db(s11.norm()),
            s11.arg().to_degrees(),
            swr(s11.norm()),
            db(s21.norm()),
            s21.arg().to_degrees()
        );
    }
}

fn load_cal(path: &Path) -> Result<CalibrationSet> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(CalibrationSet::from_json(&text)?)
}

fn save_cal(path: &Path, set: &CalibrationSet) -> Result<()> {
    fs::write(path, set.to_json()?).with_context(|| format!("writing {}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn cal(
    conn: &Conn,
    what: CalAction,
    file: &Path,
    grid: GridArgs,
    m: MeasureArgs,
    note: Option<String>,
    kit: KitArgs,
    f_min: Option<String>,
    apply: bool,
    as_delay: bool,
) -> Result<()> {
    // Options that only one action reads are an error with any other, not silently dropped.
    if what != CalAction::Fit && (f_min.is_some() || apply || as_delay) {
        bail!("--f-min, --apply and --as-delay belong to `cal fit`");
    }
    if what.standard().is_none() && note.is_some() {
        bail!("--note is stored when a standard is measured (cal open, short, load, …)");
    }
    if what.standard().is_none() && !grid.is_empty() {
        bail!("--start/--stop/--points set the grid when a standard is measured; this action uses the file as it is");
    }
    let existing = if file.exists() { Some(load_cal(file)?) } else { None };

    match what {
        CalAction::Show => {
            let set = existing.with_context(|| format!("{} does not exist", file.display()))?;
            return show_cal(&set, conn.json);
        }
        CalAction::Kit => {
            let mut set = existing.with_context(|| format!("{} does not exist", file.display()))?;
            if kit.is_empty() {
                bail!("give kit parameters, e.g. --open-c 55 (fF) --short-l 0 --load-r 50");
            }
            kit.apply_to(&mut set.kit);
            if set.kit.name == "ideal" {
                set.kit.name = "custom".into();
            }
            save_cal(file, &set)?;
            println!("kit updated in {}", file.display());
            return show_cal(&set, conn.json);
        }
        CalAction::Check => {
            let set = existing.with_context(|| format!("{} does not exist", file.display()))?;
            let chk = set.check_lines(1e9)?;
            println!(
                "line check: far end at {:.2} ns round trip ({:.3} m at VF 0.70); gates blur the outer {:.0} MHz of the band",
                chk.far_end_s * 1e9,
                chk.far_end_s * litevna::timedomain::C0 * 0.70 / 2.0,
                chk.edge_hz / 1e6
            );
            println!("ripple = RMS of linear |Γ| around a smooth trend; 'gated' keeps only the far-end reflection");
            println!(
                "{:>13} {:>10} {:>10} {:>10} {:>10} {:>12}",
                "band GHz", "open", "short", "gated o", "gated s", "near-end dB"
            );
            for b in &chk.bands {
                println!(
                    "{:>6.2}-{:<6.2} {:>10.4} {:>10.4} {:>10.4} {:>10.4} {:>12.1}",
                    b.f_lo_hz / 1e9,
                    b.f_hi_hz / 1e9,
                    b.ripple_open,
                    b.ripple_short,
                    b.gated_open,
                    b.gated_short,
                    db(b.near_end_mag)
                );
            }
            println!(
                "near-end term: open/short agree to {:.4} rms; asymmetry re-fit on the gated lines ΔC = {:+.2} fF",
                chk.near_end_open_short_rms,
                chk.gated_refit_delta_c_farad * 1e15
            );
            println!("The near-end term is residual directivity (load definition) plus the cable's input connector;");
            println!("re-measure line-short with the cable flipped: if it changes, it is the connector, not the load.");
            return Ok(());
        }
        CalAction::Fit => {
            let mut set = existing.with_context(|| format!("{} does not exist", file.display()))?;
            let f_min = parse_hz(f_min.as_deref().unwrap_or("300M"))?;
            let model = if as_delay { OpenModel::Delay } else { OpenModel::Capacitance };
            let fit = set.fit_open(f_min, model)?;
            println!("open/short asymmetry fit from {} points ≥ {}:", fit.points_used, format_hz(f_min));
            println!(
                "  ripple RMS before {:.4}  after {:.4}  (≈ radians of phase asymmetry)",
                fit.ripple_rms_before, fit.ripple_rms_after
            );
            println!(
                "  as capacitance : {:+.1} fF   (kit open C0 {:.1} fF → {:.1} fF)",
                fit.delta_c_farad * 1e15,
                set.kit.open_c0() * 1e15,
                (set.kit.open_c0() + fit.delta_c_farad) * 1e15
            );
            println!(
                "  as offset delay: {:+.2} ps   (kit open delay {:.2} ps → {:.2} ps)",
                fit.delta_delay_s * 1e12,
                set.kit.open.offset_delay_s * 1e12,
                (set.kit.open.offset_delay_s + fit.delta_delay_s) * 1e12
            );
            println!(
                "  (short assumed as defined in the kit; the fit is exact for the {} model)",
                if as_delay { "delay" } else { "capacitance" }
            );
            if apply {
                set.apply_open_fit(&fit, model);
                save_cal(file, &set)?;
                println!("applied to {}", file.display());
            } else {
                println!("(re-run with --apply [--as-delay] to store it in the kit)");
            }
            return Ok(());
        }
        CalAction::Clear => {
            if file.exists() {
                fs::remove_file(file)?;
                println!("removed {}", file.display());
            }
            return Ok(());
        }
        _ => {}
    }
    let standard = what.standard().expect("every action that is not a standard returned above");

    // Grid: from the existing file unless this is the first standard.
    let mut set = match existing {
        Some(set) => {
            if !grid.is_empty() {
                let wanted = grid.config(None)?;
                if wanted.frequencies() != set.freqs_hz {
                    bail!(
                        "{} already has a grid of {} points {}..{}; omit --start/--stop/--points or use `cal clear`",
                        file.display(),
                        set.freqs_hz.len(),
                        format_hz(set.freqs_hz[0]),
                        format_hz(*set.freqs_hz.last().unwrap())
                    );
                }
            }
            set
        }
        None => {
            if grid.is_empty() {
                bail!("new calibration file: give --start, --stop and --points for the first standard");
            }
            let cfg = grid.config(None)?;
            CalibrationSet::new(cfg.frequencies(), CalKit::ideal())
        }
    };
    if note.is_some() {
        set.note = note;
    }

    let cfg = SweepConfig::from_frequencies(&set.freqs_hz)?;
    eprintln!(
        "measuring standard {:?} — make sure it is connected to port 1{}",
        standard.name(),
        match standard {
            Standard::Through => " and port 2",
            Standard::Isolation => " (loads on both ports)",
            Standard::LineOpen | Standard::LineShort => " via the line/cable, standard at the far end",
            _ => "",
        }
    );
    let mut dev = open(conn)?;
    let sweep = measure(&mut dev, &cfg, &m)?;
    dev.release()?;

    // Quick sanity numbers so a wrong standard is noticed immediately.
    let mid = sweep.freqs_hz.len() / 2;
    eprintln!(
        "raw at {}: |S11| {:.2} dB ∠{:.0}°, |S21| {:.1} dB",
        format_hz(sweep.freqs_hz[mid]),
        db(sweep.s11[mid].norm()),
        sweep.s11[mid].arg().to_degrees(),
        db(sweep.s21[mid].norm())
    );

    if let Some(warning) = plausibility_warning(standard, &sweep) {
        eprintln!("WARNING: {warning}");
    }
    set.set(standard, &sweep)?;
    save_cal(file, &set)?;
    println!("stored {} in {}", standard.name(), file.display());
    show_cal(&set, conn.json)
}

/// Catch the classic mistakes (loose standard, wrong standard) before they
/// poison a calibration: medians of the raw magnitude over the sweep.
fn show_cal(set: &CalibrationSet, json: bool) -> Result<()> {
    if json {
        let have: Vec<_> = Standard::ALL
            .iter()
            .chain(Standard::LINE.iter())
            .filter(|s| set.has(**s))
            .map(|s| s.name())
            .collect();
        return print_json(&serde_json::json!({
            "format": set.format,
            "points": set.freqs_hz.len(),
            "start_hz": set.freqs_hz[0],
            "stop_hz": set.freqs_hz.last(),
            "note": set.note,
            "measured": have,
            "missing": set.missing_standards().iter().map(|s| s.name()).collect::<Vec<_>>(),
            "jumps": set.jumps().iter().map(|j| serde_json::json!({
                "standard": j.standard.name(),
                "f_lo_hz": j.f_lo_hz,
                "f_hi_hz": j.f_hi_hz,
                "ratio": j.ratio,
            })).collect::<Vec<_>>(),
            "kit": {
                "name": set.kit.name,
                "open_c0_farad": set.kit.open_c0(),
                "open_delay_s": set.kit.open.offset_delay_s,
                "short_l0_henry": set.kit.short_l0(),
                "short_delay_s": set.kit.short.offset_delay_s,
                "load_ohm": set.kit.load_r(),
                "through_delay_s": set.kit.through_delay_s,
            },
        }));
    }
    println!(
        "grid     : {} points, {} → {} (step {})",
        set.freqs_hz.len(),
        format_hz(set.freqs_hz[0]),
        format_hz(*set.freqs_hz.last().unwrap()),
        format_hz(set.freqs_hz.get(1).copied().unwrap_or(0) - set.freqs_hz[0])
    );
    println!(
        "kit      : {}  (open C0 {:.1} fF, delay {:.1} ps; short L0 {:.1} pH, delay {:.1} ps; load {:.2} Ω; through delay {:.1} ps)",
        set.kit.name,
        set.kit.open_c0() * 1e15,
        set.kit.open.offset_delay_s * 1e12,
        set.kit.short_l0() * 1e12,
        set.kit.short.offset_delay_s * 1e12,
        set.kit.load_r(),
        set.kit.through_delay_s * 1e12
    );
    if let Some(n) = &set.note {
        println!("note     : {n}");
    }
    let have: Vec<_> = Standard::ALL
        .iter()
        .chain(Standard::LINE.iter())
        .filter(|s| set.has(**s))
        .map(|s| s.name())
        .collect();
    let missing: Vec<_> = [Standard::Open, Standard::Short, Standard::Load]
        .iter()
        .filter(|s| !set.has(**s))
        .map(|s| s.name())
        .collect();
    println!(
        "measured : {}",
        if have.is_empty() {
            "nothing yet".to_string()
        } else {
            have.join(", ")
        }
    );
    for j in set.jumps() {
        println!(
            "jump     : {} → {} (raw {}, {:.0}× the steps around it); a sweep on another grid is unreliable between these points",
            format_hz(j.f_lo_hz),
            format_hz(j.f_hi_hz),
            j.standard.name(),
            j.ratio
        );
    }
    match set.solve() {
        Ok(cal) => {
            println!(
                "status   : complete for S11{}",
                if cal.has_through() {
                    " and S21 (through measured)"
                } else {
                    "; S21 uncorrected until a through is measured"
                }
            );
            // Show the error terms at a few points: a feel for the hardware.
            println!("{:>14} {:>12} {:>12} {:>12}", "freq", "|e00| dB", "|e11| dB", "|e10e01| dB");
            let n = cal.freqs_hz.len();
            for i in (0..n).step_by((n / 5).max(1)) {
                let t = &cal.terms[i];
                println!(
                    "{:>14} {:>12.1} {:>12.1} {:>12.2}",
                    format_hz(cal.freqs_hz[i]),
                    db(t.e00.norm()),
                    db(t.e11.norm()),
                    db(t.e10e01.norm())
                );
            }
        }
        Err(_) => println!("status   : incomplete, still needed: {}", missing.join(", ")),
    }
    Ok(())
}

fn screenshot(dev: &mut Vna, out: &Path, swap_bytes: bool) -> Result<()> {
    eprintln!("requesting screenshot …");
    let shot = dev.screenshot()?;
    eprintln!(
        "{}x{} @ {} bpp, {} bytes",
        shot.width,
        shot.height,
        shot.bits_per_pixel,
        shot.data.len()
    );
    if shot.bits_per_pixel != 16 {
        bail!("only 16-bit RGB565 screenshots are supported, got {} bpp", shot.bits_per_pixel);
    }
    let rgb = shot.to_rgb888_with(shot.big_endian != swap_bytes);
    let file = BufWriter::new(File::create(out)?);
    let mut enc = png::Encoder::new(file, shot.width as u32, shot.height as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header()?;
    w.write_image_data(&rgb)?;
    w.finish()?;
    println!("wrote {}", out.display());
    dev.release()?;
    Ok(())
}
