//! Trips out of the village. The ski trip: the train from the village
//! station up into the mountains, the ski lodge at the top (rent skis at the
//! shop, then the chairlift), a run down the mountain (jumps, trees to
//! dodge, flags to pass), and the train home. And a drive: the car parked by
//! the villager's home, out along a snowy night road.
//!
//! Each part is a scene of its own, drawn full screen in the same pixel-art
//! way as the village.

use crate::audio::Sfx;
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{h01, hash2, noise1, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Where {
    /// On the train (up to the mountains, or back home).
    Train { home: bool, since: f32 },
    /// At the ski lodge at the top of the line.
    Lodge,
    /// Up the chairlift.
    Lift { since: f32 },
    /// Skiing down.
    Ski,
    /// Out for a drive.
    Drive,
}

/// What a trip asks of the village (when it's over).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Back {
    /// At the station (off the train).
    Station,
    /// At home (out of the car).
    Home,
}

/// A run down the mountain, going on.
struct Run {
    /// How far down (columns along the slope), and the speed.
    x: f32,
    v: f32,
    /// Height above the snow (rows) and its speed (in the air).
    air: f32,
    vy: f32,
    held: bool,
    boost: f32,
    /// Tumbled into a tree: until then.
    tumble: f32,
    started: f32,
    flags: u32,
    missed: u32,
    jumps: u32,
    /// Flags already passed (or missed).
    passed: Vec<usize>,
    /// Trees already bumped into.
    bumped: Vec<usize>,
}

/// What's on the slope: (column along, kind: 0 tree, 1 flag gate, 2 bump).
fn slope_things(seed: u32) -> Vec<(f32, u8)> {
    let mut rng = Rng::new(seed as u64 ^ 0x5c1);
    let mut v = vec![];
    let mut x = 70.0;
    while x < RUN_LEN - 40.0 {
        let k = match rng.int(0, 10) {
            0..=3 => 0,
            4..=7 => 1,
            _ => 2,
        };
        v.push((x, k));
        x += rng.range(22.0, 44.0);
    }
    v
}

/// How long the run is (columns along the slope).
const RUN_LEN: f32 = 1400.0;

pub struct Trip {
    pub at: Where,
    /// Skis rented at the shop.
    pub skis: bool,
    /// The villager at the lodge: column, facing.
    pub x: f32,
    pub dir: f32,
    walk: f32,
    run: Option<Run>,
    things: Vec<(f32, u8)>,
    /// The drive: how far along the road, speed, where on the road (-1..1).
    road: f32,
    speed: f32,
    lane: f32,
    curve: f32,
    /// What to say (a caption line) and until when.
    pub caption: Option<(String, f32)>,
    /// The last run: (seconds, flags passed, of how many, jumps).
    pub last: Option<(f32, u32, u32, u32)>,
    seed: u32,
    sounds: Vec<(Sfx, f32)>,
    /// Done: back to the village.
    pub done: Option<Back>,
    /// (for the scorecard) runs finished; the best time; distance driven.
    pub runs: u32,
    pub best: Option<f32>,
    pub km: f32,
}

/// Where things stand at the lodge (columns, on an 160-wide layout
/// stretched to the screen): the platform, the shop, the lift.
const PLATFORM: f32 = 0.12;
const SHOP: f32 = 0.42;
const LIFT: f32 = 0.8;

impl Trip {
    pub fn ski(t: f32, seed: u32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Train { home: false, since: t };
        tr.say("all aboard for the mountains!", t, 3.0);
        tr.sounds.push((Sfx::TrainWhistle, 1.0));
        tr
    }

    pub fn drive(t: f32, seed: u32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Drive;
        tr.speed = 0.0;
        tr.say("↑ go · ↓ slow · ← → steer · E: back home", t, 5.0);
        tr.sounds.push((Sfx::Honk, 0.6));
        tr
    }

    fn new(seed: u32) -> Self {
        Trip {
            at: Where::Lodge,
            skis: false,
            x: PLATFORM + 0.04,
            dir: 1.0,
            walk: 0.0,
            run: None,
            things: slope_things(seed),
            road: 0.0,
            speed: 0.0,
            lane: 0.0,
            curve: 0.0,
            caption: None,
            last: None,
            seed,
            sounds: vec![],
            done: None,
            runs: 0,
            best: None,
            km: 0.0,
        }
    }

    fn say(&mut self, s: &str, t: f32, secs: f32) {
        self.caption = Some((s.to_string(), t + secs));
    }

    /// Sounds to play (effect, volume).
    pub fn take_sounds(&mut self) -> Vec<(Sfx, f32)> {
        std::mem::take(&mut self.sounds)
    }

    pub fn rebase(&mut self, s: f32) {
        self.at = match self.at {
            Where::Train { home, since } => Where::Train { home, since: since - s },
            Where::Lift { since } => Where::Lift { since: since - s },
            w => w,
        };
        if let Some(r) = &mut self.run {
            r.tumble -= s;
            r.started -= s;
        }
        if let Some(c) = &mut self.caption {
            c.1 -= s;
        }
    }

    /// E (or Enter): on through whatever's here.
    pub fn interact(&mut self, t: f32) {
        match self.at {
            // (a ride can be hurried along)
            Where::Train { home, .. } => self.arrive(home, t),
            Where::Lift { .. } => self.start_run(t),
            Where::Lodge => {
                let near = |a: f32, x: f32| (a - x).abs() < 0.06;
                if near(SHOP, self.x) {
                    if self.skis {
                        self.say("\"they suit you!\" — the skis are yours for the day", t, 3.0);
                    } else {
                        self.skis = true;
                        self.say("\"here you go — skis, poles, and a hot drink for later\"", t, 3.5);
                        self.sounds.push((Sfx::Present, 0.7));
                    }
                } else if near(LIFT, self.x) {
                    if self.skis {
                        self.at = Where::Lift { since: t };
                        self.say("up the chairlift… the village is tiny down there", t, 3.0);
                        self.sounds.push((Sfx::Click, 0.6));
                    } else {
                        self.say("skis first — the shop's just along there", t, 3.0);
                    }
                } else if near(PLATFORM, self.x) {
                    self.at = Where::Train { home: true, since: t };
                    self.say("the train home, down the mountain", t, 3.0);
                    self.sounds.push((Sfx::TrainWhistle, 0.9));
                } else {
                    self.say("the shop (skis), the lift (up), the platform (home)", t, 2.5);
                }
            }
            Where::Ski => {}
            Where::Drive => self.done = Some(Back::Home),
        }
    }

    fn arrive(&mut self, home: bool, t: f32) {
        if home {
            self.done = Some(Back::Station);
        } else {
            self.at = Where::Lodge;
            self.x = PLATFORM + 0.03;
            self.say("Pine Peak! skis at the shop, then up the lift", t, 4.0);
            self.sounds.push((Sfx::TrainWhistle, 0.7));
        }
    }

    fn start_run(&mut self, t: f32) {
        self.at = Where::Ski;
        self.run = Some(Run { x: 0.0, v: 10.0, air: 0.0, vy: 0.0, held: false, boost: 0.0, tumble: 0.0, started: t, flags: 0, missed: 0, jumps: 0, passed: vec![], bumped: vec![] });
        self.say("→ tuck (faster) · ← slow · Space jump (hold: higher) · through the flags!", t, 4.0);
    }

    /// Space: jump (skiing); held, higher.
    pub fn jump(&mut self, held: bool) {
        if let Some(r) = &mut self.run {
            if held && r.air <= 0.0 && r.tumble <= 0.0 {
                r.vy = -16.0;
                r.air = 0.01;
                r.boost = 0.35;
            }
            r.held = held;
            if !held {
                r.boost = 0.0;
            }
        }
    }

    /// A frame: `mv` the direction keys.
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        if self.caption.as_ref().is_some_and(|c| t > c.1) {
            self.caption = None;
        }
        match self.at {
            Where::Train { home, since } => {
                if t - since > 9.0 {
                    self.arrive(home, t);
                }
            }
            Where::Lift { since } => {
                if t - since > 7.0 {
                    self.start_run(t);
                }
            }
            Where::Lodge => {
                if mv.0 != 0.0 {
                    self.dir = mv.0.signum();
                    self.x = (self.x + mv.0 * 0.09 * dt).clamp(0.04, 0.95);
                    self.walk += dt * 9.0;
                }
            }
            Where::Ski => self.ski_step(dt, t, mv),
            Where::Drive => {
                // speed up, slow down, steer; the road bends this way and that
                let top = 1.0;
                self.speed = if mv.1 < 0.0 { (self.speed + dt * 0.35).min(top) } else if mv.1 > 0.0 { (self.speed - dt * 0.9).max(0.0) } else { (self.speed - dt * 0.05).max(0.0) };
                self.road += self.speed * dt * 60.0;
                self.km += self.speed * dt * 0.025;
                self.curve = noise1(self.road * 0.004, self.seed) * 2.0 - 1.0;
                self.lane = (self.lane + mv.0 * dt * 1.6 - self.curve * self.speed * dt * 0.9).clamp(-1.3, 1.3);
                if self.lane.abs() > 1.0 {
                    self.speed = (self.speed - dt * 0.8).max(0.12);
                }
            }
        }
    }

    fn ski_step(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        let things = self.things.clone();
        let Some(r) = &mut self.run else { return };
        if r.tumble > t {
            r.v = (r.v - dt * 20.0).max(0.0);
        } else {
            // downhill: faster and faster; a tuck faster still, a snowplough
            // slower
            let pull = 9.0 + if mv.0 > 0.0 { 10.0 } else { 0.0 };
            r.v += pull * dt;
            if mv.0 < 0.0 {
                r.v -= 26.0 * dt;
            }
            r.v = r.v.clamp(4.0, 70.0) - r.v * 0.08 * dt;
        }
        r.x += r.v * dt;
        // in the air
        if r.air > 0.0 {
            if r.held && r.boost > 0.0 && r.vy < 0.0 {
                r.boost -= dt;
            } else {
                r.vy += 46.0 * dt;
            }
            r.air -= r.vy * dt;
            if r.air <= 0.0 {
                r.air = 0.0;
                r.vy = 0.0;
                self.sounds.push((Sfx::Drop, 0.4));
            }
        }
        for (i, &(x, k)) in things.iter().enumerate() {
            let near = (x - r.x).abs() < 2.5;
            match k {
                // a tree: over it, or into it
                0 if near && r.air < 2.5 && !r.bumped.contains(&i) => {
                    r.bumped.push(i);
                    r.tumble = t + 1.2;
                    self.sounds.push((Sfx::Puff, 0.8));
                    self.caption = Some(("oof — into the branches!".into(), t + 1.5));
                }
                // a gate: through it on the snow
                1 if r.x > x && !r.passed.contains(&i) => {
                    r.passed.push(i);
                    if r.air < 1.0 && r.tumble <= t {
                        r.flags += 1;
                        self.sounds.push((Sfx::Chime, 0.3));
                    } else {
                        r.missed += 1;
                    }
                }
                // a bump: off it into the air
                2 if near && r.air <= 0.0 && r.v > 14.0 => {
                    r.vy = -(r.v * 0.45).min(26.0);
                    r.air = 0.01;
                    r.jumps += 1;
                    self.sounds.push((Sfx::Pickup, 0.4));
                }
                _ => {}
            }
        }
        if r.x >= RUN_LEN {
            let secs = t - r.started;
            let gates = things.iter().filter(|th| th.1 == 1).count() as u32;
            self.last = Some((secs, r.flags, gates, r.jumps));
            self.runs += 1;
            self.best = Some(self.best.map_or(secs, |b| b.min(secs)));
            self.run = None;
            self.at = Where::Lodge;
            self.x = LIFT - 0.02;
            let msg = format!("down in {secs:.1}s · {} of {gates} flags · {} jumps · E at the lift: again", self.last.unwrap().1, self.last.unwrap().3);
            self.say(&msg, t, 6.0);
            self.sounds.push((Sfx::Fanfare, 0.6));
        }
    }

    // ------------------------------------------------------------ drawing

    pub fn draw(&self, f: &mut Frame, t: f32, look: &crate::scene::sprites::Look) {
        f.ox = 0;
        f.oy = 0;
        let (w, h) = (f.w as i32, f.h as i32);
        match self.at {
            Where::Train { since, .. } => self.draw_train(f, t, t - since),
            Where::Lodge => self.draw_lodge(f, t, look),
            Where::Lift { since } => self.draw_lift(f, t, t - since, look),
            Where::Ski => self.draw_ski(f, t, look),
            Where::Drive => self.draw_drive(f, t),
        }
        // the caption, along the bottom
        if let Some((s, _)) = &self.caption {
            let bg = Bg::Mix(pal::UI_PANEL, 0.85);
            let n = s.chars().count() as i32 + 4;
            let x = (w - n) / 2;
            f.text(x, h - 3, &format!("  {s}  "), Rgb::hex(0xf4ecd8), bg, 0.0);
        }
        let hint = match self.at {
            Where::Train { .. } => " E: hurry along · Esc: home ",
            Where::Lift { .. } => " E: skip ahead · Esc: home ",
            Where::Lodge => " ← → walk · E: the shop, the lift, the train · Esc: home ",
            Where::Ski => " → tuck · ← slow · Space jump · Esc: home ",
            Where::Drive => " ↑ go · ↓ brake · ← → steer · E / Esc: home ",
        };
        f.text(1, 1, hint, pal::UI_DIM, Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
    }

    /// Sky and far mountains (pixels), `scroll` along.
    fn backdrop(&self, f: &mut Frame, scroll: f32, horizon: i32, night: f32) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        for y in 0..ph {
            let k = y as f32 / ph as f32;
            let c = Rgb::hex(0x0b1230).lerp(Rgb::hex(0x3a4a7a), k) * (1.0 - night * 0.3);
            for x in 0..pw {
                f.px_set(x, y, c);
            }
        }
        for i in 0..60 {
            let (x, y) = ((hash2(i, 3) % pw.max(1) as u32) as i32, (hash2(i, 5) % (horizon.max(2) as u32)) as i32);
            f.px_set(x, y, Rgb::hex(0xdfe7f4) * (0.4 + 0.6 * h01(hash2(i, (scroll * 0.0) as i32 + 7))));
        }
        for (layer, (amp, base, tone)) in [(44.0, 0.66, 0x5a6a98), (30.0, 0.78, 0x7a8ab8)].into_iter().enumerate() {
            let sp = 0.1 + layer as f32 * 0.15;
            for x in 0..pw {
                // ridged: sharp peaks, not rolling waves
                let u = (x as f32 + scroll * sp) * 0.012;
                let ridge = |v: f32| 1.0 - (v * 2.0 - 1.0).abs();
                let n = ridge(noise1(u, self.seed ^ layer as u32)) * 0.75 + ridge(noise1(u * 3.1, self.seed ^ 9)) * 0.25;
                let top = (horizon as f32 * base - n * amp) as i32;
                for y in top.max(0)..horizon {
                    let snow = y - top < 4 + (n * 5.0) as i32;
                    let c = if snow { Rgb::hex(0xe8eef8) } else { Rgb::hex(tone) };
                    f.px_set(x, y, c * (0.75 + 0.25 * (y - top) as f32 / 30.0).min(1.0));
                }
            }
        }
    }

    fn draw_train(&self, f: &mut Frame, t: f32, el: f32) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        // out of the window: the land rushing by
        let scroll = el * 80.0;
        self.backdrop(f, scroll, ph * 3 / 5, 0.0);
        for x in 0..pw {
            let ground = ph * 3 / 5;
            for y in ground..ph {
                f.px_set(x, y, Rgb::hex(0xdfe7f4) * (0.7 + 0.2 * h01(hash2(x + scroll as i32, y))));
            }
            // pines whipping past
            let tx = (x as f32 + scroll * 1.6) as i32;
            if tx.rem_euclid(37) == 0 {
                for k in 0..22 {
                    for dx in -(k / 2)..=(k / 2) {
                        f.px_set(x + dx, ground - 22 + k + 6, Rgb::hex(0x1e4a2e));
                    }
                }
            }
        }
        // the carriage round the window
        let (wx0, wx1, wy0, wy1) = (pw / 8, pw * 7 / 8, ph / 6, ph * 3 / 5);
        for y in 0..ph {
            for x in 0..pw {
                let inside = x >= wx0 && x < wx1 && y >= wy0 && y < wy1;
                let frame = !inside && x >= wx0 - 3 && x < wx1 + 3 && y >= wy0 - 3 && y < wy1 + 3;
                if frame {
                    f.px_set(x, y, Rgb::hex(0x6a3a22));
                } else if !inside {
                    let panel = if y > wy1 + 3 { Rgb::hex(0x7a2a2a) } else { Rgb::hex(0x8a5a32) };
                    f.px_set(x, y, panel * (0.8 + 0.2 * h01(hash2(x / 12, y / 30))));
                }
            }
        }
        // a sway, and the warm lamps
        let sway = ((t * 9.0).sin() * 0.5).round() as i32;
        for lx in [pw / 4, pw / 2, pw * 3 / 4] {
            f.px_set(lx, 6 + sway, Rgb::hex(0xffd27a));
            f.light_px(lx as f32, 6.0, Rgb::hex(0xffb050) * 4.0);
        }
        let k = (el / 9.0).clamp(0.0, 1.0);
        let bar = (k * (pw - 20) as f32) as i32;
        for x in 10..10 + bar {
            f.px_set(x, ph - 8, Rgb::hex(0xffd36b));
        }
    }

    fn draw_lodge(&self, f: &mut Frame, t: f32, look: &crate::scene::sprites::Look) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let ground = ph * 4 / 5;
        self.backdrop(f, 0.0, ground, 0.0);
        // the big mountain behind, a run down its face
        for x in 0..pw {
            let u = x as f32 / pw as f32;
            let top = (ph as f32 * (0.18 + (u - 0.7).abs() * 0.9)) as i32;
            for y in top..ground {
                let run = ((x as f32 - (pw as f32 * 0.7 - (y - top) as f32 * 0.6)).abs()) < 6.0 + (y - top) as f32 * 0.08;
                let c = if run { Rgb::hex(0xf4f8ff) } else if (x + y * 3) % 23 == 0 && y - top > 10 { Rgb::hex(0x1e4a2e) } else { Rgb::hex(0xdfe7f4) * 0.86 };
                f.px_set(x, y, c);
            }
        }
        for x in 0..pw {
            for y in ground..ph {
                f.px_set(x, y, Rgb::hex(0xe8eef8) * (0.8 + 0.15 * h01(hash2(x, y))));
            }
        }
        // the platform and its sign
        let px0 = (pw as f32 * (PLATFORM - 0.08)) as i32;
        for x in px0..px0 + pw / 6 {
            for y in ground - 3..ground {
                f.px_set(x, y, Rgb::hex(0x5a5a68));
            }
        }
        // the ski shop: a timber chalet with a warm window
        let sx = (pw as f32 * SHOP) as i32;
        let (bw, bh) = (pw / 7, ph / 4);
        for y in ground - bh..ground {
            for x in sx - bw / 2..sx + bw / 2 {
                f.px_set(x, y, if (y - ground) % 4 == 0 { Rgb::hex(0x4a2a16) } else { Rgb::hex(0x6a4428) });
            }
        }
        for k in 0..bw / 2 + 4 {
            for x in sx - k..sx + k {
                f.px_set(x, ground - bh - bw / 2 - 4 + k, if k < 3 { Rgb::hex(0xf4f8ff) } else { Rgb::hex(0x7a2a22) });
            }
        }
        for y in ground - bh * 2 / 3..ground - bh / 3 {
            for x in sx - bw / 4..sx + bw / 4 {
                f.px_set(x, y, Rgb::hex(0xffd27a));
            }
        }
        f.light_px(sx as f32, (ground - bh / 2) as f32, Rgb::hex(0xffb050) * 6.0);
        // the lift: a hut, a pylon, the cable climbing away, chairs on it
        let lx = (pw as f32 * LIFT) as i32;
        for y in ground - ph / 6..ground {
            for x in lx - 8..lx + 8 {
                f.px_set(x, y, Rgb::hex(0x3a4466));
            }
        }
        let (cx0, cy0, cx1, cy1) = (lx, ground - ph / 6, pw + 40, -20);
        for i in 0..400 {
            let u = i as f32 / 400.0;
            let (x, y) = (cx0 as f32 + (cx1 - cx0) as f32 * u, cy0 as f32 + (cy1 - cy0) as f32 * u);
            f.px_set(x as i32, y as i32, Rgb::hex(0x22242c));
        }
        for k in 0..6 {
            let u = ((t * 0.05 + k as f32 / 6.0).fract()) * 0.9;
            let (x, y) = (cx0 as f32 + (cx1 - cx0) as f32 * u, cy0 as f32 + (cy1 - cy0) as f32 * u);
            for dy in 0..5 {
                f.px_set(x as i32, y as i32 + dy, Rgb::hex(0x22242c));
            }
            for dx in -2..3 {
                f.px_set(x as i32 + dx, y as i32 + 5, Rgb::hex(0xc8202a));
            }
        }
        // labels
        let row = ground / 2 - 2;
        let label = |f: &mut Frame, at: f32, s: &str| {
            let x = (f.w as f32 * at) as i32 - s.chars().count() as i32 / 2;
            f.text(x, row - 9, s, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
        };
        label(f, PLATFORM, " ⇄ train home ");
        label(f, SHOP, " ⛷ ski shop ");
        label(f, LIFT, " ⇗ chairlift ");
        // the villager
        let x = self.x * f.w as f32;
        let step = (self.walk as i32) % 2;
        let pose = if (self.walk * 10.0) as i32 % 3 == 0 { crate::scene::sprites::Pose::Stand } else { crate::scene::sprites::Pose::Walk(step as u8) };
        person(f, look, pose, self.dir, x, (ground / 2) as f32 + 0.5, 2.0);
        if self.skis {
            let y = ground - 1;
            for dx in -6..7 {
                f.px_set(x as i32 + dx, y, Rgb::hex(0xc8202a));
            }
        }
    }

    fn draw_lift(&self, f: &mut Frame, _t: f32, el: f32, look: &crate::scene::sprites::Look) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let k = (el / 7.0).clamp(0.0, 1.0);
        // the valley falls away below
        self.backdrop(f, el * 10.0, (ph as f32 * (0.85 + k * 0.4)) as i32, 0.0);
        let ground = (ph as f32 * (0.85 + k * 0.6)) as i32;
        for x in 0..pw {
            for y in ground.min(ph)..ph {
                f.px_set(x, y, Rgb::hex(0xe8eef8) * 0.85);
            }
            if (x + (el * 20.0) as i32).rem_euclid(29) == 0 {
                for kk in 0..14 {
                    for dx in -(kk / 3)..=(kk / 3) {
                        f.px_set(x + dx, ground - 14 + kk, Rgb::hex(0x1e4a2e));
                    }
                }
            }
        }
        // the cable across, and the chair hanging from it
        let (cx, cy) = (pw / 2, ph / 3);
        for x in 0..pw {
            let y = cy - (x - cx) / 2;
            f.px_set(x, y, Rgb::hex(0x22242c));
        }
        for dy in 0..10 {
            f.px_set(cx, cy + dy, Rgb::hex(0x22242c));
        }
        for dx in -8..9 {
            f.px_set(cx + dx, cy + 18, Rgb::hex(0xc8202a));
            f.px_set(cx + dx, cy + 19, Rgb::hex(0x8a1a1a));
        }
        person(f, look, crate::scene::sprites::Pose::Sit, 1.0, cx as f32, (cy + 24) as f32 / 2.0, 2.0);
    }

    fn draw_ski(&self, f: &mut Frame, t: f32, look: &crate::scene::sprites::Look) {
        let Some(r) = &self.run else { return };
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        self.backdrop(f, r.x * 0.5, ph / 2, 0.0);
        // the slope: down to the right; the skier a third of the way along
        let sx = pw / 3;
        let fall = 0.42;
        let surface = |px: i32| -> f32 {
            let along = r.x + (px - sx) as f32;
            ph as f32 * 0.42 + (px - sx) as f32 * fall + noise1(along * 0.02, self.seed) * 6.0
        };
        for x in 0..pw {
            let top = surface(x) as i32;
            for y in top.max(0)..ph {
                let d = (y - top) as f32;
                let c = Rgb::hex(0xf4f8ff).lerp(Rgb::hex(0xb8c8e8), (d / 40.0).min(0.6));
                f.px_set(x, y, c);
            }
            // ski tracks
            let along = (r.x + (x - sx) as f32) as i32;
            if along.rem_euclid(9) < 5 {
                f.px_set(x, top + 2, Rgb::hex(0xc8d4ec));
            }
        }
        // what's on the slope
        for &(x, k) in &self.things {
            let px = sx + (x - r.x) as i32;
            if px < -20 || px > pw + 20 {
                continue;
            }
            let base = surface(px) as i32;
            match k {
                0 => {
                    for kk in 0..20 {
                        for dx in -(kk / 2)..=(kk / 2) {
                            f.px_set(px + dx, base - 22 + kk, Rgb::hex(0x1e4a2e) * (0.8 + 0.2 * h01(hash2(dx, kk))));
                        }
                    }
                    for y in base - 3..base {
                        f.px_set(px, y, Rgb::hex(0x4a2a16));
                    }
                }
                1 => {
                    for (dx, c) in [(-7, 0xc8202a), (7, 0x2a4c8a)] {
                        for y in base - 12..base {
                            f.px_set(px + dx, y, Rgb::hex(0x22242c));
                        }
                        for y in base - 12..base - 7 {
                            f.px_set(px + dx + 1, y, Rgb::hex(c));
                            f.px_set(px + dx + 2, y, Rgb::hex(c));
                        }
                    }
                }
                _ => {
                    for dx in -6i32..7 {
                        let hgt = (6 - dx.abs()) / 2;
                        for y in base - hgt..base {
                            f.px_set(px + dx, y, Rgb::hex(0xf8fbff));
                        }
                    }
                }
            }
        }
        // the skier: tucked, in the air, or tumbling
        let feet_px = surface(sx) - r.air * 2.0;
        let pose = if r.tumble > t {
            crate::scene::sprites::Pose::Lie
        } else if r.air > 0.0 {
            crate::scene::sprites::Pose::Jump
        } else {
            crate::scene::sprites::Pose::Scoop
        };
        person(f, look, pose, 1.0, sx as f32, feet_px / 2.0, 2.0);
        // skis
        let fy = feet_px as i32;
        for dx in -7..9 {
            f.px_set(sx + dx, fy + dx / 3, Rgb::hex(0xc8202a));
        }
        // snow spray
        if r.air <= 0.0 && r.v > 15.0 {
            for k in 0..6 {
                let a = h01(hash2(k, (t * 30.0) as i32));
                f.px_set(sx - 8 - (a * 8.0) as i32, fy - (a * 5.0) as i32, Rgb::hex(0xffffff));
            }
        }
        // how it's going
        let secs = t - r.started;
        let line = format!(" {:.0} km/h · {secs:.1}s · flags {} · {:.0}% down ", r.v * 1.6, r.flags, (r.x / RUN_LEN * 100.0).min(100.0));
        f.text(f.w as i32 - line.chars().count() as i32 - 2, 1, &line, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
    }

    fn draw_drive(&self, f: &mut Frame, t: f32) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let horizon = ph * 2 / 5;
        self.backdrop(f, self.road * 0.05 - self.curve * 40.0, horizon, 0.5);
        // a pseudo-3D road into the distance, bending; snow banks; posts
        for y in horizon..ph {
            let z = (y - horizon) as f32 / (ph - horizon) as f32; // 0 far .. 1 near
            let depth = 1.0 / (z + 0.02);
            let bend = self.curve * (1.0 - z).powi(2) * pw as f32 * 0.5;
            let mid = pw as f32 / 2.0 + bend - self.lane * z * pw as f32 * 0.32;
            let half = z * pw as f32 * 0.42 + 2.0;
            let stripe = ((self.road * 0.3 + depth * 3.0) as i32) % 2 == 0;
            for x in 0..pw {
                let d = (x as f32 - mid).abs();
                let c = if d < half {
                    let line = d < half * 0.03 && stripe;
                    if line {
                        Rgb::hex(0xe8d070)
                    } else {
                        Rgb::hex(0x3a3e4a) * (0.6 + 0.4 * z)
                    }
                } else if d < half * 1.12 {
                    if stripe { Rgb::hex(0xc8202a) } else { Rgb::hex(0xf4f8ff) }
                } else {
                    Rgb::hex(0xe8eef8) * (0.45 + 0.5 * z)
                };
                f.px_set(x, y, c);
            }
            // marker posts along the edges
            if ((self.road * 0.3 + depth * 3.0) as i32).rem_euclid(6) == 0 {
                for side in [-1.0, 1.0] {
                    let x = (mid + side * half * 1.25) as i32;
                    let hgt = (z * 14.0) as i32 + 1;
                    for yy in y - hgt..y {
                        f.px_set(x, yy, Rgb::hex(0xf4f8ff));
                    }
                    f.px_set(x, y - hgt, Rgb::hex(0xff6a3a));
                }
            }
        }
        // headlights on the road ahead
        f.light_px(pw as f32 / 2.0, ph as f32 * 0.7, Rgb::hex(0xfff0c0) * 10.0);
        // the car's bonnet and the wheel
        for y in ph - 10..ph {
            for x in pw / 2 - 30..pw / 2 + 30 {
                let edge = (x - pw / 2).abs() > 30 - (ph - y);
                if !edge {
                    f.px_set(x, y, Rgb::hex(0x9a1c28) * (0.7 + 0.3 * ((ph - y) as f32 / 10.0)));
                }
            }
        }
        // snow flying at the windscreen
        for k in 0..40 {
            let a = (t * (0.6 + self.speed) + h01(hash2(k, 1))).fract();
            let ang = h01(hash2(k, 2)) * std::f32::consts::TAU;
            let r = a * a * pw as f32 * 0.6;
            let (x, y) = (pw as f32 / 2.0 + ang.cos() * r, horizon as f32 + ang.sin() * r * 0.5);
            f.px_set(x as i32, y as i32, Rgb::hex(0xffffff));
        }
        let line = format!(" {:.0} km/h · {:.1} km ", self.speed * 90.0, self.km);
        f.text(f.w as i32 - line.chars().count() as i32 - 2, 1, &line, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
        let _ = Glyph::new(' ', Rgb::BLACK);
    }
}

/// The villager (or anyone) drawn as in the village, `k` times the
/// village's size, centred on `cx`, feet on row `feet`.
fn person(f: &mut Frame, look: &crate::scene::sprites::Look, pose: crate::scene::sprites::Pose, dir: f32, cx: f32, feet: f32, k: f32) {
    use crate::scene::sprites;
    if crate::frame::pictures_ok() {
        let pic = sprites::person(look, pose, dir, 1.0);
        let rows = sprites::rows_of(&pic, k);
        f.sprite(cx, feet, rows, pic);
    } else {
        let (x, y) = (cx.round() as i32, feet.round() as i32);
        f.put_g(x, y - 3, Glyph::new('▲', look.hat.unwrap_or(look.hair)).lit(0.0));
        f.put_g(x, y - 2, Glyph::new('●', look.skin).lit(0.0));
        f.put_g(x, y - 1, Glyph::new('█', look.coat).lit(0.0));
        f.put_g(x, y, Glyph::new('║', Rgb::hex(0x2a2a34)).lit(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ski_trip_there_and_back() {
        let mut tr = Trip::ski(0.0, 5);
        let mut t = 0.0;
        let dt = 1.0 / 30.0;
        // the train up
        while matches!(tr.at, Where::Train { .. }) {
            tr.update(dt, t, (0.0, 0.0));
            t += dt;
        }
        assert_eq!(tr.at, Where::Lodge);
        // no lift without skis
        tr.x = LIFT;
        tr.interact(t);
        assert_eq!(tr.at, Where::Lodge);
        tr.x = SHOP;
        tr.interact(t);
        assert!(tr.skis);
        tr.x = LIFT;
        tr.interact(t);
        assert!(matches!(tr.at, Where::Lift { .. }));
        // up, and down the mountain (tucked, jumping now and then)
        for i in 0..20_000 {
            tr.update(dt, t, (1.0, 0.0));
            tr.jump(i % 40 < 10);
            t += dt;
            if tr.at == Where::Lodge {
                break;
            }
        }
        assert_eq!(tr.at, Where::Lodge, "never got down");
        assert_eq!(tr.runs, 1);
        let (_, flags, gates, _) = tr.last.unwrap();
        assert!(gates > 0 && flags <= gates);
        // the train home
        tr.x = PLATFORM;
        tr.interact(t);
        for _ in 0..400 {
            tr.update(dt, t, (0.0, 0.0));
            t += dt;
        }
        assert_eq!(tr.done, Some(Back::Station));
    }

    #[test]
    fn a_drive_goes_somewhere_and_comes_home() {
        let mut tr = Trip::drive(0.0, 3);
        let mut t = 0.0;
        for _ in 0..600 {
            tr.update(1.0 / 30.0, t, (0.0, -1.0));
            t += 1.0 / 30.0;
        }
        assert!(tr.km > 0.05, "{}", tr.km);
        tr.interact(t);
        assert_eq!(tr.done, Some(Back::Home));
    }
}
