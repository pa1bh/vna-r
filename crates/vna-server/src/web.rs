//! The web client over plain HTTP: the three files of the browser build,
//! built in (see `build.rs`) or from a folder (`--web`).
//!
//! Every response closes the connection: a page load is three requests, so
//! keep-alive would save nothing worth its code.
//!
//! `index.html` gets `<meta name="vna-r-server" content="/ws">` in its head
//! as it is served: that is how the page knows it came from a server, and
//! where the WebSocket is. The same page on an ordinary website lacks it.

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use flate2::read::GzDecoder;

use crate::conn::Request;
use crate::WS_PATH;

/// A file embedded gzip-compressed, and its size uncompressed.
pub(crate) struct Packed {
    pub(crate) gz: &'static [u8],
    pub(crate) len: usize,
}

// BUILT_IN, INDEX, JS, WASM: written by build.rs.
include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

/// The two big files are asked for with the build's fingerprint (`?v=…`),
/// so with one they can stay in a cache; the page itself must not.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const NO_CACHE: &str = "no-cache";

pub(crate) struct Web {
    dir: Option<PathBuf>,
}

enum Body<'a> {
    Bytes(Cow<'a, [u8]>),
    /// Sent as it is to a client that takes gzip, unpacked for one that does not.
    Packed(&'a Packed),
    File(File, u64),
}

impl Web {
    pub(crate) fn new(dir: Option<PathBuf>) -> io::Result<Web> {
        if let Some(d) = &dir {
            if !d.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("--web {}: not a folder", d.display()),
                ));
            }
        }
        Ok(Web { dir })
    }

    /// For the log line at start.
    pub(crate) fn describe(&self) -> String {
        match (&self.dir, BUILT_IN) {
            (Some(d), _) => format!("from {}", d.display()),
            (None, Some(_)) => "built in".into(),
            (None, None) => "a placeholder (built without the web client, see scripts/build-web.sh)".into(),
        }
    }

    pub(crate) fn serve(&self, out: &mut impl Write, req: &Request) -> io::Result<()> {
        match &self.dir {
            Some(dir) => serve_dir(out, req, dir),
            None => built_in(out, req),
        }
    }
}

fn serve_dir(out: &mut impl Write, req: &Request, dir: &std::path::Path) -> io::Result<()> {
    let name = match req.path.as_str() {
        "/" => "index.html",
        p => p.strip_prefix('/').unwrap_or(""),
    };
    // A plain name in that folder and nothing else: no separators, no
    // dot files, no percent escapes (which are never decoded).
    let plain = !name.is_empty() && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if !plain {
        log::debug!("refused {:?}", req.path);
        return not_found(out, req);
    }
    let path = dir.join(name);
    let file = match File::open(&path) {
        Ok(f) if f.metadata().is_ok_and(|m| m.is_file()) => f,
        _ => return not_found(out, req),
    };
    let ctype = content_type(name);
    if name == "index.html" {
        let mut html = Vec::new();
        (&file).read_to_end(&mut html)?;
        return send(out, req, 200, ctype, NO_CACHE, Body::Bytes(mark_served(&html).into()));
    }
    let len = file.metadata()?.len();
    send(out, req, 200, ctype, cache_for(req), Body::File(file, len))
}

fn built_in(out: &mut impl Write, req: &Request) -> io::Result<()> {
    let packed = match req.path.as_str() {
        "/" | "/index.html" => {
            return send(
                out,
                req,
                200,
                "text/html; charset=utf-8",
                NO_CACHE,
                Body::Bytes(mark_served(INDEX).into()),
            )
        }
        "/vna-gui.js" => JS.as_ref().map(|p| (p, "text/javascript")),
        "/vna-gui_bg.wasm" => WASM.as_ref().map(|p| (p, "application/wasm")),
        _ => None,
    };
    match packed {
        Some((p, ctype)) => send(out, req, 200, ctype, cache_for(req), Body::Packed(p)),
        None => not_found(out, req),
    }
}

fn cache_for(req: &Request) -> &'static str {
    if req.query.split('&').any(|kv| kv.starts_with("v=")) {
        IMMUTABLE
    } else {
        NO_CACHE
    }
}

fn not_found(out: &mut impl Write, req: &Request) -> io::Result<()> {
    plain(out, 404, "not found", req.head_only)
}

/// A short plain-text answer, for errors.
pub(crate) fn plain(out: &mut impl Write, status: u16, text: &str, head_only: bool) -> io::Result<()> {
    let body = format!("{text}\n");
    head(out, status, "text/plain; charset=utf-8", NO_CACHE, body.len() as u64, None)?;
    if !head_only {
        out.write_all(body.as_bytes())?;
    }
    out.flush()
}

fn send(out: &mut impl Write, req: &Request, status: u16, ctype: &str, cache: &str, body: Body) -> io::Result<()> {
    match body {
        Body::Bytes(b) => {
            head(out, status, ctype, cache, b.len() as u64, None)?;
            if !req.head_only {
                out.write_all(&b)?;
            }
        }
        Body::Packed(p) if req.accepts_gzip => {
            head(out, status, ctype, cache, p.gz.len() as u64, Some("gzip"))?;
            if !req.head_only {
                out.write_all(p.gz)?;
            }
        }
        Body::Packed(p) => {
            head(out, status, ctype, cache, p.len as u64, Some("identity"))?;
            if !req.head_only {
                io::copy(&mut GzDecoder::new(p.gz), out)?;
            }
        }
        Body::File(mut f, len) => {
            head(out, status, ctype, cache, len, None)?;
            if !req.head_only {
                io::copy(&mut f, out)?;
            }
        }
    }
    out.flush()
}

/// The status line and headers. `encoding` is set for a resource that comes
/// compressed or not depending on the request, which caches must know.
fn head(out: &mut impl Write, status: u16, ctype: &str, cache: &str, len: u64, encoding: Option<&str>) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        _ => "",
    };
    let mut h = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {len}\r\nCache-Control: {cache}\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n"
    );
    if status == 405 {
        h.push_str("Allow: GET, HEAD\r\n");
    }
    match encoding {
        Some("gzip") => h.push_str("Content-Encoding: gzip\r\nVary: Accept-Encoding\r\n"),
        Some(_) => h.push_str("Vary: Accept-Encoding\r\n"),
        None => {}
    }
    h.push_str("\r\n");
    out.write_all(h.as_bytes())
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") | Some("s1p") | Some("s2p") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The page with the tag that says it came from a server, right after `<head>`.
fn mark_served(html: &[u8]) -> Vec<u8> {
    let tag = format!("\n    <meta name=\"vna-r-server\" content=\"{WS_PATH}\">");
    let lower = html.to_ascii_lowercase();
    // `<head>` or `<head …>`, whichever comes first; without one, in front.
    let at = lower
        .windows(5)
        .position(|w| w == b"<head")
        .filter(|&i| matches!(lower.get(i + 5), Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')))
        .and_then(|i| lower[i..].iter().position(|&b| b == b'>').map(|j| i + j + 1))
        .unwrap_or(0);
    let mut out = Vec::with_capacity(html.len() + tag.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&html[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_goes_into_the_head() {
        let page = mark_served(b"<!doctype html>\n<HTML><Head>\n<title>x</title></head></html>");
        let page = String::from_utf8(page).unwrap();
        assert!(
            page.contains("<Head>\n    <meta name=\"vna-r-server\" content=\"/ws\">\n<title>"),
            "{page}"
        );
        let page = String::from_utf8(mark_served(b"<head lang=\"en\"><title>")).unwrap();
        assert!(page.starts_with("<head lang=\"en\">\n    <meta name=\"vna-r-server\""), "{page}");
        // <header> is not <head>
        let page = String::from_utf8(mark_served(b"<header>")).unwrap();
        assert!(page.starts_with("\n    <meta"), "{page}");
    }

    #[test]
    fn the_built_in_page_has_the_tag() {
        let page = String::from_utf8(mark_served(INDEX)).unwrap();
        assert_eq!(page.matches("vna-r-server").count(), 1);
        assert_eq!(BUILT_IN.is_some(), JS.is_some() && WASM.is_some());
    }
}
