//! One connection: read the request, then either answer it with a file of
//! the web client and close, or take the WebSocket upgrade and stay as a
//! client of the hub until one side goes.
//!
//! The port is open to the LAN, so nothing here trusts the peer: the request
//! head is bounded in size and in time, and a client that stops reading or
//! goes silent is let go.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};
use vna_link::wire::{self, ClientMsg};

use crate::hub::{Outbox, ToHub};
use crate::{Shared, WS_PATH};

/// The request head: at most this many bytes, within this time.
const MAX_HEAD: usize = 8 << 10;
const HEAD_WITHIN: Duration = Duration::from_secs(10);
/// A write that does not get through in this time drops the connection.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often a client's thread looks for messages to send: the longest a
/// message waits in its outbox.
const POLL: Duration = Duration::from_millis(25);
/// A client sends commands of a few hundred bytes; nothing bigger is read.
const MAX_CLIENT_MESSAGE: usize = 64 << 10;

/// What the server needs to know of a request.
pub(crate) struct Request {
    pub(crate) head_only: bool,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) accepts_gzip: bool,
    upgrade: bool,
    origin: Option<String>,
}

enum Head {
    Request(Request),
    /// Answer this and close.
    Refuse(u16, &'static str),
}

pub(crate) fn serve(mut stream: TcpStream, shared: &Shared) {
    let Ok(peer) = stream.peer_addr() else { return };
    if stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err() {
        return;
    }
    let (buf, head) = match read_head(&mut stream) {
        Ok(r) => r,
        Err(e) => {
            log::debug!("{peer}: no request: {e}");
            return;
        }
    };
    let req = match head {
        Head::Request(r) => r,
        Head::Refuse(status, why) => {
            log::debug!("{peer}: {status} {why}");
            let _ = crate::web::plain(&mut stream, status, why, false);
            finish(stream);
            return;
        }
    };
    if req.path == WS_PATH {
        if req.upgrade && !req.head_only {
            websocket(
                Replay {
                    head: buf,
                    at: 0,
                    inner: stream,
                },
                peer,
                req.origin,
                shared,
            );
        } else {
            let _ = crate::web::plain(&mut stream, 400, "this address is the WebSocket of vna-server", req.head_only);
            finish(stream);
        }
        return;
    }
    if let Err(e) = shared.web.serve(&mut stream, &req) {
        log::debug!("{peer}: {}: {e}", req.path);
    }
    finish(stream);
}

/// Close after an answer: say we are done, then read away what the peer
/// may still send (the rest of a head that was too large). Closing a socket
/// with unread data makes the system reset the connection, and a reset can
/// throw away the answer before the peer has read it.
fn finish(mut stream: TcpStream) {
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut sink = [0u8; 4096];
    let mut left = 1usize << 20;
    while left > 0 {
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() || stream.set_read_timeout(Some(wait)).is_err() {
            return;
        }
        match stream.read(&mut sink) {
            Ok(0) | Err(_) => return,
            Ok(n) => left = left.saturating_sub(n),
        }
    }
}

/// Read up to the end of the request head. The bytes read are returned
/// too: a WebSocket handshake reads them again.
fn read_head(stream: &mut TcpStream) -> io::Result<(Vec<u8>, Head)> {
    let deadline = Instant::now() + HEAD_WITHIN;
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(ErrorKind::TimedOut, "request head too slow"));
        }
        // A peer that sends one byte now and then must not reset the clock.
        stream.set_read_timeout(Some(left))?;
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "closed before the request was complete"));
        }
        let from = buf.len().saturating_sub(3);
        buf.extend_from_slice(&chunk[..n]);
        if buf[from..].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > MAX_HEAD {
            return Ok((buf, Head::Refuse(431, "request head too large")));
        }
    }
    let head = parse(&buf);
    Ok((buf, head))
}

fn parse(buf: &[u8]) -> Head {
    let mut headers = [httparse::EMPTY_HEADER; 48];
    let mut r = httparse::Request::new(&mut headers);
    match r.parse(buf) {
        Ok(httparse::Status::Complete(_)) => {}
        Err(httparse::Error::TooManyHeaders) => return Head::Refuse(431, "request head too large"),
        // The head was read up to its blank line, and its size checked, by
        // `read_head`: one that still is not complete began with blank lines.
        Ok(httparse::Status::Partial) | Err(_) => return Head::Refuse(400, "not an HTTP request"),
    }
    let head_only = match r.method {
        Some("GET") => false,
        Some("HEAD") => true,
        _ => return Head::Refuse(405, "only GET and HEAD"),
    };
    let target = r.path.unwrap_or("");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let header = |name: &str| {
        r.headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| String::from_utf8_lossy(h.value).into_owned())
            .collect::<Vec<_>>()
            .join(", ")
    };
    // What tungstenite's handshake insists on; checked here so that a request
    // without it gets an answer (a failed handshake would just close).
    let upgrade = header("upgrade").to_ascii_lowercase().contains("websocket")
        && header("connection").to_ascii_lowercase().contains("upgrade")
        && !header("sec-websocket-key").is_empty()
        && header("sec-websocket-version").trim() == "13";
    let origin = Some(header("origin")).filter(|o| !o.is_empty());
    Head::Request(Request {
        head_only,
        path: path.to_string(),
        query: query.to_string(),
        accepts_gzip: accepts_gzip(&header("accept-encoding")),
        upgrade,
        origin,
    })
}

/// `gzip` (or `*`) in an Accept-Encoding header, not with `q=0`.
fn accepts_gzip(value: &str) -> bool {
    value.split(',').any(|part| {
        let mut it = part.split(';').map(str::trim);
        let name = it.next().unwrap_or("").to_ascii_lowercase();
        let refused = it.any(|p| p.strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()).is_some_and(|q| q <= 0.0));
        (name == "gzip" || name == "*") && !refused
    })
}

/// The socket, with the request head that was already read in front, so
/// that the WebSocket handshake can read it again.
struct Replay {
    head: Vec<u8>,
    at: usize,
    inner: TcpStream,
}

impl Read for Replay {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at < self.head.len() {
            let n = buf.len().min(self.head.len() - self.at);
            buf[..n].copy_from_slice(&self.head[self.at..self.at + n]);
            self.at += n;
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl Write for Replay {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn is_timeout(e: &io::Error) -> bool {
    // A read timeout is WouldBlock on Unix and TimedOut on Windows.
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// A client: after the handshake, one loop that sends what the hub queued
/// and reads what the client says, a short read timeout apart.
fn websocket(stream: Replay, peer: SocketAddr, origin: Option<String>, shared: &Shared) {
    let config = WebSocketConfig {
        max_message_size: Some(MAX_CLIENT_MESSAGE),
        max_frame_size: Some(MAX_CLIENT_MESSAGE),
        ..WebSocketConfig::default()
    };
    let mut ws: WebSocket<Replay> = match tungstenite::accept_with_config(stream, Some(config)) {
        Ok(ws) => ws,
        Err(e) => {
            log::debug!("{peer}: WebSocket handshake: {e}");
            return;
        }
    };
    if ws.get_mut().inner.set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let id = shared.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let outbox = Arc::new(Outbox::default());
    let joined = shared.hub.send(ToHub::Join {
        id,
        peer,
        origin,
        outbox: outbox.clone(),
    });
    if joined.is_err() {
        return;
    }
    let (mut heard, mut pinged) = (Instant::now(), Instant::now());
    let mut closed_by_peer = false;
    let why = 'run: loop {
        let (texts, closed) = outbox.take();
        for text in texts {
            if let Err(e) = ws.send(Message::Text(text.to_string())) {
                break 'run format!("sending: {e}");
            }
        }
        if closed {
            let _ = ws.close(None);
            let _ = ws.flush();
            break "closed by the server".to_string();
        }
        // Pinged now and then, and let go when nothing at all came from it
        // for a while: a phone that left the wifi without saying goodbye.
        if pinged.elapsed() >= shared.ping_every {
            pinged = Instant::now();
            if let Err(e) = ws.send(Message::Ping(Vec::new())) {
                break format!("sending: {e}");
            }
        }
        match ws.read() {
            Ok(Message::Text(text)) => {
                heard = Instant::now();
                let msg = ClientMsg::decode(&text);
                if shared.hub.send(ToHub::Message { id, msg }).is_err() {
                    break "the server stops".into();
                }
            }
            Ok(Message::Binary(_)) => {
                heard = Instant::now();
                let msg = Err(wire::Error::Malformed("a binary message; this protocol speaks text".into()));
                if shared.hub.send(ToHub::Message { id, msg }).is_err() {
                    break "the server stops".into();
                }
            }
            Ok(Message::Close(_)) => {
                closed_by_peer = true;
                break "closed by the client".into();
            }
            Ok(_) => heard = Instant::now(),
            Err(tungstenite::Error::Io(e)) if is_timeout(&e) => {
                if heard.elapsed() > shared.silent_for {
                    break format!("nothing heard for {:?}", shared.silent_for);
                }
            }
            Err(e) => break e.to_string(),
        }
    };
    // Gone from the hub before the client hears the close reply: whoever
    // connects after that finds the hub without this client.
    let (ack, acked) = channel();
    if shared.hub.send(ToHub::Leave { id, why, ack }).is_ok() {
        let _ = acked.recv_timeout(Duration::from_secs(2));
    }
    if closed_by_peer {
        let _ = ws.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_is_accepted_unless_refused() {
        assert!(accepts_gzip("gzip, deflate, br"));
        assert!(accepts_gzip("br;q=1.0, gzip;q=0.8"));
        assert!(accepts_gzip("*"));
        assert!(!accepts_gzip("gzip;q=0, br"));
        assert!(!accepts_gzip("identity"));
        assert!(!accepts_gzip(""));
    }

    #[test]
    fn requests_are_parsed_and_garbage_refused() {
        let Head::Request(r) =
            parse(b"GET /vna-gui_bg.wasm?v=123 HTTP/1.1\r\nHost: x\r\nAccept-Encoding: gzip\r\nOrigin: https://pa1bh.nl\r\n\r\n")
        else {
            panic!()
        };
        assert_eq!(
            (r.path.as_str(), r.query.as_str(), r.accepts_gzip),
            ("/vna-gui_bg.wasm", "v=123", true)
        );
        assert_eq!(r.origin.as_deref(), Some("https://pa1bh.nl"));
        assert!(!r.upgrade && !r.head_only);
        assert!(matches!(parse(b"POST / HTTP/1.1\r\n\r\n"), Head::Refuse(405, _)));
        assert!(matches!(parse(b"\x00\x01\x02\r\n\r\n"), Head::Refuse(400, _)));
        assert!(matches!(parse(b"\r\n\r\nGET / HTTP/1.1\r\n"), Head::Refuse(400, _)));
    }
}
