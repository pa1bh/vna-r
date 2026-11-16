//! The real worker with the simulated device: what a client of `vna-link`
//! sees, without hardware. Every test reads the event stream; the one wait
//! in here is a timeout that only fires when something is broken.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use litevna::calibration::Standard;
use litevna::SweepConfig;
use vna_link::{spawn, Cmd, Event, Purpose, WorkerHandle, SIMULATOR};

fn connect(rate: &str) -> WorkerHandle {
    let w = spawn(Arc::new(|| {}));
    let port = format!("{SIMULATOR}{rate}");
    w.tx.send(Cmd::Connect(port.clone())).unwrap();
    match next(&w.rx) {
        Event::Connected { port: p, .. } => assert_eq!(p, port),
        other => panic!("{other:?}"),
    }
    w
}

fn next(rx: &Receiver<Event>) -> Event {
    rx.recv_timeout(Duration::from_secs(10)).expect("an event within 10 s")
}

/// Events up to and including the first one `stop` accepts.
fn until(rx: &Receiver<Event>, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let e = next(rx);
        let done = stop(&e);
        seen.push(e);
        if done {
            return seen;
        }
    }
}

fn sweep(cfgs: Vec<SweepConfig>, purpose: Purpose, continuous: bool, repeat: usize) -> Cmd {
    Cmd::Sweep {
        cfgs,
        purpose,
        continuous,
        repeat,
    }
}

fn lin(start: u64, stop: u64, points: u16) -> SweepConfig {
    SweepConfig::linear(start, stop, points).unwrap()
}

fn is_done(e: &Event) -> bool {
    matches!(e, Event::SweepDone { .. })
}

#[test]
fn connect_identity_battery_and_screenshot() {
    let w = spawn(Arc::new(|| {}));
    w.tx.send(Cmd::Connect(SIMULATOR.into())).unwrap();
    let Event::Connected { info, battery_mv, .. } = next(&w.rx) else {
        panic!()
    };
    assert_eq!(info.model, "vna-r simulator");
    assert!(info.summary().contains("no hardware"), "{}", info.summary());
    assert_eq!(battery_mv, Some(3950));
    assert!(!info.limits.has_device_average);
    w.tx.send(Cmd::Battery).unwrap();
    assert!(matches!(next(&w.rx), Event::Battery(3950)));
    w.tx.send(Cmd::Screenshot).unwrap();
    let Event::Screenshot(shot) = next(&w.rx) else { panic!() };
    assert_eq!((shot.width, shot.height, shot.data.len()), (480, 320, 480 * 320 * 2));
    // averaging on the device: the simulator has none, and says nothing
    w.tx.send(Cmd::SetAverage(4)).unwrap();
    w.tx.send(Cmd::Disconnect).unwrap();
    assert!(matches!(next(&w.rx), Event::Disconnected));
}

#[test]
fn a_single_sweep_comes_back_on_the_grid_asked_for() {
    let w = connect("0");
    let cfgs = vec![lin(1_000_000, 30_000_000, 101), lin(30_000_000, 450_000_000, 51)];
    w.tx.send(sweep(cfgs.clone(), Purpose::Display, false, 3)).unwrap();
    let events = until(&w.rx, is_done);
    let progress: Vec<(usize, usize)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Progress { done, total } => Some((*done, *total)),
            _ => None,
        })
        .collect();
    assert!(progress.windows(2).all(|p| p[0].0 < p[1].0), "{progress:?}");
    assert_eq!(progress.last(), Some(&(152 * 3, 152 * 3)));
    let Some(Event::SweepDone {
        cfgs: got,
        result,
        purpose,
        repeat,
    }) = events.last()
    else {
        panic!()
    };
    assert_eq!((got, *purpose, *repeat), (&cfgs, Purpose::Display, 3));
    let grid: Vec<u64> = cfgs.iter().flat_map(|c| c.frequencies()).collect();
    assert_eq!(result.freqs_hz, grid);
    assert_eq!((result.s11.len(), result.s21.len(), result.points.len()), (152, 152, 152));
    assert_eq!(result.missing_points(), 0);
    assert!(result.s11.iter().all(|g| g.norm() < 1.05));
}

#[test]
fn continuous_sweeping_goes_on_until_stop() {
    let w = connect("0");
    w.tx.send(sweep(vec![lin(1_000_000, 30_000_000, 201)], Purpose::Display, true, 1))
        .unwrap();
    let mut sweeps = Vec::new();
    while sweeps.len() < 3 {
        if let Event::SweepDone { result, .. } = next(&w.rx) {
            sweeps.push(result);
        }
    }
    // the noise is new in every sweep, the antenna is the same
    assert_ne!(sweeps[0].s11, sweeps[1].s11);
    assert!((sweeps[0].s11[100] - sweeps[1].s11[100]).norm() < 0.02);
    w.tx.send(Cmd::Stop).unwrap();
    w.tx.send(Cmd::Battery).unwrap();
    // Commands are handled in order: once the battery answers, Stop has been
    // handled, and nothing but the next answer may follow.
    until(&w.rx, |e| matches!(e, Event::Battery(_)));
    w.tx.send(Cmd::Battery).unwrap();
    assert!(matches!(next(&w.rx), Event::Battery(_)));
}

#[test]
fn a_display_sweep_cuts_a_running_one_short() {
    // 4001 points at 5000 points/s: 0.8 s, the first block after 51 ms.
    let w = connect("5000");
    w.tx.send(sweep(vec![lin(1_000_000, 30_000_000, 4001)], Purpose::Display, false, 1))
        .unwrap();
    until(&w.rx, |e| matches!(e, Event::Progress { .. }));
    let short = vec![lin(7_000_000, 7_200_000, 21)];
    w.tx.send(sweep(short.clone(), Purpose::Display, false, 1)).unwrap();
    let events = until(&w.rx, is_done);
    let cancelled = events
        .iter()
        .position(|e| matches!(e, Event::Cancelled { purpose: Purpose::Display }))
        .expect("the long sweep is cancelled");
    assert_eq!(events.iter().filter(|e| is_done(e)).count(), 1);
    let Some(Event::SweepDone { cfgs, result, .. }) = events.last() else {
        panic!()
    };
    assert!(cancelled < events.len() - 1);
    assert_eq!((cfgs, result.freqs_hz.len()), (&short, 21));
}

#[test]
fn a_standard_waits_for_the_running_sweep_and_is_ideal() {
    let w = connect("5000");
    let grid = vec![lin(1_000_000, 30_000_000, 1001)];
    w.tx.send(sweep(grid.clone(), Purpose::Display, false, 1)).unwrap();
    until(&w.rx, |e| matches!(e, Event::Progress { .. }));
    w.tx.send(sweep(grid.clone(), Purpose::Standard(Standard::Open), false, 2)).unwrap();
    let events = until(&w.rx, |e| {
        matches!(
            e,
            Event::SweepDone {
                purpose: Purpose::Standard(_),
                ..
            }
        )
    });
    assert!(!events.iter().any(|e| matches!(e, Event::Cancelled { .. })), "nothing is cancelled");
    let done: Vec<(Purpose, usize)> = events
        .iter()
        .filter_map(|e| match e {
            Event::SweepDone { purpose, result, .. } => Some((*purpose, result.freqs_hz.len())),
            _ => None,
        })
        .collect();
    assert_eq!(done, [(Purpose::Display, 1001), (Purpose::Standard(Standard::Open), 1001)]);
    let Some(Event::SweepDone { result, repeat, .. }) = events.last() else {
        panic!()
    };
    assert_eq!(*repeat, 2);
    assert!(result.s11.iter().all(|g| (g.re - 1.0).abs() < 0.01 && g.im.abs() < 0.01));
    assert!(result.s21.iter().all(|t| t.norm() < 0.002));
}

#[test]
fn every_standard_is_on_the_port_while_it_is_measured() {
    let w = connect("0");
    let grid = vec![lin(10_000_000, 20_000_000, 11)];
    let measure = |purpose| {
        w.tx.send(sweep(grid.clone(), purpose, false, 1)).unwrap();
        match until(&w.rx, is_done).pop() {
            Some(Event::SweepDone { result, .. }) => (result.s11[5], result.s21[5]),
            other => panic!("{other:?}"),
        }
    };
    let near = |a: num_complex::Complex64, re: f64| (a.re - re).abs() < 0.01 && a.im.abs() < 0.01;
    let (s11, s21) = measure(Purpose::Standard(Standard::Short));
    assert!(near(s11, -1.0) && near(s21, 0.0));
    let (s11, _) = measure(Purpose::Standard(Standard::Load));
    assert!(near(s11, 0.0));
    let (s11, s21) = measure(Purpose::Standard(Standard::Through));
    assert!(near(s11, 0.0) && near(s21, 1.0));
    // and back to the antenna and the filter: 15 MHz is in the pass band
    let (s11, s21) = measure(Purpose::Display);
    assert!(s11.norm() > 0.1 && s21.norm() > 0.8, "{s11} {s21}");
}

#[test]
fn without_a_connection_a_sweep_fails_as_it_does_with_hardware() {
    let w = spawn(Arc::new(|| {}));
    w.tx.send(sweep(vec![lin(1_000_000, 30_000_000, 11)], Purpose::Display, false, 1))
        .unwrap();
    match next(&w.rx) {
        Event::Error { msg, stops_sweep } => assert_eq!((msg.as_str(), stops_sweep), ("sweep: not connected", true)),
        other => panic!("{other:?}"),
    }
    w.tx.send(Cmd::Connect("sim:fast".into())).unwrap();
    match next(&w.rx) {
        Event::Error { msg, stops_sweep: true } => assert!(msg.starts_with("connecting to sim:fast: invalid argument"), "{msg}"),
        other => panic!("{other:?}"),
    }
}
