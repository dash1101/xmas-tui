//! The village online: friends (by friend code), messages, and shared
//! villages, where other players' villagers walk about, talk aloud and throw
//! snowballs. The conversation with the relay is `crate::net`; this is what
//! the village does with it: the menus, the post (the last few things that
//! happened, top left), the others drawn with their names over them.

use super::*;
use crate::net::{self, Avatar, Event, Link};

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
}

/// What a line of typing is for (the front end asks for it, see
/// `take_text_ask`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextFor {
    Add,
    Mail(u8),
    JoinCode,
    Say,
}

/// The more-menu numbers for the online menus (see `more_menu`).
pub const MORE_ONLINE: u8 = 10;
pub const MORE_FRIEND: u8 = 32;

/// How long a line stays in the post.
const POST_TIME: f32 = 12.0;

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

    fn post(&mut self, s: impl Into<String>) {
        let t = self.now();
        self.post.push((t, s.into()));
        if self.post.len() > 6 {
            self.post.remove(0);
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
        let rgb = |c: Rgb| ((c.r * 255.0) as u32) << 16 | ((c.g * 255.0) as u32) << 8 | (c.b * 255.0) as u32;
        let look = player::Player::look();
        Some(Avatar { x: sx, y: sy, dir: p.dir, moving: p.moving, air: p.in_air(), coat: rgb(look.coat), hat: look.hat.map(rgb).unwrap_or(0), away })
    }

    /// Each frame: the online side moves on, and what it heard is shown.
    pub(super) fn update_online(&mut self, dt: f32, t: f32) {
        let name = if self.you_name.trim().is_empty() { "a neighbour".to_string() } else { self.you_name.trim().to_string() };
        self.online.set_name(&name);
        // (a guest: only once in the host's village, not still in their own)
        let me = match self.online.village() {
            Some((_, true)) => self.avatar(),
            Some((_, false)) if self.online.guest_seed() == Some(self.seed) => self.avatar(),
            _ => None,
        };
        self.online.tick(t, me, self.seed);
        self.online.ease(dt);
        for e in self.online.take_events() {
            match e {
                Event::Note(s) => self.post(s),
                Event::Mail { from, text } => {
                    self.post(format!("✉ {from}: {text}"));
                    if let Some(p) = &self.player {
                        self.director.sound(Sfx::Chime, p.centre(), 0.5);
                    }
                }
                Event::Asked(name) => {
                    self.post(format!("✦ {name} asks to be friends (friends & online, in ✦ things to do)"));
                    if let Some(p) = &self.player {
                        self.director.sound(Sfx::Chime, p.centre(), 0.5);
                    }
                }
                Event::Village(seed) => {
                    if seed != self.seed {
                        self.village_switch = Some(Some(seed));
                    }
                }
                Event::Home => self.village_switch = Some(None),
                Event::Said { id, text } => {
                    if let Some(o) = self.online.other(id) {
                        let (x, y) = self.local(o.shown.0, o.shown.1);
                        let name = o.name.clone();
                        let _ = name;
                        self.director.say(x.round() as i32, y as i32 - 4, &text, t, 4.0, 6);
                    }
                }
                Event::Threw { x, y, vx, vy, .. } => {
                    let (lx, ly) = self.local(x, y);
                    let u = self.anchor.2;
                    let (pvx, pvy) = (vx * u, vy * u);
                    let at_you = self.player.as_ref().is_some_and(|p| {
                        // (where it comes down: about the villager?)
                        let flight = ((p.centre() - lx) / pvx.max(1e-3).copysign(pvx)).abs();
                        (0.1..2.0).contains(&flight) && (lx + pvx * flight - p.centre()).abs() < 3.0 && (ly - p.y).abs() < 6.0
                    });
                    self.critters.throw_from(lx, ly, pvx, pvy, at_you, &mut self.director);
                }
            }
        }
        let p = &mut self.post;
        p.retain(|(at, _)| t - *at < POST_TIME);
    }

    /// The others in a shared village, with their names over them.
    pub(super) fn draw_others(&self, f: &mut Frame, t: f32, view: Option<View>) {
        if self.inside.is_some() || self.trip.is_some() {
            return;
        }
        for (id, o) in self.online.others() {
            if o.av.away != 0 {
                continue;
            }
            let (wx, wy) = self.local(o.shown.0, o.shown.1);
            let (x, y, k) = match view {
                Some(v) => (v.x(wx), v.y(wy + 1.0) - 1.0, sprite_k(self.h) * v.z),
                None => (wx, wy, sprite_k(self.h)),
            };
            let hex = |v: u32| Rgb::hex(v);
            let look = sprites::Look {
                kid: false,
                coat: hex(o.av.coat),
                hat: (o.av.hat != 0).then(|| hex(o.av.hat)),
                hair: Rgb::hex(0x3a2a1a),
                scarf: Some(hex(o.av.hat.max(0x2a2a2a)) * 0.9),
                skin: [Rgb::hex(0xe8c39e), Rgb::hex(0xc89a72), Rgb::hex(0xf0d0b0), Rgb::hex(0x8a5a3a)][(id % 4) as usize],
                mittens: hex(o.av.hat.max(0x2a2a2a)),
                holding: None,
                lantern: false,
            };
            let pose = if o.av.air {
                sprites::Pose::Jump
            } else if o.av.moving {
                sprites::Pose::Walk(((t * 6.0) as i32 % 2) as u8)
            } else {
                sprites::Pose::Stand
            };
            let pic = sprites::person(&look, pose, o.av.dir, 0.9 + 0.25 * f.day.max(0.0));
            let rows = sprites::rows_of(&pic, k);
            f.sprite(x, y + 1.0, rows, pic);
            // the name, over their head
            let name = format!(" {} ", o.name);
            let n = name.chars().count() as i32;
            let top = (y + 1.0 - rows).floor() as i32 - 1;
            f.text(x.round() as i32 - n / 2, top, &name, Rgb::hex(0xffe9a8), crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.7), 0.0);
        }
    }

    /// The post: the last few things that happened online, top left.
    pub(super) fn draw_post(&self, f: &mut Frame) {
        let t = self.now();
        let (x0, mut y) = (2, 2);
        let link = self.online.link();
        if self.online.wanted() && link != Link::On {
            f.text(x0, y, " ✉ connecting… ", crate::color::pal::UI_DIM, crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.8), 0.0);
            y += 1;
        }
        if let Some((code, host)) = self.online.village() {
            let n = self.online.others().len();
            let s = if host { format!(" ⌂ village open: {code} · {n} here ") } else { format!(" ⌂ a shared village · {} here ", n + 1) };
            f.text(x0, y, &s, Rgb::hex(0xffd36b), crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.8), 0.0);
            y += 1;
        }
        for (at, line) in &self.post {
            let a = (1.0 - ((t - at) - (POST_TIME - 2.0)).max(0.0) / 2.0).clamp(0.0, 1.0);
            let w = (self.w as usize).saturating_sub(6).max(10);
            let line: String = line.chars().take(w).collect();
            f.text(x0, y, &format!(" {line} "), crate::color::pal::UI_TEXT * a, crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.8 * a), 0.0);
            y += 1;
        }
    }

    /// The friends & online menu.
    pub fn online_menu(&self) -> (String, Vec<(String, Do)>) {
        let o = &self.online;
        let mut v: Vec<(String, Do)> = vec![];
        if !o.wanted() {
            v.push(("● go online: friends, messages, shared villages".into(), Do::Net(NetDo::On)));
            return ("friends & online".into(), v);
        }
        let title = match o.link() {
            Link::On => "friends & online ●".to_string(),
            _ => "friends & online (connecting…)".to_string(),
        };
        v.push((format!("my friend code: {} (copy)", o.code()), Do::Net(NetDo::Code)));
        v.push(("✚ add a friend (their code)…".into(), Do::Net(NetDo::Add)));
        for (i, p) in o.book.asking.iter().enumerate().take(4) {
            v.push((format!("✦ {} asks to be friends: yes!", p.name), Do::Net(NetDo::Accept(i as u8))));
            v.push((format!("   no thanks to {}", p.name), Do::Net(NetDo::Decline(i as u8))));
        }
        for (i, (p, here)) in o.friends().iter().enumerate().take(16) {
            let mark = match here {
                _ if p.waiting => "…",
                Some(h) if h.village.is_some() => "⌂",
                Some(h) if h.on => "●",
                _ => "○",
            };
            let what = match here {
                _ if p.waiting => " (asked)".to_string(),
                Some(h) if h.village.is_some() => " (village open)".to_string(),
                Some(h) if h.on => " (online)".to_string(),
                _ => String::new(),
            };
            v.push((format!("{mark} {}{what} ▸", p.name), Do::More(MORE_FRIEND + i as u8)));
        }
        match o.village() {
            Some((code, true)) => {
                v.push((format!("⌂ this village is open: {code} (copy)"), Do::Net(NetDo::VillageCode)));
                v.push(("✎ say something to everyone…".into(), Do::Net(NetDo::Say)));
                v.push(("⌂ close the village".into(), Do::Net(NetDo::Close)));
            }
            Some((_, false)) => {
                v.push(("✎ say something to everyone…".into(), Do::Net(NetDo::Say)));
                v.push(("⌂ home again".into(), Do::Net(NetDo::Leave)));
            }
            None => {
                v.push(("⌂ open this village to friends".into(), Do::Net(NetDo::Host)));
                v.push(("⌂ into a village by its code…".into(), Do::Net(NetDo::JoinCode)));
            }
        }
        v.push(("○ go offline".into(), Do::Net(NetDo::Off)));
        (title, v)
    }

    /// One friend's menu.
    pub fn friend_menu(&self, i: usize) -> (String, Vec<(String, Do)>) {
        let fr = self.online.friends();
        let Some((p, here)) = fr.get(i) else { return ("friends".into(), vec![]) };
        let mut v = vec![];
        if !p.waiting {
            v.push((format!("✉ write to {}…", p.name), Do::Net(NetDo::Mail(i as u8))));
        }
        if here.as_ref().is_some_and(|h| h.village.is_some()) {
            v.push((format!("⌂ into {}'s village", p.name), Do::Net(NetDo::Join(i as u8))));
        }
        v.push((format!("✕ unfriend {}", p.name), Do::Net(NetDo::Unfriend(i as u8))));
        (p.name.clone(), v)
    }

    pub(super) fn net_do(&mut self, n: NetDo) {
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
            NetDo::Mail(i) => {
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                self.text_ask = Some((TextFor::Mail(i), format!("a message to {name}")));
            }
            NetDo::Join(i) => {
                let code = self.online.friends().get(i as usize).and_then(|(_, h)| h.as_ref().and_then(|h| h.village.clone()));
                if let Some(code) = code {
                    if let Err(e) = self.online.join(&code) {
                        self.post(e);
                    }
                }
            }
            NetDo::Unfriend(i) => {
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                self.online.remove_friend(i as usize);
                self.post(format!("✕ {name} unfriended"));
            }
            NetDo::Accept(i) => self.online.accept(i as usize),
            NetDo::Decline(i) => self.online.decline(i as usize),
            NetDo::Host => {
                let code = self.online.host();
                self.copy_ask = Some(code.clone());
                self.post(format!("⌂ the village is open to friends: code {code} (copied)"));
            }
            NetDo::Close => {
                self.online.leave_village();
                self.post("⌂ the village is closed");
            }
            NetDo::JoinCode => self.text_ask = Some((TextFor::JoinCode, "the village's code (XXXX-XXXX-XXXX)".into())),
            NetDo::Leave => self.online.leave_village(),
            NetDo::Say => self.text_ask = Some((TextFor::Say, "say something (everyone here hears it)".into())),
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
                let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
                if self.online.mail(i as usize, text) {
                    self.post(format!("✉ to {name}: {text}"));
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
                self.you_say(text, t);
            }
        }
    }
}
