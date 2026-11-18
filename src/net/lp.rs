//! The link to the village's own relay over plain HTTPS requests ("long
//! polling"), for networks whose proxies or filters break WebSockets but let
//! ordinary web requests through. The same MQTT bytes as over a WebSocket:
//! what's sent goes up in requests of its own, one after another; one
//! request always waits at the relay for what's coming down (answered the
//! moment there's something). See the relay's poll.rs for the other end.

use super::ws::{self, Stream, Url};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

/// One kept-alive HTTP/1.1 connection to the relay (made again when it's
/// been closed).
struct Http {
    url: Url,
    timeout: Duration,
    s: Option<Box<dyn Stream>>,
    buf: Vec<u8>,
}

impl Http {
    fn new(url: &Url, timeout: Duration) -> Http {
        Http { url: Url { tls: url.tls, host: url.host.clone(), port: url.port, path: url.path.clone() }, timeout, s: None, buf: Vec::new() }
    }

    /// POST `body` to `path` under the relay's address: the answer's status
    /// and body. A connection the relay's let go of is made again, once.
    fn post(&mut self, path: &str, body: &[u8]) -> io::Result<(u16, Vec<u8>)> {
        let fresh = self.s.is_none();
        match self.try_post(path, body) {
            Ok(r) => Ok(r),
            Err(_) if !fresh => {
                self.s = None;
                self.try_post(path, body)
            }
            Err(e) => Err(e),
        }
    }

    fn try_post(&mut self, path: &str, body: &[u8]) -> io::Result<(u16, Vec<u8>)> {
        if self.s.is_none() {
            self.s = Some(ws::stream(self.url.tls, &self.url.host, self.url.port, self.timeout)?);
            self.buf.clear();
        }
        let host = if (self.url.tls && self.url.port == 443) || (!self.url.tls && self.url.port == 80) { self.url.host.clone() } else { format!("{}:{}", self.url.host, self.url.port) };
        let agent = ws::user_agent();
        let head = format!(
            "POST {}{path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {agent}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
            self.url.path.trim_end_matches('/'),
            body.len()
        );
        let r = self.exchange(head.as_bytes(), body);
        if r.is_err() {
            self.s = None;
        }
        r
    }

    fn exchange(&mut self, head: &[u8], body: &[u8]) -> io::Result<(u16, Vec<u8>)> {
        let s = self.s.as_mut().expect("connected");
        let mut req = Vec::with_capacity(head.len() + body.len());
        req.extend_from_slice(head);
        req.extend_from_slice(body);
        s.write_all(&req)?;
        s.flush()?;
        // the answer's head
        let end = loop {
            if let Some(i) = find(&self.buf, b"\r\n\r\n") {
                break i + 4;
            }
            if self.buf.len() > 16384 {
                return Err(bad("an answer's head too long"));
            }
            self.fill()?;
        };
        let head = String::from_utf8_lossy(&self.buf[..end]).to_string();
        self.buf.drain(..end);
        let mut lines = head.split("\r\n");
        let status: u16 = lines.next().and_then(|l| l.split(' ').nth(1)).and_then(|c| c.parse().ok()).ok_or_else(|| bad("no status"))?;
        let mut len = None;
        let mut chunked = false;
        let mut close = false;
        for l in lines {
            let Some((k, v)) = l.split_once(':') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
            match k.as_str() {
                "content-length" => len = v.parse::<usize>().ok(),
                "transfer-encoding" => chunked = v.contains("chunked"),
                "connection" => close = v.contains("close"),
                _ => {}
            }
        }
        let body = if chunked {
            let mut out = Vec::new();
            loop {
                let line_end = loop {
                    if let Some(i) = find(&self.buf, b"\r\n") {
                        break i;
                    }
                    self.fill()?;
                };
                let size = usize::from_str_radix(String::from_utf8_lossy(&self.buf[..line_end]).split(';').next().unwrap_or("").trim(), 16).map_err(|_| bad("a chunk's size"))?;
                self.buf.drain(..line_end + 2);
                if size > 4 << 20 {
                    return Err(bad("a chunk too big"));
                }
                self.need(size + 2)?;
                out.extend_from_slice(&self.buf[..size]);
                self.buf.drain(..size + 2);
                if size == 0 {
                    break;
                }
            }
            out
        } else if let Some(n) = len {
            if n > 4 << 20 {
                return Err(bad("an answer too big"));
            }
            self.need(n)?;
            self.buf.drain(..n).collect()
        } else if status == 204 || status == 304 || (100..200).contains(&status) {
            Vec::new()
        } else {
            // (no length: the body runs to the connection's end)
            let s = self.s.as_mut().expect("connected");
            let mut rest = std::mem::take(&mut self.buf);
            s.read_to_end(&mut rest)?;
            close = true;
            rest
        };
        if close {
            self.s = None;
        }
        Ok((status, body))
    }

    fn need(&mut self, n: usize) -> io::Result<()> {
        while self.buf.len() < n {
            self.fill()?;
        }
        Ok(())
    }

    fn fill(&mut self) -> io::Result<()> {
        let s = self.s.as_mut().expect("connected");
        let mut b = [0u8; 16384];
        let n = s.read(&mut b)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed"));
        }
        self.buf.extend_from_slice(&b[..n]);
        Ok(())
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// An open long-polling session with the relay.
pub struct Lp {
    down: Receiver<Vec<u8>>,
    left: Vec<u8>,
    up: Sender<Vec<u8>>,
    dead: Arc<AtomicBool>,
    poll: Duration,
}

/// Open a session at `url` (the relay's https://…/poll); reads after this
/// wait `poll` at most (the link's loop polls).
pub fn open(url: &Url, timeout: Duration, poll: Duration) -> io::Result<Lp> {
    let mut h = Http::new(url, timeout);
    let (status, id) = h.post("", &[])?;
    let id = String::from_utf8_lossy(&id).trim().to_string();
    if status != 200 || id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(io::ErrorKind::ConnectionRefused, format!("no session ({status})")));
    }
    let dead = Arc::new(AtomicBool::new(false));
    let (down_tx, down) = channel::<Vec<u8>>();
    let (up, up_rx) = channel::<Vec<u8>>();
    // what's coming down: one request always waiting (the relay answers
    // within 20 s, with nothing if there's nothing)
    {
        let mut h = Http::new(url, Duration::from_secs(35));
        let path = format!("/{id}/down");
        let dead = dead.clone();
        std::thread::Builder::new().name("xmas-online-down".into()).spawn(move || {
            let mut fails = 0;
            while !dead.load(Ordering::Relaxed) {
                match h.post(&path, &[]) {
                    Ok((200, b)) => {
                        fails = 0;
                        if !b.is_empty() && down_tx.send(b).is_err() {
                            break;
                        }
                    }
                    // (a request of ours still waiting there, from a
                    // connection that dropped: it gives up soon)
                    Ok((409, _)) if fails < 3 => {
                        fails += 1;
                        std::thread::sleep(Duration::from_secs(2));
                    }
                    Err(e) if fails < 2 && matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => fails += 1,
                    _ => break,
                }
            }
            dead.store(true, Ordering::Relaxed);
        })?;
    }
    // what's going up: in order, what's queued together in one request
    {
        let path_up = format!("/{id}/up");
        let path_close = format!("/{id}/close");
        let dead = dead.clone();
        std::thread::Builder::new().name("xmas-online-up".into()).spawn(move || {
            let mut closing = false;
            while !dead.load(Ordering::Relaxed) && !closing {
                let first = match up_rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(b) => b,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                let mut body = Vec::new();
                for b in std::iter::once(first).chain(up_rx.try_iter()) {
                    if b.is_empty() {
                        closing = true;
                        break;
                    }
                    body.extend(b);
                }
                if !body.is_empty() && !matches!(h.post(&path_up, &body), Ok((204, _))) {
                    break;
                }
            }
            if closing {
                let _ = h.post(&path_close, &[]);
            }
            dead.store(true, Ordering::Relaxed);
        })?;
    }
    Ok(Lp { down, left: Vec::new(), up, dead, poll })
}

impl Lp {
    /// MQTT bytes from the relay: Ok(0) once the session's over; TimedOut
    /// when nothing's come yet.
    pub fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.left.is_empty() {
            match self.down.recv_timeout(self.poll) {
                Ok(b) => self.left = b,
                Err(RecvTimeoutError::Timeout) => return Err(io::Error::new(io::ErrorKind::TimedOut, "nothing yet")),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.left.len());
        out[..n].copy_from_slice(&self.left[..n]);
        self.left.drain(..n);
        Ok(n)
    }

    /// MQTT bytes to the relay (sent in a moment, in order).
    pub fn write_all(&mut self, b: &[u8]) -> io::Result<()> {
        if self.dead.load(Ordering::Relaxed) || b.is_empty() {
            return if b.is_empty() { Ok(()) } else { Err(io::Error::new(io::ErrorKind::BrokenPipe, "session over")) };
        }
        self.up.send(b.to_vec()).map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "session over"))
    }

    /// What's queued goes up, then the session's closed.
    pub fn close(&mut self) {
        let _ = self.up.send(Vec::new());
    }
}

impl Drop for Lp {
    fn drop(&mut self) {
        let _ = self.up.send(Vec::new());
    }
}
