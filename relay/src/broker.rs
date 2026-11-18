//! Who's connected, what they listen to, the messages kept for newcomers
//! (retained: "here I am", the shared villages' state), and the limits that
//! keep the relay steady: how many connections (in all, and from one
//! address), how big and how often messages may be, how many
//! subscriptions, how much is kept. Only the village's own topics
//! (`xmas-tui/…`) are carried. What's sent is sealed end to end by the
//! village: the relay passes bytes along without being able to read them.

use crate::codec;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
            max_queue: 256,
            closed: false,
        }
    }
}

/// What's waiting to go out to every connection, in all (bytes).
static QUEUED: AtomicUsize = AtomicUsize::new(0);
/// What may wait for one connection (bytes): past it, it's too slow to
/// keep up and is let go.
pub const QUEUE_BYTES: usize = 2 << 20;
/// With this much waiting in all, a connection with more than
/// `SLOW_BYTES` waiting is let go (it's the one not reading)...
const ALL_SOFT: usize = 96 << 20;
const SLOW_BYTES: usize = 256 << 10;
/// ...and past this, nothing more is queued for anyone until it drains:
/// the relay stays well inside its memory, whatever's sent.
const ALL_HARD: usize = 192 << 20;
/// What one address may have kept on the relay (topics, bytes): plenty for
/// a household's players (here, linked devices, a listing).
const KEPT_PER_ADDR: usize = 300;
const KEPT_BYTES_PER_ADDR: usize = 4 << 20;

/// What's waiting to go out to every connection right now (bytes).
pub fn queued_all() -> usize {
    QUEUED.load(Ordering::Relaxed)
}

/// Bytes waiting to go out to one connection. What it counts is given back
/// when it's sent, or dropped with the connection: so the counts can't
/// drift, however a connection ends.
pub struct Q {
    bytes: Vec<u8>,
    n: usize,
    conn: Arc<AtomicUsize>,
}

impl Q {
    pub fn take(mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl Drop for Q {
    fn drop(&mut self) {
        self.conn.fetch_sub(self.n, Ordering::Relaxed);
        QUEUED.fetch_sub(self.n, Ordering::Relaxed);
    }
}

/// The way to one connection: its queue, and how much is waiting in it.
#[derive(Clone)]
pub struct Down {
    tx: Sender<Q>,
    queued: Arc<AtomicUsize>,
}

/// How a push went.
#[derive(Debug, PartialEq)]
pub enum Push {
    Ok,
    /// Too much waiting for it already: let it go.
    Slow,
    /// Too much waiting for everyone: this one's dropped.
    Dropped,
}

impl From<Sender<Q>> for Down {
    fn from(tx: Sender<Q>) -> Down {
        Down { tx, queued: Arc::new(AtomicUsize::new(0)) }
    }
}

impl Down {
    fn q(&self, bytes: Vec<u8>) -> Q {
        let n = bytes.len();
        self.queued.fetch_add(n, Ordering::Relaxed);
        QUEUED.fetch_add(n, Ordering::Relaxed);
        Q { bytes, n, conn: self.queued.clone() }
    }

    /// What's waiting for it now (bytes).
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    /// Queued, if there's room (never waits).
    pub fn push(&self, bytes: Vec<u8>) -> Push {
        let (all, mine) = (QUEUED.load(Ordering::Relaxed), self.queued());
        if mine + bytes.len() > QUEUE_BYTES || (all > ALL_SOFT && mine > SLOW_BYTES) {
            return Push::Slow;
        }
        if all + bytes.len() > ALL_HARD {
            return Push::Dropped;
        }
        match self.tx.try_send(self.q(bytes)) {
            Ok(()) => Push::Ok,
            Err(_) => Push::Slow,
        }
    }

    /// Queued, waiting for room (the hello's answer).
    pub async fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx.send(self.q(bytes)).await.is_ok()
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
    tx: Down,
    /// The relay's host, told who sent each village message (see
    /// `ADMIN_VIA`).
    via: bool,
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
    /// Whose (the address that kept it; none: the relay's, or its host's).
    owner: Option<IpAddr>,
    /// Which keeping this is (an old place in the queue to let go is
    /// skipped once it's been kept again).
    seq: u64,
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

/// The relay's host listens here to be told who sent each village message:
/// they come to it on `ADMIN_VIA/<connection>/<village>` instead, so a
/// player can't speak as another (the relay knows which connection sent
/// what; the message's own id is only what the sender says).
pub const ADMIN_VIA: &str = "xmas-tui/1/admin/via";

/// Topics only the relay's own host may hear.
pub fn host_only(t: &str) -> bool {
    t == ADMIN_HOST || t == ADMIN_CMD || t.starts_with(ADMIN_REPORT) || t == ADMIN_VIA || t.starts_with(&format!("{ADMIN_VIA}/"))
}

/// The kinds of topic the village uses (`xmas-tui/1/<kind>/…`).
const KINDS: [&str; 8] = ["here", "mail", "sync", "public", "listed", "village", "group", "admin"];

/// A topic's kind.
fn kind_of(t: &str) -> &str {
    t.strip_prefix("xmas-tui/1/").and_then(|r| r.split('/').next()).unwrap_or("")
}

/// The kinds the village keeps on the relay for whoever comes next; on any
/// other topic a message is passed on, never kept.
fn keepable(t: &str) -> bool {
    matches!(kind_of(t), "here" | "sync" | "public" | "listed" | "admin")
}

/// Where a subscription is filed: exact topics by themselves, `<parent>/+`
/// by the parent (so passing a message on is two lookups, however many
/// listen).
fn sub_key(f: &str) -> (bool, &str) {
    match f.strip_suffix("/+") {
        Some(parent) => (true, parent),
        None => (false, f),
    }
}

/// An IPv6 address's /48 (one site), for a looser limit than its /64's.
fn site_of(ip: IpAddr) -> Option<[u16; 3]> {
    match ip {
        IpAddr::V6(v) if v.to_ipv4_mapped().is_none() => {
            let s = v.segments();
            Some([s[0], s[1], s[2]])
        }
        _ => None,
    }
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

/// What a connection's playing on, from what it said it is ("linux",
/// "windows", "android", "macos", "web"; "older app": one from before
/// 1.25.2, which doesn't say).
pub fn platform_of(agent: &str) -> &str {
    if agent == "web" {
        return "web";
    }
    match agent.rsplit_once('(').map(|(_, os)| os.trim_end_matches(')')) {
        Some(os @ ("linux" | "windows" | "android" | "macos")) => os,
        _ => "older app",
    }
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
    /// Waiting to go out to every connection, in all (bytes).
    pub queued_bytes: usize,
    /// Connections that aren't the relay's own host (players' devices).
    pub players: usize,
    /// Those by what they're playing on.
    pub platforms: BTreeMap<String, usize>,
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
    retained: BTreeMap<String, Kept1>,
    retained_bytes: usize,
    order: VecDeque<(String, u64)>,
    seq: u64,
    /// What each address has kept (topics, bytes).
    owned: HashMap<IpAddr, (usize, usize)>,
    /// Connections let in but not yet said hello (by address block), and
    /// in all: they count against the limits from the moment they open.
    pending: HashMap<IpAddr, usize>,
    pending_all: usize,
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
        let mut b = Broker { kept, conns: HashMap::new(), next_id: 1, exact: HashMap::new(), wild: HashMap::new(), retained: BTreeMap::new(), retained_bytes: 0, order: VecDeque::new(), seq: 0, owned: HashMap::new(), pending: HashMap::new(), pending_all: 0, stats: Stats::default(), started: Instant::now(), recent: VecDeque::new(), bans, blocked: HashSet::new(), kept_out: HashMap::new(), reports: BTreeMap::new() };
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
        let block = block_of(ip);
        let from_block = self.conns.values().filter(|c| block_of(c.ip) == block).count() + self.pending.get(&block).copied().unwrap_or(0);
        let site = site_of(ip);
        let from_site = || self.conns.values().filter(|c| site_of(c.ip) == site).count() + self.pending.iter().filter(|(b, _)| site_of(**b) == site).map(|(_, n)| n).sum::<usize>();
        // (the relay's own network, its host: never shut out by the limits)
        let outside = !internal(ip);
        let why = if l.closed {
            Some("closed")
        } else if self.bans.contains(&block) {
            Some("banned")
        } else if outside && self.conns.len() + self.pending_all >= l.max_conns {
            Some("full")
        } else if outside && from_block >= l.max_conns_per_ip {
            Some("too many from one address")
        } else if outside && site.is_some() && from_site() >= l.max_conns_per_ip * 4 {
            Some("too many from one network")
        } else {
            None
        };
        match why {
            Some(w) => {
                self.stats.refused += 1;
                Err(w)
            }
            None => {
                *self.pending.entry(block).or_default() += 1;
                self.pending_all += 1;
                Ok(())
            }
        }
    }

    /// A connection let in by `admit` has gone, or said hello (and now
    /// counts as itself).
    pub fn unpend(&mut self, ip: IpAddr) {
        let block = block_of(ip);
        if let Some(n) = self.pending.get_mut(&block) {
            *n -= 1;
            self.pending_all = self.pending_all.saturating_sub(1);
            if *n == 0 {
                self.pending.remove(&block);
            }
        }
    }

    /// May a connection from `ip` still join (not blocked since it came,
    /// nor the relay closed)?
    pub fn ip_ok(&self, ip: IpAddr) -> bool {
        !self.kept.limits.closed && !self.bans.contains(&block_of(ip))
    }

    /// A connection that's said hello: its id. (One already there with the
    /// same client name is let go: MQTT's "take over".)
    pub fn join(&mut self, ip: IpAddr, client: String, will: Option<(String, Vec<u8>, bool)>, tx: impl Into<Down>) -> (u64, Option<u64>) {
        self.join_as(ip, client, will, tx, false)
    }

    /// The same, for the relay's own host (`host`: signed in).
    pub fn join_as(&mut self, ip: IpAddr, client: String, will: Option<(String, Vec<u8>, bool)>, tx: impl Into<Down>, host: bool) -> (u64, Option<u64>) {
        self.unpend(ip);
        let old = self.conns.iter().find(|(_, c)| c.client == client).map(|(&id, _)| id);
        if let Some(o) = old {
            self.leave(o, false);
        }
        let id = self.next_id;
        self.next_id += 1;
        // (a will is published by the relay itself: never on its own topics)
        // (a little: the village's is a sealed "gone")
        let will = will.filter(|(t, p, _)| ok_topic(t) && !t.starts_with(ADMIN) && p.len() <= 4096);
        let burst = self.kept.limits.burst;
        self.conns.insert(id, Conn { ip, client, host, agent: String::new(), country: String::new(), since: Instant::now(), tx: tx.into(), via: false, will, subs: HashSet::new(), tokens: burst, tokens_at: Instant::now(), sent: 0, got: 0, dropped: 0 });
        (id, old)
    }

    /// Gone: its subscriptions forgotten, and (if it didn't say goodbye) its
    /// will published.
    pub fn leave(&mut self, id: u64, clean: bool) {
        let Some(c) = self.conns.remove(&id) else { return };
        for f in &c.subs {
            self.unfile(f, id);
        }
        if !clean {
            // (their last word: not into a banned village, nor one they're
            // kept out of, whenever that came about)
            let shut = c.will.as_ref().is_some_and(|(t, ..)| self.blocked.contains(t) || self.kept_out.get(t).is_some_and(|keys| kept_out_of(keys, &c)));
            if let Some((t, p, retain)) = c.will.clone().filter(|_| !shut) {
                let owner = (!c.host).then(|| block_of(c.ip));
                self.route(&t, &p, retain, None, owner);
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
        refill(c, &l);
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
        // a banned village: nothing through (counted against the sender
        // like any message turned down, so one who floods it is let go)
        let shut = self.blocked.contains(topic);
        // (a village's listing: only from the player whose it is)
        let not_theirs = kind_of(topic) == "listed" && topic.rsplit('/').next().and_then(|h| u32::from_str_radix(h, 16).ok()) != player_of(&c.client);
        if shut || not_theirs || c.tokens < 1.0 || !ok_topic(topic) || payload.len() > l.max_payload {
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
        // kept out of this village: not heard there. Quietly, and not held
        // against them: their game may not know, and goes on talking; the
        // rest of the relay stays theirs. (It still costs them their
        // allowance of messages, so it's no way round the limits.)
        if self.kept_out.get(topic).is_some_and(|keys| kept_out_of(keys, c)) {
            return Ok(());
        }
        c.sent += 1;
        let owner = (!c.host).then(|| block_of(c.ip));
        self.stats.msgs_in += 1;
        self.stats.bytes_in += payload.len() as u64;
        self.route(topic, payload, retain, Some(id), owner);
        Ok(())
    }

    /// On to whoever listens; any that can't keep up (their queue full) are
    /// let go rather than kept in memory.
    /// Passed on to everyone listening (kept too, if asked and it's a kind
    /// the village keeps). `from`: the connection it came from; `owner`:
    /// the address it counts against, for what's kept.
    fn route(&mut self, topic: &str, payload: &[u8], retain: bool, from: Option<u64>, owner: Option<IpAddr>) {
        if retain && keepable(topic) {
            self.keep(topic, payload, owner);
        }
        let mut to: Vec<u64> = self.exact.get(topic).map(|s| s.iter().copied().collect()).unwrap_or_default();
        if let Some((parent, _)) = topic.rsplit_once('/') {
            if let Some(s) = self.wild.get(parent) {
                to.extend(s.iter().copied());
            }
        }
        to.sort_unstable();
        to.dedup();
        let bytes = codec::publish(topic, payload, false);
        // (to the relay's host, a village message says which connection
        // it came from)
        let via = from.filter(|_| kind_of(topic) == "village").map(|f| codec::publish(&format!("{ADMIN_VIA}/{f:x}/{}", topic.rsplit('/').next().unwrap_or("")), payload, false));
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
                let b = match &via {
                    Some(v) if c.host && c.via => v.clone(),
                    _ => bytes.clone(),
                };
                let n = b.len();
                match c.tx.push(b) {
                    Push::Ok => {
                        c.got += 1;
                        self.stats.msgs_out += 1;
                        self.stats.bytes_out += n as u64;
                    }
                    Push::Slow => slow.push(id),
                    Push::Dropped => {
                        c.dropped += 1;
                        self.stats.dropped += 1;
                    }
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

    /// Kept for whoever comes next (an empty one: forgotten). Each address
    /// may keep so much; the oldest go when the relay's full, never the
    /// relay's own.
    fn keep(&mut self, topic: &str, payload: &[u8], owner: Option<IpAddr>) {
        self.unkeep(topic);
        let (most, most_bytes, biggest) = (self.kept.limits.max_retained, self.kept.limits.max_retained_bytes, self.kept.limits.max_retained_payload);
        if payload.is_empty() || payload.len() > biggest {
            return;
        }
        if let Some(o) = owner {
            let (n, b) = self.owned.get(&o).copied().unwrap_or((0, 0));
            if n >= KEPT_PER_ADDR || b + payload.len() > KEPT_BYTES_PER_ADDR {
                self.stats.dropped += 1;
                return;
            }
            let e = self.owned.entry(o).or_default();
            e.0 += 1;
            e.1 += payload.len();
        }
        self.seq += 1;
        let seq = self.seq;
        self.retained_bytes += payload.len();
        self.retained.insert(topic.to_string(), Kept1 { payload: payload.to_vec(), at: now_secs(), owner, seq });
        // (the relay's own are never let go to make room)
        if !topic.starts_with(ADMIN) {
            self.order.push_back((topic.to_string(), seq));
        }
        // the oldest let go when there's too much
        while self.retained.len() > most || self.retained_bytes > most_bytes {
            let Some((t, s)) = self.order.pop_front() else { break };
            if self.retained.get(&t).is_some_and(|k| k.seq == s) {
                self.unkeep(&t);
            }
        }
        if self.order.len() > self.retained.len() * 2 + 64 {
            let r = &self.retained;
            self.order.retain(|(t, s)| r.get(t).is_some_and(|k| k.seq == *s));
        }
    }

    /// Forgotten (what it counted, given back).
    fn unkeep(&mut self, topic: &str) -> Option<Kept1> {
        let k = self.retained.remove(topic)?;
        self.retained_bytes -= k.payload.len();
        if let Some(o) = k.owner {
            if let Some(e) = self.owned.get_mut(&o) {
                e.0 = e.0.saturating_sub(1);
                e.1 = e.1.saturating_sub(k.payload.len());
                if e.0 == 0 {
                    self.owned.remove(&o);
                }
            }
        }
        Some(k)
    }

    /// A subscription forgotten (by connection).
    fn unfile(&mut self, f: &str, id: u64) {
        let (wild, key) = sub_key(f);
        let map = if wild { &mut self.wild } else { &mut self.exact };
        if let Some(s) = map.get_mut(key) {
            s.remove(&id);
            if s.is_empty() {
                map.remove(key);
            }
        }
    }

    /// Listen to `filters`; the kept messages for them go out at once. Each
    /// filter's granted QoS (0), or 0x80 for one refused.
    pub fn subscribe(&mut self, id: u64, filters: &[String]) -> Vec<u8> {
        let l = self.kept.limits.clone();
        let mut granted = vec![];
        let mut fresh = vec![];
        let Some(c) = self.conns.get_mut(&id) else { return vec![0x80; filters.len()] };
        let host = c.host;
        // (listening costs from the same allowance as talking, a little)
        refill(c, &l);
        for f in filters {
            let paid = c.tokens >= SUB_COST;
            if paid {
                c.tokens -= SUB_COST;
            } else {
                c.dropped += 1;
                self.stats.dropped += 1;
            }
            let shut = self.blocked.contains(f) || self.kept_out.get(f).is_some_and(|keys| kept_out_of(keys, c));
            if !paid || !ok_filter(f) || (c.subs.len() >= l.max_subs && !c.subs.contains(f)) || (host_only(f) && !host) || shut {
                granted.push(0x80);
                continue;
            }
            granted.push(0);
            if f == ADMIN_VIA {
                c.via = true;
            }
            if c.subs.insert(f.clone()) {
                let (wild, key) = sub_key(f);
                let map = if wild { &mut self.wild } else { &mut self.exact };
                map.entry(key.to_string()).or_default().insert(id);
                fresh.push(f.clone());
            }
        }
        // what's kept, for what's newly listened to: straight to it, until
        // its queue's full (never more than it can take)
        let mut out = vec![];
        for f in &fresh {
            match sub_key(f) {
                (true, parent) => {
                    let p = format!("{parent}/");
                    for (t, k) in self.retained.range(p.clone()..).take_while(|(t, _)| t.starts_with(&p)).filter(|(t, _)| !t[p.len()..].contains('/')).take(1024) {
                        out.push((t.clone(), k.payload.clone()));
                    }
                }
                (false, t) => {
                    if let Some(k) = self.retained.get(t) {
                        out.push((t.to_string(), k.payload.clone()));
                    }
                }
            }
        }
        if let Some(c) = self.conns.get(&id) {
            for (t, payload) in out {
                if (host_only(&t) && !host) || self.kept_out.get(&t).is_some_and(|keys| kept_out_of(keys, c)) {
                    continue;
                }
                if c.tx.push(codec::publish(&t, &payload, true)) != Push::Ok {
                    break;
                }
            }
        }
        granted
    }

    pub fn unsubscribe(&mut self, id: u64, filters: &[String]) {
        let l = self.kept.limits.clone();
        let Some(c) = self.conns.get_mut(&id) else { return };
        refill(c, &l);
        let mut gone = vec![];
        for f in filters {
            if c.tokens < SUB_COST {
                break;
            }
            c.tokens -= SUB_COST;
            if c.subs.remove(f) {
                gone.push(f.clone());
            }
        }
        for f in gone {
            self.unfile(&f, id);
        }
    }

    /// Something straight back to one connection (an ack, a pong).
    pub fn reply(&mut self, id: u64, bytes: Vec<u8>) -> Result<(), Bye> {
        let Some(c) = self.conns.get_mut(&id) else { return Ok(()) };
        match c.tx.push(bytes) {
            Push::Ok | Push::Dropped => Ok(()),
            Push::Slow => Err(Bye::Slow),
        }
    }

    // ---------------- the admin console

    pub fn stats(&self) -> Stats {
        let mut s = self.stats.clone();
        s.conns = self.conns.len();
        s.ips = self.conns.values().map(|c| c.ip).collect::<HashSet<_>>().len();
        s.subs = self.conns.values().map(|c| c.subs.len()).sum();
        s.retained = self.retained.len();
        s.retained_bytes = self.retained_bytes;
        s.queued_bytes = queued_all();
        for c in self.conns.values().filter(|c| !c.host) {
            s.players += 1;
            *s.platforms.entry(platform_of(&c.agent).to_string()).or_default() += 1;
        }
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

    /// One connection, as the console shows it: (address, country, what it
    /// said it is, its client id).
    pub fn conn_info(&self, conn: u64) -> Option<(String, String, String, String)> {
        self.conns.get(&conn).filter(|c| !c.host).map(|c| (c.ip.to_string(), c.country.clone(), c.agent.clone(), c.client.clone()))
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
                let outs = self.kept_out.get(t);
                let mut who: Vec<(String, String, String, bool, String)> = ids.iter().filter_map(|id| self.conns.get(id)).filter(|c| !outs.is_some_and(|keys| kept_out_of(keys, c))).map(|c| (c.ip.to_string(), c.client.clone(), c.agent.clone(), c.host, c.country.clone())).collect();
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
    ///
    /// Says whether a connection of theirs was found. Without one, only
    /// the id is known, and that changes each time their game starts.
    pub fn keep_out(&mut self, code: &str, id: u32, name: &str, why: &str, with_ip: bool) -> bool {
        let code = norm_code(code);
        if code.is_empty() {
            return false;
        }
        let found = self.conns.values().any(|c| !c.host && player_of(&c.client) == Some(id));
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
        found
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
            self.unkeep(t);
        }
        gone.len()
    }

    /// The relay's own message (the squares' settings): kept, and on to
    /// whoever listens.
    pub fn say(&mut self, topic: &str, payload: &[u8]) {
        self.route(topic, payload, true, None, None);
    }

    /// The public list's order, what's featured and kept out, and the
    /// extra words: for everyone.
    pub fn say_public(&mut self) {
        let k = &self.kept;
        // (a village's code is its key: only those that were listed, whose
        // codes were public anyway, go out as codes; every banned one goes
        // out as its topic, which can't be turned back into the code)
        let banned: Vec<&str> = k.village_bans.iter().filter(|b| !b.name.is_empty()).map(|b| b.key.as_str()).collect();
        let banned_ids: Vec<String> = k.village_bans.iter().map(|b| topic_of("village", &b.key).rsplit('/').next().unwrap_or("").to_string()).collect();
        let json = serde_json::to_vec(&serde_json::json!({ "featured": k.featured, "order": k.order, "banned": banned, "banned_ids": banned_ids, "words": k.words })).unwrap_or_default();
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
        self.route(ADMIN_CMD, &json, false, None, None);
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
        self.unkeep(&t);
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
            if !ok_topic(&topic) || !keepable(&topic) {
                continue;
            }
            self.keep(&topic, &payload, None);
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
            self.unkeep(&t);
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
    t.len() <= 200 && !t.contains(['+', '#', '\0']) && t.strip_prefix("xmas-tui/1/").and_then(|r| r.split_once('/')).is_some_and(|(k, rest)| KINDS.contains(&k) && !rest.is_empty())
}

/// A subscription the village makes: its prefix, wildcards only as whole
/// levels (`#` last).
pub fn ok_filter(f: &str) -> bool {
    // (a topic itself, or the two wildcards the village uses: the public
    // listings, and one's own linked devices. Nothing that lists topics
    // nobody's told the listener: a private village, a mailbox, a linked
    // device can't be found, only known.)
    if !f.contains(['+', '#']) {
        return ok_topic(f);
    }
    if f == "xmas-tui/1/listed/+" {
        return true;
    }
    f.strip_prefix("xmas-tui/1/sync/").and_then(|r| r.strip_suffix("/+")).is_some_and(|h| h.len() == 24 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// What subscribing to one filter costs from the allowance of messages.
const SUB_COST: f32 = 0.25;

/// A connection's allowance of messages, topped up for the time gone by.
fn refill(c: &mut Conn, l: &Limits) {
    let now = Instant::now();
    c.tokens = (c.tokens + now.duration_since(c.tokens_at).as_secs_f32() * l.msgs_per_sec).min(l.burst);
    c.tokens_at = now;
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

    fn drain(rx: &mut Receiver<Q>) -> Vec<Vec<u8>> {
        let mut v = vec![];
        while let Ok(q) = rx.try_recv() {
            v.push(q.take());
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
        // (a wildcard over everything: refused; the topics themselves: fine)
        assert_eq!(c.subscribe(bb, &["xmas-tui/1/#".into()]), vec![0x80]);
        c.subscribe(bb, &["xmas-tui/1/sync/abc/dev1".into(), "xmas-tui/1/here/abc".into(), ADMIN_SQUARES.into()]);
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
        assert_eq!(b.subscribe(p, &[ADMIN_HOST.into(), "xmas-tui/1/#".into()]), vec![0x80, 0x80]);
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
        b.subscribe(q, &[v.clone()]);
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
        let t = topic_of("village", "abcdefghjklm");
        b.subscribe(q, &[t.clone()]);
        b.ban_village("ABCD-EFGH-JKLM", "Grinch's", "spam");
        b.publish(p, &t, b"blocked?", false).unwrap();
        assert!(!has(&drain(&mut rq), "blocked?"));
        // (nor can anyone start listening to it)
        assert_eq!(b.subscribe(p, &[t.clone()]), vec![0x80]);
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
        // their game goes on talking, not knowing: never let go for it
        for _ in 0..3000 {
            b.kept.limits.burst = 1e9;
            if let Some(c) = b.conns.get_mut(&bad) {
                c.tokens = 1e9;
            }
            assert!(b.publish(bad, &v, b"here I am", false).is_ok(), "let go from the whole relay for talking in a village they're kept out of");
        }
        assert!(drain(&mut ra).is_empty());
        // and not shown as in it
        assert!(b.villages().iter().any(|(t, who)| *t == v && who.len() == 1), "{:?}", b.villages());
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
        // nor heard through a last word left behind (a will), kept or not
        drain(&mut ra);
        for retain in [false, true] {
            let (tw, _rw) = channel(64);
            let (w, _) = b.join(ip(2), "xt00002006bbbbbbbb0009".into(), Some((v.clone(), b"spam from a will".to_vec(), retain)), tw);
            b.leave(w, false);
        }
        assert!(drain(&mut ra).is_empty(), "heard through a will");
        let (tn, mut rn) = channel(64);
        let (newcomer, _) = b.join(ip(8), "xt00002007eeeeeeee0001".into(), None, tn);
        b.subscribe(newcomer, &[v.clone()]);
        assert!(drain(&mut rn).is_empty(), "a kept will handed to a newcomer");
        b.leave(newcomer, true);
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
        // someone not connected: only their id can be noted, and it says so
        assert!(!b.keep_out(code, 0x7777, "Gone", "", true));
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
        assert_eq!(b.subscribe(bb, &["xmas-tui/1/mail/b".into(), "xmas-tui/1/here/a".into(), "xmas-tui/1/here/+".into(), "nope/x".into()]), vec![0, 0, 0x80, 0x80]);
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
            b.publish(a, "xmas-tui/1/mail/x", b"x", false).unwrap();
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
        b.subscribe(bb, &["xmas-tui/1/mail/t".into()]);
        for _ in 0..5 {
            b.publish(a, "xmas-tui/1/mail/t", b"x", false).unwrap();
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
        // only the wildcards the village uses
        assert!(ok_filter("xmas-tui/1/sync/0123456789abcdef01234567/+") && ok_filter("xmas-tui/1/listed/+") && ok_filter("xmas-tui/1/here/abc"));
        assert!(!ok_filter("xmas-tui/1/sync/ab/+") && !ok_filter("xmas-tui/#") && !ok_filter("xmas-tui/1/here/+") && !ok_filter("xmas-tui/1/+/x"));
        assert!(!ok_filter("#") && !ok_filter("xmas-tui/1/#/x") && !ok_filter("xmas-tui/1/a+b"));
        // only the kinds the village uses
        assert!(ok_topic("xmas-tui/1/here/x") && !ok_topic("xmas-tui/1/here/+") && !ok_topic("other/x"));
        assert!(!ok_topic("xmas-tui/1/x") && !ok_topic("xmas-tui/1/made-up/x") && !ok_topic("xmas-tui/1/here/"));
    }

    /// What waits for a connection is held to so many bytes, not just so
    /// many messages: one that stops reading is let go once it's past it,
    /// and what was waiting for it is given back when it goes.
    #[test]
    fn a_connection_that_stops_reading_is_held_to_its_bytes() {
        let mut b = Broker::new(Kept::default());
        b.kept.limits.burst = 1e9;
        b.kept.limits.msgs_per_sec = 1e9;
        let (ta, _ra) = channel(10_000);
        let (tb, rb) = channel(10_000);
        let (a, _) = b.join(ip(1), "a".into(), None, ta);
        let down: Down = tb.into();
        let (bb, _) = b.join(ip(2), "b".into(), None, down.clone());
        b.subscribe(bb, &["xmas-tui/1/mail/t".into()]);
        let big = vec![7u8; 100_000];
        let mut sent = 0;
        while b.has(bb) && sent < 100 {
            b.publish(a, "xmas-tui/1/mail/t", &big, false).unwrap();
            sent += 1;
        }
        assert!(!b.has(bb), "still there with {} bytes waiting", down.queued());
        assert!(sent * 100_000 <= QUEUE_BYTES + 200_000, "let go only after {sent} messages");
        assert!(down.queued() <= QUEUE_BYTES);
        drop(rb);
        assert_eq!(down.queued(), 0, "what was waiting wasn't given back");
    }

    /// Connections count from the moment they're let in, not from their
    /// hello: opening many and saying hello later doesn't get past the
    /// limit; one that goes without a hello gives its place back.
    #[test]
    fn connections_count_before_their_hello() {
        let mut k = Kept::default();
        k.limits.max_conns_per_ip = 3;
        let mut b = Broker::new(k);
        for _ in 0..3 {
            assert!(b.admit(ip(1)).is_ok());
        }
        assert_eq!(b.admit(ip(1)), Err("too many from one address"));
        b.unpend(ip(1));
        assert!(b.admit(ip(1)).is_ok(), "a place given back");
        // saying hello turns a waiting place into the connection itself
        let (t, _r) = channel(8);
        b.join(ip(1), "x".into(), None, t);
        assert_eq!(b.admit(ip(1)), Err("too many from one address"));
        // the relay's own network: never shut out
        assert!(b.admit(IpAddr::from([172, 18, 0, 2])).is_ok());
        // a whole IPv6 site: four times the one-address limit
        let v6 = |n: u16| IpAddr::V6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 7, n, 0, 0, 0, 1));
        for n in 0..12 {
            assert!(b.admit(v6(n)).is_ok());
        }
        assert_eq!(b.admit(v6(99)), Err("too many from one network"));
    }

    /// What's kept: only the kinds the village keeps; each address only
    /// so much; the relay's own never pushed out; a listing only by the
    /// player whose it is; a kept message handed to a newcomer only up to
    /// what its queue can take.
    #[test]
    fn what_is_kept_is_bounded_and_owned() {
        let mut k = Kept::default();
        k.limits.max_retained = 50;
        let mut b = Broker::new(k);
        b.kept.limits.burst = 1e9;
        b.kept.limits.msgs_per_sec = 1e9;
        b.say_squares();
        let (t, _r) = channel(64);
        let (a, _) = b.join(ip(1), "xt00002001aaaaaaaa0001".into(), None, t);
        // a village message with the keep flag: passed on, not kept
        b.publish(a, &topic_of("village", "ABCDEFGHJKLM"), b"x", true).unwrap();
        assert_eq!(b.retained.len(), 1);
        // one address's share
        for i in 0..400 {
            b.publish(a, &format!("xmas-tui/1/here/{i}"), b"x", true).unwrap();
        }
        assert!(b.retained.len() <= 51 && b.retained.contains_key(ADMIN_SQUARES), "the relay's own pushed out, or too many kept");
        assert!(b.owned.get(&ip(1)).is_some_and(|o| o.0 <= KEPT_PER_ADDR));
        // a listing: only the player whose id it carries
        b.publish(a, "xmas-tui/1/listed/00002001", b"mine", true).unwrap();
        b.publish(a, "xmas-tui/1/listed/00000001", b"the host's", true).unwrap();
        assert!(b.retained.contains_key("xmas-tui/1/listed/00002001"));
        assert!(!b.retained.contains_key("xmas-tui/1/listed/00000001"), "a listing kept for someone else");
        // a newcomer to the listings gets them through its own queue only
        let (tn, mut rn) = channel(4);
        let (n, _) = b.join(ip(2), "n".into(), None, tn);
        b.publish(a, "xmas-tui/1/listed/00002001", b"mine again", true).unwrap();
        assert_eq!(b.subscribe(n, &["xmas-tui/1/listed/+".into()]), vec![0]);
        assert_eq!(drain(&mut rn).len(), 1);
    }

    /// The relay's host, listening on `ADMIN_VIA`, hears each village
    /// message with the connection it came from; nobody else can listen
    /// there, and others hear the village as ever.
    #[test]
    fn the_host_is_told_who_sent_what() {
        let mut b = Broker::new(Kept::default());
        let v = topic_of("village", "ABCDEFGHJKLM");
        let (th, mut rh) = channel(64);
        let (h, _) = b.join_as(IpAddr::from([172, 18, 0, 3]), "xt00000002aa".into(), None, th, true);
        let (tp, mut rp) = channel(64);
        let (p, _) = b.join(ip(2), "xt00002002bbbbbbbb0001".into(), None, tp);
        let (tq, mut rq) = channel(64);
        let (q, _) = b.join(ip(3), "xt00002003cccccccc0001".into(), None, tq);
        assert_eq!(b.subscribe(h, &[v.clone(), ADMIN_VIA.into()]), vec![0, 0]);
        assert_eq!(b.subscribe(p, &[ADMIN_VIA.into(), format!("{ADMIN_VIA}/1/x")]), vec![0x80, 0x80]);
        b.subscribe(q, &[v.clone()]);
        b.publish(p, &v, b"hello", false).unwrap();
        let hash = v.rsplit('/').next().unwrap();
        assert_eq!(drain(&mut rh), vec![codec::publish(&format!("{ADMIN_VIA}/{p:x}/{hash}"), b"hello", false)]);
        assert_eq!(drain(&mut rq), vec![codec::publish(&v, b"hello", false)]);
        assert!(drain(&mut rp).is_empty());
    }

    /// Subscribing costs from the allowance too: a flood of subscriptions
    /// is refused, not served.
    #[test]
    fn subscribing_is_held_to_the_allowance() {
        let mut k = Kept::default();
        k.limits.burst = 2.0;
        k.limits.msgs_per_sec = 0.001;
        let mut b = Broker::new(k);
        let (t, _r) = channel(64);
        let (a, _) = b.join(ip(1), "a".into(), None, t);
        let filters: Vec<String> = (0..20).map(|i| format!("xmas-tui/1/here/{i}")).collect();
        let granted = b.subscribe(a, &filters);
        assert_eq!(granted.iter().filter(|g| **g == 0).count(), 8, "{granted:?}");
    }
}
