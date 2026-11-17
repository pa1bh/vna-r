//! What travels between a server and its clients: one JSON text message per
//! WebSocket frame.
//!
//! The server owns the device. It greets every client with a [`Hello`],
//! brings a client that joins late up to date by sending it ordinary events
//! (the last [`Event::Connected`], `LinkLost` or `Disconnected`, then
//! [`Event::Continuous`] with whether the device sweeps, then the last
//! display [`Event::SweepDone`]), and from then on forwards what its worker
//! reports. A client sends [`ClientMsg`]s: the commands of [`Cmd`] that make
//! sense remotely.
//!
//! What comes from the network is checked before it is handed on: a sweep's
//! lists have one length, its segments add up to it, a screenshot's pixels
//! fill it. The GUI assumes all that of a local device too.
//!
//! JSON, so that a session can be read in the browser's developer tools. Two
//! things are not written the obvious way, because of their size: the raw
//! FIFO records of a sweep go as the bytes the device sent, and a screenshot's
//! pixels as base64 (see `sweep` and `screenshot` below). Neither the GUI nor the
//! server sees those forms: they deal in [`Event`] and [`Cmd`].
//!
//! Compatibility: a field added later must have a default on the reading
//! side, and serde ignores fields it does not know, so additions need no new
//! version. Anything else (a field removed or changed in meaning, a new
//! command a server must not ignore) raises [`PROTOCOL_VERSION`]. A message
//! that does not decode is an error for that message, not for the connection:
//! a server of the same version may know an event this client does not.

use std::fmt;

use base64::engine::general_purpose::STANDARD as BASE64;
use litevna::SweepConfig;
use serde::{Deserialize, Serialize};

use crate::{Cmd, Event, Purpose};

/// Raised for every change an older peer would misread; see the module documentation.
pub const PROTOCOL_VERSION: u32 = 1;

/// The most segments in one sweep, and the most passes averaged into one,
/// that a server takes from a client. Well above what the GUI asks for.
pub const MAX_SEGMENTS: usize = 64;
pub const MAX_REPEAT: usize = 1024;

/// The first message of a server on every connection.
///
/// Its JSON form starts `{"hello":{"protocol":N` in every version, so that
/// a client of any version can read at least that much and say which side is
/// the newer one. Do not rename `hello` or `protocol`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// [`PROTOCOL_VERSION`] of the server.
    pub protocol: u32,
    /// The server's program version, e.g. `0.4.0`.
    pub server: String,
    /// What the server calls itself (its host name, or what its owner set),
    /// for the port list and the hover of the status line.
    pub name: String,
}

impl Hello {
    /// A hello for this protocol version.
    pub fn new(server: impl Into<String>, name: impl Into<String>) -> Self {
        Hello {
            protocol: PROTOCOL_VERSION,
            server: server.into(),
            name: name.into(),
        }
    }
}

/// A message from a server to a client.
// Made, sent or taken apart at once, never kept in numbers: boxing the event
// would only cost an allocation per message.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerMsg {
    Hello(Hello),
    Event(Event),
}

/// A message from a client to a server: [`Cmd`] without `Connect` and
/// `Disconnect`. On a client those two open and close the WebSocket; the
/// server connects to its one device by itself and keeps it for all clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientMsg {
    /// See [`Cmd::Sweep`].
    Sweep {
        cfgs: Vec<SweepConfig>,
        purpose: Purpose,
        continuous: bool,
        repeat: usize,
    },
    Stop,
    SetAverage(u8),
    Battery,
    Screenshot,
}

impl From<ClientMsg> for Cmd {
    fn from(m: ClientMsg) -> Cmd {
        match m {
            ClientMsg::Sweep {
                cfgs,
                purpose,
                continuous,
                repeat,
            } => Cmd::Sweep {
                cfgs,
                purpose,
                continuous,
                repeat,
            },
            ClientMsg::Stop => Cmd::Stop,
            ClientMsg::SetAverage(a) => Cmd::SetAverage(a),
            ClientMsg::Battery => Cmd::Battery,
            ClientMsg::Screenshot => Cmd::Screenshot,
        }
    }
}

/// A command that does not travel (`Connect`, `Disconnect`) comes back as the error.
impl TryFrom<Cmd> for ClientMsg {
    type Error = Cmd;

    fn try_from(c: Cmd) -> Result<ClientMsg, Cmd> {
        Ok(match c {
            Cmd::Sweep {
                cfgs,
                purpose,
                continuous,
                repeat,
            } => ClientMsg::Sweep {
                cfgs,
                purpose,
                continuous,
                repeat,
            },
            Cmd::Stop => ClientMsg::Stop,
            Cmd::SetAverage(a) => ClientMsg::SetAverage(a),
            Cmd::Battery => ClientMsg::Battery,
            Cmd::Screenshot => ClientMsg::Screenshot,
            Cmd::Connect(_) | Cmd::Disconnect => return Err(c),
        })
    }
}

/// A message that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not JSON, or not a message of this protocol: an unknown kind of
    /// message, a missing or ill-typed field.
    Malformed(String),
    /// The server greeted with another protocol version. Only a hello
    /// gives this; `theirs` higher than `ours` means the server is the newer one.
    Version { ours: u32, theirs: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed(msg) => write!(f, "unreadable message: {msg}"),
            Error::Version { ours, theirs } if theirs > ours => write!(
                f,
                "the server is newer than this program (protocol {theirs}, this program speaks {ours}): \
                 update the program or use the page the server serves"
            ),
            Error::Version { ours, theirs } => write!(
                f,
                "the server is older than this program (protocol {theirs}, this program speaks {ours}): \
                 update the server"
            ),
        }
    }
}

impl std::error::Error for Error {}

fn malformed(e: serde_json::Error) -> Error {
    Error::Malformed(e.to_string())
}

impl ServerMsg {
    pub fn encode(&self) -> String {
        // Cannot fail: no maps with non-string keys, and the numbers JSON
        // has no words for are written as strings (see `sweep`).
        serde_json::to_string(self).expect("a server message always encodes")
    }

    /// Read a message; a hello of another protocol version is [`Error::Version`].
    pub fn decode(text: &str) -> Result<ServerMsg, Error> {
        match serde_json::from_str::<ServerMsg>(text) {
            Ok(ServerMsg::Hello(h)) if h.protocol != PROTOCOL_VERSION => Err(Error::Version {
                ours: PROTOCOL_VERSION,
                theirs: h.protocol,
            }),
            Ok(m) => check(&m).map(|()| m),
            Err(e) => {
                // A hello whose other fields changed: its version is still readable.
                #[derive(Deserialize)]
                struct Probe {
                    hello: Version,
                }
                #[derive(Deserialize)]
                struct Version {
                    protocol: u32,
                }
                match serde_json::from_str::<Probe>(text) {
                    Ok(p) if p.hello.protocol != PROTOCOL_VERSION => Err(Error::Version {
                        ours: PROTOCOL_VERSION,
                        theirs: p.hello.protocol,
                    }),
                    _ => Err(malformed(e)),
                }
            }
        }
    }
}

/// Across the fields of an event: the segments of a sweep add up to its
/// points (the GUI slices the sweep by them). Within a sweep or a screenshot
/// the serde helpers below check.
fn check(m: &ServerMsg) -> Result<(), Error> {
    if let ServerMsg::Event(Event::SweepDone { cfgs, result, .. }) = m {
        let planned: usize = cfgs.iter().map(|c| c.points as usize).sum();
        if planned != result.freqs_hz.len() || cfgs.iter().any(|c| c.points == 0) {
            return Err(Error::Malformed(format!(
                "a sweep of {} points over segments of {planned}",
                result.freqs_hz.len()
            )));
        }
    }
    Ok(())
}

impl ClientMsg {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("a client message always encodes")
    }

    pub fn decode(text: &str) -> Result<ClientMsg, Error> {
        serde_json::from_str(text).map_err(malformed)
    }

    /// What the worker takes for granted about a command, and the GUI
    /// guarantees: one from the network is checked before it gets there.
    pub fn check(&self) -> Result<(), String> {
        let ClientMsg::Sweep { cfgs, repeat, .. } = self else {
            return Ok(());
        };
        if cfgs.is_empty() {
            return Err("a sweep without segments".into());
        }
        if cfgs.len() > MAX_SEGMENTS {
            return Err(format!("a sweep of {} segments, at most {MAX_SEGMENTS}", cfgs.len()));
        }
        if *repeat > MAX_REPEAT {
            return Err(format!("a sweep repeated {repeat} times, at most {MAX_REPEAT}"));
        }
        cfgs.iter().try_for_each(|c| c.validate().map_err(|e| e.to_string()))
    }
}

/// [`SweepResult`](litevna::SweepResult) on the wire: the sweep without its
/// raw FIFO records, plus the records as the 32-byte blocks the device sent,
/// base64, and the indexes of the points that never arrived.
///
/// The GUI only asks the records which points are missing (a calibration
/// standard with a gap is refused), but that question needs them in place,
/// and records made up on the client would be a lie in waiting. The bytes
/// are exact and cost a third of what the same records cost as JSON objects.
pub(crate) mod sweep {
    use litevna::protocol::{FifoPoint, FIFO_POINT_SIZE};
    use litevna::SweepResult;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::BASE64;
    use base64::Engine as _;

    #[derive(Serialize)]
    struct Out<'a> {
        sweep: &'a SweepResult,
        #[serde(skip_serializing_if = "String::is_empty")]
        records: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        missing: Vec<usize>,
    }

    #[derive(Deserialize)]
    struct In {
        sweep: SweepResult,
        #[serde(default)]
        records: String,
        #[serde(default)]
        missing: Vec<usize>,
    }

    pub(crate) fn serialize<S: Serializer>(r: &SweepResult, s: S) -> Result<S::Ok, S::Error> {
        // Spelled out, so that a new field of SweepResult does not compile
        // until it has been thought about here. The copy without the records
        // costs far less than writing the numbers as text.
        let SweepResult {
            freqs_hz,
            s11,
            s21,
            points,
            checksum_ok,
            checksum_bad,
            duplicates,
            elapsed,
        } = r;
        let bare = SweepResult {
            freqs_hz: freqs_hz.clone(),
            s11: s11.clone(),
            s21: s21.clone(),
            points: Vec::new(),
            checksum_ok: *checksum_ok,
            checksum_bad: *checksum_bad,
            duplicates: *duplicates,
            elapsed: *elapsed,
        };
        let mut bytes = Vec::with_capacity(points.len() * FIFO_POINT_SIZE);
        let mut missing = Vec::new();
        for (i, p) in points.iter().enumerate() {
            match p {
                Some(p) => bytes.extend_from_slice(&p.to_bytes()),
                None => missing.push(i),
            }
        }
        Out {
            sweep: &bare,
            records: BASE64.encode(&bytes),
            missing,
        }
        .serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SweepResult, D::Error> {
        let In {
            mut sweep,
            records,
            missing,
        } = In::deserialize(d)?;
        if records.is_empty() && missing.is_empty() {
            check(&sweep).map_err(D::Error::custom)?;
            return Ok(sweep);
        }
        if !sweep.points.is_empty() {
            return Err(D::Error::custom("the FIFO records are given twice"));
        }
        let bytes = BASE64.decode(&records).map_err(D::Error::custom)?;
        let (records, rest) = bytes.as_chunks::<FIFO_POINT_SIZE>();
        if !rest.is_empty() {
            return Err(D::Error::custom(format!(
                "FIFO records: {} bytes is not a whole number of {FIFO_POINT_SIZE}-byte records",
                bytes.len()
            )));
        }
        let len = records.len() + missing.len();
        let mut records = records.iter();
        let mut missing = missing.into_iter().peekable();
        let mut points = Vec::with_capacity(len);
        for i in 0..len {
            if missing.peek() == Some(&i) {
                missing.next();
                points.push(None);
            } else {
                let Some(r) = records.next() else {
                    return Err(D::Error::custom("missing points not in ascending order"));
                };
                points.push(Some(FifoPoint::parse(r).map_err(D::Error::custom)?));
            }
        }
        if missing.next().is_some() {
            return Err(D::Error::custom("missing points not in ascending order"));
        }
        sweep.points = points;
        check(&sweep).map_err(D::Error::custom)?;
        Ok(sweep)
    }

    /// What the GUI takes for granted about a sweep, and a local device
    /// guarantees: a sweep from the network is checked before it gets there.
    fn check(s: &SweepResult) -> Result<(), String> {
        let n = s.freqs_hz.len();
        if n == 0 {
            return Err("a sweep without points".into());
        }
        if s.s11.len() != n || s.s21.len() != n {
            return Err(format!(
                "a sweep of {n} frequencies with {} S11 and {} S21 values",
                s.s11.len(),
                s.s21.len()
            ));
        }
        if !s.points.is_empty() && s.points.len() != n {
            return Err(format!("a sweep of {n} frequencies with {} FIFO records", s.points.len()));
        }
        Ok(())
    }
}

/// [`Screenshot`](litevna::Screenshot) on the wire: the pixels as base64
/// instead of a list of numbers, a quarter of the size.
pub(crate) mod screenshot {
    use litevna::Screenshot;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::BASE64;
    use base64::Engine as _;

    #[derive(Serialize, Deserialize)]
    struct Wire {
        width: u16,
        height: u16,
        bits_per_pixel: u8,
        big_endian: bool,
        data: String,
    }

    pub(crate) fn serialize<S: Serializer>(shot: &Screenshot, s: S) -> Result<S::Ok, S::Error> {
        let Screenshot {
            width,
            height,
            bits_per_pixel,
            big_endian,
            data,
        } = shot;
        Wire {
            width: *width,
            height: *height,
            bits_per_pixel: *bits_per_pixel,
            big_endian: *big_endian,
            data: BASE64.encode(data),
        }
        .serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Screenshot, D::Error> {
        let w = Wire::deserialize(d)?;
        let data = BASE64.decode(&w.data).map_err(D::Error::custom)?;
        // What the drivers guarantee and the GUI's conversion to an image
        // assumes (it panics on a wrong size): RGB565, width × height pixels.
        let expected = w.width as usize * w.height as usize * 2;
        if w.width == 0 || w.height == 0 || w.bits_per_pixel != 16 || data.len() != expected {
            return Err(D::Error::custom(format!(
                "a screenshot of {}×{} at {} bits per pixel with {} bytes",
                w.width,
                w.height,
                w.bits_per_pixel,
                data.len()
            )));
        }
        Ok(Screenshot {
            width: w.width,
            height: w.height,
            bits_per_pixel: w.bits_per_pixel,
            big_endian: w.big_endian,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litevna::calibration::Standard;
    use litevna::discovery::Kind;
    use litevna::protocol::{fifo_checksum, FifoPoint, FIFO_POINT_SIZE};
    use litevna::shell::ShellInfo;
    use litevna::{DeviceInfo, Limits, Screenshot, SweepResult, VnaInfo};
    use num_complex::Complex64;
    use std::time::Duration;

    /// What a LiteVNA sweep looks like: values of the size the device
    /// reports, full-precision ratios, one record per point.
    fn lite_sweep(n: u16) -> SweepResult {
        let cfg = SweepConfig::linear(50_000, 6_300_000_000, n).unwrap();
        let freqs_hz: Vec<u64> = (0..n as u64).map(|i| cfg.start_hz + i * cfg.step_hz).collect();
        let points: Vec<Option<FifoPoint>> = (0..n)
            .map(|i| {
                let x = i as f64 * 0.731;
                let mut r = [0u8; FIFO_POINT_SIZE];
                let words = [
                    (31_000_000.0 * x.cos()) as i32,
                    (31_000_000.0 * x.sin()) as i32,
                    (-9_876_543.0 * (1.3 * x).sin()) as i32,
                    (12_345_678.0 * (0.7 * x).cos()) as i32,
                    (-812_345.0 * (2.1 * x).cos()) as i32,
                    (1_234_567.0 * (1.7 * x).sin()) as i32,
                ];
                for (k, w) in words.iter().enumerate() {
                    r[4 * k..4 * k + 4].copy_from_slice(&w.to_le_bytes());
                }
                r[0x18..0x1A].copy_from_slice(&i.to_le_bytes());
                r[0x1F] = fifo_checksum(&r);
                Some(FifoPoint::parse(&r).unwrap())
            })
            .collect();
        let s11 = points.iter().map(|p| p.unwrap().s11()).collect();
        let s21 = points.iter().map(|p| p.unwrap().s21()).collect();
        SweepResult {
            freqs_hz,
            s11,
            s21,
            points,
            checksum_ok: n as usize,
            checksum_bad: 0,
            duplicates: 0,
            elapsed: Duration::from_millis(1834),
        }
    }

    fn shell_sweep(n: u16) -> SweepResult {
        let mut s = lite_sweep(n);
        s.points.clear();
        s.checksum_ok = 0;
        s
    }

    fn screenshot() -> Screenshot {
        Screenshot {
            width: 480,
            height: 320,
            bits_per_pixel: 16,
            big_endian: true,
            data: (0..480 * 320 * 2)
                .map(|i: u32| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
                .collect(),
        }
    }

    fn lite_info() -> VnaInfo {
        VnaInfo {
            kind: Kind::LiteVna,
            model: "LiteVNA".into(),
            firmware: "2.2".into(),
            detail: "hardware 2".into(),
            limits: Limits {
                min_hz: 50_000,
                max_hz: 6_300_000_000,
                max_points_per_scan: None,
                has_device_average: true,
            },
            lite: Some(DeviceInfo {
                variant: 2,
                protocol_version: 1,
                hardware_revision: 2,
                fw_major: 2,
                fw_minor: 2,
            }),
            shell: None,
        }
    }

    fn shell_info() -> VnaInfo {
        VnaInfo {
            kind: Kind::NanoVnaShell,
            model: "NanoVNA-F 4.3".into(),
            firmware: "Firmware 1.0.4 by BH5HNU".into(),
            detail: "Build time: Jan 1 2024".into(),
            limits: Limits {
                min_hz: 1_000_000,
                max_hz: 1_500_000_000,
                max_points_per_scan: Some(301),
                has_device_average: false,
            },
            lite: None,
            shell: Some(ShellInfo {
                model: "NanoVNA-F 4.3".into(),
                firmware: "Firmware 1.0.4 by BH5HNU".into(),
                build: "Jan 1 2024".into(),
                raw: "Model: NanoVNA-F 4.3\r\nVersion: ...\r\n".into(),
            }),
        }
    }

    fn grid(n: u16) -> Vec<SweepConfig> {
        vec![SweepConfig::linear(1_000_000, 30_000_000, n).unwrap()]
    }

    fn cfgs() -> Vec<SweepConfig> {
        vec![
            SweepConfig::linear(1_000_000, 30_000_000, 101).unwrap(),
            SweepConfig::linear(30_000_000, 450_000_000, 51).unwrap(),
        ]
    }

    /// Event has no PartialEq (SweepResult has none); its Debug form shows
    /// every field, NaN included, so equal Debug output is equal content.
    fn round_trip(e: Event) -> Event {
        let text = ServerMsg::Event(e).encode();
        match ServerMsg::decode(&text) {
            Ok(ServerMsg::Event(e)) => e,
            other => panic!("{other:?} from {text}"),
        }
    }

    fn assert_round_trip(e: Event) {
        let before = format!("{e:?}");
        assert_eq!(format!("{:?}", round_trip(e)), before);
    }

    #[test]
    fn every_event_survives_the_wire() {
        let events = vec![
            Event::Connected {
                port: "/dev/ttyACM0".into(),
                info: lite_info(),
                battery_mv: Some(4012),
                via: None,
            },
            Event::Connected {
                port: "/dev/ttyACM1".into(),
                info: shell_info(),
                battery_mv: None,
                via: Some(crate::Via {
                    server: "shack".into(),
                    version: "0.4.0".into(),
                    device_port: "/dev/ttyACM1".into(),
                }),
            },
            Event::Disconnected,
            Event::LinkLost {
                port: "/dev/ttyACM0".into(),
                msg: "sweep: I/O error: Broken pipe".into(),
                what: crate::Loss::Device,
            },
            Event::LinkLost {
                port: "ws://vna.local:8080/ws".into(),
                msg: "read: Connection reset by peer".into(),
                what: crate::Loss::Server,
            },
            Event::SweepDone {
                cfgs: cfgs(),
                result: lite_sweep(152),
                purpose: Purpose::Display,
                repeat: 1,
            },
            Event::SweepDone {
                cfgs: cfgs(),
                result: shell_sweep(152),
                purpose: Purpose::Standard(Standard::LineShort),
                repeat: 4,
            },
            Event::Progress {
                done: 255,
                total: 1001,
                purpose: Purpose::Display,
            },
            Event::Progress {
                done: 7,
                total: 101,
                purpose: Purpose::Standard(Standard::Short),
            },
            Event::Cancelled {
                purpose: Purpose::Standard(Standard::Through),
            },
            Event::Screenshot(screenshot()),
            Event::Battery(3987),
            Event::Error {
                msg: "screenshot: timed out".into(),
                stops_sweep: false,
            },
            Event::Continuous(true),
            Event::Continuous(false),
        ];
        for e in events {
            assert_round_trip(e);
        }
    }

    #[test]
    fn missing_points_and_bad_records_come_back_where_they_were() {
        let mut s = lite_sweep(20);
        s.points[0] = None;
        s.points[7] = None;
        s.points[19] = None;
        let mut bad = s.points[3].unwrap().to_bytes();
        bad[0x1F] ^= 0xFF;
        s.points[3] = Some(FifoPoint::parse(&bad).unwrap());
        let Event::SweepDone { result, .. } = round_trip(Event::SweepDone {
            cfgs: grid(20),
            result: s.clone(),
            purpose: Purpose::Display,
            repeat: 1,
        }) else {
            panic!()
        };
        assert_eq!(result.points, s.points);
        assert_eq!(result.missing_points(), 3);
        assert!(!result.points[3].unwrap().checksum_ok);
        // a sweep that has only gaps
        let mut gaps = lite_sweep(3);
        gaps.points = vec![None; 3];
        let text = ServerMsg::Event(Event::SweepDone {
            cfgs: grid(3),
            result: gaps,
            purpose: Purpose::Display,
            repeat: 1,
        })
        .encode();
        assert!(text.contains(r#""missing":[0,1,2]"#) && !text.contains("records"), "{text}");
        let Ok(ServerMsg::Event(Event::SweepDone { result, .. })) = ServerMsg::decode(&text) else {
            panic!("{text}")
        };
        assert_eq!(result.points, vec![None; 3]);
    }

    #[test]
    fn a_sweep_with_nan_survives_the_wire() {
        let mut s = lite_sweep(5);
        // a record whose reference wave is zero
        s.s11[1] = Complex64::new(f64::NAN, f64::NAN);
        s.s21[2] = Complex64::new(f64::INFINITY, f64::NEG_INFINITY);
        let Event::SweepDone { result, .. } = round_trip(Event::SweepDone {
            cfgs: grid(5),
            result: s,
            purpose: Purpose::Display,
            repeat: 1,
        }) else {
            panic!()
        };
        assert!(result.s11[1].re.is_nan() && result.s11[1].im.is_nan());
        assert_eq!(result.s21[2], Complex64::new(f64::INFINITY, f64::NEG_INFINITY));
        assert!(result.s11[0].is_finite());
    }

    #[test]
    fn every_command_that_travels_survives_the_wire() {
        let msgs = [
            ClientMsg::Sweep {
                cfgs: cfgs(),
                purpose: Purpose::Display,
                continuous: true,
                repeat: 1,
            },
            ClientMsg::Sweep {
                cfgs: cfgs(),
                purpose: Purpose::Standard(Standard::Open),
                continuous: false,
                repeat: 8,
            },
            ClientMsg::Stop,
            ClientMsg::SetAverage(4),
            ClientMsg::Battery,
            ClientMsg::Screenshot,
        ];
        for m in msgs {
            let text = m.encode();
            assert_eq!(ClientMsg::decode(&text), Ok(m.clone()), "{text}");
            // and through Cmd and back
            assert_eq!(ClientMsg::try_from(Cmd::from(m.clone())).ok(), Some(m));
        }
        assert!(matches!(
            ClientMsg::try_from(Cmd::Connect("ws://vna.local:8080".into())),
            Err(Cmd::Connect(p)) if p == "ws://vna.local:8080"
        ));
        assert!(matches!(ClientMsg::try_from(Cmd::Disconnect), Err(Cmd::Disconnect)));
    }

    #[test]
    fn the_hello_survives_the_wire_and_keeps_its_shape() {
        let hello = Hello::new("0.4.0", "vna.local");
        let text = ServerMsg::Hello(hello.clone()).encode();
        assert!(
            text.starts_with(&format!(r#"{{"hello":{{"protocol":{PROTOCOL_VERSION},"#)),
            "{text}"
        );
        match ServerMsg::decode(&text) {
            Ok(ServerMsg::Hello(h)) => assert_eq!(h, hello),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn another_protocol_version_is_told_apart_from_garbage() {
        let newer = r#"{"hello":{"protocol":7,"server":"9.0.0","name":"pi","extra":[1,2]}}"#;
        let e = ServerMsg::decode(newer).unwrap_err();
        assert_eq!(
            e,
            Error::Version {
                ours: PROTOCOL_VERSION,
                theirs: 7
            }
        );
        assert!(e.to_string().contains("server is newer"), "{e}");
        // a hello whose other fields changed shape still gives its version
        let reshaped = r#"{"hello":{"protocol":0,"server":{"v":[0,1]}}}"#;
        let e = ServerMsg::decode(reshaped).unwrap_err();
        assert_eq!(
            e,
            Error::Version {
                ours: PROTOCOL_VERSION,
                theirs: 0
            }
        );
        assert!(e.to_string().contains("server is older"), "{e}");
        // our own version with a broken body is just malformed
        let broken = format!(r#"{{"hello":{{"protocol":{PROTOCOL_VERSION}}}}}"#);
        assert!(matches!(ServerMsg::decode(&broken), Err(Error::Malformed(_))));
    }

    #[test]
    fn unknown_or_broken_messages_are_errors_not_panics() {
        let server = [
            "",
            "not json",
            "{}",
            "[]",
            "null",
            r#"{"event":"warp_drive"}"#,
            r#"{"event":{"battery":"full"}}"#,
            r#"{"goodbye":{}}"#,
            r#"{"event":{"screenshot":{"width":1,"height":1,"bits_per_pixel":16,"big_endian":true,"data":"@@"}}}"#,
            r#"{"event":{"sweep_done":{"cfgs":[],"result":{"sweep":{"freqs_hz":[],"s11":[],"s21":[],"checksum_ok":0,"checksum_bad":0,"duplicates":0,"elapsed":{"secs":0,"nanos":0}},"records":"AAAA"},"purpose":"display","repeat":1}}}"#,
            r#"{"event":{"sweep_done":{"cfgs":[],"result":{"sweep":{"freqs_hz":[],"s11":[],"s21":[],"checksum_ok":0,"checksum_bad":0,"duplicates":0,"elapsed":{"secs":0,"nanos":0}},"missing":[5,2]},"purpose":"display","repeat":1}}}"#,
        ];
        for text in server {
            assert!(matches!(ServerMsg::decode(text), Err(Error::Malformed(_))), "{text}");
        }
        for text in [
            "",
            "{}",
            r#""connect""#,
            r#"{"connect":"/dev/ttyACM0"}"#,
            r#""disconnect""#,
            r#"{"set_average":300}"#,
        ] {
            assert!(matches!(ClientMsg::decode(text), Err(Error::Malformed(_))), "{text}");
        }
    }

    /// The sizes in the report on the wire format; run with `--nocapture`
    /// to see them. The bounds are there to notice a format that grew by accident.
    #[test]
    fn sizes_of_the_big_messages() {
        let sweep = |result| {
            ServerMsg::Event(Event::SweepDone {
                cfgs: vec![SweepConfig::linear(50_000, 6_300_000_000, 1001).unwrap()],
                result,
                purpose: Purpose::Display,
                repeat: 1,
            })
            .encode()
            .len()
        };
        let lite = sweep(lite_sweep(1001));
        let shell = sweep(shell_sweep(1001));
        let shot = ServerMsg::Event(Event::Screenshot(screenshot())).encode().len();
        let records_as_json = serde_json::to_string(&lite_sweep(1001).points).unwrap().len();
        let pixels_as_json = serde_json::to_string(&screenshot().data).unwrap().len();
        println!("1001-point LiteVNA sweep: {lite} bytes; shell sweep: {shell} bytes; screenshot 480x320: {shot} bytes");
        println!("as JSON numbers the FIFO records alone would be {records_as_json} bytes, the pixels {pixels_as_json}");
        assert!(lite < 150_000 && shell < 110_000 && shot < 420_000);
    }

    #[test]
    fn what_the_gui_takes_for_granted_is_checked_on_arrival() {
        let sweep_done = |cfgs, result| {
            ServerMsg::Event(Event::SweepDone {
                cfgs,
                result,
                purpose: Purpose::Display,
                repeat: 1,
            })
            .encode()
        };
        let rejected = |text: String| matches!(ServerMsg::decode(&text), Err(Error::Malformed(_)));
        // S11 shorter than the grid
        let mut s = shell_sweep(10);
        s.s11.pop();
        assert!(rejected(sweep_done(grid(10), s)));
        // records for some of the points only
        let mut s = lite_sweep(10);
        s.points.truncate(9);
        assert!(rejected(sweep_done(grid(10), s)));
        // segments that do not add up to the sweep, and an empty sweep
        assert!(rejected(sweep_done(grid(11), lite_sweep(10))));
        let mut empty = shell_sweep(2);
        (empty.freqs_hz, empty.s11, empty.s21) = (vec![], vec![], vec![]);
        assert!(rejected(sweep_done(vec![], empty)));
        // a screenshot whose pixels do not fill it, or are not RGB565
        let mut shot = screenshot();
        shot.data.pop();
        assert!(rejected(ServerMsg::Event(Event::Screenshot(shot)).encode()));
        let mut shot = screenshot();
        shot.bits_per_pixel = 8;
        shot.data.truncate(480 * 320);
        assert!(rejected(ServerMsg::Event(Event::Screenshot(shot)).encode()));
        // and what is right still passes
        assert!(ServerMsg::decode(&sweep_done(grid(10), lite_sweep(10))).is_ok());
    }
}
