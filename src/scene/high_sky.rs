//! The sky above the sky: Ctrl+wheel looks up past the moon into deep night
//! — the Milky Way, more stars than below, constellations that light up
//! (and name themselves) when clicked, a hot-air balloon drifting past, the
//! North Star, and a satellite blinking across now and then.
//!
//! It lives at negative rows (above row 0 of the normal scene); the frame's
//! vertical offset brings it into view.

use super::events::Director;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::{h01, hash2, noise1, Rng};

struct Constellation {
    name: &'static str,
    /// Star positions in cells (negative rows).
    stars: Vec<(i32, i32)>,
    edges: &'static [(usize, usize)],
    lit_until: f32,
    found: bool,
}

/// (name, star offsets in cells, lines between them)
const SHAPES: &[(&str, &[(i32, i32)], &[(usize, usize)])] = &[
    ("the Big Dipper", &[(0, 0), (9, 1), (16, 4), (23, 6), (25, 12), (36, 12), (37, 5)], &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 3)]),
    ("Orion", &[(0, 0), (16, 1), (6, 8), (8, 9), (10, 10), (2, 18), (15, 17), (8, -4)], &[(0, 2), (1, 4), (2, 3), (3, 4), (2, 5), (4, 6), (0, 7), (1, 7)]),
    ("Cassiopeia", &[(0, 4), (7, 0), (13, 5), (20, 1), (27, 4)], &[(0, 1), (1, 2), (2, 3), (3, 4)]),
    ("Cygnus, the swan", &[(12, 0), (12, 7), (12, 14), (0, 6), (24, 8), (12, 20)], &[(0, 1), (1, 2), (2, 5), (3, 1), (1, 4)]),
    ("the Pleiades", &[(0, 0), (3, 1), (5, -1), (2, 3), (6, 2), (4, 4)], &[]),
];

struct Balloon {
    x: f32,
    y: f32,
    vx: f32,
    stripes: (Rgb, Rgb),
    burn_until: f32,
}

pub struct HighSky {
    w: i32,
    /// How many rows tall it is (it spans rows -depth..0).
    pub depth: i32,
    stars: Vec<(i32, i32, f32, f32)>,
    consts: Vec<Constellation>,
    balloon: Balloon,
    north: (i32, i32),
    sat_at: f32,
    rng: Rng,
}

impl HighSky {
    pub fn new(w: usize, h: usize, seed: u32) -> Self {
        let (w, depth) = (w as i32, (h as f32 * 0.8) as i32);
        let mut rng = Rng::new(seed as u64 ^ 0x5ca1e);
        let mut stars = vec![];
        for _ in 0..(w * depth / 28).max(20) {
            stars.push((rng.int(0, w), -rng.int(1, depth), rng.range(0.0, 6.28), rng.range(0.3, 1.0)));
        }
        // spread the constellations across the width, at varied heights
        let mut consts = vec![];
        let n = SHAPES.len() as i32;
        for (i, &(name, pts, edges)) in SHAPES.iter().enumerate() {
            let x0 = (w as f32 * (0.08 + 0.84 * i as f32 / n as f32)) as i32 + rng.int(0, 12);
            // the shapes extend ~20 rows downward from y0
            let y0 = -rng.int(22, ((depth as f32 * 0.85) as i32).max(23));
            let sx = if w > 300 { 1.4 } else { 1.0 };
            let stars = pts.iter().map(|&(x, y)| (x0 + (x as f32 * sx) as i32, y0 + y)).collect();
            consts.push(Constellation { name, stars, edges, lit_until: 0.0, found: false });
        }
        let colors = [(0xd8283a, 0xf4efe6), (0x2fae5c, 0xffd36b), (0x2a5aa8, 0xf4efe6), (0xffb030, 0xd8283a)];
        let (a, b) = *rng.pick(&colors);
        HighSky {
            w,
            depth,
            stars,
            consts,
            balloon: Balloon { x: w as f32 * 0.3, y: -(depth as f32) * 0.45, vx: 1.2, stripes: (Rgb::hex(a), Rgb::hex(b)), burn_until: 0.0 },
            north: ((w as f32 * 0.82) as i32, -(depth as f32 * 0.6) as i32),
            sat_at: 30.0,
            rng,
        }
    }

    pub fn rebase(&mut self, s: f32) {
        for c in &mut self.consts {
            c.lit_until -= s;
        }
        self.balloon.burn_until -= s;
        self.sat_at -= s;
    }

    pub fn update(&mut self, dt: f32, t: f32) {
        let b = &mut self.balloon;
        b.x += b.vx * dt;
        // gently bobbing; a burn lifts it, then it drifts back down
        let target = -(self.depth as f32) * 0.45 + (t * 0.2).sin() * 2.0;
        b.y += if t < b.burn_until { -2.5 * dt } else { (target - b.y) * 0.3 * dt };
        if b.x > self.w as f32 + 10.0 {
            b.x = -10.0;
        }
        if t > self.sat_at + 40.0 {
            self.sat_at = t + self.rng.range(20.0, 60.0);
        }
    }

    /// A click up here (scene cell coords, y < 0). Returns the cheer it's
    /// worth (finding a constellation for the first time).
    pub fn click(&mut self, x: i32, y: i32, t: f32, d: &mut Director, cam: f32) -> u32 {
        let bx = self.balloon.x.round() as i32;
        let by = self.balloon.y.round() as i32;
        if (x - bx).abs() <= 3 && y >= by - 3 && y <= by + 4 {
            self.balloon.burn_until = t + 2.5;
            let line = *self.rng.pick(&["hello down there!", "what a view!", "merry Christmas from up here!", "is that Santa?", "whoosh!"]);
            d.say(bx + cam as i32, by - 4, line, t + 0.3, 3.0, 6);
            d.sound(Sfx::Puff, bx as f32 + cam, 0.7);
            return 0;
        }
        if (x - self.north.0).abs() <= 2 && (y - self.north.1).abs() <= 1 {
            d.say(self.north.0 + cam as i32, self.north.1 + 1, "✦ the North Star: make a wish ✦", t, 3.5, 6);
            d.burst(self.north.0 as f32 + cam, self.north.1 as f32, 24, &[Rgb::hex(0xfff2c0), Rgb::WHITE], 7.0, 1.0);
            d.sound(Sfx::Magic, self.north.0 as f32 + cam, 0.8);
            return 0;
        }
        for c in &mut self.consts {
            if c.stars.iter().any(|&(sx, sy)| (sx - x).abs() <= 2 && (sy - y).abs() <= 1) {
                c.lit_until = t + 9.0;
                d.sound(Sfx::Chime, x as f32 + cam, 0.7);
                let first = !c.found;
                c.found = true;
                return first as u32;
            }
        }
        0
    }

    pub fn clickable(&self, x: i32, y: i32) -> bool {
        let (bx, by) = (self.balloon.x.round() as i32, self.balloon.y.round() as i32);
        ((x - bx).abs() <= 3 && y >= by - 3 && y <= by + 4)
            || ((x - self.north.0).abs() <= 2 && (y - self.north.1).abs() <= 1)
            || self.consts.iter().any(|c| c.stars.iter().any(|&(sx, sy)| (sx - x).abs() <= 2 && (sy - y).abs() <= 1))
    }

    /// Drawn with the frame's `ox = 0` (it stays put as the village scrolls)
    /// and `oy` = how far the view has been lifted.
    /// `off`: the sky's slow drift as the village scrolls.
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, bottom: Rgb, day: f32, off: i32) {
        if f.oy <= 0 {
            return;
        }
        let top = Rgb::hex(0x010208);
        let pd = self.depth * 2;
        // only the rows actually in view
        let first = (-2 * f.oy).max(-pd);
        // fade into the actual sky just below, column by column (no seam)
        let below: Vec<Rgb> = (0..self.w).map(|x| f.px_get(x, 0)).collect();
        let _ = bottom;
        for py in first..0 {
            let k = (py + pd) as f32 / pd as f32; // 0 at the very top
            for x in 0..self.w {
                let base = top.lerp(below[x as usize], k.powf(1.6));
                // the Milky Way: a faint, mottled diagonal band
                let band = ((x as f32 * 0.35 + py as f32 * 0.9) / 60.0).sin().abs();
                let mottle = noise1(x as f32 * 0.08 + py as f32 * 0.13, 71);
                let mw = ((1.0 - band).powf(5.0) * (0.4 + 0.6 * mottle)).clamp(0.0, 1.0);
                let dust = if h01(hash2(x, py)) > 0.995 { 0.25 } else { 0.0 };
                // (the band and the dust fade out toward the normal sky below)
                let fade = ((1.0 - k) * 5.0).clamp(0.0, 1.0);
                f.px_set(x, py, base + (Rgb::hex(0x404a78) * (mw * 0.35) + Rgb::hex(0xc8d0ff) * dust) * fade);
            }
        }
        // stars (only at night)
        let stars = if day < 0.45 { &self.stars[..] } else { &[] };
        for &(x, y, ph, b) in stars {
            if y < -f.oy {
                continue;
            }
            let x = (x - off).rem_euclid(self.w.max(1));
            let tw = 0.6 + 0.4 * (t * 1.7 + ph).sin();
            let ch = if b > 0.85 { '✦' } else if b > 0.6 { '·' } else { '.' };
            f.put_g(x, y, Glyph::new(ch, Rgb::hex(0xe8ecff) * (b * tw)).lit(0.0));
        }
        // constellations: always faintly there; clicked ones light up and
        // say their name
        for c in &self.consts {
            let lit = ((c.lit_until - t) / 1.5).clamp(0.0, 1.0);
            if lit > 0.0 {
                for &(a, b) in c.edges {
                    let ((x0, y0), (x1, y1)) = (c.stars[a], c.stars[b]);
                    let (px0, py0, px1, py1) = (x0 as f32, y0 as f32 * 2.0 + 1.0, x1 as f32, y1 as f32 * 2.0 + 1.0);
                    let n = ((px1 - px0).abs().max((py1 - py0).abs()) as i32).max(1);
                    for i in 0..=n {
                        let u = i as f32 / n as f32;
                        let (px, py) = (px0 + (px1 - px0) * u, py0 + (py1 - py0) * u);
                        f.px_blend(px.round() as i32, py.round() as i32, Rgb::hex(0xffd88a), 0.55 * lit);
                    }
                }
                let (lx, ly) = c.stars.iter().fold((i32::MAX, i32::MIN), |(mx, my), &(x, y)| (mx.min(x), my.max(y)));
                f.text(lx, ly + 2, c.name, Rgb::hex(0xffd88a) * lit, crate::frame::Bg::Scene, 0.0);
            }
            for &(x, y) in &c.stars {
                let b = 0.75 + 0.25 * lit;
                f.put_g(x, y, Glyph::new('✦', Rgb::hex(0xfff6dc) * (1.15 * b)).lit(0.0));
                f.light_px(x as f32 + 0.5, y as f32 * 2.0 + 1.0, Rgb::hex(0xfff0c8) * ((0.9 + lit * 1.5) * glow));
            }
        }
        // the North Star
        let (nx, ny) = self.north;
        let pulse = 0.85 + 0.15 * (t * 1.3).sin();
        f.put_g(nx, ny, Glyph::new('✷', Rgb::hex(0xfff8e0) * (1.3 * pulse)).lit(0.0));
        f.light_px(nx as f32 + 0.5, ny as f32 * 2.0 + 1.0, Rgb::hex(0xfff0c8) * (4.0 * pulse * glow));
        // a satellite crossing
        let age = t - self.sat_at;
        if (0.0..40.0).contains(&age) {
            let sx = (age / 40.0 * (self.w + 20) as f32) as i32 - 10;
            let sy = -self.depth / 3 - (age * 0.15) as i32;
            if (t * 2.0) as i32 % 2 == 0 {
                f.put_g(sx, sy, Glyph::new('•', Rgb::hex(0xff5040)).lit(0.0));
            } else {
                f.put_g(sx, sy, Glyph::new('·', Rgb::hex(0xd8e0ff)).lit(0.0));
            }
        }
        // the balloon
        let b = &self.balloon;
        let (bx, by) = (b.x.round() as i32, b.y.round() as i32);
        let rows: [&str; 5] = [" ▄██▄ ", "▐████▌", " ▀██▀ ", "  ╲╱  ", "  ▣  "];
        for (ry, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let x = bx - 3 + i as i32;
                let col = match ry {
                    0..=2 => {
                        if (i as i32 + ry as i32) % 2 == 0 {
                            b.stripes.0
                        } else {
                            b.stripes.1
                        }
                    }
                    3 => Rgb::hex(0x6a5a4a),
                    _ => Rgb::hex(0x8a5a2a),
                };
                f.put_g(x, by - 3 + ry as i32, Glyph::new(ch, col).lit(0.5));
            }
        }
        if t < b.burn_until {
            let fl = 0.7 + 0.3 * noise1(t * 20.0, 9);
            f.put_g(bx, by + 0, Glyph::new('▲', Rgb::hex(0xffb030) * (1.3 * fl)).lit(0.0));
            f.light_px(bx as f32 + 0.5, by as f32 * 2.0, Rgb::hex(0xff9030) * (5.0 * fl * glow));
        } else {
            f.light_px(bx as f32 + 0.5, (by - 2) as f32 * 2.0, Rgb::hex(0xffc060) * (0.8 * glow));
        }
    }
}
