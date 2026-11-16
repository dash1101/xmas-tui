//! TUI widgets drawn over the scene: a big glowing clock with a Christmas
//! countdown, a system panel (CPU/GPU/VRAM/RAM/disk/net), now-playing,
//! weather, the village map and a visualizer. Panels are "frosted": they
//! darken the scene behind them.

pub mod art;
pub mod data;
pub mod shelf;
pub mod viz;
#[cfg(windows)]
pub mod win;

use crate::color::{pal, Rgb};
use crate::config::Config;
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::noise1;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use data::{Music, SysStats, Weather};
#[cfg(not(windows))]
use data as sys_src;
#[cfg(windows)]
use win as sys_src;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const PANEL: Bg = Bg::Mix(pal::UI_PANEL, 0.80);
/// Clock, system, music, weather, map, visualizer; the shelves come after.
pub const BUILTIN: usize = 6;

/// Widget `i`'s saved position (fractions of the screen).
pub fn pos_of(cfg: &Config, i: usize) -> Option<[f32; 2]> {
    if i < BUILTIN {
        cfg.layout.get(i)
    } else {
        cfg.shelves.get(i - BUILTIN).and_then(|s| s.pos)
    }
}

pub fn set_pos(cfg: &mut Config, i: usize, v: Option<[f32; 2]>) {
    if i < BUILTIN {
        cfg.layout.set(i, v);
    } else if let Some(s) = cfg.shelves.get_mut(i - BUILTIN) {
        s.pos = v;
    }
}

/// A shelf's scroll and pointer state (not saved).
#[derive(Clone, Copy, Default)]
pub struct ShelfUi {
    pub scroll: usize,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Parks a data thread while its widget is switched off. The collector
/// loops (data.rs / win.rs) publish into their output mutex every cycle, so
/// holding that mutex from a helper thread blocks them right there after at
/// most one more poll: no more busctl spawns, /proc reads or PDH queries.
/// Dropping the `Park` releases the mutex and the collector carries on.
struct Park {
    _release: std::sync::mpsc::Sender<()>,
}

fn park<T: Send + 'static>(m: &Arc<Mutex<T>>) -> Park {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let m = m.clone();
    std::thread::spawn(move || {
        let _held = m.lock();
        let _ = rx.recv(); // returns once the Park (the sender) is dropped
    });
    Park { _release: tx }
}

pub struct Widgets {
    pub cfg: Config,
    sys: Option<Arc<Mutex<SysStats>>>,
    music: Option<Arc<Mutex<Music>>>,
    /// Some while the system / music widget is off (see `Park`).
    sys_park: Option<Park>,
    music_park: Option<Park>,
    weather: Option<Arc<Mutex<Weather>>>,
    weather_gen: Arc<AtomicU64>,
    viz: Option<Arc<Mutex<viz::Spectrum>>>,
    viz_gen: Arc<AtomicU64>,
    bars: VizBars,
    shelves: Option<shelf::Worker>,
    shelf_folders: Vec<std::path::PathBuf>,
    pub shelf_ui: Vec<ShelfUi>,
    /// An item being dragged off a shelf: (shelf, item), and where the
    /// pointer is.
    pub lift: Option<(usize, usize)>,
    pub lift_at: (i32, i32),
    /// Windows desktop: a desktop icon being dragged, at this cell.
    pub ext_drag: Option<(i32, i32)>,
    /// Where each widget was drawn last frame (for hit-testing).
    pub rects: Vec<Option<Rect>>,
    /// The village map's data for this frame (from the scene).
    pub map: Option<crate::scene::MiniMap>,
    pub hover: Option<usize>,
    pub dragging: Option<usize>,
}

impl Widgets {
    pub fn new(cfg: &Config) -> Self {
        let mut w = Widgets {
            cfg: cfg.clone(),
            sys: None,
            music: None,
            sys_park: None,
            music_park: None,
            weather: None,
            weather_gen: Arc::new(AtomicU64::new(0)),
            viz: None,
            viz_gen: Arc::new(AtomicU64::new(0)),
            bars: VizBars::default(),
            shelves: None,
            shelf_folders: vec![],
            shelf_ui: vec![],
            lift: None,
            lift_at: (0, 0),
            ext_drag: None,
            rects: vec![None; BUILTIN],
            map: None,
            hover: None,
            dragging: None,
        };
        w.ensure_threads(true);
        w
    }

    /// Start the data threads enabled widgets need; park / stop the ones
    /// whose widget was switched off.
    /// Some other app is playing media right now (per MPRIS / the Windows
    /// media sessions). Our own synthesized sound isn't a media session.
    pub fn media_playing(&self) -> bool {
        self.music.as_ref().and_then(|m| m.lock().ok().map(|g| g.present && g.playing)).unwrap_or(false)
    }

    fn ensure_threads(&mut self, weather_changed: bool) {
        if self.cfg.widgets.system && self.sys.is_none() {
            self.sys = Some(sys_src::spawn_sys(self.cfg.widgets.disk.clone()));
        }
        match (&self.sys, self.cfg.widgets.system) {
            (Some(s), false) if self.sys_park.is_none() => self.sys_park = Some(park(s)),
            (_, true) => self.sys_park = None,
            _ => {}
        }
        // the now-playing watcher also tells the sound when to go quiet for
        // other media, so it runs if either needs it
        let want_media = self.cfg.widgets.music || (self.cfg.sound.enabled && self.cfg.sound.pause_for_media);
        if want_media && self.music.is_none() {
            self.music = Some(sys_src::spawn_music());
        }
        match (&self.music, want_media) {
            (Some(m), false) if self.music_park.is_none() => self.music_park = Some(park(m)),
            (_, true) => self.music_park = None,
            _ => {}
        }
        if self.cfg.widgets.weather && (self.weather.is_none() || weather_changed) {
            let gen = self.weather_gen.fetch_add(1, Ordering::SeqCst) + 1;
            self.weather = Some(data::spawn_weather(self.cfg.weather.clone(), self.weather_gen.clone(), gen));
        } else if !self.cfg.widgets.weather && self.weather.take().is_some() {
            // the weather thread exits when its generation is superseded;
            // turning the widget back on starts a fresh one
            self.weather_gen.fetch_add(1, Ordering::SeqCst);
        }
        // the visualizer listens only while it's on (likewise by generation:
        // its thread ends the capture when superseded)
        if self.cfg.widgets.visualizer && self.viz.is_none() {
            let gen = self.viz_gen.fetch_add(1, Ordering::SeqCst) + 1;
            self.viz = Some(viz::spawn(self.viz_gen.clone(), gen));
        } else if !self.cfg.widgets.visualizer && self.viz.take().is_some() {
            self.viz_gen.fetch_add(1, Ordering::SeqCst);
            self.bars = VizBars::default();
        }
        // the shelves' worker scans their folders
        let folders: Vec<std::path::PathBuf> = self.cfg.shelves.iter().map(shelf::folder_of).collect();
        if !folders.is_empty() && self.shelves.is_none() {
            self.shelves = Some(shelf::spawn());
            self.shelf_folders.clear();
        }
        if folders != self.shelf_folders {
            if let Some(w) = &self.shelves {
                let _ = w.tx.send(shelf::Cmd::Folders(folders.clone()));
            }
            self.shelf_folders = folders;
        }
        self.shelf_ui.resize(self.cfg.shelves.len(), ShelfUi::default());
    }

    /// How many widgets there are (the built-in ones, then the shelves).
    pub fn count(&self) -> usize {
        BUILTIN + self.cfg.shelves.len()
    }

    /// What's on shelf `s` (once its folder has been scanned), in the order
    /// it was arranged in.
    pub fn contents(&self, s: usize) -> Option<shelf::Contents> {
        let folder = self.shelf_folders.get(s)?;
        let w = self.shelves.as_ref()?;
        let d = w.data.lock().ok()?;
        let mut c = d.get(s).filter(|c| &c.folder == folder).cloned()?;
        drop(d);
        let order = &self.cfg.shelves.get(s)?.order;
        if !order.is_empty() {
            let rank = |it: &std::sync::Arc<shelf::Item>| {
                let name = it.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                order.iter().position(|o| *o == name).unwrap_or(usize::MAX)
            };
            // (stable: the rest keep their folders-first, by-name order)
            c.items.sort_by_key(rank);
        }
        Some(c)
    }

    /// Shelf `s` with item `k` moved to slot `to`: the new order (file names).
    pub fn reordered(&self, s: usize, k: usize, to: usize) -> Option<Vec<String>> {
        let c = self.contents(s)?;
        let mut names: Vec<String> = c.items.iter().map(|it| it.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect();
        if k >= names.len() {
            return None;
        }
        let it = names.remove(k);
        names.insert(to.min(names.len()), it);
        Some(names)
    }

    pub fn shelf_folder(&self, s: usize) -> Option<std::path::PathBuf> {
        self.shelf_folders.get(s).cloned()
    }

    pub fn item_path(&self, s: usize, k: usize) -> Option<std::path::PathBuf> {
        self.contents(s)?.items.get(k).map(|it| it.path.clone())
    }

    pub fn send(&self, cmd: shelf::Cmd) {
        if let Some(w) = &self.shelves {
            let _ = w.tx.send(cmd);
        }
    }

    /// The shelf under (x, y), and what on it.
    pub fn shelf_hit(&self, x: i32, y: i32) -> Option<(usize, shelf::Hit)> {
        (0..self.cfg.shelves.len()).rev().find_map(|s| {
            let r = self.rects.get(BUILTIN + s).copied().flatten()?;
            let n = self.contents(s).map(|c| c.items.len()).unwrap_or(0);
            let ui = self.shelf_ui.get(s).copied().unwrap_or_default();
            shelf::hit(r, self.cfg.shelves[s].rolled, n, ui.scroll, x, y).map(|h| (s, h))
        })
    }

    /// Point at an item (or nothing) on the shelves.
    pub fn shelf_hover(&mut self, x: i32, y: i32) {
        let hit = self.shelf_hit(x, y);
        for (s, ui) in self.shelf_ui.iter_mut().enumerate() {
            ui.hover = match hit {
                Some((hs, shelf::Hit::Item(k))) if hs == s => Some(k),
                _ => None,
            };
        }
    }

    /// Scroll shelf `s` by `d` rows.
    pub fn shelf_scroll(&mut self, s: usize, d: i32) {
        let n = self.contents(s).map(|c| c.items.len()).unwrap_or(0);
        let Some(r) = self.rects.get(BUILTIN + s).copied().flatten() else { return };
        if let Some(ui) = self.shelf_ui.get_mut(s) {
            ui.scroll = (ui.scroll as i32 + d).clamp(0, shelf::max_scroll(r, n) as i32) as usize;
        }
    }

    /// Adopt a new config (from the settings panel or a file reload).
    pub fn apply(&mut self, cfg: &Config) {
        let wx_changed = cfg.weather != self.cfg.weather;
        self.cfg = cfg.clone();
        self.ensure_threads(wx_changed);
    }

    pub fn enabled(&self, i: usize) -> bool {
        if i >= BUILTIN {
            return i - BUILTIN < self.cfg.shelves.len();
        }
        match i {
            0 => self.cfg.clock.enabled,
            1 => self.cfg.widgets.system,
            2 => self.cfg.widgets.music,
            3 => self.cfg.widgets.weather,
            4 => self.cfg.widgets.map && self.map.as_ref().map(|m| m.ww > m.vw + 1.0).unwrap_or(false),
            _ => self.cfg.widgets.visualizer,
        }
    }

    /// The map's default size on a w-column screen.
    fn map_default(w: i32) -> (i32, i32) {
        ((w / 4).clamp(40, 160), 5)
    }

    pub fn size(&self, i: usize, w: i32) -> (i32, i32) {
        if let Some(s) = i.checked_sub(BUILTIN).and_then(|j| self.cfg.shelves.get(j)) {
            let sw = s.size[0].clamp(shelf::MIN_W, w.max(shelf::MIN_W));
            return (sw, if s.rolled { 1 } else { s.size[1].clamp(shelf::MIN_H, 60) });
        }
        match i {
            0 => (66, 11),
            1 => (54, 9),
            2 => (60, 6),
            3 => (60, 8),
            4 => match self.cfg.layout.map_size {
                Some([mw, mh]) => (mw.clamp(24, w.max(24)), mh.clamp(3, 20)),
                None => Self::map_default(w),
            },
            _ => (60, 8),
        }
    }

    fn default_pos(&self, i: usize, w: i32, h: i32) -> (i32, i32) {
        match i {
            0 => (3, 2),
            1 => (3, 14),
            2 => (w - 63, 2),
            3 => (w - 63, 9),
            // bottom middle, just above the taskbar
            4 => {
                let (mw, mh) = self.size(4, w);
                ((w - mw) / 2, h - mh - 3)
            }
            // under the weather
            5 => (w - 63, 18),
            // shelves: in the middle, each a little below the last
            _ => {
                let (sw, _) = self.size(i, w);
                let j = (i - BUILTIN) as i32;
                ((w - sw) / 2 + 3 * j, h / 3 + 2 * j)
            }
        }
    }

    /// Current rect for widget i on a w x h grid, clamped on-screen.
    pub fn rect(&self, i: usize, w: i32, h: i32) -> Rect {
        let (sw, sh) = self.size(i, w);
        let (x, y) = match pos_of(&self.cfg, i) {
            Some([fx, fy]) => ((fx * w as f32).round() as i32, (fy * h as f32).round() as i32),
            None => self.default_pos(i, w, h),
        };
        Rect { x: x.clamp(0, (w - sw).max(0)), y: y.clamp(0, (h - sh).max(0)), w: sw, h: sh }
    }

    /// Resize widget `i` (the map, or a shelf) by its `edges` (shelf::EDGE_*
    /// bits; the map only by its bottom-right corner) to reach (x, y).
    pub fn resize(&mut self, i: usize, edges: u8, x: i32, y: i32, w: i32, h: i32) {
        let r = self.rect(i, w, h);
        let keep = Some([r.x as f32 / w as f32, r.y as f32 / h as f32]);
        if i == 4 {
            let nw = (x - r.x + 1).clamp(24, (w - r.x).max(24));
            let nh = (y - r.y + 1).clamp(3, (h - r.y).clamp(3, 20));
            self.cfg.layout.map_size = Some([nw, nh]);
            self.cfg.layout.map = keep;
        } else if let Some(s) = i.checked_sub(BUILTIN).and_then(|j| self.cfg.shelves.get_mut(j)) {
            use shelf::{EDGE_B, EDGE_L, EDGE_R, EDGE_T, MIN_H, MIN_W};
            let (mut x0, mut y0, mut x1, mut y1) = (r.x, r.y, r.x + r.w - 1, r.y + r.h - 1);
            if edges & EDGE_L != 0 {
                x0 = x.min(x1 - MIN_W + 1).max(0);
            }
            if edges & EDGE_R != 0 {
                x1 = x.max(x0 + MIN_W - 1).min(w - 1);
            }
            if edges & EDGE_T != 0 {
                y0 = y.min(y1 - MIN_H + 1).max(0);
            }
            if edges & EDGE_B != 0 {
                y1 = y.max(y0 + MIN_H - 1).min(h - 1);
            }
            s.size = [x1 - x0 + 1, y1 - y0 + 1];
            s.pos = Some([x0 as f32 / w as f32, y0 as f32 / h as f32]);
        }
    }

    /// The widget whose resize grip (its bottom-right corner) is at (x, y).
    pub fn on_grip(&self, x: i32, y: i32) -> Option<usize> {
        // (shelves resize by any edge: see shelf_hit)
        self.rects[4].is_some_and(|r| x >= r.x + r.w - 2 && x < r.x + r.w && y == r.y + r.h - 1).then_some(4)
    }

    /// The world column under (x, y) on the map, if it's on the map.
    pub fn map_world_x(&self, x: i32, y: i32) -> Option<f32> {
        let r = self.rects[4]?;
        let m = self.map.as_ref()?;
        if !r.contains(x, y) {
            return None;
        }
        let iw = (r.w - 2).max(2);
        Some(((x - r.x - 1) as f32 / (iw - 1) as f32).clamp(0.0, 1.0) * m.ww)
    }

    pub fn move_to(&mut self, i: usize, x: i32, y: i32, w: i32, h: i32) {
        let (sw, sh) = self.size(i, w);
        let x = x.clamp(0, (w - sw).max(0));
        let y = y.clamp(0, (h - sh).max(0));
        set_pos(&mut self.cfg, i, Some([x as f32 / w as f32, y as f32 / h as f32]));
    }

    /// Top-most widget under a cell.
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.rects.len()).rev().find(|&i| self.rects[i].map(|r| r.contains(x, y)).unwrap_or(false))
    }

    pub fn draw(&mut self, f: &mut Frame, t: f32) {
        // speech bubbles and prompts stay readable on top of the widgets
        f.keep_ui = true;
        // a panel in front of someone hides them
        f.cover_panels = true;
        self.draw_all(f, t);
        f.cover_panels = false;
        f.keep_ui = false;
        // what's drawn after (settings, menus) hides real icons under it
        f.cover_pictures = true;
    }

    fn draw_all(&mut self, f: &mut Frame, t: f32) {
        self.rects = vec![None; self.count()];
        if f.w < 150 || f.h < 30 {
            return;
        }
        let (w, h) = (f.w as i32, f.h as i32);
        for i in 0..self.count() {
            if !self.enabled(i) {
                continue;
            }
            let r = self.rect(i, w, h);
            self.rects[i] = Some(r);
            match i {
                0 => {
                    self.draw_clock(f, r.x, r.y, t);
                }
                1 => {
                    let s = self.sys.as_ref().and_then(|s| s.lock().ok().map(|g| g.clone())).unwrap_or_default();
                    draw_system(f, r.x, r.y, &s, t);
                }
                2 => {
                    let m = self.music.as_ref().and_then(|m| m.lock().ok().map(|g| g.clone())).unwrap_or_default();
                    draw_music(f, r.x, r.y, r.w, &m, t);
                }
                3 => {
                    let wx = self.weather.as_ref().and_then(|w| w.lock().ok().map(|g| g.clone())).unwrap_or_default();
                    draw_weather(f, r.x, r.y, r.w, &wx, t);
                }
                4 => {
                    if let Some(m) = &self.map {
                        draw_map(f, r, m, t);
                    }
                }
                _ if i >= BUILTIN => {
                    let s = i - BUILTIN;
                    let contents = self.contents(s);
                    let ui = self.shelf_ui.get(s).copied().unwrap_or_default();
                    let lifted = self.lift.filter(|l| l.0 == s).map(|l| l.1);
                    let target = (self.lift.is_some_and(|l| l.0 != s) && r.contains(self.lift_at.0, self.lift_at.1)) || self.ext_drag.is_some_and(|(x, y)| r.contains(x, y));
                    // one of its own items carried about over it: where it'd go
                    let n = contents.as_ref().map(|c| c.items.len()).unwrap_or(0);
                    let insert = self.lift.filter(|l| l.0 == s && r.contains(self.lift_at.0, self.lift_at.1)).map(|_| shelf::slot_at(r, n, ui.scroll, self.lift_at.0, self.lift_at.1));
                    let view = shelf::View {
                        cfg: &self.cfg.shelves[s],
                        contents: contents.as_ref(),
                        scroll: ui.scroll,
                        hover: ui.hover,
                        selected: ui.selected,
                        lifted,
                        target,
                        insert,
                        locked: self.cfg.layout.locked,
                        seed: s as u32 * 7919 + 17,
                    };
                    shelf::draw(f, r, &view, t);
                }
                _ => {
                    let s = self.viz.as_ref().and_then(|v| {
                        let mut g = v.lock().ok()?;
                        let fresh = g.take(&mut self.bars.target);
                        Some((fresh, g.live, g.status.clone(), g.quiet_for))
                    });
                    let (live, status, quiet) = s.map(|(_, l, st, q)| (l, st, q)).unwrap_or((false, String::new(), 0.0));
                    self.bars.step(r.w - 4, bar_gap(self.cfg.widgets.visualizer_khz), t);
                    draw_viz(f, r, &self.bars, live, &status, quiet, self.cfg.widgets.visualizer_khz, t);
                }
            }
            let active = self.dragging == Some(i) || (self.dragging.is_none() && self.hover == Some(i));
            if active && !self.cfg.layout.locked {
                draw_handles(f, r, self.dragging == Some(i), t);
            }
        }
        // an item being carried off a shelf, under the pointer
        if let Some((s, k)) = self.lift {
            if let Some(it) = self.contents(s).and_then(|c| c.items.get(k).cloned()) {
                let (x, y) = self.lift_at;
                let bg = Bg::Mix(pal::UI_PANEL, 0.7);
                shelf::draw_pixels(f, x - 4, y - 2, &it.pixels, bg, 1.0);
                let label = clip(&it.name, 12);
                f.text(x - label.chars().count() as i32 / 2, y + 2, &label, Rgb::hex(0xfff1d6), bg, 0.0);
                let onto = match self.shelf_hit(x, y) {
                    Some((t, _)) if t == s => None,
                    Some((t, _)) => Some(format!("→ {}", self.cfg.shelves[t].name)),
                    None if self.hit(x, y).is_some() => None,
                    None => Some("→ desktop".to_string()),
                };
                if let Some(o) = onto {
                    f.text(x - o.chars().count() as i32 / 2, y + 3, &o, Rgb::hex(0xffd36b), bg, 0.0);
                }
            }
        }
    }

    /// Returns the next free row.
    fn draw_clock(&self, f: &mut Frame, x: i32, y: i32, t: f32) -> i32 {
        let now = crate::clock::now();
        let (h, ampm) = if self.cfg.clock.hour24 {
            (now.hour(), "")
        } else {
            let h = now.hour() % 12;
            (if h == 0 { 12 } else { h }, if now.hour() < 12 { "AM" } else { "PM" })
        };
        let s = format!("{}:{:02}", h, now.minute());
        let warm = Rgb::hex(0xfff1d6);
        // keep the clock's backdrop clean: no stars or snow between the
        // digits (trees, houses, their lights and speech bubbles stay)
        for yy in y..(y + 11) {
            for xx in x..(x + 66) {
                f.clear_speck(xx, yy);
            }
        }
        let mut cx = x;
        for ch in s.chars() {
            let rows = digit(ch);
            let wcols = if ch == ':' { 1 } else { 5 };
            for (ry, bits) in rows.iter().enumerate() {
                for bx in 0..wcols {
                    if bits >> (wcols - 1 - bx) & 1 == 1 {
                        let shimmer = 0.92 + 0.08 * noise1(t * 0.8 + (cx + bx as i32 * 2) as f32 * 0.15, 3);
                        let col = warm * shimmer;
                        for k in 0..2 {
                            let gx = cx + bx as i32 * 2 + k;
                            let gy = y + ry as i32;
                            f.put_g(gx, gy, Glyph::new('█', col).lit(0.0));
                            f.light_cell(gx, gy, Rgb::hex(0xffc878) * 0.16);
                        }
                    }
                }
            }
            cx += wcols as i32 * 2 + 2;
        }
        let mut tx = cx;
        if !ampm.is_empty() {
            f.text(tx, y + 5, ampm, pal::UI_BORDER, Bg::Scene, 0.0);
            tx += 3;
        }
        if self.cfg.clock.seconds {
            f.text(tx, y + 6, &format!(":{:02}", now.second()), pal::UI_DIM, Bg::Scene, 0.0);
        }
        let date = now.format("%A, %B %-d").to_string();
        f.text(x, y + 8, &date, pal::UI_TEXT, Bg::Scene, 0.0);
        let mut row = y + 9;
        if self.cfg.clock.countdown {
            draw_countdown(f, x, row, t);
            row += 1;
        }
        row + 1
    }
}

/// Bright corner brackets around a widget that can be (or is being) dragged.
/// The village map: mountains along the top, houses and trees along the
/// bottom, a highlight for what's on screen, the villager (●) and where they
/// started (⚑). The ◢ corner resizes it.
fn draw_map(f: &mut Frame, r: Rect, m: &crate::scene::MiniMap, t: f32) {
    let bg = Bg::Mix(pal::UI_PANEL, 0.78);
    let border = pal::UI_BORDER * 0.8;
    let (iw, ih) = ((r.w - 2).max(2), (r.h - 2).max(1));
    let col = |x: f32| r.x + 1 + ((x / m.ww).clamp(0.0, 1.0) * (iw - 1) as f32).round() as i32;
    // frame
    for x in r.x..r.x + r.w {
        let top = if x == r.x { '╭' } else if x == r.x + r.w - 1 { '╮' } else { '─' };
        let bot = if x == r.x { '╰' } else if x == r.x + r.w - 1 { '◢' } else { '─' };
        f.put_g(x, r.y, Glyph::new(top, border).bg(bg).lit(0.0));
        f.put_g(x, r.y + r.h - 1, Glyph::new(bot, if bot == '◢' { Rgb::hex(0xffd36b) * 0.8 } else { border }).bg(bg).lit(0.0));
    }
    for y in r.y + 1..r.y + r.h - 1 {
        f.put_g(r.x, y, Glyph::new('│', border).bg(bg).lit(0.0));
        f.put_g(r.x + r.w - 1, y, Glyph::new('│', border).bg(bg).lit(0.0));
        for x in r.x + 1..r.x + r.w - 1 {
            f.put_g(x, y, Glyph::new(' ', border).bg(bg).lit(0.0));
        }
    }
    f.text(r.x + 2, r.y, " map ", pal::UI_TITLE, bg, 0.0);
    // how far from where the villager started
    if let Some(p) = m.player {
        let d = p - m.home;
        let label = if d.abs() < 3.0 {
            " ⚑ home ".to_string()
        } else {
            format!(" ⚑ {} {} ", (d.abs() / 2.0).round() as i32, if d > 0.0 { "steps ←" } else { "steps →" })
        };
        let n = label.chars().count() as i32;
        if n + 9 < r.w {
            f.text(r.x + r.w - 2 - n, r.y, &label, pal::UI_DIM, bg, 0.0);
        }
    }
    // the part of the village on screen
    let (v0, v1) = (col(m.cam), col(m.cam + m.vw));
    let vbg = Bg::Mix(Rgb::hex(0x34407a), 0.85);
    for y in r.y + 1..r.y + r.h - 1 {
        for x in v0..=v1.min(r.x + r.w - 2) {
            f.put_g(x, y, Glyph::new(' ', border).bg(vbg).lit(0.0));
        }
    }
    let inview = |x: i32| x >= v0 && x <= v1;
    let cbg = |x: i32| if inview(x) { vbg } else { bg };
    let ground = r.y + r.h - 2;
    // mountains: a sparkline across the rows above the ground
    if ih >= 3 {
        const BARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let rows = (ih - 2).min(4);
        for cx in 0..iw {
            let i = (cx as usize * (m.ridge.len().max(1) - 1)) / (iw as usize - 1).max(1);
            let v = m.ridge.get(i).copied().unwrap_or(0.0) * rows as f32;
            for k in 0..rows {
                let fill = ((v - k as f32) * 8.0).round().clamp(0.0, 8.0) as usize;
                if fill == 0 {
                    continue;
                }
                let (x, y) = (r.x + 1 + cx, ground - 2 - k);
                f.put_g(x, y, Glyph::new(BARS[fill], Rgb::hex(0x5a6696)).bg(cbg(x)).lit(0.0));
            }
        }
    }
    // the ground: snow, then trees and houses on it
    for x in r.x + 1..r.x + r.w - 1 {
        f.put_g(x, ground, Glyph::new('▁', Rgb::hex(0xc8d0e4) * 0.6).bg(cbg(x)).lit(0.0));
    }
    for &tx in &m.trains {
        if ih >= 2 {
            let x = col(tx);
            for dx in -1..=1 {
                f.put_g(x + dx, ground - 1, Glyph::new('▬', Rgb::hex(0xa83a3a)).bg(cbg(x + dx)).lit(0.0));
            }
        }
    }
    for &(tx, big) in &m.trees {
        let x = col(tx);
        if big {
            f.put_g(x, ground, Glyph::new('▲', Rgb::hex(0x3fbf6a)).bg(cbg(x)).lit(0.0));
            if ih >= 2 {
                f.put_g(x, ground - 1, Glyph::new('★', Rgb::hex(0xffd65a)).bg(cbg(x)).lit(0.0));
            }
        } else {
            f.put_g(x, ground, Glyph::new('▴', Rgb::hex(0x2f9a52)).bg(cbg(x)).lit(0.0));
        }
    }
    for &(hx, church) in &m.houses {
        let x = col(hx);
        f.put_g(x, ground, Glyph::new('⌂', Rgb::hex(0xffc878)).bg(cbg(x)).lit(0.0));
        if church && ih >= 2 {
            f.put_g(x, ground - 1, Glyph::new('†', Rgb::hex(0xe8e0d0)).bg(cbg(x)).lit(0.0));
        }
    }
    for &(cx, person) in &m.critters {
        let x = col(cx);
        if ih >= 2 {
            let c = if person { Rgb::hex(0xe8c39e) } else { Rgb::hex(0xc0884a) };
            f.put_g(x, ground - 1, Glyph::new('·', c).bg(cbg(x)).lit(0.0));
        }
    }
    // home, then the villager (on top of everything)
    let hx = col(m.home);
    f.put_g(hx, ground - (ih >= 2) as i32, Glyph::new('⚑', Rgb::hex(0x7fd8ff)).bg(cbg(hx)).lit(0.0));
    if let Some(p) = m.player {
        let x = col(p);
        let k = 0.85 + 0.15 * (t * 4.0).sin();
        f.put_g(x, ground, Glyph::new('●', Rgb::hex(0xffd36b) * (1.2 * k)).bg(cbg(x)).lit(0.0));
    }
}

/// The visualizer's bars as shown: eased toward the latest levels, each
/// with a cap of snow that's pushed up by the bar and settles back slowly.
struct VizBars {
    /// The latest levels per band (from the capture thread).
    target: Vec<f32>,
    v: Vec<f32>,
    cap: Vec<f32>,
    cap_v: Vec<f32>,
    hold: Vec<f32>,
    last_t: Option<f32>,
}

impl Default for VizBars {
    fn default() -> Self {
        VizBars { target: vec![0.0; viz::BANDS], v: vec![], cap: vec![], cap_v: vec![], hold: vec![], last_t: None }
    }
}

const BAR_W: i32 = 2;

/// The gap between bars: wider with the kHz labels, so they don't run
/// into each other.
fn bar_gap(khz: bool) -> i32 {
    if khz {
        2
    } else {
        1
    }
}

impl VizBars {
    /// Advance to time `t` with bars across `iw` columns.
    fn step(&mut self, iw: i32, gap: i32, t: f32) {
        let n = ((iw + gap) / (BAR_W + gap)).max(1) as usize;
        if self.v.len() != n {
            self.v = vec![0.0; n];
            self.cap = vec![0.0; n];
            self.cap_v = vec![0.0; n];
            self.hold = vec![0.0; n];
        }
        let dt = self.last_t.map(|l| (t - l).clamp(0.0, 0.1)).unwrap_or(0.0);
        self.last_t = Some(t);
        let bands = self.target.len();
        for i in 0..n {
            let a = i * bands / n;
            let b = ((i + 1) * bands / n).max(a + 1).min(bands);
            let tgt = self.target[a..b].iter().cloned().fold(0.0, f32::max);
            // quick to rise, slower to fall
            let tau = if tgt > self.v[i] { 0.03 } else { 0.16 };
            self.v[i] += (tgt - self.v[i]) * (1.0 - (-dt / tau).exp());
            let v = self.v[i];
            if v >= self.cap[i] {
                (self.cap[i], self.cap_v[i], self.hold[i]) = (v, 0.0, 0.45);
            } else if self.hold[i] > 0.0 {
                self.hold[i] -= dt;
            } else {
                // the snow drifts down, gathering speed
                self.cap_v[i] += 0.8 * dt;
                self.cap[i] = (self.cap[i] - self.cap_v[i] * dt).max(v);
            }
        }
    }
}

/// Pine at the bottom of the bars, gold in the middle, candy red at the top.
fn viz_col(fr: f32) -> Rgb {
    let (green, gold, red) = (Rgb::hex(0x3fbf6a), Rgb::hex(0xffd36b), Rgb::hex(0xff4a4a));
    if fr < 0.5 {
        green.lerp(gold, fr * 2.0)
    } else {
        gold.lerp(red, (fr - 0.5) * 2.0)
    }
}

/// The visualizer: bars in Christmas colours, snow on top of each, standing
/// on a strip of ice that reflects them.
fn draw_viz(f: &mut Frame, r: Rect, bars: &VizBars, live: bool, status: &str, quiet: f32, khz: bool, t: f32) {
    const FILL: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    panel(f, r.x, r.y, r.w, r.h, '♬', if khz { "visualizer · kHz" } else { "visualizer" });
    let snow = Rgb::hex(0xeef4ff);
    let rows = r.h - 3;
    let base = r.y + r.h - 3;
    let ice = r.y + r.h - 2;
    let (x0, iw) = (r.x + 2, r.w - 4);
    let n = bars.v.len() as i32;
    let gap = bar_gap(khz);
    let off = (iw - (n * (BAR_W + gap) - gap)).max(0) / 2;
    let ice_bg = Bg::Mix(Rgb::hex(0x1c2c52), 0.88);
    for x in r.x + 1..r.x + r.w - 1 {
        f.put_g(x, ice, Glyph::new(' ', snow).bg(ice_bg).lit(0.0));
    }
    let levels = (rows * 8) as f32;
    for i in 0..n {
        let bx = x0 + off + i * (BAR_W + gap);
        let v = bars.v[i as usize];
        let lv = v * levels;
        let cap_cell = ((bars.cap[i as usize] * levels) / 8.0).floor().clamp(0.0, (rows - 1) as f32) as i32;
        let amt = |k: i32| (lv - k as f32 * 8.0).clamp(0.0, 8.0).round() as usize;
        let top = (0..rows).rev().find(|&k| amt(k) > 0).unwrap_or(-1);
        let bright = 0.72 + 0.5 * v;
        for k in 0..=top {
            let mut col = viz_col((k as f32 + 0.5) / rows as f32) * bright;
            if k == top && cap_cell <= top {
                // snow-capped
                col = col.lerp(snow, 0.55);
            }
            for dx in 0..BAR_W {
                f.put_g(bx + dx, base - k, Glyph::new(FILL[amt(k)], col).bg(PANEL).lit(0.0));
            }
        }
        if cap_cell > top {
            for dx in 0..BAR_W {
                f.put_g(bx + dx, base - cap_cell, Glyph::new('▁', snow * 0.9).bg(PANEL).lit(0.0));
            }
        }
        // the reflection in the ice
        if top >= 0 && !khz {
            let a = (amt(0) as f32 / 8.0) * (0.25 + 0.2 * v);
            for dx in 0..BAR_W {
                f.put_g(bx + dx, ice, Glyph::new('▀', viz_col(0.1) * a + Rgb::hex(0x2a3c66) * 0.4).bg(ice_bg).lit(0.0));
            }
        }
        // or each bar's frequency in kHz, written on the ice
        if khz {
            let bands = bars.target.len();
            let a = i as usize * bands / n as usize;
            let b = ((i as usize + 1) * bands / n as usize).max(a + 1).min(bands);
            let label = viz::khz_label(viz::centre_hz(a, b));
            f.text(bx, ice, &label, Rgb::hex(0xc8d4ec), ice_bg, 0.0);
        }
    }
    // the odd glint on the ice
    let g = (t * 0.7) as i32;
    let gx = x0 + ((g * 37).rem_euclid(iw.max(1)));
    if !khz && (t * 0.7).fract() < 0.35 {
        f.put_g(gx, ice, Glyph::new('✧', snow * 0.55).bg(ice_bg).lit(0.0));
    }
    let msg = if !live {
        if status.is_empty() { "♪ listening…".to_string() } else { status.to_string() }
    } else if quiet > 1.5 {
        "♪ waiting for music…".to_string()
    } else {
        String::new()
    };
    if !msg.is_empty() {
        // a few flakes drifting through while it's quiet
        for k in 0..6 {
            let fx = x0 + ((k * 11 + 4) % iw.max(1));
            let fy = r.y + 1 + ((t * 0.5 + k as f32 * 0.61) * 1.0).rem_euclid(rows as f32) as i32;
            let sway = ((t * 0.9 + k as f32).sin() * 1.2).round() as i32;
            f.put_g(fx + sway, fy, Glyph::new('·', snow * 0.5).bg(PANEL).lit(0.0));
        }
        let msg = clip(&msg, iw as usize);
        let mx = r.x + (r.w - msg.chars().count() as i32) / 2;
        txt(f, mx, r.y + 1 + rows / 2 - 1, &msg, pal::UI_DIM);
    }
}

fn draw_handles(f: &mut Frame, r: Rect, dragging: bool, t: f32) {
    let k = if dragging { 1.3 } else { 0.85 + 0.15 * (t * 4.0).sin() };
    let c = Rgb::hex(0xffd36b) * k;
    let (x0, y0, x1, y1) = (r.x - 1, r.y - 1, r.x + r.w, r.y + r.h);
    for (x, y, ch) in [(x0, y0, '┏'), (x1, y0, '┓'), (x0, y1, '┗'), (x1, y1, '┛')] {
        f.put_g(x, y, Glyph::new(ch, c).lit(0.0));
    }
    let hint = if dragging { " moving " } else { " drag to move " };
    let hx = r.x + r.w - hint.chars().count() as i32;
    f.text(hx, y1, hint, c, Bg::Mix(pal::UI_PANEL, 0.85), 0.0);
}

fn digit(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        ':' => [0, 0, 1, 0, 1, 0, 0],
        _ => [0; 7],
    }
}

fn draw_countdown(f: &mut Frame, x: i32, y: i32, t: f32) {
    let now = crate::clock::now();
    let year = now.year();
    let xmas = |y: i32| Local.from_local_datetime(&NaiveDate::from_ymd_opt(y, 12, 25).unwrap().and_hms_opt(0, 0, 0).unwrap()).single();
    let red = pal::CANDY_RED;
    let gold = Rgb::hex(0xffd36b);
    let sparkle = if (t * 2.0) as i32 % 2 == 0 { '✦' } else { '✧' };
    if now.month() == 12 && now.day() == 25 {
        let msg = format!("{sparkle} Merry Christmas! {sparkle}");
        for (i, ch) in msg.chars().enumerate() {
            let c = if (i as i32 + (t * 4.0) as i32) % 2 == 0 { red * 1.2 } else { Rgb::hex(0x3cd070) };
            f.put_g(x + i as i32, y, Glyph::new(ch, c).lit(0.0));
        }
        return;
    }
    let target = match xmas(year) {
        Some(d) if d > now => d,
        _ => match xmas(year + 1) {
            Some(d) => d,
            None => return,
        },
    };
    let left = target - now;
    let secs = left.num_seconds().max(0);
    let (d, h, m, s) = (secs / 86400, (secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let mut cx = f.text(x, y, &format!("{sparkle} "), gold, Bg::Scene, 0.0);
    cx = f.text(cx, y, &format!("{d}"), red * 1.25, Bg::Scene, 0.0);
    cx = f.text(cx, y, if d == 1 { " day " } else { " days " }, pal::UI_TEXT, Bg::Scene, 0.0);
    cx = f.text(cx, y, &format!("{h:02}:{m:02}:{s:02}"), gold, Bg::Scene, 0.0);
    f.text(cx, y, " until Christmas", pal::UI_TEXT, Bg::Scene, 0.0);
}

// ---------------------------------------------------------------------------

fn panel(f: &mut Frame, x: i32, y: i32, w: i32, h: i32, icon: char, title: &str) {
    let b = pal::UI_BORDER;
    for yy in y..y + h {
        for xx in x..x + w {
            let ch = match (xx == x, xx == x + w - 1, yy == y, yy == y + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(xx, yy, Glyph::new(ch, b).bg(PANEL).lit(0.0));
        }
    }
    let mut cx = x + 2;
    f.put_g(cx, y, Glyph::new(' ', b).bg(PANEL).lit(0.0));
    f.put_g(cx + 1, y, Glyph::new(icon, Rgb::hex(0xffd36b)).bg(PANEL).lit(0.0));
    cx += 2;
    cx = txt(f, cx, y, " ", b);
    cx = txt(f, cx, y, title, pal::UI_TITLE);
    txt(f, cx, y, " ", b);
}

/// At most `n` chars, ending in … when cut.
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    if n > 0 {
        out.push('…');
    }
    out
}

fn txt(f: &mut Frame, x: i32, y: i32, s: &str, c: Rgb) -> i32 {
    f.text(x, y, s, c, PANEL, 0.0)
}

fn bar(f: &mut Frame, x: i32, y: i32, w: i32, frac: f32, t: f32) {
    let n = (frac.clamp(0.0, 1.0) * w as f32).round() as i32;
    let shift = (t * 3.0) as i32;
    for i in 0..w {
        let g = if i < n {
            let c = if (i + shift).rem_euclid(2) == 0 { pal::CANDY_RED } else { pal::CANDY_WHITE };
            Glyph::new('▰', c)
        } else {
            Glyph::new('▱', pal::UI_DIM * 0.7)
        };
        f.put_g(x + i, y, g.bg(PANEL).lit(0.0));
    }
}

fn heat(v: f32) -> Rgb {
    if v < 0.5 {
        pal::PINE.lerp(Rgb::hex(0xffd36b), v * 2.0)
    } else {
        Rgb::hex(0xffd36b).lerp(pal::CANDY_RED, (v - 0.5) * 2.0)
    }
}

fn spark(f: &mut Frame, x: i32, y: i32, w: i32, hist: &VecDeque<f32>, max: Option<f32>) {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let m = max.unwrap_or_else(|| hist.iter().cloned().fold(1.0, f32::max));
    let start = hist.len().saturating_sub(w as usize);
    for (i, v) in hist.iter().skip(start).enumerate() {
        let k = (v / m).clamp(0.0, 1.0);
        let ch = BARS[((k * 7.0).round() as usize).min(7)];
        f.put_g(x + (w - (hist.len() - start) as i32) + i as i32, y, Glyph::new(ch, heat(k)).bg(PANEL).lit(0.0));
    }
}

fn gib(b: f64) -> String {
    let g = b / 1073741824.0;
    if g >= 100.0 { format!("{g:.0}") } else { format!("{g:.1}") }
}

fn rate(b: f64) -> String {
    if b >= 1e6 {
        format!("{:.1} MB/s", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB/s", b / 1e3)
    } else {
        format!("{:.0} B/s", b)
    }
}

fn draw_system(f: &mut Frame, x: i32, y: i32, s: &SysStats, t: f32) -> i32 {
    let w = 54;
    let h = 9;
    panel(f, x, y, w, h, '❄', "system");
    let lbl = |f: &mut Frame, yy: i32, l: &str| txt(f, x + 2, yy, l, Rgb::hex(0xffd36b));
    let mut yy = y + 1;
    if !s.ready {
        txt(f, x + 2, yy, "gathering stats…", pal::UI_DIM);
        return y + h;
    }
    let bx = x + 8;
    let bw = 16;
    // CPU
    lbl(f, yy, "CPU");
    bar(f, bx, yy, bw, s.cpu / 100.0, t);
    txt(f, bx + bw + 1, yy, &format!("{:>3.0}%", s.cpu), pal::UI_TEXT);
    spark(f, bx + bw + 7, yy, 14, &s.cpu_hist, Some(100.0));
    if let Some(tc) = s.cpu_temp {
        txt(f, x + w - 7, yy, &format!("{tc:>3.0}°C"), heat(((tc - 35.0) / 55.0).clamp(0.0, 1.0)));
    }
    yy += 1;
    // GPU
    if let Some(g) = s.gpu {
        lbl(f, yy, "GPU");
        bar(f, bx, yy, bw, g / 100.0, t);
        txt(f, bx + bw + 1, yy, &format!("{:>3.0}%", g), pal::UI_TEXT);
        spark(f, bx + bw + 7, yy, 14, &s.gpu_hist, Some(100.0));
        if let Some(tc) = s.gpu_temp {
            txt(f, x + w - 7, yy, &format!("{tc:>3.0}°C"), heat(((tc - 35.0) / 55.0).clamp(0.0, 1.0)));
        }
        yy += 1;
        lbl(f, yy, "VRAM");
        let fr = (s.vram_used / s.vram_total.max(1.0)) as f32;
        bar(f, bx, yy, bw, fr, t);
        txt(f, bx + bw + 1, yy, &format!("{:>3.0}%  {} / {} GiB", fr * 100.0, gib(s.vram_used), gib(s.vram_total)), pal::UI_TEXT);
        yy += 1;
    }
    // RAM
    lbl(f, yy, "RAM");
    let fr = (s.mem_used / s.mem_total.max(1.0)) as f32;
    bar(f, bx, yy, bw, fr, t);
    txt(f, bx + bw + 1, yy, &format!("{:>3.0}%  {} / {} GiB", fr * 100.0, gib(s.mem_used), gib(s.mem_total)), pal::UI_TEXT);
    yy += 1;
    // disk
    lbl(f, yy, "DISK");
    let fr = (s.disk_used / s.disk_total.max(1.0)) as f32;
    bar(f, bx, yy, bw, fr, t);
    txt(f, bx + bw + 1, yy, &format!("{:>3.0}%  {} / {} GiB", fr * 100.0, gib(s.disk_used), gib(s.disk_total)), pal::UI_TEXT);
    yy += 1;
    // net
    lbl(f, yy, "NET");
    let c = txt(f, bx, yy, "↓ ", pal::PINE * 1.3);
    let c = txt(f, c, yy, &format!("{:<10}", rate(s.rx)), pal::UI_TEXT);
    spark(f, c, yy, 10, &s.rx_hist, None);
    let c = txt(f, c + 11, yy, "↑ ", pal::CANDY_RED * 1.2);
    let c = txt(f, c, yy, &format!("{:<10}", rate(s.tx)), pal::UI_TEXT);
    let _ = c;
    yy += 1;
    spark(f, x + w - 12, yy, 10, &s.tx_hist, None);
    y + h
}

fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn marquee(s: &str, w: usize, t: f32) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= w {
        return s.to_string();
    }
    let padded: Vec<char> = chars.iter().cloned().chain("   ·   ".chars()).collect();
    let n = padded.len();
    // pause at the start of each loop
    let step = ((t * 4.0) as usize) % (n + 12);
    let off = step.saturating_sub(12);
    (0..w).map(|i| padded[(off + i) % n]).collect()
}

fn draw_music(f: &mut Frame, x: i32, y: i32, w: i32, m: &Music, t: f32) -> i32 {
    let h = 6;
    panel(f, x, y, w, h, '♫', "now playing");
    let inner = (w - 6) as usize;
    if !m.present {
        txt(f, x + 2, y + 2, "♪  nothing playing", pal::UI_DIM);
        txt(f, x + 2, y + 3, "   the snow is quiet tonight", pal::UI_DIM * 0.8);
        return y + h;
    }
    let (icon, ic) = if m.playing { ('▶', pal::PINE * 1.3) } else { ('‖', pal::UI_DIM) };
    f.put_g(x + 2, y + 1, Glyph::new(icon, ic).bg(PANEL).lit(0.0));
    txt(f, x + 4, y + 1, &marquee(&m.title, inner, t), pal::UI_TITLE);
    txt(f, x + 4, y + 2, &marquee(&m.artist, inner, t * 0.8), pal::UI_TEXT * 0.85);
    let pos = m.pos_now();
    if m.length > 0.0 {
        let a = fmt_time(pos);
        let b = fmt_time(m.length);
        let c = txt(f, x + 4, y + 3, &a, pal::UI_DIM);
        let bw = w - 4 - 6 - a.len() as i32 - b.len() as i32;
        bar(f, c + 1, y + 3, bw, (pos / m.length) as f32, t * 0.3);
        txt(f, c + 2 + bw, y + 3, &b, pal::UI_DIM);
    }
    // little equalizer (decorative) + player
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    for i in 0..8 {
        let v = if m.playing { noise1(t * 6.0 + i as f32 * 3.7, 11 + i) } else { 0.05 };
        let ch = BARS[((v * 7.0) as usize).min(7)];
        f.put_g(x + 4 + i as i32, y + 4, Glyph::new(ch, heat(v)).bg(PANEL).lit(0.0));
    }
    txt(f, x + 14, y + 4, &format!("· {}", m.player), pal::UI_DIM);
    y + h
}

#[derive(Clone, Copy)]
enum Sky {
    Clear,
    Partly,
    Cloud,
    Fog,
    Rain,
    Snow,
    Thunder,
}

fn wmo(code: i64) -> (Sky, &'static str) {
    match code {
        0 => (Sky::Clear, "Clear"),
        1 => (Sky::Clear, "Mainly clear"),
        2 => (Sky::Partly, "Partly cloudy"),
        3 => (Sky::Cloud, "Overcast"),
        45 | 48 => (Sky::Fog, "Fog"),
        51..=57 => (Sky::Rain, "Drizzle"),
        61..=67 => (Sky::Rain, "Rain"),
        71..=77 => (Sky::Snow, "Snow"),
        80..=82 => (Sky::Rain, "Showers"),
        85 | 86 => (Sky::Snow, "Snow showers"),
        95..=99 => (Sky::Thunder, "Thunderstorm"),
        _ => (Sky::Cloud, "—"),
    }
}

fn small_icon(s: Sky, day: bool) -> char {
    match s {
        Sky::Clear if day => '☀',
        Sky::Clear => '☾',
        Sky::Partly => '◐',
        Sky::Cloud => '☁',
        Sky::Fog => '≡',
        Sky::Rain => '☂',
        Sky::Snow => '❄',
        Sky::Thunder => 'ϟ',
    }
}

fn draw_icon(f: &mut Frame, x: i32, y: i32, s: Sky, day: bool, t: f32) {
    let sun = Rgb::hex(0xffd36b);
    let cloud = Rgb::hex(0xc8cede);
    let moon = Rgb::hex(0xf2ead0);
    let fall = (t * 2.0) as usize % 2;
    let (lines, cols): (Vec<String>, Vec<Rgb>) = match s {
        Sky::Clear if day => (
            vec!["  \\ │ /  ".into(), " ─  ☀  ─ ".into(), "  / │ \\  ".into(), "".into()],
            vec![sun, sun * 1.2, sun, sun],
        ),
        Sky::Clear => (
            vec!["  ✦   ·  ".into(), "   ▄▄▖   ".into(), "  ▐█     ".into(), "   ▀▀▘ ✧ ".into()],
            vec![moon * 0.7, moon, moon, moon],
        ),
        Sky::Partly => (
            vec![" \\ /     ".into(), "─ ☀ .--. ".into(), "  .(    ).".into(), " (___.__)".into()],
            vec![sun, sun, cloud, cloud],
        ),
        Sky::Fog => (
            vec!["         ".into(), " ─ ─ ─ ─ ".into(), "  ─ ─ ─  ".into(), " ─ ─ ─ ─ ".into()],
            vec![cloud; 4],
        ),
        _ => {
            let drop = match s {
                Sky::Rain => if fall == 0 { "  ╎ ╎ ╎ ╎" } else { " ╎ ╎ ╎ ╎ " },
                Sky::Snow => if fall == 0 { "  * ❄ * ❄" } else { " ❄ * ❄ * " },
                Sky::Thunder => if fall == 0 { "  ϟ ╎ ϟ  " } else { " ╎ ϟ ╎ ϟ " },
                _ => "",
            };
            let dc = match s {
                Sky::Rain => Rgb::hex(0x6aa8ff),
                Sky::Snow => Rgb::hex(0xeef4ff),
                _ => sun,
            };
            (vec!["   .--.  ".into(), " .(    ).".into(), "(___.__)_)".into(), drop.into()], vec![cloud, cloud, cloud, dc])
        }
    };
    for (i, l) in lines.iter().enumerate() {
        txt(f, x, y + i as i32, l, cols[i]);
    }
}

fn draw_weather(f: &mut Frame, x: i32, y: i32, w: i32, wx: &Weather, t: f32) -> i32 {
    let h = 8;
    let title = if wx.place.is_empty() { "weather".to_string() } else { format!("weather · {}", wx.place) };
    // the title starts at x + 5; leave the corner and one ─ before it
    panel(f, x, y, w, h, '☃', &clip(&title, (w - 8).max(0) as usize));
    if !wx.ok {
        // errors can be long (ureq includes the whole URL): wrap them over
        // the panel's empty rows, clipped to the inner width
        let inner = (w - 4).max(1) as usize;
        let chars: Vec<char> = wx.status.chars().collect();
        let rows = (h - 3) as usize; // y+2 .. above the bottom border
        for (i, line) in chars.chunks(inner).take(rows).enumerate() {
            let mut line: String = line.iter().collect();
            if i == rows - 1 && chars.len() > inner * rows {
                line = clip(&format!("{line}…"), inner);
            }
            txt(f, x + 2, y + 2 + i as i32, &line, pal::UI_DIM);
        }
        return y + h;
    }
    let (sky, desc) = wmo(wx.code);
    draw_icon(f, x + 2, y + 1, sky, wx.is_day, t);
    let tx = x + 14;
    let c = txt(f, tx, y + 1, &format!("{:.0}{}", wx.temp, wx.unit), Rgb::hex(0xfff1d6));
    txt(f, c + 2, y + 1, desc, pal::UI_TEXT);
    txt(f, tx, y + 2, &format!("feels like {:.0}{}", wx.feels, wx.unit), pal::UI_DIM);
    txt(f, tx, y + 3, &format!("wind {:.0} {}  ·  humidity {:.0}%", wx.wind, wx.wind_unit, wx.humidity), pal::UI_DIM);
    // forecast row
    let mut fx = x + 2;
    for (i, (day, code, mx, mn)) in wx.daily.iter().enumerate().take(4) {
        let name = NaiveDate::parse_from_str(day, "%Y-%m-%d").map(|d| d.format("%a").to_string()).unwrap_or_default();
        let (s, _) = wmo(*code);
        let nc = if i == 0 { Rgb::hex(0xffd36b) } else { pal::UI_TEXT };
        let c = txt(f, fx, y + 6, &format!("{name} "), nc);
        f.put_g(c, y + 6, Glyph::new(small_icon(s, true), Rgb::hex(0xffd36b)).bg(PANEL).lit(0.0));
        let c = txt(f, c + 2, y + 6, &format!("{:.0}°", mx), pal::UI_TEXT);
        txt(f, c, y + 6, &format!("/{:.0}°", mn), pal::UI_DIM);
        fx += 14;
    }
    y + h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the clock: no data threads (no network, no audio capture).
    fn clock_only() -> Widgets {
        let mut cfg = Config::default();
        (cfg.widgets.system, cfg.widgets.music, cfg.widgets.weather, cfg.widgets.map, cfg.widgets.visualizer) = (false, false, false, false, false);
        cfg.sound.enabled = false;
        Widgets::new(&cfg)
    }

    #[test]
    fn the_clock_clears_snow_but_not_trees_or_speech_bubbles() {
        let mut w = clock_only();
        let mut f = Frame::new(200, 60);
        // inside the clock's area (3..69 × 2..13), right of the digits
        f.ui = true;
        f.text(50, 6, "see that one?", Rgb::hex(0x2a1a14), Bg::Mix(Rgb::hex(0xf4efe6), 0.9), 0.0);
        f.ui = false;
        f.put_g(48, 10, Glyph::new('^', Rgb::hex(0x2f9a52)).bg(Bg::Solid(Rgb::hex(0x143f23))));
        f.specks = true;
        f.put_g(47, 9, Glyph::new('❄', Rgb::WHITE));
        f.specks = false;
        w.draw(&mut f, 1.0);
        let at = |x: usize, y: usize| f.glyph[y * f.w + x].map(|g| g.ch);
        assert_eq!(at(50, 6), Some('s'), "the speech bubble was wiped");
        assert_eq!(at(48, 10), Some('^'), "the tree was wiped");
        assert_eq!(at(47, 9), None, "the snowflake stayed between the digits");
        assert!(!f.keep_ui);
    }

    #[test]
    fn panels_dont_paint_over_speech_bubbles() {
        let mut cfg = Config::default();
        (cfg.widgets.music, cfg.widgets.weather, cfg.widgets.map, cfg.widgets.visualizer, cfg.clock.enabled) = (false, false, false, false, false);
        cfg.sound.enabled = false;
        // the system panel, without its stats thread: parked off, then drawn
        let mut w = Widgets::new(&cfg);
        let mut f = Frame::new(200, 60);
        f.ui = true;
        f.text(10, 16, "hello!", Rgb::hex(0x2a1a14), Bg::Mix(Rgb::hex(0xf4efe6), 0.9), 0.0);
        f.ui = false;
        w.draw(&mut f, 1.0);
        assert!(w.rects[1].is_some_and(|r| r.contains(10, 16)), "the system panel isn't under the bubble");
        assert_eq!(f.glyph[16 * f.w + 10].map(|g| g.ch), Some('h'));
    }
}
