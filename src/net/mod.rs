//! Friends and shared villages, online.
//!
//! Everyone talks through a free public MQTT broker (a relay: it passes
//! messages along and keeps nothing but each villager's last "here I am").
//! What goes through it is sealed end to end (see `seal`) with keys made
//! from codes, on topics named by hashes of them, so the relay, or anyone
//! listening, sees only noise:
//! - a **friend code** is the key to someone's mailbox (friend requests and
//!   messages) and to their "here I am" (online or not, hosting a village);
//! - a **village code** is the key to a shared village while it's open.
//!
//! The villager hosting a village keeps it: everyone in it builds the same
//! village from the host's seed (laid out the same on any screen), and
//! each player's own computer sends where its villager is. Nothing runs
//! anywhere but on the players' own machines.
//!
//! `Online` is only the conversation: bytes in (`feed`), bytes out
//! (`take_out`); the front end carries them (a TCP connection, see `tcp`,
//! or the web page's WebSocket).

pub mod mqtt;
pub mod seal;
#[cfg(not(target_arch = "wasm32"))]
pub mod tcp;

use seal::Seal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The relays: (host, TCP port, WebSocket address), free public brokers,
/// tried in turn. (The web page reads relays.json beside it first.)
pub const RELAYS: &[(&str, u16, &str)] = &[
    ("broker.emqx.io", 1883, "wss://broker.emqx.io:8084/mqtt"),
    ("broker.hivemq.com", 1883, "wss://broker.hivemq.com:8884/mqtt"),
    ("test.mosquitto.org", 1883, "wss://test.mosquitto.org:8081/mqtt"),
];

/// Kept in the save: online or not, this villager's friend code, friends.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Book {
    pub on: bool,
    pub code: String,
    pub friends: Vec<Pal>,
    /// Asking to be this villager's friend, waiting for an answer.
    pub asking: Vec<Pal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Pal {
    pub name: String,
    pub code: String,
    /// Asked; no answer yet.
    pub waiting: bool,
}

/// Where a villager is and how they look: sent to everyone in a shared
/// village. x, y are "village" coordinates: from the big tree, in its
/// heights (the same on any screen).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Avatar {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub moving: bool,
    pub air: bool,
    pub coat: u32,
    pub hat: u32,
    /// 0 out and about, 1 indoors, 2 away on a trip.
    pub away: u8,
}

/// A code onto the clipboard (where there's a way to).
pub fn copy_text(text: &str) {
    #[cfg(any(target_os = "linux", windows))]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let text = text.to_string();
        std::thread::spawn(move || {
            #[cfg(windows)]
            let tries: &[(&str, &[&str])] = &[("clip", &[])];
            #[cfg(target_os = "linux")]
            let tries: &[(&str, &[&str])] = &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"]), ("xsel", &["--clipboard", "--input"])];
            for (cmd, args) in tries {
                let Ok(mut c) = Command::new(cmd).args(*args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { continue };
                if let Some(mut i) = c.stdin.take() {
                    let _ = i.write_all(text.as_bytes());
                }
                if c.wait().is_ok_and(|s| s.success()) {
                    return;
                }
            }
        });
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = text;
}

/// What's on the clipboard (where there's a way to read it).
pub fn paste_text() -> String {
    #[cfg(any(target_os = "linux", windows))]
    {
        use std::process::{Command, Stdio};
        #[cfg(windows)]
        let tries: &[(&str, &[&str])] = &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])];
        #[cfg(target_os = "linux")]
        let tries: &[(&str, &[&str])] = &[("wl-paste", &["--no-newline"]), ("xclip", &["-selection", "clipboard", "-o"]), ("xsel", &["--clipboard", "--output"])];
        for (cmd, args) in tries {
            if let Ok(o) = Command::new(cmd).args(*args).stdin(Stdio::null()).stderr(Stdio::null()).output() {
                if o.status.success() {
                    return String::from_utf8_lossy(&o.stdout).chars().take(400).collect();
                }
            }
        }
    }
    String::new()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "k")]
enum Msg {
    /// In a mailbox: asking to be friends (`back`: saying yes).
    Hi { from: String, name: String, back: bool },
    /// In a mailbox: a message.
    Mail { from: String, name: String, text: String },
    /// One's own "here I am" (kept by the relay): online or not, hosting
    /// (the village code, for friends to join), when.
    Here { name: String, on: bool, village: Option<String>, at: i64 },
    /// In a village: someone's villager.
    Me { id: u32, name: String, av: Avatar },
    /// In a village: said aloud.
    Talk { id: u32, text: String },
    /// In a village: a snowball thrown (village coordinates, per second).
    Throw { id: u32, x: f32, y: f32, vx: f32, vy: f32 },
    /// In a village, from its host: the village itself.
    Village { host: u32, name: String, seed: u32 },
    /// In a village: gone (`host`: the village closes).
    Bye { id: u32, host: bool },
}

/// What happened, for the scene to show.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A line for the post (top left).
    Note(String),
    Mail { from: String, text: String },
    /// Someone's asking to be friends.
    Asked(String),
    /// Into a shared village: its seed (the village made anew from it).
    Village(u32),
    /// Out of the shared village (home again).
    Home,
    Said { id: u32, text: String },
    Threw { id: u32, x: f32, y: f32, vx: f32, vy: f32 },
}

/// The link to the relay, as the menus show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    Off,
    Connecting,
    On,
}

/// Someone else in the shared village.
#[derive(Clone, Debug)]
pub struct Other {
    pub name: String,
    pub av: Avatar,
    /// Where they're drawn (eased toward where they are).
    pub shown: (f32, f32),
    pub seen: f32,
    pub host: bool,
}

/// A friend's last "here I am".
#[derive(Clone, Debug, Default)]
pub struct Here {
    pub on: bool,
    pub village: Option<String>,
}

struct Live {
    seal: Seal,
    here_topic: String,
    here: Option<Here>,
}

struct Shared {
    code: String,
    seal: Seal,
    topic: String,
    host: bool,
    /// The host's seed, once heard (a guest).
    seed: Option<u32>,
    host_seen: f32,
    others: HashMap<u32, Other>,
}

pub struct Online {
    pub book: Book,
    /// The book's changed (worth saving soon).
    pub dirty: bool,
    name: String,
    id: u32,
    me: Seal,
    my_mail: String,
    my_here: String,
    live: Vec<Live>,
    village: Option<Shared>,
    reader: mqtt::Reader,
    out: Vec<u8>,
    link: Link,
    sub: u16,
    t: f32,
    next_ping: f32,
    next_here: f32,
    next_me: f32,
    next_village: f32,
    last_me: Option<Avatar>,
    events: Vec<Event>,
}

fn now() -> i64 {
    crate::clock::now().timestamp()
}

impl Online {
    pub fn new(mut book: Book, name: &str) -> Online {
        if seal::norm(&book.code).len() != 12 {
            book.code = seal::new_code();
        }
        let code = book.code.clone();
        let mut o = Online {
            name: name.to_string(),
            id: seal::random() as u32 | 1,
            me: Seal::new(&code),
            my_mail: seal::topic("mail", &code),
            my_here: seal::topic("here", &code),
            live: vec![],
            village: None,
            reader: mqtt::Reader::default(),
            out: vec![],
            link: Link::Off,
            sub: 1,
            t: 0.0,
            next_ping: 0.0,
            next_here: 0.0,
            next_me: 0.0,
            next_village: 0.0,
            last_me: None,
            events: vec![],
            dirty: false,
            book,
        };
        o.relive();
        o
    }

    fn relive(&mut self) {
        self.live = self.book.friends.iter().map(|p| Live { seal: Seal::new(&p.code), here_topic: seal::topic("here", &p.code), here: None }).collect();
    }

    pub fn set_name(&mut self, name: &str) {
        if self.name != name {
            self.name = name.to_string();
            self.next_here = 0.0;
        }
    }

    /// The friend code to give out.
    pub fn code(&self) -> String {
        seal::pretty(&self.book.code)
    }

    /// Whether there should be a link to the relay.
    pub fn wanted(&self) -> bool {
        self.book.on
    }

    pub fn link(&self) -> Link {
        self.link
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Bytes for the relay.
    pub fn take_out(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    fn note(&mut self, s: impl Into<String>) {
        self.events.push(Event::Note(s.into()));
    }

    fn send(&mut self, topic: &str, seal: &Seal, m: &Msg, retain: bool) {
        if self.link != Link::On {
            return;
        }
        let plain = serde_json::to_vec(m).unwrap_or_default();
        self.out.extend(mqtt::publish(topic, &seal.seal(&plain), retain));
    }

    fn here_msg(&self, on: bool) -> Msg {
        let village = self.village.as_ref().filter(|v| v.host).map(|v| v.code.clone());
        Msg::Here { name: self.name.clone(), on, village, at: now() }
    }

    fn subscribe(&mut self, topics: Vec<String>) {
        if self.link == Link::Connecting || self.link == Link::On {
            self.sub = self.sub.wrapping_add(1).max(1);
            self.out.extend(mqtt::subscribe(self.sub, &topics));
        }
    }

    /// The link's up: hello to the relay (and a will: "gone", should it
    /// drop), the topics to hear, and "here I am".
    pub fn connected(&mut self) {
        self.reader.clear();
        self.link = Link::Connecting;
        let will = self.me.seal(&serde_json::to_vec(&self.here_msg(false)).unwrap_or_default());
        let client = format!("xt{:08x}{:04x}", self.id, seal::random() as u16);
        self.out.extend(mqtt::connect(&client, 45, Some((&self.my_here.clone(), &will))));
    }

    fn on_connack(&mut self) {
        self.link = Link::On;
        let mut topics = vec![self.my_mail.clone()];
        topics.extend(self.live.iter().map(|l| l.here_topic.clone()));
        if let Some(v) = &self.village {
            topics.push(v.topic.clone());
        }
        self.subscribe(topics);
        self.next_here = 0.0;
        self.next_ping = self.t + 30.0;
        self.note("✉ online");
    }

    pub fn disconnected(&mut self) {
        if self.link != Link::Off {
            self.link = Link::Off;
        }
    }

    /// Bytes from the relay.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.reader.feed(bytes);
        while let Some(p) = self.reader.next() {
            match p {
                mqtt::Packet::ConnAck(0) => self.on_connack(),
                mqtt::Packet::ConnAck(_) => self.link = Link::Off,
                mqtt::Packet::Publish { topic, payload } => self.got(&topic, &payload),
                _ => {}
            }
        }
    }

    fn got(&mut self, topic: &str, payload: &[u8]) {
        let parse = |seal: &Seal| seal.open(payload).and_then(|p| serde_json::from_slice::<Msg>(&p).ok());
        if topic == self.my_mail {
            match parse(&self.me) {
                Some(Msg::Hi { from, name, back }) => self.hi(from, name, back),
                Some(Msg::Mail { from, name, text }) => {
                    // only from friends
                    if self.book.friends.iter().any(|p| seal::norm(&p.code) == seal::norm(&from) && !p.waiting) {
                        self.events.push(Event::Mail { from: name, text });
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(i) = self.live.iter().position(|l| l.here_topic == topic) {
            if let Some(Msg::Here { name, on, village, at }) = parse(&self.live[i].seal) {
                let fresh = now() - at < 180;
                let was_on = self.live[i].here.as_ref().is_some_and(|h| h.on);
                let on = on && fresh;
                self.live[i].here = Some(Here { on, village: village.filter(|_| on) });
                if let Some(p) = self.book.friends.get_mut(i) {
                    if !name.is_empty() && p.name != name {
                        p.name = name.clone();
                        self.dirty = true;
                    }
                }
                if on && !was_on && fresh {
                    let n = self.book.friends.get(i).map(|p| p.name.clone()).unwrap_or_default();
                    self.note(format!("● {n} is online"));
                }
            }
            return;
        }
        let Some(v) = &self.village else { return };
        if topic != v.topic {
            return;
        }
        let Some(m) = parse(&v.seal) else { return };
        let t = self.t;
        let me = self.id;
        let v = self.village.as_mut().unwrap();
        match m {
            Msg::Me { id, name, av } if id != me => {
                let new = !v.others.contains_key(&id);
                let o = v.others.entry(id).or_insert(Other { name: name.clone(), av, shown: (av.x, av.y), seen: t, host: false });
                (o.name, o.av, o.seen) = (name.clone(), av, t);
                if new {
                    self.note(format!("⌂ {name} is here"));
                }
            }
            Msg::Talk { id, text } if id != me => self.events.push(Event::Said { id, text }),
            Msg::Throw { id, x, y, vx, vy } if id != me => self.events.push(Event::Threw { id, x, y, vx, vy }),
            Msg::Village { host, name, seed } if host != me => {
                v.host_seen = t;
                if let Some(o) = v.others.get_mut(&host) {
                    o.host = true;
                }
                if !v.host && v.seed != Some(seed) {
                    v.seed = Some(seed);
                    self.events.push(Event::Village(seed));
                    self.note(format!("⌂ into {name}'s village"));
                }
            }
            Msg::Bye { id, host } if id != me => {
                let name = v.others.remove(&id).map(|o| o.name).unwrap_or_default();
                if host && !v.host {
                    self.leave_village();
                    self.note(format!("⌂ {name} closed their village: home again"));
                } else if !name.is_empty() {
                    self.note(format!("⌂ {name} went home"));
                }
            }
            _ => {}
        }
    }

    fn hi(&mut self, from: String, name: String, back: bool) {
        let code = seal::norm(&from);
        if code.len() != 12 || code == seal::norm(&self.book.code) {
            return;
        }
        if let Some(i) = self.book.friends.iter().position(|p| seal::norm(&p.code) == code) {
            // a yes to asking (or asked again by a friend: say yes again)
            let p = &mut self.book.friends[i];
            if p.waiting || p.name != name {
                p.waiting = false;
                p.name = name.clone();
                self.dirty = true;
                if back {
                    self.note(format!("★ {name} and you are friends now"));
                }
            }
            if !back {
                let seal = Seal::new(&code);
                let mail = seal::topic("mail", &code);
                let m = Msg::Hi { from: self.book.code.clone(), name: self.name.clone(), back: true };
                self.send(&mail, &seal, &m, false);
            }
            return;
        }
        if back {
            return;
        }
        if !self.book.asking.iter().any(|p| seal::norm(&p.code) == code) {
            self.book.asking.push(Pal { name: name.clone(), code, waiting: false });
            self.dirty = true;
            self.events.push(Event::Asked(name));
        }
    }

    /// Now and then: keeping the link alive, "here I am", and in a shared
    /// village where this villager is (`me`, None: no villager).
    pub fn tick(&mut self, t: f32, me: Option<Avatar>, seed: u32) {
        self.t = t;
        if self.link != Link::On {
            return;
        }
        if t >= self.next_ping {
            self.next_ping = t + 30.0;
            self.out.extend(mqtt::ping());
        }
        if t >= self.next_here {
            self.next_here = t + 60.0;
            let m = self.here_msg(true);
            let (topic, seal) = (self.my_here.clone(), self.me.clone());
            self.send(&topic, &seal, &m, true);
        }
        let Some(v) = &self.village else { return };
        let (topic, seal, host) = (v.topic.clone(), v.seal.clone(), v.host);
        // the host: the village, every couple of seconds
        if host && t >= self.next_village {
            self.next_village = t + 2.0;
            let m = Msg::Village { host: self.id, name: self.name.clone(), seed };
            self.send(&topic, &seal, &m, false);
        }
        // where this villager is: often while it changes, now and then if not
        if let Some(av) = me {
            let changed = self.last_me.is_none_or(|l| (l.x - av.x).abs() > 0.004 || (l.y - av.y).abs() > 0.004 || l.dir != av.dir || l.moving != av.moving || l.away != av.away);
            if t >= self.next_me && (changed || t >= self.next_me + 1.0) {
                self.next_me = t + 0.12;
                self.last_me = Some(av);
                let m = Msg::Me { id: self.id, name: self.name.clone(), av };
                self.send(&topic, &seal, &m, false);
            }
        }
        // the others: eased toward where they are; gone if not heard from
        let v = self.village.as_mut().unwrap();
        let gone: Vec<u32> = v.others.iter().filter(|(_, o)| t - o.seen > 12.0).map(|(k, _)| *k).collect();
        for k in gone {
            v.others.remove(&k);
        }
        // a guest whose host's gone quiet: home again
        if !v.host && v.seed.is_some() && t - v.host_seen > 20.0 {
            self.leave_village();
            self.note("⌂ the village's host has gone quiet: home again");
        }
    }

    /// The others eased toward where they are (each frame).
    pub fn ease(&mut self, dt: f32) {
        if let Some(v) = &mut self.village {
            let k = 1.0 - (-dt * 10.0).exp();
            for o in v.others.values_mut() {
                let far = (o.av.x - o.shown.0).abs() > 0.5 || (o.av.y - o.shown.1).abs() > 0.5;
                if far {
                    o.shown = (o.av.x, o.av.y);
                } else {
                    o.shown.0 += (o.av.x - o.shown.0) * k;
                    o.shown.1 += (o.av.y - o.shown.1) * k;
                }
            }
        }
    }

    pub fn others(&self) -> Vec<(u32, &Other)> {
        self.village.as_ref().map(|v| v.others.iter().map(|(k, o)| (*k, o)).collect()).unwrap_or_default()
    }

    pub fn other(&self, id: u32) -> Option<&Other> {
        self.village.as_ref().and_then(|v| v.others.get(&id))
    }

    /// In a shared village: (its code, hosting it).
    pub fn village(&self) -> Option<(String, bool)> {
        self.village.as_ref().map(|v| (seal::pretty(&v.code), v.host))
    }

    /// A guest in someone's village, its seed heard.
    pub fn guest_seed(&self) -> Option<u32> {
        self.village.as_ref().filter(|v| !v.host).and_then(|v| v.seed)
    }

    /// Friends, and what they're up to.
    pub fn friends(&self) -> Vec<(Pal, Option<Here>)> {
        self.book.friends.iter().zip(self.live.iter()).map(|(p, l)| (p.clone(), l.here.clone())).collect()
    }

    // ------------------------------------------------------------ doing

    pub fn go_online(&mut self, on: bool) {
        if self.book.on == on {
            return;
        }
        self.book.on = on;
        self.dirty = true;
        if !on {
            self.goodbye();
        }
    }

    /// Off the relay politely: "gone" for friends, out of the village.
    pub fn goodbye(&mut self) {
        if self.link == Link::On {
            if self.village.is_some() {
                self.leave_village();
            }
            let m = self.here_msg(false);
            let (topic, seal) = (self.my_here.clone(), self.me.clone());
            self.send(&topic, &seal, &m, true);
            self.out.extend(mqtt::disconnect());
        }
        self.link = Link::Off;
    }

    /// Ask someone (their friend code) to be friends.
    pub fn add_friend(&mut self, code: &str) -> Result<(), &'static str> {
        let code = seal::norm(code);
        if code.len() != 12 {
            return Err("a friend code is twelve letters and numbers");
        }
        if code == seal::norm(&self.book.code) {
            return Err("that's this villager's own code");
        }
        if let Some(i) = self.book.asking.iter().position(|p| p.code == code) {
            // they'd already asked: that's a yes
            self.accept(i);
            return Ok(());
        }
        if !self.book.friends.iter().any(|p| seal::norm(&p.code) == code) {
            self.book.friends.push(Pal { name: "(asked)".into(), code: code.clone(), waiting: true });
            self.dirty = true;
            self.relive();
            let topic = self.live.last().map(|l| l.here_topic.clone()).unwrap_or_default();
            self.subscribe(vec![topic]);
        }
        let m = Msg::Hi { from: self.book.code.clone(), name: self.name.clone(), back: false };
        self.send(&seal::topic("mail", &code), &Seal::new(&code), &m, false);
        if self.link != Link::On {
            self.note("✉ asked (it goes when online)");
        }
        Ok(())
    }

    /// Yes to someone asking to be friends.
    pub fn accept(&mut self, i: usize) {
        if i >= self.book.asking.len() {
            return;
        }
        let p = self.book.asking.remove(i);
        let code = p.code.clone();
        self.note(format!("★ {} and you are friends now", p.name));
        self.book.friends.push(Pal { waiting: false, ..p });
        self.dirty = true;
        self.relive();
        let topic = self.live.last().map(|l| l.here_topic.clone()).unwrap_or_default();
        self.subscribe(vec![topic]);
        let m = Msg::Hi { from: self.book.code.clone(), name: self.name.clone(), back: true };
        self.send(&seal::topic("mail", &code), &Seal::new(&code), &m, false);
    }

    pub fn decline(&mut self, i: usize) {
        if i < self.book.asking.len() {
            self.book.asking.remove(i);
            self.dirty = true;
        }
    }

    pub fn remove_friend(&mut self, i: usize) {
        if i < self.book.friends.len() {
            let topic = self.live[i].here_topic.clone();
            self.book.friends.remove(i);
            self.dirty = true;
            self.relive();
            if self.link == Link::On {
                self.sub = self.sub.wrapping_add(1).max(1);
                self.out.extend(mqtt::unsubscribe(self.sub, &[topic]));
            }
        }
    }

    /// A message to friend `i`.
    pub fn mail(&mut self, i: usize, text: &str) -> bool {
        let Some(p) = self.book.friends.get(i).filter(|p| !p.waiting) else { return false };
        if self.link != Link::On {
            return false;
        }
        let code = p.code.clone();
        let m = Msg::Mail { from: self.book.code.clone(), name: self.name.clone(), text: text.chars().take(280).collect() };
        self.send(&seal::topic("mail", &code), &Seal::new(&code), &m, false);
        true
    }

    fn enter(&mut self, code: String, host: bool) {
        if self.village.is_some() {
            self.leave_village();
        }
        let topic = seal::topic("village", &code);
        self.village = Some(Shared { seal: Seal::new(&code), topic: topic.clone(), code, host, seed: None, host_seen: self.t, others: HashMap::new() });
        self.subscribe(vec![topic]);
        self.next_here = 0.0;
        self.next_village = 0.0;
        self.last_me = None;
    }

    /// Open this village to friends (and anyone with its code): the code.
    pub fn host(&mut self) -> String {
        if let Some((code, true)) = self.village() {
            return code;
        }
        let code = seal::new_code();
        self.enter(seal::norm(&code), true);
        code
    }

    /// Into the village with this code (a friend's, or one given out).
    pub fn join(&mut self, code: &str) -> Result<(), &'static str> {
        let code = seal::norm(code);
        if code.len() != 12 {
            return Err("a village code is twelve letters and numbers");
        }
        if self.link != Link::On {
            return Err("not online yet");
        }
        self.enter(code, false);
        self.note("⌂ knocking…");
        Ok(())
    }

    /// Out of the shared village (closing it, if hosting).
    pub fn leave_village(&mut self) {
        let Some(v) = self.village.take() else { return };
        let m = Msg::Bye { id: self.id, host: v.host };
        self.send(&v.topic, &v.seal, &m, false);
        if self.link == Link::On {
            self.sub = self.sub.wrapping_add(1).max(1);
            self.out.extend(mqtt::unsubscribe(self.sub, &[v.topic.clone()]));
        }
        self.next_here = 0.0;
        if !v.host && v.seed.is_some() {
            self.events.push(Event::Home);
        }
    }

    /// Said aloud, to everyone in the village.
    pub fn say(&mut self, text: &str) {
        let Some(v) = &self.village else { return };
        let (topic, seal) = (v.topic.clone(), v.seal.clone());
        let m = Msg::Talk { id: self.id, text: text.chars().take(120).collect() };
        self.send(&topic, &seal, &m, false);
    }

    pub fn throw(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        let Some(v) = &self.village else { return };
        let (topic, seal) = (v.topic.clone(), v.seal.clone());
        let m = Msg::Throw { id: self.id, x, y, vx, vy };
        self.send(&topic, &seal, &m, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A relay for the tests: what's published goes to whoever's
    /// subscribed (and the last retained message on each topic is kept).
    #[derive(Default)]
    struct Relay {
        subs: Vec<(usize, String)>,
        kept: HashMap<String, Vec<u8>>,
    }

    impl Relay {
        fn pump(&mut self, all: &mut [&mut Online]) {
            for _ in 0..6 {
                let mut deliver: Vec<(usize, Vec<u8>)> = vec![];
                for (i, o) in all.iter_mut().enumerate() {
                    let out = o.take_out();
                    let mut k = 0;
                    while k < out.len() {
                        let head = out[k];
                        let (mut len, mut mul, mut j) = (0usize, 1usize, k + 1);
                        loop {
                            let b = out[j];
                            len += (b & 0x7f) as usize * mul;
                            mul *= 128;
                            j += 1;
                            if b & 0x80 == 0 {
                                break;
                            }
                        }
                        let body = &out[j..j + len];
                        let whole = out[k..j + len].to_vec();
                        match head >> 4 {
                            1 => deliver.push((i, vec![0x20, 2, 0, 0])),
                            8 => {
                                let mut p = 2;
                                while p < body.len() {
                                    let tl = u16::from_be_bytes([body[p], body[p + 1]]) as usize;
                                    let topic = String::from_utf8_lossy(&body[p + 2..p + 2 + tl]).to_string();
                                    if let Some(kept) = self.kept.get(&topic) {
                                        deliver.push((i, kept.clone()));
                                    }
                                    self.subs.push((i, topic));
                                    p += 3 + tl;
                                }
                            }
                            3 => {
                                let tl = u16::from_be_bytes([body[0], body[1]]) as usize;
                                let topic = String::from_utf8_lossy(&body[2..2 + tl]).to_string();
                                if head & 1 == 1 {
                                    self.kept.insert(topic.clone(), whole.clone());
                                }
                                for (s, t) in &self.subs {
                                    if *t == topic {
                                        deliver.push((*s, whole.clone()));
                                    }
                                }
                            }
                            _ => {}
                        }
                        k = j + len;
                    }
                }
                for (i, b) in deliver {
                    all[i].feed(&b);
                }
            }
        }
    }

    /// Through a real public relay (needs the internet):
    /// `cargo test --release net::tests::through_the_real_relay -- --ignored`
    #[test]
    #[ignore]
    fn through_the_real_relay() {
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Test A");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Test B");
        let (mut la, mut lb) = (None, None);
        let start = std::time::Instant::now();
        let mut t = 0.0;
        let step = |a: &mut Online, b: &mut Online, la: &mut Option<tcp::Tcp>, lb: &mut Option<tcp::Tcp>, t: &mut f32| {
            tcp::pump(a, la);
            tcp::pump(b, lb);
            a.tick(*t, None, 1);
            b.tick(*t, None, 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            *t += 0.05;
        };
        while (a.link() != Link::On || b.link() != Link::On) && start.elapsed().as_secs() < 30 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
        }
        assert_eq!((a.link(), b.link()), (Link::On, Link::On), "couldn't reach a relay");
        // (a moment for the subscriptions to settle)
        for _ in 0..20 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
        }
        a.add_friend(&b.code()).unwrap();
        let mut asked = false;
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if b.take_events().contains(&Event::Asked("Test A".into())) {
                asked = true;
                break;
            }
        }
        assert!(asked, "the friend request didn't arrive");
        b.accept(0);
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if !a.book.friends[0].waiting {
                break;
            }
        }
        assert_eq!(a.book.friends[0].name, "Test B");
        assert!(b.mail(0, "ping"));
        let mut got = false;
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if a.take_events().iter().any(|e| matches!(e, Event::Mail { text, .. } if text == "ping")) {
                got = true;
                break;
            }
        }
        assert!(got, "the message didn't arrive");
        a.go_online(false);
        b.go_online(false);
        step(&mut a, &mut b, &mut la, &mut lb, &mut t);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    /// Another player somewhere (the web page, a phone): asks the friend
    /// code in XMAS_FRIEND to be friends, waits for a yes, writes, then
    /// opens a village and waits for them to come in.
    /// `XMAS_FRIEND=XXXX-XXXX-XXXX cargo test --release net::tests::meet_someone -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn meet_someone() {
        let Ok(code) = std::env::var("XMAS_FRIEND") else { return };
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Desk Tester");
        let mut link = None;
        let start = std::time::Instant::now();
        let mut t = 0.0f32;
        let mut step = |a: &mut Online, link: &mut Option<tcp::Tcp>, seed: u32| {
            tcp::pump(a, link);
            a.tick(t, Some(Avatar { x: 0.5 + (t * 0.3).sin() * 4.0, y: 1.0, moving: true, dir: 1.0, coat: 0x2a4c8a, hat: 0xd8283a, ..Default::default() }), seed);
            a.ease(0.05);
            for e in a.take_events() {
                println!("  {e:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            t += 0.05;
        };
        while a.link() != Link::On && start.elapsed().as_secs() < 30 {
            step(&mut a, &mut link, 1);
        }
        assert_eq!(a.link(), Link::On);
        for _ in 0..20 {
            step(&mut a, &mut link, 1);
        }
        a.add_friend(&code).unwrap();
        println!("asked {code}");
        while a.book.friends[0].waiting && start.elapsed().as_secs() < 90 {
            step(&mut a, &mut link, 1);
        }
        assert!(!a.book.friends[0].waiting, "no answer");
        println!("friends with {}", a.book.friends[0].name);
        assert!(a.mail(0, "hello from the desktop"));
        let vcode = a.host();
        println!("hosting {vcode}");
        while a.others().is_empty() && start.elapsed().as_secs() < 150 {
            step(&mut a, &mut link, 777);
        }
        assert!(!a.others().is_empty(), "nobody came");
        println!("in the village: {:?}", a.others().iter().map(|(_, o)| (o.name.clone(), o.av.x, o.av.y)).collect::<Vec<_>>());
        for _ in 0..60 {
            step(&mut a, &mut link, 777);
        }
        a.say("welcome to the village!");
        for _ in 0..60 {
            step(&mut a, &mut link, 777);
        }
        a.go_online(false);
        step(&mut a, &mut link, 777);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    #[test]
    fn friends_meet_write_and_share_a_village() {
        let mut relay = Relay::default();
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Sam");
        a.connected();
        b.connected();
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!((a.link(), b.link()), (Link::On, Link::On));
        // Robin asks Sam; Sam says yes
        let code_b = b.code();
        a.add_friend(&code_b.to_lowercase()).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Asked("Robin".into())));
        assert_eq!(b.book.asking.len(), 1);
        b.accept(0);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(a.book.friends[0].name, "Sam");
        assert!(!a.book.friends[0].waiting);
        assert_eq!(b.book.friends[0].name, "Robin");
        // here I am: each sees the other online
        a.tick(1.0, None, 7);
        b.tick(1.0, None, 7);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.friends()[0].1.as_ref().is_some_and(|h| h.on));
        // a message
        assert!(b.mail(0, "snowball fight at the tree?"));
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Mail { from: "Sam".into(), text: "snowball fight at the tree?".into() }));
        // Sam opens up; Robin sees the code in Sam's "here I am" and joins
        let vcode = b.host();
        b.tick(2.0, Some(Avatar { x: 0.5, y: 0.1, ..Default::default() }), 4242);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(a.friends()[0].1.as_ref().and_then(|h| h.village.clone()).map(|c| seal::norm(&c)), Some(seal::norm(&vcode)));
        a.join(&vcode).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        b.tick(4.5, Some(Avatar { x: 0.6, y: 0.1, ..Default::default() }), 4242);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Village(4242)));
        assert_eq!(a.guest_seed(), Some(4242));
        a.tick(5.0, Some(Avatar { x: -0.3, y: 0.0, moving: true, ..Default::default() }), 1);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(b.others().len(), 1);
        assert_eq!(b.others()[0].1.name, "Robin");
        assert!((a.others()[0].1.av.x - 0.6).abs() < 1e-6);
        // said aloud
        a.say("hello!");
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().iter().any(|e| matches!(e, Event::Said { text, .. } if text == "hello!")));
        // the host closes up: the guest goes home
        b.leave_village();
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Home));
        assert!(a.village().is_none());
        // someone without the code reads nothing
        let mut c = Online::new(Book { on: true, ..Default::default() }, "Eve");
        c.connected();
        relay.pump(&mut [&mut a, &mut b, &mut c]);
        assert!(c.friends().is_empty() && c.take_events().iter().all(|e| matches!(e, Event::Note(_))));
    }
}
