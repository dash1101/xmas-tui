//! The relay's own host (`xmas-tui --host`): keeps the public squares and
//! the server villages open on the village's relay, the same for everyone,
//! all the time. No window and no sound: each village goes on unseen, its
//! folk out and about for whoever's in it. It does what the relay's
//! console asks (send someone home, keep them quiet, announce, open or
//! close a village), greets newcomers, and says who's where and what's
//! been said there.
//!
//! Settings (environment): XMAS_RELAY, the relay (for example
//! `ws://relay:8080/mqtt`); HOST_TOKEN, the secret it signs in with (the
//! relay's own, at least 16 letters).

use crate::config::SceneCfg;
use crate::net::{self, tcp::Tcp, Online};
use crate::scene::Scene;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

/// What the relay says the host is to keep open, and how.
#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct HostCfg {
    squares: bool,
    servers: Vec<ServerCfg>,
    muted: BTreeMap<String, Vec<u32>>,
    welcome: BTreeMap<String, String>,
    square_rules: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct ServerCfg {
    name: String,
    code: String,
    public: bool,
    limit: u16,
    seed: u32,
    rules: serde_json::Value,
}

/// A village kept open: what it is, and the village itself (its folk, its
/// online side) with its link to the relay.
struct Kept {
    what: (String, Option<usize>, usize, bool, u32, serde_json::Value),
    scene: Scene,
    link: Option<Tcp>,
    /// Who's been greeted.
    greeted: HashSet<u32>,
}

/// A village's settings over the usual (time of day, folk, surprises…):
/// only the keys the village's settings have.
fn scene_cfg(rules: &serde_json::Value) -> SceneCfg {
    let mut base = serde_json::to_value(SceneCfg::default()).unwrap_or_default();
    if let (Some(b), Some(r)) = (base.as_object_mut(), rules.as_object()) {
        for (k, v) in r {
            if b.contains_key(k) {
                b.insert(k.clone(), v.clone());
            }
        }
    }
    let mut c: SceneCfg = serde_json::from_value(base).unwrap_or_default();
    // (nobody of the host's own walks about)
    c.player = false;
    c
}

impl Kept {
    fn open(id: u32, token: &str, name: &str, code: &str, square: Option<usize>, limit: usize, listed: bool, seed: u32, rules: serde_json::Value) -> Kept {
        let mut scene = Scene::new(200, 56, seed, &scene_cfg(&rules));
        let mut o = Online::server(id, name, token);
        o.keep_open(code, square, limit, listed);
        scene.put_online(o);
        Kept { what: (code.to_string(), square, limit, listed, seed, rules), scene, link: None, greeted: HashSet::new() }
    }

    fn step(&mut self, dt: f32, welcome: Option<&String>) {
        self.scene.update(dt, 0.0);
        net::tcp::pump(&mut self.scene.online, &mut self.link);
        // newcomers: greeted (those there when the host came: not)
        let here: Vec<u32> = self.scene.online.players().into_iter().map(|p| p.0).collect();
        for id in &here {
            if self.greeted.insert(*id) {
                if let Some(w) = welcome {
                    self.scene.online.notice(*id, w);
                }
            }
        }
        self.greeted.retain(|id| here.contains(id));
    }

    /// Closed: a goodbye, and the link let go.
    fn close(mut self) {
        self.scene.online.leave_village();
        net::tcp::pump(&mut self.scene.online, &mut self.link);
    }
}

pub fn run() -> Result<(), String> {
    let relay = std::env::var("XMAS_RELAY").map_err(|_| "XMAS_RELAY isn't set (the relay, e.g. ws://relay:8080/mqtt)".to_string())?;
    let token = std::env::var("HOST_TOKEN").ok().map(|t| t.trim().to_string()).filter(|t| t.len() >= 16).ok_or("HOST_TOKEN isn't set (the relay's, at least 16 letters)")?;
    // (that relay and no other: never a public broker)
    std::env::set_var("XMAS_RELAY_ONLY", "1");
    eprintln!("xmas-tui {} host: through {relay}", env!("CARGO_PKG_VERSION"));
    let mut control = Online::server(1, "the relay's host", &token);
    let mut control_link: Option<Tcp> = None;
    let mut cfg = HostCfg::default();
    let mut kept: BTreeMap<String, Kept> = BTreeMap::new();
    let mut next_id = 2u32;
    let start = Instant::now();
    let (mut last, mut reported, mut looked) = (start, -10.0f32, -100.0f32);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.5);
        last = now;
        let t = now.duration_since(start).as_secs_f32();
        control.tick(t, None, 0);
        net::tcp::pump(&mut control, &mut control_link);
        // what the relay's said: the villages to keep, and things to do
        for (topic, payload) in std::mem::take(&mut control.admin_in) {
            if topic == net::ADMIN_HOST {
                if let Ok(c) = serde_json::from_slice::<HostCfg>(&payload) {
                    cfg = c;
                    reconcile(&cfg, &mut kept, &mut next_id, &token);
                }
            } else if let Ok(cmd) = serde_json::from_slice::<serde_json::Value>(&payload) {
                obey(&cmd, &mut kept);
            }
        }
        for (name, k) in kept.iter_mut() {
            k.scene.online.set_muted(cfg.muted.get(name).cloned().unwrap_or_default());
            k.step(dt, cfg.welcome.get(name));
        }
        // the public list, looked at now and then (for the console)
        if t - looked > 30.0 {
            looked = t;
            control.watch_public();
        }
        if t - reported >= 3.0 {
            reported = t;
            report(&mut control, &mut kept);
        }
    }
}

/// The villages kept open, as the relay has them now: new ones opened,
/// changed ones opened afresh, others closed.
fn reconcile(cfg: &HostCfg, kept: &mut BTreeMap<String, Kept>, next_id: &mut u32, token: &str) {
    let mut want: BTreeMap<String, (String, Option<usize>, usize, bool, u32, serde_json::Value)> = BTreeMap::new();
    if cfg.squares {
        for (i, name) in net::SQUARES.iter().enumerate() {
            let rules = cfg.square_rules.get(*name).cloned().unwrap_or(serde_json::Value::Null);
            want.insert(name.to_string(), (net::square_code(i, 1), Some(i), net::SQUARE_LIMIT, true, net::square_seed(i), rules));
        }
    }
    for s in &cfg.servers {
        let code = net::seal::norm(&s.code);
        if s.name.trim().is_empty() || code.len() != 12 {
            continue;
        }
        want.insert(s.name.trim().to_string(), (code, None, (s.limit as usize).clamp(2, net::MAX_LIMIT), s.public, s.seed.max(1), s.rules.clone()));
    }
    let gone: Vec<String> = kept.iter().filter(|(n, k)| want.get(*n) != Some(&k.what)).map(|(n, _)| n.clone()).collect();
    for n in gone {
        if let Some(k) = kept.remove(&n) {
            eprintln!("xmas-tui host: closing {n}");
            k.close();
        }
    }
    for (name, w) in want {
        if kept.contains_key(&name) {
            continue;
        }
        let id = *next_id;
        *next_id = if *next_id + 1 >= net::HOST_IDS { 2 } else { *next_id + 1 };
        eprintln!("xmas-tui host: keeping {name} open");
        let (code, square, limit, listed, seed, rules) = w;
        kept.insert(name.clone(), Kept::open(id, token, &name, &code, square, limit, listed, seed, rules));
    }
}

/// What the console asked: someone sent home, an announcement, a village
/// closed (banned).
fn obey(cmd: &serde_json::Value, kept: &mut BTreeMap<String, Kept>) {
    let pick = |slot: &str, name: &str| slot == "*" || slot == name;
    if let Some(k) = cmd.get("kick") {
        let (slot, id) = (k["slot"].as_str().unwrap_or("*"), k["id"].as_u64().unwrap_or(0) as u32);
        for (name, v) in kept.iter_mut().filter(|(n, _)| pick(slot, n)) {
            if v.scene.online.players().iter().any(|p| p.0 == id) {
                eprintln!("xmas-tui host: sending {id:08x} home from {name}");
                v.scene.online.kick(id);
            }
        }
    }
    if let Some(s) = cmd.get("say") {
        let (to, text) = (s["to"].as_str().unwrap_or("*"), s["text"].as_str().unwrap_or(""));
        if !text.trim().is_empty() {
            for (_, v) in kept.iter_mut().filter(|(n, _)| pick(to, n)) {
                v.scene.online.notice(0, text);
            }
        }
    }
    if let Some(code) = cmd.get("close").and_then(|c| c.as_str()) {
        let code = net::seal::norm(code);
        let gone: Vec<String> = kept.iter().filter(|(_, k)| k.what.0 == code).map(|(n, _)| n.clone()).collect();
        for n in gone {
            if let Some(k) = kept.remove(&n) {
                k.close();
            }
        }
    }
}

/// To the relay: each village kept (who's there, what's been said since),
/// and the public list as it stands.
fn report(control: &mut Online, kept: &mut BTreeMap<String, Kept>) {
    for (name, k) in kept.iter_mut() {
        let o = &k.scene.online;
        let muted = o.muted_here();
        let players: Vec<serde_json::Value> = o.players().into_iter().map(|(id, n)| serde_json::json!({ "id": id, "name": n, "muted": muted.contains(&id) })).collect();
        let chat: Vec<serde_json::Value> = o.said_here.iter().map(|(at, id, n, text)| serde_json::json!({ "at": at, "id": id, "name": n, "text": text })).collect();
        let (code, square, limit, listed, ..) = &k.what;
        let v = serde_json::json!({
            "name": name,
            "kind": if square.is_some() { "square" } else { "server village" },
            "code": code,
            "public": listed,
            "n": players.len(),
            "limit": limit,
            "players": players,
            "chat": chat,
            "online": o.link() == net::Link::On,
        });
        if let Ok(b) = serde_json::to_vec(&v) {
            control.publish_raw(&format!("{}{}", net::ADMIN_REPORT, name), &b, false);
        }
    }
    let listings: Vec<serde_json::Value> = control.listings().into_iter().map(|l| serde_json::json!({ "name": l.name, "code": l.code, "n": l.n, "limit": l.limit, "server": l.server })).collect();
    if let Ok(b) = serde_json::to_vec(&serde_json::json!({ "listings": listings })) {
        control.publish_raw(&format!("{}_public", net::ADMIN_REPORT), &b, false);
    }
}
