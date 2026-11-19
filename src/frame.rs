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
    /// Text over the scene (speech bubbles, prompts): keeps its size when
    /// the view is zoomed.
    pub ui: bool,
}

static PICTURES_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The front-end can show pictures over the cells (the window, kitty, the
/// exporter); otherwise real icons fall back to pixel art.
pub fn set_pictures_ok(on: bool) {
    PICTURES_OK.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn pictures_ok() -> bool {
    PICTURES_OK.load(std::sync::atomic::Ordering::Relaxed)
}

static ZOOM_LETTERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Zoomed in, the scene's letters drawn big as pictures (where pictures
/// are cheap: drawn into our own pixels). A terminal sends each picture as
/// an image of its own, and hundreds of them, placed afresh every frame,
/// would bog it down: there they're drawn as pixels.
pub fn set_zoom_letters(on: bool) {
    ZOOM_LETTERS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A letter as a picture (the font's own shape, in its colour), to draw
/// the scene's text bigger when zoomed in. Kept for reuse (colours are
/// rounded a little so a twinkling light doesn't make a new one each frame).
pub fn glyph_picture(ch: char, fg: Rgb, alpha: f32) -> std::sync::Arc<Picture> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    type Cache = HashMap<(char, u32, u8), Arc<Picture>>;
    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    let [r, g, b] = fg.tonemap();
    let q = |v: u8| (v >> 3) << 3;
    let key = (ch, (q(r) as u32) << 16 | (q(g) as u32) << 8 | q(b) as u32, (alpha.clamp(0.0, 1.0) * 7.0).round() as u8);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let c = cache.get_or_insert_with(HashMap::new);
    if let Some(p) = c.get(&key) {
        return p.clone();
    }
    if c.len() > 2500 {
        c.clear();
    }
    let m = crate::raster::glyph_mask(ch);
    let a = key.2 as f32 / 7.0;
    let mut rgba = Vec::with_capacity(m.w * m.h * 4);
    for &cov in &m.a {
        rgba.extend_from_slice(&[q(r), q(g), q(b), (cov * a * 255.0).round().min(255.0) as u8]);
    }
    let p = Arc::new(Picture::new(m.w, m.h, rgba));
    c.insert(key, p.clone());
    p
}

/// A see-through panel's cell made solid: the panel's colour over the
/// village behind it, averaged (the frosted look, at any size: blended over
/// what's on screen instead, the village's every flake and letter would
/// show through, and a cell drawn over itself again would darken).
pub fn frost(cells: &[Cell], cols: usize, (x0, x1, y0, y1): (usize, usize, usize, usize), panel: [u8; 3], a: u8) -> [u8; 3] {
    let (mut sum, mut n) = ([0u32; 3], 0u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = &cells[y * cols + x];
            for k in 0..3 {
                sum[k] += match c.ch {
                    ' ' => c.bg[k] as u32,
                    '▀' | '▄' => (c.fg[k] as u32 + c.bg[k] as u32) / 2,
                    _ => (c.bg[k] as u32 * 4 + c.fg[k] as u32) / 5,
                };
            }
            n += 1;
        }
    }
    if n == 0 {
        return panel;
    }
    let a = a as u32;
    [0, 1, 2].map(|k| ((sum[k] / n * (255 - a) + panel[k] as u32 * a) / 255) as u8)
}

/// An image (RGBA, straight alpha), shared and identified for caching.
pub struct Picture {
    pub id: u64,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Picture {
    pub fn new(w: usize, h: usize, rgba: Vec<u8>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Picture { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), w, h, rgba }
    }

    /// Blit at a whole-number scale `s`, crisp (nearest pixel), with its
    /// top-left at (x, y) (may be partly off the buffer).
    pub fn blit_scaled(&self, buf: &mut [u32], stride: usize, x: i32, y: i32, s: usize) {
        let rows = (buf.len() / stride.max(1)) as i32;
        for sy in 0..self.h {
            for sx in 0..self.w {
                let i = (sy * self.w + sx) * 4;
                let a = self.rgba[i + 3] as u32;
                if a == 0 {
                    continue;
                }
                let (r, g, b) = (self.rgba[i] as u32, self.rgba[i + 1] as u32, self.rgba[i + 2] as u32);
                for dy in 0..s as i32 {
                    let py = y + sy as i32 * s as i32 + dy;
                    if py < 0 || py >= rows {
                        continue;
                    }
                    for dx in 0..s as i32 {
                        let px = x + sx as i32 * s as i32 + dx;
                        if px < 0 || px >= stride as i32 {
                            continue;
                        }
                        let o = &mut buf[py as usize * stride + px as usize];
                        *o = if a == 255 {
                            r << 16 | g << 8 | b
                        } else {
                            let m = |c: u32, d: u32| (c * a + d * (255 - a)) / 255;
                            m(r, (*o >> 16) & 255) << 16 | m(g, (*o >> 8) & 255) << 8 | m(b, *o & 255)
                        };
                    }
                }
            }
        }
    }

    /// The same picture scaled up `s` times (nearest pixel), for a terminal
    /// that shows images at their own size.
    pub fn scaled(&self, s: usize) -> Picture {
        let (w, h) = (self.w * s, self.h * s);
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = ((y / s) * self.w + x / s) * 4;
                out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&self.rgba[i..i + 4]);
            }
        }
        Picture { id: 0, w, h, rgba: out }
    }

    /// Blit into a 0RGB pixel buffer, scaled to fit `bw`×`bh` at (x, y)
    /// (centred, aspect kept; alpha-blended over what's there). Pixel art
    /// stays crisp: grown by a whole number when that nearly fills the
    /// space, otherwise each pixel kept square-edged with only the seam
    /// between two of them blended ("sharp bilinear"); shrunk, bilinear.
    pub fn blit(&self, buf: &mut [u32], stride: usize, x: usize, y: usize, bw: usize, bh: usize) {
        if self.w == 0 || self.h == 0 || bw == 0 || bh == 0 {
            return;
        }
        let fit = (bw as f32 / self.w as f32).min(bh as f32 / self.h as f32);
        // (a whole-number scale, if it loses little of the space)
        let whole = fit.floor();
        let k = if whole >= 1.0 && whole / fit >= 0.85 { whole } else { fit };
        if k == whole {
            let (dw, dh) = (self.w * k as usize, self.h * k as usize);
            self.blit_scaled(buf, stride, (x + (bw - dw) / 2) as i32, (y + (bh - dh) / 2) as i32, k as usize);
            return;
        }
        // how sharp the seams are: none when shrinking (plain bilinear)
        let sharp = k.max(1.0);
        let edge = |f: f32| ((f - 0.5) * sharp + 0.5).clamp(0.0, 1.0);
        let (dw, dh) = ((self.w as f32 * k).round() as usize, (self.h as f32 * k).round() as usize);
        let (ox, oy) = (x + (bw - dw) / 2, y + (bh - dh) / 2);
        let rows = buf.len() / stride.max(1);
        for dy in 0..dh {
            let py = oy + dy;
            if py >= rows {
                break;
            }
            let sy = ((dy as f32 + 0.5) / k - 0.5).clamp(0.0, (self.h - 1) as f32);
            let (y0, fy) = (sy.floor() as usize, edge(sy.fract()));
            let y1 = (y0 + 1).min(self.h - 1);
            for dx in 0..dw {
                let px = ox + dx;
                if px >= stride {
                    break;
                }
                let sx = ((dx as f32 + 0.5) / k - 0.5).clamp(0.0, (self.w - 1) as f32);
                let (x0, fx) = (sx.floor() as usize, edge(sx.fract()));
                let x1 = (x0 + 1).min(self.w - 1);
                let mut c = [0f32; 4];
                for (xx, yy, wgt) in [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x1, y0, fx * (1.0 - fy)), (x0, y1, (1.0 - fx) * fy), (x1, y1, fx * fy)] {
                    let i = (yy * self.w + xx) * 4;
                    let a = self.rgba[i + 3] as f32 / 255.0 * wgt;
                    c[0] += self.rgba[i] as f32 * a;
                    c[1] += self.rgba[i + 1] as f32 * a;
                    c[2] += self.rgba[i + 2] as f32 * a;
                    c[3] += a;
                }
                if c[3] <= 0.004 {
                    continue;
                }
                let o = &mut buf[py * stride + px];
                let (br, bg, bb) = ((*o >> 16) & 255, (*o >> 8) & 255, *o & 255);
                let a = c[3].min(1.0);
                let mix = |src: f32, dst: u32| ((src + dst as f32 * (1.0 - a)).round() as u32).min(255);
                *o = (mix(c[0], br) << 16) | (mix(c[1], bg) << 8) | mix(c[2], bb);
            }
        }
    }
}

/// A picture placed over cells.
#[derive(Clone)]
pub struct Placed {
    /// The cells it covers (a sprite: about; enough to redraw under it).
    pub x: i32,
    pub y: i32,
    pub cols: i32,
    pub rows: i32,
    pub pic: std::sync::Arc<Picture>,
    /// A sprite (people, pets): its centre column and feet row (both
    /// fractional, on the grid) and how many rows tall it's meant to be.
    /// It's drawn at the biggest whole-number scale that fits, crisp, with
    /// its feet on that row. None: fitted into its cells (an icon).
    pub sprite: Option<(f32, f32, f32)>,
    /// Stretched to fill exactly this rectangle (x, y, w, h: cells, from
    /// the grid's corner, fractions fine): a zoomed-in letter, filling its
    /// big cell edge to edge so lines and blocks join up with the next.
    pub fit: Option<[f32; 4]>,
    /// Cells a panel's been drawn over (that much of it isn't shown).
    pub hidden: Vec<(i32, i32)>,
}

thread_local! {
    /// Pictures with parts cut away (under a panel), kept while they're
    /// wanted: the same cut, the same picture (a terminal sends each once).
    static CUTS: std::cell::RefCell<std::collections::HashMap<(u64, u64), std::sync::Arc<Picture>>> = Default::default();
}

impl Placed {
    /// The picture with the cells under panels cut away (None: nothing of
    /// it left showing). `aspect`: a cell's height over its width.
    fn cut(&self, aspect: f32) -> Option<std::sync::Arc<Picture>> {
        use std::hash::{Hash, Hasher};
        let mut hidden = self.hidden.clone();
        hidden.sort_unstable();
        hidden.dedup();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        hidden.hash(&mut h);
        if let Some(cx) = self.sprite.map(|s| ((s.0 * 8.0) as i32, (s.1 * 8.0) as i32)) {
            cx.hash(&mut h);
        }
        if let Some(r) = self.fit {
            r.map(|v| (v * 8.0) as i32).hash(&mut h);
        }
        let key = (self.pic.id, h.finish());
        if let Some(p) = CUTS.with(|c| c.borrow().get(&key).cloned()) {
            return (!p.rgba.is_empty()).then_some(p);
        }
        let (w, hgt) = (self.pic.w, self.pic.h);
        // where each of the picture's pixels lands (in cells)
        let (x0, y0, sx, sy) = match self.sprite {
            _ if self.fit.is_some() => {
                let [x, y, fw, fh] = self.fit.unwrap_or_default();
                (x, y, fw / w.max(1) as f32, fh / hgt.max(1) as f32)
            }
            Some((cx, feet, rows)) => {
                let wc = w as f32 / hgt.max(1) as f32 * rows * aspect;
                (cx - wc / 2.0, feet - rows, wc / w.max(1) as f32, rows / hgt.max(1) as f32)
            }
            None => (self.x as f32, self.y as f32, self.cols as f32 / w.max(1) as f32, self.rows as f32 / hgt.max(1) as f32),
        };
        let mut rgba = self.pic.rgba.clone();
        let mut left = 0;
        for py in 0..hgt {
            let cy = (y0 + (py as f32 + 0.5) * sy).floor() as i32;
            for px in 0..w {
                let i = (py * w + px) * 4 + 3;
                if rgba[i] == 0 {
                    continue;
                }
                let cx = (x0 + (px as f32 + 0.5) * sx).floor() as i32;
                if hidden.binary_search(&(cx, cy)).is_ok() {
                    rgba[i] = 0;
                } else {
                    left += 1;
                }
            }
        }
        let pic = std::sync::Arc::new(if left == 0 { Picture::new(0, 0, vec![]) } else { Picture::new(w, hgt, rgba) });
        CUTS.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 96 {
                c.clear();
            }
            c.insert(key, pic.clone());
        });
        (left > 0).then_some(pic)
    }

    /// A sprite's top-left (pixels, from the grid's corner) and scale, for
    /// cells `cw`×`ch` pixels.
    pub fn sprite_px(&self, cw: usize, ch: usize) -> Option<(i32, i32, usize)> {
        let (cx, feet, rows) = self.sprite?;
        let s = ((rows * ch as f32 / self.pic.h.max(1) as f32).floor() as usize).max(1);
        let (pw, ph) = ((self.pic.w * s) as f32, (self.pic.h * s) as f32);
        Some(((cx * cw as f32 - pw / 2.0).round() as i32, (feet * ch as f32 - ph).round() as i32, s))
    }

    /// A fitted picture's pixel rectangle (x, y, w, h), for cells `cw`×`ch`
    /// pixels: edges on whole pixels, so neighbours meet without a gap.
    pub fn fit_px(&self, cw: usize, ch: usize) -> Option<(i32, i32, usize, usize)> {
        let [x, y, w, h] = self.fit?;
        let (cw, ch) = (cw as f32, ch as f32);
        let (x0, y0) = ((x * cw).round() as i32, (y * ch).round() as i32);
        let (x1, y1) = (((x + w) * cw).round() as i32, ((y + h) * ch).round() as i32);
        Some((x0, y0, (x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize))
    }

    /// Draw a fitted picture into `buf` (the grid's corner at (ox, oy));
    /// false if it isn't one (the caller draws it its own way).
    pub fn draw_fitted(&self, buf: &mut [u32], stride: usize, ox: i32, oy: i32, cw: usize, ch: usize) -> bool {
        let Some((x, y, w, h)) = self.fit_px(cw, ch) else { return false };
        if w > 0 && h > 0 {
            self.pic.fitted(w, h).blit_scaled(buf, stride, ox + x, oy + y, 1);
        }
        true
    }
}

thread_local! {
    /// Pictures resized to fit (a zoomed letter at this zoom's size), kept
    /// while they're shown.
    static FITTED: std::cell::RefCell<std::collections::HashMap<(u64, usize, usize), std::sync::Arc<Picture>>> = Default::default();
}

impl Picture {
    /// This picture resized to exactly `w`×`h` (smoothly: averaged down,
    /// blended up), kept for reuse.
    pub fn fitted(&self, w: usize, h: usize) -> std::sync::Arc<Picture> {
        let key = (self.id, w, h);
        if let Some(p) = FITTED.with(|c| c.borrow().get(&key).cloned()) {
            return p;
        }
        let p = std::sync::Arc::new(Picture::new(w, h, self.resample(w, h)));
        FITTED.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 4000 {
                c.clear();
            }
            c.insert(key, p.clone());
        });
        p
    }

    /// The pixels resized to `w`×`h` (straight alpha in and out; colours
    /// weighted by how solid they are): each new pixel the average of the
    /// old ones it covers, or between them when growing.
    fn resample(&self, w: usize, h: usize) -> Vec<u8> {
        let mut out = vec![0u8; w * h * 4];
        if self.w == 0 || self.h == 0 || w == 0 || h == 0 {
            return out;
        }
        let (kx, ky) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        // the old pixels each new one spans (with how much of each)
        let span = |i: usize, k: f32, n: usize| -> Vec<(usize, f32)> {
            if k <= 1.0 {
                // growing: between the two nearest
                let f = ((i as f32 + 0.5) * k - 0.5).clamp(0.0, (n - 1) as f32);
                let i0 = f as usize;
                let i1 = (i0 + 1).min(n - 1);
                vec![(i0, 1.0 - f.fract()), (i1, f.fract())]
            } else {
                let (a, b) = (i as f32 * k, (i as f32 + 1.0) * k);
                let mut v = vec![];
                let mut j = a.floor() as usize;
                while (j as f32) < b && j < n {
                    let cover = (b.min(j as f32 + 1.0) - a.max(j as f32)).max(0.0);
                    if cover > 0.0 {
                        v.push((j, cover / k));
                    }
                    j += 1;
                }
                v
            }
        };
        let xs: Vec<Vec<(usize, f32)>> = (0..w).map(|x| span(x, kx, self.w)).collect();
        for y in 0..h {
            let ys = span(y, ky, self.h);
            for (x, xs) in xs.iter().enumerate() {
                let mut c = [0f32; 4];
                for &(sy, wy) in &ys {
                    for &(sx, wx) in xs {
                        let i = (sy * self.w + sx) * 4;
                        let a = self.rgba[i + 3] as f32 * wx * wy;
                        c[0] += self.rgba[i] as f32 * a;
                        c[1] += self.rgba[i + 1] as f32 * a;
                        c[2] += self.rgba[i + 2] as f32 * a;
                        c[3] += a;
                    }
                }
                if c[3] > 0.0 {
                    let o = (y * w + x) * 4;
                    out[o] = (c[0] / c[3]).round().min(255.0) as u8;
                    out[o + 1] = (c[1] / c[3]).round().min(255.0) as u8;
                    out[o + 2] = (c[2] / c[3]).round().min(255.0) as u8;
                    out[o + 3] = c[3].round().min(255.0) as u8;
                }
            }
        }
        out
    }
}

impl Glyph {
    pub fn new(ch: char, fg: Rgb) -> Self {
        Glyph { ch, fg, bg: Bg::Scene, alpha: 1.0, lit: 1.0, ui: false }
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

/// (Cloned to hold a picture still: see `App`'s resting.)
#[derive(Clone)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    /// Camera: world column shown at the left edge. Drawing helpers take
    /// world x and subtract this; screen-fixed things (sky, UI) draw with 0.
    /// Rows at the bottom something else covers (the taskbar): the UI
    /// keeps above them (`ui_in` / `ui_out`).
    pub safe_bottom: usize,
    /// Zoomed in: the scene's glyphs, drawn bigger as pictures of the
    /// font's own letters (kept as text, not blurred into pixels), and the
    /// cells text was put back in over them (see `zoom`, `zoom_pictures`).
    zoom_glyphs: Vec<Placed>,
    zoom_text: Vec<(i32, i32)>,
    pub ox: i32,
    /// Vertical camera: everything scene-drawn moves down this many rows
    /// (looking up into the sky).
    pub oy: i32,
    /// Daylight (0 night … 1 day) and sunrise/sunset warmth: cached land
    /// layers are brightened by this as they're composited.
    pub day: f32,
    pub day_warm: f32,
    pub pw: usize,
    pub ph: usize,
    pub px: Vec<Rgb>,
    pub glyph: Vec<Option<Glyph>>,
    pub light: Vec<Rgb>,
    /// Pixels of sprites drawn big (glyphs turned pixels): lit the way a
    /// glyph is, not like the scenery (1), or as usual (0).
    pub soft: Vec<u8>,
    near: Vec<Rgb>,
    tmp: Vec<Rgb>,
    half: Vec<Rgb>,
    half_tmp: Vec<Rgb>,
    /// Per-column bilinear taps for the far-glow upsample.
    xtaps: Vec<(usize, usize, f32)>,
    pub out: Vec<Cell>,
    /// While set, glyphs put are marked as text over the scene (see
    /// `Glyph::ui`).
    pub ui: bool,
    /// While set (the widgets drawing), text over the scene isn't painted
    /// over: speech bubbles and prompts stay readable on top.
    pub keep_ui: bool,
    /// Which cells hold a speck (a snowflake or a star), and whether glyphs
    /// put now are specks: the clock clears specks from between its digits
    /// and leaves trees, houses and their lights alone.
    speck: Vec<bool>,
    pub specks: bool,
    /// Pictures over the cells (real icons on shelves): the window blits
    /// them, the terminal shows them with kitty's graphics protocol.
    pub pictures: Vec<Placed>,
    /// Set once the widgets are drawn: anything drawn after (the settings,
    /// menus, cards) hides a picture it overlaps.
    pub cover_pictures: bool,
    /// While the widgets draw: a cell with a background of its own (a
    /// panel) hides a picture under it (people behind a widget).
    pub cover_panels: bool,
    /// A cell's height over its width (the front-end sets it), for working
    /// out how many columns a sprite takes.
    pub cell_aspect: f32,
    /// Light already bloomed (and magnified) by `zoom`, added at compose.
    pre: Vec<Rgb>,
    pre_on: bool,
    pub glow_near: f32,
    pub glow_far: f32,
    /// How much bigger the view's drawn (zoomed in): the glow spreads as
    /// far again, so it looks as it does at 1×, only bigger.
    glow_k: f32,
    /// The glow worked out only every so many frames (a live wallpaper:
    /// every other; it changes too slowly to see), the last one's kept in
    /// `near` meanwhile; and how many frames since, and whether there is a
    /// last one yet (none after a resize).
    pub bloom_every: u32,
    bloom_tick: u32,
    bloomed: bool,
    /// Where the view is (the camera, the zoom's window): when it's moved
    /// since the glow was last worked out, it's worked out again, not kept
    /// a column off (every light flickering as the village scrolls).
    pub view: u64,
    bloom_view: u64,
    /// The camera's part of a cell, for what it follows (the villager,
    /// their pets): added to where they're drawn (columns, rows), so they
    /// hold still on screen while the village steps by under them.
    pub track: (f32, f32),
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        let mut f = Frame {
            safe_bottom: 0,
            zoom_glyphs: vec![],
            zoom_text: vec![],
            ox: 0,
            oy: 0,
            day: 0.0,
            day_warm: 0.0,
            w: 0,
            h: 0,
            pw: 0,
            ph: 0,
            px: vec![],
            glyph: vec![],
            light: vec![],
            soft: vec![],
            near: vec![],
            tmp: vec![],
            half: vec![],
            half_tmp: vec![],
            xtaps: vec![],
            out: vec![],
            ui: false,
            keep_ui: false,
            speck: vec![],
            specks: false,
            pictures: vec![],
            cover_pictures: false,
            cover_panels: false,
            cell_aspect: 2.1,
            pre: vec![],
            pre_on: false,
            glow_near: 1.0,
            glow_far: 1.0,
            glow_k: 1.0,
            bloom_every: 1,
            bloom_tick: 0,
            bloomed: false,
            view: 0,
            bloom_view: 0,
            track: (0.0, 0.0),
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
        self.soft = vec![0; np];
        self.near = vec![Rgb::BLACK; np];
        self.bloomed = false;
        self.tmp = vec![Rgb::BLACK; np];
        let nh = self.pw.div_ceil(2) * self.ph.div_ceil(2);
        self.half = vec![Rgb::BLACK; nh];
        self.half_tmp = vec![Rgb::BLACK; nh];
        self.glyph = vec![None; w * h];
        self.speck = vec![false; w * h];
        self.out = vec![Cell::default(); w * h];
    }

    pub fn clear(&mut self) {
        self.px.fill(Rgb::BLACK);
        self.light.fill(Rgb::BLACK);
        self.glow_k = 1.0;
        self.soft.fill(0);
        self.pre_on = false;
        self.ui = false;
        self.specks = false;
        self.glyph.fill(None);
        self.speck.fill(false);
        self.pictures.clear();
        self.cover_pictures = false;
        self.cover_panels = false;
        self.track = (0.0, 0.0);
    }

    /// Drawing the UI from here: the frame's height is what's above the
    /// taskbar (anything placed from the bottom up lands clear of it). Give
    /// what this returns to `ui_out` afterwards.
    pub fn ui_in(&mut self) -> usize {
        let full = self.h;
        if self.safe_bottom > 0 && full > self.safe_bottom + 8 {
            self.h = full - self.safe_bottom;
        }
        full
    }

    pub fn ui_out(&mut self, full: usize) {
        self.h = full;
    }

    /// A sprite (a person, a pet): centred on column `cx`, feet on row
    /// `feet` (grid coordinates, fractional), `rows` tall. Drawn crisp at a
    /// whole-number scale, over the cells.
    pub fn sprite(&mut self, cx: f32, feet: f32, rows: f32, pic: std::sync::Arc<Picture>) {
        let (cx, feet) = (cx - self.ox as f32, feet + self.oy as f32);
        // the cells it's on (about: the renderer redraws a cell round it too)
        let wc = pic.w as f32 / pic.h.max(1) as f32 * rows * self.cell_aspect;
        let (x, y) = ((cx - wc / 2.0).floor() as i32, (feet - rows).floor() as i32);
        let (cols, rws) = ((cx + wc / 2.0).ceil() as i32 - x, feet.ceil() as i32 - y);
        if x + cols < 0 || y + rws < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        self.pictures.push(Placed { x, y, cols, rows: rws, pic, sprite: Some((cx, feet, rows)), fit: None, hidden: vec![] });
    }

    /// A picture over `cols`×`rows` cells at (x, y) (the cells under it are
    /// whatever was drawn there; the picture goes on top).
    pub fn picture(&mut self, x: i32, y: i32, cols: i32, rows: i32, pic: std::sync::Arc<Picture>) {
        let (x, y) = (x - self.ox, y + self.oy);
        if x >= 0 && y >= 0 && x + cols <= self.w as i32 && y + rows <= self.h as i32 {
            // not over a speech bubble (it stays readable on top)
            let w = self.w;
            let ui = (y..y + rows).any(|yy| (x..x + cols).any(|xx| self.glyph[yy as usize * w + xx as usize].is_some_and(|g| g.ui)));
            if !ui {
                self.pictures.push(Placed { x, y, cols, rows, pic, sprite: None, fit: None, hidden: vec![] });
            }
        }
    }

    /// A picture over `cols`×`rows` cells at (x, y) on a panel (over its
    /// text: a line under a row, say), wherever it is.
    pub fn picture_on_panel(&mut self, x: i32, y: i32, cols: i32, rows: i32, pic: std::sync::Arc<Picture>) {
        let (x, y) = (x - self.ox, y + self.oy);
        if x >= 0 && y >= 0 && x + cols <= self.w as i32 && y + rows <= self.h as i32 {
            self.pictures.push(Placed { x, y, cols, rows, pic, sprite: None, fit: None, hidden: vec![] });
        }
    }

    // ---------------- pixel layer ----------------

    #[inline]
    pub fn px_in(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.pw && (y as usize) < self.ph
    }

    #[inline]
    pub fn px_set(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x - self.ox;
        let y = y + 2 * self.oy;
        if self.px_in(x, y) {
            self.px[y as usize * self.pw + x as usize] = c;
        }
    }

    /// A sprite's pixel: set, and lit the way a glyph is (see `soft`).
    #[inline]
    pub fn px_sprite(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x - self.ox;
        let y = y + 2 * self.oy;
        if self.px_in(x, y) {
            let i = y as usize * self.pw + x as usize;
            self.px[i] = c;
            self.soft[i] = 1;
        }
    }

    #[inline]
    pub fn px_get(&self, x: i32, y: i32) -> Rgb {
        let x = x - self.ox;
        let y = y + 2 * self.oy;
        if self.px_in(x, y) {
            self.px[y as usize * self.pw + x as usize]
        } else {
            Rgb::BLACK
        }
    }

    #[inline]
    pub fn px_blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        let x = x - self.ox;
        let y = y + 2 * self.oy;
        if self.px_in(x, y) {
            let i = y as usize * self.pw + x as usize;
            self.px[i] = self.px[i].lerp(c, a);
        }
    }

    #[inline]
    pub fn px_add(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x - self.ox;
        let y = y + 2 * self.oy;
        if self.px_in(x, y) {
            let i = y as usize * self.pw + x as usize;
            self.px[i] += c;
        }
    }

    /// Soft disc blended into the pixel layer (used for smoke, snowman, etc.).
    pub fn px_disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, a: f32) {
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        // (nothing of it on the screen: nothing to do)
        let (sx0, sx1, sy0, sy1) = (x0 - self.ox, x1 - self.ox, y0 + 2 * self.oy, y1 + 2 * self.oy);
        if sx1 < 0 || sy1 < 0 || sx0 >= self.pw as i32 || sy0 >= self.ph as i32 {
            return;
        }
        // (well inside or well outside its edge: no square root needed)
        let (inner, outer) = ((r - 0.5).max(0.0).powi(2), (r + 0.5).powi(2));
        for y in y0..=y1 {
            let dy2 = (y as f32 + 0.5 - cy).powi(2);
            for x in x0..=x1 {
                let d2 = (x as f32 + 0.5 - cx).powi(2) + dy2;
                if d2 >= outer {
                    continue;
                }
                let cov = if d2 <= inner { 1.0 } else { (r + 0.5 - d2.sqrt()).clamp(0.0, 1.0) };
                if cov > 0.0 {
                    self.px_blend(x, y, c, a * cov);
                }
            }
        }
    }

    /// A night-painted colour as it looks at the current time of day.
    #[inline]
    pub fn grade(&self, c: Rgb) -> Rgb {
        if self.day <= 0.0 {
            return c;
        }
        let g = (c.r + c.g + c.b) / 3.0;
        let n = c.lerp(Rgb { r: g, g, b: g }, 0.25 * self.day);
        (n * (1.0 + 1.25 * self.day)).lerp(Rgb::hex(0xffb070) * (g * 1.6), 0.18 * self.day_warm)
    }

    /// Daylight for everything already on the glyph layer (the scene's
    /// glyphs, before the widgets go on top), and less glow from lights.
    pub fn grade_scene(&mut self, sun: f32) {
        if sun <= 0.0 {
            return;
        }
        let k = 1.0 + 0.7 * sun;
        for g in self.glyph.iter_mut().flatten() {
            g.fg = g.fg * k;
            if let Bg::Solid(c) = g.bg {
                g.bg = Bg::Solid(c * k);
            }
        }
        let l = 1.0 - 0.7 * sun;
        for c in self.light.iter_mut() {
            *c = *c * l;
        }
    }

    // ---------------- glyph layer ----------------

    #[inline]
    pub fn cell_in(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    /// Clear the speck (snowflake or star) at a cell, if there is one.
    pub fn clear_speck(&mut self, x: i32, y: i32) {
        if self.cell_in(x, y) {
            let i = y as usize * self.w + x as usize;
            if self.speck[i] {
                self.glyph[i] = None;
                self.speck[i] = false;
            }
        }
    }

    #[inline]
    pub fn put_g(&mut self, x: i32, y: i32, g: Glyph) {
        let x = x - self.ox;
        let y = y + self.oy;
        if self.cell_in(x, y) {
            let i = y as usize * self.w + x as usize;
            if self.keep_ui && self.glyph[i].is_some_and(|o| o.ui) {
                return;
            }
            if (self.cover_pictures || (self.cover_panels && !matches!(g.bg, Bg::Scene))) && !self.pictures.is_empty() {
                // (only the part of a picture under the panel is hidden)
                for p in self.pictures.iter_mut().filter(|p| x >= p.x && x < p.x + p.cols && y >= p.y && y < p.y + p.rows) {
                    p.hidden.push((x, y));
                }
            }
            let g = if self.ui { Glyph { ui: true, ..g } } else { g };
            self.glyph[i] = Some(g);
            self.speck[i] = self.specks;
        }
    }

    #[inline]
    pub fn get_g(&self, x: i32, y: i32) -> Option<Glyph> {
        let x = x - self.ox;
        let y = y + self.oy;
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
            self.put_g(cx, y, Glyph { ch, fg, bg, alpha: 1.0, lit, ui: false });
            cx += 1;
        }
        cx
    }

    /// A glyph drawn into the pixel layer, filling the pixel rectangle at
    /// (x, y) sized w x h (pixel coordinates; fractions are fine): sprites
    /// that keep their size at any resolution, and the zoomed view.
    #[allow(clippy::too_many_arguments)]
    pub fn big_glyph(&mut self, x: f32, y: f32, w: f32, h: f32, ch: char, col: Rgb, alpha: f32) {
        if ch == ' ' || w <= 0.0 || h <= 0.0 || alpha <= 0.0 {
            return;
        }
        let m = crate::raster::glyph_mask(ch);
        let (sx, sy) = (m.w as f32 / w, m.h as f32 / h);
        let (x0, x1) = (x.floor() as i32, (x + w).ceil() as i32);
        let (y0, y1) = (y.floor() as i32, (y + h).ceil() as i32);
        const N: usize = 2;
        for py in y0..y1 {
            for px in x0..x1 {
                let mut acc = 0.0;
                for j in 0..N {
                    let fy = (py as f32 + (j as f32 + 0.5) / N as f32 - y) * sy;
                    if fy < 0.0 || fy >= m.h as f32 {
                        continue;
                    }
                    let row = fy as usize * m.w;
                    for i in 0..N {
                        let fx = (px as f32 + (i as f32 + 0.5) / N as f32 - x) * sx;
                        if fx >= 0.0 && fx < m.w as f32 {
                            acc += m.a[row + fx as usize];
                        }
                    }
                }
                // crisp edges (pixel art), not a smudge of coverage
                let cov = if acc / (N * N) as f32 >= 0.45 { 1.0 } else { 0.0 };
                if cov > 0.0 {
                    self.px_blend(px, py, col, alpha.min(1.0));
                    if alpha > 0.4 {
                        let (x, y) = (px - self.ox, py + 2 * self.oy);
                        if self.px_in(x, y) {
                            self.soft[y as usize * self.pw + x as usize] = 1;
                        }
                    }
                }
            }
        }
    }

    /// One cell of a glyph sprite drawn `k` times bigger, as pixels, its
    /// top left at cell (x, y) (fractions fine). Daylight is applied the way
    /// `grade_scene` does it for glyphs.
    pub fn sprite_cell(&mut self, x: f32, y: f32, k: f32, g: Glyph) {
        let col = g.fg * (1.0 + 0.7 * self.day.max(0.0));
        self.big_glyph(x, y * 2.0, k, 2.0 * k, g.ch, col, g.alpha);
    }

    /// Magnify the view by `z` around the cell (fx, fy): the pixels scale
    /// up (crisp), the glow is bloomed first and then scaled smoothly, the
    /// scene's glyphs are drawn bigger (their backgrounds too), and text
    /// over the scene — speech bubbles, prompts, signs — keeps its size and
    /// moves with the view. `spare` is a scratch frame (any size). Returns
    /// the cell → source-cell transform (x0, y0): source = (x0 + x / z,
    /// y0 + y / z). Only `bounds` (cells from the top left) of the drawing
    /// is ever shown.
    /// Pictures placed before a zoom, moved and grown with it (the view's
    /// left column `x0` and top row `y0`, `z` times bigger).
    pub fn zoom_pictures(&mut self, z: f32, x0: f32, y0: f32) {
        let aspect = self.cell_aspect;
        self.zoom_pictures_only(z, x0, y0, aspect);
        // the scene's letters, drawn big (already where they go), under the
        // speech bubbles and signs put back over them
        let mut letters = std::mem::take(&mut self.zoom_glyphs);
        for &(x, y) in &self.zoom_text {
            for p in letters.iter_mut().filter(|p| x >= p.x && x < p.x + p.cols && y >= p.y && y < p.y + p.rows) {
                p.hidden.push((x, y));
            }
        }
        let n = self.pictures.len();
        self.pictures.splice(0..0, letters);
        let _ = n;
    }

    fn zoom_pictures_only(&mut self, z: f32, x0: f32, y0: f32, aspect: f32) {
        for p in &mut self.pictures {
            if let Some([x, y, w, h]) = p.fit {
                let r = [(x - x0) * z, (y - y0) * z, w * z, h * z];
                (p.x, p.y) = (r[0].floor() as i32, r[1].floor() as i32);
                (p.cols, p.rows) = ((r[0] + r[2]).ceil() as i32 - p.x, (r[1] + r[3]).ceil() as i32 - p.y);
                p.fit = Some(r);
                continue;
            }
            match p.sprite {
                Some((cx, feet, rows)) => {
                    let (cx, feet, rows) = ((cx - x0) * z, (feet - y0) * z, rows * z);
                    let wc = p.pic.w as f32 / p.pic.h.max(1) as f32 * rows * aspect;
                    let (x, y) = ((cx - wc / 2.0).floor() as i32, (feet - rows).floor() as i32);
                    p.cols = (cx + wc / 2.0).ceil() as i32 - x;
                    p.rows = feet.ceil() as i32 - y;
                    (p.x, p.y) = (x, y);
                    p.sprite = Some((cx, feet, rows));
                }
                None => {
                    let (x, y) = (((p.x as f32 - x0) * z).floor() as i32, ((p.y as f32 - y0) * z).floor() as i32);
                    p.cols = (p.cols as f32 * z).round() as i32;
                    p.rows = (p.rows as f32 * z).round() as i32;
                    (p.x, p.y) = (x, y);
                }
            }
        }
        let (w, h) = (self.w as i32, self.h as i32);
        self.pictures.retain(|p| p.x + p.cols > 0 && p.y + p.rows > 0 && p.x < w && p.y < h);
    }

    /// The zoomed view's window round (fx, fy): its top left as drawn
    /// (on whole source pixels, so nothing shimmers as it moves) and
    /// exactly (between them: what follows the villager is drawn by this,
    /// so it holds still while the view steps along under it). The same
    /// at its edges.
    pub fn zoom_window(&self, z: f32, fx: f32, fy: f32, bounds: (f32, f32)) -> ((f32, f32), (f32, f32)) {
        let (w, h) = (self.w as f32, self.h as f32);
        let (vw, vh) = (w / z, h / z);
        let (bw, bh) = (bounds.0.min(w), bounds.1.min(h));
        let x = (fx - vw * 0.5).clamp(0.0, (bw - vw).max(0.0).floor());
        let y = (fy - vh * 0.55).clamp(0.0, ((bh - vh).max(0.0) * 2.0).floor() / 2.0);
        ((x.floor(), (y * 2.0).floor() / 2.0), (x, y))
    }

    pub fn zoom(&mut self, z: f32, fx: f32, fy: f32, bounds: (f32, f32), spare: &mut Frame) -> (f32, f32) {
        if z <= 1.001 {
            return (0.0, 0.0);
        }
        let ((x0, y0), _) = self.zoom_window(z, fx, fy, bounds);
        if spare.w != self.w || spare.h != self.h {
            spare.resize(self.w, self.h);
        }
        std::mem::swap(&mut self.px, &mut spare.px);
        std::mem::swap(&mut self.light, &mut spare.light);
        std::mem::swap(&mut self.glyph, &mut spare.glyph);
        std::mem::swap(&mut self.soft, &mut spare.soft);
        self.glyph.fill(None);
        self.light.fill(Rgb::BLACK);
        let (vw, vh) = (self.w as f32 / z, self.h as f32 / z);
        let (pw, ph) = (self.pw, self.ph);
        let cols: Vec<usize> = (0..pw).map(|x| ((x0 + (x as f32 + 0.5) / z) as usize).min(pw - 1)).collect();
        for y in 0..ph {
            let sy = ((2.0 * y0 + (y as f32 + 0.5) / z) as usize).min(ph - 1);
            let (src, dst) = (&spare.px[sy * pw..(sy + 1) * pw], &mut self.px[y * pw..(y + 1) * pw]);
            for (d, &c) in dst.iter_mut().zip(&cols) {
                *d = src[c];
            }
            let (src, dst) = (&spare.soft[sy * pw..(sy + 1) * pw], &mut self.soft[y * pw..(y + 1) * pw]);
            for (d, &c) in dst.iter_mut().zip(&cols) {
                *d = src[c];
            }
        }
        // the lights, magnified with everything else (and spread over the
        // bigger area they cover), then bloomed here at full size: a tight
        // glow round each light, not one blurry wash scaled up
        // (smoothly, not in blocks: a light's glow is round at any size)
        let k = 1.0 / z.powf(0.5);
        let taps: Vec<(usize, usize, f32)> = (0..pw)
            .map(|x| {
                let f = (x0 + (x as f32 + 0.5) / z - 0.5).clamp(0.0, (pw - 1) as f32);
                let i = f as usize;
                (i, (i + 1).min(pw - 1), f.fract())
            })
            .collect();
        for y in 0..ph {
            let f = (2.0 * y0 + (y as f32 + 0.5) / z - 0.5).clamp(0.0, (ph - 1) as f32);
            let (r0, r1, ty) = (f as usize, (f as usize + 1).min(ph - 1), f.fract());
            let (a, b) = (&spare.light[r0 * pw..(r0 + 1) * pw], &spare.light[r1 * pw..(r1 + 1) * pw]);
            let dst = &mut self.light[y * pw..(y + 1) * pw];
            for (d, &(i0, i1, tx)) in dst.iter_mut().zip(&taps) {
                *d = a[i0].lerp(a[i1], tx).lerp(b[i0].lerp(b[i1], tx), ty) * k;
            }
        }
        self.glow_k = z;
        self.pre_on = false;
        let (ox, oy) = (self.ox, self.oy);
        (self.ox, self.oy) = (0, 0);
        let (cw, chh) = (self.w as i32, self.h as i32);
        let (cx0, cy0) = (x0.floor() as i32 - 1, y0.floor() as i32 - 1);
        let (cx1, cy1) = ((x0 + vw).ceil() as i32 + 1, (y0 + vh).ceil() as i32 + 1);
        let at = |x: i32, y: i32| -> Option<Glyph> {
            if x >= 0 && y >= 0 && x < cw && y < chh {
                spare.glyph[(y * cw + x) as usize]
            } else {
                None
            }
        };
        // text over the scene: marked so, or letters on a background of
        // their own (signs)
        let ui = |g: &Glyph| g.ui || (g.ch.is_alphanumeric() && !matches!(g.bg, Bg::Scene));
        // the scene's glyphs, bigger (with their backgrounds): as the font's
        // own letters drawn big (pictures), where the front-end shows them;
        // else as pixels
        let letters = pictures_ok() && ZOOM_LETTERS.load(std::sync::atomic::Ordering::Relaxed);
        self.zoom_glyphs.clear();
        self.zoom_text.clear();
        for cy in cy0.max(0)..cy1.min(chh) {
            for cx in cx0.max(0)..cx1.min(cw) {
                let Some(g) = at(cx, cy).filter(|g| !ui(g)) else { continue };
                let (px, py) = ((cx as f32 - x0) * z, (2.0 * (cy as f32 - y0)) * z);
                match g.bg {
                    Bg::Scene => {}
                    Bg::Solid(c) => self.fill_rect(px, py, z, 2.0 * z, c, 1.0),
                    Bg::Mix(c, a) => self.fill_rect(px, py, z, 2.0 * z, c, a),
                }
                if letters && g.ch != ' ' && g.alpha > 0.0 {
                    // (filling the big cell exactly: a line or a block runs
                    // on into the next one, as it does at 1×; on the same
                    // whole pixels its background was filled on, so no
                    // sliver of sky shows between them at 1.1× or 3.5×)
                    let pic = glyph_picture(g.ch, g.fg, g.alpha);
                    let (gx0, gx1) = (px.round(), (px + z).round());
                    let (gy0, gy1) = (py.round(), (py + 2.0 * z).round());
                    let (fx, fy, fw, fh) = (gx0, gy0 / 2.0, gx1 - gx0, (gy1 - gy0) / 2.0);
                    let (x, y) = (fx.floor() as i32, fy.floor() as i32);
                    let (cols, rws) = ((fx + fw).ceil() as i32 - x, (fy + fh).ceil() as i32 - y);
                    if fw > 0.0 && fh > 0.0 && x + cols >= 0 && y + rws >= 0 && x < self.w as i32 && y < self.h as i32 {
                        self.zoom_glyphs.push(Placed { x, y, cols, rows: rws, pic, sprite: None, fit: Some([fx, fy, fw, fh]), hidden: vec![] });
                    }
                } else {
                    self.big_glyph(px, py, z, 2.0 * z, g.ch, g.fg, g.alpha);
                }
            }
        }
        // text: each connected block keeps its shape, anchored by its
        // bottom middle (speech bubbles hang over heads)
        let mut seen = vec![false; (cw * chh) as usize];
        let mut stack = vec![];
        for cy in cy0.max(0)..cy1.min(chh) {
            for cx in cx0.max(0)..cx1.min(cw) {
                let i = (cy * cw + cx) as usize;
                if seen[i] || !at(cx, cy).is_some_and(|g| ui(&g)) {
                    continue;
                }
                let mut cells = vec![];
                stack.push((cx, cy));
                seen[i] = true;
                while let Some((x, y)) = stack.pop() {
                    cells.push((x, y));
                    for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                        if nx >= 0 && ny >= 0 && nx < cw && ny < chh {
                            let j = (ny * cw + nx) as usize;
                            if !seen[j] && at(nx, ny).is_some_and(|g| ui(&g)) {
                                seen[j] = true;
                                stack.push((nx, ny));
                            }
                        }
                    }
                }
                let (bx0, bx1) = cells.iter().fold((i32::MAX, i32::MIN), |(a, b), &(x, _)| (a.min(x), b.max(x)));
                let by1 = cells.iter().map(|c| c.1).max().unwrap_or(cy);
                let mid = (bx0 + bx1) as f32 * 0.5;
                let (ax, ay) = ((mid - x0) * z, (by1 as f32 + 1.0 - y0) * z);
                for (x, y) in cells {
                    let (dx, dy) = ((ax + (x as f32 - mid)).round() as i32, (ay - (by1 + 1 - y) as f32).round() as i32);
                    if dx >= 0 && dy >= 0 && dx < cw && dy < chh {
                        self.glyph[(dy * cw + dx) as usize] = at(x, y);
                        self.zoom_text.push((dx, dy));
                    }
                }
            }
        }
        (self.ox, self.oy) = (ox, oy);
        (x0, y0)
    }

    /// Nothing drawn in cell (x, y) on the screen (no glyph over it).
    pub fn clear_glyph(&mut self, x: i32, y: i32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.glyph[y as usize * self.w + x as usize] = None;
        }
    }

    /// Fill a pixel rectangle (fractions round to whole pixels) with `c`
    /// at opacity `a`.
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
        let (x0, x1) = (x.round() as i32, (x + w).round() as i32);
        let (y0, y1) = (y.round() as i32, (y + h).round() as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                self.px_blend(px, py, c, a);
            }
        }
    }

    /// Copy in a pixel image (and its light) `w` x `ph` pixels at the left,
    /// `oy` rows down (what doesn't fit is left out).
    pub fn paste(&mut self, px: &[Rgb], light: &[Rgb], glyph: &[Option<Glyph>], w: usize, ph: usize, oy: i32) {
        let n = w.min(self.pw);
        for cy in 0..ph / 2 {
            let dy = cy as i32 + oy;
            if dy < 0 || dy >= self.h as i32 || (cy + 1) * w > glyph.len() {
                continue;
            }
            let (s, d) = (cy * w, dy as usize * self.w);
            for x in 0..n.min(self.w) {
                if let Some(g) = glyph[s + x] {
                    self.glyph[d + x] = Some(g);
                }
            }
        }
        for y in 0..ph {
            let dy = y as i32 + 2 * oy;
            if dy < 0 || dy >= self.ph as i32 || (y + 1) * w > px.len() {
                continue;
            }
            let (s, d) = (y * w, dy as usize * self.pw);
            self.px[d..d + n].copy_from_slice(&px[s..s + n]);
            if (y + 1) * w <= light.len() {
                for (o, l) in self.light[d..d + n].iter_mut().zip(&light[s..s + n]) {
                    *o += *l;
                }
            }
        }
    }

    // ---------------- light ----------------

    /// Add emitted light at a pixel-space position (bilinear splat).
    pub fn light_px(&mut self, x: f32, y: f32, c: Rgb) {
        let x = x - self.ox as f32;
        let y = y + 2.0 * self.oy as f32;
        // just off the edge: its glow still reaches in, fading with the
        // distance (so a light coming into view doesn't pop on)
        const EDGE: f32 = 14.0;
        let (w, h) = (self.pw as f32, self.ph as f32);
        let out = (-x).max(x - (w - 1.0)).max(0.0).max((-y).max(y - (h - 1.0)).max(0.0));
        if out > 0.0 {
            if out < EDGE && w > 0.0 && h > 0.0 {
                let (cx, cy) = (x.clamp(0.0, w - 1.0) as usize, y.clamp(0.0, h - 1.0) as usize);
                let k = (1.0 - out / EDGE).powi(2);
                let i = cy * self.pw + cx;
                self.light[i] += c * k;
            }
            return;
        }
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

    /// The light, bloomed: a tight halo and a wide pool, summed into
    /// `near` (the total light at each pixel).
    fn bloom(&mut self) {
        let (pw, ph) = (self.pw, self.ph);
        // near glow: tight halo (as far again as the view's bigger, zoomed in)
        let zk = self.glow_k.max(1.0);
        let near_r = (zk.round() as usize).max(1);
        let far_r = ((4.0 * zk).round() as usize).max(4);
        self.near.copy_from_slice(&self.light);
        for _ in 0..3 {
            box_blur(&mut self.near, &mut self.tmp, pw, ph, near_r);
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
            box_blur(&mut self.half, &mut self.half_tmp, hw, hh, far_r);
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
            self.near[i] = self.near[i] * gn + self.light[i] * gf;
        }
    }

    /// Pictures partly under panels (or under writing put over them):
    /// those parts cut away. `compose` does it; a UI layer, which isn't
    /// composed, has it done on its own.
    pub fn cut_hidden(&mut self) {
        if self.pictures.iter().any(|p| !p.hidden.is_empty()) {
            let aspect = self.cell_aspect;
            let all = std::mem::take(&mut self.pictures);
            self.pictures = all
                .into_iter()
                .filter_map(|mut p| {
                    if p.hidden.is_empty() {
                        return Some(p);
                    }
                    let cut = p.cut(aspect)?;
                    p.pic = cut;
                    p.hidden.clear();
                    Some(p)
                })
                .collect();
        }
    }

    pub fn compose(&mut self) {
        self.cut_hidden();
        self.bloom_tick = self.bloom_tick.wrapping_add(1);
        if !self.bloomed || self.bloom_every <= 1 || self.bloom_tick % self.bloom_every == 0 || self.view != self.bloom_view {
            self.bloom();
            self.bloomed = true;
            self.bloom_view = self.view;
        }
        let (pw, ph) = (self.pw, self.ph);
        // (the glow, and the light baked in beforehand, added as they're
        // read: the glow is kept as it is for the frames it's not worked
        // out again)
        let pre = (self.pre_on && self.pre.len() == pw * ph).then_some(&self.pre);
        let lit = |near: &[Rgb], i: usize| match pre {
            Some(p) => near[i] + p[i],
            None => near[i],
        };
        for i in 0..pw * ph {
            let l = lit(&self.near, i);
            // (a big sprite's pixels take light like a glyph: lit 0.6)
            self.px[i] += if self.soft[i] != 0 { l * 0.27 } else { l };
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
                        let light = (lit(&self.near, ti) + lit(&self.near, bi)) * 0.5;
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
        // (a room is built smaller than the frame and magnified later)
        if self.h > f.ph {
            return;
        }
        let w = self.w as i32;
        let shift = 2 * f.oy;
        for y in 0..self.h {
            let dy = y as i32 + shift;
            if dy < 0 || dy >= f.ph as i32 {
                continue;
            }
            let (src, dst) = (y * self.w, dy as usize * f.pw);
            for x in 0..f.pw {
                let sx = x0 + x as i32;
                if sx < 0 || sx >= w {
                    continue;
                }
                let i = src + sx as usize;
                let a = self.a[i];
                if a >= 0.999 {
                    f.px[dst + x] = f.grade(self.c[i]);
                } else if a > 0.0 {
                    f.px[dst + x] = f.px[dst + x].lerp(f.grade(self.c[i]), a);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixel art scaled up stays crisp: by a whole number, every pixel its
    /// own colour; by a fraction, all but the seams between them.
    #[test]
    fn pictures_grow_crisp() {
        // a 2×2 checker, black and white
        let (b, w) = ([0u8, 0, 0, 255], [255u8, 255, 255, 255]);
        let pic = Picture::new(2, 2, [b, w, w, b].concat());
        let pure = |buf: &[u32]| buf.iter().filter(|&&p| p == 0 || p == 0xff_ffff).count();
        // 8× (a whole number fits)
        let mut buf = vec![0x0080_8080u32; 16 * 16];
        pic.blit(&mut buf, 16, 0, 0, 16, 16);
        assert_eq!(pure(&buf), 256, "blended at a whole-number scale");
        // 2.5× (no whole number near enough): only the middle seam blends
        let mut buf = vec![0x0080_8080u32; 5 * 5];
        pic.blit(&mut buf, 5, 0, 0, 5, 5);
        assert!(pure(&buf) >= 16, "only {} of 25 crisp", pure(&buf));
    }

    /// Zoomed in, the scene's pixels get bigger but a speech bubble keeps
    /// its letters side by side (and its size).
    #[test]
    fn zoom_keeps_text_in_one_piece() {
        let mut f = Frame::new(80, 30);
        let mut spare = Frame::new(1, 1);
        for y in 0..60 {
            for x in 0..80 {
                f.px[y * 80 + x] = Rgb::hex(0x102030) * (1.0 + (x % 7) as f32 * 0.1);
            }
        }
        let bg = Bg::Mix(Rgb::hex(0xffffff), 0.9);
        f.text(30, 12, "hello", Rgb::BLACK, bg, 0.0);
        f.put_g(40, 20, Glyph::new('●', Rgb::hex(0xffffff)));
        f.zoom(2.0, 40.0, 15.0, (80.0, 30.0), &mut spare);
        // "hello", still in one row, letters adjacent
        let row: Vec<(usize, char)> = (0..30).flat_map(|y| (0..80).map(move |x| (x, y))).filter_map(|(x, y)| f.glyph[y * 80 + x].map(|g| (y * 80 + x, g.ch))).collect();
        let s: String = row.iter().map(|r| r.1).collect();
        assert_eq!(s, "hello", "{row:?}");
        assert!(row.windows(2).all(|w| w[1].0 == w[0].0 + 1));
        // the scene glyph became pixels (bigger), not a glyph
        assert!(f.soft.iter().filter(|&&v| v != 0).count() >= 4, "the ● wasn't drawn big");
    }

    /// Scene glyphs on a background of their own (a tree's needles) are
    /// magnified with the scene, background and all — not kept small like
    /// text (that left a small tree inside the magnified one).
    #[test]
    fn zoom_magnifies_solid_scene_glyphs() {
        let mut f = Frame::new(80, 30);
        let mut spare = Frame::new(1, 1);
        let green = Rgb::hex(0x1a5a2a);
        for y in 10..14 {
            for x in 38..42 {
                f.put_g(x, y, Glyph::new('^', Rgb::hex(0x2a8a3a)).solid(green).lit(0.6));
            }
        }
        let (_, y0) = f.zoom(2.0, 40.0, 12.0, (80.0, 30.0), &mut spare);
        assert!(f.glyph.iter().all(|g| g.is_none()), "tree glyphs kept as glyphs");
        // the 4x4 block of needles is now about 8 columns of green pixels
        let row = (((12.0 - y0) * 4.0) as usize + 1).min(59);
        let greenish = (0..80).filter(|&x| {
            let c = f.px[row * 80 + x];
            c.g > c.r && c.g > c.b
        });
        assert!(greenish.count() >= 6, "not magnified");
    }

    /// The glow's worked out only every other frame, but again whenever
    /// the view's moved: a light's glow is where the light is, not a
    /// column behind it for a frame.
    #[test]
    fn the_glow_moves_with_the_view() {
        let mut f = Frame::new(40, 10);
        f.bloom_every = 2;
        let glow_at = |f: &Frame| (0..f.pw).max_by(|&a, &b| f.near[5 * f.pw + a].r.total_cmp(&f.near[5 * f.pw + b].r)).unwrap();
        // (frame 4: not one it'd be worked out on, but the view's moved)
        for (frame, x, view) in [(0, 10.0, 1u64), (1, 10.0, 1), (2, 13.0, 1), (3, 16.0, 2), (4, 19.0, 3)] {
            f.clear();
            f.view = view;
            f.light_px(x + 0.5, 5.5, Rgb::hex(0xffffff));
            f.compose();
            match frame {
                // (the same view: kept from the frame before, every other)
                2 => assert_eq!(glow_at(&f), 10, "kept"),
                _ => assert_eq!(glow_at(&f), x as usize, "frame {frame}: the glow at {} not {x}", glow_at(&f)),
            }
        }
    }
}
