//! Trips out of the village. The ski trip: the shuttle sleigh from the
//! village up to the station, the train (a carriage to walk along, a
//! conductor, folk in the seats, the mountains going by), the ski lodge at
//! the top of the line (skis from the shop, the chairlift), a run down the
//! mountain (lanes, jumps and tricks, flags and snowflakes for points,
//! others racing down too), and the same way home. And a drive: the car
//! parked by the villager's home, out along a quiet snowy road at night.
//! And the moon: jumped high enough, on Christmas spirit, right off the top
//! of the sky, the villager's up there (a little moon, round in a few
//! hundred steps, light enough to bound about on, the Earth hanging over
//! it), with anyone else who's got there; ↓ comes back down.
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
    /// On the moon (since when: the first moments, flying up to it).
    Moon { since: f32 },
}

/// What a trip asks of the village (when it's over).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Back {
    /// At the station's shuttle stop.
    Station,
    /// At home (out of the car).
    Home,
    /// Down from the sky (the moon), where the villager left.
    Sky,
}

/// Round the little moon (columns): walking on from here comes back to the
/// start.
pub const MOON_W: f32 = 360.0;
/// Flying up to the moon (s), and how high a bound goes up there.
const LIFT_OFF: f32 = 2.2;
const MOON_G: f32 = 9.0;

/// On the moon: where round it (columns), how high off the dust and how
/// fast up, the jump held, the steps (for footprints), footprints and
/// flags planted (where round).
struct MoonWalk {
    x: f32,
    air: f32,
    vy: f32,
    held: bool,
    walk: f32,
    prints: Vec<f32>,
    flags: Vec<f32>,
    /// The time last frame.
    now: f32,
}

/// How long the train takes (s); sitting down, it goes by quicker.
const RIDE: f32 = 36.0;
/// How long the shuttle takes (s).
const SHUTTLE: f32 = 5.5;

/// Someone in the carriage: their name and look, where along (0..1),
/// and, once the train's in, when they get up and go for the door.
#[derive(Clone)]
struct Passenger {
    name: &'static str,
    coat: Rgb,
    hat: Option<Rgb>,
    hair: Rgb,
    skin: Rgb,
    scarf: Option<Rgb>,
    x: f32,
    kid: bool,
    /// Up and off at this time (the train in), and walking.
    off: Option<f32>,
    dir: f32,
    walk: f32,
}

impl Passenger {
    fn look(&self) -> crate::scene::sprites::Look {
        crate::scene::sprites::Look { kid: self.kid, coat: self.coat, hat: self.hat, hair: self.hair, scarf: self.scarf, skin: self.skin, mittens: self.hat.unwrap_or(self.coat * 0.8), holding: None, lantern: false }
    }
}

/// Someone about at the lodge: off the train to the shop for skis, then
/// up the lift (or along to the platform for the train home).
#[derive(Clone)]
struct Folk {
    p: Passenger,
    /// Where they're making for (0..1 across), and what for: 0 the shop,
    /// 1 the lift, 2 gone (up the lift, or off on the train).
    to: f32,
    stage: u8,
    skis: bool,
    /// Stopped (at the shop's window, a chat) until then.
    wait: f32,
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
    /// A race: at the top since when (waiting for everyone), and when it
    /// starts (scene time; set by whoever asked, once everyone's up).
    pub gate: Option<f32>,
    pub go_at: Option<f32>,
    /// The countdown last said (3, 2, 1).
    counted: i32,
    /// Skis rented at the shop.
    pub skis: bool,
    /// The villager at the lodge or in the carriage: column (0..1), facing.
    pub x: f32,
    pub dir: f32,
    walk: f32,
    /// Sat in a seat on the train.
    sat: bool,
    passengers: Vec<Passenger>,
    /// The train in: when it stopped; how fast it's going (0..1) and how
    /// far the view out of the window has gone by.
    stopped: Option<f32>,
    pace: f32,
    scroll: f32,
    /// Folk about at the lodge.
    folk: Vec<Folk>,
    /// Down at the bottom of a run: since when (everyone's waited for,
    /// then it's back up the lift); the lift's taking the villager back up
    /// to the lodge (not to the top of a run).
    down: Option<f32>,
    lift_home: bool,
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
    pub seed: u32,
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
    /// On the moon (see `Where::Moon`).
    moon: Option<MoonWalk>,
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

    /// Up to the moon (`x`: where round it the villager comes down).
    pub fn moon(t: f32, seed: u32, x: f32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Moon { since: t };
        tr.moon = Some(MoonWalk { x: x.rem_euclid(MOON_W), air: 0.0, vy: 0.0, held: false, walk: 0.0, prints: vec![], flags: vec![], now: t });
        tr.say("up, up… and up! is that… the moon?!", t, 2.4);
        tr.sounds.push((Sfx::Magic, 1.0));
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
        let passengers = Self::folk_for(&mut rng);
        Trip {
            at: Where::Lodge,
            fellows: vec![],
            race: false,
            finished: None,
            results: vec![],
            gate: None,
            go_at: None,
            counted: 0,
            skis: false,
            x: 0.05,
            dir: 1.0,
            walk: 0.0,
            sat: false,
            passengers,
            stopped: None,
            pace: 1.0,
            scroll: 0.0,
            folk: vec![],
            down: None,
            lift_home: false,
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
            moon: None,
        }
    }

    /// Whoever's on the train this time: a different lot each trip (names,
    /// coats, hats or none, hair, skin, kids), in seats here and there.
    fn folk_for(rng: &mut Rng) -> Vec<Passenger> {
        let mut seats: Vec<usize> = (0..6).collect();
        let n = rng.int(3, 7) as usize;
        let mut out = vec![];
        for _ in 0..n {
            if seats.is_empty() {
                break;
            }
            let s = seats.remove(rng.int(0, seats.len() as i32) as usize);
            let kid = rng.chance(0.3);
            const MORE_COATS: [u32; 10] = [0xb3202a, 0x1f6e3a, 0x2a4c8a, 0x6a3a8a, 0xc8862a, 0x3a6a6a, 0xd8588a, 0x5a5e68, 0x8a4a2a, 0xd8d8e0];
            const HATS: [u32; 7] = [0xd8283a, 0x2fae5c, 0x4a8dff, 0xffc23a, 0xff7ae0, 0xf0f0f4, 0x7a3aa8];
            const HAIRS: [u32; 5] = [0x6a4a2a, 0x2a1c14, 0xc89a5a, 0xa8461e, 0xb0b0b8];
            const SKINS: [u32; 4] = [0xf0d0b0, 0xe8c39e, 0xc89a72, 0x8a5a3a];
            let hat = rng.chance(0.75).then(|| Rgb::hex(*rng.pick(&HATS)));
            out.push(Passenger {
                name: crate::scene::dialogue::name(rng, kid),
                coat: Rgb::hex(*rng.pick(&MORE_COATS)),
                hat,
                hair: Rgb::hex(*rng.pick(&HAIRS)),
                skin: Rgb::hex(*rng.pick(&SKINS)),
                scarf: rng.chance(0.5).then(|| hat.unwrap_or(Rgb::hex(0xd8283a)) * 0.85),
                x: 0.14 + s as f32 * 0.13,
                kid,
                off: None,
                dir: -1.0,
                walk: 0.0,
            });
        }
        out
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
            // (a race: at the top, waiting for the others)
            Where::Lift { .. } if self.gate.is_some() => TripAt { at: 7, ..base },
            Where::Lift { since } => TripAt { at: 4, el: t - since, ..base },
            Where::Ski => match &self.run {
                Some(r) => TripAt { at: 5, x: r.x, lane: r.lane, air: r.air, score: r.score, sat: r.tumble > t, el: t - r.started, dir: if (((r.spin / 90.0).round() as i32).rem_euclid(4)) == 2 { -1.0 } else { 1.0 }, ..base },
                None => TripAt { at: 3, ..base },
            },
            Where::Drive => TripAt { at: 6, ..base },
            Where::Moon { since } => match &self.moon {
                Some(m) => TripAt { at: 8, x: m.x, air: m.air, sat: m.walk.fract() > 0.5 && m.air <= 0.0, el: t - since, ..base },
                None => TripAt { at: 8, ..base },
            },
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

    /// A race: at the top of the chairlift, waiting till everyone's up and
    /// whoever asked says go (a countdown to it); late, straight off; and
    /// if nobody says, off anyway after a while.
    fn at_the_gate(&mut self, t: f32) {
        let gate = match self.gate {
            Some(g) => g,
            None => {
                self.gate = Some(t);
                self.say("at the top · waiting for everyone to get here…", t, 6.0);
                t
            }
        };
        match self.go_at {
            Some(go) if t >= go => {
                self.gate = None;
                self.go_at = None;
                self.counted = 0;
                self.sounds.push((Sfx::Chime, 0.9));
                self.start_run(t);
                self.say("GO!", t, 1.2);
            }
            Some(go) => {
                let n = (go - t).ceil() as i32;
                if n != self.counted && n <= 3 {
                    self.counted = n;
                    self.say(&format!("{n}…"), t, 1.0);
                    self.sounds.push((Sfx::Click, 0.8));
                }
            }
            None if t - gate > 40.0 => self.go(t),
            None => {}
        }
    }

    /// A race: off at `at` (scene time; a countdown to it at the top).
    pub fn go(&mut self, at: f32) {
        if self.race && self.go_at.is_none() && self.run.is_none() {
            self.go_at = Some(at);
        }
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
            "lodge" => {
                self.at = Where::Lodge;
                self.x = SHOP - 0.02;
                let mut rng = Rng::new(9);
                for (k, mut p) in Self::folk_for(&mut rng).into_iter().take(3).enumerate() {
                    p.x = PLATFORM + 0.08 + k as f32 * 0.17;
                    p.dir = 1.0;
                    p.walk = k as f32;
                    self.folk.push(Folk { p, to: LIFT, stage: 1, skis: k > 0, wait: 0.0 });
                }
            }
            "arrived" => {
                self.board(false, t);
                self.x = 0.7;
                for k in 0..400 {
                    self.update(1.0 / 30.0, t + k as f32 / 30.0, (0.0, 0.0));
                }
                self.ride = 0.9999;
                for k in 0..120 {
                    self.update(1.0 / 30.0, t + 40.0 + k as f32 / 30.0, (0.0, 0.0));
                }
            }
            "bottom" => {
                self.skis = true;
                self.start_run(t);
                if let Some(r) = &mut self.run {
                    r.x = RUN_LEN - 4.0;
                    for (k, rv) in r.rivals.iter_mut().enumerate() {
                        rv.x = RUN_LEN - 30.0 + k as f32 * 20.0;
                    }
                }
                for k in 0..60 {
                    self.ski_step(1.0 / 30.0, t + k as f32 / 30.0, (0.0, 0.0));
                }
            }
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
        self.gate = self.gate.map(|g| g - s);
        self.go_at = self.go_at.map(|g| g - s);
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
            Where::Lift { .. } if self.lift_home => self.lift_arrive(t),
            Where::Lift { .. } => self.start_run(t),
            Where::Lodge => {
                let near = |a: f32, x: f32| (a - x).abs() < 0.06;
                if near(SHOP, self.x) {
                    if self.skis {
                        self.skis = false;
                        self.say("\"back in one piece! thanks — come again\" (the skis are returned)", t, 3.5);
                        self.sounds.push((Sfx::Drop, 0.6));
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
            // at the bottom, waiting: back up now
            Where::Ski if self.down.is_some() => self.lift_back(t),
            Where::Ski => {}
            Where::Drive => self.done = Some(Back::Home),
            Where::Moon { since } => {
                // a flag planted where the villager stands (five at most)
                if t - since < LIFT_OFF {
                    return;
                }
                if let Some(m) = &mut self.moon {
                    if m.air > 0.0 {
                        return;
                    }
                    if m.flags.iter().any(|&fx| moon_gap(fx, m.x).abs() < 6.0) {
                        self.say("there's a flag just here already", t, 2.0);
                        return;
                    }
                    if m.flags.len() >= 5 {
                        m.flags.remove(0);
                    }
                    m.flags.push(m.x);
                    self.say("a flag on the moon! (a candy-cane one)", t, 2.5);
                    self.sounds.push((Sfx::Present, 0.7));
                }
            }
        }
    }

    /// Everyone down (or E): the chairlift back up to the lodge.
    fn lift_back(&mut self, t: f32) {
        self.down = None;
        self.run = None;
        self.lift_home = true;
        self.at = Where::Lift { since: t };
        self.say("everyone's down — back up the chairlift…", t, 3.0);
        self.sounds.push((Sfx::Click, 0.6));
    }

    /// Off the lift at the top, by the lodge.
    fn lift_arrive(&mut self, t: f32) {
        self.lift_home = false;
        self.at = Where::Lodge;
        self.x = LIFT - 0.02;
        self.dir = -1.0;
        self.say("back up at Pine Peak · the lift: another run · the shop: return the skis · the platform: home", t, 5.0);
    }

    /// What E would do just now, for the [E] over the villager's head.
    pub fn prompt(&self) -> Option<&'static str> {
        match self.at {
            Where::Shuttle { home: false, .. } => Some("hurry along"),
            Where::Shuttle { home: true, .. } => Some("off at the station"),
            Where::Carriage { home, .. } => {
                let x = self.x;
                if self.sat {
                    Some("up again")
                } else if self.ride >= 1.0 && x > 0.86 {
                    Some(if home { "off the train" } else { "off at Pine Peak" })
                } else if (self.conductor.0 - x).abs() < 0.05 {
                    Some("a word with the conductor")
                } else if self.passengers.iter().any(|p| p.off.is_none() && (p.x - x).abs() < 0.04) {
                    Some("say hello")
                } else {
                    let seat = (x - 0.14) / 0.13;
                    ((seat - seat.round()).abs() < 0.25 && (0.0..6.0).contains(&seat.round()) && !self.passengers.iter().any(|p| p.off.is_none() && (p.x - (0.14 + seat.round() * 0.13)).abs() < 0.02)).then_some("sit down")
                }
            }
            Where::Lodge => {
                let near = |a: f32| (a - self.x).abs() < 0.06;
                if near(SHOP) {
                    Some(if self.skis { "return the skis" } else { "rent skis" })
                } else if near(LIFT) {
                    Some(if self.skis { "up the chairlift" } else { "(skis first: the shop)" })
                } else if near(PLATFORM) {
                    Some("the train home")
                } else {
                    None
                }
            }
            Where::Lift { .. } => Some("skip ahead"),
            Where::Ski if self.down.is_some() => Some("back up the lift now"),
            Where::Ski => None,
            Where::Drive => Some("home"),
            Where::Moon { since } if self.t_ok(since) => Some("plant a flag"),
            Where::Moon { .. } => None,
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
        self.stopped = None;
        self.pace = 0.0;
        // a different lot aboard each time
        self.passengers = Self::folk_for(&mut self.rng);
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
                // the folk off the train, ahead along the platform
                let off: Vec<Passenger> = self.passengers.iter().filter(|p| p.off.is_some()).cloned().collect();
                for (k, mut p) in off.into_iter().enumerate() {
                    p.x = (PLATFORM + 0.05 + k as f32 * 0.04).min(0.3);
                    p.dir = 1.0;
                    self.folk.push(Folk { p, to: SHOP + self.rng.range(-0.04, 0.04), stage: 0, skis: false, wait: 0.0 });
                }
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
        if let Some(p) = self.passengers.iter().find(|p| p.off.is_none() && (p.x - x).abs() < 0.04) {
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
        if let (Where::Moon { .. }, Some(m)) = (self.at, &mut self.moon) {
            // (light up here: a bound, held for longer still)
            if held && m.air <= 0.0 {
                m.vy = 13.0;
                m.air = 0.01;
                self.sounds.push((Sfx::Pickup, 0.35));
            }
            m.held = held;
            return;
        }
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
            Where::Carriage { home, .. } => {
                let pace = if self.sat { 3.5 } else { 1.0 };
                let was = self.ride;
                self.ride = (self.ride + dt * pace / RIDE).min(1.0);
                // pulling away, slowing in, stopped
                let want = if self.ride >= 1.0 { 0.0 } else if self.ride > 0.94 { 0.3 } else { 1.0 };
                self.pace += (want - self.pace) * (1.0 - (-dt * 1.5).exp());
                self.scroll += self.pace * pace.min(2.0) * 60.0 * dt;
                if was < 1.0 && self.ride >= 1.0 {
                    let stop = if home { "the village! everybody off — mind the step" } else { "Pine Peak! everybody off — mind the step" };
                    self.say(&format!("{stop} (the door's at the far end: →, then E)"), t, 5.0);
                    self.talk = Some(("conductor".into(), stop.to_string(), self.conductor.0, t + 3.5));
                    self.conductor.2 = t + 3.5;
                    self.sounds.push((Sfx::TrainWhistle, 0.8));
                    self.sat = false;
                    self.stopped = Some(t);
                    // everyone up and off, one after another
                    let n = self.passengers.len();
                    for (k, p) in self.passengers.iter_mut().enumerate() {
                        p.off = Some(t + 1.0 + k as f32 * 0.9 + (n as f32 - k as f32) * 0.1);
                    }
                }
                // folk making for the door, and stepping off
                for p in self.passengers.iter_mut() {
                    if p.off.is_some_and(|o| t > o) && p.x < 0.98 {
                        p.dir = 1.0;
                        p.x += 0.11 * dt;
                        p.walk += dt * 9.0;
                    }
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
                    if self.lift_home {
                        self.lift_arrive(t);
                    } else if self.race {
                        self.at_the_gate(t);
                    } else {
                        self.start_run(t);
                    }
                }
            }
            Where::Lodge => {
                if mv.0 != 0.0 {
                    self.dir = mv.0.signum();
                    self.x = (self.x + mv.0 * 0.09 * dt).clamp(0.04, 0.95);
                    self.walk += dt * 9.0;
                }
                self.update_folk(dt, t);
            }
            Where::Ski => self.ski_step(dt, t, mv),
            Where::Moon { since } => self.moon_step(dt, t, since, mv),
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

    /// The folk at the lodge: to the shop for skis, a moment at its
    /// window, then over to the lift and up it; now and then someone new
    /// along from the platform.
    fn update_folk(&mut self, dt: f32, t: f32) {
        if self.folk.len() < 4 && self.rng.chance(dt * 0.06) {
            let mut p = Self::folk_for(&mut self.rng).remove(0);
            p.x = PLATFORM;
            p.dir = 1.0;
            self.folk.push(Folk { p, to: SHOP, stage: 0, skis: false, wait: 0.0 });
        }
        for f in self.folk.iter_mut() {
            if t < f.wait {
                continue;
            }
            let d = f.to - f.p.x;
            if d.abs() > 0.01 {
                f.p.dir = d.signum();
                f.p.x += d.signum() * 0.07 * dt;
                f.p.walk += dt * 9.0;
                continue;
            }
            match f.stage {
                0 => {
                    f.skis = true;
                    f.stage = 1;
                    f.wait = t + 2.5;
                    f.to = LIFT + 0.02;
                }
                _ => f.stage = 2,
            }
        }
        self.folk.retain(|f| f.stage < 2);
    }

    fn ski_step(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        // down at the bottom: waiting for the others, then back up
        if let Some(since) = self.down {
            let race = self.race;
            let (fellows_down, results) = (self.fellows.iter().all(|fw| fw.at.at != 5), self.results.len());
            if let Some(r) = &mut self.run {
                // gliding to a stop past the line
                r.v = (r.v - dt * 30.0).max(0.0);
                r.x = (r.x + r.v * dt).min(RUN_LEN + 40.0);
                for (k, rv) in r.rivals.iter_mut().enumerate() {
                    let stop = RUN_LEN + 22.0 + k as f32 * 7.0;
                    if rv.x < stop {
                        rv.v = (rv.v + dt * 5.0).min(30.0);
                        rv.x = (rv.x + rv.v * dt).min(stop);
                        if rv.x >= RUN_LEN && rv.done.is_none() {
                            rv.done = Some(t);
                        }
                    }
                }
                let all = r.rivals.iter().all(|rv| rv.done.is_some()) && (!race || fellows_down || results > 0);
                if (all && t - since > 3.0) || t - since > 30.0 {
                    self.lift_back(t);
                }
            }
            let _ = mv;
            return;
        }
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
            // (the run's kept: everyone glides in at the bottom, and waits)
            self.down = Some(t);
            let place = match place {
                1 => "1st".to_string(),
                2 => "2nd".into(),
                3 => "3rd".into(),
                n => format!("{n}th"),
            };
            let msg = format!("{place}! {score} points · {secs:.1}s · {flags}/{gates} flags · {tricks} tricks · waiting for everyone to finish…");
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
            Where::Lift { since } => self.draw_lift(f, t, (t - since).min(7.0), look),
            Where::Ski => self.draw_ski(f, t, look),
            Where::Drive => self.draw_drive(f, t),
            Where::Moon { since } => self.draw_moon(f, t, t - since, look),
        }
        let _ = (w, h);
    }

    /// Where the villager's head is (cells), for the [E] over it.
    fn head_at(&self, f: &Frame) -> (i32, i32) {
        let (w, h) = (f.w as f32, f.h as f32);
        // (the feet: as each part draws the villager; a head about 11 rows
        // above them, at the size they're drawn)
        match self.at {
            Where::Shuttle { .. } => ((w / 3.0) as i32 - 4, (h * 0.75 - 9.0) as i32),
            Where::Carriage { .. } => ((self.x * w) as i32, (h * 0.8 - 12.0) as i32),
            Where::Lodge => ((self.x * w) as i32, (h * 0.8 - 12.0) as i32),
            Where::Lift { .. } => ((w / 2.0) as i32, (h / 3.0) as i32),
            Where::Ski => ((w / 3.0) as i32, (h * 0.5 - 12.0) as i32),
            Where::Drive => ((w / 2.0) as i32, (h * 0.6) as i32),
            Where::Moon { .. } => ((w / 2.0) as i32, (moon_surface(0.0, f.ph as f32) / 2.0 - self.moon.as_ref().map(|m| m.air).unwrap_or(0.0) - 9.0) as i32),
        }
    }

    /// Over the trip, kept clear of the widgets (`avoid`: their rects, x,
    /// y, w, h) and above the taskbar: the keys along the top, the caption
    /// along the bottom, the [E] over the villager, the score and the
    /// standings on the slope.
    pub fn draw_ui(&self, f: &mut Frame, t: f32, avoid: &[(i32, i32, i32, i32)]) {
        let (w, h) = (f.w as i32, f.h as i32);
        // what's been put where so far (the widgets, then each of these)
        let mut taken: Vec<(i32, i32, i32, i32)> = avoid.to_vec();
        // somewhere free (the top first; a side; down the screen), taken
        let mut spot = |n: i32, rows: i32, prefer_right: bool| -> (i32, i32) {
            let xs = if prefer_right { [w - n - 2, (w - n) / 2, 1] } else { [1, (w - n) / 2, w - n - 2] };
            let free = |x: i32, y: i32, taken: &[(i32, i32, i32, i32)]| x >= 0 && taken.iter().all(|&(ax, ay, aw, ah)| x >= ax + aw || x + n <= ax || y >= ay + ah || y + rows <= ay);
            let mut at = (xs[1].max(0), (h - 6 - rows).max(1));
            'find: for y in (1..(h - 6 - rows).max(2)).step_by(2) {
                for &x in &xs {
                    if free(x, y, &taken) {
                        at = (x, y);
                        break 'find;
                    }
                }
            }
            taken.push((at.0, at.1, n, rows));
            at
        };
        let hint = match self.at {
            Where::Shuttle { .. } => " E: hurry along · Esc: home ",
            Where::Carriage { .. } => " ← → walk the carriage · E: talk, sit, the door · Esc: home ",
            Where::Lift { .. } => " E: skip ahead · Esc: home ",
            Where::Lodge => " ← → walk · E: the shop, the lift, the train · Esc: home ",
            Where::Ski if self.down.is_some() => " waiting for everyone at the bottom · E: back up the lift now ",
            Where::Ski => " → tuck · ← slow · ↑ ↓ lanes · Space jump · air: ← → spin, ↓ grab ",
            Where::Drive => " → faster · ← slower · ↑ ↓ lanes · Space honk · E home ",
            Where::Moon { .. } => " ← → walk round the moon · Space: bound (so light!) · E: a flag · ↓ or Esc: back down home ",
        };
        let n = hint.chars().count() as i32;
        let (hx, hy) = spot(n, 1, false);
        f.text(hx, hy, hint, pal::UI_DIM, Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
        let _ = (hx, hy);
        // on the slope: the score line, and the standings
        if let (Where::Ski, Some(r)) = (self.at, &self.run) {
            let secs = if self.down.is_some() { self.last.map(|l| l.0).unwrap_or(0.0) } else { t - r.started };
            let line = format!(" {} pts · {:.0} km/h · {secs:.1}s · flags {}/{} · {:.0}% down ", r.score, r.v * 1.6, r.flags, r.gates, (r.x / RUN_LEN * 100.0).min(100.0));
            let n = line.chars().count() as i32;
            let (lx, ly) = spot(n, 1, true);
            f.text(lx, ly, &line, Rgb::hex(0xffd36b), Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
            let mut all: Vec<(String, f32)> = r.rivals.iter().map(|rv| (rv.name.to_string(), if rv.done.is_some() { RUN_LEN + 100.0 - rv.done.unwrap_or(0.0) * 0.001 } else { rv.x })).collect();
            all.extend(self.fellows.iter().filter(|fw| fw.at.at == 5).map(|fw| (fw.name.clone(), fw.at.x)));
            all.extend(self.results.iter().map(|(n, ..)| (n.clone(), RUN_LEN + 1.0)));
            all.push(("you".into(), if self.down.is_some() { RUN_LEN + 50.0 } else { r.x }));
            all.sort_by(|a, b| b.1.total_cmp(&a.1));
            let wide = all.iter().map(|(n, _)| n.chars().count() as i32 + 4).max().unwrap_or(8);
            let rows = all.len() as i32;
            let (sx, sy) = spot(wide, rows, false);
            for (i, (name, _)) in all.iter().enumerate() {
                let me = name == "you";
                f.text(sx, sy + i as i32, &format!("{}. {name}", i + 1), if me { Rgb::hex(0xffd36b) } else { pal::UI_TEXT }, Bg::Mix(pal::UI_PANEL, 0.6), 0.0);
            }
        }
        // the [E] over the villager, and what it does
        if let Some(what) = self.prompt() {
            let (x, y) = self.head_at(f);
            let y = y.clamp(2, h - 4);
            crate::scene::player::Player::draw_prompt(f, x, y, 'E', t);
            let label = format!(" {what} ");
            let n = label.chars().count() as i32;
            f.text((x - n / 2).clamp(0, (w - n).max(0)), y - 1, &label, Rgb::hex(0xffe9a8), Bg::Mix(Rgb::hex(0x1a1a24), 0.7), 0.0);
        }
        // the caption, along the bottom
        if let Some((s, _)) = &self.caption {
            let bg = Bg::Mix(pal::UI_PANEL, 0.85);
            let n = s.chars().count() as i32 + 4;
            let x = (w - n) / 2;
            f.text(x.max(0), h - 3, &format!("  {s}  "), Rgb::hex(0xf4ecd8), bg, 0.0);
        }
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
        // (rocking along; still, once in)
        let sway = ((t * 2.3).sin() * self.pace).round() as i32;
        // the walls and the windows' strip of the outside
        let (wy0, wy1) = (ph / 5, ph / 2);
        let scroll = self.scroll * if home { -1.0 } else { 1.0 };
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
        // the folk in their seats (or, the train in, off along the aisle
        // to the door)
        for p in &self.passengers {
            let l: Look = p.look();
            match p.off {
                Some(o) if self.stopped.is_some() && t > o => {
                    if p.x < 0.96 {
                        let pose = Pose::Walk(((p.walk as i32) % 2) as u8);
                        person(f, &l, pose, p.dir, p.x * pw as f32, feet + 2.0, 2.0);
                    }
                }
                _ => person(f, &l, Pose::Sit, -1.0, (p.x + 0.03) * pw as f32, feet - 1.0, 2.0),
            }
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
        let to = match (home, self.stopped.is_some()) {
            (true, true) => "the village · everybody off",
            (false, true) => "Pine Peak · everybody off",
            (true, false) => "next stop: the village",
            (false, false) => "next stop: Pine Peak",
        };
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
        // Pine Peak station: the platform, the station house (a clock in
        // the gable, the name board, lit windows, a bench, a lamp)
        let px0 = (pw as f32 * (PLATFORM - 0.1)).max(0.0) as i32;
        let px1 = px0 + pw / 5;
        for x in px0..px1 {
            for y in ground - 4..ground {
                let c = if y == ground - 4 { Rgb::hex(0xffd36b) * 0.8 } else { Rgb::hex(0x6a6a78) * (0.85 + 0.1 * h01(hash2(x / 6, y))) };
                f.px_set(x, y, c);
            }
            // the track along in front
            f.px_set(x, ground + 2, Rgb::hex(0x3a3a44));
            if x % 6 == 0 {
                f.px_set(x, ground + 3, Rgb::hex(0x5a3a24));
            }
        }
        let hx = (pw as f32 * (PLATFORM - 0.02)) as i32;
        let (hw, hh) = ((pw / 9).max(16), (ph / 6).max(14));
        let (hx0, hy0) = (hx - hw / 2, ground - 4 - hh);
        for y in hy0..ground - 4 {
            for x in hx0..hx0 + hw {
                let plank = (x - hx0) % 4 == 0;
                f.px_set(x, y, if plank { Rgb::hex(0x4a2a1a) } else { Rgb::hex(0x7a4a2a) * (0.9 + 0.08 * h01(hash2(x, y / 3))) });
            }
        }
        // the roof, snow on it, icicles under the eaves
        let rh = hw / 2;
        for k in 0..rh + 3 {
            for x in hx - k - 2..hx + k + 2 {
                let snow = k < 3 || x <= hx - k || x >= hx + k + 1;
                f.px_set(x, hy0 - rh + k, if snow { Rgb::hex(0xf4f8ff) } else { Rgb::hex(0x5a2a22) });
            }
        }
        for x in (hx0 - 2..hx0 + hw + 2).step_by(3) {
            let n = 1 + (hash2(x, 7) % 3) as i32;
            for y in 0..n {
                f.px_set(x, hy0 + 1 + y, Rgb::hex(0xdff0ff));
            }
        }
        // the clock in the gable
        let (ccx, ccy) = (hx, hy0 - rh / 2 + 1);
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 10 {
                    f.px_set(ccx + dx, ccy + dy, if dx * dx + dy * dy >= 8 { Rgb::hex(0xc8a040) } else { Rgb::hex(0xf4ecd8) });
                }
            }
        }
        let mins = (t / 60.0) as i32;
        f.px_set(ccx, ccy - 1, Rgb::hex(0x2a1a14));
        f.px_set(ccx + if mins % 2 == 0 { 1 } else { -1 }, ccy, Rgb::hex(0x2a1a14));
        f.light_px(ccx as f32, ccy as f32, Rgb::hex(0xffd27a) * 1.5);
        // windows and the door
        for wxk in [hx0 + hw / 5, hx0 + hw - hw / 5 - 4] {
            for y in hy0 + hh / 4..hy0 + hh / 4 + hh / 3 {
                for x in wxk..wxk + 4 {
                    let bar = x == wxk + 2 || y == hy0 + hh / 4 + hh / 6;
                    f.px_set(x, y, if bar { Rgb::hex(0x4a2a1a) } else { Rgb::hex(0xffd27a) });
                }
            }
            f.light_px((wxk + 2) as f32, (hy0 + hh / 3) as f32, Rgb::hex(0xffb050) * 3.0);
        }
        for y in ground - 4 - hh / 2..ground - 4 {
            for x in hx - 2..hx + 3 {
                f.px_set(x, y, Rgb::hex(0x3a1a10));
            }
        }
        // a bench (snow on it) and a lamp
        let bx = hx0 + hw + 3;
        for x in bx..bx + 8 {
            f.px_set(x, ground - 7, Rgb::hex(0xf4f8ff));
            f.px_set(x, ground - 6, Rgb::hex(0x5a3a24));
        }
        for y in ground - 6..ground - 4 {
            f.px_set(bx, y, Rgb::hex(0x3a2414));
            f.px_set(bx + 7, y, Rgb::hex(0x3a2414));
        }
        let lmp = hx0 - 4;
        for y in ground - 4 - hh..ground - 4 {
            f.px_set(lmp, y, Rgb::hex(0x22242c));
        }
        f.px_set(lmp, ground - 5 - hh, Rgb::hex(0xffd27a));
        f.px_set(lmp - 1, ground - 5 - hh, Rgb::hex(0xffd27a));
        f.light_px(lmp as f32, (ground - 5 - hh) as f32, Rgb::hex(0xffb050) * 5.0);
        // the name board under the eaves
        f.text(hx / 1 - 5, (hy0 / 2).max(1), " PINE PEAK ", Rgb::hex(0xffd36b), Bg::Solid(Rgb::hex(0x2a1a14)), 0.0);
        // the ski shop: a chalet with a striped awning, skis in the window
        // and racked outside, a wreath on the door, lights along the eaves,
        // smoke from the chimney
        let sx = (pw as f32 * SHOP) as i32;
        let (bw, bh) = (pw / 6, ph / 4);
        let (sx0, sy0) = (sx - bw / 2, ground - bh);
        for y in sy0..ground {
            for x in sx0..sx0 + bw {
                let log = (y - sy0) % 5;
                let c = match log {
                    0 => Rgb::hex(0x3a2214),
                    1 => Rgb::hex(0x9a6a3a),
                    _ => Rgb::hex(0x7a4a28) * (0.9 + 0.1 * h01(hash2(x / 5, y / 5))),
                };
                f.px_set(x, y, c);
            }
        }
        let roof = bw / 2 + 6;
        for k in 0..roof {
            for x in sx - k - 4..sx + k + 4 {
                let snow = k < 4 || x <= sx - k - 2 || x >= sx + k + 2;
                f.px_set(x, sy0 - roof + k, if snow { Rgb::hex(0xf4f8ff) } else { Rgb::hex(0x8a2a22) * (0.9 + 0.1 * ((x + k) % 3) as f32) });
            }
        }
        // the chimney, smoke curling up
        let cx0 = sx + bw / 4;
        for y in sy0 - roof - 2..sy0 - roof / 2 {
            for x in cx0..cx0 + 4 {
                f.px_set(x, y, Rgb::hex(0x6a3a2a));
            }
        }
        for i in 0..6 {
            let a = (t * 0.6 + i as f32 * 0.4).fract();
            let (x, y) = (cx0 as f32 + 2.0 + (a * 6.0 + i as f32).sin() * 2.0 + a * 4.0, (sy0 - roof - 3) as f32 - a * 14.0);
            f.px_blend(x as i32, y as i32, Rgb::hex(0xc8ccd8), 0.5 * (1.0 - a));
        }
        // lights along the eaves
        for (i, x) in (sx - roof - 2..sx + roof + 2).step_by(4).enumerate() {
            let dy = (roof - (x - sx).abs()).max(0);
            let y = sy0 - dy + 1;
            let c = [0xff4a4a, 0x4aff7a, 0x4a9aff, 0xffd34a][i % 4];
            let tw = 0.7 + 0.3 * ((t * 3.0 + i as f32).sin() * 0.5 + 0.5);
            f.px_set(x, y, Rgb::hex(c) * tw);
            f.light_px(x as f32, y as f32, Rgb::hex(c) * (1.2 * tw));
        }
        // the awning (red and white) over the big window
        let (wx0, wx1) = (sx0 + 3, sx + 2);
        for x in wx0 - 1..wx1 + 2 {
            for y in sy0 + 3..sy0 + 6 {
                f.px_set(x, y, if (x / 3) % 2 == 0 { Rgb::hex(0xd8283a) } else { Rgb::hex(0xf4f0ea) });
            }
        }
        for y in sy0 + 7..sy0 + 7 + bh / 2 {
            for x in wx0..wx1 {
                f.px_set(x, y, Rgb::hex(0xffd27a) * 0.95);
            }
        }
        // skis on show in the window
        for (k, c) in [0xd8283a, 0x2a4c8a, 0x2fae5c, 0xffc23a].into_iter().enumerate() {
            let x = wx0 + 2 + k as i32 * 3;
            if x + 1 < wx1 {
                for y in sy0 + 9..sy0 + 6 + bh / 2 {
                    f.px_set(x, y, Rgb::hex(c));
                }
            }
        }
        f.light_px(((wx0 + wx1) / 2) as f32, (sy0 + 7 + bh / 4) as f32, Rgb::hex(0xffb050) * 6.0);
        // the door, a wreath on it
        let dx0 = sx + bw / 4 - 2;
        for y in ground - bh / 2..ground {
            for x in dx0..dx0 + 6 {
                f.px_set(x, y, Rgb::hex(0x4a2414));
            }
        }
        for (ddx, ddy) in [(2, 0), (3, 0), (1, 1), (4, 1), (1, 2), (4, 2), (2, 3), (3, 3)] {
            f.px_set(dx0 + ddx, ground - bh / 2 + 3 + ddy, Rgb::hex(0x1f6e3a));
        }
        f.px_set(dx0 + 3, ground - bh / 2 + 6, Rgb::hex(0xd8283a));
        // skis racked outside, a snowboard
        let rx = sx0 + bw + 2;
        for (k, c) in [0xd8283a, 0x2fae5c, 0x4a8dff, 0xff7ae0].into_iter().enumerate() {
            let x = rx + k as i32 * 2;
            for y in ground - bh / 2..ground {
                f.px_set(x, y, Rgb::hex(c));
            }
            f.px_set(x, ground - bh / 2 - 1, Rgb::hex(c) * 1.2);
        }
        for y in ground - bh / 2 - 2..ground {
            f.px_set(rx + 10, y, Rgb::hex(0x222a44));
            f.px_set(rx + 11, y, Rgb::hex(0x3a4a7a));
        }
        // a lantern by the door
        f.px_set(dx0 - 2, ground - bh / 2, Rgb::hex(0xffd27a));
        f.light_px((dx0 - 2) as f32, (ground - bh / 2) as f32, Rgb::hex(0xffb050) * 4.0);
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
        label(f, PLATFORM, " ⇄ trains: the village ");
        label(f, SHOP, " ❄ SKI & SNOWBOARD · rentals ");
        label(f, LIFT, " ⇗ chairlift ");
        // folk about the lodge (skis over the shoulder, once they've some)
        for fo in &self.folk {
            let x = fo.p.x * f.w as f32;
            let pose = if t < fo.wait { crate::scene::sprites::Pose::Stand } else { crate::scene::sprites::Pose::Walk(((fo.p.walk as i32) % 2) as u8) };
            person(f, &fo.p.look(), pose, fo.p.dir, x, (ground / 2) as f32 + 0.5, 2.0);
            if fo.skis {
                let (px, py) = ((x * f.pw as f32 / f.w as f32) as i32, ground - 14);
                for k in 0..12 {
                    f.px_set(px - 3 + k / 2, py + k - 6, Rgb::hex(0xd8283a));
                    f.px_set(px - 2 + k / 2, py + k - 6, Rgb::hex(0x2a4c8a));
                }
            }
        }
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
        // the finish: a banner across the slope, flags either side
        let fx = sx + (RUN_LEN - r.x) as i32;
        if fx > -30 && fx < pw + 30 {
            let (b0, b1) = (surface(fx, 0.0) as i32, surface(fx, (LANES - 1) as f32 + 0.6) as i32);
            for y in b1 - 26..b0 {
                f.px_set(fx - 1, y, Rgb::hex(0x22242c));
                f.px_set(fx + 1, y, Rgb::hex(0x22242c));
            }
            for y in b1 - 26..b1 - 20 {
                for x in fx - 14..fx + 15 {
                    let check = ((x - fx + 14) / 3 + (y - b1 + 26) / 3) % 2 == 0;
                    f.px_set(x, y, if check { Rgb::hex(0x14141c) } else { Rgb::hex(0xf4f4f8) });
                }
            }
            f.text(fx - 3, (b1 - 34) / 2, " FINISH ", Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x1a1014), 0.8), 0.0);
        }
        // the others racing down (and those down, standing about at the
        // bottom, skis off)
        for rv in &r.rivals {
            let px = sx + (rv.x - r.x) as i32;
            if px < -20 || px > pw + 20 {
                continue;
            }
            let l = Look { kid: false, coat: rv.coat, hat: Some(rv.coat * 0.7), hair: Rgb::hex(0x3a2a1a), scarf: Some(Rgb::hex(0xf4f0ea)), skin: Rgb::hex(0xe8c39e), mittens: rv.coat * 0.7, holding: None, lantern: false };
            let feet = surface(px, rv.lane) - rv.air * 2.0;
            if rv.done.is_some() && rv.x >= RUN_LEN + 20.0 {
                let pose = if ((t + rv.x) * 0.7) as i32 % 3 == 0 { Pose::Wave } else { Pose::Stand };
                person(f, &l, pose, -1.0, px as f32, feet / 2.0, 2.0);
                f.text(px - 2, ((feet / 2.0) as i32 - 9).max(2), rv.name, Rgb::hex(0xf4ecd8), Bg::Mix(Rgb::hex(0x1a1014), 0.6), 0.0);
                continue;
            }
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
        let pose = if self.down.is_some() && r.v < 1.0 {
            Pose::Stand
        } else if r.tumble > t {
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
        // what was just pulled off, over the skier
        if let Some((s, until)) = &r.pop {
            if t < *until {
                let n = s.chars().count() as i32;
                f.text(f.w as i32 / 3 - n / 2, (fy / 2 - 10).max(3), s, Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x1a1014), 0.85), 0.0);
            }
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

/// How far round the moon from `a` to `b`, the short way (columns).
fn moon_gap(a: f32, b: f32) -> f32 {
    (b - a + MOON_W / 2.0).rem_euclid(MOON_W) - MOON_W / 2.0
}

/// The moon's dust under screen column `dx` from the middle (pixel row, a
/// screen `ph` pixels tall): a little round moon, its edge falling away
/// either side.
fn moon_surface(dx: f32, ph: f32) -> f32 {
    ph * 0.7 + dx * dx * 0.0016
}

impl Trip {
    fn t_ok(&self, since: f32) -> bool {
        self.moon.as_ref().is_some_and(|m| m.now - since >= LIFT_OFF)
    }

    /// On the moon, each frame: walking round, bounding (low, slow, a held
    /// jump floating longer), footprints; ↓ back home.
    fn moon_step(&mut self, dt: f32, t: f32, since: f32, mv: (f32, f32)) {
        let Some(m) = &mut self.moon else { return };
        m.now = t;
        if t - since < LIFT_OFF {
            return;
        }
        if mv.1 > 0.5 && m.air <= 0.0 {
            self.done = Some(Back::Sky);
            self.say("back down to the village… wheee!", t, 2.0);
            self.sounds.push((Sfx::Magic, 0.7));
            return;
        }
        if mv.0 != 0.0 {
            self.dir = mv.0.signum();
            m.x = (m.x + mv.0 * 10.0 * dt).rem_euclid(MOON_W);
            if m.air <= 0.0 {
                let was = m.walk;
                m.walk += dt * 4.0;
                // a footprint each step, in the dust
                if was.floor() != m.walk.floor() {
                    m.prints.push(m.x);
                    if m.prints.len() > 160 {
                        m.prints.remove(0);
                    }
                }
            }
        }
        if m.air > 0.0 || m.vy > 0.0 {
            let g = if m.held && m.vy > 0.0 { MOON_G * 0.45 } else { MOON_G };
            m.vy -= g * dt;
            m.air += m.vy * dt;
            if m.air <= 0.0 {
                m.air = 0.0;
                m.vy = 0.0;
                m.prints.push(m.x);
                self.sounds.push((Sfx::Drop, 0.25));
            }
        }
    }

    /// The moon: the black sky full of stars, the Earth hanging in it, the
    /// dust (craters, footprints, flags) curving away, whoever else is up
    /// here, and the villager in the middle; at first, flying up to it.
    fn draw_moon(&self, f: &mut Frame, t: f32, el: f32, look: &crate::scene::sprites::Look) {
        use crate::scene::sprites::Pose;
        let Some(m) = &self.moon else { return };
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        let up = (el / LIFT_OFF).clamp(0.0, 1.0);
        // the sky: black, the stars (streaming by while flying up)
        for y in 0..ph {
            for x in 0..pw {
                f.px_set(x, y, Rgb::hex(0x02030a));
            }
        }
        let rush = (1.0 - up) * 220.0;
        for i in 0..(pw * ph / 90).clamp(60, 900) {
            let sx = (h01(hash2(i, 17)) * pw as f32 + m.x * 0.15).rem_euclid(pw as f32);
            let sy = (h01(hash2(i, 29)) * ph as f32 + el * rush).rem_euclid(ph as f32);
            let tw = 0.5 + 0.5 * (t * (1.0 + h01(hash2(i, 3)) * 2.0) + i as f32).sin();
            let c = Rgb::hex(0xdfe7f4) * (0.25 + 0.6 * h01(hash2(i, 41)) * (0.6 + 0.4 * tw));
            f.px_set(sx as i32, sy as i32, c);
            if rush > 1.0 {
                f.px_set(sx as i32, sy as i32 - 1, c * 0.5);
            }
        }
        // the Earth, far below and away now: blue sea, green and white
        // land, cloud, a glint where the village is
        // (up in the middle of the sky: the corners are the widgets')
        let er = (ph as f32 * 0.1 * (1.0 + (1.0 - up) * 2.0)).max(4.0);
        let (ex, ey) = (pw as f32 * 0.6, ph as f32 * (0.3 + (1.0 - up) * 0.9));
        for dy in -(er as i32)..=er as i32 {
            for dx in -(er as i32)..=er as i32 {
                let (u, v) = (dx as f32 / er, dy as f32 / er);
                let d2 = u * u + v * v;
                if d2 > 1.0 {
                    continue;
                }
                let spin = t * 0.02;
                let land = noise1(u * 2.2 + spin * 6.0, 7) * noise1(v * 2.0 + 3.0, 11);
                let cloud = noise1(u * 3.0 - spin * 9.0 + v, 23);
                let pole = v < -0.72 || v > 0.8;
                let mut c = if pole { Rgb::hex(0xf2f6ff) } else if land > 0.32 { Rgb::hex(0x3a8a4a) } else { Rgb::hex(0x1f5aa8) };
                if cloud > 0.62 {
                    c = c.lerp(Rgb::hex(0xf4f6fa), 0.7);
                }
                // (the sun from the left: the right side in shadow)
                let lit = (0.35 + 0.65 * (1.0 - ((u + 0.3).max(0.0) * 1.1).min(1.0))).max(0.15) * (1.0 - d2 * 0.25);
                f.px_set((ex + dx as f32) as i32, (ey + dy as f32) as i32, c * lit);
            }
        }
        if up >= 1.0 && (t * 2.0).sin() > 0.0 {
            f.light_px(ex - er * 0.35, ey - er * 0.55, Rgb::hex(0xffd36b) * 1.2);
        }
        // the dust: grey, a little bluish at the far edge, craters and
        // rocks round the moon (the same for everyone up here)
        let cam = m.x;
        let mid = pw as f32 / 2.0;
        let rise = (1.0 - up) * ph as f32 * 0.6;
        let surf = |x: f32| moon_surface(x - mid, ph as f32) + rise;
        for x in 0..pw {
            let wx = (cam + (x as f32 - mid)).rem_euclid(MOON_W);
            let top = surf(x as f32);
            for y in (top as i32).max(0)..ph {
                let depth = (y as f32 - top) / (ph as f32 - top).max(1.0);
                let n = h01(hash2((wx * 1.0) as i32, y / 2));
                let mut c = Rgb::hex(0xa8a8b4) * (0.78 + 0.12 * n - depth * 0.25);
                // craters: rings in the dust, darker in, a lit rim
                for k in 0..14 {
                    let cx = h01(hash2(k, self.seed as i32 ^ 0x3c)) * MOON_W;
                    let r = 4.0 + 10.0 * h01(hash2(k, 77));
                    let dxw = moon_gap(cx, wx);
                    let dyw = (y as f32 - top - r * 0.45) * 2.4;
                    let d = (dxw * dxw + dyw * dyw).sqrt();
                    if d < r {
                        c = c * if d > r * 0.8 { 1.15 } else { 0.72 };
                    }
                }
                f.px_set(x, y, c);
            }
            // (the edge against the sky: a little brighter)
            f.px_set(x, top as i32, Rgb::hex(0xd8d8e4));
        }
        // footprints and flags
        let screen = |wx: f32| mid + moon_gap(cam, wx);
        for &p in &m.prints {
            let sx = screen(p);
            if (0.0..pw as f32).contains(&sx) {
                f.px_set(sx as i32, surf(sx) as i32 + 1, Rgb::hex(0x5a5a68));
            }
        }
        for &fl in &m.flags {
            let sx = screen(fl) as i32;
            if !(0..pw).contains(&sx) {
                continue;
            }
            let base = surf(sx as f32) as i32;
            for y in 0..14 {
                f.px_set(sx, base - y, if (y / 2) % 2 == 0 { Rgb::hex(0xd8283a) } else { Rgb::hex(0xf4f0ea) });
            }
            let wave = ((t * 3.0 + fl).sin() * 1.0) as i32;
            for dy in 0..4 {
                for dx in 1..7 {
                    f.px_set(sx + dx, base - 13 + dy + if dx > 3 { wave } else { 0 }, if (dx + dy) % 2 == 0 { Rgb::hex(0x2fae5c) } else { Rgb::hex(0xffd36b) });
                }
            }
        }
        // whoever else is up here, where they are round the moon
        for fw in self.fellows.iter().filter(|fw| fw.at.at == 8) {
            let sx = screen(fw.at.x);
            if !(-10.0..pw as f32 + 10.0).contains(&sx) {
                continue;
            }
            let feet = surf(sx) / 2.0 - fw.at.air;
            let pose = if fw.at.air > 0.2 { Pose::Jump } else if fw.at.sat { Pose::Walk(1) } else { Pose::Stand };
            person(f, &fw.look, pose, fw.at.dir, sx, feet, 1.6);
            name_tag(f, &fw.name, sx, (feet - 11.0) as i32);
        }
        // the villager, flying up into place, then about on the dust
        let feet = if up < 1.0 { surf(mid) / 2.0 + (1.0 - up) * ph as f32 * 0.5 } else { surf(mid) / 2.0 - m.air };
        let pose = if up < 1.0 || m.air > 0.0 { Pose::Jump } else if m.walk.fract() > 0.5 { Pose::Walk(1) } else { Pose::Stand };
        person(f, look, pose, self.dir, mid, feet, 1.6);
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
        // the train in: it stops, and the others get up and off
        for _ in 0..400 {
            step(&mut tr, &mut t, (1.0, 0.0));
        }
        assert!(tr.stopped.is_some() && tr.pace < 0.05, "the train never stopped");
        assert!(tr.passengers.iter().all(|p| p.off.is_some() && p.x > 0.9), "folk still in their seats");
        assert_eq!(tr.prompt(), Some("off at Pine Peak"));
        tr.interact(t);
        assert_eq!(tr.at, Where::Lodge);
        assert!(!tr.folk.is_empty(), "nobody off the train at the lodge");
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
        // up, and down the mountain (tucked, jumping and spinning now and
        // then); everyone waited for at the bottom, then back up the lift
        let mut waited = false;
        for i in 0..20_000 {
            waited |= tr.down.is_some();
            let spin = if tr.run.as_ref().is_some_and(|r| r.air > 0.0) { -1.0 } else { 1.0 };
            step(&mut tr, &mut t, (spin, if i % 90 == 0 { -1.0 } else { 0.0 }));
            tr.jump(i % 40 < 10);
            if tr.at == Where::Lodge {
                break;
            }
        }
        assert_eq!(tr.at, Where::Lodge, "never got down");
        assert!(waited, "straight back to the top without waiting at the bottom");
        assert_eq!(tr.runs, 1);
        assert!(tr.best_score > 0, "no points at all");
        // the skis back to the shop
        tr.x = SHOP;
        assert_eq!(tr.prompt(), Some("return the skis"));
        tr.interact(t);
        assert!(!tr.skis);
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
