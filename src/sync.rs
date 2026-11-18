//! One villager on several devices (a phone, a laptop, the desktop):
//! linked by a link code, each device keeps the villager in step with the
//! others through the relay (see `crate::net`, `Online::sync_on`), sealed
//! with a key made from the code.
//!
//! Each device sends only its own side: what it's done (its share of the
//! counters — cheer earned and spent, snowballs thrown, steps — and what
//! it's found: neighbours greeted, homes visited), and the villager as last
//! changed there (their name and look, the pet, friends & family; the
//! village, should a new one be made). The relay keeps each device's last
//! word, so one that's been away catches up when it's back. Counters add
//! up across the devices, whatever order things happen in (and both being
//! played at once); for the villager, the newest change wins.

use crate::net::{Book, ChatLine, Group, Pal};
use crate::save::You;
use crate::scene::shop::Pockets;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Kept in the save (this device's own): the link, and what's been heard
/// from the others.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Linked {
    pub code: String,
    /// This device: its id and its name ("dash-pc", "phone").
    pub dev: String,
    pub label: String,
    /// When the villager (name, look, pet, friends & family…) and the
    /// village were last changed.
    pub you_at: i64,
    pub seed_at: i64,
    /// Just linked: the first word from another device makes this one its
    /// villager.
    pub joining: bool,
    /// The others' sides, by device id.
    pub others: BTreeMap<String, Theirs>,
    /// Friends unfriended (not to come back from the others' lists); groups
    /// left; chats cleared (each friend's: lines up to then gone).
    pub gone: Vec<String>,
    /// (each group's last leaving or joining: when, and left; the newest
    /// on any device wins)
    pub left: BTreeMap<String, (i64, bool)>,
    pub cleared: BTreeMap<String, i64>,
    /// The villager as last sent or taken (a difference: changed here).
    pub last_you: Option<You>,
    pub last_seed: u32,
    /// Running now (the others hear so), and the village open here.
    #[serde(skip)]
    pub live: bool,
    #[serde(skip)]
    pub open: String,
}

/// Another device's side, as last heard.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Theirs {
    pub label: String,
    /// When they sent it; when it came.
    pub at: i64,
    pub heard: i64,
    /// Their share of the counters (JSON).
    pub mine: String,
    /// Running (not closed), and the village they have open (its code).
    pub live: bool,
    pub open: String,
}

/// Two linked devices open at once: what this one should do so they're in
/// one village (see `Linked::together`).
#[derive(Clone, Debug, PartialEq)]
pub enum Together {
    /// Go into the other device's open village (its code, its name).
    Join(String, String),
    /// Open this one's village (the other comes in).
    Host,
    Alone,
}

/// What goes to the other devices.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct State {
    label: String,
    you: Option<You>,
    you_at: i64,
    seed: u32,
    seed_at: i64,
    code: String,
    friends: Vec<Pal>,
    asking: Vec<Pal>,
    gone: Vec<String>,
    left: BTreeMap<String, (i64, bool)>,
    cleared: BTreeMap<String, i64>,
    mine: String,
    /// The chats with friends (the latest lines of each, the most lately
    /// talked to first, as many as fit), and the groups.
    chats: BTreeMap<String, Vec<ChatLine>>,
    groups: Vec<Group>,
    /// Running, and the village open here (its code); a beat that changes
    /// every couple of minutes, so the others hear this one's still about.
    live: bool,
    open: String,
    beat: i64,
}

/// What a word from another device changed here.
#[derive(Default, Debug)]
pub struct Applied {
    pub pockets: Option<Pockets>,
    /// The villager, as changed there (apply it to the config).
    pub you: Option<You>,
    /// Another village (lay it out anew).
    pub seed: Option<u32>,
    /// The friend code and friends (a new friend code: link again).
    pub book: Option<Book>,
    /// Who it was from.
    pub from: String,
}

/// Kept on the device alone (not added up across them).
const LOCAL: &[&str] = &["cozy_until", "lantern_on"];
/// Lists of things found (all of them, from every device).
const SETS: &[&str] = &["greeted", "houses"];
/// The best (lowest, highest) of each device's.
const MIN: &[&str] = &["ski_best"];
const MAX: &[&str] = &["ski_score"];
/// Goals done, a bit each.
const BITS: &[&str] = &["done"];

fn obj(p: &Pockets) -> Map<String, Value> {
    match serde_json::to_value(p) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// A number like `like` (whole numbers stay whole, never below nothing).
fn like(like: &Value, x: f64) -> Value {
    if like.is_u64() || like.is_i64() {
        Value::from(x.round().max(0.0) as u64)
    } else {
        serde_json::Number::from_f64(x.max(0.0)).map(Value::Number).unwrap_or(Value::from(0))
    }
}

/// This device's share: what it has, less what came from the others (and
/// its lists, bests and goals as they are).
fn share(p: &Pockets, others: &BTreeMap<String, Theirs>) -> Map<String, Value> {
    let mine = obj(p);
    let theirs: Vec<Map<String, Value>> = others.values().filter_map(|t| serde_json::from_str::<Map<String, Value>>(&t.mine).ok()).collect();
    let mut out = Map::new();
    for (k, v) in &mine {
        if LOCAL.contains(&k.as_str()) {
            continue;
        }
        let special = SETS.contains(&k.as_str()) || MIN.contains(&k.as_str()) || MAX.contains(&k.as_str()) || BITS.contains(&k.as_str());
        let val = match v {
            _ if special => v.clone(),
            Value::Number(_) => {
                let sum: f64 = theirs.iter().filter_map(|t| t.get(k)).map(num).sum();
                // (a share can be below nothing: cheer earned there, spent here)
                let x = num(v) - sum;
                if v.is_u64() || v.is_i64() {
                    Value::from(x.round() as i64)
                } else {
                    serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::from(0))
                }
            }
            Value::Array(a) if a.iter().all(|e| e.is_number()) => Value::Array(
                a.iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let sum: f64 = theirs.iter().filter_map(|t| t.get(k).and_then(|x| x.get(i))).map(num).sum();
                        Value::from((num(e) - sum).round() as i64)
                    })
                    .collect(),
            ),
            _ => v.clone(),
        };
        out.insert(k.clone(), val);
    }
    out
}

/// A device's new share taken in, in place of its old one.
fn take_share(p: &Pockets, old: &Map<String, Value>, new: &Map<String, Value>) -> Pockets {
    let mut m = obj(p);
    for (k, n) in new {
        if LOCAL.contains(&k.as_str()) {
            continue;
        }
        let Some(cur) = m.get(k).cloned() else { continue };
        let o = old.get(k);
        let v = if SETS.contains(&k.as_str()) {
            let mut all: Vec<Value> = cur.as_array().cloned().unwrap_or_default();
            for e in n.as_array().into_iter().flatten() {
                if !all.contains(e) {
                    all.push(e.clone());
                }
            }
            Value::Array(all)
        } else if MIN.contains(&k.as_str()) {
            match (cur.as_f64(), n.as_f64()) {
                (Some(a), Some(b)) => like(&cur, a.min(b)),
                (None, Some(_)) => n.clone(),
                _ => cur,
            }
        } else if MAX.contains(&k.as_str()) {
            like(&cur, num(&cur).max(num(n)))
        } else if BITS.contains(&k.as_str()) {
            Value::from(cur.as_u64().unwrap_or(0) | n.as_u64().unwrap_or(0))
        } else {
            match (&cur, n) {
                (Value::Number(_), Value::Number(_)) => like(&cur, num(&cur) + num(n) - o.map(num).unwrap_or(0.0)),
                (Value::Array(a), Value::Array(b)) if a.iter().all(|e| e.is_number()) => Value::Array(
                    a.iter()
                        .enumerate()
                        .map(|(i, e)| {
                            let was = o.and_then(|o| o.get(i)).map(num).unwrap_or(0.0);
                            like(e, num(e) + b.get(i).map(num).unwrap_or(0.0) - was)
                        })
                        .collect(),
                ),
                (Value::Array(a), Value::Array(b)) => Value::Array(a.iter().zip(b.iter().chain(std::iter::repeat(&Value::Bool(false)))).map(|(x, y)| Value::Bool(x.as_bool().unwrap_or(false) || y.as_bool().unwrap_or(false))).collect()),
                (Value::Bool(a), Value::Bool(b)) => Value::Bool(*a || *b),
                _ => cur,
            }
        };
        m.insert(k.clone(), v);
    }
    serde_json::from_value(Value::Object(m)).unwrap_or_else(|_| p.clone())
}

/// A fresh device id.
fn new_dev() -> String {
    format!("{:08x}", crate::net::seal::random() as u32)
}

/// This device's name, for the others' lists.
pub fn device_name() -> String {
    #[cfg(target_os = "android")]
    {
        "phone".to_string()
    }
    #[cfg(target_arch = "wasm32")]
    {
        "a web browser".to_string()
    }
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        let host = std::env::var("COMPUTERNAME").ok().or_else(|| std::fs::read_to_string("/etc/hostname").ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        host.unwrap_or_else(|| if cfg!(windows) { "a Windows PC".into() } else { "a computer".into() })
    }
}

impl Linked {
    /// Linked by `code` (this device making it, or taking it from another:
    /// `joining`).
    pub fn new(code: &str, joining: bool) -> Linked {
        Linked { code: crate::net::seal::norm(code), dev: new_dev(), label: device_name(), joining, ..Default::default() }
    }

    /// The link code, to type on another device.
    pub fn pretty_code(&self) -> String {
        crate::net::seal::pretty(&self.code)
    }

    /// This device's side, to send (`now`: seconds since 1970). A change
    /// to the villager or the village made here is noted (the newest wins).
    pub fn state(&mut self, you: &You, seed: u32, pockets: &Pockets, book: &Book, now: i64) -> String {
        if self.last_you.as_ref() != Some(you) {
            if self.last_you.is_some() {
                self.you_at = now;
            }
            self.last_you = Some(you.clone());
        }
        if self.last_seed != seed {
            if self.last_seed != 0 {
                self.seed_at = now;
            }
            self.last_seed = seed;
        }
        let st = State {
            label: self.label.clone(),
            // (still joining: nothing of this device's to give but what it's done)
            you: (!self.joining).then(|| you.clone()),
            you_at: if self.joining { 0 } else { self.you_at },
            seed,
            seed_at: if self.joining { 0 } else { self.seed_at },
            code: if self.joining { String::new() } else { book.code.clone() },
            friends: book.friends.clone(),
            asking: book.asking.clone(),
            gone: self.gone.clone(),
            left: self.left.clone(),
            cleared: self.cleared.clone(),
            mine: serde_json::to_string(&share(pockets, &self.others)).unwrap_or_default(),
            chats: chats_to_send(book),
            groups: book.groups.clone(),
            live: self.live,
            open: self.open.clone(),
            beat: now / 120,
        };
        serde_json::to_string(&st).unwrap_or_default()
    }

    /// A friend unfriended here (so it doesn't come back from the others).
    pub fn unfriended(&mut self, code: &str) {
        let c = crate::net::seal::norm(code);
        if !self.gone.contains(&c) {
            self.gone.push(c);
        }
    }

    /// A group left here (`left`), or joined (again), `now`: the others do
    /// the same.
    pub fn left_group(&mut self, code: &str, left: bool, now: i64) {
        self.left.insert(crate::net::seal::norm(code), (now, left));
    }

    /// A friend's chat cleared here, up to `now`: the others' lines up to
    /// then don't bring it back.
    pub fn cleared_chat(&mut self, code: &str, now: i64) {
        let c = crate::net::seal::norm(code);
        let e = self.cleared.entry(c).or_insert(0);
        *e = (*e).max(now);
    }

    /// Another of this villager's devices open now too: into its village
    /// if it has one open; else the one first in line opens theirs.
    pub fn together(&self, now: i64) -> Together {
        let live: Vec<(&String, &Theirs)> = self.others.iter().filter(|(_, t)| t.live && now - t.heard < 330).collect();
        if let Some((_, t)) = live.iter().find(|(_, t)| t.open.len() == 12) {
            return Together::Join(t.open.clone(), t.label.clone());
        }
        if !live.is_empty() && live.iter().all(|(d, _)| self.dev.as_str() < d.as_str()) {
            return Together::Host;
        }
        Together::Alone
    }

    /// The others: (name, seconds since heard from).
    pub fn devices(&self, now: i64) -> Vec<(String, i64)> {
        self.others.values().map(|t| (t.label.clone(), now - t.heard)).collect()
    }

    /// Another device's side, taken in: what changed here.
    #[allow(clippy::too_many_arguments)]
    pub fn take(&mut self, dev: &str, at: i64, json: &str, now: i64, you: &You, seed: u32, pockets: &Pockets, book: &Book) -> Applied {
        let mut out = Applied::default();
        let Ok(st) = serde_json::from_str::<State>(json) else { return out };
        if dev == self.dev || st.code.is_empty() && self.joining {
            // (ours, or another device that's joining too: nothing to go by)
            if dev != self.dev && st.code.is_empty() {
                let old = self.others.get(dev).map(|t| t.mine.clone()).unwrap_or_default();
                self.take_counts(dev, &st, at, now, &old, pockets, &mut out);
            }
            return out;
        }
        if self.others.get(dev).is_some_and(|t| t.at > at) {
            return out;
        }
        out.from = st.label.clone();
        let old = self.others.get(dev).map(|t| t.mine.clone()).unwrap_or_default();
        let joining = std::mem::take(&mut self.joining);
        // the villager: theirs, if changed there since (or joining them)
        if let Some(theirs) = &st.you {
            if joining || st.you_at > self.you_at {
                if theirs != you {
                    out.you = Some(theirs.clone());
                }
                self.you_at = st.you_at;
                self.last_you = Some(theirs.clone());
            }
        }
        if (joining || st.seed_at > self.seed_at) && st.seed != 0 {
            if st.seed != seed {
                out.seed = Some(st.seed);
            }
            self.seed_at = st.seed_at;
            self.last_seed = st.seed;
        }
        // friends: everyone on either list (but those unfriended)
        for g in &st.gone {
            if !self.gone.contains(g) {
                self.gone.push(g.clone());
            }
        }
        let norm = crate::net::seal::norm;
        let mut b = book.clone();
        if joining && !st.code.is_empty() {
            b.code = st.code.clone();
        }
        let gone = &self.gone;
        for p in &st.friends {
            let known = b.friends.iter_mut().find(|q| norm(&q.code) == norm(&p.code));
            match known {
                Some(q) if q.waiting && !p.waiting => *q = p.clone(),
                Some(_) => {}
                None if !gone.contains(&norm(&p.code)) => b.friends.push(p.clone()),
                None => {}
            }
        }
        b.friends.retain(|p| !gone.contains(&norm(&p.code)));
        for p in &st.asking {
            if !b.asking.iter().any(|q| norm(&q.code) == norm(&p.code)) && !b.friends.iter().any(|q| norm(&q.code) == norm(&p.code)) && !gone.contains(&norm(&p.code)) {
                b.asking.push(p.clone());
            }
        }
        b.asking.retain(|p| !b.friends.iter().any(|q| norm(&q.code) == norm(&p.code)));
        // groups left, chats cleared, on either: so here too
        for (g, &(at, l)) in &st.left {
            let e = self.left.entry(norm(g)).or_insert((at, l));
            if at > e.0 {
                *e = (at, l);
            }
        }
        for (code, at) in &st.cleared {
            let e = self.cleared.entry(norm(code)).or_insert(0);
            *e = (*e).max(*at);
        }
        let cleared = &self.cleared;
        let left: Vec<String> = self.left.iter().filter(|(_, v)| v.1).map(|(k, _)| k.clone()).collect();
        // chats: every line either has, once, in order (but those cleared)
        for (code, lines) in &st.chats {
            let code = norm(code);
            if code.len() != 12 || gone.contains(&code) {
                continue;
            }
            let upto = cleared.get(&code).copied().unwrap_or(i64::MIN);
            let fresh: Vec<ChatLine> = lines.iter().filter(|l| l.at > upto).cloned().collect();
            let mine = b.chats.entry(code).or_default();
            merge_chat(mine, &fresh);
        }
        for (code, upto) in cleared {
            if let Some(v) = b.chats.get_mut(code) {
                v.retain(|l| l.at > *upto);
            }
        }
        b.chats.retain(|_, v| !v.is_empty());
        // groups: any either's in (but those left)
        for g in &st.groups {
            if norm(&g.code).len() == 12 && !left.contains(&norm(&g.code)) && !b.groups.iter().any(|h| norm(&h.code) == norm(&g.code)) && b.groups.len() < 30 {
                b.groups.push(Group { name: crate::net::clean(&g.name, 32), code: norm(&g.code) });
            }
        }
        b.groups.retain(|g| !left.contains(&norm(&g.code)));
        if &b != book {
            out.book = Some(b);
        }
        self.take_counts(dev, &st, at, now, &old, pockets, &mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn take_counts(&mut self, dev: &str, st: &State, at: i64, now: i64, old: &str, pockets: &Pockets, out: &mut Applied) {
        let old: Map<String, Value> = serde_json::from_str(old).unwrap_or_default();
        let new: Map<String, Value> = serde_json::from_str(&st.mine).unwrap_or_default();
        if old != new {
            out.pockets = Some(take_share(pockets, &old, &new));
        }
        self.others.insert(dev.to_string(), Theirs { label: st.label.clone(), at, heard: now, mine: st.mine.clone(), live: st.live, open: crate::net::seal::norm(&st.open) });
    }
}

/// The chats to send the other devices: each friend's latest lines (20 at
/// most), the most lately talked to first, while they fit (some 32 KB: the
/// relay takes what's kept on it in small pieces).
fn chats_to_send(book: &Book) -> BTreeMap<String, Vec<ChatLine>> {
    let mut by_latest: Vec<(&String, &Vec<ChatLine>)> = book.chats.iter().filter(|(_, v)| !v.is_empty()).collect();
    by_latest.sort_by_key(|(_, v)| std::cmp::Reverse(v.last().map(|l| l.at).unwrap_or(0)));
    let mut out = BTreeMap::new();
    let mut size = 0;
    for (code, lines) in by_latest {
        let keep: Vec<ChatLine> = lines.iter().rev().take(20).rev().cloned().collect();
        size += keep.iter().map(|l| l.text.len() + l.invite.len() + 40).sum::<usize>();
        if size > 32 * 1024 {
            break;
        }
        out.insert(code.clone(), keep);
    }
    out
}

/// Another device's lines of a chat taken in: each line once (the same
/// time, side and words: the same line), in order, the latest kept.
fn merge_chat(mine: &mut Vec<ChatLine>, theirs: &[ChatLine]) {
    for l in theirs.iter().take(60) {
        let text = crate::net::clean(&l.text, 300);
        // (a line from the friend that both devices heard: the same number
        // from their side; or, from an older version without one, the same
        // words within a couple of minutes, each device having stamped it
        // as it came. This villager's own: the same time, the same words.)
        let same = |m: &ChatLine| {
            m.mine == l.mine
                && if !l.mine && l.n != 0 && m.n != 0 {
                    m.n == l.n
                } else {
                    m.text == text && if l.mine { m.at == l.at } else { (m.at - l.at).abs() <= 120 }
                }
        };
        match mine.iter_mut().find(|m| same(m)) {
            // (got there, as either side knows it)
            Some(m) => {
                if l.state == crate::net::sent::THERE && m.state != crate::net::sent::THERE {
                    m.state = crate::net::sent::THERE;
                }
            }
            None => mine.push(ChatLine { text, invite: crate::net::seal::norm(&l.invite), ..l.clone() }),
        }
    }
    mine.sort_by_key(|l| l.at);
    if mine.len() > crate::net::CHAT_KEPT {
        let extra = mine.len() - crate::net::CHAT_KEPT;
        mine.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn you(name: &str) -> You {
        You { name: name.into(), ..Default::default() }
    }

    /// Two devices open at once: the first in line opens its village, the
    /// other goes in; one closed (or quiet a while) is left alone.
    #[test]
    fn chats_groups_and_the_friend_code_come_across() {
        let pk = Pockets::default();
        let mut desk = Linked::new("ABCD-EFGH-JKLM", false);
        let mut phone = Linked::new("ABCD-EFGH-JKLM", true);
        (desk.dev, phone.dev) = ("desk".into(), "phone".into());
        let sam = "SAMSAMSAMSAM".to_string();
        let line = |at: i64, mine: bool, t: &str| ChatLine { at, mine, text: t.into(), ..Default::default() };
        let mut bd = Book { code: "DESKDESKDESK".into(), ..Default::default() };
        bd.friends.push(Pal { name: "Sam".into(), code: sam.clone(), ..Default::default() });
        bd.chats.insert(sam.clone(), vec![line(10, false, "snowball fight?"), line(12, true, "you're on")]);
        bd.groups.push(Group { name: "carols".into(), code: "CAROLCAROLCA".into() });
        let bp = Book { code: "PHONEPHONEPH".into(), ..Default::default() };
        let now = 1_000;
        // the phone joins: the desk's friend code, friends, chats, groups
        let sd = desk.state(&you("Robin"), 5, &pk, &bd, now);
        let got = phone.take("desk", now, &sd, now, &you("x"), 9, &pk, &bp).book.expect("the book");
        assert_eq!(got.code, bd.code);
        assert_eq!(got.friends.len(), 1);
        assert_eq!(got.chats[&sam].iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["snowball fight?", "you're on"]);
        assert_eq!(got.groups.len(), 1);
        // a line written on the phone comes back to the desk, once; the
        // desk's own lines aren't doubled
        let mut bp = got;
        bp.chats.get_mut(&sam).unwrap().push(line(20, true, "see you at the tree"));
        let sp = phone.state(&you("Robin"), 5, &pk, &bp, now + 5);
        let back = desk.take("phone", now + 5, &sp, now + 5, &you("Robin"), 5, &pk, &bd).book.expect("the book");
        let texts: Vec<&str> = back.chats[&sam].iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["snowball fight?", "you're on", "see you at the tree"]);
    }

    /// A group left, a chat cleared, on one device: the other's word
    /// doesn't bring them back (joining again does); a friend's line both
    /// devices heard is kept once, though each stamped it as it came.
    #[test]
    fn leaving_and_clearing_stick_and_lines_heard_twice_show_once() {
        let pk = Pockets::default();
        let mut desk = Linked::new("ABCD-EFGH-JKLM", false);
        let mut phone = Linked::new("ABCD-EFGH-JKLM", false);
        (desk.dev, phone.dev) = ("desk".into(), "phone".into());
        let sam = "SAMSAMSAMSAM".to_string();
        let carols = Group { name: "carols".into(), code: "CAROLCAROLCA".into() };
        let mut bd = Book { code: "DESKDESKDESK".into(), ..Default::default() };
        bd.friends.push(Pal { name: "Sam".into(), code: sam.clone(), ..Default::default() });
        let mut bp = bd.clone();
        bp.groups.push(carols.clone());
        bp.chats.insert(sam.clone(), vec![ChatLine { at: 100, mine: false, text: "hi".into(), n: 7, ..Default::default() }]);
        // the desk left the group and cleared the chat (it had them too)
        desk.left_group(&carols.code, true, 200);
        desk.cleared_chat(&sam, 200);
        let sp = phone.state(&you("Robin"), 5, &pk, &bp, 300);
        let got = desk.take("phone", 300, &sp, 300, &you("Robin"), 5, &pk, &bd).book.unwrap_or(bd.clone());
        assert!(got.groups.is_empty(), "the group came back");
        assert!(got.chats.get(&sam).is_none_or(|v| v.is_empty()), "the chat came back");
        // and the phone, hearing the desk, does the same
        let sd = desk.state(&you("Robin"), 5, &pk, &got, 310);
        let bp2 = phone.take("desk", 310, &sd, 310, &you("Robin"), 5, &pk, &bp).book.expect("the phone's book");
        assert!(bp2.groups.is_empty() && bp2.chats.get(&sam).is_none_or(|v| v.is_empty()));
        // joined again on the phone, later: back on both
        phone.left_group(&carols.code, false, 400);
        let mut bp3 = bp2.clone();
        bp3.groups.push(carols.clone());
        let sp = phone.state(&you("Robin"), 5, &pk, &bp3, 400);
        let got = desk.take("phone", 400, &sp, 400, &you("Robin"), 5, &pk, &got).book.expect("the desk's book");
        assert_eq!(got.groups.len(), 1, "joined again");
        // a line from Sam both heard, stamped a second apart: once
        let mut bd = got;
        bd.chats.insert(sam.clone(), vec![ChatLine { at: 500, mine: false, text: "snow!".into(), n: 9, ..Default::default() }]);
        bp3.chats.insert(sam.clone(), vec![ChatLine { at: 501, mine: false, text: "snow!".into(), n: 9, ..Default::default() }]);
        let sp = phone.state(&you("Robin"), 5, &pk, &bp3, 510);
        let got = desk.take("phone", 510, &sp, 510, &you("Robin"), 5, &pk, &bd).book.unwrap_or(bd.clone());
        assert_eq!(got.chats[&sam].iter().filter(|l| l.text == "snow!").count(), 1);
    }

    #[test]
    fn two_devices_open_at_once_share_a_village() {
        let (book, pk) = (Book::default(), Pockets::default());
        let mut a = Linked::new("ABCD-EFGH-JKLM", false);
        let mut b = Linked::new("ABCD-EFGH-JKLM", false);
        (a.dev, b.dev) = ("aaa".into(), "bbb".into());
        (a.live, b.live) = (true, true);
        let now = 1_000_000;
        let sa = a.state(&you("Robin"), 5, &pk, &book, now);
        let sb = b.state(&you("Robin"), 5, &pk, &book, now);
        b.take("aaa", now, &sa, now, &you("Robin"), 5, &pk, &book);
        a.take("bbb", now, &sb, now, &you("Robin"), 5, &pk, &book);
        assert_eq!(a.together(now), Together::Host);
        assert_eq!(b.together(now), Together::Alone);
        a.open = "WXYZWXYZWXYZ".into();
        let sa = a.state(&you("Robin"), 5, &pk, &book, now + 5);
        b.take("aaa", now + 5, &sa, now + 5, &you("Robin"), 5, &pk, &book);
        assert!(matches!(b.together(now + 5), Together::Join(code, _) if code == "WXYZWXYZWXYZ"));
        // gone quiet (no beat for a good while): not waited on
        assert_eq!(b.together(now + 1000), Together::Alone);
    }

    /// Two devices played at once, each told of the other now and then:
    /// what both did adds up; spent cheer stays spent; the villager's
    /// newest look wins; friends from either.
    #[test]
    fn two_devices_keep_one_villager() {
        let mut desk = Linked::new("ABCD-EFGH-JKLM", false);
        let mut phone = Linked::new("ABCD-EFGH-JKLM", true);
        let (mut pd, mut pp) = (Pockets::default(), Pockets::default());
        pd.cheer = 10;
        pd.steps = 100.0;
        pd.greeted = vec![1, 2];
        pp.cheer = 3; // (the phone's own, before linking)
        pp.greeted = vec![2, 7];
        let mut bd = Book { code: "DESKCODE2345".into(), friends: vec![Pal { name: "Sam".into(), code: "SAMSAMSAMSAM".into(), waiting: false, quiet: false }], ..Default::default() };
        let bp = Book { code: "PHONECODE234".into(), ..Default::default() };
        let (mut yd, mut yp) = (you("Robin"), you("phone person"));
        let (sd, sp) = (111u32, 222u32);
        // the desk speaks first; the phone, joining, becomes Robin
        let st = desk.state(&yd, sd, &pd, &bd, 1000);
        let a = phone.take(&desk.dev.clone(), 1000, &st, 1001, &yp, sp, &pp, &bp);
        assert_eq!(a.you.as_ref().map(|y| y.name.as_str()), Some("Robin"));
        assert_eq!(a.seed, Some(111));
        let bp = a.book.expect("the desk's friend code");
        assert_eq!(bp.code, "DESKCODE2345");
        assert_eq!(bp.friends.len(), 1);
        yp = a.you.unwrap();
        let sp = a.seed.unwrap();
        pp = a.pockets.expect("counts");
        assert_eq!(pp.cheer, 13);
        assert_eq!(pp.greeted.len(), 3);
        // the phone tells the desk: the desk adds the phone's own three
        let st = phone.state(&yp, sp, &pp, &bp, 1002);
        let a = desk.take(&phone.dev.clone(), 1002, &st, 1003, &yd, sd, &pd, &bd);
        pd = a.pockets.expect("counts");
        assert_eq!(pd.cheer, 13);
        assert!(a.you.is_none() && a.seed.is_none());
        // both played at once: the desk earns 5, the phone spends 4 and
        // walks a bit
        pd.cheer += 5;
        pp.cheer -= 4;
        pp.steps += 20.0;
        let st_d = desk.state(&yd, sd, &pd, &bd, 2000);
        let st_p = phone.state(&yp, sp, &pp, &bp, 2000);
        let a = desk.take(&phone.dev.clone(), 2000, &st_p, 2001, &yd, sd, &pd, &bd);
        pd = a.pockets.unwrap();
        let a = phone.take(&desk.dev.clone(), 2000, &st_d, 2001, &yp, sp, &pp, &bp);
        pp = a.pockets.unwrap();
        assert_eq!((pd.cheer, pp.cheer), (14, 14));
        assert!((pd.steps - 120.0).abs() < 1e-3 && (pp.steps - 120.0).abs() < 1e-3);
        // and again, nothing new: nothing changes (no counting twice)
        let st_d = desk.state(&yd, sd, &pd, &bd, 2100);
        let a = phone.take(&desk.dev.clone(), 2100, &st_d, 2101, &yp, sp, &pp, &bp);
        assert!(a.pockets.is_none() || a.pockets.as_ref().unwrap().cheer == 14);
        // a new look on the phone: the desk takes it
        yp.motto = "snow is the best".into();
        let st_p = phone.state(&yp, sp, &pp, &bp, 3000);
        let a = desk.take(&phone.dev.clone(), 3000, &st_p, 3001, &yd, sd, &pd, &bd);
        assert_eq!(a.you.as_ref().map(|y| y.motto.as_str()), Some("snow is the best"));
        yd = a.you.unwrap();
        // unfriended on the desk: gone from the phone too
        desk.unfriended("SAMSAMSAMSAM");
        bd.friends.clear();
        let st_d = desk.state(&yd, sd, &pd, &bd, 3100);
        let a = phone.take(&desk.dev.clone(), 3100, &st_d, 3101, &yp, sp, &pp, &bp);
        assert!(a.book.expect("friends").friends.is_empty());
        // (saves and comes back)
        let toml = toml::to_string(&desk).unwrap();
        let back: Linked = toml::from_str(&toml).unwrap();
        assert_eq!(back, desk);
    }
}
