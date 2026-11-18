//! The link to the relay (the desktop and the Android app): a thread that
//! connects (each relay in turn, backing off when none will), and passes
//! bytes both ways, over TCP, a WebSocket or HTTPS long polling. What's sent is sealed already (see `seal`).

use super::{Link, Online, RELAYS};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

pub enum Up {
    Connected,
    Bytes(Vec<u8>),
    Closed,
}

pub struct Tcp {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Up>,
    stop: Arc<AtomicBool>,
    /// Which relay it's through, and how (once connected).
    via: Arc<std::sync::Mutex<String>>,
}

impl Tcp {
    /// The link, trying the relays in turn from the `first`.
    pub fn start(first: usize) -> Option<Tcp> {
        let (tx, out_rx) = channel::<Vec<u8>>();
        let (up_tx, rx) = channel::<Up>();
        let stop = Arc::new(AtomicBool::new(false));
        let st = stop.clone();
        let via = Arc::new(std::sync::Mutex::new(String::new()));
        let v = via.clone();
        std::thread::Builder::new()
            .name("xmas-online".into())
            .spawn(move || run(out_rx, up_tx, st, first, v))
            .ok()?;
        Some(Tcp { tx, rx, stop, via })
    }

    /// Which relay it's through, and how ("xmas-relay.novalabs.app ·
    /// WebSocket"); empty while connecting.
    pub fn via(&self) -> String {
        self.via.lock().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn send(&self, b: Vec<u8>) {
        if !b.is_empty() {
            let _ = self.tx.send(b);
        }
    }

    pub fn poll(&self) -> Vec<Up> {
        let mut v = vec![];
        loop {
            match self.rx.try_recv() {
                Ok(u) => v.push(u),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    v.push(Up::Closed);
                    break;
                }
            }
        }
        v
    }

    /// The last bytes (a goodbye), then the link closes.
    pub fn close(self, last: Vec<u8>) {
        self.send(last);
        // (an empty one: close once what's before it is written)
        let _ = self.tx.send(vec![]);
    }
}

impl Drop for Tcp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A link to one relay: plain MQTT over TCP, over a WebSocket, or over
/// HTTPS long polling.
enum Pipe {
    Tcp(TcpStream),
    Ws(super::ws::Ws),
    Lp(super::lp::Lp),
}

impl Pipe {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Pipe::Tcp(s) => s.read(b),
            Pipe::Ws(w) => w.read(b),
            Pipe::Lp(l) => l.read(b),
        }
    }

    fn write_all(&mut self, b: &[u8]) -> std::io::Result<()> {
        match self {
            Pipe::Tcp(s) => s.write_all(b),
            Pipe::Ws(w) => w.write_all(b),
            Pipe::Lp(l) => l.write_all(b),
        }
    }

    fn close(&mut self) {
        match self {
            Pipe::Tcp(s) => {
                let _ = s.flush();
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            Pipe::Ws(w) => w.close(),
            Pipe::Lp(l) => l.close(),
        }
    }
}

/// A relay, and how it's reached.
#[derive(Debug, PartialEq)]
pub enum Relay {
    /// A WebSocket (wss://…).
    Ws(String),
    /// HTTPS long polling (https://…/poll): the village's own relay, where
    /// WebSockets don't get through.
    Poll(String),
    /// Plain MQTT over TCP.
    Tcp(String, u16),
}

/// The relays to try, in turn: XMAS_RELAY first if it's set (a ws(s)://
/// or an http(s):// address), then the village's own (WebSocket, then long
/// polling), then the free ones.
pub fn relays() -> Vec<Relay> {
    let mut v = vec![];
    let custom = std::env::var("XMAS_RELAY").ok().or_else(custom_relay);
    if let Some(r) = custom {
        if super::ws::parse(&r).is_some() {
            v.push(if r.starts_with("http") { Relay::Poll(r) } else { Relay::Ws(r) });
        }
    }
    // (the relay's own host: that relay and no other)
    if std::env::var_os("XMAS_RELAY_ONLY").is_some() && !v.is_empty() {
        return v;
    }
    for &(host, port, url) in RELAYS {
        v.push(match port {
            0 if url.starts_with("http") => Relay::Poll(url.to_string()),
            0 => Relay::Ws(url.to_string()),
            _ => Relay::Tcp(host.to_string(), port),
        });
    }
    v
}

/// A relay of the player's own choosing (Menu → Online: a community's
/// own server), tried before the village's.
static CUSTOM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_custom_relay(url: Option<String>) {
    if let Ok(mut c) = CUSTOM.lock() {
        *c = url.filter(|u| super::ws::parse(u).is_some());
    }
}

pub fn custom_relay() -> Option<String> {
    CUSTOM.lock().ok().and_then(|c| c.clone())
}

fn open(relay: &Relay) -> Option<Pipe> {
    let poll = Duration::from_millis(40);
    let timeout = Duration::from_secs(6);
    match relay {
        Relay::Ws(url) => super::ws::open(&super::ws::parse(url)?, timeout, poll).ok().map(Pipe::Ws),
        Relay::Poll(url) => super::lp::open(&super::ws::parse(url)?, timeout, poll).ok().map(Pipe::Lp),
        Relay::Tcp(host, port) => {
            let s = super::ws::connect(host, *port, timeout).ok()?;
            let _ = s.set_read_timeout(Some(poll));
            let _ = s.set_nodelay(true);
            Some(Pipe::Tcp(s))
        }
    }
}

impl Relay {
    /// Where it is and how it's reached, in a few words.
    pub fn describe(&self) -> String {
        let host = |u: &str| super::ws::parse(u).map(|u| u.host).unwrap_or_else(|| u.to_string());
        match self {
            Relay::Ws(u) => format!("{} · WebSocket", host(u)),
            Relay::Poll(u) => format!("{} · HTTPS", host(u)),
            Relay::Tcp(h, p) => format!("{h} · TCP {p}"),
        }
    }
}

fn run(out: Receiver<Vec<u8>>, up: Sender<Up>, stop: Arc<AtomicBool>, first: usize, via: Arc<std::sync::Mutex<String>>) {
    let mut wait = Duration::from_secs(2);
    let list = relays();
    let mut which = first;
    while !stop.load(Ordering::Relaxed) {
        let relay = &list[which % list.len()];
        which += 1;
        let Some(mut s) = open(relay) else {
            if which % list.len() == 0 {
                sleep_watching(&stop, wait);
                wait = (wait * 2).min(Duration::from_secs(60));
            }
            continue;
        };
        // (anything queued for an earlier connection is stale)
        while out.try_recv().is_ok() {}
        if let Ok(mut v) = via.lock() {
            *v = relay.describe();
        }
        if up.send(Up::Connected).is_err() {
            return;
        }
        which -= 1;
        wait = Duration::from_secs(2);
        let up_since = std::time::Instant::now();
        let mut buf = vec![0u8; 16384];
        let closing = 'conn: loop {
            if stop.load(Ordering::Relaxed) {
                break false;
            }
            loop {
                match out.try_recv() {
                    Ok(b) if b.is_empty() => break 'conn true,
                    Ok(b) => {
                        if let Err(e) = s.write_all(&b) {
                            note(&format!("{}: writing: {e}", relay.describe()));
                            break 'conn false;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'conn true,
                }
            }
            match s.read(&mut buf) {
                Ok(0) => {
                    note(&format!("{}: closed by the other end", relay.describe()));
                    break false;
                }
                Ok(n) => {
                    if up.send(Up::Bytes(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {}
                Err(e) => {
                    note(&format!("{}: {e} ({:?})", relay.describe(), e.kind()));
                    break false;
                }
            }
        };
        s.close();
        if let Ok(mut v) = via.lock() {
            v.clear();
        }
        if closing || up.send(Up::Closed).is_err() {
            return;
        }
        // a link that had been up a while: straight back (a blip on the
        // way, most likely); one that wasn't: a moment first
        if up_since.elapsed() < Duration::from_secs(15) {
            sleep_watching(&stop, wait);
        } else {
            sleep_watching(&stop, Duration::from_millis(150));
        }
    }
}

/// Why a link ended, for the log (and XMAS_NET_DEBUG on stderr).
fn note(why: &str) {
    if std::env::var_os("XMAS_NET_DEBUG").is_some() {
        eprintln!("xmas-online: {why}");
    }
}

fn sleep_watching(stop: &AtomicBool, d: Duration) {
    let mut left = d;
    while !stop.load(Ordering::Relaxed) && left > Duration::ZERO {
        let step = left.min(Duration::from_millis(200));
        std::thread::sleep(step);
        left -= step;
    }
}

/// Once a frame: the link up or down as wanted, bytes both ways.
pub fn pump(online: &mut Online, link: &mut Option<Tcp>) {
    if !online.wanted() {
        if let Some(l) = link.take() {
            l.close(online.take_out());
        }
        return;
    }
    // too old for the relay: nowhere, until updated (or it's worth a try)
    if online.too_old() {
        if let Some(l) = link.take() {
            l.close(online.take_out());
        }
        return;
    }
    if online.take_relink() {
        // (another friend code: a new link, with its own goodbye-will)
        if let Some(l) = link.take() {
            l.close(online.take_out());
        }
    }
    // a relay that took the connection but goes nowhere (no answer to the
    // hello, or it turned us down): on to the next
    if link.is_some() && online.link_stuck() {
        link.take();
        online.disconnected();
        online.skip_relay();
    }
    if link.is_none() {
        *link = Tcp::start(online.relay_skip());
    }
    let Some(l) = link else { return };
    for u in l.poll() {
        match u {
            Up::Connected => {
                // (the village's own relay is believed when it speaks as itself)
                // (and one chosen by name: a community's own relay, whose
                // own word is believed there)
                online.set_trusted(l.via().contains("xmas-relay.novalabs.app") || std::env::var_os("XMAS_RELAY").is_some() || custom_relay().is_some_and(|c| l.via().contains(super::ws::parse(&c).map(|u| u.host).unwrap_or_default().as_str())));
                online.connected();
            }
            Up::Bytes(b) => online.feed(&b),
            Up::Closed => online.disconnected(),
        }
    }
    if online.link() != Link::Off {
        l.send(online.take_out());
    } else {
        online.take_out();
    }
}
