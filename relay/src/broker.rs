//! Who's connected, what they listen to, the messages kept for newcomers
//! (retained: "here I am", the shared villages' state), and the limits that
//! keep the relay steady: how many connections (in all, and from one
//! address), how big and how often messages may be, how many
//! subscriptions, how much is kept. Only the village's own topics
//! (`xmas-tui/…`) are carried. What's sent is sealed end to end by the
//! village: the relay passes bytes along without being able to read them.

use crate::codec;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

/// The only topics carried.
pub const PREFIX: &str = "xmas-tui/";

/// The relay's limits (set in the admin console, kept in its data file).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Limits {
    pub max_conns: usize,
    pub max_conns_per_ip: usize,
    /// Biggest message (bytes).
    pub max_payload: usize,
    /// Messages a second one connection may send, kept up (and in a burst).
    pub msgs_per_sec: f32,
    pub burst: f32,
    pub max_subs: usize,
    /// Kept messages: how many, how big each, how much in all.
    pub max_retained: usize,
    pub max_retained_payload: usize,
    pub max_retained_bytes: usize,
    /// Messages waiting to go out to one slow connection before it's dropped.
    pub max_queue: usize,
    /// New connections refused (the relay winding down).
    pub closed: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_conns: 2000,
            max_conns_per_ip: 24,
            max_payload: 128 * 1024,
            msgs_per_sec: 25.0,
            burst: 120.0,
            max_subs: 120,
            max_retained: 30_000,
            max_retained_payload: 32 * 1024,
            max_retained_bytes: 96 * 1024 * 1024,
            max_queue: 2048,
            closed: false,
        }
    }
}

/// What the relay keeps between restarts (its data file).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Kept {
    pub limits: Limits,
    /// Addresses not let in.
    pub bans: Vec<String>,
    /// Each public square's own settings (its name: the village's settings,
    /// as the game has them: time, critters, most_at_once, events,
    /// sleigh_minutes, chattiness, bushes, traffic).
    pub squares: std::collections::BTreeMap<String, serde_json::Value>,
}

pub struct Conn {
    pub ip: IpAddr,
    pub client: String,
    pub since: Instant,
    tx: Sender<Vec<u8>>,
    will: Option<(String, Vec<u8>, bool)>,
    subs: HashSet<String>,
    tokens: f32,
    tokens_at: Instant,
    pub sent: u64,
    pub got: u64,
    pub dropped: u64,
}

struct Kept1 {
    payload: Vec<u8>,
    /// When it was kept (seconds since 1970: it outlasts a restart).
    at: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Where the relay itself says things (the squares' settings): read by
/// everyone, written only by the relay.
pub const ADMIN: &str = "xmas-tui/1/admin/";
/// The public squares' settings, from the console.
pub const ADMIN_SQUARES: &str = "xmas-tui/1/admin/squares";

/// Counts for the admin console.
#[derive(Serialize, Default, Clone, Debug)]
pub struct Stats {
    pub conns: usize,
    pub ips: usize,
    pub subs: usize,
    pub retained: usize,
    pub retained_bytes: usize,
    pub msgs_in: u64,
    pub msgs_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub refused: u64,
    pub dropped: u64,
    pub kicked: u64,
    /// Messages in, a second, over the last minute or so.
    pub rate: f32,
    pub uptime_s: u64,
    /// Kept messages by kind (`xmas-tui/1/<kind>/…`): here, mail, village…
    pub kinds: BTreeMap<String, usize>,
}

/// Why a connection is to be let go.
#[derive(Debug, PartialEq)]
pub enum Bye {
    /// Its own queue filled (it can't keep up).
    Slow,
    /// Over its limits, far and for long.
    Flood,
}

pub struct Broker {
    pub kept: Kept,
    conns: HashMap<u64, Conn>,
    next_id: u64,
    /// Subscriptions: exact topics (most), and filters with wildcards.
    exact: HashMap<String, HashSet<u64>>,
    wild: HashMap<String, HashSet<u64>>,
    retained: HashMap<String, Kept1>,
    retained_bytes: usize,
    order: VecDeque<String>,
    stats: Stats,
    started: Instant,
    recent: VecDeque<(Instant, u64)>,
    bans: HashSet<IpAddr>,
}

impl Broker {
    pub fn new(kept: Kept) -> Self {
        let bans = kept.bans.iter().filter_map(|b| b.parse().ok()).collect();
        Broker { kept, conns: HashMap::new(), next_id: 1, exact: HashMap::new(), wild: HashMap::new(), retained: HashMap::new(), retained_bytes: 0, order: VecDeque::new(), stats: Stats::default(), started: Instant::now(), recent: VecDeque::new(), bans }
    }

    pub fn limits(&self) -> &Limits {
        &self.kept.limits
    }

    /// May a connection from `ip` come in?
    pub fn admit(&mut self, ip: IpAddr) -> Result<(), &'static str> {
        let l = &self.kept.limits;
        let why = if l.closed {
            Some("closed")
        } else if self.bans.contains(&ip) {
            Some("banned")
        } else if self.conns.len() >= l.max_conns {
            Some("full")
        } else if self.conns.values().filter(|c| c.ip == ip).count() >= l.max_conns_per_ip {
            Some("too many from one address")
        } else {
            None
        };
        match why {
            Some(w) => {
                self.stats.refused += 1;
                Err(w)
            }
            None => Ok(()),
        }
    }

    /// A connection that's said hello: its id. (One already there with the
    /// same client name is let go: MQTT's "take over".)
    pub fn join(&mut self, ip: IpAddr, client: String, will: Option<(String, Vec<u8>, bool)>, tx: Sender<Vec<u8>>) -> (u64, Option<u64>) {
        let old = self.conns.iter().find(|(_, c)| c.client == client).map(|(&id, _)| id);
        if let Some(o) = old {
            self.leave(o, false);
        }
        let id = self.next_id;
        self.next_id += 1;
        let will = will.filter(|(t, p, _)| ok_topic(t) && p.len() <= self.kept.limits.max_payload);
        let burst = self.kept.limits.burst;
        self.conns.insert(id, Conn { ip, client, since: Instant::now(), tx, will, subs: HashSet::new(), tokens: burst, tokens_at: Instant::now(), sent: 0, got: 0, dropped: 0 });
        (id, old)
    }

    /// Gone: its subscriptions forgotten, and (if it didn't say goodbye) its
    /// will published.
    pub fn leave(&mut self, id: u64, clean: bool) {
        let Some(c) = self.conns.remove(&id) else { return };
        for f in &c.subs {
            for map in [&mut self.exact, &mut self.wild] {
                if let Some(s) = map.get_mut(f) {
                    s.remove(&id);
                    if s.is_empty() {
                        map.remove(f);
                    }
                }
            }
        }
        if !clean {
            if let Some((t, p, retain)) = c.will {
                self.route(&t, &p, retain);
            }
        }
    }

    /// Is this connection still here?
    pub fn has(&self, id: u64) -> bool {
        self.conns.contains_key(&id)
    }

    /// A message from connection `id`: on to whoever listens (and kept, if
    /// it's to be). Err: let the connection go.
    pub fn publish(&mut self, id: u64, topic: &str, payload: &[u8], retain: bool) -> Result<(), Bye> {
        let l = self.kept.limits.clone();
        let Some(c) = self.conns.get_mut(&id) else { return Ok(()) };
        // (a bucket of tokens, refilled at the rate allowed)
        let now = Instant::now();
        c.tokens = (c.tokens + now.duration_since(c.tokens_at).as_secs_f32() * l.msgs_per_sec).min(l.burst);
        c.tokens_at = now;
        if topic.starts_with(ADMIN) {
            return Ok(());
        }
        if c.tokens < 1.0 || !ok_topic(topic) || payload.len() > l.max_payload {
            c.dropped += 1;
            self.stats.dropped += 1;
            // far over, for long: gone
            if c.dropped > 2000 && c.dropped > c.sent {
                self.stats.kicked += 1;
                return Err(Bye::Flood);
            }
            return Ok(());
        }
        c.tokens -= 1.0;
        c.sent += 1;
        self.stats.msgs_in += 1;
        self.stats.bytes_in += payload.len() as u64;
        self.route(topic, payload, retain);
        Ok(())
    }

    /// On to whoever listens; any that can't keep up (their queue full) are
    /// let go rather than kept in memory.
    fn route(&mut self, topic: &str, payload: &[u8], retain: bool) {
        if retain {
            self.keep(topic, payload);
        }
        let mut to: Vec<u64> = self.exact.get(topic).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for (f, s) in &self.wild {
            if codec::matches(f, topic) {
                to.extend(s.iter().copied());
            }
        }
        to.sort_unstable();
        to.dedup();
        let bytes = codec::publish(topic, payload, false);
        let mut slow = vec![];
        for id in to {
            if let Some(c) = self.conns.get_mut(&id) {
                if c.tx.try_send(bytes.clone()).is_ok() {
                    c.got += 1;
                    self.stats.msgs_out += 1;
                    self.stats.bytes_out += bytes.len() as u64;
                } else {
                    slow.push(id);
                }
            }
        }
        self.recent.push_back((Instant::now(), 1));
        while self.recent.front().is_some_and(|(t, _)| t.elapsed().as_secs() > 60) {
            self.recent.pop_front();
        }
        self.stats.kicked += slow.len() as u64;
        for s in slow {
            self.leave(s, false);
        }
    }

    fn keep(&mut self, topic: &str, payload: &[u8]) {
        if let Some(old) = self.retained.remove(topic) {
            self.retained_bytes -= old.payload.len();
        }
        let l = &self.kept.limits;
        if payload.is_empty() || payload.len() > l.max_retained_payload {
            return;
        }
        self.retained_bytes += payload.len();
        self.retained.insert(topic.to_string(), Kept1 { payload: payload.to_vec(), at: now_secs() });
        self.order.push_back(topic.to_string());
        // the oldest let go when there's too much
        while self.retained.len() > l.max_retained || self.retained_bytes > l.max_retained_bytes {
            let Some(t) = self.order.pop_front() else { break };
            if let Some(k) = self.retained.remove(&t) {
                self.retained_bytes -= k.payload.len();
            }
        }
        if self.order.len() > self.retained.len() * 2 + 64 {
            let r = &self.retained;
            self.order.retain(|t| r.contains_key(t));
        }
    }

    /// Listen to `filters`; the kept messages for them go out at once. Each
    /// filter's granted QoS (0), or 0x80 for one refused.
    pub fn subscribe(&mut self, id: u64, filters: &[String]) -> Vec<u8> {
        let max = self.kept.limits.max_subs;
        let mut granted = vec![];
        let mut fresh = vec![];
        let Some(c) = self.conns.get_mut(&id) else { return vec![0x80; filters.len()] };
        for f in filters {
            if !ok_filter(f) || (c.subs.len() >= max && !c.subs.contains(f)) {
                granted.push(0x80);
                continue;
            }
            granted.push(0);
            if c.subs.insert(f.clone()) {
                let map = if f.contains(['+', '#']) { &mut self.wild } else { &mut self.exact };
                map.entry(f.clone()).or_default().insert(id);
                fresh.push(f.clone());
            }
        }
        // what's kept, for what's newly listened to
        let mut out = vec![];
        for f in &fresh {
            if f.contains(['+', '#']) {
                for (t, k) in &self.retained {
                    if codec::matches(f, t) {
                        out.push(codec::publish(t, &k.payload, true));
                    }
                }
            } else if let Some(k) = self.retained.get(f) {
                out.push(codec::publish(f, &k.payload, true));
            }
        }
        if let Some(c) = self.conns.get_mut(&id) {
            for b in out {
                if c.tx.try_send(b).is_err() {
                    break;
                }
            }
        }
        granted
    }

    pub fn unsubscribe(&mut self, id: u64, filters: &[String]) {
        let Some(c) = self.conns.get_mut(&id) else { return };
        for f in filters {
            if c.subs.remove(f) {
                for map in [&mut self.exact, &mut self.wild] {
                    if let Some(s) = map.get_mut(f) {
                        s.remove(&id);
                        if s.is_empty() {
                            map.remove(f);
                        }
                    }
                }
            }
        }
    }

    /// Something straight back to one connection (an ack, a pong).
    pub fn reply(&mut self, id: u64, bytes: Vec<u8>) -> Result<(), Bye> {
        let Some(c) = self.conns.get_mut(&id) else { return Ok(()) };
        if c.tx.try_send(bytes).is_err() {
            return Err(Bye::Slow);
        }
        Ok(())
    }

    // ---------------- the admin console

    pub fn stats(&self) -> Stats {
        let mut s = self.stats.clone();
        s.conns = self.conns.len();
        s.ips = self.conns.values().map(|c| c.ip).collect::<HashSet<_>>().len();
        s.subs = self.conns.values().map(|c| c.subs.len()).sum();
        s.retained = self.retained.len();
        s.retained_bytes = self.retained_bytes;
        s.uptime_s = self.started.elapsed().as_secs();
        let span = self.recent.front().map(|(t, _)| t.elapsed().as_secs_f32()).unwrap_or(0.0).max(1.0);
        s.rate = self.recent.len() as f32 / span;
        for t in self.retained.keys() {
            let kind = t.split('/').nth(2).unwrap_or("?").to_string();
            *s.kinds.entry(kind).or_default() += 1;
        }
        s
    }

    /// The connections, for the console: (id, address, client, seconds
    /// connected, subscriptions, sent, received, dropped).
    pub fn conns(&self) -> Vec<(u64, String, String, u64, usize, u64, u64, u64)> {
        let mut v: Vec<_> = self.conns.iter().map(|(&id, c)| (id, c.ip.to_string(), c.client.clone(), c.since.elapsed().as_secs(), c.subs.len(), c.sent, c.got, c.dropped)).collect();
        v.sort_by_key(|c| c.0);
        v
    }

    /// Let a connection go (the console's "kick"); its will is published.
    pub fn kick(&mut self, id: u64) -> bool {
        if self.conns.contains_key(&id) {
            self.stats.kicked += 1;
            self.leave(id, false);
            true
        } else {
            false
        }
    }

    /// Not let in again (and those connected from it let go).
    pub fn ban(&mut self, ip: IpAddr) {
        self.bans.insert(ip);
        if !self.kept.bans.contains(&ip.to_string()) {
            self.kept.bans.push(ip.to_string());
        }
        let ids: Vec<u64> = self.conns.iter().filter(|(_, c)| c.ip == ip).map(|(&id, _)| id).collect();
        for id in ids {
            self.kick(id);
        }
    }

    pub fn unban(&mut self, ip: IpAddr) {
        self.bans.remove(&ip);
        self.kept.bans.retain(|b| *b != ip.to_string());
    }

    /// Forget kept messages whose topic starts with `prefix`: how many.
    pub fn clear_retained(&mut self, prefix: &str) -> usize {
        let gone: Vec<String> = self.retained.keys().filter(|t| t.starts_with(prefix)).cloned().collect();
        for t in &gone {
            if let Some(k) = self.retained.remove(t) {
                self.retained_bytes -= k.payload.len();
            }
        }
        gone.len()
    }

    /// The relay's own message (the squares' settings): kept, and on to
    /// whoever listens.
    pub fn say(&mut self, topic: &str, payload: &[u8]) {
        self.keep(topic, payload);
        self.route(topic, payload, true);
    }

    /// The squares' settings, as they are now, for everyone.
    pub fn say_squares(&mut self) {
        let json = serde_json::to_vec(&self.kept.squares).unwrap_or_default();
        self.say(ADMIN_SQUARES, &json);
    }

    /// What's kept, to write out (so a restart or an update loses
    /// nothing): each topic, when, and its bytes.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = b"xmas-relay kept 1\n".to_vec();
        for (t, k) in &self.retained {
            out.extend_from_slice(&(t.len() as u16).to_le_bytes());
            out.extend_from_slice(t.as_bytes());
            out.extend_from_slice(&k.at.to_le_bytes());
            out.extend_from_slice(&(k.payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&k.payload);
        }
        out
    }

    /// Kept messages back from a snapshot (anything malformed: skipped
    /// from there on). Returns how many.
    pub fn restore(&mut self, b: &[u8]) -> usize {
        let head = b"xmas-relay kept 1\n";
        let Some(mut r) = b.strip_prefix(head.as_slice()) else { return 0 };
        let mut n = 0;
        let take = |r: &mut &[u8], k: usize| -> Option<Vec<u8>> {
            if r.len() < k {
                return None;
            }
            let (a, rest) = r.split_at(k);
            *r = rest;
            Some(a.to_vec())
        };
        while !r.is_empty() {
            let Some(tl) = take(&mut r, 2).map(|v| u16::from_le_bytes([v[0], v[1]]) as usize) else { break };
            let Some(topic) = take(&mut r, tl).and_then(|v| String::from_utf8(v).ok()) else { break };
            let Some(at) = take(&mut r, 8).map(|v| u64::from_le_bytes(v.try_into().unwrap_or([0; 8]))) else { break };
            let Some(pl) = take(&mut r, 4).map(|v| u32::from_le_bytes(v.try_into().unwrap_or([0; 4])) as usize) else { break };
            let Some(payload) = take(&mut r, pl) else { break };
            if !ok_topic(&topic) {
                continue;
            }
            self.keep(&topic, &payload);
            if let Some(k) = self.retained.get_mut(&topic) {
                k.at = at;
            }
            n += 1;
        }
        n
    }

    /// Kept messages older than `secs` forgotten (a cleanup now and then:
    /// nothing the village keeps is meant to last for ever).
    /// (A linked device's side lasts longer: a phone left in a drawer for a
    /// month still catches up; the relay's own are kept for ever.)
    pub fn expire(&mut self, secs: u64) {
        let now = now_secs();
        let old: Vec<String> = self
            .retained
            .iter()
            .filter(|(t, k)| {
                let keep = if t.starts_with(ADMIN) { u64::MAX } else if t.split('/').nth(2) == Some("sync") { secs.saturating_mul(9) } else { secs };
                now.saturating_sub(k.at) > keep
            })
            .map(|(t, _)| t.clone())
            .collect();
        for t in old {
            if let Some(k) = self.retained.remove(&t) {
                self.retained_bytes -= k.payload.len();
            }
        }
    }
}

/// A topic the village uses: its prefix, no wildcards, nothing odd.
pub fn ok_topic(t: &str) -> bool {
    t.starts_with(PREFIX) && t.len() <= 200 && !t.contains(['+', '#', '\0'])
}

/// A subscription the village makes: its prefix, wildcards only as whole
/// levels (`#` last).
pub fn ok_filter(f: &str) -> bool {
    if !f.starts_with(PREFIX) || f.len() > 200 || f.contains('\0') {
        return false;
    }
    let levels: Vec<&str> = f.split('/').collect();
    levels.iter().enumerate().all(|(i, l)| match *l {
        "+" => true,
        "#" => i == levels.len() - 1,
        l => !l.contains(['+', '#']),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{channel, Receiver};

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, n])
    }

    fn drain(rx: &mut Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
        let mut v = vec![];
        while let Ok(b) = rx.try_recv() {
            v.push(b);
        }
        v
    }

    /// What's kept outlasts a restart; nobody but the relay speaks on its
    /// own topics; a linked device's side is kept longer than the rest.
    #[test]
    fn kept_messages_outlast_a_restart_and_only_the_relay_speaks_as_itself() {
        let mut b = Broker::new(Kept::default());
        let (ta, _ra) = channel(64);
        let (a, _) = b.join(ip(1), "a".into(), None, ta);
        b.publish(a, "xmas-tui/1/sync/abc/dev1", b"my side", true).unwrap();
        b.publish(a, "xmas-tui/1/here/abc", b"on", true).unwrap();
        // pretending to be the relay: not kept, not passed on
        b.publish(a, ADMIN_SQUARES, b"{\"North Pole Square\":{\"time\":\"noon\"}}", true).unwrap();
        b.kept.squares.insert("Frosty Hollow".into(), serde_json::json!({ "time": "night" }));
        b.say_squares();
        let snap = b.snapshot();
        // a new relay, from the snapshot
        let mut c = Broker::new(Kept::default());
        assert_eq!(c.restore(&snap), 3);
        assert_eq!(c.restore(b"not a snapshot"), 0);
        let (tb, mut rb) = channel(64);
        let (bb, _) = c.join(ip(2), "b".into(), None, tb);
        c.subscribe(bb, &["xmas-tui/1/#".into()]);
        let got = drain(&mut rb);
        let has = |t: &str| got.iter().any(|m| m.windows(t.len()).any(|w| w == t.as_bytes()));
        assert!(has("xmas-tui/1/sync/abc/dev1") && has("xmas-tui/1/here/abc"));
        assert!(has("Frosty Hollow") && !has("North Pole Square"), "the relay's squares, not the pretender's");
        // nine days on: the "here" is forgotten, the device's side isn't
        for k in c.retained.values_mut() {
            k.at -= 9 * 24 * 3600;
        }
        c.expire(7 * 24 * 3600);
        assert!(c.retained.contains_key("xmas-tui/1/sync/abc/dev1"));
        assert!(!c.retained.contains_key("xmas-tui/1/here/abc"));
        assert!(c.retained.contains_key(ADMIN_SQUARES));
    }

    #[test]
    fn messages_reach_who_listens_and_kept_ones_reach_newcomers() {
        let mut b = Broker::new(Kept::default());
        let (ta, mut ra) = channel(64);
        let (tb, mut rb) = channel(64);
        let (a, _) = b.join(ip(1), "a".into(), Some(("xmas-tui/1/here/a".into(), b"gone".to_vec(), true)), ta);
        let (bb, _) = b.join(ip(2), "b".into(), None, tb);
        assert_eq!(b.subscribe(bb, &["xmas-tui/1/mail/b".into(), "xmas-tui/1/here/+".into(), "nope/x".into()]), vec![0, 0, 0x80]);
        b.publish(a, "xmas-tui/1/mail/b", b"hi", false).unwrap();
        b.publish(a, "xmas-tui/1/here/a", b"on", true).unwrap();
        assert_eq!(drain(&mut rb), vec![codec::publish("xmas-tui/1/mail/b", b"hi", false), codec::publish("xmas-tui/1/here/a", b"on", false)]);
        // someone new hears the kept "here"
        let (tc, mut rc) = channel(64);
        let (c, _) = b.join(ip(3), "c".into(), None, tc);
        b.subscribe(c, &["xmas-tui/1/here/a".into()]);
        assert_eq!(drain(&mut rc), vec![codec::publish("xmas-tui/1/here/a", b"on", true)]);
        // a drops without a goodbye: its will goes out (and is kept)
        b.leave(a, false);
        assert_eq!(drain(&mut rc), vec![codec::publish("xmas-tui/1/here/a", b"gone", false)]);
        assert!(drain(&mut ra).is_empty());
        let st = b.stats();
        assert_eq!((st.conns, st.retained), (2, 1));
        assert_eq!(st.kinds.get("here"), Some(&1));
    }

    #[test]
    fn limits_hold() {
        let mut k = Kept::default();
        k.limits.max_conns_per_ip = 2;
        k.limits.burst = 5.0;
        k.limits.msgs_per_sec = 0.001;
        let mut b = Broker::new(k);
        assert!(b.admit(ip(1)).is_ok());
        let (t1, _r1) = channel(4);
        let (t2, _r2) = channel(4);
        let (a, _) = b.join(ip(1), "a".into(), None, t1);
        b.join(ip(1), "a2".into(), None, t2);
        assert!(b.admit(ip(1)).is_err(), "two from one address at most");
        assert!(b.admit(ip(2)).is_ok());
        // a flood: past the burst, dropped (not passed on)
        for _ in 0..20 {
            b.publish(a, "xmas-tui/1/x", b"x", false).unwrap();
        }
        assert_eq!(b.stats().msgs_in, 5);
        // off-limits topics never pass
        let (t3, _r3) = channel(4);
        let (c, _) = b.join(ip(3), "c".into(), None, t3);
        b.publish(c, "elsewhere/x", b"x", false).unwrap();
        b.publish(c, "xmas-tui/1/+", b"x", false).unwrap();
        assert_eq!(b.stats().msgs_in, 5);
        // a ban lets them go, and keeps them out
        b.ban(ip(1));
        assert!(!b.has(a) && b.admit(ip(1)).is_err());
        b.unban(ip(1));
        assert!(b.admit(ip(1)).is_ok());
    }

    #[test]
    fn one_that_cannot_keep_up_is_let_go() {
        let mut b = Broker::new(Kept::default());
        let (ta, _ra) = channel(64);
        let (tb, _rb) = channel(2);
        let (a, _) = b.join(ip(1), "a".into(), None, ta);
        let (bb, _) = b.join(ip(2), "b".into(), None, tb);
        b.subscribe(bb, &["xmas-tui/1/t".into()]);
        for _ in 0..5 {
            b.publish(a, "xmas-tui/1/t", b"x", false).unwrap();
        }
        assert!(!b.has(bb), "the slow one's gone");
        assert!(b.has(a), "not the one sending");
    }

    #[test]
    fn kept_messages_stay_within_bounds() {
        let mut k = Kept::default();
        k.limits.max_retained = 3;
        let mut b = Broker::new(k);
        let (t, _r) = channel(64);
        let (a, _) = b.join(ip(1), "a".into(), None, t);
        for i in 0..10 {
            b.publish(a, &format!("xmas-tui/1/here/{i}"), b"x", true).unwrap();
        }
        assert_eq!(b.stats().retained, 3);
        // an empty one clears
        b.publish(a, "xmas-tui/1/here/9", b"", true).unwrap();
        assert_eq!(b.stats().retained, 2);
        assert_eq!(b.clear_retained("xmas-tui/1/here/"), 2);
    }

    #[test]
    fn filters_and_topics_are_checked() {
        assert!(ok_filter("xmas-tui/1/sync/ab/+") && ok_filter("xmas-tui/#"));
        assert!(!ok_filter("#") && !ok_filter("xmas-tui/1/#/x") && !ok_filter("xmas-tui/1/a+b"));
        assert!(ok_topic("xmas-tui/1/here/x") && !ok_topic("xmas-tui/1/here/+") && !ok_topic("other/x"));
    }
}
