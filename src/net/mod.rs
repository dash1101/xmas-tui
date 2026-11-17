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
    /// Told once where online is (the first-time hint).
    pub tipped: bool,
    /// Groups: chat rooms shared by code, like a little server.
    pub groups: Vec<Group>,
}

/// A group: a chat room anyone with its code can be in.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Group {
    pub name: String,
    pub code: String,
}

impl Card {
    /// A card from someone else, its words made safe to show.
    pub fn cleaned(self) -> Card {
        Card { name: clean(&self.name, 32), motto: clean(&self.motto, 60), pet: clean(&self.pet, 48), ..self }
    }
}

impl Avatar {
    /// Where someone else says they are, kept to sane numbers (nothing
    /// that could send a drawing off into the billions, or NaN).
    pub fn cleaned(mut self) -> Avatar {
        let ok = |v: f32, lim: f32| if v.is_finite() { v.clamp(-lim, lim) } else { 0.0 };
        self.x = ok(self.x, 1e4);
        self.y = ok(self.y, 1e4);
        self.dir = if self.dir < 0.0 { -1.0 } else { 1.0 };
        self.seat = self.seat.clamp(-1, 8);
        if let Some(tr) = &mut self.trip {
            tr.x = ok(tr.x, 1e5);
            tr.dir = if tr.dir < 0.0 { -1.0 } else { 1.0 };
            tr.lane = ok(tr.lane, 100.0);
            tr.air = ok(tr.air, 1e3);
            tr.el = ok(tr.el, 1e6);
        }
        self
    }
}

/// Text from someone else, made safe to show: no control characters (no
/// terminal escapes, no new lines), no direction overrides that would
/// make it read as something else, and not too long.
pub fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() && !matches!(*c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'))
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
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
#[serde(default)]
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
    pub hair: u32,
    pub skin: u32,
    /// 0: no scarf.
    pub scarf: u32,
    /// Away on a trip: where, and how far along.
    pub trip: Option<TripAt>,
    /// In a games room: whose (its table's number), and which chair (-1:
    /// not sat down).
    pub room: u32,
    pub seat: i8,
}

/// Where someone is on a trip (the train, the ski lodge, the slope), for
/// the others there to see them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TripAt {
    /// 1 the shuttle, 2 the carriage, 3 the lodge, 4 the chairlift, 5 the
    /// slope, 6 a drive.
    pub at: u8,
    /// Along the carriage or the lodge (0..1), or down the slope (columns).
    pub x: f32,
    pub dir: f32,
    pub lane: f32,
    /// Up off the snow (the slope); how far the train's gone (the carriage).
    pub air: f32,
    pub sat: bool,
    /// Homeward (the shuttle, the train).
    pub home: bool,
    pub score: u32,
    /// Seconds into this part of the trip.
    pub el: f32,
    /// The trip's seed (the same carriage, the same mountain).
    pub seed: u32,
}

/// A villager's profile card: who they are, for friends (and anyone in a
/// shared village) to see.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Card {
    pub name: String,
    pub motto: String,
    /// "a cat called Biscuit".
    pub pet: String,
    /// Coat, hat (0 none), hair, skin, scarf (0 none).
    pub look: [u32; 5],
    /// Games won at the table, runs down the mountain, the best score
    /// there, snowball fights won, cheer.
    pub won: u32,
    pub runs: u32,
    pub best: u32,
    pub fights: u32,
    pub cheer: u32,
    /// How many friends.
    pub friends: u32,
}

/// What can be asked of someone in a shared village (`Msg::Ask`).
pub mod ask {
    /// A game at the table: chess, checkers, Snowflakes, Twenty-One.
    pub const CHESS: u8 = 1;
    pub const CHECKERS: u8 = 2;
    pub const SNOWFLAKES: u8 = 3;
    pub const TWENTY_ONE: u8 = 4;
    /// Tag, the two of them.
    pub const TAG: u8 = 10;
    /// A race down the mountain (the seed is the course).
    pub const RACE: u8 = 11;
    /// Along on the trip up to the slopes (to wherever the asker is).
    pub const TRIP: u8 = 12;
    /// A snowball fight, the two of them.
    pub const FIGHT: u8 = 13;
    /// (A guest asking the host to change one of the village's settings:
    /// only ever an answer, from the host.)
    pub const SETTING: u8 = 20;
}

/// A code onto the clipboard (where there's a way to).
pub fn copy_text(text: &str) {
    // Windows: the system's clipboard itself (`clip` would mangle anything
    // but plain ASCII, and flash a console window up)
    #[cfg(windows)]
    crate::winsys::set_clipboard_text(text);
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let text = text.to_string();
        std::thread::spawn(move || {
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

/// The clipboard's text (a friend's code to paste), cut short.
pub fn paste_text() -> String {
    #[cfg(windows)]
    return crate::winsys::clipboard_text().map(|s| s.chars().take(400).collect()).unwrap_or_default();
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        let tries: &[(&str, &[&str])] = &[("wl-paste", &["--no-newline"]), ("xclip", &["-selection", "clipboard", "-o"]), ("xsel", &["--clipboard", "--output"])];
        for (cmd, args) in tries {
            if let Ok(o) = Command::new(cmd).args(*args).stdin(Stdio::null()).stderr(Stdio::null()).output() {
                if o.status.success() {
                    return String::from_utf8_lossy(&o.stdout).chars().take(400).collect();
                }
            }
        }
    }
    #[cfg(not(windows))]
    String::new()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "k")]
enum Msg {
    /// In a mailbox: asking to be friends (`back`: saying yes).
    Hi { from: String, name: String, back: bool },
    /// In a mailbox: a message.
    Mail { from: String, name: String, text: String },
    /// In a mailbox: come into this village (its code).
    Invite { from: String, name: String, village: String },
    /// In a mailbox: a ping (a little tone at the friend's end).
    Ping { from: String, name: String },
    /// In a group's room: said to everyone in it; and "I'm here" now and
    /// then (no text).
    Group { id: u32, name: String, text: String },
    /// One's own "here I am" (kept by the relay): online or not, hosting
    /// (the village code, for friends to join), when; the profile card.
    Here {
        name: String,
        on: bool,
        village: Option<String>,
        at: i64,
        #[serde(default)]
        card: Option<Card>,
    },
    /// On a linked device's own topic (see `sync_on`): its side of the
    /// villager (see `crate::sync`), when.
    Sync { dev: String, at: i64, state: String },
    /// In a village: someone's profile card.
    Card { id: u32, card: Card },
    /// In a village: a wave (0), a cheer (1), a laugh (2), a heart (3), at
    /// someone (0: everyone).
    Wave { id: u32, to: u32, what: u8 },
    /// In a village: asking someone (0: everyone) to play (see `ask`).
    Ask { id: u32, to: u32, what: u8, seed: u32 },
    Answer { id: u32, to: u32, what: u8, yes: bool },
    /// In a village: a game at a table (its number), as it stands after a
    /// move (see `scene::games`), and the move's number.
    Table { id: u32, game: u32, seq: u32, kind: u8, state: String },
    /// Up from the table.
    Up { id: u32, game: u32 },
    /// In a village: a light show (0, seconds), fireworks (1, how many), a
    /// carol (2) at x (village coordinates).
    Show { id: u32, what: u8, n: u16, x: f32 },
    /// Tag: who's "it" now, and how long the game has left (s).
    Tagged { id: u32, it: u32, left: f32 },
    /// A race down the mountain finished: the score and the time.
    Finish { id: u32, name: String, score: u32, secs: f32 },
    /// In a village: someone's villager.
    Me { id: u32, name: String, av: Avatar },
    /// In a village: said aloud.
    Talk { id: u32, text: String },
    /// In a village: said to one person only (`to`).
    Whisper { id: u32, to: u32, name: String, text: String },
    /// In a village: asking its host to change one of the village's
    /// settings (see `Rules` in the scene).
    Want { id: u32, key: String, value: String },
    /// In a village, from its host: who's out and about in it, and what
    /// they've said (see `Pup` in the scene), so everyone sees the same.
    Crowd { id: u32, data: String },
    /// In a village: a snowball thrown (village coordinates, per second).
    Throw { id: u32, x: f32, y: f32, vx: f32, vy: f32 },
    /// In a village, from its host: the village itself, its settings, and
    /// who may change them besides the host.
    Village {
        host: u32,
        name: String,
        seed: u32,
        #[serde(default)]
        rules: String,
        #[serde(default)]
        admins: Vec<u32>,
    },
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
    /// A friend asks this villager into their village (its code).
    Invited { name: String, code: String },
    Waved { id: u32, to: u32, what: u8 },
    /// Someone asks to play (see `ask`; `to` 0: everyone).
    Play { id: u32, to: u32, what: u8, seed: u32 },
    Answer { id: u32, what: u8, yes: bool },
    Table { id: u32, game: u32, seq: u32, kind: u8, state: String },
    Up { id: u32, game: u32 },
    Show { id: u32, what: u8, n: u16, x: f32 },
    Tagged { id: u32, it: u32, left: f32 },
    Finish { id: u32, name: String, score: u32, secs: f32 },
    /// A linked device's side of the villager (see `crate::sync`).
    Synced { dev: String, at: i64, state: String },
    /// A friend pinged this villager.
    Pinged(String),
    /// Said to this villager only, by someone in the village.
    Whispered { id: u32, name: String, text: String },
    /// Said in a group (its name), by whom.
    GroupSaid { group: String, name: String, text: String },
    /// Someone asks the host (this villager) to change a village setting;
    /// `admin`: they may (it's done without asking).
    Wanted { id: u32, name: String, key: String, value: String, admin: bool },
    /// The host's village settings (a guest), and whether this villager
    /// may change them.
    Rules { rules: String, admin: bool },
    /// Who's out in the host's village (a guest; see `Msg::Crowd`).
    Crowd(String),
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
    pub card: Option<Card>,
    /// Waving (or cheering…) until then, and how.
    pub wave: (f32, u8),
    /// How fast they're going (village units a second), from the last few
    /// places heard: where they're drawn runs on ahead between messages,
    /// so walking reads smooth, not in hops.
    pub vel: (f32, f32),
}

/// A friend's last "here I am".
#[derive(Clone, Debug, Default)]
pub struct Here {
    pub on: bool,
    pub village: Option<String>,
    pub card: Option<Card>,
}

/// Linked devices: their topics (one each, under the link code's).
struct SyncLink {
    seal: Seal,
    base: String,
    dev: String,
    /// The last of this device's own sent (sent again now and then).
    last: Option<(i64, String)>,
    next: f32,
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
    /// A guest knocking: nobody's answered by then, the village isn't open.
    knock_until: f32,
    /// The host's settings last heard (a guest), and whether this
    /// villager's an admin.
    rules: (String, bool),
    /// Who the host is (a guest: as heard).
    host_id: Option<u32>,
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
    /// This villager's profile card (in "here I am", and to the others in
    /// a shared village).
    card: Card,
    next_card: f32,
    sync: Option<SyncLink>,
    /// The friend code changed (a linked device's villager taken on): the
    /// front end closes the link and makes a new one.
    relink: bool,
    /// The groups' rooms (topic, seal), and who's been heard in each lately
    /// (id → name, when).
    rooms: Vec<(String, Seal)>,
    pub room_seen: Vec<HashMap<u32, (String, f32)>>,
    next_rooms: f32,
    /// This village's settings (hosting), for guests; and who else may
    /// change them.
    rules: String,
    admins: Vec<u32>,
    /// A village to go into once online (asked for while offline).
    pending_join: Option<String>,
    /// Heard lately, per sender: (when the window started, how many),
    /// to shrug off a flood.
    heard: HashMap<u32, (f32, u32)>,
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
            card: Card::default(),
            next_card: 0.0,
            sync: None,
            relink: false,
            rooms: vec![],
            room_seen: vec![],
            next_rooms: 0.0,
            rules: String::new(),
            admins: vec![],
            pending_join: None,
            heard: HashMap::new(),
            book,
        };
        o.relive();
        o.regroup();
        o
    }

    /// The groups' rooms, from the book.
    fn regroup(&mut self) {
        self.rooms = self.book.groups.iter().map(|g| (seal::topic("group", &g.code), Seal::new(&g.code))).collect();
        self.room_seen.resize_with(self.rooms.len(), HashMap::new);
    }

    fn relive(&mut self) {
        let old = std::mem::take(&mut self.live);
        self.live = self
            .book
            .friends
            .iter()
            .map(|p| {
                let here_topic = seal::topic("here", &p.code);
                let here = old.iter().find(|l| l.here_topic == here_topic).and_then(|l| l.here.clone());
                Live { seal: Seal::new(&p.code), here_topic, here }
            })
            .collect();
    }

    /// This villager's profile card (sent in "here I am" when it changes).
    pub fn set_card(&mut self, card: Card) {
        if self.card != card {
            self.card = card;
            self.next_here = self.next_here.min(self.t + 2.0);
            self.next_card = self.next_card.min(self.t + 2.0);
        }
    }

    /// A friend's profile card (from their "here I am").
    pub fn friend_card(&self, i: usize) -> Option<Card> {
        self.live.get(i).and_then(|l| l.here.as_ref()).and_then(|h| h.card.clone())
    }

    /// Take on another friend code and friends (a linked device's
    /// villager): the link starts again with them.
    pub fn rekey(&mut self, book: Book) {
        if self.link == Link::On {
            self.goodbye();
        }
        self.link = Link::Off;
        self.village = None;
        let on = self.book.on;
        self.book = Book { on, ..book };
        if seal::norm(&self.book.code).len() != 12 {
            self.book.code = seal::new_code();
        }
        let code = self.book.code.clone();
        self.me = Seal::new(&code);
        self.my_mail = seal::topic("mail", &code);
        self.my_here = seal::topic("here", &code);
        self.live.clear();
        self.relive();
        self.dirty = true;
        self.relink = true;
    }

    /// The friends list changed elsewhere (a linked device): taken on.
    pub fn merge_book(&mut self, book: Book) {
        let on = self.book.on;
        self.book = Book { on, ..book };
        self.relive();
        self.dirty = true;
        let topics: Vec<String> = self.live.iter().map(|l| l.here_topic.clone()).collect();
        if !topics.is_empty() {
            self.subscribe(topics);
        }
    }

    /// The link should be made again (see `rekey`).
    pub fn take_relink(&mut self) -> bool {
        std::mem::take(&mut self.relink)
    }

    /// Linked devices: this one's id, and the link code (None: not linked).
    pub fn sync_on(&mut self, link: Option<(&str, &str)>) {
        let want = link.map(|(code, dev)| (seal::topic("sync", code), dev.to_string()));
        if self.sync.as_ref().map(|s| (s.base.clone(), s.dev.clone())) == want {
            return;
        }
        if let Some(old) = self.sync.take() {
            if self.link == Link::On {
                self.sub = self.sub.wrapping_add(1).max(1);
                self.out.extend(mqtt::unsubscribe(self.sub, &[format!("{}/+", old.base)]));
            }
        }
        if let Some((code, dev)) = link {
            let base = seal::topic("sync", code);
            self.sync = Some(SyncLink { seal: Seal::new(code), base: base.clone(), dev: dev.to_string(), last: None, next: 0.0 });
            self.subscribe(vec![format!("{base}/+")]);
        }
    }

    /// This device's side of the villager, for the linked ones (kept by
    /// the relay, sent again now and then).
    pub fn sync_send(&mut self, at: i64, state: String) {
        let Some(s) = &mut self.sync else { return };
        if s.last.as_ref().is_some_and(|l| l.1 == state) {
            return;
        }
        s.last = Some((at, state));
        s.next = 0.0;
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
        let card = (!self.card.name.is_empty() || !self.card.motto.is_empty()).then(|| self.card.clone());
        Msg::Here { name: self.name.clone(), on, village, at: now(), card }
    }

    /// To everyone in the shared village.
    fn to_village(&mut self, m: &Msg) {
        let Some(v) = &self.village else { return };
        let (topic, seal) = (v.topic.clone(), v.seal.clone());
        self.send(&topic, &seal, m, false);
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
        if let Some(s) = &mut self.sync {
            topics.push(format!("{}/+", s.base));
            s.next = 0.0;
        }
        topics.extend(self.rooms.iter().map(|r| r.0.clone()));
        self.subscribe(topics);
        self.next_here = 0.0;
        self.next_rooms = self.t + 2.0;
        self.next_ping = self.t + 30.0;
        self.note("✉ online");
        if let Some(code) = self.pending_join.take() {
            self.enter(code, false);
            self.note("⌂ knocking…");
        }
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
                    if self.is_friend(&from) && self.not_flooded(0, 8) {
                        self.events.push(Event::Mail { from: clean(&name, 32), text: clean(&text, 280) });
                    }
                }
                Some(Msg::Invite { from, name, village }) => {
                    if self.is_friend(&from) && self.not_flooded(0, 8) && seal::norm(&village).len() == 12 {
                        self.events.push(Event::Invited { name: clean(&name, 32), code: seal::norm(&village) });
                    }
                }
                Some(Msg::Ping { from, name }) => {
                    if self.is_friend(&from) && self.not_flooded(0, 8) {
                        self.events.push(Event::Pinged(clean(&name, 32)));
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(i) = self.live.iter().position(|l| l.here_topic == topic) {
            if let Some(Msg::Here { name, on, village, at, card }) = parse(&self.live[i].seal) {
                let name = clean(&name, 32);
                let village = village.map(|v| seal::norm(&v)).filter(|v| v.len() == 12);
                let card = card.map(Card::cleaned);
                let fresh = now() - at < 180;
                let was_on = self.live[i].here.as_ref().is_some_and(|h| h.on);
                let on = on && fresh;
                self.live[i].here = Some(Here { on, village: village.filter(|_| on), card });
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
        if let Some(k) = self.rooms.iter().position(|r| r.0 == topic) {
            if let Some(Msg::Group { id, name, text }) = parse(&self.rooms[k].1) {
                let (name, text) = (clean(&name, 32), clean(&text, 280));
                let t = self.t;
                if let Some(seen) = self.room_seen.get_mut(k) {
                    if seen.len() < 200 || seen.contains_key(&id) {
                        seen.insert(id, (name.clone(), t));
                    }
                }
                if id != self.id && !text.is_empty() && self.not_flooded(id, 6) {
                    let group = self.book.groups.get(k).map(|g| g.name.clone()).unwrap_or_default();
                    self.events.push(Event::GroupSaid { group, name, text });
                }
            }
            return;
        }
        if let Some(sy) = &self.sync {
            if topic.strip_prefix(sy.base.as_str()).is_some_and(|r| r.starts_with('/')) {
                if let Some(Msg::Sync { dev, at, state }) = parse(&sy.seal) {
                    if dev != sy.dev {
                        self.events.push(Event::Synced { dev, at, state });
                    }
                }
                return;
            }
        }
        let Some(v) = &self.village else { return };
        if topic != v.topic {
            return;
        }
        let Some(m) = parse(&v.seal) else { return };
        let t = self.t;
        let me = self.id;
        // (one sender flooding the village is shrugged off)
        let sender = match &m {
            Msg::Me { id, .. } | Msg::Card { id, .. } | Msg::Wave { id, .. } | Msg::Ask { id, .. } | Msg::Answer { id, .. } | Msg::Table { id, .. } | Msg::Up { id, .. } | Msg::Show { id, .. } | Msg::Tagged { id, .. } | Msg::Finish { id, .. } | Msg::Talk { id, .. } | Msg::Whisper { id, .. } | Msg::Want { id, .. } | Msg::Throw { id, .. } | Msg::Bye { id, .. } => *id,
            Msg::Village { host, .. } => *host,
            Msg::Crowd { id, .. } => *id,
            _ => 0,
        };
        if sender != me && !self.not_flooded(sender, 60) {
            return;
        }
        let admins_here = self.admins.clone();
        let v = self.village.as_mut().unwrap();
        match m {
            // (room for a few dozen at most)
            Msg::Me { id, .. } if id != me && !v.others.contains_key(&id) && v.others.len() >= 40 => {}
            Msg::Me { id, name, av } if id != me => {
                let name = clean(&name, 32);
                let av = av.cleaned();
                let new = !v.others.contains_key(&id);
                let host = v.host_id == Some(id);
                let o = v.others.entry(id).or_insert(Other { name: name.clone(), av, shown: (av.x, av.y), seen: t, host, card: None, wave: (0.0, 0), vel: (0.0, 0.0) });
                // (their speed: from where they were to where they are now)
                let gap = (t - o.seen).max(0.05);
                let v = ((av.x - o.av.x) / gap, (av.y - o.av.y) / gap);
                let fast = v.0.abs() > 3.0 || v.1.abs() > 3.0;
                o.vel = if !av.moving || fast || av.away != 0 || o.av.away != av.away { (0.0, 0.0) } else { (o.vel.0 * 0.4 + v.0 * 0.6, o.vel.1 * 0.4 + v.1 * 0.6) };
                (o.name, o.av, o.seen) = (name.clone(), av, t);
                if new {
                    self.note(format!("⌂ {name} is here"));
                    // (they'll want to know who's who)
                    self.next_card = self.next_card.min(t + 0.5);
                }
            }
            Msg::Card { id, card } if id != me => {
                if let Some(o) = v.others.get_mut(&id) {
                    o.card = Some(card.cleaned());
                }
            }
            Msg::Wave { id, to, what } if id != me && (to == 0 || to == me) => {
                if let Some(o) = v.others.get_mut(&id) {
                    o.wave = (t + 2.0, what);
                }
                self.events.push(Event::Waved { id, to, what });
            }
            Msg::Ask { id, to, what, seed } if id != me && (to == 0 || to == me) => self.events.push(Event::Play { id, to, what, seed }),
            Msg::Answer { id, to, what, yes } if to == me => self.events.push(Event::Answer { id, what, yes }),
            Msg::Table { id, game, seq, kind, state } if id != me && state.len() <= 8192 => self.events.push(Event::Table { id, game, seq, kind, state }),
            Msg::Up { id, game } if id != me => self.events.push(Event::Up { id, game }),
            Msg::Show { id, what, n, x } if id != me => self.events.push(Event::Show { id, what, n, x }),
            Msg::Tagged { id, it, left } if id != me => self.events.push(Event::Tagged { id, it, left }),
            Msg::Finish { id, name, score, secs } if id != me => self.events.push(Event::Finish { id, name: clean(&name, 32), score, secs: if secs.is_finite() { secs } else { 0.0 } }),
            Msg::Talk { id, text } if id != me => self.events.push(Event::Said { id, text: clean(&text, 120) }),
            Msg::Whisper { id, to, name, text } if id != me && to == me => self.events.push(Event::Whispered { id, name: clean(&name, 32), text: clean(&text, 280) }),
            Msg::Crowd { id, data } if id != me && !v.host && v.host_id == Some(id) && data.len() <= 48 * 1024 => self.events.push(Event::Crowd(data)),
            Msg::Want { id, key, value } if id != me && v.host && v.others.contains_key(&id) && key.len() <= 24 && value.len() <= 64 => {
                let name = v.others.get(&id).map(|o| o.name.clone()).unwrap_or_default();
                self.events.push(Event::Wanted { id, name, key, value, admin: admins_here.contains(&id) });
            }
            Msg::Throw { id, x, y, vx, vy } if id != me && [x, y, vx, vy].iter().all(|v| v.is_finite() && v.abs() < 1e4) => self.events.push(Event::Threw { id, x, y, vx, vy }),
            // (the first host heard is the host: nobody else takes over)
            Msg::Village { host, .. } if host != me && v.host_id.is_some_and(|h| h != host) => {}
            Msg::Village { host, name, seed, rules, admins } if host != me => {
                v.host_seen = t;
                v.host_id = Some(host);
                if let Some(o) = v.others.get_mut(&host) {
                    o.host = true;
                }
                if !v.host {
                    let admin = admins.contains(&me);
                    let rules = if rules.len() <= 4096 { rules } else { String::new() };
                    if v.rules != (rules.clone(), admin) {
                        v.rules = (rules.clone(), admin);
                        self.events.push(Event::Rules { rules, admin });
                    }
                }
                if !v.host && v.seed != Some(seed) {
                    v.seed = Some(seed);
                    self.events.push(Event::Village(seed));
                    self.note(format!("⌂ into {}'s village", clean(&name, 32)));
                }
            }
            Msg::Bye { id, host } if id != me => {
                // (only the host closes the village)
                let host = host && v.host_id == Some(id);
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

    fn is_friend(&self, code: &str) -> bool {
        let code = seal::norm(code);
        self.book.friends.iter().any(|p| seal::norm(&p.code) == code && !p.waiting)
    }

    /// Fewer than `max` messages from this sender (0: the mailbox) in the
    /// last couple of seconds? (A flood is dropped, not shown.)
    fn not_flooded(&mut self, from: u32, max: u32) -> bool {
        let t = self.t;
        if self.heard.len() > 400 {
            self.heard.retain(|_, (at, _)| t - *at < 4.0);
        }
        let e = self.heard.entry(from).or_insert((t, 0));
        if t - e.0 > 2.0 {
            *e = (t, 0);
        }
        e.1 += 1;
        e.1 <= max
    }

    fn hi(&mut self, from: String, name: String, back: bool) {
        let name = clean(&name, 32);
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
        if !self.book.asking.iter().any(|p| seal::norm(&p.code) == code) && self.book.asking.len() < 20 {
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
        // linked devices: this one's side, when it changes and now and then
        if let Some(sy) = &mut self.sync {
            if t >= sy.next {
                sy.next = t + 300.0;
                if let Some((at, state)) = sy.last.clone() {
                    let topic = format!("{}/{}", sy.base, sy.dev);
                    let seal = sy.seal.clone();
                    let m = Msg::Sync { dev: sy.dev.clone(), at, state };
                    self.send(&topic, &seal, &m, true);
                }
            }
        }
        // in each group's room: "I'm here", now and then
        if t >= self.next_rooms && !self.rooms.is_empty() {
            self.next_rooms = t + 60.0;
            let m = Msg::Group { id: self.id, name: self.name.clone(), text: String::new() };
            for (topic, seal) in self.rooms.clone() {
                self.send(&topic, &seal, &m, false);
            }
        }
        if self.village.is_some() && t >= self.next_card {
            self.next_card = t + 30.0;
            let m = Msg::Card { id: self.id, card: self.card.clone() };
            self.to_village(&m);
        }
        let Some(v) = &self.village else { return };
        let (topic, seal, host) = (v.topic.clone(), v.seal.clone(), v.host);
        // the host: the village, every couple of seconds
        if host && t >= self.next_village {
            self.next_village = t + 2.0;
            let m = Msg::Village { host: self.id, name: self.name.clone(), seed, rules: self.rules.clone(), admins: self.admins.clone() };
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
        } else if !v.host && v.seed.is_none() && t > v.knock_until {
            // knocked, and nobody came
            self.leave_village();
            self.note("⌂ nobody answered: that village isn't open right now (or the code's not right)");
        }
    }

    /// The others eased toward where they are (each frame).
    pub fn ease(&mut self, dt: f32) {
        let t = self.t;
        if let Some(v) = &mut self.village {
            let k = 1.0 - (-dt * 12.0).exp();
            for o in v.others.values_mut() {
                // where they'll be by now, going on as they were (for a
                // moment at most: a message late isn't a walk off forever)
                let ahead = (t - o.seen).clamp(0.0, 0.3);
                let (tx, ty) = (o.av.x + o.vel.0 * ahead, o.av.y + o.vel.1 * ahead);
                let far = (tx - o.shown.0).abs() > 0.5 || (ty - o.shown.1).abs() > 0.5;
                if far {
                    o.shown = (tx, ty);
                } else {
                    o.shown.0 += (tx - o.shown.0) * k;
                    o.shown.1 += (ty - o.shown.1) * k;
                }
            }
        }
    }

    pub fn others(&self) -> Vec<(u32, &Other)> {
        let mut v: Vec<(u32, &Other)> = self.village.as_ref().map(|v| v.others.iter().map(|(k, o)| (*k, o)).collect()).unwrap_or_default();
        // (always in the same order: menus list them)
        v.sort_by_key(|(k, _)| *k);
        v
    }

    /// Previews: a shared village with these others in it (id, name,
    /// look, where).
    pub fn pretend(&mut self, others: Vec<(u32, &str, Avatar, Option<Card>)>) {
        self.book.on = true;
        self.link = Link::On;
        let code = seal::new_code();
        let mut v = Shared { seal: Seal::new(&code), topic: seal::topic("village", &code), code: seal::norm(&code), host: true, seed: None, host_seen: 0.0, others: HashMap::new(), knock_until: f32::INFINITY, rules: (String::new(), false), host_id: None };
        for (id, name, av, card) in others {
            v.others.insert(id, Other { name: name.into(), av, shown: (av.x, av.y), seen: 1e9, host: false, card, wave: (0.0, 0), vel: (0.0, 0.0) });
        }
        self.village = Some(v);
    }

    /// Previews: friends, each online or not (and with a village open).
    pub fn pretend_friends(&mut self, friends: &[(&str, bool, bool)]) {
        for (i, (name, on, village)) in friends.iter().enumerate() {
            let code = format!("PRVW{i:04}ABCD");
            self.book.friends.push(Pal { name: name.to_string(), code, waiting: false });
            self.relive();
            if let Some(l) = self.live.last_mut() {
                l.here = Some(Here { on: *on, village: village.then(|| "ABCD-EFGH-JKLM".into()), card: None });
            }
        }
    }

    /// This villager's id in the shared village.
    pub fn my_id(&self) -> u32 {
        self.id
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
            // linked devices: this one's closed (they stop waiting on it)
            if let Some(sy) = &self.sync {
                if let Some((at, st)) = &sy.last {
                    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(st) {
                        v["live"] = serde_json::Value::Bool(false);
                        v["open"] = serde_json::Value::String(String::new());
                        let (topic, seal) = (format!("{}/{}", sy.base, sy.dev), sy.seal.clone());
                        let m = Msg::Sync { dev: sy.dev.clone(), at: *at, state: v.to_string() };
                        self.send(&topic, &seal, &m, true);
                    }
                }
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
        self.village = Some(Shared { seal: Seal::new(&code), topic: topic.clone(), code, host, seed: None, host_seen: self.t, others: HashMap::new(), knock_until: self.t + 12.0, rules: (String::new(), false), host_id: None });
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
            // (in as soon as the link's up)
            if !self.book.on {
                self.go_online(true);
            }
            self.pending_join = Some(code);
            self.note("✉ going online first…");
            return Ok(());
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
        let m = Msg::Talk { id: self.id, text: text.chars().take(120).collect() };
        self.to_village(&m);
    }

    pub fn throw(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        let m = Msg::Throw { id: self.id, x, y, vx, vy };
        self.to_village(&m);
    }

    /// Ask friend `i` into this (open) village.
    pub fn invite(&mut self, i: usize) -> bool {
        let Some((code, true)) = self.village.as_ref().map(|v| (v.code.clone(), v.host)) else { return false };
        let Some(p) = self.book.friends.get(i).filter(|p| !p.waiting) else { return false };
        if self.link != Link::On {
            return false;
        }
        let to = p.code.clone();
        let m = Msg::Invite { from: self.book.code.clone(), name: self.name.clone(), village: code };
        self.send(&seal::topic("mail", &to), &Seal::new(&to), &m, false);
        true
    }

    pub fn wave(&mut self, to: u32, what: u8) {
        let m = Msg::Wave { id: self.id, to, what };
        self.to_village(&m);
    }

    /// Ask someone (0: everyone) to play (see `ask`).
    pub fn ask(&mut self, to: u32, what: u8, seed: u32) {
        let m = Msg::Ask { id: self.id, to, what, seed };
        self.to_village(&m);
    }

    pub fn answer(&mut self, to: u32, what: u8, yes: bool) {
        let m = Msg::Answer { id: self.id, to, what, yes };
        self.to_village(&m);
    }

    /// The game at table `game` as it stands (after this villager's move),
    /// for everyone at it.
    pub fn table(&mut self, game: u32, seq: u32, kind: u8, state: String) {
        let m = Msg::Table { id: self.id, game, seq, kind, state };
        self.to_village(&m);
    }

    pub fn table_up(&mut self, game: u32) {
        let m = Msg::Up { id: self.id, game };
        self.to_village(&m);
    }

    pub fn show(&mut self, what: u8, n: u16, x: f32) {
        let m = Msg::Show { id: self.id, what, n, x };
        self.to_village(&m);
    }

    pub fn tagged(&mut self, it: u32, left: f32) {
        let m = Msg::Tagged { id: self.id, it, left };
        self.to_village(&m);
    }

    pub fn finish(&mut self, score: u32, secs: f32) {
        let m = Msg::Finish { id: self.id, name: self.name.clone(), score, secs };
        self.to_village(&m);
    }

    /// A ping to friend `i` (a little tone at their end).
    pub fn ping(&mut self, i: usize) -> bool {
        let Some(p) = self.book.friends.get(i).filter(|p| !p.waiting) else { return false };
        if self.link != Link::On {
            return false;
        }
        let code = p.code.clone();
        let m = Msg::Ping { from: self.book.code.clone(), name: self.name.clone() };
        self.send(&seal::topic("mail", &code), &Seal::new(&code), &m, false);
        true
    }

    /// Said to one person here only.
    pub fn whisper(&mut self, to: u32, text: &str) {
        let m = Msg::Whisper { id: self.id, to, name: self.name.clone(), text: text.chars().take(280).collect() };
        self.to_village(&m);
    }

    /// A new group (its room's code), with this villager in it.
    pub fn new_group(&mut self, name: &str) -> String {
        let code = seal::new_code();
        self.add_group(name, &code);
        code
    }

    /// Into the group with this code (named as it's known here).
    pub fn join_group(&mut self, name: &str, code: &str) -> Result<(), &'static str> {
        if seal::norm(code).len() != 12 {
            return Err("a group code is twelve letters and numbers");
        }
        if self.book.groups.iter().any(|g| seal::norm(&g.code) == seal::norm(code)) {
            return Err("already in that group");
        }
        if self.book.groups.len() >= 12 {
            return Err("that's a lot of groups: leave one first");
        }
        self.add_group(name, code);
        Ok(())
    }

    fn add_group(&mut self, name: &str, code: &str) {
        let name = clean(name, 24);
        let name = if name.is_empty() { format!("group {}", self.book.groups.len() + 1) } else { name };
        self.book.groups.push(Group { name, code: seal::norm(code) });
        self.dirty = true;
        self.regroup();
        if let Some(r) = self.rooms.last().cloned() {
            self.subscribe(vec![r.0.clone()]);
            let m = Msg::Group { id: self.id, name: self.name.clone(), text: String::new() };
            self.send(&r.0, &r.1, &m, false);
        }
    }

    pub fn leave_group(&mut self, i: usize) {
        if i < self.book.groups.len() {
            let topic = self.rooms[i].0.clone();
            self.book.groups.remove(i);
            self.room_seen.remove(i);
            self.dirty = true;
            self.regroup();
            if self.link == Link::On {
                self.sub = self.sub.wrapping_add(1).max(1);
                self.out.extend(mqtt::unsubscribe(self.sub, &[topic]));
            }
        }
    }

    /// Said in group `i`'s room.
    pub fn group_say(&mut self, i: usize, text: &str) -> bool {
        let Some((topic, seal)) = self.rooms.get(i).cloned() else { return false };
        if self.link != Link::On {
            return false;
        }
        let m = Msg::Group { id: self.id, name: self.name.clone(), text: text.chars().take(280).collect() };
        self.send(&topic, &seal, &m, false);
        true
    }

    /// Who's been heard in group `i` lately (names), this villager aside.
    pub fn group_here(&self, i: usize) -> Vec<String> {
        let t = self.t;
        self.room_seen.get(i).map(|s| s.iter().filter(|(id, (_, at))| **id != self.id && t - at < 150.0).map(|(_, (n, _))| n.clone()).collect()).unwrap_or_default()
    }

    /// Hosting: who's out in the village (and what they said), for the
    /// guests.
    pub fn crowd(&mut self, data: String) {
        if self.village.as_ref().is_some_and(|v| v.host && !v.others.is_empty()) {
            let m = Msg::Crowd { id: self.id, data };
            self.to_village(&m);
        }
    }

    /// Hosting: the village's settings (for the guests), sent with the
    /// village from now on.
    pub fn set_rules(&mut self, rules: String) {
        if rules != self.rules {
            self.rules = rules;
            self.next_village = self.next_village.min(self.t + 0.3);
        }
    }

    /// Hosting: let someone here change the village's settings (or not).
    pub fn set_admin(&mut self, id: u32, on: bool) {
        self.admins.retain(|&a| a != id);
        if on {
            self.admins.push(id);
        }
        self.next_village = self.next_village.min(self.t + 0.1);
    }

    pub fn is_admin(&self, id: u32) -> bool {
        self.admins.contains(&id)
    }

    /// A guest: asking the host to change a village setting.
    pub fn want(&mut self, key: &str, value: &str) {
        let m = Msg::Want { id: self.id, key: key.to_string(), value: value.to_string() };
        self.to_village(&m);
    }

}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A subscription (`+`: any one level) and a topic.
    fn matches(sub: &str, topic: &str) -> bool {
        let (a, b): (Vec<&str>, Vec<&str>) = (sub.split('/').collect(), topic.split('/').collect());
        a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| *x == "+" || x == y)
    }

    /// A relay for the tests: what's published goes to whoever's
    /// subscribed (and the last retained message on each topic is kept).
    #[derive(Default)]
    pub(crate) struct Relay {
        subs: Vec<(usize, String)>,
        kept: HashMap<String, Vec<u8>>,
    }

    impl Relay {
        pub(crate) fn pump(&mut self, all: &mut [&mut Online]) {
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
                                    for (k, kept) in &self.kept {
                                        if matches(&topic, k) {
                                            deliver.push((i, kept.clone()));
                                        }
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
                                    if matches(t, &topic) {
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
        // B opens up and asks A in; A comes; B asks A to chess, A says yes
        let code = b.host();
        assert!(b.invite(0));
        let mut inv = None;
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            inv = a.take_events().into_iter().find_map(|e| if let Event::Invited { code, .. } = e { Some(code) } else { None });
            if inv.is_some() {
                break;
            }
        }
        assert_eq!(inv.map(|c| seal::norm(&c)), Some(seal::norm(&code)), "the invite didn't arrive");
        a.join(&code).unwrap();
        let av = Some(Avatar { x: 1.0, ..Default::default() });
        for _ in 0..200 {
            tcp::pump(&mut a, &mut la);
            tcp::pump(&mut b, &mut lb);
            a.tick(t, av, 1);
            b.tick(t, av, 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            t += 0.05;
            if b.other(a.my_id()).is_some() && a.other(b.my_id()).is_some() {
                break;
            }
        }
        assert!(b.other(a.my_id()).is_some(), "A never turned up in B's village");
        b.ask(a.my_id(), ask::CHESS, 9);
        let mut asked = false;
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if a.take_events().iter().any(|e| matches!(e, Event::Play { what: ask::CHESS, .. })) {
                asked = true;
                break;
            }
        }
        assert!(asked, "the question didn't arrive");
        a.answer(b.my_id(), ask::CHESS, true);
        let mut yes = false;
        for _ in 0..200 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if b.take_events().iter().any(|e| matches!(e, Event::Answer { yes: true, .. })) {
                yes = true;
                break;
            }
        }
        assert!(yes, "the answer didn't arrive");
        // linked devices: two of A's (A, and a third link), each hearing the other
        let mut a2 = Online::new(Book { on: true, ..Default::default() }, "Test A");
        let mut l2 = None;
        let link = seal::new_code();
        a.sync_on(Some((&link, "dev-one")));
        a2.sync_on(Some((&link, "dev-two")));
        a.sync_send(5, "one's side".into());
        let mut heard = false;
        for _ in 0..300 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            tcp::pump(&mut a2, &mut l2);
            a2.tick(t, None, 1);
            if a2.take_events().iter().any(|e| matches!(e, Event::Synced { dev, .. } if dev == "dev-one")) {
                heard = true;
                break;
            }
        }
        assert!(heard, "the linked device didn't hear");
        a.go_online(false);
        b.go_online(false);
        a2.go_online(false);
        step(&mut a, &mut b, &mut la, &mut lb, &mut t);
        tcp::pump(&mut a2, &mut l2);
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
        let mut step = |a: &mut Online, link: &mut Option<tcp::Tcp>, seed: u32| -> Vec<Event> {
            tcp::pump(a, link);
            a.tick(t, Some(Avatar { x: 0.5 + (t * 0.3).sin() * 4.0, y: 1.0, moving: true, dir: 1.0, coat: 0x2a4c8a, hat: 0xd8283a, ..Default::default() }), seed);
            a.ease(0.05);
            let ev = a.take_events();
            for e in &ev {
                println!("  {e:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            t += 0.05;
            ev
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
        a.set_card(Card { name: "Desk Tester".into(), motto: "testing, testing".into(), won: 1, ..Default::default() });
        for _ in 0..60 {
            step(&mut a, &mut link, 777);
        }
        // a game of chess with them: asked, and (a yes) the first move
        let (them, name) = a.others().first().map(|(id, o)| (*id, o.name.clone())).unwrap();
        a.ask(them, ask::CHESS, 3);
        println!("asked {name} to chess");
        let mut yes = false;
        while !yes && start.elapsed().as_secs() < 240 {
            yes = step(&mut a, &mut link, 777).iter().any(|e| matches!(e, Event::Answer { yes: true, .. }));
        }
        assert!(yes, "no yes to chess");
        let me = a.my_id();
        let mut tb = crate::scene::games::Table::online(crate::scene::games::Kind::Chess, 3, vec![me, them], vec!["Desk Tester".into(), name.clone()], me, 3);
        tb.test_move(4, 1, 4, 3);
        let (seq, st) = tb.take_out().expect("a move");
        a.table(3, seq, 1, st);
        println!("e2-e4 played; waiting for theirs");
        let mut theirs = false;
        while !theirs && start.elapsed().as_secs() < 300 {
            for e in step(&mut a, &mut link, 777) {
                if let Event::Table { seq, state, .. } = &e {
                    theirs = tb.adopt(*seq, state);
                    println!("their move came: {}", tb.status);
                }
            }
        }
        assert!(theirs, "no move back");
        for _ in 0..40 {
            step(&mut a, &mut link, 777);
        }
        a.go_online(false);
        step(&mut a, &mut link, 777);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    /// Pings, messages (made harmless), a group, whispers, the host's
    /// village settings asked about and an admin's, a guest who can't close
    /// the village, and a knock nobody answers.
    #[test]
    fn talking_groups_pings_and_the_hosts_settings() {
        let mut relay = Relay::default();
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Sam");
        a.connected();
        b.connected();
        relay.pump(&mut [&mut a, &mut b]);
        a.add_friend(&b.code()).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        b.accept(0);
        relay.pump(&mut [&mut a, &mut b]);
        a.take_events();
        b.take_events();
        // a ping, and a message with something nasty in it (a terminal
        // escape, new lines): it arrives harmless
        assert!(a.ping(0));
        assert!(a.mail(0, "hi\x1b[2J\nthere\u{202e}"));
        relay.pump(&mut [&mut a, &mut b]);
        let ev = b.take_events();
        assert!(ev.contains(&Event::Pinged("Robin".into())), "{ev:?}");
        assert!(ev.contains(&Event::Mail { from: "Robin".into(), text: "hi[2Jthere".into() }), "{ev:?}");
        // a group: made, joined by its code, talked in
        let code = a.new_group("carols");
        b.join_group("", &code).unwrap();
        assert!(b.join_group("", &code).is_err());
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.group_say(0, "anyone about?"));
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::GroupSaid { group: "carols".into(), name: "Sam".into(), text: "anyone about?".into() }));
        assert_eq!(a.group_here(0), vec!["Sam".to_string()]);
        // the village: Robin hosts; Sam knocks and is let in
        let vcode = a.host();
        a.set_rules("{\"snow\":2.0}".into());
        b.join(&vcode).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        a.tick(3.0, Some(Avatar::default()), 5);
        b.tick(3.0, Some(Avatar::default()), 5);
        relay.pump(&mut [&mut a, &mut b]);
        let (ia, ib) = (a.my_id(), b.my_id());
        let ev = b.take_events();
        assert!(ev.contains(&Event::Rules { rules: "{\"snow\":2.0}".into(), admin: false }), "{ev:?}");
        // whispered: only Sam hears
        a.whisper(ib, "psst");
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Whispered { id: ia, name: "Robin".into(), text: "psst".into() }));
        // Sam asks for more snow; then may change things himself
        b.tick(3.5, Some(Avatar::default()), 5);
        relay.pump(&mut [&mut a, &mut b]);
        b.want("snow", "3.0");
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Wanted { id: ib, name: "Sam".into(), key: "snow".into(), value: "3.0".into(), admin: false }));
        a.set_admin(ib, true);
        a.tick(6.0, Some(Avatar::default()), 5);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Rules { rules: "{\"snow\":2.0}".into(), admin: true }));
        b.want("snow", "3.0");
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().iter().any(|e| matches!(e, Event::Wanted { admin: true, .. })));
        // who's out in the village: from the host only
        a.crowd("{\"folk\":[]}".into());
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Crowd("{\"folk\":[]}".into())));
        // only the host closes the village: a guest's "closing" is just a goodbye
        let mut c = Online::new(Book { on: true, ..Default::default() }, "Mallory");
        c.connected();
        relay.pump(&mut [&mut a, &mut b, &mut c]);
        c.join(&vcode).unwrap();
        c.tick(7.0, Some(Avatar::default()), 5);
        relay.pump(&mut [&mut a, &mut b, &mut c]);
        let fake = Msg::Bye { id: c.my_id(), host: true };
        c.to_village(&fake);
        let fake = Msg::Crowd { id: c.my_id(), data: "{}".into() };
        c.to_village(&fake);
        relay.pump(&mut [&mut a, &mut b, &mut c]);
        assert!(b.guest_seed().is_some(), "a guest closed someone else's village");
        assert!(!b.take_events().iter().any(|e| matches!(e, Event::Crowd(_))), "a guest's crowd taken for the host's");
        // knocking at a village nobody has open: told so, home again
        let mut d = Online::new(Book { on: true, ..Default::default() }, "Kim");
        d.connected();
        relay.pump(&mut [&mut d]);
        d.join("ZZZZ-ZZZZ-ZZZZ").unwrap();
        d.take_events();
        d.tick(20.0, Some(Avatar::default()), 5);
        assert!(d.village().is_none());
        assert!(d.take_events().iter().any(|e| matches!(e, Event::Note(n) if n.contains("nobody answered"))));
    }

    /// In a shared village: asked in by a friend, asked to play (and
    /// answered), a move at the table, a light show, tag, a card; and two
    /// devices of one villager hearing each other (and no one else).
    #[test]
    fn playing_together_and_linked_devices() {
        let mut relay = Relay::default();
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Sam");
        a.connected();
        b.connected();
        relay.pump(&mut [&mut a, &mut b]);
        a.add_friend(&b.code()).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        b.accept(0);
        relay.pump(&mut [&mut a, &mut b]);
        a.take_events();
        b.take_events();
        // Robin opens up and asks Sam in
        let code = a.host();
        a.set_card(Card { name: "Robin".into(), motto: "snow!".into(), won: 3, ..Default::default() });
        assert!(a.invite(0));
        relay.pump(&mut [&mut a, &mut b]);
        let inv = b.take_events().into_iter().find_map(|e| match e {
            Event::Invited { name, code } => Some((name, code)),
            _ => None,
        });
        assert_eq!(inv.as_ref().map(|i| (i.0.as_str(), seal::norm(&i.1))), Some(("Robin", seal::norm(&code))));
        b.join(&inv.unwrap().1).unwrap();
        relay.pump(&mut [&mut a, &mut b]);
        a.tick(3.0, Some(Avatar { x: 1.0, ..Default::default() }), 5);
        b.tick(3.0, Some(Avatar { x: 2.0, ..Default::default() }), 5);
        relay.pump(&mut [&mut a, &mut b]);
        let (ia, ib) = (a.my_id(), b.my_id());
        assert!(b.other(ia).is_some() && a.other(ib).is_some());
        // (cards go round when someone new turns up)
        a.tick(4.0, Some(Avatar { x: 1.0, ..Default::default() }), 5);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(b.other(ia).and_then(|o| o.card.clone()).map(|c| c.motto), Some("snow!".into()));
        // a game of chess: asked, said yes to, a move
        a.ask(ib, ask::CHESS, 77);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Play { id: ia, to: ib, what: ask::CHESS, seed: 77 }));
        b.answer(ia, ask::CHESS, true);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Answer { id: ib, what: ask::CHESS, yes: true }));
        a.table(77, 1, 1, "{}".into());
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.take_events().contains(&Event::Table { id: ia, game: 77, seq: 1, kind: 1, state: "{}".into() }));
        // asked of someone else: not heard
        a.ask(12345, ask::TAG, 1);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(!b.take_events().iter().any(|e| matches!(e, Event::Play { .. })));
        // a show, and tag
        a.show(1, 5, 0.5);
        a.tagged(ib, 60.0);
        relay.pump(&mut [&mut a, &mut b]);
        let ev = b.take_events();
        assert!(ev.contains(&Event::Show { id: ia, what: 1, n: 5, x: 0.5 }));
        assert!(ev.contains(&Event::Tagged { id: ia, it: ib, left: 60.0 }));
        // Robin's other device, linked: they hear each other's sides;
        // Sam (not linked) hears nothing of it
        let mut a2 = Online::new(Book { on: true, ..Default::default() }, "Robin");
        a2.connected();
        relay.pump(&mut [&mut a, &mut b, &mut a2]);
        a.sync_on(Some(("LINK-CODE-2345", "desk")));
        a2.sync_on(Some(("LINK-CODE-2345", "phone")));
        a.sync_send(100, "from the desk".into());
        a.tick(5.0, None, 5);
        relay.pump(&mut [&mut a, &mut b, &mut a2]);
        assert!(a2.take_events().contains(&Event::Synced { dev: "desk".into(), at: 100, state: "from the desk".into() }));
        assert!(!a.take_events().iter().any(|e| matches!(e, Event::Synced { .. })), "heard itself");
        assert!(!b.take_events().iter().any(|e| matches!(e, Event::Synced { .. })));
        // a device linked later hears what's kept
        let mut a3 = Online::new(Book { on: true, ..Default::default() }, "Robin");
        a3.connected();
        a3.sync_on(Some(("LINK-CODE-2345", "laptop")));
        relay.pump(&mut [&mut a, &mut b, &mut a2, &mut a3]);
        assert!(a3.take_events().contains(&Event::Synced { dev: "desk".into(), at: 100, state: "from the desk".into() }));
        // taking on another friend code: a new link wanted
        let mut book = a2.book.clone();
        book.code = a.book.code.clone();
        a2.rekey(book);
        assert!(a2.take_relink());
        assert_eq!(a2.code(), a.code());
    }

    /// Another device of one's own (a phone, the web page): links to it by
    /// the link code in XMAS_LINK, becomes its villager, and adds its own
    /// five cheer to theirs.
    /// `XMAS_LINK=XXXX-XXXX-XXXX cargo test --release net::tests::link_to_someone -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn link_to_someone() {
        let Ok(code) = std::env::var("XMAS_LINK") else { return };
        let mut l = crate::sync::Linked::new(&code, true);
        l.label = "desk test".into();
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Desk Device");
        a.sync_on(Some((&l.code.clone(), &l.dev.clone())));
        let mut link = None;
        let start = std::time::Instant::now();
        let (mut t, mut you, mut seed) = (0.0f32, crate::save::You::default(), 1u32);
        let mut pockets = crate::scene::shop::Pockets { cheer: 5, ..Default::default() };
        let mut book = a.book.clone();
        let mut took = false;
        while start.elapsed().as_secs() < 120 {
            tcp::pump(&mut a, &mut link);
            a.tick(t, None, seed);
            if t as i32 % 5 == 0 && (t - 0.05) as i32 % 5 != 0 {
                let st = l.state(&you, seed, &pockets, &book, crate::clock::now().timestamp());
                a.sync_send(crate::clock::now().timestamp(), st);
            }
            for e in a.take_events() {
                if let Event::Synced { dev, at, state } = e {
                    let r = l.take(&dev, at, &state, crate::clock::now().timestamp(), &you, seed, &pockets, &book);
                    println!("from {}: name {:?} seed {:?} friend code {:?} cheer {:?}", r.from, r.you.as_ref().map(|y| y.name.clone()), r.seed, r.book.as_ref().map(|b| b.code.clone()), r.pockets.as_ref().map(|p| p.cheer));
                    if let Some(y) = r.you {
                        you = y;
                    }
                    if let Some(s) = r.seed {
                        seed = s;
                    }
                    if let Some(b) = r.book {
                        book = b;
                    }
                    if let Some(p) = r.pockets {
                        pockets = p;
                    }
                    took = true;
                } else {
                    println!("  {e:?}");
                }
            }
            if took && start.elapsed().as_secs() > 40 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            t += 0.05;
        }
        assert!(took, "nothing from the other device");
        println!("now: cheer {} (5 of it from here), friend code {}", pockets.cheer, book.code);
        a.go_online(false);
        tcp::pump(&mut a, &mut link);
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
