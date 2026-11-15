//! Santa's sleigh and reindeer crossing the sky every few minutes, leaving
//! a trail of golden sparkles (and passing in front of the moon).

use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::Rng;

struct Spark {
    x: f32,
    y: f32,
    age: f32,
    life: f32,
    ch: char,
}

pub struct Sleigh {
    active: bool,
    x: f32,
    y0: f32,
    speed: f32,
    /// Next scheduled fly-by (scene time; f32::MAX when `interval` is 0).
    next: f32,
    /// An explicit fly-by asked for (moon click, "hohoho", Christmas Eve,
    /// --export --sleigh), due at this time. Separate from `next` so it
    /// works even when the regular schedule is off ("never").
    requested: Option<f32>,
    interval: f32,
    sparks: Vec<Spark>,
    spawn: f32,
    rng: Rng,
    w: f32,
    moon_row: f32,
}

const DEER: usize = 4;
const RED: Rgb = Rgb::hex(0xc8202a);
const GOLD: Rgb = Rgb::hex(0xffcc55);
const FUR: Rgb = Rgb::hex(0x8a5a32);
const WHITE: Rgb = Rgb::hex(0xf2efe8);

impl Sleigh {
    pub fn new(w: usize, moon_row: f32, minutes: f32, seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x51e1);
        let interval = minutes * 60.0;
        let first = if minutes > 0.0 { rng.range(25.0, 60.0) } else { f32::MAX };
        Sleigh {
            active: false,
            x: 0.0,
            y0: 0.0,
            speed: 0.0,
            next: first,
            requested: None,
            interval,
            sparks: vec![],
            spawn: 0.0,
            rng,
            w: w as f32,
            moon_row,
        }
    }

    /// Force a fly-by at time `t` (or right after the current one).
    pub fn trigger(&mut self, t: f32) {
        self.requested = Some(self.requested.map_or(t, |r| r.min(t)));
    }

    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        if self.next != f32::MAX {
            self.next -= s;
        }
        if let Some(r) = &mut self.requested {
            *r -= s;
        }
    }

    /// Centre column of the team + sleigh while flying (for the bells).
    pub fn audio_x(&self) -> Option<f32> {
        self.active.then_some(self.x + (DEER * 5) as f32 * 0.75)
    }

    fn y_at(&self, x: f32, t: f32) -> f32 {
        self.y0 + (x * 0.035).sin() * 1.6 + (t * 2.2).sin() * 0.3
    }

    pub fn update(&mut self, dt: f32, t: f32) {
        let asked = self.requested.is_some_and(|r| t >= r);
        let due = self.interval > 0.0 && t >= self.next;
        if !self.active && (asked || due) {
            self.active = true;
            self.requested = None;
            self.x = self.w + 4.0;
            self.speed = self.rng.range(22.0, 30.0);
            self.y0 = self.moon_row + self.rng.range(-2.0, 2.0);
            // only reschedule when there is a schedule (interval 0 = "never")
            if self.interval > 0.0 {
                self.next = t + self.interval * self.rng.range(0.75, 1.25);
            }
        }
        if self.active {
            self.x -= self.speed * dt;
            self.spawn += dt;
            while self.spawn > 0.03 {
                self.spawn -= 0.03;
                let tail = self.x + (DEER * 5) as f32 + 9.0;
                let ch = *self.rng.pick(&['✦', '·', '*', '⋆', '·', '✧']);
                let jitter = self.rng.range(-0.8, 1.2);
                let yy = self.y_at(tail, t) + 0.5 + jitter;
                let life = self.rng.range(1.5, 3.5);
                self.sparks.push(Spark { x: tail + self.rng.range(-0.5, 2.0), y: yy, age: 0.0, life, ch });
            }
            if self.x < -((DEER * 5) as f32 + 14.0) {
                self.active = false;
            }
        }
        for s in &mut self.sparks {
            s.age += dt;
            s.y += dt * 0.6;
        }
        self.sparks.retain(|s| s.age < s.life);
    }

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        for s in &self.sparks {
            let k = (1.0 - s.age / s.life).powf(1.3);
            let col = GOLD.lerp(WHITE, 0.3) * (k * 1.1);
            f.put_g(s.x as i32, s.y as i32, Glyph::new(s.ch, col).alpha(k).lit(0.0));
            if k > 0.6 {
                f.light_cell(s.x as i32, s.y as i32, GOLD * (k * 0.5 * glow));
            }
        }
        if !self.active {
            return;
        }
        let gallop = ((t * 8.0) as i32) % 2 == 0;
        // reindeer team, Rudolph in front
        for d in 0..DEER {
            let x = self.x + (d * 5) as f32;
            let y = self.y_at(x, t).round() as i32;
            let xi = x.round() as i32;
            if d == 0 {
                f.put_g(xi - 1, y, Glyph::new('●', Rgb::hex(0xff2a2a) * 1.4).lit(0.0));
                f.light_cell(xi - 1, y, Rgb::hex(0xff3030) * (4.0 * glow));
            }
            f.put_g(xi, y - 1, Glyph::new('Ψ', FUR * 1.2).lit(0.3));
            f.put_g(xi, y, Glyph::new('◣', FUR).lit(0.3));
            f.put_g(xi + 1, y, Glyph::new('█', FUR).lit(0.3));
            f.put_g(xi + 2, y, Glyph::new('▀', FUR).lit(0.3));
            let (l1, l2) = if gallop { ('╱', '╲') } else { ('╲', '╱') };
            f.put_g(xi + 1, y + 1, Glyph::new(l1, FUR * 0.9).lit(0.3));
            f.put_g(xi + 2, y + 1, Glyph::new(l2, FUR * 0.9).lit(0.3));
            // harness
            f.put_g(xi + 3, y, Glyph::new('╌', GOLD * 0.7).lit(0.0));
            f.put_g(xi + 4, y, Glyph::new('╌', GOLD * 0.7).lit(0.0));
        }
        // the sleigh
        let sx = self.x + (DEER * 5) as f32;
        let y = self.y_at(sx, t).round() as i32;
        let x = sx.round() as i32;
        // Santa
        f.put_g(x + 3, y - 2, Glyph::new('▲', RED * 1.1).lit(0.3));
        f.put_g(x + 4, y - 2, Glyph::new('•', WHITE).lit(0.3));
        f.put_g(x + 3, y - 1, Glyph::new('☻', WHITE).solid(RED * 0.9).lit(0.3));
        f.put_g(x + 5, y - 1, Glyph::new('▄', Rgb::hex(0x7a3a1a)).lit(0.3));
        f.put_g(x + 6, y - 1, Glyph::new('▄', Rgb::hex(0x2a6a3a)).lit(0.3));
        // body
        f.put_g(x, y, Glyph::new('◥', GOLD).lit(0.3));
        for i in 1..8 {
            f.put_g(x + i, y, Glyph::new('▄', RED).solid(RED * 0.55).lit(0.3));
        }
        f.put_g(x + 8, y, Glyph::new('▛', RED).lit(0.3));
        f.put_g(x + 9, y - 1, Glyph::new('▗', RED).lit(0.3));
        // runners
        f.put_g(x, y + 1, Glyph::new('╰', GOLD).lit(0.0));
        for i in 1..9 {
            f.put_g(x + i, y + 1, Glyph::new('━', GOLD).lit(0.0));
        }
        f.put_g(x + 9, y + 1, Glyph::new('╯', GOLD).lit(0.0));
        f.light_cell(x + 1, y, GOLD * (1.5 * glow));
    }
}
