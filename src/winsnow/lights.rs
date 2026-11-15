//! Strings of twinkling bulbs around the topmost few windows, following
//! their rounded corners (the top run droops between bulbs).
//!
//! Where the wire runs, where each bulb sits and the per-pixel shape of each
//! bulb's glow depend only on the window's own rect, so a strand is built
//! once and kept until its window moves or resizes; which parts are hidden
//! behind other windows is re-checked whenever the layout changes. A frame
//! only re-tints the bulbs.

use super::{argb, over, Canvas, Win};
use crate::rng::noise1;
use std::f32::consts::{FRAC_PI_2, PI};

const COLORS: [(u8, u8, u8); 5] = [(0xff, 0x3b, 0x3b), (0xff, 0xc2, 0x3a), (0x3c, 0xff, 0x73), (0x4a, 0x8d, 0xff), (0xff, 0x7a, 0x2a)];
const WARM: [(u8, u8, u8); 3] = [(0xff, 0xd6, 0x96), (0xff, 0xec, 0xbe), (0xff, 0xc8, 0x78)];
const ICY: [(u8, u8, u8); 3] = [(0x96, 0xc8, 0xff), (0xe1, 0xf0, 0xff), (0x5a, 0x96, 0xff)];
const CANDY: [(u8, u8, u8); 2] = [(0xff, 0x32, 0x3c), (0xff, 0xfa, 0xf5)];

/// Bulb colours for a light style (0 multi, 1 warm, 2 icy, 3 candy cane).
fn palette(style: u8) -> &'static [(u8, u8, u8)] {
    match style {
        1 => &WARM,
        2 => &ICY,
        3 => &CANDY,
        _ => &COLORS,
    }
}

struct Bulb {
    /// Index in the light string (picks the colour and twinkle phase).
    i: usize,
    /// Where it hangs on the frame (tested for being covered).
    at: (f32, f32),
    shown: bool,
    x: f32,
    y: f32,
    /// Glow falloff (1 - d/r)² per canvas pixel.
    glow: Vec<(u32, f32)>,
    /// Anti-aliased coverage of the bulb itself per canvas pixel.
    dot: Vec<(u32, f32)>,
}

struct Strand {
    id: String,
    /// What the geometry was built for: window rect, corner, scale, canvas size.
    built_for: [u32; 8],
    z: i64,
    /// Wire dots (top-left corners) and whether each is visible.
    wire: Vec<(f32, f32)>,
    wire_shown: Vec<bool>,
    /// The canvas pixels the visible dots cover, in drawing order (dots
    /// overlap a little, and those pixels are blended twice, as drawn).
    wire_px: Vec<u32>,
    bulbs: Vec<Bulb>,
    /// Bounding rects of everything this strand draws (for clearing).
    rects: Vec<(i32, i32, i32, i32)>,
}

#[derive(Default)]
pub struct Lights {
    /// Fingerprint of the layout visibility was last worked out for.
    key: u64,
    strands: Vec<Strand>,
    /// Each bulb colour premultiplied by every alpha.
    glow_lut: Vec<[u32; 256]>,
    lut_style: u8,
}

const WIRE: (u8, u8, u8, u8) = (0xe0, 0x14, 0x2e, 0x1c);

fn built_for(w: &Win, corner: f32, k: f32, c: &Canvas) -> [u32; 8] {
    [w.x.to_bits(), w.y.to_bits(), w.w.to_bits(), w.h.to_bits(), corner.to_bits(), k.to_bits(), c.w as u32, c.h as u32]
}

/// Hash of everything the strands' geometry depends on.
fn fingerprint(wins: &[Win], corner: f32, k: f32, w: usize, h: usize) -> u64 {
    let mut hsh: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| hsh = (hsh ^ v).wrapping_mul(0x0100_0000_01b3);
    mix(corner.to_bits() as u64);
    mix(k.to_bits() as u64);
    mix(w as u64);
    mix(h as u64);
    for win in wins {
        for v in [win.x, win.y, win.w, win.h] {
            mix(v.to_bits() as u64);
        }
        mix(win.kind as u64);
        mix(win.z as u64);
    }
    hsh
}

impl Lights {
    /// Draw the strands for this frame, rebuilding them if the layout changed.
    /// `style`: colour scheme, `count`: how many (topmost) windows get a
    /// strand, `bright`: bulb/halo brightness multiplier.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, c: &mut Canvas, wins: &[Win], covered: impl Fn(f32, f32, i64) -> bool, corner: f32, k: f32, t: f32, style: u8, count: usize, bright: f32) {
        let key = fingerprint(wins, corner, k, c.w, c.h) ^ (count as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        if key != self.key {
            self.key = key;
            // keep strands whose window didn't move or resize
            let mut old = std::mem::take(&mut self.strands);
            for w in wins.iter().rev().filter(|w| w.kind == 0).take(count.max(1)) {
                let want = built_for(w, corner, k, c);
                let mut st = match old.iter().position(|s| s.id == w.id && s.built_for == want) {
                    Some(i) => old.swap_remove(i),
                    None => Strand::build(c, w, corner, k),
                };
                st.z = w.z;
                st.wire_px.clear();
                for (shown, &(x, y)) in st.wire_shown.iter_mut().zip(&st.wire) {
                    *shown = !covered(x, y, w.z);
                    // the same pixels Canvas::fill would cover
                    if let (true, Some((x0, y0, x1, y1))) = (*shown, c.clip(x, y, 1.5 * k, 1.5 * k)) {
                        for yy in y0..y1 {
                            st.wire_px.extend((x0..x1).map(|xx| (yy * c.w + xx) as u32));
                        }
                    }
                }
                for b in &mut st.bulbs {
                    b.shown = !covered(b.at.0, b.at.1, w.z);
                }
                self.strands.push(st);
            }
        }
        let pal = palette(style);
        if self.glow_lut.is_empty() || self.lut_style != style {
            self.glow_lut = pal.iter().map(|&(r, g, b)| std::array::from_fn(|a| argb(a as u8, r, g, b))).collect();
            self.lut_style = style;
        }
        let wire = argb(WIRE.0, WIRE.1, WIRE.2, WIRE.3);
        let rec = std::mem::replace(&mut c.rec, false);
        for s in &self.strands {
            c.touched.extend_from_slice(&s.rects);
            for &i in &s.wire_px {
                let i = i as usize;
                c.px[i] = over(c.px[i], wire);
            }
            for b in s.bulbs.iter().filter(|b| b.shown) {
                let (cr, cg, cb) = pal[b.i % pal.len()];
                let lut = &self.glow_lut[b.i % pal.len()];
                let lv = 0.45 + 0.55 * noise1(t * 1.6 + b.i as f32 * 1.7 + s.z as f32 * 3.1, 21);
                // halo (fully transparent pixels leave the canvas as it is)
                let kg = (0.38 * lv * bright).min(1.0);
                for &(i, f) in &b.glow {
                    let a = f * kg;
                    let a8 = (255.0 * a) as u8;
                    if a8 > 0 {
                        let i = i as usize;
                        c.px[i] = over(c.px[i], lut[a8 as usize]);
                    }
                }
                // bulb (brighter toward white as it twinkles up)
                let kd = ((0.55 + 0.45 * lv) * bright.max(0.3)).min(1.0);
                let l = |v: u8| ((v as f32 * kd) + 255.0 * (1.0 - kd) * 0.15).min(255.0) as u8;
                let (lr, lg, lb) = (l(cr), l(cg), l(cb));
                for &(i, a) in &b.dot {
                    let i = i as usize;
                    c.px[i] = over(c.px[i], argb((255.0 * a) as u8, lr, lg, lb));
                }
                // glint
                c.fill(b.x - k, b.y - 2.0 * k, k, k, argb((200.0 * lv) as u8, 0xff, 0xff, 0xff));
            }
        }
        c.rec = rec;
    }
}

impl Strand {
    fn build(c: &Canvas, w: &Win, corner: f32, k: f32) -> Strand {
        let off = 3.0 * k;
        let (x0, y0, ww, hh) = (w.x - off, w.y - off, w.w + 2.0 * off, w.h + 2.0 * off);
        let r = (corner + off).min(ww / 2.0).min(hh / 2.0);
        let (lw, lh) = (ww - 2.0 * r, hh - 2.0 * r);
        let arc = FRAC_PI_2 * r;
        let per = 2.0 * (lw + lh) + 4.0 * arc;
        let spacing = 30.0 * k;
        let corner_at = |cx: f32, cy: f32, a0: f32, s: f32| (cx + r * (a0 + s / r.max(0.01)).cos(), cy + r * (a0 + s / r.max(0.01)).sin());
        // position at arc length s around the frame (clockwise from the top
        // left), and whether it's on the drooping top run
        let at = |s: f32| -> (f32, f32, bool) {
            let mut s = s.rem_euclid(per);
            if s < lw {
                let u = (s % spacing) / spacing;
                return (x0 + r + s, y0 + (PI * u).sin() * 4.0 * k, true);
            }
            s -= lw;
            if s < arc {
                let (x, y) = corner_at(x0 + ww - r, y0 + r, -FRAC_PI_2, s);
                return (x, y, false);
            }
            s -= arc;
            if s < lh {
                return (x0 + ww, y0 + r + s, false);
            }
            s -= lh;
            if s < arc {
                let (x, y) = corner_at(x0 + ww - r, y0 + hh - r, 0.0, s);
                return (x, y, false);
            }
            s -= arc;
            if s < lw {
                return (x0 + ww - r - s, y0 + hh, false);
            }
            s -= lw;
            if s < arc {
                let (x, y) = corner_at(x0 + r, y0 + hh - r, FRAC_PI_2, s);
                return (x, y, false);
            }
            s -= arc;
            if s < lh {
                return (x0, y0 + hh - r - s, false);
            }
            s -= lh;
            let (x, y) = corner_at(x0 + r, y0 + r, PI, s.min(arc));
            (x, y, false)
        };
        let mut st = Strand { id: w.id.clone(), built_for: built_for(w, corner, k, c), z: w.z, wire: vec![], wire_shown: vec![], wire_px: vec![], bulbs: vec![], rects: vec![] };
        // the wire, in runs of up to 64 dots so the clear rects stay tight
        let mut s = 0.0;
        let mut run: Option<(f32, f32, f32, f32)> = None;
        let mut n = 0;
        while s < per {
            let (x, y, _) = at(s);
            st.wire.push((x, y));
            let d = 1.5 * k;
            run = Some(match run {
                Some((a, b, cc, e)) => (a.min(x), b.min(y), cc.max(x + d), e.max(y + d)),
                None => (x, y, x + d, y + d),
            });
            n += 1;
            if n >= 64 {
                if let Some(rc) = run.take() {
                    st.touch(c, rc);
                }
                n = 0;
            }
            s += 1.5 * k;
        }
        if let Some(rc) = run {
            st.touch(c, rc);
        }
        st.wire_shown = vec![true; st.wire.len()];
        // the bulbs
        let count = (per / spacing) as usize;
        for i in 0..count {
            let (x, at_y, top) = at(i as f32 * spacing + spacing * 0.5);
            let y = if top { at_y + 2.0 * k } else { at_y };
            let mut b = Bulb { i, at: (x, at_y), shown: true, x, y, glow: vec![], dot: vec![] };
            // a soft halo (radius 9) and an anti-aliased bulb (radius 3)
            let gr = 9.0 * k;
            if let Some((x0, y0, x1, y1)) = c.clip(x - gr, y - gr, 2.0 * gr, 2.0 * gr) {
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let d = ((xx as f32 + 0.5 - x).powi(2) + (yy as f32 + 0.5 - y).powi(2)).sqrt() / gr;
                        if d < 1.0 {
                            b.glow.push(((yy * c.w + xx) as u32, (1.0 - d).powi(2)));
                        }
                    }
                }
                st.rects.push((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32));
            }
            let dr = 3.0 * k;
            if let Some((x0, y0, x1, y1)) = c.clip(x - dr - 1.0, y - dr - 1.0, 2.0 * dr + 2.0, 2.0 * dr + 2.0) {
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let d = ((xx as f32 + 0.5 - x).powi(2) + (yy as f32 + 0.5 - y).powi(2)).sqrt();
                        let a = (dr + 0.5 - d).clamp(0.0, 1.0);
                        if a > 0.0 {
                            b.dot.push(((yy * c.w + xx) as u32, a));
                        }
                    }
                }
                st.rects.push((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32));
            }
            st.touch(c, (x - k, y - 2.0 * k, x, y - k));
            st.bulbs.push(b);
        }
        st
    }

    fn touch(&mut self, c: &Canvas, (x0, y0, x1, y1): (f32, f32, f32, f32)) {
        if let Some((a, b, cc, d)) = c.clip(x0, y0, x1 - x0, y1 - y0) {
            self.rects.push((a as i32, b as i32, (cc - a) as i32, (d - b) as i32));
        }
    }
}
