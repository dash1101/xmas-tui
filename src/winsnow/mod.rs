//! Window snow & lights. Snowflakes fall in front of everything and settle
//! on the visible top edges of windows and the taskbar/panel, piling up to a
//! capped height (drawn as stacked eighth-blocks, TUI style). Piles ride
//! along when a window moves, fly off when it's shaken, get brushed away by
//! the cursor, and fall when the window closes or minimizes. Optionally,
//! strings of twinkling bulbs hang around each window's frame.
//!
//! Geometry comes from the compositor (KWin script on KDE); drawing goes to
//! a transparent, click-through overlay above all windows.

#[cfg(target_os = "linux")]
pub mod kde;
#[cfg(target_os = "linux")]
pub mod bench;
mod lights;
#[cfg(target_os = "linux")]
pub mod overlay;
#[cfg(windows)]
pub mod win;

use crate::rng::{noise1, Rng};
use std::collections::HashMap;


#[derive(Clone, Debug)]
pub struct Win {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 0 normal, 1 dock/panel, 2 fullscreen, 3 maximized
    pub kind: u8,
    pub z: i64,
}

/// A parsed report: the visible windows (bottom to top), the ids of windows
/// that are only on another virtual desktop / activity, and the cursor.
#[cfg(target_os = "linux")]
pub type Report = (Vec<Win>, Vec<String>, (f32, f32));

/// Parse the KWin script's JSON report.
#[cfg(target_os = "linux")]
pub fn parse(json: &str) -> Option<Report> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut wins = vec![];
    for w in v["w"].as_array()? {
        let a = w.as_array()?;
        let f = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        wins.push(Win {
            id: a.first()?.as_str()?.to_string(),
            x: f(1),
            y: f(2),
            w: f(3),
            h: f(4),
            kind: f(5) as u8,
            z: f(6) as i64,
        });
    }
    wins.sort_by_key(|w| w.z);
    let away = v["a"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let c = v["c"].as_array()?;
    Some((wins, away, (c.first()?.as_f64()? as f32, c.get(1)?.as_f64()? as f32)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxCfg {
    pub snow: bool,
    pub lights: bool,
    /// Max pile height in text rows.
    pub cap_rows: f32,
    pub density: f32,
    /// Draw the falling flakes (off: they still fall and settle, unseen).
    pub front: bool,
    /// Snow settles on the taskbar/panels too.
    pub docks: bool,
    /// Pile build-up speed multiplier.
    pub settle: f32,
    /// 0 multi, 1 warm, 2 icy, 3 candy cane.
    pub light_style: u8,
    pub light_windows: u32,
    pub light_glow: f32,
    /// Max pile height on the taskbar / panels, in text rows.
    pub dock_rows: f32,
    pub fall: f32,
    pub wind: f32,
    /// Shake-off sensitivity (0: never).
    pub shake: f32,
    pub brush: bool,
    /// Melting rate (0: piles stay).
    pub melt: f32,
}

impl Default for FxCfg {
    fn default() -> Self {
        FxCfg {
            snow: true,
            lights: true,
            cap_rows: 1.5,
            density: 1.0,
            front: true,
            docks: true,
            settle: 1.0,
            light_style: 0,
            light_windows: 6,
            light_glow: 1.0,
            dock_rows: 1.5,
            fall: 1.0,
            wind: 1.0,
            shake: 1.0,
            brush: true,
            melt: 0.0,
        }
    }
}

impl FxCfg {
    /// From the `[windows]` config section.
    pub fn from_cfg(w: &crate::config::WindowsCfg) -> Self {
        let style = crate::config::LIGHT_STYLES.iter().position(|s| *s == w.light_style).unwrap_or(0) as u8;
        FxCfg {
            snow: w.snow,
            lights: w.lights,
            cap_rows: w.pile_rows,
            density: w.density,
            front: w.front,
            docks: w.docks,
            settle: w.settle,
            light_style: style,
            light_windows: w.light_windows,
            light_glow: w.light_glow,
            dock_rows: w.dock_rows,
            fall: w.fall,
            wind: w.wind,
            shake: w.shake,
            brush: w.brush,
            melt: w.melt,
        }
    }
}

struct Pile {
    cols: Vec<f32>,
    x: f32,
    y: f32,
    w: f32,
    seen: f32,
    /// Since when the window has been on another desktop (kept, not drawn).
    away: Option<f32>,
}

/// How long a pile is kept for a window that's on another virtual desktop.
const DORMANT_FOR: f32 = 30.0 * 60.0;

struct Flake {
    x: f32,
    y: f32,
    vy: f32,
    sway: f32,
    phase: f32,
    ch: char,
}

/// A snowball rolled up out of a pile: sitting on a window's top, being
/// rolled along it, or picked up off it (dragged up away from the top) and
/// carried at the pointer, to be thrown.
#[derive(Clone, Debug)]
struct Ball {
    win: String,
    /// Centre x from the window's left edge.
    dx: f32,
    r: f32,
    rolling: bool,
    carried: bool,
    /// Seconds left alone: after a while it melts away.
    idle: f32,
}

/// A snowball thrown: in the air, then splatted on a window's glass, sliding
/// slowly down it and off the bottom edge (or burst against the desktop).
#[derive(Debug)]
struct Throw {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,
    age: f32,
    /// How long it's in the air before it meets the glass.
    flight: f32,
    splat: Option<Splat>,
    seed: u32,
    done: bool,
}

#[derive(Debug)]
struct Splat {
    win: String,
    /// Where it hit, from the window's top-left corner.
    dx: f32,
    dy: f32,
    /// How far it's slid down since, and how fast it's going.
    slid: f32,
    v: f32,
    age: f32,
    r: f32,
}

struct Part {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
}

/// The top edge of an open window, where it shows (not under another
/// window), for the villager to climb up onto: in fractions of the screen
/// the overlay covers. `id` is the window's, so a villager up there moves
/// with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Top {
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub bottom: f32,
}

static TOPS: std::sync::Mutex<Vec<Top>> = std::sync::Mutex::new(Vec::new());

/// The top of a taskbar / panel along the bottom of the screen, as a
/// fraction of the screen's height (the UI keeps above it).
static DOCK_TOP: std::sync::Mutex<Option<f32>> = std::sync::Mutex::new(None);

#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
pub fn dock_top() -> Option<f32> {
    DOCK_TOP.lock().ok().and_then(|d| *d)
}

/// The open windows' tops, as last seen (none while no overlay runs).
pub fn tops() -> Vec<Top> {
    TOPS.lock().map(|t| t.clone()).unwrap_or_default()
}

/// Where each normal window's top edge can be seen: split round the windows
/// in front of the strip just above it.
fn visible_tops(wins: &[Win], sw: f32, sh: f32) -> Vec<Top> {
    let mut out = vec![];
    if sw <= 0.0 || sh <= 0.0 {
        return out;
    }
    for w in wins.iter().filter(|w| w.kind == 0 && w.w > 60.0 && w.y > 12.0 && w.y < sh - 12.0) {
        let id = w.id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
        let line = w.y - 6.0;
        let mut segs = vec![(w.x.max(0.0), (w.x + w.w).min(sw))];
        for o in wins.iter().filter(|o| o.z > w.z && o.y <= line && o.y + o.h > line) {
            let (a, b) = (o.x, o.x + o.w);
            segs = segs
                .into_iter()
                .flat_map(|(s0, s1)| {
                    if b <= s0 || a >= s1 {
                        vec![(s0, s1)]
                    } else {
                        [(s0, a.min(s1)), (b.max(s0), s1)].into_iter().filter(|(x, y)| y > x).collect()
                    }
                })
                .collect();
        }
        for (k, (s0, s1)) in segs.into_iter().filter(|(a, b)| b - a >= 24.0).enumerate() {
            out.push(Top { id: id ^ (k as u64) << 56, x0: s0 / sw, x1: s1 / sw, y: w.y / sh, bottom: ((w.y + w.h) / sh).min(1.0) });
        }
    }
    out
}

pub struct Fx {
    pub cfg: FxCfg,
    pub sw: f32,
    pub sh: f32,
    /// Output position in global (compositor) coordinates.
    pub origin: (f32, f32),
    wins: Vec<Win>,
    piles: HashMap<String, Pile>,
    flakes: Vec<Flake>,
    parts: Vec<Part>,
    cursor: Option<(f32, f32)>,
    cursor_t: f32,
    cursor_v: (f32, f32),
    /// The mouse button's held down (front-ends that can tell: Windows).
    pub button: bool,
    /// Snowballs rolled up out of the piles.
    balls: Vec<Ball>,
    /// Snowballs thrown: in the air, or splatted on a window.
    throws: Vec<Throw>,
    /// The button was down at the last look (presses and releases).
    was_button: bool,
    rng: Rng,
    melt_acc: f32,
    pub hidden: bool,
    /// Pixel scale (1.0 = the 96-dpi design size).
    scale: f32,
    col: f32,
    step: f32,
    /// Window corner radius, so light strings follow rounded corners.
    pub corner: f32,
    /// Floating panels: (min length, max length, visible thickness). The
    /// panel's window is taller than the visible bar; snow should sit on
    /// the bar, not on the invisible margin.
    pub docks: Vec<(f32, f32, f32)>,
    /// Light strings, cached per window layout.
    lights: lights::Lights,
}

impl Drop for Fx {
    // the overlay's gone: no windows to climb
    fn drop(&mut self) {
        if let Ok(mut tops) = TOPS.lock() {
            tops.clear();
        }
        if let Ok(mut d) = DOCK_TOP.lock() {
            *d = None;
        }
    }
}

impl Fx {
    pub fn new(sw: f32, sh: f32, cfg: FxCfg, scale: f32) -> Self {
        let mut fx = Fx {
            scale,
            col: 8.0 * scale,
            step: 17.0 / 8.0 * scale,
            corner: 10.0 * scale,
            docks: vec![],
            lights: Default::default(),
            cfg,
            sw,
            sh,
            origin: (0.0, 0.0),
            wins: vec![],
            piles: HashMap::new(),
            flakes: vec![],
            parts: vec![],
            cursor: None,
            cursor_t: 0.0,
            button: false,
            balls: vec![],
            throws: vec![],
            was_button: false,
            melt_acc: 0.0,
            cursor_v: (0.0, 0.0),
            rng: Rng::new(0x5a0b),
            hidden: false,
        };
        fx.reflake();
        fx
    }

    /// Pixels per 96-dpi pixel.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Apply new settings: re-seed the flakes when the density changed, and
    /// drop all settled and flying snow when snow is turned off.
    pub fn set_cfg(&mut self, cfg: FxCfg) {
        let reflake = (cfg.density - self.cfg.density).abs() > 1e-3;
        if !cfg.snow {
            self.piles.clear();
            self.parts.clear();
        }
        self.cfg = cfg;
        if reflake {
            self.reflake();
        }
    }

    /// Visible thickness of the floating panel whose window is `len` long,
    /// if one can be told apart. Only a panel whose configured length
    /// matches counts: an exact min/max length first, else a panel whose
    /// length range holds it (fit-to-content panels) when all such panels
    /// agree on the thickness. No match, no inset.
    fn dock_thickness(&self, len: f32) -> Option<f32> {
        if let Some(d) = self.docks.iter().find(|d| (d.0 - len).abs() < 2.0 || (d.1 - len).abs() < 2.0) {
            return Some(d.2);
        }
        let mut within = self.docks.iter().filter(|d| d.0 > 0.0 && d.1 >= d.0 && len > d.0 - 2.0 && len < d.1 + 2.0).map(|d| d.2);
        let th = within.next()?;
        within.all(|t| (t - th).abs() < 0.5).then_some(th)
    }

    fn cap(&self) -> f32 {
        (self.cfg.cap_rows.max(self.cfg.dock_rows) * 8.0).round() * self.step
    }

    /// Max pile height on window `id` (the taskbar has its own setting).
    fn cap_for(&self, id: &str) -> f32 {
        let dock = self.wins.iter().any(|w| w.id == id && w.kind == 1);
        let rows = if dock { self.cfg.dock_rows } else { self.cfg.cap_rows };
        (rows * 8.0).round() * self.step
    }

    fn reflake(&mut self) {
        // same count per 96-dpi area, whatever the pixel density
        let k = self.scale * self.scale;
        let n = ((self.sw * self.sh / (22_000.0 * k)) * self.cfg.density) as usize;
        let n = n.min(600);
        self.flakes.clear();
        for _ in 0..n {
            let f = self.new_flake(true);
            self.flakes.push(f);
        }
    }

    fn new_flake(&mut self, anywhere: bool) -> Flake {
        let (r, k) = (&mut self.rng, self.scale);
        Flake {
            x: r.range(0.0, self.sw),
            y: if anywhere { r.range(-20.0 * k, self.sh) } else { r.range(-30.0 * k, -5.0 * k) },
            vy: r.range(28.0 * k, 60.0 * k) * self.cfg.fall.clamp(0.25, 3.0),
            sway: r.range(4.0 * k, 14.0 * k),
            phase: r.range(0.0, 6.28),
            ch: *r.pick(&['❄', '❅', '❆', '*', '•', '·', '*']),
        }
    }

    /// Is (x, y) inside a window stacked above z?
    fn covered(&self, x: f32, y: f32, z: i64) -> bool {
        self.wins.iter().any(|w| w.z > z && x >= w.x && x < w.x + w.w && y >= w.y && y < w.y + w.h)
    }

    fn holds_snow(w: &Win, cap: f32, docks: bool) -> bool {
        (w.kind == 0 || (w.kind == 1 && docks) || w.kind == 3) && w.y > cap * 0.5 + 2.0
    }

    fn spill(&mut self, x: f32, y: f32, vx: f32, vy: f32, amount: f32) {
        let n = ((amount / self.step) as usize).clamp(1, 6);
        for _ in 0..n {
            if self.parts.len() > 1500 {
                break;
            }
            self.parts.push(Part {
                x: x + self.rng.range(0.0, self.col),
                y: y - self.rng.range(0.0, amount),
                vx: vx + self.rng.range(-40.0, 40.0) * self.scale,
                vy: vy + self.rng.range(-60.0, 10.0) * self.scale,
                age: 0.0,
                life: self.rng.range(1.5, 3.5),
            });
        }
    }

    /// New geometry from the compositor (global logical coordinates).
    /// `away`: windows that exist but are on another virtual desktop or
    /// activity; their piles are kept (undrawn) for when they come back.
    pub fn set_windows(&mut self, mut wins: Vec<Win>, away: &[String], cursor: (f32, f32), t: f32) {
        let (ox, oy) = self.origin;
        for w in &mut wins {
            w.x -= ox;
            w.y -= oy;
        }
        let cursor = (cursor.0 - ox, cursor.1 - oy);
        // floating panels: move the snow line down onto the visible bar
        for w in wins.iter_mut().filter(|w| w.kind == 1 && w.w > w.h) {
            if let Some(th) = self.dock_thickness(w.w).map(|th| th * self.scale) {
                if w.h > th + 1.0 {
                    let inset = ((w.h - th) / 2.0).floor();
                    w.y += inset;
                    w.h -= 2.0 * inset;
                }
            }
        }
        // fullscreen app on top (game, video): get out of the way
        self.hidden = wins.last().map(|w| w.kind == 2).unwrap_or(false);
        let cap = self.cap();
        let mut spills = vec![];
        for w in &wins {
            if !Self::holds_snow(w, cap, self.cfg.docks) {
                if let Some(p) = self.piles.remove(&w.id) {
                    for (i, &h) in p.cols.iter().enumerate() {
                        if h > 0.0 {
                            spills.push((p.x + i as f32 * self.col, p.y, 0.0, 0.0, h));
                        }
                    }
                }
                continue;
            }
            let n = (w.w / self.col).ceil().max(1.0) as usize;
            let p = self.piles.entry(w.id.clone()).or_insert(Pile { cols: vec![0.0; n], x: w.x, y: w.y, w: w.w, seen: t, away: None });
            // back from another desktop: `seen` is old, so it isn't a fling
            p.away = None;
            if p.cols.len() != n {
                p.cols.resize(n, 0.0);
            }
            let dt = (t - p.seen).max(1e-3);
            let (dx, dy) = (w.x - p.x, w.y - p.y);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 0.5 && dt < 0.25 {
                let speed = dist / dt;
                // shaken or flung: snow flies off in the direction of travel
                // (thresholds in 96-dpi px/s)
                let sens = self.cfg.shake;
                let (fling, k) = (1100.0 * self.scale / sens.max(0.05), 2500.0 * self.scale);
                if sens > 0.0 && speed > fling {
                    let frac = ((speed - fling) / k).clamp(0.2, 0.85);
                    for (i, c) in p.cols.iter_mut().enumerate() {
                        if *c > 0.0 {
                            let lost = (*c * frac / self.step).ceil() * self.step;
                            let lost = lost.min(*c);
                            *c -= lost;
                            spills.push((w.x + i as f32 * self.col, w.y, dx / dt * 0.5, dy / dt * 0.5, lost));
                        }
                    }
                }
            }
            p.x = w.x;
            p.y = w.y;
            p.w = w.w;
            p.seen = t;
        }
        // windows that vanished (closed / minimized): their snow falls;
        // windows on another desktop keep theirs for a while
        let ids: std::collections::HashSet<&str> = wins.iter().map(|w| w.id.as_str()).collect();
        let away: std::collections::HashSet<&str> = away.iter().map(|s| s.as_str()).collect();
        let gone: Vec<String> = self.piles.keys().filter(|k| !ids.contains(k.as_str())).cloned().collect();
        for id in gone {
            if away.contains(id.as_str()) {
                let p = self.piles.get_mut(&id).unwrap();
                let since = *p.away.get_or_insert(t);
                if t - since > DORMANT_FOR {
                    self.piles.remove(&id);
                }
                continue;
            }
            if let Some(p) = self.piles.remove(&id) {
                if p.away.is_some() {
                    // closed while on another desktop: nothing here to fall
                    continue;
                }
                for (i, &h) in p.cols.iter().enumerate() {
                    if h > 0.0 {
                        spills.push((p.x + i as f32 * self.col, p.y, 0.0, 0.0, h));
                    }
                }
            }
        }
        for (x, y, vx, vy, a) in spills {
            self.spill(x, y, vx, vy, a);
        }
        // cursor velocity
        if let Some((px, py)) = self.cursor {
            let dt = (t - self.cursor_t).max(1e-3);
            if dt < 0.3 {
                self.cursor_v = ((cursor.0 - px) / dt, (cursor.1 - py) / dt);
            } else {
                self.cursor_v = (0.0, 0.0);
            }
        }
        self.cursor = Some(cursor);
        self.cursor_t = t;
        if let Ok(mut tops) = TOPS.lock() {
            *tops = visible_tops(&wins, self.sw, self.sh);
        }
        // a taskbar / panel along the bottom (across most of the screen)
        let dock = wins
            .iter()
            .filter(|w| w.kind == 1 && w.w > w.h * 3.0 && w.w > self.sw * 0.4 && w.y > self.sh * 0.6 && w.y < self.sh)
            .map(|w| w.y)
            .fold(None, |a: Option<f32>, y| Some(a.map_or(y, |a| a.min(y))));
        if let Ok(mut d) = DOCK_TOP.lock() {
            *d = dock.filter(|_| self.sh > 0.0).map(|y| y / self.sh);
        }
        self.wins = wins;
        self.brush();
        self.roll();
    }

    /// The button held down over a pile: the snow there gathers into a
    /// snowball, and dragged along the window's top it rolls, picking up
    /// the snow it passes over and growing. Let go, it stays sitting there
    /// (until the window goes, or it melts). Dragged up away from the top
    /// it's picked up, and let go it's thrown (with the pointer's swing); a
    /// fast fling off the top throws it too. A press on a sitting snowball
    /// picks it up again.
    fn roll(&mut self) {
        let k = self.scale;
        let held = self.button;
        let (pressed, released) = (held && !self.was_button, !held && self.was_button);
        self.was_button = held;
        // the windows' balls go with them (a window gone: its ball too),
        // all but one in the hand
        let ids: Vec<String> = self.wins.iter().map(|w| w.id.clone()).collect();
        self.balls.retain(|b| b.carried || ids.contains(&b.win));
        let Some((cx, cy)) = self.cursor else { return };
        if released {
            let (vx, vy) = self.cursor_v;
            let speed = (vx * vx + vy * vy).sqrt();
            let cap = 2600.0 * k;
            let (vx, vy) = if speed > cap { (vx * cap / speed, vy * cap / speed) } else { (vx, vy) };
            let mut thrown = vec![];
            for (i, b) in self.balls.iter().enumerate() {
                let from = if b.carried {
                    Some((cx, cy))
                } else if b.rolling && speed > 1100.0 * k {
                    // flung off the top
                    self.wins.iter().find(|w| w.id == b.win).map(|w| (w.x + b.dx, w.y - b.r))
                } else {
                    None
                };
                if let Some((x, y)) = from {
                    thrown.push((i, x, y, b.r));
                }
            }
            for &(i, x, y, r) in thrown.iter().rev() {
                self.balls.remove(i);
                let seed = self.rng.next_u32();
                let flight = 0.38 + 0.12 * (r / (20.0 * k)).min(1.0);
                self.throws.push(Throw { x, y, vx, vy, r, age: 0.0, flight, splat: None, seed, done: false });
            }
            for b in &mut self.balls {
                b.rolling = false;
                b.idle = 0.0;
            }
            return;
        }
        if !held {
            return;
        }
        let rolling = self.balls.iter().position(|b| b.rolling || b.carried);
        if rolling.is_none() && pressed {
            // a press on a sitting snowball: back in hand
            let hit = self.balls.iter().position(|b| {
                self.wins.iter().find(|w| w.id == b.win).is_some_and(|w| {
                    let (bx, by) = (w.x + b.dx, w.y - b.r);
                    (cx - bx).powi(2) + (cy - by).powi(2) <= (b.r + 6.0 * k).powi(2)
                })
            });
            if let Some(i) = hit {
                self.balls[i].rolling = true;
                self.balls[i].idle = 0.0;
                return;
            }
        }
        if rolling.is_some_and(|i| self.balls[i].carried) {
            return;
        }
        // which pile is under the pointer (or the one being rolled along)
        let target = match rolling {
            Some(i) => self.wins.iter().find(|w| w.id == self.balls[i].win).cloned(),
            None => self.wins.iter().rev().find(|w| {
                let Some(p) = self.piles.get(&w.id) else { return false };
                if cx < w.x || cx >= w.x + w.w {
                    return false;
                }
                let col = (((cx - w.x) / self.col) as usize).min(p.cols.len().saturating_sub(1));
                let top = p.cols.get(col).copied().unwrap_or(0.0);
                top > 1.0 && cy > w.y - top - 14.0 * k && cy < w.y + 4.0 * k && !self.covered(cx, w.y - 1.0, w.z)
            }).cloned(),
        };
        let Some(w) = target else { return };
        // dragged up away from the top: picked up
        if let Some(i) = rolling {
            if cy < w.y - self.balls[i].r * 2.0 - 24.0 * k {
                self.balls[i].carried = true;
                return;
            }
        }
        let Some(p) = self.piles.get_mut(&w.id) else { return };
        let i = match rolling {
            Some(i) => i,
            None => {
                self.balls.push(Ball { win: w.id.clone(), dx: cx - w.x, r: 2.0 * k, rolling: true, carried: false, idle: 0.0 });
                self.balls.len() - 1
            }
        };
        let r = self.balls[i].r;
        let x = cx.clamp(w.x + r, w.x + w.w - r);
        // the snow under the ball, gathered into it
        let col = self.col;
        let (c0, c1) = (((x - r - w.x) / col).floor().max(0.0) as usize, (((x + r - w.x) / col).ceil() as usize).min(p.cols.len()));
        let mut got = 0.0;
        for h in &mut p.cols[c0.min(c1)..c1] {
            let take = (*h * 0.25).min(*h);
            *h -= take;
            got += take * col;
        }
        // area in, radius up (a little at a time)
        let area = std::f32::consts::PI * r * r + got * 0.8;
        let r2 = (area / std::f32::consts::PI).sqrt().min(40.0 * k);
        let b = &mut self.balls[i];
        b.dx = x - w.x;
        b.r = r2;
    }

    /// Snowballs left sitting melt away; thrown ones fly, splat on the
    /// window they meet, slide down the glass and drop off the bottom.
    fn update_balls(&mut self, dt: f32) {
        let k = self.scale;
        for b in self.balls.iter_mut().filter(|b| !b.rolling && !b.carried) {
            b.idle += dt;
            if b.idle > 40.0 {
                b.r -= dt * 1.2 * k;
            }
        }
        self.balls.retain(|b| b.r > 1.5 * k || b.rolling || b.carried);
        let g = 1500.0 * k;
        let mut bursts = vec![];
        let wins = std::mem::take(&mut self.wins);
        for th in self.throws.iter_mut() {
            th.age += dt;
            match &mut th.splat {
                None => {
                    th.vy += g * dt;
                    th.vx *= 1.0 - (0.8 * dt).min(0.5);
                    th.x += th.vx * dt;
                    th.y += th.vy * dt;
                    if th.x < -200.0 * k || th.x > self.sw + 200.0 * k || th.y > self.sh + 200.0 * k {
                        th.done = true;
                    } else if th.age >= th.flight {
                        // it meets the glass: the window in front there, if any
                        let hit = wins
                            .iter()
                            .filter(|w| (w.kind == 0 || w.kind == 3) && th.x >= w.x && th.x < w.x + w.w && th.y >= w.y && th.y < w.y + w.h)
                            .max_by_key(|w| w.z);
                        let r = th.r * (1.0 - 0.3 * (th.age / th.flight).min(1.0));
                        match hit {
                            Some(w) => th.splat = Some(Splat { win: w.id.clone(), dx: th.x - w.x, dy: th.y - w.y, slid: 0.0, v: 0.0, age: 0.0, r }),
                            None => {
                                bursts.push((th.x, th.y, r));
                                th.done = true;
                            }
                        }
                    }
                }
                Some(s) => {
                    s.age += dt;
                    let Some(w) = wins.iter().find(|w| w.id == s.win) else {
                        th.done = true;
                        continue;
                    };
                    // it sticks a moment, then slides, slowly
                    if s.age > 0.8 {
                        s.v = (s.v + 26.0 * k * dt).min(55.0 * k);
                        s.slid += s.v * dt;
                    }
                    if s.dy + s.slid > w.h - s.r * 0.4 {
                        bursts.push((w.x + s.dx, w.y + w.h, s.r * 0.7));
                        th.done = true;
                    } else if s.age > 30.0 {
                        th.done = true;
                    }
                }
            }
        }
        self.wins = wins;
        self.throws.retain(|t| !t.done);
        for (x, y, r) in bursts {
            self.spill(x, y, 0.0, -60.0 * k, (r * r * 0.25).max(self.step));
        }
    }

    /// Sweeping the cursor through a pile knocks the snow away.
    fn brush(&mut self) {
        // (the button held: rolling a snowball, not sweeping)
        if !self.cfg.brush || self.button {
            return;
        }
        let Some((cx, cy)) = self.cursor else { return };
        let (vx, vy) = self.cursor_v;
        let k = self.scale;
        if vx * vx + vy * vy < 60.0 * 60.0 * k * k {
            return;
        }
        let mut spills = vec![];
        for w in self.wins.iter().rev() {
            let Some(p) = self.piles.get(&w.id) else { continue };
            if cx < w.x - self.col || cx >= w.x + w.w + self.col {
                continue;
            }
            let col = ((cx - w.x) / self.col).floor() as i32;
            let top = p.cols.get(col.clamp(0, p.cols.len() as i32 - 1) as usize).copied().unwrap_or(0.0);
            if cy < w.y - top - 10.0 * k || cy > w.y + 6.0 * k || self.covered(cx, w.y - 1.0, w.z) {
                continue;
            }
            spills.push((w.id.clone(), col));
            break;
        }
        for (id, col) in spills {
            let (wx, wy, cols) = {
                let p = self.piles.get_mut(&id).unwrap();
                let mut taken = vec![];
                for i in (col - 2)..=(col + 2) {
                    if i >= 0 && (i as usize) < p.cols.len() && p.cols[i as usize] > 0.0 {
                        taken.push((i as usize, p.cols[i as usize]));
                        p.cols[i as usize] = 0.0;
                    }
                }
                (p.x, p.y, taken)
            };
            for (i, h) in cols {
                self.spill(wx + i as f32 * self.col, wy, vx * 0.45, vy * 0.45 - 120.0 * k, h);
            }
        }
    }

    /// Topmost visible surface a falling thing crosses between y0 and y1 at x.
    fn land_at(&self, x: f32, y0: f32, y1: f32) -> Option<(String, usize)> {
        let cap = self.cap();
        for w in self.wins.iter().rev() {
            if !Self::holds_snow(w, cap, self.cfg.docks) || x < w.x || x >= w.x + w.w {
                continue;
            }
            let Some(p) = self.piles.get(&w.id) else { continue };
            let col = (((x - w.x) / self.col) as usize).min(p.cols.len() - 1);
            let sy = w.y - p.cols[col];
            if y0 <= sy && sy < y1 && !self.covered(x, sy - 1.0, w.z) {
                return Some((w.id.clone(), col));
            }
        }
        None
    }

    fn add(&mut self, id: &str, col: usize) {
        let cap = self.cap_for(id);
        let Some(p) = self.piles.get_mut(id) else { return };
        let n = p.cols.len();
        // rounded ends: the outermost columns hold less
        let end = (col.min(n - 1 - col) as f32 + 1.0) / 3.0;
        let lim = (cap * end.min(1.0)).max(self.step);
        if p.cols[col] + self.step <= lim {
            p.cols[col] += self.step;
        } else {
            // roll to a lower neighbour if there is one
            for nb in [col.wrapping_sub(1), col + 1] {
                if nb < n && p.cols[nb] + self.step < p.cols[col] {
                    p.cols[nb] += self.step;
                    return;
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32, t: f32) {
        self.update_balls(dt);
        let k = self.scale;
        let wind = (noise1(t * 0.07, 33) - 0.5) * 30.0 * k * self.cfg.wind;
        // melting: now and then each pile loses a little off its highest point
        if self.cfg.melt > 0.0 {
            self.melt_acc += dt * self.cfg.melt * 0.35;
            while self.melt_acc >= 1.0 {
                self.melt_acc -= 1.0;
                let step = self.step;
                for p in self.piles.values_mut() {
                    if let Some(top) = p.cols.iter_mut().max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)) {
                        *top = (*top - step).max(0.0);
                    }
                }
            }
        }
        // falling flakes
        let mut flakes = std::mem::take(&mut self.flakes);
        if self.cfg.snow {
            for f in flakes.iter_mut() {
                let ny = f.y + f.vy * dt;
                let nx = f.x + (wind + (t * 0.9 + f.phase).sin() * f.sway) * dt;
                if let Some((id, col)) = self.land_at(nx, f.y, ny) {
                    // settling speed: each landing adds `settle` steps on average
                    let s = self.cfg.settle.max(0.0);
                    let mut n = s.floor() as u32 + self.rng.chance(s.fract()) as u32;
                    while n > 0 {
                        self.add(&id, col);
                        n -= 1;
                    }
                    *f = self.new_flake(false);
                } else {
                    f.x = nx;
                    f.y = ny;
                    if f.y > self.sh + 10.0 * k || f.x < -20.0 * k || f.x > self.sw + 20.0 * k {
                        *f = self.new_flake(false);
                    }
                }
            }
        }
        self.flakes = flakes;
        if !self.cfg.snow {
            return;
        }
        // flying clumps
        let mut parts = std::mem::take(&mut self.parts);
        parts.retain_mut(|p| {
            p.age += dt;
            p.vy += 900.0 * k * dt;
            p.vx *= 1.0 - 1.5 * dt;
            let ny = p.y + p.vy * dt;
            let nx = p.x + p.vx * dt;
            if p.vy > 0.0 && p.age > 0.15 {
                if let Some((id, col)) = self.land_at(nx, p.y, ny) {
                    if self.rng.chance(0.6) {
                        self.add(&id, col);
                    }
                    return false;
                }
            }
            p.x = nx;
            p.y = ny;
            p.age < p.life && p.y < self.sh + 20.0 * k
        });
        self.parts = parts;
        // settle: snow slumps until neighbouring columns differ by ≤ 2 steps
        let flip = (t * 10.0) as i32 % 2 == 0;
        for p in self.piles.values_mut() {
            let n = p.cols.len();
            for k in 0..n.saturating_sub(1) {
                let i = if flip { k } else { n - 2 - k };
                let d = p.cols[i] - p.cols[i + 1];
                if d > 2.0 * self.step {
                    p.cols[i] -= self.step;
                    p.cols[i + 1] += self.step;
                } else if -d > 2.0 * self.step {
                    p.cols[i + 1] -= self.step;
                    p.cols[i] += self.step;
                }
            }
        }
    }

    /// One-line status for debugging (XMAS_SNOW_DEBUG=1).
    pub fn status(&self) -> String {
        let snow: f32 = self.piles.values().map(|p| p.cols.iter().sum::<f32>()).sum();
        let mut tops: Vec<String> = self
            .wins
            .iter()
            .map(|w| {
                let m = self.piles.get(&w.id).map(|p| p.cols.iter().cloned().fold(0.0, f32::max)).unwrap_or(-1.0);
                format!("[{} {} {} {} k{} z{} max{:.0}]", w.x, w.y, w.w, w.h, w.kind, w.z, m)
            })
            .collect();
        tops.truncate(12);
        let dormant = self.piles.values().filter(|p| p.away.is_some()).count();
        format!(
            "screen {}x{} origin {:?} wins {} piles {} dormant {dormant} snow_px {:.0} flakes {} parts {} cursor {:?} hidden {} {}",
            self.sw, self.sh, self.origin, self.wins.len(), self.piles.len(), snow, self.flakes.len(), self.parts.len(), self.cursor, self.hidden, tops.join(" ")
        )
    }

    // ------------------------------------------------------------------

    pub fn render(&mut self, c: &mut Canvas, glyphs: &mut Glyphs, t: f32) {
        if self.hidden {
            return;
        }
        // piles: stacked eighth-blocks, white with a bright crust and cool shade
        for w in self.wins.iter().filter(|_| self.cfg.snow) {
            let Some(p) = self.piles.get(&w.id) else { continue };
            let top = p.cols.iter().cloned().fold(0.0, f32::max);
            if top < 0.5 {
                continue;
            }
            c.rec = false;
            // everything below stays inside this rect (it's what gets cleared)
            c.touch(w.x, w.y - top - 2.0, w.w, top + 2.0);
            for (i, &h) in p.cols.iter().enumerate() {
                let q = (h / self.step).round() * self.step;
                if q < 0.5 {
                    continue;
                }
                let x0 = w.x + i as f32 * self.col;
                let cw = self.col.min(w.x + w.w - x0);
                if cw <= 0.0 || self.covered(x0 + cw / 2.0, w.y - 1.0, w.z) {
                    continue;
                }
                let y0 = w.y - q;
                c.fill(x0, y0, cw, q, argb(0xf0, 0xd6, 0xe0, 0xf2));
                c.fill(x0, y0, cw, 2.0, argb(0xff, 0xf6, 0xfa, 0xff));
                c.fill(x0, w.y - 2.0, cw, 2.0, argb(0xd0, 0xa8, 0xb6, 0xd4));
                // an occasional glint, kept inside the column (a narrow last
                // column or a one-step pile would let it poke out)
                let (gw, gh) = ((cw - 3.0).min(2.0), (q - 1.0).min(2.0));
                if gw > 0.0 && gh > 0.0 && noise1(t * 2.0 + i as f32 * 7.3 + w.z as f32, 9) > 0.93 {
                    c.fill(x0 + 3.0, y0 + 1.0, gw, gh, argb(0xff, 0xff, 0xff, 0xff));
                }
            }
            c.rec = true;
        }
        // snowballs rolled up out of the piles (or in the hand)
        let px = (2.0 * k_of(self.scale)).round().max(2.0);
        for b in &self.balls {
            let (x, y, base) = if b.carried {
                let Some((cx, cy)) = self.cursor else { continue };
                (cx, cy, None)
            } else {
                let Some(w) = self.wins.iter().find(|w| w.id == b.win) else { continue };
                let x = w.x + b.dx;
                if self.covered(x, w.y - 1.0, w.z) {
                    continue;
                }
                (x, w.y - b.r, Some(w.y))
            };
            let r = b.r;
            c.rec = false;
            c.touch(x - r - px * 2.0, y - r - px * 2.0, r * 2.0 + px * 4.0, r * 2.0 + px * 5.0);
            // a soft shadow under one sitting on a top
            if let Some(top) = base {
                c.fill((x - r * 0.8).round(), top - px, (r * 1.6).round(), px, argb(0x60, 0x50, 0x5a, 0x80));
            }
            pixel_ball(c, x, y, r, px, 0xff);
            c.rec = true;
        }
        // thrown: in the air (getting smaller as it goes), then splatted
        for th in &self.throws {
            match &th.splat {
                None => {
                    let r = th.r * (1.0 - 0.3 * (th.age / th.flight).min(1.0));
                    c.rec = false;
                    c.touch(th.x - r - px * 2.0, th.y - r - px * 2.0, r * 2.0 + px * 4.0, r * 2.0 + px * 4.0);
                    pixel_ball(c, th.x, th.y, r, px, 0xff);
                    c.rec = true;
                }
                Some(s) => {
                    let Some(w) = self.wins.iter().find(|w| w.id == s.win) else { continue };
                    let (x, y) = (w.x + s.dx, w.y + s.dy + s.slid);
                    if self.covered(x, y, w.z) {
                        continue;
                    }
                    // fading out at the very end
                    let a = (((30.0 - s.age) / 2.0).clamp(0.0, 1.0) * 255.0) as u8;
                    let r = s.r;
                    c.rec = false;
                    c.touch(x - r * 2.2 - px * 2.0, w.y + s.dy - r * 1.6 - px * 2.0, r * 4.4 + px * 4.0, s.slid + r * 4.2 + px * 4.0);
                    // the wet streak it leaves on the way down
                    if s.slid > px {
                        let tw = (r * 0.9 / px).round().max(1.0) * px;
                        c.fill((x - tw / 2.0).round(), (w.y + s.dy).round(), tw, s.slid.round(), argb(a / 6, 0xd8, 0xe6, 0xff));
                    }
                    pixel_splat(c, x, y, r, px, th.seed, a);
                    c.rec = true;
                }
            }
        }
        // light strings around the topmost few normal windows
        if self.cfg.lights {
            let mut lights = std::mem::take(&mut self.lights);
            let cfg = self.cfg;
            lights.draw(c, &self.wins, |x, y, z| self.covered(x, y, z), self.corner, self.scale, t, cfg.light_style, cfg.light_windows as usize, cfg.light_glow);
            self.lights = lights;
        }
        // falling flakes (unless they're set to stay out of the way) and
        // flying clumps
        if self.cfg.snow && self.cfg.front {
            for f in &self.flakes {
                glyphs.draw(c, f.ch, f.x, f.y, (0xf2, 0xf6, 0xff), 0.85, 13.0 * self.scale);
            }
        }
        for p in self.parts.iter().filter(|_| self.cfg.snow) {
            let a = (1.0 - p.age / p.life).clamp(0.0, 1.0);
            c.fill(p.x, p.y, 3.0 * self.scale, 3.0 * self.scale, argb((235.0 * a) as u8, 0xee, 0xf4, 0xff));
        }
    }
}

fn k_of(scale: f32) -> f32 {
    scale.max(0.5)
}

fn h32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// The snowball palette: outline, shadow, mid, light, highlight.
const BALL: [(u8, u8, u8); 5] = [(0x6f, 0x80, 0xa8), (0xa6, 0xb7, 0xd6), (0xcf, 0xdc, 0xf0), (0xec, 0xf2, 0xff), (0xff, 0xff, 0xff)];

fn ball_px(i: usize, a: u8) -> u32 {
    let (r, g, b) = BALL[i];
    let k = a as u32;
    argb(a, (r as u32 * k / 255) as u8, (g as u32 * k / 255) as u8, (b as u32 * k / 255) as u8)
}

/// A snowball in pixel art: whole `px`-sized blocks, lit from the top
/// left, a darker rim, one bright glint.
fn pixel_ball(c: &mut Canvas, x: f32, y: f32, r: f32, px: f32, a: u8) {
    let (x, y) = (x.round(), y.round());
    let n = (r / px).ceil() as i32;
    // (a hair inside the radius: no lone block poking out at each side)
    let rr = (r - px * 0.35).max(px * 0.5).powi(2);
    let rim = (r - px * 1.35).max(0.0).powi(2);
    for j in -n..=n {
        for i in -n..=n {
            let (bx, by) = (i as f32 * px, j as f32 * px);
            let d = bx * bx + by * by;
            if d > rr {
                continue;
            }
            let (u, v) = (bx / r.max(1.0), by / r.max(1.0));
            // lit from the top left: a bright cap, a shadowed crescent
            // underneath, a darker rim round the lower side
            let lit = (u + 0.28).powi(2) + (v + 0.32).powi(2) < 0.62f32.powi(2);
            let shade = if d > rim && u + v > -0.35 {
                0
            } else if (u + 0.18).powi(2) + (v + 0.2).powi(2) > 0.98f32.powi(2) {
                1
            } else if lit {
                3
            } else {
                2
            };
            c.fill(x + bx - px / 2.0, y + by - px / 2.0, px, px, ball_px(shade, a));
        }
    }
    // the glint
    if n >= 2 {
        let g = (-(n as f32) * 0.45).round() as i32;
        c.fill(x + g as f32 * px - px / 2.0, y + g as f32 * px - px / 2.0, px, px, ball_px(4, a));
    }
}

/// A snowball flattened against the glass: a lumpy pixel blob, a few
/// flecks round it, and a drip or two hanging under it.
fn pixel_splat(c: &mut Canvas, x: f32, y: f32, r: f32, px: f32, seed: u32, a: u8) {
    let (x, y) = (x.round(), y.round());
    let (ha, hb) = ((r * 1.5).max(px), (r * 0.85).max(px));
    let (nx, ny) = ((ha * 1.3 / px).ceil() as i32, (hb * 1.3 / px).ceil() as i32);
    for j in -ny..=ny {
        for i in -nx..=nx {
            let (bx, by) = (i as f32 * px, j as f32 * px);
            let d = (bx / ha).powi(2) + (by / hb).powi(2);
            // a lumpy edge: a bit more or less in each direction
            let ang = ((by.atan2(bx) + std::f32::consts::PI) / std::f32::consts::TAU * 12.0) as u32;
            let lump = (h32(seed ^ ang.wrapping_mul(0x9e37)) % 100) as f32 / 100.0 * 0.45;
            if d > 1.0 + lump {
                continue;
            }
            let shade = if d > 0.8 + lump * 0.8 {
                1
            } else if d < 0.25 && j <= 0 {
                3
            } else {
                2
            };
            c.fill(x + bx - px / 2.0, y + by - px / 2.0, px, px, ball_px(shade, a));
        }
    }
    // flecks thrown off round it
    for f in 0..5u32 {
        let h = h32(seed.wrapping_add(f * 0x51));
        let ang = (h % 360) as f32 / 360.0 * std::f32::consts::TAU;
        let dist = 1.25 + (h >> 9) as f32 % 50.0 / 100.0;
        let (fx, fy) = ((ang.cos() * ha * dist / px).round() * px, (ang.sin() * hb * dist / px).round() * px);
        c.fill(x + fx - px / 2.0, y + fy - px / 2.0, px, px, ball_px(3, a));
    }
    // drips under it
    for d in 0..2u32 {
        let h = h32(seed ^ (0xd00d + d));
        let dx = ((h % 100) as f32 / 100.0 - 0.5) * ha * 1.2;
        let len = ((0.4 + (h >> 8) as f32 % 80.0 / 100.0) * r / px).round().max(1.0);
        let dx = (dx / px).round() * px;
        for k in 0..len as i32 {
            c.fill(x + dx - px / 2.0, y + hb + k as f32 * px - px / 2.0, px, px, ball_px(if k as f32 == len - 1.0 { 3 } else { 2 }, a));
        }
    }
}

/// Premultiplied ARGB.
pub fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    let k = a as u32;
    (k << 24) | ((r as u32 * k / 255) << 16) | ((g as u32 * k / 255) << 8) | (b as u32 * k / 255)
}

#[inline]
fn over(dst: u32, src: u32) -> u32 {
    let sa = src >> 24;
    if sa == 255 {
        return src;
    }
    let inv = 255 - sa;
    let ch = |s: u32| -> u32 {
        let sc = (src >> s) & 0xff;
        let dc = (dst >> s) & 0xff;
        (sc + dc * inv / 255).min(255) << s
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// Transparent drawing surface that remembers what it touched (for
/// clearing next frame and for compositor damage).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
    pub touched: Vec<(i32, i32, i32, i32)>,
    /// When false, draws don't record their own rects (caller records a
    /// bounding rect instead — keeps damage lists short).
    pub rec: bool,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![0; w * h], touched: vec![], rec: true }
    }

    fn clip(&self, x: f32, y: f32, w: f32, h: f32) -> Option<(usize, usize, usize, usize)> {
        let x0 = x.floor().max(0.0) as i64;
        let y0 = y.floor().max(0.0) as i64;
        let x1 = ((x + w).ceil() as i64).min(self.w as i64);
        let y1 = ((y + h).ceil() as i64).min(self.h as i64);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
    }

    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: u32) {
        let Some((x0, y0, x1, y1)) = self.clip(x, y, w, h) else { return };
        for yy in y0..y1 {
            let row = yy * self.w;
            for xx in x0..x1 {
                self.px[row + xx] = over(self.px[row + xx], c);
            }
        }
        if self.rec {
            self.touched.push((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32));
        }
    }

    /// Record a (clipped) rect as touched.
    pub fn touch(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some((x0, y0, x1, y1)) = self.clip(x, y, w, h) {
            self.touched.push((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32));
        }
    }

    /// Wipe everything drawn last frame; returns those rects (for damage).
    pub fn clear_touched(&mut self) -> Vec<(i32, i32, i32, i32)> {
        let rects = std::mem::take(&mut self.touched);
        for &(x, y, w, h) in &rects {
            for yy in y..y + h {
                let row = yy as usize * self.w;
                self.px[row + x as usize..row + (x + w) as usize].fill(0);
            }
        }
        rects
    }
}

/// Glyph sprites (falling flakes), rendered from the bundled font.
pub struct Glyphs {
    font: fontdue::Font,
    cache: HashMap<(char, u32), (i32, i32, usize, usize, Vec<u8>)>,
}

impl Glyphs {
    pub fn new() -> Self {
        Glyphs {
            font: fontdue::Font::from_bytes(crate::raster::FONT_DATA[1], fontdue::FontSettings::default()).expect("font"),
            cache: HashMap::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, c: &mut Canvas, ch: char, x: f32, y: f32, col: (u8, u8, u8), alpha: f32, size: f32) {
        let font = &self.font;
        let (gx, gy, w, h, bmp) = self.cache.entry((ch, size.to_bits())).or_insert_with(|| {
            let (m, b) = font.rasterize(ch, size);
            (m.xmin, -(m.height as i32) - m.ymin, m.width, m.height, b)
        });
        let (x0, y0) = (x.round() as i32 + *gx, y.round() as i32 + *gy);
        let mut any = false;
        for yy in 0..*h {
            let py = y0 + yy as i32;
            if py < 0 || py as usize >= c.h {
                continue;
            }
            for xx in 0..*w {
                let px = x0 + xx as i32;
                if px < 0 || px as usize >= c.w {
                    continue;
                }
                let a = bmp[yy * *w + xx] as f32 / 255.0 * alpha;
                if a > 0.02 {
                    let i = py as usize * c.w + px as usize;
                    c.px[i] = over(c.px[i], argb((a * 255.0) as u8, col.0, col.1, col.2));
                    any = true;
                }
            }
        }
        if any {
            c.touch(x0 as f32, y0 as f32, *w as f32, *h as f32);
        }
    }
}

/// Most damage rects handed to the compositor per commit. KWin repaints
/// each rect as its own scissored pass, so hundreds of tiny rects cost far
/// more than a few somewhat larger ones. (Measured on a nested KWin: its
/// cost per commit is flat from 8 to ~128 rects and rises beyond that, so
/// 64 keeps the uploaded area small without paying for the rect count.)
const MAX_RECTS: usize = 64;

/// Side of the tiles that change detection and damage work in.
const TILE: i32 = 16;

/// Coalesce many small rects into at most MAX_RECTS: snap them to a tile
/// grid (16px to start with), join tiles into row runs and equal runs into
/// columns, and retry with tiles twice as big while there are too many.
pub fn merge_damage(rects: &[(i32, i32, i32, i32)], w: i32, h: i32) -> Vec<(i32, i32, i32, i32)> {
    let (mut tw, mut th) = ((w + TILE - 1) / TILE, (h + TILE - 1) / TILE);
    let mut grid = tile_grid(rects, tw, th);
    let mut t = TILE;
    loop {
        let out = grid_rects(&grid, tw, th, t, w, h);
        if out.len() <= MAX_RECTS || (tw <= 1 && th <= 1) {
            return out;
        }
        // twice the tile size: each new tile is set if any of its 2x2 was
        let (nw, nh) = ((tw + 1) / 2, (th + 1) / 2);
        let mut coarse = vec![false; (nw * nh) as usize];
        for ty in 0..th {
            for tx in 0..tw {
                if grid[(ty * tw + tx) as usize] {
                    coarse[((ty / 2) * nw + tx / 2) as usize] = true;
                }
            }
        }
        (grid, tw, th, t) = (coarse, nw, nh, t * 2);
    }
}

/// Which tiles of a tw x th grid of 16px tiles the rects touch.
fn tile_grid(rects: &[(i32, i32, i32, i32)], tw: i32, th: i32) -> Vec<bool> {
    let mut grid = vec![false; (tw * th).max(0) as usize];
    for &(x, y, rw, rh) in rects {
        if rw <= 0 || rh <= 0 {
            continue;
        }
        for ty in (y / TILE).max(0)..=((y + rh - 1) / TILE).min(th - 1) {
            for tx in (x / TILE).max(0)..=((x + rw - 1) / TILE).min(tw - 1) {
                grid[(ty * tw + tx) as usize] = true;
            }
        }
    }
    grid
}

/// The set tiles of a tw x th grid of t-px tiles as rects: row runs, with a
/// run that exactly continues one from the row above merged into it.
fn grid_rects(grid: &[bool], tw: i32, th: i32, t: i32, w: i32, h: i32) -> Vec<(i32, i32, i32, i32)> {
    let mut out: Vec<(i32, i32, i32, i32)> = vec![];
    let mut open: Vec<usize> = vec![]; // rects that reach the previous row
    for ty in 0..th {
        let mut still_open = vec![];
        let mut tx = 0;
        while tx < tw {
            if !grid[(ty * tw + tx) as usize] {
                tx += 1;
                continue;
            }
            let start = tx;
            while tx < tw && grid[(ty * tw + tx) as usize] {
                tx += 1;
            }
            let (x, y) = (start * t, ty * t);
            let (rw, rh) = ((tx * t).min(w) - x, ((ty + 1) * t).min(h) - y);
            match open.iter().find(|&&k| out[k].0 == x && out[k].2 == rw) {
                Some(&k) => {
                    out[k].3 += rh;
                    still_open.push(k);
                }
                None => {
                    out.push((x, y, rw, rh));
                    still_open.push(out.len() - 1);
                }
            }
        }
        open = still_open;
    }
    out
}

/// Finds what really changed on the canvas since the last frame, so things
/// that are redrawn identically every frame (the light wire, settled snow)
/// cost the compositor nothing. Hashes each 16px tile that was drawn into
/// or cleared, and compares with the tile's hash from last time.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Changes {
    tw: usize,
    th: usize,
    hash: Vec<u64>,
    /// Frame stamp per tile, so each tile is hashed at most once a frame.
    seen: Vec<u32>,
    stamp: u32,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Changes {
    pub fn new(w: usize, h: usize) -> Self {
        let t = TILE as usize;
        let (tw, th) = (w.div_ceil(t), h.div_ceil(t));
        // a blank tile hashes to 0, and the canvas starts out blank
        Changes { tw, th, hash: vec![0; tw * th], seen: vec![0; tw * th], stamp: 0 }
    }

    /// The tiles under `rects` whose pixels differ from the last call, as
    /// rects in canvas pixels.
    pub fn find(&mut self, c: &Canvas, rects: &[(i32, i32, i32, i32)]) -> Vec<(i32, i32, i32, i32)> {
        self.stamp = self.stamp.wrapping_add(1);
        let t = TILE as usize;
        let mut out = vec![];
        for &(x, y, rw, rh) in rects {
            if rw <= 0 || rh <= 0 {
                continue;
            }
            let (x, y, rw, rh) = (x as usize, y as usize, rw as usize, rh as usize);
            for ty in y / t..=((y + rh - 1) / t).min(self.th - 1) {
                for tx in x / t..=((x + rw - 1) / t).min(self.tw - 1) {
                    let i = ty * self.tw + tx;
                    if self.seen[i] == self.stamp {
                        continue;
                    }
                    self.seen[i] = self.stamp;
                    let (x0, y0) = (tx * t, ty * t);
                    let (x1, y1) = ((x0 + t).min(c.w), (y0 + t).min(c.h));
                    let hsh = tile_hash(&c.px, c.w, x0, y0, x1, y1);
                    if hsh != self.hash[i] {
                        self.hash[i] = hsh;
                        out.push((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32));
                    }
                }
            }
        }
        out
    }
}

/// Hash of a block of pixels; 0 exactly when they're all transparent.
/// (Each step is a bijection of the running hash, so a single changed
/// pixel always changes the result.)
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn tile_hash(px: &[u32], stride: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> u64 {
    const K: u64 = 0x0100_0000_01b3;
    // pixels are taken in pairs, into four independent lanes so the
    // multiplies don't serialize
    let mut lanes = [0u64; 4];
    for y in y0..y1 {
        let row = &px[y * stride + x0..y * stride + x1];
        let mut octs = row.chunks_exact(8);
        for o in &mut octs {
            for (l, pair) in lanes.iter_mut().zip(o.chunks_exact(2)) {
                *l = (*l ^ (pair[0] as u64 | (pair[1] as u64) << 32)).wrapping_mul(K);
            }
        }
        for (l, &p) in lanes.iter_mut().zip(octs.remainder()) {
            *l = (*l ^ p as u64).wrapping_mul(K);
        }
    }
    lanes.iter().fold(0, |h, &l| (h ^ l).wrapping_mul(K))
}

/// Rects snapped to the 16px tile grid and joined into runs, without
/// coarsening: the same pixels, each once, in long rows (for copying).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn coalesce(rects: &[(i32, i32, i32, i32)], w: i32, h: i32) -> Vec<(i32, i32, i32, i32)> {
    let (tw, th) = ((w + TILE - 1) / TILE, (h + TILE - 1) / TILE);
    grid_rects(&tile_grid(rects, tw, th), tw, th, TILE, w, h)
}

/// Copy a rect of the canvas into a same-sized ARGB8888 (little-endian)
/// pixel buffer.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn copy_rect(c: &Canvas, dst: &mut [u8], (x, y, rw, rh): (i32, i32, i32, i32)) {
    for yy in y..y + rh {
        let i = yy as usize * c.w + x as usize;
        let (src, dst) = (&c.px[i..i + rw as usize], &mut dst[i * 4..(i + rw as usize) * 4]);
        // ARGB8888 is little-endian u32s, so on a little-endian CPU this is
        // a plain memcpy
        #[cfg(target_endian = "little")]
        dst.copy_from_slice(unsafe { std::slice::from_raw_parts(src.as_ptr().cast::<u8>(), src.len() * 4) });
        #[cfg(not(target_endian = "little"))]
        for (d, s) in dst.chunks_exact_mut(4).zip(src) {
            d.copy_from_slice(&s.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FxCfg {
        FxCfg { lights: false, ..FxCfg::default() }
    }

    #[test]
    fn window_tops_show_where_nothing_is_in_front() {
        // a at the back; b in front, over the middle of a's top edge
        let a = win("a", 100.0, 300.0, 600.0, 300.0, 1);
        let b = win("b", 300.0, 200.0, 200.0, 300.0, 2);
        let tops = visible_tops(&[a, b], 1000.0, 1000.0);
        let mine: Vec<&Top> = tops.iter().filter(|t| (t.y - 0.3).abs() < 1e-6).collect();
        assert_eq!(mine.len(), 2, "a's top, either side of b: {tops:?}");
        assert!((mine[0].x0 - 0.1).abs() < 1e-6 && (mine[0].x1 - 0.3).abs() < 1e-6);
        assert!((mine[1].x0 - 0.5).abs() < 1e-6 && (mine[1].x1 - 0.7).abs() < 1e-6);
        assert_ne!(mine[0].id, mine[1].id);
        // b's own top is all there
        assert!(tops.iter().any(|t| (t.y - 0.2).abs() < 1e-6 && (t.x1 - t.x0 - 0.2).abs() < 1e-6));
        // a maximized window or the taskbar isn't climbed
        let mut m = win("m", 0.0, 0.0, 1000.0, 1000.0, 3);
        m.kind = 3;
        assert!(visible_tops(&[m], 1000.0, 1000.0).is_empty());
    }

    #[test]
    fn holding_the_button_rolls_the_snow_into_a_snowball() {
        let mut fx = Fx::new(1920.0, 1080.0, FxCfg::default(), 1.0);
        let w = win("a", 400.0, 300.0, 600.0, 400.0, 1);
        fx.set_windows(vec![w.clone()], &[], (0.0, 0.0), 0.0);
        if let Some(p) = fx.piles.get_mut("a") {
            p.cols.iter_mut().for_each(|h| *h = 12.0);
        }
        let before: f32 = fx.piles["a"].cols.iter().sum();
        fx.button = true;
        for k in 0..30 {
            fx.set_windows(vec![w.clone()], &[], (500.0 + k as f32 * 8.0, 292.0), 0.1 * k as f32);
        }
        let (r, rolling) = fx.balls.first().map(|b| (b.r, b.rolling)).expect("no snowball");
        assert!(rolling && r > 6.0, "radius {r}");
        assert!(fx.piles["a"].cols.iter().sum::<f32>() < before, "no snow picked up");
        // let go: it stays put, done rolling
        fx.button = false;
        fx.set_windows(vec![w], &[], (900.0, 292.0), 4.0);
        assert!(!fx.balls[0].rolling);
    }

    #[test]
    fn a_snowball_picked_up_and_thrown_splats_and_slides_off() {
        let mut fx = Fx::new(1920.0, 1080.0, FxCfg::default(), 1.0);
        let a = win("a", 400.0, 300.0, 600.0, 400.0, 1);
        let b = win("b", 1100.0, 20.0, 700.0, 900.0, 2);
        fx.set_windows(vec![a.clone(), b.clone()], &[], (0.0, 0.0), 0.0);
        if let Some(p) = fx.piles.get_mut("a") {
            p.cols.iter_mut().for_each(|h| *h = 12.0);
        }
        fx.button = true;
        let mut t = 0.0;
        for k in 0..20 {
            t = 0.1 * k as f32;
            fx.set_windows(vec![a.clone(), b.clone()], &[], (500.0 + k as f32 * 6.0, 292.0), t);
        }
        assert_eq!(fx.balls.len(), 1);
        // dragged up off the top: in the hand
        for k in 0..6 {
            t += 0.05;
            fx.set_windows(vec![a.clone(), b.clone()], &[], (620.0 + k as f32 * 4.0, 250.0 - k as f32 * 20.0), t);
        }
        assert!(fx.balls[0].carried, "{:?}", fx.balls);
        // a fling towards the other window, let go
        fx.set_windows(vec![a.clone(), b.clone()], &[], (700.0, 120.0), t + 0.05);
        fx.set_windows(vec![a.clone(), b.clone()], &[], (780.0, 100.0), t + 0.1);
        fx.button = false;
        fx.set_windows(vec![a.clone(), b.clone()], &[], (860.0, 80.0), t + 0.15);
        assert!(fx.balls.is_empty() && fx.throws.len() == 1, "not thrown");
        // it flies, meets the glass of the window in front, and sticks
        for _ in 0..40 {
            fx.update(1.0 / 60.0, t);
        }
        let s = fx.throws[0].splat.as_ref().expect("didn't splat");
        assert_eq!(s.win, "b");
        // then slides down and off the bottom
        for _ in 0..60 * 30 {
            fx.update(1.0 / 60.0, t);
        }
        assert!(fx.throws.is_empty(), "still on the glass");
        // drawing one leaves nothing outside what's wiped
        let mut c = Canvas::new(1920, 1080);
        let mut g = Glyphs::new();
        fx.throws.push(Throw { x: 1300.0, y: 500.0, vx: 0.0, vy: 0.0, r: 14.0, age: 1.0, flight: 0.4, splat: Some(Splat { win: "b".into(), dx: 200.0, dy: 300.0, slid: 40.0, v: 0.0, age: 3.0, r: 14.0 }), seed: 7, done: false });
        fx.balls.push(Ball { win: "a".into(), dx: 100.0, r: 12.0, rolling: false, carried: false, idle: 0.0 });
        fx.render(&mut c, &mut g, 0.0);
        let rects = c.clear_touched();
        assert!(!rects.is_empty());
        assert!(c.px.iter().all(|&p| p == 0), "pixels left outside the touched rects");
    }

    /// A look at the snowball drawings: XMAS_SNOW_PPM=out.ppm.
    #[test]
    #[ignore]
    fn snowball_look() {
        let mut c = Canvas::new(240, 120);
        c.fill(0.0, 0.0, 240.0, 120.0, argb(0xff, 0x30, 0x3a, 0x55));
        pixel_ball(&mut c, 30.0, 40.0, 6.0, 2.0, 0xff);
        pixel_ball(&mut c, 70.0, 40.0, 14.0, 2.0, 0xff);
        pixel_ball(&mut c, 120.0, 40.0, 24.0, 3.0, 0xff);
        pixel_splat(&mut c, 60.0, 95.0, 12.0, 2.0, 11, 0xff);
        pixel_splat(&mut c, 160.0, 90.0, 20.0, 3.0, 5, 0xff);
        if let Some(out) = std::env::var_os("XMAS_SNOW_PPM") {
            let mut b = format!("P6 {} {} 255\n", c.w, c.h).into_bytes();
            for p in &c.px {
                b.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(out, b).unwrap();
        }
    }

    #[test]
    fn a_snowball_left_alone_melts_away() {
        let mut fx = Fx::new(1920.0, 1080.0, FxCfg::default(), 1.0);
        let a = win("a", 400.0, 300.0, 600.0, 400.0, 1);
        fx.set_windows(vec![a.clone()], &[], (0.0, 0.0), 0.0);
        fx.balls.push(Ball { win: "a".into(), dx: 100.0, r: 12.0, rolling: false, carried: false, idle: 0.0 });
        for _ in 0..60 * 60 {
            fx.update(1.0 / 60.0, 0.0);
        }
        assert!(fx.balls.is_empty());
    }

    fn win(id: &str, x: f32, y: f32, w: f32, h: f32, z: i64) -> Win {
        Win { id: id.into(), x, y, w, h, kind: 0, z }
    }

    /// Every pixel the pile drawing leaves on the canvas must lie inside a
    /// rect it recorded, or `clear_touched` never wipes it.
    #[test]
    fn pile_pixels_stay_inside_touched_rects() {
        for scale in [1.0f32, 1.25, 1.5] {
            let mut fx = Fx::new(400.0, 300.0, cfg(), scale);
            fx.flakes.clear();
            // last column narrower than the glint's x offset
            let w = win("a", 20.0, 100.0, 8.0 * scale * 5.0 + 3.0, 80.0, 1);
            fx.set_windows(vec![w], &[], (0.0, 0.0), 0.0);
            let step = fx.step;
            for (i, c) in fx.piles.get_mut("a").unwrap().cols.iter_mut().enumerate() {
                *c = if i % 2 == 0 { step } else { 2.0 * step };
            }
            let mut c = Canvas::new(400, 300);
            let mut g = Glyphs::new();
            for f in 0..2000 {
                c.clear_touched();
                fx.render(&mut c, &mut g, f as f32 * 0.05);
                for y in 60..110 {
                    for x in 0..90 {
                        if c.px[y * c.w + x] != 0 {
                            let (x, y) = (x as i32, y as i32);
                            assert!(
                                c.touched.iter().any(|&(rx, ry, rw, rh)| x >= rx && x < rx + rw && y >= ry && y < ry + rh),
                                "pixel {x},{y} outside the damage rects (scale {scale}, frame {f})"
                            );
                        }
                    }
                }
            }
        }
    }

    fn snowed(fx: &mut Fx, id: &str) {
        let step = fx.step;
        for c in fx.piles.get_mut(id).unwrap().cols.iter_mut() {
            *c = 3.0 * step;
        }
    }

    fn snow(fx: &Fx, id: &str) -> f32 {
        fx.piles.get(id).map(|p| p.cols.iter().sum()).unwrap_or(0.0)
    }

    #[test]
    fn piles_survive_a_desktop_switch() {
        let mut fx = Fx::new(1000.0, 800.0, cfg(), 1.0);
        let (a, b) = (win("a", 50.0, 200.0, 300.0, 300.0, 1), win("b", 500.0, 200.0, 300.0, 300.0, 2));
        fx.set_windows(vec![a.clone(), b.clone()], &[], (0.0, 0.0), 0.0);
        snowed(&mut fx, "a");
        let before = snow(&fx, "a");
        // switch away: "a" is on the other desktop, "b" is on all desktops
        fx.set_windows(vec![b.clone()], &["a".to_string()], (0.0, 0.0), 1.0);
        assert_eq!(snow(&fx, "a"), before, "pile kept while away");
        assert!(fx.parts.is_empty(), "nothing spilled on a desktop switch");
        // not drawn / landed on while away
        assert!(fx.land_at(100.0, 150.0, 250.0).is_none());
        // and back, even after the window moved meanwhile: no fling
        let moved = win("a", 80.0, 220.0, 300.0, 300.0, 1);
        fx.set_windows(vec![moved, b.clone()], &[], (0.0, 0.0), 120.0);
        assert_eq!(snow(&fx, "a"), before);
        assert!(fx.parts.is_empty());
        assert!(fx.piles["a"].away.is_none());
    }

    #[test]
    fn dormant_piles_expire_quietly() {
        let mut fx = Fx::new(1000.0, 800.0, cfg(), 1.0);
        fx.set_windows(vec![win("a", 50.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), 0.0);
        snowed(&mut fx, "a");
        let away = ["a".to_string()];
        fx.set_windows(vec![], &away, (0.0, 0.0), 1.0);
        fx.set_windows(vec![], &away, (0.0, 0.0), 1.0 + DORMANT_FOR - 1.0);
        assert!(fx.piles.contains_key("a"));
        fx.set_windows(vec![], &away, (0.0, 0.0), 2.0 + DORMANT_FOR);
        assert!(!fx.piles.contains_key("a"), "expired");
        assert!(fx.parts.is_empty());
        // a window closed while on another desktop doesn't spill here either
        fx.set_windows(vec![win("b", 50.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), 3.0 + DORMANT_FOR);
        snowed(&mut fx, "b");
        fx.set_windows(vec![], &["b".to_string()], (0.0, 0.0), 4.0 + DORMANT_FOR);
        fx.set_windows(vec![], &[], (0.0, 0.0), 5.0 + DORMANT_FOR);
        assert!(fx.piles.is_empty() && fx.parts.is_empty());
    }

    #[test]
    fn closing_a_visible_window_still_spills() {
        let mut fx = Fx::new(1000.0, 800.0, cfg(), 1.0);
        fx.set_windows(vec![win("a", 50.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), 0.0);
        snowed(&mut fx, "a");
        fx.set_windows(vec![], &[], (0.0, 0.0), 1.0);
        assert!(fx.piles.is_empty());
        assert!(!fx.parts.is_empty());
    }

    #[test]
    fn turning_snow_off_clears_settled_snow() {
        let mut fx = Fx::new(1000.0, 800.0, FxCfg { lights: true, ..cfg() }, 1.0);
        fx.set_windows(vec![win("a", 50.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), 0.0);
        snowed(&mut fx, "a");
        // a fling leaves clumps flying
        fx.set_windows(vec![win("a", 650.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), 0.05);
        assert!(!fx.parts.is_empty());
        fx.set_cfg(FxCfg { snow: false, lights: true, ..cfg() });
        assert!(fx.parts.is_empty());
        assert_eq!(snow(&fx, "a"), 0.0);
        // lights keep running; no snow comes back, none is drawn
        let mut c = Canvas::new(1000, 800);
        let mut g = Glyphs::new();
        for i in 0..200 {
            let t = 0.1 + i as f32 / 30.0;
            fx.set_windows(vec![win("a", 650.0, 200.0, 300.0, 300.0, 1)], &[], (0.0, 0.0), t);
            fx.update(1.0 / 30.0, t);
            c.clear_touched();
            fx.render(&mut c, &mut g, t);
        }
        assert_eq!(snow(&fx, "a"), 0.0);
        // and even a pile that somehow had snow isn't drawn
        fx.cfg.lights = false;
        snowed(&mut fx, "a");
        c.clear_touched();
        fx.render(&mut c, &mut g, 9.0);
        assert!(c.px.iter().all(|&p| p == 0));
    }

    #[test]
    fn floating_panel_inset_needs_a_matching_panel() {
        let mut fx = Fx::new(3000.0, 1300.0, cfg(), 1.0);
        let dock = |w: f32| Win { id: "p".into(), x: 100.0, y: 1200.0, w, h: 60.0, kind: 1, z: 5 };
        // a stale / unrelated floating panel must not inset this one
        fx.docks = vec![(1500.0, 1500.0, 30.0)];
        fx.set_windows(vec![dock(1200.0)], &[], (0.0, 0.0), 0.0);
        assert_eq!((fx.wins[0].y, fx.wins[0].h), (1200.0, 60.0));
        // unset lengths (0) match nothing
        fx.docks = vec![(0.0, 0.0, 44.0)];
        fx.set_windows(vec![dock(1200.0)], &[], (0.0, 0.0), 0.1);
        assert_eq!((fx.wins[0].y, fx.wins[0].h), (1200.0, 60.0));
        // exact length match: inset by half the margin
        fx.docks = vec![(1500.0, 1500.0, 30.0), (1200.0, 1200.0, 44.0)];
        fx.set_windows(vec![dock(1200.0)], &[], (0.0, 0.0), 0.2);
        assert_eq!((fx.wins[0].y, fx.wins[0].h), (1208.0, 44.0));
        // fit-to-content panel: length within its min..max
        fx.docks = vec![(903.0, 2922.0, 48.0)];
        fx.set_windows(vec![dock(1200.0)], &[], (0.0, 0.0), 0.3);
        assert_eq!((fx.wins[0].y, fx.wins[0].h), (1206.0, 48.0));
        // two ranges that disagree: can't tell, no inset
        fx.docks = vec![(903.0, 2922.0, 48.0), (800.0, 2000.0, 30.0)];
        fx.set_windows(vec![dock(1200.0)], &[], (0.0, 0.0), 0.4);
        assert_eq!((fx.wins[0].y, fx.wins[0].h), (1200.0, 60.0));
    }
}
