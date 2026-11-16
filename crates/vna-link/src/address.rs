//! Server addresses: what a user types, and how it is written as a port.
//!
//! A server is a port with another kind of name (docs/10, "Een server is een
//! poort"): `ws://vna.local:8080/ws` where a serial port is `/dev/ttyACM0`.

use std::net::{Ipv4Addr, Ipv6Addr};

/// The TCP port of a vna-server unless the address says otherwise.
pub const DEFAULT_SERVER_PORT: u16 = 8080;

/// Whether `port` (a port name as the GUI keeps it) is a server rather than
/// a serial port: it starts with `ws://` or `wss://`.
pub fn is_server_address(port: &str) -> bool {
    let p = port.trim_start().as_bytes();
    let starts = |s: &[u8]| p.len() >= s.len() && p[..s.len()].eq_ignore_ascii_case(s);
    starts(b"ws://") || starts(b"wss://")
}

/// What a user typed, as the address of the server's WebSocket:
/// `vna.local`, `vna.local:9000`, `192.168.1.5`, `[fe80::1]`, `ws://…` or
/// `http://vna.local:8080/` all become `ws://host:port/ws` (port
/// [`DEFAULT_SERVER_PORT`] when none is given). `wss://` (and `https://`) is
/// for a server behind a TLS proxy: its host, port and path are kept as they
/// are, only an empty path becomes `/ws`. `None` for anything that is not an
/// address.
pub fn server_url(input: &str) -> Option<String> {
    let input = input.trim();
    let (secure, rest) = match input.find("://") {
        Some(i) => {
            let scheme = input[..i].to_ascii_lowercase();
            let secure = match scheme.as_str() {
                "ws" | "http" => false,
                "wss" | "https" => true,
                _ => return None,
            };
            (secure, &input[i + 3..])
        }
        None => (false, input),
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let path = tail.split(['?', '#']).next().unwrap_or("");
    let (host, port) = split_authority(authority)?;
    let path = match path.trim_end_matches('/') {
        "" | "/index.html" => "/ws".to_string(),
        p if secure => p.to_string(),
        "/ws" => "/ws".to_string(),
        // A plain server has nothing else; a path behind a proxy needs wss.
        _ => return None,
    };
    let scheme = if secure { "wss" } else { "ws" };
    Some(match (port, secure) {
        (Some(p), _) => format!("{scheme}://{host}:{p}{path}"),
        (None, true) => format!("{scheme}://{host}{path}"),
        (None, false) => format!("{scheme}://{host}:{DEFAULT_SERVER_PORT}{path}"),
    })
}

/// `host[:port]`, with an IPv6 literal in brackets, or bare when there is no port.
fn split_authority(a: &str) -> Option<(String, Option<u16>)> {
    if a.is_empty() || a.contains('@') {
        return None;
    }
    let port = |p: &str| p.parse::<u16>().ok().filter(|&p| p != 0);
    if let Some(inner) = a.strip_prefix('[') {
        let (ip, after) = inner.split_once(']')?;
        ip.parse::<Ipv6Addr>().ok()?;
        let port = match after {
            "" => None,
            p => Some(port(p.strip_prefix(':')?)?),
        };
        return Some((format!("[{ip}]"), port));
    }
    if let Ok(ip) = a.parse::<Ipv6Addr>() {
        return Some((format!("[{ip}]"), None));
    }
    let (host, p) = match a.rsplit_once(':') {
        Some((h, p)) => (h, Some(port(p)?)),
        None => (a, None),
    };
    let name_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .split('.')
            .all(|l| !l.is_empty() && !l.starts_with('-') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    if !(name_ok || host.parse::<Ipv4Addr>().is_ok()) {
        return None;
    }
    Some((host.to_string(), p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_user_types_becomes_a_websocket_address() {
        let cases = [
            ("vna.local", "ws://vna.local:8080/ws"),
            ("  vna.local:9000  ", "ws://vna.local:9000/ws"),
            ("192.168.1.5", "ws://192.168.1.5:8080/ws"),
            ("raspberrypi", "ws://raspberrypi:8080/ws"),
            ("ws://vna.local:8080/ws", "ws://vna.local:8080/ws"),
            ("ws://vna.local/ws/", "ws://vna.local:8080/ws"),
            ("WS://vna.local", "ws://vna.local:8080/ws"),
            ("http://vna.local:8080/", "ws://vna.local:8080/ws"),
            ("http://192.168.1.5:8080/index.html?x=1", "ws://192.168.1.5:8080/ws"),
            ("[::1]", "ws://[::1]:8080/ws"),
            ("[fe80::1]:8081", "ws://[fe80::1]:8081/ws"),
            ("::1", "ws://[::1]:8080/ws"),
            ("ws://[2001:db8::7]:8080/ws", "ws://[2001:db8::7]:8080/ws"),
            ("wss://vna.example.org/shack/ws", "wss://vna.example.org/shack/ws"),
            ("wss://vna.example.org:8443/ws", "wss://vna.example.org:8443/ws"),
            ("https://vna.example.org/", "wss://vna.example.org/ws"),
        ];
        for (typed, url) in cases {
            assert_eq!(server_url(typed).as_deref(), Some(url), "{typed:?}");
            assert!(is_server_address(url));
        }
    }

    #[test]
    fn rubbish_is_not_an_address() {
        for typed in [
            "",
            "   ",
            "vna local",
            "vna.local:99999",
            "vna.local:0",
            "vna.local:http",
            "ftp://vna.local",
            "pa1bh@vna.local",
            "[::1",
            "[::1]x",
            "[not:an:ip]",
            "http://",
            "ws://:8080/ws",
            "/dev/ttyACM0",
            "-bad.local",
            "a..b",
            "ws://vna.local/elsewhere",
        ] {
            assert_eq!(server_url(typed), None, "{typed:?}");
        }
    }

    #[test]
    fn serial_ports_are_not_servers() {
        for p in ["/dev/ttyACM0", "/dev/cu.usbmodemv1_01", "COM3", "sim:", "sim:0"] {
            assert!(!is_server_address(p), "{p}");
        }
        assert!(is_server_address("wss://x/ws") && is_server_address("WS://x"));
    }
}
