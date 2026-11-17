//! Cell-grid → pixels. Used by the PNG exporter and by the native window
//! backend (Windows, or `--windowed`). Like kitty, block elements, box
//! drawing and diagonals are drawn procedurally so lines join seamlessly;
//! everything else comes from bundled fonts so it looks the same everywhere.

use crate::frame::Cell;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

pub(crate) static FONT_DATA: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans.ttf"),
    include_bytes!("../assets/fonts/NotoSansSymbols2-Regular.ttf"),
];

struct Bitmap {
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    a: Vec<u8>,
}

pub struct Raster {
    pub cw: usize,
    pub ch: usize,
    fonts: Vec<Font>,
    px: f32,
    baseline: f32,
    cache: HashMap<char, Bitmap>,
}

#[inline]
pub fn pack(c: [u8; 3]) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

#[inline]
fn mix(bg: u32, fg: [u8; 3], a: f32) -> u32 {
    if a >= 0.999 {
        return pack(fg);
    }
    let br = ((bg >> 16) & 0xff) as f32;
    let bgc = ((bg >> 8) & 0xff) as f32;
    let bb = (bg & 0xff) as f32;
    let r = br + (fg[0] as f32 - br) * a;
    let g = bgc + (fg[1] as f32 - bgc) * a;
    let b = bb + (fg[2] as f32 - bb) * a;
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

impl Raster {
    /// `cw` x `ch` pixel cells. The font is sized so DejaVu Sans Mono's
    /// advance fills the cell width.
    pub fn new(cw: usize, ch: usize) -> Self {
        let fonts: Vec<Font> = FONT_DATA.iter().filter_map(|d| Font::from_bytes(*d, FontSettings::default()).ok()).collect();
        let px = (cw as f32 / 0.602).min(ch as f32 / 1.17);
        let baseline = fonts
            .first()
            .and_then(|f| f.horizontal_line_metrics(px))
            .map(|m| m.ascent + (ch as f32 - (m.ascent - m.descent)) / 2.0)
            .unwrap_or(ch as f32 * 0.75);
        Raster { cw, ch, fonts, px, baseline, cache: HashMap::new() }
    }

    fn glyph(&mut self, c: char) -> &Bitmap {
        if !self.cache.contains_key(&c) {
            let (cw, px, baseline) = (self.cw, self.px, self.baseline);
            let font = self.fonts.iter().find(|f| f.lookup_glyph_index(c) != 0).or(self.fonts.first());
            let bm = match font {
                Some(f) => {
                    let (mut m, mut a) = f.rasterize(c, px);
                    if m.width > cw + 2 {
                        let r = f.rasterize(c, px * (cw + 1) as f32 / m.width as f32);
                        m = r.0;
                        a = r.1;
                    }
                    let x = ((cw as i32 - m.width as i32) / 2).max(m.xmin.min(0));
                    let y = baseline as i32 - m.height as i32 - m.ymin;
                    Bitmap { x, y, w: m.width, h: m.height, a }
                }
                None => Bitmap { x: 0, y: 0, w: 0, h: 0, a: vec![] },
            };
            self.cache.insert(c, bm);
        }
        &self.cache[&c]
    }

    /// Draw one cell into a 0x00RRGGBB buffer.
    /// A cell drawn over what's there: its background at `bg_a` (0: none,
    /// see through), its letter at `fg_a`. (The UI's own layer, over the
    /// village.)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_cell_over(&mut self, buf: &mut [u32], stride: usize, x0: usize, y0: usize, ch: char, fg: [u8; 3], bg: [u8; 3], bg_a: f32, fg_a: f32) {
        let (cw, chh) = (self.cw, self.ch);
        if (y0 + chh) * stride > buf.len() || x0 + cw > stride {
            return;
        }
        if bg_a > 0.0 {
            for y in 0..chh {
                let row = (y0 + y) * stride + x0;
                for p in &mut buf[row..row + cw] {
                    *p = mix(*p, bg, bg_a.min(1.0));
                }
            }
        }
        if ch == ' ' || fg_a <= 0.0 {
            return;
        }
        if let Some(cov) = shape_coverage(ch, cw as f32, chh as f32) {
            for y in 0..chh {
                for x in 0..cw {
                    let a = cov(x as f32 + 0.5, y as f32 + 0.5);
                    if a > 0.0 {
                        let i = (y0 + y) * stride + x0 + x;
                        buf[i] = mix(buf[i], fg, (a * fg_a).min(1.0));
                    }
                }
            }
            return;
        }
        let g = self.glyph(ch);
        for gy in 0..g.h {
            let py = g.y + gy as i32;
            if py < 0 || py as usize >= chh {
                continue;
            }
            for gx in 0..g.w {
                let px = g.x + gx as i32;
                if px < 0 || px as usize >= cw {
                    continue;
                }
                let a = g.a[gy * g.w + gx];
                if a > 0 {
                    let i = (y0 + py as usize) * stride + x0 + px as usize;
                    buf[i] = mix(buf[i], fg, a as f32 / 255.0 * fg_a);
                }
            }
        }
    }

    pub fn draw_cell(&mut self, buf: &mut [u32], stride: usize, x0: usize, y0: usize, cell: &Cell) {
        let (cw, ch) = (self.cw, self.ch);
        let bgp = pack(cell.bg);
        for y in 0..ch {
            let row = (y0 + y) * stride + x0;
            buf[row..row + cw].fill(bgp);
        }
        if cell.ch == ' ' {
            return;
        }
        let fg = cell.fg;
        if let Some(cov) = shape_coverage(cell.ch, cw as f32, ch as f32) {
            for y in 0..ch {
                for x in 0..cw {
                    let a = cov(x as f32 + 0.5, y as f32 + 0.5);
                    if a > 0.0 {
                        let i = (y0 + y) * stride + x0 + x;
                        buf[i] = mix(buf[i], fg, a.min(1.0));
                    }
                }
            }
            return;
        }
        let g = self.glyph(cell.ch);
        for gy in 0..g.h {
            let py = g.y + gy as i32;
            if py < 0 || py as usize >= ch {
                continue;
            }
            for gx in 0..g.w {
                let px = g.x + gx as i32;
                if px < 0 || px as usize >= cw {
                    continue;
                }
                let a = g.a[gy * g.w + gx];
                if a > 0 {
                    let i = (y0 + py as usize) * stride + x0 + px as usize;
                    buf[i] = mix(buf[i], fg, a as f32 / 255.0);
                }
            }
        }
    }
}

type Cov = Box<dyn Fn(f32, f32) -> f32>;

/// Box-drawing description: arms (up, right, down, left), 0 none, 1 light,
/// 2 heavy, 3 double; plus dash count.
fn box_arms(c: char) -> Option<([u8; 4], u8)> {
    Some(match c {
        '─' => ([0, 1, 0, 1], 0),
        '│' => ([1, 0, 1, 0], 0),
        '┌' | '╭' => ([0, 1, 1, 0], 0),
        '┐' | '╮' => ([0, 0, 1, 1], 0),
        '└' | '╰' => ([1, 1, 0, 0], 0),
        '┘' | '╯' => ([1, 0, 0, 1], 0),
        '├' => ([1, 1, 1, 0], 0),
        '┤' => ([1, 0, 1, 1], 0),
        '┬' => ([0, 1, 1, 1], 0),
        '┴' => ([1, 1, 0, 1], 0),
        '┼' => ([1, 1, 1, 1], 0),
        '━' => ([0, 2, 0, 2], 0),
        '┃' => ([2, 0, 2, 0], 0),
        '┏' => ([0, 2, 2, 0], 0),
        '┓' => ([0, 0, 2, 2], 0),
        '┗' => ([2, 2, 0, 0], 0),
        '┛' => ([2, 0, 0, 2], 0),
        '╋' => ([2, 2, 2, 2], 0),
        '┻' => ([2, 2, 0, 2], 0),
        '╂' => ([2, 1, 2, 1], 0),
        '═' => ([0, 3, 0, 3], 0),
        '║' => ([3, 0, 3, 0], 0),
        '╨' => ([3, 1, 0, 1], 0),
        '╴' => ([0, 0, 0, 1], 0),
        '╶' => ([0, 1, 0, 0], 0),
        '╵' => ([1, 0, 0, 0], 0),
        '╷' => ([0, 0, 1, 0], 0),
        '╹' => ([2, 0, 0, 0], 0),
        '╻' => ([0, 0, 2, 0], 0),
        '╽' => ([1, 0, 2, 0], 0),
        '╿' => ([2, 0, 1, 0], 0),
        '┄' => ([0, 1, 0, 1], 3),
        '╌' => ([0, 1, 0, 1], 2),
        _ => return None,
    })
}

fn shape_coverage(c: char, w: f32, h: f32) -> Option<Cov> {
    let on = |b: bool| b as u8 as f32;
    if let Some((arms, dashes)) = box_arms(c) {
        let t1 = (w / 9.0).round().max(1.0);
        let t2 = (w / 4.5).round().max(2.0);
        let (cx, cy) = ((w / 2.0).floor(), (h / 2.0).floor());
        return Some(Box::new(move |x, y| {
            let thick = |k: u8| match k {
                1 => t1,
                2 => t2,
                _ => t1,
            };
            let mut a = 0.0f32;
            let in_band = |v: f32, centre: f32, t: f32| v >= centre - (t / 2.0).floor() && v < centre - (t / 2.0).floor() + t;
            let dash_ok = |along: f32, len: f32| -> bool {
                if dashes == 0 {
                    return true;
                }
                let seg = len / dashes as f32;
                (along % seg) < seg * 0.6
            };
            // horizontal arms
            for (k, from, to) in [(arms[3], 0.0, cx + t2), (arms[1], cx - t2 / 2.0, w)] {
                if k == 0 || x < from || x > to || !dash_ok(x, w) {
                    continue;
                }
                if k == 3 {
                    if in_band(y, cy - t1 - 1.0, t1) || in_band(y, cy + t1 + 1.0, t1) {
                        a = 1.0;
                    }
                } else if in_band(y, cy, thick(k)) {
                    a = 1.0;
                }
            }
            // vertical arms
            for (k, from, to) in [(arms[0], 0.0, cy + t2), (arms[2], cy - t2 / 2.0, h)] {
                if k == 0 || y < from || y > to {
                    continue;
                }
                if k == 3 {
                    if in_band(x, cx - t1 - 1.0, t1) || in_band(x, cx + t1 + 1.0, t1) {
                        a = 1.0;
                    }
                } else if in_band(x, cx, thick(k)) {
                    a = 1.0;
                }
            }
            a
        }));
    }
    let f: Cov = match c {
        '▀' => Box::new(move |_, y| on(y < h / 2.0)),
        '▄' => Box::new(move |_, y| on(y >= h / 2.0)),
        '█' => Box::new(|_, _| 1.0),
        '▌' => Box::new(move |x, _| on(x < w / 2.0)),
        '▐' => Box::new(move |x, _| on(x >= w / 2.0)),
        '░' => Box::new(|_, _| 0.25),
        '▒' => Box::new(|_, _| 0.5),
        '▓' => Box::new(|_, _| 0.75),
        '▔' => Box::new(move |_, y| on(y < h / 8.0)),
        '▕' => Box::new(move |x, _| on(x >= w * 7.0 / 8.0)),
        '▏' => Box::new(move |x, _| on(x < w / 8.0)),
        '▁'..='▇' => {
            let n = (c as u32 - '▁' as u32 + 1) as f32;
            Box::new(move |_, y| on(y >= h - h * n / 8.0))
        }
        '▖' | '▗' | '▘' | '▝' | '▙' | '▚' | '▛' | '▜' | '▞' | '▟' => {
            let q: [bool; 4] = match c {
                '▖' => [false, false, true, false],
                '▗' => [false, false, false, true],
                '▘' => [true, false, false, false],
                '▝' => [false, true, false, false],
                '▙' => [true, false, true, true],
                '▚' => [true, false, false, true],
                '▛' => [true, true, true, false],
                '▜' => [true, true, false, true],
                '▞' => [false, true, true, false],
                _ => [false, true, true, true],
            };
            Box::new(move |x, y| {
                let i = (if y >= h / 2.0 { 2 } else { 0 }) + (if x >= w / 2.0 { 1 } else { 0 });
                on(q[i])
            })
        }
        '◢' => Box::new(move |x, y| (x / w + y / h - 1.0).mul_add(w, 0.5).clamp(0.0, 1.0)),
        '◣' => Box::new(move |x, y| ((w - x) / w + y / h - 1.0).mul_add(w, 0.5).clamp(0.0, 1.0)),
        '◤' => Box::new(move |x, y| (1.0 - x / w - y / h).mul_add(w, 0.5).clamp(0.0, 1.0)),
        '◥' => Box::new(move |x, y| (x / w - y / h).mul_add(w, 0.5).clamp(0.0, 1.0)),
        '╱' | '╲' => {
            let rising = c == '╱';
            let t = (w / 9.0).max(1.0);
            Box::new(move |x, y| {
                // distance from the cell diagonal, in pixels
                let (u, v) = (x / w, y / h);
                let d = if rising { u + v - 1.0 } else { u - v };
                let dist = d.abs() * (w * h) / (w * w + h * h).sqrt();
                (t / 2.0 + 0.5 - dist).clamp(0.0, 1.0)
            })
        }
        _ => return None,
    };
    Some(f)
}

/// A glyph's shape as coverage (0..1) on a fixed grid: used to draw glyphs
/// into the pixel layer at any size (sprites that scale with the scene,
/// and the zoomed view).
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
}

const MASK_W: usize = 32;
const MASK_H: usize = 64;

/// The (cached) coverage mask for a character.
pub fn glyph_mask(c: char) -> std::sync::Arc<Mask> {
    use std::sync::{Arc, Mutex, OnceLock};
    type Cache = (Option<Raster>, HashMap<char, Arc<Mask>>);
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let m = CACHE.get_or_init(|| Mutex::new((None, HashMap::new())));
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mask) = g.1.get(&c) {
        return mask.clone();
    }
    let r = g.0.get_or_insert_with(|| Raster::new(MASK_W, MASK_H));
    let mut buf = vec![0u32; MASK_W * MASK_H];
    r.draw_cell(&mut buf, MASK_W, 0, 0, &Cell { ch: c, fg: [255, 255, 255], bg: [0, 0, 0] });
    let mask = Arc::new(Mask { w: MASK_W, h: MASK_H, a: buf.iter().map(|p| (p & 0xff) as f32 / 255.0).collect() });
    g.1.insert(c, mask.clone());
    mask
}
