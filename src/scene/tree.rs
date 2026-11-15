//! Decorated Christmas trees (glyph-built, tiered, cone-shaded) with light
//! strings that cycle through patterns, tinsel garlands, ornaments with
//! glints and a pulsing star topper. Plus presents.

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{h01, hash2, hash3, noise1, Rng};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub enum LightStyle {
    /// Classic multicolour, cycles through patterns.
    Multi,
    /// All warm-white fairy lights.
    Warm,
    /// Cool blue/white.
    Icy,
}

struct Bulb {
    x: i32,
    y: i32,
    color: Rgb,
    idx: usize,
    phase: f32,
}

struct Ornament {
    x: i32,
    y: i32,
    ch: char,
    color: Rgb,
    seed: u32,
}

struct TreeCell {
    ch: char,
    fg: Rgb,
    bg: Rgb,
    /// edge cells are half-transparent triangles drawn over the scene
    edge: bool,
}

pub struct Tree {
    pub cx: i32,
    pub top: i32,
    pub hw_base: f32,
    cells: HashMap<(i32, i32), TreeCell>,
    order: Vec<(i32, i32)>,
    garland: Vec<(i32, i32, char, Rgb, f32)>,
    bulbs: Vec<Bulb>,
    ornaments: Vec<Ornament>,
    trunk: Vec<(i32, i32, char)>,
    style: LightStyle,
    big: bool,
    seed: u32,
    /// Clicked: all bulbs blaze and sparkle until this time.
    pub boost_until: f32,
    /// Power cut: dark until this time (then flickers back).
    pub dark_until: f32,
}

impl Tree {
    pub fn new(cx: i32, base: i32, height: i32, style: LightStyle, seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x7ee);
        let big = height >= 30;
        let trunk_h = (height as f32 * 0.07).round().max(1.0) as i32;
        let body_bot = base - trunk_h; // last row of foliage
        let top = base - height + 1;
        let body_h = (body_bot - top + 1) as f32;
        // ~0.62 cells of half-width per row keeps a classic fir silhouette
        let hw_max = body_h * if big { 0.60 } else { 0.56 };
        let tiers = if big { 5 } else if height >= 16 { 4 } else { 3 };

        let hw_at = |y: i32| -> f32 {
            let rel = (y - top) as f32;
            let mut best: f32 = -1.0;
            for i in 0..tiers {
                let fi = i as f32;
                let t0 = (body_h * fi / tiers as f32) * 0.86 - if i == 0 { 0.0 } else { 0.5 };
                let t1 = body_h * (fi + 1.0) / tiers as f32;
                if rel >= t0.floor() && rel <= t1 {
                    let wb = hw_max * (0.35 + 0.65 * (fi + 1.0) / tiers as f32);
                    let wt = if i == 0 { 0.0 } else { wb * 0.42 };
                    let u = ((rel - t0) / (t1 - t0)).clamp(0.0, 1.0);
                    best = best.max(wt + (wb - wt) * u.powf(0.9));
                }
            }
            best
        };

        let mut cells = HashMap::new();
        let mut order = vec![];
        let needle_dark = Rgb::hex(0x09291a);
        let needle = Rgb::hex(0x2f9e55);
        for y in top..=body_bot {
            let hw = hw_at(y);
            if hw < 0.0 {
                continue;
            }
            // tier shadow: top of each tier sits in shadow of the one above
            let rel = (y - top) as f32 / body_h;
            let tier_u = (rel * tiers as f32).fract();
            let shadow = 0.72 + 0.28 * tier_u;
            let l = (cx as f32 - hw).round() as i32;
            let r = (cx as f32 + hw).round() as i32;
            for x in l..=r {
                let n = (x - cx) as f32 / hw.max(0.5);
                let cone = (1.0 - n * n).max(0.0).sqrt();
                let lightdir = 0.12 * n; // moonlight from the right
                let shade = (0.55 + 0.45 * cone + lightdir) * shadow;
                let hsh = hash3(x, y, seed as i32);
                let jit = 0.88 + 0.24 * h01(hsh);
                let bg = needle_dark * (shade * jit);
                let fg = needle * (shade * (0.85 + 0.3 * h01(hsh >> 7)));
                let edge = x == l || x == r;
                let ch = if edge {
                    if x == l { '◢' } else { '◣' }
                } else {
                    match hsh % 11 {
                        0..=5 => '^',
                        6 => '⌃',
                        7 => '\'',
                        8 => ',',
                        9 => '`',
                        _ => '^',
                    }
                };
                // edges: fg is the foliage colour (triangle fill), bg is scene
                let fg = if edge { bg * 1.05 } else { fg };
                cells.insert((x, y), TreeCell { ch, fg, bg, edge });
                order.push((x, y));
            }
        }
        // the very tip
        if let Some(c) = cells.get_mut(&(cx, top)) {
            c.ch = '▲';
            c.edge = true;
            c.fg = needle * 0.9;
        }

        let inside = |cells: &HashMap<(i32, i32), TreeCell>, x: i32, y: i32| -> bool {
            matches!(cells.get(&(x, y)), Some(c) if !c.edge)
        };

        // light strings and garlands: parallel spiral swoops (front side)
        let mut bulbs = vec![];
        let mut garland = vec![];
        let n_strings = (body_h / if big { 5.2 } else { 4.2 }).round().max(2.0) as i32;
        let mut bulb_idx = 0usize;
        let bulb_colors: Vec<Rgb> = match style {
            LightStyle::Multi => pal::BULBS.to_vec(),
            LightStyle::Warm => vec![pal::WARM_WHITE, Rgb::hex(0xffd99a)],
            LightStyle::Icy => vec![Rgb::hex(0x9fd0ff), Rgb::hex(0xe8f4ff), Rgb::hex(0x5aa0ff)],
        };
        let spacing = if big { 3 } else { 2 };
        for s in 0..n_strings {
            let y0 = top as f32 + 2.5 + (s as f32 + 0.3) * (body_h - 3.0) / n_strings as f32;
            let slope = 0.20;
            let sag = 1.1 + rng.f() * 0.6;
            let hw = hw_at(y0 as i32).max(1.0) + 2.0;
            let (xl, xr) = (cx as f32 - hw, cx as f32 + hw);
            let mut k = 0;
            for x in (xl.floor() as i32)..=(xr.ceil() as i32) {
                let u = (x as f32 - xl) / (xr - xl);
                let yf = y0 + (x as f32 - xl) * slope * 0.5 - hw * slope * 0.5 + sag * (std::f32::consts::PI * u).sin();
                let y = yf.round() as i32;
                if !inside(&cells, x, y) {
                    continue;
                }
                if k % spacing == 0 {
                    let color = bulb_colors[bulb_idx % bulb_colors.len()];
                    bulbs.push(Bulb { x, y, color, idx: bulb_idx, phase: rng.range(0.0, 6.28) });
                    bulb_idx += 1;
                }
                k += 1;
            }
            // tinsel garland halfway between this string and the next
            if big || s % 2 == 0 {
                let gy0 = y0 + (body_h - 3.0) / n_strings as f32 * 0.5;
                let ghw = hw_at(gy0 as i32).max(1.0) + 2.0;
                let (gl, gr) = (cx as f32 - ghw, cx as f32 + ghw);
                let gold = s % 2 == 0;
                for x in (gl.floor() as i32)..=(gr.ceil() as i32) {
                    let u = (x as f32 - gl) / (gr - gl);
                    let yf = gy0 + (x as f32 - gl) * slope * 0.5 - ghw * slope * 0.5 + 0.9 * (std::f32::consts::PI * u).sin();
                    let y = yf.round() as i32;
                    if !inside(&cells, x, y) || bulbs.iter().any(|b| b.x == x && b.y == y) {
                        continue;
                    }
                    let frac = yf - yf.floor();
                    let ch = if frac < 0.33 { '⁀' } else if frac > 0.66 { '‿' } else { '~' };
                    let col = if gold { Rgb::hex(0xffd36b) } else { Rgb::hex(0xd8e2f0) };
                    garland.push((x, y, ch, col, rng.range(0.0, 6.28)));
                }
            }
        }

        // ornaments
        let mut ornaments = vec![];
        let area = cells.len() as f32;
        let n_orn = (area / if big { 26.0 } else { 30.0 }) as usize;
        let ornament_set: [(char, Rgb); 8] = [
            ('◉', Rgb::hex(0xe8323c)),
            ('◉', Rgb::hex(0xffc23a)),
            ('●', Rgb::hex(0xc0c8d8)),
            ('◉', Rgb::hex(0x3b7bff)),
            ('●', Rgb::hex(0xb04ad8)),
            ('❄', Rgb::hex(0xe6f0ff)),
            ('✦', Rgb::hex(0xffd36b)),
            ('♦', Rgb::hex(0xff4d5a)),
        ];
        let keys: Vec<(i32, i32)> = order.clone();
        let mut tries = 0;
        while ornaments.len() < n_orn && tries < n_orn * 20 {
            tries += 1;
            let (x, y) = keys[rng.int(0, keys.len() as i32) as usize];
            if !inside(&cells, x, y) || y < top + 2 {
                continue;
            }
            let near = |xx: i32, yy: i32| (xx - x).abs() <= 1 && (yy - y).abs() <= 0;
            if bulbs.iter().any(|b| near(b.x, b.y))
                || garland.iter().any(|g| g.0 == x && g.1 == y)
                || ornaments.iter().any(|o: &Ornament| (o.x - x).abs() < 3 && (o.y - y).abs() < 2)
            {
                continue;
            }
            let (ch, color) = *rng.pick(&ornament_set);
            ornaments.push(Ornament { x, y, ch, color, seed: rng.next_u32() });
        }

        // trunk
        let mut trunk = vec![];
        let tw = (hw_max * 0.13).round().max(1.0) as i32;
        for y in (body_bot + 1)..=base {
            for x in (cx - tw)..=(cx + tw) {
                let ch = if x == cx - tw { '▐' } else if x == cx + tw { '▌' } else { '█' };
                trunk.push((x, y, ch));
            }
        }

        Tree {
            cx,
            top,
            hw_base: hw_max,
            cells,
            order,
            garland,
            bulbs,
            ornaments,
            trunk,
            style,
            big,
            seed,
            boost_until: 0.0,
            dark_until: 0.0,
        }
    }

    /// Brightness 0..1 for a bulb under the current light pattern.
    fn bulb_level(&self, b: &Bulb, t: f32, mode: usize) -> f32 {
        let i = b.idx as f32;
        match mode {
            // twinkle
            0 => 0.35 + 0.65 * noise1(t * 1.7 + b.phase * 3.0, self.seed ^ b.idx as u32).powf(1.5),
            // chase along the string
            1 => {
                let v = ((i / 5.0 - t * 1.4) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                0.15 + 0.85 * v.powi(3)
            }
            // wave sweeping down the tree
            2 => {
                let v = ((b.y - self.top) as f32 * 0.35 - t * 3.2).sin() * 0.5 + 0.5;
                0.2 + 0.8 * v * v
            }
            // slow breathe by colour group
            3 => {
                let v = (t * 1.3 + (b.idx % 5) as f32 * 1.256).sin() * 0.5 + 0.5;
                0.25 + 0.75 * v
            }
            // steady on with random sparkle
            _ => {
                let spark = h01(hash2((t * 6.0) as i32, b.idx as i32 ^ self.seed as i32)) > 0.97;
                if spark { 1.25 } else { 0.85 }
            }
        }
    }

    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        self.boost_until -= s;
        self.dark_until -= s;
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.cells.contains_key(&(x, y))
            || self.trunk.iter().any(|&(tx, ty, _)| tx == x && ty == y)
            || ((y == self.top - 1 || y == self.top - 2) && (x - self.cx).abs() <= 2)
    }

    pub fn star(&self) -> (i32, i32) {
        (self.cx, self.top - 1)
    }

    /// Bottom row (the foot of the trunk).
    pub fn base(&self) -> i32 {
        self.trunk.iter().map(|c| c.1).max().unwrap_or(self.top)
    }

    pub fn is_big(&self) -> bool {
        self.big
    }

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, show: f32) {
        // trunk
        for &(x, y, ch) in &self.trunk {
            f.put_g(x, y, Glyph::new(ch, Rgb::hex(0x3d2414)).lit(0.8));
        }
        // foliage
        for k in &self.order {
            let c = &self.cells[k];
            if c.edge {
                f.put_g(k.0, k.1, Glyph::new(c.ch, c.fg).lit(0.35));
            } else {
                f.put_g(k.0, k.1, Glyph::new(c.ch, c.fg).solid(c.bg).lit(0.3));
            }
        }
        let bg_at = |x: i32, y: i32| -> Bg {
            match self.cells.get(&(x, y)) {
                Some(c) if !c.edge => Bg::Solid(c.bg),
                _ => Bg::Scene,
            }
        };
        // garland shimmer
        for &(x, y, ch, col, ph) in &self.garland {
            let s = 0.55 + 0.45 * (t * 2.2 + x as f32 * 0.55 + ph).sin();
            let spark = if h01(hash3(x, y, (t * 8.0) as i32)) > 0.985 { 1.8 } else { 1.0 };
            f.put_g(x, y, Glyph { ch, fg: col * (s * spark), bg: bg_at(x, y), alpha: 1.0, lit: 0.6, ui: false });
            if spark > 1.0 {
                f.light_cell(x, y, col * 0.8);
            }
        }
        // ornaments
        for o in &self.ornaments {
            let glint = {
                let period = 7.0 + h01(o.seed) * 9.0;
                let p = ((t + h01(o.seed >> 4) * period) % period) / period;
                if p < 0.04 { (1.0 - (p / 0.04 - 0.5).abs() * 2.0).max(0.0) } else { 0.0 }
            };
            let fg = o.color.lerp(Rgb::WHITE, glint * 0.8) * (1.0 + glint * 0.6);
            f.put_g(o.x, o.y, Glyph { ch: o.ch, fg, bg: bg_at(o.x, o.y), alpha: 1.0, lit: 0.7, ui: false });
            if glint > 0.0 {
                f.light_cell(o.x, o.y, Rgb::WHITE * (glint * 1.5 * glow));
            }
        }
        // lights: pattern cycles every 18s with a 2s crossfade
        let (mode_a, mode_b, mix) = if self.style == LightStyle::Multi {
            let period = 18.0;
            let n = 5;
            let ph = t / period;
            let a = (ph.floor() as usize + self.seed as usize) % n;
            let b = (a + 1) % n;
            let fr = ph.fract();
            let mix = ((fr - 0.89) / 0.11).clamp(0.0, 1.0);
            (a, b, mix)
        } else {
            (0, 0, 0.0)
        };
        let bulb_ch = if self.big { '●' } else { '•' };
        for b in &self.bulbs {
            let lv_a = self.bulb_level(b, t, mode_a);
            let lv = if mix > 0.0 { lv_a + (self.bulb_level(b, t, mode_b) - lv_a) * mix } else { lv_a };
            let mut lv = if self.style == LightStyle::Multi { lv } else { 0.55 + 0.45 * lv };
            let mut color = b.color;
            if show > 0.0 {
                // village-wide light show: a rainbow sweeping across the screen
                let rainbow = Rgb::from_hsv(b.x as f32 * 0.012 - t * 0.35, 0.85, 1.0);
                let pulse = 0.35 + 0.65 * ((b.x as f32 * 0.08 - t * 5.0).sin() * 0.5 + 0.5);
                color = color.lerp(rainbow, show);
                lv += (pulse - lv) * show;
            }
            if t < self.boost_until {
                let k = ((self.boost_until - t) / 3.0).clamp(0.0, 1.0);
                let spark = h01(hash3(b.idx as i32, (t * 12.0) as i32, self.seed as i32));
                lv = lv.max(0.9 + 0.5 * spark * k);
            }
            if t < self.dark_until {
                // power's out (the last second: flickering back)
                let left = self.dark_until - t;
                lv = if left < 1.0 && h01(hash3(b.idx as i32, (t * 20.0) as i32, 3)) > left { lv } else { 0.0 };
            }
            let fg = color.lerp(Rgb::WHITE, (lv - 0.75).max(0.0) * 0.9) * (0.35 + 0.95 * lv);
            f.put_g(b.x, b.y, Glyph { ch: bulb_ch, fg, bg: bg_at(b.x, b.y), alpha: 1.0, lit: 0.3, ui: false });
            let e = if self.big { 2.4 } else { 1.8 };
            f.light_cell(b.x, b.y, color * (lv * e * glow));
        }
        // star topper
        let sy = self.top - 1;
        let pulse = (0.8 + 0.2 * (t * 2.1).sin() + 0.08 * (t * 7.3).sin()) * if t < self.dark_until { 0.12 } else { 1.0 };
        let gold = Rgb::hex(0xffd65a);
        f.put_g(self.cx, sy, Glyph::new('★', gold * (1.25 * pulse)).lit(0.0));
        f.light_cell(self.cx, sy, gold * ((if self.big { 9.0 } else { 4.5 }) * pulse * glow));
        if self.big {
            let phase = (t * 1.6) as i32 % 2;
            let ray = gold * (0.75 * pulse);
            let far = gold * (0.4 * pulse);
            if phase == 0 {
                f.put_g(self.cx - 1, sy, Glyph::new('─', ray).lit(0.0));
                f.put_g(self.cx + 1, sy, Glyph::new('─', ray).lit(0.0));
                f.put_g(self.cx - 2, sy, Glyph::new('╶', far).lit(0.0));
                f.put_g(self.cx + 2, sy, Glyph::new('╴', far).lit(0.0));
                f.put_g(self.cx, sy - 1, Glyph::new('│', ray).lit(0.0));
                f.put_g(self.cx, sy - 2, Glyph::new('╷', far).lit(0.0));
            } else {
                f.put_g(self.cx - 1, sy - 1, Glyph::new('╲', ray).lit(0.0));
                f.put_g(self.cx + 1, sy - 1, Glyph::new('╱', ray).lit(0.0));
                f.put_g(self.cx - 1, sy, Glyph::new('·', far).lit(0.0));
                f.put_g(self.cx + 1, sy, Glyph::new('·', far).lit(0.0));
                f.put_g(self.cx - 2, sy - 1, Glyph::new('✧', far).lit(0.0));
                f.put_g(self.cx + 2, sy - 1, Glyph::new('✧', far).lit(0.0));
            }
        }
    }
}

// ---------------------------------------------------------------------------

pub struct Present {
    x: i32,
    base: i32,
    w: i32,
    h: i32,
    body: Rgb,
    ribbon: Rgb,
    pattern: u8,
    seed: u32,
    pub jiggle_until: f32,
}

const BOX_COLORS: [Rgb; 7] = [
    Rgb::hex(0xa8202c),
    Rgb::hex(0x1d6e38),
    Rgb::hex(0x1f4c9a),
    Rgb::hex(0xb8862a),
    Rgb::hex(0x64297f),
    Rgb::hex(0xcfcac0),
    Rgb::hex(0x8a1f4a),
];
const RIBBONS: [Rgb; 4] = [Rgb::hex(0xffd166), Rgb::hex(0xe8eaf2), Rgb::hex(0xff4d4d), Rgb::hex(0x3ad07a)];

impl Present {
    pub fn new(x: i32, base: i32, w: i32, h: i32, rng: &mut Rng) -> Self {
        let body = *rng.pick(&BOX_COLORS);
        let mut ribbon = *rng.pick(&RIBBONS);
        if (ribbon.r - body.r).abs() + (ribbon.g - body.g).abs() < 0.3 {
            ribbon = RIBBONS[0];
        }
        Present { x, base, w, h, body, ribbon, pattern: rng.int(0, 4) as u8, seed: rng.next_u32(), jiggle_until: 0.0 }
    }

    /// Arrange a pile of presents around a tree base.
    pub fn pile(cx: i32, base: i32, spread: i32, count: usize, big: bool, u: f32, rng: &mut Rng) -> Vec<Present> {
        let mut v = vec![];
        for i in 0..count {
            let front = i % 2 == 1;
            let (w, h) = if big { (rng.int(6, 13), rng.int(2, 5)) } else { (rng.int(4, 8), rng.int(2, 3)) };
            let (w, h) = (((w as f32 * u).round() as i32).max(4), ((h as f32 * u).round() as i32).max(2));
            let x = cx + rng.int(-spread, spread) - w / 2;
            let b = base + if front { rng.int(1, 3) } else { 0 };
            v.push(Present::new(x, b, w, h, rng));
        }
        v.sort_by_key(|p| p.base);
        v
    }

    /// The box (rows base-h+1..=base) plus the bow above its centre. The
    /// rest of the bow row is empty, so it mustn't take clicks from the tree
    /// foliage behind it.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let in_box = x >= self.x && x < self.x + self.w && y > self.base - self.h && y <= self.base;
        in_box || (y == self.base - self.h && x == self.x + self.w / 2)
    }

    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        self.jiggle_until -= s;
    }

    pub fn base_row(&self) -> i32 {
        self.base
    }

    pub fn top_center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.base - self.h)
    }

    pub fn draw(&self, f: &mut Frame, t: f32) {
        let wiggle = if t < self.jiggle_until { ((t * 28.0).sin() * 1.2).round() as i32 } else { 0 };
        let (x0, top) = (self.x + wiggle, self.base - self.h + 1 - (wiggle != 0) as i32);
        let rx = x0 + self.w / 2;
        let hy = top + (self.h + 1) / 2;
        let base = top + self.h - 1;
        // soft shadow on the snow
        for x in (x0 - 1)..=(x0 + self.w) {
            for dy in 0..2 {
                let py = base * 2 + 2 + dy;
                let c = f.px_get(x, py);
                f.px_set(x, py, c * (0.6 + 0.2 * dy as f32));
            }
        }
        for y in top..=base {
            for x in x0..(x0 + self.w) {
                let lid = y == top;
                let bottom = y == base;
                let shade = if lid { 1.25 } else if bottom { 0.72 } else { 1.0 };
                let edge_shade = if x == x0 { 0.8 } else if x == x0 + self.w - 1 { 0.9 } else { 1.0 };
                let bg = self.body * (shade * edge_shade);
                let (ch, fg) = if x == rx && y == hy && !lid {
                    ('╋', self.ribbon)
                } else if x == rx {
                    ('┃', self.ribbon * if lid { 1.1 } else { 1.0 })
                } else if y == hy && !lid {
                    ('━', self.ribbon)
                } else if lid {
                    ('▔', self.body * 1.6)
                } else {
                    match self.pattern {
                        0 if (x + y) % 2 == 0 => ('·', self.body * 1.8),
                        1 => ('╱', self.body * 1.25),
                        2 if x % 3 == 0 => ('✦', self.body.lerp(Rgb::WHITE, 0.3)),
                        _ => (' ', bg),
                    }
                };
                f.put_g(x, y, Glyph::new(ch, fg).solid(bg).lit(0.9));
            }
        }
        // bow
        let by = top - 1;
        f.put_g(rx, by, Glyph::new('❀', self.ribbon).lit(0.6));
        // occasional sparkle on the ribbon
        let period = 5.0 + h01(self.seed) * 6.0;
        let p = ((t + h01(self.seed >> 3) * period) % period) / period;
        if p < 0.06 {
            let k = 1.0 - (p / 0.06 - 0.5).abs() * 2.0;
            let sy = top + (h01(self.seed >> 9) * self.h as f32) as i32;
            f.put_g(rx, sy.min(base), Glyph::new('✧', Rgb::WHITE * (0.8 + k)).solid(self.ribbon * 0.6).lit(0.0));
            f.light_cell(rx, sy, Rgb::WHITE * (k * 1.2));
        }
    }
}
