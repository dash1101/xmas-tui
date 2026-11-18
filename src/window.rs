//! Native window front-end: draws the same cell grid itself (bundled fonts,
//! CPU raster, only changed cells re-drawn). On Windows it attaches itself
//! behind the desktop icons; with `--windowed` it's an ordinary window on
//! any OS (handy for testing, or for Lively Wallpaper's "application" mode).

use crate::app::{App, Btn, KeyIn, MouseIn};
use crate::config::Config;
use crate::frame::Cell;
use crate::raster::Raster;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

pub struct Opts {
    /// Attach behind the desktop icons (Windows) instead of a normal window.
    pub desktop: bool,
    /// Target number of text rows (sets the cell size).
    pub rows: u32,
    /// Start full screen (like a game) instead of in a window.
    pub fullscreen: bool,
}

struct Gfx {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    raster: Raster,
    fb: Vec<u32>,
    prev: Vec<Cell>,
    /// Pictures drawn over the cells last frame.
    pics: Vec<crate::frame::Placed>,
    size: (u32, u32),
    #[cfg_attr(not(windows), allow(dead_code))]
    origin: (i32, i32),
    offset: (usize, usize),
    force: bool,
    /// Too small to hold a single cell (e.g. minimized: Resized(0x0)):
    /// nothing is laid out or drawn until the next usable size.
    hidden: bool,
    /// Our HWND (Windows), for the desktop attach / liveness checks.
    #[cfg(windows)]
    hwnd: usize,
    /// The UI's own layers (bigger letters over the village, for the parts
    /// of it set bigger), bottom to top.
    layers: Vec<GLayer>,
}

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

/// Windows desktop mode: the window we were attached to is gone (Explorer
/// restarted or rebuilt its desktop) and we're waiting to attach a new one.
#[cfg(windows)]
struct Reattach {
    /// Next time to look.
    at: Instant,
    /// When Explorer's desktop was first seen again (we let it settle).
    ready: Option<Instant>,
    tries: u32,
}

struct Runner {
    cfg: Config,
    seed: u32,
    path: PathBuf,
    opts: Opts,
    gfx: Option<Gfx>,
    app: Option<App>,
    next: Instant,
    cursor: (f64, f64),
    left_down: bool,
    /// Windows desktop: the left press went to a desktop icon (Explorer's
    /// drag); where it's let go may be a shelf.
    #[cfg(windows)]
    icon_drag: bool,
    pointer: &'static str,
    #[cfg(windows)]
    polled: (bool, bool),
    /// Walking keys last seen down (Windows desktop mode, see winsys::desktop_keys).
    #[cfg_attr(not(windows), allow(dead_code))]
    keys: [bool; 18],
    /// The render scale the layout was last made for.
    scale: f32,
    /// The window has the keyboard (desktop mode: after a click on the
    /// village; keys then come as key events, not polled).
    focused: bool,
    /// Desktop mode: bring the keyboard here (until then, or until it's come).
    #[cfg(windows)]
    want_focus: Option<Instant>,
    /// Ctrl is held (Ctrl+wheel looks up into the sky).
    ctrl: bool,
    shift: bool,
    /// Desktop mode: the wallpaper is showing full screen, in front.
    wall_full: bool,
    paused: bool,
    /// Desktop mode: attach_to_desktop succeeded (false = we're the
    /// top-level fallback window).
    #[cfg(windows)]
    attached: bool,
    #[cfg(windows)]
    reattach: Option<Reattach>,
    /// Last seen (monitor rect, parent client origin), re-checked about once
    /// a second to follow resolution / layout / primary-monitor changes.
    #[cfg(windows)]
    geom: (Option<(i32, i32, i32, i32)>, (i32, i32)),
    #[cfg(windows)]
    last_geom_check: Instant,
    #[cfg(windows)]
    last_pause_check: Instant,
    /// How often resting has started or stopped (for the log).
    #[cfg(windows)]
    pause_changes: u32,
    /// Since when there's been a reason to rest (see `pause_reason`).
    #[cfg(windows)]
    rest_since: Option<Instant>,
    /// Resting, why (the village stands still and the widgets and online
    /// go on: drawn twice a second); popped out into a window of its own
    /// (another copy's online then: nothing drawn here at all).
    #[cfg(windows)]
    rest_why: Option<String>,
    #[cfg(windows)]
    popped: bool,
    /// A game running, the computer busy (see `crate::rest`).
    #[cfg(windows)]
    watch: Option<crate::rest::Watch>,
    /// When we attached (the layout is logged again a few seconds later,
    /// once everything has settled).
    #[cfg(windows)]
    logged_at: Option<Instant>,
    /// Window snow & lights overlay (Windows desktop mode, or `--windowed`
    /// with XMAS_WINFX=1 for testing). Dropped = stopped.
    #[cfg(windows)]
    winfx: Option<crate::winsnow::win::Overlay>,
    /// Something changed that should show at once (a menu opened).
    #[cfg_attr(not(windows), allow(dead_code))]
    draw_now: bool,
    /// Windows desktop: the cell last checked for whether a right-click
    /// there is the village's, and when (see winsys::rclick_target).
    #[cfg(windows)]
    rc_seen: ((i32, i32), Instant),
    /// Desktop mode: the mouse (or keyboard) in use until then. The desktop
    /// is polled often while it is, less while it's left alone.
    input_hot: Instant,
    /// Since when the pointer has been moving with the right-click hook
    /// hearing nothing (Windows dropped it?), and when it was last put back.
    #[cfg(windows)]
    hook_doubt: (Option<Instant>, Instant),
    /// Desktop mode: attach afresh at this time (after something that can
    /// leave the wallpaper hidden or stuck: see winsys::take_desktop_event),
    /// and why.
    #[cfg(windows)]
    refresh: Option<(Instant, &'static str)>,
    /// The wall clock at the last loop: a jump means the PC slept.
    #[cfg(windows)]
    wall_seen: std::time::SystemTime,
    /// Watchdog re-attaches lately (see winsys::attach_ok), and the first
    /// one's time: capped, in case the check is wrong for some layout.
    #[cfg(windows)]
    fixes: (u32, Instant),
}

/// XMAS_TEST_HANG / XMAS_TEST_PANIC (seconds after starting): trouble on
/// purpose, to see the guard (`guard.rs`) put it right. Only once per run of
/// restarts.
fn test_trouble() -> Option<(&'static str, f32)> {
    static T: std::sync::OnceLock<Option<(&'static str, f32)>> = std::sync::OnceLock::new();
    *T.get_or_init(|| {
        if std::env::var_os("XMAS_RESTARTS").is_some() {
            return None;
        }
        let secs = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        secs("XMAS_TEST_HANG").map(|s| ("hang", s)).or_else(|| secs("XMAS_TEST_PANIC").map(|s| ("panic", s)))
    })
}

fn cell_size(height: u32, rows: u32) -> (usize, usize) {
    let ch = (height / rows.max(10)).clamp(6, 64) as usize;
    let cw = ((ch as f32 * 0.474).round() as usize).max(3);
    (cw, ch)
}

impl Runner {
    fn layout(&mut self, size: PhysicalSize<u32>) {
        // the render scale: bigger cells (fewer rows) or smaller
        let scale = self.app.as_ref().map(|a| a.cfg.scale).unwrap_or(self.cfg.scale).clamp(0.5, 2.0);
        self.scale = scale;
        let rows = ((self.opts.rows as f32 / scale).round() as u32).max(10);
        let Some(g) = self.gfx.as_mut() else { return };
        let (w, h) = (size.width.max(1), size.height.max(1));
        let (cw, ch) = cell_size(h, rows);
        // smaller than one cell (minimized, or dragged tiny): the offset
        // maths below would underflow; just stop drawing until it's back
        g.hidden = (w as usize) < cw || (h as usize) < ch;
        if g.hidden {
            return;
        }
        g.raster = Raster::new(cw, ch);
        g.size = (w, h);
        let (cols, rows) = (w as usize / cw, h as usize / ch); // both >= 1 now
        g.offset = ((w as usize - cols * cw) / 2, (h as usize - rows * ch) / 2);
        g.fb = vec![0x0003_050f; (w * h) as usize];
        g.prev.clear();
        g.force = true;
        let _ = g.surface.resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        match self.app.as_mut() {
            Some(app) => app.resize(cols, rows),
            None => {
                let desktop = self.opts.desktop;
                let mut app = App::new(self.cfg.clone(), self.seed, &self.path, cols, rows, desktop, !desktop, true);
                app.set_windowed(!desktop);
                app.set_can_focus(desktop);
                // (this front-end draws its own letters: a UI layer of bigger
                // ones can go over the village)
                app.set_ui_layer_ok(true);
                app.start_audio();
                if crate::save::on() {
                    app.keep_saves();
                }
                self.app = Some(app);
            }
        }
        #[cfg(windows)]
        self.update_safe();
    }

    /// Windows desktop mode: how many rows at the bottom the taskbar
    /// covers (the village's tips and boxes keep above it).
    #[cfg(windows)]
    fn update_safe(&mut self) {
        let (Some(g), Some(app)) = (self.gfx.as_ref(), self.app.as_mut()) else { return };
        if !self.opts.desktop || g.hidden {
            return;
        }
        let px = crate::winsys::taskbar_bottom(g.hwnd as _).max(0) as usize;
        let ch = g.raster.ch.max(1);
        let rows = app.scene_size().1;
        let clear = (g.size.1 as usize).saturating_sub(px + g.offset.1) / ch;
        app.set_safe_bottom(rows.saturating_sub(clear));
    }

    /// Desktop mode: the wallpaper comes out in front, full screen, with
    /// the keyboard (on), or goes back behind the icons (off).
    #[cfg(windows)]
    fn set_wall_full(&mut self, on: bool) {
        let Some(g) = self.gfx.as_ref() else { return };
        if !self.opts.desktop || on == self.wall_full {
            return;
        }
        let hwnd = g.hwnd as windows_sys::Win32::Foundation::HWND;
        let (x, y, w, h) = (g.origin.0, g.origin.1, g.size.0 as i32, g.size.1 as i32);
        if on {
            crate::winsys::bring_to_front(hwnd, x, y, w, h);
        } else {
            self.attached = crate::winsys::back_to_desktop(hwnd, x, y, w, h);
        }
        self.wall_full = on;
        self.paused = false;
        if let Some(app) = self.app.as_mut() {
            app.set_keyboard(on);
            app.release_all();
        }
        if let Some(g) = self.gfx.as_mut() {
            g.force = true;
            g.window.request_redraw();
        }
    }

    /// Full screen on / off (a normal window).
    fn toggle_fullscreen(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let on = g.window.fullscreen().is_some();
        g.window.set_fullscreen(if on { None } else { Some(winit::window::Fullscreen::Borderless(None)) });
    }

    /// Physical pixel (window-relative) → cell (the UI's, which is what
    /// the app takes the pointer in: the village's own, without a layer).
    /// A pixel to where it is in the village's cells (fractions: each part
    /// of the UI, at its own size, takes its own cell from it).
    fn to_scene(&self, x: f64, y: f64) -> (f32, f32) {
        let Some(g) = self.gfx.as_ref() else { return (-1.0, -1.0) };
        (((x - g.offset.0 as f64) / g.raster.cw.max(1) as f64) as f32, ((y - g.offset.1 as f64) / g.raster.ch.max(1) as f64) as f32)
    }

    fn draw(&mut self) {
        let desktop = self.opts.desktop;
        let (Some(g), Some(app)) = (self.gfx.as_mut(), self.app.as_mut()) else { return };
        if g.hidden {
            return;
        }
        // the grid on screen (the village's cells), for putting a file
        // dragged off a shelf where it was let go (Windows desktop: the
        // window is at the monitor's origin)
        app.px_map = desktop.then(|| (g.origin.0 + g.offset.0 as i32, g.origin.1 + g.offset.1 as i32, g.raster.cw as i32, g.raster.ch as i32));
        app.screen_map = desktop.then(|| (g.size.0 as f32, g.size.1 as f32, g.offset.0 as f32, g.offset.1 as f32, g.raster.cw as f32, g.raster.ch as f32));
        app.frame.cell_aspect = g.raster.ch as f32 / g.raster.cw.max(1) as f32;
        app.step();
        let (cols, rows) = app.scene_size();
        let (cw, ch) = (g.raster.cw, g.raster.ch);
        // the UI's own layers: each one's grid (as the app has it), cells to
        // fit over the village's
        let want: Vec<(usize, usize, usize, usize)> = app.ui_layers().iter().map(|(f, _)| (f.w, f.h, (cols * cw / f.w.max(1)).max(1), (rows * ch / f.h.max(1)).max(1))).collect();
        // (the app finds the pointer on them by these: whole pixels, a
        // little off each layer's size)
        app.layer_px = want.iter().map(|&(_, _, ucw, uch)| (ucw as f32 / cw.max(1) as f32, uch as f32 / ch.max(1) as f32)).collect();
        if want.len() != g.layers.len() || want.iter().zip(&g.layers).any(|(a, l)| *a != l.grid) {
            g.layers = want.iter().map(|&grid| GLayer { grid, raster: Raster::new(grid.2, grid.3), prev: vec![], pics: vec![] }).collect();
            g.force = true;
        }
        let (cells, pics) = app.output();
        let pics = pics.to_vec();
        let now: Vec<(Vec<Option<UiCell>>, Vec<crate::frame::Placed>)> = app.ui_layers().iter().map(|(f, _)| (f.glyph.iter().map(|g| g.as_ref().map(UiCell::of)).collect(), f.pictures.clone())).collect();
        let stride = g.size.0 as usize;
        let full = g.force || g.prev.len() != cells.len();
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
            *n = full || cells[i] != g.prev[i];
        }
        let mark = |need: &mut Vec<bool>, (x0, x1, y0, y1): (usize, usize, usize, usize)| {
            for y in y0..y1 {
                for x in x0..x1 {
                    need[y * cols + x] = true;
                }
            }
        };
        for p in pics.iter().chain(g.pics.iter()) {
            // (a sprite's cells are approximate: a cell more all round)
            let m = p.sprite.is_some() as i32;
            let (px, py, pc, pr) = (p.x - m, p.y - m, p.cols + 2 * m, p.rows + 2 * m);
            let c = |v: i32, n: usize| (v.max(0) as usize).min(n);
            mark(&mut need, (c(px, cols), c(px + pc, cols), c(py, rows), c(py + pr, rows)));
        }
        // ...or under a layer's cell that changed (or went), or its pictures
        let mut dirty: Vec<Vec<bool>> = vec![];
        for (l, (ui_now, pics_now)) in g.layers.iter().zip(&now) {
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
            for (li, (l, (ui_now, _))) in g.layers.iter().zip(&now).enumerate() {
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
        for cy in 0..rows {
            for cx in 0..cols {
                let i = cy * cols + cx;
                if need[i] {
                    g.raster.draw_cell(&mut g.fb, stride, g.offset.0 + cx * cw, g.offset.1 + cy * ch, &cells[i]);
                }
            }
        }
        // (the rows drawn again: what's sent to the screen)
        let (y0, y1) = match (need.chunks(cols.max(1)).position(|r| r.iter().any(|&n| n)), need.chunks(cols.max(1)).rposition(|r| r.iter().any(|&n| n))) {
            (Some(a), Some(b)) => (a, b),
            _ => (usize::MAX, 0),
        };
        g.prev = cells.to_vec();
        for p in &pics {
            if p.draw_fitted(&mut g.fb, stride, g.offset.0 as i32, g.offset.1 as i32, cw, ch) {
                continue;
            }
            match p.sprite_px(cw, ch) {
                Some((x, y, k)) => p.pic.blit_scaled(&mut g.fb, stride, g.offset.0 as i32 + x, g.offset.1 as i32 + y, k),
                None => p.pic.blit(&mut g.fb, stride, g.offset.0 + p.x as usize * cw, g.offset.1 + p.y as usize * ch, p.cols as usize * cw, p.rows as usize * ch),
            }
        }
        // the UI's layers over it all, bottom to top: their cells that
        // changed, or that sit over the village drawn again, then their
        // pictures
        for (li, (l, (ui_now, pics_now))) in g.layers.iter_mut().zip(now).enumerate() {
            let (uw, _, ucw, uch) = l.grid;
            let uw = uw.max(1);
            for (i, c) in ui_now.iter().enumerate() {
                let Some(c) = c else { continue };
                let (ux, uy) = (i % uw, i / uw);
                let sp = span(l.grid, ux, uy, 1, 1);
                if dirty[li][i] || redraw(&need, sp) {
                    // (a see-through panel: solid, the village behind it averaged)
                    let (bg, bga) = if c.bga > 0 && c.bga < 255 { (crate::frame::frost(cells, cols, sp, c.bg, c.bga), 255) } else { (c.bg, c.bga) };
                    l.raster.draw_cell_over(&mut g.fb, stride, g.offset.0 + ux * ucw, g.offset.1 + uy * uch, c.ch, c.fg, bg, bga as f32 / 255.0, c.fga as f32 / 255.0);
                }
            }
            for p in &pics_now {
                if !p.draw_fitted(&mut g.fb, stride, g.offset.0 as i32, g.offset.1 as i32, ucw, uch) {
                    match p.sprite_px(ucw, uch) {
                        Some((x, y, k)) => p.pic.blit_scaled(&mut g.fb, stride, g.offset.0 as i32 + x, g.offset.1 as i32 + y, k),
                        None => p.pic.blit(&mut g.fb, stride, g.offset.0 + p.x.max(0) as usize * ucw, g.offset.1 + p.y.max(0) as usize * uch, p.cols as usize * ucw, p.rows as usize * uch),
                    }
                }
            }
            l.prev = ui_now;
            l.pics = pics_now;
        }
        g.pics = pics;
        let Ok(mut buf) = g.surface.buffer_mut() else { return };
        let (dy0, dy1) = if full || buf.age() != 1 {
            (0, g.size.1 as usize)
        } else if y0 == usize::MAX {
            (0, 0)
        } else {
            (g.offset.1 + y0 * ch, g.offset.1 + (y1 + 1) * ch)
        };
        if full || buf.age() != 1 {
            buf.copy_from_slice(&g.fb);
        } else if dy1 > dy0 {
            buf[dy0 * stride..dy1 * stride].copy_from_slice(&g.fb[dy0 * stride..dy1 * stride]);
        }
        g.force = false;
        if dy1 > dy0 {
            let rect = softbuffer::Rect {
                x: 0,
                y: dy0 as u32,
                width: NonZeroU32::new(g.size.0).unwrap(),
                height: NonZeroU32::new((dy1 - dy0) as u32).unwrap(),
            };
            let _ = buf.present_with_damage(&[rect]);
        } else {
            let _ = buf.present();
        }
        // pointer shape (only meaningful for a normal window)
        let want = app.pointer();
        if want != self.pointer && !self.opts.desktop {
            self.pointer = want;
            g.window.set_cursor(match want {
                "pointer" => CursorIcon::Pointer,
                "grab" => CursorIcon::Grab,
                "grabbing" => CursorIcon::Grabbing,
                "text" => CursorIcon::Text,
                _ => CursorIcon::Default,
            });
        }
    }

    /// Windows desktop mode: we sit under the icon layer, so read the mouse
    /// ourselves and only react to clicks on the bare desktop.
    #[cfg(windows)]
    fn poll_desktop_input(&mut self) {
        let p = crate::winsys::poll();
        let Some(g) = self.gfx.as_ref() else { return };
        let (lx, ly) = ((p.x - g.origin.0) as f64, (p.y - g.origin.1) as f64);
        let inside = lx >= 0.0 && ly >= 0.0 && lx < g.size.0 as f64 && ly < g.size.1 as f64;
        let (fx, fy) = self.to_scene(lx, ly);
        let (cx, cy) = (fx.floor() as i32, fy.floor() as i32);
        let (was_l, was_r) = self.polled;
        // (in use: moving over the village, a button down, the keyboard ours)
        if (inside && (lx, ly) != self.cursor) || p.left || p.right || self.focused || self.want_focus.is_some() {
            self.input_hot = Instant::now() + Duration::from_millis(1500);
        }
        self.polled = (p.left, p.right);
        let Some(app) = self.app.as_mut() else { return };
        let now = Instant::now();
        // the keyboard: ours (after a click on the village) or Explorer's
        let has = crate::winsys::has_focus(g.hwnd as _);
        if has != self.focused {
            self.focused = has;
            app.set_keyboard(has);
            app.release_all();
            self.keys = [false; 18];
        }
        // what a right-click here would be, worked out now (the hook can't
        // wait to ask): the village's, if something of its own is under the
        // pointer on the bare desktop, not on an icon
        if (cx, cy) != self.rc_seen.0 || now.duration_since(self.rc_seen.1) > Duration::from_millis(300) {
            self.rc_seen = ((cx, cy), now);
            let bare = inside && p.on_desktop && !crate::winsys::over_icon(p.x, p.y);
            let ours = bare && app.right_click_ours(fx, fy);
            // (the village's cell: the finest the UI's parts, at any size, are cut)
            let (cw, ch) = (g.raster.cw as i32, g.raster.ch as i32);
            let x0 = g.origin.0 + g.offset.0 as i32 + cx * cw;
            let y0 = g.origin.1 + g.offset.1 as i32 + cy * ch;
            crate::winsys::rclick_target((x0, y0, x0 + cw, y0 + ch), ours, bare);
        }
        // the wheel (and a touchpad's two-finger scrolls and pinches) over
        // the bare desktop, taken by the mouse hook
        for (delta, side, ctrl, shift) in crate::winsys::take_wheel() {
            self.input_hot = now + Duration::from_millis(1500);
            let notches = delta as f32 / 120.0;
            let ev = if ctrl {
                // a pinch, or Ctrl + the wheel: zoom
                MouseIn::Zoom((notches * 1.12f32.ln()).exp().clamp(0.5, 2.0))
            } else if delta % 120 != 0 {
                // a touchpad's smooth scroll, a few cells per notch's worth:
                // sideways along the village, up and down into the sky
                if side {
                    MouseIn::Pad(-notches * 4.0, 0.0)
                } else {
                    MouseIn::Pad(0.0, notches * 3.0)
                }
            } else if side {
                // (a tilt to the right turns the other way to the wheel)
                MouseIn::ScrollSide(delta > 0)
            } else if shift {
                MouseIn::ScrollSky(delta > 0)
            } else {
                MouseIn::Scroll(delta > 0)
            };
            app.mouse_at(ev, fx, fy);
            self.draw_now = true;
        }
        // a line to type (or a setting to edit): the keyboard, here
        if std::mem::take(&mut app.want_keys) {
            self.want_focus = Some(now + Duration::from_millis(700));
        }
        if let Some(until) = self.want_focus {
            if self.focused || now > until {
                if !self.focused {
                    eprintln!("xmas-tui: couldn't take the keyboard from the desktop");
                }
                self.want_focus = None;
            } else if !p.left {
                crate::winsys::take_focus(g.hwnd as _);
            }
        }
        // the pointer moving while the hook hears nothing: Windows dropped
        // it (it does that to a hook that was slow to answer); put it back
        if (lx, ly) != self.cursor {
            match crate::winsys::rclick_hook_quiet_ms() {
                Some(q) if q > 1500 => {
                    let since = *self.hook_doubt.0.get_or_insert(now);
                    if now.duration_since(since) > Duration::from_secs(2) && now.duration_since(self.hook_doubt.1) > Duration::from_secs(10) {
                        eprintln!("xmas-tui: the right-click hook stopped hearing the mouse; putting it back");
                        crate::winsys::rclick_rehook();
                        self.hook_doubt = (None, now);
                    }
                }
                _ => self.hook_doubt.0 = None,
            }
        }
        if inside {
            if (lx, ly) != self.cursor {
                self.cursor = (lx, ly);
                app.mouse_at(if p.left && self.left_down { MouseIn::Drag } else { MouseIn::Move }, fx, fy);
            }
            // a press on a desktop icon belongs to Explorer (open / drag the
            // icon), not to the scene or a widget under it
            if p.on_desktop && p.left && !was_l {
                if crate::winsys::over_icon(p.x, p.y) {
                    self.icon_drag = true;
                } else {
                    self.left_down = true;
                    // a click on the village: the keyboard comes here (every
                    // key, Ctrl and all), until something else is clicked
                    // (asked for over the next moment: Explorer handles the
                    // click itself too, and focuses its icons)
                    self.want_focus = Some(now + Duration::from_millis(700));
                    app.mouse_at(MouseIn::Down(Btn::Left), fx, fy);
                }
            }
            // an icon being dragged over a shelf lights it up
            if self.icon_drag && p.left {
                app.icon_drag(Some((fx, fy)));
            }
        }
        // an icon let go over a shelf (and not onto another icon) goes on it
        if self.icon_drag && !p.left {
            self.icon_drag = false;
            app.icon_drag(None);
            if inside && p.on_desktop && !crate::winsys::over_icon(p.x, p.y) {
                app.desktop_drop(fx, fy);
            }
        }
        // a right-click the hook took from the desktop: our menu if there's
        // something of ours there, else Windows' menu after all
        if let Some((sx, sy)) = crate::winsys::take_rclick() {
            let (cw, ch) = (g.raster.cw.max(1), g.raster.ch.max(1));
            let rx = (sx - g.origin.0 - g.offset.0 as i32) as f32 / cw as f32;
            let ry = (sy - g.origin.1 - g.offset.1 as i32) as f32 / ch as f32;
            if app.right_click(rx, ry) {
                // the menu shows now, not at the next frame
                self.draw_now = true;
            } else {
                crate::winsys::pass_rclick();
            }
        }
        if std::mem::take(&mut app.desktop_menu) {
            crate::winsys::pass_rclick();
        }
        // the release always counts, wherever it happens (e.g. a widget
        // dragged off onto another monitor): clamp it onto the grid, or the
        // drag would stay stuck
        if !p.left && self.left_down {
            self.left_down = false;
            let (sw, sh) = app.scene_size();
            app.mouse_at(MouseIn::Up(Btn::Left), fx.clamp(0.0, sw.max(1) as f32 - 0.01), fy.clamp(0.0, sh.max(1) as f32 - 0.01));
        }
        let _ = was_r;
        // walking around: keys go to Explorer's desktop window, so read
        // them while it's focused and feed presses / releases through
        // (unless the keyboard's ours: then they come as key events)
        let keys = if self.focused { None } else { crate::winsys::desktop_keys() }.unwrap_or([false; 18]);
        app.set_shift(keys[13]);
        let names = [
            KeyIn::Left,
            KeyIn::Right,
            KeyIn::Up,
            KeyIn::Down,
            KeyIn::Char('e'),
            KeyIn::Char('f'),
            KeyIn::Char('r'),
            KeyIn::Char('i'),
            KeyIn::Char('1'),
            KeyIn::Char('2'),
            KeyIn::Char('3'),
            KeyIn::Char('4'),
            KeyIn::Char('5'),
            KeyIn::Char('\0'),
            KeyIn::Char('q'),
            KeyIn::Char(' '),
            KeyIn::Esc,
            KeyIn::Char('c'),
        ];
        // polling sees every release
        app.set_exact_keys();
        for i in (0..names.len()).filter(|&i| i != 13) {
            match (self.keys[i], keys[i]) {
                (false, true) => app.key(names[i]),
                (true, false) => app.key_up(names[i]),
                _ => {}
            }
        }
        self.keys = keys;
    }

    /// Windows desktop mode, about once a second: are we still attached to
    /// a live desktop, and did the monitor / desktop geometry change?
    #[cfg(windows)]
    fn check_desktop(&mut self, el: &ActiveEventLoop) {
        use windows_sys::Win32::Foundation::HWND;
        // (the taskbar may have moved, or started hiding itself)
        self.update_safe();
        let Some(g) = self.gfx.as_mut() else { return };
        let hwnd = g.hwnd as HWND;
        if self.attached && !crate::winsys::still_attached(hwnd) {
            eprintln!("xmas-tui: lost the desktop window (Explorer restarted?); re-attaching");
            self.lost_desktop();
            return;
        }
        if self.attached && crate::winsys::keep_layered(hwnd) {
            eprintln!("xmas-tui: the window had lost its layered style; restored");
        }
        if self.attached && crate::winsys::keep_under_icons(hwnd) {
            eprintln!("xmas-tui: the window had come up over the desktop icons; put back under them");
        }
        // hidden behind a layer Explorer put in front of it (or hidden
        // outright): attach afresh, at most a few times an hour
        if self.attached && self.refresh.is_none() {
            if let Err(why) = crate::winsys::attach_ok(hwnd) {
                let now = Instant::now();
                if now.duration_since(self.fixes.1) > Duration::from_secs(3600) {
                    self.fixes = (0, now);
                }
                if self.fixes.0 < 6 {
                    self.fixes.0 += 1;
                    eprintln!("xmas-tui: the wallpaper isn't showing ({why}); attaching afresh");
                    self.refresh = Some((now, "watchdog"));
                }
            }
        }
        if self.logged_at.is_some_and(|at| at.elapsed() > Duration::from_secs(5)) {
            self.logged_at = None;
            eprintln!("xmas-tui: 5 s after attaching (paused={}):\n{}", self.paused, crate::winsys::describe(hwnd));
        }
        let mon = el.primary_monitor().or_else(|| el.available_monitors().next());
        let rect = mon.map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32));
        let geom = (rect, crate::winsys::parent_origin(hwnd));
        if geom == self.geom {
            return;
        }
        let rect_changed = geom.0 != self.geom.0;
        self.geom = geom;
        if let Some((x, y, w, h)) = rect {
            // the Resized event this causes re-runs layout()
            crate::winsys::reposition(hwnd, x, y, w, h);
            g.origin = (x, y);
            if rect_changed {
                self.restart_winfx(Some((x, y, w, h)));
            }
        }
    }

    /// Attach a fresh window behind the icons. The old one goes once the
    /// new one is in place (so the wallpaper doesn't blink); if attaching
    /// fails, the usual re-attach takes over.
    #[cfg(windows)]
    fn refresh_desktop(&mut self, el: &ActiveEventLoop, why: &str) {
        eprintln!("xmas-tui: attaching afresh ({why})");
        if std::mem::take(&mut self.left_down) {
            if let Some(app) = self.app.as_mut() {
                app.mouse(MouseIn::Up(Btn::Left), 0, 0);
            }
        }
        let old = self.gfx.take();
        let ok = self.create(el) && self.attached;
        drop(old);
        if !ok {
            self.lost_desktop();
        }
    }

    /// Our desktop window died with its parent: drop it and start looking
    /// for Explorer's new desktop (handled in about_to_wait). The App (and
    /// with it the scene, widgets and audio) lives on.
    #[cfg(windows)]
    fn lost_desktop(&mut self) {
        self.gfx = None;
        // finish a widget drag in progress (Up doesn't use the position)
        if std::mem::take(&mut self.left_down) {
            if let Some(app) = self.app.as_mut() {
                app.mouse(MouseIn::Up(Btn::Left), 0, 0);
            }
        }
        self.reattach = Some(Reattach { at: Instant::now() + Duration::from_millis(500), ready: None, tries: 0 });
    }

    /// (Re)start the window snow & lights overlay for monitor `rect`. It
    /// skips our own window (should attaching have failed, that's a
    /// borderless monitor-sized window that would look like a game).
    #[cfg(windows)]
    fn restart_winfx(&mut self, rect: Option<(i32, i32, i32, i32)>) {
        if !(self.opts.desktop || std::env::var_os("XMAS_WINFX").is_some()) {
            return;
        }
        drop(self.winfx.take());
        let own = self.gfx.as_ref().map(|g| g.hwnd).unwrap_or(0);
        if let (Some(rect), Some(app)) = (rect, self.app.as_mut()) {
            let fx = std::sync::Arc::new(std::sync::Mutex::new(app.fx_cfg()));
            self.winfx = Some(crate::winsnow::win::spawn(fx.clone(), rect, own));
            app.set_fx(fx);
        }
    }

    /// Create the window (and in desktop mode attach it behind the icons).
    /// False if the window couldn't be created.
    fn create(&mut self, el: &ActiveEventLoop) -> bool {
        let icon = winit::window::Icon::from_rgba(crate::icon::rgba(64), 64, 64).ok();
        let mut attrs = Window::default_attributes().with_title("xmas-tui").with_window_icon(icon);
        #[cfg(target_os = "linux")]
        {
            // the desktop finds the icon by this id (see icon::install_desktop_files)
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs = attrs.with_name("xmas-tui", "xmas-tui");
        }
        let mut origin = (0, 0);
        let mon = el.primary_monitor().or_else(|| el.available_monitors().next());
        // the monitor we cover (physical px), for the window-snow overlay
        #[cfg_attr(not(windows), allow(unused_variables))]
        let mon_rect = mon.as_ref().map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32));
        if self.opts.desktop {
            if let Some(m) = mon {
                let (p, s) = (m.position(), m.size());
                origin = (p.x, p.y);
                attrs = attrs
                    .with_decorations(false)
                    .with_position(PhysicalPosition::new(p.x, p.y))
                    .with_inner_size(PhysicalSize::new(s.width, s.height));
            }
            #[cfg(windows)]
            {
                use winit::platform::windows::WindowAttributesExtWindows;
                attrs = attrs.with_skip_taskbar(true);
            }
        } else {
            attrs = attrs.with_inner_size(PhysicalSize::new(1600u32, 900u32));
            if self.opts.fullscreen {
                attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(mon.clone())));
            }
        }
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("xmas-tui: can't create window: {e}");
                return false;
            }
        };
        #[cfg(windows)]
        let mut own_hwnd = 0usize;
        #[cfg(windows)]
        if self.opts.desktop {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = window.window_handle() {
                if let RawWindowHandle::Win32(w32) = h.as_raw() {
                    let s = window.inner_size();
                    let hwnd = w32.hwnd.get() as windows_sys::Win32::Foundation::HWND;
                    own_hwnd = hwnd as usize;
                    self.attached = crate::winsys::attach_to_desktop(hwnd, origin.0, origin.1, s.width as i32, s.height as i32);
                    if !self.attached {
                        eprintln!("xmas-tui: couldn't find the desktop window; running as a normal window");
                    }
                    eprintln!("xmas-tui: monitor {:?}, window {}x{}\n{}", mon_rect, s.width, s.height, crate::winsys::describe(hwnd));
                    self.logged_at = Some(Instant::now());
                    // right-clicks on the village (not on icons) get its menus;
                    // sleep, unlock and display changes are looked out for
                    if self.attached {
                        crate::winsys::rclick_hook();
                        crate::winsys::watch_desktop_events();
                    }
                    self.geom = (mon_rect, crate::winsys::parent_origin(hwnd));
                }
            }
        }
        let context = Context::new(window.clone()).expect("softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("softbuffer surface");
        let size = window.inner_size();
        self.gfx = Some(Gfx {
            window: window.clone(),
            surface,
            raster: { crate::frame::set_pictures_ok(true); Raster::new(8, 17) },
            fb: vec![],
            prev: vec![],
            pics: vec![],
            size: (0, 0),
            origin,
            offset: (0, 0),
            force: true,
            hidden: false,
            #[cfg(windows)]
            hwnd: own_hwnd,
            layers: vec![],
        });
        self.layout(size);
        // window snow & lights in front of the other windows
        #[cfg(windows)]
        self.restart_winfx(mon_rect);
        self.next = Instant::now();
        window.request_redraw();
        true
    }

    /// Windows desktop mode after losing the desktop: once Explorer's new
    /// desktop exists (and had a moment to settle), create a fresh window
    /// and attach it. A failed attach is retried for a while before we
    /// settle for the top-level fallback window.
    #[cfg(windows)]
    fn try_reattach(&mut self, el: &ActiveEventLoop, now: Instant) {
        let Some(r) = self.reattach.as_mut() else { return };
        if now < r.at {
            return;
        }
        if !crate::winsys::desktop_ready() {
            r.ready = None;
            r.at = now + Duration::from_secs(1);
            return;
        }
        if r.ready.is_none() {
            r.ready = Some(now);
            r.at = now + Duration::from_millis(1500);
            return;
        }
        let tries = r.tries + 1;
        let ok = self.create(el);
        if ok && self.attached {
            self.reattach = None;
            return;
        }
        if tries >= 20 {
            // give up: keep whatever we have (the fallback window, if any)
            self.reattach = None;
            if !ok {
                el.exit();
            }
            return;
        }
        // not attached (yet): throw this window away and try again later
        self.gfx = None;
        self.reattach = Some(Reattach { at: now + Duration::from_secs(2), ready: None, tries });
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        #[cfg(windows)]
        if self.reattach.is_some() {
            return;
        }
        if !self.create(el) {
            el.exit();
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        // take the overlay down with us (destroys its window on its thread)
        #[cfg(windows)]
        drop(self.winfx.take());
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Destroyed => {
                // a window we replaced ourselves (re-attach): nothing to do
                if self.gfx.as_ref().map(|g| g.window.id()) != Some(id) {
                    return;
                }
                // attached to the desktop, our window dies with Explorer's
                // (restart / crash / desktop rebuilt): attach a new one
                #[cfg(windows)]
                if self.opts.desktop && self.attached {
                    eprintln!("xmas-tui: the desktop window went away (Explorer restarted?); re-attaching");
                    self.lost_desktop();
                    return;
                }
                el.exit();
            }
            WindowEvent::Resized(size) => self.layout(size),
            WindowEvent::Focused(on) => {
                // (behind the icons, the polling above follows the focus)
                if !self.opts.desktop {
                    self.focused = on;
                }
                if let Some(app) = self.app.as_mut() {
                    if !on {
                        app.release_all();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // resting: still drawn (twice a second), the village
                // standing still, the widgets and online going on; popped
                // out: nothing here at all
                #[cfg(windows)]
                {
                    if self.popped && self.paused {
                        return;
                    }
                    let why = if self.paused { Some(self.rest_why.clone().unwrap_or_else(|| "nobody can see it".into())) } else { None };
                    if let Some(app) = self.app.as_mut() {
                        app.set_rest(why);
                    }
                    self.draw();
                }
                #[cfg(not(windows))]
                if !self.paused {
                    self.draw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                let (fx, fy) = self.to_scene(position.x, position.y);
                let drag = self.left_down;
                if let Some(app) = self.app.as_mut() {
                    app.mouse_at(if drag { MouseIn::Drag } else { MouseIn::Move }, fx, fy);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (fx, fy) = self.to_scene(self.cursor.0, self.cursor.1);
                let btn = match button {
                    MouseButton::Left => Btn::Left,
                    MouseButton::Right => Btn::Right,
                    _ => return,
                };
                if btn == Btn::Left {
                    self.left_down = state == ElementState::Pressed;
                }
                if let Some(app) = self.app.as_mut() {
                    app.mouse_at(if state == ElementState::Pressed { MouseIn::Down(btn) } else { MouseIn::Up(btn) }, fx, fy);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (fx, fy) = self.to_scene(self.cursor.0, self.cursor.1);
                let cell = self.gfx.as_ref().map(|g| (g.raster.cw.max(1) as f32, g.raster.ch.max(1) as f32)).unwrap_or((8.0, 16.0));
                let Some(app) = self.app.as_mut() else { return };
                match delta {
                    // a trackpad: smooth, both ways at once (along the
                    // village, and up into the sky); with Ctrl (how a pinch
                    // arrives on many desktops), zoom
                    MouseScrollDelta::PixelDelta(p) if !self.ctrl => app.mouse_at(MouseIn::Pad(p.x as f32 / cell.0, p.y as f32 / cell.1), fx, fy),
                    MouseScrollDelta::PixelDelta(p) => app.mouse_at(MouseIn::Zoom((p.y as f32 * 0.006).exp()), fx, fy),
                    // a wheel (a tilt sideways too); Ctrl: zoom; Shift: up
                    // into the sky
                    // a touchpad on Windows: fractions of a notch (smooth)
                    MouseScrollDelta::LineDelta(x, y) if self.ctrl && (x.fract() != 0.0 || y.fract() != 0.0) => app.mouse_at(MouseIn::Zoom((y * 1.12f32.ln()).exp().clamp(0.5, 2.0)), fx, fy),
                    MouseScrollDelta::LineDelta(x, y) if x.fract() != 0.0 || y.fract() != 0.0 => app.mouse_at(MouseIn::Pad(x * 4.0, y * 3.0), fx, fy),
                    MouseScrollDelta::LineDelta(x, y) => {
                        if x.abs() > y.abs() {
                            app.mouse_at(MouseIn::ScrollSide(x < 0.0), fx, fy);
                        } else if y != 0.0 {
                            let up = y > 0.0;
                            let ev = if self.ctrl {
                                MouseIn::Zoom(if up { 1.12 } else { 1.0 / 1.12 })
                            } else if self.shift {
                                MouseIn::ScrollSky(up)
                            } else {
                                MouseIn::Scroll(up)
                            };
                            app.mouse_at(ev, fx, fy);
                        }
                    }
                }
            }
            // a pinch (macOS): zoom
            WindowEvent::PinchGesture { delta, .. } if delta.is_finite() => {
                let (fx, fy) = self.to_scene(self.cursor.0, self.cursor.1);
                if let Some(app) = self.app.as_mut() {
                    app.mouse_at(MouseIn::Zoom((1.0 + delta as f32).clamp(0.5, 2.0)), fx, fy);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.ctrl = m.state().control_key();
                self.shift = m.state().shift_key();
                if let Some(app) = self.app.as_mut() {
                    app.set_shift(self.shift);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // F11: full screen on and off (a normal window only)
                if event.logical_key == Key::Named(NamedKey::F11) && event.state == ElementState::Pressed && !event.repeat {
                    if !self.opts.desktop {
                        self.toggle_fullscreen();
                    } else if let Some(app) = self.app.as_mut() {
                        app.toggle_wall_full();
                    }
                    return;
                }
                let Some(app) = self.app.as_mut() else { return };
                if event.logical_key == Key::Named(NamedKey::F1) && event.state == ElementState::Pressed && !event.repeat {
                    app.toggle_help();
                    return;
                }
                // Ctrl + = / - / 0: zoom (with Shift: resolution)
                if self.ctrl {
                    if let (Key::Character(s), ElementState::Pressed) = (&event.logical_key, event.state) {
                        for c in s.chars() {
                            app.ctrl_key(c, self.shift);
                        }
                    }
                    return;
                }
                let keys: Vec<KeyIn> = match &event.logical_key {
                    Key::Named(NamedKey::Enter) => vec![KeyIn::Enter],
                    Key::Named(NamedKey::Backspace) => vec![KeyIn::Backspace],
                    Key::Named(NamedKey::Escape) => vec![KeyIn::Esc],
                    Key::Named(NamedKey::ArrowUp) => vec![KeyIn::Up],
                    Key::Named(NamedKey::ArrowDown) => vec![KeyIn::Down],
                    Key::Named(NamedKey::ArrowLeft) => vec![KeyIn::Left],
                    Key::Named(NamedKey::ArrowRight) => vec![KeyIn::Right],
                    Key::Named(NamedKey::Space) => vec![KeyIn::Char(' ')],
                    Key::Character(s) => s.chars().map(KeyIn::Char).collect(),
                    _ => vec![],
                };
                app.set_exact_keys();
                for k in keys {
                    match (event.state, event.repeat) {
                        (ElementState::Pressed, false) => app.key(k),
                        (ElementState::Pressed, true) => app.key_repeat(k),
                        (ElementState::Released, _) => app.key_up(k),
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        crate::guard::beat();
        // testing the guard: XMAS_TEST_HANG / XMAS_TEST_PANIC = seconds in
        if let Some((what, at)) = test_trouble() {
            if crate::guard::uptime() > at {
                if what == "hang" {
                    eprintln!("xmas-tui: (test) hanging");
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
                panic!("(test) a panic");
            }
        }
        if self.app.as_ref().map(|a| a.quit).unwrap_or(false) || crate::tray::quit_requested() || crate::update::restart_requested() {
            crate::guard::leaving();
            el.exit();
            return;
        }
        let now = Instant::now();
        // the settings' full-screen button
        if self.app.as_mut().is_some_and(|a| std::mem::take(&mut a.fullscreen_toggle)) {
            self.toggle_fullscreen();
        }
        // the wallpaper itself full screen (or back behind the icons)
        #[cfg(windows)]
        if let Some(on) = self.app.as_mut().and_then(|a| a.wall_full_req.take()) {
            self.set_wall_full(on);
        }
        // render scale changed in settings: lay the grid out again
        if let Some(size) = self.app.as_ref().filter(|a| (a.cfg.scale - self.scale).abs() > 1e-3).and(self.gfx.as_ref().map(|g| PhysicalSize::new(g.size.0, g.size.1))) {
            self.layout(size);
        }
        #[cfg(windows)]
        if self.opts.desktop {
            // something that can leave the wallpaper hidden or stuck (waking
            // from sleep, unlocking, a display change…): attach afresh once
            // things have settled
            if let Some(what) = crate::winsys::take_desktop_event() {
                self.refresh = Some((now + Duration::from_millis(1500), what));
            }
            // the wall clock jumped ahead: the PC was asleep (a laptop's lid
            // closed), even if the wake-up message never came
            let wall = std::time::SystemTime::now();
            if wall.duration_since(self.wall_seen).is_ok_and(|d| d > Duration::from_secs(15)) && self.refresh.is_none() {
                self.refresh = Some((now + Duration::from_millis(1500), "the PC was asleep"));
            }
            self.wall_seen = wall;
            if self.reattach.is_some() {
                self.refresh = None;
            } else if let Some((at, why)) = self.refresh {
                // (still locked or in a game: when it's back on show)
                if now >= at && self.gfx.is_some() && !self.wall_full && !self.paused {
                    self.refresh = None;
                    self.refresh_desktop(el, why);
                }
            }
            if self.reattach.is_some() {
                self.try_reattach(el, now);
            }
            if self.gfx.is_some() && !self.wall_full && now - self.last_geom_check >= Duration::from_secs(1) {
                self.last_geom_check = now;
                self.check_desktop(el);
            }
            // nobody can see the wallpaper during a fullscreen game or while
            // the session is locked: rest (checked a few times a second)
            if self.gfx.is_none() {
                self.paused = true;
            } else if self.paused || now - self.last_pause_check >= Duration::from_millis(250) {
                self.last_pause_check = now;
                let was = self.paused;
                let own = self.gfx.as_ref().map(|g| g.hwnd).unwrap_or(0);
                // (also while the village is popped out into a window of its own)
                let popped = self.app.as_mut().is_some_and(|a| a.popped_out());
                self.popped = popped;
                let why = if self.wall_full {
                    None
                } else if popped {
                    Some(("popped out into a window".to_string(), 0.0))
                } else {
                    crate::winsys::pause_reason(own as windows_sys::Win32::Foundation::HWND)
                };
                // a game running, or the computer busy, while the desktop
                // (or the village itself) isn't what has the focus
                let games = self.app.as_ref().is_some_and(|a| a.rest_for_games());
                if games && self.watch.is_none() {
                    self.watch = Some(crate::rest::spawn());
                }
                let why = why.or_else(|| {
                    if !games || self.wall_full || crate::winsys::desktop_has_focus(own as windows_sys::Win32::Foundation::HWND) {
                        return None;
                    }
                    self.watch.as_ref().and_then(|w| w.why()).map(|w| (w, 0.0))
                });
                // a reason has to hold for its delay before the wallpaper
                // rests (a flickering fullscreen verdict mustn't make it stutter)
                let why = match why {
                    Some((w, delay)) => {
                        let since = *self.rest_since.get_or_insert(now);
                        (now.duration_since(since).as_secs_f32() >= delay).then_some(w)
                    }
                    None => {
                        self.rest_since = None;
                        None
                    }
                };
                self.paused = why.is_some();
                self.rest_why = why.clone();
                if was != self.paused && self.gfx.is_some() {
                    // in the log, so a wallpaper that keeps stopping can be
                    // explained (the first 40 changes, then every 200th)
                    self.pause_changes += 1;
                    if self.pause_changes <= 40 || self.pause_changes % 200 == 0 {
                        match &why {
                            Some(w) => eprintln!("xmas-tui: resting ({w}) [change {}]", self.pause_changes),
                            None => eprintln!("xmas-tui: drawing again [change {}]", self.pause_changes),
                        }
                    }
                }
                if was && !self.paused {
                    // back: draw right away rather than at the end of a 500 ms rest
                    self.next = self.next.min(now);
                }
            }
            // right-clicks are only taken from the desktop while we draw
            crate::winsys::rclick_active(self.gfx.is_some() && self.attached && !self.paused && !self.wall_full);
            if !self.paused && !self.wall_full {
                self.poll_desktop_input();
                if std::mem::take(&mut self.draw_now) {
                    self.next = self.next.min(now);
                }
            }
        }
        let dt = if self.paused {
            Duration::from_millis(500)
        } else {
            self.app.as_ref().map(|a| a.frame_dt()).unwrap_or(Duration::from_millis(33))
        };
        if now >= self.next {
            self.next = if now - self.next > dt { now + dt } else { self.next + dt };
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        // poll input a few times per frame in desktop mode for snappy clicks
        // while the mouse is in use; less often left alone (a click still
        // lasts longer than that); not while paused (then only every 500 ms)
        let poll = Duration::from_millis(if now < self.input_hot { 8 } else { 24 });
        let wake = if self.opts.desktop && !self.paused && !self.wall_full { self.next.min(now + poll) } else { self.next };
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

pub fn run(cfg: Config, seed: u32, path: PathBuf, opts: Opts) -> Result<(), Box<dyn std::error::Error>> {
    let el = EventLoop::new()?;
    let mut r = Runner {
        cfg,
        seed,
        path,
        opts,
        gfx: None,
        app: None,
        next: Instant::now(),
        cursor: (-1.0, -1.0),
        left_down: false,
        #[cfg(windows)]
        icon_drag: false,
        pointer: "",
        #[cfg(windows)]
        polled: (false, false),
        keys: [false; 18],
        scale: 1.0,
        focused: false,
        #[cfg(windows)]
        want_focus: None,
        ctrl: false,
        shift: false,
        wall_full: false,
        paused: false,
        #[cfg(windows)]
        attached: false,
        #[cfg(windows)]
        reattach: None,
        #[cfg(windows)]
        geom: (None, (0, 0)),
        #[cfg(windows)]
        last_geom_check: Instant::now(),
        #[cfg(windows)]
        last_pause_check: Instant::now(),
        #[cfg(windows)]
        pause_changes: 0,
        #[cfg(windows)]
        rest_since: None,
        #[cfg(windows)]
        rest_why: None,
        #[cfg(windows)]
        popped: false,
        #[cfg(windows)]
        watch: None,
        #[cfg(windows)]
        logged_at: None,
        #[cfg(windows)]
        winfx: None,
        draw_now: false,
        #[cfg(windows)]
        refresh: None,
        #[cfg(windows)]
        wall_seen: std::time::SystemTime::now(),
        #[cfg(windows)]
        fixes: (0, Instant::now()),
        #[cfg(windows)]
        rc_seen: ((i32::MIN, i32::MIN), Instant::now()),
        input_hot: Instant::now(),
        #[cfg(windows)]
        hook_doubt: (None, Instant::now()),
    };
    el.run_app(&mut r)?;
    Ok(())
}
