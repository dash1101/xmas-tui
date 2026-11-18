//! The link to a relay over a WebSocket (wss://, through TLS; ws:// for one
//! on this machine): the village's own relay is reached this way, on the
//! web's own port (443), which networks that block the free relays' ports
//! still let through. MQTT's bytes go in binary frames both ways.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// A relay's WebSocket address (or its long-polling one, https://), taken
/// apart.
#[derive(Debug, PartialEq)]
pub struct Url {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

pub fn parse(url: &str) -> Option<Url> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("wss://").or_else(|| url.strip_prefix("https://")) {
        (true, r)
    } else if let Some(r) = url.strip_prefix("ws://").or_else(|| url.strip_prefix("http://")) {
        (false, r)
    } else {
        return None;
    };
    let (hostport, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (hostport, if tls { 443 } else { 80 }),
    };
    (!host.is_empty()).then(|| Url { tls, host: host.to_string(), port, path })
}

pub(super) trait Stream: Read + Write + Send {
    fn tcp(&self) -> &TcpStream;
}

impl Stream for TcpStream {
    fn tcp(&self) -> &TcpStream {
        self
    }
}

impl Stream for rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    fn tcp(&self) -> &TcpStream {
        &self.sock
    }
}

/// An open WebSocket to a relay.
pub struct Ws {
    s: Box<dyn Stream>,
    /// Bytes read but not yet a whole frame; payload not yet handed on.
    raw: Vec<u8>,
    ready: Vec<u8>,
    closed: bool,
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CFG: std::sync::OnceLock<Arc<rustls::ClientConfig>> = std::sync::OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

/// A TCP connection to `host`: each address it has tried in turn, IPv4
/// first (a machine without IPv6 fails those at once; one that has it gets
/// there either way).
pub fn connect(host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
    let mut addrs: Vec<std::net::SocketAddr> = (host, port).to_socket_addrs()?.collect();
    addrs.sort_by_key(|a| a.is_ipv6());
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for a in addrs {
        match TcpStream::connect_timeout(&a, timeout) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// A connection to `host`, through TLS if `tls` (its certificate checked
/// against the usual roots), reads and writes waiting `timeout` at most.
pub(super) fn stream(tls: bool, host: &str, port: u16, timeout: Duration) -> io::Result<Box<dyn Stream>> {
    let tcp = connect(host, port, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    let _ = tcp.set_nodelay(true);
    Ok(if tls {
        let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad host name"))?;
        let conn = rustls::ClientConnection::new(tls_config(), name).map_err(io::Error::other)?;
        Box::new(rustls::StreamOwned::new(conn, tcp))
    } else {
        Box::new(tcp)
    })
}

/// The app's version: the main Cargo.toml's, read when this is built (the
/// Android library is built from these same files as a package of its own,
/// whose CARGO_PKG_VERSION isn't the app's).
fn version() -> &'static str {
    include_str!("../../Cargo.toml").lines().find_map(|l| l.strip_prefix("version = \"")?.split('"').next()).unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// What the app calls itself to a relay, in the User-Agent of the
/// WebSocket's opening request and of each long-polling request: its
/// version and the system it's on, as in "xmas-tui/1.26.0 (linux)". The
/// relay's admin console shows it beside the connection.
pub(crate) fn user_agent() -> String {
    format!("xmas-tui/{} ({})", version(), std::env::consts::OS)
}

/// Connect, and ask for the WebSocket (the "mqtt" subprotocol). Reads
/// after this wait `poll` at most (the link's loop polls).
pub fn open(url: &Url, timeout: Duration, poll: Duration) -> io::Result<Ws> {
    let mut s = stream(url.tls, &url.host, url.port, timeout)?;
    let key = {
        let a = super::seal::random().to_le_bytes();
        let b = super::seal::random().to_le_bytes();
        let mut k = [0u8; 16];
        k[..8].copy_from_slice(&a);
        k[8..].copy_from_slice(&b);
        base64(&k)
    };
    let host = if (url.tls && url.port == 443) || (!url.tls && url.port == 80) { url.host.clone() } else { format!("{}:{}", url.host, url.port) };
    let agent = user_agent();
    let req = format!("GET {} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: mqtt\r\nUser-Agent: {agent}\r\n\r\n", url.path);
    s.write_all(req.as_bytes())?;
    s.flush()?;
    // the answer's head: "101 Switching Protocols"
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 8192 || s.read(&mut b)? == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no WebSocket answer"));
        }
        head.push(b[0]);
    }
    let status = String::from_utf8_lossy(&head);
    if !status.starts_with("HTTP/1.1 101") {
        let line = status.lines().next().unwrap_or("").to_string();
        return Err(io::Error::new(io::ErrorKind::ConnectionRefused, line));
    }
    s.tcp().set_read_timeout(Some(poll))?;
    Ok(Ws { s, raw: Vec::new(), ready: Vec::new(), closed: false })
}

impl Ws {
    /// MQTT bytes from the relay: Ok(0) when it's closed; WouldBlock or
    /// TimedOut when nothing's come yet.
    pub fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            if !self.ready.is_empty() {
                let n = out.len().min(self.ready.len());
                out[..n].copy_from_slice(&self.ready[..n]);
                self.ready.drain(..n);
                return Ok(n);
            }
            if self.closed {
                return Ok(0);
            }
            if self.frame()? {
                continue;
            }
            let mut buf = [0u8; 16384];
            let n = self.s.read(&mut buf)?;
            if n == 0 {
                return Ok(0);
            }
            self.raw.extend_from_slice(&buf[..n]);
            if self.raw.len() > 1 << 20 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too big"));
            }
        }
    }

    /// One whole frame from what's read, if there is one (true).
    fn frame(&mut self) -> io::Result<bool> {
        let r = &self.raw;
        if r.len() < 2 {
            return Ok(false);
        }
        let op = r[0] & 0x0f;
        let masked = r[1] & 0x80 != 0;
        let (len, mut at) = match r[1] & 0x7f {
            126 if r.len() >= 4 => (u16::from_be_bytes([r[2], r[3]]) as usize, 4),
            127 if r.len() >= 10 => (u64::from_be_bytes([r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]]) as usize, 10),
            126 | 127 => return Ok(false),
            n => (n as usize, 2),
        };
        let mask = if masked {
            if r.len() < at + 4 {
                return Ok(false);
            }
            let m = [r[at], r[at + 1], r[at + 2], r[at + 3]];
            at += 4;
            Some(m)
        } else {
            None
        };
        if r.len() < at + len {
            return Ok(false);
        }
        let mut payload = r[at..at + len].to_vec();
        if let Some(m) = mask {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= m[i % 4];
            }
        }
        self.raw.drain(..at + len);
        match op {
            0x0..=0x2 => self.ready.extend(payload),
            0x8 => self.closed = true,
            0x9 => self.send(0xA, &payload)?,
            _ => {}
        }
        Ok(true)
    }

    fn send(&mut self, op: u8, payload: &[u8]) -> io::Result<()> {
        let mut f = Vec::with_capacity(payload.len() + 14);
        f.push(0x80 | op);
        match payload.len() {
            n if n < 126 => f.push(0x80 | n as u8),
            n if n < 65536 => {
                f.push(0x80 | 126);
                f.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                f.push(0x80 | 127);
                f.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        let m = (super::seal::random() as u32).to_le_bytes();
        f.extend_from_slice(&m);
        f.extend(payload.iter().enumerate().map(|(i, b)| b ^ m[i % 4]));
        self.s.write_all(&f)?;
        self.s.flush()
    }

    /// MQTT bytes to the relay, in a binary frame.
    pub fn write_all(&mut self, b: &[u8]) -> io::Result<()> {
        self.send(0x2, b)
    }

    pub fn close(&mut self) {
        let _ = self.send(0x8, &[]);
        let _ = self.s.tcp().shutdown(std::net::Shutdown::Both);
    }
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_come_apart() {
        assert_eq!(parse("wss://xmas-relay.novalabs.app/mqtt"), Some(Url { tls: true, host: "xmas-relay.novalabs.app".into(), port: 443, path: "/mqtt".into() }));
        assert_eq!(parse("ws://127.0.0.1:18080/mqtt"), Some(Url { tls: false, host: "127.0.0.1".into(), port: 18080, path: "/mqtt".into() }));
        assert_eq!(parse("https://xmas-relay.novalabs.app/poll").map(|u| (u.tls, u.port, u.path)), Some((true, 443, "/poll".into())));
        assert_eq!(parse("ftp://x"), None);
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fo"), "Zm8=");
    }

    #[test]
    fn the_app_names_itself() {
        let ua = user_agent();
        assert!(ua.starts_with("xmas-tui/"), "{ua}");
        assert!(ua.ends_with(&format!(" ({})", std::env::consts::OS)), "{ua}");
        // (it goes in a request's head as it is: nothing that would break a line)
        assert!(!ua.chars().any(|c| c.is_control()), "{ua}");
        // the app's own version, whichever package builds this file
        assert!(version().starts_with(|c: char| c.is_ascii_digit()), "{ua}");
        if env!("CARGO_PKG_NAME") == "xmas-tui" {
            assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        }
    }
}
