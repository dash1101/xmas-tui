//! You: a little bundled-up villager you walk around with WASD / the arrow
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
    /// When the player last moved or did something (scene time).
    pub active_at: f32,
    phase: f32,
    step_cd: f32,
    breath_at: f32,
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Player { x, y, dir: 1.0, berries: 0, moving: false, active_at: f32::NEG_INFINITY, phase: 0.0, step_cd: 0.0, breath_at: 4.0 }
    }

    pub fn rebase(&mut self, s: f32) {
        self.active_at -= s;
        self.breath_at -= s;
    }

    /// Centre column of the sprite.
    pub fn centre(&self) -> f32 {
        self.x + 1.0
    }

    /// Move by the held direction keys. `lo(x)`: the first walkable row at
    /// column x; `w`/`h`: world size.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), lo: impl Fn(f32) -> f32, w: f32, h: f32, d: &mut Director) {
        let (mx, my) = mv;
        self.moving = mx != 0.0 || my != 0.0;
        if self.moving {
            self.active_at = t;
            if mx != 0.0 {
                self.dir = mx.signum();
            }
            self.x = (self.x + mx * SPEED_X * dt).clamp(1.0, w - 4.0);
            self.y += my * SPEED_Y * dt;
            self.phase += dt * if mx != 0.0 { 9.0 } else { 6.0 };
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

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        let step = (self.phase as i32) % 2 == 0;
        // facing right; mirrored for left
        let rows: [&str; 4] = [" ▲", " ●", "~█", if self.moving && step { " ╨" } else { " ║" }];
        let wave = !self.moving && (t * 0.5).sin() > 0.97;
        let top = self.y.round() as i32 - 3;
        let x0 = self.x.round() as i32;
        for (ry, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let (cx, ch) = if self.dir > 0.0 { (x0 + i as i32, ch) } else { (x0 + 1 - i as i32, super::events::mirror(ch)) };
                let col = match (ry, ch) {
                    (0, _) => HAT,
                    (1, _) => SKIN,
                    (2, '~') => SCARF,
                    (3, _) => BOOTS,
                    _ => COAT,
                };
                // the scarf's loose end flutters
                let ch = if ch == '~' && (t * 6.0) as i32 % 2 == 0 { '≈' } else { ch };
                f.put_g(cx, top + ry as i32, Glyph::new(ch, col).bg(Bg::Scene).lit(0.6));
            }
        }
        // the pompom, and a wave now and then while standing about
        f.put_g(x0 + if self.dir > 0.0 { 1 } else { 0 }, top - 1, Glyph::new('•', Rgb::hex(0xf4f0ea)).lit(0.5));
        if wave {
            let ax = if self.dir > 0.0 { x0 + 2 } else { x0 - 1 };
            f.put_g(ax, top + 1, Glyph::new(if self.dir > 0.0 { '╱' } else { '╲' }, COAT).lit(0.6));
        }
        // a soft lantern-ish glow so you can always find yourself
        f.light_cell(x0 + 1, top + 2, Rgb::hex(0xffc878) * (1.6 * glow));
    }

    /// The little prompt shown over whatever E would interact with.
    pub fn draw_prompt(f: &mut Frame, x: i32, y: i32, key: char, t: f32) {
        let bob = if (t * 2.0) as i32 % 2 == 0 { 0 } else { -1 };
        let bg = Bg::Mix(Rgb::hex(0x1a1a24), 0.75);
        f.put_g(x - 1, y + bob, Glyph::new('[', Rgb::hex(0xc8c0b0)).bg(bg).lit(0.0));
        f.put_g(x, y + bob, Glyph::new(key, Rgb::hex(0xffe08a)).bg(bg).lit(0.0));
        f.put_g(x + 1, y + bob, Glyph::new(']', Rgb::hex(0xc8c0b0)).bg(bg).lit(0.0));
    }
}
