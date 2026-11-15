//! Animals and people. They wander through, graze, munch berries from the
//! festive bushes, and meet each other: passers-by pet the animals and greet
//! each other, a fox plays chase with a rabbit (it never catches it), kids
//! have snowball fights, people say hi to the snowman. Animals startle when
//! the cursor rushes at them and come over, curious, when it rests nearby.
//!
//! And anyone can be picked up: click-and-drag to carry them (they wiggle,
//! giggle or complain), let go to drop them back in the snow (people get a
//! little dizzy). A click without dragging is a friendly poke / pet.

use super::events::{Ctx, Director, Kind};
use super::pets::{self, PetKind, Pose};
use super::dialogue::{self, Ev, Talk, TalkCtx};
use super::lines::Lines;
use crate::audio::Sfx;
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{h01, hash2, noise1, Rng};

#[derive(Clone, Copy, PartialEq, Debug)]
enum St {
    Walk,
    Idle { until: f32 },
    Graze { until: f32 },
    Munch { bush: usize, until: f32, bite: f32 },
    Held { ox: f32, oy: f32, since: f32 },
    Fall { vy: f32, to: f32, from: f32 },
    Dizzy { until: f32 },
    Petting { until: f32 },
    Petted { until: f32 },
    Greet { until: f32 },
    Flee { until: f32 },
    Chase { target: u64, until: f32 },
    Curious { until: f32 },
    Throw { until: f32 },
    /// Walking along behind the player (`moving`: catching up right now).
    Follow { until: f32, moving: bool },
    /// In a conversation; `resume`: go back to following afterwards.
    Talk { until: f32, resume: bool },
    /// Gathering round the big tree for a light show: heading for (tx,
    /// ty), then standing facing the tree at column `face`.
    Gather { tx: f32, ty: f32, face: f32, until: f32, moving: bool },
}

struct Critter {
    id: u64,
    kind: Kind,
    x: f32,
    /// Baseline row (float while being carried / falling).
    y: f32,
    dir: f32,
    speed: f32,
    age: f32,
    st: St,
    color: Rgb,
    hat: Rgb,
    /// Not before this time: no new meetings (keeps things from looping).
    social_at: f32,
    /// After this time animals stop turning back at the edges and wander off.
    leave_at: f32,
    /// Next "carry" remark.
    chatter_at: f32,
    /// Walking in a group: the leader's id (0: on their own) and how far
    /// behind them to keep.
    group: u64,
    gap: f32,
    /// Stepping round someone: which way (rows; 0 = not dodging).
    dodge: f32,
    /// People have names; `met`: has said hello to the player; `chat`: a
    /// chat with the player in progress (topic, next step).
    name: &'static str,
    met: bool,
    chat: Option<(usize, usize)>,
    /// A pet out for a walk with them.
    pet: Option<VPet>,
}

/// A villager's pet: trots along behind them (a dog on its lead), sits when
/// they stop, chases after them when they're carried off.
struct VPet {
    kind: PetKind,
    body: Rgb,
    marks: Rgb,
    name: &'static str,
    x: f32,
    y: f32,
    dir: f32,
    walking: bool,
    /// Petted: bouncing about happily until then.
    happy_until: f32,
}

/// What people say about their pet (by kind): {n} is its name.
const PET_FACTS: [&[&str]; 3] = [
    &["this is {n}! he thinks every snowflake is a tennis ball", "that's {n}. she's never met a stranger", "{n}! sit! …well, nearly", "say hello, {n}! he's been dying to meet you"],
    &["this is {n}. she lets me walk with her", "{n} insisted on coming. cats do that, apparently", "that's {n}. he's only out for the attention", "{n}! don't eat the snow!"],
    &["this is {n}! she hops when she's happy", "that's {n}. he's very brave about the snow", "{n} likes carrots and adventures", "meet {n}! careful, she's ticklish"],
];

impl Critter {
    fn animal(&self) -> bool {
        matches!(self.kind, Kind::Deer | Kind::Fox | Kind::Rabbit)
    }
    fn person(&self) -> bool {
        matches!(self.kind, Kind::Walker | Kind::Kid)
    }
    fn free(&self) -> bool {
        matches!(self.st, St::Walk | St::Idle { .. } | St::Graze { .. } | St::Curious { .. })
    }
    fn held(&self) -> bool {
        matches!(self.st, St::Held { .. })
    }
    /// Sprite rows (right-facing) for the current state.
    fn sprite(&self, t: f32) -> Vec<&'static str> {
        let moving = matches!(self.st, St::Walk | St::Flee { .. } | St::Chase { .. } | St::Curious { .. } | St::Follow { moving: true, .. } | St::Gather { moving: true, .. });
        let rate = match self.kind {
            Kind::Fox => 10.0,
            Kind::Kid => 9.0,
            _ => 6.0,
        };
        let step = (self.age * rate) as i32 % 2 == 0;
        let kick = (t * 7.0) as i32 % 2 == 0;
        let chew = (t * 5.0) as i32 % 2 == 0;
        let held = self.held() || matches!(self.st, St::Fall { .. });
        let head_down = matches!(self.st, St::Graze { .. } | St::Munch { .. });
        match self.kind {
            Kind::Deer if held => vec!["     ψ", "▗▄▄▄▟▀", if kick { " ╲╱╲╱ " } else { " ╱╲╱╲ " }],
            Kind::Deer if head_down => vec!["      ", "▗▄▄▄▄▖", if chew { " ┃  ┃▼" } else { " ┃  ┃▾" }],
            Kind::Deer => vec!["     ψ", "▗▄▄▄▟▀", if moving && step { " ╱  ╲ " } else { " ┃  ┃ " }],
            Kind::Fox if held => vec!["    ◢◣", "▂▄▄▄█▘", if kick { " ╲╱╲╱ " } else { " ╱╲╱╲ " }],
            Kind::Fox if head_down => vec!["      ", "▂▄▄▄▄◣", if chew { " ╱ ╲ ▾" } else { " ╱ ╲ ▼" }],
            Kind::Fox => vec!["    ◢◣", "▂▄▄▄█▘", if moving && step { " ╲ ╱  " } else { " ╱ ╲  " }],
            Kind::Rabbit if held => vec![" ‖", "▗█▙", if kick { " ╵ " } else { "  ╵" }],
            Kind::Rabbit if head_down => vec![" ╻", if chew { "▗█▙" } else { "▗█▟" }],
            Kind::Rabbit => vec![" ‖", "▗█▙"],
            Kind::Walker if held => vec![" ▲ ", "╲●╱", " █ ", if kick { " ╱ " } else { " ╲ " }],
            Kind::Walker => match self.st {
                St::Petting { .. } => vec![" ▲ ", " ● ", "◘█╲", " ╨ "],
                St::Greet { .. } => vec![" ▲╱", " ● ", "◘█ ", " ╨ "],
                _ => vec![" ▲", " ●", "◘█", if moving && step { " ╨" } else { " ║" }],
            },
            Kind::Kid if held => vec![" ▴ ", "╲●╱", " █ ", if kick { " ╱ " } else { " ╲ " }],
            Kind::Kid => match self.st {
                St::Petting { .. } => vec!["▴ ", "● ", "█╲", "╨ "],
                St::Throw { .. } | St::Greet { .. } => vec!["▴╱", "● ", "█ ", "╨ "],
                _ => vec!["▴", "●", "█", if moving && step { "╨" } else { "║" }],
            },
            Kind::Train => vec![],
        }
    }
    /// (x, top, w, h) of the sprite in cells, drawn `k` times bigger.
    fn bbox(&self, t: f32, k: f32) -> (i32, i32, i32, i32) {
        let rows = self.sprite(t);
        let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(1) as f32;
        let h = rows.len() as f32;
        let (bw, bh) = ((w * k).round() as i32, (h * k).round() as i32);
        let left = (self.x + w * (1.0 - k) / 2.0).round() as i32;
        (left, self.y.round() as i32 - bh + 1, bw.max(1), bh.max(1))
    }
    fn centre(&self) -> f32 {
        let w = match self.kind {
            Kind::Deer | Kind::Fox => 6.0,
            Kind::Rabbit => 3.0,
            Kind::Walker => 2.0,
            _ => 1.0,
        };
        self.x + w / 2.0
    }
    fn sfx(&self) -> Option<Sfx> {
        match self.kind {
            Kind::Deer => Some(Sfx::Deer),
            Kind::Fox => Some(Sfx::Fox),
            Kind::Rabbit => Some(Sfx::Rabbit),
            Kind::Kid => Some(Sfx::Laugh),
            Kind::Walker => Some(Sfx::Greet),
            Kind::Train => None,
        }
    }
}

/// A snow-capped holly-ish bush with berries that animals snack on.
/// Someone out of the scene for a while (inside an igloo).
pub struct Hidden(Critter);

pub struct Bush {
    pub x: i32,
    pub base: i32,
    w: i32,
    /// (dx, dy, ripe_at): a berry shows once `t >= ripe_at`.
    berries: Vec<(i32, i32, f32)>,
    shake_until: f32,
}

impl Bush {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y > self.base - 3 && y <= self.base
    }
    fn ripe(&self, t: f32) -> usize {
        self.berries.iter().filter(|b| t >= b.2).count()
    }
    fn centre(&self) -> f32 {
        self.x as f32 + self.w as f32 / 2.0
    }
}

struct Snowball {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// Who it's aimed at: a critter id, `AT_YOU`, or 0 (just thrown).
    target: u64,
    age: f32,
    /// Thrown by the player (the target reacts to the player).
    yours: bool,
    /// The row it comes down on if it misses.
    land: f32,
}

/// A snowball aimed at the player.
const AT_YOU: u64 = u64::MAX;

struct Bird {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
}

pub struct Critters {
    /// How big everyone's drawn (1 at 75 rows; 2 at 150), and how far they
    /// walk to match.
    pub k: f32,
    list: Vec<Critter>,
    pub bushes: Vec<Bush>,
    prints: Vec<(i32, i32, f32)>,
    balls: Vec<Snowball>,
    birds: Vec<Bird>,
    rng: Rng,
    next_id: u64,
    /// Christmas Eve night: people stay home.
    asleep: bool,
    held: Option<u64>,
    grab_at: (f32, f32),
    moved: bool,
    w: i32,
    h: i32,
    /// The camera's view of the world: (left column, width).
    view: (f32, f32),
    /// First walkable row per column (the snowfield in front of the village).
    lo: Vec<i32>,
    mult: f32,
    startle_cd: f32,
    /// What people talk about: what's going on, and what's been said.
    talk: Talk,
    tctx: TalkCtx,
    /// Conversations under way, for passers-by to chime in on:
    /// (participants, centre column, row, until, joined yet).
    convs: Vec<(Vec<u64>, f32, f32, f32, bool)>,
    /// Who reacted to what lately (no chorus of the same line).
    react_cd: f32,
    /// Someone hit by the player's snowball who'll throw one back: (id, when).
    revenge: Vec<(u64, f32)>,
    /// Snowballs that hit the player since the scene last asked.
    hits_on_you: u32,
    /// The player's snowballs that hit someone, since last asked.
    your_hits: u32,
    /// The player character: (x, baseline row, facing, walking).
    player: Option<(f32, f32, f32, bool)>,
    /// How long the player has been standing still, and when followers
    /// may next strike up a conversation.
    still: f32,
    follow_chat_at: f32,
    /// Snowball fights between the kids, and when the next may start.
    fights: Vec<Fight>,
    /// Who the player's snowballs hit (for what they say about it).
    your_hit_ids: Vec<u64>,
    next_fight: f32,
}

/// Two sides of kids pelting each other from where they've dug in.
struct Fight {
    /// (id, their spot: column, row) per side.
    sides: [Vec<(u64, f32, f32)>; 2],
    /// The no-man's-land between them (what they face).
    mid: f32,
    until: f32,
    next_line: f32,
}

const FIGHT_START: &[&str] = &["SNOWBALL FIGHT!!", "you're going DOWN!", "snowball fight! you vs us!", "ready… aim…"];
const FIGHT_MID: &[&str] = &["gotcha!", "missed me!", "ha ha!", "take THAT!", "no fair!", "right in the hat!", "incoming!!", "duck!", "ha — too slow!", "I've got a stockpile!"];
const FIGHT_END: &[&str] = &["truce! truce!", "okay okay, you win!", "rematch tomorrow!", "my hands are frozen!", "we're BOTH winners", "best fight ever"];

const RED_BERRY: Rgb = Rgb::hex(0xe8283c);
const LEAF: Rgb = Rgb::hex(0x1e5a32);
const LEAF_DARK: Rgb = Rgb::hex(0x143f23);
const SNOW_CAP: Rgb = Rgb::hex(0xdfe7f4);

impl Critters {
    /// `avoid`: column ranges (x0, x1) where bushes mustn't go (the main
    /// tree and its presents, the snowman…).
    pub fn new(ctx: &Ctx, seed: u32, mult: f32, bushes: bool, avoid: &[(i32, i32)]) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0xc417);
        let lo: Vec<i32> = ctx.ground_row.iter().map(|g| (g + 4).min(ctx.h - 2)).collect();
        let mut c = Critters {
            k: 1.0,
            list: vec![],
            bushes: vec![],
            prints: vec![],
            balls: vec![],
            birds: vec![],
            rng: Rng::new(seed as u64 ^ 0x5eed),
            next_id: 1,
            asleep: false,
            held: None,
            grab_at: (0.0, 0.0),
            moved: false,
            w: ctx.w,
            h: ctx.h,
            view: (ctx.cam, ctx.vw as f32),
            lo,
            mult,
            startle_cd: 0.0,
            talk: Talk::new(seed),
            tctx: TalkCtx::default(),
            convs: vec![],
            react_cd: 0.0,
            revenge: vec![],
            hits_on_you: 0,
            your_hits: 0,
            player: None,
            still: 0.0,
            follow_chat_at: 0.0,
            fights: vec![],
            your_hit_ids: vec![],
            next_fight: rng.range(40.0, 120.0),
        };
        if bushes && ctx.h >= 20 {
            let u = (ctx.h as f32 / 75.0).max(0.5);
            let bw = ((7.0 * u).round() as i32).clamp(5, 10);
            let n = ((ctx.w as f32 / 85.0).round() as i32).clamp(2, 8);
            let mut tries = 0;
            while (c.bushes.len() as i32) < n && tries < 200 {
                tries += 1;
                let x = rng.int(2, (ctx.w - bw - 2).max(3));
                if avoid.iter().any(|&(a, b)| x + bw > a && x < b) || c.bushes.iter().any(|o| (o.x - x).abs() < bw + 6) {
                    continue;
                }
                let lo = c.lo[(x as usize).min(c.lo.len() - 1)];
                let base = rng.int(lo + 1, (ctx.h - 2).max(lo + 2));
                let mut berries = vec![];
                for _ in 0..rng.int(4, 8) {
                    let b = (rng.int(1, bw - 1), rng.int(0, 2), 0.0);
                    if !berries.iter().any(|o: &(i32, i32, f32)| o.0 == b.0 && o.1 == b.1) {
                        berries.push(b);
                    }
                }
                c.bushes.push(Bush { x, base, w: bw, berries, shake_until: 0.0 });
            }
        }
        c
    }

    fn lo_at(&self, x: f32) -> f32 {
        self.lo[(x.max(0.0) as usize).min(self.lo.len() - 1)] as f32
    }

    /// The director asked for someone to come by.
    pub fn request(&mut self, kind: Kind, t: f32) {
        if self.mult <= 0.0 || self.list.len() >= 24 || (self.asleep && matches!(kind, Kind::Walker | Kind::Kid)) {
            return;
        }
        // fewer/more visitors with the "Animals & people" setting
        let mut n = self.mult.floor() as i32 + self.rng.chance(self.mult.fract()) as i32;
        while n > 0 {
            let id = self.spawn(kind, t, None);
            // people often go about in twos and threes
            if kind == Kind::Walker && self.rng.chance(0.45) {
                let extra = if self.rng.chance(0.3) { 2 } else { 1 };
                self.companions(id, extra, t);
            }
            n -= 1;
        }
    }

    /// `extra` people walking along with `leader`.
    fn companions(&mut self, leader: u64, extra: usize, t: f32) {
        let Some(l) = self.list.iter().find(|c| c.id == leader) else { return };
        let (lx, ly, dir, speed) = (l.x, l.y, l.dir, l.speed);
        for k in 0..extra {
            let gap = 3.0 + k as f32 * 3.0 + self.rng.range(0.0, 0.8);
            let kind = if self.rng.chance(0.25) { Kind::Kid } else { Kind::Walker };
            let y = (ly + self.rng.range(-1.0, 1.0).round()).clamp(self.lo_at(lx), (self.h - 1) as f32);
            let id = self.spawn(kind, t, Some((lx - dir * gap, y)));
            if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
                (c.dir, c.speed, c.group, c.gap, c.leave_at) = (dir, speed, leader, gap, 0.0);
            }
        }
    }

    fn spawn(&mut self, kind: Kind, t: f32, at: Option<(f32, f32)>) -> u64 {
        // the village is in bed: only the animals are out
        let kind = if self.asleep && at.is_none() && matches!(kind, Kind::Walker | Kind::Kid) { *self.rng.pick(&[Kind::Deer, Kind::Fox, Kind::Rabbit]) } else { kind };
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        let x = match at {
            Some((x, _)) => x,
            // just off the edge of the view (people walk in from the sides)
            None if dir > 0.0 => (self.view.0 - 8.0).max(-8.0),
            None => (self.view.0 + self.view.1 + 2.0).min(self.w as f32 + 2.0),
        };
        let lo = self.lo_at(x.clamp(0.0, self.w as f32 - 1.0));
        let y = match at {
            Some((_, y)) => y,
            None => self.rng.range(lo, (self.h - 1) as f32).round(),
        };
        let coats = [Rgb::hex(0xb3202a), Rgb::hex(0x1f6e3a), Rgb::hex(0x2a4c8a), Rgb::hex(0x6a3a8a), Rgb::hex(0xc8862a)];
        let (speed, color) = match kind {
            Kind::Deer => (self.rng.range(4.0, 7.0), Rgb::hex(0x8a5a32)),
            Kind::Fox => (self.rng.range(8.0, 11.0), Rgb::hex(0xd8682a)),
            Kind::Rabbit => (self.rng.range(6.0, 9.0), Rgb::hex(0xd8d4cc)),
            Kind::Kid => (self.rng.range(5.0, 7.5), *self.rng.pick(&coats)),
            _ => (self.rng.range(3.0, 4.5), *self.rng.pick(&coats)),
        };
        let hat = *self.rng.pick(&[pal::CANDY_RED, Rgb::hex(0x2fae5c), Rgb::hex(0x4a8dff), Rgb::hex(0xffc23a), Rgb::hex(0xff7ae0)]);
        // animals linger (turning back at the edges) for a while; people
        // pass through unless something catches their eye
        let stay = match kind {
            Kind::Deer | Kind::Fox | Kind::Rabbit => self.rng.range(70.0, 220.0),
            Kind::Kid => self.rng.range(25.0, 70.0),
            _ => 0.0,
        };
        let id = self.next_id;
        self.next_id += 1;
        self.list.push(Critter {
            id,
            kind,
            x,
            y,
            dir,
            speed,
            age: self.rng.range(0.0, 3.0),
            st: St::Walk,
            color,
            hat,
            social_at: t + self.rng.range(2.0, 8.0),
            leave_at: t + stay,
            chatter_at: 0.0,
            group: 0,
            gap: 0.0,
            dodge: 0.0,
            name: if matches!(kind, Kind::Walker | Kind::Kid) { dialogue::name(&mut self.rng, kind == Kind::Kid) } else { "" },
            met: false,
            chat: None,
            pet: None,
        });
        // some folk bring their pet along
        let p = match kind {
            Kind::Walker => 0.22,
            Kind::Kid => 0.12,
            _ => 0.0,
        };
        if at.is_none() && self.rng.chance(p) {
            let pk = if kind == Kind::Kid {
                *self.rng.pick(&[PetKind::Dog, PetKind::Bunny, PetKind::Cat, PetKind::Bunny])
            } else {
                *self.rng.pick(&[PetKind::Dog, PetKind::Dog, PetKind::Dog, PetKind::Cat])
            };
            let coats = pk.coats();
            let (_, b, m) = coats[self.rng.int(0, coats.len() as i32) as usize];
            let name = *self.rng.pick(super::pets::NAMES);
            if let Some(c) = self.list.last_mut() {
                c.pet = Some(VPet { kind: pk, body: Rgb::hex(b), marks: Rgb::hex(m), name, x: c.x - c.dir * 3.0, y, dir: c.dir, walking: false, happy_until: 0.0 });
            }
        }
        id
    }

    /// For previews: someone already on screen.
    /// `x`: world column.
    pub fn spawn_demo(&mut self, kind: Kind, x: f32, t: f32) {
        let y = (self.lo_at(x) + 2.0).min((self.h - 1) as f32);
        self.spawn(kind, t, Some((x, y)));
    }

    /// For previews: someone out walking their pet (kind 0 dog, 1 cat, 2
    /// bunny; `coat` index).
    pub fn spawn_demo_pet(&mut self, kind: usize, coat: usize, x: f32, t: f32) {
        let pk = [PetKind::Dog, PetKind::Cat, PetKind::Bunny][kind % 3];
        self.spawn_demo(if pk == PetKind::Bunny { Kind::Kid } else { Kind::Walker }, x, t);
        let coats = pk.coats();
        let (_, b, m) = coats[coat % coats.len()];
        if let Some(c) = self.list.last_mut() {
            c.pet = Some(VPet { kind: pk, body: Rgb::hex(b), marks: Rgb::hex(m), name: pets::NAMES[coat % pets::NAMES.len()], x: c.x - c.dir * 3.0, y: c.y, dir: c.dir, walking: false, happy_until: 0.0 });
            c.st = St::Idle { until: t + 30.0 };
        }
    }

    /// For previews: a little group out for a walk.
    pub fn spawn_demo_group(&mut self, x: f32, t: f32) {
        self.spawn_demo(Kind::Walker, x, t);
        let id = self.list.last().map(|c| c.id).unwrap_or(0);
        self.companions(id, 2, t);
    }


    pub fn rebase(&mut self, s: f32) {
        self.next_fight -= s;
        for f in &mut self.fights {
            f.until -= s;
            f.next_line -= s;
        }
        for c in &mut self.list {
            c.social_at -= s;
            c.leave_at -= s;
            c.chatter_at -= s;
            match &mut c.st {
                St::Idle { until }
                | St::Graze { until }
                | St::Dizzy { until }
                | St::Petting { until }
                | St::Petted { until }
                | St::Greet { until }
                | St::Flee { until }
                | St::Curious { until }
                | St::Throw { until }
                | St::Follow { until, .. }
                | St::Talk { until, .. }
                | St::Gather { until, .. }
                | St::Chase { until, .. } => *until -= s,
                St::Munch { until, bite, .. } => {
                    *until -= s;
                    *bite -= s;
                }
                St::Held { since, .. } => *since -= s,
                St::Walk | St::Fall { .. } => {}
            }
        }
        for p in &mut self.prints {
            p.2 -= s;
        }
        for b in &mut self.bushes {
            b.shake_until -= s;
            for br in &mut b.berries {
                br.2 -= s;
            }
        }
        self.startle_cd -= s;
        self.follow_chat_at -= s;
        self.react_cd -= s;
        for c in &mut self.convs {
            c.3 -= s;
        }
        for r in &mut self.revenge {
            r.1 -= s;
        }
    }

    // ------------------------------------------------------------ handling

    fn index_at(&self, x: i32, y: i32, t: f32) -> Option<usize> {
        // nearest (front-most) first
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in self.list.iter().enumerate() {
            if c.kind == Kind::Train {
                continue;
            }
            let (bx, by, bw, bh) = c.bbox(t, self.k);
            // a little forgiveness around small sprites
            if x >= bx - 1 && x <= bx + bw && y >= by - 1 && y <= by + bh - 1 && best.map(|b| c.y > b.1).unwrap_or(true) {
                best = Some((i, c.y));
            }
        }
        best.map(|b| b.0)
    }

    /// Is there someone under the cursor to pick up?
    pub fn grabbable(&self, x: i32, y: i32, t: f32) -> bool {
        self.index_at(x, y, t).is_some()
    }

    pub fn bush_at(&self, x: i32, y: i32) -> Option<usize> {
        self.bushes.iter().position(|b| b.contains(x, y))
    }

    pub fn grab(&mut self, x: i32, y: i32, t: f32, d: &mut Director) -> bool {
        let Some(i) = self.index_at(x, y, t) else { return false };
        let c = &mut self.list[i];
        c.st = St::Held { ox: x as f32 - c.x, oy: y as f32 - c.y, since: t };
        c.chatter_at = t + 0.6;
        self.held = Some(c.id);
        self.grab_at = (x as f32, y as f32);
        self.moved = false;
        d.sound(Sfx::Pickup, c.centre(), 1.0);
        true
    }

    pub fn drag(&mut self, x: i32, y: i32) {
        let Some(id) = self.held else { return };
        if (x as f32 - self.grab_at.0).abs() + (y as f32 - self.grab_at.1).abs() >= 1.0 {
            self.moved = true;
        }
        let (w, h) = (self.w as f32, self.h as f32);
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
            if let St::Held { ox, oy, .. } = c.st {
                c.x = (x as f32 - ox).clamp(-4.0, w - 2.0);
                c.y = (y as f32 - oy).clamp(2.0, h - 1.0);
            }
        }
    }

    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    /// Put whoever's held straight back down where they were picked up
    /// (a long press turned out to want the menu, not a carry).
    pub fn let_go(&mut self, t: f32) {
        let Some(id) = self.held.take() else { return };
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
            c.st = St::Idle { until: t + 1.0 };
        }
    }

    /// Let go: a click without dragging is a friendly poke; otherwise they
    /// drop back into the snow.
    pub fn release(&mut self, t: f32, d: &mut Director, lines: &mut Lines) {
        let Some(id) = self.held.take() else { return };
        let moved = self.moved;
        let lo = |x: f32, s: &Self| s.lo_at(x);
        let Some(i) = self.list.iter().position(|c| c.id == id) else { return };
        let ground = lo(self.list[i].centre(), self);
        let h = (self.h - 1) as f32;
        let c = &mut self.list[i];
        if !moved {
            // a poke / a pet
            let (cx, top) = (c.centre(), c.y - 3.0);
            if c.animal() {
                c.st = St::Petted { until: t + 2.5 };
                d.emote(cx, top, '♥', Rgb::hex(0xff6b8a), 3);
                if let Some(s) = c.sfx() {
                    d.sound(s, cx, 1.0);
                }
                d.sound(Sfx::Pet, cx, 0.6);
            } else {
                c.st = St::Greet { until: t + 3.0 };
                let msg = lines.poked(c.kind == Kind::Kid);
                d.bubble(cx as i32, top as i32, &msg, t);
                if let Some(s) = c.sfx() {
                    d.sound(s, cx, 1.0);
                }
            }
            c.social_at = c.social_at.max(t + 4.0);
            return;
        }
        let to = c.y.round().clamp(ground, h);
        c.st = St::Fall { vy: 0.0, to, from: c.y };
    }

    // ------------------------------------------------------------ update

    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, ctx: &Ctx, d: &mut Director, lines: &mut Lines, cursor: Option<(f32, f32, f32, f32)>, snowman: Option<(i32, i32)>) {
        let (w, h) = (self.w as f32, (self.h - 1) as f32);
        self.view = (ctx.cam, ctx.vw as f32);
        let (v0, vw) = self.view;
        // keep a few around the view (scaled by the setting)
        let floor = (1.5 * self.mult).round() as usize;
        let free = self.list.iter().filter(|c| !c.held() && c.x > v0 - vw * 0.5 && c.x < v0 + vw * 1.5).count();
        if free < floor && self.rng.chance(dt * 0.08) {
            let k = *self.rng.pick(&[Kind::Deer, Kind::Fox, Kind::Rabbit, Kind::Walker, Kind::Rabbit, Kind::Deer, Kind::Kid]);
            self.spawn(k, t, None);
            if k == Kind::Kid {
                self.spawn(Kind::Kid, t, None);
            }
        }

        // ---- per-critter movement and timers
        let ids: Vec<u64> = self.list.iter().map(|c| c.id).collect();
        let player = self.player;
        // where each group's leader is and whether they're walking on
        let leaders: Vec<(u64, f32, f32, bool)> = self.list.iter().map(|c| (c.id, c.x, c.dir, matches!(c.st, St::Walk))).collect();
        // followers line up behind the player in the order they joined
        let mut queue: Vec<u64> = self.list.iter().filter(|c| matches!(c.st, St::Follow { .. } | St::Talk { resume: true, .. })).map(|c| c.id).collect();
        queue.sort_unstable();
        for i in 0..self.list.len() {
            let lo = self.lo_at(self.list[i].centre());
            let rng = &mut self.rng;
            let c = &mut self.list[i];
            c.age += dt;
            let wk = self.k;
            let mv = |c: &mut Critter, k: f32| {
                let k = k * wk;
                let hop = if c.kind == Kind::Rabbit { ((c.age * 3.0).fract() < 0.5) as i32 as f32 * 1.6 } else { 1.0 };
                c.x += c.dir * c.speed * k * dt * hop;
            };
            match c.st {
                St::Held { .. } => {
                    // the occasional remark while being carried
                    if t > c.chatter_at {
                        c.chatter_at = t + rng.range(2.2, 4.5);
                        let (cx, top) = (c.centre(), c.y - 4.0);
                        if c.person() && rng.chance(0.7) {
                            let msg = lines.held(c.kind == Kind::Kid);
                            d.bubble(cx as i32, top as i32, &msg, t);
                        } else if let Some(s) = c.sfx() {
                            d.sound(s, cx, 0.7);
                            if c.animal() {
                                d.emote(cx, top + 1.0, '!', Rgb::hex(0xffd36b), 1);
                            }
                        }
                    }
                }
                St::Fall { vy, to, from } => {
                    let vy = vy + 70.0 * dt;
                    c.y += vy * dt;
                    c.st = St::Fall { vy, to, from };
                    if c.y >= to {
                        c.y = to;
                        let (cx, high) = (c.centre(), to - from > 3.0);
                        d.snow_puff(cx, to);
                        d.sound(Sfx::Drop, cx, if high { 1.0 } else { 0.6 });
                        if c.person() && high {
                            c.st = St::Dizzy { until: t + 2.2 };
                            let msg = lines.dropped();
                            d.bubble(cx as i32, (to - 4.0) as i32, &msg, t + 0.4);
                            if c.kind == Kind::Kid {
                                d.sound(Sfx::Laugh, cx, 0.8);
                            }
                        } else if c.animal() && high {
                            // a startled little dash, then back to normal
                            c.st = St::Flee { until: t + 1.4 };
                            if rng.chance(0.5) {
                                c.dir = -c.dir;
                            }
                        } else {
                            c.st = St::Idle { until: t + 0.6 };
                        }
                    }
                }
                St::Walk if c.group != 0 => {
                    // keep pace with the rest of the group
                    match leaders.iter().find(|l| l.0 == c.group) {
                        Some(&(_, lx, ldir, true)) => {
                            c.dir = ldir;
                            let behind = (lx - ldir * c.gap - c.x) * ldir;
                            mv(c, (1.0 + behind * 0.35).clamp(0.3, 1.9));
                        }
                        Some(_) => {} // they've stopped; wait for them
                        None => c.group = 0,
                    }
                    if c.y < lo {
                        c.y = (c.y + dt * 2.0).min(lo);
                    }
                }
                St::Follow { until, .. } => {
                    let mut moving = false;
                    if let Some((px, py, pdir, _)) = player {
                        let rank = queue.iter().position(|&id| id == c.id).unwrap_or(0) as f32;
                        let tx = px - pdir * (4.5 + rank * 4.0) - 0.5;
                        let dx = tx - c.x;
                        if dx.abs() > 1.2 {
                            c.dir = dx.signum();
                            // hurry to keep up (the player walks at ~11 cells/s)
                            let v = (dx.abs() * 2.5).clamp(c.speed, 40.0);
                            c.x += c.dir * v.min(dx.abs() / dt.max(1e-3)) * dt;
                            moving = true;
                        } else {
                            c.dir = pdir;
                        }
                        let ty = py.clamp(lo, h);
                        let dy = ty - c.y;
                        if dy.abs() > 0.3 {
                            c.y += dy.signum() * (dt * 6.0).min(dy.abs());
                        }
                    }
                    c.st = if t > until || player.is_none() { St::Walk } else { St::Follow { until, moving } };
                }
                St::Gather { tx, ty, face, until, .. } => {
                    let dx = tx - c.x;
                    let mut moving = false;
                    if dx.abs() > 1.0 {
                        c.dir = dx.signum();
                        c.x += c.dir * (c.speed * 1.7).min(dx.abs() / dt.max(1e-3)) * dt;
                        moving = true;
                    } else {
                        c.dir = (face - c.centre()).signum();
                        if rng.chance(dt * 0.08) {
                            d.emote(c.centre(), c.y - 4.0, if rng.chance(0.5) { '✦' } else { '♥' }, Rgb::hex(0xffd36b), 1);
                        }
                    }
                    let dy = ty.clamp(lo, h) - c.y;
                    if dy.abs() > 0.3 {
                        c.y += dy.signum() * (dt * 2.5).min(dy.abs());
                        moving = true;
                    }
                    c.st = if t > until { St::Walk } else { St::Gather { tx, ty, face, until, moving } };
                }
                St::Talk { until, resume } => {
                    if t > until {
                        c.st = if resume && player.is_some() && t < c.leave_at { St::Follow { until: c.leave_at, moving: false } } else { St::Walk };
                    }
                }
                St::Walk => {
                    mv(c, 1.0);
                    // animals that are sticking around turn back at the edges
                    if t < c.leave_at && ((c.x < 4.0 && c.dir < 0.0) || (c.x > w - 10.0 && c.dir > 0.0)) {
                        c.dir = -c.dir;
                    }
                    if c.kind == Kind::Deer && rng.chance(dt * 0.07) {
                        c.st = St::Graze { until: t + rng.range(2.0, 5.0) };
                    } else if c.animal() && rng.chance(dt * 0.03) {
                        c.st = St::Idle { until: t + rng.range(1.0, 3.0) };
                    }
                    // drift gently toward a comfortable depth on the snowfield
                    if c.y < lo {
                        c.y = (c.y + dt * 2.0).min(lo);
                    }
                }
                St::Flee { until } => {
                    mv(c, 2.4);
                    if t > until {
                        c.st = St::Walk;
                    }
                }
                St::Curious { until } => {
                    if let Some((cx, cy, _, _)) = cursor {
                        let dx = cx - c.centre();
                        if dx.abs() > 4.0 {
                            c.dir = dx.signum();
                            mv(c, 0.6);
                        } else {
                            c.st = St::Idle { until: t + rng.range(2.0, 4.0) };
                            d.emote(c.centre(), c.y - 3.0, if rng.chance(0.5) { '♥' } else { '?' }, Rgb::hex(0xff9ab0), 1);
                        }
                        let _ = cy;
                    }
                    if t > until {
                        c.st = St::Walk;
                    }
                }
                St::Chase { target, until } => {
                    mv(c, 1.6);
                    if t > until || !ids.contains(&target) {
                        c.st = St::Idle { until: t + 1.5 };
                        if c.kind == Kind::Fox {
                            let msg = lines.fox_gives_up();
                            d.bubble(c.centre() as i32, c.y as i32 - 3, &msg, t);
                        }
                    }
                }
                St::Munch { bush, until, bite } => {
                    if t >= bite {
                        let b = &mut self.bushes[bush];
                        if let Some(br) = b.berries.iter_mut().filter(|br| t >= br.2).nth(0) {
                            br.2 = t + rng.range(60.0, 150.0); // regrows
                            b.shake_until = t + 0.25;
                            d.sound(Sfx::Munch, b.centre(), 0.8);
                            if rng.chance(0.4) {
                                d.emote(c.centre(), c.y - 3.0, '♥', Rgb::hex(0xff6b8a), 1);
                            }
                            c.st = St::Munch { bush, until, bite: t + rng.range(0.7, 1.2) };
                        } else {
                            c.st = St::Walk; // all gone
                        }
                    }
                    if t > until {
                        c.st = St::Walk;
                    }
                }
                St::Idle { until }
                | St::Graze { until }
                | St::Dizzy { until }
                | St::Petting { until }
                | St::Petted { until }
                | St::Greet { until }
                | St::Throw { until } => {
                    if t > until {
                        c.st = St::Walk;
                    }
                }
            }
            if matches!(c.st, St::Walk | St::Flee { .. } | St::Chase { .. } | St::Curious { .. }) && rng.chance(dt * 1.5) {
                self.prints.push((c.centre() as i32, c.y as i32, t));
            }
        }
        self.avoid(dt);
        let held = self.held;
        // gone: off the world, or passers-by wandered well out of view
        self.list.retain(|c| {
            Some(c.id) == held || (c.x > -40.0 && c.x < w + 40.0 && !(t > c.leave_at && (c.x < v0 - vw * 1.2 || c.x > v0 + vw * 2.2)))
        });
        self.prints.retain(|p| t - p.2 < 30.0);

        self.meetings(t, d, lines, snowman);
        self.group_chats(dt, t, d, lines);
        self.chime_ins(dt, t, d);
        self.follower_chats(dt, t, d, lines);
        self.cursor_reactions(dt, t, d, cursor);
        self.snack(dt, t);

        self.fights_update(dt, t, d);
        // pets keep up with their people
        for c in &mut self.list {
            let (cx, cy, cdir, held) = (c.centre(), c.y, c.dir, c.held() || matches!(c.st, St::Fall { .. }));
            let ground = self.lo.get((cx.max(0.0) as usize).min(self.lo.len().saturating_sub(1))).copied().unwrap_or(0) as f32;
            let Some(p) = &mut c.pet else { continue };
            // a step behind (the dog a lead's length), and on the ground even
            // while their person is up in the air
            let tx = cx - cdir * if p.kind == PetKind::Dog { 4.0 } else { 3.0 } * self.k;
            let ty = if held { p.y.max(ground) } else { cy };
            let dx = tx - p.x;
            p.walking = dx.abs() > 0.6;
            if p.walking {
                let v = (dx.abs() * 3.0).clamp(2.0, if held { 14.0 } else { 30.0 }) * self.k;
                p.x += dx.signum() * (v * dt).min(dx.abs());
                p.dir = dx.signum();
            } else {
                p.dir = cdir;
            }
            p.y += (ty - p.y) * (1.0 - (-dt * 4.0).exp());
        }

        // snowballs
        let mut hits = vec![];
        for b in &mut self.balls {
            b.age += dt;
            b.vy += 30.0 * dt;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
        }
        let list = &self.list;
        let you = self.player;
        let mut on_you = 0;
        let mut landed = vec![];
        self.balls.retain(|b| {
            if b.target == AT_YOU {
                if let Some((px, py, ..)) = you {
                    if (b.x - (px + 1.0)).abs() < 1.5 && b.y >= py - 3.0 && b.y <= py + 0.5 {
                        on_you += 1;
                        landed.push((b.x, b.y));
                        return false;
                    }
                }
            } else if let Some(c) = list.iter().find(|c| c.id == b.target) {
                if (b.x - c.centre()).abs() < 1.2 && b.y >= c.y - 3.0 {
                    hits.push((b.x, b.y, c.id, b.yours));
                    return false;
                }
            }
            // missed: it lands in the snow
            if b.vy > 0.0 && b.age > 0.15 && b.y >= b.land {
                landed.push((b.x, b.y));
                return false;
            }
            b.age < 3.0 && b.y < h + 2.0
        });
        for (x, y) in landed {
            d.snow_puff(x, y);
            d.sound(Sfx::Snowball, x, 0.5);
        }
        self.hits_on_you += on_you;
        for (x, y, id, yours) in hits {
            d.snow_puff(x, y);
            d.sound(Sfx::Snowball, x, 0.9);
            let Some(i) = self.list.iter().position(|c| c.id == id) else { continue };
            let kid = self.list[i].kind == Kind::Kid;
            if !yours {
                d.sound(Sfx::Laugh, x, 0.8);
                let c = &mut self.list[i];
                c.st = St::Dizzy { until: t + 1.0 };
                // half the time they throw one back
                if self.rng.chance(0.5) {
                    c.social_at = t + 1.2;
                }
                continue;
            }
            // the player's snowball: everyone takes it a little differently
            self.your_hits += 1;
            self.your_hit_ids.push(id);
            if self.list[i].animal() {
                let away = (self.list[i].centre() - you.map(|p| p.0).unwrap_or(x)).signum();
                let c = &mut self.list[i];
                c.st = St::Flee { until: t + 2.0 };
                c.dir = if away == 0.0 { 1.0 } else { away };
                d.emote(c.centre(), c.y - 3.0, '!', Rgb::hex(0xffd36b), 1);
                if let Some(s) = c.sfx() {
                    d.sound(s, c.centre(), 0.8);
                }
                continue;
            }
            let msg = lines.hit(kid);
            let back = self.rng.chance(if kid { 0.75 } else { 0.35 });
            let c = &mut self.list[i];
            c.st = St::Dizzy { until: t + 1.0 };
            d.say(c.centre().round() as i32, c.y as i32 - 4, &msg, t + 0.3, 2.6, 6);
            if kid {
                d.sound(Sfx::Laugh, c.centre(), 0.9);
            }
            if back {
                self.revenge.push((id, t + self.rng.range(1.4, 2.4)));
            }
        }
        // ...and here it comes back at the player
        if let Some((px, py, ..)) = you {
            let due: Vec<u64> = self.revenge.iter().filter(|r| t >= r.1).map(|r| r.0).collect();
            self.revenge.retain(|r| t < r.1);
            for id in due {
                let Some(c) = self.list.iter_mut().find(|c| c.id == id) else { continue };
                if !c.free() && !matches!(c.st, St::Dizzy { .. } | St::Walk) {
                    continue;
                }
                let (ax, ay) = (c.centre(), c.y);
                let dist = px + 1.0 - ax;
                if dist.abs() > 45.0 {
                    continue; // you got away
                }
                c.dir = dist.signum();
                c.st = St::Throw { until: t + 0.8 };
                let flight = (dist.abs() / 30.0).clamp(0.35, 0.9);
                self.balls.push(Snowball { x: ax, y: ay - 2.0, vx: dist / flight, vy: (py - ay) / flight - 15.0 * flight, target: AT_YOU, age: 0.0, yours: false, land: py + 0.5 });
                d.sound(Sfx::Snowball, ax, 0.5);
            }
        } else {
            self.revenge.clear();
        }
        // birds flying off
        for b in &mut self.birds {
            b.age += dt;
            b.vy -= 4.0 * dt;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
        }
        self.birds.retain(|b| b.age < 8.0 && b.y > -2.0 && b.x > -4.0 && b.x < w + 4.0);
    }

    /// Two characters bump into each other.
    fn meetings(&mut self, t: f32, d: &mut Director, lines: &mut Lines, snowman: Option<(i32, i32)>) {
        let n = self.list.len();
        for i in 0..n {
            let a = &self.list[i];
            if !matches!(a.st, St::Walk) || t < a.social_at {
                continue;
            }
            // people saying hi to the snowman
            if let (true, Some((sx, sy))) = (a.person(), snowman) {
                if (a.centre() - sx as f32).abs() < 6.0 && self.rng.chance(0.5) {
                    let (hi, reply) = lines.to_snowman();
                    let a = &mut self.list[i];
                    a.st = St::Greet { until: t + 3.0 };
                    a.dir = (sx as f32 - a.centre()).signum();
                    a.social_at = t + 40.0;
                    d.bubble(a.centre() as i32, a.y as i32 - 3, &hi, t);
                    d.bubble(sx, sy, &reply, t + 1.2);
                    d.sound(Sfx::Greet, a.centre(), 0.8);
                    d.sound(Sfx::Snowman, sx as f32, 0.6);
                    continue;
                }
            }
            for j in 0..n {
                if i == j {
                    continue;
                }
                let (a, b) = (&self.list[i], &self.list[j]);
                if !b.free() || t < b.social_at || (a.centre() - b.centre()).abs() > 7.0 || (a.y - b.y).abs() > 3.0 {
                    continue;
                }
                // companions already walking together chat on their own schedule
                if a.group == b.id || b.group == a.id || (a.group != 0 && a.group == b.group) {
                    continue;
                }
                let (ak, bk) = (a.kind, b.kind);
                let towards = (b.centre() - a.centre()).signum();
                let (ax, bx, ay, by) = (a.centre(), b.centre(), a.y, b.y);
                let cool = |s: &mut Self, k: usize, secs: f32| {
                    let r = s.rng.range(secs, secs * 2.0);
                    s.list[k].social_at = t + r;
                };
                if a.person() && b.animal() {
                    // a pat for the animal
                    self.list[i].st = St::Petting { until: t + 3.5 };
                    self.list[i].dir = towards;
                    self.list[j].st = St::Petted { until: t + 3.5 };
                    self.list[j].dir = -towards;
                    let which = match bk {
                        Kind::Deer => 0,
                        Kind::Fox => 1,
                        _ => 2,
                    };
                    let msg = lines.pet(which);
                    d.bubble(ax as i32, ay as i32 - 3, &msg, t);
                    d.emote(bx, by - 3.0, '♥', Rgb::hex(0xff6b8a), 3);
                    d.sound(Sfx::Pet, ax, 0.8);
                    if let Some(s) = self.list[j].sfx() {
                        d.sound(s, bx, 0.6);
                    }
                } else if a.person() && b.person() && self.rng.chance((if ak == Kind::Kid && bk == Kind::Kid { 0.3 } else { 0.45 }) * super::events::chattiness()) {
                    // they stop for a proper chat
                    self.converse(&[i, j], t, d, lines);
                } else if ak == Kind::Walker && bk == Kind::Walker {
                    let (an, bn) = (self.list[i].name, self.list[j].name);
                    let (m1, m2) = self.talk.greet(&self.tctx, ax, an, bn);
                    self.list[i].st = St::Greet { until: t + 3.2 };
                    self.list[i].dir = towards;
                    self.list[j].st = St::Greet { until: t + 3.2 };
                    self.list[j].dir = -towards;
                    d.bubble(ax as i32, ay as i32 - 4, &m1, t);
                    d.bubble(bx as i32, by as i32 - 4, &m2, t + 1.3);
                    d.sound(Sfx::Greet, ax, 0.8);
                    // two pets meeting: noses, then a happy bounce about
                    if let (Some(pa), Some(pb)) = (self.list[i].pet.as_ref().map(|p| (p.x, p.y)), self.list[j].pet.as_ref().map(|p| (p.x, p.y))) {
                        for k in [i, j] {
                            if let Some(p) = &mut self.list[k].pet {
                                p.happy_until = t + 2.8;
                            }
                        }
                        d.emote((pa.0 + pb.0) / 2.0, pa.1.min(pb.1) - 2.0, '♥', Rgb::hex(0xff6b8a), 2);
                        d.sound(Sfx::Bark, pa.0, 0.35);
                    }
                } else if ak == Kind::Kid && bk == Kind::Kid {
                    // snowball!
                    self.list[i].st = St::Throw { until: t + 0.8 };
                    self.list[i].dir = towards;
                    let dist = bx - ax;
                    let flight = 0.55;
                    self.balls.push(Snowball { x: ax, y: ay - 2.0, vx: dist / flight, vy: (by - ay) / flight - 15.0 * flight, target: self.list[j].id, age: 0.0, yours: false, land: by + 0.5 });
                    d.sound(Sfx::Snowball, ax, 0.5);
                } else if ak == Kind::Rabbit && bk == Kind::Rabbit {
                    // tag! one hops off, the other gives chase (and then the
                    // other way round, next time they meet)
                    self.list[j].st = St::Flee { until: t + 2.2 };
                    self.list[j].dir = towards;
                    let rid = self.list[j].id;
                    self.list[i].st = St::Chase { target: rid, until: t + 2.0 };
                    self.list[i].dir = towards;
                    d.emote((ax + bx) / 2.0, ay.min(by) - 2.0, '♥', Rgb::hex(0xff9ab0), 1);
                    d.sound(Sfx::Rabbit, ax, 0.4);
                } else if ak == Kind::Deer && bk == Kind::Deer {
                    // a nuzzle, nose to nose
                    self.list[i].st = St::Idle { until: t + 3.0 };
                    self.list[i].dir = towards;
                    self.list[j].st = St::Idle { until: t + 3.0 };
                    self.list[j].dir = -towards;
                    d.emote((ax + bx) / 2.0, ay.min(by) - 3.0, '♥', Rgb::hex(0xff6b8a), 2);
                    d.sound(Sfx::Deer, ax, 0.4);
                } else if ak == Kind::Fox && bk == Kind::Rabbit {
                    // a playful chase (the fox never wins)
                    self.list[j].st = St::Flee { until: t + 3.5 };
                    self.list[j].dir = towards;
                    let rid = self.list[j].id;
                    self.list[i].st = St::Chase { target: rid, until: t + 3.0 };
                    self.list[i].dir = towards;
                    d.sound(Sfx::Fox, ax, 0.7);
                    d.emote(bx, by - 2.0, '!', Rgb::hex(0xffd36b), 1);
                } else {
                    continue;
                }
                cool(self, i, 18.0);
                cool(self, j, 18.0);
                break;
            }
        }
    }

    /// Nobody walks straight through anybody: someone heading into another
    /// person or animal (or the player) on their row steps a row up or down
    /// to go round them. (When they meet properly, `meetings` has them stop
    /// and chat first; they step round once they set off again.)
    fn avoid(&mut self, dt: f32) {
        let h = (self.h - 1) as f32;
        let you = self.player.map(|p| (u64::MAX, p.0 + 1.0, p.1, 0u64, false));
        let walking = |c: &Critter| matches!(c.st, St::Walk | St::Flee { .. } | St::Chase { .. } | St::Curious { .. } | St::Follow { moving: true, .. } | St::Gather { moving: true, .. });
        let others: Vec<(u64, f32, f32, u64, bool)> = self
            .list
            .iter()
            .filter(|c| !c.held() && !matches!(c.st, St::Fall { .. }))
            .map(|c| (c.id, c.centre(), c.y, c.group, walking(c)))
            .chain(you)
            .collect();
        for i in 0..self.list.len() {
            let c = &self.list[i];
            if !walking(c) {
                self.list[i].dodge = 0.0;
                continue;
            }
            let (id, cx, cy, dir, group) = (c.id, c.centre(), c.y, c.dir, c.group);
            let reach = if c.animal() { 6.0 } else { 4.0 };
            // the nearest one in the way, on (about) the same row
            let block = others
                .iter()
                .filter(|o| o.0 != id && o.0 != group && o.3 != id && !(group != 0 && o.3 == group))
                .filter(|o| {
                    let ahead = (o.1 - cx) * dir;
                    ahead > 0.0 && ahead < reach && (o.2 - cy).abs() < 1.6
                })
                .min_by(|a, b| ((a.1 - cx) * dir).partial_cmp(&((b.1 - cx) * dir)).unwrap_or(std::cmp::Ordering::Equal));
            let Some(&(oid, _, oy, _, other_walking)) = block else {
                self.list[i].dodge = 0.0;
                continue;
            };
            // one of a pair gives way (the later one); anyone stepping round
            // someone standing still or the player does the stepping
            if other_walking && oid != u64::MAX && oid > id {
                continue;
            }
            let lo = self.lo_at(cx);
            // pick a side once (away from them, where there's room) and keep
            // to it until clear
            let c = &mut self.list[i];
            if c.dodge == 0.0 {
                let away = if oy > cy { -1.0 } else if oy < cy { 1.0 } else { -1.0 };
                let room = |d: f32| if d < 0.0 { cy - lo } else { h - cy };
                c.dodge = if room(away) >= 1.6 - (oy - cy).abs() { away } else { -away };
            }
            let ny = c.y + c.dodge * 4.0 * dt;
            if ny < lo || ny > h {
                c.dodge = -c.dodge;
            }
            c.y = ny.clamp(lo, h);
        }
    }

    /// A light show's about to start: the townsfolk nearby gather round the
    /// tree at column `x` (in a loose crowd in front of it) until `until`.
    /// `line`: what the first one in view says about it.
    pub fn gather(&mut self, x: f32, until: f32, t: f32, d: &mut Director, line: &str) {
        let h = (self.h - 1) as f32;
        let mut k = 0;
        let mut announced = false;
        for i in 0..self.list.len() {
            let c = &self.list[i];
            if !c.person() || c.held() || !matches!(c.st, St::Walk | St::Idle { .. } | St::Greet { .. } | St::Talk { resume: false, .. }) || (c.centre() - x).abs() > self.view.1 * 1.5 {
                continue;
            }
            // alternate sides, spreading out as more arrive
            let side = if k % 2 == 0 { -1.0 } else { 1.0 };
            let tx = x + side * (7.0 + (k / 2) as f32 * 4.5 + self.rng.range(0.0, 2.0));
            let lo = self.lo_at(tx);
            let ty = self.rng.range(lo + 1.0, h).round();
            let (cx, cy) = (c.centre(), c.y);
            self.list[i].st = St::Gather { tx, ty, face: x, until, moving: true };
            self.list[i].leave_at = self.list[i].leave_at.max(until);
            if !announced && (cx - self.view.0) > 0.0 && (cx - self.view.0) < self.view.1 {
                announced = true;
                d.say(cx.round() as i32, cy as i32 - 4, line, t, 3.0, 6);
            }
            k += 1;
        }
    }

    /// A conversation between two or three people (indices into the list):
    /// they stop, face each other and take turns.
    fn converse(&mut self, idx: &[usize], t: f32, d: &mut Director, lines: &mut Lines) {
        let n = idx.len().min(3);
        if n < 2 {
            return;
        }
        let idx = &idx[..n];
        let _ = lines;
        let names: Vec<&str> = idx.iter().map(|&i| self.list[i].name).collect();
        let kids: Vec<bool> = idx.iter().map(|&i| self.list[i].kind == Kind::Kid).collect();
        let mid = idx.iter().map(|&i| self.list[i].centre()).sum::<f32>() / n as f32;
        let script = self.talk.conversation(&self.tctx, mid, &names, &kids);
        if script.is_empty() {
            return;
        }
        let mut at = t + 0.3;
        for (sp, line) in &script {
            let c = &self.list[idx[(*sp).min(n - 1)]];
            let life = (1.8 + line.chars().count() as f32 * 0.06) * super::events::talk_time();
            d.say(c.centre().round() as i32, c.y as i32 - 4, line, at, life, 40);
            at += life + 0.2;
        }
        let row = self.list[idx[0]].y;
        self.convs.push((idx.iter().map(|&i| self.list[i].id).collect(), mid, row, at, false));
        for &i in idx {
            let resume = matches!(self.list[i].st, St::Follow { .. });
            let r = self.rng.range(25.0, 50.0);
            let c = &mut self.list[i];
            let to = mid - c.centre();
            if to.abs() > 0.5 {
                c.dir = to.signum();
            }
            c.st = St::Talk { until: at + 0.4, resume };
            c.social_at = at + r;
        }
        d.sound(Sfx::Greet, mid, 0.6);
    }

    /// Groups out for a walk stop for a chat now and then.
    fn group_chats(&mut self, dt: f32, t: f32, d: &mut Director, lines: &mut Lines) {
        for i in 0..self.list.len() {
            let l = &self.list[i];
            if l.group != 0 || !l.person() || !matches!(l.st, St::Walk) || t < l.social_at || !self.rng.chance(dt * 0.07 * super::events::chattiness()) {
                continue;
            }
            let (id, lx) = (l.id, l.centre());
            let mut g = vec![i];
            g.extend(
                (0..self.list.len()).filter(|&j| self.list[j].group == id && matches!(self.list[j].st, St::Walk) && (self.list[j].centre() - lx).abs() < 12.0),
            );
            if g.len() >= 2 {
                self.converse(&g, t, d, lines);
                return;
            }
        }
    }

    /// While the player stands still, whoever's following them chats —
    /// with each other, or with someone the player has brought them to.
    fn follower_chats(&mut self, dt: f32, t: f32, d: &mut Director, lines: &mut Lines) {
        let walking = self.player.map(|p| p.3).unwrap_or(true);
        self.still = if walking { 0.0 } else { self.still + dt };
        if self.still < 2.0 || t < self.follow_chat_at {
            return;
        }
        let fol: Vec<usize> = (0..self.list.len()).filter(|&i| self.list[i].person() && matches!(self.list[i].st, St::Follow { moving: false, .. })).collect();
        if fol.is_empty() {
            return;
        }
        let near = |s: &Self, c: &Critter| fol.iter().any(|&i| (s.list[i].centre() - c.centre()).abs() < 14.0 && (s.list[i].y - c.y).abs() <= 4.0);
        let other = (0..self.list.len()).find(|&j| {
            let c = &self.list[j];
            c.person() && matches!(c.st, St::Walk | St::Idle { .. }) && t >= c.social_at && near(self, c)
        });
        let mut g: Vec<usize> = other.into_iter().collect();
        g.extend(fol.iter().copied().take(3 - g.len()));
        if g.len() >= 2 {
            self.converse(&g, t, d, lines);
            self.follow_chat_at = t + self.rng.range(12.0, 22.0);
        } else {
            self.follow_chat_at = t + 2.0;
        }
    }

    // ------------------------------------------------------------ the player

    /// (id, centre column, baseline row) of everyone, for speech bubbles.
    pub fn anchors(&self) -> Vec<(u64, f32, f32)> {
        self.list.iter().map(|c| (c.id, c.centre(), c.y)).collect()
    }

    /// Whose pet is at (x, y) (world column, row), if any.
    pub fn pet_at(&self, x: i32, y: i32) -> Option<u64> {
        self.list.iter().find(|c| c.pet.as_ref().is_some_and(|p| (p.x - x as f32 - 0.5).abs() <= pets::half_width(p.kind) && (y as f32 - p.y).abs() <= 1.5 && y as f32 <= p.y + 0.5)).map(|c| c.id)
    }

    /// Someone's pet, petted: it bounces, and its person says who it is.
    pub fn pet_the_pet(&mut self, owner: u64, t: f32, d: &mut Director) {
        let Some(c) = self.list.iter_mut().find(|c| c.id == owner) else { return };
        let (cx, cy, kid) = (c.centre(), c.y, c.kind == Kind::Kid);
        let Some(p) = &mut c.pet else { return };
        p.happy_until = t + 2.5;
        let (px, py) = (p.x, p.y);
        d.emote(px, py - 2.0, '♥', Rgb::hex(0xff6b8a), 3);
        d.sound(match p.kind {
            PetKind::Dog => Sfx::Bark,
            PetKind::Cat => Sfx::Meow,
            PetKind::Bunny => Sfx::Rabbit,
        }, px, 0.8);
        let i = match p.kind {
            PetKind::Dog => 0,
            PetKind::Cat => 1,
            PetKind::Bunny => 2,
        };
        let mut line = self.rng.pick(PET_FACTS[i]).replace("{n}", p.name);
        if kid {
            line = format!("{line}!!");
        }
        d.say(cx.round() as i32, cy as i32 - 4, &line, t + 0.4, 3.0, 6);
    }

    /// Their pet, if they have one: (kind, name).
    /// An animal near (x, y) within `r` columns that's free to play or be
    /// chased: (id, kind name, centre, row).
    pub fn animal_near(&self, x: f32, y: f32, r: f32) -> Option<(u64, &'static str, f32, f32)> {
        self.list
            .iter()
            .filter(|c| c.animal() && c.free() && (c.centre() - x).abs() <= r && (c.y - y).abs() <= 6.0)
            .min_by(|a, b| (a.centre() - x).abs().partial_cmp(&(b.centre() - x).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|c| {
                let k = match c.kind {
                    Kind::Deer => "deer",
                    Kind::Fox => "fox",
                    _ => "rabbit",
                };
                (c.id, k, c.centre(), c.y)
            })
    }

    /// Startled by something at column `from`: off they go, the other way.
    pub fn startle(&mut self, id: u64, from: f32, t: f32, d: &mut Director) {
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id && c.free()) {
            c.dir = (c.centre() - from).signum();
            c.st = St::Flee { until: t + 2.5 };
            d.emote(c.centre(), c.y - 2.0, '!', Rgb::hex(0xffd36b), 1);
        }
    }

    /// A villager's pet near (x, y): (owner id, its column, row, kind).
    pub fn vpet_near(&self, x: f32, y: f32, r: f32) -> Option<(u64, f32, f32, PetKind)> {
        self.list.iter().filter_map(|c| c.pet.as_ref().map(|p| (c.id, p.x, p.y, p.kind))).find(|&(_, px, py, _)| (px - x).abs() <= r && (py - y).abs() <= 3.0)
    }

    /// That villager's pet bounces about happily (playing with another).
    pub fn vpet_play(&mut self, owner: u64, t: f32) {
        if let Some(p) = self.list.iter_mut().find(|c| c.id == owner).and_then(|c| c.pet.as_mut()) {
            p.happy_until = t + 3.0;
        }
    }

    pub fn pet_of(&self, id: u64) -> Option<(PetKind, &'static str)> {
        self.list.iter().find(|c| c.id == id).and_then(|c| c.pet.as_ref().map(|p| (p.kind, p.name)))
    }

    /// Christmas Eve night: everyone heads home to bed (anyone walking with
    /// the player stays), and nobody new comes out.
    pub fn set_asleep(&mut self, on: bool, t: f32, d: &mut Director) {
        if on == self.asleep {
            return;
        }
        self.asleep = on;
        if !on {
            return;
        }
        let mut said = 0;
        for c in &mut self.list {
            if c.person() && c.free() {
                c.leave_at = 0.0;
                c.st = St::Walk;
                if said < 3 && self.rng.chance(0.5) {
                    said += 1;
                    let line = *self.rng.pick(&["off to bed — Santa's coming!", "goodnight, everyone!", "early night tonight!", "can't let Santa catch me awake!", "night night!"]);
                    d.say(c.centre().round() as i32, c.y as i32 - 4, line, t + said as f32 * 0.8, 2.6, 6);
                }
            }
        }
    }

    /// Kids having snowball fights: now and then a few in view square off
    /// in two sides and pelt each other for half a minute or so.
    fn fights_update(&mut self, dt: f32, t: f32, d: &mut Director) {
        let (v0, vw) = self.view;
        if !self.asleep && self.fights.is_empty() && t > self.next_fight {
            self.next_fight = t + self.rng.range(90.0, 220.0);
            let mut kids: Vec<(u64, f32)> = self.list.iter().filter(|c| c.kind == Kind::Kid && c.free() && c.x > v0 + 4.0 && c.x < v0 + vw - 4.0).map(|c| (c.id, c.centre())).collect();
            if kids.len() < 2 {
                // round up a couple more kids for next time
                self.request(Kind::Kid, t);
                self.request(Kind::Kid, t);
                self.next_fight = t + self.rng.range(15.0, 30.0);
            } else {
                kids.truncate(4);
                let mid = (kids.iter().map(|k| k.1).sum::<f32>() / kids.len() as f32).clamp(v0 + 24.0, v0 + vw - 24.0);
                let h = (self.h - 1) as f32;
                let mut sides: [Vec<(u64, f32, f32)>; 2] = [vec![], vec![]];
                for (k, &(id, _)) in kids.iter().enumerate() {
                    let side = k % 2;
                    let sign = if side == 0 { -1.0 } else { 1.0 };
                    let x = mid + sign * (self.rng.range(9.0, 13.0) + (k / 2) as f32 * 3.0);
                    let lo = self.lo_at(x);
                    let y = self.rng.range(lo + 1.0, h).round();
                    sides[side].push((id, x, y));
                }
                let until = t + self.rng.range(20.0, 35.0);
                for side in &sides {
                    for &(id, x, y) in side {
                        self.work_at(id, x, y, mid, until);
                    }
                }
                if let Some(&(id, ..)) = sides[0].first() {
                    if let Some((x, y)) = self.position(id) {
                        let line = *self.rng.pick(FIGHT_START);
                        d.say(x.round() as i32, y as i32 - 4, line, t, 2.4, 6);
                        d.sound(Sfx::Laugh, x, 0.6);
                    }
                }
                self.fights.push(Fight { sides, mid, until, next_line: t + 4.0 });
            }
        }
        let mut fights = std::mem::take(&mut self.fights);
        for f in &mut fights {
            // anyone who's been carried off or gone home is out of it
            for side in &mut f.sides {
                side.retain(|&(id, ..)| self.list.iter().any(|c| c.id == id && !c.held()));
            }
            let over = t > f.until || f.sides.iter().any(|s| s.is_empty());
            if over {
                let all: Vec<u64> = f.sides.iter().flatten().map(|m| m.0).collect();
                if let Some((x, y)) = all.first().and_then(|&id| self.position(id)) {
                    let line = *self.rng.pick(FIGHT_END);
                    d.say(x.round() as i32, y as i32 - 4, line, t, 2.6, 6);
                }
                for id in all {
                    self.stop_work(id, t);
                    if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
                        if matches!(c.st, St::Dizzy { .. } | St::Throw { .. }) {
                            c.st = St::Walk;
                        }
                    }
                }
                f.until = -1.0;
                continue;
            }
            for s in 0..2 {
                for k in 0..f.sides[s].len() {
                    let (id, x, y) = f.sides[s][k];
                    let Some(i) = self.list.iter().position(|c| c.id == id) else { continue };
                    // back to their spot after a throw or a hit
                    if matches!(self.list[i].st, St::Walk | St::Idle { .. }) {
                        self.list[i].st = St::Gather { tx: x, ty: y, face: f.mid, until: f.until, moving: true };
                    }
                    if !matches!(self.list[i].st, St::Gather { moving: false, .. }) || !self.rng.chance(dt * 0.55) {
                        continue;
                    }
                    // throw at someone on the other side
                    let foes = &f.sides[1 - s];
                    let (fid, ..) = foes[self.rng.int(0, foes.len() as i32) as usize];
                    let Some(j) = self.list.iter().position(|c| c.id == fid) else { continue };
                    let (ax, ay) = (self.list[i].centre(), self.list[i].y);
                    let (bx, by) = (self.list[j].centre(), self.list[j].y);
                    let dist = bx - ax + self.rng.range(-1.5, 1.5);
                    let flight = (dist.abs() / 30.0).clamp(0.4, 0.8);
                    self.list[i].dir = dist.signum();
                    self.list[i].st = St::Throw { until: t + 0.6 };
                    self.balls.push(Snowball { x: ax, y: ay - 2.0, vx: dist / flight, vy: (by - ay) / flight - 15.0 * flight, target: fid, age: 0.0, yours: false, land: by + 0.5 });
                    d.sound(Sfx::Snowball, ax, 0.4);
                }
            }
            if t > f.next_line {
                f.next_line = t + self.rng.range(2.5, 5.0);
                let all: Vec<u64> = f.sides.iter().flatten().map(|m| m.0).collect();
                let who = all[self.rng.int(0, all.len() as i32) as usize];
                if let Some((x, y)) = self.position(who) {
                    let line = *self.rng.pick(FIGHT_MID);
                    d.say(x.round() as i32, y as i32 - 4, line, t, 1.8, 6);
                }
            }
        }
        fights.retain(|f| f.until > 0.0);
        self.fights = fights;
    }

    /// In a conversation with the player: they stand and face them.
    pub fn hold_talk(&mut self, id: u64, px: f32, t: f32) {
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id && (c.free() || matches!(c.st, St::Talk { .. } | St::Greet { .. }))) {
            let resume = matches!(c.st, St::Talk { resume: true, .. });
            c.st = St::Talk { until: t + 1.0, resume };
            let toward = (px - c.centre()).signum();
            if toward != 0.0 {
                c.dir = toward;
            }
            c.social_at = c.social_at.max(t + 8.0);
            c.met = true;
        }
    }

    pub fn is_kid(&self, id: u64) -> bool {
        self.list.iter().any(|c| c.id == id && c.kind == Kind::Kid)
    }

    /// A kid up for a snowball fight with the player: a few throws coming.
    pub fn challenge(&mut self, id: u64, t: f32) {
        for k in 0..3 {
            self.revenge.push((id, t + 1.2 + k as f32 * 1.6));
        }
    }

    /// In a snowball fight right now.
    pub fn in_fight(&self, id: u64) -> bool {
        self.fights.iter().any(|f| f.sides.iter().flatten().any(|m| m.0 == id))
    }

    /// Who's being carried, and whether they've been moved yet.
    pub fn held_info(&self) -> Option<(u64, bool)> {
        self.held.map(|id| (id, self.moved))
    }

    /// Who the player's snowballs hit, since last asked.
    pub fn take_your_hit_ids(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.your_hit_ids)
    }

    #[cfg(test)]
    pub fn fighting(&self) -> usize {
        self.fights.len()
    }

    /// Send someone to stand at (tx, ty) facing column `face` until `until`
    /// (kids building something in the snow).
    pub fn work_at(&mut self, id: u64, tx: f32, ty: f32, face: f32, until: f32) {
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id && !c.held()) {
            c.st = St::Gather { tx, ty, face, until, moving: true };
            c.leave_at = c.leave_at.max(until + 20.0);
        }
    }

    /// Done with whatever they were at: back to wandering.
    pub fn stop_work(&mut self, id: u64, t: f32) {
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id && matches!(c.st, St::Gather { .. })) {
            c.st = St::Walk;
            c.leave_at = c.leave_at.max(t + 25.0);
        }
    }

    /// Whether someone is standing where they were sent (not walking there).
    pub fn arrived(&self, id: u64) -> bool {
        self.list.iter().any(|c| c.id == id && matches!(c.st, St::Gather { moving: false, .. }))
    }

    /// Someone goes indoors somewhere (an igloo): out of the scene, kept
    /// as they are for when they come back out.
    pub fn take(&mut self, id: u64) -> Option<Hidden> {
        let i = self.list.iter().position(|c| c.id == id && !c.held())?;
        Some(Hidden(self.list.remove(i)))
    }

    /// …and back out again at column `x`, row `y`.
    pub fn put_back(&mut self, h: Hidden, x: f32, y: f32, t: f32) {
        let mut c = h.0;
        c.x = x - c.centre() + c.x;
        c.y = y;
        c.st = St::Walk;
        c.leave_at = t + self.rng.range(15.0, 40.0);
        c.dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        self.list.push(c);
    }

    /// The people out and about, free to chat: (id, centre column, row, is a kid).
    pub fn people(&self) -> Vec<(u64, f32, f32, bool)> {
        self.list.iter().filter(|c| c.person() && c.free()).map(|c| (c.id, c.centre(), c.y, c.kind == Kind::Kid)).collect()
    }

    /// Where everyone is (x, is a person), for the map.
    pub fn positions(&self) -> Vec<(f32, bool)> {
        self.list.iter().map(|c| (c.centre(), c.person())).collect()
    }

    pub fn set_player(&mut self, p: Option<(f32, f32, f32, bool)>) {
        self.player = p;
    }

    /// The nearest character the player could talk to / pet: within `rx`
    /// columns and a few rows of (x, y).
    pub fn nearest(&self, x: f32, y: f32, rx: f32) -> Option<u64> {
        self.list
            .iter()
            .filter(|c| !c.held() && !matches!(c.st, St::Fall { .. }) && (c.centre() - x).abs() <= rx && (c.y - y).abs() <= 3.5)
            .min_by(|a, b| (a.centre() - x).abs().partial_cmp(&(b.centre() - x).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|c| c.id)
    }

    /// Who's at (x, y), if anyone.
    pub fn id_at(&self, x: i32, y: i32, t: f32) -> Option<u64> {
        self.index_at(x, y, t).map(|i| self.list[i].id)
    }

    /// (centre column, row) of someone.
    pub fn position(&self, id: u64) -> Option<(f32, f32)> {
        self.list.iter().find(|c| c.id == id).map(|c| (c.centre(), c.y))
    }

    /// What to call someone in a menu: their name, or what they are.
    pub fn label(&self, id: u64) -> String {
        match self.list.iter().find(|c| c.id == id) {
            Some(c) if c.person() => c.name.to_string(),
            Some(c) => match c.kind {
                Kind::Deer => "a deer".into(),
                Kind::Fox => "a fox".into(),
                _ => "a rabbit".into(),
            },
            None => "someone".into(),
        }
    }

    pub fn bbox_of(&self, id: u64, t: f32) -> Option<(i32, i32, i32, i32)> {
        self.list.iter().find(|c| c.id == id).map(|c| c.bbox(t, self.k))
    }

    pub fn centre_of(&self, id: u64) -> Option<f32> {
        self.list.iter().find(|c| c.id == id).map(|c| c.centre())
    }

    #[cfg(test)]
    pub fn bush_count(&self) -> usize {
        self.bushes.len()
    }

    /// (x, base row, width) of a bush.
    pub fn bush_rect(&self, i: usize) -> (i32, i32, i32) {
        let b = &self.bushes[i];
        (b.x, b.base, b.w)
    }

    /// How many are in a conversation right now (tests).
    #[cfg(test)]
    pub fn talking(&self) -> usize {
        self.list.iter().filter(|c| matches!(c.st, St::Talk { .. })).count()
    }

    #[cfg(test)]
    pub fn x_of(&self, id: u64) -> Option<f32> {
        self.list.iter().find(|c| c.id == id).map(|c| c.x)
    }

    pub fn pos_of(&self, id: u64) -> Option<(f32, f32)> {
        self.list.iter().find(|c| c.id == id).map(|c| (c.centre(), c.y))
    }

    #[cfg(test)]
    pub fn set_walking(&mut self, id: u64, dir: f32) {
        if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
            c.st = St::Walk;
            c.dir = dir;
            c.leave_at = 1e9;
            c.social_at = 1e9; // no stopping to chat in this test
        }
    }

    #[cfg(test)]
    pub fn last_id(&self) -> u64 {
        self.list.last().map(|c| c.id).unwrap_or(0)
    }

    pub fn is_person(&self, id: u64) -> bool {
        self.list.iter().any(|c| c.id == id && c.person())
    }

    pub fn is_animal(&self, id: u64) -> bool {
        self.list.iter().any(|c| c.id == id && c.animal())
    }

    /// A present from the player (gingerbread, a candy cane…).
    pub fn gift(&mut self, id: u64, what: &str, t: f32, d: &mut Director) {
        let Some(c) = self.list.iter_mut().find(|c| c.id == id) else { return };
        let (cx, top) = (c.centre(), c.y - 4.0);
        let kid = c.kind == Kind::Kid;
        let line = if kid {
            format!("a {what}?! for ME? thank you!!")
        } else {
            format!("oh, a {what}! how kind — thank you!")
        };
        c.st = St::Greet { until: t + 3.0 };
        d.say(cx.round() as i32, top as i32, &line, t, 3.2, 6);
        d.emote(cx, top + 1.0, '♥', Rgb::hex(0xff6b8a), 4);
        d.sound(if kid { Sfx::Laugh } else { Sfx::Greet }, cx, 0.9);
    }

    /// A carrot for an animal: a friend for a good while.
    pub fn befriend(&mut self, id: u64, t: f32, d: &mut Director) {
        let Some(c) = self.list.iter_mut().find(|c| c.id == id) else { return };
        let until = t + 90.0;
        c.leave_at = c.leave_at.max(until);
        c.st = St::Follow { until, moving: false };
        d.emote(c.centre(), c.y - 3.0, '♥', Rgb::hex(0xff6b8a), 5);
        d.sound(Sfx::Munch, c.centre(), 1.0);
    }

    /// The player's snowballs that hit someone since last asked.
    pub fn take_your_hits(&mut self) -> u32 {
        std::mem::take(&mut self.your_hits)
    }

    pub fn following(&self, id: u64) -> bool {
        self.list.iter().any(|c| c.id == id && matches!(c.st, St::Follow { .. } | St::Talk { resume: true, .. }))
    }

    #[cfg(test)]
    pub fn followers(&self) -> usize {
        self.list.iter().filter(|c| matches!(c.st, St::Follow { .. } | St::Talk { resume: true, .. })).count()
    }

    /// The player says hello (people) or pets / feeds (animals).
    #[allow(clippy::too_many_arguments)]
    /// Returns the cheer it earned (a first hello, or a chat run its course).
    pub fn meet(&mut self, id: u64, px: f32, py: f32, berries: &mut u32, t: f32, d: &mut Director, lines: &mut Lines) -> u32 {
        let _ = lines;
        let Some(i) = self.list.iter().position(|c| c.id == id) else { return 0 };
        let mut cheer = 0;
        // people: a hello (with their name) the first time; after that, each
        // E is the next exchange of a little chat about what's going on
        if self.list[i].person() {
            let c = &self.list[i];
            let (cx, cy, kid, name) = (c.centre(), c.y, c.kind == Kind::Kid, c.name);
            let (mine, theirs) = if !c.met {
                cheer += 1;
                ("hello there!".to_string(), self.talk.intro(&self.tctx, cx, name, kid))
            } else {
                let (topic, step) = match c.chat {
                    Some(ch) => ch,
                    None => (self.talk.chat_topic(&self.tctx, cx, kid), 0),
                };
                match self.talk.chat_line(topic, step, self.tctx.days) {
                    Some(pair) => {
                        self.list[i].chat = Some((topic, step + 1));
                        pair
                    }
                    None => {
                        self.list[i].chat = None;
                        cheer += 1;
                        ("lovely talking with you!".to_string(), format!("you too! see you around{}", if kid { "!!" } else { "." }))
                    }
                }
            };
            let c = &mut self.list[i];
            c.met = true;
            let toward = (px - cx).signum();
            if toward != 0.0 {
                c.dir = toward;
            }
            let was_following = matches!(c.st, St::Follow { .. });
            d.say(px.round() as i32, py as i32 - 4, &mine, t, 2.2, 40);
            d.say(cx.round() as i32, cy as i32 - 4, &theirs, t + 2.4, 3.6, 40);
            c.st = St::Talk { until: t + 6.5, resume: was_following };
            c.social_at = c.social_at.max(t + 12.0);
            if let Some(s) = c.sfx() {
                d.sound(s, cx, 0.8);
            }
            return cheer;
        }
        let follow_until = self.list[i].leave_at;
        let c = &mut self.list[i];
        let (cx, top) = (c.centre(), c.y - 3.0);
        let toward = (px - cx).signum();
        if toward != 0.0 {
            c.dir = toward;
        }
        let was_following = matches!(c.st, St::Follow { .. });
        if *berries > 0 {
            // a berry from the player's hand: a friend for a while
            *berries -= 1;
            d.sound(Sfx::Munch, cx, 1.0);
            d.emote(cx, top, '♥', Rgb::hex(0xff6b8a), 4);
            let until = t + 60.0;
            c.leave_at = c.leave_at.max(until);
            c.st = St::Follow { until, moving: false };
        } else {
            c.st = if was_following { St::Follow { until: follow_until, moving: false } } else { St::Petted { until: t + 2.5 } };
            d.emote(cx, top, '♥', Rgb::hex(0xff6b8a), 3);
            d.sound(Sfx::Pet, cx, 0.7);
            if let Some(s) = c.sfx() {
                d.sound(s, cx, 0.8);
            }
        }
        cheer
    }

    /// What's going on, for what people say (set each frame by the scene).
    pub fn set_talk(&mut self, ctx: TalkCtx) {
        self.tctx = ctx;
    }

    /// Someone in view says something in passing about what's going on.
    pub fn chatter(&mut self, t: f32, d: &mut Director) {
        let (v0, vw) = self.view;
        let walkers: Vec<usize> = (0..self.list.len())
            .filter(|&i| {
                let c = &self.list[i];
                c.person() && matches!(c.st, St::Walk) && c.x > v0 + 4.0 && c.x < v0 + vw - 6.0
            })
            .collect();
        if walkers.is_empty() {
            return;
        }
        let i = walkers[self.rng.int(0, walkers.len() as i32) as usize];
        let (cx, cy, kid) = (self.list[i].centre(), self.list[i].y, self.list[i].kind == Kind::Kid);
        let line = self.talk.remark(&self.tctx, cx, kid);
        d.say(cx.round() as i32, cy as i32 - 4, &line, t, 2.8, 6);
    }

    /// Something just happened (near column `x`, if it's somewhere): the
    /// people in view stop, look and say so.
    pub fn react(&mut self, ev: Ev, x: Option<f32>, t: f32, d: &mut Director) {
        // (a quiet village keeps its thoughts to itself)
        if !self.rng.chance(super::events::chattiness().min(1.0)) {
            return;
        }
        if t < self.react_cd {
            return;
        }
        self.react_cd = t + 6.0;
        let (v0, vw) = self.view;
        let mut who: Vec<usize> = (0..self.list.len())
            .filter(|&i| {
                let c = &self.list[i];
                c.person() && matches!(c.st, St::Walk | St::Idle { .. } | St::Greet { .. } | St::Follow { .. } | St::Gather { .. }) && c.x > v0 && c.x < v0 + vw
            })
            .collect();
        // a couple of them say something; the rest just look
        for k in (1..who.len()).rev() {
            let j = self.rng.int(0, k as i32 + 1) as usize;
            who.swap(k, j);
        }
        for (n, &i) in who.iter().take(4).enumerate() {
            let kid = self.list[i].kind == Kind::Kid;
            let line = (n < 2).then(|| self.talk.react(ev, kid));
            let c = &mut self.list[i];
            let (cx, cy) = (c.centre(), c.y);
            if let Some(x) = x {
                let to = (x - cx).signum();
                if to != 0.0 {
                    c.dir = to;
                }
            }
            let resume_follow = matches!(c.st, St::Follow { .. });
            if !resume_follow && !matches!(c.st, St::Gather { .. }) {
                c.st = St::Idle { until: t + 2.5 + n as f32 * 0.4 };
            }
            d.emote(cx, cy - 3.0, '!', Rgb::hex(0xffd36b), 1);
            if let Some(line) = line {
                d.say(cx.round() as i32, cy as i32 - 4, &line, t + 0.2 + n as f32 * 1.1, 2.8, 6);
            }
            if kid && n < 2 {
                d.sound(Sfx::Laugh, cx, 0.5);
            }
        }
    }

    /// A passer-by stops to chime in on a conversation under way.
    fn chime_ins(&mut self, dt: f32, t: f32, d: &mut Director) {
        self.convs.retain(|c| t < c.3);
        for k in 0..self.convs.len() {
            if self.convs[k].4 || !self.rng.chance(dt * 0.6) {
                continue;
            }
            let (ids, mx, row) = (self.convs[k].0.clone(), self.convs[k].1, self.convs[k].2);
            let Some(j) = (0..self.list.len()).find(|&j| {
                let c = &self.list[j];
                c.person() && matches!(c.st, St::Walk) && t >= c.social_at && !ids.contains(&c.id) && (c.centre() - mx).abs() < 10.0 && (c.y - row).abs() < 4.0
            }) else {
                continue;
            };
            self.convs[k].4 = true;
            let name = |id: u64| self.list.iter().find(|c| c.id == id).map(|c| c.name).unwrap_or("friend");
            let (a, b) = (name(ids[0]), name(*ids.get(1).unwrap_or(&ids[0])));
            let (cname, cx, cy) = (self.list[j].name, self.list[j].centre(), self.list[j].y);
            let (line, reply) = self.talk.chime(&self.tctx, mx, a, b, cname);
            d.say(cx.round() as i32, cy as i32 - 4, &line, t, 2.6, 6);
            // whoever's nearest in the group answers
            if let Some(r) = self.list.iter().filter(|c| ids.contains(&c.id)).min_by(|p, q| (p.centre() - cx).abs().partial_cmp(&(q.centre() - cx).abs()).unwrap_or(std::cmp::Ordering::Equal)) {
                d.say(r.centre().round() as i32, r.y as i32 - 4, &reply, t + 2.8, 3.0, 6);
            }
            let c = &mut self.list[j];
            c.dir = (mx - cx).signum();
            c.st = St::Greet { until: t + 5.5 };
            c.social_at = t + 30.0;
        }
    }

    /// "Come with me" — or, if they're already along, "see you!".
    /// Animals only tag along for berries. Returns whether they joined.
    pub fn invite(&mut self, id: u64, berries: &mut u32, t: f32, d: &mut Director, lines: &mut Lines) -> bool {
        let Some(c) = self.list.iter_mut().find(|c| c.id == id) else { return false };
        let (cx, top) = (c.centre(), c.y - 3.0);
        if matches!(c.st, St::Follow { .. } | St::Talk { resume: true, .. }) {
            c.st = St::Walk;
            c.leave_at = if c.animal() { t + 60.0 } else { 0.0 };
            c.social_at = t + 10.0;
            if c.person() {
                let msg = lines.follow_bye();
                d.say(cx.round() as i32, top as i32 - 1, &msg, t, 3.0, 6);
            }
            return false;
        }
        if c.person() {
            let msg = lines.follow_yes(c.kind == Kind::Kid);
            d.say(cx.round() as i32, top as i32 - 1, &msg, t, 3.0, 6);
            let until = t + 900.0;
            (c.leave_at, c.group) = (until, 0);
            c.st = St::Follow { until, moving: false };
            d.sound(Sfx::Greet, cx, 0.8);
            true
        } else if *berries > 0 {
            *berries -= 1;
            d.sound(Sfx::Munch, cx, 1.0);
            d.emote(cx, top, '♥', Rgb::hex(0xff6b8a), 3);
            let until = t + 60.0;
            c.leave_at = c.leave_at.max(until);
            c.st = St::Follow { until, moving: false };
            true
        } else {
            // empty hands: a curious sniff, then off they go
            d.emote(cx, top, '?', Rgb::hex(0xffd36b), 1);
            if let Some(s) = c.sfx() {
                d.sound(s, cx, 0.6);
            }
            false
        }
    }

    /// Everyone following the player heads off on their own again.
    pub fn dismiss(&mut self, t: f32, d: &mut Director, lines: &mut Lines) -> bool {
        let mut any = false;
        let mut said = false;
        for c in &mut self.list {
            if matches!(c.st, St::Follow { .. } | St::Talk { resume: true, .. }) {
                any = true;
                c.st = St::Walk;
                c.leave_at = if c.animal() { t + 60.0 } else { 0.0 };
                c.social_at = t + 10.0;
                if c.person() && !said {
                    said = true;
                    let msg = lines.follow_bye();
                    d.say(c.centre().round() as i32, c.y as i32 - 4, &msg, t, 3.0, 6);
                }
            }
        }
        any
    }

    /// The player throws a snowball from (x, y) the way they face: at the
    /// nearest person or animal over there, or into the snow.
    pub fn throw(&mut self, x: f32, y: f32, dir: f32, d: &mut Director) {
        let target = self
            .list
            .iter()
            .filter(|c| !c.held() && !matches!(c.st, St::Fall { .. }) && (c.centre() - x) * dir > 1.5 && (c.centre() - x).abs() < 40.0 && (c.y - y).abs() < 7.0)
            .min_by(|a, b| (a.centre() - x).abs().partial_cmp(&(b.centre() - x).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|c| (c.id, c.centre(), c.y));
        let (id, tx, ty) = target.unwrap_or((0, x + dir * 16.0, y));
        let (sx, sy) = (x + dir, y - 2.0);
        let dist = tx - sx;
        let flight = (dist.abs() / 30.0).clamp(0.35, 0.9);
        self.balls.push(Snowball { x: sx, y: sy, vx: dist / flight, vy: (ty - 1.0 - sy) / flight - 15.0 * flight, target: id, age: 0.0, yours: true, land: ty + 0.5 });
        d.sound(Sfx::Snowball, sx, 0.6);
    }

    /// Snowballs that hit the player since last asked.
    pub fn take_hits_on_you(&mut self) -> u32 {
        std::mem::take(&mut self.hits_on_you)
    }

    /// A bush within reach of (x, y).
    pub fn bush_near(&self, x: f32, y: f32) -> Option<usize> {
        self.bushes.iter().position(|b| (b.centre() - x).abs() <= b.w as f32 / 2.0 + 2.5 && (b.base as f32 - y).abs() <= 3.0)
    }

    /// Pick a few ripe berries (they grow back). Returns how many.
    pub fn pick_berries(&mut self, i: usize, t: f32, d: &mut Director) -> u32 {
        let Some(b) = self.bushes.get_mut(i) else { return 0 };
        b.shake_until = t + 0.4;
        let mut n = 0;
        for br in b.berries.iter_mut().filter(|br| t >= br.2).take(3) {
            br.2 = t + self.rng.range(60.0, 150.0);
            n += 1;
        }
        d.sound(Sfx::Rustle, b.centre(), 0.9);
        n
    }

    /// Rushing at animals startles them; resting the cursor nearby makes
    /// them curious.
    fn cursor_reactions(&mut self, dt: f32, t: f32, d: &mut Director, cursor: Option<(f32, f32, f32, f32)>) {
        let Some((cx, cy, vx, vy)) = cursor else { return };
        let speed = (vx * vx + vy * vy * 4.0).sqrt();
        for c in &mut self.list {
            if !c.animal() || !c.free() {
                continue;
            }
            let (dx, dy) = (cx - c.centre(), (cy - c.y) * 2.0);
            let dist = (dx * dx + dy * dy).sqrt();
            if speed > 35.0 && dist < 9.0 {
                c.st = St::Flee { until: t + 1.6 };
                c.dir = -dx.signum();
                if t > self.startle_cd {
                    self.startle_cd = t + 1.0;
                    d.emote(c.centre(), c.y - 3.0, '!', Rgb::hex(0xffd36b), 1);
                    if let Some(s) = c.sfx() {
                        d.sound(s, c.centre(), 0.5);
                    }
                }
            } else if speed < 3.0 && dist < 16.0 && dist > 4.0 && matches!(c.st, St::Walk | St::Idle { .. }) && self.rng.chance(dt * 0.35) {
                c.st = St::Curious { until: t + 6.0 };
            }
        }
    }

    /// Animals passing a bush with ripe berries stop for a snack.
    fn snack(&mut self, dt: f32, t: f32) {
        for c in &mut self.list {
            if !c.animal() || !matches!(c.st, St::Walk | St::Idle { .. }) {
                continue;
            }
            for (bi, b) in self.bushes.iter().enumerate() {
                if (c.centre() - b.centre()).abs() < b.w as f32 / 2.0 + 2.0 && (c.y - b.base as f32).abs() <= 4.0 && b.ripe(t) > 0 && self.rng.chance(dt * 1.2) {
                    c.st = St::Munch { bush: bi, until: t + self.rng.range(3.0, 7.0), bite: t + 0.4 };
                    c.dir = (b.centre() - c.centre()).signum();
                    break;
                }
            }
        }
    }

    /// Clicked a bush: it rustles; sometimes a cardinal flies out, or a
    /// rabbit pops out and hops off.
    pub fn shake_bush(&mut self, i: usize, t: f32, d: &mut Director) {
        let Some(b) = self.bushes.get_mut(i) else { return };
        b.shake_until = t + 0.5;
        let (bx, by) = (b.centre(), b.base as f32 - 2.0);
        d.sound(Sfx::Rustle, bx, 1.0);
        d.snow_puff(bx, by);
        let roll = self.rng.f();
        if roll < 0.4 {
            let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            self.birds.push(Bird { x: bx, y: by, vx: dir * self.rng.range(7.0, 11.0), vy: -self.rng.range(3.0, 5.0), age: 0.0 });
            d.sound(Sfx::Bird, bx, 1.0);
        } else if roll < 0.55 {
            let id = self.spawn(Kind::Rabbit, t, Some((bx - 1.0, (by + 2.0).min((self.h - 1) as f32))));
            if let Some(c) = self.list.iter_mut().find(|c| c.id == id) {
                c.st = St::Flee { until: t + 1.5 };
            }
            d.sound(Sfx::Rabbit, bx, 1.0);
        }
    }

    // ------------------------------------------------------------ drawing

    /// Footprints in the snow (behind everything standing).
    pub fn draw_back(&self, f: &mut Frame, t: f32) {
        for &(x, y, born) in &self.prints {
            let k = (1.0 - (t - born) / 30.0).max(0.0);
            if !f.has_g(x, y) {
                f.put_g(x, y, Glyph::new('·', Rgb::hex(0x56607f)).alpha(0.8 * k).lit(0.5));
            }
        }
    }

    /// Bushes and characters, back to front (the held one is drawn later,
    /// on top of everything, by `draw_top`).
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        enum Item<'a> {
            B(&'a Bush),
            C(&'a Critter),
        }
        let mut items: Vec<(f32, Item)> = self.bushes.iter().map(|b| (b.base as f32, Item::B(b))).collect();
        items.extend(self.list.iter().filter(|c| !c.held() && !matches!(c.st, St::Fall { .. })).map(|c| (c.y, Item::C(c))));
        items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, it) in items {
            match it {
                Item::B(b) => draw_bush(f, b, t, glow),
                Item::C(c) => draw_critter(f, c, t, glow, self.k),
            }
        }
        // pets (pixel art, under the glyphs): the lead first, then the pet
        let pk = pet_scale(self.k);
        for c in &self.list {
            let Some(p) = &c.pet else { continue };
            let feet = (p.y * 2.0).round() as i32 + 1;
            if p.kind == PetKind::Dog && !c.held() {
                // from the hand to the collar, sagging a little
                let (hx, hy) = (c.centre() + c.dir * 0.2 * self.k, (c.y + 1.0) * 2.0 - 4.0 * self.k);
                let (kx, ky) = (p.x + 0.5 + p.dir * 1.5 * pk as f32, feet as f32 - 3.0 * pk as f32);
                let n = ((hx - kx).abs().max((hy - ky).abs()) as i32).max(1);
                for k in 0..=n {
                    let u = k as f32 / n as f32;
                    let sag = (u * std::f32::consts::PI).sin() * 1.0;
                    f.px_set((hx + (kx - hx) * u).round() as i32, (hy + (ky - hy) * u + sag).round() as i32, Rgb::hex(0x3a2a4a));
                }
            }
            let pose = if t < p.happy_until { Pose::Up } else if p.walking { Pose::Walk } else { Pose::Sit };
            pets::draw(f, p.kind, p.body, p.marks, p.x + 0.5, feet, p.dir, pose, t + c.id as f32, pk, Some(Rgb::hex(0xc8202a)));
        }
    }

    /// Whoever is being carried or falling, snowballs and birds.
    pub fn draw_top(&self, f: &mut Frame, t: f32, glow: f32) {
        for c in self.list.iter().filter(|c| c.held() || matches!(c.st, St::Fall { .. })) {
            draw_critter(f, c, t, glow, self.k);
        }
        for b in &self.balls {
            f.put_g(b.x.round() as i32, b.y.round() as i32, Glyph::new('●', Rgb::hex(0xf2f6ff)).lit(0.3));
        }
        for b in &self.birds {
            let flap = (b.age * 12.0) as i32 % 2 == 0;
            let ch = if (b.vx > 0.0) == flap { '◥' } else { '◤' };
            f.put_g(b.x.round() as i32, b.y.round() as i32, Glyph::new(ch, Rgb::hex(0xd8202a)).lit(0.4));
        }
    }
}

fn draw_bush(f: &mut Frame, b: &Bush, t: f32, glow: f32) {
    let wiggle = if t < b.shake_until { ((t * 30.0).sin() * 1.2).round() as i32 } else { 0 };
    let x0 = b.x + wiggle;
    let rows = [b.base - 2, b.base - 1, b.base];
    for (ry, &y) in rows.iter().enumerate() {
        for dx in 0..b.w {
            let x = x0 + dx;
            let edge_l = dx == 0;
            let edge_r = dx == b.w - 1;
            let shade = 0.85 + 0.3 * h01(hash2(b.x + dx, y));
            let leaf = LEAF * shade;
            let g = match ry {
                // snow-capped top with rounded shoulders
                0 if edge_l => Glyph::new('◢', leaf),
                0 if edge_r => Glyph::new('◣', leaf),
                0 => Glyph::new('▀', SNOW_CAP * (0.85 + 0.15 * shade)).solid(leaf),
                _ if edge_l => Glyph::new('▐', leaf),
                _ if edge_r => Glyph::new('▌', leaf),
                _ => {
                    let tex = if hash2(b.x * 7 + dx, y) % 5 == 0 { ',' } else { ' ' };
                    Glyph::new(tex, LEAF_DARK * 1.4).solid(leaf)
                }
            };
            f.put_g(x, y, g.lit(0.7));
        }
    }
    // berries: ripe ones bright red (with a faint festive glow), regrowing
    // ones creep back in dark
    for &(dx, dy, ripe_at) in &b.berries {
        let (x, y) = (x0 + dx, b.base - 1 + dy.min(1));
        let leaf = LEAF * (0.85 + 0.3 * h01(hash2(b.x + dx, y)));
        if t >= ripe_at {
            f.put_g(x, y, Glyph::new('●', RED_BERRY).solid(leaf).lit(0.5));
            f.light_cell(x, y, RED_BERRY * (0.18 * glow));
        } else if ripe_at - t < 20.0 {
            f.put_g(x, y, Glyph::new('•', Rgb::hex(0x7a2030)).solid(leaf).lit(0.5));
        }
    }
}

/// Pets' pixel scale for a village drawn `k` times bigger.
pub fn pet_scale(k: f32) -> i32 {
    (k.round() as i32).max(1)
}

fn draw_critter(f: &mut Frame, c: &Critter, t: f32, glow: f32, k: f32) {
    // people, on a screen tall enough: the same pixel-art folk as indoors
    if k >= 1.75 && c.person() && !c.held() && !matches!(c.st, St::Fall { .. }) {
        let moving = matches!(c.st, St::Walk | St::Flee { .. } | St::Chase { .. } | St::Curious { .. } | St::Follow { moving: true, .. } | St::Gather { moving: true, .. });
        let rate = if c.kind == Kind::Kid { 9.0 } else { 6.0 };
        let kid = c.kind == Kind::Kid;
        let wave = matches!(c.st, St::Greet { .. } | St::Throw { .. } | St::Petting { .. });
        let bob = if matches!(c.st, St::Petted { .. }) && (t * 4.0) as i32 % 2 == 0 { 1 } else { 0 };
        let b = super::interior::Big {
            x: c.centre(),
            feet: c.y.round() as i32 - bob,
            dir: c.dir,
            step: moving && (c.age * rate) as i32 % 2 == 0,
            hat: Some(if kid { c.hat } else { pal::CANDY_RED }),
            hair: c.hat,
            coat: c.color,
            scarf: None,
            sit: None,
            kid,
            arms_up: wave && (t * 3.0) as i32 % 2 == 0,
            outdoor: true,
        };
        b.draw(f, t);
        if c.kind == Kind::Walker {
            // the lantern in hand
            let (lx, ly) = (c.centre().round() as i32 + if c.dir > 0.0 { 3 } else { -4 }, c.y.round() as i32 * 2 - 5);
            let fl = 0.85 + 0.15 * (t * 9.0 + c.x).sin();
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                f.px_sprite(lx + dx, ly + dy, Rgb::hex(0xffd27a) * (1.2 * fl));
            }
            f.light_px(lx as f32 + 1.0, ly as f32 + 1.0, Rgb::hex(0xffc060) * (4.5 * fl * glow));
        }
        return;
    }
    let rows = c.sprite(t);
    let hop = if c.kind == Kind::Rabbit && matches!(c.st, St::Walk | St::Flee { .. }) && (c.age * 3.0).fract() < 0.5 { -1 } else { 0 };
    let bob = if matches!(c.st, St::Petted { .. }) && (t * 4.0) as i32 % 2 == 0 { -1 } else { 0 };
    let top = c.y.round() as i32 - rows.len() as i32 + 1 + hop + bob;
    let n = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
    let x0 = c.x.round() as i32;
    // drawn bigger on a taller screen: each cell scaled about the feet
    let (axi, ayi) = (x0, c.y.round() as i32 + 1);
    let (ax, ay) = (c.x + n as f32 * (1.0 - k) / 2.0, c.y + 1.0);
    let place = move |cx: i32, cy: i32| (ax + (cx - axi) as f32 * k, ay + (cy - ayi) as f32 * k);
    for (ry, row) in rows.iter().enumerate() {
        for (i, ch) in row.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let (cx, ch) = if c.dir > 0.0 { (x0 + i as i32, ch) } else { (x0 + n - 1 - i as i32, super::events::mirror(ch)) };
            let col = match (c.kind, ch) {
                (Kind::Deer, '▗') | (Kind::Deer, '▖') => Rgb::hex(0xeae4d8),
                (Kind::Deer, 'ψ') => Rgb::hex(0x5a3a22),
                (Kind::Fox, '▂') => Rgb::hex(0xf2e6d0),
                (Kind::Walker, '▲') => pal::CANDY_RED,
                (Kind::Kid, '▴') => c.hat,
                (Kind::Walker, '●') | (Kind::Kid, '●') => Rgb::hex(0xe8c39e),
                (Kind::Walker, '◘') => Rgb::hex(0xffd27a) * 1.3,
                (Kind::Walker | Kind::Kid, '╨' | '║') => Rgb::hex(0x2a2a34),
                (Kind::Walker | Kind::Kid, '╱' | '╲') => c.color * 0.9,
                _ => c.color,
            };
            let cy = top + ry as i32;
            if k <= 1.0 {
                f.put_g(cx, cy, Glyph::new(ch, col).bg(Bg::Scene).lit(0.6));
            } else {
                let (sx, sy) = place(cx, cy);
                f.sprite_cell(sx, sy, k, Glyph::new(ch, col));
            }
            if c.kind == Kind::Walker && ch == '◘' {
                let fl = 0.85 + 0.15 * (t * 9.0 + c.x).sin();
                if k <= 1.0 {
                    f.light_cell(cx, cy, Rgb::hex(0xffc060) * (4.5 * fl * glow));
                } else {
                    let (sx, sy) = place(cx, cy);
                    f.light_px(sx + 0.5 * k, (sy + 0.5 * k) * 2.0, Rgb::hex(0xffc060) * (4.5 * fl * glow));
                }
            }
        }
    }
    // dizzy stars circling over their head
    if matches!(c.st, St::Dizzy { .. }) {
        let cx = c.centre();
        let top = place(x0, top).1;
        for k in 0..3 {
            let a = t * 5.0 + k as f32 * 2.09;
            let (x, y) = (cx + a.cos() * 1.6, top - 1.0 + a.sin() * 0.4);
            let tw = 0.6 + 0.4 * noise1(t * 3.0 + k as f32, 41);
            f.put_g(x.round() as i32, y.round() as i32, Glyph::new(if k % 2 == 0 { '✦' } else { '✧' }, Rgb::hex(0xffe08a) * tw).lit(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (Ctx, Director, Lines) {
        let (w, h) = (200, 40);
        let ctx = Ctx { w, h, ground_row: vec![24; w as usize], track_row: 20, occluded: vec![false; (w * h) as usize], church: None, sky_rows: 10, station: None, cam: 0.0, vw: w };
        (ctx, Director::new(3, 0.0), Lines::new(3))
    }

    fn run(c: &mut Critters, ctx: &Ctx, d: &mut Director, l: &mut Lines, from: f32, secs: f32) -> f32 {
        let mut t = from;
        while t < from + secs {
            c.update(1.0 / 30.0, t, ctx, d, l, None, None);
            t += 1.0 / 30.0;
        }
        t
    }

    #[test]
    fn dropping_someone_from_the_sky_lands_them_in_the_snow_dizzy() {
        let (ctx, mut d, mut l) = world();
        let mut c = Critters::new(&ctx, 1, 0.0, false, &[]);
        c.spawn_demo(Kind::Walker, 100.0, 0.0);
        let (x, y) = (c.list[0].x as i32 + 1, c.list[0].y as i32);
        assert!(c.grab(x, y, 0.0, &mut d));
        c.drag(x + 10, 4); // way up in the sky
        assert!(c.list[0].y < 10.0);
        c.release(0.1, &mut d, &mut l);
        assert!(matches!(c.list[0].st, St::Fall { .. }));
        run(&mut c, &ctx, &mut d, &mut l, 0.1, 1.5);
        let p = &c.list[0];
        assert!(p.y >= 28.0 && p.y <= 39.0, "landed at {}", p.y);
        assert!(matches!(p.st, St::Dizzy { .. }), "{:?}", p.st);
        assert!(!c.holding());
    }

    #[test]
    fn a_click_without_dragging_pets_an_animal() {
        let (ctx, mut d, mut l) = world();
        let mut c = Critters::new(&ctx, 1, 0.0, false, &[]);
        c.spawn_demo(Kind::Deer, 100.0, 0.0);
        let (x, y) = (c.list[0].x as i32 + 2, c.list[0].y as i32 - 1);
        assert!(c.grab(x, y, 0.0, &mut d));
        c.release(0.05, &mut d, &mut l);
        assert!(matches!(c.list[0].st, St::Petted { .. }));
    }

    #[test]
    fn animals_eat_berries_from_a_bush_and_they_regrow() {
        let (ctx, mut d, mut l) = world();
        let mut c = Critters::new(&ctx, 1, 0.0, false, &[]);
        c.bushes.push(Bush { x: 100, base: 32, w: 7, berries: vec![(2, 0, 0.0), (4, 1, 0.0), (5, 0, 0.0)], shake_until: 0.0 });
        c.spawn(Kind::Deer, 0.0, Some((98.0, 32.0)));
        c.list[0].speed = 0.0; // stay put next to the bush
        let t = run(&mut c, &ctx, &mut d, &mut l, 0.0, 12.0);
        let eaten = c.bushes[0].berries.iter().filter(|b| b.2 > t).count();
        assert!(eaten > 0, "nothing eaten");
        // and they come back within a few minutes
        let back = c.bushes[0].berries.iter().all(|b| b.2 < t + 160.0);
        assert!(back);
    }

    #[test]
    fn someone_being_carried_is_never_culled_offscreen() {
        let (ctx, mut d, mut l) = world();
        let mut c = Critters::new(&ctx, 1, 0.0, false, &[]);
        c.spawn_demo(Kind::Fox, 100.0, 0.0);
        let (x, y) = (c.list[0].x as i32 + 2, c.list[0].y as i32);
        assert!(c.grab(x, y, 0.0, &mut d));
        c.drag(-30, 20);
        run(&mut c, &ctx, &mut d, &mut l, 0.0, 2.0);
        assert_eq!(c.list.len(), 1);
        assert!(c.holding());
    }
}
