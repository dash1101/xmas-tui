//! The hot-air balloon, an easter egg. Now and then one drifts down over
//! the village and settles a while, its basket on the snow; or one comes by
//! for whoever's climbed right up to the star on top of the big tree, its
//! rope ladder hanging down to them. E by the basket: aboard. Then ← → to
//! drift, ↑ the burner (up), ↓ the vent (down), and E, low over the snow,
//! to hop off again (it rises away).

use crate::color::Rgb;
use crate::frame::Frame;
use crate::rng::{h01, hash2, noise1};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Drifting in to (x, y): the basket's resting place.
    Coming { x: f32, y: f32 },
    /// There, waiting to be boarded (until then).
    Waiting { until: f32 },
    /// Off up and away (nobody aboard).
    Leaving,
}

pub struct Balloon {
    /// The basket's centre column and the row its bottom's at (world).
    pub x: f32,
    pub y: f32,
    vx: f32,
    vy: f32,
    stage: Stage,
    pub aboard: bool,
    /// The burner's lit until then (a roar and a flame).
    burner: f32,
    /// Hovering for someone up the big tree: the ladder hangs down this far
    /// (rows).
    ladder: f32,
    colours: [Rgb; 3],
    seed: u32,
}

/// The envelope's colours: (gores, the other gores, the band round it).
const COLOURS: [[u32; 3]; 4] = [[0xd8283a, 0xf4f0ea, 0xffc23a], [0x2a4c8a, 0xffc23a, 0xd8283a], [0x1f6e3a, 0xd8283a, 0xf4f0ea], [0x6a3a8a, 0xffd36b, 0x2fae5c]];

impl Balloon {
    /// One drifting in from the side `from` (world column) to settle with
    /// its basket at (x, y) (`ladder`: hovering that many rows up, a rope
    /// ladder down).
    pub fn coming(from: f32, top: f32, x: f32, y: f32, ladder: f32, seed: u32) -> Self {
        let c = COLOURS[(seed % 4) as usize];
        Balloon {
            x: from,
            y: top,
            vx: 0.0,
            vy: 0.0,
            stage: Stage::Coming { x, y: y - ladder },
            aboard: false,
            burner: 0.0,
            ladder,
            colours: [Rgb::hex(c[0]), Rgb::hex(c[1]), Rgb::hex(c[2])],
            seed,
        }
    }

    /// Gone: drifted off well out of the village (nobody aboard).
    pub fn gone(&self, ww: f32) -> bool {
        self.stage == Stage::Leaving && (self.y < -60.0 || self.x < -60.0 || self.x > ww + 60.0)
    }

    /// Close enough to climb aboard (the basket, or the foot of the ladder).
    pub fn reachable(&self, px: f32, py: f32) -> bool {
        !self.aboard && matches!(self.stage, Stage::Waiting { .. }) && (px - self.x).abs() < 4.0 && py >= self.y - 3.0 && py <= self.y + self.ladder + 2.0
    }

    /// The basket's settled on (or hovering just over) the snow.
    pub fn landed(&self, ground: f32) -> bool {
        self.y >= ground - 1.5
    }

    pub fn board(&mut self) {
        self.aboard = true;
        self.ladder = 0.0;
        self.stage = Stage::Waiting { until: f32::INFINITY };
    }

    /// Off again (on the snow): it's away up into the sky.
    pub fn hop_off(&mut self) {
        self.aboard = false;
        self.stage = Stage::Leaving;
    }

    /// Each frame: `mv` the keys (aboard), `ground` the row the basket
    /// settles on under it, `sky` the highest row it may go, `ww` the
    /// village's width. Returns true when the burner's just been lit.
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), ground: f32, sky: f32, ww: f32) -> bool {
        let wind = (noise1(t * 0.05, self.seed) - 0.5) * 2.0;
        let mut lit = false;
        match self.stage {
            _ if self.aboard => {
                // ← → drift (a little wind too); ↑ the burner; ↓ the vent;
                // left alone it sinks slowly
                self.vx += (mv.0 * 9.0 + wind) * dt;
                self.vx *= 1.0 - (1.2 * dt).min(0.5);
                if mv.1 < 0.0 {
                    if t > self.burner {
                        lit = true;
                    }
                    self.burner = t + 0.25;
                    self.vy -= 11.0 * dt;
                } else if mv.1 > 0.0 {
                    self.vy += 8.0 * dt;
                } else {
                    self.vy += 1.5 * dt;
                }
                self.vy = self.vy.clamp(-7.0, 6.0) * (1.0 - (0.8 * dt).min(0.5));
            }
            Stage::Coming { x, y } => {
                // drift over, then down onto the spot
                let (dx, dy) = (x - self.x, y - self.y);
                self.vx += (dx.signum() * 6.0 - self.vx) * (1.0 - (-dt).exp()) * if dx.abs() < 3.0 { 0.0 } else { 1.0 };
                if dx.abs() < 3.0 {
                    self.vx *= 1.0 - (2.0 * dt).min(0.9);
                    self.x += dx * (1.0 - (-dt * 1.5).exp());
                }
                self.vy = dy.clamp(-4.0, 4.0) * if dx.abs() < 12.0 { 1.0 } else { 0.2 };
                if dx.abs() < 0.3 && dy.abs() < 0.3 {
                    self.stage = Stage::Waiting { until: t + 75.0 };
                    self.vx = 0.0;
                    self.vy = 0.0;
                }
                if dy < -0.5 && t > self.burner && h01(hash2((t * 3.0) as i32, self.seed as i32)) < 0.05 {
                    self.burner = t + 0.6;
                }
            }
            Stage::Waiting { until } => {
                self.vx = 0.0;
                self.vy = 0.0;
                if t > until {
                    self.stage = Stage::Leaving;
                }
            }
            Stage::Leaving => {
                self.vx += (wind * 3.0 + 2.0 - self.vx) * dt;
                self.vy = -3.5;
                self.burner = t + 0.2;
            }
        }
        self.x = (self.x + self.vx * dt).clamp(-80.0, ww + 80.0);
        self.y += self.vy * dt;
        if self.aboard {
            self.x = self.x.clamp(4.0, ww - 4.0);
            if self.y > ground {
                self.y = ground;
                self.vy = 0.0;
            }
            if self.y < sky {
                self.y = sky;
                self.vy = self.vy.max(0.0);
            }
        }
        lit
    }

    /// The envelope, the ropes and the burner (behind whoever's aboard);
    /// `k`: how big things are drawn (1 at 75 rows). Drawn as a picture,
    /// over the village (in front of the trees and houses).
    pub fn draw_back(&self, f: &mut Frame, t: f32, k: f32) {
        let lit = t < self.burner;
        let fl = if lit { 0.8 + 0.4 * h01(hash2((t * 30.0) as i32, 3)) } else { 0.0 };
        let (rw, rv) = ((9.0 * k).round() as i32, (13.0 * k).round() as i32);
        let rope = (7.0 * k).round() as i32;
        let bw = (4.0 * k).round().max(3.0) as i32;
        let pic = back_picture(rw, rv, rope, bw, self.colours, lit, (fl * 4.0) as u8);
        // (its bottom: the ropes' ends at the basket's rim)
        let bh = (4.0 * k).round().max(3.0) as i32;
        let feet = self.y - bh as f32 / 2.0;
        let rows = pic.h as f32 / 2.0;
        f.sprite(self.x, feet, rows, pic);
        if lit {
            let mouth_y = (self.y * 2.0) - bh as f32 - rope as f32;
            f.light_px(self.x, mouth_y + 1.0, Rgb::hex(0xff9030) * (6.0 * fl));
        }
    }

    /// The basket (in front of whoever's aboard) and, hovering, the rope
    /// ladder down.
    pub fn draw_front(&self, f: &mut Frame, k: f32) {
        let bw = (4.0 * k).round().max(3.0) as i32;
        let bh = (4.0 * k).round().max(3.0) as i32;
        let ladder = if self.ladder > 0.5 && !self.aboard { (self.ladder * 2.0).round() as i32 } else { 0 };
        let pic = basket_picture(bw, bh, ladder);
        let rows = pic.h as f32 / 2.0;
        f.sprite(self.x, self.y + ladder as f32 / 2.0, rows, pic);
    }
}

type Pic = std::sync::Arc<crate::frame::Picture>;

/// A picture from a pixel grid (0: clear).
fn picture(w: i32, h: i32, px: &[u32]) -> Pic {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for &p in px {
        if p == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
        }
    }
    std::sync::Arc::new(crate::frame::Picture::new(w as usize, h as usize, rgba))
}

fn pack(c: Rgb) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    // (never 0: that's "clear")
    (b(c.r) << 16 | b(c.g) << 8 | b(c.b)).max(1)
}

thread_local! {
    static PICS: std::cell::RefCell<std::collections::HashMap<u64, Pic>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn cached(key: u64, make: impl FnOnce() -> Pic) -> Pic {
    PICS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.entry(key).or_insert_with(make).clone()
    })
}

fn back_picture(rw: i32, rv: i32, rope: i32, bw: i32, colours: [Rgb; 3], lit: bool, fl: u8) -> Pic {
    let key = (rw as u64) | (rv as u64) << 8 | (rope as u64) << 16 | (bw as u64) << 24 | (pack(colours[0]) as u64) << 32 | (lit as u64) << 60 | (fl as u64 & 7) << 61;
    cached(key ^ 0xba11, || {
        let tail = rv / 4 + 1;
        let (w, h) = (2 * rw + 3, 2 * rv + tail + rope + 1);
        let mut px = vec![0u32; (w * h) as usize];
        let mut set = |x: i32, y: i32, c: Rgb| {
            if x >= 0 && y >= 0 && x < w && y < h {
                px[(y * w + x) as usize] = pack(c);
            }
        };
        let (cx, cy) = (w / 2, rv);
        // the envelope: a teardrop, gores in two colours, a band round it
        for dy in -rv..=(rv + tail - 1) {
            let half = if dy <= rv / 3 { rw as f32 * (1.0 - (dy as f32 / rv as f32).powi(2)).max(0.0).sqrt() } else { rw as f32 * (1.0 - (dy - rv / 3) as f32 / (rv as f32 * 1.1)) * 0.95 };
            let half = half.max(1.0).round() as i32;
            for dx in -half..=half {
                let u = dx as f32 / half.max(1) as f32;
                let gore = ((u + 1.0) * 3.0) as i32 % 2 == 0;
                let band = (dy + 1).abs() <= (rv / 8).max(1);
                let base = if band { colours[2] } else if gore { colours[0] } else { colours[1] };
                // lit from the top left; a darker edge
                let edge = dx.abs() == half;
                let shade = (1.05 - 0.25 * (u + 0.4).max(0.0) - 0.15 * (dy as f32 / rv as f32).max(0.0)) * if edge { 0.75 } else { 1.0 };
                set(cx + dx, cy + dy, base * shade);
            }
        }
        // the ropes down to the basket's rim
        let mouth = cy + rv + tail;
        let top_w = (rw / 3).max(2);
        for (a, b) in [(-top_w, -bw), (top_w, bw)] {
            for i in 0..=rope {
                let x = cx + a + (b - a) * i / rope.max(1);
                set(x, mouth + i - 1, Rgb::hex(0x5a4430));
            }
        }
        // the burner, roaring when lit
        if lit {
            let k = 0.8 + fl as f32 * 0.1;
            for i in 0..(rope / 2).max(2) {
                set(cx, mouth + i, Rgb::hex(0xffb030) * (k * (1.3 - i as f32 * 0.2)));
            }
        }
        picture(w, h, &px)
    })
}

fn basket_picture(bw: i32, bh: i32, ladder: i32) -> Pic {
    let key = (bw as u64) | (bh as u64) << 8 | (ladder as u64) << 16;
    cached(key ^ 0xba5c, || {
        let (w, h) = (2 * bw + 1, bh + ladder);
        let mut px = vec![0u32; (w * h) as usize];
        for y in 0..bh {
            for x in 0..w {
                let weave = (x + y) % 3 == 0;
                let c = if y == 0 { Rgb::hex(0x8a5a2a) } else if weave { Rgb::hex(0x6a4420) } else { Rgb::hex(0xa8743a) };
                px[(y * w + x) as usize] = pack(c);
            }
        }
        for i in 0..ladder {
            let y = bh + i;
            for x in [bw - 2, bw + 2] {
                if x >= 0 && x < w {
                    px[(y * w + x) as usize] = pack(Rgb::hex(0x8a6a3a));
                }
            }
            if i % 3 == 1 {
                for x in bw - 1..=bw + 1 {
                    px[(y * w + x) as usize] = pack(Rgb::hex(0xc8a060));
                }
            }
        }
        picture(w, h, &px)
    })
}
