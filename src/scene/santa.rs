//! Santa's rounds: the sleigh comes down out of the sky and goes roof to
//! roof. At each house it hovers beside the roof while Santa climbs out,
//! wades across to the chimney, goes down ("ho ho ho!"), comes back up and
//! off they go to the next one. Each delivery leaves another present under
//! the big tree. On an ordinary December night that's one stop; on
//! Christmas Eve it's every house in the village, taking his time.

use super::events::Director;
use super::sleigh::{draw_santa, draw_team, SEAT};
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::Frame;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Flying in to hover beside the next roof.
    Fly,
    /// Santa climbing out and over to the chimney (0..1).
    Out(f32),
    /// Down the chimney until then.
    Down(f32),
    /// Back up and over to the sleigh (0..1).
    Back(f32),
    /// Off into the sky (all done).
    Away,
}

/// One stop: the house and where its chimney is (column, cell row).
#[derive(Clone, Copy, Debug)]
pub struct Stop {
    pub house: usize,
    pub chimney: (f32, f32),
}

pub struct Rounds {
    stops: Vec<Stop>,
    next: usize,
    /// The team's lead column and row (world).
    x: f32,
    y: f32,
    phase: Phase,
    rng: Rng,
    /// Deliveries made (for the scene: a present under the tree each).
    pub delivered: Vec<usize>,
    /// Santa's down the chimney of this house right now.
    pub inside: Option<usize>,
}

impl Rounds {
    /// Start a round from just off the view (`from`: world column) through
    /// these stops.
    pub fn new(stops: Vec<Stop>, from: f32, seed: u32) -> Self {
        let y = stops.first().map(|s| s.chimney.1 - 12.0).unwrap_or(4.0);
        Rounds { stops, next: 0, x: from, y: y.max(2.0), phase: Phase::Fly, rng: Rng::new(seed as u64 ^ 0x5a17a), delivered: vec![], inside: None }
    }

    pub fn done(&self) -> bool {
        self.phase == Phase::Away && self.y < -12.0
    }

    /// Where the team hovers for a stop: its lead column, so the seat ends
    /// up a few cells right of the chimney, and the row just above the roof.
    fn hover(s: &Stop) -> (f32, f32) {
        (s.chimney.0 + 5.0 - SEAT, s.chimney.1 - 2.0)
    }

    pub fn rebase(&mut self, s: f32) {
        if let Phase::Down(u) = &mut self.phase {
            *u -= s;
        }
    }

    pub fn update(&mut self, dt: f32, t: f32, d: &mut Director) {
        match self.phase {
            Phase::Fly => {
                let Some(stop) = self.stops.get(self.next).copied() else {
                    self.phase = Phase::Away;
                    return;
                };
                let (hx, hy) = Self::hover(&stop);
                let (dx, dy) = (hx - self.x, hy - self.y);
                let dist = (dx * dx + dy * dy * 4.0).sqrt();
                if dist < 0.6 {
                    self.x = hx;
                    self.y = hy;
                    self.phase = Phase::Out(0.0);
                } else {
                    // easing in, never faster than a brisk glide
                    let v = (dist * 1.2).clamp(4.0, 26.0);
                    self.x += dx / dist * v * dt;
                    self.y += dy / dist * v * dt * 0.5;
                }
            }
            Phase::Out(u) => {
                let u = u + dt / 2.2;
                self.phase = if u >= 1.0 {
                    let stop = self.stops[self.next];
                    self.inside = Some(stop.house);
                    let line = *self.rng.pick(&["ho ho ho!", "ho… ho… ho…", "shh — ho ho ho", "merry Christmas!", "down we go!"]);
                    d.say(stop.chimney.0.round() as i32, stop.chimney.1 as i32 - 2, line, t, 2.4, 6);
                    d.sound(Sfx::Magic, stop.chimney.0, 0.6);
                    Phase::Down(t + self.rng.range(3.5, 6.0))
                } else {
                    Phase::Out(u)
                };
            }
            Phase::Down(until) => {
                if t >= until {
                    let stop = self.stops[self.next];
                    self.inside = None;
                    self.delivered.push(stop.house);
                    d.burst(stop.chimney.0, stop.chimney.1 - 1.0, 14, &[Rgb::hex(0xffd36b), Rgb::WHITE, Rgb::hex(0xff6b6b)], 4.0, 2.0);
                    self.phase = Phase::Back(0.0);
                }
            }
            Phase::Back(u) => {
                let u = u + dt / 2.0;
                self.phase = if u >= 1.0 {
                    self.next += 1;
                    if self.next >= self.stops.len() {
                        d.sound(Sfx::Magic, self.x + SEAT, 0.8);
                        Phase::Away
                    } else {
                        Phase::Fly
                    }
                } else {
                    Phase::Back(u)
                };
            }
            Phase::Away => {
                self.x -= 24.0 * dt;
                self.y -= 6.0 * dt;
            }
        }
    }

    /// Where the lead reindeer is (column, row).
    pub fn team(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Where Santa is (column, row), if he's out of the sleigh.
    fn walker(&self) -> Option<(f32, f32)> {
        let stop = self.stops.get(self.next)?;
        let seat = (self.x + SEAT, self.y - 1.0);
        let chim = (stop.chimney.0, stop.chimney.1 - 1.0);
        let lerp = |u: f32| (seat.0 + (chim.0 - seat.0) * u, seat.1 + (chim.1 - seat.1) * u - (u * std::f32::consts::PI).sin() * 1.5);
        match self.phase {
            Phase::Out(u) => Some(lerp(u)),
            Phase::Back(u) => Some(lerp(1.0 - u)),
            _ => None,
        }
    }

    /// Drawn with the frame's camera offset (world coordinates).
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        let (x, y) = (self.x, self.y);
        let aboard = matches!(self.phase, Phase::Fly | Phase::Away);
        let bob = |x: f32| y + (x * 0.2 + t * 2.0).sin() * 0.25;
        draw_team(f, x, bob, t, glow, aboard);
        if let Some((sx, sy)) = self.walker() {
            draw_santa(f, sx.round() as i32, sy.round() as i32);
        }
        // a glow down the chimney while he's inside
        if let (Phase::Down(_), Some(stop)) = (self.phase, self.stops.get(self.next)) {
            let k = 0.6 + 0.4 * (t * 9.0).sin();
            f.light_cell(stop.chimney.0 as i32, stop.chimney.1 as i32, Rgb::hex(0xffd36b) * (3.0 * k * glow));
        }
    }
}

// ----------------------------------------------------------------------
// Santa stopping for a word with the village (the sleigh was clicked).

const OPEN: &[&str] = &[
    "ho ho ho! good evening, everyone!",
    "whoa, Dasher — whoa! … hello down there!",
    "well now! who's out on a night like this?",
    "ho ho! I thought I heard someone waving!",
    "evening, village! everyone keeping warm?",
    "a quick stop — Rudolph needs a breather!",
];

/// (a kid's line, Santa's answer).
const KIDS: &[(&str, &str)] = &[
    ("SANTA!! I've been good! …mostly!", "mostly is plenty — ho ho ho!"),
    ("can I pet Rudolph?", "he'd love that, but he's on duty tonight!"),
    ("did you get my letter?", "every word — and the drawing too!"),
    ("what do reindeer eat?", "carrots, mostly. and the odd cookie."),
    ("are you REAL?!", "as real as the snow on your hat!"),
    ("how do you fit down chimneys?", "a little Christmas magic — and I hold my breath!"),
    ("I want a sled for Christmas!", "a sled, eh? I'll make a note… ✎"),
    ("is it Christmas yet?!", "not yet — {days} more sleeps!"),
    ("can I come with you?", "ho ho — maybe when you're a little taller!"),
    ("do the elves get presents too?", "the best ones! they make their own."),
    ("which reindeer is the fastest?", "don't tell the others… it's Comet."),
    ("I left you cookies last year!", "the chocolate ones! I remember!"),
];

/// (a grown-up's line, Santa's answer).
const ADULTS: &[(&str, &str)] = &[
    ("evening, Santa! busy night?", "they're all busy this time of year!"),
    ("cold enough up there?", "brisk! the reindeer like it this way."),
    ("out for a practice run?", "every night in December — ho ho!"),
    ("give Mrs. Claus our best!", "I will! she sends her cookies."),
    ("need a cocoa for the road?", "never say no to cocoa!"),
    ("the kids won't sleep a wink now!", "ho ho — then I'd better be quick!"),
    ("mind the power lines!", "Rudolph's nose sees them a mile off!"),
    ("how's the workshop?", "noisy, sticky and very, very merry!"),
    ("still on the nice list, am I?", "ho ho — I'll check it twice!"),
    ("lovely night for flying!", "the best — not a cloud between here and the pole!"),
];

const BYE: &[&str] = &["on, Dasher! on, Dancer! ho ho ho!", "merry Christmas, all!", "off we go — be good, now!", "up, up and away! ho ho ho!", "see you on Christmas Eve!"];
const WAVE: &[&str] = &["bye, Santa!!", "merry Christmas!", "safe travels!", "bye Rudolph!", "wooo!"];

pub const HO_HO: &[&str] = &["HO HO HO!", "ho ho ho!", "HO! HO! HO!", "merry Christmas! ho ho ho!", "ho ho — hello down there!"];

/// A little exchange with Santa: (opening, the villager's line, Santa's
/// answer, Santa's goodbye, a wave back). `days`: until Christmas.
pub fn chat(rng: &mut Rng, kid: bool, days: i64) -> [String; 5] {
    let (ask, ans) = *rng.pick(if kid { KIDS } else { ADULTS });
    let ans = if days <= 0 && ans.contains("{days}") { "it is! merry Christmas!".to_string() } else { ans.replace("{days}", &days.to_string()) };
    [rng.pick(OPEN).to_string(), ask.to_string(), ans, rng.pick(BYE).to_string(), rng.pick(WAVE).to_string()]
}
