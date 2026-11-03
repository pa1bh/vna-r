//! `vna-cli` — proof of concept for talking to a LiteVNA 67 from Rust.

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use litevna::discovery;
use litevna::freq::{format_hz, parse_hz};
use litevna::protocol::{Channel, DataMode, reg};
use litevna::touchstone::{db, swr, write_csv, write_s2p};
use litevna::{LiteVna, SweepConfig};

#[derive(Parser)]
#[command(name = "vna-cli", version, about = "LiteVNA 67 proof of concept (PA1BH)")]
struct Cli {
    /// Serial port, e.g. /dev/cu.usbmodemXXXX. Auto-detected when omitted.
    #[arg(short, long, global = true)]
    port: Option<String>,
    /// Print every byte sent and received.
    #[arg(long, global = true)]
    trace: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List serial ports and mark the ones that look like a VNA.
    List,
    /// Read identity registers and battery voltage.
    Info,
    /// Read any register by hand: `peek F0`, `peek --size 2 5C`.
    Peek {
        /// Register address in hex (e.g. F0).
        addr: String,
        /// Register width in bytes: 1, 2 or 4.
        #[arg(long, default_value_t = 1)]
        size: u8,
    },
    /// Run one sweep and print / export S11 and S21.
    Sweep {
        /// Start frequency, e.g. 1M, 50k, 1.3G.
        #[arg(long, default_value = "1M")]
        start: String,
        /// Stop frequency.
        #[arg(long, default_value = "30M")]
        stop: String,
        /// Number of points (2..=65535).
        #[arg(long, default_value_t = 101)]
        points: u16,
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
        /// Write a Touchstone .s2p file.
        #[arg(long)]
        s2p: Option<PathBuf>,
        /// Write a CSV file ("-" for stdout).
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Print at most this many table rows (0 = all).
        #[arg(long, default_value_t = 25)]
        rows: usize,
        /// Leave the device in USB data mode afterwards (screen stays frozen).
        #[arg(long)]
        keep_usb_mode: bool,
        /// Do not restore the device's previous average/channel settings afterwards.
        #[arg(long)]
        keep_settings: bool,
    },
    /// Grab the device screen as PNG.
    Screenshot {
        #[arg(long, default_value = "screenshot.png")]
        out: PathBuf,
        /// Interpret RGB565 pixels as little-endian instead of big-endian.
        #[arg(long)]
        little_endian: bool,
    },
    /// Hand the screen back to the on-device UI (rawSamplesMode = 2).
    Release,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = Conn { port: cli.port.clone(), trace: cli.trace };
    match cli.cmd {
        Cmd::List => list(),
        Cmd::Info => {
            let mut dev = open(&conn)?;
            info(&mut dev)
        }
        Cmd::Peek { addr, size } => {
            let mut dev = open(&conn)?;
            peek(&mut dev, &addr, size)
        }
        Cmd::Sweep {
            start,
            stop,
            points,
            average,
            channel,
            calibrated,
            fifo_all,
            s2p,
            csv,
            rows,
            keep_usb_mode,
            keep_settings,
        } => {
            let mut dev = open(&conn)?;
            sweep(
                &mut dev,
                SweepArgs { start, stop, points, average, channel, calibrated, fifo_all, s2p, csv, rows, keep_usb_mode, keep_settings },
            )
        }
        Cmd::Screenshot { out, little_endian } => {
            let mut dev = open(&conn)?;
            screenshot(&mut dev, &out, !little_endian)
        }
        Cmd::Release => {
            let mut dev = open(&conn)?;
            dev.exit_usb_mode()?;
            println!("device UI released");
            Ok(())
        }
    }
}

struct Conn {
    port: Option<String>,
    trace: bool,
}

fn open(conn: &Conn) -> Result<LiteVna> {
    let path = match &conn.port {
        Some(p) => p.clone(),
        None => match discovery::find_vna_port()? {
            Some(p) => p,
            None => bail!(
                "no LiteVNA found (USB {:04X}:{:04X}). Is it switched on and connected? Try `vna-cli list`.",
                discovery::LITEVNA_VID,
                discovery::LITEVNA_PID
            ),
        },
    };
    eprintln!("opening {path}");
    let mut dev = LiteVna::open(&path).with_context(|| format!("opening {path}"))?;
    dev.trace = conn.trace;
    dev.indicate().context("device did not answer INDICATE with '2'")?;
    Ok(dev)
}

fn list() -> Result<()> {
    let ports = discovery::list_ports()?;
    if ports.is_empty() {
        println!("no serial ports found");
        return Ok(());
    }
    for p in ports {
        let mark = if p.looks_like_vna() { "VNA " } else { "    " };
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

fn info(dev: &mut LiteVna) -> Result<()> {
    let i = dev.device_info()?;
    println!("device variant     : 0x{:02X}{}", i.variant, if i.variant == 0x02 { " (NanoVNA V2 / LiteVNA family)" } else { "" });
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

fn peek(dev: &mut LiteVna, addr: &str, size: u8) -> Result<()> {
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

struct SweepArgs {
    start: String,
    stop: String,
    points: u16,
    average: Option<u8>,
    channel: Option<String>,
    calibrated: bool,
    fifo_all: bool,
    s2p: Option<PathBuf>,
    csv: Option<PathBuf>,
    rows: usize,
    keep_usb_mode: bool,
    keep_settings: bool,
}

fn sweep(dev: &mut LiteVna, a: SweepArgs) -> Result<()> {
    let start = parse_hz(&a.start)?;
    let stop = parse_hz(&a.stop)?;
    let mut cfg = SweepConfig::linear(start, stop, a.points)?;
    cfg.calibrated = a.calibrated;

    // These registers persist in the device, so remember what was there.
    let prev_average = match a.average {
        Some(avg) => {
            let prev = dev.average()?;
            dev.set_average(avg)?;
            Some(prev)
        }
        None => None,
    };
    let prev_channel = if a.channel.is_some() { Some(dev.channel()?) } else { None };
    if let Some(ch) = &a.channel {
        let ch = match ch.to_ascii_lowercase().as_str() {
            "s11" => Channel::S11Only,
            "s21" => Channel::S21Only,
            "both" => Channel::Both,
            other => bail!("unknown channel {other:?}, use s11, s21 or both"),
        };
        dev.set_channel(ch)?;
    }

    eprintln!(
        "sweep {} → {} in {} points (step {}){}",
        format_hz(cfg.start_hz),
        format_hz(cfg.stop_hz()),
        cfg.points,
        format_hz(cfg.step_hz),
        if cfg.calibrated { ", device-calibrated data" } else { ", raw data" }
    );
    let result = dev.sweep(&cfg, a.fifo_all)?;
    if !a.keep_settings {
        if let Some(prev) = prev_average {
            dev.write_u8(reg::AVERAGE, prev)?;
        }
        if let Some(prev) = prev_channel {
            dev.write_u8(reg::CHANNEL_SELECT, prev)?;
        }
    }
    if a.keep_usb_mode {
        dev.release_on_drop = false;
    } else {
        dev.exit_usb_mode()?;
    }

    eprintln!(
        "got {} points in {:.0} ms ({:.0} pts/s), missing {}, duplicates (stale FIFO records) {}, checksum ok {} / bad {}",
        result.points.len() - result.missing_points(),
        result.elapsed.as_secs_f64() * 1000.0,
        cfg.points as f64 / result.elapsed.as_secs_f64().max(1e-9),
        result.missing_points(),
        result.duplicates,
        result.checksum_ok,
        result.checksum_bad
    );
    if result.missing_points() > 0 {
        let idx = result.missing_indexes();
        eprintln!("missing indexes: {:?}{}", &idx[..idx.len().min(40)], if idx.len() > 40 { " …" } else { "" });
    }

    let n = result.freqs_hz.len();
    let shown = if a.rows == 0 { n } else { a.rows.min(n) };
    let stride = (n + shown - 1) / shown.max(1);
    println!("{:>14} {:>9} {:>8} {:>7} {:>9} {:>8}", "freq", "S11 dB", "S11 °", "SWR", "S21 dB", "S21 °");
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
    if let Some(p) = result.points.iter().flatten().next() {
        eprintln!(
            "first raw record: fwd0=({},{}) rev0=({},{}) rev1=({},{}) reserved={:02X?} checksum=0x{:02X}",
            p.fwd0_re, p.fwd0_im, p.rev0_re, p.rev0_im, p.rev1_re, p.rev1_im, p.reserved, p.checksum
        );
    }

    if let Some(path) = &a.s2p {
        let f = BufWriter::new(File::create(path).with_context(|| format!("creating {}", path.display()))?);
        let comment = format!("LiteVNA sweep, {} points, {}", cfg.points, if cfg.calibrated { "device calibrated" } else { "uncalibrated raw" });
        write_s2p(f, &result, Some(&comment))?;
        eprintln!("wrote {}", path.display());
    }
    if let Some(path) = &a.csv {
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

fn screenshot(dev: &mut LiteVna, out: &PathBuf, big_endian: bool) -> Result<()> {
    eprintln!("requesting screenshot (register 0x{:02X}) …", reg::SCREENSHOT);
    let shot = dev.screenshot()?;
    eprintln!("{}x{} @ {} bpp, {} bytes", shot.width, shot.height, shot.bits_per_pixel, shot.data.len());
    if shot.bits_per_pixel != 16 {
        bail!("only 16-bit RGB565 screenshots are supported, got {} bpp", shot.bits_per_pixel);
    }
    let rgb = shot.to_rgb888(big_endian);
    let file = BufWriter::new(File::create(out)?);
    let mut enc = png::Encoder::new(file, shot.width as u32, shot.height as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header()?;
    w.write_image_data(&rgb)?;
    w.finish()?;
    println!("wrote {}", out.display());
    // make sure the UI is alive again
    dev.set_data_mode(DataMode::Normal)?;
    Ok(())
}

