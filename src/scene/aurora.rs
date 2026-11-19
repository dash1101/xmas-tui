//! The cinematic aurora: curtains of light. Each curtain is a ribbon hung
//! across the sky from a wandering lower edge, folding back on itself here
//! and there (its sideways sway outrunning it), and its light rises from
//! that edge in rays: a sharp, near-white hem, a green body fading upward,
//! a crimson and violet crown where a pulse of brightness is running along
//! it. Folds and curtains overlapping add up, brighter, as real ones do.
//!
//! Drawn into a buffer of its own (how bright each pixel of the sky is, and
//! in what colour), then added to the sky's pixels and its light (for the
//! glow), and written over in letters, its rays as `. : ¦ |` by how bright
//! they are: the cinematic look. The frozen lake mirrors the same buffer.

use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::{fbm1, h01, hash2, noise1};

/// The aurora's light over a sky `w`×`h` pixels: how bright each pixel is,
/// and its colour weighted by that.
pub struct Aurora {
    pub w: usize,
    pub h: usize,
    pub lum: Vec<f32>,
    pub col: Vec<Rgb>,
    /// When it was last drawn, and for what (time, sky, drift): it's
    /// drawn again 30 times a second at most (it moves slowly; a terminal
    /// redraws less), or straight away for another sky or a new drift.
    last: Option<(f32, usize, f32, f32)>,
}

/// The aurora's own frame rate (it moves slowly).
const RATE: f32 = 30.0;

/// The lower edge: near white, a touch of cyan. The body: green, a little
/// teal higher up. The crown: crimson into violet.
const HEM: Rgb = Rgb::hex(0xc4fff2);
const BODY: Rgb = Rgb::hex(0x2cf08e);
const TEAL: Rgb = Rgb::hex(0x2ad8c8);
const CRIMSON: Rgb = Rgb::hex(0xff3f86);
const VIOLET: Rgb = Rgb::hex(0x8c5cff);

/// Along a curtain, a sample every this many pixels (two to a column: a
/// fold's two layers both land).
const STEP: f32 = 0.5;

/// A ray's light `dy` pixels up from the hem (below it: negative), the ray
/// `len` long: (how bright, what colour). `crown`: how much crimson there
/// is at its top (a pulse, a flare).
#[inline]
fn ray(dy: f32, len: f32, crown: f32) -> (f32, Rgb) {
    if dy < 0.0 {
        // a soft glow just under the hem
        return (0.5 * (dy * 0.9).exp(), HEM.lerp(TEAL, 0.5));
    }
    if dy < 1.5 {
        return (1.4, HEM);
    }
    let u = (dy / len.max(1.0)).min(1.0);
    let mut a = (-(dy - 1.5) / (len * 0.4).max(1.0)).exp() * (1.0 - u * u * u) * 0.9;
    let c = if u < 0.12 {
        HEM.lerp(BODY, u / 0.12)
    } else if u < 0.5 {
        BODY.lerp(TEAL, (u - 0.12) / 0.38 * 0.35)
    } else {
        let k = (u - 0.5) / 0.5;
        // (the crown glows on its own a little in a pulse)
        a += crown * 0.22 * (1.0 - u);
        BODY.lerp(TEAL, 0.35).lerp(CRIMSON.lerp(VIOLET, k), (k * 1.6).min(1.0) * crown)
    };
    (a, c)
}

/// Pulses of brightness running along a curtain (0..1): a few at a time,
/// each a soft swell travelling the curtain's way.
fn pulses(ws: f32, t: f32, speed: f32, sd: u32) -> f32 {
    let mut p = 0.0f32;
    for j in 0..3 {
        let period = 700.0 + 350.0 * j as f32;
        let at = (t * 26.0 * speed + h01(hash2(sd as i32, j)) * period).rem_euclid(period);
        let d = (ws.rem_euclid(period) - at).abs();
        let d = d.min(period - d);
        p = p.max((-(d / 34.0).powi(2)).exp());
    }
    p
}

impl Default for Aurora {
    fn default() -> Self {
        Self::new()
    }
}

impl Aurora {
    pub fn new() -> Self {
        Aurora { w: 0, h: 0, lum: vec![], col: vec![], last: None }
    }

    /// The curtains, drawn afresh, over a sky `w` pixels wide whose horizon
    /// is at row `horizon` (they hang between it and the top): drifted
    /// `off` columns (the sky's slow parallax as the view moves), `flare`
    /// (0..1) the aurora flaring up, `n` curtains (the first the brightest).
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, w: usize, horizon: f32, t: f32, seed: u32, off: f32, flare: f32, n: usize) {
        let h = (horizon.max(1.0).ceil() as usize + 4).max(2);
        if let Some((at, lw, lh, loff)) = self.last {
            if (lw, lh) == (w, horizon) && (off - loff).abs() < 0.25 && (0.0..1.0 / RATE).contains(&(t - at)) {
                return;
            }
        }
        self.last = Some((t, w, horizon, off));
        if (self.w, self.h) != (w, h) {
            (self.w, self.h) = (w, h);
            self.lum = vec![0.0; w * h];
            self.col = vec![Rgb::BLACK; w * h];
        } else {
            self.lum.fill(0.0);
            self.col.fill(Rgb::BLACK);
        }
        if w == 0 {
            return;
        }
        let sky = horizon.max(1.0);
        for k in 0..n {
            let kk = k as i32;
            let r = |salt: i32| h01(hash2(seed as i32 ^ salt, kk));
            // (each its own: how high its hem hangs, how long its rays,
            // which way and how fast it moves, how bright)
            let alt = 0.10 + 0.22 * r(11);
            let reach = 0.40 + 0.25 * r(12);
            let dir = if r(13) > 0.5 { 1.0 } else { -1.0 };
            let speed = (0.5 + 0.9 * r(14)) * dir;
            let ph = r(15) * 300.0;
            let bright = if k == 0 { 1.0 } else { 0.5 + 0.4 * r(16) };
            let sd = seed.wrapping_add(k as u32 * 977);
            let mut s = -0.15 * w as f32;
            let end = 1.15 * w as f32;
            while s < end {
                let ws = s + off;
                s += STEP;
                // how bright along it: it comes and goes
                // (sharp-ish ends, dark sky between: separate ribbons)
                let env = ((fbm1(ws * 0.0032 + ph + t * 0.012 * dir, 3, sd ^ 0x11) - 0.42) / 0.12).clamp(0.0, 1.0);
                if env <= 0.0 {
                    continue;
                }
                let pulse = pulses(ws, t, speed, sd);
                // rays: fine vertical striations, shimmering
                let rays = noise1(ws * 0.42 + t * 0.7 * speed + ph, sd ^ 0x22);
                let fine = noise1(ws * 1.3 - t * 1.6, sd ^ 0x23);
                let rays = 0.2 + 0.8 * (rays * rays * rays) * (0.55 + 0.45 * fine);
                let a = env * rays * bright * (1.0 + 0.9 * pulse + 0.8 * flare) * STEP;
                // the sideways sway (where it outruns the ribbon, a fold:
                // the ribbon doubles back, brighter where it overlaps)
                let fold = 30.0 * fbm1(ws * 0.0035 + ph * 0.5 + t * 0.01, 2, sd ^ 0x33);
                let x = s + fold * (ws * 0.05 + t * 0.07 * speed + ph).sin();
                let xi = x.round() as i32;
                if xi < 0 || xi >= w as i32 {
                    continue;
                }
                // (swooping: low here, high there, as a ribbon seen from below)
                let hem = horizon - sky * (alt + 0.9 * (fbm1(ws * 0.0075 - t * 0.016 * speed + ph, 3, sd ^ 0x44) - 0.42).max(-0.08));
                let len = sky * reach * (0.65 + 0.6 * rays) * (1.0 + 0.3 * pulse);
                let crown = (0.45 + 0.55 * pulse).max(flare);
                let top = (hem - len).max(0.0) as i32;
                let bot = (hem + 3.0).min(h as f32 - 1.0) as i32;
                for y in top..=bot {
                    let (p, c) = ray(hem - y as f32, len, crown);
                    let q = p * a;
                    let i = y as usize * w + xi as usize;
                    self.lum[i] += q;
                    self.col[i] += c * q;
                }
            }
        }
    }

    /// Its colour at pixel `i` (the weighted mix there), and how bright.
    #[inline]
    pub fn at(&self, i: usize) -> (Rgb, f32) {
        let l = self.lum[i];
        (self.col[i] * (1.0 / l.max(1e-4)), l)
    }

    /// Into the sky's pixels, and some into its light (for the glow): `k`
    /// as strong (the night: none by day), `glow` the glow setting. Faint
    /// over `skip` (the moon: centre and radius, px), which it'd tint.
    pub fn add_to(&self, f: &mut Frame, k: f32, glow: f32, skip: Option<(f32, f32, f32)>) {
        if k <= 0.0 {
            return;
        }
        let w = self.w.min(f.pw);
        for y in 0..self.h.min(f.ph) {
            for x in 0..w {
                let i = y * self.w + x;
                let l = self.lum[i];
                if l <= 0.002 {
                    continue;
                }
                let mut k = k;
                if let Some((mx, my, r)) = skip {
                    let (dx, dy) = (x as f32 + 0.5 - mx, y as f32 + 0.5 - my);
                    let d = (dx * dx + dy * dy).sqrt() / r.max(1.0);
                    k *= (0.25 + 0.75 * (d - 1.0) / 0.4).clamp(0.25, 1.0);
                }
                // (eased on the brightest: a ray stays a colour, not white)
                let c = self.col[i] * (k * 0.85 / (1.0 + 0.35 * l));
                f.px_add(x as i32, y as i32, c);
                if glow > 0.0 && l > 0.05 {
                    f.light_px(x as f32 + 0.5, y as f32 + 0.5, c * (0.3 * glow));
                }
            }
        }
    }

    /// The rays in letters over the sky's cells: where it's bright enough,
    /// `. : ¦ |` by how bright, in its own colour and brighter than the
    /// light under them. Not round `skip` (the moon: centre and radius,
    /// px). `k`: how strong (the night).
    pub fn letters(&self, f: &mut Frame, k: f32, skip: Option<(f32, f32, f32)>) {
        if k <= 0.02 || self.h < 2 {
            return;
        }
        let rows = (self.h / 2).min(f.h);
        for cy in 0..rows {
            let (ty, by) = (cy * 2, (cy * 2 + 1).min(self.h - 1));
            for cx in 0..self.w.min(f.w) {
                let (ti, bi) = (ty * self.w + cx, by * self.w + cx);
                let sum = self.lum[ti] + self.lum[bi];
                let l = sum * 0.5 * k;
                if l < 0.05 {
                    continue;
                }
                if let Some((mx, my, r)) = skip {
                    let (dx, dy) = (cx as f32 + 0.5 - mx, cy as f32 * 2.0 + 1.0 - my);
                    if dx * dx + dy * dy < (r * 1.6).powi(2) {
                        continue;
                    }
                }
                let ch = if l < 0.12 {
                    '.'
                } else if l < 0.22 {
                    ':'
                } else if l < 0.36 {
                    '¦'
                } else {
                    '|'
                };
                let c = (self.col[ti] + self.col[bi]) * (1.0 / sum.max(1e-4));
                f.put_g(cx as i32, cy as i32, Glyph::new(ch, c * (0.5 + 0.85 * l.min(1.3))).lit(0.0));
            }
        }
    }
}

/// A faint grid of dots over the night sky (the cinematic look): a dim dot
/// in every empty cell above row `rows`, a little brighter than the sky
/// behind it; `k` how strong (the night). Not round `skip` (the moon).
pub fn dots(f: &mut Frame, rows: i32, k: f32, skip: Option<(f32, f32, f32)>) {
    if k <= 0.02 {
        return;
    }
    let lift = Rgb::hex(0x080d18) * k;
    for cy in 0..rows.min(f.h as i32) {
        for cx in 0..f.w as i32 {
            if f.has_g(cx, cy) {
                continue;
            }
            if let Some((mx, my, r)) = skip {
                let (dx, dy) = (cx as f32 + 0.5 - mx, cy as f32 * 2.0 + 1.0 - my);
                if dx * dx + dy * dy < (r * 1.4).powi(2) {
                    continue;
                }
            }
            let (a, b) = (f.px_get(cx, cy * 2), f.px_get(cx, cy * 2 + 1));
            let star = if a.r + a.g + a.b > b.r + b.g + b.b { a } else { b };
            if (a.r + a.g + a.b - b.r - b.g - b.b).abs() > 0.12 || star.r + star.g + star.b > 0.6 {
                // (a star there: a bright dot of its own, not a grey cell)
                f.put_g(cx, cy, Glyph::new('·', star * 1.3).lit(0.0));
                continue;
            }
            let sky = (a + b) * 0.5;
            f.put_g(cx, cy, Glyph::new('·', sky * (1.0 + 0.35 * k) + lift).lit(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Curtains light the sky between the horizon and the top (none below
    /// the horizon's hem glow), green in the body, near white at the hem;
    /// in letters, brightest as `|`.
    #[test]
    fn curtains_hang_above_the_horizon() {
        let mut a = Aurora::new();
        let mut lit = 0;
        for seed in 0..6u32 {
            a.render(300, 70.0, 12.0 + seed as f32, seed, 0.0, 0.0, 3);
            let n = a.lum.iter().filter(|&&l| l > 0.05).count();
            lit += n;
            // (the hems hang above the horizon: nothing below it)
            for y in 72..a.h {
                assert!((0..300).all(|x| a.lum[y * 300 + x] < 0.2), "light below the horizon");
            }
        }
        assert!(lit > 6 * 300, "hardly any aurora: {lit}");
        let mut f = Frame::new(300, 40);
        a.letters(&mut f, 1.0, None);
        assert!(f.glyph.iter().flatten().any(|g| g.ch == '|'), "no bright rays in letters");
        // the colour of a bright pixel: greener or whiter than it's red
        let i = (0..a.lum.len()).max_by(|&p, &q| a.lum[p].total_cmp(&a.lum[q])).unwrap();
        let (c, _) = a.at(i);
        assert!(c.g > c.r, "{c:?}");
    }
}
