//! Night sky: gradient, twinkling stars, moon with halo, aurora, shooting stars.

use crate::color::{pal, Rgb};
use crate::frame::{Frame, Glyph};
use crate::rng::{fbm1, h01, hash2, noise1, Rng};

struct Star {
    x: f32,
    y: f32,
    b: f32,
    speed: f32,
    phase: f32,
    tint: Rgb,
    glyph: Option<char>,
}

struct Shooting {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
}

pub struct Sky {
    seed: u32,
    stars: Vec<Star>,
    pub moon: (f32, f32, f32),
    shooting: Option<Shooting>,
    next_shoot: f32,
    pub aurora: bool,
    town_x: f32,
    base: Vec<Rgb>,
    horizon: f32,
}

impl Sky {
    /// `ceiling(x)` = highest y (px) any land reaches at column x, so stars stay above it.
    pub fn new(pw: usize, ph: usize, seed: u32, aurora: bool, town_x: f32, ceiling: &[f32], horizon: f32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x5717);
        let mr = (ph as f32 * 0.075).max(4.0);
        // keep the moon clear of the default right-hand widget column
        let mx = (pw as f32 * 0.775).min(pw as f32 - 72.0 - mr).max(pw as f32 * 0.6);
        let moon = (mx, ph as f32 * 0.19, mr);
        let area = pw as f32 * ph as f32 * 0.45;
        let n = (area / 55.0) as usize;
        let mut stars = Vec::with_capacity(n);
        for _ in 0..n {
            let x = rng.range(0.0, pw as f32);
            let y = rng.range(0.0, ph as f32 * 0.5);
            let col = (x as usize).min(pw - 1);
            if y > ceiling[col] - 3.0 {
                continue;
            }
            let (dx, dy) = (x - moon.0, y - moon.1);
            if (dx * dx + dy * dy).sqrt() < moon.2 * 2.2 {
                continue;
            }
            let r = rng.f();
            let b = 0.12 + r.powf(3.0) * 0.9;
            let tint = match rng.int(0, 10) {
                0 | 1 => Rgb::hex(0xbcd4ff),
                2 => Rgb::hex(0xffe2b0),
                _ => Rgb::hex(0xeef2ff),
            };
            let glyph = if r > 0.955 && y < ceiling[col] - 6.0 {
                Some(*rng.pick(&['✦', '✧', '·', '+', '⋆', '✦', '·']))
            } else {
                None
            };
            stars.push(Star { x, y, b, speed: rng.range(0.6, 2.6), phase: rng.range(0.0, 6.28), tint, glyph });
        }
        let mut sky = Sky {
            seed,
            stars,
            moon,
            shooting: None,
            next_shoot: rng.range(6.0, 20.0),
            aurora,
            town_x,
            base: vec![],
            horizon,
        };
        sky.build_base(pw, ph);
        sky
    }

    /// Gradient + moon never change: render them once.
    fn build_base(&mut self, pw: usize, ph: usize) {
        let mut f = Frame::new(pw, ph / 2);
        let horizon = self.horizon;
        for y in 0..ph {
            let v = (y as f32 / horizon).min(1.2);
            let base = if v < 0.45 {
                pal::SKY_TOP.lerp(pal::SKY_MID, v / 0.45)
            } else if v < 0.8 {
                pal::SKY_MID.lerp(pal::SKY_LOW, (v - 0.45) / 0.35)
            } else {
                pal::SKY_LOW.lerp(pal::SKY_HORIZON, (v - 0.8) / 0.2)
            };
            let warm_v = ((v - 0.55) / 0.45).clamp(0.0, 1.0);
            for x in 0..pw {
                let dx = (x as f32 - self.town_x) / (pw as f32 * 0.22);
                let warm = (-dx * dx).exp() * warm_v * warm_v * 0.35;
                f.px[y * pw + x] = base + pal::TOWN_GLOW * warm;
            }
        }
        self.draw_moon(&mut f);
        self.base = f.px;
    }

    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        self.next_shoot -= s;
    }

    pub fn update(&mut self, dt: f32, t: f32, pw: usize, ph: usize) {
        if let Some(s) = &mut self.shooting {
            s.age += dt;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            if s.age > s.life {
                self.shooting = None;
            }
        } else if t > self.next_shoot {
            let mut rng = Rng::new((t * 1000.0) as u64 ^ self.seed as u64);
            let dir = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let speed = rng.range(150.0, 240.0);
            let ang = rng.range(0.18, 0.45);
            self.shooting = Some(Shooting {
                x: rng.range(pw as f32 * 0.15, pw as f32 * 0.85),
                y: rng.range(2.0, ph as f32 * 0.18),
                vx: dir * speed * ang.cos(),
                vy: speed * ang.sin(),
                age: 0.0,
                life: rng.range(0.6, 1.1),
            });
            self.next_shoot = t + rng.range(12.0, 40.0);
        }
    }

    /// Pixel-layer sky (call first).
    /// The colour at the very top of the sky (where the high sky above
    /// fades into it).
    pub fn top_color(&self) -> Rgb {
        let n = self.base.len().min(64).max(1);
        let mut c = Rgb::BLACK;
        for p in &self.base[..n] {
            c += *p;
        }
        c * (1.0 / n as f32)
    }

    /// `off`: how far the sky has drifted left (a slow parallax as the
    /// village scrolls); it wraps round.
    pub fn draw_px(&self, f: &mut Frame, t: f32, boost: f32, off: i32) {
        if self.base.len() == f.px.len() {
            // shifted down while looking up into the sky, and drifted
            let pw = f.pw;
            let shift = (2 * f.oy).clamp(0, f.ph as i32) as usize;
            let rot = off.rem_euclid(pw.max(1) as i32) as usize;
            for y in 0..f.ph - shift {
                let (src, dst) = (&self.base[y * pw..(y + 1) * pw], (y + shift) * pw);
                f.px[dst..dst + pw - rot].copy_from_slice(&src[rot..]);
                f.px[dst + pw - rot..dst + pw].copy_from_slice(&src[..rot]);
            }
        }
        let horizon = self.horizon;
        if self.aurora {
            self.draw_aurora(f, t, horizon, boost, off);
        }

        if let Some(s) = &self.shooting {
            let env = (std::f32::consts::PI * (s.age / s.life)).sin().max(0.0);
            let len = 22.0;
            let speed = (s.vx * s.vx + s.vy * s.vy).sqrt();
            let (dx, dy) = (s.vx / speed, s.vy / speed);
            for i in 0..(len as i32 * 2) {
                let d = i as f32 * 0.5;
                let a = (1.0 - d / len).powf(1.6) * env;
                f.px_add((s.x - dx * d) as i32, (s.y - dy * d) as i32, Rgb::hex(0xfff6dc) * (a * 0.9));
            }
            f.light_px(s.x, s.y, Rgb::hex(0xfff0c0) * (env * 4.0));
        }
    }

    fn draw_aurora(&self, f: &mut Frame, t: f32, horizon: f32, boost: f32, off: i32) {
        let pw = f.pw;
        let green = Rgb::hex(0x2ef59a);
        let teal = Rgb::hex(0x2bc4d8);
        let violet = Rgb::hex(0x8a5cff);
        for x in 0..pw {
            let xf = (x as i32 + off) as f32;
            let band = fbm1(xf * 0.006 - t * 0.025, 3, self.seed ^ 0xa0);
            let band = ((band - 0.42 + boost * 0.3) / 0.3).clamp(0.0, 1.0);
            if band <= 0.0 {
                continue;
            }
            let rays = 0.55 + 0.45 * noise1(xf * 0.22 + t * 0.35, self.seed ^ 0xa1);
            let base = horizon * 0.62 + (fbm1(xf * 0.009 + t * 0.02, 3, self.seed ^ 0xa2) - 0.5) * horizon * 0.35;
            let len = horizon * (0.25 + 0.25 * fbm1(xf * 0.013 - t * 0.015, 2, self.seed ^ 0xa3));
            let strength = band * rays * 0.22 * (1.0 + boost * 1.8);
            let y0 = (base - len).max(0.0) as i32;
            for y in y0..=(base as i32 + 6) {
                let d = base - y as f32; // px above the curtain's lower edge
                let u = d / len; // 0 bottom .. 1 top
                let a = if d < 0.0 {
                    (1.0 + d / 6.0).max(0.0).powi(2)
                } else {
                    (1.0 - u).max(0.0).powf(1.4)
                };
                let col = if u < 0.45 { green.lerp(teal, u / 0.45) } else { teal.lerp(violet, (u - 0.45) / 0.55) };
                f.px_add(x as i32, y, col * (a * strength));
            }
        }
    }

    fn draw_moon(&self, f: &mut Frame) {
        let (mx, my, r) = self.moon;
        let glow = Rgb::hex(0x7f8fc0);
        let reach = r * 7.0;
        let (x0, x1) = ((mx - reach) as i32, (mx + reach) as i32);
        let (y0, y1) = ((my - reach) as i32, (my + reach) as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - mx, y as f32 + 0.5 - my);
                let d = (dx * dx + dy * dy).sqrt() / r;
                if d > 1.0 {
                    let g = 0.22 / (d * d) - 0.22 / 49.0;
                    if g > 0.0 {
                        f.px_add(x, y, glow * g);
                    }
                } else {
                    let limb = 0.82 + 0.18 * (1.0 - d * d).sqrt();
                    let crater = crate::rng::fbm2(dx / r * 2.2 + 3.0, dy / r * 2.2, 3, self.seed ^ 0x300);
                    let shade = limb * (0.68 + 0.36 * crater);
                    let c = Rgb::hex(0xf6efd6) * shade;
                    let edge = ((1.0 - d) * r).clamp(0.0, 1.0);
                    f.px_blend(x, y, c, edge);
                }
            }
        }
    }

    /// Glyph-layer stars (bright ones get real characters + a small glow).
    pub fn draw_glyphs(&self, f: &mut Frame, t: f32, off: i32) {
        let w = f.w.max(1) as i32;
        for s in &self.stars {
            let (cx, cy) = ((s.x as i32 - off).rem_euclid(w), (s.y / 2.0) as i32);
            let Some(ch) = s.glyph else {
                if f.has_g(cx, cy) {
                    continue;
                }
                let tw = 0.55 + 0.45 * (t * s.speed + s.phase).sin();
                let fl = noise1(t * 3.0 + s.phase * 10.0, 7);
                let k = (0.35 + s.b) * tw * (0.8 + 0.4 * fl);
                let ch = if s.b < 0.16 { '.' } else if s.b < 0.3 { '·' } else if s.b < 0.55 { '∙' } else { '+' };
                f.put_g(cx, cy, Glyph::new(ch, s.tint * k.min(1.3)).lit(0.1));
                continue;
            };
            let tw = 0.5 + 0.5 * (t * s.speed * 0.7 + s.phase).sin();
            let spark = if h01(hash2((t * 2.0) as i32, (s.x * 7.0) as i32)) > 0.985 { 1.6 } else { 1.0 };
            let k = (s.b * 0.6 + 0.4) * (0.45 + 0.55 * tw) * spark;
            f.put_g(cx, cy, Glyph::new(ch, s.tint * k).lit(0.2));
            f.light_cell(cx, cy, s.tint * (k * 0.35));
        }
    }
}
