//! The frame: a half-block pixel canvas (w x 2h), a glyph layer on top (w x h),
//! and a light buffer that gets blurred into a two-radius bloom and added
//! to both layers at compose time.

use crate::color::Rgb;

#[derive(Clone, Copy, Debug)]
pub enum Bg {
    /// Use whatever the pixel canvas has under this cell (lit).
    Scene,
    /// Fixed background color (still receives light scaled by `lit`).
    Solid(Rgb),
    /// Blend scene toward a color by the given amount (frosted panels).
    Mix(Rgb, f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Bg,
    /// 1 = opaque fg; <1 blends fg toward the final background.
    pub alpha: f32,
    /// How strongly scene light tints this glyph (0 for UI).
    pub lit: f32,
}

impl Glyph {
    pub fn new(ch: char, fg: Rgb) -> Self {
        Glyph { ch, fg, bg: Bg::Scene, alpha: 1.0, lit: 1.0 }
    }
    pub fn bg(mut self, bg: Bg) -> Self {
        self.bg = bg;
        self
    }
    pub fn solid(mut self, c: Rgb) -> Self {
        self.bg = Bg::Solid(c);
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = a;
        self
    }
    pub fn lit(mut self, l: f32) -> Self {
        self.lit = l;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', fg: [0; 3], bg: [0; 3] }
    }
}

pub struct Frame {
    pub w: usize,
    pub h: usize,
    /// Camera: world column shown at the left edge. Drawing helpers take
    /// world x and subtract this; screen-fixed things (sky, UI) draw with 0.
    pub ox: i32,
    pub pw: usize,
    pub ph: usize,
    pub px: Vec<Rgb>,
    pub glyph: Vec<Option<Glyph>>,
    pub light: Vec<Rgb>,
    near: Vec<Rgb>,
    tmp: Vec<Rgb>,
    half: Vec<Rgb>,
    half_tmp: Vec<Rgb>,
    /// Per-column bilinear taps for the far-glow upsample.
    xtaps: Vec<(usize, usize, f32)>,
    pub out: Vec<Cell>,
    pub glow_near: f32,
    pub glow_far: f32,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        let mut f = Frame {
            ox: 0,
            w: 0,
            h: 0,
            pw: 0,
            ph: 0,
            px: vec![],
            glyph: vec![],
            light: vec![],
            near: vec![],
            tmp: vec![],
            half: vec![],
            half_tmp: vec![],
            xtaps: vec![],
            out: vec![],
            glow_near: 1.0,
            glow_far: 1.0,
        };
        f.resize(w, h);
        f
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.pw = w;
        self.ph = h * 2;
        let np = self.pw * self.ph;
        self.px = vec![Rgb::BLACK; np];
        self.light = vec![Rgb::BLACK; np];
        self.near = vec![Rgb::BLACK; np];
        self.tmp = vec![Rgb::BLACK; np];
        let nh = self.pw.div_ceil(2) * self.ph.div_ceil(2);
        self.half = vec![Rgb::BLACK; nh];
        self.half_tmp = vec![Rgb::BLACK; nh];
        self.glyph = vec![None; w * h];
        self.out = vec![Cell::default(); w * h];
    }

    pub fn clear(&mut self) {
        self.px.fill(Rgb::BLACK);
        self.light.fill(Rgb::BLACK);
        self.glyph.fill(None);
    }

    // ---------------- pixel layer ----------------

    #[inline]
    pub fn px_in(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.pw && (y as usize) < self.ph
    }

    #[inline]
    pub fn px_set(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x - self.ox;
        if self.px_in(x, y) {
            self.px[y as usize * self.pw + x as usize] = c;
        }
    }

    #[inline]
    pub fn px_get(&self, x: i32, y: i32) -> Rgb {
        let x = x - self.ox;
        if self.px_in(x, y) {
            self.px[y as usize * self.pw + x as usize]
        } else {
            Rgb::BLACK
        }
    }

    #[inline]
    pub fn px_blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        let x = x - self.ox;
        if self.px_in(x, y) {
            let i = y as usize * self.pw + x as usize;
            self.px[i] = self.px[i].lerp(c, a);
        }
    }

    #[inline]
    pub fn px_add(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x - self.ox;
        if self.px_in(x, y) {
            let i = y as usize * self.pw + x as usize;
            self.px[i] += c;
        }
    }

    /// Soft disc blended into the pixel layer (used for smoke, snowman, etc.).
    pub fn px_disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, a: f32) {
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.px_blend(x, y, c, a * cov);
                }
            }
        }
    }

    // ---------------- glyph layer ----------------

    #[inline]
    pub fn cell_in(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    #[inline]
    pub fn put_g(&mut self, x: i32, y: i32, g: Glyph) {
        let x = x - self.ox;
        if self.cell_in(x, y) {
            self.glyph[y as usize * self.w + x as usize] = Some(g);
        }
    }

    #[inline]
    pub fn get_g(&self, x: i32, y: i32) -> Option<Glyph> {
        let x = x - self.ox;
        if self.cell_in(x, y) {
            self.glyph[y as usize * self.w + x as usize]
        } else {
            None
        }
    }

    pub fn has_g(&self, x: i32, y: i32) -> bool {
        self.get_g(x, y).is_some()
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Bg, lit: f32) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            self.put_g(cx, y, Glyph { ch, fg, bg, alpha: 1.0, lit });
            cx += 1;
        }
        cx
    }

    // ---------------- light ----------------

    /// Add emitted light at a pixel-space position (bilinear splat).
    pub fn light_px(&mut self, x: f32, y: f32, c: Rgb) {
        let x = x - self.ox as f32;
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
        let (tx, ty) = (fx - ix as f32, fy - iy as f32);
        for (dx, dy, wgt) in [
            (0, 0, (1.0 - tx) * (1.0 - ty)),
            (1, 0, tx * (1.0 - ty)),
            (0, 1, (1.0 - tx) * ty),
            (1, 1, tx * ty),
        ] {
            let (px, py) = (ix + dx, iy + dy);
            if self.px_in(px, py) {
                let i = py as usize * self.pw + px as usize;
                self.light[i] += c * wgt;
            }
        }
    }

    /// Light emitted from the centre of a glyph cell.
    pub fn light_cell(&mut self, cx: i32, cy: i32, c: Rgb) {
        self.light_px(cx as f32 + 0.5, cy as f32 * 2.0 + 0.5, c * 0.5);
        self.light_px(cx as f32 + 0.5, cy as f32 * 2.0 + 1.5, c * 0.5);
    }

    // ---------------- compose ----------------

    pub fn compose(&mut self) {
        let (pw, ph) = (self.pw, self.ph);
        // near glow: tight halo
        self.near.copy_from_slice(&self.light);
        for _ in 0..3 {
            box_blur(&mut self.near, &mut self.tmp, pw, ph, 1);
        }
        // far glow: wide ambient pool, blurred at half resolution and
        // upsampled bilinearly back into `light` (4x cheaper, same look)
        let (hw, hh) = (pw.div_ceil(2), ph.div_ceil(2));
        for y in 0..hh {
            for x in 0..hw {
                let mut acc = Rgb::BLACK;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((x * 2 + dx).min(pw - 1), (y * 2 + dy).min(ph - 1));
                    acc += self.light[sy * pw + sx];
                }
                self.half[y * hw + x] = acc * 0.25;
            }
        }
        for _ in 0..3 {
            box_blur(&mut self.half, &mut self.half_tmp, hw, hh, 4);
        }
        // bilinear taps depend only on the coordinate: work them out once per
        // column/row instead of once per pixel
        let tap = |i: usize, n: usize| {
            let f = ((i as f32 + 0.5) * 0.5 - 0.5).clamp(0.0, (n - 1) as f32);
            let i0 = f as usize;
            (i0, (i0 + 1).min(n - 1), f.fract())
        };
        self.xtaps.clear();
        self.xtaps.extend((0..pw).map(|x| tap(x, hw)));
        for y in 0..ph {
            let (y0, y1, ty) = tap(y, hh);
            let (r0, r1) = (&self.half[y0 * hw..(y0 + 1) * hw], &self.half[y1 * hw..(y1 + 1) * hw]);
            let out = &mut self.light[y * pw..(y + 1) * pw];
            for (o, &(x0, x1, tx)) in out.iter_mut().zip(&self.xtaps) {
                let a = r0[x0].lerp(r0[x1], tx);
                let b = r1[x0].lerp(r1[x1], tx);
                *o = a.lerp(b, ty);
            }
        }
        let (gn, gf) = (self.glow_near, self.glow_far * 5.5);
        for i in 0..pw * ph {
            let l = self.near[i] * gn + self.light[i] * gf;
            self.near[i] = l; // keep total light for glyph lighting
            self.px[i] += l;
        }

        for cy in 0..self.h {
            for cx in 0..self.w {
                let ti = (cy * 2) * pw + cx;
                let bi = ti + pw;
                let (top, bot) = (self.px[ti], self.px[bi]);
                let cell = match self.glyph[cy * self.w + cx] {
                    None => {
                        let (t8, b8) = (top.tonemap(), bot.tonemap());
                        if t8 == b8 {
                            Cell { ch: ' ', fg: b8, bg: b8 }
                        } else {
                            Cell { ch: '▀', fg: t8, bg: b8 }
                        }
                    }
                    Some(g) => {
                        let light = (self.near[ti] + self.near[bi]) * 0.5;
                        let scene = (top + bot) * 0.5;
                        let bg = match g.bg {
                            Bg::Scene => scene,
                            Bg::Solid(c) => c + light * (g.lit * 0.8),
                            Bg::Mix(c, a) => scene.lerp(c, a),
                        };
                        let fg = g.fg + light * (g.lit * 0.45);
                        let fg = if g.alpha < 1.0 { bg.lerp(fg, g.alpha) } else { fg };
                        Cell { ch: g.ch, fg: fg.tonemap(), bg: bg.tonemap() }
                    }
                };
                self.out[cy * self.w + cx] = cell;
            }
        }
    }
}

/// An RGBA pixel layer used to cache static scenery once and composite it
/// every frame (non-premultiplied "over").
pub struct Layer {
    pub w: usize,
    pub h: usize,
    pub c: Vec<Rgb>,
    pub a: Vec<f32>,
}

impl Layer {
    pub fn new(w: usize, h: usize) -> Self {
        Layer { w, h, c: vec![Rgb::BLACK; w * h], a: vec![0.0; w * h] }
    }

    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, col: Rgb, a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || a <= 0.0 {
            return;
        }
        let i = y as usize * self.w + x as usize;
        let a = a.min(1.0);
        let oa = self.a[i];
        let na = a + oa * (1.0 - a);
        self.c[i] = (col * a + self.c[i] * (oa * (1.0 - a))) * (1.0 / na.max(1e-6));
        self.a[i] = na;
    }

    pub fn set(&mut self, x: i32, y: i32, col: Rgb) {
        self.blend(x, y, col, 1.0);
    }

    /// Composite the slice of a (wider) layer starting at column `x0` onto
    /// the frame — how scrolled scenery is drawn.
    pub fn composite_from(&self, f: &mut Frame, x0: i32) {
        if self.h != f.ph {
            return;
        }
        let w = self.w as i32;
        for y in 0..self.h {
            let (src, dst) = (y * self.w, y * f.pw);
            for x in 0..f.pw {
                let sx = x0 + x as i32;
                if sx < 0 || sx >= w {
                    continue;
                }
                let i = src + sx as usize;
                let a = self.a[i];
                if a >= 0.999 {
                    f.px[dst + x] = self.c[i];
                } else if a > 0.0 {
                    f.px[dst + x] = f.px[dst + x].lerp(self.c[i], a);
                }
            }
        }
    }
}

/// One horizontal + one vertical box-blur pass of radius r (in place via tmp).
fn box_blur(buf: &mut [Rgb], tmp: &mut [Rgb], w: usize, h: usize, r: usize) {
    let inv = 1.0 / (2 * r + 1) as f32;
    // horizontal: buf -> tmp. A running sum is a serial chain of adds, so
    // rows are swept four at a time to give the CPU independent work.
    const ROWS: usize = 4;
    for y in (0..h).step_by(ROWS) {
        let rows = (h - y).min(ROWS);
        let mut acc = [Rgb::BLACK; ROWS];
        for (j, a) in acc.iter_mut().enumerate().take(rows) {
            let row = (y + j) * w;
            for k in 0..=r {
                *a += buf[row + k.min(w - 1)];
            }
            *a += buf[row] * r as f32;
        }
        for x in 0..w {
            let (add, sub) = ((x + r + 1).min(w - 1), x.saturating_sub(r));
            for (j, a) in acc.iter_mut().enumerate().take(rows) {
                let row = (y + j) * w;
                tmp[row + x] = *a * inv;
                *a += buf[row + add];
                *a += buf[row + sub] * -1.0;
            }
        }
    }
    // vertical: tmp -> buf, swept row by row with one running sum per
    // column (same arithmetic as a column-at-a-time loop, but cache friendly)
    let mut acc = vec![Rgb::BLACK; w];
    for k in 0..=r {
        let src = &tmp[k.min(h - 1) * w..][..w];
        for (a, &s) in acc.iter_mut().zip(src) {
            *a += s;
        }
    }
    for (a, &s) in acc.iter_mut().zip(&tmp[..w]) {
        *a += s * r as f32;
    }
    for y in 0..h {
        let add = &tmp[(y + r + 1).min(h - 1) * w..][..w];
        let sub = &tmp[y.saturating_sub(r) * w..][..w];
        let dst = &mut buf[y * w..(y + 1) * w];
        for x in 0..w {
            dst[x] = acc[x] * inv;
            acc[x] += add[x];
            acc[x] += sub[x] * -1.0;
        }
    }
}
