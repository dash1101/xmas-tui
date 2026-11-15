//! Three-depth snowfall with gusty wind. Far flakes are single pixels behind
//! everything, mid flakes are dim glyphs behind objects, near flakes are big
//! bright glyphs in front of the whole scene.

use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::{fbm1, Rng};

struct Flake {
    x: f32,
    y: f32,
    vy: f32,
    sway: f32,
    freq: f32,
    phase: f32,
    b: f32,
    ch: char,
    land: f32,
}

pub struct Snow {
    far: Vec<Flake>,
    mid: Vec<Flake>,
    near: Vec<Flake>,
    rng: Rng,
    pub wind: f32,
    wind_cfg: f32,
    pw: f32,
    ph: f32,
}

impl Snow {
    pub fn new(pw: usize, ph: usize, density: f32, wind_cfg: f32, seed: u32, ground: &[f32], hills: &[f32]) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x5e0);
        let area = pw as f32 * ph as f32;
        let (pwf, phf) = (pw as f32, ph as f32);
        let mk = |n: usize, layer: u8, rng: &mut Rng| -> Vec<Flake> {
            (0..n)
                .map(|_| {
                    let x = rng.range(0.0, pwf);
                    let col = (x as usize).min(pw - 1);
                    let (vy, ch, b, land) = match layer {
                        0 => (rng.range(4.0, 8.0), '.', rng.range(0.10, 0.28), rng.range(hills[col], ground[col] + 4.0)),
                        1 => (
                            rng.range(9.0, 15.0),
                            *rng.pick(&['·', '·', '•', '∙']),
                            rng.range(0.45, 0.75),
                            rng.range(ground[col], phf),
                        ),
                        _ => (
                            rng.range(18.0, 28.0),
                            *rng.pick(&['❄', '❅', '❆', '*', '✻', '•']),
                            rng.range(0.75, 1.0),
                            phf + 2.0,
                        ),
                    };
                    Flake {
                        x,
                        y: rng.range(-4.0, land),
                        vy,
                        sway: rng.range(0.5, 2.5) * (1.0 + layer as f32 * 0.6),
                        freq: rng.range(0.4, 1.4),
                        phase: rng.range(0.0, 6.28),
                        b,
                        ch,
                        land,
                    }
                })
                .collect()
        };
        let far = mk((area / 240.0 * density) as usize, 0, &mut rng);
        let mid = mk((area / 260.0 * density) as usize, 1, &mut rng);
        let near = mk((area / 1400.0 * density) as usize, 2, &mut rng);
        Snow { far, mid, near, rng, wind: 0.0, wind_cfg, pw: pwf, ph: phf }
    }

    /// `cursor` is (x, y, vx, vy) in pixel units; flakes near it get shoved
    /// along with the pointer and swirl away.
    pub fn update(&mut self, dt: f32, t: f32, extra_wind: f32, cursor: Option<(f32, f32, f32, f32)>) {
        let gust = (fbm1(t * 0.05, 3, 0x91d) - 0.45) * 2.0;
        self.wind = self.wind_cfg * (1.5 + gust * 7.0) + extra_wind;
        let (pw, wind) = (self.pw, self.wind);
        let rng = &mut self.rng;
        for (layer, flakes) in [(0u8, &mut self.far), (1, &mut self.mid), (2, &mut self.near)] {
            let depth = [0.45, 0.8, 1.4][layer as usize];
            for f in flakes.iter_mut() {
                f.y += f.vy * dt;
                f.x += (wind * depth + (t * f.freq + f.phase).cos() * f.sway) * dt;
                if let (Some((cx, cy, vx, vy)), true) = (cursor, layer > 0) {
                    let (dx, dy) = (f.x - cx, f.y - cy);
                    let d2 = dx * dx + dy * dy;
                    let r = 16.0;
                    if d2 < r * r {
                        let d = d2.sqrt().max(0.5);
                        let k = (1.0 - d / r) * dt;
                        // radial push + a swirl + drag along with the pointer
                        f.x += (dx / d * 55.0 - dy / d * 30.0 + vx * 0.5) * k;
                        f.y += (dy / d * 40.0 + dx / d * 20.0 + vy * 0.5) * k;
                    }
                }
                if f.x < -2.0 {
                    f.x += pw + 4.0;
                } else if f.x > pw + 2.0 {
                    f.x -= pw + 4.0;
                }
                if f.y > f.land {
                    f.y = rng.range(-8.0, -1.0);
                    f.x = rng.range(0.0, pw);
                }
            }
        }
    }

    pub fn draw_far(&self, f: &mut Frame) {
        let c = Rgb::hex(0xdfe8ff);
        for fl in &self.far {
            let (cx, cy) = (fl.x as i32, (fl.y / 2.0) as i32);
            if !f.has_g(cx, cy) {
                f.put_g(cx, cy, Glyph::new('.', c).alpha(fl.b * 1.6).lit(0.4));
            }
        }
    }

    pub fn draw_mid(&self, f: &mut Frame) {
        let c = Rgb::hex(0xe4ecff);
        for fl in &self.mid {
            let (cx, cy) = (fl.x as i32, (fl.y / 2.0) as i32);
            if !f.has_g(cx, cy) {
                f.put_g(cx, cy, Glyph::new(fl.ch, c).alpha(fl.b).lit(0.5));
            }
        }
    }

    pub fn draw_near(&self, f: &mut Frame, t: f32) {
        let c = Rgb::hex(0xf2f6ff);
        for fl in &self.near {
            let (cx, cy) = (fl.x as i32, (fl.y / 2.0) as i32);
            let tw = 0.85 + 0.15 * (t * 3.0 + fl.phase).sin();
            f.put_g(cx, cy, Glyph::new(fl.ch, c).alpha(fl.b * tw).lit(0.6));
        }
        let _ = self.ph;
    }
}
