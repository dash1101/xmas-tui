//! The player: a little bundled-up villager walked around with WASD / the arrow
//! keys. They crunch through the snow, leave footprints, and can say hello
//! to people, pet and feed the animals, pick berries, knock on doors and
//! poke at things (E / Enter), or ask someone to walk along (F). They jump
//! (Space: held, higher, up to four times as high), leap up onto houses,
//! trees and widgets, walk along the tops of things (W heads up to the top),
//! and lie down in the snow to rest or make a snow angel (Z).

use super::events::Director;
use super::ledges::Ledge;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Bg, Frame, Glyph};

const BOOTS: Rgb = Rgb::hex(0x2a2a34);

/// The villager's look (Settings → You): coat, hat (0: none), hair, skin,
/// scarf (0: none).
static WEAR: std::sync::RwLock<[u32; 5]> = std::sync::RwLock::new([0xe0a030, 0x2fae5c, 0x3a2a1a, 0xe8c39e, 0xd8283a]);

/// Dress the villager (from the config's `wear`).
pub fn set_wear(w: &crate::config::FriendCfg) {
    let (coat, hat, hair, skin, scarf) = w.wear_colours();
    if let Ok(mut l) = WEAR.write() {
        *l = [coat, hat.unwrap_or(0), hair, skin, scarf.unwrap_or(0)];
    }
}

fn wear() -> [u32; 5] {
    WEAR.read().map(|l| *l).unwrap_or([0xe0a030, 0x2fae5c, 0x3a2a1a, 0xe8c39e, 0xd8283a])
}

#[allow(non_snake_case)]
fn COAT() -> Rgb {
    Rgb::hex(wear()[0])
}
#[allow(non_snake_case)]
fn HAT() -> Option<Rgb> {
    let h = wear()[1];
    (h != 0).then(|| Rgb::hex(h))
}
#[allow(non_snake_case)]
fn HAIR() -> Rgb {
    Rgb::hex(wear()[2])
}
#[allow(non_snake_case)]
fn SKIN() -> Rgb {
    Rgb::hex(wear()[3])
}
#[allow(non_snake_case)]
fn SCARF() -> Option<Rgb> {
    let s = wear()[4];
    (s != 0).then(|| Rgb::hex(s))
}
/// Mittens: the scarf's colour, or the hat's, or the coat's (darker).
#[allow(non_snake_case)]
fn MITTENS() -> Rgb {
    SCARF().or(HAT()).unwrap_or(COAT() * 0.7)
}

/// Walking speed (cells/s) across and up/down the snowfield.
const SPEED_X: f32 = 11.0;
const SPEED_Y: f32 = 5.0;
/// A jump's upward speed (rows a second; about five rows high), and how
/// fast the villager climbs.
const JUMP: f32 = 26.0;
const CLIMB: f32 = 9.0;
/// The highest a leap up onto something reaches (rows, at the 75-row size).
const LEAP_MAX: f32 = 24.0;
/// Gravity (rows a second, a second).
const GRAVITY: f32 = 70.0;
/// Space held: the jump keeps rising this long (as a share of the time a
/// plain jump takes to its top) — up to twice as high.
const BOOST: f32 = 0.5;
/// How long a snow angel takes (s).
pub const ANGEL_TIME: f32 = 2.4;

pub struct Player {
    pub x: f32,
    /// Baseline row (feet).
    pub y: f32,
    pub dir: f32,
    pub berries: u32,
    pub moving: bool,
    /// Walking speed multiplier (the "Walk speed" setting).
    pub speed: f32,
    /// Shift is held: running (twice as fast).
    pub sprint: bool,
    /// Something held out in the other hand (glyph, colour).
    pub hold: Option<(char, Rgb)>,
    /// How big the village is drawn (1 at 75 rows; 2 at 150): the villager
    /// is drawn (and walks) to match.
    pub k: f32,
    /// When the player last moved or did something (scene time).
    pub active_at: f32,
    phase: f32,
    step_cd: f32,
    breath_at: f32,
    /// Being carried by the mouse: grab offset from (x, y).
    pub held: Option<(f32, f32)>,
    /// Falling after being let go: (speed, where it started).
    fall: Option<(f32, f32)>,
    pub dizzy_until: f32,
    /// Crouched, packing a snowball, until then.
    pub scoop_until: f32,
    /// A snowball in hand, ready to throw.
    pub snowball: bool,
    pub throw_until: f32,
    /// Carrying a lit lantern / warm from cocoa (set by the scene).
    pub lantern: bool,
    pub cozy: bool,
    /// Petting the pet until then; holding it in their arms (a cuddle).
    pub petting_until: f32,
    pub cuddling: bool,
    /// Up on something (a roof, a tree, a widget…): which ledge.
    pub perch: Option<usize>,
    /// Climbing up onto a ledge: which, and the row to get to.
    pub climb: Option<(usize, f32)>,
    /// The fall under way lands on ledges (a jump, stepping off an edge,
    /// being let go of), not just the snowfield.
    ledge_fall: bool,
    /// A ledge not to land on straight away (the one just hopped down from).
    left: Option<usize>,
    /// Arriving by magic (a soft landing); let go of by the mouse (a high
    /// drop makes them dizzy).
    magic: bool,
    dropped: bool,
    /// A jump from the snowfield: the row it left from.
    depth: Option<f32>,
    /// Dropping in on top of this one (passing anything above it).
    aim: Option<usize>,
    /// Dropping in from the sky (right-click → drop in here): the row to
    /// land on; drawn shimmering on the way down.
    pub drop_to: Option<f32>,
    /// Waving (at someone online) until then.
    pub wave_until: f32,
    /// Space still held: a jump keeps rising for as long as this lasts (s).
    pub jump_held: bool,
    boost: f32,
    /// Leaping onto a foothold a step or two along: the column to drift to.
    glide: Option<f32>,
    /// Scrambling up (or down) a steep bit of a tree, until then.
    scramble_until: f32,
    /// Lying in the snow, resting (since when).
    pub lie: Option<f32>,
    /// Making a snow angel, lying there (since when).
    pub angel_at: Option<f32>,
    /// A snow angel just finished (the scene presses it into the snow).
    pub angel_made: bool,
    /// Drawn this far along from where they are (the camera's part of a
    /// cell, so they hold still on the screen while the village scrolls by
    /// a cell at a time).
    pub shift: f32,
    /// Hopping down off something (S): straight down to the snowfield, not
    /// onto whatever's below on the way.
    hop: bool,
    /// Autopilot's steering: a steady pace (1.5×), whatever the setting.
    pub auto_pace: bool,
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Player { x, y, dir: 1.0, berries: 0, moving: false, speed: 1.0, sprint: false, hold: None, k: 1.0, active_at: f32::NEG_INFINITY, phase: 0.0, step_cd: 0.0, breath_at: 4.0, held: None, fall: None, dizzy_until: 0.0, scoop_until: 0.0, snowball: false, throw_until: 0.0, lantern: false, cozy: false, petting_until: 0.0, cuddling: false, perch: None, climb: None, ledge_fall: false, left: None, magic: false, dropped: false, depth: None, aim: None, drop_to: None, jump_held: false,
            wave_until: 0.0, boost: 0.0, glide: None, scramble_until: 0.0, lie: None, angel_at: None, angel_made: false, shift: 0.0, hop: false, auto_pace: false }
    }

    /// How fast to go: the walk speed, doubled while running.
    pub fn pace(&self) -> f32 {
        let base = if self.auto_pace { 1.5 } else { self.speed.clamp(0.25, 6.0) };
        base * if self.sprint { 2.0 } else { 1.0 }
    }

    pub fn rebase(&mut self, s: f32) {
        self.active_at -= s;
        self.breath_at -= s;
        self.dizzy_until -= s;
        self.scoop_until -= s;
        self.throw_until -= s;
        self.scramble_until -= s;
        self.lie = self.lie.map(|l| l - s);
        self.angel_at = self.angel_at.map(|a| a - s);
    }

    /// How the villager looks (for drawing them elsewhere: on a trip).
    pub fn look() -> super::sprites::Look {
        super::sprites::Look { kid: false, coat: COAT(), hat: HAT(), hair: HAIR(), scarf: SCARF(), skin: SKIN(), mittens: MITTENS(), holding: None, lantern: false }
    }

    /// Up in the air (a jump, a fall).
    pub fn in_air(&self) -> bool {
        self.fall.is_some()
    }

    /// Can walk / act right now (not carried, falling, dizzy or busy)?
    pub fn free(&self, t: f32) -> bool {
        self.held.is_none() && self.fall.is_none() && t >= self.dizzy_until && t >= self.scoop_until && self.lie.is_none()
    }

    /// Down into the snow on their back, for a rest (on a roof or a branch
    /// too).
    pub fn lie_down(&mut self, t: f32, d: &mut Director) -> bool {
        if !self.free(t) || self.climb.is_some() || self.cuddling {
            return false;
        }
        self.lie = Some(t);
        self.angel_at = None;
        self.moving = false;
        self.snowball = false;
        self.active_at = t;
        d.snow_puff(self.centre(), self.y);
        d.sound(Sfx::Rustle, self.centre(), 0.5);
        true
    }

    /// Up again, brushing the snow off.
    pub fn get_up(&mut self, t: f32, d: &mut Director) {
        if self.lie.take().is_some() {
            self.angel_at = None;
            self.active_at = t;
            d.snow_puff(self.centre(), self.y - 1.0);
            d.sound(Sfx::Rustle, self.centre(), 0.4);
        }
    }

    /// Lying there: arms and legs swept out and back, a snow angel.
    pub fn flap(&mut self, t: f32, d: &mut Director) {
        if self.lie.is_some() && self.angel_at.is_none() {
            self.angel_at = Some(t);
            self.active_at = t;
            d.sound(Sfx::Rustle, self.centre(), 0.6);
        }
    }

    /// Lying in the snow (for drawing, and what the keys do).
    pub fn lying(&self) -> bool {
        self.lie.is_some()
    }

    /// Space let go: the jump stops rising (and falls as usual).
    pub fn release_jump(&mut self) {
        self.jump_held = false;
        self.boost = 0.0;
    }

    /// (x, top, w, h) of the sprite, in cells.
    pub fn bbox(&self) -> (i32, i32, i32, i32) {
        (self.x.round() as i32, self.y.round() as i32 - 4, 2, 5)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (bx, by, bw, bh) = self.bbox();
        x >= bx - 1 && x <= bx + bw && y >= by && y < by + bh
    }

    pub fn grab(&mut self, x: i32, y: i32, t: f32) {
        self.held = Some((x as f32 - self.x, y as f32 - self.y));
        self.fall = None;
        self.perch = None;
        self.climb = None;
        self.aim = None;
        self.snowball = false;
        self.scoop_until = 0.0;
        self.active_at = t;
    }

    pub fn drag(&mut self, x: i32, y: i32, w: f32, h: f32) {
        if let Some((ox, oy)) = self.held {
            self.x = (x as f32 - ox).clamp(1.0, w - 4.0);
            self.y = (y as f32 - oy).clamp(4.0, h - 1.0);
        }
    }

    /// Let go: down into the snow (or onto whatever's underneath).
    pub fn release(&mut self) {
        if self.held.take().is_some() {
            self.fall = Some((0.0, self.y));
            self.ledge_fall = true;
            self.left = None;
            self.depth = None;
            self.aim = None;
            self.dropped = true;
        }
    }

    /// Up off the ground (on a ledge, climbing, or in the air)?
    pub fn aloft(&self) -> bool {
        self.perch.is_some() || self.climb.is_some() || (self.fall.is_some() && self.ledge_fall)
    }

    /// A jump (from the snow, or off a ledge). Up against something (a
    /// house, a tree, a lamp post at the back of the snowfield; a widget or a
    /// window above), it's a leap right up onto it. `ground`: the first
    /// snowfield row here.
    pub fn jump(&mut self, t: f32, ledges: &[Ledge], d: &mut Director) {
        // in the air already: up again anyway, on Christmas spirit
        if let (Some((vy, from)), true) = (self.fall, self.ledge_fall && self.held.is_none() && self.drop_to.is_none() && !self.magic) {
            let v = JUMP * self.k.sqrt();
            self.fall = Some((vy.min(-v), from));
            self.aim = None;
            self.glide = None;
            self.hop = false;
            self.jump_held = true;
            self.boost = BOOST * v / GRAVITY;
            self.active_at = t;
            d.burst(self.centre(), self.y, 10, &[Rgb::hex(0xffd36b), Rgb::WHITE, Rgb::hex(0xbfe4ff)], 4.0, 2.0);
            d.sound(Sfx::Magic, self.centre(), 0.25);
            return;
        }
        if self.held.is_some() || self.fall.is_some() || self.climb.is_some() || t < self.dizzy_until {
            return;
        }
        if self.lie.is_some() {
            // lying down: arms and legs out, a snow angel
            self.flap(t, d);
            return;
        }
        if self.perch.is_none() {
            if let Some((li, f, cx)) = super::ledges::climbable(ledges, self.centre(), self.y, LEAP_MAX * self.k) {
                // just over the top, then down onto it
                let v = (2.0 * GRAVITY * (self.y - f + 1.5)).sqrt();
                self.fall = Some((-v, self.y));
                self.ledge_fall = true;
                self.aim = Some(li);
                self.glide = ((cx - self.centre()).abs() > 0.4).then_some(cx);
                if let Some(g) = self.glide {
                    self.dir = (g - self.centre()).signum();
                }
                self.depth = None;
                self.left = None;
                self.dropped = false;
                self.boost = 0.0;
                self.active_at = t;
                d.sound(Sfx::Pickup, self.centre(), 0.5);
                d.snow_puff(self.centre(), self.y);
                return;
            }
        }
        // from the snow it lands back on the row it left (or on something
        // standing about as far forward); from up on something, back on it
        // (or on whatever it's steered over to). Held, it goes on up.
        self.depth = self.perch.take().is_none().then_some(self.y);
        self.left = None;
        let v = JUMP * self.k.sqrt();
        self.fall = Some((-v, self.y));
        self.ledge_fall = true;
        self.dropped = false;
        self.jump_held = true;
        self.boost = BOOST * v / GRAVITY;
        self.active_at = t;
        d.sound(Sfx::Pickup, self.centre(), 0.35);
    }

    /// Carried along (in the balloon's basket): just there, feet at row
    /// `y`, not falling, perched or climbing.
    pub fn ride(&mut self, x: f32, y: f32) {
        self.moving = (x - self.x).abs() > 1e-3;
        self.x = x;
        self.y = y;
        self.fall = None;
        self.perch = None;
        self.climb = None;
        self.held = None;
        self.lie = None;
        self.drop_to = None;
        self.left = None;
        self.depth = None;
    }

    /// What was underfoot has gone (a window closed or moved off): down.
    pub fn lose_footing(&mut self) {
        self.perch = None;
        self.lie = None;
        self.angel_at = None;
        self.climb = None;
        self.fall = Some((0.0, self.y));
        self.ledge_fall = true;
        self.left = None;
        self.depth = None;
        self.aim = None;
    }

    /// Climbing (for drawing): hand over hand.
    pub fn climbing(&self) -> bool {
        self.climb.is_some()
    }

    /// Scrambling up a steep bit of a tree (for drawing).
    fn scrambling(&self, t: f32) -> bool {
        t < self.scramble_until && self.perch.is_some()
    }

    /// In the air from a jump or a step off an edge (for drawing).
    pub fn leaping(&self) -> bool {
        self.fall.is_some() && self.ledge_fall && self.held.is_none() && self.drop_to.is_none() && !self.magic
    }

    /// Arrive from the sky at column `x`, landing on row `row`.
    pub fn drop_in(&mut self, x: f32, row: f32, t: f32, d: &mut Director) {
        self.held = None;
        self.perch = None;
        self.climb = None;
        self.aim = None;
        self.ledge_fall = false;
        self.x = x - 1.0;
        self.y = (row - 30.0).max(1.0);
        self.fall = Some((4.0, self.y));
        self.drop_to = Some(row);
        self.snowball = false;
        self.active_at = t;
        d.sound(Sfx::Magic, x, 0.8);
    }

    /// Hit by a snowball.
    pub fn bonk(&mut self, t: f32, d: &mut Director) {
        self.dizzy_until = t + 1.2;
        self.snowball = false;
        d.snow_puff(self.centre(), self.y - 2.0);
    }

    /// Walk-cycle phase (for drawing).
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Centre column of the sprite.
    pub fn centre(&self) -> f32 {
        self.x + 1.0
    }

    /// Arrive from the sky onto ledge `aim` (its top at column `x`).
    pub fn drop_on(&mut self, x: f32, aim: Option<usize>, t: f32, d: &mut Director) {
        self.drop_in(x, 0.0, t, d);
        self.drop_to = None;
        self.ledge_fall = true;
        self.left = None;
        self.depth = None;
        self.aim = aim;
        self.magic = true;
    }

    /// Move by the held direction keys. `lo(x)`: the first walkable row at
    /// column x; `w`/`h`: world size; `ledges`: what can be stood on.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), lo: impl Fn(f32) -> f32, w: f32, h: f32, ledges: &[Ledge], d: &mut Director) {
        let ground = lo(self.centre()).max(0.0);
        if self.held.is_some() {
            self.moving = false;
            self.lie = None;
            return;
        }
        // lying in the snow: still, but for a snow angel; a direction key
        // gets up
        if self.lie.is_some() {
            self.moving = false;
            if let Some(a) = self.angel_at {
                if t - a >= ANGEL_TIME {
                    self.angel_at = None;
                    self.angel_made = self.perch.is_none();
                    d.snow_puff(self.centre(), self.y);
                } else if (t * 4.0).fract() < dt * 4.0 {
                    d.sound(Sfx::Crunch, self.centre(), 0.12);
                }
            }
            if mv != (0.0, 0.0) && self.angel_at.is_none() {
                self.get_up(t, d);
            }
            if t > self.breath_at {
                self.breath_at = t + 3.0 + (t * 7.3).sin().abs() * 2.5;
                d.emote(self.centre() - self.dir * 1.5, self.y - 1.6, '∘', Rgb::hex(0xc8d0e0), 1);
            }
            return;
        }
        // climbing up onto something, hand over hand
        if let Some((li, to)) = self.climb {
            self.y -= CLIMB * self.k * dt;
            self.phase += dt * 8.0;
            self.moving = true;
            if self.y <= to {
                self.y = to;
                self.climb = None;
                self.perch = Some(li);
                d.snow_puff(self.centre(), self.y);
            }
            return;
        }
        if let Some((vy, from)) = self.fall {
            // in a jump (or off an edge) the arrows still steer a little
            if let Some(g) = self.glide {
                // drifting over to the foothold being leapt for
                let dx = g - self.centre();
                let step = (24.0 * self.k * dt).min(dx.abs());
                self.x += step * dx.signum();
                if dx.abs() <= step + 0.01 {
                    self.glide = None;
                }
            } else if self.ledge_fall && self.drop_to.is_none() && mv.0 != 0.0 {
                self.dir = mv.0.signum();
                self.x = (self.x + mv.0 * SPEED_X * self.pace() * self.k * dt).clamp(1.0, w - 4.0);
            }
            let prev = self.y;
            // Space held: still rising, for a while
            let rising = self.ledge_fall && self.jump_held && self.boost > 0.0 && vy < 0.0;
            let vy = if rising {
                self.boost -= dt;
                vy
            } else {
                vy + GRAVITY * dt
            };
            self.y += vy * dt;
            // landing on something on the way down (not hopping down)
            if self.ledge_fall && vy > 0.0 && !self.hop {
                let found = match self.aim {
                    Some(a) => ledges.get(a).and_then(|l| l.feet(self.centre())).filter(|&f| prev <= f + 0.01 && self.y >= f).map(|f| (a, f)),
                    None => super::ledges::landing(ledges, self.centre(), prev, self.y, self.left, self.depth),
                };
                if let Some((li, f)) = found {
                    self.depth = None;
                    self.aim = None;
                    self.glide = None;
                    self.y = f;
                    self.fall = None;
                    self.perch = Some(li);
                    self.left = None;
                    d.snow_puff(self.centre(), self.y);
                    d.sound(Sfx::Drop, self.centre(), 0.5);
                    if std::mem::take(&mut self.magic) {
                        d.burst(self.centre(), self.y - 1.0, 24, &[Rgb::hex(0xbfe4ff), Rgb::WHITE, Rgb::hex(0xffd36b)], 7.0, 4.0);
                    }
                    self.moving = false;
                    return;
                }
            }
            // (well clear of it: past where it could catch them again)
            if self.ledge_fall && self.y > prev && self.left.is_some_and(|l| ledges.get(l).and_then(|l| l.feet(self.centre())).is_none_or(|f| self.y > f + super::ledges::CATCH + 0.5)) {
                // clear of the ledge just left: it can be landed on again
                self.left = None;
            }
            // down to the snow (a jump from it: back to the row it left)
            let to = self.drop_to.unwrap_or(from).min(h - 1.0).max(ground);
            if self.drop_to.is_some() && (t * 12.0).fract() < dt * 12.0 {
                // a trail of sparkles on the way down
                d.emote(self.centre(), self.y - 2.0, '✦', Rgb::hex(0xbfe4ff), 1);
            }
            if self.y >= to && self.y >= ground && vy > 0.0 {
                self.y = to.round();
                self.fall = None;
                self.depth = None;
                self.aim = None;
                self.glide = None;
                self.hop = false;
                d.snow_puff(self.centre(), self.y);
                // a jump or a hop down lands on its feet; dropped from a
                // height, it's a bit of a tumble
                let high = self.y - from > if self.ledge_fall && !self.dropped { 14.0 } else { 3.0 };
                self.dropped = false;
                self.ledge_fall = false;
                self.left = None;
                d.sound(Sfx::Drop, self.centre(), if high { 1.0 } else { 0.6 });
                if self.drop_to.take().is_some() || std::mem::take(&mut self.magic) {
                    // a soft, magical landing (no dizziness)
                    d.burst(self.centre(), self.y - 1.0, 24, &[Rgb::hex(0xbfe4ff), Rgb::WHITE, Rgb::hex(0xffd36b)], 7.0, 4.0);
                } else if high {
                    self.dizzy_until = t + 1.8;
                }
            } else {
                self.fall = Some((vy, from));
            }
            self.moving = false;
            return;
        }
        let (mx, my) = if self.free(t) { mv } else { (0.0, 0.0) };
        // up on something: along its top (up and down its slopes), off the
        // end to drop down; S hops down
        if let Some(li) = self.perch {
            let Some(l) = ledges.get(li) else {
                self.perch = None;
                self.fall = Some((0.0, self.y));
                self.ledge_fall = true;
                return;
            };
            if my > 0.0 {
                // S: down off it, all the way to the snow
                self.perch = None;
                self.left = Some(li);
                self.fall = Some((0.0, self.y));
                self.ledge_fall = true;
                self.hop = true;
                return;
            }
            // W: up towards the top of it (the ridge, the star)
            let mx = if mx == 0.0 && my < 0.0 {
                l.peak(self.centre()).map(|px| px - self.centre()).filter(|dx| dx.abs() > 0.6).map(f32::signum).unwrap_or(0.0)
            } else {
                mx
            };
            self.moving = mx != 0.0;
            let tree = l.kind == super::ledges::Kind::Tree;
            let step = if tree { super::ledges::TREE_STEP * self.k.max(1.0) } else { super::ledges::STEP };
            if mx != 0.0 {
                self.active_at = t;
                self.dir = mx.signum();
                // slower, scrambling up through the branches
                let pace = if t < self.scramble_until { 0.55 } else { 1.0 };
                let nx = (self.x + mx * SPEED_X * self.pace() * pace * self.k * dt).clamp(1.0, w - 4.0);
                match l.feet(nx + 1.0) {
                    // a step up or down the slope (a scramble, on a tree)
                    Some(f) if (f - self.y).abs() <= step => {
                        if (f - self.y).abs() > super::ledges::STEP * 0.6 {
                            self.scramble_until = t + 0.3;
                        }
                        self.x = nx;
                        self.y = f;
                    }
                    // a wall (a chimney, the steeple): no further
                    Some(f) if f < self.y => {}
                    // off the end (or a sheer drop): down we go
                    _ => {
                        self.x = nx;
                        self.perch = None;
                        self.left = Some(li);
                        self.fall = Some((0.0, self.y));
                        self.ledge_fall = true;
                        return;
                    }
                }
                self.phase += dt * self.pace().sqrt() * 9.0;
                self.step_cd -= dt;
                if self.step_cd <= 0.0 {
                    self.step_cd = 0.32;
                    d.sound(Sfx::Crunch, self.centre(), 0.14);
                }
            } else if let Some(f) = l.feet(self.centre()) {
                self.y = f;
            }
            return;
        }
        self.moving = mx != 0.0 || my != 0.0;
        if self.moving {
            self.active_at = t;
            if mx != 0.0 {
                self.dir = mx.signum();
            }
            let k = self.pace();
            self.x = (self.x + mx * SPEED_X * k * self.k * dt).clamp(1.0, w - 4.0);
            self.y += my * SPEED_Y * k.sqrt() * self.k * dt;
            self.phase += dt * k.sqrt() * if mx != 0.0 { 9.0 } else { 6.0 };
            self.step_cd -= dt;
            if self.step_cd <= 0.0 {
                self.step_cd = 0.32;
                d.footprint(self.centre().round() as i32, self.y.round() as i32, t);
                d.sound(Sfx::Crunch, self.centre(), 0.18);
            }
        }
        self.y = self.y.clamp(lo(self.centre()), h - 1.0);
        // standing about in the cold: a puff of breath now and then
        if !self.moving && t > self.breath_at {
            self.breath_at = t + 3.5 + (t * 7.3).sin().abs() * 3.0;
            let bx = self.centre() + self.dir * 1.2;
            d.emote(bx, self.y - 2.6, '∘', Rgb::hex(0xc8d0e0), 1);
        }
    }

    /// Drawn where they stand, or through a zoomed `view` (straight into
    /// the zoomed frame, at the bigger size).
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, view: Option<super::View>) {
        // (centre column, feet row, scale) on screen
        let (sc, sy, sk) = match view {
            Some(v) => (v.x(self.centre()), v.y(self.y + 1.0) - 1.0, self.k * v.z),
            None => (self.centre() - self.shift, self.y, self.k),
        };
        let lying = self.lie.is_some();
        let flap = self.angel_at.map(|a| ((t - a) * 3.0) as i32 % 2 == 0);
        let step = (self.phase as i32) % 2 == 0;
        let kick = (t * 7.0) as i32 % 2 == 0;
        let up = self.held.is_some() || self.fall.is_some();
        let scooping = t < self.scoop_until;
        let throwing = t < self.throw_until;
        // facing right; mirrored for left
        let rows: [&str; 4] = if up {
            [" ▲", "╲●╱", " █", if kick { " ╱" } else { " ╲" }]
        } else if scooping {
            ["", " ▲", "~●▖", " ▀"]
        } else if throwing {
            [" ▲╱", " ●", "~█", " ║"]
        } else {
            [" ▲", " ●", "~█", if self.moving && step { " ╨" } else { " ║" }]
        };
        let wave = !self.moving && !up && !scooping && !throwing && ((t * 0.5).sin() > 0.97 || t < self.wave_until);
        // the detailed sprite, where the front-end can show pictures
        if crate::frame::pictures_ok() {
            use super::sprites::{self, Look, Pose};
            let pose = if lying {
                match flap {
                    Some(out) => Pose::Angel(out as u8),
                    None => Pose::Lie,
                }
            } else if self.climbing() || self.scrambling(t) {
                Pose::Climb(step as u8)
            } else if self.leaping() {
                Pose::Jump
            } else if up {
                Pose::ArmsUp
            } else if scooping {
                Pose::Scoop
            } else if throwing {
                Pose::Throw
            } else if self.cuddling {
                Pose::Hold
            } else if t < self.petting_until && !self.moving {
                Pose::Pet
            } else if self.moving {
                Pose::Walk(step as u8)
            } else if wave {
                Pose::Wave
            } else {
                Pose::Stand
            };
            use super::sprites::Hold;
            let holding = if self.snowball && !throwing {
                Some(Hold::Snowball)
            } else {
                self.hold.and_then(|(ch, _)| match ch {
                    '◘' => Some(Hold::Cocoa),
                    '◆' => Some(Hold::Gingerbread),
                    'ʃ' => Some(Hold::CandyCane),
                    '▾' => Some(Hold::Carrot),
                    '⁘' => Some(Hold::Berries),
                    _ => None,
                })
            };
            let look = Look { holding: if self.lantern { None } else { holding }, lantern: self.lantern, ..Self::look() };
            let light = 0.9 + 0.25 * f.day.max(0.0);
            let pic = sprites::person(&look, pose, self.dir, light);
            let rows = sprites::rows_of(&pic, sk);
            f.sprite(sc, sy + 1.0, rows, pic);
            let (bx, by) = (sc, sy * 2.0 - 4.0 * sk);
            if self.lantern {
                let fl = 0.85 + 0.15 * (t * 9.0).sin();
                f.light_px(bx + self.dir * 2.0 * sk, sy * 2.0 - 2.0 * sk, Rgb::hex(0xffb050) * (7.0 * fl * glow));
            }
            if t < self.dizzy_until {
                for k in 0..3 {
                    let a = t * 5.0 + k as f32 * 2.09;
                    let (x, y) = (bx + a.cos() * 3.0, (by - 6.0 * sk) / 2.0 + a.sin() * 0.6);
                    f.put_g(x.round() as i32, y.round() as i32, Glyph::new(if k % 2 == 0 { '✦' } else { '✧' }, Rgb::hex(0xffe08a)).lit(0.0));
                }
            }
            if self.drop_to.is_some() {
                f.light_px(bx, by, Rgb::hex(0x9fd8ff) * (5.0 * glow));
            }
            f.light_px(bx, by, Rgb::hex(0xffc878) * (1.6 * glow));
            if self.cozy {
                f.light_px(bx, by - 2.0, Rgb::hex(0xff9a7a) * (1.4 * glow));
            }
            return;
        }
        // lying in the snow: flat out, arms and legs swept out and in
        if lying {
            let feet = sy.round() as i32;
            let x0 = (sc - 2.0).round() as i32;
            let cells: [(i32, char, Rgb); 5] = [(0, '•', Rgb::hex(0xf4f0ea)), (1, '●', SKIN()), (2, '▬', COAT()), (3, '▬', COAT()), (4, '▀', BOOTS)];
            for (dx, ch, c) in cells {
                let x = if self.dir > 0.0 { x0 + dx } else { x0 + 4 - dx };
                f.put_g(x, feet, Glyph::new(ch, c).bg(Bg::Scene).lit(0.6));
            }
            if flap == Some(true) {
                let (a, b) = if self.dir > 0.0 { (x0 + 2, x0 + 3) } else { (x0 + 2, x0 + 1) };
                f.put_g(a, feet - 1, Glyph::new('╲', COAT()).lit(0.6));
                f.put_g(b, feet - 1, Glyph::new('╱', COAT()).lit(0.6));
            }
            f.light_cell(x0 + 2, feet, Rgb::hex(0xffc878) * (1.6 * glow));
            return;
        }
        // on a screen tall enough: the pixel-art villager, as indoors
        if sk >= 1.75 && !up && !scooping {
            let b = super::interior::Big {
                x: sc,
                feet: sy.round() as i32,
                dir: self.dir,
                step: self.moving && step,
                hat: HAT(),
                hair: HAIR(),
                coat: COAT(),
                scarf: SCARF(),
                sit: None,
                kid: false,
                arms_up: throwing || wave || self.climbing(),
                outdoor: true,
                behind: None,
            };
            b.draw(f, t);
            let (hx, hy) = (sc.round() as i32 + if self.dir > 0.0 { 3 } else { -4 }, sy.round() as i32 * 2 - 5);
            let thing = if self.snowball && !throwing {
                Some(Rgb::hex(0xf2f6ff))
            } else if self.lantern {
                None
            } else {
                self.hold.map(|(_, c)| c)
            };
            if let Some(c) = thing {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    f.px_sprite(hx + dx, hy + dy, c);
                }
            }
            let bx = sc;
            let by = sy * 2.0 - 8.0;
            if self.lantern {
                let fl = 0.85 + 0.15 * (t * 9.0).sin();
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    f.px_sprite(hx + dx, hy + dy, Rgb::hex(0xffd27a) * (1.2 * fl));
                }
                f.light_px(hx as f32 + 1.0, hy as f32 + 1.0, Rgb::hex(0xffb050) * (7.0 * fl * glow));
            }
            if t < self.dizzy_until {
                for k in 0..3 {
                    let a = t * 5.0 + k as f32 * 2.09;
                    let (sx, sy) = (bx + a.cos() * 3.0, (by - 10.0) / 2.0 + a.sin() * 0.6);
                    f.put_g(sx.round() as i32, sy.round() as i32, Glyph::new(if k % 2 == 0 { '✦' } else { '✧' }, Rgb::hex(0xffe08a)).lit(0.0));
                }
            }
            if self.drop_to.is_some() {
                f.light_px(bx, by, Rgb::hex(0x9fd8ff) * (5.0 * glow));
            }
            f.light_px(bx, by, Rgb::hex(0xffc878) * (1.6 * glow));
            if self.cozy {
                f.light_px(bx, by - 2.0, Rgb::hex(0xff9a7a) * (1.4 * glow));
            }
            return;
        }
        let top = sy.round() as i32 - 3;
        let x0 = (sc - 1.0).round() as i32;
        // drawn bigger on a taller screen: each cell scaled about the feet
        let k = sk;
        let (axi, ayi, ax, ay) = (x0 + 1, top + 4, sc, sy + 1.0);
        let place = move |cx: i32, cy: i32| (ax + (cx - axi) as f32 * k, ay + (cy - ayi) as f32 * k);
        let put = |f: &mut Frame, cx: i32, cy: i32, g: Glyph| {
            if k <= 1.0 {
                f.put_g(cx, cy, g);
            } else {
                let (sx, sy) = place(cx, cy);
                f.sprite_cell(sx, sy, k, g);
            }
        };
        let light = |f: &mut Frame, cx: i32, cy: i32, c: Rgb| {
            if k <= 1.0 {
                f.light_cell(cx, cy, c);
            } else {
                let (sx, sy) = place(cx, cy);
                f.light_px(sx + 0.5 * k, (sy + 0.5 * k) * 2.0, c);
            }
        };
        for (ry, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let (cx, ch) = if self.dir > 0.0 { (x0 + i as i32, ch) } else { (x0 + 1 - i as i32, super::events::mirror(ch)) };
                let col = match (ry, ch) {
                    (_, '▲') => HAT().unwrap_or(HAIR()),
                    (_, '●') => SKIN(),
                    (_, '~') => SCARF().unwrap_or(COAT()),
                    (3, '╨' | '║' | '▀') => BOOTS,
                    (3, '╱' | '╲') if up => BOOTS,
                    _ => COAT(),
                };
                // the scarf's loose end flutters
                let ch = if ch == '~' && (t * 6.0) as i32 % 2 == 0 { '≈' } else { ch };
                // phasing in from the sky: shimmering, half there
                let a = if self.drop_to.is_some() { 0.45 + 0.35 * (t * 14.0 + ry as f32).sin().abs() } else { 1.0 };
                let col = if self.drop_to.is_some() { col.lerp(Rgb::hex(0xbfe4ff), 0.5) } else { col };
                put(f, cx, top + ry as i32, Glyph::new(ch, col).bg(Bg::Scene).alpha(a).lit(0.6));
            }
        }
        // the pompom, and a wave now and then while standing about
        let ptop = if scooping { top } else { top - 1 };
        put(f, x0 + if self.dir > 0.0 { 1 } else { 0 }, ptop, Glyph::new('•', Rgb::hex(0xf4f0ea)).lit(0.5));
        // a snowball in hand
        if self.snowball && !throwing && !scooping {
            let hx = if self.dir > 0.0 { x0 + 2 } else { x0 - 1 };
            put(f, hx, top + 2, Glyph::new('●', Rgb::hex(0xf2f6ff)).lit(0.4));
        }
        // whatever's held out (berries, a carrot, a mug…)
        if let Some((ch, col)) = self.hold.filter(|_| !throwing && !scooping && !up && !(self.snowball && !throwing)) {
            let hx = if self.dir > 0.0 { x0 + 2 } else { x0 - 1 };
            put(f, hx, top + 2, Glyph::new(ch, col).lit(0.4));
        }
        // dizzy stars
        if t < self.dizzy_until {
            let cx = sc;
            for k in 0..3 {
                let a = t * 5.0 + k as f32 * 2.09;
                let (sx, sy) = (cx + a.cos() * 1.6, top as f32 - 1.0 + a.sin() * 0.4);
                put(f, sx.round() as i32, sy.round() as i32, Glyph::new(if k % 2 == 0 { '✦' } else { '✧' }, Rgb::hex(0xffe08a)).lit(0.0));
            }
        }
        if wave {
            let ax = if self.dir > 0.0 { x0 + 2 } else { x0 - 1 };
            put(f, ax, top + 1, Glyph::new(if self.dir > 0.0 { '╱' } else { '╲' }, COAT()).lit(0.6));
        }
        if self.drop_to.is_some() {
            light(f, x0 + 1, top + 1, Rgb::hex(0x9fd8ff) * (5.0 * glow));
        }
        // a soft glow so the villager is always easy to spot (much more
        // with a lantern; a little more, warm and rosy, after cocoa)
        light(f, x0 + 1, top + 2, Rgb::hex(0xffc878) * (1.6 * glow));
        if self.lantern && !up {
            let lx = if self.dir > 0.0 { x0 + 2 } else { x0 - 1 };
            let fl = 0.85 + 0.15 * (t * 9.0).sin();
            put(f, lx, top + 2, Glyph::new('▣', Rgb::hex(0xffd27a) * (1.2 * fl)).lit(0.0));
            light(f, lx, top + 2, Rgb::hex(0xffb050) * (7.0 * fl * glow));
        }
        if self.cozy {
            light(f, x0 + 1, top + 1, Rgb::hex(0xff9a7a) * (1.4 * glow));
        }
    }

    /// The little prompt shown over whatever E would interact with.
    pub fn draw_prompt(f: &mut Frame, x: i32, y: i32, key: char, t: f32) {
        let bob = if (t * 2.0) as i32 % 2 == 0 { 0 } else { -1 };
        let bg = Bg::Mix(Rgb::hex(0x1a1a24), 0.75);
        let was = std::mem::replace(&mut f.ui, true);
        f.put_g(x - 1, y + bob, Glyph::new('[', Rgb::hex(0xc8c0b0)).bg(bg).lit(0.0));
        f.put_g(x, y + bob, Glyph::new(key, Rgb::hex(0xffe08a)).bg(bg).lit(0.0));
        f.put_g(x + 1, y + bob, Glyph::new(']', Rgb::hex(0xc8c0b0)).bg(bg).lit(0.0));
        f.ui = was;
    }
}
