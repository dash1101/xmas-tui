//! Trips out of the village. The ski trip: the shuttle sleigh from the
//! village up to the station, the train (a carriage to walk along, a
//! conductor, folk in the seats, the mountains going by), the ski lodge at
//! the top of the line (skis from the shop, the chairlift), a run down the
//! mountain (lanes, jumps and tricks, flags and snowflakes for points,
//! others racing down too), and the same way home. And a drive: the car
//! parked by the villager's home, out along a quiet snowy road at night.
//!
//! Each part is a scene of its own, drawn full screen in the same pixel-art
//! way as the village.

use crate::audio::Sfx;
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{h01, hash2, noise1, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Where {
    /// The shuttle sleigh between the village and the station (true: on
    /// the way home).
    Shuttle { home: bool, since: f32 },
    /// In the train's carriage, up to the mountains or back down.
    Carriage { home: bool, since: f32 },
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
    /// At the station's shuttle stop.
    Station,
    /// At home (out of the car).
    Home,
}

/// How long the train takes (s); sitting down, it goes by quicker.
const RIDE: f32 = 36.0;
/// How long the shuttle takes (s).
const SHUTTLE: f32 = 5.5;

/// Someone in the carriage: (name, coat, column 0..1, a kid, sat).
struct Passenger {
    name: &'static str,
    coat: Rgb,
    hat: Rgb,
    x: f32,
    kid: bool,
}

/// Someone else on the slope.
struct Rival {
    name: &'static str,
    coat: Rgb,
    x: f32,
    v: f32,
    lane: f32,
    want: f32,
    air: f32,
    vy: f32,
    done: Option<f32>,
    score: u32,
}

/// A run down the mountain, going on.
struct Run {
    /// How far down (columns along the slope), and the speed.
    x: f32,
    v: f32,
    /// Which lane (0 at the front .. LANES-1 at the back; eased toward
    /// `want`).
    lane: f32,
    want: f32,
    /// Height above the snow (rows) and its speed (in the air).
    air: f32,
    vy: f32,
    held: bool,
    boost: f32,
    /// In the air: how far round (degrees) and grabbing the skis.
    spin: f32,
    grab: bool,
    /// Tumbled: until then.
    tumble: f32,
    started: f32,
    score: u32,
    flags: u32,
    gates: u32,
    tricks: u32,
    flakes: u32,
    /// What's been passed (gates, snowflakes, trees bumped).
    passed: Vec<usize>,
    /// The last thing worth a cheer, and until when it shows.
    pop: Option<(String, f32)>,
    rivals: Vec<Rival>,
}

/// Lanes across the slope (front to back).
const LANES: usize = 3;
/// How long the run is (columns along the slope).
const RUN_LEN: f32 = 1600.0;

/// What's on the slope: (column along, lane, kind: 0 tree, 1 gate, 2 bump
/// (all lanes), 3 snowflake).
fn slope_things(seed: u32) -> Vec<(f32, usize, u8)> {
    let mut rng = Rng::new(seed as u64 ^ 0x5c1);
    let mut v = vec![];
    let mut x = 60.0;
    while x < RUN_LEN - 40.0 {
        let lane = rng.int(0, LANES as i32) as usize;
        let k = match rng.int(0, 12) {
            0..=3 => 0,
            4..=6 => 1,
            7..=8 => 2,
            _ => 3,
        };
        v.push((x, lane, k));
        // a snowflake trail now and then
        if k == 3 {
            for i in 1..4 {
                v.push((x + i as f32 * 5.0, lane, 3));
            }
        }
        x += rng.range(16.0, 32.0);
    }
    v
}

/// Someone else on the trip (a friend online): who, how they look, where.
#[derive(Clone, Debug)]
pub struct Fellow {
    pub name: String,
    pub look: crate::scene::sprites::Look,
    pub at: crate::net::TripAt,
}

pub struct Trip {
    pub at: Where,
    /// Friends online on the same trip (set by the village each frame).
    pub fellows: Vec<Fellow>,
    /// A race with friends: no one else on the slope, and the run's
    /// result to tell them (score, seconds) once it's down.
    pub race: bool,
    pub finished: Option<(u32, f32)>,
    /// The others' results in the race (name, score, seconds).
    pub results: Vec<(String, u32, f32)>,
    /// Skis rented at the shop.
    pub skis: bool,
    /// The villager at the lodge or in the carriage: column (0..1), facing.
    pub x: f32,
    pub dir: f32,
    walk: f32,
    /// Sat in a seat on the train.
    sat: bool,
    passengers: Vec<Passenger>,
    /// The conductor: where along (0..1), which way, stopped until.
    conductor: (f32, f32, f32),
    /// Something said in the carriage: (who, what, where along, until).
    talk: Option<(String, String, f32, f32)>,
    /// How far the train's gone (0..1).
    ride: f32,
    run: Option<Run>,
    things: Vec<(f32, usize, u8)>,
    /// The drive: how far along, speed (0..1), which lane (0 near, 1 far,
    /// eased), and the honk.
    road: f32,
    speed: f32,
    lane: f32,
    want_lane: f32,
    /// What to say (a caption line) and until when.
    pub caption: Option<(String, f32)>,
    /// The last run: (seconds, flags passed, of how many, jumps).
    pub last: Option<(f32, u32, u32, u32)>,
    seed: u32,
    rng: Rng,
    sounds: Vec<(Sfx, f32)>,
    /// Done: back to the village.
    pub done: Option<Back>,
    /// (for the scorecard) runs finished; the best time; distance driven;
    /// the best score down the mountain.
    pub runs: u32,
    pub best: Option<f32>,
    pub km: f32,
    pub best_score: u32,
}

/// Where things stand at the lodge (columns, on an 160-wide layout
/// stretched to the screen): the platform, the shop, the lift.
const PLATFORM: f32 = 0.12;
const SHOP: f32 = 0.42;
const LIFT: f32 = 0.8;

const NAMES: [&str; 12] = ["Martha", "Henry", "Agnes", "Walter", "Rosie", "Ned", "Clara", "Otto", "Mabel", "Ezra", "Ivy", "Felix"];
const COATS: [u32; 6] = [0xb3202a, 0x1f6e3a, 0x2a4c8a, 0x6a3a8a, 0xc8862a, 0x3a6a6a];
const CHAT: &[&str] = &[
    "off to Pine Peak? the snow's perfect up there",
    "I always sit on this side — the view's better",
    "my grandad built half the lodge, you know",
    "first time skiing? just lean forward and hope",
    "the cocoa at the top is worth the trip alone",
    "look, deer! out by the trees",
    "I fell off the chairlift once. don't tell anyone",
    "we're going sledding after. want to come?",
];
const CONDUCTOR: &[&str] = &["tickets, please! … lovely, thank you", "next stop, Pine Peak!", "mind the step when we stop", "a fine night for it", "hot drinks in the end carriage!"];

impl Trip {
    pub fn ski(t: f32, seed: u32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Shuttle { home: false, since: t };
        tr.say("the shuttle up to the station…", t, 3.0);
        tr.sounds.push((Sfx::Chime, 0.6));
        tr
    }

    pub fn drive(t: f32, seed: u32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Drive;
        tr.speed = 0.0;
        tr.say("→ faster · ← slower · ↑ ↓ change lanes · Space honk · E home", t, 5.0);
        tr.sounds.push((Sfx::Honk, 0.5));
        tr
    }

    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x7121);
        let passengers = (0..6)
            .filter(|_| rng.chance(0.7))
            .enumerate()
            .map(|(k, _)| Passenger { name: NAMES[(seed as usize + k * 5) % NAMES.len()], coat: Rgb::hex(COATS[k % COATS.len()]), hat: Rgb::hex([0xd8283a, 0x2fae5c, 0x4a8dff, 0xffc23a][k % 4]), x: 0.14 + k as f32 * 0.13, kid: k % 3 == 2 })
            .collect();
        Trip {
            at: Where::Lodge,
            fellows: vec![],
            race: false,
            finished: None,
            results: vec![],
            skis: false,
            x: 0.05,
            dir: 1.0,
            walk: 0.0,
            sat: false,
            passengers,
            conductor: (0.6, -1.0, 0.0),
            talk: None,
            ride: 0.0,
            run: None,
            things: slope_things(seed),
            road: 0.0,
            speed: 0.0,
            lane: 0.0,
            want_lane: 0.0,
            caption: None,
            last: None,
            seed,
            rng,
            sounds: vec![],
            done: None,
            runs: 0,
            best: None,
            km: 0.0,
            best_score: 0,
        }
    }

    /// A race down the mountain with friends: the course from `seed`, the
    /// chairlift up first (it starts at the top in `wait` seconds).
    pub fn racing(t: f32, seed: u32, wait: f32) -> Self {
        let mut tr = Trip::new(seed);
        tr.skis = true;
        tr.race = true;
        tr.at = Where::Lift { since: t + wait.clamp(0.0, 30.0) - 7.0 };
        tr.say("a race! up the chairlift, and it starts at the top…", t, 4.0);
        tr.sounds.push((Sfx::Chime, 0.7));
        tr
    }

    /// Where the villager is on the trip, for friends to see.
    pub fn here(&self, t: f32) -> crate::net::TripAt {
        use crate::net::TripAt;
        let base = TripAt { x: self.x, dir: self.dir, seed: self.seed, ..Default::default() };
        match self.at {
            Where::Shuttle { home, since } => TripAt { at: 1, home, el: t - since, ..base },
            Where::Carriage { home, since } => TripAt { at: 2, home, air: self.ride, sat: self.sat, el: t - since, ..base },
            Where::Lodge => TripAt { at: 3, sat: self.skis, ..base },
            Where::Lift { since } => TripAt { at: 4, el: t - since, ..base },
            Where::Ski => match &self.run {
                Some(r) => TripAt { at: 5, x: r.x, lane: r.lane, air: r.air, score: r.score, sat: r.tumble > t, el: t - r.started, dir: if (((r.spin / 90.0).round() as i32).rem_euclid(4)) == 2 { -1.0 } else { 1.0 }, ..base },
                None => TripAt { at: 3, ..base },
            },
            Where::Drive => TripAt { at: 6, ..base },
        }
    }

    /// Along to where a friend is on their trip (the same shuttle, the same
    /// train, the lodge at the top).
    pub fn join(t: f32, at: crate::net::TripAt) -> Self {
        let mut tr = Trip::new(at.seed);
        match at.at {
            1 => {
                tr.at = Where::Shuttle { home: at.home, since: t - at.el.clamp(0.0, SHUTTLE - 0.5) };
                tr.say("aboard the shuttle — just in time!", t, 3.0);
            }
            2 => {
                tr.board(at.home, t);
                tr.ride = at.air.clamp(0.0, 0.98);
                tr.x = (at.x - 0.05).clamp(0.05, 0.9);
                tr.say("on the same train — they're along the carriage", t, 3.5);
            }
            _ => {
                tr.at = Where::Lodge;
                tr.skis = true;
                tr.x = LIFT - 0.06;
                tr.say("up at Pine Peak with them · E at the lift: down the mountain", t, 4.0);
            }
        }
        tr.sounds.push((Sfx::Chime, 0.6));
        tr
    }

    /// A friend's result in the race.
    pub fn result(&mut self, name: &str, score: u32, secs: f32, t: f32) {
        self.results.retain(|r| r.0 != name);
        self.results.push((name.to_string(), score, secs));
        if self.run.is_none() && self.race {
            let msg = format!("{name} is down: {score} points in {secs:.1}s");
            self.say(&msg, t, 4.0);
        }
    }

    /// Previews (--perch trip-…): straight to a part of the trip.
    pub fn preview(&mut self, what: &str, t: f32) {
        match what {
            "carriage" => {
                self.board(false, t);
                self.ride = 0.6;
                self.x = 0.5;
                if let Some(p) = self.passengers.first() {
                    self.talk = Some((p.name.to_string(), CHAT[0].to_string(), p.x, t + 3.0));
                }
            }
            "lodge" => self.at = Where::Lodge,
            "lift" => {
                self.skis = true;
                self.at = Where::Lift { since: t - 3.0 };
            }
            "ski" => {
                self.skis = true;
                self.start_run(t);
                for k in 0..150 {
                    self.ski_step(1.0 / 30.0, t + k as f32 / 30.0, (1.0, 0.0));
                }
            }
            "drive" => {
                for k in 0..200 {
                    self.update(1.0 / 30.0, t + k as f32 / 30.0, (1.0, 0.0));
                }
            }
            _ => {}
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
            Where::Shuttle { home, since } => Where::Shuttle { home, since: since - s },
            Where::Carriage { home, since } => Where::Carriage { home, since: since - s },
            Where::Lift { since } => Where::Lift { since: since - s },
            w => w,
        };
        if let Some(r) = &mut self.run {
            r.tumble -= s;
            r.started -= s;
            if let Some(p) = &mut r.pop {
                p.1 -= s;
            }
            for rv in &mut r.rivals {
                if let Some(d) = &mut rv.done {
                    *d -= s;
                }
            }
        }
        if let Some(c) = &mut self.caption {
            c.1 -= s;
        }
        if let Some(c) = &mut self.talk {
            c.3 -= s;
        }
        self.conductor.2 -= s;
    }

    /// E (or Enter): on through whatever's here.
    pub fn interact(&mut self, t: f32) {
        match self.at {
            // (a ride can be hurried along)
            Where::Shuttle { home, .. } => self.off_shuttle(home, t),
            Where::Carriage { home, .. } => self.carriage_e(home, t),
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
                    self.board(true, t);
                } else {
                    self.say("the shop (skis), the lift (up), the platform (home)", t, 2.5);
                }
            }
            Where::Ski => {}
            Where::Drive => self.done = Some(Back::Home),
        }
    }

    fn off_shuttle(&mut self, home: bool, t: f32) {
        if home {
            self.done = Some(Back::Station);
        } else {
            self.board(false, t);
        }
    }

    /// Onto the train (up, or home).
    fn board(&mut self, home: bool, t: f32) {
        self.at = Where::Carriage { home, since: t };
        self.ride = 0.0;
        self.x = 0.06;
        self.sat = false;
        self.sounds.push((Sfx::TrainWhistle, 0.9));
        let msg = if home { "the train home — find a seat, or walk the carriage" } else { "all aboard for Pine Peak! ← → walk · E: talk, sit, the door" };
        self.say(msg, t, 4.0);
    }

    /// E in the carriage: a word with whoever's there, a seat, or (arrived)
    /// the door.
    fn carriage_e(&mut self, home: bool, t: f32) {
        if self.sat {
            self.sat = false;
            self.say("up again", t, 1.5);
            return;
        }
        let x = self.x;
        if self.ride >= 1.0 && x > 0.86 {
            if home {
                self.at = Where::Shuttle { home: true, since: t };
                self.say("the shuttle back down into the village…", t, 3.0);
                self.sounds.push((Sfx::Chime, 0.6));
            } else {
                self.at = Where::Lodge;
                self.x = PLATFORM + 0.03;
                self.say("Pine Peak! skis at the shop, then up the lift", t, 4.0);
            }
            self.sounds.push((Sfx::TrainWhistle, 0.6));
            return;
        }
        if (self.conductor.0 - x).abs() < 0.05 {
            let line = (*self.rng.pick(CONDUCTOR)).to_string();
            self.talk = Some(("conductor".into(), line, self.conductor.0, t + 3.0));
            self.conductor.2 = t + 3.0;
            return;
        }
        if let Some(p) = self.passengers.iter().find(|p| (p.x - x).abs() < 0.04) {
            let line = (*self.rng.pick(CHAT)).to_string();
            self.talk = Some((p.name.to_string(), line, p.x, t + 3.5));
            self.sounds.push((Sfx::Greet, 0.4));
            return;
        }
        // an empty seat (one every 13% along): sit, and the ride goes quicker
        let seat = (x - 0.14) / 0.13;
        if (seat - seat.round()).abs() < 0.25 && (0.0..6.0).contains(&seat.round()) {
            self.sat = true;
            self.x = 0.14 + seat.round() * 0.13 + 0.03;
            self.say("ahh. a seat by the window", t, 2.0);
        } else if self.ride < 1.0 {
            self.say("not there yet — sit down, or have a chat", t, 2.0);
        } else {
            self.say("we're here! the door's at the end", t, 2.0);
        }
    }

    fn start_run(&mut self, t: f32) {
        self.at = Where::Ski;
        let n = if self.race { 0 } else { 4 };
        let rivals = (0..n)
            .map(|k| Rival { name: NAMES[(self.seed as usize + k * 3 + 1) % NAMES.len()], coat: Rgb::hex(COATS[(k + 2) % COATS.len()]), x: -6.0 - k as f32 * 9.0, v: 9.0 + k as f32, lane: (k % LANES) as f32, want: (k % LANES) as f32, air: 0.0, vy: 0.0, done: None, score: 0 })
            .collect();
        self.run = Some(Run { x: 0.0, v: 10.0, lane: 1.0, want: 1.0, air: 0.0, vy: 0.0, held: false, boost: 0.0, spin: 0.0, grab: false, tumble: 0.0, started: t, score: 0, flags: 0, gates: 0, tricks: 0, flakes: 0, passed: vec![], pop: None, rivals });
        self.say("→ tuck · ← slow · ↑ ↓ lanes · Space jump · in the air: ← → spin, ↓ grab", t, 5.0);
    }

    /// Space: jump (skiing); held, higher. The car's horn, driving.
    pub fn jump(&mut self, held: bool) {
        if self.at == Where::Drive && held {
            self.sounds.push((Sfx::Honk, 0.7));
            return;
        }
        if let Some(r) = &mut self.run {
            if held && r.air <= 0.0 && r.tumble <= 0.0 {
                r.vy = -16.0;
                r.air = 0.01;
                r.boost = 0.25;
                r.spin = 0.0;
            }
            r.held = held;
            if !held {
                r.boost = 0.0;
            }
        }
    }

    /// A frame: `mv` the direction keys (and `edge`: pressed just now,
    /// for lane changes).
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        if self.caption.as_ref().is_some_and(|c| t > c.1) {
            self.caption = None;
        }
        if self.talk.as_ref().is_some_and(|c| t > c.3) {
            self.talk = None;
        }
        match self.at {
            Where::Shuttle { home, since } => {
                if t - since > SHUTTLE {
                    self.off_shuttle(home, t);
                }
            }
            Where::Carriage { .. } => {
                let pace = if self.sat { 3.5 } else { 1.0 };
                let was = self.ride;
                self.ride = (self.ride + dt * pace / RIDE).min(1.0);
                if was < 1.0 && self.ride >= 1.0 {
                    self.say("we're here! the door's at the far end (→, then E)", t, 5.0);
                    self.sounds.push((Sfx::TrainWhistle, 0.8));
                    self.sat = false;
                }
                if mv.0 != 0.0 && !self.sat {
                    self.dir = mv.0.signum();
                    self.x = (self.x + mv.0 * 0.12 * dt).clamp(0.03, 0.95);
                    self.walk += dt * 9.0;
                }
                // the conductor up and down the aisle, stopping for tickets
                if t > self.conductor.2 {
                    let (cx, cd, _) = self.conductor;
                    let nx = cx + cd * 0.04 * dt;
                    if !(0.08..=0.9).contains(&nx) {
                        self.conductor.1 = -cd;
                    } else {
                        self.conductor.0 = nx;
                    }
                    if self.rng.chance(dt * 0.08) {
                        self.conductor.2 = t + 2.5;
                        let line = (*self.rng.pick(CONDUCTOR)).to_string();
                        self.talk = Some(("conductor".into(), line, self.conductor.0, t + 2.5));
                    }
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
                // a gentle drive: speed up, ease off, the other lane
                self.speed = if mv.0 > 0.0 { (self.speed + dt * 0.3).min(1.0) } else if mv.0 < 0.0 { (self.speed - dt * 0.6).max(0.0) } else { (self.speed - dt * 0.03).max(0.15) };
                if mv.1 != 0.0 {
                    self.want_lane = if mv.1 < 0.0 { 1.0 } else { 0.0 };
                }
                self.lane += (self.want_lane - self.lane) * (1.0 - (-dt * 4.0).exp());
                self.road += self.speed * dt * 40.0;
                self.km += self.speed * dt * 0.022;
            }
        }
    }

    fn ski_step(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        let things = self.things.clone();
        let Some(r) = &mut self.run else { return };
        // lanes: a push up or down moves over one
        if mv.1 != 0.0 && (r.want - r.lane).abs() < 0.2 && r.air <= 0.0 {
            r.want = (r.lane.round() + if mv.1 < 0.0 { 1.0 } else { -1.0 }).clamp(0.0, (LANES - 1) as f32);
        }
        r.lane += (r.want - r.lane) * (1.0 - (-dt * 8.0).exp());
        if r.tumble > t {
            r.v = (r.v - dt * 20.0).max(0.0);
        } else if r.air <= 0.0 {
            // downhill: faster and faster; a tuck faster still, a
            // snowplough slower
            let pull = 9.0 + if mv.0 > 0.0 { 10.0 } else { 0.0 };
            r.v += pull * dt;
            if mv.0 < 0.0 {
                r.v -= 26.0 * dt;
            }
            r.v = r.v.clamp(4.0, 70.0) - r.v * 0.08 * dt;
        }
        r.x += r.v * dt;
        // in the air: spin (← →), grab (↓), and land square or tumble
        if r.air > 0.0 {
            if mv.0 != 0.0 {
                r.spin += mv.0 * 540.0 * dt;
            }
            r.grab = mv.1 > 0.0;
            if r.held && r.boost > 0.0 && r.vy < 0.0 {
                r.boost -= dt;
            } else {
                r.vy += 46.0 * dt;
            }
            r.air -= r.vy * dt;
            if r.air <= 0.0 {
                r.air = 0.0;
                r.vy = 0.0;
                let turns = (r.spin.abs() / 360.0).round();
                let off = (r.spin.abs() - turns * 360.0).abs();
                if off > 70.0 {
                    r.tumble = t + 1.3;
                    r.pop = Some(("wipeout!".into(), t + 1.5));
                    self.sounds.push((Sfx::Puff, 0.8));
                } else {
                    let mut pts = 0;
                    let mut what = vec![];
                    if turns >= 1.0 {
                        pts += 300 * turns as u32 * turns as u32;
                        what.push(format!("{}°", turns as u32 * 360));
                    }
                    if r.grab {
                        pts += 150;
                        what.push("grab".into());
                    }
                    if pts > 0 {
                        r.score += pts;
                        r.tricks += 1;
                        r.pop = Some((format!("{}! +{pts}", what.join(" + ")), t + 1.6));
                        self.sounds.push((Sfx::Chime, 0.5));
                    }
                    self.sounds.push((Sfx::Drop, 0.4));
                }
                r.spin = 0.0;
                r.grab = false;
            }
        }
        let lane = r.lane.round() as usize;
        for (i, &(x, l, k)) in things.iter().enumerate() {
            let near = (x - r.x).abs() < 2.0;
            match k {
                // a tree in this lane: over it, or into it
                0 if near && l == lane && r.air < 2.5 && !r.passed.contains(&i) => {
                    r.passed.push(i);
                    r.tumble = t + 1.2;
                    r.score = r.score.saturating_sub(50);
                    r.pop = Some(("oof — into the branches! −50".into(), t + 1.5));
                    self.sounds.push((Sfx::Puff, 0.8));
                }
                // a gate: through it, in its lane
                1 if r.x > x && !r.passed.contains(&i) => {
                    r.passed.push(i);
                    r.gates += 1;
                    if l == lane && r.tumble <= t {
                        r.flags += 1;
                        r.score += 100;
                        r.pop = Some(("through the flags +100".into(), t + 1.0));
                        self.sounds.push((Sfx::Chime, 0.3));
                    }
                }
                // a bump (all across): off it into the air
                2 if near && r.air <= 0.0 && r.v > 14.0 => {
                    r.vy = -(r.v * 0.45).min(26.0);
                    r.air = 0.01;
                    r.spin = 0.0;
                    self.sounds.push((Sfx::Pickup, 0.4));
                }
                // a snowflake, picked up in passing
                3 if near && l == lane && !r.passed.contains(&i) => {
                    r.passed.push(i);
                    r.flakes += 1;
                    r.score += 25;
                }
                _ => {}
            }
        }
        // the others, racing down
        for rv in r.rivals.iter_mut().filter(|rv| rv.done.is_none()) {
            rv.v = (rv.v + dt * 7.0).min(30.0 + (rv.name.len() as f32) * 2.0);
            rv.x += rv.v * dt;
            let ln = rv.lane.round() as usize;
            // a tree coming up in their lane: over a lane
            if things.iter().any(|&(x, l, k)| k == 0 && l == ln && x > rv.x && x - rv.x < 12.0) {
                rv.want = if ln == 0 { 1.0 } else if ln + 1 == LANES { (LANES - 2) as f32 } else if self.rng.chance(0.5) { (ln + 1) as f32 } else { (ln - 1) as f32 };
            }
            rv.lane += (rv.want - rv.lane) * (1.0 - (-dt * 6.0).exp());
            if things.iter().any(|&(x, l, k)| k == 1 && l == ln && (x - rv.x).abs() < rv.v * dt) {
                rv.score += 100;
            }
            if rv.air > 0.0 {
                rv.vy += 46.0 * dt;
                rv.air -= rv.vy * dt;
                rv.air = rv.air.max(0.0);
            } else if things.iter().any(|&(x, _, k)| k == 2 && (x - rv.x).abs() < 2.0) && rv.v > 14.0 {
                rv.vy = -(rv.v * 0.4).min(22.0);
                rv.air = 0.01;
                rv.score += 150;
            }
            if rv.x >= RUN_LEN {
                rv.done = Some(t);
            }
        }
        if r.x >= RUN_LEN {
            let secs = t - r.started;
            let bonus = ((150.0 - secs).max(0.0) * 8.0) as u32;
            r.score += bonus;
            let score = r.score;
            let mut place = 1 + r.rivals.iter().filter(|rv| rv.done.is_some_and(|d| d < t)).count();
            if self.race {
                place = 1 + self.results.iter().filter(|o| o.1 > score).count();
                self.finished = Some((score, secs));
            }
            let (flags, gates, tricks) = (r.flags, r.gates, r.tricks);
            self.last = Some((secs, flags, gates, tricks));
            self.runs += 1;
            self.best = Some(self.best.map_or(secs, |b| b.min(secs)));
            self.best_score = self.best_score.max(score);
            self.run = None;
            self.at = Where::Lodge;
            self.x = LIFT - 0.02;
            let place = match place {
                1 => "1st".to_string(),
                2 => "2nd".into(),
                3 => "3rd".into(),
                n => format!("{n}th"),
            };
            let msg = format!("{place}! {score} points · {secs:.1}s · {flags}/{gates} flags · {tricks} tricks · E at the lift: again");
            self.say(&msg, t, 7.0);
            self.sounds.push((Sfx::Fanfare, 0.6));
        }
    }

    // ------------------------------------------------------------ drawing

    pub fn draw(&self, f: &mut Frame, t: f32, look: &crate::scene::sprites::Look) {
        f.ox = 0;
        f.oy = 0;
        let (w, h) = (f.w as i32, f.h as i32);
        match self.at {
            Where::Shuttle { home, since } => self.draw_shuttle(f, t, t - since, home, look),
            Where::Carriage { home, .. } => self.draw_carriage(f, t, home, look),
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
            Where::Shuttle { .. } => " E: hurry along · Esc: home ",
            Where::Carriage { .. } => " ← → walk the carriage · E: talk, sit, the door · Esc: home ",
            Where::Lift { .. } => " E: skip ahead · Esc: home ",
            Where::Lodge => " ← → walk · E: the shop, the lift, the train · Esc: home ",
            Where::Ski => " → tuck · ← slow · ↑ ↓ lanes · Space jump · air: ← → spin, ↓ grab ",
            Where::Drive => " → faster · ← slower · ↑ ↓ lanes · Space honk · E home ",
        };
        f.text(1, 1, hint, pal::UI_DIM, Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
    }

    /// A word in a bubble over someone (column, the row their head's at).
    fn bubble(f: &mut Frame, who: &str, line: &str, cx: i32, top: i32) {
        let bg = Bg::Mix(Rgb::hex(0xf4efe6), 0.95);
        let w = line.chars().count().max(who.chars().count() + 2) as i32 + 4;
        let x0 = (cx - w / 2).clamp(0, (f.w as i32 - w).max(0));
        let y0 = (top - 3).max(1);
        for y in y0..y0 + 3 {
            for x in x0..x0 + w {
                f.put_g(x, y, Glyph::new(' ', Rgb::BLACK).bg(bg).lit(0.0));
            }
        }
        f.text(x0 + 2, y0 + 1, line, Rgb::hex(0x2a1a14), bg, 0.0);
        f.text(x0 + 2, y0, &format!(" {who} "), Rgb::hex(0x9a4a1a), bg, 0.0);
    }

    /// The shuttle: a little sleigh-bus pulled by a horse, from the village
    /// along the lane to the station (or back), the snow falling.
    fn draw_shuttle(&self, f: &mut Frame, t: f32, el: f32, home: bool, look: &crate::scene::sprites::Look) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let ground = ph * 3 / 4;
        let k = (el / SHUTTLE).clamp(0.0, 1.0);
        self.backdrop(f, el * 12.0, ground, 0.2);
        // the snowy lane, fences going by
        for x in 0..pw {
            for y in ground..ph {
                let c = if (y - ground) < 3 { Rgb::hex(0xc8d4ec) } else { Rgb::hex(0xe8eef8) * (0.8 + 0.12 * h01(hash2(x + (el * 30.0) as i32, y))) };
                f.px_set(x, y, c);
            }
            let fx = (x as f32 + el * 30.0) as i32;
            if fx.rem_euclid(16) < 2 {
                for y in ground - 8..ground {
                    f.px_set(x, y, Rgb::hex(0x5a3a24));
                }
            }
            if fx.rem_euclid(16) < 16 && ((ground - 6)..=(ground - 5)).contains(&(ground - 6)) {
                f.px_set(x, ground - 6, Rgb::hex(0x6a4428));
            }
        }
        // the station (or the village) getting nearer
        let (sx, label) = if home { ((pw as f32 * (1.1 - k * 0.45)) as i32, "THE VILLAGE") } else { ((pw as f32 * (1.1 - k * 0.45)) as i32, "STATION") };
        let (bw, bh) = (pw / 5, ph / 6);
        for y in ground - bh..ground {
            for x in sx - bw / 2..sx + bw / 2 {
                f.px_set(x, y, if (y - ground + bh) < 4 { Rgb::hex(0xe8eef8) } else { Rgb::hex(0x6a3a2a) });
            }
        }
        for wx in [sx - bw / 4, sx + bw / 4] {
            for y in ground - bh / 2..ground - bh / 4 {
                f.px_set(wx, y, Rgb::hex(0xffd27a));
                f.px_set(wx + 1, y, Rgb::hex(0xffd27a));
            }
            f.light_px(wx as f32, (ground - bh / 3) as f32, Rgb::hex(0xffb050) * 3.0);
        }
        let lx = sx / 1 - label.len() as i32 / 2;
        f.text(lx.max(0), (ground - bh - 3) / 2, label, Rgb::hex(0xffd36b), Bg::Solid(Rgb::hex(0x6a3a2a)), 0.0);
        // the shuttle: a red cabin on runners, the horse trotting ahead
        let cx = pw / 3;
        let bob = ((t * 8.0).sin() * 1.0).round() as i32;
        for y in ground - 16 + bob..ground - 3 + bob {
            for x in cx - 14..cx + 14 {
                let edge = x == cx - 14 || x == cx + 13 || y == ground - 16 + bob;
                f.px_set(x, y, if edge { Rgb::hex(0xffd36b) } else { Rgb::hex(0xb3202a) });
            }
        }
        for x in cx - 10..cx + 10 {
            if (x - cx).rem_euclid(7) < 5 {
                for y in ground - 13 + bob..ground - 8 + bob {
                    f.px_set(x, y, Rgb::hex(0xffd27a));
                }
            }
        }
        f.light_px(cx as f32, (ground - 10) as f32, Rgb::hex(0xffb050) * 4.0);
        for x in cx - 16..cx + 16 {
            f.px_set(x, ground - 2, Rgb::hex(0xc8a040));
        }
        // the horse
        let hx = cx + 26;
        let gait = ((t * 9.0) as i32) % 2;
        for y in ground - 12..ground - 6 {
            for x in hx - 6..hx + 6 {
                f.px_set(x, y, Rgb::hex(0x5a3a24));
            }
        }
        for (dx, dy) in [(6, -14), (7, -14), (8, -13), (6, -13), (7, -12), (8, -12), (9, -12)] {
            f.px_set(hx + dx, ground + dy, Rgb::hex(0x5a3a24));
        }
        for (i, lx) in [hx - 5, hx - 2, hx + 2, hx + 5].into_iter().enumerate() {
            let lift = if (i as i32 + gait) % 2 == 0 { 1 } else { 0 };
            for y in ground - 6..ground - lift {
                f.px_set(lx, y, Rgb::hex(0x3a2414));
            }
        }
        for x in cx + 14..hx - 6 {
            f.px_set(x, ground - 9, Rgb::hex(0xc8a040));
        }
        // the villager at the window (friends along beside them)
        person(f, look, crate::scene::sprites::Pose::Sit, 1.0, cx as f32 - 4.0, (ground - 6) as f32 / 2.0, 1.0);
        for (i, fw) in self.fellows.iter().filter(|fw| fw.at.at == 1 && fw.at.home == home).take(2).enumerate() {
            let x = cx as f32 + 3.0 + i as f32 * 7.0;
            person(f, &fw.look, crate::scene::sprites::Pose::Sit, 1.0, x, (ground - 6) as f32 / 2.0, 1.0);
            name_tag(f, &fw.name, x / 1.0, (ground - 18 + bob) / 2 - 2);
        }
        // snow
        for i in 0..80 {
            let x = (h01(hash2(i, 1)) * pw as f32 - el * 40.0).rem_euclid(pw as f32);
            let y = (h01(hash2(i, 2)) * ph as f32 + el * 18.0).rem_euclid(ph as f32);
            f.px_set(x as i32, y as i32, Rgb::hex(0xf4f8ff));
        }
        let bar = (k * (pw - 20) as f32) as i32;
        for x in 10..10 + bar {
            f.px_set(x, ph - 6, Rgb::hex(0xffd36b));
        }
    }

    /// Inside the carriage: seats down its length, folk in them, the
    /// conductor in the aisle, the windows full of the mountains going by,
    /// warm lamps swaying overhead.
    fn draw_carriage(&self, f: &mut Frame, t: f32, home: bool, look: &crate::scene::sprites::Look) {
        use crate::scene::sprites::{Look, Pose};
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let floor = ph * 4 / 5;
        let sway = ((t * 2.3).sin() * 1.0).round() as i32;
        // the walls and the windows' strip of the outside
        let (wy0, wy1) = (ph / 5, ph / 2);
        let scroll = t * 60.0 * if home { -1.0 } else { 1.0 };
        for y in 0..ph {
            for x in 0..pw {
                let c = if y < 6 {
                    Rgb::hex(0x3a2418)
                } else if y >= wy0 && y < wy1 && (x % (pw / 6).max(1)) > 6 {
                    // outside: sky, mountains, pines whipping by
                    let k = (y - wy0) as f32 / (wy1 - wy0) as f32;
                    let sx = x as f32 + scroll * 0.15;
                    let ridge = (noise1(sx * 0.02, self.seed) * (wy1 - wy0) as f32 * 0.6) as i32;
                    let tree = ((x as f32 + scroll).rem_euclid(23.0) as i32) < 7 && y > wy1 - 12 + ((x as f32 + scroll).rem_euclid(23.0) as i32 - 3).abs() * 2;
                    if tree {
                        Rgb::hex(0x1e4a2e)
                    } else if y - wy0 > (wy1 - wy0) / 2 - ridge / 3 {
                        Rgb::hex(0xdfe7f4) * (0.75 + 0.2 * k)
                    } else {
                        Rgb::hex(0x0e1838).lerp(Rgb::hex(0x3a4a7a), k)
                    }
                } else if y < floor {
                    let panel = if y > wy1 + 6 { Rgb::hex(0x6a2a2a) } else { Rgb::hex(0x8a5a32) };
                    panel * (0.82 + 0.15 * h01(hash2(x / 10, y / 40)))
                } else {
                    // the carpet down the aisle
                    let stripe = (y - floor) % 6 < 2;
                    if stripe { Rgb::hex(0x7a1a24) } else { Rgb::hex(0x5a1418) }
                };
                f.px_set(x, y + if y < floor { sway } else { 0 }, c);
            }
        }
        // window frames, lamps
        for k in 0..6 {
            let x0 = k * (pw / 6).max(1);
            for y in wy0 - 2..wy1 + 2 {
                for dx in 0..6 {
                    f.px_set(x0 + dx, y + sway, Rgb::hex(0x4a2a16));
                }
            }
            let lx = x0 + pw / 12;
            f.px_set(lx, 7 + sway, Rgb::hex(0xffd27a));
            f.light_px(lx as f32, 8.0, Rgb::hex(0xffb050) * 4.0);
        }
        // seats: one every 13% along (a back and a cushion)
        for s in 0..6 {
            let sx = ((0.14 + s as f32 * 0.13 + 0.03) * pw as f32) as i32;
            for y in floor - 20..floor - 2 {
                for x in sx + 4..sx + 8 {
                    f.px_set(x, y, Rgb::hex(0x2a4c8a) * 0.9);
                }
            }
            for y in floor - 8..floor - 4 {
                for x in sx - 6..sx + 8 {
                    f.px_set(x, y, Rgb::hex(0x2a4c8a));
                }
            }
        }
        let feet = (floor - 1) as f32 / 2.0;
        // the folk in their seats
        for p in &self.passengers {
            let l = Look { kid: p.kid, coat: p.coat, hat: Some(p.hat), hair: Rgb::hex(0x3a2a1a), scarf: None, skin: Rgb::hex(0xe8c39e), mittens: p.coat * 0.8, holding: None, lantern: false };
            person(f, &l, Pose::Sit, -1.0, (p.x + 0.03) * pw as f32, feet - 1.0, 2.0);
        }
        // the conductor (navy coat, peaked cap)
        let cl = Look { kid: false, coat: Rgb::hex(0x1a2a5a), hat: Some(Rgb::hex(0x1a1a2a)), hair: Rgb::hex(0x8a8a8a), scarf: Some(Rgb::hex(0xc8a040)), skin: Rgb::hex(0xe8c39e), mittens: Rgb::hex(0x1a1a2a), holding: None, lantern: false };
        let walking = t > self.conductor.2;
        let pose = if walking { Pose::Walk(((t * 5.0) as i32 % 2) as u8) } else { Pose::Wave };
        person(f, &cl, pose, self.conductor.1, self.conductor.0 * pw as f32, feet + 1.0, 2.0);
        // friends on the same train
        for fw in self.fellows.iter().filter(|fw| fw.at.at == 2 && fw.at.home == home) {
            let pose = if fw.at.sat { Pose::Sit } else if (fw.at.el * 3.0) as i32 % 3 == 0 { Pose::Stand } else { Pose::Walk(((t * 5.0) as i32 % 2) as u8) };
            let x = fw.at.x * pw as f32;
            person(f, &fw.look, pose, fw.at.dir, x, feet + if fw.at.sat { -1.0 } else { 2.0 }, 2.0);
            name_tag(f, &fw.name, x, (feet as i32) - 12);
        }
        // the villager
        let pose = if self.sat { Pose::Sit } else if (self.walk * 10.0) as i32 % 3 == 0 { Pose::Stand } else { Pose::Walk(((self.walk as i32) % 2) as u8) };
        person(f, look, pose, self.dir, self.x * pw as f32, feet + if self.sat { -1.0 } else { 2.0 }, 2.0);
        // the door at the far end, lit when it's time
        let dx = pw - 10;
        let open = self.ride >= 1.0;
        for y in floor - 36..floor {
            for x in dx..dx + 8 {
                f.px_set(x, y, if open { Rgb::hex(0xffd27a) } else { Rgb::hex(0x4a2a16) });
            }
        }
        if open {
            f.light_px((dx + 4) as f32, (floor - 18) as f32, Rgb::hex(0xffb050) * 5.0);
        }
        // the next stop, and how far along
        let to = if home { "next stop: the village" } else { "next stop: Pine Peak" };
        f.text(f.w as i32 / 2 - 10, 2, to, Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x1a1014), 0.9), 0.0);
        let bar = (self.ride * (pw - 20) as f32) as i32;
        for x in 10..10 + bar {
            f.px_set(x, ph - 4, Rgb::hex(0xffd36b));
        }
        if let Some((who, line, x, _)) = &self.talk {
            Self::bubble(f, who, line, (x * f.w as f32) as i32, (floor / 2) - 12);
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
        label(f, SHOP, " ❄ ski shop ");
        label(f, LIFT, " ⇗ chairlift ");
        // friends up here too (at the lodge, or on the lift)
        for fw in self.fellows.iter().filter(|fw| fw.at.at == 3 || fw.at.at == 4) {
            let (fx, lift) = if fw.at.at == 4 { (LIFT * f.w as f32 + 2.0, true) } else { (fw.at.x * f.w as f32, false) };
            let pose = if lift { crate::scene::sprites::Pose::Wave } else { crate::scene::sprites::Pose::Stand };
            person(f, &fw.look, pose, fw.at.dir, fx, (ground / 2) as f32 + 0.5, 2.0);
            let tag = if lift { format!("{} (on the lift)", fw.name) } else { fw.name.clone() };
            name_tag(f, &tag, fx, ground / 2 - 12);
        }
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
        use crate::scene::sprites::{Look, Pose};
        let Some(r) = &self.run else { return };
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        self.backdrop(f, r.x * 0.5, ph / 2, 0.0);
        // the slope: down to the right; the skier a third of the way along.
        // Lanes: further back is higher up the screen, a little smaller.
        let sx = pw / 3;
        let fall = 0.42;
        let surface = |px: i32, lane: f32| -> f32 {
            let along = r.x + (px - sx) as f32;
            ph as f32 * 0.5 + (px - sx) as f32 * fall + noise1(along * 0.02, self.seed) * 6.0 - lane * 9.0
        };
        for x in 0..pw {
            let top = surface(x, (LANES - 1) as f32 + 0.6) as i32;
            for y in top.max(0)..ph {
                let d = (y - top) as f32;
                let c = Rgb::hex(0xf4f8ff).lerp(Rgb::hex(0xb8c8e8), (d / 60.0).min(0.6));
                f.px_set(x, y, c);
            }
            // each lane's groomed track
            let along = (r.x + (x - sx) as f32) as i32;
            for l in 0..LANES {
                if along.rem_euclid(9) < 5 {
                    f.px_set(x, surface(x, l as f32) as i32 + 1, Rgb::hex(0xc8d4ec));
                }
            }
        }
        // what's on the slope (back lanes first)
        let mut order: Vec<&(f32, usize, u8)> = self.things.iter().collect();
        order.sort_by_key(|th| std::cmp::Reverse(th.1));
        for &&(x, lane, k) in &order {
            let px = sx + (x - r.x) as i32;
            if px < -20 || px > pw + 20 {
                continue;
            }
            let base = surface(px, if k == 2 { 0.0 } else { lane as f32 }) as i32;
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
                    for (dx, c) in [(-6, 0xc8202a), (6, 0x2a4c8a)] {
                        for y in base - 12..base {
                            f.px_set(px + dx, y, Rgb::hex(0x22242c));
                        }
                        for y in base - 12..base - 7 {
                            f.px_set(px + dx + 1, y, Rgb::hex(c));
                            f.px_set(px + dx + 2, y, Rgb::hex(c));
                        }
                    }
                }
                2 => {
                    // a ramp right across
                    for l in 0..LANES {
                        let b = surface(px, l as f32) as i32;
                        for dx in -6i32..7 {
                            let hgt = (6 - dx.abs()) / 2;
                            for y in b - hgt..b {
                                f.px_set(px + dx, y, Rgb::hex(0xf8fbff));
                            }
                        }
                    }
                }
                _ => {
                    if !r.passed.iter().any(|&p| self.things.get(p).is_some_and(|th| th.0 == x && th.1 == lane)) {
                        let tw = ((t * 6.0 + x).sin() * 0.5 + 0.5) * 0.4 + 0.8;
                        for (dx, dy) in [(0, -6), (-1, -5), (1, -5), (0, -4), (-2, -5), (2, -5), (0, -7), (0, -3)] {
                            f.px_set(px + dx, base + dy, Rgb::hex(0xbfe4ff) * tw);
                        }
                        f.light_px(px as f32, (base - 5) as f32, Rgb::hex(0x9fd8ff) * 1.5);
                    }
                }
            }
        }
        // the others racing down
        for rv in &r.rivals {
            let px = sx + (rv.x - r.x) as i32;
            if px < -20 || px > pw + 20 || rv.done.is_some() {
                continue;
            }
            let l = Look { kid: false, coat: rv.coat, hat: Some(rv.coat * 0.7), hair: Rgb::hex(0x3a2a1a), scarf: Some(Rgb::hex(0xf4f0ea)), skin: Rgb::hex(0xe8c39e), mittens: rv.coat * 0.7, holding: None, lantern: false };
            let feet = surface(px, rv.lane) - rv.air * 2.0;
            person(f, &l, if rv.air > 0.0 { Pose::Jump } else { Pose::Scoop }, 1.0, px as f32, feet / 2.0, 2.0);
            for dx in -7..9 {
                f.px_set(px + dx, feet as i32 + dx / 3, Rgb::hex(0x2a4c8a));
            }
            f.text(px / 1 - 2, ((feet / 2.0) as i32 - 9).max(2), rv.name, Rgb::hex(0xf4ecd8), Bg::Mix(Rgb::hex(0x1a1014), 0.6), 0.0);
        }
        // friends on the slope
        for fw in self.fellows.iter().filter(|fw| fw.at.at == 5) {
            let px = sx + (fw.at.x - r.x) as i32;
            if px < -20 || px > pw + 20 {
                continue;
            }
            let feet = surface(px, fw.at.lane) - fw.at.air * 2.0;
            let pose = if fw.at.sat { Pose::Lie } else if fw.at.air > 0.0 { Pose::Jump } else { Pose::Scoop };
            person(f, &fw.look, pose, fw.at.dir, px as f32, feet / 2.0, 2.0);
            for dx in -7..9 {
                f.px_set(px + dx, feet as i32 + dx / 3, fw.look.hat.unwrap_or(fw.look.coat));
            }
            name_tag(f, &fw.name, px as f32, ((feet / 2.0) as i32 - 10).max(2));
        }
        // the skier: tucked, in the air (spinning, grabbing), or tumbling
        let feet_px = surface(sx, r.lane) - r.air * 2.0;
        let spun = ((r.spin / 90.0).round() as i32).rem_euclid(4);
        let dir = if spun == 2 { -1.0 } else { 1.0 };
        let pose = if r.tumble > t {
            Pose::Lie
        } else if r.air > 0.0 {
            if r.grab { Pose::Scoop } else if spun % 2 == 1 { Pose::ArmsUp } else { Pose::Jump }
        } else {
            Pose::Scoop
        };
        person(f, look, pose, dir, sx as f32, feet_px / 2.0, 2.0);
        let fy = feet_px as i32;
        for dx in -7..9 {
            f.px_set(sx + dx, fy + dx / 3, Rgb::hex(0xc8202a));
        }
        if r.air <= 0.0 && r.v > 15.0 {
            for k in 0..6 {
                let a = h01(hash2(k, (t * 30.0) as i32));
                f.px_set(sx - 8 - (a * 8.0) as i32, fy - (a * 5.0) as i32, Rgb::hex(0xffffff));
            }
        }
        // the score, and what was just pulled off
        let secs = t - r.started;
        let line = format!(" {} pts · {:.0} km/h · {secs:.1}s · flags {}/{} · {:.0}% down ", r.score, r.v * 1.6, r.flags, r.gates, (r.x / RUN_LEN * 100.0).min(100.0));
        f.text(f.w as i32 - line.chars().count() as i32 - 2, 1, &line, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
        if let Some((s, until)) = &r.pop {
            if t < *until {
                let n = s.chars().count() as i32;
                f.text(f.w as i32 / 3 - n / 2, (fy / 2 - 10).max(3), s, Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x1a1014), 0.85), 0.0);
            }
        }
        // the standings
        let mut all: Vec<(String, f32)> = r.rivals.iter().map(|rv| (rv.name.to_string(), rv.x)).collect();
        all.extend(self.fellows.iter().filter(|fw| fw.at.at == 5).map(|fw| (fw.name.clone(), fw.at.x)));
        all.extend(self.results.iter().map(|(n, ..)| (n.clone(), RUN_LEN + 1.0)));
        all.push(("you".into(), r.x));
        all.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (i, (n, _)) in all.iter().enumerate() {
            let me = n == "you";
            f.text(2, 3 + i as i32, &format!("{}. {n}", i + 1), if me { Rgb::hex(0xffd36b) } else { pal::UI_TEXT }, Bg::Mix(pal::UI_PANEL, 0.6), 0.0);
        }
    }

    /// A quiet drive at night, seen from the side: the car along a snowy
    /// country road, its headlights on the road ahead, hills and pines and
    /// far-off windows going by at their own speeds, now and then a car the
    /// other way, lamps along the verge, the snow coming down.
    fn draw_drive(&self, f: &mut Frame, t: f32) {
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let road_y = ph * 3 / 4;
        let d = self.road;
        // the sky, the stars, the moon
        for y in 0..road_y {
            let k = y as f32 / road_y as f32;
            let c = Rgb::hex(0x060a1e).lerp(Rgb::hex(0x24305a), k);
            for x in 0..pw {
                f.px_set(x, y, c);
            }
        }
        for i in 0..70 {
            let (x, y) = ((hash2(i, 3) % pw.max(1) as u32) as i32, (hash2(i, 5) % (road_y as u32 / 2).max(1)) as i32);
            f.px_set(x, y, Rgb::hex(0xdfe7f4) * (0.4 + 0.5 * h01(hash2(i, 9))));
        }
        let (mx, my) = (pw * 4 / 5, road_y / 5);
        for y in -6..7 {
            for x in -6..7 {
                if x * x + y * y <= 36 {
                    f.px_set(mx + x, my + y, Rgb::hex(0xf4ecd0));
                }
            }
        }
        f.light_px(mx as f32, my as f32, Rgb::hex(0xbfd0ff) * 3.0);
        // layers: far hills, a ridge of pines with windows, the near verge
        for (layer, (base, amp, tone, speed)) in [(0.55, 24.0, 0x2a3458, 0.05), (0.7, 14.0, 0x1a2a3a, 0.2)].into_iter().enumerate() {
            for x in 0..pw {
                let u = (x as f32 + d * speed) * 0.01;
                let n = noise1(u, self.seed ^ layer as u32);
                let top = (road_y as f32 * base - n * amp) as i32;
                for y in top.max(0)..road_y {
                    let snow = y - top < 2;
                    f.px_set(x, y, if snow { Rgb::hex(0xc8d4ec) * 0.7 } else { Rgb::hex(tone) });
                }
                // pines along the nearer ridge, and a lit window now and then
                if layer == 1 {
                    let px = (x as f32 + d * speed) as i32;
                    if px.rem_euclid(11) == 0 {
                        for k in 0..10 {
                            for dx in -(k / 3)..=(k / 3) {
                                f.px_set(x + dx, top - 10 + k, Rgb::hex(0x0e2a1a));
                            }
                        }
                    }
                    if px.rem_euclid(97) == 0 {
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            f.px_set(x + dx, top + 3 + dy, Rgb::hex(0xffd27a));
                        }
                        f.light_px(x as f32, (top + 4) as f32, Rgb::hex(0xffb050) * 1.5);
                    }
                }
            }
        }
        // the road: two lanes, the verge, a dashed line down the middle
        for y in road_y..ph {
            for x in 0..pw {
                let c = if y < road_y + 2 {
                    Rgb::hex(0xe8eef8) * 0.85
                } else if y > ph - 3 {
                    Rgb::hex(0xe8eef8) * 0.75
                } else {
                    Rgb::hex(0x2a2c36) * (0.85 + 0.1 * h01(hash2(x / 4, y)))
                };
                f.px_set(x, y, c);
            }
        }
        let mid = (road_y + ph) / 2;
        for x in 0..pw {
            if ((x as f32 + d * 1.0) as i32).rem_euclid(24) < 10 {
                f.px_set(x, mid, Rgb::hex(0xe8d070) * 0.8);
            }
        }
        // lamps along the far verge, pools of light on the road
        for k in 0..4 {
            let lx = (k as f32 * pw as f32 / 3.0 - (d * 1.0) % (pw as f32 / 3.0) + pw as f32 / 3.0) as i32 - pw / 6;
            for y in road_y - 20..road_y + 1 {
                f.px_set(lx, y, Rgb::hex(0x22242c));
            }
            f.px_set(lx + 1, road_y - 20, Rgb::hex(0xffd27a));
            f.light_px((lx + 1) as f32, (road_y - 19) as f32, Rgb::hex(0xffb050) * 4.0);
            f.light_px((lx + 1) as f32, (road_y + 4) as f32, Rgb::hex(0xffb050) * 2.0);
        }
        // a car the other way now and then, headlights first
        let ox = (pw as f32 * 1.4 - ((d * 1.6 + self.seed as f32) % (pw as f32 * 2.2))) as i32;
        if ox > -40 && ox < pw + 40 {
            let oy = mid - 4;
            for y in oy - 6..oy {
                for x in ox - 12..ox + 12 {
                    f.px_set(x, y, Rgb::hex(0x3a5a8a));
                }
            }
            f.px_set(ox - 12, oy - 3, Rgb::hex(0xfff0c0));
            f.light_px((ox - 18) as f32, (oy - 3) as f32, Rgb::hex(0xfff0c0) * 4.0);
        }
        // the villager's car (red, snow on the roof), in its lane
        let cy = (ph - 6) as f32 - self.lane * ((ph - 6 - mid) as f32 - 2.0);
        let (cx, cy) = (pw / 3, cy as i32);
        let bob = ((t * 7.0).sin() * 0.6).round() as i32;
        let body = Rgb::hex(0xb3202a);
        let rows: [&str; 6] = ["      ##########        ", "     #==#==#==#==#       ", "  ######################  ", " ######################## ", " ######################## ", "   @@@@          @@@@     "];
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let c = match ch {
                    '#' => body * if dy == 0 { 1.1 } else { 1.0 },
                    '=' => Rgb::hex(0x9ac8f0) * 0.8,
                    '@' => if ((t * self.speed * 20.0) as usize + dx) % 2 == 0 { Rgb::hex(0x1a1a22) } else { Rgb::hex(0x3a3a44) },
                    _ => continue,
                };
                f.px_set(cx - 12 + dx as i32, cy - 6 + dy as i32 + bob, c);
            }
        }
        for dx in 6..16 {
            f.px_set(cx - 12 + dx, cy - 7 + bob, Rgb::hex(0xf4f8ff));
        }
        // the headlights' beam on the road ahead, the tail lights
        f.light_px((cx + 18) as f32, (cy - 2) as f32, Rgb::hex(0xfff0c0) * 5.0);
        for k in 1..8 {
            f.light_px((cx + 14 + k * 5) as f32, (cy - 1) as f32, Rgb::hex(0xfff0c0) * (2.0 - k as f32 * 0.2));
        }
        f.px_set(cx - 12, cy - 3 + bob, Rgb::hex(0xff3030));
        f.light_px((cx - 13) as f32, (cy - 3) as f32, Rgb::hex(0xff3030) * 1.5);
        // the snow, drifting past
        for i in 0..90 {
            let x = (h01(hash2(i, 1)) * pw as f32 - t * (20.0 + self.speed * 60.0) * (0.5 + h01(hash2(i, 4)))).rem_euclid(pw as f32);
            let y = (h01(hash2(i, 2)) * ph as f32 + t * 14.0).rem_euclid(ph as f32);
            f.px_set(x as i32, y as i32, Rgb::hex(0xf4f8ff) * 0.9);
        }
        let line = format!(" {:.0} km/h · {:.1} km ", self.speed * 80.0, self.km);
        f.text(f.w as i32 - line.chars().count() as i32 - 2, 1, &line, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
    }
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

}

/// A friend's name, over them (column, row).
fn name_tag(f: &mut Frame, name: &str, cx: f32, row: i32) {
    let s = format!(" {name} ");
    let n = s.chars().count() as i32;
    f.text(cx.round() as i32 - n / 2, row.max(1), &s, Rgb::hex(0xffe9a8), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
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
        let step = |tr: &mut Trip, t: &mut f32, mv: (f32, f32)| {
            tr.update(dt, *t, mv);
            *t += dt;
        };
        // the shuttle, then the train: a seat, and to the door when we're there
        while matches!(tr.at, Where::Shuttle { .. }) {
            step(&mut tr, &mut t, (0.0, 0.0));
        }
        assert!(matches!(tr.at, Where::Carriage { home: false, .. }));
        tr.x = 0.14 + 0.13 * 2.0;
        tr.interact(t);
        while tr.ride < 1.0 {
            step(&mut tr, &mut t, (0.0, 0.0));
        }
        for _ in 0..400 {
            step(&mut tr, &mut t, (1.0, 0.0));
        }
        tr.interact(t);
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
        // up, and down the mountain (tucked, jumping and spinning now and then)
        for i in 0..20_000 {
            let spin = if tr.run.as_ref().is_some_and(|r| r.air > 0.0) { -1.0 } else { 1.0 };
            step(&mut tr, &mut t, (spin, if i % 90 == 0 { -1.0 } else { 0.0 }));
            tr.jump(i % 40 < 10);
            if tr.at == Where::Lodge {
                break;
            }
        }
        assert_eq!(tr.at, Where::Lodge, "never got down");
        assert_eq!(tr.runs, 1);
        assert!(tr.best_score > 0, "no points at all");
        // the train home, and the shuttle back into the village
        tr.x = PLATFORM;
        tr.interact(t);
        assert!(matches!(tr.at, Where::Carriage { home: true, .. }));
        tr.interact(t);
        while tr.ride < 1.0 {
            step(&mut tr, &mut t, (0.0, 0.0));
        }
        for _ in 0..400 {
            step(&mut tr, &mut t, (1.0, 0.0));
        }
        tr.interact(t);
        for _ in 0..400 {
            step(&mut tr, &mut t, (0.0, 0.0));
        }
        assert_eq!(tr.done, Some(Back::Station));
    }

    #[test]
    fn a_drive_goes_somewhere_and_comes_home() {
        let mut tr = Trip::drive(0.0, 3);
        let mut t = 0.0;
        for _ in 0..600 {
            tr.update(1.0 / 30.0, t, (1.0, -1.0));
            t += 1.0 / 30.0;
        }
        assert!(tr.km > 0.05, "{}", tr.km);
        assert!(tr.lane > 0.9, "didn't change lanes");
        tr.interact(t);
        assert_eq!(tr.done, Some(Back::Home));
    }
}
