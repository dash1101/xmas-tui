//! The village's cells (and the UI's layers over them) into pixels, for a
//! front end that draws its own letters: the native window (Windows, or
//! `--windowed`) and the Linux wallpaper drawn straight onto the desktop
//! (`wlwall`). Only what changed since the last frame is drawn again, and
//! what was drawn is said, so only that goes to the screen.

use crate::app::App;
use crate::frame::Cell;
use crate::raster::Raster;

/// A layer of the UI as drawn: its grid and cell size (uw, uh, cw, ch),
/// its letters, what was drawn of it last frame (cells, pictures).
struct GLayer {
    grid: (usize, usize, usize, usize),
    raster: Raster,
    prev: Vec<Option<UiCell>>,
    pics: Vec<crate::frame::Placed>,
}

/// A cell of the UI's layer as drawn (to tell when it changes).
#[derive(Clone, Copy, PartialEq)]
struct UiCell {
    ch: char,
    fg: [u8; 3],
    bg: [u8; 3],
    bga: u8,
    fga: u8,
}

impl UiCell {
    fn of(g: &crate::frame::Glyph) -> UiCell {
        let (bg, bga) = match g.bg {
            crate::frame::Bg::Scene => ([0, 0, 0], 0),
            crate::frame::Bg::Solid(c) => (c.tonemap(), 255),
            crate::frame::Bg::Mix(c, a) => (c.tonemap(), (a.clamp(0.0, 1.0) * 255.0) as u8),
        };
        UiCell { ch: g.ch, fg: g.fg.tonemap(), bg, bga, fga: (g.alpha.clamp(0.0, 1.0) * 255.0) as u8 }
    }
}

/// What a frame drew: everything (`full`), or the cells drawn again (as
/// pixel rects, a row's run of cells each) and the band of rows they're in.
pub struct Drawn {
    pub full: bool,
    pub rects: Vec<(i32, i32, i32, i32)>,
    pub rows: Option<(usize, usize)>,
    /// The cells drawn again (the grid's, row by row).
    pub cells: Vec<bool>,
}

/// The picture of the village as it's drawn, kept between frames.
pub struct Blit {
    pub raster: Raster,
    /// The pixels (0xRRGGBB), `size` wide and high; the grid starts at
    /// `offset` (centred).
    pub fb: Vec<u32>,
    pub size: (u32, u32),
    pub offset: (usize, usize),
    /// Everything drawn again next frame.
    pub force: bool,
    prev: Vec<Cell>,
    /// Pictures drawn over the cells last frame.
    pics: Vec<crate::frame::Placed>,
    /// The UI's own layers (bigger letters over the village, for the parts
    /// of it set bigger), bottom to top.
    layers: Vec<GLayer>,
}

impl Blit {
    pub fn new() -> Self {
        Blit { raster: Raster::new(8, 16), fb: vec![], size: (0, 0), offset: (0, 0), force: true, prev: vec![], pics: vec![], layers: vec![] }
    }

    /// A screen `w` × `h` pixels of `cw` × `ch` cells: the grid that fits
    /// (cols, rows), centred, everything drawn again next frame.
    pub fn layout(&mut self, w: u32, h: u32, cw: usize, ch: usize) -> (usize, usize) {
        self.raster = Raster::new(cw, ch);
        self.size = (w, h);
        let (cols, rows) = ((w as usize / cw).max(1), (h as usize / ch).max(1));
        self.offset = ((w as usize).saturating_sub(cols * cw) / 2, (h as usize).saturating_sub(rows * ch) / 2);
        self.fb = vec![0x0003_050f; (w * h) as usize];
        self.prev.clear();
        self.force = true;
        (cols, rows)
    }

    /// The app's frame (stepped already) drawn: what changed since the
    /// last.
    pub fn draw(&mut self, app: &mut App) -> Drawn {
        let (cols, rows) = app.scene_size();
        let (cw, ch) = (self.raster.cw, self.raster.ch);
        // the UI's own layers: each one's grid (as the app has it), cells to
        // fit over the village's
        let want: Vec<(usize, usize, usize, usize)> = app.ui_layers().iter().map(|(f, _)| (f.w, f.h, (cols * cw / f.w.max(1)).max(1), (rows * ch / f.h.max(1)).max(1))).collect();
        // (the app finds the pointer on them by these: whole pixels, a
        // little off each layer's size)
        app.layer_px = want.iter().map(|&(_, _, ucw, uch)| (ucw as f32 / cw.max(1) as f32, uch as f32 / ch.max(1) as f32)).collect();
        if want.len() != self.layers.len() || want.iter().zip(&self.layers).any(|(a, l)| *a != l.grid) {
            self.layers = want.iter().map(|&grid| GLayer { grid, raster: Raster::new(grid.2, grid.3), prev: vec![], pics: vec![] }).collect();
            self.force = true;
        }
        let (cells, pics) = app.output();
        let pics = pics.to_vec();
        let now: Vec<(Vec<Option<UiCell>>, Vec<crate::frame::Placed>)> = app.ui_layers().iter().map(|(f, _)| (f.glyph.iter().map(|g| g.as_ref().map(UiCell::of)).collect(), f.pictures.clone())).collect();
        let stride = self.size.0 as usize;
        let full = self.force || self.prev.len() != cells.len();
        // (a grid bigger than the pixels: nothing that can be drawn)
        if cols * cw + self.offset.0 > stride || rows * ch + self.offset.1 > self.size.1 as usize || cells.len() < cols * rows {
            return Drawn { full: false, rects: vec![], rows: None, cells: vec![] };
        }
        // the village's cells under a layer's: (x0, x1, y0, y1)
        let span = |grid: (usize, usize, usize, usize), ux: usize, uy: usize, n: usize, m: usize| {
            let (_, _, ucw, uch) = grid;
            ((ux * ucw / cw).min(cols), ((ux + n) * ucw).div_ceil(cw).min(cols), (uy * uch / ch).min(rows), ((uy + m) * uch).div_ceil(ch).min(rows))
        };
        // which of the village's cells are drawn again: the ones that
        // changed, or under pictures (real icons: redrawn every frame, so a
        // picture always goes onto fresh cells; and last frame's, so a moved
        // one leaves nothing behind)...
        let mut need = vec![false; cols * rows];
        for (i, n) in need.iter_mut().enumerate() {
            *n = full || cells[i] != self.prev[i];
        }
        let mark = |need: &mut Vec<bool>, (x0, x1, y0, y1): (usize, usize, usize, usize)| {
            for y in y0..y1 {
                for x in x0..x1 {
                    need[y * cols + x] = true;
                }
            }
        };
        for p in pics.iter().chain(self.pics.iter()) {
            // (a sprite's cells are approximate: a cell more all round)
            let m = p.sprite.is_some() as i32;
            let (px, py, pc, pr) = (p.x - m, p.y - m, p.cols + 2 * m, p.rows + 2 * m);
            let c = |v: i32, n: usize| (v.max(0) as usize).min(n);
            mark(&mut need, (c(px, cols), c(px + pc, cols), c(py, rows), c(py + pr, rows)));
        }
        // ...or under a layer's cell that changed (or went), or its pictures
        let mut dirty: Vec<Vec<bool>> = vec![];
        for (l, (ui_now, pics_now)) in self.layers.iter().zip(&now) {
            let uw = l.grid.0.max(1);
            let mut d = vec![false; ui_now.len()];
            for (i, c) in ui_now.iter().enumerate() {
                let was = l.prev.get(i).copied().flatten();
                if full || was != *c {
                    d[i] = true;
                    if was.is_some() || c.is_some() {
                        mark(&mut need, span(l.grid, i % uw, i / uw, 1, 1));
                    }
                }
            }
            for p in pics_now.iter().chain(l.pics.iter()) {
                mark(&mut need, span(l.grid, p.x.max(0) as usize, p.y.max(0) as usize, (p.cols + 1).max(0) as usize, (p.rows + 1).max(0) as usize));
            }
            dirty.push(d);
        }
        // a layer's cell drawn again has all of the village under it drawn
        // again first (so nothing see-through goes over itself twice); that
        // may take in more of the layers' cells: until it settles
        let redraw = |need: &[bool], (x0, x1, y0, y1): (usize, usize, usize, usize)| (y0..y1).any(|y| (x0..x1).any(|x| need[y * cols + x]));
        for _ in 0..4 {
            let mut grew = false;
            for (li, (l, (ui_now, _))) in self.layers.iter().zip(&now).enumerate() {
                let uw = l.grid.0.max(1);
                for (i, c) in ui_now.iter().enumerate() {
                    if c.is_none() {
                        continue;
                    }
                    let sp = span(l.grid, i % uw, i / uw, 1, 1);
                    if (dirty[li][i] || redraw(&need, sp)) && !(sp.2..sp.3).all(|y| (sp.0..sp.1).all(|x| need[y * cols + x])) {
                        mark(&mut need, sp);
                        grew = true;
                    }
                }
            }
            if !grew {
                break;
            }
        }
        let (ox, oy) = self.offset;
        let mut rects = vec![];
        for cy in 0..rows {
            let mut run: Option<usize> = None;
            for cx in 0..=cols {
                let on = cx < cols && need[cy * cols + cx];
                if on {
                    self.raster.draw_cell(&mut self.fb, stride, ox + cx * cw, oy + cy * ch, &cells[cy * cols + cx]);
                    run.get_or_insert(cx);
                } else if let Some(x0) = run.take() {
                    rects.push(((ox + x0 * cw) as i32, (oy + cy * ch) as i32, ((cx - x0) * cw) as i32, ch as i32));
                }
            }
        }
        // (the rows drawn again)
        let band = match (need.chunks(cols.max(1)).position(|r| r.iter().any(|&n| n)), need.chunks(cols.max(1)).rposition(|r| r.iter().any(|&n| n))) {
            (Some(a), Some(b)) => Some((oy + a * ch, oy + (b + 1) * ch)),
            _ => None,
        };
        self.prev = cells.to_vec();
        for p in &pics {
            if p.draw_fitted(&mut self.fb, stride, ox as i32, oy as i32, cw, ch) {
                continue;
            }
            match p.sprite_px(cw, ch) {
                Some((x, y, k)) => p.pic.blit_scaled(&mut self.fb, stride, ox as i32 + x, oy as i32 + y, k),
                None => p.pic.blit(&mut self.fb, stride, ox + p.x as usize * cw, oy + p.y as usize * ch, p.cols as usize * cw, p.rows as usize * ch),
            }
        }
        // the UI's layers over it all, bottom to top: their cells that
        // changed, or that sit over the village drawn again, then their
        // pictures
        for (li, (l, (ui_now, pics_now))) in self.layers.iter_mut().zip(now).enumerate() {
            let (uw, _, ucw, uch) = l.grid;
            let uw = uw.max(1);
            for (i, c) in ui_now.iter().enumerate() {
                let Some(c) = c else { continue };
                let (ux, uy) = (i % uw, i / uw);
                let sp = span(l.grid, ux, uy, 1, 1);
                if dirty[li][i] || redraw(&need, sp) {
                    // (a see-through panel: solid, the village behind it averaged)
                    let (bg, bga) = if c.bga > 0 && c.bga < 255 { (crate::frame::frost(cells, cols, sp, c.bg, c.bga), 255) } else { (c.bg, c.bga) };
                    l.raster.draw_cell_over(&mut self.fb, stride, ox + ux * ucw, oy + uy * uch, c.ch, c.fg, bg, bga as f32 / 255.0, c.fga as f32 / 255.0);
                }
            }
            for p in &pics_now {
                if !p.draw_fitted(&mut self.fb, stride, ox as i32, oy as i32, ucw, uch) {
                    match p.sprite_px(ucw, uch) {
                        Some((x, y, k)) => p.pic.blit_scaled(&mut self.fb, stride, ox as i32 + x, oy as i32 + y, k),
                        None => p.pic.blit(&mut self.fb, stride, ox + p.x.max(0) as usize * ucw, oy + p.y.max(0) as usize * uch, p.cols as usize * ucw, p.rows as usize * uch),
                    }
                }
            }
            l.prev = ui_now;
            l.pics = pics_now;
        }
        self.pics = pics;
        let was_forced = std::mem::take(&mut self.force);
        Drawn { full: full || was_forced, rects, rows: band, cells: need }
    }
}

impl Blit {
    /// The pixel rects of the cells marked (the grid's, `cols` wide): runs
    /// along each row, a few cells' gap bridged (fewer, longer copies).
    pub fn rects_of(&self, cells: &[bool], cols: usize) -> Vec<(i32, i32, i32, i32)> {
        let (cw, ch) = (self.raster.cw, self.raster.ch);
        let (ox, oy) = self.offset;
        let mut out = vec![];
        for (cy, row) in cells.chunks(cols.max(1)).enumerate() {
            let mut run: Option<(usize, usize)> = None;
            for (cx, &on) in row.iter().enumerate() {
                if !on {
                    continue;
                }
                run = match run {
                    Some((a, b)) if cx <= b + 4 => Some((a, cx)),
                    Some((a, b)) => {
                        out.push(((ox + a * cw) as i32, (oy + cy * ch) as i32, ((b + 1 - a) * cw) as i32, ch as i32));
                        Some((cx, cx))
                    }
                    None => Some((cx, cx)),
                };
            }
            if let Some((a, b)) = run {
                out.push(((ox + a * cw) as i32, (oy + cy * ch) as i32, ((b + 1 - a) * cw) as i32, ch as i32));
            }
        }
        out
    }
}

impl Default for Blit {
    fn default() -> Self {
        Self::new()
    }
}
