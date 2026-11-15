//! The player: a little bundled-up villager walked around with WASD / the arrow
//! keys. They crunch through the snow, leave footprints, and can say hello
//! to people, pet and feed the animals, pick berries, knock on doors and
//! poke at things (E / Space / Enter), or ask someone to walk along (F).

use super::events::Director;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Bg, Frame, Glyph};

const COAT: Rgb = Rgb::hex(0xe0a030);
const HAT: Rgb = Rgb::hex(0x2fae5c);
const SCARF: Rgb = Rgb::hex(0xd8283a);
const SKIN: Rgb = Rgb::hex(0xe8c39e);
const BOOTS: Rgb = Rgb::hex(0x2a2a34);

/// Walking speed (cells/s) across and up/down the snowfield.
const SPEED_X: f32 = 11.0;
const SPEED_Y: f32 = 5.0;

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
    /// Dropping in from the sky (right-click → drop in here): the row to
    /// land on; drawn shimmering on the way down.
    pub drop_to: Option<f32>,
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Player { x, y, dir: 1.0, berries: 0, moving: false, speed: 1.0, sprint: false, hold: None, k: 1.0, active_at: f32::NEG_INFINITY, phase: 0.0, step_cd: 0.0, breath_at: 4.0, held: None, fall: None, dizzy_until: 0.0, scoop_until: 0.0, snowball: false, throw_until: 0.0, lantern: false, cozy: false, drop_to: None }
    }

    /// How fast to go: the walk speed, doubled while running.
    pub fn pace(&self) -> f32 {
        self.speed.clamp(0.25, 6.0) * if self.sprint { 2.0 } else { 1.0 }
    }

    pub fn rebase(&mut self, s: f32) {
        self.active_at -= s;
        self.breath_at -= s;
        self.dizzy_until -= s;
        self.scoop_until -= s;
        self.throw_until -= s;
    }

    /// Can walk / act right now (not carried, falling, dizzy or busy)?
    pub fn free(&self, t: f32) -> bool {
        self.held.is_none() && self.fall.is_none() && t >= self.dizzy_until && t >= self.scoop_until
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

    /// Let go: down into the snow.
    pub fn release(&mut self) {
        if self.held.take().is_some() {
            self.fall = Some((0.0, self.y));
        }
    }

    /// Arrive from the sky at column `x`, landing on row `row`.
    pub fn drop_in(&mut self, x: f32, row: f32, t: f32, d: &mut Director) {
        self.held = None;
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

    /// Move by the held direction keys. `lo(x)`: the first walkable row at
    /// column x; `w`/`h`: world size.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), lo: impl Fn(f32) -> f32, w: f32, h: f32, d: &mut Director) {
        let ground = lo(self.centre()).max(0.0);
        if self.held.is_some() {
            self.moving = false;
            return;
        }
        if let Some((vy, from)) = self.fall {
            let vy = vy + 70.0 * dt;
            self.y += vy * dt;
            let to = self.drop_to.unwrap_or(self.y).min(h - 1.0).max(ground);
            if self.drop_to.is_some() && (t * 12.0).fract() < dt * 12.0 {
                // a trail of sparkles on the way down
                d.emote(self.centre(), self.y - 2.0, '✦', Rgb::hex(0xbfe4ff), 1);
            }
            if self.y >= to && self.y >= ground {
                self.y = to.round();
                self.fall = None;
                d.snow_puff(self.centre(), self.y);
                let high = self.y - from > 3.0;
                d.sound(Sfx::Drop, self.centre(), if high { 1.0 } else { 0.6 });
                if self.drop_to.take().is_some() {
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
            None => (self.centre(), self.y, self.k),
        };
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
        let wave = !self.moving && !up && !scooping && !throwing && (t * 0.5).sin() > 0.97;
        // on a screen tall enough: the pixel-art villager, as indoors
        if sk >= 1.75 && !up && !scooping {
            let b = super::interior::Big {
                x: sc,
                feet: sy.round() as i32,
                dir: self.dir,
                step: self.moving && step,
                hat: Some(HAT),
                hair: Rgb::hex(0x3a2a1a),
                coat: COAT,
                scarf: Some(SCARF),
                sit: None,
                kid: false,
                arms_up: throwing || wave,
                outdoor: true,
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
                    (_, '▲') => HAT,
                    (_, '●') => SKIN,
                    (_, '~') => SCARF,
                    (3, '╨' | '║' | '▀') => BOOTS,
                    (3, '╱' | '╲') if up => BOOTS,
                    _ => COAT,
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
            put(f, ax, top + 1, Glyph::new(if self.dir > 0.0 { '╱' } else { '╲' }, COAT).lit(0.6));
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
