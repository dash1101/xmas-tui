//! Scene composition and layout. Everything is placed relative to the grid
//! size, tuned for a 32:9 ultrawide (~568x75 cells) but usable smaller.

pub mod critters;
pub mod events;
pub mod land;
pub mod lines;
pub mod player;
pub mod sky;
pub mod sleigh;
pub mod snow;
pub mod tree;
pub mod village;

use crate::audio::{AmbState, Sfx, SoundEvent};
use crate::color::{pal, Rgb};
use crate::config::SceneCfg;
use crate::frame::Frame;
use crate::rng::Rng;
use chrono::{Datelike, Local, Timelike};
use events::{Ctx, Director, CENTER};
use tree::{LightStyle, Present, Tree};
use village::{House, Lamp, Snowman, Village};

/// The scene keeps its own clock instead of using the app's `t`, for two
/// reasons:
///
/// * Precision. The app's `t` is an f32 of seconds since process start; the
///   wallpaper runs for days, and at 2^19 s (~6 days) f32 resolution is
///   1/16 s, so every animation would visibly step at ~16 Hz. The scene
///   accumulates the (precise) frame `dt` into an f64 and hands subsystems
///   an f32 that is periodically rebased to stay below `REBASE_AT`
///   (resolution ≤ 2^-9 s ≈ 2 ms).
/// * Rebuilds. A Scene is rebuilt on every scene-setting change, resize or
///   shuffle; its schedules (first sleigh, first event, first shooting
///   star, …) are offsets from its own creation, so time must start at 0
///   with it, not at the app's uptime (where they'd all be overdue at once).
///
/// The rebase subtracts `REBASE_BY` from the clock and from every stored
/// timestamp, so timers/envelopes carry on exactly. `REBASE_BY` is a whole
/// multiple of the scene's stepped cycles (18 s tree-light patterns, 90 s
/// window epochs, the 1.5/1.6/8 Hz flip-flops), so those don't jump either;
/// only free-running sin/noise phases skip once every 8 hours.
const REBASE_AT: f64 = 32768.0;
const REBASE_BY: f64 = 28800.0;

pub struct Scene {
    /// Visible width (cells).
    pub w: usize,
    pub h: usize,
    /// World width: the village is wider than the screen and scrolls.
    pub ww: usize,
    /// Camera: world column at the left edge, and where it's heading.
    cam: f32,
    cam_target: f32,
    cfg: SceneCfg,
    sky: sky::Sky,
    land: land::Land,
    village: Village,
    trees: Vec<Tree>,
    presents: Vec<Present>,
    snow: snow::Snow,
    pub sleigh: sleigh::Sleigh,
    director: Director,
    ctx: Ctx,
    /// Animals, people and berry bushes.
    critters: critters::Critters,
    /// You (when enabled), the direction keys held, and what E would do.
    player: Option<player::Player>,
    mv: (f32, f32),
    focus: Option<Focus>,
    /// Cursor in cells (None when the pointer left the desktop).
    cursor: Option<(f32, f32)>,
    cursor_prev: Option<(f32, f32)>,
    cursor_vel: (f32, f32),
    cursor_seen: f32,
    snowman_clicks: u32,
    typed: String,
    next_eve_sleigh: f32,
    rng: Rng,
    lines: lines::Lines,
    /// Seconds since this scene was created (sum of frame `dt`s).
    clock: f64,
    /// Total rebased away; scene time handed out is `clock - epoch`.
    epoch: f64,
}

/// What the player would interact with, and where its [E] prompt goes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Critter(u64, i32, i32),
    Bush(usize, i32, i32),
    Spot(i32, i32),
}

impl Focus {
    fn mark(&self) -> (i32, i32) {
        match *self {
            Focus::Critter(_, x, y) | Focus::Bush(_, x, y) | Focus::Spot(x, y) => (x, y),
        }
    }
}

impl Scene {
    pub fn new(w: usize, h: usize, seed: u32, cfg: &SceneCfg) -> Self {
        let (pw, ph) = (w, h * 2);
        let (wf, hf) = (w as f32, h as f32);
        // a wider world to wander through (small screens just get one screen)
        let ww = if w >= 150 && h >= 30 { (wf * 2.6) as usize } else { w };
        let wwf = ww as f32;
        let mut rng = Rng::new(seed as u64);
        let land = land::Land::new(ww, w, ph, seed);
        let ground_row = |x: f32| (land.ground_at(x) / 2.0).ceil() as i32;
        // everything was designed at 75 rows; sizes scale with height so the
        // scene keeps its proportions on any monitor, and the sides are
        // filled outward from the main tree with as much as fits
        let u = (hf / 75.0).max(0.45);
        let sz = |v: f32, min: i32| ((v * u).round() as i32).max(min);

        // ---- the main tree ----
        let mut trees = vec![];
        let mut presents = vec![];
        let main_cx = (wwf * 0.5) as i32;
        let main_base = (hf * 0.90).round() as i32;
        let main_h = (hf * 0.72).round() as i32;
        let main = Tree::new(main_cx, main_base, main_h, LightStyle::Multi, seed ^ 0x1);
        let spread = main.hw_base as i32 + 3;
        let mut pile = Present::pile(main_cx, main_base, spread, ((spread as f32) / 3.2) as usize + 3, true, u.min(1.3), &mut rng);
        presents.append(&mut pile);
        let inner = spread + 8; // keep the pile clear

        // ---- pack each side ----
        #[derive(Clone, Copy)]
        enum It {
            Tree(f32, f32, LightStyle, bool),
            House(i32, i32, bool, f32),
            Snowman,
            Fence(i32),
        }
        let tree_w = |fh: f32, gifts: bool| ((hf * fh) * 1.1) as i32 + if gifts { 10 } else { 4 };
        let width = |it: &It| match *it {
            It::Tree(fh, _, _, g) => tree_w(fh, g),
            It::House(w, _, church, _) => w + if church { 4 } else { 4 },
            It::Snowman => sz(16.0, 8),
            It::Fence(len) => len,
        };
        let house = |rng: &mut Rng| It::House(sz(rng.range(12.0, 19.0), 8), sz(rng.range(4.0, 7.0), 3), false, rng.range(0.80, 0.835));
        // each side is a long procession; packing stops at the world's edge
        let mut right = vec![
            house(&mut rng),
            It::Tree(0.30, 0.875, LightStyle::Multi, true),
            house(&mut rng),
            house(&mut rng),
            It::House(sz(18.0, 12), sz(7.0, 4), true, 0.815),
            house(&mut rng),
            house(&mut rng),
            house(&mut rng),
            It::Tree(0.22, 0.86, LightStyle::Warm, false),
            It::Fence(sz(46.0, 12)),
            house(&mut rng),
        ];
        let mut left = vec![
            It::Tree(0.18, 0.85, LightStyle::Icy, false),
            It::Snowman,
            It::Tree(0.30, 0.90, LightStyle::Warm, true),
            It::House(sz(24.0, 14), sz(7.0, 4), false, 0.86),
            It::Fence(sz(50.0, 12)),
            house(&mut rng),
            It::Tree(0.20, 0.87, LightStyle::Icy, false),
        ];
        let styles = [LightStyle::Multi, LightStyle::Warm, LightStyle::Icy];
        for side in [&mut right, &mut left] {
            for k in 0..40 {
                side.push(match (k + rng.int(0, 2) as usize) % 5 {
                    0 => It::Tree(rng.range(0.18, 0.30), rng.range(0.85, 0.9), *rng.pick(&styles), rng.chance(0.4)),
                    1 => It::Fence(sz(rng.range(24.0, 50.0), 10)),
                    _ => house(&mut rng),
                });
            }
        }
        let gap_min = sz(3.0, 2);
        let gap_max = sz(14.0, 4);
        // returns (item, left x) and the gaps (for lamps)
        let pack = |items: &[It], avail: i32| -> Vec<(It, i32)> {
            let mut taken = vec![];
            let mut used = 0;
            for it in items {
                let wdt = width(it);
                if used + wdt + gap_min * (taken.len() as i32 + 1) > avail {
                    break;
                }
                used += wdt;
                taken.push(*it);
            }
            let n = taken.len() as i32;
            if n == 0 {
                return vec![];
            }
            let gap = ((avail - used) / (n + 1)).clamp(gap_min, gap_max);
            let mut x = gap;
            taken
                .into_iter()
                .map(|it| {
                    let at = x;
                    x += width(&it) + gap;
                    (it, at)
                })
                .collect()
        };
        let margin = sz(2.0, 1);
        let avail_r = ww as i32 - (main_cx + inner) - margin;
        let avail_l = main_cx - inner - margin;
        // left side packs outward too: offsets are measured from the tree
        let mut placed: Vec<(It, i32)> = vec![];
        for (it, off) in pack(&right, avail_r) {
            placed.push((it, main_cx + inner + off));
        }
        for (it, off) in pack(&left, avail_l) {
            placed.push((it, main_cx - inner - off - width(&it)));
        }

        let mut village = Village::new(ww, ph, seed);
        let fence_row = (hf * 0.885).round() as i32;
        let mut town = vec![];
        let mut edges: Vec<(i32, i32)> = vec![];
        for (i, &(it, x)) in placed.iter().enumerate() {
            let wdt = width(&it);
            edges.push((x, x + wdt));
            match it {
                It::Tree(fh, fb, style, gifts) => {
                    let height = (hf * fh).round() as i32;
                    if height < 6 {
                        continue;
                    }
                    let cx = x + wdt / 2;
                    let base = ((hf * fb).round() as i32).max(ground_row(cx as f32) + 1);
                    let t = Tree::new(cx, base, height, style, seed ^ (0x100 + i as u32));
                    if gifts {
                        let sp = t.hw_base as i32 + 1;
                        presents.append(&mut Present::pile(cx, base, sp, 3, false, u.min(1.3), &mut rng));
                    }
                    trees.push(t);
                }
                It::House(hw, wall_h, church, fb) => {
                    let hx = x + 2;
                    let base = ((hf * fb).round() as i32).max(ground_row(hx as f32 + hw as f32 / 2.0) + 1);
                    if x > main_cx {
                        town.push(hx + hw / 2);
                    }
                    village.houses.push(House::new(hx, base, hw, wall_h, church, &mut rng));
                }
                It::Snowman => {
                    village.snowmen.push(Snowman { cx: x + wdt / 2, base: (hf * 0.93).round() as i32, scale: u.min(1.4) });
                }
                It::Fence(len) => village.fences.push((x, x + len - 1, fence_row)),
            }
        }
        // lamp posts in the roomier gaps, plus one either side of the tree
        edges.sort();
        let lamp_h = ((hf * 0.11).round() as i32).max(4);
        let lamp_base = (hf * 0.92).round() as i32;
        let mut lamp_xs = vec![main_cx - inner + 3, main_cx + inner - 3];
        for pair in edges.windows(2) {
            let (a, b) = (pair[0].1, pair[1].0);
            if b - a >= 5 && lamp_xs.iter().all(|&l| (l - (a + b) / 2).abs() > sz(40.0, 12)) {
                lamp_xs.push((a + b) / 2);
            }
        }
        for x in lamp_xs {
            if x > 1 && x < ww as i32 - 2 {
                village.lamps.push(Lamp { x, base: lamp_base, h: lamp_h, off_until: 0.0 });
            }
        }
        trees.push(main);
        let town_x = if town.is_empty() { wf * 0.72 } else { town.iter().sum::<i32>() as f32 / town.len() as f32 };
        let _ = town_x;
        // the sky (stars, moon, aurora) stays put on screen; the warm glow
        // of the town sits where the default view puts it
        let sky = sky::Sky::new(pw, ph, seed, cfg.aurora, wf * 0.72, &land.sky_ceiling(w), land.horizon);
        village.bake();

        let cam = ((ww - w) / 2) as f32;
        let c0 = cam as usize;
        let snow = snow::Snow::new(pw, ph, cfg.snow, cfg.wind, seed, &land.ground[c0..c0 + w], &land.hills[c0..c0 + w]);
        let sleigh = sleigh::Sleigh::new(w, sky.moon.1 / 2.0, cfg.sleigh_minutes, seed);
        // facts for the event director
        let (wi, hi) = (ww as i32, h as i32);
        let ground_row: Vec<i32> = (0..ww).map(|x| (land.ground[x] / 2.0).ceil() as i32).collect();
        let track_row = land.track_row;
        let mut occluded = vec![false; ww * h];
        // only cells the house art actually covers, not its bounds rectangle
        // (whose empty corners above the roofs hid the train over open hills)
        for house in &village.houses {
            for &(x, y) in house.cells() {
                if x >= 0 && x < wi && y >= 0 && y < hi {
                    occluded[(y * wi + x) as usize] = true;
                }
            }
        }
        let church = village.houses.iter().find_map(|h| h.steeple());
        let sky_rows = (land.horizon / 2.0) as i32;
        let ctx = Ctx { w: wi, h: hi, ground_row, track_row, occluded, church, sky_rows, cam, vw: w as i32 };
        let mut rng = Rng::new(seed as u64 ^ 0xc11c);
        // relative to creation, so a rebuild on Christmas Eve doesn't launch
        // Santa on the very next frame
        let next_eve_sleigh = rng.range(20.0, 45.0);
        // berry bushes go on the open snow, clear of trees, presents and the snowman
        let mut avoid: Vec<(i32, i32)> = trees.iter().map(|t| (t.cx - t.hw_base as i32 - 4, t.cx + t.hw_base as i32 + 4)).collect();
        avoid.extend(village.snowmen.iter().map(|s| (s.cx - 9, s.cx + 9)));
        avoid.extend(village.lamps.iter().map(|l| (l.x - 1, l.x + 2)));
        let critters = critters::Critters::new(&ctx, seed, cfg.critters, cfg.bushes, &avoid);
        // you start out a little left of the big tree, in the snow
        let player = cfg.player.then(|| {
            let px = cam + wf * 0.40;
            let lo = (ctx.ground_row[(px as usize).min(ww - 1)] + 4).min(hi - 2) as f32;
            player::Player::new(px, ((lo + hf - 1.0) / 2.0).round())
        });
        Scene {
            w,
            h,
            ww,
            cam,
            cam_target: cam,
            player,
            mv: (0.0, 0.0),
            focus: None,
            cfg: cfg.clone(),
            sky,
            land,
            village,
            trees,
            presents,
            snow,
            sleigh,
            director: Director::new(seed, cfg.events),
            critters,
            ctx,
            cursor: None,
            cursor_prev: None,
            cursor_vel: (0.0, 0.0),
            cursor_seen: -10.0,
            snowman_clicks: 0,
            typed: String::new(),
            next_eve_sleigh,
            rng,
            lines: lines::Lines::new(seed),
            clock: 0.0,
            epoch: 0.0,
        }
    }

    /// Current scene time (see `REBASE_AT`). All `t` arguments the app passes
    /// in are ignored in favour of this.
    fn now(&self) -> f32 {
        (self.clock - self.epoch) as f32
    }

    /// Advance the clock by `dt` and rebase it if it's grown too large.
    fn tick(&mut self, dt: f32) {
        self.clock += dt.max(0.0) as f64;
        let local = self.clock - self.epoch;
        if local >= REBASE_AT {
            let shift = ((local - REBASE_AT) / REBASE_BY).floor() * REBASE_BY + REBASE_BY;
            self.epoch += shift;
            let s = shift as f32;
            self.sky.rebase(s);
            self.sleigh.rebase(s);
            self.director.rebase(s);
            self.critters.rebase(s);
            self.village.rebase(s);
            for tr in &mut self.trees {
                tr.rebase(s);
            }
            for p in &mut self.presents {
                p.rebase(s);
            }
            self.cursor_seen -= s;
            self.next_eve_sleigh -= s;
            if let Some(p) = &mut self.player {
                p.rebase(s);
            }
        }
    }

    /// For previews: put some of everything on screen at once.
    pub fn demo(&mut self, _app_t: f32) {
        use events::Kind;
        let t = self.now();
        self.director.spawn_near(Kind::Train, &self.ctx);
        for (k, frac) in [(Kind::Deer, 0.24), (Kind::Fox, 0.62), (Kind::Rabbit, 0.44), (Kind::Walker, 0.57), (Kind::Kid, 0.35), (Kind::Kid, 0.39)] {
            self.critters.spawn_demo(k, self.cam + self.w as f32 * frac, t);
        }
        self.critters.spawn_demo_group(self.cam + self.w as f32 * 0.78, t);
        if let Some(p) = &mut self.player {
            p.active_at = t;
        }
        self.director.light_show(t, 30.0);
        let (sx, sy) = self.trees.last().map(|t| t.star()).unwrap_or((0, 0));
        self.director.burst(sx as f32, sy as f32, 50, &[pal::GOLD, Rgb::WHITE], 11.0, 2.5);
        if let Some(s) = self.village.snowmen.first() {
            let (hx, hy) = village::snowman_head(s);
            self.director.bubble(hx, hy, "brr… chilly out here!", t + 2.0);
        }
        self.cursor = Some((self.w as f32 * 0.42, self.h as f32 * 0.93));
        self.cursor_prev = Some((self.w as f32 * 0.40, self.h as f32 * 0.93));
    }

    /// Drain this frame's queued sounds, with x converted to pan 0..1.
    pub fn take_sounds(&mut self) -> Vec<SoundEvent> {
        let w = self.w.max(1) as f32;
        let cam = self.cam;
        let mut v = std::mem::take(&mut self.director.sounds);
        for e in &mut v {
            if e.pan.is_finite() {
                // world x → position in view; things further away are quieter
                let sx = e.pan - cam;
                let out = (-sx).max(sx - w).max(0.0);
                e.gain *= (1.0 - out / 90.0).clamp(0.0, 1.0);
                e.pan = (sx / w).clamp(0.0, 1.0);
            } else {
                e.pan = 0.5;
            }
        }
        v.retain(|e| e.gain > 0.01);
        v
    }

    /// What the ambience should sound like right now.
    pub fn audio_state(&self, _app_t: f32) -> AmbState {
        let t = self.now();
        let w = self.w.max(1) as f32;
        // audible a little beyond the screen edges, fading out
        let edge = |x: f32| (1.0 - ((x / w - 0.5).abs() - 0.5).max(0.0) * w / 40.0).clamp(0.0, 1.0);
        let bliz = self.director.blizzard.value(t);
        AmbState {
            wind: 0.3 * self.cfg.wind.min(3.0) + self.director.gust_wind(t).abs() / 10.0,
            snow: if self.cfg.snow > 0.0 { self.cfg.snow.min(3.0) * (1.0 + bliz) } else { 0.0 },
            sleigh: self.sleigh.audio_x().map(|x| ((x / w).clamp(0.0, 1.0), edge(x))),
            train: self.director.train_x().map(|x| x - self.cam).map(|x| ((x / w).clamp(0.0, 1.0), edge(x))),
            aurora: if self.cfg.aurora { self.director.flare.value(t) } else { 0.0 },
        }
    }

    pub fn set_cursor(&mut self, pos: Option<(i32, i32)>) {
        self.cursor = pos.map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5));
    }

    fn on_ground(&self, x: i32, y: i32) -> bool {
        y >= 0 && (y as f32 * 2.0 + 1.0) >= self.land.ground_at(x as f32)
    }

    /// `sx`: screen column (the mountains scroll with parallax).
    fn in_sky(&self, sx: i32, y: i32) -> bool {
        let col = ((sx as f32 + self.cam * land::PARALLAX).max(0.0) as usize).min(self.land.mountains.len() - 1);
        (y as f32 * 2.0 + 1.0) < self.land.mountains[col]
    }

    fn cam_i(&self) -> i32 {
        self.cam.round() as i32
    }

    /// The direction keys currently held: x and y each -1, 0 or 1.
    pub fn set_move(&mut self, mx: f32, my: f32) {
        self.mv = (mx, my);
    }

    #[cfg(test)]
    pub fn player_x(&self) -> Option<f32> {
        self.player.as_ref().map(|p| p.x)
    }

    /// Previews: you stand about idly (no [E] prompts).
    pub fn quiet_player(&mut self) {
        if let Some(p) = &mut self.player {
            p.active_at = f32::NEG_INFINITY;
        }
    }

    pub fn has_player(&self) -> bool {
        self.player.is_some()
    }

    /// The nearest thing the player could interact with: someone to talk
    /// to or pet, a berry bush, or something in the village in front of them.
    fn find_focus(&self) -> Option<Focus> {
        let p = self.player.as_ref()?;
        let (px, py) = (p.centre(), p.y);
        let t = self.now();
        if let Some(id) = self.critters.nearest(px, py, 7.0) {
            let (bx, by, bw, _) = self.critters.bbox_of(id, t)?;
            return Some(Focus::Critter(id, bx + bw / 2, by - 1));
        }
        if let Some(b) = self.critters.bush_near(px, py) {
            let (bx, base, bw) = self.critters.bush_rect(b);
            return Some(Focus::Bush(b, bx + bw / 2, base - 3));
        }
        // in front first, then right where you stand, then just behind
        let (x0, y0) = (px.round() as i32, py.round() as i32);
        let d = p.dir as i32;
        for k in [1, 2, 0, 3, 4, -1, 5, 6, -2] {
            let x = x0 + d * k;
            for y in (y0 - 12..=y0).rev() {
                if self.pokeable(x, y) {
                    return Some(Focus::Spot(x, y));
                }
            }
        }
        None
    }

    fn pokeable(&self, x: i32, y: i32) -> bool {
        self.presents.iter().any(|p| p.contains(x, y))
            || self.trees.iter().any(|t| t.contains(x, y))
            || self.village.snowmen.iter().any(|s| {
                let (x0, y0, x1, y1) = village::snowman_bounds(s);
                x >= x0 && x <= x1 && y >= y0 && y <= y1
            })
            || self.village.lamps.iter().any(|l| l.x == x && y > l.base - l.h && y <= l.base)
            || self.village.houses.iter().any(|h| h.contains(x, y))
    }

    /// E / Space / Enter: say hello, pet or feed, pick berries, or poke
    /// whatever's in front of you.
    pub fn interact(&mut self) {
        let t = self.now();
        let Some(p) = self.player.as_mut() else { return };
        p.active_at = t;
        let (px, py) = (p.centre(), p.y);
        match self.focus {
            Some(Focus::Critter(id, ..)) => {
                let face = self.critters.centre_of(id).map(|cx| (cx - px).signum()).unwrap_or(0.0);
                if face != 0.0 {
                    p.dir = face;
                }
                self.critters.meet(id, px, py, &mut p.berries, t, &mut self.director, &mut self.lines);
            }
            Some(Focus::Bush(b, ..)) => {
                let n = self.critters.pick_berries(b, t, &mut self.director);
                if n > 0 {
                    p.berries = (p.berries + n).min(9);
                    let msg = format!("{} ({} ●)", self.lines.pick(), p.berries);
                    self.director.say(px.round() as i32, py as i32 - 4, &msg, t, 2.6, 6);
                } else {
                    self.director.say(px.round() as i32, py as i32 - 4, "no ripe berries yet…", t, 2.4, 6);
                }
            }
            Some(Focus::Spot(x, y)) => {
                self.poke(x, y);
            }
            None => {
                // nothing about: a happy little hop in the snow
                self.director.snow_puff(px, py);
            }
        }
    }

    /// F: ask whoever's nearby to walk with you (animals want a berry), or
    /// say goodbye to everyone following you.
    pub fn invite(&mut self) {
        let t = self.now();
        let Some(p) = self.player.as_mut() else { return };
        p.active_at = t;
        if let Some(Focus::Critter(id, ..)) = self.focus {
            if !self.critters.following(id) {
                self.critters.invite(id, &mut p.berries, t, &mut self.director, &mut self.lines);
                return;
            }
        }
        self.critters.dismiss(t, &mut self.director, &mut self.lines);
    }

    /// Move the view instantly by `dx` cells (previews).
    pub fn jump(&mut self, dx: f32) {
        self.pan(dx);
        self.cam = self.cam_target;
        self.ctx.cam = self.cam;
    }

    /// Scroll the view (mouse wheel), in cells.
    pub fn pan(&mut self, dx: f32) {
        let max = (self.ww - self.w) as f32;
        self.cam_target = (self.cam_target + dx).clamp(0.0, max);
    }

    fn on_moon(&self, x: i32, y: i32) -> bool {
        let (mx, my, r) = self.sky.moon;
        let (dx, dy) = (x as f32 + 0.5 - mx, y as f32 * 2.0 + 1.0 - my);
        dx * dx + dy * dy <= (r + 2.0) * (r + 2.0)
    }

    /// Someone under the cursor who can be picked up?
    pub fn grabbable(&self, x: i32, y: i32) -> bool {
        self.critters.grabbable(x + self.cam_i(), y, self.now())
    }

    /// Start carrying whoever is at (x, y). False if nobody's there.
    pub fn grab(&mut self, x: i32, y: i32) -> bool {
        let t = self.now();
        let x = x + self.cam_i();
        self.critters.grab(x, y, t, &mut self.director)
    }

    pub fn drag(&mut self, x: i32, y: i32) {
        self.set_cursor(Some((x, y)));
        self.critters.drag(x + self.cam_i(), y);
    }

    /// Let go (a click without dragging becomes a pat or a hello).
    pub fn release(&mut self) {
        let t = self.now();
        self.critters.release(t, &mut self.director, &mut self.lines);
    }

    pub fn holding(&self) -> bool {
        self.critters.holding()
    }

    /// Is there something under this cell that reacts to a click?
    pub fn clickable(&self, x: i32, y: i32) -> bool {
        let (sx, x) = (x, x + self.cam_i());
        if self.critters.bush_at(x, y).is_some() {
            return true;
        }
        self.presents.iter().any(|p| p.contains(x, y))
            || self.trees.iter().any(|t| t.contains(x, y))
            || self.village.snowmen.iter().any(|s| {
                let (x0, y0, x1, y1) = village::snowman_bounds(s);
                x >= x0 && x <= x1 && y >= y0 && y <= y1
            })
            || self.village.lamps.iter().any(|l| l.x == x && y > l.base - l.h && y <= l.base)
            || self.village.houses.iter().any(|h| h.contains(x, y))
            || self.on_moon(sx, y)
    }

    /// Something in the village at world (x, y) — a present, the snowman, a
    /// tree, a lamp, a house — reacts as if clicked. Returns whether
    /// anything was there.
    fn poke(&mut self, x: i32, y: i32) -> bool {
        let t = self.now();
        let (xf, yf) = (x as f32, y as f32);
        let d = &mut self.director;
        if let Some(p) = self.presents.iter_mut().rev().find(|p| p.contains(x, y)) {
            p.jiggle_until = t + 0.7;
            let (cx, cy) = p.top_center();
            d.sound(Sfx::Present, cx as f32, 1.0);
            d.burst(cx as f32, cy as f32, 16, &[pal::GOLD, Rgb::WHITE, pal::RED], 6.0, 6.0);
            if self.rng.chance(0.6) {
                let msg = self.lines.gift();
                d.bubble(cx, cy - 1, &msg, t);
            }
            return true;
        }
        if let Some(s) = self.village.snowmen.iter().find(|s| {
            let (x0, y0, x1, y1) = village::snowman_bounds(s);
            x >= x0 && x <= x1 && y >= y0 && y <= y1
        }) {
            self.snowman_clicks += 1;
            let (hx, hy) = village::snowman_head(s);
            if self.snowman_clicks % 7 == 0 {
                d.bubble(hx, hy, "✦ you found my secret! let's light it up ✦", t);
                d.sound(Sfx::Fanfare, hx as f32, 1.0);
                d.light_show(t, 16.0);
                d.burst(hx as f32, hy as f32, 30, &[pal::GOLD, pal::RED, pal::GREEN, Rgb::WHITE], 8.0, 4.0);
            } else {
                let line = self.lines.snowman();
                d.bubble(hx, hy, &line, t);
                d.sound(Sfx::Snowman, hx as f32, 1.0);
                d.snow_puff(hx as f32, hy as f32 + 4.0);
            }
            return true;
        }
        if let Some(tr) = self.trees.iter_mut().find(|tr| tr.contains(x, y)) {
            tr.boost_until = t + 3.0;
            let (sx, sy) = tr.star();
            let on_star = (y - sy).abs() <= 1 && (x - sx).abs() <= 2;
            if on_star {
                d.sound(Sfx::Starburst, sx as f32, 1.0);
                d.burst(sx as f32, sy as f32, 50, &[pal::GOLD, Rgb::hex(0xfff2c0), Rgb::WHITE], 11.0, 2.5);
            } else {
                d.sound(Sfx::Sparkle, xf, 1.0);
                d.burst(xf, yf, 14, &pal::BULBS, 5.0, 5.0);
            }
            return true;
        }
        if let Some(l) = self.village.lamps.iter_mut().find(|l| l.x == x && y > l.base - l.h && y <= l.base) {
            l.off_until = t + 2.5;
            d.sound(Sfx::Lamp, x as f32, 1.0);
            return true;
        }
        // the real silhouette, so empty sky beside the steeple or above a
        // roof is still sky (shooting star / wish) rather than a house
        if let Some(hs) = self.village.houses.iter_mut().find(|h| h.contains(x, y)) {
            if let Some((cx, cy)) = hs.steeple() {
                d.notes(cx as f32, cy as f32, 6);
                d.sound(Sfx::ChurchPeal, cx as f32, 1.0);
            } else {
                let (bx, _, bw, _) = hs.bounds();
                d.sound(Sfx::Click, (bx + bw / 2) as f32, 1.0);
                if t < hs.lights_off_until {
                    hs.lights_off_until = 0.0;
                } else {
                    hs.lights_off_until = t + 1e7;
                }
                if self.rng.chance(0.5) {
                    let (bx, by, bw, _) = hs.bounds();
                    let msg = self.lines.house();
                    d.bubble(bx + bw / 2, by + 1, &msg, t);
                }
            }
            return true;
        }
        false
    }

    /// Left click on the scene: everything reacts a little differently.
    pub fn click(&mut self, x: i32, y: i32, _app_t: f32) {
        let (sx, x) = (x, x + self.cam_i());
        if let Some(b) = self.critters.bush_at(x, y) {
            let t = self.now();
            self.critters.shake_bush(b, t, &mut self.director);
            return;
        }
        let t = self.now();
        let (xf, yf) = (x as f32, y as f32);
        let (moon_hit, sky_hit, ground_hit) = (self.on_moon(sx, y), self.in_sky(sx, y), self.on_ground(x, y));
        if self.poke(x, y) {
            return;
        }
        let d = &mut self.director;
        if moon_hit {
            self.sleigh.trigger(t);
            let (mx, my, r) = self.sky.moon;
            let mx = mx + (x - sx) as f32; // the moon is on screen; bubbles live in the world
            d.sound(Sfx::Magic, mx, 1.0);
            let msg = self.lines.moon();
            d.bubble(mx as i32, ((my - r) / 2.0) as i32, &msg, t);
            return;
        }
        if sky_hit {
            d.shooting_star(sx as f32, yf * 2.0);
            if self.rng.chance(0.3) {
                let msg = self.lines.wish();
                d.bubble(x, y, &msg, t);
            }
            return;
        }
        if ground_hit {
            d.snow_puff(xf, yf);
        }
    }

    /// Keyboard easter eggs (only reachable when the wallpaper has focus).
    pub fn typed(&mut self, ch: char, _app_t: f32) {
        let t = self.now();
        self.typed.push(ch.to_ascii_lowercase());
        if self.typed.len() > 24 {
            self.typed.remove(0);
        }
        let s = self.typed.clone();
        let (cx, cy) = (self.cam_i() + (self.w / 2) as i32, (self.h / 3) as i32);
        let d = &mut self.director;
        if s.ends_with("uuddlrlrba") {
            d.light_show(t, 20.0);
            d.blizzard.fire(t, 12.0);
            self.sleigh.trigger(t);
            d.bubble(cx, cy, "↑↑↓↓←→←→BA — cheat code accepted ✦", t);
        } else if s.ends_with("hohoho") {
            self.sleigh.trigger(t);
            d.bubble(cx, cy, "Santa heard you!", t);
        } else if s.ends_with("snow") {
            d.blizzard.fire(t, 20.0);
        } else if s.ends_with("joy") {
            for _ in 0..6 {
                d.firework(&self.ctx);
            }
        } else if s.ends_with("aurora") {
            d.flare.fire(t, 40.0);
        } else if s.ends_with("lights") {
            d.light_show(t, 20.0);
        } else {
            return;
        }
        // every easter egg gets a little fanfare
        d.sound(Sfx::Fanfare, CENTER, 1.0);
        self.typed.clear();
    }


    pub fn update(&mut self, dt: f32, _app_t: f32) {
        self.tick(dt);
        let t = self.now();
        // ease the camera toward where it's been asked to look
        let max = (self.ww - self.w) as f32;
        self.cam_target = self.cam_target.clamp(0.0, max);
        let d = self.cam_target - self.cam;
        self.cam = if d.abs() < 0.05 { self.cam_target } else { self.cam + d * (1.0 - (-dt * 6.0).exp()) };
        self.ctx.cam = self.cam;
        self.ctx.vw = self.w as i32;
        self.sky.update(dt, t, self.w, self.h * 2);
        // cursor velocity (cells/s), smoothed
        match (self.cursor, self.cursor_prev) {
            (Some(c), Some(p)) if dt > 0.0 => {
                let v = ((c.0 - p.0) / dt, (c.1 - p.1) / dt);
                self.cursor_vel = (self.cursor_vel.0 * 0.5 + v.0 * 0.5, self.cursor_vel.1 * 0.5 + v.1 * 0.5);
                if v.0 != 0.0 || v.1 != 0.0 {
                    self.cursor_seen = t;
                }
            }
            _ => self.cursor_vel = (0.0, 0.0),
        }
        self.cursor_prev = self.cursor;
        if let Some((cx, cy)) = self.cursor {
            let wx = cx + self.cam;
            let ground = self.on_ground(wx as i32, cy as i32);
            self.director.cursor(wx, cy, self.cursor_vel.0, self.cursor_vel.1, ground, dt);
        }
        if let Some(p) = &mut self.player {
            let (g, h) = (&self.ctx.ground_row, self.h as f32);
            let lo = |x: f32| (g[(x.max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32;
            p.update(dt, t, self.mv, lo, self.ww as f32, h, &mut self.director);
            // the camera keeps you comfortably in view while you walk
            if p.moving {
                let (w, c) = (self.w as f32, p.centre());
                let (l, r) = (w * 0.3, w * 0.7);
                if c - self.cam_target < l {
                    self.cam_target = c - l;
                } else if c - self.cam_target > r {
                    self.cam_target = c - r;
                }
                self.cam_target = self.cam_target.clamp(0.0, (self.ww - self.w) as f32);
            }
        }
        self.critters.set_player(self.player.as_ref().map(|p| (p.x, p.y, p.dir, p.moving)));
        self.director.update(dt, t, &self.ctx);
        for k in std::mem::take(&mut self.director.spawn_requests) {
            self.critters.request(k, t);
        }
        // the cursor matters to the animals for a while after it last moved
        let cam = self.cam;
        let cur = self.cursor.filter(|_| t - self.cursor_seen < 15.0).map(|(x, y)| (x + cam, y, self.cursor_vel.0, self.cursor_vel.1));
        let snowman = self.village.snowmen.first().map(village::snowman_head);
        self.critters.update(dt, t, &self.ctx, &mut self.director, &mut self.lines, cur, snowman);
        self.focus = self.find_focus();
        // passers-by sometimes say hello
        if self.rng.chance(dt * 0.04) {
            let walkers = self.critters.people();
            if !walkers.is_empty() {
                let (wx, wy) = walkers[self.rng.int(0, walkers.len() as i32) as usize];
                let msg = self.lines.walker();
                self.director.bubble(wx, wy, &msg, t);
            }
        }
        if let Some(i) = self.director.house_toggle.take() {
            let n = self.village.houses.len();
            if n > 0 {
                let hs = &mut self.village.houses[i % n];
                if !hs.church {
                    hs.lights_off_until = if t < hs.lights_off_until { 0.0 } else { t + self.rng.range(20.0, 60.0) };
                    let (bx, _, bw, _) = hs.bounds();
                    self.director.sound(Sfx::Click, (bx + bw / 2) as f32, 0.5);
                }
            }
        }
        // Christmas Eve: Santa is busy tonight
        let now = Local::now();
        if now.month() == 12 && now.day() == 24 && now.hour() >= 17 && t > self.next_eve_sleigh {
            self.sleigh.trigger(t);
            self.next_eve_sleigh = t + self.rng.range(60.0, 120.0);
        }
        if self.cfg.snow > 0.0 {
            let fresh = t - self.cursor_seen < 2.5;
            let cur = self.cursor.filter(|_| fresh).map(|(x, y)| (x, y * 2.0, self.cursor_vel.0, self.cursor_vel.1 * 2.0));
            self.snow.update(dt, t, self.director.gust_wind(t), cur);
        }
        self.village.update(dt, self.snow.wind);
        self.sleigh.update(dt, t);
    }

    pub fn draw(&self, f: &mut Frame, _app_t: f32) {
        let t = self.now();
        let glow = self.cfg.glow;
        let cam = self.cam_i();
        // pixel layer (sky things stay on screen, the world scrolls)
        f.ox = 0;
        self.sky.draw_px(f, t, self.director.flare.value(t));
        self.director.draw_px(f);
        self.land.draw_px(f, self.cam);
        f.ox = cam;
        self.village.draw_px(f, self.cam);
        // glyph layer, back to front
        f.ox = 0;
        self.sky.draw_glyphs(f, t);
        self.sleigh.draw(f, t, glow);
        f.ox = cam;
        self.village.draw_glyphs(f, t, glow);
        self.land.draw_glints(f, t);
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            self.snow.draw_far(f);
        }
        f.ox = cam;
        self.director.draw_back(f, &self.ctx, t);
        self.critters.draw_back(f, t);
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            self.snow.draw_mid(f);
        }
        f.ox = cam;
        let show = self.director.show.value(t);
        for tr in &self.trees {
            tr.draw(f, t, glow, show);
        }
        for p in &self.presents {
            p.draw(f, t);
        }
        self.critters.draw(f, t, glow);
        if let Some(p) = &self.player {
            p.draw(f, t, glow);
        }
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            self.snow.draw_near(f, t);
        }
        f.ox = cam;
        self.director.draw_front(f, t, glow);
        self.critters.draw_top(f, t, glow);
        // what E would do, while you're out walking
        if let (Some(p), Some(fc)) = (&self.player, self.focus) {
            if t - p.active_at < 20.0 {
                // over the person / bush; for village things, over your head
                let (x, y) = match fc {
                    Focus::Spot(..) => (p.centre().round() as i32, p.y.round() as i32 - 5),
                    _ => fc.mark(),
                };
                player::Player::draw_prompt(f, x, y, 'E', t);
            }
        }
        f.ox = 0;
        // a soft lantern-like glow that follows the pointer while it moves
        if let Some((cx, cy)) = self.cursor {
            let k = (1.0 - (t - self.cursor_seen) / 2.5).clamp(0.0, 1.0);
            if k > 0.0 {
                f.light_cell(cx as i32, cy as i32, Rgb::hex(0xffd9a0) * (2.2 * k * glow));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 30.0;

    fn scene() -> Scene {
        Scene::new(160, 40, 7, &SceneCfg::default())
    }

    /// A week of uptime: the app's f32 `t` is down to 1/16 s steps there, but
    /// the time the scene animates with must still advance smoothly by `dt`.
    #[test]
    fn scene_time_stays_precise_after_days() {
        let week = 7.0 * 86400.0f64;
        // what animations used to get: the app's absolute f32 time
        let app_t = |i: usize| (week + i as f64 * DT as f64) as f32;
        let steps: Vec<f32> = (1..60).map(|i| app_t(i) - app_t(i - 1)).collect();
        assert!(steps.iter().all(|&d| d == 0.0 || d == 0.0625), "f32 app time is quantized: {steps:?}");

        let mut s = scene();
        s.clock = week; // as if this scene had been up for a week
        let mut prev: Option<f32> = None;
        for i in 0..300 {
            s.update(DT, app_t(i));
            let t = s.now();
            assert!((t as f64) < REBASE_AT, "scene time rebased below {REBASE_AT}: {t}");
            // exact to within half an f32 ulp at < 2^15 s (~1 ms)...
            let exact = s.clock - s.epoch;
            assert!((t as f64 - exact).abs() < 1e-3, "frame {i}: {t} vs {exact}");
            // ...so every frame advances by dt (± that rounding), never 0 or 1/16 s
            if let Some(p) = prev {
                let d = t - p;
                assert!((d - DT).abs() < 2.5e-3, "frame {i}: scene time stepped by {d}, not {DT}");
            }
            prev = Some(t);
        }
    }

    /// Crossing a rebase keeps timers and envelopes continuous.
    #[test]
    fn rebase_keeps_timers_continuous() {
        let mut s = scene();
        s.clock = REBASE_AT - 1.0;
        s.update(DT, 0.0);
        let t0 = s.now();
        s.director.flare.fire(t0, 40.0);
        s.trees[0].boost_until = t0 + 3.0;
        let left = |s: &Scene| s.trees[0].boost_until - s.now();
        let mut last = s.director.flare.value(t0);
        for i in 1..=90 {
            s.update(DT, 0.0);
            let t = s.now();
            let v = s.director.flare.value(t);
            assert!((v - last).abs() < 0.05, "flare jumped {last} -> {v} at frame {i}");
            last = v;
            let want = 3.0 - i as f32 * DT;
            assert!((left(&s) - want).abs() < 1e-2, "boost timer {} != {want}", left(&s));
        }
        assert!(s.epoch > 0.0, "the clock should have been rebased");
    }

    /// A scene rebuilt long after app start (settings change) must not find
    /// all its "first X after N seconds" timers already overdue.
    #[test]
    fn rebuilt_scene_does_not_fire_at_once() {
        let mut s = scene();
        for i in 0..30 {
            s.update(DT, 3600.0 + i as f32 * DT);
        }
        assert!(s.sleigh.audio_x().is_none(), "sleigh launched straight after a rebuild");
    }

    /// Clicking the moon summons Santa even with fly-bys set to "never".
    #[test]
    fn moon_click_summons_sleigh_when_never() {
        let cfg = SceneCfg { sleigh_minutes: 0.0, ..SceneCfg::default() };
        let mut s = Scene::new(160, 40, 7, &cfg);
        for _ in 0..300 {
            s.update(DT, 0.0);
        }
        assert!(s.sleigh.audio_x().is_none(), "no scheduled fly-bys when set to never");
        let (mx, my, _) = s.sky.moon;
        s.click(mx as i32, (my / 2.0) as i32, 0.0);
        s.update(DT, 0.0);
        assert!(s.sleigh.audio_x().is_some(), "moon click didn't launch the sleigh");
    }

    /// Houses are hit only on their art, not the empty corners of bounds().
    #[test]
    fn house_hit_test_uses_silhouette() {
        let s = Scene::new(568, 75, 7, &SceneCfg::default());
        let church = s.village.houses.iter().find(|h| h.church).expect("a church");
        let (x0, y0, bw, _) = church.bounds();
        // top corners of the bounds rectangle are empty sky beside the steeple
        assert!(!church.contains(x0, y0) && !church.contains(x0 + bw - 1, y0 + 2));
        let (cx, cy) = church.steeple().unwrap();
        assert!(church.contains(cx, cy), "the cross itself is clickable");
        assert!(church.contains(cx, church.base), "the walls are clickable");
    }

    /// The empty part of a present's bow row doesn't steal tree clicks.
    #[test]
    fn present_hit_box_excludes_bow_row() {
        let mut rng = Rng::new(1);
        let p = Present::new(10, 20, 6, 3, &mut rng);
        assert!(p.contains(13, 17), "the bow (centre of the row above the box)");
        assert!(!p.contains(10, 17) && !p.contains(15, 17), "rest of the bow row");
        assert!(p.contains(10, 18) && p.contains(15, 20), "the box");
    }

    fn walker_scene() -> Scene {
        let mut cfg = SceneCfg::default();
        cfg.events = 0.0;
        cfg.critters = 0.0; // nobody wanders in uninvited
        Scene::new(160, 40, 7, &cfg)
    }

    fn run(s: &mut Scene, secs: f32) {
        for _ in 0..(secs / DT) as usize {
            s.update(DT, 0.0);
        }
    }

    #[test]
    fn walking_right_moves_you_and_the_camera_follows() {
        let mut s = walker_scene();
        assert!(s.ww > s.w, "the world should be wider than the screen");
        let (x0, cam0) = (s.player.as_ref().unwrap().x, s.cam);
        s.set_move(1.0, 0.0);
        run(&mut s, 6.0);
        s.set_move(0.0, 0.0);
        run(&mut s, 1.0);
        let p = s.player.as_ref().unwrap();
        assert!(p.x > x0 + 50.0, "walked {} → {}", x0, p.x);
        assert!(s.cam > cam0 + 10.0, "camera {} → {}", cam0, s.cam);
        // still comfortably on screen
        let sx = p.centre() - s.cam;
        assert!(sx > 0.0 && sx < s.w as f32, "player at screen x {sx}");
        // and never off the end of the world
        s.set_move(1.0, 0.0);
        run(&mut s, 60.0);
        let p = s.player.as_ref().unwrap();
        assert!(p.x <= s.ww as f32 - 4.0 && s.cam <= (s.ww - s.w) as f32);
    }

    #[test]
    fn invited_people_follow_you_and_chat_when_you_stop() {
        let mut s = walker_scene();
        let t = s.now();
        let px = s.player.as_ref().unwrap().centre();
        let py = s.player.as_ref().unwrap().y;
        // two people standing just in front of you
        let mut ids = vec![];
        for dx in [3.0, 7.0] {
            s.critters.spawn_demo(events::Kind::Walker, px + dx, t);
            ids.push(s.critters.last_id());
        }
        for &id in &ids {
            let _ = py;
            // face them and ask each one along
            s.update(DT, 0.0);
            s.focus = s.critters.bbox_of(id, s.now()).map(|(x, y, w, _)| Focus::Critter(id, x + w / 2, y - 1));
            s.invite();
        }
        assert_eq!(s.critters.followers(), 2);
        // walk left a good way: they come along
        s.set_move(-1.0, 0.0);
        run(&mut s, 4.0);
        s.set_move(0.0, 0.0);
        let px2 = s.player.as_ref().unwrap().centre();
        assert!(px2 < px - 30.0);
        for &id in &ids {
            let x = s.critters.x_of(id).expect("follower still around");
            assert!((x - px2).abs() < 16.0, "follower at {x}, you at {px2}");
        }
        // stand still a moment: they strike up a conversation
        let mut talked = false;
        for _ in 0..(8.0 / DT) as usize {
            s.update(DT, 0.0);
            talked |= s.critters.talking() >= 2;
        }
        assert!(talked, "followers never chatted");
        // F with nobody in focus: everyone heads off again
        s.focus = None;
        s.invite();
        assert_eq!(s.critters.followers(), 0);
    }

    #[test]
    fn e_on_a_bush_picks_berries_and_feeding_an_animal_makes_a_friend() {
        let mut s = walker_scene();
        let t = s.now();
        if s.critters.bush_count() == 0 {
            return; // no bushes in this layout
        }
        let (bx, base, bw) = s.critters.bush_rect(0);
        {
            let p = s.player.as_mut().unwrap();
            p.x = (bx + bw / 2) as f32 - 1.0;
            p.y = base as f32;
        }
        s.update(DT, 0.0);
        assert!(matches!(s.focus, Some(Focus::Bush(0, ..))), "focus {:?}", s.focus);
        s.interact();
        let got = s.player.as_ref().unwrap().berries;
        assert!(got > 0, "no berries picked");
        // a deer comes by: a berry from your hand and it tags along
        let px = s.player.as_ref().unwrap().centre();
        s.critters.spawn_demo(events::Kind::Deer, px + 4.0, t);
        let id = s.critters.last_id();
        s.focus = s.critters.bbox_of(id, s.now()).map(|(x, y, w, _)| Focus::Critter(id, x + w / 2, y - 1));
        s.interact();
        assert_eq!(s.player.as_ref().unwrap().berries, got - 1);
        assert!(s.critters.following(id));
    }
}
