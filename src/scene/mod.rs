//! Scene composition and layout. Everything is placed relative to the grid
//! size, tuned for a 32:9 ultrawide (~568x75 cells) but usable smaller.

pub mod critters;
pub mod events;
pub mod daylight;
pub mod shop;
pub mod high_sky;
pub mod interior;
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
    /// The player (when enabled), the direction keys held, and what E would do.
    player: Option<player::Player>,
    mv: (f32, f32),
    focus: Option<Focus>,
    /// Powerline easter egg: zaps in a row, when the count resets, and the
    /// blackout they cause.
    zaps: u32,
    zap_reset: f32,
    /// The map's unchanging parts (houses, trees, mountains, home).
    map: MiniMap,
    /// Indoors (the player went through a front door).
    inside: Option<Box<interior::Interior>>,
    /// The sky above the sky, and how far the view has been lifted into
    /// it (rows; Ctrl+wheel).
    high: high_sky::HighSky,
    sky_up: f32,
    sky_up_target: f32,
    /// The Cocoa & Gifts cabin, the player's pockets (and cheer), and
    /// whether the pockets panel is showing.
    shop: Option<shop::Shop>,
    /// Time of day (set every frame by the app).
    daylight: daylight::Daylight,
    pockets: shop::Pockets,
    show_pockets: bool,
    show_cheered: bool,
    blackout_until: f32,
    power_back: bool,
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

/// What the map widget shows: the world from end to end.
#[derive(Clone, Default)]
pub struct MiniMap {
    /// World width, the camera and the view width (cells).
    pub ww: f32,
    pub cam: f32,
    pub vw: f32,
    pub player: Option<f32>,
    /// Where the player started.
    pub home: f32,
    /// House centres (true: the church).
    pub houses: Vec<(f32, bool)>,
    /// Trees (true: the big one).
    pub trees: Vec<(f32, bool)>,
    /// Mountain heights across the world, 0 (low) .. 1 (high).
    pub ridge: Vec<f32>,
    /// Animals and people (true: a person).
    pub critters: Vec<(f32, bool)>,
    pub trains: Vec<f32>,
}

/// What the player would interact with, and where its [E] prompt goes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Shop(i32, i32),
    Door(usize, i32, i32),
    Critter(u64, i32, i32),
    Bush(usize, i32, i32),
    Spot(i32, i32),
}

impl Focus {
    fn mark(&self) -> (i32, i32) {
        match *self {
            Focus::Shop(x, y) | Focus::Door(_, x, y) | Focus::Critter(_, x, y) | Focus::Bush(_, x, y) | Focus::Spot(x, y) => (x, y),
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
        let tree_cols: Vec<(i32, i32)> = trees.iter().map(|t| (t.cx - t.hw_base as i32 - 2, t.cx + t.hw_base as i32 + 2)).collect();
        let station = find_station(&occluded, wi, hi, track_row, &tree_cols);
        let ctx = Ctx { w: wi, h: hi, ground_row, track_row, occluded, church, sky_rows, station, cam, vw: w as i32 };
        let mut rng = Rng::new(seed as u64 ^ 0xc11c);
        // relative to creation, so a rebuild on Christmas Eve doesn't launch
        // Santa on the very next frame
        let next_eve_sleigh = rng.range(20.0, 45.0);
        // berry bushes go on the open snow, clear of trees, presents and the snowman
        let mut avoid: Vec<(i32, i32)> = trees.iter().map(|t| (t.cx - t.hw_base as i32 - 4, t.cx + t.hw_base as i32 + 4)).collect();
        avoid.extend(village.snowmen.iter().map(|s| (s.cx - 9, s.cx + 9)));
        avoid.extend(village.lamps.iter().map(|l| (l.x - 1, l.x + 2)));
        let critters = critters::Critters::new(&ctx, seed, cfg.critters, cfg.bushes, &avoid);
        // the player starts out a little left of the big tree, in the snow
        let player = cfg.player.then(|| {
            let px = cam + wf * 0.40;
            let lo = (ctx.ground_row[(px as usize).min(ww - 1)] + 4).min(hi - 2) as f32;
            player::Player::new(px, ((lo + hf - 1.0) / 2.0).round())
        });
        let mut s = Scene {
            w,
            h,
            ww,
            cam,
            cam_target: cam,
            player,
            mv: (0.0, 0.0),
            focus: None,
            zaps: 0,
            zap_reset: 0.0,
            map: MiniMap::default(),
            inside: None,
            high: high_sky::HighSky::new(w, h, seed),
            sky_up: 0.0,
            sky_up_target: 0.0,
            shop: None,
            daylight: daylight::Daylight::default(),
            pockets: shop::Pockets::new(),
            show_pockets: false,
            show_cheered: false,
            blackout_until: 0.0,
            power_back: false,
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
        };
        s.map = s.static_map();
        s.shop = s.place_shop();
        s
    }

    /// A free spot on the snowfield for the cocoa cabin, near where the
    /// player starts.
    fn place_shop(&self) -> Option<shop::Shop> {
        let p = self.player.as_ref()?;
        let want = p.centre() as i32 - 22;
        let ww = self.ww as i32;
        let lo = |x: i32| (self.ctx.ground_row[(x.clamp(0, ww - 1)) as usize] + 4).min(self.h as i32 - 2);
        let clear = |x0: i32| {
            let base = lo(x0 + shop::SHOP_W / 2) + 1;
            if x0 < 2 || x0 + shop::SHOP_W >= ww - 2 || base >= self.h as i32 {
                return None;
            }
            for x in x0 - 2..x0 + shop::SHOP_W + 2 {
                for y in base - 7..=base + 1 {
                    if self.pokeable(x, y) || self.critters.bush_at(x, y).is_some() {
                        return None;
                    }
                }
            }
            Some(base)
        };
        (0..ww).flat_map(|d| [want - d, want + d]).find_map(|x| clear(x).map(|base| shop::Shop { x, base, open: false }))
    }

    /// The player's pockets (kept when the scene is rebuilt).
    pub fn take_pockets(&mut self) -> shop::Pockets {
        std::mem::take(&mut self.pockets)
    }

    pub fn set_pockets(&mut self, p: shop::Pockets) {
        if p.cheer > 0 || p.items.iter().any(|&n| n > 0) || !p.greeted.is_empty() {
            self.pockets = p;
        }
    }

    /// Cheer for being neighbourly, with a little sparkle over the player.
    fn cheer(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        self.pockets.cheer += n;
        if let Some(p) = &self.player {
            self.director.emote(p.centre(), p.y - 4.0, '✦', Rgb::hex(0xffd36b), n as usize);
            self.director.sound(Sfx::Sparkle, p.centre(), 0.35);
        }
    }

    /// I: show / hide the pockets panel.
    pub fn toggle_pockets(&mut self) {
        if self.player.is_some() {
            self.show_pockets = !self.show_pockets;
        }
    }

    /// Esc: close the shop / pockets first. True if something closed.
    pub fn close_panels(&mut self) -> bool {
        let shop_open = self.shop.as_ref().map(|s| s.open).unwrap_or(false);
        if let Some(s) = &mut self.shop {
            s.open = false;
        }
        let had = shop_open || self.show_pockets;
        self.show_pockets = false;
        had
    }

    /// 1-5: buy (at the open shop) or use an item.
    pub fn number(&mut self, n: usize) {
        let Some(&item) = shop::ITEMS.get(n.wrapping_sub(1)) else { return };
        let t = self.now();
        let Some(p) = self.player.as_ref() else { return };
        let (px, py) = (p.centre(), p.y);
        if self.inside.is_some() {
            return;
        }
        if let Some(s) = self.shop.as_ref().filter(|s| s.open) {
            let (kx, ky) = (s.centre(), s.base - 4);
            if self.pockets.cheer >= item.price() {
                self.pockets.cheer -= item.price();
                self.pockets.add(item);
                let line = *self.rng.pick(&["here you go!", "enjoy!", "merry Christmas!", "a fine choice!"]);
                self.director.say(kx, ky, line, t, 2.2, 6);
                self.director.sound(Sfx::Present, kx as f32, 0.7);
            } else {
                self.director.say(kx, ky, "a bit more cheer first — go be neighbourly!", t, 3.0, 6);
            }
            return;
        }
        let have = self.pockets.count(item) > 0;
        if !have {
            self.director.say(px.round() as i32, py as i32 - 4, &format!("no {} in my pockets", item.name()), t, 2.0, 6);
            return;
        }
        let target = match self.focus {
            Some(Focus::Critter(id, ..)) => Some(id),
            _ => None,
        };
        match item {
            shop::Item::Cocoa => {
                self.pockets.take(item);
                self.pockets.cozy_until = t + 45.0;
                self.director.say(px.round() as i32, py as i32 - 4, "mmm, cocoa!", t, 2.0, 6);
                self.director.emote(px, py - 3.0, '♥', Rgb::hex(0xff6b8a), 2);
                self.director.sound(Sfx::Munch, px, 0.6);
            }
            shop::Item::Gingerbread | shop::Item::CandyCane => match target.filter(|&id| self.critters.is_person(id)) {
                Some(id) => {
                    self.pockets.take(item);
                    self.critters.gift(id, item.name(), t, &mut self.director);
                    self.cheer(1);
                }
                None => {
                    self.pockets.take(item);
                    self.director.say(px.round() as i32, py as i32 - 4, "*crunch* delicious", t, 2.0, 6);
                    self.director.sound(Sfx::Munch, px, 0.6);
                }
            },
            shop::Item::Carrot => match target.filter(|&id| self.critters.is_animal(id)) {
                Some(id) => {
                    self.pockets.take(item);
                    self.critters.befriend(id, t, &mut self.director);
                    self.cheer(1);
                }
                None => {
                    self.director.say(px.round() as i32, py as i32 - 4, "this is for a deer or a rabbit", t, 2.4, 6);
                }
            },
            shop::Item::Lantern => {
                self.pockets.lantern_on = !self.pockets.lantern_on;
                self.director.sound(Sfx::Lamp, px, 0.6);
            }
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
            self.zap_reset -= s;
            self.blackout_until -= s;
            self.high.rebase(s);
            if let Some(inn) = &mut self.inside {
                inn.rebase(s);
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
        if let Some(inn) = &mut self.inside {
            // indoors the village is muffled; the room's own sounds are on
            // screen coordinates
            for e in &mut v {
                e.gain *= 0.35;
            }
            for mut e in std::mem::take(&mut inn.director.sounds) {
                e.pan += cam;
                v.push(e);
            }
        }
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
            train: self.director.train_x(self.cam + w / 2.0).map(|x| x - self.cam).map(|x| ((x / w).clamp(0.0, 1.0), edge(x))),
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

    /// How many rows the view is lifted into the sky.
    fn oy_i(&self) -> i32 {
        if self.inside.is_some() { 0 } else { self.sky_up.round() as i32 }
    }

    /// Previews: look `rows` up into the sky right away.
    pub fn look_up(&mut self, rows: f32) {
        self.sky_up_target = rows.clamp(0.0, self.high.depth as f32);
        self.sky_up = self.sky_up_target;
    }

    /// Ctrl+wheel: look up into (or back down from) the sky.
    pub fn pan_up(&mut self, up: bool) {
        if self.inside.is_some() {
            return;
        }
        let step = (self.h as f32 * 0.12).max(3.0);
        self.sky_up_target = (self.sky_up_target + if up { step } else { -step }).clamp(0.0, self.high.depth as f32);
    }

    fn static_map(&self) -> MiniMap {
        let ww = self.ww as f32;
        let m = &self.land.mountains;
        let (lo, hi) = m.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let ridge = (0..128).map(|i| {
            let v = m[(i * (m.len() - 1)) / 127];
            1.0 - (v - lo) / (hi - lo).max(1.0)
        });
        MiniMap {
            ww,
            vw: self.w as f32,
            home: self.player.as_ref().map(|p| p.centre()).unwrap_or(ww / 2.0),
            houses: self.village.houses.iter().map(|h| (h.x as f32 + h.w as f32 / 2.0, h.church)).collect(),
            trees: self.trees.iter().map(|t| (t.cx as f32, t.is_big())).collect(),
            ridge: ridge.collect(),
            ..MiniMap::default()
        }
    }

    /// Everything the map widget needs this frame.
    pub fn minimap(&self) -> MiniMap {
        let mut m = self.map.clone();
        m.cam = self.cam;
        m.player = match (&self.inside, &self.player) {
            (Some(inn), _) => Some(inn.outside.0 + 1.0),
            (None, p) => p.as_ref().map(|p| p.centre()),
        };
        m.critters = self.critters.positions();
        m.trains = self.director.trains();
        m
    }

    /// Look at world column `x` (the map widget was clicked).
    pub fn look_at(&mut self, x: f32) {
        let max = (self.ww - self.w) as f32;
        self.cam_target = (x - self.w as f32 / 2.0).clamp(0.0, max);
    }

    pub fn set_daylight(&mut self, d: daylight::Daylight) {
        self.daylight = d;
    }

    /// Settings that apply without rebuilding the scene.
    pub fn set_live(&mut self, walk_speed: f32, drive_left: bool) {
        self.director.drive_left = drive_left;
        if let Some(p) = &mut self.player {
            p.speed = walk_speed;
        }
    }

    /// The direction keys currently held: x and y each -1, 0 or 1.
    pub fn set_move(&mut self, mx: f32, my: f32) {
        self.mv = (mx, my);
    }

    #[cfg(test)]
    pub fn player_x(&self) -> Option<f32> {
        self.player.as_ref().map(|p| p.x)
    }

    /// Previews: the player stands about idly (no [E] prompts).
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
        // at the cocoa cabin's counter
        if let Some(s) = &self.shop {
            if (s.centre() as f32 - px).abs() <= 9.0 && py >= s.base as f32 && py <= s.base as f32 + 6.0 {
                return Some(Focus::Shop(s.centre(), s.base - 8));
            }
        }
        if let Some(id) = self.critters.nearest(px, py, 7.0) {
            let (bx, by, bw, _) = self.critters.bbox_of(id, t)?;
            return Some(Focus::Critter(id, bx + bw / 2, by - 1));
        }
        if let Some(b) = self.critters.bush_near(px, py) {
            let (bx, base, bw) = self.critters.bush_rect(b);
            return Some(Focus::Bush(b, bx + bw / 2, base - 3));
        }
        // on a doorstep: go in
        for (i, h) in self.village.houses.iter().enumerate() {
            if (h.door() as f32 + 0.5 - px).abs() <= 2.5 && p.y - (h.base as f32) <= 5.0 && p.y >= h.base as f32 - 1.0 {
                return Some(Focus::Door(i, h.door(), h.base - 3));
            }
        }
        // in front first, then right underfoot, then just behind
        let (x0, y0) = (px.round() as i32, py.round() as i32);
        let d = p.dir as i32;
        for k in [1, 2, 0, 3, 4, -1, 5, 6, -2] {
            let x = x0 + d * k;
            for y in (y0 - 16..=y0).rev() {
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
            || self.on_station(x, y)
            || self.shop.as_ref().map(|s| s.contains(x, y)).unwrap_or(false)
    }

    fn on_station(&self, x: i32, y: i32) -> bool {
        self.ctx.station.map(|(sx, sw)| x >= sx && x <= sx + sw && y >= self.ctx.track_row - 4 && y <= self.ctx.track_row).unwrap_or(false)
    }

    /// E / Space / Enter: say hello, pet or feed, pick berries, or poke
    /// whatever's in front of the player.
    pub fn interact(&mut self) {
        let t = self.now();
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
            if !p.free(t) {
                return;
            }
            p.active_at = t;
            if let Some((what, ..)) = inn.nearest(p.centre()) {
                if inn.poke(what, p, t) {
                    self.leave();
                }
            }
            return;
        }
        if let Some(Focus::Door(i, ..)) = self.focus {
            self.enter(i);
            return;
        }
        let Some(p) = self.player.as_mut().filter(|p| p.free(t)) else { return };
        p.active_at = t;
        let (px, py) = (p.centre(), p.y);
        match self.focus {
            Some(Focus::Critter(id, ..)) => {
                let face = self.critters.centre_of(id).map(|cx| (cx - px).signum()).unwrap_or(0.0);
                if face != 0.0 {
                    p.dir = face;
                }
                let berries = p.berries;
                self.critters.meet(id, px, py, &mut p.berries, t, &mut self.director, &mut self.lines);
                let fed = p.berries < berries;
                // cheer: a first hello, or feeding an animal
                if self.critters.is_person(id) && !self.pockets.greeted.contains(&id) {
                    self.pockets.greeted.push(id);
                    self.cheer(1);
                } else if fed {
                    self.cheer(2);
                }
            }
            Some(Focus::Shop(..)) => {
                if let Some(s) = &mut self.shop {
                    s.open = !s.open;
                    if s.open {
                        let (kx, ky) = (s.centre(), s.base - 4);
                        self.director.say(kx, ky, "welcome! cocoa's hot", t, 2.4, 6);
                        self.director.sound(Sfx::Greet, kx as f32, 0.7);
                    }
                }
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
            Some(Focus::Door(..)) => {}
            None => {
                // nothing about: a happy little hop in the snow
                self.director.snow_puff(px, py);
            }
        }
    }

    /// Go in through house `i`'s front door.
    fn enter(&mut self, i: usize) {
        let t = self.now();
        let seed = self.seed_for(i);
        let Some(h) = self.village.houses.get(i) else { return };
        let Some(p) = self.player.as_mut() else { return };
        let inn = interior::Interior::new(i, h.church, h.wall, self.w, self.h, seed, (p.x, p.y), t);
        let (ex, ey) = inn.entrance();
        (p.x, p.y, p.dir) = (ex, ey, 1.0);
        p.snowball = false;
        self.director.sound(Sfx::Click, h.door() as f32, 0.9);
        self.inside = Some(Box::new(inn));
    }

    /// Back out into the snow, on the doorstep.
    pub fn leave(&mut self) {
        let Some(inn) = self.inside.take() else { return };
        if let Some(p) = &mut self.player {
            (p.x, p.y) = inn.outside;
            p.dir = 1.0;
            self.director.sound(Sfx::Click, p.centre(), 0.9);
        }
    }

    /// Previews: standing at the cocoa cabin, menu and pockets open.
    pub fn preview_shop(&mut self) {
        let Some((kx, base)) = self.shop.as_ref().map(|s| (s.centre(), s.base)) else { return };
        let t = self.now();
        if let Some(p) = &mut self.player {
            p.x = kx as f32 + 2.0;
            p.y = base as f32 + 2.0;
            p.dir = -1.0;
            p.active_at = t;
        }
        self.look_at(kx as f32);
        self.cam = self.cam_target;
        self.pockets.cheer = 9;
        self.pockets.add(shop::Item::Cocoa);
        self.pockets.add(shop::Item::Carrot);
        if let Some(s) = &mut self.shop {
            s.open = true;
        }
        self.show_pockets = true;
    }

    /// Previews: step inside house `i` (an index past the end: the church).
    pub fn preview_inside(&mut self, i: usize) {
        let i = if i < self.village.houses.len() { i } else { self.village.houses.iter().position(|h| h.church).unwrap_or(0) };
        self.enter(i);
        let t = self.now();
        if let Some(p) = &mut self.player {
            p.active_at = t;
            p.x = self.w as f32 * 0.36;
        }
    }

    pub fn is_inside(&self) -> bool {
        self.inside.is_some()
    }

    fn seed_for(&self, i: usize) -> u32 {
        (self.ww as u32).wrapping_mul(2654435761) ^ (i as u32).wrapping_mul(0x9e37_79b9) ^ self.h as u32
    }

    /// F: ask whoever's nearby to come along (animals want a berry), or
    /// say goodbye to everyone following.
    pub fn invite(&mut self) {
        if self.inside.is_some() {
            return;
        }
        let t = self.now();
        let Some(p) = self.player.as_mut().filter(|p| p.free(t)) else { return };
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
        if self.inside.is_some() {
            return;
        }
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
        if self.inside.is_some() {
            return false;
        }
        let y = y - self.oy_i();
        let wx = x + self.cam_i();
        self.player.as_ref().map(|p| p.contains(wx, y)).unwrap_or(false) || self.critters.grabbable(wx, y, self.now())
    }

    /// Start carrying whoever is at (x, y) — the player included. False if
    /// nobody's there.
    pub fn grab(&mut self, x: i32, y: i32) -> bool {
        if self.inside.is_some() {
            return false;
        }
        let y = y - self.oy_i();
        let t = self.now();
        let x = x + self.cam_i();
        if let Some(p) = self.player.as_mut().filter(|p| p.contains(x, y)) {
            p.grab(x, y, t);
            self.director.sound(Sfx::Pickup, p.centre(), 1.0);
            return true;
        }
        self.critters.grab(x, y, t, &mut self.director)
    }

    pub fn drag(&mut self, x: i32, y: i32) {
        self.set_cursor(Some((x, y)));
        let y = y - self.oy_i();
        let (wx, ww, h) = (x + self.cam_i(), self.ww as f32, self.h as f32);
        if let Some(p) = self.player.as_mut().filter(|p| p.held.is_some()) {
            p.drag(wx, y, ww, h);
            return;
        }
        self.critters.drag(wx, y);
    }

    /// Let go (a click without dragging becomes a pat or a hello).
    pub fn release(&mut self) {
        let t = self.now();
        if let Some(p) = self.player.as_mut().filter(|p| p.held.is_some()) {
            p.release();
            return;
        }
        self.critters.release(t, &mut self.director, &mut self.lines);
    }

    pub fn holding(&self) -> bool {
        self.player.as_ref().map(|p| p.held.is_some()).unwrap_or(false) || self.critters.holding()
    }

    /// R: pack a snowball (a moment, crouched), then throw it the way
    /// the player is facing — at whoever's over there.
    pub fn snowball(&mut self) {
        if self.inside.is_some() {
            return;
        }
        let t = self.now();
        let Some(p) = self.player.as_mut() else { return };
        if !p.free(t) {
            return;
        }
        p.active_at = t;
        if p.snowball {
            p.snowball = false;
            p.throw_until = t + 0.35;
            let (x, y, dir) = (p.centre(), p.y, p.dir);
            self.critters.throw(x, y, dir, &mut self.director);
        } else {
            p.scoop_until = t + 0.6;
            p.snowball = true;
            self.director.sound(Sfx::Crunch, p.centre(), 0.5);
        }
    }

    /// Is there something under this cell that reacts to a click?
    pub fn clickable(&self, x: i32, y: i32) -> bool {
        if let Some(inn) = &self.inside {
            return inn.thing_at(x, y).is_some();
        }
        let y = y - self.oy_i();
        if y < 0 {
            return self.high.clickable(x, y);
        }
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
            || self.land.on_powerline(x, y)
            || self.on_moon(sx, y)
    }

    /// Clicking the powerline: sparks. Keep at it and the whole village's
    /// power goes out for a while.
    fn zap(&mut self, x: i32, y: i32) {
        let t = self.now();
        if t < self.blackout_until {
            return;
        }
        self.zaps = if t < self.zap_reset { self.zaps + 1 } else { 1 };
        self.zap_reset = t + 10.0;
        let sparks = [Rgb::WHITE, Rgb::hex(0xfff2a0), Rgb::hex(0x9fd8ff)];
        let (xf, yf) = (x as f32, y as f32);
        self.director.burst(xf, yf, 6 + self.zaps as usize * 4, &sparks, 5.0 + self.zaps as f32, 9.0);
        self.director.sound(Sfx::Lamp, xf, 0.6 + 0.1 * self.zaps as f32);
        if self.zaps < 5 {
            return;
        }
        // blackout
        self.zaps = 0;
        let end = t + 25.0;
        self.blackout_until = end;
        self.power_back = true;
        self.director.burst(xf, yf, 50, &sparks, 12.0, 6.0);
        self.director.sound(Sfx::Puff, xf, 1.0);
        for h in &mut self.village.houses {
            // the lights come back one house at a time
            h.lights_off_until = h.lights_off_until.max(end + self.rng.range(0.0, 3.0));
        }
        for l in &mut self.village.lamps {
            l.off_until = l.off_until.max(end + self.rng.range(0.0, 2.0));
        }
        for tr in &mut self.trees {
            tr.dark_until = end + self.rng.range(0.0, 1.5);
        }
        let mut people = self.critters.people();
        people.truncate(2);
        for (i, (px, py)) in people.into_iter().enumerate() {
            let line = *self.rng.pick(&["who turned out the lights?!", "the power's out!", "eek! it's dark!", "was that the powerline?", "anyone got a candle?"]);
            self.director.say(px, py - 1, line, t + 0.6 + i as f32 * 1.3, 2.8, 6);
        }
    }

    /// Something in the village at world (x, y) — a present, the snowman, a
    /// tree, a lamp, a house — reacts as if clicked. Returns whether
    /// anything was there.
    fn poke(&mut self, x: i32, y: i32) -> bool {
        let t = self.now();
        if let Some((kx, ky)) = self.shop.as_ref().filter(|s| s.contains(x, y)).map(|s| (s.centre(), s.base - 4)) {
            let line = *self.rng.pick(&["hot cocoa, gingerbread, candy canes!", "come by the counter for a cocoa!", "merry Christmas, neighbour!", "warm your hands by the window"]);
            self.director.say(kx, ky, line, t, 2.6, 6);
            self.director.sound(Sfx::Greet, kx as f32, 0.6);
            return true;
        }
        if self.on_station(x, y) {
            let (sx, sw) = self.ctx.station.unwrap_or((x, 1));
            let wait = self.director.next_train_in(t).max(1.0);
            let line = if self.director.trains().is_empty() { format!("next train in about {} s", wait.round()) } else { "a train's on the line!".to_string() };
            self.director.say(sx + sw / 2, self.ctx.track_row - 5, &line, t, 3.0, 6);
            self.director.sound(Sfx::TrainWhistle, (sx + sw / 2) as f32, 0.4);
            return true;
        }
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
        let t = self.now();
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_ref()) {
            if let Some(what) = inn.thing_at(x, y) {
                if inn.poke(what, p, t) {
                    self.leave();
                }
            }
            return;
        }
        let y = y - self.oy_i();
        if y < 0 {
            // up in the high sky
            let cam = self.cam;
            let cheer = self.high.click(x, y, t, &mut self.director, cam);
            self.cheer(cheer);
            return;
        }
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
        if self.land.on_powerline(x, y) {
            self.zap(x, y);
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
        let dv = self.sky_up_target - self.sky_up;
        self.sky_up = if dv.abs() < 0.05 { self.sky_up_target } else { self.sky_up + dv * (1.0 - (-dt * 5.0).exp()) };
        self.high.update(dt, t);
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
        let lift = self.sky_up.round();
        if let Some((cx, cy)) = self.cursor.map(|(x, y)| (x, y - lift)) {
            let wx = cx + self.cam;
            let ground = self.on_ground(wx as i32, cy as i32);
            self.director.cursor(wx, cy, self.cursor_vel.0, self.cursor_vel.1, ground, dt);
        }
        let mut walked_out = false;
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
            walked_out = inn.update(dt, t, self.mv, p);
        } else if let Some(p) = &mut self.player {
            let (g, h) = (&self.ctx.ground_row, self.h as f32);
            let lo = |x: f32| (g[(x.max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32;
            p.update(dt, t, self.mv, lo, self.ww as f32, h, &mut self.director);
            // the camera keeps the player comfortably in view while walking
            if p.moving {
                // walking brings the view back down to earth
                self.sky_up_target = 0.0;
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
        if walked_out {
            self.leave();
        }
        // indoors, anyone following the player waits by the door
        let you = match (&self.inside, &self.player) {
            (Some(inn), Some(p)) => Some((inn.outside.0, inn.outside.1, p.dir, false)),
            (None, Some(p)) => Some((p.x, p.y, p.dir, p.moving)),
            _ => None,
        };
        self.critters.set_player(you);
        self.director.update(dt, t, &self.ctx);
        for x in std::mem::take(&mut self.director.arrivals) {
            // passengers off the train walk down into the village
            let kind = if self.rng.chance(0.25) { events::Kind::Kid } else { events::Kind::Walker };
            self.critters.spawn_demo(kind, x, t);
        }
        if let Some(p) = &mut self.player {
            p.lantern = self.pockets.lantern_on && self.pockets.count(shop::Item::Lantern) > 0;
            p.cozy = t < self.pockets.cozy_until;
            // walked away from the counter: the shop menu closes
            if let Some(s) = &mut self.shop {
                if s.open && ((s.centre() as f32 - p.centre()).abs() > 14.0 || self.inside.is_some()) {
                    s.open = false;
                }
            }
        }
        let hits = self.critters.take_your_hits();
        self.cheer(hits);
        // watching a light show by the big tree
        let showing = self.director.show.active(t);
        if !showing {
            self.show_cheered = false;
        } else if !self.show_cheered {
            let tree = self.trees.iter().find(|tr| tr.is_big()).map(|tr| tr.cx as f32);
            if let (Some(tx), Some(p)) = (tree, &self.player) {
                if (p.centre() - tx).abs() < 45.0 && self.inside.is_none() {
                    self.show_cheered = true;
                    self.cheer(3);
                }
            }
        }
        if let Some(until) = self.director.gather.take() {
            // a few more folk come out for the show, then everyone gathers
            for _ in 0..3 {
                self.critters.request(events::Kind::Walker, t);
            }
            let tree = self.trees.iter().find(|tr| tr.is_big()).map(|tr| tr.cx as f32).unwrap_or(self.ww as f32 / 2.0);
            self.critters.gather(tree, until, t, &mut self.director, &mut self.lines);
        }
        for k in std::mem::take(&mut self.director.spawn_requests) {
            self.critters.request(k, t);
        }
        // the cursor matters to the animals for a while after it last moved
        let cam = self.cam;
        let cur = self.cursor.filter(|_| t - self.cursor_seen < 15.0).map(|(x, y)| (x + cam, y - lift, self.cursor_vel.0, self.cursor_vel.1));
        let snowman = self.village.snowmen.first().map(village::snowman_head);
        self.critters.update(dt, t, &self.ctx, &mut self.director, &mut self.lines, cur, snowman);
        // speech bubbles follow their speakers (the player included)
        let mut who = self.critters.anchors();
        if let (None, Some(p)) = (&self.inside, &self.player) {
            who.push((u64::MAX - 1, p.centre(), p.y));
        }
        self.director.track(&who);
        if self.critters.take_hits_on_you() > 0 {
            if let Some(p) = &mut self.player {
                p.bonk(t, &mut self.director);
                let line = *self.rng.pick(&["brr!", "hey!", "ha — got me!", "oof!", "you're on!"]);
                self.director.say(p.centre().round() as i32, p.y as i32 - 4, line, t + 0.2, 2.0, 6);
            }
        }
        self.focus = if self.inside.is_some() { None } else { self.find_focus() };
        // passers-by sometimes say hello
        if self.rng.chance(dt * 0.04) {
            let walkers = self.critters.people();
            if !walkers.is_empty() {
                let (wx, wy) = walkers[self.rng.int(0, walkers.len() as i32) as usize];
                let msg = self.lines.walker();
                self.director.bubble(wx, wy, &msg, t);
            }
        }
        if self.power_back && t >= self.blackout_until {
            self.power_back = false;
            let mid = self.cam + self.w as f32 / 2.0;
            self.director.sound(Sfx::Chime, mid, 0.8);
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
        f.day = 0.0;
        f.day_warm = 0.0;
        if let (Some(inn), Some(p)) = (&self.inside, &self.player) {
            let prompt = (t - p.active_at < 20.0).then(|| inn.nearest(p.centre()).map(|(_, x, y)| (x, y))).flatten();
            inn.draw(f, t, glow, p, prompt);
            f.ox = 0;
            return;
        }
        let cam = self.cam_i();
        let oy = self.oy_i();
        f.oy = oy;
        // pixel layer (sky things stay on screen, the world scrolls)
        f.ox = 0;
        self.sky.draw_px(f, t, self.director.flare.value(t));
        let dl = self.daylight;
        if oy > 0 {
            self.high.draw(f, t, glow, self.sky.top_color(), dl.sun);
        }
        daylight::sky_pass(f, &dl, -2 * oy, self.land.horizon as i32, glow);
        (f.day, f.day_warm) = (dl.sun, dl.warm);
        self.director.draw_px(f);
        self.land.draw_px(f, self.cam);
        self.land.draw_lights(f, self.cam, t, glow);
        f.ox = cam;
        self.village.draw_px(f, self.cam);
        // glyph layer, back to front
        f.ox = 0;
        if dl.sun < 0.45 {
            // stars and the moon's glyphs (not in daylight)
            self.sky.draw_glyphs(f, t);
        }
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
        if let Some(s) = &self.shop {
            s.draw(f, t, glow);
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
        // what E would do, while the player is out walking
        if let (Some(p), Some(fc)) = (&self.player, self.focus) {
            if t - p.active_at < 20.0 {
                // over the person / bush; for village things, over the player's head
                let (x, y) = match fc {
                    Focus::Spot(..) => (p.centre().round() as i32, p.y.round() as i32 - 5),
                    _ => fc.mark(),
                };
                player::Player::draw_prompt(f, x, y, 'E', t);
            }
        }
        // daylight for the scene's glyphs (before any panels go on top)
        f.grade_scene(dl.sun);
        f.day = 0.0;
        if let Some(s) = self.shop.as_ref().filter(|s| s.open) {
            s.draw_menu(f, &self.pockets);
        }
        f.ox = 0;
        f.oy = 0;
        if self.show_pockets {
            shop::draw_pockets(f, &self.pockets);
        }
        // a soft lantern-like glow that follows the pointer while it moves
        if let Some((cx, cy)) = self.cursor {
            let k = (1.0 - (t - self.cursor_seen) / 2.5).clamp(0.0, 1.0);
            if k > 0.0 {
                f.light_cell(cx as i32, cy as i32, Rgb::hex(0xffd9a0) * (2.2 * k * glow));
            }
        }
    }
}

/// A spot along the railway, not hidden behind any house, for the station
/// (as near the middle-left of the village as there is room).
fn find_station(occluded: &[bool], w: i32, h: i32, track_row: i32, trees: &[(i32, i32)]) -> Option<(i32, i32)> {
    const SW: i32 = 18;
    if track_row < 5 || track_row >= h || w < SW + 4 {
        return None;
    }
    let free = |x0: i32| {
        (x0 - 1..x0 + SW + 2).all(|x| x >= 0 && x < w && (track_row - 4..=track_row).all(|y| !occluded[(y * w + x) as usize]))
            && !trees.iter().any(|&(a, b)| x0 + SW + 1 > a && x0 - 1 < b)
    };
    let want = (w as f32 * 0.42) as i32;
    (0..w).flat_map(|d| [want - d, want + d]).find(|&x| free(x)).map(|x| (x, SW))
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
        // two people standing just in front of the player
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
        // a deer comes by: a berry from the player's hand and it tags along
        let px = s.player.as_ref().unwrap().centre();
        s.critters.spawn_demo(events::Kind::Deer, px + 4.0, t);
        let id = s.critters.last_id();
        s.focus = s.critters.bbox_of(id, s.now()).map(|(x, y, w, _)| Focus::Critter(id, x + w / 2, y - 1));
        s.interact();
        assert_eq!(s.player.as_ref().unwrap().berries, got - 1);
        assert!(s.critters.following(id));
    }

    #[test]
    fn people_step_around_each_other_instead_of_through() {
        let mut cfg = SceneCfg::default();
        cfg.events = 0.0;
        cfg.critters = 0.0;
        let mut s = Scene::new(300, 75, 7, &cfg);
        let t = s.now();
        let x = s.cam + s.w as f32 * 0.2;
        s.critters.spawn_demo(events::Kind::Walker, x, t);
        let a = s.critters.last_id();
        s.critters.spawn_demo(events::Kind::Walker, x + 30.0, t);
        let b = s.critters.last_id();
        s.critters.set_walking(a, 1.0);
        s.critters.set_walking(b, -1.0);
        let mut closest = f32::MAX;
        for _ in 0..(8.0 / DT) as usize {
            s.update(DT, 0.0);
            let (Some((ax, ay)), Some((bx, by))) = (s.critters.pos_of(a), s.critters.pos_of(b)) else { break };
            if (ax - bx).abs() < 1.5 {
                closest = closest.min((ay.round() - by.round()).abs());
            }
            if std::env::var_os("DBG").is_some() { eprintln!("{ax:.1},{ay:.2}  {bx:.1},{by:.2}"); }
        }
        assert!(closest >= 1.0 && closest < f32::MAX, "they walked through each other (row gap {closest})");
    }

    #[test]
    fn a_snowball_hit_earns_cheer() {
        let mut s = walker_scene();
        let t = s.now();
        let (px, py) = { let p = s.player.as_ref().unwrap(); (p.centre(), p.y) };
        s.player.as_mut().unwrap().dir = 1.0;
        s.critters.spawn_demo(events::Kind::Walker, px + 12.0, t);
        let id = s.critters.last_id();
        s.critters.set_walking(id, 0.0);
        let _ = py;
        let before = s.pockets.cheer;
        s.snowball(); // scoop…
        run(&mut s, 0.8);
        s.snowball(); // …and throw
        run(&mut s, 2.0);
        assert!(s.pockets.cheer > before, "cheer {before} → {}", s.pockets.cheer);
    }

    #[test]
    fn the_shop_sells_cocoa_for_cheer() {
        let mut s = walker_scene();
        let Some((kx, base)) = s.shop.as_ref().map(|k| (k.centre(), k.base)) else { panic!("no shop placed") };
        {
            let p = s.player.as_mut().unwrap();
            p.x = kx as f32 - 1.0;
            p.y = base as f32 + 2.0;
        }
        s.update(DT, 0.0);
        assert!(matches!(s.focus, Some(Focus::Shop(..))), "focus {:?}", s.focus);
        s.interact();
        assert!(s.shop.as_ref().unwrap().open);
        assert_eq!(s.pockets.cheer, 5);
        s.number(1); // cocoa, 3
        assert_eq!((s.pockets.cheer, s.pockets.count(shop::Item::Cocoa)), (2, 1));
        s.number(1); // can't afford another
        assert_eq!((s.pockets.cheer, s.pockets.count(shop::Item::Cocoa)), (2, 1));
        assert!(s.close_panels());
        s.number(1); // drink it
        assert_eq!(s.pockets.count(shop::Item::Cocoa), 0);
        assert!(s.pockets.cozy_until > s.now());
    }

    #[test]
    fn walking_in_a_front_door_and_back_out() {
        let mut s = walker_scene();
        let (door, base) = { let h = &s.village.houses[0]; (h.door(), h.base) };
        let lo = (s.ctx.ground_row[door as usize] + 4).min(s.h as i32 - 2) as f32;
        {
            let p = s.player.as_mut().unwrap();
            p.x = door as f32 - 0.5;
            p.y = lo.max(base as f32);
        }
        s.update(DT, 0.0);
        if !matches!(s.focus, Some(Focus::Door(0, ..))) {
            // this house's doorstep isn't reachable from the snowfield here
            return;
        }
        let outside = { let p = s.player.as_ref().unwrap(); (p.x, p.y) };
        s.interact();
        assert!(s.is_inside());
        run(&mut s, 1.0);
        s.leave();
        assert!(!s.is_inside());
        let p = s.player.as_ref().unwrap();
        assert_eq!((p.x, p.y), outside);
    }

    #[test]
    fn daylight_follows_the_sun() {
        use daylight::compute;
        let (rise, set) = (7.5 * 60.0, 17.0 * 60.0);
        assert_eq!(compute("auto", 0.0, rise, set).sun, 0.0);
        assert!(compute("auto", 12.0 * 60.0, rise, set).sun > 0.99);
        let dusk = compute("auto", set + 10.0, rise, set);
        assert!(dusk.sun > 0.0 && dusk.sun < 1.0 && dusk.warm > 0.8, "{dusk:?}");
        assert_eq!(compute("night", 12.0 * 60.0, rise, set).sun, 0.0);
        assert!(compute("noon", 0.0, rise, set).sun > 0.99);
    }
}
