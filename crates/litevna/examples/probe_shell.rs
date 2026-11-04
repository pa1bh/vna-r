//! Scratch probe for the NanoVNA text shell: send each argument as a command, read until the "ch> " prompt.
//! Prefix a command with "raw:" to dump the reply as bytes (for scan_bin / capture).
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("port");
    let mut port = serialport::new(&path, 115_200)
        .timeout(Duration::from_millis(100))
        .open()
        .expect("open");
    let read_until_prompt = |port: &mut Box<dyn serialport::SerialPort>, max: Duration| -> Vec<u8> {
        let mut out = Vec::new();
        let start = Instant::now();
        let mut buf = [0u8; 4096];
        loop {
            if let Ok(n) = port.read(&mut buf) {
                if n > 0 {
                    out.extend_from_slice(&buf[..n]);
                    if out.ends_with(b"ch> ") {
                        break;
                    }
                }
            }
            if start.elapsed() > max {
                break;
            }
        }
        out
    };
    let read_quiet = |port: &mut Box<dyn serialport::SerialPort>, quiet: Duration, max: Duration| -> Vec<u8> {
        let mut out = Vec::new();
        let start = Instant::now();
        let mut last = Instant::now();
        let mut buf = [0u8; 4096];
        loop {
            if let Ok(n) = port.read(&mut buf) {
                if n > 0 {
                    out.extend_from_slice(&buf[..n]);
                    last = Instant::now();
                }
            }
            if last.elapsed() > quiet || start.elapsed() > max {
                break;
            }
        }
        out
    };
    port.write_all(b"\r").unwrap();
    let junk = read_quiet(&mut port, Duration::from_millis(800), Duration::from_secs(20));
    println!("== wake ({} bytes): {:?}", junk.len(), String::from_utf8_lossy(&junk));
    for cmd in args {
        let (raw, cmd) = match cmd.strip_prefix("raw:") {
            Some(c) => (true, c.to_string()),
            None => (false, cmd),
        };
        let (bin, cmd) = match cmd.strip_prefix("bin:") {
            Some(c) => (true, c.to_string()),
            None => (false, cmd),
        };
        let t0 = Instant::now();
        port.write_all(cmd.as_bytes()).unwrap();
        port.write_all(b"\r").unwrap();
        if bin {
            // echo line, then header {mask u16, points u16}, then points * (4 + 8*fields)
            let mut acc = Vec::new();
            let mut buf = [0u8; 4096];
            let echo_len = cmd.len() + 2;
            let mut need = echo_len + 4;
            let mut t_first = None;
            let start = Instant::now();
            loop {
                if acc.len() >= need {
                    if need == echo_len + 4 {
                        let mask = u16::from_le_bytes([acc[echo_len], acc[echo_len + 1]]);
                        let points = u16::from_le_bytes([acc[echo_len + 2], acc[echo_len + 3]]) as usize;
                        let per = 4 * (mask & 1) as usize + 8 * ((mask >> 1) & 1) as usize + 8 * ((mask >> 2) & 1) as usize;
                        println!("header: mask {mask:#06x} points {points} per-point {per} B");
                        need += points * per;
                        if points * per == 0 {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                if let Ok(n) = port.read(&mut buf) {
                    if n > 0 {
                        if t_first.is_none() {
                            t_first = Some(start.elapsed());
                        }
                        acc.extend_from_slice(&buf[..n]);
                    }
                }
                if start.elapsed() > Duration::from_secs(60) {
                    println!("TIMEOUT with {} bytes", acc.len());
                    break;
                }
            }
            let prompt = read_until_prompt(&mut port, Duration::from_secs(5));
            println!(
                "== bin {cmd:?} -> {} bytes, first byte after {:?}, done in {:?}, then {:?}",
                acc.len(),
                t_first,
                t0.elapsed(),
                String::from_utf8_lossy(&prompt)
            );
            if acc.len() > echo_len + 16 {
                let f = u32::from_le_bytes(acc[echo_len + 4..echo_len + 8].try_into().unwrap());
                let re = f32::from_le_bytes(acc[echo_len + 8..echo_len + 12].try_into().unwrap());
                let im = f32::from_le_bytes(acc[echo_len + 12..echo_len + 16].try_into().unwrap());
                println!("first point: {f} Hz  {re} {im}");
            }
            continue;
        }
        if raw {
            let reply = read_quiet(&mut port, Duration::from_millis(2000), Duration::from_secs(120));
            println!("== raw {cmd:?} -> {} bytes in {:?}", reply.len(), t0.elapsed());
            let n = reply.len().min(96);
            println!(
                "head: {}",
                reply[..n].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
            );
            let m = reply.len().saturating_sub(48);
            println!(
                "tail: {}",
                reply[m..].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
            );
            continue;
        }
        let reply = read_until_prompt(&mut port, Duration::from_secs(60));
        let text = String::from_utf8_lossy(&reply);
        println!("== {cmd:?} -> {} bytes in {:?}", reply.len(), t0.elapsed());
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= 16 {
            println!("{text}");
        } else {
            for l in &lines[..6] {
                println!("{l}");
            }
            println!("... ({} lines) ...", lines.len());
            for l in &lines[lines.len() - 4..] {
                println!("{l}");
            }
        }
    }
}
