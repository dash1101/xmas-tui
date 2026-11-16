//! Village: half-block pixel-art houses and a church (cached), with glyph
//! windows that flicker like firelight, roofline lights, wreaths, chimney
//! smoke, lamp posts, a fence and a snowman.

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph, Layer};
use crate::rng::{h01, hash2, noise1, Rng};
use std::collections::HashSet;

pub struct House {
    pub x: i32,
    pub base: i32,
    pub w: i32,
    pub wall_h: i32,
    pub roof_h: i32,
    pub wall: Rgb,
    pub roof: Rgb,
    pub church: bool,
    pub seed: u32,
    windows: Vec<(i32, i32)>,
    door: i32,
    chimney: Option<i32>,
    eave_colors: Vec<Rgb>,
    /// Lights are off until this time (clicks / random events).
    pub lights_off_until: f32,
    /// Christmas Eve night: everyone's in bed (windows dark, but for the
    /// odd night-light).
    pub asleep: bool,
    /// A carrot left out on the step for the reindeer.
    pub carrot: bool,
    /// The villager's own house (lit rooms upstairs, a flag by the door).
    pub home: bool,
    /// Cells actually covered by the art (walls, roof, chimney, steeple,
    /// cross), for hit-testing and the train's occlusion. `bounds()` is a
    /// loose rectangle that's mostly empty sky around the roof/steeple.
    mask: HashSet<(i32, i32)>,
}

struct Smoke {
    x: f32,
    y: f32,
    age: f32,
    life: f32,
    drift: f32,
}

pub struct Lamp {
    pub x: i32,
    pub base: i32,
    pub h: i32,
    pub off_until: f32,
}

pub struct Snowman {
    pub cx: i32,
    pub base: i32,
    /// Size relative to the 75-row design.
    pub scale: f32,
}

pub struct Village {
    pub houses: Vec<House>,
    pub lamps: Vec<Lamp>,
    pub fences: Vec<(i32, i32, i32)>,
    pub snowmen: Vec<Snowman>,
    cache: Layer,
    smoke: Vec<Smoke>,
    smoke_timer: f32,
    rng: Rng,
}

impl House {
    pub fn new(x: i32, base: i32, w: i32, wall_h: i32, church: bool, rng: &mut Rng) -> Self {
        let walls = [Rgb::hex(0x5a3423), Rgb::hex(0x4a3a30), Rgb::hex(0x6b4a2e), Rgb::hex(0x3d4a5a), Rgb::hex(0x6a2f2a)];
        let roofs = [Rgb::hex(0x7a1f26), Rgb::hex(0x2f3b52), Rgb::hex(0x3a2a22), Rgb::hex(0x1f4a3a)];
        let wall = if church { Rgb::hex(0x55556a) } else { *rng.pick(&walls) };
        let roof = if church { Rgb::hex(0x2a2f45) } else { *rng.pick(&roofs) };
        let roof_h = if church { (w as f32 * 0.35) as i32 } else { (w as f32 * 0.30).round() as i32 }.max(2);
        // windows: second wall row, pairs of 2 cells, leaving room for the door
        let door = x + w / 2 + if w > 10 { rng.int(-w / 4, w / 4) } else { 0 };
        let mut windows = vec![];
        let wy = base - wall_h + 2;
        let mut cx = x + 2;
        while cx + 1 < x + w - 1 {
            if (cx - door).abs() > 1 && (cx + 1 - door).abs() > 1 {
                windows.push((cx, wy));
                windows.push((cx + 1, wy));
                if wall_h >= 5 && !church {
                    windows.push((cx, wy + 2));
                    windows.push((cx + 1, wy + 2));
                }
            }
            cx += if church { 3 } else { 4 };
        }
        let chimney = if church { None } else { Some(x + w / 4 + rng.int(0, w / 2)) };
        let eave_colors = if rng.chance(0.5) {
            pal::BULBS.to_vec()
        } else {
            vec![pal::WARM_WHITE]
        };
        let mut h = House {
            x,
            base,
            w,
            wall_h,
            roof_h,
            wall,
            roof,
            church,
            seed: rng.next_u32(),
            windows,
            door,
            chimney,
            eave_colors,
            lights_off_until: 0.0,
            asleep: false,
            carrot: false,
            home: false,
            mask: HashSet::new(),
        };
        // a cell counts if either of its half-block pixels is painted
        let mut mask = HashSet::new();
        h.paint(&mut |x, y, _| {
            mask.insert((x, y.div_euclid(2)));
        });
        if let Some(cross) = h.steeple() {
            mask.insert(cross);
        }
        h.mask = mask;
        h
    }

    /// Is cell (x, y) part of the house's visible silhouette?
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.mask.contains(&(x, y))
    }

    /// The top edge of the silhouette in each column, in rows (a roof
    /// whose top cell is only half painted tops out half a row lower).
    pub fn tops(&self) -> std::collections::BTreeMap<i32, f32> {
        let mut px = std::collections::BTreeMap::<i32, i32>::new();
        self.paint(&mut |x, y, _| {
            let t = px.entry(x).or_insert(y);
            *t = (*t).min(y);
        });
        if let Some((cx, cy)) = self.steeple() {
            let t = px.entry(cx).or_insert(cy * 2);
            *t = (*t).min(cy * 2);
        }
        px.into_iter().map(|(x, y)| (x, y as f32 / 2.0)).collect()
    }

    /// Every cell of the silhouette (see `contains`).
    pub fn cells(&self) -> impl Iterator<Item = &(i32, i32)> {
        self.mask.iter()
    }

    /// (x, top_row, w, h) covering roof/steeple to the ground.
    /// Column of the front door.
    pub fn door(&self) -> i32 {
        self.door
    }

    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        let extra = if self.church { 13 } else { 2 };
        let top = self.base - self.wall_h - self.roof_h - extra;
        (self.x - 2, top, self.w + 4, self.base - top + 1)
    }

    /// Where the church cross sits (bell notes rise from here).
    pub fn steeple(&self) -> Option<(i32, i32)> {
        if !self.church {
            return None;
        }
        let rp = self.roof_h * 2;
        Some((self.x + self.w / 2, (self.wall_top_px() - rp - 10 - 14) / 2 - 1))
    }

    fn wall_top_px(&self) -> i32 {
        (self.base - self.wall_h + 1) * 2
    }

    fn render(&self, l: &mut Layer) {
        self.paint(&mut |x, y, c| l.set(x, y, c));
    }

    /// Emit every pixel of the house art; shared by `render` and the mask so
    /// the hit area always matches what's drawn.
    fn paint(&self, l: &mut dyn FnMut(i32, i32, Rgb)) {
        let wt = self.wall_top_px();
        let wb = self.base * 2 + 1;
        // walls with log/stone courses, darker on the left
        for y in wt..=wb {
            for x in self.x..(self.x + self.w) {
                let course = if self.church {
                    if (y + (x / 3) % 2) % 3 == 0 { 0.85 } else { 1.0 }
                } else if y % 2 == 0 {
                    1.0
                } else {
                    0.8
                };
                let side = 0.82 + 0.18 * ((x - self.x) as f32 / self.w as f32);
                l(x, y, self.wall * (course * side));
            }
        }
        // snow drift against the wall
        for x in self.x..(self.x + self.w) {
            let d = (h01(hash2(x, self.seed as i32)) * 1.6) as i32;
            for y in (wb - d)..=wb {
                l(x, y, pal::SNOW * 0.72);
            }
        }
        // roof triangle with overhang + snow along the top surface
        let c = self.x as f32 + (self.w as f32 - 1.0) / 2.0;
        let rp = self.roof_h * 2;
        let half_base = self.w as f32 / 2.0 + 1.5;
        let mut first = std::collections::HashMap::new();
        for k in 0..rp {
            let y = wt - rp + k;
            let hw = half_base * (k + 1) as f32 / rp as f32;
            for x in ((c - hw).round() as i32)..=((c + hw).round() as i32) {
                let e = first.entry(x).or_insert(y);
                let depth = y - *e;
                let shingle = if (y + (x / 2) % 2) % 2 == 0 { 1.0 } else { 0.85 };
                // snow along the ridge, and flecks of it down the roof (dimmer
                // lower down, never below a soft grey: tall roofs used to
                // go negative here, black specks)
                let col = if depth < 2 || h01(hash2(x, y ^ self.seed as i32)) > 0.93 {
                    pal::SNOW * (0.82 - depth.min(6) as f32 * 0.06)
                } else {
                    self.roof * shingle
                };
                l(x, y, col);
            }
        }
        // chimney
        if let Some(chx) = self.chimney {
            let ytop = wt - rp - 1 + ((chx as f32 - c).abs() / half_base * rp as f32) as i32 - 3;
            for y in ytop..(wt - rp + ((chx as f32 - c).abs() / half_base * rp as f32) as i32 + 2) {
                for x in chx..(chx + 2) {
                    l(x, y, Rgb::hex(0x5a2a22) * if y % 2 == 0 { 1.0 } else { 0.8 });
                }
            }
            l(chx, ytop, pal::SNOW * 0.8);
            l(chx + 1, ytop, pal::SNOW * 0.8);
        }
        // church steeple
        if self.church {
            let tw = 4;
            let tx = c.round() as i32 - tw / 2;
            let tower_top = wt - rp - 10;
            for y in tower_top..(wt - rp + 2) {
                for x in tx..(tx + tw) {
                    l(x, y, self.wall * if y % 3 == 0 { 0.85 } else { 0.95 });
                }
            }
            let sp = 14;
            for k in 0..sp {
                let y = tower_top - sp + k;
                let hw = (tw as f32 / 2.0 + 0.5) * (k + 1) as f32 / sp as f32;
                for x in ((c - hw).round() as i32)..=((c + hw).round() as i32) {
                    l(x, y, if k < 2 { pal::SNOW * 0.8 } else { self.roof });
                }
            }
        }
    }

    /// Top of the chimney: (column, pixel row). None for the church.
    pub fn chimney_top(&self) -> Option<(f32, f32)> {
        let chx = self.chimney?;
        let c = self.x as f32 + (self.w as f32 - 1.0) / 2.0;
        let half_base = self.w as f32 / 2.0 + 1.5;
        let rp = self.roof_h * 2;
        let ytop = self.wall_top_px() - rp - 1 + ((chx as f32 - c).abs() / half_base * rp as f32) as i32 - 3;
        Some((chx as f32 + 1.0, ytop as f32 - 0.5))
    }

    fn draw_glyphs(&self, f: &mut Frame, t: f32, glow: f32) {
        let wt_row = self.base - self.wall_h + 1;
        // roofline lights
        if !self.church && t >= self.lights_off_until {
            for (i, x) in (self.x..(self.x + self.w)).enumerate() {
                if i % 2 == 0 {
                    let col = self.eave_colors[(i / 2) % self.eave_colors.len()];
                    let tw = 0.55 + 0.45 * noise1(t * 1.3 + i as f32 * 0.7, self.seed);
                    f.put_g(x, wt_row, Glyph::new('•', col * (0.5 + 0.7 * tw)).lit(0.2));
                    f.light_cell(x, wt_row, col * (1.3 * tw * glow));
                } else {
                    f.put_g(x, wt_row, Glyph::new('‿', Rgb::hex(0x1a1a1a)).lit(0.2));
                }
            }
        }
        // windows: firelight flicker, some go dark for a while
        for (i, &(x, y)) in self.windows.iter().enumerate() {
            let pair = (i / 2) as i32;
            let epoch = (t / 90.0) as i32;
            // asleep: dark, but one house in four leaves a night-light on
            let night_light = pair == 0 && self.seed % 4 == 0;
            let on = h01(hash2(pair ^ self.seed as i32, epoch)) > 0.12 && t >= self.lights_off_until && (!self.asleep || night_light || self.church);
            if !on {
                f.put_g(x, y, Glyph::new('┼', Rgb::hex(0x2a2a38)).solid(Rgb::hex(0x141826)).lit(0.5));
                continue;
            }
            let flick = 0.82 + 0.18 * noise1(t * 5.0 + pair as f32 * 3.1, self.seed ^ 0xf1);
            let (glowc, frame) = if self.church {
                let stained = [Rgb::hex(0x4a7bff), Rgb::hex(0xff4a5a), Rgb::hex(0xffc84a), Rgb::hex(0x5adf8a)];
                (stained[(pair as usize) % stained.len()] * 0.85, Rgb::hex(0x1a1420))
            } else {
                (pal::WINDOW, Rgb::hex(0x2a1406))
            };
            let ch = if self.church { '┃' } else { '┼' };
            f.put_g(x, y, Glyph::new(ch, frame).solid(glowc * flick).lit(0.0));
            f.light_cell(x, y, glowc * (1.6 * flick * glow));
        }
        // home: lit rooms up under the roof (the bedroom, the attic) and a
        // little flag by the door
        if self.home {
            let c = self.x + self.w / 2;
            let rp = self.roof_h * 2;
            let row = (self.wall_top_px() - rp * 45 / 100) / 2;
            let warm = Rgb::hex(0xffd27a);
            for dx in [-3, 3] {
                f.put_g(c + dx, row, Glyph::new('▪', warm).solid(Rgb::hex(0x3a2214)).lit(0.0));
                f.light_cell(c + dx, row, warm * (1.4 * glow));
            }
            if self.roof_h >= 4 {
                f.put_g(c, row - 2, Glyph::new('◉', Rgb::hex(0xffe0a0)).lit(0.0));
                f.light_cell(c, row - 2, warm * (1.0 * glow));
            }
            let fx = self.door + 2;
            for y in self.base - 3..self.base {
                f.put_g(fx, y, Glyph::new('│', Rgb::hex(0x8a8a92)).lit(0.4));
            }
            let wave = (t * 3.0).sin() > 0.0;
            f.put_g(fx + 1, self.base - 3, Glyph::new(if wave { '▶' } else { '▸' }, Rgb::hex(0xd8283a)).lit(0.3));
        }
        // a carrot on the step for the reindeer
        if self.carrot {
            f.put_g(self.door + 2, self.base, Glyph::new('▼', Rgb::hex(0xff8a2a)).lit(0.5));
            f.put_g(self.door + 3, self.base, Glyph::new('ˬ', Rgb::hex(0x3ad07a)).lit(0.5));
        }
        // door + wreath
        let dt = self.base - 1;
        f.put_g(self.door, dt, Glyph::new('❂', Rgb::hex(0x2fae5c)).solid(Rgb::hex(0x3b2316)).lit(0.7));
        f.put_g(self.door, self.base, Glyph::new('·', Rgb::hex(0xffc23a)).solid(Rgb::hex(0x34200f)).lit(0.7));
        if self.church {
            let c = self.x + self.w / 2;
            let rp = self.roof_h * 2;
            let top_px = self.wall_top_px() - rp - 10 - 14;
            let cy = top_px / 2 - 1;
            f.put_g(c, cy, Glyph::new('✝', Rgb::hex(0xffd65a)).lit(0.0));
            f.light_cell(c, cy, Rgb::hex(0xffd65a) * (1.5 * glow));
            // round window in the tower
            let wy = (self.wall_top_px() - rp - 6) / 2;
            f.put_g(c, wy, Glyph::new('◉', Rgb::hex(0xffd88a)).solid(self.wall * 0.8).lit(0.0));
            f.light_cell(c, wy, pal::WINDOW * (1.2 * glow));
        }
    }
}

impl Village {
    pub fn new(pw: usize, ph: usize, seed: u32) -> Self {
        Village {
            houses: vec![],
            lamps: vec![],
            fences: vec![],
            snowmen: vec![],
            cache: Layer::new(pw, ph),
            smoke: vec![],
            smoke_timer: 0.0,
            rng: Rng::new(seed as u64 ^ 0x5a0e),
        }
    }

    /// Render all static pixel art once (call after adding everything).
    pub fn bake(&mut self) {
        let mut l = Layer::new(self.cache.w, self.cache.h);
        for h in &self.houses {
            h.render(&mut l);
        }
        for s in &self.snowmen {
            render_snowman(s, &mut l);
        }
        self.cache = l;
    }

    /// Shift by `-s` seconds (scene clock rebase, see `Scene::update`).
    pub fn rebase(&mut self, s: f32) {
        for h in &mut self.houses {
            h.lights_off_until -= s;
        }
        for l in &mut self.lamps {
            l.off_until -= s;
        }
    }

    pub fn update(&mut self, dt: f32, wind: f32) {
        self.smoke_timer += dt;
        while self.smoke_timer > 0.12 {
            self.smoke_timer -= 0.12;
            for h in &self.houses {
                if let Some((x, y)) = h.chimney_top() {
                    self.smoke.push(Smoke { x, y, age: 0.0, life: self.rng.range(5.0, 8.0), drift: self.rng.range(-0.5, 0.5) });
                }
            }
        }
        for s in &mut self.smoke {
            s.age += dt;
            s.y -= dt * (5.0 - s.age * 0.3).max(1.5);
            s.x += dt * (wind * 3.5 * (s.age * 0.4).min(1.5) + s.drift + (s.age * 1.3 + s.drift * 9.0).sin() * 0.6);
        }
        self.smoke.retain(|s| s.age < s.life);
    }

    pub fn draw_px(&self, f: &mut Frame, cam: f32) {
        self.cache.composite_from(f, cam.round() as i32);
        for s in &self.smoke {
            let u = s.age / s.life;
            let r = 0.6 + u * 2.6;
            let a = (1.0 - u).powf(1.5) * 0.28 * (u * 8.0).min(1.0);
            f.px_disc(s.x, s.y, r, Rgb::hex(0x8d91a8), a);
        }
    }

    pub fn draw_glyphs(&self, f: &mut Frame, t: f32, glow: f32) {
        for h in &self.houses {
            h.draw_glyphs(f, t, glow);
        }
        for &(x0, x1, y) in &self.fences {
            for x in x0..=x1 {
                let post = (x - x0) % 5 == 0;
                let wood = Rgb::hex(0x4a3020);
                if post {
                    f.put_g(x, y - 1, Glyph::new('▄', pal::SNOW * 0.8).lit(0.8));
                    f.put_g(x, y, Glyph::new('╂', wood).lit(0.9));
                } else {
                    f.put_g(x, y, Glyph::new('━', wood).lit(0.9));
                }
            }
        }
        for l in &self.lamps {
            let warm = Rgb::hex(0xffcf7a);
            let flick = if t < l.off_until {
                // clicked: sputters back to life
                let left = l.off_until - t;
                if left < 1.0 && noise1(t * 30.0, 5) > left { 0.8 } else { 0.06 }
            } else {
                0.92 + 0.08 * noise1(t * 6.0 + l.x as f32, 77)
            };
            let top = l.base - l.h + 1;
            let iron = Rgb::hex(0x22242c);
            f.put_g(l.x, top, Glyph::new('▲', iron).lit(0.6));
            f.put_g(l.x, top + 1, Glyph::new('▣', warm * (1.2 * flick)).solid(Rgb::hex(0x8a5a1a) * flick).lit(0.0));
            for y in (top + 2)..l.base {
                f.put_g(l.x, y, Glyph::new('┃', iron).lit(0.6));
            }
            f.put_g(l.x, top + 3, Glyph::new('❦', Rgb::hex(0xd02a3a)).lit(0.5));
            f.put_g(l.x, l.base, Glyph::new('┻', iron).lit(0.6));
            f.light_cell(l.x, top + 1, warm * (5.0 * flick * glow));
        }
        for s in &self.snowmen {
            draw_snowman_glyphs(s, f);
        }
    }
}

/// (x0, y0, x1, y1) in cells.
pub fn snowman_bounds(s: &Snowman) -> (i32, i32, i32, i32) {
    let [_, _, (_, hy, hr)] = snowman_geom(s);
    let half = (7.0 * s.scale).round() as i32;
    (s.cx - half, ((hy - hr - 6.0 * s.scale) / 2.0) as i32, s.cx + half, s.base)
}

pub fn snowman_head(s: &Snowman) -> (i32, i32) {
    let [_, _, (hx, hy, hr)] = snowman_geom(s);
    (hx as i32, ((hy - hr - 6.0 * s.scale) / 2.0) as i32)
}

fn snowman_geom(s: &Snowman) -> [(f32, f32, f32); 3] {
    let cx = s.cx as f32 + 0.5;
    let b = s.base as f32 * 2.0 + 2.0;
    let k = s.scale;
    let (r0, r1, r2) = (5.5 * k, 4.0 * k, 3.0 * k);
    [(cx, b - r0, r0), (cx, b - 2.0 * r0 - r1 + 1.0, r1), (cx, b - 2.0 * r0 - 2.0 * r1 - r2 + 2.0, r2)]
}

fn render_snowman(s: &Snowman, l: &mut Layer) {
    for (cx, cy, r) in snowman_geom(s) {
        let (x0, x1) = ((cx - r - 1.0) as i32, (cx + r + 1.0) as i32);
        let (y0, y1) = ((cy - r - 1.0) as i32, (cy + r + 1.0) as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let lit = 0.72 + 0.28 * (dx / r) - 0.12 * (dy / r);
                    l.blend(x, y, Rgb::hex(0xdfe6f6) * lit.clamp(0.45, 1.1), cov);
                }
            }
        }
    }
    // hat
    let [_, _, (hx, hy, hr)] = snowman_geom(s);
    let brim = hy - hr + 0.5;
    for x in ((hx - hr - 0.5) as i32)..=((hx + hr) as i32) {
        l.set(x, brim as i32, Rgb::hex(0x15151c));
    }
    for y in ((brim - 5.0 * s.scale).round() as i32)..(brim as i32) {
        for x in ((hx - hr * 0.6) as i32)..=((hx + hr * 0.6) as i32) {
            let band = y == brim as i32 - 1;
            l.set(x, y, if band { Rgb::hex(0xb01e2a) } else { Rgb::hex(0x1b1b24) });
        }
    }
}

fn draw_snowman_glyphs(s: &Snowman, f: &mut Frame) {
    let [(_, by, br), (mx, my, mr), (hx, hy, _)] = snowman_geom(s);
    let hrow = (hy / 2.0) as i32;
    let cx = hx as i32;
    let coal = Rgb::hex(0x15151c);
    f.put_g(cx - 1, hrow, Glyph::new('•', coal).lit(0.3));
    f.put_g(cx + 1, hrow, Glyph::new('•', coal).lit(0.3));
    f.put_g(cx, hrow + 1, Glyph::new('>', Rgb::hex(0xff7a1a)).lit(0.3));
    // scarf
    let neck = ((my - mr) / 2.0).round() as i32;
    for x in (cx - 2)..=(cx + 2) {
        f.put_g(x, neck, Glyph::new('≈', Rgb::hex(0xe8323c)).bg(Bg::Solid(Rgb::hex(0x8a1520))).lit(0.6));
    }
    f.put_g(cx + 2, neck + 1, Glyph::new('▌', Rgb::hex(0xc02030)).lit(0.6));
    // buttons
    let mrow = (my / 2.0) as i32;
    f.put_g(cx, mrow, Glyph::new('•', coal).lit(0.3));
    f.put_g(cx, mrow + 1, Glyph::new('•', coal).lit(0.3));
    f.put_g(cx, (by / 2.0) as i32, Glyph::new('•', coal).lit(0.3));
    // stick arms
    let ax = mx as i32;
    let ar = mr as i32;
    let wood = Rgb::hex(0x5a3a22);
    f.put_g(ax - ar - 1, mrow, Glyph::new('╲', wood).lit(0.6));
    f.put_g(ax - ar - 2, mrow - 1, Glyph::new('╲', wood).lit(0.6));
    f.put_g(ax - ar - 3, mrow - 2, Glyph::new('Y', wood).lit(0.6));
    f.put_g(ax + ar + 1, mrow, Glyph::new('╱', wood).lit(0.6));
    f.put_g(ax + ar + 2, mrow - 1, Glyph::new('╱', wood).lit(0.6));
    f.put_g(ax + ar + 3, mrow - 2, Glyph::new('Y', wood).lit(0.6));
    let _ = br;
}
