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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Kept {
    pub limits: Limits,
    /// Addresses not let in.
    pub bans: Vec<String>,
    /// Each public square's own settings (its name: the village's settings,
    /// as the game has them: time, critters, most_at_once, events,
    /// sleigh_minutes, chattiness, bushes, traffic).
    pub squares: std::collections::BTreeMap<String, serde_json::Value>,
    /// The relay's own host hosts the public squares (else the first
    /// player in does, as ever).
    pub host_squares: bool,
    /// Villages the relay's own host keeps open (not a player's game).
    pub servers: Vec<ServerVillage>,
    /// Villages kept out (by code), and players (by id, as hex).
    pub village_bans: Vec<Ban>,
    pub player_bans: Vec<Ban>,
    /// The public list: first (featured), then in this order (names or
    /// codes); the rest after, busiest first.
    pub featured: Vec<String>,
    pub order: Vec<String>,
    /// Words starred out in the public villages, on top of the game's own.
    pub words: Vec<String>,
    /// Who can't talk in a village the host keeps (by its name).
    pub muted: BTreeMap<String, Vec<u32>>,
    /// Said to whoever comes into a village the host keeps (by its name).
    pub welcome: BTreeMap<String, String>,
    /// Announcements to come.
    pub schedules: Vec<Schedule>,
    /// Players kept out of one village (by the village's code): by their
    /// id, their tag, or their address ("ip:…").
    pub keepouts: BTreeMap<String, Vec<Ban>>,
}

impl Default for Kept {
    fn default() -> Self {
        Kept {
            limits: Limits::default(),
            bans: vec![],
            squares: BTreeMap::new(),
            host_squares: true,
            servers: vec![],
            village_bans: vec![],
            player_bans: vec![],
            featured: vec![],
            order: vec![],
            words: vec![],
            muted: BTreeMap::new(),
            welcome: BTreeMap::new(),
            schedules: vec![],
            keepouts: BTreeMap::new(),
        }
    }
}

/// A village the relay's own host keeps open: its name, its code (a
/// private one's is shared by whoever's given it), listed for anyone or
/// not, the most it takes, its village (seed), and its settings (as the
/// squares' are).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct ServerVillage {
    pub name: String,
    pub code: String,
    pub public: bool,
    pub limit: u16,
    pub seed: u32,
    pub rules: serde_json::Value,
}

/// Kept out: what (a village's code, a player's id), who it was (as they
/// called themselves), why, when.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Ban {
    pub key: String,
    pub name: String,
    pub why: String,
    pub at: u64,
}

/// An announcement to come: what, where ("*": every village the host
/// keeps; else one by name), when (seconds since 1970), and again every
/// so many seconds (0: once).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Schedule {
    pub id: u32,
    pub text: String,
    pub to: String,
    pub at: u64,
    pub every: u64,
    pub on: bool,
}

pub struct Conn {
    pub ip: IpAddr,
    pub client: String,
    /// The relay's own host (signed in with its token).
    pub host: bool,
    /// What it said it is (the app's version and system; "web": a
    /// browser), for the console.
    pub agent: String,
    /// The country its address is in, as Cloudflare has it (two letters),
    /// for the console.
    pub country: String,
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

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Where the relay itself says things (the squares' settings): read by
/// everyone, written only by the relay.
pub const ADMIN: &str = "xmas-tui/1/admin/";
/// The public squares' settings, from the console.
pub const ADMIN_SQUARES: &str = "xmas-tui/1/admin/squares";
/// The public list (its order, what's featured, what's kept out) and the
/// public villages' extra words, from the console, for everyone.
pub const ADMIN_PUBLIC: &str = "xmas-tui/1/admin/public";
/// For the relay's own host only: what it keeps open and how (kept), and
/// what it's asked to do (kick, mute, announce).
pub const ADMIN_HOST: &str = "xmas-tui/1/admin/host";
pub const ADMIN_CMD: &str = "xmas-tui/1/admin/cmd";
/// From the host only: who's in each village it keeps, what's said there.
pub const ADMIN_REPORT: &str = "xmas-tui/1/admin/report/";
/// The host's own word in a village (`…/v/<the village's topic id>`): only
/// the host may speak there, so nobody can pass for it; anyone listens.
pub const ADMIN_V: &str = "xmas-tui/1/admin/v/";
/// Player ids below this are the relay's own host's.
pub const HOST_IDS: u32 = 0x1000;

/// Topics only the relay's own host may hear.
pub fn host_only(t: &str) -> bool {
    t == ADMIN_HOST || t == ADMIN_CMD || t.starts_with(ADMIN_REPORT)
}

/// A village's topic, as the game makes it from its code (`kind`:
/// "village", "public"…): to keep a banned one out.
pub fn topic_of(kind: &str, code: &str) -> String {
    use sha2::{Digest, Sha256};
    let norm: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    let mut h = Sha256::new();
    for p in [&b"xmas-tui topic"[..], kind.as_bytes(), norm.as_bytes()] {
        h.update((p.len() as u32).to_le_bytes());
        h.update(p);
    }
    let d = h.finalize();
    let hex: String = d[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("xmas-tui/1/{kind}/{hex}")
}

/// A village's code as typed, tidied (letters and numbers, capitals).
pub fn norm_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).take(32).collect()
}

/// Is this connection one of those kept out (by its player id, its tag,
/// or its address)? (Never the relay's own host.)
fn kept_out_of(keys: &HashSet<String>, c: &Conn) -> bool {
    if c.host {
        return false;
    }
    if keys.contains(&format!("ip:{}", block_of(c.ip))) {
        return true;
    }
    if player_of(&c.client).is_some_and(|id| keys.contains(&format!("{id:08x}"))) {
        return true;
    }
    tag_of(&c.client).is_some_and(|t| keys.contains(&format!("tag:{}", t.to_ascii_lowercase())))
}

/// The player id in a client's name ("xt" and eight hex digits), if any.
pub fn player_of(client: &str) -> Option<u32> {
    client.strip_prefix("xt").and_then(|r| r.get(..8)).and_then(|h| u32::from_str_radix(h, 16).ok())
}

/// The player's tag in a client's name (the eight hex digits after the
/// id: the same each time a villager comes, whatever their id that day).
pub fn tag_of(client: &str) -> Option<&str> {
    client.strip_prefix("xt").and_then(|r| r.get(8..16)).filter(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

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
    /// Topics of the villages kept out.
    blocked: HashSet<String>,
    /// Topics of the villages some players are kept out of, and who.
    kept_out: HashMap<String, HashSet<String>>,
    /// The host's last word on each village it keeps (when, what).
    pub reports: BTreeMap<String, (Instant, serde_json::Value)>,
}

impl Broker {
    pub fn new(kept: Kept) -> Self {
        let bans = kept.bans.iter().filter_map(|b| b.parse().ok()).map(block_of).collect();
        let mut b = Broker { kept, conns: HashMap::new(), next_id: 1, exact: HashMap::new(), wild: HashMap::new(), retained: HashMap::new(), retained_bytes: 0, order: VecDeque::new(), stats: Stats::default(), started: Instant::now(), recent: VecDeque::new(), bans, blocked: HashSet::new(), kept_out: HashMap::new(), reports: BTreeMap::new() };
        b.reblock();
        b.rekeep();
        b
    }

    /// The topics of the villages some players are kept out of (the
    /// village's own, and its official twin), from their codes.
    fn rekeep(&mut self) {
        self.kept_out.clear();
        for (code, bans) in &self.kept.keepouts {
            let keys: HashSet<String> = bans.iter().map(|b| b.key.clone()).collect();
            if keys.is_empty() {
                continue;
            }
            let v = topic_of("village", code);
            let twin = format!("{ADMIN_V}{}", v.rsplit('/').next().unwrap_or(""));
            self.kept_out.insert(v, keys.clone());
            self.kept_out.insert(twin, keys);
        }
    }

    /// The banned villages' topics, from their codes.
    fn reblock(&mut self) {
        self.blocked = self.kept.village_bans.iter().flat_map(|v| ["village", "public"].map(|k| topic_of(k, &v.key))).collect();
        let more: Vec<String> = self.blocked.iter().filter(|t| t.starts_with("xmas-tui/1/village/")).map(|t| format!("{ADMIN_V}{}", t.rsplit('/').next().unwrap_or(""))).collect();
        self.blocked.extend(more);
    }

    /// May this client come in (by the player id in its name)? The host's
    /// ids are the host's alone; a banned player's aren't let in.
    pub fn may_join(&self, client: &str, host: bool) -> Result<(), &'static str> {
        let Some(id) = player_of(client) else { return Ok(()) };
        if id < HOST_IDS && !host {
            return Err("that id is the relay's own");
        }
        let tag = tag_of(client).map(|t| format!("tag:{}", t.to_ascii_lowercase()));
        if self.kept.player_bans.iter().any(|b| b.key == format!("{id:08x}") || Some(&b.key) == tag.as_ref()) {
            return Err("banned");
        }
        Ok(())
    }

    pub fn limits(&self) -> &Limits {
        &self.kept.limits
    }

    /// May a connection from `ip` come in?
    pub fn admit(&mut self, ip: IpAddr) -> Result<(), &'static str> {
        let l = &self.kept.limits;
        let why = if l.closed {
            Some("closed")
        } else if self.bans.contains(&block_of(ip)) {
            Some("banned")
        } else if self.conns.len() >= l.max_conns {
            Some("full")
        } else if !internal(ip) && self.conns.values().filter(|c| block_of(c.ip) == block_of(ip)).count() >= l.max_conns_per_ip {
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
        self.join_as(ip, client, will, tx, false)
    }

    /// The same, for the relay's own host (`host`: signed in).
    pub fn join_as(&mut self, ip: IpAddr, client: String, will: Option<(String, Vec<u8>, bool)>, tx: Sender<Vec<u8>>, host: bool) -> (u64, Option<u64>) {
        let old = self.conns.iter().find(|(_, c)| c.client == client).map(|(&id, _)| id);
        if let Some(o) = old {
            self.leave(o, false);
        }
        let id = self.next_id;
        self.next_id += 1;
        // (a will is published by the relay itself: never on its own topics)
        let will = will.filter(|(t, p, _)| ok_topic(t) && !t.starts_with(ADMIN) && p.len() <= self.kept.limits.max_payload);
        let burst = self.kept.limits.burst;
        self.conns.insert(id, Conn { ip, client, host, agent: String::new(), country: String::new(), since: Instant::now(), tx, will, subs: HashSet::new(), tokens: burst, tokens_at: Instant::now(), sent: 0, got: 0, dropped: 0 });
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
        // the relay's own topics: only its host speaks there, and only in
        // its villages (`v/`) and to the relay (`report/`)
        if topic.starts_with(ADMIN) {
            if !c.host {
                return Ok(());
            }
            if let Some(slot) = topic.strip_prefix(ADMIN_REPORT) {
                if payload.len() <= 256 * 1024 && slot.len() <= 64 {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(payload) {
                        self.reports.insert(slot.to_string(), (Instant::now(), v));
                        // (a village closed: its last report goes after a while)
                        self.reports.retain(|_, (at, _)| at.elapsed().as_secs() < 120);
                    }
                }
                return Ok(());
            }
            if !topic.starts_with(ADMIN_V) {
                return Ok(());
            }
        }
        // a banned village, or one this player's kept out of: nothing
        // through (counted against them like any message turned down, so
        // one who floods it is still let go)
        let shut = self.blocked.contains(topic) || self.kept_out.get(topic).is_some_and(|keys| kept_out_of(keys, c));
        if shut || c.tokens < 1.0 || !ok_topic(topic) || payload.len() > l.max_payload {
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
        let only_host = host_only(topic);
        // (someone kept out of this village hears nothing of it, however
        // they're listening)
        let outs = self.kept_out.get(topic).cloned();
        for id in to {
            if let Some(c) = self.conns.get_mut(&id) {
                if only_host && !c.host {
                    continue;
                }
                if outs.as_ref().is_some_and(|keys| kept_out_of(keys, c)) {
                    continue;
                }
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
        let host = c.host;
        for f in filters {
            if !ok_filter(f) || (c.subs.len() >= max && !c.subs.contains(f)) || (host_only(f) && !host) || self.kept_out.get(f).is_some_and(|keys| kept_out_of(keys, c)) {
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
                    let kept_out = self.kept_out.get(t).is_some_and(|keys| self.conns.get(&id).is_some_and(|c| kept_out_of(keys, c)));
                    if codec::matches(f, t) && (host || !host_only(t)) && !kept_out {
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
    /// connected, subscriptions, sent, received, dropped, what it said it
    /// is, the relay's own host or not).
    #[allow(clippy::type_complexity)]
    pub fn conns(&self) -> Vec<(u64, String, String, u64, usize, u64, u64, u64, String, bool, String)> {
        let mut v: Vec<_> = self.conns.iter().map(|(&id, c)| (id, c.ip.to_string(), c.client.clone(), c.since.elapsed().as_secs(), c.subs.len(), c.sent, c.got, c.dropped, c.agent.clone(), c.host, c.country.clone())).collect();
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

    /// Not let in again (and those connected from it let go). An IPv6
    /// address stands for its whole block (see `block_of`).
    pub fn ban(&mut self, ip: IpAddr) {
        let ip = block_of(ip);
        self.bans.insert(ip);
        if !self.kept.bans.contains(&ip.to_string()) {
            self.kept.bans.push(ip.to_string());
        }
        let ids: Vec<u64> = self.conns.iter().filter(|(_, c)| block_of(c.ip) == ip).map(|(&id, _)| id).collect();
        for id in ids {
            self.kick(id);
        }
    }

    pub fn unban(&mut self, ip: IpAddr) {
        // (as it was typed, and as its block: one kept before blocks were)
        let block = block_of(ip);
        self.bans.remove(&ip);
        self.bans.remove(&block);
        self.kept.bans.retain(|b| *b != ip.to_string() && *b != block.to_string());
    }

    /// What a connection said it is (the app's version and system), for
    /// the console.
    pub fn set_agent(&mut self, id: u64, agent: &str, country: &str) {
        if let Some(c) = self.conns.get_mut(&id) {
            c.agent = agent.chars().filter(|ch| !ch.is_control()).take(48).collect();
            c.country = country.chars().filter(|ch| ch.is_ascii_alphanumeric()).take(2).collect();
        }
    }

    /// Where a player's connected from (address, country), and with what
    /// (for the console).
    pub fn where_from(&self, player: u32) -> Option<(String, String, String)> {
        self.conns.values().find(|c| !c.host && player_of(&c.client) == Some(player)).map(|c| (c.ip.to_string(), c.country.clone(), c.agent.clone()))
    }

    /// The villages in use right now, by their topic: who's listening on
    /// each (address, client, what it is, the relay's own host or not,
    /// country), the fullest first. A private village shows only as its topic: the
    /// relay can't tell more about it.
    #[allow(clippy::type_complexity)]
    pub fn villages(&self) -> Vec<(String, Vec<(String, String, String, bool, String)>)> {
        let mut v: Vec<(String, Vec<(String, String, String, bool, String)>)> = self
            .exact
            .iter()
            .filter(|(t, _)| t.starts_with("xmas-tui/1/village/"))
            .map(|(t, ids)| {
                let mut who: Vec<(String, String, String, bool, String)> = ids.iter().filter_map(|id| self.conns.get(id)).map(|c| (c.ip.to_string(), c.client.clone(), c.agent.clone(), c.host, c.country.clone())).collect();
                who.sort();
                (t.clone(), who)
            })
            .filter(|(_, who)| !who.is_empty())
            .collect();
        v.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        v
    }

    /// A player kept out of one village (by its code): by their id, by
    /// their tag if they're connected (the same next time they come), and
    /// by their address too if asked (never one on the relay's own
    /// network: that would be the host, or a proxy with everyone behind
    /// it). What's said there no longer reaches them, nor what they say
    /// anyone there; the rest of the relay is theirs as before.
    pub fn keep_out(&mut self, code: &str, id: u32, name: &str, why: &str, with_ip: bool) {
        let code = norm_code(code);
        if code.is_empty() {
            return;
        }
        let mut keys = vec![format!("{id:08x}")];
        for c in self.conns.values().filter(|c| !c.host && player_of(&c.client) == Some(id)) {
            if let Some(t) = tag_of(&c.client).map(|t| format!("tag:{}", t.to_ascii_lowercase())) {
                if !keys.contains(&t) {
                    keys.push(t);
                }
            }
            let k = format!("ip:{}", block_of(c.ip));
            if with_ip && !internal(c.ip) && !keys.contains(&k) {
                keys.push(k);
            }
        }
        let at = now_secs();
        let list = self.kept.keepouts.entry(code).or_default();
        for key in keys {
            if !list.iter().any(|b| b.key == key) && list.len() < 500 {
                list.push(Ban { key, name: name.chars().take(60).collect(), why: why.chars().take(200).collect(), at });
            }
        }
        self.rekeep();
    }

    /// Let back into a village: one key, as the console shows it.
    pub fn let_back(&mut self, code: &str, key: &str) {
        let code = norm_code(code);
        if let Some(list) = self.kept.keepouts.get_mut(&code) {
            list.retain(|b| b.key != key);
            if list.is_empty() {
                self.kept.keepouts.remove(&code);
            }
        }
        self.rekeep();
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

    /// The public list's order, what's featured and kept out, and the
    /// extra words: for everyone.
    pub fn say_public(&mut self) {
        let k = &self.kept;
        let banned: Vec<&str> = k.village_bans.iter().map(|b| b.key.as_str()).collect();
        let json = serde_json::to_vec(&serde_json::json!({ "featured": k.featured, "order": k.order, "banned": banned, "words": k.words })).unwrap_or_default();
        self.say(ADMIN_PUBLIC, &json);
    }

    /// For the host: the villages it keeps, who can't talk where, what's
    /// said to newcomers.
    pub fn say_host(&mut self) {
        let k = &self.kept;
        let json = serde_json::to_vec(&serde_json::json!({ "squares": k.host_squares, "servers": k.servers, "muted": k.muted, "welcome": k.welcome, "square_rules": k.squares })).unwrap_or_default();
        self.say(ADMIN_HOST, &json);
    }

    /// Ask the host to do something now (kick, mute, announce…).
    pub fn host_cmd(&mut self, v: serde_json::Value) {
        let json = serde_json::to_vec(&v).unwrap_or_default();
        self.route(ADMIN_CMD, &json, false);
    }

    /// Is the relay's own host here?
    pub fn host_here(&self) -> bool {
        self.conns.values().any(|c| c.host)
    }

    /// A village kept out (by code): nobody through on its topics, and
    /// gone from the public list.
    pub fn ban_village(&mut self, code: &str, name: &str, why: &str) {
        let key: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
        if key.len() != 12 || self.kept.village_bans.iter().any(|b| b.key == key) {
            return;
        }
        self.kept.village_bans.push(Ban { key: key.clone(), name: name.chars().take(60).collect(), why: why.chars().take(200).collect(), at: now_secs() });
        self.reblock();
        // (its listing, kept here, forgotten)
        let t = topic_of("public", &key);
        if let Some(k) = self.retained.remove(&t) {
            self.retained_bytes -= k.payload.len();
        }
        self.say_public();
        self.host_cmd(serde_json::json!({ "close": key }));
    }

    pub fn unban_village(&mut self, code: &str) {
        let key: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
        self.kept.village_bans.retain(|b| b.key != key);
        self.reblock();
        self.say_public();
    }

    /// A player kept out (by id): those connected as them let go.
    pub fn ban_player(&mut self, id: u32, name: &str, why: &str) {
        // (by their tag too, if they're here: the same next time they come,
        // whatever their id then)
        let mut keys = vec![format!("{id:08x}")];
        for c in self.conns.values().filter(|c| player_of(&c.client) == Some(id)) {
            if let Some(t) = tag_of(&c.client).map(|t| format!("tag:{}", t.to_ascii_lowercase())) {
                if !keys.contains(&t) {
                    keys.push(t);
                }
            }
        }
        for key in keys {
            if !self.kept.player_bans.iter().any(|b| b.key == key) {
                self.kept.player_bans.push(Ban { key, name: name.chars().take(60).collect(), why: why.chars().take(200).collect(), at: now_secs() });
            }
        }
        let gone: Vec<u64> = self.conns.iter().filter(|(_, c)| player_of(&c.client) == Some(id)).map(|(&k, _)| k).collect();
        for k in gone {
            self.kick(k);
        }
    }

    pub fn unban_player(&mut self, id: u32) {
        let key = format!("{id:08x}");
        // (and the tag banned with it)
        let tags: Vec<String> = self.kept.player_bans.iter().filter(|b| b.key == key).map(|b| (b.name.clone(), b.at)).flat_map(|(n, at)| self.kept.player_bans.iter().filter(move |o| o.key.starts_with("tag:") && o.name == n && o.at == at).map(|o| o.key.clone())).collect();
        self.kept.player_bans.retain(|b| b.key != key && !tags.contains(&b.key));
    }

    /// Unbanned by its key as the console shows it (an id, or "tag:…").
    pub fn unban_key(&mut self, key: &str) {
        match u32::from_str_radix(key, 16) {
            Ok(id) if key.len() == 8 => self.unban_player(id),
            _ => self.kept.player_bans.retain(|b| b.key != key),
        }
    }

    /// The announcements due now (and when each comes next): what to say,
    /// where. A missed one isn't said late; a repeating one goes on.
    pub fn due(&mut self, now: u64) -> Vec<(String, String)> {
        // (the host's reports: gone a while after it stops sending them,
        // whether or not anything else comes)
        self.reports.retain(|_, (at, _)| at.elapsed().as_secs() < 120);
        let mut out = vec![];
        for s in self.kept.schedules.iter_mut().filter(|s| s.on) {
            if s.at > now {
                continue;
            }
            if now - s.at < 120 {
                out.push((s.to.clone(), s.text.clone()));
            }
            if s.every >= 60 {
                while s.at <= now {
                    s.at += s.every;
                }
            } else {
                s.on = false;
            }
        }
        out
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

/// The address a connection counts as, for the limit on connections from
/// one address and for blocking: an IPv4 address is itself; an IPv6 one
/// stands for its whole /64 (the block one home or one machine is given,
/// within which it can pick a new address whenever it likes).
pub fn block_of(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
    }
}

/// An address on this machine or its own network (the relay's host, in
/// the next container): not held to the one-address limit. (Everyone else
/// comes through Cloudflare, by their own address.)
pub fn internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback() || v.is_private(),
        IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00,
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

    /// (Addresses as players have them: public ones, held to the limits;
    /// TEST-NET-3.)
    fn ip(n: u8) -> IpAddr {
        IpAddr::from([203, 0, 113, n])
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
        // pretending to be the relay: not kept, not passed on (nor as a will)
        b.publish(a, ADMIN_SQUARES, b"{\"North Pole Square\":{\"time\":\"noon\"}}", true).unwrap();
        let (tw, _rw) = channel(64);
        let (w, _) = b.join(ip(9), "w".into(), Some((ADMIN_SQUARES.into(), b"{\"North Pole Square\":{}}".to_vec(), true)), tw);
        b.leave(w, false);
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

    /// The relay's own host connects from its own network, many times (a
    /// connection for each village it keeps): not held to the one-address
    /// limit, which players still are.
    #[test]
    fn the_hosts_own_network_is_not_held_to_the_address_limit() {
        assert!(internal(IpAddr::from([172, 18, 0, 3])) && internal(IpAddr::from([127, 0, 0, 1])));
        assert!(!internal(IpAddr::from([203, 0, 113, 9])));
    }

    /// The relay's own topics: only its host hears what's meant for it
    /// (not even through a wildcard), only the host speaks for it in a
    /// village or reports to the relay; its ids are its own; a banned
    /// village gets nothing through, a banned player is let go and kept out.
    #[test]
    fn the_host_alone_speaks_as_the_host_and_bans_hold() {
        let mut b = Broker::new(Kept::default());
        let (th, mut rh) = channel(64);
        let (h, _) = b.join_as(ip(1), "xt00000001aa".into(), None, th, true);
        let (tp, mut rp) = channel(64);
        let (p, _) = b.join(ip(2), "xt12345678bb".into(), None, tp);
        assert_eq!(b.subscribe(p, &[ADMIN_HOST.into(), "xmas-tui/1/#".into()]), vec![0x80, 0]);
        assert_eq!(b.subscribe(h, &[ADMIN_CMD.into(), ADMIN_HOST.into()]), vec![0, 0]);
        b.say_host();
        b.host_cmd(serde_json::json!({ "say": { "to": "*", "text": "hello" } }));
        let (gh, gp) = (drain(&mut rh), drain(&mut rp));
        let has = |v: &Vec<Vec<u8>>, t: &str| v.iter().any(|m| m.windows(t.len()).any(|w| w == t.as_bytes()));
        assert!(has(&gh, ADMIN_CMD) && has(&gh, ADMIN_HOST), "the host didn't hear");
        assert!(!has(&gp, ADMIN_CMD) && !has(&gp, ADMIN_HOST), "a player heard the host's");
        // a newcomer's wildcard doesn't pick up what's kept for the host
        let (tq, mut rq) = channel(64);
        let (q, _) = b.join(ip(3), "xt22222222cc".into(), None, tq);
        b.subscribe(q, &["xmas-tui/1/#".into()]);
        assert!(!has(&drain(&mut rq), ADMIN_HOST));
        // speaking as the host in a village: the host can, a player can't
        let v = format!("{ADMIN_V}abc");
        b.publish(p, &v, b"forged", false).unwrap();
        assert!(!has(&drain(&mut rq), "forged"));
        b.publish(h, &v, b"really", false).unwrap();
        assert!(has(&drain(&mut rq), "really"));
        // reports: the host's only
        b.publish(p, &format!("{ADMIN_REPORT}x"), b"{\"n\":9}", false).unwrap();
        assert!(b.reports.is_empty());
        b.publish(h, &format!("{ADMIN_REPORT}x"), b"{\"n\":2}", false).unwrap();
        assert_eq!(b.reports.get("x").map(|r| r.1["n"].clone()), Some(serde_json::json!(2)));
        // a report not sent again: gone after two minutes, nothing else coming
        if let Some(r) = b.reports.get_mut("x") {
            r.0 = Instant::now().checked_sub(std::time::Duration::from_secs(121)).unwrap();
        }
        b.due(now_secs());
        assert!(b.reports.is_empty(), "a stale report kept");
        // the host's ids are its own
        assert!(b.may_join("xt00000fffzz", false).is_err());
        assert!(b.may_join("xt00000fffzz", true).is_ok());
        assert!(b.may_join("xt00001000zz", false).is_ok());
        // a banned village: nothing through on its topics
        b.ban_village("ABCD-EFGH-JKLM", "Grinch's", "spam");
        let t = topic_of("village", "abcdefghjklm");
        b.publish(p, &t, b"blocked?", false).unwrap();
        assert!(!has(&drain(&mut rq), "blocked?"));
        b.unban_village("ABCDEFGHJKLM");
        b.publish(p, &t, b"open again", false).unwrap();
        assert!(has(&drain(&mut rq), "open again"));
        // a banned player: let go, and kept out (by their tag too: back
        // tomorrow with another id, still out)
        let (tp2, _rp2) = channel(64);
        let (p2, _) = b.join(ip(4), "xt12345678abcdef01aa".into(), None, tp2);
        b.ban_player(0x1234_5678, "Grinch", "spam");
        assert!(!b.has(p) && !b.has(p2));
        assert!(b.may_join("xt12345678dd", false).is_err());
        assert!(b.may_join("xt99999999abcdef01bb", false).is_err(), "back with another id");
        assert!(b.may_join("xt99999999aaaaaaaabb", false).is_ok());
        b.unban_player(0x1234_5678);
        assert!(b.may_join("xt12345678dd", false).is_ok());
        assert!(b.may_join("xt99999999abcdef01bb", false).is_ok(), "the tag's ban stayed");
    }

    /// Announcements: said when due, once or again and again; one missed
    /// by a long way (the relay was down) isn't said late.
    /// A player kept out of one village: nothing said there reaches them
    /// and nothing they say reaches it, by id, by tag when they come back
    /// under another id, and by address if asked; elsewhere they're as
    /// anyone; let back, they hear it again. The host is never kept out,
    /// and nor is an address on the relay's own network.
    #[test]
    fn a_player_kept_out_of_one_village_only() {
        let mut b = Broker::new(Kept::default());
        let code = "ABCD-EFGH-JKLM";
        let (v, other) = (topic_of("village", code), topic_of("village", "ZZZZ-ZZZZ-ZZZZ"));
        let (ta, mut ra) = channel(64);
        let (tb, mut rb) = channel(64);
        let (a, _) = b.join(ip(1), "xt00002001aaaaaaaa0001".into(), None, ta);
        let (bad, _) = b.join(ip(2), "xt00002002bbbbbbbb0001".into(), None, tb);
        for id in [a, bad] {
            assert_eq!(b.subscribe(id, &[v.clone(), other.clone()]), vec![0, 0]);
        }
        b.keep_out(code, 0x2002, "Bot", "spam", true);
        assert_eq!(b.kept.keepouts[&norm_code(code)].iter().map(|k| k.key.as_str()).collect::<Vec<_>>(), vec!["00002002", "tag:bbbbbbbb", "ip:203.0.113.2"]);
        (drain(&mut ra), drain(&mut rb));
        // not heard there, and hears nothing of it
        b.publish(bad, &v, b"spam", false).unwrap();
        b.publish(a, &v, b"hello", false).unwrap();
        assert_eq!(drain(&mut ra).len(), 1, "the spam got through, or hello didn't come back");
        assert!(drain(&mut rb).is_empty(), "still hearing the village");
        // elsewhere: as anyone
        b.publish(bad, &other, b"hi", false).unwrap();
        assert_eq!((drain(&mut ra).len(), drain(&mut rb).len()), (1, 1));
        // back under another id, the same tag: still out; and a new tag
        // from the same address: out too
        for (n, client) in [(3u8, "xt00002003bbbbbbbb0002"), (2u8, "xt00002004cccccccc0001")] {
            let (tc, mut rc) = channel(64);
            let (c, _) = b.join(ip(n), client.into(), None, tc);
            assert_eq!(b.subscribe(c, &[v.clone()]), vec![0x80], "{client} let back in");
            b.publish(a, &v, b"hello", false).unwrap();
            assert!(drain(&mut rc).is_empty());
            b.leave(c, true);
        }
        // let back (each key): hears it again
        for key in ["00002002", "tag:bbbbbbbb", "ip:203.0.113.2"] {
            b.let_back(code, key);
        }
        assert!(b.kept.keepouts.is_empty());
        drain(&mut rb);
        b.publish(a, &v, b"hello", false).unwrap();
        assert_eq!(drain(&mut rb).len(), 1);
        // an address on the relay's own network is never one to keep out
        let (tl, _rl) = channel(64);
        b.join(IpAddr::from([172, 18, 0, 3]), "xt00002005dddddddd0001".into(), None, tl);
        b.keep_out(code, 0x2005, "Local", "", true);
        assert!(b.kept.keepouts[&norm_code(code)].iter().all(|k| !k.key.starts_with("ip:")));
        // the villages in use: this one and the other, with who's there
        let vs = b.villages();
        assert!(vs.iter().any(|(t, who)| *t == v && who.len() == 2), "{vs:?}");
    }

    /// An IPv6 address counts as its whole /64: blocked, another address
    /// in the same block is blocked too (and those connected from it let
    /// go), the next block isn't; the limit on connections from one
    /// address counts the block; unblocking by any address in it lifts it.
    #[test]
    fn an_ipv6_address_stands_for_its_block() {
        let v6 = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(block_of(v6("2001:db8:1:2:aaaa:bbbb:cccc:dddd")), v6("2001:db8:1:2::"));
        assert_eq!(block_of(v6("::ffff:203.0.113.7")), ip(7));
        assert_eq!(block_of(ip(7)), ip(7));
        let mut b = Broker::new(Kept::default());
        let (ta, _ra) = channel(8);
        let (a, _) = b.join(v6("2001:db8:1:2::10"), "a".into(), None, ta);
        b.ban(v6("2001:db8:1:2::99"));
        assert!(!b.has(a), "still connected from the blocked block");
        assert_eq!(b.kept.bans, vec!["2001:db8:1:2::".to_string()]);
        assert_eq!(b.admit(v6("2001:db8:1:2:1234::1")), Err("banned"));
        assert!(b.admit(v6("2001:db8:1:3::1")).is_ok());
        // (kept, it holds after a restart)
        let mut again = Broker::new(b.kept.clone());
        assert_eq!(again.admit(v6("2001:db8:1:2:ffff::2")), Err("banned"));
        b.unban(v6("2001:db8:1:2::1"));
        assert!(b.kept.bans.is_empty());
        assert!(b.admit(v6("2001:db8:1:2:1234::1")).is_ok());
        // the limit from one address: the block's
        b.kept.limits.max_conns_per_ip = 2;
        for n in 1..=2u16 {
            let (t, _r) = channel(8);
            std::mem::forget(_r);
            b.join(IpAddr::V6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 9, 9, 0, 0, 0, n)), format!("c{n}"), None, t);
        }
        assert_eq!(b.admit(v6("2001:db8:9:9:abcd::1")), Err("too many from one address"));
        assert!(b.admit(v6("2001:db8:9:a::1")).is_ok());
    }

    #[test]
    fn announcements_come_when_due() {
        let mut b = Broker::new(Kept::default());
        let now = 1_000_000;
        b.kept.schedules = vec![
            Schedule { id: 1, text: "once".into(), to: "*".into(), at: now - 10, every: 0, on: true },
            Schedule { id: 2, text: "hourly".into(), to: "*".into(), at: now - 10, every: 3600, on: true },
            Schedule { id: 3, text: "long missed".into(), to: "*".into(), at: now - 3 * 3600, every: 0, on: true },
            Schedule { id: 4, text: "later".into(), to: "*".into(), at: now + 60, every: 0, on: true },
        ];
        let said: Vec<String> = b.due(now).into_iter().map(|d| d.1).collect();
        assert_eq!(said, vec!["once", "hourly"]);
        assert!(!b.kept.schedules[0].on && b.kept.schedules[1].on && b.kept.schedules[1].at > now && !b.kept.schedules[2].on);
        assert!(b.due(now + 30).is_empty());
        assert_eq!(b.due(now + 61).into_iter().map(|d| d.1).collect::<Vec<_>>(), vec!["later"]);
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
