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

pub mod filter;
pub mod mqtt;
pub mod seal;
#[cfg(not(target_arch = "wasm32"))]
pub mod tcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod ws;
#[cfg(not(target_arch = "wasm32"))]
pub mod lp;

use seal::Seal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The relays, tried in turn: (host, TCP port, WebSocket address). The
/// village's own first: a WebSocket on the web's own port (443), which
/// networks that block the others' ports let through (TCP port 0: only
/// its WebSocket, or its https:// long polling). Then free public brokers
/// (the apps over plain TCP). (The web page reads relays.json beside it
/// first; XMAS_RELAY, a ws(s):// or http(s):// address, puts another one
/// first.)
pub const RELAYS: &[(&str, u16, &str)] = &[
    ("xmas-relay.novalabs.app", 0, "wss://xmas-relay.novalabs.app/mqtt"),
    // (the same relay over plain HTTPS requests, where a network's proxy
    // or filter breaks WebSockets)
    ("xmas-relay.novalabs.app", 0, "https://xmas-relay.novalabs.app/poll"),
    ("broker.emqx.io", 1883, "wss://broker.emqx.io:8084/mqtt"),
    ("broker.hivemq.com", 1883, "wss://broker.hivemq.com:8884/mqtt"),
    ("test.mosquitto.org", 1883, "wss://test.mosquitto.org:8081/mqtt"),
];

/// The public squares: villages anyone can walk into, always there (as a
/// few copies of each, should one fill up). Each is the same village
/// whoever's hosting it: the first in hosts, and when they go, the next
/// takes over.
pub const SQUARES: &[&str] = &["North Pole Square", "Candy Cane Lane", "Frosty Hollow", "Pine Peak Lodge"];
/// The most in one copy of a square.
pub const SQUARE_LIMIT: usize = 16;
/// Copies of a square, tried in turn when the first are full.
pub const SQUARE_COPIES: u32 = 6;
/// The most any village takes; one's own, unless it's set otherwise.
pub const MAX_LIMIT: usize = 48;
pub const DEFAULT_LIMIT: usize = 8;

/// Square `i`'s code (copy `n`, from 1): the same for everyone.
pub fn square_code(i: usize, n: u32) -> String {
    seal::norm(&seal::code_of(&[b"xmas-tui square", SQUARES[i % SQUARES.len()].as_bytes(), &n.to_le_bytes()]))
}

/// Square `i`'s village (its seed): the same whoever hosts.
pub fn square_seed(i: usize) -> u32 {
    seal::number_of(&[b"xmas-tui square seed", SQUARES[i % SQUARES.len()].as_bytes()]).max(1)
}

/// Where public villages say how they are (kept by the relay): a square's
/// copy, and one a villager's listed.
fn status_topic(code: &str) -> String {
    seal::topic("public", code)
}

fn listed_topic(id: u32) -> String {
    format!("xmas-tui/1/listed/{id:08x}")
}

/// The seal everyone opens a public village's status with (it's public).
fn public_seal() -> Seal {
    Seal::new("xmas-tui public")
}

/// A public village, as its host last said: (name, code, how many in it,
/// the most it takes, a square).
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub name: String,
    pub code: String,
    pub n: usize,
    pub limit: usize,
    pub square: bool,
    /// Kept open by the relay's own host (not a player's game).
    pub server: bool,
    at: i64,
}

/// What the village's own relay says about the public villages (from its
/// console): featured first, then in this order (names or codes), the
/// codes kept out, and words starred out in public villages.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PublicRules {
    pub featured: Vec<String>,
    pub order: Vec<String>,
    pub banned: Vec<String>,
    /// Banned villages by their topic (what's after `village/`): a private
    /// village's code is never sent out, only this.
    #[serde(default)]
    pub banned_ids: Vec<String>,
    pub words: Vec<String>,
    /// The relay's own version, and the oldest game it lets in.
    #[serde(default)]
    pub relay: String,
    #[serde(default)]
    pub min_app: String,
}

impl PublicRules {
    /// Is the village with this code kept out by the relay?
    pub fn is_banned(&self, code: &str) -> bool {
        let code = seal::norm(code);
        self.banned.contains(&code) || {
            let t = seal::topic("village", &code);
            self.banned_ids.iter().any(|b| t.rsplit('/').next() == Some(b.as_str()))
        }
    }
}

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
    /// This villager's own village, when open, listed for anyone to find
    /// (in Public villages).
    pub listed: bool,
    /// What's been written to and from each friend (by their code), the
    /// latest last: a chat's history.
    pub chats: std::collections::BTreeMap<String, Vec<ChatLine>>,
    /// Read up to when, each friend's chat (by their code), and how many
    /// lines that second had then (times are to the second): what's after
    /// is new.
    #[serde(default)]
    pub read: std::collections::BTreeMap<String, (i64, u32)>,
    /// What's for friends' mailboxes and not known to have reached them yet
    /// (asking to be friends, a yes, messages, invites): it goes when this
    /// villager's online, and again whenever they're next online, until
    /// their side says it has it (see `Msg::Got`).
    pub outbox: Vec<Outgoing>,
    /// The number the next one goes out with.
    pub next_n: u32,
    /// The numbers lately heard from each (their code), so one that comes
    /// again (their side never heard it arrive) isn't shown twice.
    pub got: std::collections::BTreeMap<String, Vec<u32>>,
}

/// Something on its way to a friend's mailbox (see `Book::outbox`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Outgoing {
    /// Their code.
    pub to: String,
    pub n: u32,
    /// What: see `out`.
    pub kind: u8,
    /// A message's words; an invite's village code.
    pub text: String,
    /// When it was written, and last sent (seconds since 1970; 0: not
    /// yet), and how many times it's gone.
    pub at: i64,
    pub sent: i64,
    pub tries: u8,
}

/// What's in the outbox (see `Outgoing::kind`).
pub mod out {
    /// Asking to be friends.
    pub const HI: u8 = 0;
    /// Yes to being friends.
    pub const YES: u8 = 1;
    pub const MAIL: u8 = 2;
    pub const INVITE: u8 = 3;
}

/// A chat line's way there (see `ChatLine::state`).
pub mod sent {
    /// There (or theirs).
    pub const THERE: u8 = 0;
    /// Waiting to go (this villager, or they, offline).
    pub const WAITING: u8 = 1;
    /// Never got there (given up on).
    pub const LOST: u8 = 2;
}

/// One line of a chat with a friend.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ChatLine {
    /// When (seconds since 1970).
    pub at: i64,
    /// Written by this villager (else by the friend).
    pub mine: bool,
    pub text: String,
    /// This villager's: its number in the outbox, and whether it's there
    /// yet (see `sent`).
    pub n: u32,
    pub state: u8,
    /// An invite: the village's code (a "join" by it).
    pub invite: String,
}

/// The most lines kept of a chat with one friend.
pub const CHAT_KEPT: usize = 60;

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
    /// (As they call themselves, or as renamed here.)
    pub name: String,
    pub code: String,
    /// Asked; no answer yet.
    pub waiting: bool,
    /// Quiet: what they write is kept in the chat (and counted as new),
    /// with no chime and no line in the post; their pings don't sound.
    pub quiet: bool,
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
    /// Paused (the pause button, or the wallpaper resting while a game
    /// runs): still here, standing still; the others show them greyed.
    pub rest: bool,
    /// Their pets following them (up to four): the kind (1 up; 0 none) in
    /// the high byte, which of its coats in the low one.
    pub pets: [u16; 4],
    /// Up in a hot-air balloon: its seed and 1 (0: not).
    pub ride: u32,
    /// When this was sent: seconds on the sender's own clock (0: an older
    /// version, which doesn't say). The others draw them a moment behind,
    /// between the places heard, so walking reads smooth however the
    /// messages arrive.
    pub at: f32,
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
    /// In a mailbox: asking to be friends (`back`: saying yes). `n`: its
    /// number from the sender's outbox (0: an older version), for a `Got`
    /// back.
    Hi {
        from: String,
        name: String,
        back: bool,
        #[serde(default)]
        n: u32,
    },
    /// In a mailbox: a message.
    Mail {
        from: String,
        name: String,
        text: String,
        #[serde(default)]
        n: u32,
    },
    /// In a mailbox: come into this village (its code).
    Invite {
        from: String,
        name: String,
        village: String,
        #[serde(default)]
        n: u32,
    },
    /// In a mailbox: what the sender sent with number `n` got here.
    Got { from: String, n: u32 },
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
        /// What they're playing on ("linux", "windows", "android", "web").
        #[serde(default)]
        os: String,
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
    /// (`seed`: what was asked's, so a late answer to an old ask isn't
    /// taken for a newer one's; 0 from an older version.)
    Answer {
        id: u32,
        to: u32,
        what: u8,
        yes: bool,
        #[serde(default)]
        seed: u32,
    },
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
    /// A shooting star sent off (a tap on the sky) at x (village
    /// coordinates), `y` pixels up.
    Star { id: u32, x: f32, y: f32 },
    /// A race down the mountain (its seed), from whoever asked: everyone's
    /// at the top, and it starts at `at` (milliseconds, the wall clock).
    Go { id: u32, seed: u32, at: i64 },
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
        /// The most it takes (0: as many as there's room for).
        #[serde(default)]
        limit: u16,
        /// The host's clock: minutes into the day, and sunrise and sunset
        /// (minutes), so everyone in the village has the same sky.
        #[serde(default)]
        clock: Option<[f32; 3]>,
        /// The host's music, for guests to play the same (as far as they
        /// want it).
        #[serde(default)]
        music: Option<Tune>,
        /// Who the host's keeping from talking there.
        #[serde(default)]
        muted: Vec<u32>,
    },
    /// In a village: gone (`host`: the village closes; a square's next
    /// villager takes over).
    Bye { id: u32, host: bool },
    /// The host, to someone come in when the village is full.
    Full { id: u32, to: u32 },
    /// A public village's state (kept by the relay, on a topic of its own):
    /// its name, its code, how many are in it, the most it takes, when.
    Status {
        name: String,
        code: String,
        n: u16,
        limit: u16,
        at: i64,
        square: bool,
        /// Kept open by the relay's own host.
        #[serde(default)]
        server: bool,
    },
    /// From a village's host: sent home (`to`).
    Kick { id: u32, to: u32 },
    /// From a village's host: an announcement (to everyone there, or `to`).
    Notice { id: u32, to: u32, text: String },
    /// In a village: something in the sky (or come down from it) for
    /// everyone there to see the same (see `sky`): from the host when it's
    /// due (the sleigh, a balloon, a comet…), from whoever set it off
    /// (Santa knocked out of his sleigh, and where he runs).
    Sky { id: u32, what: u8, x: f32, y: f32, seed: u32 },
    /// In a village: never mind what was asked (`what`, its seed): the
    /// asker's said yes to something else.
    Never { id: u32, what: u8, seed: u32 },
}

/// What's in the sky (see `Msg::Sky`).
pub mod sky {
    /// The sleigh crosses (`seed`: its speed and height).
    pub const SLEIGH: u8 = 1;
    /// The sleigh pulls up a moment (`y`: seconds).
    pub const HALT: u8 = 2;
    /// A hot-air balloon comes down to rest at `x` (`y`: from the left 0,
    /// the right 1).
    pub const BALLOON: u8 = 3;
    /// Santa tumbles out of his sleigh at `x` (`y`: rows from the top).
    pub const SANTA: u8 = 4;
    /// Where he's running (`x`; `y`: his way, 1 or -1; `seed` 1: down on
    /// the ice).
    pub const SANTA_AT: u8 = 5;
    /// Caught (by `seed`, an id), at `x`.
    pub const CAUGHT: u8 = 6;
    /// Run clean off.
    pub const SANTA_GONE: u8 = 7;
    /// A comet (`y`: its way), a shower of shooting stars, the aurora
    /// flaring (`y`: seconds), a blizzard (seconds), a gust (`y`: its way,
    /// `x`: seconds), a light show by the big tree (seconds).
    pub const COMET: u8 = 10;
    pub const METEORS: u8 = 11;
    pub const FLARE: u8 = 12;
    pub const BLIZZARD: u8 = 13;
    pub const GUST: u8 = 14;
    pub const LIGHTS: u8 = 15;

    /// Only the host's word goes (what's due: the others leave theirs to
    /// it).
    pub fn hosts(what: u8) -> bool {
        matches!(what, SLEIGH | BALLOON | COMET | METEORS | FLARE | BLIZZARD | GUST | LIGHTS)
    }
}

/// What happened, for the scene to show.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A line for the post (top left).
    Note(String),
    /// (`links`: the links taken out of `text`, kept for showing if wanted.)
    Mail { from: String, text: String, links: Vec<String> },
    /// Someone's asking to be friends.
    Asked(String),
    /// Into a shared village: its seed (the village made anew from it).
    Village(u32),
    /// Out of the shared village (home again).
    Home,
    Said { id: u32, text: String, links: Vec<String> },
    Threw { id: u32, x: f32, y: f32, vx: f32, vy: f32 },
    /// A friend asks this villager into their village (its code).
    Invited { name: String, code: String },
    Waved { id: u32, to: u32, what: u8 },
    /// Someone asks to play (see `ask`; `to` 0: everyone).
    Play { id: u32, to: u32, what: u8, seed: u32 },
    Answer { id: u32, what: u8, yes: bool, seed: u32 },
    Table { id: u32, game: u32, seq: u32, kind: u8, state: String },
    Up { id: u32, game: u32 },
    Show { id: u32, what: u8, n: u16, x: f32 },
    Tagged { id: u32, it: u32, left: f32 },
    Star { x: f32, y: f32 },
    Go { seed: u32, at: i64 },
    Finish { id: u32, name: String, score: u32, secs: f32 },
    /// A linked device's side of the villager (see `crate::sync`).
    Synced { dev: String, at: i64, state: String },
    /// A friend pinged this villager.
    Pinged(String),
    /// Said to this villager only, by someone in the village.
    Whispered { id: u32, name: String, text: String, links: Vec<String> },
    /// Said in a group (its name), by whom.
    GroupSaid { group: String, name: String, text: String, links: Vec<String> },
    /// Someone asks the host (this villager) to change a village setting;
    /// `admin`: they may (it's done without asking).
    Wanted { id: u32, name: String, key: String, value: String, admin: bool },
    /// The host's village settings (a guest), and whether this villager
    /// may change them.
    Rules { rules: String, admin: bool },
    /// Who's out in the host's village (a guest; see `Msg::Crowd`).
    Crowd(String),
    /// An announcement from the village's host.
    Notice(String),
    /// Something in the sky (see `Msg::Sky`); `host`: from the village's
    /// host.
    Sky { id: u32, what: u8, x: f32, y: f32, seed: u32, host: bool },
    /// Never mind what they asked (see `Msg::Never`).
    Never { id: u32, what: u8, seed: u32 },
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
    /// so walking reads smooth, not in hops (an older version, which
    /// doesn't say when it sent; see `trail`).
    pub vel: (f32, f32),
    /// Where they've been heard, and when (they're drawn a moment behind,
    /// between two of those).
    pub trail: Trail,
    /// The same on a trip (down the slope: how far, which lane; how high
    /// off the snow).
    pub trip_trail: Trail,
    pub trip_air: Trail,
}

/// The places someone's been heard at (the sender's clock, x, y), oldest
/// first: drawn a moment behind, between two of them, so walking reads
/// smooth however unevenly the messages arrive (`at`).
#[derive(Clone, Debug, Default)]
pub struct Trail {
    /// (their clock, x, y, how they were: see `heard`)
    pts: std::collections::VecDeque<(f32, f32, f32, u8)>,
    /// Their clock against ours: the least (ours - theirs) seen, the
    /// quickest a message has come, creeping up slowly after it so a drift
    /// in either clock isn't left behind (`low`); and what's drawn by,
    /// following it a little each frame (`tick`: a sudden change would show
    /// as a lurch).
    low: f32,
    off: Option<f32>,
    /// How much later than the quickest their messages come (the most
    /// lately, fading slowly), and when that was last heard: what the
    /// delay must cover for them to be in on time.
    late: f32,
    late_at: f32,
    /// How far behind they're drawn now (see `delay`): eased toward what
    /// `late` needs, never a sudden change.
    delay: f32,
}

impl Trail {
    /// Heard at (x, y), sent at `at` on their clock (`now` on ours); a step
    /// further than `jump` is a leap (a door, a teleport): no gliding there.
    pub fn heard(&mut self, now: f32, at: f32, x: f32, y: f32, jump: f32) {
        self.heard_as(now, at, x, y, jump, 0);
    }

    /// Heard at (x, y), and how they were then (`how`: up in the air 1,
    /// walking 2, facing right 4), for drawing them as they were there.
    pub fn heard_as(&mut self, now: f32, at: f32, x: f32, y: f32, jump: f32, how: u8) {
        if ![now, at, x, y].iter().all(|v| v.is_finite()) {
            return;
        }
        if let Some(&(last, lx, ly, _)) = self.pts.back() {
            if at <= last {
                // (late, and out of turn: dropped; their clock started again: afresh)
                if at > last - 5.0 {
                    return;
                }
                self.pts.clear();
                self.off = None;
            } else if (x - lx).abs() > jump || (y - ly).abs() > jump {
                self.pts.clear();
            }
        }
        let d = now - at;
        self.low = if self.off.is_some() { (self.low + 0.0005).min(d) } else { d };
        // (how late this one was, against the quickest; the most late,
        // fading 0.05 s a second)
        let late = (d - self.low).clamp(0.0, 2.0);
        self.late = late.max(self.late - 0.05 * (now - self.late_at).max(0.0));
        self.late_at = now;
        // (far out, their clock jumped, or the first heard: from there)
        if self.off.is_none_or(|o| (self.low - o).abs() > 2.0) {
            self.off = Some(self.low);
        }
        self.pts.push_back((at, x, y, how));
        while self.pts.len() > 12 {
            self.pts.pop_front();
        }
    }

    /// Each frame: what's drawn by follows their clock, at most a few
    /// hundredths faster or slower than it.
    pub fn tick(&mut self, dt: f32) {
        let mut moved = 0.0;
        if let Some(o) = &mut self.off {
            moved = (self.low - *o).clamp(-0.04 * dt, 0.04 * dt);
            *o += moved;
        }
        // the delay: a little over how late their messages come, between
        // the usual 0.2 s (a good link) and 0.8 s (long polling through a
        // slow network); a little at a time, so they're never seen to jump
        // (while it grows they walk 5% slower, while it shrinks 3% quicker:
        // too little to see)
        // (and with the clock's own following above, never more than 5%
        // in all)
        let want = (self.late * 1.15 + 0.05).clamp(TRAIL_DELAY, MAX_DELAY);
        let now = self.delay.max(TRAIL_DELAY);
        let step = (want - now).clamp(-0.03 * dt, 0.05 * dt);
        let step = (moved + step).clamp(-0.05 * dt, 0.05 * dt) - moved;
        self.delay = (now + step).clamp(TRAIL_DELAY, MAX_DELAY);
    }

    /// How far behind them to draw them (see `tick`).
    pub fn delay(&self) -> f32 {
        self.delay.max(TRAIL_DELAY)
    }

    /// Anything heard yet (an older version doesn't say when).
    pub fn known(&self) -> bool {
        self.off.is_some() && !self.pts.is_empty()
    }

    /// Where to draw them at `now` (ours), `delay` behind; past the last
    /// heard, on the way they were going for `coast` seconds at most.
    /// How they were where they're drawn at `now` (see `heard_as`).
    pub fn how(&self, now: f32, delay: f32) -> Option<u8> {
        let rt = now - self.off? - delay;
        self.pts.iter().rev().find(|p| p.0 <= rt).or(self.pts.front()).map(|p| p.3)
    }

    pub fn at(&self, now: f32, delay: f32, coast: f32) -> Option<(f32, f32)> {
        let rt = now - self.off? - delay;
        let (first, last) = (*self.pts.front()?, *self.pts.back()?);
        if rt <= first.0 {
            return Some((first.1, first.2));
        }
        if rt >= last.0 {
            let n = self.pts.len();
            if n >= 2 {
                let p = self.pts[n - 2];
                let gap = last.0 - p.0;
                if gap > 0.0 && gap < 1.0 {
                    let ahead = (rt - last.0).min(coast);
                    return Some((last.1 + (last.1 - p.1) / gap * ahead, last.2 + (last.2 - p.2) / gap * ahead));
                }
            }
            return Some((last.1, last.2));
        }
        let i = self.pts.iter().position(|p| p.0 > rt)?.max(1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let k = ((rt - a.0) / (b.0 - a.0).max(1e-4)).clamp(0.0, 1.0);
        Some((a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k))
    }

    pub fn clear(&mut self) {
        self.pts.clear();
    }
}

/// How far behind others are drawn: a little over the time between their
/// messages, so the next is nearly always in by then.
const TRAIL_DELAY: f32 = 0.2;
/// The most it grows to, on a link whose messages come very unevenly.
const MAX_DELAY: f32 = 0.8;

/// A piece of music as it's playing (see `crate::audio::Cue`), and when
/// that was (milliseconds, the wall clock): a shared village's guests play
/// the same, from the same place.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Tune {
    pub style: u8,
    pub carol: i16,
    pub seed: u32,
    pub t: f32,
    pub at: i64,
}

/// A friend's last "here I am".
#[derive(Clone, Debug, Default)]
pub struct Here {
    pub on: bool,
    pub village: Option<String>,
    pub card: Option<Card>,
    /// What they're playing on, if they said ("linux", "windows",
    /// "android", "macos", "web").
    pub os: Option<String>,
}

/// The app's version: the main Cargo.toml's, read when this is built (the
/// Android library and the web page are built from these same files as
/// packages of their own, whose CARGO_PKG_VERSION isn't the app's).
pub fn app_version() -> &'static str {
    include_str!("../../Cargo.toml").lines().find_map(|l| l.strip_prefix("version = \"")?.split('"').next()).unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// What the app tells a relay it is, as "xmas-tui/1.25.4 (linux)": in its
/// sign-in (every way in, the web page's too) and the web request's
/// User-Agent. The relay turns away versions older than it allows.
pub fn agent() -> String {
    format!("xmas-tui/{} ({})", app_version(), platform())
}

/// The relay this one is out of date before ("1.6.0": the first that
/// turns away games too old to be safe, and says its own version).
pub const MIN_RELAY: &str = "1.6.0";

/// A version ("1.25.4") as numbers, to compare.
pub fn version_num(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split(['.', ' ', '-']).map(|n| n.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

/// What this is playing on, as friends are told.
pub fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else {
        std::env::consts::OS
    }
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
    /// A public square (which, and which copy).
    square: Option<(usize, u32)>,
    /// The most it takes.
    limit: usize,
    /// The host's clock as last heard (a guest), and when it was heard.
    clock: Option<([f32; 3], f32)>,
    /// Who's kept from talking here (by the host; a guest: as heard).
    muted: Vec<u32>,
}

/// Where the village's own relay says things (the public squares'
/// settings, from its console): plain JSON, believed only from that relay.
pub const ADMIN_SQUARES: &str = "xmas-tui/1/admin/squares";
/// The public list's order, featured places, banned codes, extra words.
pub const ADMIN_PUBLIC: &str = "xmas-tui/1/admin/public";
/// For the relay's own host: what it keeps open, and what it's asked to do.
pub const ADMIN_HOST: &str = "xmas-tui/1/admin/host";
pub const ADMIN_CMD: &str = "xmas-tui/1/admin/cmd";
/// The host's reports, by village (`…/report/<name>`).
pub const ADMIN_REPORT: &str = "xmas-tui/1/admin/report/";
/// A village's official word, from the relay's own host (only it may
/// speak there, the relay sees to that): `…/v/<the village topic's id>`.
pub const ADMIN_V: &str = "xmas-tui/1/admin/v/";
/// The relay's host hears village messages here instead, with the
/// connection each came from (`ADMIN_VIA/<connection>/<village>`).
pub const ADMIN_VIA: &str = "xmas-tui/1/admin/via";
/// Ids below this are the relay's own host's: from anyone else, ignored.
pub const HOST_IDS: u32 = 0x1000;

/// A village topic's official twin (where only the relay's host speaks).
fn official(topic: &str) -> String {
    format!("{ADMIN_V}{}", topic.rsplit('/').next().unwrap_or(""))
}

pub struct Online {
    /// Connected to the village's own relay (what it says as itself is
    /// believed), and each public square's settings as it last said them
    /// (JSON, by the square's name).
    trusted: bool,
    /// The same each time from this villager's friend code, and nothing
    /// to get it back from (see `connected`).
    tag: u32,
    /// The relay's own host (no villager of its own; signs in with
    /// `login`): it alone may speak on a village's official topic.
    server: bool,
    login: Option<(String, String)>,
    /// What the relay says of the public villages.
    pub public_rules: PublicRules,
    /// The relay's own host: what the relay's said to it (topic, JSON).
    pub admin_in: Vec<(String, Vec<u8>)>,
    /// The relay's own host: what's been said in its village lately (when,
    /// who, their name, what).
    pub said_here: std::collections::VecDeque<(i64, u32, String, String)>,
    /// The relay's own host: which connection each player id has come
    /// from (as the relay tells it, see `ADMIN_VIA`), and the other way:
    /// one connection, one id, so nobody can speak as somebody else.
    owners: HashMap<u32, u64>,
    conn_ids: HashMap<u64, u32>,
    square_rules: HashMap<String, String>,
    /// This side's clock (minutes into the day, sunrise, sunset), sent to
    /// the guests while hosting (see `set_clock`).
    my_clock: [f32; 3],
    /// This side's music (sent to the guests while hosting), and the
    /// host's as last heard (a guest), not yet taken.
    my_tune: Option<Tune>,
    host_tune: Option<Tune>,
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
    /// When the link last came up (said hello to the relay), and whether
    /// the relay turned the hello down: a relay that never answers, or
    /// won't have us, is left for the next one.
    link_since: f32,
    refused: bool,
    /// Times the relay's turned this connection away (a wrong sign-in, a
    /// banned player): for the relay's own host to say so.
    pub refusals: u32,
    /// Turned away by the relay as too old (its online clock, then): online
    /// waits for an update, rather than going elsewhere, and tries again
    /// in half an hour.
    too_old_at: Option<f32>,
    /// Told this relay is out of date (once).
    said_old_relay: bool,
    relay_skip: usize,
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
    /// A square to go into once the link's up.
    pending_square: Option<usize>,
    /// The most in this villager's own village, and whether it's listed
    /// for anyone to find (in Public villages) when open.
    pub limit: usize,
    /// Strong language in what others say starred out.
    pub clean_words: bool,
    /// Public villages heard of (by code), and listening for them until.
    public: HashMap<String, Listing>,
    watch_until: f32,
    next_status: f32,
    /// Heard lately, per sender: (when the window started, how many),
    /// to shrug off a flood.
    heard: HashMap<u32, (f32, u32)>,
    /// Friends (their codes) whose waiting messages go at once (they've
    /// just come online), and when the outbox is next looked over.
    out_due: std::collections::HashSet<String>,
    next_out: f32,
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
            // (above the relay's own host's ids)
            id: (seal::random() as u32 | 1).max(HOST_IDS + 1),
            tag: seal::number_of(&[b"xmas-tui player", seal::norm(&code).as_bytes()]),
            server: false,
            login: None,
            public_rules: PublicRules::default(),
            admin_in: vec![],
            said_here: std::collections::VecDeque::new(),
            owners: HashMap::new(),
            conn_ids: HashMap::new(),
            trusted: false,
            square_rules: HashMap::new(),
            my_clock: [720.0, 450.0, 1020.0],
            my_tune: None,
            host_tune: None,
            me: Seal::new(&code),
            my_mail: seal::topic("mail", &code),
            my_here: seal::topic("here", &code),
            live: vec![],
            village: None,
            reader: mqtt::Reader::default(),
            out: vec![],
            link: Link::Off,
            link_since: 0.0,
            refused: false,
            refusals: 0,
            too_old_at: None,
            said_old_relay: false,
            relay_skip: 0,
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
            pending_square: None,
            limit: DEFAULT_LIMIT,
            clean_words: true,
            public: HashMap::new(),
            watch_until: 0.0,
            next_status: 0.0,
            heard: HashMap::new(),
            out_due: Default::default(),
            next_out: 0.0,
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

    /// A new link (another relay chosen): the old one closed, the next made.
    pub fn relink_now(&mut self) {
        self.relink = true;
        self.relay_skip = 0;
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

    /// Turned away by the relay as too old, lately (half an hour): not to
    /// be connected anywhere until updated (or until it's worth a try).
    pub fn too_old(&self) -> bool {
        self.too_old_at.is_some_and(|at| self.t - at < 1800.0)
    }

    pub fn link(&self) -> Link {
        self.link
    }

    /// The relay's connection is open but going nowhere: no answer to the
    /// hello in a good while, or it was turned down. (The front end drops
    /// it and tries the next relay.)
    pub fn link_stuck(&self) -> bool {
        self.refused || (self.link == Link::Connecting && self.t - self.link_since > 10.0)
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
        let sealed = seal.seal(&plain);
        // the relay's own host, in a village it keeps: on the official twin
        // too (where only it may speak: newer games believe it there; older
        // ones hear the usual copy)
        if self.server && self.village.as_ref().is_some_and(|v| v.topic == topic) {
            self.out.extend(mqtt::publish(&official(topic), &sealed, false));
        }
        self.out.extend(mqtt::publish(topic, &sealed, retain));
    }

    fn here_msg(&self, on: bool) -> Msg {
        let village = self.village.as_ref().filter(|v| v.host).map(|v| v.code.clone());
        let card = (!self.card.name.is_empty() || !self.card.motto.is_empty()).then(|| self.card.clone());
        Msg::Here { name: self.name.clone(), on, village, at: now(), card, os: platform().into() }
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
        (self.link_since, self.refused) = (self.t, false);
        let will = self.me.seal(&serde_json::to_vec(&self.here_msg(false)).unwrap_or_default());
        // (the id, then a tag that's the same each time from this friend
        // code, which can't be turned back into it: the relay keeps a
        // banned player out by it)
        let client = format!("xt{:08x}{:08x}{:04x}", self.id, self.tag, seal::random() as u16);
        // (the relay's own host signs in as itself; everyone else says what
        // version they are, and a relay turns away one too old)
        let login = self.login.clone().or_else(|| Some((agent(), String::new())));
        self.out.extend(mqtt::connect_as(&client, 45, Some((&self.my_here.clone(), &will)), login.as_ref().map(|(u, p)| (u.as_str(), p.as_str()))));
    }

    fn on_connack(&mut self) {
        self.link = Link::On;
        if self.trusted {
            self.subscribe(vec![ADMIN_SQUARES.to_string(), ADMIN_PUBLIC.to_string()]);
        }
        // the relay's own host: what it's to keep open, and do
        if self.server {
            self.subscribe(vec![ADMIN_HOST.to_string(), ADMIN_CMD.to_string(), ADMIN_VIA.to_string()]);
        }
        // back (perhaps after a blip): where this villager is, at once
        self.next_me = 0.0;
        self.last_me = None;
        self.next_village = 0.0;
        let mut topics = vec![self.my_mail.clone()];
        topics.extend(self.live.iter().map(|l| l.here_topic.clone()));
        if let Some(v) = &self.village {
            topics.push(v.topic.clone());
            topics.push(official(&v.topic));
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
        // (whatever was written meanwhile goes now)
        self.next_out = self.t + 1.0;
        if let Some(code) = self.pending_join.take() {
            self.enter(code, false);
            self.note("⌂ knocking…");
        }
        if let Some(i) = self.pending_square.take() {
            self.join_square(i);
        }
        // (how the public villages are, ready for when they're looked at)
        self.watch_public();
    }

    pub fn disconnected(&mut self) {
        if self.link != Link::Off {
            self.link = Link::Off;
        }
        self.refused = false;
    }

    /// A relay that went nowhere was dropped: the next one's tried first
    /// (how many have been passed over so far).
    pub fn skip_relay(&mut self) -> usize {
        self.relay_skip += 1;
        self.relay_skip
    }

    pub fn relay_skip(&self) -> usize {
        self.relay_skip
    }

    /// Bytes from the relay.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.reader.feed(bytes);
        while let Some(p) = self.reader.next() {
            match p {
                mqtt::Packet::ConnAck(0) => self.on_connack(),
                // too old for this relay: wait for an update (not another relay)
                mqtt::Packet::ConnAck(1) if !self.server => {
                    self.link = Link::Off;
                    self.refused = true;
                    self.refusals = self.refusals.wrapping_add(1);
                    if self.too_old_at.is_none() {
                        self.note(format!("⚠ this version of xmas-tui ({}) is too old for the village's relay: update it to play online", app_version()));
                    }
                    self.too_old_at = Some(self.t);
                }
                mqtt::Packet::ConnAck(_) => {
                    self.link = Link::Off;
                    self.refused = true;
                    self.refusals = self.refusals.wrapping_add(1);
                }
                mqtt::Packet::Publish { topic, payload } => self.got(&topic, &payload),
                _ => {}
            }
        }
    }

    fn got(&mut self, topic: &str, payload: &[u8]) {
        // (the relay's own host: a village message, with the connection it
        // came from)
        let mut via = None;
        let rewritten;
        let topic = match topic.strip_prefix(ADMIN_VIA).and_then(|r| r.strip_prefix('/')).and_then(|r| r.split_once('/')) {
            // (only the relay's own host listens there, and only its relay
            // can send there: nobody else can subscribe to it)
            Some((conn, hash)) if self.server => {
                let Ok(conn) = u64::from_str_radix(conn, 16) else { return };
                via = Some(conn);
                rewritten = format!("xmas-tui/1/village/{hash}");
                rewritten.as_str()
            }
            Some(_) => return,
            None => topic,
        };
        // the public squares' settings, from the village's own relay
        if topic == ADMIN_PUBLIC {
            if self.trusted && payload.len() <= 64 * 1024 {
                if let Ok(r) = serde_json::from_slice::<PublicRules>(payload) {
                    self.public_rules = PublicRules { banned: r.banned.iter().map(|c| seal::norm(c)).collect(), banned_ids: r.banned_ids.iter().take(2000).map(|b| b.to_lowercase()).collect(), words: r.words.iter().take(400).map(|w| w.to_lowercase()).collect(), relay: r.relay.chars().take(20).collect(), min_app: r.min_app.chars().take(20).collect(), ..r };
                    // a relay older than the fixes this game expects: said
                    // (its keeper's to update; a community's own, say)
                    let old = version_num(&self.public_rules.relay).is_none_or(|v| version_num(MIN_RELAY).is_some_and(|m| v < m));
                    if old && !self.said_old_relay {
                        self.said_old_relay = true;
                        let v = if self.public_rules.relay.is_empty() { "an older version".to_string() } else { self.public_rules.relay.clone() };
                        self.note(format!("⚠ this relay ({v}) is out of date: whoever runs it should update it"));
                    }
                }
            }
            return;
        }
        if self.server && (topic == ADMIN_HOST || topic == ADMIN_CMD) {
            if payload.len() <= 512 * 1024 && self.admin_in.len() < 64 {
                self.admin_in.push((topic.to_string(), payload.to_vec()));
            }
            return;
        }
        if topic == ADMIN_SQUARES {
            if self.trusted && payload.len() <= 16 * 1024 {
                if let Ok(serde_json::Value::Object(m)) = serde_json::from_slice::<serde_json::Value>(payload) {
                    self.square_rules = m.into_iter().filter(|(k, v)| SQUARES.contains(&k.as_str()) && v.is_object()).map(|(k, v)| (k, v.to_string())).collect();
                }
            }
            return;
        }
        let parse = |seal: &Seal| seal.open(payload).and_then(|p| serde_json::from_slice::<Msg>(&p).ok());
        // a public village's state (a square's copy, or one listed)
        if topic.starts_with("xmas-tui/1/public/") || topic.starts_with("xmas-tui/1/listed/") {
            if let Some(Msg::Status { name, code, n, limit, at, square, server }) = parse(&public_seal()) {
                let code = seal::norm(&code);
                // (a square's status only on its own topic; a listing only
                // for a real code)
                let ok = code.len() == 12 && (!square || status_topic(&code) == topic) && self.public.len() < 400;
                if ok {
                    let name = if self.clean_words { filter::words(&clean(&name, 40)) } else { clean(&name, 40) };
                    let limit = (limit as usize).clamp(2, MAX_LIMIT);
                    self.public.insert(code.clone(), Listing { name, code, n: (n as usize).min(limit), limit, square, server, at });
                }
            }
            return;
        }
        if topic == self.my_mail {
            match parse(&self.me) {
                Some(Msg::Hi { from, name, back, n }) => {
                    if self.not_flooded(0, 12) && self.got_it(&from, n, !back || self.book.friends.iter().any(|p| seal::norm(&p.code) == seal::norm(&from))) {
                        self.hi(from, name, back);
                    }
                }
                Some(Msg::Got { from, n }) => self.arrived(&from, n),
                Some(Msg::Mail { from, name, text, n }) => {
                    // only from friends
                    if self.is_friend(&from) && self.not_flooded(0, 12) && self.got_it(&from, n, true) {
                        let (text, links) = filter::tidy(&clean(&text, 280), self.clean_words);
                        self.keep_chat(&from, false, &text);
                        // (by the name they have here; a quiet friend's only
                        // goes in the chat)
                        let pal = self.book.friends.iter().find(|p| seal::norm(&p.code) == seal::norm(&from));
                        if !pal.is_some_and(|p| p.quiet) {
                            let name = pal.map(|p| p.name.clone()).unwrap_or_else(|| clean(&name, 32));
                            self.events.push(Event::Mail { from: name, text, links });
                        }
                    }
                }
                Some(Msg::Invite { from, name, village, n }) => {
                    if self.is_friend(&from) && self.not_flooded(0, 12) && seal::norm(&village).len() == 12 && self.got_it(&from, n, true) {
                        let code = seal::norm(&village);
                        // (in the chat with them too, with a way in)
                        self.keep_line(&from, ChatLine { at: now(), mine: false, text: "⌂ come and join me in my village!".into(), invite: code.clone(), ..Default::default() });
                        let pal = self.book.friends.iter().find(|p| seal::norm(&p.code) == seal::norm(&from));
                        let name = pal.map(|p| p.name.clone()).unwrap_or_else(|| clean(&name, 32));
                        self.events.push(Event::Invited { name, code });
                    }
                }
                Some(Msg::Ping { from, name }) => {
                    let quiet = self.book.friends.iter().any(|p| p.quiet && seal::norm(&p.code) == seal::norm(&from));
                    if self.is_friend(&from) && !quiet && self.not_flooded(0, 8) {
                        self.events.push(Event::Pinged(clean(&name, 32)));
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(i) = self.live.iter().position(|l| l.here_topic == topic) {
            if let Some(Msg::Here { name, on, village, at, card, os }) = parse(&self.live[i].seal) {
                let name = clean(&name, 32);
                let village = village.map(|v| seal::norm(&v)).filter(|v| v.len() == 12);
                let card = card.map(Card::cleaned);
                let fresh = now() - at < 180;
                let was_on = self.live[i].here.as_ref().is_some_and(|h| h.on);
                let on = on && fresh;
                let os = Some(os.as_str()).filter(|o| matches!(*o, "linux" | "windows" | "android" | "macos" | "web")).map(String::from);
                self.live[i].here = Some(Here { on, village: village.filter(|_| on), card, os });
                if let Some(p) = self.book.friends.get_mut(i) {
                    if !name.is_empty() && p.name != name {
                        p.name = name.clone();
                        self.dirty = true;
                    }
                }
                if on && !was_on && fresh {
                    let n = self.book.friends.get(i).map(|p| p.name.clone()).unwrap_or_default();
                    self.note(format!("● {n} is online"));
                    // (anything waiting for them goes now)
                    if let Some(code) = self.book.friends.get(i).map(|p| seal::norm(&p.code)) {
                        self.out_due.insert(code.clone());
                        self.flush(&code);
                    }
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
                    let (text, links) = filter::tidy(&text, self.clean_words);
                    let name = if self.clean_words { filter::words(&name) } else { name };
                    self.events.push(Event::GroupSaid { group, name, text, links });
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
        // (its official twin: the relay's own host's word, only from our
        // relay, which lets nobody else speak there)
        let from_host = topic == official(&v.topic) && self.trusted;
        if topic != v.topic && !from_host {
            return;
        }
        let Some(m) = parse(&v.seal) else { return };
        let t = self.t;
        let me = self.id;
        // (one sender flooding the village is shrugged off)
        let said_by = match &m {
            Msg::Me { id, .. } | Msg::Card { id, .. } | Msg::Wave { id, .. } | Msg::Ask { id, .. } | Msg::Answer { id, .. } | Msg::Table { id, .. } | Msg::Up { id, .. } | Msg::Show { id, .. } | Msg::Tagged { id, .. } | Msg::Star { id, .. } | Msg::Go { id, .. } | Msg::Finish { id, .. } | Msg::Talk { id, .. } | Msg::Whisper { id, .. } | Msg::Want { id, .. } | Msg::Throw { id, .. } | Msg::Bye { id, .. } => Some(*id),
            Msg::Village { host, .. } => Some(*host),
            Msg::Crowd { id, .. } | Msg::Full { id, .. } | Msg::Kick { id, .. } | Msg::Notice { id, .. } | Msg::Sky { id, .. } | Msg::Never { id, .. } => Some(*id),
            _ => None,
        };
        // the relay's own host's ids: only on its official topic (and 0 is
        // nobody's)
        if said_by.is_some_and(|id| id == 0 || (id < HOST_IDS && !from_host)) {
            return;
        }
        let sender = said_by.unwrap_or(0);
        // once the relay's own host keeps this village, the host's word
        // (who's host, who's sent home, notices, who's about) is only its
        // own, from its official topic
        let hosts_word = matches!(m, Msg::Village { .. } | Msg::Crowd { .. } | Msg::Full { .. } | Msg::Kick { .. } | Msg::Notice { .. });
        if hosts_word && !from_host && v.host_id.is_some_and(|h| h < HOST_IDS) {
            return;
        }
        // the relay's own host: each id from one connection only, and each
        // connection with one id (the relay says which sent what)
        if let (Some(conn), Some(id)) = (via, said_by) {
            match (self.owners.get(&id), self.conn_ids.get(&conn)) {
                (Some(c), _) if *c != conn => return,
                (None, Some(other)) if *other != id => return,
                (None, _) => {
                    if self.owners.len() < 4096 {
                        self.owners.insert(id, conn);
                        self.conn_ids.insert(conn, id);
                    }
                }
                _ => {}
            }
            if let Msg::Bye { .. } = m {
                self.owners.remove(&id);
                self.conn_ids.remove(&conn);
            }
        }
        if sender != me && !self.not_flooded(sender, 60) {
            return;
        }
        let admins_here = self.admins.clone();
        let server = self.server;
        let v = self.village.as_mut().unwrap();
        // the relay's own host, on its official topic: the host, whoever
        // was taken for it before (a player hosting the square till now
        // steps down; one that only claimed it is forgotten)
        if let Msg::Village { host, .. } = &m {
            if from_host && *host != me && v.host_id != Some(*host) && !server {
                v.host = false;
                v.host_id = None;
            }
        }
        match m {
            // full (the host turns them away; anyone else just doesn't take
            // in more than the most there's ever room for)
            Msg::Me { id, .. } if id != me && !v.others.contains_key(&id) && v.host && v.others.len() + 1 >= v.limit => {
                let (topic, seal) = (v.topic.clone(), v.seal.clone());
                if self.not_flooded(id ^ 0x5f11_0000, 3) {
                    self.send(&topic, &seal, &Msg::Full { id: me, to: id }, false);
                }
            }
            Msg::Me { id, .. } if id != me && !v.others.contains_key(&id) && v.others.len() >= MAX_LIMIT => {}
            Msg::Me { id, name, av } if id != me => {
                let name = if self.clean_words { filter::words(&clean(&name, 32)) } else { clean(&name, 32) };
                let av = av.cleaned();
                let new = !v.others.contains_key(&id);
                let host = v.host_id == Some(id);
                let o = v.others.entry(id).or_insert(Other { name: name.clone(), av, shown: (av.x, av.y), seen: t, host, card: None, wave: (0.0, 0), vel: (0.0, 0.0), trail: Trail::default(), trip_trail: Trail::default(), trip_air: Trail::default() });
                // (going in or out, off on a trip: a leap, not a walk)
                if o.av.away != av.away {
                    o.trail.clear();
                }
                if av.at > 0.0 {
                    let how = av.air as u8 | (av.moving as u8) << 1 | ((av.dir > 0.0) as u8) << 2;
                    o.trail.heard_as(t, av.at, av.x, av.y, 0.6, how);
                    // (down the slope, round the moon: drawn smoothly)
                    match av.trip.filter(|tr| av.away == 2 && (tr.at == 5 || tr.at == 8)) {
                        Some(tr) => {
                            o.trip_trail.heard(t, av.at, tr.x, tr.lane, 40.0);
                            o.trip_air.heard(t, av.at, tr.air, 0.0, 1e6);
                        }
                        None => {
                            o.trip_trail.clear();
                            o.trip_air.clear();
                        }
                    }
                }
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
            Msg::Answer { id, to, what, yes, seed } if to == me => self.events.push(Event::Answer { id, what, yes, seed }),
            Msg::Table { id, game, seq, kind, state } if id != me && state.len() <= 8192 => self.events.push(Event::Table { id, game, seq, kind, state }),
            Msg::Up { id, game } if id != me => self.events.push(Event::Up { id, game }),
            Msg::Show { id, what, n, x } if id != me => self.events.push(Event::Show { id, what, n, x }),
            Msg::Tagged { id, it, left } if id != me => self.events.push(Event::Tagged { id, it, left }),
            Msg::Go { id, seed, at } if id != me => self.events.push(Event::Go { seed, at }),
            Msg::Star { id, x, y } if id != me && x.is_finite() && y.is_finite() && x.abs() < 1e4 => self.events.push(Event::Star { x, y: y.clamp(0.0, 400.0) }),
            Msg::Sky { id, what, x, y, seed } if id != me && x.is_finite() && y.is_finite() && x.abs() < 1e5 && y.abs() < 1e4 => {
                let host = v.host_id == Some(id);
                // (what's due is the host's to say)
                if host || !sky::hosts(what) {
                    self.events.push(Event::Sky { id, what, x, y, seed, host });
                }
            }
            Msg::Never { id, what, seed } if id != me => self.events.push(Event::Never { id, what, seed }),
            Msg::Finish { id, name, score, secs } if id != me => self.events.push(Event::Finish { id, name: clean(&name, 32), score, secs: if secs.is_finite() { secs } else { 0.0 } }),
            // (someone the host's keeping quiet: not heard)
            Msg::Talk { id, .. } if v.muted.contains(&id) => {}
            Msg::Talk { id, text } if id != me => {
                let (text, links) = filter::tidy(&clean(&text, 120), self.clean_words);
                // a public village: its relay's words starred out too
                let public = v.square.is_some() || self.public.contains_key(&v.code);
                let text = if public { filter::also(&text, &self.public_rules.words) } else { text };
                if self.server {
                    let name = v.others.get(&id).map(|o| o.name.clone()).unwrap_or_default();
                    self.said_here.push_back((now(), id, name, text.clone()));
                    while self.said_here.len() > 60 {
                        self.said_here.pop_front();
                    }
                }
                self.events.push(Event::Said { id, text, links });
            }
            // sent home by the host
            Msg::Kick { id, to } if to == me && v.host_id == Some(id) => {
                self.leave_village();
                self.note("⌂ the village's host sent you home");
                return;
            }
            // the host's announcement
            Msg::Notice { id, to, text } if (to == 0 || to == me) && v.host_id == Some(id) => {
                let text = clean(&text, 200);
                if !text.is_empty() {
                    self.events.push(Event::Notice(text));
                }
            }
            Msg::Whisper { id, to, name, text } if id != me && to == me => {
                let (text, links) = filter::tidy(&clean(&text, 280), self.clean_words);
                let name = if self.clean_words { filter::words(&clean(&name, 32)) } else { clean(&name, 32) };
                self.events.push(Event::Whispered { id, name, text, links });
            }
            Msg::Crowd { id, data } if id != me && !v.host && v.host_id == Some(id) && data.len() <= 48 * 1024 => self.events.push(Event::Crowd(data)),
            Msg::Want { id, key, value } if id != me && v.host && v.others.contains_key(&id) && key.len() <= 24 && value.len() <= 64 => {
                let name = v.others.get(&id).map(|o| o.name.clone()).unwrap_or_default();
                self.events.push(Event::Wanted { id, name, key, value, admin: admins_here.contains(&id) });
            }
            Msg::Throw { id, x, y, vx, vy } if id != me && [x, y, vx, vy].iter().all(|v| v.is_finite() && v.abs() < 1e4) => self.events.push(Event::Threw { id, x, y, vx, vy }),
            Msg::Full { id, to } if to == me && !v.host && v.host_id.is_none_or(|h| h == id) => {
                match v.square {
                    // a square: the next copy of it
                    Some((i, n)) if n < SQUARE_COPIES => {
                        self.leave_village_quietly();
                        self.enter_square(i, n + 1);
                    }
                    _ => {
                        self.leave_village();
                        self.note("⌂ that village is full right now");
                    }
                }
            }
            // a square with two hosts (both came in at once): the lower id
            // keeps it, the other steps down
            Msg::Village { host, .. } if host != me && v.square.is_some() && v.host && host < me && !server => {
                v.host = false;
                v.host_id = Some(host);
                v.host_seen = t;
            }
            Msg::Village { host, .. } if host != me && v.square.is_some() && (v.host || v.host_id.is_some_and(|h| h < host)) => {}
            // (the first host heard is the host: nobody else takes over)
            Msg::Village { host, .. } if host != me && v.square.is_none() && v.host_id.is_some_and(|h| h != host) => {}
            Msg::Village { host, name, seed, rules, admins, limit, clock, music, muted } if host != me => {
                self.host_tune = music.filter(|m| m.t.is_finite() && m.t >= 0.0 && (m.style as usize) < 4);
                if v.host_id.is_none_or(|h| h == host) {
                    v.muted = muted.into_iter().take(500).collect();
                }
                v.host_seen = t;
                v.host_id = Some(host);
                if let Some(c) = clock.filter(|c| c.iter().all(|x| x.is_finite() && (0.0..=1440.0).contains(x))) {
                    v.clock = Some((c, t));
                }
                if limit > 0 {
                    v.limit = (limit as usize).clamp(2, MAX_LIMIT);
                }
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
                if host && v.square.is_some() {
                    // a square stays open: the next villager hosts it
                    self.next_host();
                } else if host && !v.host {
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
                let m = Msg::Hi { from: self.book.code.clone(), name: self.name.clone(), back: true, n: 0 };
                self.send(&mail, &seal, &m, false);
            }
            // (they've said yes, or asked too: asking them is done)
            let before = self.book.outbox.len();
            self.book.outbox.retain(|o| !(o.to == code && o.kind == out::HI));
            if self.book.outbox.len() != before {
                self.dirty = true;
            }
            return;
        }
        if back {
            return;
        }
        if !self.book.asking.iter().any(|p| seal::norm(&p.code) == code) && self.book.asking.len() < 20 {
            self.book.asking.push(Pal { name: name.clone(), code, waiting: false, quiet: false });
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
        if self.watch_until > 0.0 && t >= self.watch_until {
            self.watch_until = 0.0;
            self.unwatch_public();
        }
        if t >= self.next_ping {
            self.next_ping = t + 30.0;
            self.out.extend(mqtt::ping());
        }
        if t >= self.next_out {
            self.next_out = t + 5.0;
            self.tend_outbox();
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
        // the host: the village, every couple of seconds (a square: as it
        // is, the same for whoever hosts it)
        let square = self.village.as_ref().and_then(|v| v.square);
        let limit = self.village.as_ref().map(|v| v.limit).unwrap_or(DEFAULT_LIMIT);
        let muted = self.village.as_ref().map(|v| v.muted.clone()).unwrap_or_default();
        if host && t >= self.next_village {
            self.next_village = t + 2.0;
            let m = match square {
                Some((i, _)) => Msg::Village { host: self.id, name: SQUARES[i].to_string(), seed: square_seed(i), rules: String::new(), admins: vec![], limit: limit as u16, clock: Some(self.my_clock), music: self.my_tune, muted: muted.clone() },
                None => Msg::Village { host: self.id, name: self.name.clone(), seed, rules: self.rules.clone(), admins: self.admins.clone(), limit: limit as u16, clock: Some(self.my_clock), music: self.my_tune, muted: muted.clone() },
            };
            self.send(&topic, &seal, &m, false);
        }
        // a public village's host: how it is, for Public villages (kept by
        // the relay), now and then
        if host && (square.is_some() || self.book.listed) && t >= self.next_status {
            self.next_status = t + 15.0;
            let v = self.village.as_ref().unwrap();
            // (the relay's own host isn't someone there)
            let n = (v.others.len() + !self.server as usize) as u16;
            let (name, code, at) = match square {
                Some((i, c)) => (if c > 1 { format!("{} #{c}", SQUARES[i]) } else { SQUARES[i].to_string() }, v.code.clone(), now()),
                None if self.server => (self.name.clone(), v.code.clone(), now()),
                None => (format!("{}'s village", self.name), v.code.clone(), now()),
            };
            let m = Msg::Status { name, code: code.clone(), n, limit: limit as u16, at, square: square.is_some(), server: self.server };
            let st = if square.is_some() { status_topic(&code) } else { listed_topic(self.id) };
            self.send(&st, &public_seal(), &m, true);
        }
        // where this villager is: often while it changes, now and then if not
        if let Some(av) = me {
            let changed = self.last_me.is_none_or(|l| (l.x - av.x).abs() > 0.004 || (l.y - av.y).abs() > 0.004 || l.dir != av.dir || l.moving != av.moving || l.away != av.away || l.rest != av.rest);
            if t >= self.next_me && (changed || t >= self.next_me + 1.0) {
                self.next_me = t + 0.12;
                let mut av = av;
                av.at = t;
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
        // a square: nobody there to answer (the first in hosts it), or its
        // host gone quiet (the next takes over)
        if v.square.is_some() && !v.host {
            if v.host_id.is_none() && t > v.knock_until {
                self.host_square();
            } else if v.host_id.is_some() && t - v.host_seen > 8.0 {
                self.next_host();
            }
            return;
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
            let kt = 1.0 - (-dt * 25.0).exp();
            for o in v.others.values_mut() {
                // drawn a moment behind, between the places heard (only a
                // light easing on top, for a message that changes course)
                o.trail.tick(dt);
                o.trip_trail.tick(dt);
                o.trip_air.tick(dt);
                if let Some((tx, ty)) = o.trail.known().then(|| o.trail.at(t, o.trail.delay(), 0.3)).flatten() {
                    if (tx - o.shown.0).abs() > 0.6 || (ty - o.shown.1).abs() > 0.6 {
                        o.shown = (tx, ty);
                    } else {
                        o.shown.0 += (tx - o.shown.0) * kt;
                        o.shown.1 += (ty - o.shown.1) * kt;
                    }
                    continue;
                }
                // (an older version: where they'll be by now, going on as
                // they were, for a moment at most)
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

    /// Someone's trip as it's drawn: down the slope, a moment behind,
    /// between the places heard (see `Trail`).
    pub fn trip_shown(&self, o: &Other) -> Option<TripAt> {
        let mut at = o.av.trip?;
        if o.trip_trail.known() {
            if let Some((x, lane)) = o.trip_trail.at(self.t, o.trip_trail.delay(), 0.3) {
                (at.x, at.lane) = (x, lane);
            }
            if let Some((air, _)) = o.trip_air.at(self.t, o.trip_trail.delay(), 0.0) {
                at.air = air.max(0.0);
            }
        }
        Some(at)
    }

    /// How someone is where they're drawn (a moment behind: see `Trail`):
    /// up in the air, walking, which way they face.
    pub fn shown_as(&self, o: &Other) -> (bool, bool, f32) {
        match o.trail.known().then(|| o.trail.how(self.t, o.trail.delay())).flatten() {
            Some(h) => (h & 1 != 0, h & 2 != 0, if h & 4 != 0 { 1.0 } else { -1.0 }),
            None => (o.av.air, o.av.moving, o.av.dir),
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
        let mut v = Shared { seal: Seal::new(&code), topic: seal::topic("village", &code), code: seal::norm(&code), host: true, seed: None, host_seen: 0.0, others: HashMap::new(), knock_until: f32::INFINITY, rules: (String::new(), false), host_id: None, square: None, limit: DEFAULT_LIMIT, clock: None, muted: vec![] };
        for (id, name, av, card) in others {
            v.others.insert(id, Other { name: name.into(), av, shown: (av.x, av.y), seen: 1e9, host: false, card, wave: (0.0, 0), vel: (0.0, 0.0), trail: Trail::default(), trip_trail: Trail::default(), trip_air: Trail::default() });
        }
        self.village = Some(v);
    }

    /// Previews: friends, each online or not (and with a village open).
    pub fn pretend_friends(&mut self, friends: &[(&str, bool, bool)]) {
        for (i, (name, on, village)) in friends.iter().enumerate() {
            let code = format!("PRVW{i:04}ABCD");
            self.book.friends.push(Pal { name: name.to_string(), code, waiting: false, quiet: false });
            self.relive();
            if let Some(l) = self.live.last_mut() {
                l.here = Some(Here { on: *on, village: village.then(|| "ABCD-EFGH-JKLM".into()), card: None, os: Some(["linux", "android", "windows", "web"][i % 4].into()) });
            }
        }
    }

    /// Previews: a chat with friend `i` (theirs: `mine` false), `ago`
    /// seconds back.
    pub fn pretend_chat(&mut self, i: usize, lines: &[(bool, &str, i64)]) {
        let Some(code) = self.book.friends.get(i).map(|p| seal::norm(&p.code)) else { return };
        let now = crate::clock::now().timestamp();
        let v = self.book.chats.entry(code).or_default();
        for (mine, text, ago) in lines {
            v.push(ChatLine { at: now - ago, mine: *mine, text: text.to_string(), ..Default::default() });
        }
    }

    /// This villager's id in the shared village.
    pub fn my_id(&self) -> u32 {
        self.id
    }

    /// Connected to the village's own relay: what it says as itself (the
    /// squares' settings) is believed.
    pub fn set_trusted(&mut self, on: bool) {
        self.trusted = on;
        if !on {
            self.square_rules.clear();
        }
    }

    /// In a public square: its settings from the relay's console (JSON;
    /// "{}" when it's said none), or None outside the squares.
    pub fn square_rules(&self) -> Option<String> {
        let i = self.village.as_ref()?.square?.0;
        Some(self.square_rules.get(SQUARES[i]).cloned().unwrap_or_else(|| "{}".into()))
    }

    /// This side's clock (minutes into the day, sunrise, sunset): what the
    /// guests get while hosting.
    pub fn set_clock(&mut self, mins: f32, rise: f32, set: f32) {
        self.my_clock = [mins, rise, set];
    }

    /// A guest: the host's clock as it is now (minutes into the day,
    /// sunrise, sunset), so the sky's the same for everyone in the village.
    pub fn village_clock(&self) -> Option<(f32, f32, f32)> {
        let v = self.village.as_ref().filter(|v| !v.host)?;
        let ([m, r, s], at) = v.clock?;
        Some(((m + (self.t - at) / 60.0).rem_euclid(1440.0), r, s))
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
        self.village.as_ref().filter(|v| !v.host || v.square.is_some()).and_then(|v| v.seed)
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
            self.book.friends.push(Pal { name: "(asked)".into(), code: code.clone(), waiting: true, quiet: false });
            self.dirty = true;
            self.relive();
            let topic = self.live.last().map(|l| l.here_topic.clone()).unwrap_or_default();
            self.subscribe(vec![topic]);
        }
        // (asked again: the one already waiting will do)
        if !self.book.outbox.iter().any(|o| o.to == code && o.kind == out::HI) {
            self.queue(&code, out::HI, String::new());
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
        self.queue(&code, out::YES, String::new());
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
            // (the chat with them goes too, and anything waiting for them)
            self.clear_chat(i);
            let code = seal::norm(&self.book.friends[i].code);
            self.book.outbox.retain(|o| o.to != code);
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
    /// (Offline, or they are: it waits, shown so, and goes when it can.)
    pub fn mail(&mut self, i: usize, text: &str) -> bool {
        let Some(p) = self.book.friends.get(i).filter(|p| !p.waiting) else { return false };
        let code = seal::norm(&p.code);
        let text: String = text.chars().take(280).collect();
        let n = self.queue(&code, out::MAIL, text.clone());
        self.keep_line(&code, ChatLine { at: now(), mine: true, text, n, state: sent::WAITING, invite: String::new() });
        self.flush(&code);
        true
    }

    /// A line of a chat with the friend whose code that is, kept.
    fn keep_chat(&mut self, code: &str, mine: bool, text: &str) {
        self.keep_line(code, ChatLine { at: now(), mine, text: text.to_string(), ..Default::default() });
    }

    fn keep_line(&mut self, code: &str, line: ChatLine) {
        let key = seal::norm(code);
        let lines = self.book.chats.entry(key).or_default();
        lines.push(line);
        if lines.len() > CHAT_KEPT {
            let extra = lines.len() - CHAT_KEPT;
            lines.drain(..extra);
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ the outbox

    /// Into the outbox for `to` (sent now if it can be; see `flush`): its
    /// number.
    fn queue(&mut self, to: &str, kind: u8, text: String) -> u32 {
        let to = seal::norm(to);
        self.book.next_n = self.book.next_n.wrapping_add(1).max(1);
        let n = self.book.next_n;
        self.book.outbox.push(Outgoing { to: to.clone(), n, kind, text, at: now(), sent: 0, tries: 0 });
        // (a long way offline: the oldest give way, at most 200 waiting)
        while self.book.outbox.len() > 200 {
            let o = self.book.outbox.remove(0);
            self.lost(&o);
        }
        self.dirty = true;
        self.out_due.insert(to.clone());
        self.flush(&to);
        n
    }

    /// What goes for an outbox entry.
    fn out_msg(&self, o: &Outgoing) -> Msg {
        let (from, name, n) = (self.book.code.clone(), self.name.clone(), o.n);
        match o.kind {
            out::HI => Msg::Hi { from, name, back: false, n },
            out::YES => Msg::Hi { from, name, back: true, n },
            out::INVITE => Msg::Invite { from, name, village: o.text.clone(), n },
            _ => Msg::Mail { from, name, text: o.text.clone(), n },
        }
    }

    /// Whatever's waiting for `to` that's due goes (online): never sent
    /// yet, or not lately (they've just come online: `out_due`).
    fn flush(&mut self, to: &str) {
        if self.link != Link::On {
            return;
        }
        let due_now = self.out_due.remove(to);
        let t = now();
        let mut sent_n = 0;
        let mut msgs = vec![];
        for o in self.book.outbox.iter_mut().filter(|o| o.to == to) {
            // (a few at a time: their mailbox shrugs off a flood)
            if sent_n >= 4 {
                self.out_due.insert(to.to_string());
                break;
            }
            let on = self.live.iter().zip(&self.book.friends).any(|(l, p)| seal::norm(&p.code) == to && l.here.as_ref().is_some_and(|h| h.on));
            if o.sent == 0 || due_now || (on && t - o.sent >= 600) {
                o.sent = t;
                o.tries = o.tries.saturating_add(1);
                sent_n += 1;
                msgs.push(o.clone());
            }
        }
        if !msgs.is_empty() {
            self.dirty = true;
        }
        for o in msgs {
            let m = self.out_msg(&o);
            self.send(&seal::topic("mail", &o.to), &Seal::new(&o.to), &m, false);
        }
    }

    /// Now and then: what's due goes; what's waited too long is given up
    /// (a month; an invite, once its village has closed or after an hour).
    fn tend_outbox(&mut self) {
        let t = now();
        let open = self.village.as_ref().filter(|v| v.host).map(|v| v.code.clone());
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.book.outbox).into_iter().partition(|o| {
            let old = t - o.at > if o.kind == out::INVITE { 3600 } else { 30 * 86400 };
            let closed = o.kind == out::INVITE && open.as_deref() != Some(o.text.as_str());
            !(old || closed || o.tries >= 12)
        });
        self.book.outbox = keep;
        for o in &gone {
            self.lost(o);
        }
        if !gone.is_empty() {
            self.dirty = true;
        }
        let mut to: Vec<String> = self.book.outbox.iter().map(|o| o.to.clone()).collect();
        to.dedup();
        for code in to {
            self.flush(&code);
        }
    }

    /// Given up on: its line in the chat says so.
    fn lost(&mut self, o: &Outgoing) {
        if let Some(l) = self.book.chats.get_mut(&o.to).and_then(|v| v.iter_mut().find(|l| l.mine && l.n == o.n)) {
            l.state = sent::LOST;
        }
        if o.kind == out::HI {
            self.note("✉ a friend request couldn't be sent (a month waiting): try again");
        }
    }

    /// From `from`, its number `n`: their side's told it came (true: it's
    /// new, to be shown; false: had it already). `back`: worth saying so.
    fn got_it(&mut self, from: &str, n: u32, back: bool) -> bool {
        let code = seal::norm(from);
        if n == 0 || code.len() != 12 {
            return true;
        }
        if back {
            let m = Msg::Got { from: self.book.code.clone(), n };
            self.send(&seal::topic("mail", &code), &Seal::new(&code), &m, false);
        }
        if self.book.got.len() >= 400 && !self.book.got.contains_key(&code) {
            return true;
        }
        let seen = self.book.got.entry(code).or_default();
        if seen.contains(&n) {
            return false;
        }
        seen.push(n);
        if seen.len() > 64 {
            seen.remove(0);
        }
        self.dirty = true;
        true
    }

    /// Their side says number `n` came: out of the outbox, and its line in
    /// the chat says so.
    fn arrived(&mut self, from: &str, n: u32) {
        let code = seal::norm(from);
        let Some(i) = self.book.outbox.iter().position(|o| o.to == code && o.n == n) else { return };
        self.book.outbox.remove(i);
        if let Some(l) = self.book.chats.get_mut(&code).and_then(|v| v.iter_mut().find(|l| l.mine && l.n == n)) {
            l.state = sent::THERE;
        }
        self.dirty = true;
    }

    /// Something for friend `i` not there yet: asking to be friends (or a
    /// yes), how many messages.
    pub fn waiting_for(&self, i: usize) -> (bool, usize) {
        let Some(code) = self.book.friends.get(i).map(|p| seal::norm(&p.code)) else { return (false, 0) };
        let mine = self.book.outbox.iter().filter(|o| o.to == code);
        let (mut hi, mut n) = (false, 0);
        for o in mine {
            match o.kind {
                out::HI | out::YES => hi = true,
                _ => n += 1,
            }
        }
        (hi, n)
    }

    /// The chat with friend `i` so far (the latest last).
    pub fn chat(&self, i: usize) -> &[ChatLine] {
        self.book.friends.get(i).and_then(|p| self.book.chats.get(&seal::norm(&p.code))).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The chat with friend `i`, gone from this device (it's kept nowhere
    /// else).
    pub fn clear_chat(&mut self, i: usize) {
        let Some(code) = self.book.friends.get(i).map(|p| seal::norm(&p.code)) else { return };
        self.book.chats.remove(&code);
        self.book.read.remove(&code);
        self.dirty = true;
    }

    /// How many lines from friend `i` haven't been read yet.
    pub fn unread(&self, i: usize) -> usize {
        let Some(p) = self.book.friends.get(i) else { return 0 };
        let code = seal::norm(&p.code);
        let (since, n) = self.book.read.get(&code).copied().unwrap_or((0, 0));
        let mut same = 0;
        self.book
            .chats
            .get(&code)
            .map(|v| {
                v.iter()
                    .filter(|l| {
                        if l.at == since {
                            same += 1;
                        }
                        !l.mine && (l.at > since || (l.at == since && same > n))
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Friend `i`'s chat, read (as far as it goes now).
    pub fn mark_read(&mut self, i: usize) {
        let Some(p) = self.book.friends.get(i) else { return };
        let code = seal::norm(&p.code);
        let v = self.book.chats.get(&code).map(|v| v.as_slice()).unwrap_or(&[]);
        let last = v.last().map(|l| l.at).unwrap_or(0);
        let now = (last, v.iter().filter(|l| l.at == last).count() as u32);
        if self.book.read.get(&code) != Some(&now) {
            self.book.read.insert(code, now);
            self.dirty = true;
        }
    }

    fn enter(&mut self, code: String, host: bool) {
        if self.village.is_some() {
            self.leave_village();
        }
        let topic = seal::topic("village", &code);
        let limit = if host { self.limit.clamp(2, MAX_LIMIT) } else { MAX_LIMIT };
        self.village = Some(Shared { seal: Seal::new(&code), topic: topic.clone(), code, host, seed: None, host_seen: self.t, others: HashMap::new(), knock_until: self.t + 12.0, rules: (String::new(), false), host_id: None, square: None, limit, clock: None, muted: vec![] });
        // (and its official twin: the relay's own host's word, if it keeps it)
        self.subscribe(vec![topic.clone(), official(&topic)]);
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
        if self.public_rules.is_banned(&code) {
            return Err("that village has been closed by the relay");
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

    /// Into a public square (the first copy of it with room, as last
    /// heard), going online first if need be.
    pub fn join_square(&mut self, i: usize) {
        let i = i % SQUARES.len();
        if self.link != Link::On {
            if !self.book.on {
                self.go_online(true);
            }
            self.pending_square = Some(i);
            self.note("✉ going online first…");
            return;
        }
        let fresh = now() - 60;
        let n = (1..=SQUARE_COPIES).find(|&n| self.public.get(&square_code(i, n)).is_none_or(|l| l.at < fresh || l.n < l.limit)).unwrap_or(1);
        self.enter_square(i, n);
    }

    fn enter_square(&mut self, i: usize, n: u32) {
        self.enter(square_code(i, n), false);
        let seed = square_seed(i);
        let t = self.t;
        if let Some(v) = self.village.as_mut() {
            v.square = Some((i, n));
            v.seed = Some(seed);
            v.limit = SQUARE_LIMIT;
            // (nobody answering in a moment: it's empty, and this villager hosts it)
            v.knock_until = t + 5.0;
        }
        self.events.push(Event::Village(seed));
        self.note(format!("⌂ into {}{}…", SQUARES[i], if n > 1 { format!(" #{n}") } else { String::new() }));
    }

    /// Nobody else in the square: this villager hosts it.
    fn host_square(&mut self) {
        let me = self.id;
        if let Some(v) = self.village.as_mut() {
            v.host = true;
            v.host_id = Some(me);
            v.limit = SQUARE_LIMIT;
        }
        self.next_village = 0.0;
        self.next_status = 0.0;
    }

    /// A square's host has gone: whoever's here with the lowest id takes
    /// over (everyone works out the same one).
    fn next_host(&mut self) {
        let me = self.id;
        let t = self.t;
        let Some(v) = self.village.as_mut() else { return };
        let old = v.host_id;
        if let Some(o) = old {
            v.others.remove(&o);
        }
        let first = v.others.keys().copied().chain(std::iter::once(me)).min().unwrap_or(me);
        if first == me {
            self.host_square();
            if let Some((i, _)) = self.village.as_ref().and_then(|v| v.square) {
                self.note(format!("⌂ hosting {} now (it carries on when you go)", SQUARES[i]));
            }
        } else {
            v.host = false;
            v.host_id = None;
            // (waiting a moment for the new host to say so)
            v.host_seen = t;
            v.knock_until = t + 8.0;
        }
    }

    /// Off to another copy of a square: out without a fuss.
    fn leave_village_quietly(&mut self) {
        let Some(v) = self.village.take() else { return };
        let m = Msg::Bye { id: self.id, host: false };
        self.send(&v.topic, &v.seal, &m, false);
        if self.link == Link::On {
            self.sub = self.sub.wrapping_add(1).max(1);
            self.out.extend(mqtt::unsubscribe(self.sub, &[v.topic.clone()]));
        }
    }

    /// In a public square: which (its name, with the copy), and hosting it.
    pub fn square(&self) -> Option<(String, bool)> {
        self.village.as_ref().and_then(|v| v.square.map(|(i, n)| (if n > 1 { format!("{} #{n}", SQUARES[i]) } else { SQUARES[i].to_string() }, v.host)))
    }

    /// How many are in this village, and the most it takes.
    pub fn headcount(&self) -> Option<(usize, usize)> {
        self.village.as_ref().map(|v| (v.others.len() + 1, v.limit))
    }

    /// Listen for the public villages (the squares, and villages listed by
    /// their hosts) a while: Public villages is open.
    pub fn watch_public(&mut self) {
        if self.link != Link::On {
            return;
        }
        if self.t >= self.watch_until {
            let mut topics: Vec<String> = (0..SQUARES.len()).flat_map(|i| (1..=SQUARE_COPIES).map(move |n| status_topic(&square_code(i, n)))).collect();
            topics.push("xmas-tui/1/listed/+".into());
            self.subscribe(topics);
        }
        self.watch_until = self.t + 90.0;
    }

    fn unwatch_public(&mut self) {
        if self.link == Link::On {
            let mut topics: Vec<String> = (0..SQUARES.len()).flat_map(|i| (1..=SQUARE_COPIES).map(move |n| status_topic(&square_code(i, n)))).collect();
            topics.push("xmas-tui/1/listed/+".into());
            self.sub = self.sub.wrapping_add(1).max(1);
            self.out.extend(mqtt::unsubscribe(self.sub, &topics));
        }
    }

    /// The squares, and how many are in each (all its copies), as heard.
    pub fn squares(&self) -> Vec<(String, usize)> {
        let fresh = now() - 60;
        SQUARES.iter().enumerate().map(|(i, name)| (name.to_string(), (1..=SQUARE_COPIES).filter_map(|n| self.public.get(&square_code(i, n))).filter(|l| l.at >= fresh).map(|l| l.n).sum())).collect()
    }

    /// Villages their hosts have listed for anyone, as heard (busiest
    /// first; a few dozen at most).
    pub fn listings(&self) -> Vec<Listing> {
        let fresh = now() - 60;
        let own = self.village.as_ref().map(|v| v.code.clone());
        let rules = &self.public_rules;
        let mut v: Vec<Listing> = self.public.values().filter(|l| !l.square && l.at >= fresh && Some(&l.code) != own.as_ref() && !rules.is_banned(&l.code)).cloned().collect();
        // (featured first, then as the relay orders them, then the busiest)
        v.sort_by(|a, b| self.rank(a).cmp(&self.rank(b)).then(b.n.cmp(&a.n)).then(a.name.cmp(&b.name)));
        v.truncate(30);
        v
    }

    /// Where a public village goes in the list, as the relay has it.
    fn rank(&self, l: &Listing) -> usize {
        let r = &self.public_rules;
        let at = |list: &[String]| list.iter().position(|x| x.trim().eq_ignore_ascii_case(&l.name) || seal::norm(x) == l.code);
        match (at(&r.featured), at(&r.order)) {
            (Some(i), _) => i,
            (None, Some(i)) => r.featured.len() + i,
            _ => usize::MAX / 2,
        }
    }

    /// Featured by the relay (a ★ in the list).
    pub fn featured(&self, l: &Listing) -> bool {
        self.public_rules.featured.iter().any(|x| x.trim().eq_ignore_ascii_case(&l.name) || seal::norm(x) == l.code)
    }

    // ---------------- the relay's own host (see `crate::host`)

    /// The relay's own host, as one of its villages' (`id` below
    /// `HOST_IDS`): no villager of its own, signed in with the token.
    pub fn server(id: u32, name: &str, token: &str) -> Online {
        let mut o = Online::new(Book { on: true, ..Default::default() }, name);
        (o.server, o.id) = (true, id.clamp(1, HOST_IDS - 1));
        o.login = Some(("xmas-host".into(), token.into()));
        o
    }

    /// Keep this village open (the relay's own host): a square (`square`),
    /// or a server village by its code, listed for anyone or not.
    pub fn keep_open(&mut self, code: &str, square: Option<usize>, limit: usize, listed: bool) {
        self.enter(seal::norm(code), true);
        let me = self.id;
        if let Some(v) = self.village.as_mut() {
            v.square = square.map(|i| (i % SQUARES.len(), 1));
            v.limit = if square.is_some() { SQUARE_LIMIT } else { limit.clamp(2, MAX_LIMIT) };
            v.host_id = Some(me);
        }
        self.book.listed = listed && square.is_none();
        (self.next_village, self.next_status) = (0.0, 0.0);
    }

    /// The relay's own host: someone sent home from its village.
    pub fn kick(&mut self, id: u32) {
        let m = Msg::Kick { id: self.id, to: id };
        self.to_village(&m);
        if let Some(v) = self.village.as_mut() {
            v.others.remove(&id);
        }
    }

    /// The relay's own host: an announcement (to everyone there, or `to`).
    pub fn notice(&mut self, to: u32, text: &str) {
        let m = Msg::Notice { id: self.id, to, text: text.chars().take(200).collect() };
        self.to_village(&m);
    }

    /// The relay's own host: who's kept from talking in its village (said
    /// with the village's next word).
    pub fn set_muted(&mut self, ids: Vec<u32>) {
        if let Some(v) = self.village.as_mut() {
            if v.muted != ids {
                v.muted = ids;
                self.next_village = 0.0;
            }
        }
    }

    /// The village's host, as heard (or this villager, hosting).
    #[cfg(test)]
    pub fn host_id(&self) -> Option<u32> {
        self.village.as_ref().and_then(|v| v.host_id)
    }

    /// Who's in the village (id, name).
    /// The relay's host: the connection a player id has come from (as the
    /// relay said), for its reports.
    pub fn conn_of(&self, id: u32) -> Option<u64> {
        self.owners.get(&id).copied()
    }

    pub fn players(&self) -> Vec<(u32, String)> {
        self.others().into_iter().map(|(id, o)| (id, o.name.clone())).collect()
    }

    /// Who's muted in the village.
    pub fn muted_here(&self) -> Vec<u32> {
        self.village.as_ref().map(|v| v.muted.clone()).unwrap_or_default()
    }

    /// A message of the relay's own (the host's reports): as it is.
    pub fn publish_raw(&mut self, topic: &str, payload: &[u8], retain: bool) {
        if self.link == Link::On {
            self.out.extend(mqtt::publish(topic, payload, retain));
        }
    }

    /// Out of the shared village (closing it, if hosting).
    pub fn leave_village(&mut self) {
        let Some(v) = self.village.take() else { return };
        let m = Msg::Bye { id: self.id, host: v.host };
        self.send(&v.topic, &v.seal, &m, false);
        // a listed village closing: off the list
        if v.host && v.square.is_none() && self.book.listed && self.link == Link::On {
            self.out.extend(mqtt::publish(&listed_topic(self.id), &[], true));
        }
        if self.link == Link::On {
            self.sub = self.sub.wrapping_add(1).max(1);
            self.out.extend(mqtt::unsubscribe(self.sub, &[v.topic.clone()]));
        }
        self.next_here = 0.0;
        if (!v.host || v.square.is_some()) && v.seed.is_some() {
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
        let to = seal::norm(&p.code);
        // (asked in already, and still waiting: that one will do)
        if self.book.outbox.iter().any(|o| o.to == to && o.kind == out::INVITE && o.text == code) {
            return true;
        }
        let n = self.queue(&to, out::INVITE, code.clone());
        self.keep_line(&to, ChatLine { at: now(), mine: true, text: "⌂ asked you into my village".into(), n, state: sent::WAITING, invite: code });
        self.flush(&to);
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

    pub fn answer(&mut self, to: u32, what: u8, yes: bool, seed: u32) {
        let m = Msg::Answer { id: self.id, to, what, yes, seed };
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

    /// A race: everyone's at the top; off at `at` (wall-clock ms).
    pub fn go(&mut self, seed: u32, at: i64) {
        let m = Msg::Go { id: self.id, seed, at };
        self.to_village(&m);
    }

    /// This side's music as it's playing (sent to the guests while
    /// hosting).
    pub fn set_tune(&mut self, t: Option<Tune>) {
        self.my_tune = t;
    }

    /// A guest: the host's music, newly heard (once), brought up to now.
    pub fn take_tune(&mut self) -> Option<Tune> {
        self.village.as_ref().filter(|v| !v.host)?;
        let mut m = self.host_tune.take()?;
        let now = crate::clock::now().timestamp_millis();
        m.t += ((now - m.at).clamp(0, 10_000)) as f32 / 1000.0;
        m.at = now;
        Some(m)
    }

    /// Something in the sky, for everyone in the village (see `sky`).
    pub fn sky(&mut self, what: u8, x: f32, y: f32, seed: u32) {
        let m = Msg::Sky { id: self.id, what, x, y, seed };
        self.to_village(&m);
    }

    /// Never mind what this villager asked (they've said yes to something
    /// else).
    pub fn never(&mut self, what: u8, seed: u32) {
        let m = Msg::Never { id: self.id, what, seed };
        self.to_village(&m);
    }

    /// Hosting the shared village (or not in one: the village is this
    /// villager's own).
    pub fn hosting(&self) -> bool {
        self.village.as_ref().is_none_or(|v| v.host)
    }

    /// A shooting star sent off here, for everyone in the village.
    pub fn star(&mut self, x: f32, y: f32) {
        let m = Msg::Star { id: self.id, x, y };
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
        /// The `i`th's gone (offline): nothing more reaches it.
        pub(crate) fn gone(&mut self, i: usize) {
            self.subs.retain(|s| s.0 != i);
        }

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

    fn run(relay: &mut Relay, all: &mut [&mut Online], t: &mut f32, secs: f32) {
        for _ in 0..(secs * 10.0) as usize {
            for o in all.iter_mut() {
                o.tick(*t, Some(Avatar::default()), 1);
            }
            relay.pump(all);
            *t += 0.1;
        }
    }

    /// Someone walking at an even pace, heard every 0.12 s but with the
    /// messages arriving unevenly (up to 150 ms late, in bunches): drawn
    /// at an even pace too, no hops, no stops.
    #[test]
    fn a_walk_heard_unevenly_is_drawn_evenly() {
        let mut tr = Trail::default();
        // (their clock runs 1000 s ahead of ours: only the differences count)
        let mut sent: Vec<(f32, f32)> = (0..60).map(|i| (1000.0 + i as f32 * 0.12, i as f32 * 0.12)).collect();
        // arrival times: a fixed 60 ms, plus a jitter that comes in bunches
        let arrive: Vec<f32> = sent.iter().enumerate().map(|(i, (at, _))| at - 1000.0 + 0.06 + [0.0, 0.15, 0.02, 0.12, 0.0, 0.09, 0.15, 0.01][i % 8]).collect();
        let mut next = 0;
        let (mut last, mut steps) = (None::<f32>, vec![]);
        let dt = 1.0 / 60.0;
        let mut now = 0.0f32;
        while now < 6.5 {
            while next < sent.len() && arrive[next] <= now {
                let (at, x) = sent[next];
                tr.heard(now, at, x, 0.0, 0.6);
                next += 1;
            }
            tr.tick(dt);
            if let Some((x, _)) = tr.at(now, tr.delay(), 0.3) {
                if let Some(l) = last {
                    steps.push(x - l);
                }
                last = Some(x);
            }
            now += dt;
        }
        // the walk's speed is 1 a second: each frame a sixtieth, give or take a little
        let walking: Vec<f32> = steps.iter().copied().skip(30).take(250).collect();
        let (lo, hi) = walking.iter().fold((f32::MAX, f32::MIN), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(lo > dt * 0.92 && hi < dt * 1.08, "uneven: {lo} .. {hi} a frame (want about {dt})");
        sent.clear();
    }

    /// The same walk heard over long polling through a slow network: the
    /// messages come in bursts, one a round trip, up to half a second late.
    /// The delay grows to cover it (no hops, no stops, once it has); on a
    /// good link it stays at the usual 0.2 s.
    #[test]
    fn a_walk_heard_in_long_polling_bursts_is_drawn_evenly() {
        let walk = |late: &dyn Fn(usize, f32) -> f32| -> (Vec<f32>, f32) {
            let mut tr = Trail::default();
            let sent: Vec<(f32, f32)> = (0..160).map(|i| (500.0 + i as f32 * 0.1, i as f32 * 0.1)).collect();
            let arrive: Vec<f32> = sent.iter().enumerate().map(|(i, (at, _))| at - 500.0 + late(i, at - 500.0)).collect();
            let (mut next, mut last, mut steps, mut now) = (0, None::<f32>, vec![], 0.0f32);
            let dt = 1.0 / 60.0;
            while now < 15.0 {
                while next < sent.len() && arrive[next] <= now {
                    tr.heard(now, sent[next].0, sent[next].1, 0.0, 0.6);
                    next += 1;
                }
                tr.tick(dt);
                if let Some((x, _)) = tr.at(now, tr.delay(), 0.3) {
                    if let Some(l) = last {
                        steps.push(x - l);
                    }
                    last = Some(x);
                }
                now += dt;
            }
            (steps, tr.delay())
        };
        let dt = 1.0 / 60.0;
        // bursts: everything sent waits for the next round trip (0.15 to
        // 0.45 s apart), then comes at once, 40 ms on
        let trips: Vec<f32> = {
            let mut v = vec![0.0f32];
            let gaps = [0.15, 0.3, 0.45, 0.2, 0.4, 0.25, 0.35];
            for i in 0..200 {
                let n = v[v.len() - 1] + gaps[i % gaps.len()];
                v.push(n);
            }
            v
        };
        let burst = |_: usize, sent: f32| -> f32 { trips.iter().find(|&&r| r >= sent).copied().unwrap_or(sent) - sent + 0.04 };
        let (steps, delay) = walk(&burst);
        // (once it's grown: after the first eight seconds)
        let later: Vec<f32> = steps.iter().copied().skip(8 * 60).take(6 * 60).collect();
        let (lo, hi) = later.iter().fold((f32::MAX, f32::MIN), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(lo > dt * 0.85 && hi < dt * 1.15, "uneven over long polling: {lo} .. {hi} a frame (want about {dt}), drawn {delay} s behind");
        assert!(delay > 0.4 && delay <= MAX_DELAY, "{delay}");
        // a good link: 60 ms, steady: the usual delay
        let (_, delay) = walk(&|_, _| 0.06);
        assert!((delay - TRAIL_DELAY).abs() < 1e-3, "{delay}");
    }

    /// A public square's own settings come from the village's own relay
    /// (believed only from it), and the host's clock reaches the guests.
    #[test]
    fn a_square_keeps_its_keepers_settings_and_the_hosts_clock() {
        let mut relay = Relay::default();
        let mk = |n: &str| {
            let mut o = Online::new(Book { on: true, ..Default::default() }, n);
            o.connected();
            o
        };
        let (mut a, mut b) = (mk("Ann"), mk("Bo"));
        let mut t = 1.0;
        run(&mut relay, &mut [&mut a, &mut b], &mut t, 0.5);
        a.join_square(0);
        run(&mut relay, &mut [&mut a, &mut b], &mut t, 6.0);
        b.join_square(0);
        a.set_clock(1300.0, 480.0, 960.0);
        // the host's music, for the guest to play the same
        let at = crate::clock::now().timestamp_millis();
        a.set_tune(Some(Tune { style: 1, carol: 2, seed: 77, t: 30.0, at }));
        run(&mut relay, &mut [&mut a, &mut b], &mut t, 3.0);
        assert_eq!(b.square(), Some(("North Pole Square".to_string(), false)));
        let m = b.take_tune().expect("no music heard from the host");
        assert_eq!((m.style, m.carol, m.seed), (1, 2, 77));
        assert!(m.t >= 30.0, "not brought up to now");
        assert!(a.take_tune().is_none(), "the host follows nobody");
        // the host's clock, moved on by the time since it was heard
        let (m, r, s) = b.village_clock().expect("the host's clock");
        assert!((1300.0..1301.0).contains(&m) && (r, s) == (480.0, 960.0), "{m} {r} {s}");
        assert!(a.village_clock().is_none(), "the host keeps its own");
        // the relay's console speaks: believed from the village's own relay…
        let said = br#"{"North Pole Square":{"time":"night","critters":0.5},"Nowhere":{"time":"noon"}}"#;
        let pkt = super::mqtt::publish(ADMIN_SQUARES, said, true);
        b.set_trusted(true);
        b.feed(&pkt);
        assert_eq!(b.square_rules().as_deref(), Some(r#"{"critters":0.5,"time":"night"}"#));
        // …not from any other
        a.set_trusted(false);
        a.feed(&pkt);
        assert_eq!(a.square_rules().as_deref(), Some("{}"));
        // outside a square: none
        b.leave_village();
        assert_eq!(b.square_rules(), None);
    }

    /// The public squares: the first in hosts; the next is a guest; the
    /// host going, the guest takes over. A village full turns newcomers
    /// away; a listed one shows in Public villages, with how many are in it.
    #[test]
    fn public_squares_hand_over_and_villages_fill_up() {
        let mut relay = Relay::default();
        let mk = |n: &str| {
            let mut o = Online::new(Book { on: true, ..Default::default() }, n);
            o.connected();
            o
        };
        let (mut a, mut b, mut c, mut d) = (mk("Ann"), mk("Bo"), mk("Cy"), mk("Di"));
        let mut t = 1.0;
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 0.5);
        assert_eq!(a.link(), Link::On);
        // into an empty square: a hosts it
        a.join_square(0);
        assert!(a.take_events().contains(&Event::Village(square_seed(0))), "the square's village, for the scene");
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 6.0);
        assert_eq!(a.square(), Some(("North Pole Square".to_string(), true)));
        // the next: a guest there
        b.join_square(0);
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 2.0);
        assert_eq!(b.square(), Some(("North Pole Square".to_string(), false)));
        assert_eq!(a.headcount(), Some((2, SQUARE_LIMIT)));
        assert_eq!(b.guest_seed(), Some(square_seed(0)));
        // the host goes: the square stays, and Bo hosts it
        a.leave_village();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 1.0);
        assert_eq!(b.square(), Some(("North Pole Square".to_string(), true)));
        assert!(b.village().is_some());
        // a village for two at most, listed: Ann in, Di turned away
        c.limit = 2;
        c.book.listed = true;
        let code = c.host();
        a.join(&code).unwrap();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 2.0);
        assert!(a.village().is_some_and(|(_, host)| !host), "Ann's in");
        d.join(&code).unwrap();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 2.0);
        assert!(d.village().is_none(), "Di's turned away: it's full");
        assert!(d.take_events().iter().any(|e| matches!(e, Event::Note(n) if n.contains("full"))));
        // Public villages, as Di sees it: Cy's village, two of two; the
        // square with Bo in it
        d.watch_public();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 16.0);
        let l = d.listings();
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!((l[0].name.as_str(), l[0].n, l[0].limit), ("Cy's village", 2, 2));
        assert_eq!(d.squares()[0], ("North Pole Square".to_string(), 1));
        // closed: off the list
        c.leave_village();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d], &mut t, 1.0);
        let mut e = mk("Ed");
        e.watch_public();
        run(&mut relay, &mut [&mut a, &mut b, &mut c, &mut d, &mut e], &mut t, 1.0);
        assert!(e.listings().is_empty(), "{:?}", e.listings());
    }

    /// Two villagers in one village through a real relay for a while
    /// (XMAS_SOAK seconds, 120 by default; XMAS_RELAY picks the relay):
    /// how often the link drops, how long each goes unseen by the other,
    /// how much each sends. Prints what it found; fails on a drop or on
    /// either going unseen for more than a few seconds.
    #[test]
    #[ignore]
    fn a_village_holds_together_through_the_real_relay() {
        let secs: f32 = std::env::var("XMAS_SOAK").ok().and_then(|v| v.parse().ok()).unwrap_or(120.0);
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Soak A");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Soak B");
        let (mut la, mut lb) = (None, None);
        let mut t = 0.0f32;
        let start = std::time::Instant::now();
        let run = |a: &mut Online, b: &mut Online, la: &mut Option<tcp::Tcp>, lb: &mut Option<tcp::Tcp>, t: f32| {
            tcp::pump(a, la);
            tcp::pump(b, lb);
            let av = |p: f32| Some(Avatar { x: 0.5 + 0.3 * (t * 0.4 + p).sin(), y: 0.8, dir: 1.0, moving: true, ..Default::default() });
            a.tick(t, av(0.0), 1);
            b.tick(t, av(1.5), 1);
            a.ease(0.05);
            b.ease(0.05);
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        while (a.link() != Link::On || b.link() != Link::On) && start.elapsed().as_secs() < 30 {
            run(&mut a, &mut b, &mut la, &mut lb, t);
            t += 0.05;
        }
        assert_eq!((a.link(), b.link()), (Link::On, Link::On), "couldn't reach a relay");
        let via = la.as_ref().map(|l| l.via()).unwrap_or_default();
        let code = a.host();
        b.join(&code).unwrap();
        // in together
        let t0 = t;
        while t - t0 < 20.0 && !(a.others().len() == 1 && b.others().len() == 1) {
            run(&mut a, &mut b, &mut la, &mut lb, t);
            t += 0.05;
        }
        let met = t - t0;
        assert!(a.others().len() == 1 && b.others().len() == 1, "they never met");
        // and then a while together
        let (mut drops, mut was) = (0, (true, true));
        let (mut worst_a, mut worst_b) = (0.0f32, 0.0f32);
        let (mut unseen_a, mut unseen_b) = (0.0f32, 0.0f32);
        let t1 = t;
        while t - t1 < secs {
            run(&mut a, &mut b, &mut la, &mut lb, t);
            t += 0.05;
            let now = (a.link() == Link::On, b.link() == Link::On);
            if (was.0 && !now.0) || (was.1 && !now.1) {
                drops += 1;
                eprintln!("  a link dropped at {:.0}s (A {:?}, B {:?})", t - t1, a.link(), b.link());
            }
            was = now;
            // how long since each heard the other
            let age = |o: &Online| o.others().first().map(|(_, x)| o.t - x.seen).unwrap_or(99.0);
            let (ga, gb) = (age(&a), age(&b));
            worst_a = worst_a.max(ga);
            worst_b = worst_b.max(gb);
            if ga > 2.0 {
                unseen_a += 0.05;
            }
            if gb > 2.0 {
                unseen_b += 0.05;
            }
        }
        eprintln!("soak via {via}: met after {met:.1}s; {secs:.0}s together: {drops} drop(s); longest unheard: A {worst_a:.1}s, B {worst_b:.1}s; unseen >2s: A {unseen_a:.1}s, B {unseen_b:.1}s");
        assert_eq!(drops, 0, "the link dropped");
        assert!(worst_a < 4.0 && worst_b < 4.0, "one went unheard for a while");
    }

    /// A public square through a real relay (needs the internet, or
    /// XMAS_RELAY): the first in hosts it, the next finds them there.
    /// With XMAS_SQUARE_HOSTED=1, someone else (the web page, say) is
    /// hosting North Pole Square already: only the second comes, and finds
    /// them.
    #[test]
    #[ignore]
    fn a_square_through_the_real_relay() {
        let hosted = std::env::var("XMAS_SQUARE_HOSTED").is_ok_and(|v| v == "1");
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Square A");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Square B");
        let (mut la, mut lb) = (None, None);
        let mut t = 0.0;
        let av = Some(Avatar { x: 1.0, ..Default::default() });
        let step = |a: &mut Online, b: &mut Online, la: &mut Option<tcp::Tcp>, lb: &mut Option<tcp::Tcp>, t: &mut f32| {
            if !hosted {
                tcp::pump(a, la);
                a.tick(*t, av, 1);
            }
            tcp::pump(b, lb);
            b.tick(*t, av, 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            *t += 0.05;
        };
        for _ in 0..600 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if (hosted || a.link() == Link::On) && b.link() == Link::On {
                break;
            }
        }
        assert_eq!(b.link(), Link::On, "couldn't reach a relay");
        if !hosted {
            a.join_square(0);
            for _ in 0..200 {
                step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            }
            assert_eq!(a.square(), Some(("North Pole Square".to_string(), true)), "the first in hosts it");
        }
        b.join_square(0);
        let mut met = false;
        for _ in 0..300 {
            step(&mut a, &mut b, &mut la, &mut lb, &mut t);
            if b.headcount().is_some_and(|(n, _)| n >= 2) && b.square().is_some_and(|(_, host)| !host) {
                met = true;
                break;
            }
        }
        assert!(met, "the second found the square's host: {:?} {:?}", b.square(), b.headcount());
        assert_eq!(b.headcount().map(|h| h.1), Some(SQUARE_LIMIT));
    }

    /// The real relay, its squares kept by its own host: a player joining
    /// North Pole Square is a guest there, not its host. Needs only the
    /// relay (`XMAS_RELAY=wss://xmas-relay.novalabs.app/mqtt cargo test
    /// --release live_squares -- --ignored`).
    #[test]
    #[ignore]
    fn live_squares_are_kept_by_the_relays_host() {
        std::env::var("XMAS_RELAY").expect("XMAS_RELAY (the relay, e.g. wss://xmas-relay.novalabs.app/mqtt)");
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Check");
        let mut la = None;
        let av = Some(Avatar { x: 1.0, ..Default::default() });
        let mut t = 0.0;
        let mut step = |a: &mut Online, n: usize| {
            for _ in 0..n {
                tcp::pump(a, &mut la);
                a.tick(t, av, 1);
                a.take_events();
                std::thread::sleep(std::time::Duration::from_millis(50));
                t += 0.05;
            }
        };
        step(&mut a, 100);
        assert_eq!(a.link(), Link::On, "couldn't reach the relay");
        a.join_square(0);
        step(&mut a, 160);
        assert_eq!(a.square().map(|s| s.1), Some(false), "hosting the square itself (the relay's host isn't there?)");
        assert!(a.host_id().is_some_and(|h| h < HOST_IDS), "the square's host isn't the relay's: {:?}", a.host_id());
        a.leave_village();
        step(&mut a, 10);
    }

    /// The relay's own host keeping the squares (needs a relay and a host
    /// running: XMAS_RELAY, and XMAS_ADMIN for the relay's console, opened
    /// with ADMIN_OPEN=1): players come in as its guests, never hosting;
    /// the console's announcement reaches them, a mute keeps one quiet, a
    /// kick sends one home; a server village opens by code and, banned, is
    /// closed; a banned player is let go and kept out.
    #[test]
    #[ignore]
    fn a_square_kept_by_the_relays_own_host() {
        let admin = std::env::var("XMAS_ADMIN").expect("XMAS_ADMIN (the console, e.g. http://127.0.0.1:18981)");
        let post = |path: &str, body: serde_json::Value| -> u16 {
            use std::io::{Read, Write};
            let u = super::ws::parse(&admin.replace("http://", "ws://")).unwrap();
            let mut s = std::net::TcpStream::connect((u.host.as_str(), u.port)).unwrap();
            let b = body.to_string();
            write!(s, "POST {path} HTTP/1.1\r\nHost: localhost\r\ncontent-type: application/json\r\nx-relay-admin: 1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}", b.len()).unwrap();
            let mut r = String::new();
            let _ = s.read_to_string(&mut r);
            r.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
        };
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Ann");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Bo");
        let (mut la, mut lb) = (None, None);
        let mut t = 0.0;
        let av = Some(Avatar { x: 1.0, ..Default::default() });
        let mut evs: (Vec<Event>, Vec<Event>) = (vec![], vec![]);
        let mut step = |a: &mut Online, b: &mut Online, n: usize, evs: &mut (Vec<Event>, Vec<Event>)| {
            for _ in 0..n {
                tcp::pump(a, &mut la);
                a.tick(t, av, 1);
                tcp::pump(b, &mut lb);
                b.tick(t, av, 1);
                evs.0.extend(a.take_events());
                evs.1.extend(b.take_events());
                std::thread::sleep(std::time::Duration::from_millis(50));
                t += 0.05;
            }
        };
        step(&mut a, &mut b, 60, &mut evs);
        assert_eq!((a.link(), b.link()), (Link::On, Link::On), "couldn't reach the relay");
        a.join_square(0);
        b.join_square(0);
        step(&mut a, &mut b, 120, &mut evs);
        for (who, o) in [("Ann", &a), ("Bo", &b)] {
            assert_eq!(o.square().map(|s| s.1), Some(false), "{who} is hosting (the relay's host isn't there?)");
            assert!(o.host_id().is_some_and(|h| h < HOST_IDS), "{who}'s host isn't the relay's: {:?}", o.host_id());
        }
        assert!(a.other(b.my_id()).is_some(), "Ann doesn't see Bo");
        // an announcement
        assert_eq!(post("/api/say", serde_json::json!({ "to": "North Pole Square", "text": "hello, squares" })), 204);
        step(&mut a, &mut b, 40, &mut evs);
        for (who, e) in [("Ann", &evs.0), ("Bo", &evs.1)] {
            assert!(e.contains(&Event::Notice("hello, squares".into())), "{who} didn't hear it: {e:?}");
        }
        // a mute: Bo's not heard
        assert_eq!(post("/api/moderate", serde_json::json!({ "slot": "North Pole Square", "id": b.my_id(), "action": "mute" })), 204);
        step(&mut a, &mut b, 60, &mut evs);
        evs.0.clear();
        b.say("can anyone hear me");
        step(&mut a, &mut b, 30, &mut evs);
        assert!(!evs.0.iter().any(|e| matches!(e, Event::Said { .. })), "Bo was heard though muted");
        assert_eq!(post("/api/moderate", serde_json::json!({ "slot": "North Pole Square", "id": b.my_id(), "action": "unmute" })), 204);
        step(&mut a, &mut b, 60, &mut evs);
        b.say("and now?");
        step(&mut a, &mut b, 30, &mut evs);
        assert!(evs.0.iter().any(|e| matches!(e, Event::Said { text, .. } if text == "and now?")), "Bo still muted");
        // a kick: Bo's sent home
        assert_eq!(post("/api/moderate", serde_json::json!({ "slot": "North Pole Square", "id": b.my_id(), "action": "kick" })), 204);
        step(&mut a, &mut b, 40, &mut evs);
        assert!(b.village().is_none(), "Bo wasn't sent home");
        // a server village, private: in by its code
        let code = "TESTVLGE2345";
        assert_eq!(post("/api/servers", serde_json::json!([{ "name": "Test Village", "code": code, "public": false, "limit": 6, "seed": 77, "rules": {} }])), 204);
        step(&mut a, &mut b, 60, &mut evs);
        b.join(code).unwrap();
        step(&mut a, &mut b, 80, &mut evs);
        assert!(b.host_id().is_some_and(|h| h < HOST_IDS), "no host in the server village: {:?}", b.host_id());
        // banned: closed, and not to be joined again
        assert_eq!(post("/api/ban-village", serde_json::json!({ "code": code, "name": "Test Village", "why": "a test" })), 204);
        step(&mut a, &mut b, 40, &mut evs);
        assert!(b.public_rules.banned.contains(&code.to_string()), "the ban didn't reach the game");
        b.leave_village();
        assert!(b.join(code).is_err());
        assert_eq!(post("/api/unban-village", serde_json::json!({ "code": code })), 204);
        assert_eq!(post("/api/servers", serde_json::json!([])), 204);
        // a banned player: let go, and kept out on the way back
        assert_eq!(post("/api/ban-player", serde_json::json!({ "id": a.my_id(), "name": "Ann", "why": "a test" })), 204);
        step(&mut a, &mut b, 120, &mut evs);
        assert_ne!(a.link(), Link::On, "Ann's still connected though banned");
        assert_eq!(post("/api/unban-player", serde_json::json!({ "id": a.my_id() })), 204);
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
        a.answer(b.my_id(), ask::CHESS, true, 0);
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
    /// A relay that takes the connection but never answers the hello, or
    /// turns it down: left for the next one (and not before it's had a
    /// fair while).
    #[test]
    fn a_relay_that_goes_nowhere_is_left() {
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        a.tick(1.0, None, 1);
        a.connected();
        a.tick(8.0, None, 1);
        assert!(!a.link_stuck(), "still waiting for an answer");
        a.tick(12.0, None, 1);
        assert!(a.link_stuck(), "no answer in ten seconds");
        a.disconnected();
        assert!(!a.link_stuck());
        assert_eq!(a.skip_relay(), 1);
        // an answer: fine
        a.connected();
        a.feed(&[0x20, 2, 0, 0]);
        a.tick(30.0, None, 1);
        assert!(a.link() == Link::On && !a.link_stuck());
        // turned down: left at once
        a.disconnected();
        a.connected();
        a.feed(&[0x20, 2, 0, 5]);
        assert!(a.link_stuck());
    }

    /// The game says what version it is when it signs in, and a relay that
    /// turns it away as too old gets a word to update, not another relay:
    /// it stays off for half an hour (until updated, really), then tries
    /// again.
    #[test]
    fn too_old_for_the_relay_waits_for_an_update() {
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        a.tick(1.0, None, 1);
        a.connected();
        let hello = a.take_out();
        let me = agent();
        assert!(me.starts_with(&format!("xmas-tui/{} (", env!("CARGO_PKG_VERSION"))), "{me}");
        assert!(hello.windows(me.len()).any(|w| w == me.as_bytes()), "the hello doesn't say the version");
        a.take_events();
        a.feed(&[0x20, 2, 0, 1]);
        assert!(a.too_old() && a.link_stuck());
        assert!(a.take_events().iter().any(|e| matches!(e, Event::Note(n) if n.contains("too old"))), "nothing said");
        a.disconnected();
        a.tick(1000.0, None, 1);
        assert!(a.too_old(), "tried again too soon");
        a.tick(1900.0, None, 1);
        assert!(!a.too_old(), "never tries again");
        // a relay older than the fixes: said once
        a.set_trusted(true);
        a.connected();
        a.feed(&[0x20, 2, 0, 0]);
        a.take_events();
        let rules = serde_json::json!({ "featured": [], "order": [], "banned": [], "words": [], "relay": "1.5.0", "min_app": "1.25.4" });
        let p = mqtt::publish(ADMIN_PUBLIC, &serde_json::to_vec(&rules).unwrap(), true);
        a.feed(&p);
        a.feed(&p);
        let said = a.take_events().iter().filter(|e| matches!(e, Event::Note(n) if n.contains("out of date"))).count();
        assert_eq!(said, 1);
        assert_eq!(version_num("1.25.4"), Some((1, 25, 4)));
    }

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
        assert!(ev.contains(&Event::Mail { from: "Robin".into(), text: "hi[2Jthere".into(), links: vec![] }), "{ev:?}");
        // a group: made, joined by its code, talked in
        let code = a.new_group("carols");
        b.join_group("", &code).unwrap();
        assert!(b.join_group("", &code).is_err());
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.group_say(0, "anyone about?"));
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::GroupSaid { group: "carols".into(), name: "Sam".into(), text: "anyone about?".into(), links: vec![] }));
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
        assert!(b.take_events().contains(&Event::Whispered { id: ia, name: "Robin".into(), text: "psst".into(), links: vec![] }));
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
        b.answer(ia, ask::CHESS, true, 0);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().iter().any(|e| matches!(e, Event::Answer { id, what: ask::CHESS, yes: true, .. } if *id == ib)));
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
        assert!(a.take_events().contains(&Event::Mail { from: "Sam".into(), text: "snowball fight at the tree?".into(), links: vec![] }));
        assert_eq!(a.unread(0), 1);
        a.mark_read(0);
        assert_eq!(a.unread(0), 0);
        // renamed here, and quiet: kept in the chat (and new), nothing else
        a.book.friends[0].name = "Sammy".into();
        a.book.friends[0].quiet = true;
        assert!(b.mail(0, "you there?"));
        assert!(b.ping(0));
        relay.pump(&mut [&mut a, &mut b]);
        let ev = a.take_events();
        assert!(!ev.iter().any(|e| matches!(e, Event::Mail { .. } | Event::Pinged(_))), "{ev:?}");
        assert_eq!(a.unread(0), 1);
        assert_eq!(a.chat(0).last().map(|l| l.text.as_str()), Some("you there?"));
        a.book.friends[0].quiet = false;
        assert!(b.mail(0, "hello?"));
        relay.pump(&mut [&mut a, &mut b]);
        assert!(a.take_events().contains(&Event::Mail { from: "Sammy".into(), text: "hello?".into(), links: vec![] }), "by the name it has here");
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

    /// Written while offline (or while they are): it waits, shown so, and
    /// goes when both are online; once there, it's said so, and it's never
    /// shown twice. An invite shows in the chat too, with the way in.
    #[test]
    fn written_while_offline_goes_when_both_are_on() {
        let mut relay = Relay::default();
        let mut a = Online::new(Book { on: true, ..Default::default() }, "Robin");
        let mut b = Online::new(Book { on: true, ..Default::default() }, "Sam");
        // Robin, offline, asks Sam: waiting
        let code_b = b.code();
        a.add_friend(&code_b).unwrap();
        assert_eq!(a.waiting_for(0), (true, 0));
        // Robin comes online; Sam's still off: it hasn't reached Sam
        a.connected();
        relay.pump(&mut [&mut a, &mut b]);
        a.tick(1.0, None, 7);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(b.book.asking.is_empty());
        assert_eq!(a.waiting_for(0), (true, 0));
        // Sam comes on: Robin sees, sends it again, Sam has it and says so
        b.connected();
        relay.pump(&mut [&mut a, &mut b]);
        b.tick(1.0, None, 7);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(b.book.asking.len(), 1, "the request came");
        assert_eq!(a.waiting_for(0), (false, 0), "and Robin knows it did");
        b.accept(0);
        relay.pump(&mut [&mut a, &mut b]);
        assert!(!a.book.friends[0].waiting);
        assert!(b.book.outbox.is_empty(), "the yes came");
        // Sam goes off (the relay says so: "gone"); Robin writes: waiting,
        // shown so
        b.go_online(false);
        relay.pump(&mut [&mut a, &mut b]);
        relay.gone(1);
        b.disconnected();
        assert!(!a.friends()[0].1.as_ref().is_some_and(|h| h.on));
        assert!(a.mail(0, "see you at the tree"));
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(a.waiting_for(0), (false, 1));
        assert_eq!(a.chat(0).last().map(|l| l.state), Some(sent::WAITING));
        // back on: it comes, once, and Robin's line says it's there
        b.go_online(true);
        b.connected();
        relay.pump(&mut [&mut a, &mut b]);
        b.tick(400.0, None, 7);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(b.chat(0).iter().filter(|l| l.text == "see you at the tree").count(), 1);
        assert_eq!(a.chat(0).last().map(|l| l.state), Some(sent::THERE));
        assert_eq!(a.waiting_for(0), (false, 0));
        // the same again (its "came" lost): not shown twice
        let again = Outgoing { to: seal::norm(&code_b), n: a.book.next_n, kind: out::MAIL, text: "see you at the tree".into(), ..Default::default() };
        a.book.outbox.push(again);
        a.tick(10.0, None, 7);
        relay.pump(&mut [&mut a, &mut b]);
        assert_eq!(b.chat(0).iter().filter(|l| l.text == "see you at the tree").count(), 1);
        assert!(a.book.outbox.is_empty());
        // an invite: in Sam's chat, with the village's code
        let vcode = a.host();
        a.tick(12.0, Some(Avatar::default()), 4242);
        assert!(a.invite(0));
        relay.pump(&mut [&mut a, &mut b]);
        let l = b.chat(0).last().cloned().unwrap();
        assert_eq!(seal::norm(&l.invite), seal::norm(&vcode));
        assert!(b.take_events().iter().any(|e| matches!(e, Event::Invited { .. })));
        assert!(a.chat(0).last().is_some_and(|l| l.mine && !l.invite.is_empty()));
    }
}
