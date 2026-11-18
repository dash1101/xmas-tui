//! The village online: friends (by friend code), messages, and shared
//! villages, where other players' villagers walk about, talk aloud and throw
//! snowballs, and play together: games at a table, tag, snowball fights,
//! light shows and fireworks everyone sees, the train and the slopes
//! together, races down the mountain. And the villager's profile card (what
//! friends see of them), devices linked to share one villager, and other
//! villagers kept on this device. The conversation with the relay is
//! `crate::net`; this is what the village does with it: the menus, the
//! post (the last few things that happened, top left), the others drawn
//! with their names over them, the questions asked of the villager.

use super::*;
use crate::net::{self, ask, Avatar, Card, Event, Link};

/// What can be done online (the friends & online menu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetDo {
    On,
    Off,
    /// Copy this villager's friend code (or, in an open village, its code).
    Code,
    VillageCode,
    Add,
    Mail(u8),
    Join(u8),
    Unfriend(u8),
    Accept(u8),
    Decline(u8),
    Host,
    Close,
    JoinCode,
    Leave,
    Say,
    /// Ask friend i into this (open) village.
    Invite(u8),
    /// Into the village friend i asked this villager into.
    GoInvited(u8),
    /// Ask someone here (their id) to play (see `net::ask`).
    With(u32, u8),
    /// A card game round the table: everyone here asked to sit in.
    WithAll(u8),
    /// Wave (0), cheer (1), laugh (2), a heart (3) at someone (0: all).
    Wave(u32, u8),
    /// Yes (or no thanks) to what was asked (which of them).
    Yes(u8),
    No(u8),
    /// Along to where someone is on their trip.
    JoinTrip(u32),
    /// A race down the mountain: everyone here asked along.
    Race,
    /// Profile cards: someone here's, a friend's, this villager's own.
    Card(u32),
    FriendCard(u8),
    MyCard,
    /// The line on this villager's card.
    Motto,
    /// Linked devices: the link code (to type on another device), a code
    /// typed from another, no longer linked.
    LinkShow,
    LinkUse,
    Unlink,
    /// Villagers kept on this device: switch to one, a new one.
    Villager(u8),
    NewVillager,
    /// A ping to friend i (a little tone at their end).
    Ping(u8),
    /// The talk line (as the / key): to everyone here, or "@name …".
    Chat,
    /// Groups: a new one, into one by its code, say something in one, its
    /// code copied, leave it.
    GroupNew,
    GroupJoin,
    GroupSay(u8),
    GroupCode(u8),
    GroupLeave(u8),
    /// Hosting: let someone here change the village's settings (or not).
    Admin(u32, bool),
    /// The messages, all of them (marked read).
    Messages,
    /// Friend i: a name of my own for them (asked for).
    Rename(u8),
    /// Friend i: quiet, or not.
    Quiet(u8),
    /// Friend i: the chat with them, cleared.
    ClearChat(u8),
    /// Friend i: their chat pinned over the village.
    PinFriend(u8),
    /// Friends & chats (the window).
    Friends,
    /// Friends & chats, with friend i picked.
    ChatWith(u8),
    /// Something said just to someone here (their id).
    Whisper(u32),
    /// The link(s) hidden in a message (its place in the messages, newest
    /// first): shown, and copied.
    ShowLink(u8),
    /// Public villages: into a square, into a listed village (its place in
    /// the list), this villager's own open village listed for anyone (or not).
    Square(u8),
    Listed(u8),
    ListMine(bool),
}

/// What a line of typing is for (the front end asks for it, see
/// `take_text_ask`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextFor {
    Add,
    Mail(u8),
    JoinCode,
    Say,
    Motto,
    /// A name for friend i (just here).
    Rename(u8),
    /// The link code from another device.
    Link,
    /// A new villager's name.
    NewVillager,
    /// The talk line: to everyone here, "@name …" to one person (a friend
    /// anywhere, or someone here), "@group …" to a group.
    Chat,
    /// A group: a new one's name; a code to join; something to say in one.
    GroupNew,
    GroupJoin,
    GroupSay(u8),
    /// Said just to someone here (their id).
    Whisper(u32),
}

/// Things only the front end can do (it holds the save and the config).
#[derive(Clone, Debug, PartialEq)]
pub enum Front {
    /// The line on the profile card, typed.
    Motto(String),
    /// Show (and copy) the link code for another device (making one).
    LinkShow,
    /// Link to another device by its code (this one becomes its villager).
    Link(String),
    Unlink,
    /// Switch to villager i (of `villagers`); a new one with this name.
    Villager(usize),
    NewVillager(String),
    /// A friend unfriended (their code; linked devices drop them too).
    Unfriended(String),
    /// Hosting: one of the village's settings changed by a guest (asked,
    /// and said yes to; or an admin's): its key and value (see `Rules`).
    SetRule(String, String),
    /// The shared village closed (or left) by hand: linked devices open
    /// at once stay apart from now on.
    Apart,
}

/// A village's own settings: the host's, and what everyone in the village
/// sees (a guest's own are kept for their own village). A guest's change is
/// asked of the host instead (or made, by an admin).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Rules {
    pub snow: f32,
    pub wind: f32,
    pub glow: f32,
    pub events: f32,
    pub critters: f32,
    pub most_at_once: u32,
    pub sleigh_minutes: f32,
    pub aurora: bool,
    pub bushes: bool,
    pub time: String,
    pub traffic: String,
    pub chattiness: f32,
}

impl Default for Rules {
    fn default() -> Self {
        Rules::of(&crate::config::SceneCfg::default())
    }
}

/// Whose a village's settings are, when they aren't this villager's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lock {
    /// A friend's village: a change is asked of the host.
    Host,
    /// A friend's village, and the host lets this villager change them.
    Admin,
    /// A public square: the village's own settings are its keeper's (from
    /// the relay's console), the same for everyone in it; the rest (snow,
    /// wind, glow, the aurora) stay this villager's.
    Keeper,
}

impl Rules {
    /// The keeper's settings onto these.
    pub fn take_owned(&mut self, k: &Rules) {
        self.time = k.time.clone();
        self.critters = k.critters;
        self.most_at_once = k.most_at_once;
        self.events = k.events;
        self.sleigh_minutes = k.sleigh_minutes;
        self.chattiness = k.chattiness;
        self.bushes = k.bushes;
        self.traffic = k.traffic.clone();
    }

    pub fn of(c: &crate::config::SceneCfg) -> Rules {
        Rules {
            snow: c.snow,
            wind: c.wind,
            glow: c.glow,
            events: c.events,
            critters: c.critters,
            most_at_once: c.most_at_once,
            sleigh_minutes: c.sleigh_minutes,
            aurora: c.aurora,
            bushes: c.bushes,
            time: c.time.clone(),
            traffic: c.traffic.clone(),
            chattiness: c.chattiness,
        }
    }

    pub fn apply(&self, c: &mut crate::config::SceneCfg) {
        let r = self.clone().clamped();
        c.snow = r.snow;
        c.wind = r.wind;
        c.glow = r.glow;
        c.events = r.events;
        c.critters = r.critters;
        c.most_at_once = r.most_at_once;
        c.sleigh_minutes = r.sleigh_minutes;
        c.aurora = r.aurora;
        c.bushes = r.bushes;
        c.time = r.time;
        c.traffic = r.traffic;
        c.chattiness = r.chattiness;
    }

    /// Kept to what the settings themselves allow (it came from someone
    /// else).
    pub fn clamped(mut self) -> Rules {
        let f = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 1.0 };
        self.snow = f(self.snow, 5.0);
        self.wind = f(self.wind, 5.0);
        self.glow = f(self.glow, 3.0);
        self.events = f(self.events, 5.0);
        self.critters = f(self.critters, 5.0);
        self.chattiness = f(self.chattiness, 5.0);
        self.sleigh_minutes = f(self.sleigh_minutes, 240.0);
        self.most_at_once = self.most_at_once.min(80);
        if !["auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"].contains(&self.time.as_str()) {
            self.time = "auto".into();
        }
        if !["auto", "left", "right"].contains(&self.traffic.as_str()) {
            self.traffic = "auto".into();
        }
        self
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn parse(s: &str) -> Option<Rules> {
        serde_json::from_str::<Rules>(s).ok().map(Rules::clamped)
    }

    /// What's different in `new`: (key, value) each.
    pub fn changes(&self, new: &Rules) -> Vec<(String, String)> {
        let (a, b) = (serde_json::to_value(self).unwrap_or_default(), serde_json::to_value(new).unwrap_or_default());
        let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else { return vec![] };
        b.iter().filter(|(k, v)| a.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.to_string())).collect()
    }

    /// One setting set from a key and its value (as `changes` gives them);
    /// false if there's no such setting or the value doesn't fit it.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let Ok(mut all) = serde_json::to_value(&*self) else { return false };
        let Some(old) = all.get(key).cloned() else { return false };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(value) else { return false };
        let same_kind = matches!((&old, &v), (serde_json::Value::Number(_), serde_json::Value::Number(_)) | (serde_json::Value::Bool(_), serde_json::Value::Bool(_)) | (serde_json::Value::String(_), serde_json::Value::String(_)));
        if !same_kind {
            return false;
        }
        all[key] = v;
        match serde_json::from_value::<Rules>(all) {
            Ok(r) => {
                *self = r.clamped();
                true
            }
            Err(_) => false,
        }
    }

    /// A setting in words: "snowfall → 2.00×".
    pub fn words(key: &str, value: &str) -> String {
        let v = value.trim_matches('"');
        let num = v.parse::<f32>().ok();
        match (key, num) {
            ("snow", Some(n)) => format!("snowfall → {n:.2}×"),
            ("wind", Some(n)) => format!("wind → {n:.2}×"),
            ("glow", Some(n)) => format!("glow → {n:.2}×"),
            ("events", Some(n)) => format!("surprises → {n:.2}×"),
            ("critters", Some(n)) => format!("animals & people → {n:.2}×"),
            ("chattiness", Some(n)) => format!("chattiness → {n:.2}×"),
            ("most_at_once", Some(n)) => format!("most at once → {n:.0}"),
            ("sleigh_minutes", Some(n)) if n <= 0.0 => "Santa's sleigh → never".to_string(),
            ("sleigh_minutes", Some(n)) => format!("Santa's sleigh → every {n:.0} min"),
            ("aurora", _) => format!("aurora → {}", if v == "true" { "on" } else { "off" }),
            ("bushes", _) => format!("berry bushes → {}", if v == "true" { "on" } else { "off" }),
            ("time", _) => format!("time of day → {v}"),
            ("traffic", _) => format!("traffic → {v}"),
            _ => format!("{key} → {v}"),
        }
    }
}

/// What the friends widget shows (from the village's online side): each
/// friend (name; 0 offline, 1 online, 2 with a village open, 3 asked and
/// waiting; their place in the friends list), unread messages, the last
/// few lines.
#[derive(Clone, Debug, Default)]
pub struct FriendsView {
    /// Going online at all (off: the widget offers to).
    pub online: bool,
    pub connected: bool,
    pub friends: Vec<(String, u8, usize)>,
    pub unread: u32,
    pub lines: Vec<String>,
    /// In a shared village: how many here.
    pub here: usize,
}

/// A line in the messages: when (unix seconds), and what.
#[derive(Clone, Debug)]
pub struct Line {
    pub at: i64,
    pub text: String,
    /// Links taken out of it (shown only when asked for).
    pub links: Vec<String>,
}

/// The more-menu numbers for the online menus (see `more_menu`).
pub const MORE_ONLINE: u8 = 10;
pub const MORE_PEOPLE: u8 = 11;
pub const MORE_DEVICES: u8 = 12;
pub const MORE_VILLAGERS: u8 = 13;
pub const MORE_FRIENDS: u8 = 14;
pub const MORE_GROUPS: u8 = 15;
pub const MORE_GROUP: u8 = 16;
pub const MORE_MESSAGES: u8 = 29;
pub const MORE_PUBLIC: u8 = 30;
pub const MORE_FRIEND: u8 = 32;
pub const MORE_OTHER: u8 = 64;

/// How long a line stays in the post.
const POST_TIME: f32 = 12.0;
/// How long a question asked stays up (s).
const ASK_TIME: f32 = 40.0;
/// Tag, a snowball fight with a friend: how long (s).
const TAG_TIME: f32 = 90.0;
const FIGHT_TIME: f32 = 90.0;
/// A race starts at the top this long after it's called (s).
const RACE_WAIT: f32 = 14.0;
/// A "hit": the snowball fight's word for "got me" (a wave's `what`).
const HIT: u8 = 9;

/// Something asked of the villager by someone in the shared village.
#[derive(Clone, Debug)]
pub struct Asked {
    pub id: u32,
    pub name: String,
    pub what: u8,
    pub seed: u32,
    pub until: f32,
    /// Asked of everyone (0) or just this villager.
    pub to: u32,
    /// A village setting asked to be changed: (key, value).
    pub rule: Option<(String, String)>,
}

/// Tag with a friend online: the other player, who's "it" (an id; this
/// villager's own is `Online::my_id`), no tagging back until `grace`, the
/// end, the tags made.
#[derive(Clone, Copy, Debug)]
pub struct NetTag {
    pub other: u32,
    pub it: u32,
    pub grace: f32,
    pub until: f32,
    pub tags: u32,
}

/// A snowball fight with a friend: who, until when, hits each way.
#[derive(Clone, Copy, Debug)]
pub struct NetFight {
    pub other: u32,
    pub until: f32,
    pub mine: u32,
    pub theirs: u32,
}

/// The villager's side of things online: their look and card, what's been
/// asked of them (and what they've asked), games with friends, the card
/// being looked at, what the front end's been asked to do.
#[derive(Default)]
pub struct Social {
    /// Paused (the pause button, or the wallpaper resting while a game
    /// runs): the village stands still, online goes on (`rest_tick`), and
    /// the others see this villager greyed.
    pub resting: bool,
    /// Friends & chats (the window).
    pub chats: super::chats::Chats,
    /// A race this villager asked for: who said yes, and whether it's gone.
    pub race: Option<RaceHost>,
    /// The others last seen up in the air (or not): a jump or a landing
    /// puffs the snow.
    pub others_air: std::collections::HashMap<u32, bool>,
    /// (Tests, whose online side keeps to the village's clock: the seconds
    /// spent resting, which that clock doesn't count.)
    pub rested: f32,
    /// The online side's own clock: real seconds since it started (the
    /// village's clock is held back while it's out of sight or resting;
    /// pings and who's been heard from mustn't be), from the wall clock at
    /// the start, and the last reading (it never goes back).
    #[cfg_attr(test, allow(dead_code))]
    pub clock: Option<(i64, f32)>,
    /// Public villages looked at (or about to be): listen for how they are.
    pub want_public: std::cell::Cell<bool>,
    pub wear: crate::config::FriendCfg,
    pub motto: String,
    pub asked: Vec<Asked>,
    /// Asked someone (their id, what, the seed, until when), and when it's
    /// asked again (no answer yet: a message can go astray).
    pub asking: Option<(u32, u8, u32, f32)>,
    asked_again: f32,
    /// Said yes to someone (their id, what): said again a few times, until.
    yes_again: Vec<(u32, u8, f32, f32)>,
    pub tag: Option<NetTag>,
    pub fight: Option<NetFight>,
    /// Friends asking the villager into their villages (name, code).
    pub invited: Vec<(String, String)>,
    /// The card on show (whose; the friend code, for this villager's own).
    pub card: Option<(Card, Option<String>)>,
    /// Carols being sung by someone here (their id, until).
    pub carols: Vec<(u32, f32, f32)>,
    /// Linked devices' sides of the villager, for the front end.
    pub synced: Vec<(String, i64, String)>,
    pub front: Vec<Front>,
    /// What the front end says of linked devices: linked, this one's name,
    /// the others (name, seconds since heard from).
    pub devices: Option<(String, Vec<(String, i64)>)>,
    /// Other villagers kept on this device (their names).
    pub villagers: Vec<String>,
    /// In a games room (its table's number), and the chair to sit in (None:
    /// sat, or left to the villager); when the table comes up.
    pub room: Option<u32>,
    pub to_seat: bool,
    pub panel_at: f32,
    /// Shown once: online's there to be found.
    tipped: bool,
    /// The post goes top left (a phone, the web page: no widgets there,
    /// and the joystick's bottom left); else bottom left.
    pub post_top: bool,
    next_card: f32,
    /// Where the question box and the post were drawn (for clicks).
    prompt_at: std::cell::Cell<Option<(i32, i32, i32, i32, i32)>>,
    post_at: std::cell::Cell<Option<(i32, i32, i32, i32)>>,
    /// The messages: said here, whispered, mailed, in groups, pings (the
    /// last few dozen), and how many came in unread.
    pub log: Vec<Line>,
    pub unread: u32,
    /// A guest: the host's village settings, and whether this villager may
    /// change them (an admin).
    pub rules_in: Option<(Rules, bool)>,
    /// Hosting: what was last told the guests.
    rules_out: Option<Rules>,
    /// Hosting: when who's out is next told the guests; a guest: when it
    /// was last heard.
    crowd_at: f32,
}

/// A race asked for here: its seed, who's in it, and gone (said go).
pub struct RaceHost {
    pub seed: u32,
    pub racers: Vec<u32>,
    pub went: bool,
}

/// Who's out in the village and what they've said, as its host sends it.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct Crowd {
    folk: Vec<critters::Pup>,
    said: Vec<(u64, String, f32)>,
    /// When the host sent it (its online clock, seconds; 0: an older
    /// version): guests draw everyone a moment behind, between two of these.
    at: f32,
}

/// What's asked, in words ("a game of chess").
pub fn ask_words(what: u8) -> &'static str {
    match what {
        ask::CHESS => "a game of chess",
        ask::CHECKERS => "a game of checkers",
        ask::SNOWFLAKES => "a game of Snowflakes",
        ask::TWENTY_ONE => "a hand of Twenty-One",
        ask::TAG => "a game of tag",
        ask::RACE => "a race down the mountain",
        ask::TRIP => "the train up to the slopes",
        ask::FIGHT => "a snowball fight",
        _ => "something",
    }
}

fn ask_icon(what: u8) -> char {
    match what {
        ask::CHESS => '♞',
        ask::CHECKERS => '◉',
        ask::SNOWFLAKES => '❄',
        ask::TWENTY_ONE => '♠',
        ask::TAG => '⚑',
        ask::RACE => '⇘',
        ask::TRIP => '⇄',
        ask::FIGHT => '●',
        _ => '✦',
    }
}

impl Scene {
    /// The online side, to carry over into a new village (another seed).
    /// The villager's own village's seed (this one being someone else's).
    pub fn set_own_seed(&mut self, seed: u32) {
        self.own_seed = seed;
    }

    pub fn take_online(&mut self) -> net::Online {
        std::mem::replace(&mut self.online, net::Online::new(net::Book::default(), ""))
    }

    pub fn put_online(&mut self, o: net::Online) {
        self.online = o;
    }

    /// The villager's look and the line on their card (Settings → You).
    pub fn set_wear(&mut self, wear: &crate::config::FriendCfg, motto: &str) {
        if &self.social.wear != wear {
            self.social.wear = wear.clone();
            player::set_wear(wear);
        }
        if self.social.motto != motto {
            self.social.motto = motto.to_string();
        }
    }

    /// The social side, carried into a new village (the look, the devices,
    /// the villagers, what's been asked).
    pub fn take_social(&mut self) -> Social {
        std::mem::take(&mut self.social)
    }

    pub fn put_social(&mut self, s: Social) {
        self.social = s;
        // (a game, a race or tag doesn't carry over into another village)
        self.social.tag = None;
        self.social.fight = None;
        self.social.asking = None;
        self.social.asked.clear();
        self.social.carols.clear();
    }

    /// Things for the front end to do.
    pub fn take_front(&mut self) -> Vec<Front> {
        std::mem::take(&mut self.social.front)
    }

    /// Linked devices' sides of the villager (device, when, its state).
    pub fn take_synced(&mut self) -> Vec<(String, i64, String)> {
        std::mem::take(&mut self.social.synced)
    }

    /// What the front end says of linked devices (None: not linked): this
    /// one's name, the others (name, seconds since heard from).
    pub fn set_devices(&mut self, d: Option<(String, Vec<(String, i64)>)>) {
        self.social.devices = d;
    }

    /// The post at the top left (see `Social::post_top`).
    #[allow(dead_code)]
    pub fn set_post_top(&mut self, on: bool) {
        self.social.post_top = on;
    }

    /// The other villagers kept on this device (their names).
    pub fn set_villagers(&mut self, names: Vec<String>) {
        self.social.villagers = names;
    }

    /// Into someone's village: the villager arrives beside its host.
    pub fn arrive_by_host(&mut self) {
        let host = self.online.others().iter().find(|(_, o)| o.host).map(|(_, o)| o.av);
        if let Some(av) = host {
            let (x, y) = self.local(av.x, av.y);
            self.place_player(Some([x - 4.0 * av.dir.signum().max(-1.0), y]));
        }
    }

    /// Village coordinates (see `net::Avatar`): from the big tree, in the
    /// village's own units, the same on any screen.
    pub(super) fn shared(&self, x: f32, y: f32) -> (f32, f32) {
        let (ax, ay, u) = self.anchor;
        ((x - ax) / u, (y - ay) / u)
    }

    pub(super) fn local(&self, sx: f32, sy: f32) -> (f32, f32) {
        let (ax, ay, u) = self.anchor;
        (ax + sx * u, ay + sy * u)
    }

    /// A shared village joined (its seed), or home again (None), for the
    /// front end to lay out anew.
    pub fn take_village(&mut self) -> Option<Option<u32>> {
        self.village_switch.take()
    }

    /// A line of typing wanted: what for, and the question.
    pub fn take_text_ask(&mut self) -> Option<(TextFor, String)> {
        self.text_ask.take()
    }

    /// Text to put on the clipboard (a code).
    pub fn take_copy(&mut self) -> Option<String> {
        self.copy_ask.take()
    }

    pub(super) fn post(&mut self, s: impl Into<String>) {
        let t = self.now();
        self.post.push((t, s.into()));
        if self.post.len() > 6 {
            self.post.remove(0);
        }
    }

    /// A line in the post from the front end.
    pub fn note(&mut self, s: &str) {
        self.post(s.to_string());
    }

    fn chime(&mut self) {
        if let Some(p) = &self.player {
            self.director.sound(Sfx::Chime, p.centre(), 0.5);
        }
    }

    /// Where the villager is, for the others in a shared village.
    fn avatar(&self) -> Option<Avatar> {
        let p = self.player.as_ref()?;
        let away = if self.trip.is_some() { 2 } else if self.inside.is_some() { 1 } else { 0 };
        let (x, y) = match &self.inside {
            Some(inn) => (inn.outside.0, inn.outside.1),
            None => (p.centre(), p.y),
        };
        let (sx, sy) = self.shared(x, y);
        let rgb = |c: Rgb| ((c.r.clamp(0.0, 1.0) * 255.0) as u32) << 16 | ((c.g.clamp(0.0, 1.0) * 255.0) as u32) << 8 | (c.b.clamp(0.0, 1.0) * 255.0) as u32;
        let look = player::Player::look();
        let t = self.now();
        Some(Avatar {
            x: sx,
            y: sy,
            dir: p.dir,
            moving: p.moving,
            air: p.in_air(),
            coat: rgb(look.coat),
            hat: look.hat.map(rgb).unwrap_or(0),
            away,
            hair: rgb(look.hair),
            skin: rgb(look.skin),
            scarf: look.scarf.map(rgb).unwrap_or(0),
            trip: self.trip.as_ref().map(|tr| tr.here(t)),
            room: self.social.room.unwrap_or(0),
            seat: match (self.social.room, self.inside.as_ref().and_then(|i| i.sitting()), self.table.as_ref().and_then(|tb| tb.net.as_ref())) {
                (Some(_), Some(_), Some(n)) if !n.seats.is_empty() => n.seat as i8,
                _ => -1,
            },
            rest: self.social.resting,
            pets: self.pets_sent(),
            // (stamped as it's sent)
            at: 0.0,
        })
    }

    /// The pets following this villager, for the others to see them too
    /// (those at home stay out of it).
    fn pets_sent(&self) -> [u16; 4] {
        let mut out = [0u16; 4];
        let following = self.my_pet.iter().chain(self.more_pets.iter()).filter(|p| !p.staying);
        for (slot, p) in out.iter_mut().zip(following) {
            let kind = pets::KINDS.iter().position(|k| *k == p.kind).unwrap_or(0) as u16;
            let coat = p.kind.coats().iter().position(|c| c.0.eq_ignore_ascii_case(p.coat.trim())).unwrap_or(0) as u16;
            *slot = (kind + 1) << 8 | coat.min(255);
        }
        out
    }

    /// Paused (the pause button, the wallpaper resting): the village
    /// stands still while only the online side goes on (see `rest_tick`).
    pub fn set_resting(&mut self, on: bool) {
        self.social.resting = on;
    }

    pub fn resting(&self) -> bool {
        self.social.resting
    }

    /// Resting: the online side alone goes on (friends' messages, a village
    /// hosted here kept open, this villager still about for the others, if
    /// standing still), the village itself held as it was.
    pub fn rest_tick(&mut self, dt: f32) {
        let t = self.now();
        self.social.rested += dt;
        self.update_online(dt, t);
    }

    /// This villager's profile card, as friends see it.
    pub fn my_card(&self) -> Card {
        let (coat, hat, hair, skin, scarf) = self.social.wear.wear_colours();
        let pet = match (self.pet_cfg.kind.as_str(), self.pet_cfg.name.trim()) {
            ("none", _) | ("", _) => String::new(),
            (k, "") => format!("a {k}"),
            (k, n) => format!("a {k} called {n}"),
        };
        let pk = &self.pockets;
        Card {
            name: self.your_name().to_string(),
            motto: self.social.motto.clone(),
            pet,
            look: [coat, hat.unwrap_or(0), hair, skin, scarf.unwrap_or(0)],
            won: pk.games_won,
            runs: pk.ski_runs,
            best: pk.ski_score,
            fights: pk.fights_won,
            cheer: pk.cheer,
            friends: self.online.book.friends.iter().filter(|p| !p.waiting).count() as u32,
        }
    }

    /// Each frame: the online side moves on, and what it heard is shown.
    pub(super) fn update_online(&mut self, dt: f32, t: f32) {
        self.update_chats();
        if self.social.want_public.replace(false) {
            self.online.watch_public();
        }
        // (the villager's own settings for being online)
        self.online.limit = (self.cfg.village_limit as usize).clamp(2, crate::net::MAX_LIMIT);
        self.online.clean_words = self.cfg.clean_words;
        let name = if self.you_name.trim().is_empty() { "a neighbour".to_string() } else { self.you_name.trim().to_string() };
        self.online.set_name(&name);
        if t >= self.social.next_card {
            self.social.next_card = t + 3.0;
            let c = self.my_card();
            self.online.set_card(c);
        }
        // once: online's there to be found
        if !self.social.tipped && t > 25.0 && !self.online.wanted() && self.player.is_some() {
            self.social.tipped = true;
            if !self.online.book.tipped {
                self.online.book.tipped = true;
                self.online.dirty = true;
                self.post("✉ play with friends online: the ✉ button, or O");
            }
        }
        // (a guest: only once in the host's village, not still in their own)
        let me = match self.online.village() {
            Some((_, true)) => self.avatar(),
            Some((_, false)) if self.online.guest_seed() == Some(self.seed) => self.avatar(),
            _ => None,
        };
        // (tests run the village's clock fast: the online side keeps to it)
        #[cfg(test)]
        let real = t + self.social.rested;
        #[cfg(not(test))]
        let real = {
            let ms = crate::clock::now().timestamp_millis();
            let (start, last) = self.social.clock.get_or_insert((ms, 0.0));
            let now = (((ms - *start) as f64) / 1000.0).max(*last as f64) as f32;
            *last = now;
            now
        };
        self.online.tick(real, me, self.seed);
        self.online.ease(dt);
        // the others' jumps and landings (as they're drawn): a puff of
        // snow, as for this villager
        let mut puffs = vec![];
        for (id, o) in self.online.others() {
            let air = o.av.away == 0 && self.online.shown_as(o).0;
            if self.social.others_air.insert(id, air).is_some_and(|was| was != air) {
                puffs.push(self.local(o.shown.0, o.shown.1));
            }
        }
        if self.social.others_air.len() > 64 {
            self.social.others_air.clear();
        }
        for (x, y) in puffs {
            self.director.snow_puff(x, y);
        }
        // (back home: the village's own settings again)
        if self.online.guest_seed().is_none() {
            self.social.rules_in = None;
        }
        // the same folk out and about for everyone in a shared village: the
        // host says who (a few times a second); a guest shows them
        match self.online.village() {
            Some((_, true)) if !self.online.others().is_empty() && t >= self.social.crowd_at => {
                self.social.crowd_at = t + 0.35;
                let (ax, ay, u) = self.anchor;
                let folk = self.critters.crowd(t, |x, y| ((x - ax) / u, (y - ay) / u));
                let said = self.director.fresh_said(t);
                if let Ok(data) = serde_json::to_string(&Crowd { folk, said, at: real }) {
                    self.online.crowd(data);
                }
            }
            // (the host's gone quiet about it: the village's own folk again;
            // home, the village is made anew anyway)
            _ if self.critters.puppets && t - self.social.crowd_at > 8.0 => self.critters.end_crowd(),
            _ => {}
        }
        for e in self.online.take_events() {
            self.on_event(e, t);
        }
        self.social.asked.retain(|a| t < a.until);
        if self.social.asking.is_some_and(|a| t > a.3) {
            self.social.asking = None;
        }
        // (asked, no answer yet; said yes: again, in case it went astray)
        if let Some((to, what, seed, _)) = self.social.asking.filter(|a| a.0 != 0) {
            if t >= self.social.asked_again {
                self.social.asked_again = t + 4.0;
                self.online.ask(to, what, seed);
            }
        }
        let mut again = std::mem::take(&mut self.social.yes_again);
        again.retain(|y| t < y.3);
        for y in again.iter_mut().filter(|y| t >= y.2) {
            y.2 = t + 2.0;
            self.online.answer(y.0, y.1, true);
        }
        self.social.yes_again = again;
        self.update_net_tag(t);
        self.update_net_fight(t);
        self.update_carols(t);
        self.update_net_table(t);
        let p = &mut self.post;
        p.retain(|(at, _)| t - *at < POST_TIME);
    }

    /// A line in the messages.
    fn log_line(&mut self, text: String, to_me: bool) {
        self.log_line_with(text, to_me, vec![]);
    }

    fn log_line_with(&mut self, text: String, to_me: bool, links: Vec<String>) {
        let at = crate::clock::now().timestamp();
        self.social.log.push(Line { at, text, links });
        if self.social.log.len() > 60 {
            self.social.log.remove(0);
        }
        if to_me {
            self.social.unread = (self.social.unread + 1).min(99);
        }
    }

    /// Where the villager is, in a few words (the web page's title):
    /// "Sam's village", "the games room", "on the slopes"…
    #[cfg_attr(not(any(target_arch = "wasm32", target_os = "android")), allow(dead_code))]
    pub fn place_words(&self) -> String {
        let t = self.now();
        if let Some(tr) = &self.trip {
            return match tr.here(t).at {
                1 => "on the shuttle".into(),
                2 => "on the train".into(),
                3 => "at the ski lodge".into(),
                4 => "on the chairlift".into(),
                5 => "on the slopes".into(),
                6 => "out for a drive".into(),
                _ => "away on a trip".into(),
            };
        }
        if let Some(inn) = &self.inside {
            let room = inn.kind().title();
            return if inn.home && !matches!(inn.kind(), interior::RoomKind::Games | interior::RoomKind::Igloo) { "at home".into() } else { room.into() };
        }
        if let Some((name, _)) = self.online.square() {
            return name;
        }
        if self.online.guest_seed().is_some() {
            if let Some((_, o)) = self.online.others().into_iter().find(|(_, o)| o.host) {
                return format!("{}'s village", o.name);
            }
            return "a friend's village".into();
        }
        match self.your_name().trim() {
            "" | "a neighbour" | "neighbour" => String::new(),
            n => format!("{n}'s village"),
        }
    }

    /// What the friends widget shows (None: not online).
    pub fn friends_view(&self) -> Option<FriendsView> {
        let mut friends: Vec<(String, u8, usize)> = self
            .online
            .friends()
            .iter()
            .enumerate()
            .map(|(i, (p, h))| {
                let state = match h {
                    _ if p.waiting => 3,
                    Some(h) if h.village.is_some() => 2,
                    Some(h) if h.on => 1,
                    _ => 0,
                };
                (p.name.clone(), state, i)
            })
            .collect();
        // a village open, then online, then the rest; by name
        friends.sort_by_key(|f| ([2, 1, 0, 3][f.1 as usize % 4], f.0.to_lowercase()));
        let n = self.social.log.len();
        let lines = self.social.log[n.saturating_sub(3)..].iter().map(|l| l.text.clone()).collect();
        let here = if self.online.village().is_some() { self.online.others().len() + 1 } else { 0 };
        Some(FriendsView { online: self.online.wanted(), connected: self.online.link() == Link::On, friends, unread: self.social.unread, lines, here })
    }

    pub fn messages_read(&mut self) {
        self.social.unread = 0;
    }

    /// Hosting: the village's settings as they stand (told the guests).
    pub fn set_rules(&mut self, r: &Rules) {
        if self.social.rules_out.as_ref() != Some(r) {
            self.social.rules_out = Some(r.clone());
            self.online.set_rules(r.json());
        }
    }

    /// A guest: the host's village settings (what this village is, here),
    /// and whether this villager may change them.
    pub fn village_rules(&self) -> Option<&(Rules, bool)> {
        self.social.rules_in.as_ref().filter(|_| self.online.guest_seed().is_some())
    }

    /// The village's settings when they aren't this villager's own: a
    /// friend's village, the host's; a public square, its keeper's for the
    /// village's own settings and this villager's for the rest.
    pub fn rules_now(&self, own: &crate::config::SceneCfg) -> Option<(Rules, Lock)> {
        if let Some(json) = self.online.square_rules() {
            let keeper: Rules = serde_json::from_str(&json).unwrap_or_default();
            let mut r = Rules::of(own);
            r.take_owned(&keeper);
            return Some((r.clamped(), Lock::Keeper));
        }
        self.village_rules().map(|(r, admin)| (r.clone(), if *admin { Lock::Admin } else { Lock::Host }))
    }

    /// A guest changing one of the village's settings: asked of the host
    /// (an admin's goes straight in).
    pub fn ask_rule(&mut self, key: &str, value: &str) {
        let Some((_, admin)) = self.village_rules().cloned() else { return };
        self.online.want(key, value);
        let words = Rules::words(key, value);
        if admin {
            self.post(format!("⚙ {words} (for everyone here)"));
        } else {
            let host = self.online.others().iter().find(|(_, o)| o.host).map(|(_, o)| o.name.clone()).unwrap_or_else(|| "the host".into());
            self.post(format!("⚙ {host} is asked: {words}"));
        }
    }

    /// Out in the village (not away on a trip, not indoors): free to go
    /// into another (a linked device's).
    pub fn out_and_about(&self) -> bool {
        self.trip.is_none() && self.inside.is_none() && self.table.is_none() && self.player.is_some()
    }

    /// The talk line (the / key).
    pub fn open_chat(&mut self) {
        let ask = if self.online.village().is_some() { "say something (@name: to one person · @group: to a group)" } else { "@name: to a friend · @group: to a group" };
        self.text_ask = Some((TextFor::Chat, ask.into()));
    }

    /// A line from the talk line: "@who what" to one person or a group,
    /// anything else aloud to everyone here.
    fn chat(&mut self, text: &str, t: f32) {
        let Some(rest) = text.strip_prefix('@') else {
            if self.online.village().is_some() {
                self.online.say(text);
                self.log_line(format!("☺ you: {text}"), false);
            } else if self.player.is_some() {
                self.post("(nobody online here to hear it: @name for a friend)");
            }
            self.you_say(text, t);
            return;
        };
        let (who, what) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let (who, what) = (who.trim().trim_start_matches('#').to_lowercase(), what.trim());
        if who.is_empty() || what.is_empty() {
            self.post("@name, then what to say");
            return;
        }
        let starts = |n: &str| {
            let n = n.to_lowercase();
            n == who || n.replace(' ', "") == who || n.starts_with(&who)
        };
        // a group
        if let Some(i) = self.online.book.groups.iter().position(|g| starts(&g.name)) {
            let name = self.online.book.groups[i].name.clone();
            if self.online.group_say(i, what) {
                self.log_line(format!("#{name} you: {what}"), false);
                self.post(format!("#{name} you: {what}"));
            } else {
                self.post("not online yet");
            }
            return;
        }
        // someone here
        if let Some((id, name)) = self.online.others().iter().find(|(_, o)| starts(&o.name)).map(|(id, o)| (*id, o.name.clone())) {
            self.online.whisper(id, what);
            self.log_line(format!("✉ to {name} (just them): {what}"), false);
            self.post(format!("✉ to {name} (just them): {what}"));
            return;
        }
        // a friend, wherever they are
        if let Some(i) = self.online.book.friends.iter().position(|p| !p.waiting && starts(&p.name)) {
            let name = self.online.book.friends[i].name.clone();
            if self.online.mail(i, what) {
                self.log_line(format!("✉ to {name}: {what}"), false);
                self.post(format!("✉ to {name}: {what}"));
            } else {
                self.post("✉ not sent (not online yet)");
            }
            return;
        }
        self.post(format!("nobody called {who}: a friend, someone here, or a group"));
    }

    fn other_name(&self, id: u32) -> String {
        self.online.other(id).map(|o| o.name.clone()).unwrap_or_else(|| "someone".into())
    }

    pub(super) fn on_event(&mut self, e: Event, t: f32) {
        match e {
            Event::Note(s) => self.post(s),
            Event::Mail { from, text, links } => {
                self.post(format!("✉ {from}: {text}"));
                self.log_line_with(format!("✉ {from}: {text}"), true, links);
                self.chime();
            }
            Event::Pinged(name) => {
                self.post(format!("♪ {name} pinged you"));
                self.log_line(format!("♪ {name} pinged you"), true);
                // a little tone: two notes
                if let Some(p) = &self.player {
                    let x = p.centre();
                    self.director.sound(Sfx::Ping, x, 0.8);
                } else {
                    self.chime();
                }
            }
            Event::Whispered { id, name, text, links } => {
                let _ = id;
                self.post(format!("✉ {name} (just to you): {text}"));
                self.log_line_with(format!("✉ {name} (just to you): {text}"), true, links);
                self.chime();
            }
            Event::GroupSaid { group, name, text, links } => {
                self.post(format!("#{group} {name}: {text}"));
                self.log_line_with(format!("#{group} {name}: {text}"), true, links);
                self.chime();
            }
            Event::Wanted { id, name, key, value, admin } => {
                let mut probe = self.social.rules_out.clone().unwrap_or_default();
                if !probe.set(&key, &value) {
                    return;
                }
                let words = Rules::words(&key, &value);
                if admin {
                    self.social.front.push(Front::SetRule(key, value));
                    self.post(format!("⚙ {name} set {words}"));
                } else {
                    self.social.asked.retain(|a| !(a.id == id && a.rule.as_ref().is_some_and(|r| r.0 == key)));
                    self.social.asked.push(Asked { id, name: name.clone(), what: ask::SETTING, seed: 0, until: t + ASK_TIME, to: self.online.my_id(), rule: Some((key, value)) });
                    self.post(format!("⚙ {name} asks: {words}? (Y yes · N no)"));
                    self.chime();
                }
            }
            Event::Crowd(data) => {
                if let Ok(c) = serde_json::from_str::<Crowd>(&data) {
                    self.social.crowd_at = t;
                    let (ax, ay, u) = self.anchor;
                    self.critters.set_crowd(&c.folk, |x, y| (ax + x * u, ay + y * u), t, c.at);
                    // what they said, over their heads
                    for (id, text, life) in c.said.iter().take(8) {
                        if let Some((x, y)) = self.critters.where_is(*id) {
                            let text = crate::net::clean(text, 120);
                            self.director.said_by(*id, x.round() as i32, y as i32 - 4, &text, t, life.clamp(1.0, 12.0));
                        }
                    }
                }
            }
            Event::Rules { rules, admin } => {
                if let Some(r) = Rules::parse(&rules) {
                    self.social.rules_in = Some((r, admin));
                }
            }
            Event::Asked(name) => {
                self.post(format!("✦ {name} asks to be friends (✉ friends to say yes)"));
                self.chime();
            }
            Event::Village(seed) => {
                if seed != self.seed {
                    self.village_switch = Some(Some(seed));
                }
            }
            Event::Home => self.village_switch = Some(None),
            Event::Said { id, text, links } => {
                if let Some(o) = self.online.other(id) {
                    let name = o.name.clone();
                    let (x, y) = self.local(o.shown.0, o.shown.1);
                    self.director.say(x.round() as i32, y as i32 - 4, &text, t, 4.0, 6);
                    self.log_line_with(format!("☺ {name}: {text}"), true, links);
                }
            }
            Event::Threw { id, x, y, vx, vy } => {
                let (lx, ly) = self.local(x, y);
                let u = self.anchor.2;
                let (pvx, pvy) = (vx * u, vy * u);
                let at_you = self.trip.is_none()
                    && self.inside.is_none()
                    && self.player.as_ref().is_some_and(|p| {
                        // (where it comes down: about the villager?)
                        let flight = ((p.centre() - lx) / pvx.max(1e-3).copysign(pvx)).abs();
                        (0.1..2.0).contains(&flight) && (lx + pvx * flight - p.centre()).abs() < 3.0 && (ly - p.y).abs() < 6.0
                    });
                self.critters.throw_from(lx, ly, pvx, pvy, at_you, &mut self.director);
                if at_you {
                    // (they got the villager: they hear so)
                    self.online.wave(id, HIT);
                    if let Some(f) = &mut self.social.fight {
                        if f.other == id {
                            f.theirs += 1;
                        }
                    }
                }
            }
            Event::Invited { name, code } => {
                self.social.invited.retain(|(n, _)| *n != name);
                self.social.invited.push((name.clone(), code));
                if self.social.invited.len() > 6 {
                    self.social.invited.remove(0);
                }
                self.post(format!("⌂ {name} asks you into their village (✉ friends to go)"));
                self.chime();
            }
            Event::Waved { id, to, what } => {
                let name = self.other_name(id);
                let me = self.online.my_id();
                if what == HIT {
                    if to == me {
                        self.post(format!("● you got {name}!"));
                        self.pockets.hits += 1;
                        if let Some(f) = &mut self.social.fight {
                            if f.other == id {
                                f.mine += 1;
                            }
                        }
                    }
                    return;
                }
                if let Some(o) = self.online.other(id) {
                    let (x, y) = self.local(o.shown.0, o.shown.1);
                    let (word, ch, c) = match what {
                        1 => ("hooray!", '★', Rgb::hex(0xffd36b)),
                        2 => ("ha ha!", '♪', Rgb::hex(0xbfe4ff)),
                        3 => ("♥", '♥', Rgb::hex(0xff6a8a)),
                        _ => ("hi!", '✦', Rgb::hex(0xffe9a8)),
                    };
                    if self.trip.is_none() && self.inside.is_none() {
                        self.director.emote(x, y - 5.0, ch, c, 2);
                        self.director.say(x.round() as i32, y as i32 - 4, word, t, 1.6, 6);
                    }
                }
                if to == me {
                    let line = match what {
                        3 => format!("♥ {name} sends you a heart"),
                        1 => format!("★ {name} cheers you on"),
                        _ => format!("✦ {name} waves at you"),
                    };
                    self.post(line);
                }
            }
            Event::Play { id, to, what, seed } => {
                // (asked again, already said yes to: that game's on)
                if self.social.yes_again.iter().any(|y| y.0 == id && y.1 == what) || self.social.asked.iter().any(|a| a.id == id && a.what == what && a.seed == seed) {
                    return;
                }
                let name = self.other_name(id);
                self.social.asked.retain(|a| !(a.id == id && a.what == what));
                self.social.asked.push(Asked { id, name: name.clone(), what, seed, until: t + ASK_TIME, to, rule: None });
                if self.social.asked.len() > 8 {
                    self.social.asked.remove(0);
                }
                let key = if to == 0 { "" } else { " (Y yes · N no)" };
                self.post(format!("{} {name}: {}?{key}", ask_icon(what), ask_words(what)));
                self.chime();
            }
            Event::Answer { id, what, yes } => {
                let name = self.other_name(id);
                if what == ask::SETTING {
                    self.post(if yes { format!("⚙ {name} said yes") } else { format!("⚙ {name} would rather not") });
                    return;
                }
                let Some((to, w, seed, _)) = self.social.asking else {
                    if yes && what == ask::RACE {
                        self.post(format!("⇘ {name}'s in the race!"));
                    }
                    return;
                };
                if w != what || (to != id && to != 0) {
                    return;
                }
                if to != 0 {
                    self.social.asking = None;
                }
                if !yes {
                    self.post(format!("{name} says no thanks"));
                    return;
                }
                match what {
                    ask::RACE => {
                        if let Some(r) = self.social.race.as_mut().filter(|r| r.seed == seed && !r.racers.contains(&id)) {
                            r.racers.push(id);
                        }
                        self.post(format!("⇘ {name}'s in the race!"));
                    }
                    ask::TRIP => self.post(format!("⇄ {name}'s coming along!")),
                    ask::TAG => self.start_net_tag(id, true, t),
                    ask::FIGHT => self.start_net_fight(id, t),
                    // everyone asked: they sit down at the table waiting
                    _ if to == 0 => {
                        if let Some(tb) = self.table.as_mut().filter(|tb| tb.net.as_ref().is_some_and(|n| n.game == seed)) {
                            tb.sit_in(id, &name);
                            self.post(format!("☺ {name} sits down at the table"));
                        }
                    }
                    k => {
                        let me = self.online.my_id();
                        let mine = self.your_name().to_string();
                        self.start_net_game(k, seed, vec![me, id], vec![mine, name.clone()], seed);
                    }
                }
            }
            Event::Table { id, game, seq, kind, state } => {
                let _ = id;
                if let Some(tb) = self.table.as_mut().filter(|tb| tb.kind.code() == kind && tb.net.as_ref().is_some_and(|n| n.game == game)) {
                    if tb.adopt(seq, &state) {
                        self.director.sound(Sfx::Click, self.cam + self.w as f32 / 2.0, 0.5);
                    }
                }
            }
            Event::Up { id, game } => {
                let name = self.other_name(id);
                if let Some(tb) = self.table.as_mut().filter(|tb| tb.net.as_ref().is_some_and(|n| n.game == game && n.seats.contains(&id))) {
                    let n = tb.net.as_ref().unwrap();
                    // seat 0 gone (or a game of two): it's off; else seat 0
                    // deals on without them
                    let owner_left = n.seats.first() == Some(&id);
                    if owner_left || n.seats.len() <= 2 {
                        if let Some(n) = &mut tb.net {
                            n.left = true;
                        }
                        tb.status = format!("{name} got up from the table · Esc: up too");
                        tb.result = tb.result.or(Some(games::Result::Drawn));
                        tb.scored = true;
                    } else if n.owner() {
                        tb.sit_out(id);
                    } else {
                        tb.status = format!("{name} got up · waiting for the next hand");
                    }
                }
            }
            Event::Show { id, what, n, x } => {
                let name = self.other_name(id);
                let (lx, _) = self.local(x, 0.0);
                match what {
                    0 => {
                        // (as long as the menu's longest, at most)
                        if self.director.plan_light_show(t, events::SHOW_LEAD * 0.6, (n as f32).min(900.0)) {
                            self.post(format!("✦ {name}'s put on a light show by the big tree"));
                        }
                    }
                    1 => {
                        // (as many as the grand finale, at most)
                        self.director.fireworks(t + 0.6, (n as usize).min(30), lx);
                        self.post(format!("✸ {name}'s setting off fireworks"));
                    }
                    _ => {
                        self.social.carols.retain(|c| c.0 != id);
                        self.social.carols.push((id, t + 12.0, t));
                        let (px, _) = self.local(x, 0.0);
                        self.critters.gather(px, t + 12.0, t, &mut self.director, "a carol! wait for me!");
                        self.post(format!("♪ {name} is singing a carol"));
                    }
                }
            }
            // a race: everyone's at the top; off at the time said
            Event::Go { seed, at } => {
                let wait = ((at - crate::clock::now().timestamp_millis()) as f32 / 1000.0).clamp(0.0, 10.0);
                if let Some(tr) = self.trip.as_mut().filter(|tr| tr.race && tr.seed == seed) {
                    tr.go(t + wait);
                }
            }
            // the village's host announcing something
            Event::Notice(text) => {
                self.post(format!("✦ {text}"));
                self.log_line(format!("✦ (the host) {text}"), true);
                self.chime();
            }
            // a shooting star someone sent off
            Event::Star { x, y } => {
                let sx = self.local(x, 0.0).0 - self.cam;
                if (-20.0..self.w as f32 + 20.0).contains(&sx) {
                    self.director.shooting_star(sx, y.min(self.ctx.sky_rows as f32 * 2.0));
                }
            }
            Event::Tagged { id, it, left } => {
                let me = self.online.my_id();
                if let Some(g) = &mut self.social.tag {
                    if g.other == id {
                        g.it = it;
                        g.grace = t + 1.5;
                        g.until = t + left.clamp(0.0, TAG_TIME);
                        if it == me {
                            self.post(format!("⚑ {} tagged you — you're it!", self.other_name(id)));
                            self.director.sound(Sfx::Laugh, self.cam + self.w as f32 / 2.0, 0.5);
                        }
                    }
                } else if it == me || it == id {
                    // (they said yes, and the game's begun)
                    self.social.tag = Some(NetTag { other: id, it, grace: t + 1.5, until: t + left.clamp(0.0, TAG_TIME), tags: 0 });
                }
            }
            Event::Finish { name, score, secs, .. } => {
                if let Some(tr) = &mut self.trip {
                    tr.result(&name, score, secs, t);
                } else {
                    self.post(format!("⇘ {name} raced down: {score} points in {secs:.1}s"));
                }
            }
            Event::Synced { dev, at, state } => self.social.synced.push((dev, at, state)),
        }
    }

    // ------------------------------------------------------------ playing

    /// A game with friends at the table in the games room, dealt now
    /// (`seats`: their ids, the asker first, and names).
    fn start_net_game(&mut self, what: u8, game: u32, seats: Vec<u32>, names: Vec<String>, seed: u32) {
        self.start_table(what, game, seats, names, seed, false);
    }

    /// A table in the games room: dealt now, or (`lobby`) waiting for
    /// everyone to sit down.
    fn start_table(&mut self, what: u8, game: u32, seats: Vec<u32>, names: Vec<String>, seed: u32, lobby: bool) {
        let Some(kind) = games::Kind::from_code(what) else { return };
        let me = self.online.my_id();
        let others: Vec<String> = names.iter().zip(&seats).filter(|(_, &s)| s != me).map(|(n, _)| n.clone()).collect();
        let mut tb = if lobby { games::Table::lobby(kind, game, seats, names, me) } else { games::Table::online(kind, game, seats, names, me, seed) };
        if let Some(old) = self.table.take() {
            tb.pos = old.pos;
            self.left_table(&old);
        }
        self.table = Some(tb);
        self.convo = None;
        let with = if others.is_empty() { "friends".to_string() } else { others.join(", ") };
        self.post(format!("{} {} with {with}: to the games room", kind.icon(), kind.name()));
        // (round the table in the games room)
        self.goal = None;
        self.soon(play::Pending::GameRoom(game));
    }

    /// Into the games room (the table's number): the villager comes in by
    /// the door, and goes to their chair.
    pub(super) fn enter_games_room(&mut self, game: u32) {
        let t = self.now();
        let Some(p) = self.player.as_ref() else { return };
        let outside = match &self.inside {
            Some(inn) => inn.outside,
            None => (p.x, p.y),
        };
        if self.inside.is_some() {
            // (out of wherever the villager was, first)
            self.leave();
        }
        let (rw, rh) = room_size(self.w, self.h);
        let inn = interior::Interior::games_room(rw, rh, game, outside, t);
        let (ex, ey) = inn.entrance();
        if let Some(p) = self.player.as_mut() {
            (p.x, p.y, p.dir) = (ex, ey, 1.0);
            p.lie = None;
            p.moving = false;
        }
        if let Some(pet) = self.my_pet.as_mut().filter(|pet| !pet.staying) {
            (pet.x, pet.y) = (ex + 9.0, ey + 1.0);
        }
        self.inside = Some(Box::new(inn));
        self.social.room = Some(game);
        self.social.to_seat = true;
        self.social.panel_at = t + 3.0;
    }

    /// In the games room, on the way to a chair: the steps there (None: sat
    /// down, or left to the villager).
    pub(super) fn seat_steer(&mut self) -> Option<(f32, f32)> {
        if !self.social.to_seat {
            return None;
        }
        let now = self.now();
        let chair = self.table.as_ref().and_then(|tb| tb.net.as_ref()).filter(|n| !n.seats.is_empty()).map(|n| n.seat).unwrap_or(0);
        let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) else {
            self.social.to_seat = false;
            return None;
        };
        if !inn.is_games() {
            self.social.to_seat = false;
            return None;
        }
        if inn.take_seat(chair, p) {
            self.social.to_seat = false;
            self.social.panel_at = self.social.panel_at.min(now + 0.6);
            return None;
        }
        let x = inn.seat_x(chair)?;
        Some(((x - p.centre()).signum(), 0.0))
    }

    /// Who's sat where in the games room (friends online in their chairs).
    fn update_room(&mut self) {
        let Some(room) = self.social.room else { return };
        if !self.inside.as_ref().is_some_and(|i| i.is_games()) {
            self.social.room = None;
            self.social.to_seat = false;
            // (out of the games room: up from its table too)
            if let Some(old) = self.table.take_if(|tb| tb.net.as_ref().is_some_and(|n| n.game == room)) {
                self.left_table(&old);
            }
            return;
        }
        let visitors: Vec<interior::Visitor> = self
            .online
            .others()
            .iter()
            .filter(|(_, o)| o.av.room == room && o.av.seat >= 0)
            .map(|(id, o)| interior::Visitor { name: o.name.clone(), look: look_of(&o.av, *id), seat: o.av.seat as usize })
            .collect();
        if let Some(inn) = self.inside.as_mut() {
            inn.visitors = visitors;
        }
        // (the game over the room, not over the table and its chairs)
        let w = self.w as i32;
        if let Some(tb) = self.table.as_mut().filter(|tb| tb.pos.is_none()) {
            tb.pos = Some(((w - 54) / 2, 1));
        }
    }

    /// The table shows (in the games room, once sat down, or a moment in).
    pub(super) fn table_shows(&self) -> bool {
        self.social.room.is_none() || !self.social.to_seat || self.now() >= self.social.panel_at
    }

    /// The game with a friend: moves made go to them (and again now and
    /// then while it's their move, in case one went astray).
    fn update_net_table(&mut self, t: f32) {
        self.update_room();
        let Some(tb) = &mut self.table else { return };
        let Some(game) = tb.net.as_ref().filter(|n| !n.left).map(|n| n.game) else { return };
        let kind = tb.kind.code();
        let mut send = None;
        if let Some((seq, st)) = tb.take_out() {
            send = Some((seq, st));
        } else if let Some(n) = &tb.net {
            // (again now and then, in case it went astray: while waiting on
            // the others, and seat 0 while friends are sitting down)
            if n.seq > 0 && t - n.sent > 3.0 && (tb.theirs() || tb.result.is_some()) && t - n.sent < 600.0 {
                send = Some((n.seq, tb.state_json()));
            }
        }
        if let Some((seq, st)) = send {
            if let Some(n) = &mut tb.net {
                n.sent = t;
            }
            self.online.table(game, seq, kind, st);
        }
        // anyone gone (out of the village): seat 0 deals on without them;
        // seat 0 gone (or a game of two), the game's off
        let gone: Vec<u32> = match self.table.as_ref().and_then(|tb| tb.net.as_ref()) {
            Some(n) => n.others().into_iter().filter(|&id| self.online.other(id).is_none()).collect(),
            None => vec![],
        };
        for id in gone {
            let Some(tb) = &mut self.table else { return };
            let Some(n) = tb.net.as_ref() else { return };
            if n.seats.first() == Some(&id) || n.seats.len() <= 2 {
                if let Some(n) = &mut tb.net {
                    n.left = true;
                }
                tb.status = "they've gone · Esc: up from the table".into();
                tb.result = tb.result.or(Some(games::Result::Drawn));
                tb.scored = true;
            } else if n.owner() {
                tb.sit_out(id);
            }
        }
    }

    /// Up from a game with friends: they hear so.
    pub(super) fn left_table(&mut self, tb: &games::Table) {
        if let Some(n) = &tb.net {
            self.online.table_up(n.game);
        }
    }

    fn start_net_tag(&mut self, other: u32, asker: bool, t: f32) {
        let me = self.online.my_id();
        // (whoever asked is "it" first)
        let it = if asker { me } else { other };
        self.social.tag = Some(NetTag { other, it, grace: t + 2.0, until: t + TAG_TIME, tags: 0 });
        self.online.tagged(it, TAG_TIME);
        let name = self.other_name(other);
        self.post(if it == me { format!("⚑ tag with {name}: you're it — catch them!") } else { format!("⚑ tag with {name}: they're it — run!") });
        self.pockets.tag_games += 1;
    }

    fn update_net_tag(&mut self, t: f32) {
        let Some(mut g) = self.social.tag else { return };
        let me = self.online.my_id();
        let name = self.other_name(g.other);
        if t > g.until || self.online.other(g.other).is_none() {
            self.social.tag = None;
            self.post(format!("⚑ tag's over: {} tags ({name})", g.tags));
            if g.tags >= 2 {
                self.cheer(1);
            }
            return;
        }
        if g.it == me && t > g.grace && self.trip.is_none() && self.inside.is_none() {
            let other = self.online.other(g.other).filter(|o| o.av.away == 0).map(|o| self.local(o.shown.0, o.shown.1));
            if let (Some((ox, oy)), Some(p)) = (other, self.player.as_ref()) {
                if (ox - p.centre()).abs() < 2.5 && (oy - p.y).abs() < 2.0 {
                    g.it = g.other;
                    g.grace = t + 1.5;
                    g.tags += 1;
                    self.pockets.tags += 1;
                    self.online.tagged(g.it, g.until - t);
                    self.you_say(&format!("tag! you're it, {name}!"), t);
                    self.director.sound(Sfx::Pickup, ox, 0.5);
                }
            }
        }
        self.social.tag = Some(g);
    }

    fn start_net_fight(&mut self, other: u32, t: f32) {
        self.social.fight = Some(NetFight { other, until: t + FIGHT_TIME, mine: 0, theirs: 0 });
        let name = self.other_name(other);
        self.post(format!("● a snowball fight with {name}: R to scoop and throw!"));
    }

    fn update_net_fight(&mut self, t: f32) {
        let Some(f) = self.social.fight else { return };
        if t < f.until && self.online.other(f.other).is_some() {
            return;
        }
        self.social.fight = None;
        let name = self.other_name(f.other);
        if f.mine > f.theirs {
            self.pockets.fights_won += 1;
            self.cheer(2);
            self.post(format!("● you won the snowball fight, {} to {} ({name})", f.mine, f.theirs));
        } else if f.mine < f.theirs {
            self.pockets.fights_lost += 1;
            self.post(format!("● {name} won the snowball fight, {} to {}", f.theirs, f.mine));
        } else {
            self.post(format!("● a draw with {name}, {} each", f.mine));
        }
    }

    /// Someone here singing: notes rising over them.
    fn update_carols(&mut self, t: f32) {
        let mut keep = vec![];
        for (id, until, mut next) in std::mem::take(&mut self.social.carols) {
            if t > until {
                continue;
            }
            if t >= next {
                next = t + 0.55;
                if let Some(o) = self.online.other(id).filter(|o| o.av.away == 0) {
                    let (x, y) = self.local(o.shown.0, o.shown.1);
                    let k = (t * 3.0) as usize % 2;
                    self.director.emote(x, y - 4.5, ['♪', '♫'][k], Rgb::hex(0xffd36b), 1);
                }
            }
            keep.push((id, until, next));
        }
        self.social.carols = keep;
    }

    /// A light show (0), fireworks (1) or a carol (2) the villager's begun:
    /// everyone in the shared village sees it too.
    pub(super) fn share_show(&mut self, what: u8, n: u16) {
        if self.online.village().is_none() {
            return;
        }
        let x = self.player.as_ref().map(|p| p.centre()).unwrap_or(self.cam + self.w as f32 / 2.0);
        let (sx, _) = self.shared(x, 0.0);
        self.online.show(what, n, sx);
    }

    /// Off up the mountain in a shared village: everyone's asked along.
    pub(super) fn share_trip(&mut self) {
        if self.online.village().is_some() {
            self.online.ask(0, ask::TRIP, 0);
        }
    }

    /// A race finished (or the trip's friends to show): to the others.
    pub(super) fn update_trip_online(&mut self) {
        let fellows: Vec<trips::Fellow> = self
            .online
            .others()
            .iter()
            .filter_map(|(id, o)| {
                // (on the slope: drawn a moment behind, smoothly)
                let at = self.online.trip_shown(o).filter(|_| o.av.away == 2)?;
                Some(trips::Fellow { name: o.name.clone(), look: look_of(&o.av, *id), at })
            })
            .collect();
        let done = match &mut self.trip {
            Some(tr) => {
                tr.fellows = fellows;
                tr.finished.take()
            }
            None => None,
        };
        if let Some((score, secs)) = done {
            self.online.finish(score, secs);
        }
        // a race this villager asked for: once everyone who said yes is at
        // the top (25 s at most), go, a moment from now, for everyone
        let t = self.now();
        if let (Some(race), Some(tr)) = (&mut self.social.race, &mut self.trip) {
            if !race.went && tr.race && tr.seed == race.seed && tr.gate.is_some() {
                let since = tr.gate.unwrap_or(t);
                let up = race.racers.iter().all(|id| self.online.other(*id).and_then(|o| o.av.trip).is_some_and(|at| at.at == 7 || at.at == 5));
                if up || t - since > 25.0 {
                    race.went = true;
                    let at = crate::clock::now().timestamp_millis() + 3200;
                    self.online.go(race.seed, at);
                    tr.go(t + 3.2);
                }
            }
        }
    }

    /// Answer what someone asked (`i` of them): yes, or no thanks.
    fn answer_asked(&mut self, i: usize, yes: bool) {
        if i >= self.social.asked.len() {
            return;
        }
        let a = self.social.asked.remove(i);
        let t = self.now();
        self.online.answer(a.id, a.what, yes);
        if let Some((key, value)) = a.rule {
            if yes {
                self.post(format!("⚙ {}", Rules::words(&key, &value)));
                self.social.front.push(Front::SetRule(key, value));
            }
            return;
        }
        if !yes {
            return;
        }
        if a.what != ask::RACE {
            self.social.yes_again.push((a.id, a.what, t + 2.0, t + 7.0));
        }
        match a.what {
            ask::TAG => {} // (the asker starts it, and says who's it)
            ask::FIGHT => self.start_net_fight(a.id, t),
            ask::RACE => {
                let left = (a.until - ASK_TIME + RACE_WAIT - t).max(3.0);
                self.goal = None;
                self.soon(play::Pending::Race(a.seed, t + left));
            }
            ask::TRIP => self.soon(play::Pending::Join(a.id)),
            // everyone asked: sat down at their table, waiting for the deal
            k if a.to == 0 => self.start_table(k, a.seed, vec![], vec![], a.seed, true),
            k => {
                let me = self.online.my_id();
                let mine = self.your_name().to_string();
                self.start_net_game(k, a.seed, vec![a.id, me], vec![a.name.clone(), mine], a.seed);
            }
        }
    }

    /// Y or N with a question up: the newest answered (true if there was
    /// one).
    pub fn prompt_key(&mut self, yes: bool) -> bool {
        if self.social.asked.is_empty() {
            return false;
        }
        let i = self.social.asked.len() - 1;
        self.answer_asked(i, yes);
        true
    }

    /// A click on the question box (yes / no) or the post (true: taken;
    /// `Some(true)` from `post_click`: open the online menu).
    pub fn prompt_click(&mut self, x: i32, y: i32) -> bool {
        let Some((x0, y0, w, yes_x, no_x)) = self.social.prompt_at.get() else { return false };
        if y < y0 || y > y0 + 2 || x < x0 || x >= x0 + w {
            return false;
        }
        let i = self.social.asked.len().saturating_sub(1);
        if y == y0 + 1 && x >= yes_x && x < yes_x + 9 {
            self.answer_asked(i, true);
        } else if y == y0 + 1 && x >= no_x && x < no_x + 12 {
            self.answer_asked(i, false);
        }
        true
    }

    /// A click on the post (top left): the online menu, the front end opens.
    pub fn post_click(&self, x: i32, y: i32) -> bool {
        self.social.post_at.get().is_some_and(|(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h)
    }

    /// A question's up (Y and N answer it).
    pub fn asked_now(&self) -> bool {
        !self.social.asked.is_empty()
    }

    pub fn close_card(&mut self) -> bool {
        self.social.card.take().is_some()
    }

    /// Someone here under (x, y) (world cells): their id.
    pub(super) fn other_at(&self, x: i32, y: i32) -> Option<u32> {
        if self.inside.is_some() || self.trip.is_some() {
            return None;
        }
        let k = sprite_k(self.h);
        self.online.others().iter().filter(|(_, o)| o.av.away == 0).find_map(|(id, o)| {
            let (ox, oy) = self.local(o.shown.0, o.shown.1);
            ((x as f32 - ox).abs() <= 2.0 * k && (y as f32) <= oy + 0.5 && (y as f32) >= oy - 5.0 * k).then_some(*id)
        })
    }

    /// Previews (--perch online, card, netchess, netcards).
    pub(super) fn preview_online(&mut self, what: &str) {
        let Some((px, py)) = self.player.as_ref().map(|p| (p.centre(), p.y)) else { return };
        let (sx, sy) = self.shared(px, py);
        let u = self.anchor.2.max(0.01);
        let av = |dx: f32, coat: u32, hat: u32, hair: u32, skin: u32| Avatar { x: sx + dx / u, y: sy, dir: -1.0, coat, hat, hair, skin, scarf: hat, ..Default::default() };
        let sam = Card { name: "Sam".into(), motto: "first down the mountain, last to bed".into(), pet: "a dog called Pudding".into(), look: [0x2a4c8a, 0xd8283a, 0x6a4a2a, 0xf0d0b0, 0xd8283a], won: 7, runs: 12, best: 2840, fights: 3, cheer: 64, friends: 4 };
        self.online.pretend(vec![(11, "Sam", av(9.0, 0x2a4c8a, 0xd8283a, 0x6a4a2a, 0xf0d0b0), Some(sam.clone())), (12, "Alex", av(-14.0, 0x6a3a8a, 0, 0xc89a5a, 0xc89a72), None)]);
        self.post("✉ online");
        self.post("⌂ Sam is here");
        self.post("✦ Alex waves at you");
        self.online.pretend_friends(&[("Sam", true, false), ("Alex", true, true), ("Kim", false, false), ("Jo", false, false)]);
        self.log_line("#carols Kim: anyone singing tonight?".into(), true);
        self.log_line("✉ Alex: come and see the lights!".into(), true);
        self.log_line("☺ Sam: race you down the mountain".into(), true);
        match what {
            "gamesroom" => {
                // round the table with Sam and Alex, Twenty-One dealt
                let me = self.online.my_id();
                let mine = self.your_name().to_string();
                let game = 4242;
                let sat = |id: u32, name: &str, seat: i8, coat: u32, hat: u32| (id, name.to_string(), Avatar { coat, hat, hair: 0x6a4a2a, skin: 0xf0d0b0, scarf: hat, away: 1, room: game, seat, ..Default::default() });
                let others = [sat(11, "Sam", 1, 0x2a4c8a, 0xd8283a), sat(12, "Alex", 2, 0x6a3a8a, 0xffc23a)];
                self.online.pretend(others.iter().map(|(i, n, a)| (*i, n.as_str(), *a, None)).collect());
                self.start_net_game(ask::TWENTY_ONE, game, vec![me, 11, 12], vec![mine, "Sam".into(), "Alex".into()], 6);
                self.pending = None;
                self.iris = None;
                self.enter_games_room(game);
                if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
                    if let Some(x) = inn.seat_x(0) {
                        p.x = x - 1.0;
                    }
                    inn.take_seat(0, p);
                }
                self.social.to_seat = false;
                self.social.panel_at = 0.0;
            }
            "card" => self.social.card = Some((sam, None)),
            "chats" => {
                self.online.pretend_chat(0, &[(false, "race you down the mountain after dinner?", 3600), (true, "you're on! the black run this time", 3500), (false, "ha. bring cocoa for the loser", 3400), (false, "also the lights on the square are up, come and see", 120)]);
                self.online.pretend_chat(1, &[(false, "come and see the lights!", 40)]);
                self.online.book.asking.push(net::Pal { name: "Robin".into(), code: "PRVWASKS0000".into(), waiting: false, quiet: false });
                self.open_chats(Some(0));
            }
            "netchess" | "netcards" => {
                let k = if what == "netchess" { ask::CHESS } else { ask::SNOWFLAKES };
                let me = self.online.my_id();
                let mine = self.your_name().to_string();
                self.start_net_game(k, 5, vec![me, 11], vec![mine, "Sam".into()], 5);
                if let Some(tb) = &mut self.table {
                    for key in [games::Key::Pick, games::Key::Right, games::Key::Pick] {
                        tb.key(key, 0.0);
                    }
                }
            }
            _ => {
                let t = self.now();
                self.social.asked.push(Asked { id: 11, name: "Sam".into(), what: ask::CHESS, seed: 1, until: t + ASK_TIME, to: 1, rule: None });
                self.social.tag = Some(NetTag { other: 12, it: 12, grace: 0.0, until: t + 60.0, tags: 2 });
            }
        }
    }

    // ------------------------------------------------------------ drawing

    /// The others in a shared village, with their names over them.
    pub(super) fn draw_others(&self, f: &mut Frame, t: f32, view: Option<View>) {
        if self.inside.is_some() || self.trip.is_some() {
            return;
        }
        let it = self.social.tag.map(|g| g.it);
        for (id, o) in self.online.others() {
            if o.av.away != 0 {
                continue;
            }
            let (wx, wy) = self.local(o.shown.0, o.shown.1);
            let (x, y, k) = match view {
                Some(v) => (v.x(wx), v.y(wy + 1.0) - 1.0, sprite_k(self.h) * v.z),
                None => (wx, wy, sprite_k(self.h)),
            };
            // (paused at their end: greyed, standing still; otherwise as
            // they were where they're drawn)
            let (air, moving, dir) = self.online.shown_as(o);
            let look = if o.av.rest { greyed(look_of(&o.av, id)) } else { look_of(&o.av, id) };
            let pose = if o.av.rest {
                sprites::Pose::Stand
            } else if air {
                sprites::Pose::Jump
            } else if moving {
                sprites::Pose::Walk(((t * 6.0) as i32 % 2) as u8)
            } else if t < o.wave.0 {
                if o.wave.1 == 1 { sprites::Pose::ArmsUp } else { sprites::Pose::Wave }
            } else {
                sprites::Pose::Stand
            };
            let pic = sprites::person(&look, pose, dir, 0.9 + 0.25 * f.day.max(0.0));
            let rows = sprites::rows_of(&pic, k);
            // their pets, one behind the other (drawn first: behind them)
            let ps = critters::pet_scale(k);
            for (j, &code) in o.av.pets.iter().filter(|c| **c >> 8 != 0).enumerate() {
                let Some(&kind) = pets::KINDS.get(((code >> 8) - 1) as usize) else { continue };
                let coats = kind.coats();
                let (_, body, marks) = coats.get((code & 0xff) as usize).copied().unwrap_or(coats[0]);
                let (body, marks) = if o.av.rest { (Rgb::hex(body) * 0.55, Rgb::hex(marks) * 0.55) } else { (Rgb::hex(body), Rgb::hex(marks)) };
                let px = x - dir * (3.5 + 3.5 * j as f32) * k;
                let pose = if o.av.rest { pets::Pose::Sit } else if moving { pets::Pose::Walk } else { pets::Pose::Sit };
                pets::draw(f, kind, body, marks, px, (y * 2.0).round() as i32 + 1, dir, pose, t + j as f32 + id as f32, ps as f32, Some(Rgb::hex(0xc8202a)));
            }
            f.sprite(x, y + 1.0, rows, pic);
            // the name, over their head ("it" at tag)
            let mark = if it == Some(id) { " ⚑ it!" } else if o.av.rest { " ⏸" } else { "" };
            let name = format!(" {}{mark} ", o.name);
            let n = name.chars().count() as i32;
            let top = (y + 1.0 - rows).floor() as i32 - 1;
            let c = if o.av.rest { Rgb::hex(0xa8a8b0) } else { Rgb::hex(0xffe9a8) };
            f.text(x.round() as i32 - n / 2, top, &name, c, crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.7), 0.0);
        }
        // the villager "it" at tag: marked too
        if let (Some(g), Some(p)) = (self.social.tag, self.player.as_ref()) {
            if g.it == self.online.my_id() {
                let (x, y) = match view {
                    Some(v) => (v.x(p.centre()), v.y(p.y + 1.0) - 1.0),
                    None => (p.centre(), p.y),
                };
                let k = sprite_k(self.h) * view.map(|v| v.z).unwrap_or(1.0);
                f.text(x.round() as i32 - 3, (y - 5.5 * k).floor() as i32, " ⚑ it! ", Rgb::hex(0xffd36b), crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.7), 0.0);
            }
        }
    }

    /// The post: the last few things that happened online, bottom left (a
    /// click on it opens friends & online).
    fn draw_post(&self, f: &mut Frame) {
        let t = self.now();
        let n = self.online.wanted() as i32 + self.online.village().is_some() as i32 + self.social.tag.is_some() as i32 + self.social.fight.is_some() as i32 + self.post.len() as i32;
        let (x0, mut y) = (2, if self.social.post_top { 2 } else { (f.h as i32 - 4 - n).max(1) });
        let top = y;
        let mut wide = 0;
        let link = self.online.link();
        let bg = crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.8);
        if self.online.wanted() {
            let s = if self.online.too_old() {
                " ✉ offline: update xmas-tui to play online ".to_string()
            } else if link != Link::On {
                " ✉ connecting… ".to_string()
            } else {
                let about = self.online.friends().iter().filter(|(_, h)| h.as_ref().is_some_and(|h| h.on)).count();
                match about {
                    0 => " ✉ online ".to_string(),
                    1 => " ✉ online · a friend about ".to_string(),
                    n => format!(" ✉ online · {n} friends about "),
                }
            };
            let c = if link == Link::On { Rgb::hex(0x9be08a) } else { crate::color::pal::UI_DIM };
            wide = wide.max(f.text(x0, y, &s, c, bg, 0.0) - x0);
            y += 1;
        }
        if let Some((code, host)) = self.online.village() {
            let n = self.online.others().len();
            let s = match (self.online.square(), self.online.headcount()) {
                (Some((name, _)), Some((n, most))) => format!(" ⌂ {name} · {n} of {most} here "),
                _ if host => format!(" ⌂ village open: {code} · {} here ", n + 1),
                _ => format!(" ⌂ a shared village · {} here ", n + 1),
            };
            wide = wide.max(f.text(x0, y, &s, Rgb::hex(0xffd36b), bg, 0.0) - x0);
            y += 1;
        }
        if let Some(g) = self.social.tag {
            let me = g.it == self.online.my_id();
            let s = format!(" ⚑ tag with {} · {} · {:.0}s ", self.other_name(g.other), if me { "you're it!" } else { "run!" }, (g.until - t).max(0.0));
            wide = wide.max(f.text(x0, y, &s, Rgb::hex(0xffd36b), bg, 0.0) - x0);
            y += 1;
        }
        if let Some(fi) = self.social.fight {
            let s = format!(" ● snowball fight with {} · you {} · them {} · {:.0}s ", self.other_name(fi.other), fi.mine, fi.theirs, (fi.until - t).max(0.0));
            wide = wide.max(f.text(x0, y, &s, Rgb::hex(0xffd36b), bg, 0.0) - x0);
            y += 1;
        }
        for (at, line) in &self.post {
            let a = (1.0 - ((t - at) - (POST_TIME - 2.0)).max(0.0) / 2.0).clamp(0.0, 1.0);
            let w = (self.w as usize).saturating_sub(6).max(10);
            let line: String = line.chars().take(w).collect();
            wide = wide.max(f.text(x0, y, &format!(" {line} "), crate::color::pal::UI_TEXT * a, crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.8 * a), 0.0) - x0);
            y += 1;
        }
        self.social.post_at.set((y > top).then_some((x0, top, wide, y - top)));
    }

    /// Over everything (the widgets too; the front end calls it last): the
    /// post, a question asked of the villager, a profile card, a game with
    /// a friend out in the village.
    pub fn draw_top(&self, f: &mut Frame) {
        let (ox, oy, cover) = (f.ox, f.oy, f.cover_pictures);
        (f.ox, f.oy) = (0, 0);
        f.cover_pictures = true;
        if self.trip.is_none() && self.look.is_none() && !self.chats_focused() {
            self.draw_post(f);
        }
        if let Some(tb) = self.table.as_ref().filter(|_| self.table_shows()) {
            tb.draw(f, self.ui_look, self.now());
        }
        self.draw_social(f);
        self.draw_chats(f);
        (f.ox, f.oy, f.cover_pictures) = (ox, oy, cover);
    }

    /// Questions asked of the villager (top middle, yes / no), and a
    /// profile card on show.
    fn draw_social(&self, f: &mut Frame) {
        let bg = crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.95);
        self.social.prompt_at.set(None);
        if let Some(a) = self.social.asked.last() {
            let t = self.now();
            let q = match &a.rule {
                Some((k, v)) => format!("⚙ {} asks: {}?", a.name, Rules::words(k, v)),
                None => format!("{} {} asks: {}?", ask_icon(a.what), a.name, ask_words(a.what)),
            };
            let w = (q.chars().count() as i32 + 6).max(36);
            let (x0, y0) = ((f.w as i32 - w) / 2, 1);
            for y in y0..y0 + 3 {
                for x in x0..x0 + w {
                    f.put_g(x, y, crate::frame::Glyph::new(' ', crate::color::pal::UI_TEXT).bg(bg).lit(0.0));
                }
            }
            f.text(x0 + 3, y0, &q, Rgb::hex(0xffd36b), bg, 0.0);
            let yes_x = x0 + 3;
            let no_x = yes_x + 11;
            let hot = |x: i32, n: i32| self.cursor.is_some_and(|(cx, cy)| cy as i32 == y0 + 1 && (cx as i32) >= x && (cx as i32) < x + n);
            let b = |h: bool| crate::frame::Bg::Mix(if h { Rgb::hex(0x34406a) } else { Rgb::hex(0x24304a) }, 0.95);
            f.text(yes_x, y0 + 1, " ✓ Y yes ", Rgb::hex(0x9be08a), b(hot(yes_x, 9)), 0.0);
            f.text(no_x, y0 + 1, " ✕ N no thanks ", crate::color::pal::UI_TEXT, b(hot(no_x, 15)), 0.0);
            let left = (a.until - t).max(0.0);
            f.text(x0 + w - 6, y0 + 1, &format!("{left:>3.0}s"), crate::color::pal::UI_DIM, bg, 0.0);
            if self.social.asked.len() > 1 {
                f.text(x0 + 3, y0 + 2, &format!("(and {} more: ✉ friends)", self.social.asked.len() - 1), crate::color::pal::UI_DIM, bg, 0.0);
            }
            crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h: 3 }, self.ui_look, bg, 23);
            self.social.prompt_at.set(Some((x0, y0, w, yes_x, no_x)));
        }
        if let Some((c, code)) = &self.social.card {
            draw_card(f, c, code.as_deref(), self.ui_look);
        }
    }

    // ------------------------------------------------------------ menus

    /// The friends & online menu.
    pub fn online_menu(&self) -> (String, Vec<(String, Do)>) {
        let o = &self.online;
        let mut v: Vec<(String, Do)> = vec![];
        // (the friends menu open: how the public villages are, soon wanted)
        self.social.want_public.set(true);
        if !o.wanted() {
            v.push(("● go online: friends, messages, shared villages".into(), Do::Net(NetDo::On)));
            v.push(("⌂ public villages: drop in, anyone's welcome ▸".into(), Do::More(MORE_PUBLIC)));
            v.push(("★ my profile card".into(), Do::Net(NetDo::MyCard)));
            v.push(("⇄ my devices (one villager on all of them) ▸".into(), Do::More(MORE_DEVICES)));
            v.push(("☺ villagers on this device ▸".into(), Do::More(MORE_VILLAGERS)));
            return ("friends & online".into(), v);
        }
        let title = match (o.link(), o.headcount(), o.square()) {
            (Link::On, Some((n, most)), Some((name, _))) => format!("{name} · {n} of {most} here"),
            (Link::On, Some((n, most)), None) => format!("friends & online ● · {n} of {most} here"),
            (Link::On, None, _) => "friends & online ●".to_string(),
            _ if o.too_old() => "friends & online (this version's too old: update to play online)".to_string(),
            _ => "friends & online (connecting…)".to_string(),
        };
        // asked: yes or no
        for (i, a) in self.social.asked.iter().enumerate().rev().take(3) {
            v.push((format!("{} {}: {}? yes!", ask_icon(a.what), a.name, ask_words(a.what)), Do::Net(NetDo::Yes(i as u8))));
            v.push((format!("   no thanks, {}", a.name), Do::Net(NetDo::No(i as u8))));
        }
        for (i, (name, _)) in self.social.invited.iter().enumerate().take(3) {
            v.push((format!("⌂ into {name}'s village (they asked you)"), Do::Net(NetDo::GoInvited(i as u8))));
        }
        for (i, p) in o.book.asking.iter().enumerate().take(4) {
            v.push((format!("✦ {} asks to be friends: yes!", p.name), Do::Net(NetDo::Accept(i as u8))));
            v.push((format!("   no thanks to {}", p.name), Do::Net(NetDo::Decline(i as u8))));
        }
        if !o.others().is_empty() {
            v.push((format!("☺ with the {} here: play, wave, cards ▸", o.others().len() + 1), Do::More(MORE_PEOPLE)));
        }
        if o.village().is_none() {
            v.push(("⌂ public villages: drop in, anyone's welcome ▸".into(), Do::More(MORE_PUBLIC)));
        }
        v.push(("✎ talk… (/ · @name: to one person)".into(), Do::Net(NetDo::Chat)));
        let unread = self.social.unread;
        v.push((if unread > 0 { format!("✉ messages ({unread} new) ▸") } else { "✉ messages ▸".into() }, Do::More(MORE_MESSAGES)));
        let fr = o.friends();
        let about = fr.iter().filter(|(p, h)| !p.waiting && h.as_ref().is_some_and(|h| h.on)).count();
        let n = fr.iter().filter(|(p, _)| !p.waiting).count();
        let friends = match (n, about) {
            (0, _) => "★ friends & chats: add one".to_string(),
            (n, 0) => format!("★ friends & chats ({n})"),
            (n, a) => format!("★ friends & chats ({n} · {a} online)"),
        };
        v.push((friends, Do::Net(NetDo::Friends)));
        let groups = o.book.groups.len();
        v.push((if groups == 0 { "# groups (chat rooms shared by code) ▸".to_string() } else { format!("# groups ({groups}) ▸") }, Do::More(MORE_GROUPS)));
        match o.village() {
            Some((code, true)) => {
                v.push((format!("⌂ this village is open: {code} (copy)"), Do::Net(NetDo::VillageCode)));
                v.push(("✎ say something to everyone…".into(), Do::Net(NetDo::Say)));
                v.push(("⌂ close the village".into(), Do::Net(NetDo::Close)));
            }
            Some((_, false)) => {
                v.push(("✎ say something to everyone…".into(), Do::Net(NetDo::Say)));
                v.push(("⌂ home again".into(), Do::Net(NetDo::Leave)));
                v.push(("⌂ public villages ▸".into(), Do::More(MORE_PUBLIC)));
            }
            None => {
                v.push(("⌂ open this village to friends (play together)".into(), Do::Net(NetDo::Host)));
                v.push(("⌂ into a village by its code…".into(), Do::Net(NetDo::JoinCode)));
            }
        }
        v.push(("★ my profile card".into(), Do::Net(NetDo::MyCard)));
        v.push(("✎ the line on my card…".into(), Do::Net(NetDo::Motto)));
        v.push(("⇄ my devices (one villager on all of them) ▸".into(), Do::More(MORE_DEVICES)));
        v.push(("☺ villagers on this device ▸".into(), Do::More(MORE_VILLAGERS)));
        v.push(("○ go offline".into(), Do::Net(NetDo::Off)));
        (title, v)
    }

    /// Friends: each with their own menu; adding one; the friend code.
    fn friends_menu(&self) -> (String, Vec<(String, Do)>) {
        let o = &self.online;
        let mut v: Vec<(String, Do)> = vec![];
        for (i, p) in o.book.asking.iter().enumerate().take(4) {
            v.push((format!("✦ {} asks to be friends: yes!", p.name), Do::Net(NetDo::Accept(i as u8))));
            v.push((format!("   no thanks to {}", p.name), Do::Net(NetDo::Decline(i as u8))));
        }
        // online first, then the rest
        let mut fr: Vec<(usize, (net::Pal, Option<net::Here>))> = o.friends().into_iter().enumerate().take(32).collect();
        fr.sort_by_key(|(_, (p, h))| (p.waiting, !h.as_ref().is_some_and(|h| h.on), p.name.to_lowercase()));
        for (i, (p, here)) in fr {
            let mark = match &here {
                _ if p.waiting => "…",
                Some(h) if h.village.is_some() => "⌂",
                Some(h) if h.on => "●",
                _ => "○",
            };
            let what = match &here {
                _ if p.waiting => " (asked)".to_string(),
                Some(h) if h.village.is_some() => " (village open)".to_string(),
                Some(h) if h.on => " (online)".to_string(),
                _ => String::new(),
            };
            v.push((format!("{mark} {}{what} ▸", p.name), Do::More(MORE_FRIEND + i as u8)));
        }
        v.push(("✚ add a friend (their code)…".into(), Do::Net(NetDo::Add)));
        v.push((format!("my friend code: {} (copy)", o.code()), Do::Net(NetDo::Code)));
        ("friends".into(), v)
    }

    /// Groups: chat rooms anyone with the code can be in.
    fn groups_menu(&self) -> (String, Vec<(String, Do)>) {
        let o = &self.online;
        let mut v: Vec<(String, Do)> = vec![];
        for (i, g) in o.book.groups.iter().enumerate().take(12) {
            let here = o.group_here(i).len();
            let s = if here > 0 { format!("# {} ({here} about) ▸", g.name) } else { format!("# {} ▸", g.name) };
            v.push((s, Do::More(MORE_GROUP + i as u8)));
        }
        v.push(("✚ a new group…".into(), Do::Net(NetDo::GroupNew)));
        v.push(("⇥ into a group by its code…".into(), Do::Net(NetDo::GroupJoin)));
        ("groups".into(), v)
    }

    fn group_menu(&self, i: usize) -> (String, Vec<(String, Do)>) {
        let o = &self.online;
        let Some(g) = o.book.groups.get(i) else { return self.groups_menu() };
        let mut v: Vec<(String, Do)> = vec![(format!("✎ say something in #{}…", g.name), Do::Net(NetDo::GroupSay(i as u8)))];
        let here = o.group_here(i);
        if !here.is_empty() {
            let mut names = here.join(", ");
            if names.chars().count() > 40 {
                names = names.chars().take(38).collect::<String>() + "…";
            }
            v.push((format!("☺ about lately: {names}"), Do::More(MORE_GROUP + i as u8)));
        }
        v.push((format!("its code: {} (copy, to ask someone in)", crate::net::seal::pretty(&g.code)), Do::Net(NetDo::GroupCode(i as u8))));
        v.push(("✕ leave the group".into(), Do::Net(NetDo::GroupLeave(i as u8))));
        (format!("# {}", g.name), v)
    }

    /// The messages: the last few dozen, newest first.
    /// Public villages: the squares (anyone can walk in; always there), and
    /// villages their hosts have listed; this villager's own, listed or not.
    fn public_menu(&self) -> (String, Vec<(String, Do)>) {
        self.social.want_public.set(true);
        let o = &self.online;
        let mut v: Vec<(String, Do)> = vec![];
        let on = o.link() == Link::On;
        for (i, (name, n)) in o.squares().into_iter().enumerate() {
            let here = match (on, n) {
                (false, _) => "anyone's welcome".to_string(),
                (true, 0) => "nobody yet: be the first".to_string(),
                (true, 1) => "1 here".to_string(),
                (true, n) => format!("{n} here"),
            };
            v.push((format!("⌂ {name} · {here}"), Do::Net(NetDo::Square(i as u8))));
        }
        let listed = o.listings();
        if !listed.is_empty() {
            for (k, l) in listed.iter().enumerate().take(12) {
                let full = l.n >= l.limit;
                // (★ featured by the relay; a server village: kept open by it)
                let star = if o.featured(l) { "★ " } else { "" };
                let server = if l.server { " · server" } else { "" };
                v.push((format!("{star}⌂ {} · {} of {}{server}{}", l.name, l.n, l.limit, if full { " (full)" } else { "" }), Do::Net(NetDo::Listed(k as u8))));
            }
        } else if on {
            v.push(("(no villagers' own villages listed right now)".into(), Do::More(MORE_PUBLIC)));
        }
        if o.village().is_some_and(|(_, host)| host) && o.square().is_none() {
            let mine = o.book.listed;
            v.push((if mine { "☑ my village is listed here for anyone".into() } else { "☐ list my village here for anyone".into() }, Do::Net(NetDo::ListMine(!mine))));
        } else if o.village().is_none() {
            v.push(("⌂ open my village (and list it here for anyone)".into(), Do::Net(NetDo::ListMine(true))));
        }
        v.push(("↻ look again".into(), Do::More(MORE_PUBLIC)));
        ("public villages".into(), v)
    }

    fn messages_menu(&self) -> (String, Vec<(String, Do)>) {
        let mut v: Vec<(String, Do)> = vec![("✎ talk… (/ · @name: to one person)".into(), Do::Net(NetDo::Chat))];
        let now = crate::clock::now().timestamp();
        for (k, l) in self.social.log.iter().rev().take(18).enumerate() {
            let mut s: String = l.text.chars().take(58).collect();
            if l.text.chars().count() > 58 {
                s.push('…');
            }
            v.push((format!("{s} · {}", ago_short(now - l.at)), Do::Net(NetDo::Messages)));
            // (a link in it: hidden, unless asked for)
            if !l.links.is_empty() {
                v.push((format!("   🔗 show the link{} (copied too)", if l.links.len() > 1 { "s" } else { "" }), Do::Net(NetDo::ShowLink(k as u8))));
            }
        }
        if self.social.log.is_empty() {
            v.push(("(nothing yet)".into(), Do::Net(NetDo::Messages)));
        }
        ("messages".into(), v)
    }

    /// One friend's menu.
    /// A chat with friend `i`: its title, then the last few lines (each
    /// on a line of its own), for the box it's typed in.
    pub fn chat_prompt(&self, i: usize) -> String {
        let name = self.online.book.friends.get(i).map(|p| p.name.clone()).unwrap_or_default();
        // (the chat's on show in friends & chats: only who it's to)
        if self.chats_shown() {
            return format!("✉ to {name}");
        }
        let lines = self.online.chat(i);
        let mut s = format!("✉ chat with {name}");
        if lines.is_empty() {
            s.push_str("\n  (nothing yet: say hello!)");
        }
        for l in lines.iter().rev().take(8).rev() {
            let who = if l.mine { "you".to_string() } else { name.clone() };
            let text: String = l.text.chars().take(72).collect();
            s.push_str(&format!("\n  {who}: {text}"));
        }
        s
    }

    pub fn friend_menu(&self, i: usize) -> (String, Vec<(String, Do)>) {
        let fr = self.online.friends();
        let Some((p, here)) = fr.get(i) else { return ("friends".into(), vec![]) };
        let mut v = vec![];
        if !p.waiting {
            v.push((format!("✉ chat with {}", p.name), Do::Net(NetDo::ChatWith(i as u8))));
            v.push((format!("♪ ping {} (a little tone)", p.name), Do::Net(NetDo::Ping(i as u8))));
        }
        if here.as_ref().is_some_and(|h| h.village.is_some()) {
            v.push((format!("⌂ into {}'s village", p.name), Do::Net(NetDo::Join(i as u8))));
        }
        if !p.waiting && self.online.village().is_some_and(|(_, host)| host) {
            v.push((format!("⌂ ask {} into this village", p.name), Do::Net(NetDo::Invite(i as u8))));
        } else if !p.waiting && self.online.village().is_none() {
            v.push((format!("⌂ open this village and ask {} in", p.name), Do::Net(NetDo::Invite(i as u8))));
        }
        if here.as_ref().is_some_and(|h| h.card.is_some()) {
            v.push((format!("★ {}'s profile card", p.name), Do::Net(NetDo::FriendCard(i as u8))));
        }
        v.push((format!("✎ rename {} (just here)", p.name), Do::Net(NetDo::Rename(i as u8))));
        v.push((if p.quiet { format!("♪ let {}'s messages chime again", p.name) } else { format!("⊘ quiet: no chime for {}'s messages", p.name) }, Do::Net(NetDo::Quiet(i as u8))));
        if !p.waiting && !self.social.chats.friend_pins.iter().any(|f| f.0 == crate::net::seal::norm(&p.code)) {
            v.push((format!("▣ pin {}'s chat over the village", p.name), Do::Net(NetDo::PinFriend(i as u8))));
        }
        if !self.online.chat(i).is_empty() {
            v.push((format!("⌫ clear the chat with {} (on this device)", p.name), Do::Net(NetDo::ClearChat(i as u8))));
        }
        v.push((format!("✕ unfriend {}", p.name), Do::Net(NetDo::Unfriend(i as u8))));
        (p.name.clone(), v)
    }

    /// Everyone here in the shared village.
    fn people_menu(&self) -> (String, Vec<(String, Do)>) {
        let mut v: Vec<(String, Do)> = vec![];
        for (k, (_, o)) in self.online.others().iter().enumerate().take(30) {
            let where_ = match (o.av.away, o.av.trip.map(|tr| tr.at)) {
                (2, Some(1)) => " (on the shuttle)",
                (2, Some(2)) => " (on the train)",
                (2, Some(3 | 4)) => " (up at the ski lodge)",
                (2, Some(5)) => " (skiing)",
                (2, _) => " (away)",
                (1, _) => " (indoors)",
                _ => "",
            };
            v.push((format!("☺ {}{where_} ▸", o.name), Do::More(MORE_OTHER + k as u8)));
        }
        v.push(("✦ wave to everyone".into(), Do::Net(NetDo::Wave(0, 0))));
        v.push(("★ a cheer for everyone".into(), Do::Net(NetDo::Wave(0, 1))));
        v.push(("♠ Twenty-One round the table (everyone asked)".into(), Do::Net(NetDo::WithAll(ask::TWENTY_ONE))));
        v.push(("❄ Snowflakes round the table (everyone asked)".into(), Do::Net(NetDo::WithAll(ask::SNOWFLAKES))));
        v.push(("⇘ a race down the mountain (everyone asked)".into(), Do::Net(NetDo::Race)));
        v.push(("✎ say something to everyone…".into(), Do::Net(NetDo::Say)));
        ("everyone here".into(), v)
    }

    /// Someone here's menu (by their place in `others`).
    fn other_menu(&self, k: usize) -> (String, Vec<(String, Do)>) {
        let others = self.online.others();
        let Some(&(id, o)) = others.get(k) else { return ("everyone here".into(), vec![]) };
        let n = &o.name;
        let mut v: Vec<(String, Do)> = vec![];
        if let Some(tr) = o.av.trip.filter(|tr| o.av.away == 2 && (1..=5).contains(&tr.at)) {
            let where_ = match tr.at {
                1 => "on the shuttle",
                2 => "on the train",
                5 => "on the slopes",
                _ => "at the ski lodge",
            };
            v.push((format!("⇄ go and join {n} ({where_})"), Do::Net(NetDo::JoinTrip(id))));
        }
        v.push((format!("✦ wave to {n}"), Do::Net(NetDo::Wave(id, 0))));
        v.push((format!("✉ say something just to {n}…"), Do::Net(NetDo::Whisper(id))));
        v.push((format!("♥ send {n} a heart"), Do::Net(NetDo::Wave(id, 3))));
        for kind in games::Kind::ALL {
            v.push((format!("{} {} with {n}", kind.icon(), kind.name()), Do::Net(NetDo::With(id, kind.code()))));
        }
        v.push((format!("⚑ tag with {n}"), Do::Net(NetDo::With(id, ask::TAG))));
        v.push((format!("● a snowball fight with {n}"), Do::Net(NetDo::With(id, ask::FIGHT))));
        v.push((format!("⇘ a race down the mountain with {n}"), Do::Net(NetDo::With(id, ask::RACE))));
        if o.card.is_some() {
            v.push((format!("★ {n}'s profile card"), Do::Net(NetDo::Card(id))));
        }
        if self.online.village().is_some_and(|(_, host)| host) {
            if self.online.is_admin(id) {
                v.push((format!("⚙ {n} may change the village's settings: stop that"), Do::Net(NetDo::Admin(id, false))));
            } else {
                v.push((format!("⚙ let {n} change the village's settings"), Do::Net(NetDo::Admin(id, true))));
            }
        }
        (n.clone(), v)
    }

    /// Linked devices: one villager on all of them.
    fn devices_menu(&self) -> (String, Vec<(String, Do)>) {
        let mut v: Vec<(String, Do)> = vec![];
        match &self.social.devices {
            Some((this, others)) => {
                v.push((format!("this one: {this} (linked)"), Do::More(MORE_DEVICES)));
                for (name, ago) in others.iter().take(8) {
                    v.push((format!("   {name} · {}", ago_words(*ago)), Do::More(MORE_DEVICES)));
                }
                v.push(("⇄ link another device: show the link code".into(), Do::Net(NetDo::LinkShow)));
                v.push(("✕ unlink this device".into(), Do::Net(NetDo::Unlink)));
            }
            None => {
                v.push(("⇄ link another device to this villager: show the link code".into(), Do::Net(NetDo::LinkShow)));
                v.push(("⇄ link this device to my villager elsewhere (type its code)…".into(), Do::Net(NetDo::LinkUse)));
            }
        }
        ("my devices".into(), v)
    }

    /// Other villagers kept on this device.
    fn villagers_menu(&self) -> (String, Vec<(String, Do)>) {
        let mut v: Vec<(String, Do)> = vec![(format!("this one: {}", self.your_name()), Do::More(MORE_VILLAGERS))];
        for (i, n) in self.social.villagers.iter().enumerate().take(12) {
            v.push((format!("☺ switch to {n}"), Do::Net(NetDo::Villager(i as u8))));
        }
        v.push(("✚ a new villager (their own village)…".into(), Do::Net(NetDo::NewVillager)));
        ("villagers on this device".into(), v)
    }

    /// The online menus behind a "▸" (see `more_menu`), if `k` is one.
    pub(super) fn online_more(&self, k: u8) -> Option<(String, Vec<(String, Do)>)> {
        let (title, mut v) = self.online_more_only(k)?;
        // each of the menus behind: the way back up first
        let back = match k {
            k if (MORE_GROUP..MORE_GROUP + 12).contains(&k) => Some(Do::More(MORE_GROUPS)),
            k if k >= MORE_OTHER => Some(Do::More(MORE_PEOPLE)),
            k if k >= MORE_FRIEND => Some(Do::Net(NetDo::ChatWith(k - MORE_FRIEND))),
            MORE_PEOPLE | MORE_DEVICES | MORE_VILLAGERS | MORE_FRIENDS | MORE_GROUPS | MORE_MESSAGES | MORE_PUBLIC => Some(Do::More(MORE_ONLINE)),
            _ => None,
        };
        if let Some(b) = back {
            v.insert(0, ("◂ back".into(), b));
        }
        Some((title, v))
    }

    fn online_more_only(&self, k: u8) -> Option<(String, Vec<(String, Do)>)> {
        Some(match k {
            MORE_ONLINE => self.online_menu(),
            MORE_PEOPLE => self.people_menu(),
            MORE_DEVICES => self.devices_menu(),
            MORE_VILLAGERS => self.villagers_menu(),
            MORE_FRIENDS => self.friends_menu(),
            MORE_GROUPS => self.groups_menu(),
            MORE_MESSAGES => self.messages_menu(),
            MORE_PUBLIC => self.public_menu(),
            k if (MORE_GROUP..MORE_GROUP + 12).contains(&k) => self.group_menu((k - MORE_GROUP) as usize),
            k if k >= MORE_OTHER => self.other_menu((k - MORE_OTHER) as usize),
            k if k >= MORE_FRIEND => self.friend_menu((k - MORE_FRIEND) as usize),
            _ => return None,
        })
    }

    /// Someone here, right-clicked: their menu.
    pub(super) fn other_menu_of(&self, id: u32) -> Option<(String, Vec<(String, Do)>)> {
        let k = self.online.others().iter().position(|(i, _)| *i == id)?;
        Some(self.other_menu(k))
    }

    pub(super) fn net_do(&mut self, n: NetDo) {
        let t = self.now();
        match n {
            NetDo::On => {
                self.online.go_online(true);
                self.post("✉ going online…");
            }
            NetDo::Off => {
                self.online.go_online(false);
                if self.online.guest_seed().is_some() || self.seed != self.own_seed {
                    self.village_switch = Some(None);
                }
                self.post("○ offline");
            }
            NetDo::Code => {
                self.copy_ask = Some(self.online.code());
                self.post(format!("★ friend code {} copied: give it to a friend", self.online.code()));
            }
            NetDo::VillageCode => {
                if let Some((code, _)) = self.online.village() {
                    self.copy_ask = Some(code.clone());
                    self.post(format!("⌂ village code {code} copied"));
                }
            }
            NetDo::Add => self.text_ask = Some((TextFor::Add, "a friend's code (XXXX-XXXX-XXXX)".into())),
            NetDo::Mail(i) => self.text_ask = Some((TextFor::Mail(i), self.chat_prompt(i as usize))),
            NetDo::Join(i) => {
                let code = self.online.friends().get(i as usize).and_then(|(_, h)| h.as_ref().and_then(|h| h.village.clone()));
                if let Some(code) = code {
                    if let Err(e) = self.online.join(&code) {
                        self.post(e);
                    }
                }
            }
            NetDo::Unfriend(i) => {
                let (name, code) = self.online.book.friends.get(i as usize).map(|p| (p.name.clone(), p.code.clone())).unwrap_or_default();
                self.online.remove_friend(i as usize);
                self.social.front.push(Front::Unfriended(code));
                self.post(format!("✕ {name} unfriended"));
            }
            NetDo::Accept(i) => self.online.accept(i as usize),
            NetDo::Decline(i) => self.online.decline(i as usize),
            NetDo::Host => {
                let code = self.online.host();
                self.copy_ask = Some(code.clone());
                self.post(format!("⌂ the village is open to friends: code {code} (copied) · ask them in from their name in ✉"));
            }
            NetDo::Close => {
                self.online.leave_village();
                self.social.front.push(Front::Apart);
                self.post("⌂ the village is closed");
            }
            NetDo::JoinCode => self.text_ask = Some((TextFor::JoinCode, "the village's code (XXXX-XXXX-XXXX)".into())),
            NetDo::Leave => {
                self.online.leave_village();
                self.social.front.push(Front::Apart);
            }
            NetDo::Say => self.text_ask = Some((TextFor::Say, "say something (everyone here hears it)".into())),
            NetDo::Invite(i) => {
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                if self.online.village().is_none() {
                    let code = self.online.host();
                    self.post(format!("⌂ the village is open to friends: code {code}"));
                }
                if self.online.invite(i as usize) {
                    self.post(format!("⌂ {name} is asked in"));
                } else {
                    self.post("not online yet");
                }
            }
            NetDo::GoInvited(i) => {
                if (i as usize) < self.social.invited.len() {
                    let (_, code) = self.social.invited.remove(i as usize);
                    if let Err(e) = self.online.join(&code) {
                        self.post(e);
                    }
                }
            }
            NetDo::With(id, what) => {
                let seed = self.rng.next_u32();
                self.online.ask(id, what, seed);
                self.social.asking = Some((id, what, seed, t + ASK_TIME));
                self.social.asked_again = t + 4.0;
                let name = self.other_name(id);
                if what == ask::RACE {
                    // (the race is on whether they come or not)
                    self.goal = None;
                    self.soon(play::Pending::Race(seed, t + RACE_WAIT));
                    self.post(format!("⇘ {name} is asked to race: up the chairlift…"));
                } else {
                    self.post(format!("{} {name} is asked: {}", ask_icon(what), ask_words(what)));
                }
            }
            NetDo::WithAll(what) => {
                let seed = self.rng.next_u32();
                self.online.ask(0, what, seed);
                self.social.asking = Some((0, what, seed, t + ASK_TIME));
                let me = self.online.my_id();
                let mine = self.your_name().to_string();
                self.start_table(what, seed, vec![me], vec![mine], seed, true);
            }
            NetDo::Wave(id, what) => {
                self.online.wave(id, what);
                let line = match what {
                    1 => "hooray!",
                    3 => "♥",
                    _ => "hello!",
                };
                self.you_say(line, t);
                if let Some(p) = &mut self.player {
                    p.wave_until = t + 1.2;
                }
            }
            NetDo::Yes(i) => self.answer_asked(i as usize, true),
            NetDo::No(i) => self.answer_asked(i as usize, false),
            NetDo::JoinTrip(id) => {
                self.goal = None;
                self.soon(play::Pending::Join(id));
            }
            NetDo::Race => {
                let seed = self.rng.next_u32();
                self.social.race = Some(RaceHost { seed, racers: vec![], went: false });
                self.online.ask(0, ask::RACE, seed);
                self.social.asking = Some((0, ask::RACE, seed, t + ASK_TIME));
                self.goal = None;
                self.soon(play::Pending::Race(seed, t + RACE_WAIT));
                self.post("⇘ a race! everyone's asked · up the chairlift…");
            }
            NetDo::Card(id) => {
                if let Some(c) = self.online.other(id).and_then(|o| o.card.clone()) {
                    self.social.card = Some((c, None));
                }
            }
            NetDo::FriendCard(i) => {
                if let Some(c) = self.online.friend_card(i as usize) {
                    self.social.card = Some((c, None));
                }
            }
            NetDo::MyCard => {
                let c = self.my_card();
                self.social.card = Some((c, Some(self.online.code())));
            }
            NetDo::Motto => self.text_ask = Some((TextFor::Motto, "a line for your profile card".into())),
            NetDo::LinkShow => {
                if !self.online.wanted() {
                    self.online.go_online(true);
                }
                self.social.front.push(Front::LinkShow);
            }
            NetDo::LinkUse => self.text_ask = Some((TextFor::Link, "the link code from your other device (XXXX-XXXX-XXXX; this one becomes that villager)".into())),
            NetDo::Unlink => self.social.front.push(Front::Unlink),
            NetDo::Villager(i) => self.social.front.push(Front::Villager(i as usize)),
            NetDo::NewVillager => self.text_ask = Some((TextFor::NewVillager, "the new villager's name".into())),
            NetDo::Ping(i) => {
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                if self.online.ping(i as usize) {
                    self.post(format!("♪ {name} pinged"));
                    self.log_line(format!("♪ you pinged {name}"), false);
                } else {
                    self.post("not online yet");
                }
            }
            NetDo::Chat => self.open_chat(),
            NetDo::GroupNew => self.text_ask = Some((TextFor::GroupNew, "the new group's name".into())),
            NetDo::GroupJoin => self.text_ask = Some((TextFor::GroupJoin, "the group's code (XXXX-XXXX-XXXX), then its name if you like".into())),
            NetDo::GroupSay(i) => {
                let name = self.online.book.groups.get(i as usize).map(|g| g.name.clone()).unwrap_or_default();
                self.text_ask = Some((TextFor::GroupSay(i), format!("say something in #{name}")));
            }
            NetDo::GroupCode(i) => {
                if let Some(g) = self.online.book.groups.get(i as usize) {
                    let code = crate::net::seal::pretty(&g.code);
                    self.copy_ask = Some(code.clone());
                    self.post(format!("# {}'s code {code} copied: whoever has it can join", g.name));
                }
            }
            NetDo::GroupLeave(i) => {
                let name = self.online.book.groups.get(i as usize).map(|g| g.name.clone()).unwrap_or_default();
                self.online.leave_group(i as usize);
                self.post(format!("# left {name}"));
            }
            NetDo::Admin(id, on) => {
                self.online.set_admin(id, on);
                let name = self.other_name(id);
                self.post(if on { format!("⚙ {name} may change the village's settings") } else { format!("⚙ {name} asks first again") });
            }
            NetDo::Messages => self.messages_read(),
            NetDo::Friends => self.open_chats(None),
            NetDo::Rename(i) => {
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                self.text_ask = Some((TextFor::Rename(i), format!("a name for {name} (just here; empty: as it was)")));
            }
            NetDo::PinFriend(i) => self.pin_friend(i as usize),
            NetDo::ClearChat(i) => {
                self.online.clear_chat(i as usize);
                self.post("⌫ the chat's cleared (from this device)");
            }
            NetDo::Quiet(i) => {
                if let Some(p) = self.online.book.friends.get_mut(i as usize) {
                    p.quiet ^= true;
                    let note = if p.quiet { format!("⊘ {} is quiet: their messages wait in the chat", p.name) } else { format!("♪ {}'s messages chime again", p.name) };
                    self.online.dirty = true;
                    self.post(note);
                }
            }
            NetDo::ChatWith(i) => self.open_chats(Some(i as usize)),
            NetDo::ShowLink(k) => {
                let n = self.social.log.len();
                if let Some(l) = (k as usize).checked_add(1).and_then(|k| n.checked_sub(k)).and_then(|i| self.social.log.get_mut(i)) {
                    let links = l.links.join(" ");
                    if !links.is_empty() {
                        for link in &l.links {
                            l.text = l.text.replacen("🔗link", link, 1);
                        }
                        l.links.clear();
                        self.copy_ask = Some(links.clone());
                        self.post(format!("🔗 {links} (copied: open it only if you trust who sent it)"));
                    }
                }
            }
            NetDo::Square(i) => {
                if self.online.village().is_some() {
                    self.online.leave_village();
                }
                self.online.join_square(i as usize);
            }
            NetDo::Listed(k) => {
                if let Some(l) = self.online.listings().get(k as usize).cloned() {
                    if self.online.village().is_some() {
                        self.online.leave_village();
                    }
                    let _ = self.online.join(&l.code);
                }
            }
            NetDo::ListMine(on) => {
                self.online.book.listed = on;
                self.online.dirty = true;
                if on && self.online.village().is_none() {
                    self.net_do(NetDo::Host);
                } else {
                    self.post(if on { "⌂ your village is listed in Public villages while it's open" } else { "⌂ your village isn't listed any more (friends and codes still work)" });
                }
            }
            NetDo::Whisper(id) => {
                let name = self.other_name(id);
                self.text_ask = Some((TextFor::Whisper(id), format!("just to {name}")));
            }
        }
    }

    /// The line typed for `what` (empty: never mind).
    pub fn give_text(&mut self, what: TextFor, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let t = self.now();
        match what {
            TextFor::Add => match self.online.add_friend(text) {
                Ok(()) => self.post("✉ asked: they'll see it when they're online"),
                Err(e) => self.post(e),
            },
            TextFor::Mail(i) => {
                // (nothing written: the chat's closed)
                if text.trim().is_empty() {
                    return;
                }
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                if self.online.mail(i as usize, text) {
                    self.log_line(format!("✉ to {name}: {text}"), false);
                    // the chat stays open for the next line
                    self.text_ask = Some((TextFor::Mail(i), self.chat_prompt(i as usize)));
                } else {
                    self.post("✉ not sent (not online yet)");
                }
            }
            TextFor::JoinCode => {
                if let Err(e) = self.online.join(text) {
                    self.post(e);
                }
            }
            TextFor::Say => {
                self.online.say(text);
                self.log_line(format!("☺ you: {text}"), false);
                self.you_say(text, t);
            }
            TextFor::Rename(i) => {
                let name = crate::net::clean(text.trim(), 32);
                if let Some(p) = self.online.book.friends.get_mut(i as usize).filter(|_| !name.is_empty()) {
                    p.name = name;
                    self.online.dirty = true;
                }
            }
            TextFor::Motto => {
                let m: String = text.chars().take(60).collect();
                self.social.motto = m.clone();
                self.social.front.push(Front::Motto(m));
                self.post("★ your card's line is set");
            }
            TextFor::Link => {
                if crate::net::seal::norm(text).len() != 12 {
                    self.post("a link code is twelve letters and numbers");
                } else {
                    if !self.online.wanted() {
                        self.online.go_online(true);
                    }
                    self.social.front.push(Front::Link(text.to_string()));
                }
            }
            TextFor::NewVillager => self.social.front.push(Front::NewVillager(text.chars().take(24).collect())),
            TextFor::Chat => self.chat(text, t),
            TextFor::GroupNew => {
                let code = self.online.new_group(text);
                let pretty = crate::net::seal::pretty(&code);
                self.copy_ask = Some(pretty.clone());
                let name = self.online.book.groups.last().map(|g| g.name.clone()).unwrap_or_default();
                self.post(format!("# {name} made: its code {pretty} (copied) lets others in · @{} to talk there", name.split_whitespace().next().unwrap_or("")));
            }
            TextFor::GroupJoin => {
                let mut words = text.split_whitespace();
                let code = words.next().unwrap_or("");
                let name: String = words.collect::<Vec<_>>().join(" ");
                match self.online.join_group(&name, code) {
                    Ok(()) => {
                        let name = self.online.book.groups.last().map(|g| g.name.clone()).unwrap_or_default();
                        self.post(format!("# into {name}: @{} to talk there", name.split_whitespace().next().unwrap_or("")));
                    }
                    Err(e) => self.post(e),
                }
            }
            TextFor::GroupSay(i) => {
                let name = self.online.book.groups.get(i as usize).map(|g| g.name.clone()).unwrap_or_default();
                if self.online.group_say(i as usize, text) {
                    self.log_line(format!("#{name} you: {text}"), false);
                    self.post(format!("#{name} you: {text}"));
                } else {
                    self.post("not online yet");
                }
            }
            TextFor::Whisper(id) => {
                let name = self.other_name(id);
                self.online.whisper(id, text);
                self.log_line(format!("✉ to {name} (just them): {text}"), false);
                self.post(format!("✉ to {name} (just them): {text}"));
            }
        }
    }
}

/// How someone online looks (their colours; a skin picked from their id
/// if they didn't say).
pub(super) fn look_of(av: &Avatar, id: u32) -> sprites::Look {
    let hex = |v: u32| Rgb::hex(v);
    let skin = if av.skin != 0 { hex(av.skin) } else { [Rgb::hex(0xe8c39e), Rgb::hex(0xc89a72), Rgb::hex(0xf0d0b0), Rgb::hex(0x8a5a3a)][(id % 4) as usize] };
    let hair = if av.hair != 0 { hex(av.hair) } else { Rgb::hex(0x3a2a1a) };
    let scarf = if av.scarf != 0 {
        Some(hex(av.scarf))
    } else if av.skin == 0 {
        // (an older version: no scarf said, so one in the hat's colour)
        Some(hex(av.hat.max(0x2a2a2a)) * 0.9)
    } else {
        None
    };
    let hat = (av.hat != 0).then(|| hex(av.hat));
    sprites::Look { kid: false, coat: hex(av.coat), hat, hair, scarf, skin, mittens: scarf.or(hat).unwrap_or(hex(av.coat) * 0.7), holding: None, lantern: false }
}

/// A look greyed (someone paused at their end): most of the colour gone,
/// a little darker.
fn greyed(l: sprites::Look) -> sprites::Look {
    let g = |c: Rgb| {
        let y = c.r * 0.3 + c.g * 0.59 + c.b * 0.11;
        Rgb { r: (c.r * 0.2 + y * 0.8) * 0.8, g: (c.g * 0.2 + y * 0.8) * 0.8, b: (c.b * 0.2 + y * 0.8) * 0.85 }
    };
    sprites::Look { coat: g(l.coat), hat: l.hat.map(g), hair: g(l.hair), scarf: l.scarf.map(g), skin: g(l.skin), mittens: g(l.mittens), ..l }
}

pub(super) fn ago_short(s: i64) -> String {
    match s {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}

fn ago_words(s: i64) -> String {
    match s {
        s if s < 0 => "not heard from yet".into(),
        s if s < 90 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86400 * 2 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

/// A profile card, in the middle of the screen: the villager as they look,
/// their line, their pet, a few things they've done; this villager's own
/// shows the friend code too.
pub fn draw_card(f: &mut Frame, c: &Card, code: Option<&str>, look: &str) {
    use crate::color::pal;
    use crate::frame::{Bg, Glyph};
    let (w, h) = (48, 16);
    let (x0, y0) = ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(0));
    let bg = Bg::Mix(pal::UI_PANEL, 0.96);
    let border = Rgb::hex(0xffd36b) * 0.8;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
        }
    }
    let name = if c.name.is_empty() { "a villager" } else { &c.name };
    f.text(x0 + 2, y0, &format!(" ★ {name} "), pal::UI_TITLE, bg, 0.0);
    f.text(x0 + w - 4, y0, " ✕", pal::UI_DIM, bg, 0.0);
    // them, as they look
    let hex = |v: u32| Rgb::hex(v);
    let l = sprites::Look {
        kid: false,
        coat: hex(c.look[0].max(1)),
        hat: (c.look[1] != 0).then(|| hex(c.look[1])),
        hair: hex(c.look[2].max(1)),
        skin: hex(if c.look[3] == 0 { 0xe8c39e } else { c.look[3] }),
        scarf: (c.look[4] != 0).then(|| hex(c.look[4])),
        mittens: hex(if c.look[4] != 0 { c.look[4] } else { c.look[0].max(1) }),
        holding: None,
        lantern: false,
    };
    let pic = sprites::person(&l, sprites::Pose::Wave, 1.0, 1.0);
    let rows = sprites::rows_of(&pic, 2.2);
    f.sprite((x0 + 8) as f32, (y0 + 12) as f32, rows, pic);
    let (tx, mut y) = (x0 + 16, y0 + 2);
    let motto = if c.motto.is_empty() { "(no line yet)".to_string() } else { format!("“{}”", c.motto) };
    for line in wrap(&motto, (w - 18) as usize).into_iter().take(2) {
        f.text(tx, y, &line, Rgb::hex(0xf4ecd8), bg, 0.0);
        y += 1;
    }
    y += 1;
    if !c.pet.is_empty() {
        f.text(tx, y, &format!("with {}", c.pet), pal::UI_TEXT, bg, 0.0);
        y += 1;
    }
    f.text(tx, y, &format!("♞ games won {} · ● fights won {}", c.won, c.fights), pal::UI_TEXT, bg, 0.0);
    y += 1;
    let best = if c.best > 0 { format!(" · best {}", c.best) } else { String::new() };
    f.text(tx, y, &format!("⇘ runs {}{best}", c.runs), pal::UI_TEXT, bg, 0.0);
    y += 1;
    f.text(tx, y, &format!("✦ cheer {} · ♥ friends {}", c.cheer, c.friends), pal::UI_TEXT, bg, 0.0);
    if let Some(code) = code {
        f.text(x0 + 2, y0 + h - 3, &format!("friend code {code}"), Rgb::hex(0xffd36b), bg, 0.0);
        f.text(x0 + 2, y0 + h - 2, "look and line: Settings → You · Esc: close", pal::UI_DIM, bg, 0.0);
    } else {
        f.text(x0 + 2, y0 + h - 2, "Esc (or a click): close", pal::UI_DIM, bg, 0.0);
    }
    crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h }, look, bg, 29);
}

fn wrap(s: &str, w: usize) -> Vec<String> {
    let mut out = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > w {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}
