//! Distant mountains, a pine treeline, rolling snowy hills and the
//! foreground snowfield. All static, so it is rendered once into a cached
//! RGBA layer; only the snow glints animate.

use crate::color::{pal, Rgb};
use crate::frame::{Frame, Glyph, Layer};
use crate::rng::{fbm1, h01, hash2, Rng};

/// How fast the far mountains scroll compared with the ground.
pub const PARALLAX: f32 = 0.35;

pub struct Land {
    /// Far ridge, over the (narrower) parallax layer.
    pub mountains: Vec<f32>,
    /// Hills and the snowfield, over the whole world width.
    pub hills: Vec<f32>,
    pub ground: Vec<f32>,
    pub horizon: f32,
    /// Cell row of the distant railway, and the road just below it.
    pub track_row: i32,
    /// Cell rows of the sidewalk through the village (top row).
    pub walk_row: i32,
    /// Powerline poles on the hills: (x, top pixel row, base pixel row).
    pub poles: Vec<(i32, i32, i32)>,
    /// Little cabins up on the mountains (far-layer pixel coords of their
    /// lit window).
    pub cabins: Vec<(f32, f32)>,
    far: Layer,
    cache: Layer,
    glints: Vec<(i32, i32, f32, f32)>,
}

impl Land {
    /// `pw`: world width (px = cells), `screen`: the visible width.
    pub fn new(pw: usize, screen: usize, ph: usize, seed: u32) -> Self {
        let phf = ph as f32;
        let fw = screen + ((pw.saturating_sub(screen)) as f32 * PARALLAX).ceil() as usize + 1;
        let mut mountains = vec![0.0; fw];
        for (x, m) in mountains.iter_mut().enumerate() {
            let xf = x as f32;
            let r = fbm1(xf / 48.0, 4, seed ^ 0x11);
            let peaks = fbm1(xf / 140.0, 2, seed ^ 0x12);
            *m = phf * 0.60 - r.powf(1.4) * phf * 0.20 - peaks * phf * 0.06;
        }
        let mut hills = vec![0.0; pw];
        let mut ground = vec![0.0; pw];
        for x in 0..pw {
            let xf = x as f32;
            hills[x] = phf * 0.675 - fbm1(xf / 85.0, 3, seed ^ 0x13) * phf * 0.07 - (xf / 41.0).sin() * phf * 0.012;
            ground[x] = phf * 0.80 - fbm1(xf / 60.0, 3, seed ^ 0x14) * phf * 0.03;
        }
        let horizon = mountains.iter().cloned().fold(f32::MAX, f32::min).max(1.0);
        let max_hill = hills.iter().cloned().fold(0.0, f32::max);
        let h = ph / 2;
        let track_row = ((max_hill / 2.0).ceil() as i32 + 1).min(h as i32 - 2);
        let walk_row = ((h as f32 * 0.875).round() as i32).min(h as i32 - 3);
        let mut l = Land { mountains, hills, ground, horizon, track_row, walk_row, poles: vec![], cabins: vec![], far: Layer::new(fw, ph), cache: Layer::new(pw, ph), glints: vec![] };
        l.build(seed);
        l
    }

    /// Highest point any land reaches (for keeping the screen-fixed stars
    /// above the scrolling mountains).
    pub fn sky_ceiling(&self, screen: usize) -> Vec<f32> {
        vec![self.horizon; screen]
    }

    fn build(&mut self, seed: u32) {
        let (pw, ph) = (self.cache.w, self.cache.h);
        let fog = pal::SKY_HORIZON;
        // mountains (their own, slower-scrolling layer)
        let fw = self.far.w;
        let layer = &mut self.far;
        for x in 0..fw {
            let pw = fw;
            let top = self.mountains[x];
            let slope = self.mountains[(x + 1).min(pw - 1)] - self.mountains[x.saturating_sub(1)];
            let lit = (0.75 + slope * 0.18).clamp(0.45, 1.2);
            let cap = h01(hash2(x as i32 / 2, seed as i32)) * 4.0 + 3.0;
            for y in (top.floor() as usize)..ph {
                let depth = y as f32 - top;
                let base = if depth < cap { Rgb::hex(0x7d88b4) * lit } else { Rgb::hex(0x2a3158) * (0.85 + 0.15 * lit) };
                let fogk = (depth / 30.0).clamp(0.0, 1.0) * 0.55 + 0.25;
                let a = if depth < 1.0 { (1.0 - (top - top.floor())).clamp(0.0, 1.0) } else { 1.0 };
                layer.blend(x as i32, y as i32, base.lerp(fog, fogk), a);
            }
        }
        // a few cabins tucked up on the mountainsides, windows lit
        let mut crng = Rng::new(seed as u64 ^ 0xcab1);
        let n = (fw / 110).max(2);
        let mut tries = 0;
        while self.cabins.len() < n && tries < 400 {
            tries += 1;
            let x = crng.int(8, fw as i32 - 9) as usize;
            let (l, r) = (self.mountains[x - 5], self.mountains[x + 5]);
            if (l - r).abs() > 3.0 || self.cabins.iter().any(|c| (c.0 - x as f32).abs() < 40.0) {
                continue;
            }
            let y = (self.mountains[x - 3].max(self.mountains[x + 3]) + crng.range(4.0, 9.0)).round() as i32;
            if y as f32 > ph as f32 * 0.55 {
                continue;
            }
            let xi = x as i32;
            let wood = Rgb::hex(0x3a2a26).lerp(fog, 0.35);
            let snow = Rgb::hex(0xdfe6f4).lerp(fog, 0.3);
            for dx in -3..=3 {
                self.far.set(xi + dx, y - 2, if dx.abs() == 3 { snow * 0.8 } else { snow });
            }
            for dx in -2..=2 {
                self.far.set(xi + dx, y - 1, wood);
                self.far.set(xi + dx, y, wood);
            }
            self.far.set(xi + 2, y - 3, Rgb::hex(0x4a3a36).lerp(fog, 0.3)); // chimney
            self.far.set(xi - 1, y, Rgb::hex(0xffc060));
            self.far.set(xi + 1, y, Rgb::hex(0xffb050));
            self.cabins.push((xi as f32, y as f32));
        }
        let layer = &mut self.cache;
        // pine treeline along the hills
        let mut rng = Rng::new(seed as u64 ^ 0x9e7);
        let mut x = 0.0;
        while x < pw as f32 {
            let dense = fbm1(x / 70.0, 2, seed ^ 0x15);
            if dense > 0.40 {
                let col = (x as usize).min(pw - 1);
                let h = rng.range(5.0, 13.0) * (0.6 + dense * 0.6);
                let base = self.hills[col] + rng.range(0.5, 3.0);
                let shade = rng.range(0.7, 1.15);
                let c = Rgb::hex(0x0d1c26).lerp(fog, 0.18) * shade;
                let tip = base - h;
                for y in (tip.floor() as i32)..=(base as i32) {
                    let u = ((y as f32 - tip) / h).max(0.0);
                    let tier = (u * 3.0).fract();
                    let hw = (u * 0.38 + tier * 0.14) * h * 0.55 + 0.3;
                    let (l, r) = ((x - hw).round() as i32, (x + hw).round() as i32);
                    for px in l..=r {
                        layer.set(px, y, c);
                    }
                    if u > 0.2 && tier < 0.3 {
                        layer.set(r, y, Rgb::hex(0x5d6d96) * shade);
                    }
                }
            }
            x += rng.range(1.5, 4.5);
        }
        // hills
        for x in 0..pw {
            let top = self.hills[x];
            let slope = self.hills[(x + 1).min(pw - 1)] - self.hills[x.saturating_sub(1)];
            let lit = (0.85 + slope * 0.35).clamp(0.6, 1.15);
            for y in (top.floor() as usize)..ph {
                let depth = y as f32 - top;
                let n = fbm1(x as f32 / 9.0 + depth * 0.1, 2, seed ^ 0x31);
                let c = Rgb::hex(0x4b5788).lerp(Rgb::hex(0x323c66), (depth / 25.0).min(1.0)) * (lit * (0.92 + 0.12 * n));
                let a = if depth < 1.0 { (1.0 - (top - top.floor())).clamp(0.0, 1.0) } else { 1.0 };
                layer.blend(x as i32, y as i32, c, a);
            }
        }
        // a powerline marching along the hills (subtle; it's an easter egg)
        let pole_c = Rgb::hex(0x1a1620).lerp(fog, 0.22);
        let wire_c = Rgb::hex(0x121218);
        let mut px = rng.int(6, 20);
        while (px as usize) < pw - 2 {
            let base = (self.hills[px as usize] + 3.0) as i32;
            let top = base - 10;
            for y in top..=base {
                layer.blend(px, y, pole_c, 0.8);
            }
            for dx in -1..=1 {
                layer.blend(px + dx, top + 1, pole_c, 0.8);
            }
            self.poles.push((px, top, base));
            px += 26 + rng.int(0, 7);
        }
        for w in self.poles.windows(2) {
            let ((x0, t0, _), (x1, t1, _)) = (w[0], w[1]);
            for k in [-1, 1] {
                let span = (x1 - x0) as f32;
                for x in (x0 + k)..=(x1 + k) {
                    let u = (x - x0 - k) as f32 / span;
                    let y = (t0 + 1) as f32 + (t1 - t0) as f32 * u + 2.5 * 4.0 * u * (1.0 - u);
                    layer.blend(x, y.round() as i32, wire_c, 0.45);
                }
            }
        }
        // foreground snowfield
        for x in 0..pw {
            let top = self.ground[x];
            for y in (top.floor() as usize)..ph {
                let dy = y as f32 - top;
                let depth = dy / (ph as f32 - top).max(1.0);
                let n = fbm1(x as f32 / 14.0 + y as f32 * 0.37, 2, seed ^ 0x41);
                let mut c = pal::SNOW_SHADOW.lerp(pal::SNOW, 0.25 + depth * 0.45) * ((0.93 + 0.1 * n) * 0.78);
                if dy < 1.5 {
                    c = c * 1.12;
                }
                let a = if dy < 1.0 { (1.0 - (top - top.floor())).clamp(0.0, 1.0) } else { 1.0 };
                layer.blend(x as i32, y as i32, c, a);
                let hsh = hash2(x as i32, y as i32 ^ seed as i32);
                if hsh % 89 == 0 && !(y as i32 >= self.walk_row * 2 - 1 && y as i32 <= self.walk_row * 2 + 3) {
                    self.glints.push((x as i32, y as i32, 0.6 + h01(hsh) * 2.4, h01(hsh >> 5) * 60.0));
                }
            }
        }
        // the sidewalk through the village: worn cobbles between snowbanks
        let wy = self.walk_row * 2; // first pixel row of the stones
        for x in 0..pw {
            let xi = x as i32;
            for y in wy..wy + 3 {
                let hsh = hash2(xi / 3 + (y - wy) * 7, y ^ seed as i32);
                // stones in a staggered running bond with darker joints
                let joint = (xi + if (y - wy) % 2 == 0 { 0 } else { 2 }) % 4 == 0;
                let stone = Rgb::hex(0x59607a) * (0.82 + 0.3 * h01(hsh));
                let mut c = if joint { Rgb::hex(0x3a3f55) } else { stone };
                // a dusting of snow here and there
                if h01(hash2(xi, y * 31 ^ seed as i32)) > 0.9 {
                    c = c.lerp(pal::SNOW * 0.8, 0.6);
                }
                layer.set(xi, y, c);
            }
            // snowy kerbs either side, a little lumpy
            let lump = (h01(hash2(xi / 2, seed as i32 ^ 0x77)) * 1.6) as i32;
            for k in 0..=lump {
                layer.set(xi, wy - 1 - k, pal::SNOW * (0.86 - 0.04 * k as f32));
            }
            layer.set(xi, wy + 3, pal::SNOW * 0.7);
        }
        // the distant road, just below the railway: two lanes (one cell row
        // each), cars keep to their side
        let ry = (self.track_row + 1) * 2;
        for x in 0..pw {
            let xi = x as i32;
            for y in ry..ry + 4 {
                let c = Rgb::hex(0x262a3c).lerp(fog, 0.35);
                layer.set(xi, y, c);
            }
            if (xi / 3) % 3 == 0 {
                // faint centre-line dashes between the lanes
                layer.set(xi, ry + 2, Rgb::hex(0x6b6a58).lerp(fog, 0.4));
            }
            layer.set(xi, ry + 4, Rgb::hex(0x56607f).lerp(fog, 0.3));
        }
    }

    /// Is cell (x, cy) (world column) on a powerline pole or wire?
    pub fn on_powerline(&self, x: i32, cy: i32) -> bool {
        let (p0, p1) = (cy * 2 - 1, cy * 2 + 2);
        for w in self.poles.windows(2) {
            let ((x0, t0, b0), (x1, t1, _)) = (w[0], w[1]);
            if (x - x0).abs() <= 1 && p1 >= t0 && p0 <= b0 {
                return true;
            }
            if x > x0 && x < x1 {
                let u = (x - x0) as f32 / (x1 - x0) as f32;
                let y = (t0 + 1) as f32 + (t1 - t0) as f32 * u + 10.0 * u * (1.0 - u);
                return y >= p0 as f32 && y <= p1 as f32;
            }
        }
        false
    }

    /// The mountain cabins' window glow (they're on their own power).
    pub fn draw_lights(&self, f: &mut Frame, cam: f32, t: f32, glow: f32) {
        let off = cam * PARALLAX;
        for (i, &(x, y)) in self.cabins.iter().enumerate() {
            let fl = 0.8 + 0.2 * crate::rng::noise1(t * 3.0 + i as f32 * 7.0, 91);
            f.light_px(x - off + 0.5, y + 0.5, Rgb::hex(0xffb050) * (0.9 * fl * glow));
        }
    }

    /// `cam`: world column at the screen's left edge.
    pub fn draw_px(&self, f: &mut Frame, cam: f32) {
        self.far.composite_from(f, (cam * PARALLAX).round() as i32);
        self.cache.composite_from(f, cam.round() as i32);
    }

    /// Snow glints as tiny sparkle glyphs (only on otherwise empty cells).
    pub fn draw_glints(&self, f: &mut Frame, t: f32) {
        for &(x, y, sp, ph) in &self.glints {
            let tw = (t * sp + ph).sin().max(0.0).powi(10);
            if tw > 0.05 {
                let (cx, cy) = (x, y / 2);
                if !f.has_g(cx, cy) {
                    let ch = if tw > 0.7 { '✧' } else if tw > 0.3 { '·' } else { '.' };
                    f.put_g(cx, cy, Glyph::new(ch, Rgb::hex(0xf2f6ff)).alpha(0.35 + 0.65 * tw).lit(0.5));
                }
            }
        }
    }

    pub fn ground_at(&self, x: f32) -> f32 {
        let i = (x.max(0.0) as usize).min(self.ground.len() - 1);
        self.ground[i]
    }
}
