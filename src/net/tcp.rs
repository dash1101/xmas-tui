//! The link to the relay over TCP (the desktop and the Android app): a
//! thread that connects (each relay in turn, backing off when none will),
//! and passes bytes both ways. What's sent is sealed already (see `seal`).

use super::{Link, Online, RELAYS};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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
}

impl Tcp {
    pub fn start() -> Option<Tcp> {
        let (tx, out_rx) = channel::<Vec<u8>>();
        let (up_tx, rx) = channel::<Up>();
        let stop = Arc::new(AtomicBool::new(false));
        let st = stop.clone();
        std::thread::Builder::new()
            .name("xmas-online".into())
            .spawn(move || run(out_rx, up_tx, st))
            .ok()?;
        Some(Tcp { tx, rx, stop })
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

fn run(out: Receiver<Vec<u8>>, up: Sender<Up>, stop: Arc<AtomicBool>) {
    let mut wait = Duration::from_secs(2);
    let mut which = 0usize;
    while !stop.load(Ordering::Relaxed) {
        let (host, port, _) = RELAYS[which % RELAYS.len()];
        which += 1;
        let addr = (host, port).to_socket_addrs().ok().and_then(|mut a| a.next());
        let Some(addr) = addr else {
            sleep_watching(&stop, wait);
            continue;
        };
        let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_secs(6)) else {
            if which % RELAYS.len() == 0 {
                sleep_watching(&stop, wait);
                wait = (wait * 2).min(Duration::from_secs(60));
            }
            continue;
        };
        let _ = s.set_read_timeout(Some(Duration::from_millis(40)));
        let _ = s.set_nodelay(true);
        // (anything queued for an earlier connection is stale)
        while out.try_recv().is_ok() {}
        if up.send(Up::Connected).is_err() {
            return;
        }
        which -= 1;
        wait = Duration::from_secs(2);
        let mut buf = vec![0u8; 16384];
        let closing = 'conn: loop {
            if stop.load(Ordering::Relaxed) {
                break false;
            }
            loop {
                match out.try_recv() {
                    Ok(b) if b.is_empty() => {
                        let _ = s.flush();
                        break 'conn true;
                    }
                    Ok(b) => {
                        if s.write_all(&b).is_err() {
                            break 'conn false;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'conn true,
                }
            }
            match s.read(&mut buf) {
                Ok(0) => break false,
                Ok(n) => {
                    if up.send(Up::Bytes(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {}
                Err(_) => break false,
            }
        };
        let _ = s.shutdown(std::net::Shutdown::Both);
        if closing || up.send(Up::Closed).is_err() {
            return;
        }
        sleep_watching(&stop, wait);
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
    if online.take_relink() {
        // (another friend code: a new link, with its own goodbye-will)
        if let Some(l) = link.take() {
            l.close(online.take_out());
        }
    }
    if link.is_none() {
        *link = Tcp::start();
    }
    let Some(l) = link else { return };
    for u in l.poll() {
        match u {
            Up::Connected => online.connected(),
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
