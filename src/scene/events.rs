//! The "director": keeps the scene alive with random ambient happenings
//! (wildlife, walkers with lanterns, a distant train, gusts, meteor showers,
//! aurora flares, light shows, a rare comet, church bells on the hour,
//! fireworks at New Year), plus the particles and speech bubbles used by the
//! clickable easter eggs.

use crate::audio::{Sfx, SoundEvent};
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::Rng;
use chrono::{Datelike, Timelike};
use std::sync::atomic::{AtomicI64, Ordering};

/// Last hour the church bells tolled for. Process-wide rather than per
/// Director so that rebuilding the scene (any settings change, resize,
/// shuffle) during minute :00 doesn't toll the same hour a second time.
static LAST_BELL_KEY: AtomicI64 = AtomicI64::new(-1);

/// Width in cells of the train sprite (locomotive + `TRAIN_CARS` cars).
const TRAIN_CARS: i32 = 5;
const TRAIN_LEN: i32 = 5 + TRAIN_CARS * 6;

/// Smooth 0→1→0 envelope over [start, start+dur].
#[derive(Clone, Copy, Default)]
pub struct Env {
    start: f32,
    dur: f32,
}

/// Fraction of the envelope spent ramping in (and again ramping out).
const ENV_EDGE: f32 = 0.18;

impl Env {
    /// Start the envelope at `t`. If it's already running, it continues from
    /// its current level instead of snapping back to 0 (an aurora flare or
    /// gust re-fired by a random event used to vanish for a frame and ramp in
    /// again); and a re-fire that would end sooner than the running one is
    /// ignored, so a short trigger never cuts a long one short.
    pub fn fire(&mut self, t: f32, dur: f32) {
        let v = self.value(t);
        if v > 0.0 && dur > 0.0 {
            // place the new ramp-in so that it passes through `v` at `t`
            let start = t - ENV_EDGE * dur * v.sqrt();
            if start + dur > self.start + self.dur {
                self.start = start;
                self.dur = dur;
            }
            return;
        }
        self.start = t;
        self.dur = dur;
    }
    pub fn value(&self, t: f32) -> f32 {
        if self.dur <= 0.0 || t < self.start || t > self.start + self.dur {
            return 0.0;
        }
        let u = (t - self.start) / self.dur;
        let edge = ENV_EDGE;
        if u < edge {
            (u / edge).powi(2)
        } else if u > 1.0 - edge {
            ((1.0 - u) / edge).powi(2)
        } else {
            1.0
        }
    }
    pub fn active(&self, t: f32) -> bool {
        self.value(t) > 0.0
    }
    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        self.start -= s;
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Deer,
    Fox,
    Rabbit,
    Walker,
    Kid,
    Train,
}

pub struct Actor {
    kind: Kind,
    x: f32,
    y: i32,
    dir: f32,
    speed: f32,
    age: f32,
    pause: f32,
    color: Rgb,
    /// A train that has already stopped at the station.
    served: bool,
}

struct Part {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    grav: f32,
    age: f32,
    life: f32,
    ch: char,
    color: Rgb,
    glow: f32,
}

struct Streak {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    len: f32,
    bright: f32,
}

struct Bubble {
    x: i32,
    y: i32,
    text: String,
    born: f32,
    life: f32,
    /// Who's saying it (0: nobody/not matched yet), and where the bubble
    /// sits relative to them, so it follows them around (carried off
    /// mid-sentence, say).
    owner: u64,
    dy: i32,
    matched: bool,
}

/// A car (or now and then a little bus) on the distant road.
struct Car {
    x: f32,
    dir: f32,
    speed: f32,
    color: Rgb,
    bus: bool,
    /// Clicked: honking with its high beams on until then.
    honk: f32,
}

impl Car {
    fn len(&self) -> i32 {
        if self.bus { 7 } else { 4 }
    }
}

struct Rocket {
    x: f32,
    y: f32,
    vy: f32,
    burst_y: f32,
    color: Rgb,
}

/// Static facts about the layout the director needs.
pub struct Ctx {
    pub w: i32,
    pub h: i32,
    pub ground_row: Vec<i32>,
    pub track_row: i32,
    pub occluded: Vec<bool>,
    pub church: Option<(i32, i32)>,
    pub sky_rows: i32,
    /// The railway station: (left column, width).
    pub station: Option<(i32, i32)>,
    /// Camera (world column at the screen's left edge) and view width.
    pub cam: f32,
    pub vw: i32,
}

/// How long speech bubbles stay up and how chatty people are (settings;
/// f32 bits, 1.0 by default).
static TALK_TIME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);
static CHATTY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

pub fn set_talk(time: f32, chatty: f32) {
    TALK_TIME.store(time.clamp(0.25, 4.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
    CHATTY.store(chatty.clamp(0.0, 3.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn talk_time() -> f32 {
    f32::from_bits(TALK_TIME.load(std::sync::atomic::Ordering::Relaxed))
}

/// How often people talk and remark on things (0 = they keep to themselves).
pub fn chattiness() -> f32 {
    f32::from_bits(CHATTY.load(std::sync::atomic::Ordering::Relaxed))
}

/// How long a line of `text` stays up: at least `min` seconds, longer
/// lines longer (time to read them), scaled by the setting.
pub fn read_time(text: &str, min: f32) -> f32 {
    let n = text.chars().count() as f32;
    min.max(1.4 + n * 0.065) * talk_time()
}

pub struct Director {
    rng: Rng,
    next: f32,
    mult: f32,
    actors: Vec<Actor>,
    parts: Vec<Part>,
    streaks: Vec<Streak>,
    bubbles: Vec<Bubble>,
    rockets: Vec<Rocket>,
    cars: Vec<Car>,
    cars_seeded: bool,
    /// The director of a room indoors (just bubbles, sparkles, notes).
    pub indoor: bool,
    /// Christmas Eve night: no trains, no traffic.
    pub quiet: bool,
    /// A train was clicked: windows blazing, extra steam, until then.
    train_glow: f32,
    /// A light show announced for later: (start, duration).
    show_at: Option<(f32, f32)>,
    /// The village should gather round the big tree until this time.
    pub gather: Option<f32>,
    /// Passengers off a train at the station (columns), for the scene to
    /// send walking into the village.
    pub arrivals: Vec<f32>,
    /// Things people might notice: (what, where, if anywhere in particular).
    pub happenings: Vec<(super::dialogue::Ev, Option<f32>)>,
    car_cd: f32,
    /// Traffic keeps left (UK, Japan, Australia…) instead of right.
    pub drive_left: bool,
    next_train: f32,
    prints: Vec<(i32, i32, f32)>,
    pub gust: Env,
    gust_dir: f32,
    pub flare: Env,
    pub show: Env,
    pub blizzard: Env,
    bell_key: i64,
    bells_until: f32,
    next_firework: f32,
    pub house_toggle: Option<usize>,
    /// Sounds queued this frame (x in cells; see `Scene::take_sounds`).
    pub sounds: Vec<SoundEvent>,
    bell_strikes: u32,
    next_strike: f32,
    crunch_cd: f32,
    sparkle_cd: f32,
    /// Animals and people the scene should send by (handled by `critters`).
    pub spawn_requests: Vec<Kind>,
}

/// Sound position for "everywhere / no particular place".
pub const CENTER: f32 = f32::NAN;

impl Director {
    pub fn new(seed: u32, mult: f32) -> Self {
        Director {
            rng: Rng::new(seed as u64 ^ 0xd1ec),
            next: 6.0,
            mult,
            actors: vec![],
            parts: vec![],
            streaks: vec![],
            bubbles: vec![],
            rockets: vec![],
            cars: vec![],
            cars_seeded: false,
            indoor: false,
            quiet: false,
            train_glow: -1.0,
            show_at: None,
            gather: None,
            arrivals: vec![],
            happenings: vec![],
            car_cd: 0.0,
            drive_left: false,
            next_train: 20.0,
            prints: vec![],
            gust: Env::default(),
            gust_dir: 1.0,
            flare: Env::default(),
            show: Env::default(),
            blizzard: Env::default(),
            bell_key: LAST_BELL_KEY.load(Ordering::Relaxed),
            bells_until: 0.0,
            next_firework: 0.0,
            house_toggle: None,
            sounds: vec![],
            bell_strikes: 0,
            next_strike: 0.0,
            crunch_cd: 0.0,
            sparkle_cd: 0.0,
            spawn_requests: vec![],
        }
    }

    /// Shift every stored timestamp by `-s` seconds (scene clock rebase).
    pub fn rebase(&mut self, s: f32) {
        self.next -= s;
        self.train_glow -= s;
        for c in &mut self.cars {
            c.honk -= s;
        }
        for e in [&mut self.gust, &mut self.flare, &mut self.show, &mut self.blizzard] {
            e.rebase(s);
        }
        self.bells_until -= s;
        self.next_train -= s;
        if let Some(sa) = &mut self.show_at {
            sa.0 -= s;
        }
        self.next_firework -= s;
        self.next_strike -= s;
        for p in &mut self.prints {
            p.2 -= s;
        }
        for b in &mut self.bubbles {
            b.born -= s;
        }
    }

    /// Queue a sound at cell column `x` (bounded, in case nobody drains it).
    pub fn sound(&mut self, sfx: Sfx, x: f32, gain: f32) {
        if self.sounds.len() < 64 {
            self.sounds.push(SoundEvent { sfx, pan: x, gain });
        }
    }

    /// Village-wide rainbow light show, with its sparkle.
    pub fn light_show(&mut self, t: f32, dur: f32) {
        self.happenings.push((super::dialogue::Ev::LightShow, None));
        self.show.fire(t, dur);
        self.sound(Sfx::LightShow, CENTER, 1.0);
    }

    /// Centre column of the distant train, while one is running.
    /// Seconds until the next scheduled train.
    pub fn next_train_in(&self, t: f32) -> f32 {
        self.next_train - t
    }

    /// Centres of all the trains running.
    pub fn trains(&self) -> Vec<f32> {
        self.actors.iter().filter(|a| a.kind == Kind::Train).map(|a| a.x + (TRAIN_LEN / 2) as f32).collect()
    }

    /// The train nearest world column `near`, if any is running.
    pub fn train_x(&self, near: f32) -> Option<f32> {
        self.actors
            .iter()
            .filter(|a| a.kind == Kind::Train)
            .map(|a| a.x + (TRAIN_LEN / 2) as f32)
            .min_by(|a, b| (a - near).abs().partial_cmp(&(b - near).abs()).unwrap_or(std::cmp::Ordering::Equal))
    }

    pub fn gust_wind(&self, t: f32) -> f32 {
        self.gust.value(t) * 11.0 * self.gust_dir + self.blizzard.value(t) * 16.0
    }

    // ------------------------------------------------------------------
    // spawning helpers (also used by clicks)

    pub fn bubble(&mut self, x: i32, y: i32, text: &str, t: f32) {
        self.bubbles.retain(|b| (b.x - x).abs() > 4 || (b.y - y).abs() > 2);
        let life = read_time(text, 3.8);
        self.bubbles.push(Bubble { x, y, text: text.to_string(), born: t, life, owner: 0, dy: 0, matched: false });
    }

    /// Keep speech bubbles over whoever's speaking. `who`: (id, centre
    /// column, baseline row) of everyone who might be talking. A new bubble
    /// is matched to the speaker it was placed over; after that it follows
    /// them.
    pub fn track(&mut self, who: &[(u64, f32, f32)]) {
        for b in &mut self.bubbles {
            if !b.matched {
                b.matched = true;
                let best = who
                    .iter()
                    .filter(|(_, x, y)| (x.round() as i32 - b.x).abs() <= 1 && b.y >= *y as i32 - 6 && b.y <= *y as i32)
                    .min_by_key(|(_, x, _)| (x.round() as i32 - b.x).abs());
                if let Some(&(id, _, y)) = best {
                    b.owner = id;
                    b.dy = b.y - y.round() as i32;
                }
            }
            if b.owner != 0 {
                if let Some(&(_, x, y)) = who.iter().find(|w| w.0 == b.owner) {
                    b.x = x.round() as i32;
                    b.y = y.round() as i32 + b.dy;
                }
            }
        }
    }

    /// One line of a conversation, said at `t`: any speech bubble nearby
    /// (within `r` columns) that's showing then ends as this one appears,
    /// so the exchange reads one line at a time.
    pub fn say(&mut self, x: i32, y: i32, text: &str, t: f32, life: f32, r: i32) {
        for b in &mut self.bubbles {
            if (b.x - x).abs() <= r && (b.y - y).abs() <= 3 && b.born < t {
                b.life = b.life.min(t - b.born);
            }
        }
        let life = read_time(text, life);
        self.bubbles.push(Bubble { x, y, text: text.to_string(), born: t, life, owner: 0, dy: 0, matched: false });
    }

    pub fn footprint(&mut self, x: i32, y: i32, t: f32) {
        self.prints.push((x, y, t));
    }

    pub fn burst(&mut self, x: f32, y: f32, n: usize, colors: &[Rgb], speed: f32, grav: f32) {
        for _ in 0..n {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let s = self.rng.range(0.3, 1.0) * speed;
            let color = *self.rng.pick(colors);
            let ch = *self.rng.pick(&['✦', '✧', '*', '·', '•', '⋆']);
            self.parts.push(Part {
                x,
                y,
                vx: a.cos() * s * 2.0,
                vy: a.sin() * s,
                grav,
                age: 0.0,
                life: self.rng.range(0.8, 1.8),
                ch,
                color,
                glow: 0.6,
            });
        }
    }

    /// A few glyphs (♥, ?, ✧, ♪ …) floating gently up from a spot.
    pub fn emote(&mut self, x: f32, y: f32, ch: char, color: Rgb, n: usize) {
        for i in 0..n {
            self.parts.push(Part {
                x: x + self.rng.range(-1.2, 1.2),
                y: y - i as f32 * 0.4,
                vx: self.rng.range(-0.8, 0.8),
                vy: -self.rng.range(1.6, 2.6),
                grav: 0.0,
                age: -(i as f32) * 0.18,
                life: self.rng.range(1.2, 1.8),
                ch,
                color,
                glow: 0.25,
            });
        }
    }

    pub fn snow_puff(&mut self, x: f32, y: f32) {
        self.sound(Sfx::Puff, x, 1.0);
        for _ in 0..18 {
            let vx = self.rng.range(-9.0, 9.0);
            let vy = self.rng.range(-9.0, -3.0);
            let ch = *self.rng.pick(&['*', '·', '•', '❄', '.']);
            self.parts.push(Part { x, y, vx, vy, grav: 14.0, age: 0.0, life: self.rng.range(0.7, 1.4), ch, color: Rgb::hex(0xeef4ff), glow: 0.0 });
        }
    }

    pub fn shooting_star(&mut self, x: f32, y: f32) {
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        let speed = self.rng.range(160.0, 240.0);
        let ang = self.rng.range(0.2, 0.5);
        self.streaks.push(Streak { x, y, vx: dir * speed * ang.cos(), vy: speed * ang.sin(), age: 0.0, life: self.rng.range(0.6, 1.0), len: 22.0, bright: 1.0 });
    }

    pub fn notes(&mut self, x: f32, y: f32, n: usize) {
        for i in 0..n {
            let ch = *self.rng.pick(&['♪', '♫', '♩', '♬']);
            self.parts.push(Part {
                x: x + self.rng.range(-1.0, 1.0),
                y: y - i as f32 * 0.3,
                vx: self.rng.range(-2.0, 2.0),
                vy: self.rng.range(-3.0, -1.5),
                grav: 0.0,
                age: -(i as f32) * 0.35,
                life: 3.5,
                ch,
                color: Rgb::hex(0xffd36b),
                glow: 0.4,
            });
        }
    }

    pub fn firework(&mut self, ctx: &Ctx) {
        self.happenings.push((super::dialogue::Ev::Fireworks, None));
        let colors = [pal::RED, pal::GOLD, pal::GREEN, pal::BLUE, Rgb::hex(0xff7ae0), Rgb::WHITE];
        let x = ctx.cam + self.rng.range(ctx.vw as f32 * 0.15, ctx.vw as f32 * 0.85);
        let color = *self.rng.pick(&colors);
        self.sound(Sfx::Launch, x, 1.0);
        self.rockets.push(Rocket { x, y: ctx.h as f32 * 0.7, vy: -self.rng.range(14.0, 20.0), burst_y: self.rng.range(4.0, ctx.sky_rows as f32 * 0.6), color });
    }

    /// Cursor effects. (x, y) in cells, (vx, vy) in cells/sec.
    /// Moving leaves a twinkling trail; moving through the snowfield kicks
    /// snow up and around in the direction of travel.
    pub fn cursor(&mut self, x: f32, y: f32, vx: f32, vy: f32, on_ground: bool, dt: f32) {
        let speed = (vx * vx + vy * vy * 4.0).sqrt();
        self.crunch_cd -= dt;
        self.sparkle_cd -= dt;
        if speed < 2.0 {
            return;
        }
        // a soft airy sparkle when the cursor really moves
        if speed > 45.0 && self.sparkle_cd <= 0.0 {
            self.sparkle_cd = 0.15;
            self.sound(Sfx::Cursor, x, (speed / 120.0).clamp(0.4, 1.0));
        }
        // sparkle trail
        let n = (speed * dt * 0.6).min(4.0);
        let mut k = n;
        while k > 0.0 {
            if k >= 1.0 || self.rng.chance(k) {
                let ch = *self.rng.pick(&['·', '✧', '⋆', '·', '∙', '✦']);
                let color = *self.rng.pick(&[Rgb::hex(0xffe6a8), Rgb::hex(0xfff6e8), Rgb::hex(0xffd36b), Rgb::hex(0xbfe0ff)]);
                self.parts.push(Part {
                    x: x + self.rng.range(-1.0, 1.0),
                    y: y + self.rng.range(-0.5, 0.5),
                    vx: self.rng.range(-1.5, 1.5) - vx * 0.05,
                    vy: self.rng.range(-0.5, 1.2),
                    grav: 0.6,
                    age: 0.0,
                    life: self.rng.range(0.4, 0.9),
                    ch,
                    color,
                    glow: 0.25,
                });
            }
            k -= 1.0;
        }
        // kick snow up when ploughing through the snowfield
        if on_ground {
            let n = (speed * dt * 1.2).min(8.0) as usize + self.rng.chance((speed * dt * 1.2).fract()) as usize;
            if n > 0 && self.crunch_cd <= 0.0 {
                self.crunch_cd = 0.12;
                self.sound(Sfx::Crunch, x, (speed / 40.0).clamp(0.3, 1.0));
            }
            for _ in 0..n {
                let ch = *self.rng.pick(&['*', '·', '•', '.', '❄', '·']);
                self.parts.push(Part {
                    x: x + self.rng.range(-1.5, 1.5),
                    y: y + self.rng.range(-0.3, 0.3),
                    vx: vx * self.rng.range(0.25, 0.6) + self.rng.range(-6.0, 6.0),
                    vy: -self.rng.range(4.0, 10.0) - speed * 0.04,
                    grav: 16.0,
                    age: 0.0,
                    life: self.rng.range(0.6, 1.3),
                    ch,
                    color: Rgb::hex(0xeef4ff),
                    glow: 0.0,
                });
            }
        }
    }

    /// Spawn an actor already on screen (previews).
    pub fn spawn_near(&mut self, kind: Kind, ctx: &Ctx) {
        if kind != Kind::Train {
            self.spawn_requests.push(kind);
            return;
        }
        // A refused spawn pushes nothing, so move an existing train rather
        // than whatever was pushed last.
        let target = if self.spawn_actor(kind, ctx) {
            self.actors.last_mut()
        } else {
            self.actors.iter_mut().find(|a| a.kind == kind)
        };
        if let Some(a) = target {
            let frac = match kind {
                Kind::Deer => 0.24,
                Kind::Fox => 0.62,
                Kind::Rabbit => 0.44,
                Kind::Walker | Kind::Kid => 0.57,
                Kind::Train => 0.30,
            };
            a.x = ctx.cam + ctx.vw as f32 * frac;
            // a right-moving train has its locomotive at the far end
            // (x + TRAIN_LEN); put the locomotive at the spot, as for a
            // left-moving one, so it doesn't run straight behind the main tree
            if kind == Kind::Train && a.dir > 0.0 {
                a.x -= TRAIN_LEN as f32;
            }
        }
    }

    /// Returns false if the spawn was refused.
    fn spawn_actor(&mut self, kind: Kind, ctx: &Ctx) -> bool {
        if kind == Kind::Train {
            return self.spawn_train(ctx);
        }
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        // sprites are drawn rightwards from x, so a right-moving actor must
        // start a whole sprite-width off-screen (the 35-cell train used to
        // pop in with its locomotive already visible)
        let len = if kind == Kind::Train { TRAIN_LEN + 2 } else { 12 };
        let x = if dir > 0.0 { -len as f32 } else { ctx.w as f32 + 12.0 };
        let gx = ((x.max(0.0) as usize).min(ctx.ground_row.len() - 1)) as usize;
        let low = (ctx.ground_row[gx] + 4).min(ctx.h - 2);
        let y = if kind == Kind::Train { ctx.track_row } else { self.rng.int(low, ctx.h - 1) };
        if kind == Kind::Train {
            self.sound(Sfx::TrainWhistle, x, 1.0);
        }
        let (speed, color) = match kind {
            Kind::Deer => (self.rng.range(4.0, 7.0), Rgb::hex(0x8a5a32)),
            Kind::Fox => (self.rng.range(9.0, 13.0), Rgb::hex(0xd8682a)),
            Kind::Rabbit => (self.rng.range(6.0, 9.0), Rgb::hex(0xd8d4cc)),
            Kind::Walker | Kind::Kid => {
                let coats = [Rgb::hex(0xb3202a), Rgb::hex(0x1f6e3a), Rgb::hex(0x2a4c8a), Rgb::hex(0x6a3a8a)];
                (self.rng.range(3.0, 4.5), *self.rng.pick(&coats))
            }
            Kind::Train => (self.rng.range(7.0, 10.0), Rgb::hex(0x7a1f26)),
        };
        self.actors.push(Actor { kind, x, y, dir, speed, age: 0.0, pause: 0.0, color, served: false });
        true
    }

    fn random_event(&mut self, t: f32, ctx: &Ctx) {
        let roll = self.rng.int(0, 100);
        match roll {
            0..=17 => self.spawn_requests.push(Kind::Deer),
            18..=27 => self.spawn_requests.push(Kind::Fox),
            28..=37 => self.spawn_requests.push(Kind::Rabbit),
            38..=46 => self.spawn_requests.push(Kind::Walker),
            47..=52 => {
                // kids come in pairs (snowball fights need two)
                self.spawn_requests.push(Kind::Kid);
                self.spawn_requests.push(Kind::Kid);
            }
            53..=61 => {
                self.spawn_train(ctx);
            }
            62..=71 => {
                // don't flip the wind around in the middle of a running gust
                let dir = if self.rng.chance(0.65) { 1.0 } else { -1.0 };
                if !self.gust.active(t) {
                    self.gust_dir = dir;
                }
                self.gust.fire(t, self.rng.range(5.0, 10.0));
            }
            72..=77 => {
                for i in 0..self.rng.int(5, 10) {
                    let x = self.rng.range(10.0, ctx.vw as f32 - 10.0);
                    let y = self.rng.range(2.0, ctx.sky_rows as f32 * 0.8);
                    self.shooting_star(x, y);
                    self.streaks.last_mut().unwrap().age = -(i as f32) * self.rng.range(0.3, 0.8);
                }
            }
            78..=83 => {
                self.flare.fire(t, self.rng.range(25.0, 45.0));
                self.happenings.push((super::dialogue::Ev::Aurora, None));
            }
            84..=88 => {
                // announced: everyone gathers round the big tree first
                if self.show_at.is_none() && !self.show.active(t) {
                    let dur = self.rng.range(14.0, 22.0);
                    self.show_at = Some((t + 10.0, dur));
                    self.gather = Some(t + 10.0 + dur);
                }
            }
            89..=94 => self.house_toggle = Some(self.rng.int(0, 64) as usize),
            95..=96 => {
                self.blizzard.fire(t, self.rng.range(15.0, 25.0));
                self.happenings.push((super::dialogue::Ev::Blizzard, None));
            }
            _ => {
                // rare: a comet drifting across the whole sky
                let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                let x = if dir > 0.0 { -10.0 } else { ctx.vw as f32 + 10.0 };
                self.streaks.push(Streak {
                    x,
                    y: self.rng.range(4.0, ctx.sky_rows as f32 * 1.2),
                    vx: dir * ctx.vw as f32 / 45.0,
                    vy: self.rng.range(0.2, 0.8),
                    age: 0.0,
                    life: 50.0,
                    len: 70.0,
                    bright: 0.7,
                });
            }
        }
    }

    // ------------------------------------------------------------------

    pub fn update(&mut self, dt: f32, t: f32, ctx: &Ctx) {
        if t >= self.next && self.mult > 0.0 {
            self.random_event(t, ctx);
            self.next = t + self.rng.range(6.0, 18.0) / self.mult;
        }
        if let Some((start, dur)) = self.show_at {
            if t >= start {
                self.show_at = None;
                self.light_show(t, dur);
            }
        }
        // (a room indoors has no bells, trains or traffic of its own)
        if !self.indoor {
            // calendar events
            let now = crate::clock::now();
            let key = now.ordinal() as i64 * 100 + now.hour() as i64;
            if now.minute() == 0 && self.bell_key != key {
                self.bell_key = key;
                LAST_BELL_KEY.store(key, Ordering::Relaxed);
                self.bells_until = t + 8.0 + (now.hour() % 12) as f32;
                // toll the hour
                self.bell_strikes = if now.hour() % 12 == 0 { 12 } else { now.hour() % 12 };
                self.next_strike = t + 1.0;
            }
            if self.bell_strikes > 0 && t >= self.next_strike {
                self.bell_strikes -= 1;
                self.next_strike = t + 2.6;
                if let Some((cx, _)) = ctx.church {
                    self.sound(Sfx::ChurchBell, cx as f32, 1.0);
                }
            }
            if t < self.bells_until && self.rng.chance(dt * 1.6) {
                if let Some((cx, cy)) = ctx.church {
                    self.notes(cx as f32, cy as f32, 1);
                }
            }
            let nye = (now.month() == 12 && now.day() == 31 && now.hour() >= 20) || (now.month() == 1 && now.day() == 1 && now.hour() < 1);
            if nye && t > self.next_firework {
                self.firework(ctx);
                let midnight = now.month() == 1 && now.hour() == 0 && now.minute() < 15;
                self.next_firework = t + if midnight { self.rng.range(0.2, 0.8) } else { self.rng.range(3.0, 9.0) };
            }

            // trains: every minute or so (several can be out on the long line)
            if t >= self.next_train && self.mult > 0.0 && !self.quiet {
                self.spawn_train(ctx);
                self.next_train = t + self.rng.range(35.0, 75.0) / self.mult.max(0.3);
            }
            // traffic along the whole road, both ways (each direction in its lane)
            let target = (ctx.w as usize / 50).max(4);
            if !self.cars_seeded && !self.quiet {
                self.cars_seeded = true;
                for _ in 0..target {
                    let x = self.rng.range(0.0, ctx.w as f32);
                    if !self.cars.iter().any(|c| (c.x - x).abs() < 10.0) {
                        self.add_car(x);
                    }
                }
            }
            self.car_cd -= dt;
            if self.cars.len() < target && self.car_cd <= 0.0 && !self.quiet {
                self.car_cd = self.rng.range(0.4, 2.0);
                let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                let x = if dir > 0.0 { -10.0 } else { ctx.w as f32 + 4.0 };
                // room at the on-ramp?
                if !self.cars.iter().any(|c| c.dir == dir && (c.x - x).abs() < 14.0) {
                    self.add_car(x);
                    self.cars.last_mut().unwrap().dir = dir;
                }
            }
            for c in &mut self.cars {
                c.x += c.dir * c.speed * dt;
            }
            // don't drive through the car ahead: match its speed
            let n = self.cars.len();
            for i in 0..n {
                for j in 0..n {
                    let (a, b) = (&self.cars[i], &self.cars[j]);
                    if i != j && a.dir == b.dir && (b.x - a.x) * a.dir > 0.0 && (b.x - a.x).abs() < (a.len() + 3) as f32 && a.speed > b.speed {
                        let v = b.speed;
                        self.cars[i].speed = v;
                    }
                }
            }
            self.cars.retain(|c| c.x > -20.0 && c.x < ctx.w as f32 + 20.0);
        }

        // actors
        // trains stop at the station; a train behind a stopped one waits
        let trains: Vec<(f32, f32, bool)> = self.actors.iter().filter(|a| a.kind == Kind::Train).map(|a| (a.x, a.dir, a.pause > 0.0)).collect();
        for a in &mut self.actors {
            a.age += dt;
            if a.kind == Kind::Train && a.pause <= 0.0 {
                let mid = a.x + TRAIN_LEN as f32 / 2.0;
                if let (false, Some((sx, sw))) = (a.served, ctx.station) {
                    let scx = sx as f32 + sw as f32 / 2.0;
                    if (mid - scx) * a.dir >= 0.0 && (mid - scx).abs() < 2.0 {
                        a.served = true;
                        a.pause = 9.0;
                        self.sounds.push(SoundEvent { sfx: Sfx::TrainWhistle, pan: scx, gain: 0.9 });
                        let n = self.rng.int(1, 4);
                        for k in 0..n {
                            self.arrivals.push(scx - 4.0 + k as f32 * 3.0);
                        }
                    }
                }
                let blocked = trains.iter().any(|&(ox, odir, stopped)| stopped && odir == a.dir && (ox - a.x) * a.dir > 0.0 && (ox - a.x).abs() < TRAIN_LEN as f32 + 8.0);
                if blocked {
                    continue;
                }
            }
            if a.pause > 0.0 {
                a.pause -= dt;
                continue;
            }
            let hop = if a.kind == Kind::Rabbit { ((a.age * 3.0).fract() < 0.5) as i32 as f32 } else { 1.0 };
            a.x += a.dir * a.speed * dt * hop;
            if a.kind == Kind::Deer && self.rng.chance(dt * 0.12) {
                a.pause = self.rng.range(2.0, 5.0);
            }
            if matches!(a.kind, Kind::Deer | Kind::Fox | Kind::Rabbit | Kind::Walker) && self.rng.chance(dt * 1.5) {
                self.prints.push((a.x as i32 + 1, a.y, t));
            }
        }
        let w = ctx.w as f32;
        self.actors.retain(|a| a.x > -40.0 && a.x < w + 40.0);
        // the train whistles now and then as it goes
        if let Some(x) = self.train_x(ctx.cam + ctx.vw as f32 / 2.0) {
            if self.rng.chance(dt * 0.04) {
                self.sound(Sfx::TrainWhistle, x, 0.8);
            }
        }
        self.prints.retain(|p| t - p.2 < 30.0);

        for p in &mut self.parts {
            p.age += dt;
            if p.age < 0.0 {
                continue;
            }
            p.vy += p.grav * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
        self.parts.retain(|p| p.age < p.life);
        let mut lit = vec![];
        for s in &mut self.streaks {
            let was = s.age;
            s.age += dt;
            if was <= 0.0 && s.age > 0.0 {
                lit.push((if s.life > 10.0 { Sfx::Comet } else { Sfx::ShootingStar }, s.x));
                let ev = if s.life > 10.0 { super::dialogue::Ev::Comet } else { super::dialogue::Ev::ShootingStar };
                self.happenings.push((ev, Some(s.x + ctx.cam)));
            }
            if s.age > 0.0 {
                s.x += s.vx * dt;
                s.y += s.vy * dt;
            }
        }
        self.streaks.retain(|s| s.age < s.life);
        for (sfx, x) in lit {
            // streaks live on screen; sounds are placed in the world
            let x = x + ctx.cam;
            self.sound(sfx, x, if sfx == Sfx::ShootingStar { 0.8 } else { 1.0 });
        }
        self.bubbles.retain(|b| t - b.born < b.life);
        let mut bursts = vec![];
        for r in &mut self.rockets {
            r.y += r.vy * dt;
            if r.y <= r.burst_y {
                bursts.push((r.x, r.y, r.color));
            }
        }
        self.rockets.retain(|r| r.y > r.burst_y);
        for (x, y, c) in bursts {
            self.sound(Sfx::Firework, x, 1.0);
            self.burst(x, y, 40, &[c, c, Rgb::WHITE], 9.0, 3.0);
        }
    }

    /// A train rolls in from just beyond the edge of the view. There's one
    /// track, so every train runs the same way, at the same speed, spaced
    /// out. Returns false if there's no room for one right now.
    fn spawn_train(&mut self, ctx: &Ctx) -> bool {
        let running: Vec<(f32, f32, f32)> = self.actors.iter().filter(|a| a.kind == Kind::Train).map(|a| (a.x, a.dir, a.speed)).collect();
        let (dir, speed) = match running.first() {
            Some(&(_, d, s)) => (d, s),
            None => (if self.rng.chance(0.5) { 1.0 } else { -1.0 }, self.rng.range(7.0, 10.0)),
        };
        let len = TRAIN_LEN as f32;
        let x = if dir > 0.0 { ctx.cam - len - 4.0 } else { ctx.cam + ctx.vw as f32 + 4.0 };
        let x = x.clamp(-len - 2.0, ctx.w as f32 + 2.0);
        if running.iter().any(|&(ox, ..)| ox < x + len + 40.0 && ox + len + 40.0 > x) {
            return false;
        }
        self.sound(Sfx::TrainWhistle, x + len / 2.0, 1.0);
        self.actors.push(Actor { kind: Kind::Train, x, y: ctx.track_row, dir, speed, age: 0.0, pause: 0.0, color: Rgb::hex(0x7a1f26), served: false });
        true
    }

    fn add_car(&mut self, x: f32) {
        let colors = [0xb3202a, 0x1f6e3a, 0x2a4c8a, 0xd8d4cc, 0x3a3a44, 0xc8862a, 0x6a3a8a, 0x8fb8d8];
        let bus = self.rng.chance(0.12);
        let color = if bus { Rgb::hex(0xc8a02a) } else { Rgb::hex(*self.rng.pick(&colors)) };
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        let speed = if bus { self.rng.range(7.0, 9.0) } else { self.rng.range(9.0, 16.0) };
        self.cars.push(Car { x, dir, speed, color, bus, honk: -1.0 });
    }

    /// The car at (x, y) (world column, row), if any shows there.
    pub fn car_at(&self, x: i32, y: i32, ctx: &Ctx) -> Option<usize> {
        self.cars.iter().position(|c| {
            let near = (c.dir > 0.0) != self.drive_left;
            let road = ctx.track_row + if near { 2 } else { 1 };
            let x0 = c.x as i32;
            (y - road).abs() <= 1 && x >= x0 - 1 && x <= x0 + c.len() && road < ctx.h && x >= 0 && x < ctx.w && !ctx.occluded[(road * ctx.w + x) as usize]
        })
    }

    /// A clicked car honks and flashes its high beams.
    pub fn honk(&mut self, i: usize, t: f32) {
        let Some(c) = self.cars.get_mut(i) else { return };
        c.honk = t + 0.9;
        let x = c.x;
        self.sound(Sfx::Honk, x, 0.8);
    }

    /// Whether (x, y) is on a train.
    pub fn train_at(&self, x: i32, y: i32) -> bool {
        self.actors.iter().any(|a| a.kind == Kind::Train && (y - a.y).abs() <= 1 && x >= a.x as i32 - 1 && x <= a.x as i32 + TRAIN_LEN)
    }

    /// A clicked train: a whistle, a big puff of steam, every window aglow.
    pub fn toot(&mut self, x: f32, y: f32, t: f32) {
        self.train_glow = t + 2.5;
        self.sound(Sfx::TrainWhistle, x, 0.9);
        self.say(x.round() as i32, y as i32 - 2, "toot toot!", t, 1.8, 6);
        for _ in 0..3 {
            let dx = self.rng.range(-3.0, 3.0);
            self.snow_puff(x + dx, y - 1.0);
        }
    }

    /// The shooting star at (sx, y) on screen (column, row), if any.
    pub fn star_at(&self, sx: i32, y: i32) -> Option<usize> {
        self.streaks.iter().position(|s| {
            if s.age < 0.0 || s.life > 10.0 {
                return false; // (comets are the sky's own business)
            }
            let speed = (s.vx * s.vx + s.vy * s.vy).sqrt().max(0.01);
            let (dx, dy) = (s.vx / speed, s.vy / speed);
            // anywhere along the bright head of the trail
            (0..10).any(|k| {
                let d = k as f32 * 1.2;
                ((s.x - dx * d) - sx as f32).abs() < 2.5 && ((s.y - dy * d) / 2.0 - y as f32).abs() < 1.5
            })
        })
    }

    /// Catch a shooting star: it bursts into sparkles (at world column
    /// `wx`, which is where on screen it was plus the camera).
    pub fn catch_star(&mut self, i: usize, wx: f32) {
        if i >= self.streaks.len() {
            return;
        }
        let s = self.streaks.remove(i);
        let y = s.y / 2.0;
        self.burst(wx, y, 18, &[Rgb::hex(0xfff6dc), Rgb::hex(0xffd36b), Rgb::hex(0xbfe4ff)], 5.0, 1.5);
        self.sound(Sfx::Starburst, wx, 0.8);
    }

    /// Pixel-layer things: meteor/comet streaks and rocket trails.
    pub fn draw_px(&self, f: &mut Frame) {
        for s in &self.streaks {
            if s.age < 0.0 {
                continue;
            }
            let env = (std::f32::consts::PI * (s.age / s.life)).sin().max(0.0).powf(if s.life > 10.0 { 0.3 } else { 1.0 });
            let speed = (s.vx * s.vx + s.vy * s.vy).sqrt().max(0.01);
            let (dx, dy) = (s.vx / speed, s.vy / speed);
            for i in 0..(s.len as i32 * 2) {
                let d = i as f32 * 0.5;
                let a = (1.0 - d / s.len).powf(1.6) * env * s.bright;
                let tint = if s.life > 10.0 { Rgb::hex(0xbfe4ff) } else { Rgb::hex(0xfff6dc) };
                f.px_add((s.x - dx * d) as i32, (s.y - dy * d) as i32, tint * (a * 0.9));
            }
            f.light_px(s.x, s.y, Rgb::hex(0xfff0c0) * (env * if s.life > 10.0 { 6.0 } else { 3.5 }));
        }
    }

    /// Glyph things behind the trees: the distant train and footprints.
    pub fn draw_back(&self, f: &mut Frame, ctx: &Ctx, t: f32) {
        // the railway line the train runs on
        let ry = ctx.track_row * 2 + 1;
        for x in 0..ctx.w {
            if !ctx.occluded[(ctx.track_row * ctx.w + x) as usize] {
                let c = f.px_get(x, ry);
                f.px_set(x, ry, c * 0.72);
            }
        }
        for &(x, y, born) in &self.prints {
            let k = (1.0 - (t - born) / 30.0).max(0.0);
            if !f.has_g(x, y) {
                f.put_g(x, y, Glyph::new('·', Rgb::hex(0x56607f)).alpha(0.8 * k).lit(0.5));
            }
        }
        // the railway station, by the tracks
        if let Some((sx, sw)) = ctx.station {
            let ty = ctx.track_row;
            let snow = Rgb::hex(0xe8eef8);
            let slate = Rgb::hex(0x3a4466);
            let brick = Rgb::hex(0x6a3a2a);
            let warm = Rgb::hex(0xffcf7a);
            for x in sx..sx + sw {
                f.put_g(x, ty - 4, Glyph::new('▄', snow).lit(0.6));
                f.put_g(x, ty - 3, Glyph::new('▀', snow * 0.85).solid(slate).lit(0.6));
                f.put_g(x, ty - 2, Glyph::new(' ', brick).solid(brick).lit(0.6));
                f.put_g(x, ty - 1, Glyph::new(' ', brick).solid(brick * 0.9).lit(0.6));
                f.put_g(x, ty, Glyph::new('▔', Rgb::hex(0x8a8a9a)).lit(0.6));
            }
            let mid = sx + sw / 2;
            f.put_g(mid, ty - 3, Glyph::new('◷', Rgb::hex(0xf0e8d0)).solid(slate).lit(0.4));
            f.text(mid - 3, ty - 2, "STATION", Rgb::hex(0xffd36b), crate::frame::Bg::Solid(brick), 0.3);
            for &wx in &[sx + 1, sx + 2, sx + sw - 3, sx + sw - 2] {
                f.put_g(wx, ty - 1, Glyph::new('▪', warm).solid(brick * 0.9).lit(0.0));
                f.light_cell(wx, ty - 1, warm * 0.6);
            }
            f.put_g(mid, ty - 1, Glyph::new('▐', Rgb::hex(0x2a1810)).solid(Rgb::hex(0x4a2a18)).lit(0.5));
            // a lamp on the platform
            f.put_g(sx + sw, ty - 2, Glyph::new('▣', warm).lit(0.0));
            f.put_g(sx + sw, ty - 1, Glyph::new('┃', Rgb::hex(0x22242c)).lit(0.6));
            f.light_cell(sx + sw, ty - 2, warm * 1.2);
        }
        // traffic, in front of the railway, behind the houses: two lanes,
        // keeping to the right (nearer lane going right) unless drive_left
        if ctx.track_row + 2 < ctx.h {
            for c in &self.cars {
                let near = (c.dir > 0.0) != self.drive_left;
                let road = ctx.track_row + if near { 2 } else { 1 };
                let n = c.len();
                let x0 = c.x as i32;
                for i in 0..n {
                    let x = x0 + i;
                    if x < 0 || x >= ctx.w || ctx.occluded[(road * ctx.w + x) as usize] {
                        continue;
                    }
                    // i counts from the back of the car to its nose
                    let k = if c.dir > 0.0 { i } else { n - 1 - i };
                    let g = if c.bus {
                        if k == 0 || k == n - 1 { Glyph::new('▆', c.color) } else { Glyph::new('▪', Rgb::hex(0xffd27a)).solid(c.color) }
                    } else {
                        Glyph::new(if k == 0 || k == n - 1 { '▄' } else { '▆' }, c.color)
                    };
                    f.put_g(x, road, g.lit(0.5));
                    if k == n - 1 {
                        // headlights reach ahead along the road (high beams
                        // flashing when it honks)
                        let hi = if t < c.honk && ((c.honk - t) * 8.0) as i32 % 2 == 0 { 3.5 } else { 1.0 };
                        f.light_cell(x + c.dir as i32, road, Rgb::hex(0xfff0c8) * (0.9 * hi));
                        f.light_cell(x + 2 * c.dir as i32, road, Rgb::hex(0xfff0c8) * (0.35 * hi));
                        if hi > 1.0 {
                            f.light_cell(x + 4 * c.dir as i32, road, Rgb::hex(0xfff0c8) * 0.8);
                        }
                    } else if k == 0 {
                        f.light_cell(x, road, Rgb::hex(0xff2020) * 0.5);
                    }
                }
            }
        }
        for a in self.actors.iter().filter(|a| a.kind == Kind::Train) {
            let cars = TRAIN_CARS;
            let mut cells: Vec<(i32, char, Rgb, Option<Rgb>, bool)> = vec![];
            // locomotive first in the direction of travel
            let loco: [(char, Rgb); 5] = [('▄', a.color), ('▟', a.color), ('█', a.color), ('▙', a.color), ('▖', Rgb::hex(0x2a2a2a))];
            let mut body = vec![];
            for (ch, c) in loco {
                body.push((ch, c, None, false));
            }
            for _ in 0..cars {
                body.push(('─', Rgb::hex(0x2a2a2a), None, false));
                body.push(('▐', Rgb::hex(0x1f4a3a), None, false));
                for _ in 0..3 {
                    body.push(('▪', Rgb::hex(0xffd27a), Some(Rgb::hex(0x1f4a3a)), true));
                }
                body.push(('▌', Rgb::hex(0x1f4a3a), None, false));
            }
            if a.dir > 0.0 {
                body.reverse();
            }
            for (i, (ch, c, bg, lit)) in body.into_iter().enumerate() {
                let ch = if a.dir > 0.0 { mirror(ch) } else { ch };
                cells.push((a.x as i32 + i as i32, ch, c, bg, lit));
            }
            for (x, ch, c, bg, lit) in cells {
                if x < 0 || x >= ctx.w || ctx.occluded[(a.y * ctx.w + x) as usize] {
                    continue;
                }
                let mut g = Glyph::new(ch, c).lit(0.5);
                if let Some(b) = bg {
                    g = g.solid(b);
                }
                f.put_g(x, a.y, g);
                if lit {
                    let k = if t < self.train_glow { 1.6 } else { 0.35 };
                    f.light_cell(x, a.y, Rgb::hex(0xffc060) * k);
                }
            }
            // steam
            let lx = if a.dir > 0.0 { a.x as i32 + 4 + cars * 6 } else { a.x as i32 + 1 };
            let (puffs, big) = if t < self.train_glow { (8, 1.8) } else { (4, 1.0) };
            for k in 0..puffs {
                let age = (t * 1.5 + k as f32 / puffs as f32).fract();
                let sx = lx as f32 - a.dir * age * 6.0;
                let sy = a.y as f32 * 2.0 - 1.0 - age * 7.0 * big;
                f.px_disc(sx, sy, (0.8 + age * 2.0) * big, Rgb::hex(0x9aa0b4), (1.0 - age) * 0.35);
            }
        }
    }

    /// Glyph things in front of everything: animals, walkers, particles, bubbles.
    pub fn draw_front(&self, f: &mut Frame, t: f32, glow: f32) {
        for a in self.actors.iter().filter(|a| a.kind != Kind::Train) {
            let step = ((a.age * if a.kind == Kind::Fox { 10.0 } else { 6.0 }) as i32 % 2 == 0) && a.pause <= 0.0;
            let rows: Vec<&str> = match a.kind {
                Kind::Deer => vec!["     ψ", "▗▄▄▄▟▀", if step { " ┃  ┃ " } else { " ╱  ╲ " }],
                Kind::Fox => vec!["    ◢◣", "▂▄▄▄█▘", if step { " ╱ ╲  " } else { " ╲ ╱  " }],
                Kind::Rabbit => vec![" ‖", "▗█▙"],
                Kind::Walker | Kind::Kid => vec![" ▲", " ●", "◘█", if step { " ╨" } else { " ║" }],
                Kind::Train => vec![],
            };
            let hop = if a.kind == Kind::Rabbit && (a.age * 3.0).fract() < 0.5 { -1 } else { 0 };
            let top = a.y - rows.len() as i32 + 1 + hop;
            // mirror every row against the sprite's full width, not the row's
            // own (ragged rows like the rabbit's ear would shift otherwise)
            let n = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
            for (ry, row) in rows.iter().enumerate() {
                let chars: Vec<char> = row.chars().collect();
                for (i, &ch) in chars.iter().enumerate() {
                    if ch == ' ' {
                        continue;
                    }
                    let (cx, ch) = if a.dir > 0.0 { (a.x as i32 + i as i32, ch) } else { (a.x as i32 + n - 1 - i as i32, mirror(ch)) };
                    let col = match (a.kind, ch) {
                        (Kind::Deer, '▗') | (Kind::Deer, '▖') => Rgb::hex(0xeae4d8),
                        (Kind::Deer, 'ψ') => Rgb::hex(0x5a3a22),
                        (Kind::Fox, '▂') => Rgb::hex(0xf2e6d0),
                        (Kind::Walker, '▲') => pal::CANDY_RED,
                        (Kind::Walker, '●') => Rgb::hex(0xe8c39e),
                        (Kind::Walker, '◘') => Rgb::hex(0xffd27a) * 1.3,
                        (Kind::Walker, '╨') | (Kind::Walker, '║') => Rgb::hex(0x2a2a34),
                        _ => a.color,
                    };
                    f.put_g(cx, top + ry as i32, Glyph::new(ch, col).lit(0.6));
                    if a.kind == Kind::Walker && ch == '◘' {
                        let fl = 0.85 + 0.15 * (t * 9.0 + a.x).sin();
                        f.light_cell(cx, top + ry as i32, Rgb::hex(0xffc060) * (4.5 * fl * glow));
                    }
                }
            }
        }
        for r in &self.rockets {
            f.put_g(r.x as i32, r.y as i32, Glyph::new('|', Rgb::hex(0xffe0a0)).lit(0.0));
            f.light_cell(r.x as i32, r.y as i32, r.color * 1.5);
        }
        for p in &self.parts {
            if p.age < 0.0 {
                continue;
            }
            let k = (1.0 - p.age / p.life).clamp(0.0, 1.0);
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            f.put_g(x, y, Glyph::new(p.ch, p.color * (0.6 + 0.6 * k)).alpha(k.sqrt()).lit(0.0));
            if p.glow > 0.0 {
                f.light_cell(x, y, p.color * (p.glow * k * 2.0 * glow));
            }
        }
        // speech bubbles: only once they've started (lines of a conversation
        // are queued ahead of time), oldest first; one that would overlap a
        // bubble already placed is lifted above it, its tail stretched down
        let mut order: Vec<&Bubble> = self.bubbles.iter().filter(|b| t >= b.born && b.x >= f.ox && b.x < f.ox + f.w as i32).collect();
        order.sort_by(|a, b| a.born.partial_cmp(&b.born).unwrap_or(std::cmp::Ordering::Equal));
        let mut placed: Vec<(i32, i32, i32)> = vec![]; // (x0, x1, y0)
        for b in order {
            let age = t - b.born;
            let shown = ((age * 40.0) as usize).min(b.text.chars().count());
            let text: String = b.text.chars().take(shown).collect();
            let w = b.text.chars().count() as i32 + 4;
            // keep the box on screen (its tail still points at b.x)
            let x0 = (b.x - w / 2).min(f.ox + f.w as i32 - w).max(f.ox);
            let mut y0 = b.y - 3;
            for _ in 0..4 {
                let hit = placed.iter().any(|&(px0, px1, py0)| x0 <= px1 + 1 && x0 + w - 1 >= px0 - 1 && (y0 - py0).abs() < 3);
                if !hit {
                    break;
                }
                y0 -= 3;
            }
            placed.push((x0, x0 + w - 1, y0));
            let fade = if age > b.life - 0.5 { (b.life - age) / 0.5 } else { 1.0 };
            let bg = Bg::Mix(Rgb::hex(0xf4efe6), 0.92 * fade);
            let fg = Rgb::hex(0x2a1a14);
            for x in x0..x0 + w {
                let top = if x == x0 { '╭' } else if x == x0 + w - 1 { '╮' } else { '─' };
                let bot = if x == x0 { '╰' } else if x == x0 + w - 1 { '╯' } else if x == b.x { '┬' } else { '─' };
                f.put_g(x, y0, Glyph::new(top, fg).bg(bg).lit(0.0));
                f.put_g(x, y0 + 2, Glyph::new(bot, fg).bg(bg).lit(0.0));
                f.put_g(x, y0 + 1, Glyph::new(' ', fg).bg(bg).lit(0.0));
            }
            f.put_g(x0, y0 + 1, Glyph::new('│', fg).bg(bg).lit(0.0));
            f.put_g(x0 + w - 1, y0 + 1, Glyph::new('│', fg).bg(bg).lit(0.0));
            f.text(x0 + 2, y0 + 1, &text, fg, bg, 0.0);
            let tail = Rgb::hex(0xf4efe6) * fade;
            for y in y0 + 3..b.y {
                f.put_g(b.x, y, Glyph::new('│', tail * 0.8).lit(0.0));
            }
            f.put_g(b.x, b.y.max(y0 + 3), Glyph::new('╵', tail).lit(0.0));
        }
    }
}

/// Horizontal mirror for the box/block characters used in sprites.
pub fn mirror(ch: char) -> char {
    match ch {
        '▗' => '▖',
        '▖' => '▗',
        '▘' => '▝',
        '▝' => '▘',
        '▟' => '▙',
        '▙' => '▟',
        '▛' => '▜',
        '▜' => '▛',
        '◢' => '◣',
        '◣' => '◢',
        '◤' => '◥',
        '◥' => '◤',
        '╱' => '╲',
        '╲' => '╱',
        '▌' => '▐',
        '▐' => '▌',
        '◖' => '◗',
        '◗' => '◖',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_refire_keeps_level() {
        let mut e = Env::default();
        e.fire(0.0, 30.0);
        assert_eq!(e.value(15.0), 1.0);
        // re-firing at the plateau used to snap the level to 0
        e.fire(15.0, 30.0);
        assert!(e.value(15.0) > 0.999);
        assert!(e.value(33.0) > 0.9, "and it now runs on past the old end (30 s)");
        // a shorter re-fire doesn't cut a longer one short
        e.fire(16.0, 2.0);
        assert!(e.value(30.0) > 0.9);
        // mid ramp-out: continues from the current level
        let v = e.value(37.0);
        assert!(v > 0.0 && v < 1.0);
        e.fire(37.0, 20.0);
        assert!((e.value(37.0) - v).abs() < 1e-4);
        assert!(e.value(45.0) > 0.9);
    }
}
