//! Scene composition and layout. Everything is placed relative to the grid
//! size, tuned for a 32:9 ultrawide (~568x75 cells) but usable smaller.

pub mod critters;
pub mod events;
pub mod skylife;
pub mod daylight;
pub mod dialogue;
pub mod shop;
pub mod high_sky;
pub mod interior;
pub mod land;
pub mod lines;
pub mod player;
pub mod sky;
pub mod builds;
pub mod newyear;
pub mod pets;
pub mod santa;
pub mod story;
pub mod sleigh;
pub mod snow;
pub mod tree;
pub mod village;

use crate::audio::{AmbState, Sfx, SoundEvent};
use crate::color::{pal, Rgb};
use crate::config::SceneCfg;
use crate::frame::Frame;
use crate::rng::Rng;
use chrono::{Datelike, Timelike};
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
    /// When things last happened, for what people talk about: a train
    /// at the station, a light show, a shooting star, Santa overhead.
    last_train: f32,
    last_show: f32,
    last_stars: f32,
    last_sleigh: f32,
    sleigh_was: bool,
    /// The map's unchanging parts (houses, trees, mountains, home).
    map: MiniMap,
    /// Indoors (the player went through a front door).
    inside: Option<Box<interior::Interior>>,
    /// Clouds, planes, birds, an owl, lanterns.
    skylife: skylife::SkyLife,
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
    show_stats: bool,
    goal: Option<Goal>,
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
    /// Santa's rounds, roof to roof (December nights; all night on
    /// Christmas Eve), and when the next one's due.
    rounds: Option<santa::Rounds>,
    next_round: f32,
    /// New Year's Eve: the ball drop and the countdown.
    newyear: Option<newyear::NewYear>,
    /// Snowmen and igloos the kids are building.
    builds: builds::Builds,
    /// A conversation with someone (the dialogue box), its own way of
    /// picking topics, and which answer the pointer's over.
    convo: Option<story::Convo>,
    story_talk: dialogue::Talk,
    convo_hover: Option<usize>,
    /// Where the box was last dragged to (kept for the next one), and the
    /// grab offset while it's being dragged.
    convo_pos: Option<(i32, i32)>,
    convo_drag: Option<(i32, i32)>,
    /// What the villager's been up to lately, for people who saw it.
    deeds: Vec<(Deed, f32, f32)>,
    /// How many times the villager has talked with each person.
    talks: std::collections::HashMap<u64, u32>,
    /// Looking out of a window indoors (the view, and how far the villager
    /// leans to one side).
    look: Option<interior::Look>,
    lean: f32,
    /// The villager's own house.
    home: Option<usize>,
    /// The villager's pet, its config as last applied, and a change made
    /// in the village (named, restyled) for the app to save.
    my_pet: Option<pets::MyPet>,
    pet_cfg: crate::config::PetCfg,
    pet_change: Option<crate::config::PetCfg>,
    /// The villager's name (empty: "neighbour").
    you_name: String,
    /// The animal the villager's pet is after.
    pet_chasing: Option<u64>,
    /// Drawing for a zoomed view: people are left for `draw_people`.
    zoomed: std::cell::Cell<bool>,
    /// What the villager has in hand (Q to change): given to whoever E
    /// is aimed at, or used.
    held: Option<Held>,
    rng: Rng,
    lines: lines::Lines,
    /// Seconds since this scene was created (sum of frame `dt`s).
    clock: f64,
    /// Total rebased away; scene time handed out is `clock - epoch`.
    epoch: f64,
}

/// Conversation ids for people in their homes (not critters).
const HOST_ID: u64 = 1 << 40;
/// The villager's pet's menu.
const PET_ID: u64 = HOST_ID - 7;

/// Something clickable that's moving about or far off.
#[derive(Clone, Copy, Debug)]
enum Thing {
    Sleigh,
    Star(usize),
    Flyer(u8),
    Cabin(usize),
    Train,
    Car(usize),
}

/// Something the villager did that people nearby might mention.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Deed {
    Fed(&'static str),
    Pet(&'static str),
    Threw,
    Hit(u64),
    Gift(u64),
    Carried(u64),
    DropIn,
    Visit,
    SantaChat(u64),
}

/// "a rabbit" → what people call it.
fn animal_word(label: &str) -> &'static str {
    if label.contains("rabbit") {
        "bunny"
    } else if label.contains("deer") {
        "deer"
    } else {
        "fox"
    }
}

/// The special nights (and day) of the season.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Special {
    None,
    /// Christmas Eve, from 6 pm until dawn: the village asleep, Santa's rounds.
    EveNight,
    /// Christmas Day.
    XmasDay,
    /// New Year's Eve, from 6 pm to half past midnight.
    NewYear,
}

/// Which special time it is (by the clock, see `crate::clock`).
pub fn special_now() -> Special {
    let now = crate::clock::now();
    let (m, d, h) = (now.month(), now.day(), now.hour());
    match (m, d) {
        (12, 24) if h >= 18 => Special::EveNight,
        (12, 25) if h < 6 => Special::EveNight,
        (12, 25) => Special::XmasDay,
        (12, 31) if h >= 18 => Special::NewYear,
        (1, 1) if h == 0 && now.minute() < 30 => Special::NewYear,
        _ => Special::None,
    }
}

/// Something to do, from the right-click menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Do {
    /// Arrive from the sky at (world column, row).
    DropIn(f32, f32),
    WalkTo(f32, f32),
    Stats,
    Pockets,
    Talk(u64),
    Invite(u64),
    Give(u64, shop::Item),
    Pet(u64),
    Snowball(u64),
    Visit(usize),
    Shop,
    /// Poke whatever's at this world cell (the station, a house's lights…).
    Poke(i32, i32),
    Wish,
    LookUp,
    LookDown,
    Leave,
}

/// Where the villager is walking on their own, and what to do there.
#[derive(Clone, Copy, Debug)]
struct Goal {
    then: Do,
    /// Give up after this (scene time).
    until: f32,
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
    /// The villager's own pet.
    Pet(i32, i32),
    Shop(i32, i32),
    Door(usize, i32, i32),
    Critter(u64, i32, i32),
    Bush(usize, i32, i32),
    Spot(i32, i32),
}

impl Focus {
    fn mark(&self) -> (i32, i32) {
        match *self {
            Focus::Pet(x, y) | Focus::Shop(x, y) | Focus::Door(_, x, y) | Focus::Critter(_, x, y) | Focus::Bush(_, x, y) | Focus::Spot(x, y) => (x, y),
        }
    }
}

impl Scene {
    pub fn new(w: usize, h: usize, seed: u32, cfg: &SceneCfg) -> Self {
        let (pw, ph) = (w, h * 2);
        let (wf, hf) = (w as f32, h as f32);
        // a wider world to wander through (small screens just get one screen)
        let ww = if w >= 150 && h >= 30 { (wf * 3.6) as usize } else { w };
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
        let next_round = rng.range(20.0, 45.0);
        // berry bushes go on the open snow, clear of trees, presents and the snowman
        let mut avoid: Vec<(i32, i32)> = trees.iter().map(|t| (t.cx - t.hw_base as i32 - 4, t.cx + t.hw_base as i32 + 4)).collect();
        avoid.extend(village.snowmen.iter().map(|s| (s.cx - 9, s.cx + 9)));
        avoid.extend(village.lamps.iter().map(|l| (l.x - 1, l.x + 2)));
        let mut critters = critters::Critters::new(&ctx, seed, cfg.critters, cfg.bushes, &avoid);
        critters.k = sprite_k(h);
        critters.set_max(cfg.most_at_once as usize);
        // the player starts out a little left of the big tree, in the snow
        let player = cfg.player.then(|| {
            let px = cam + wf * 0.40;
            let lo = (ctx.ground_row[(px as usize).min(ww - 1)] + 4).min(hi - 2) as f32;
            let mut p = player::Player::new(px, ((lo + hf - 1.0) / 2.0).round());
            p.k = sprite_k(h);
            p
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
            last_train: -1e9,
            last_show: -1e9,
            last_stars: -1e9,
            last_sleigh: -1e9,
            sleigh_was: false,
            map: MiniMap::default(),
            inside: None,
            high: high_sky::HighSky::new(w, h, seed),
            skylife: skylife::SkyLife::new(w, land.horizon, seed),
            sky_up: 0.0,
            sky_up_target: 0.0,
            shop: None,
            daylight: daylight::Daylight::default(),
            pockets: shop::Pockets::new(),
            show_pockets: false,
            show_stats: false,
            goal: None,
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
            rounds: None,
            next_round,
            newyear: None,
            builds: builds::Builds::new(h, seed),
            convo: None,
            story_talk: dialogue::Talk::new(seed ^ 0x570),
            convo_hover: None,
            convo_pos: None,
            convo_drag: None,
            deeds: vec![],
            talks: std::collections::HashMap::new(),
            look: None,
            lean: 0.0,
            home: None,
            my_pet: None,
            pet_cfg: cfg.pet.clone(),
            pet_change: None,
            you_name: cfg.name.clone(),
            pet_chasing: None,
            zoomed: std::cell::Cell::new(false),
            held: None,
            rng,
            lines: lines::Lines::new(seed),
            clock: 0.0,
            epoch: 0.0,
        };
        s.map = s.static_map();
        s.shop = s.place_shop();
        // the villager's own house: the one nearest where they start out
        if let Some(px) = s.player.as_ref().map(|p| p.centre()) {
            let home = s.village.houses.iter().enumerate().filter(|(_, h)| !h.church).min_by_key(|(_, h)| (h.door() as f32 - px).abs() as i32).map(|(i, _)| i);
            if let Some(i) = home {
                s.village.houses[i].home = true;
            }
            s.home = home;
            let py = s.player.as_ref().map(|p| p.y).unwrap_or(0.0);
            s.my_pet = pets::MyPet::new(&cfg.pet, px - 5.0, py);
        }
        // bubbles clear the heads of people drawn bigger
        s.director.lift = ((sprite_k(h) - 1.0) * 4.0).round() as i32;
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
        (0..ww).flat_map(|d| [want - d, want + d]).find_map(|x| clear(x).map(|base| shop::Shop { x, base, open: false, closed: false }))
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

    /// The right-click menu for screen cell (x, y): a title and what can be
    /// done (the app adds "settings…").
    pub fn menu_at(&self, sx: i32, sy: i32) -> (String, Vec<(String, Do)>) {
        let t = self.now();
        let mut out: Vec<(String, Do)> = vec![];
        if self.inside.is_some() {
            return ("indoors".into(), vec![("step back outside".into(), Do::Leave)]);
        }
        let y = sy - self.oy_i();
        if y < 0 {
            return ("the high sky".into(), vec![("wish on a star".into(), Do::Wish), ("back down to the village".into(), Do::LookDown)]);
        }
        let x = sx + self.cam_i();
        let Some(p) = &self.player else {
            return ("the village".into(), vec![("look up into the sky".into(), Do::LookUp)]);
        };
        if p.contains(x, y) {
            return ("you".into(), vec![("my stats & Christmas list".into(), Do::Stats), ("my pockets".into(), Do::Pockets)]);
        }
        if let Some(id) = self.critters.id_at(x, y, t) {
            let who = self.critters.label(id);
            if self.critters.is_person(id) {
                out.push((format!("say hello to {who}"), Do::Talk(id)));
                let along = self.critters.following(id);
                out.push((if along { format!("say goodbye to {who}") } else { format!("invite {who} along") }, Do::Invite(id)));
                for it in [shop::Item::Gingerbread, shop::Item::CandyCane] {
                    if self.pockets.count(it) > 0 {
                        out.push((format!("give {who} a {}", it.name()), Do::Give(id, it)));
                    }
                }
            } else {
                out.push((format!("pet {who}"), Do::Pet(id)));
                if p.berries > 0 {
                    out.push((format!("feed {who} a berry"), Do::Pet(id)));
                }
                if self.pockets.count(shop::Item::Carrot) > 0 {
                    out.push((format!("give {who} a carrot"), Do::Give(id, shop::Item::Carrot)));
                }
            }
            out.push((format!("throw a snowball at {who}"), Do::Snowball(id)));
            return (who, out);
        }
        if let Some(s) = self.shop.as_ref().filter(|s| s.contains(x, y)) {
            let _ = s;
            return ("Cocoa & Gifts".into(), vec![("visit the cocoa cabin".into(), Do::Shop)]);
        }
        if self.on_station(x, y) {
            return ("the station".into(), vec![("when's the next train?".into(), Do::Poke(x, y))]);
        }
        if let Some(i) = self.village.houses.iter().position(|h| h.contains(x, y)) {
            let church = self.village.houses[i].church;
            let home = self.home == Some(i);
            let title = if church {
                "the church"
            } else if home {
                "home ⚑"
            } else {
                "a cozy house"
            };
            let label = if church {
                "go inside"
            } else if home {
                "go home"
            } else {
                "visit"
            };
            let mut v = vec![(label.to_string(), Do::Visit(i))];
            v.push((if church { "ring the bells".into() } else { "lights on / off".into() }, Do::Poke(x, y)));
            return (title.into(), v);
        }
        if self.in_sky(sx, y) {
            return ("the sky".into(), vec![("wish on a star".into(), Do::Wish), ("look up into the sky".into(), Do::LookUp)]);
        }
        // open ground (or anywhere else): drop in, or walk over
        let lo = self.lo_at(x as f32);
        let row = (y as f32).clamp(lo, self.h as f32 - 1.0);
        ("here".into(), vec![("✦ drop in here".into(), Do::DropIn(x as f32, row)), ("walk over here".into(), Do::WalkTo(x as f32, row))])
    }

    /// First walkable row at world column `x`.
    fn lo_at(&self, x: f32) -> f32 {
        let g = &self.ctx.ground_row;
        (g[(x.max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32
    }

    /// Do something from the right-click menu.
    pub fn act(&mut self, what: Do) {
        let t = self.now();
        match what {
            Do::DropIn(x, row) => {
                self.goal = None;
                if let Some(p) = &mut self.player {
                    p.drop_in(x, row, t, &mut self.director);
                }
                self.deed(Deed::DropIn, x);
                self.look_at(x);
                self.sky_up_target = 0.0;
            }
            Do::Stats => {
                self.show_stats = !self.show_stats;
                self.show_pockets = false;
            }
            Do::Pockets => {
                self.show_pockets = !self.show_pockets;
                self.show_stats = false;
            }
            Do::Poke(x, y) => {
                self.poke(x, y);
            }
            Do::Wish => {
                let (x, y) = (self.rng.range(10.0, self.w as f32 - 10.0), self.rng.range(2.0, self.ctx.sky_rows as f32 * 1.4));
                self.director.shooting_star(x, y);
                self.director.happenings.push((dialogue::Ev::ShootingStar, Some(x + self.cam)));
                if let Some(p) = &self.player {
                    self.director.say(p.centre().round() as i32, p.y as i32 - 4, "✦ I wish… ✦", t, 2.4, 6);
                }
            }
            Do::LookUp => {
                self.pan_up(true);
                self.pan_up(true);
            }
            Do::LookDown => self.sky_up_target = 0.0,
            Do::Leave => self.leave(),
            // everything else: walk over there first
            _ => self.goal = Some(Goal { then: what, until: t + 25.0 }),
        }
    }

    /// Where a goal is (world column, row, how close counts as there).
    fn goal_spot(&self, g: &Goal) -> Option<(f32, f32, f32)> {
        match g.then {
            Do::WalkTo(x, y) => Some((x, y, 1.0)),
            Do::Talk(id) | Do::Invite(id) | Do::Give(id, _) | Do::Pet(id) => self.critters.position(id).map(|(x, y)| (x, y, 5.0)),
            Do::Snowball(id) => self.critters.position(id).map(|(x, y)| (x, y, 22.0)),
            Do::Visit(i) => self.village.houses.get(i).map(|h| (h.door() as f32 + 0.5, self.lo_at(h.door() as f32).max(h.base as f32), 1.2)),
            Do::Shop => self.shop.as_ref().map(|s| (s.centre() as f32, s.base as f32 + 2.0, 3.0)),
            _ => None,
        }
    }

    /// Walking to a goal: the direction keys to "hold" this frame, and
    /// doing the thing on arrival.
    fn steer(&mut self, t: f32) -> Option<(f32, f32)> {
        let g = self.goal?;
        let p = self.player.as_ref()?;
        if t > g.until || self.inside.is_some() {
            self.goal = None;
            return None;
        }
        let Some((gx, gy, reach)) = self.goal_spot(&g) else {
            self.goal = None;
            return None;
        };
        let (dx, dy) = (gx - p.centre(), gy - p.y);
        let there = dx.abs() <= reach && dy.abs() <= if reach > 4.0 { 3.0 } else { 0.8 };
        if !there {
            let mx = if dx.abs() > reach * 0.6 { dx.signum() } else { 0.0 };
            let my = if dy.abs() > 0.6 { dy.signum() } else { 0.0 };
            return Some((mx, my));
        }
        self.goal = None;
        if let Some(p) = &mut self.player {
            if dx.abs() > 0.5 {
                p.dir = dx.signum();
            }
        }
        let focus_on = |s: &mut Self, id: u64| s.focus = s.critters.position(id).map(|(x, y)| Focus::Critter(id, x as i32, y as i32 - 4 - s.director.lift));
        match g.then {
            Do::Talk(id) | Do::Pet(id) => {
                focus_on(self, id);
                self.interact();
            }
            Do::Invite(id) => {
                focus_on(self, id);
                self.invite();
            }
            Do::Give(id, it) => {
                focus_on(self, id);
                if let Some(n) = shop::ITEMS.iter().position(|&i| i == it) {
                    self.number(n + 1);
                }
            }
            Do::Snowball(id) => {
                focus_on(self, id);
                // scoop (once), then throw as soon as it's packed
                let (has, free) = self.player.as_ref().map(|p| (p.snowball, p.free(t))).unwrap_or((false, false));
                if free {
                    self.snowball();
                }
                if !(has && free) {
                    self.goal = Some(Goal { then: Do::Snowball(id), until: t + 3.0 });
                }
            }
            Do::Visit(i) => self.enter(i),
            Do::Shop => {
                if let Some(s) = &mut self.shop {
                    s.open = !s.closed;
                }
            }
            _ => {}
        }
        None
    }

    /// The pet's settings (from the config): make it, restyle it or let it
    /// go.
    pub fn set_pet(&mut self, cfg: &crate::config::PetCfg) {
        if *cfg == self.pet_cfg {
            return;
        }
        self.pet_cfg = cfg.clone();
        match (&mut self.my_pet, pets::PetKind::from_str(&cfg.kind)) {
            (Some(p), Some(_)) => p.restyle(cfg),
            (None, Some(_)) => {
                if let Some(p) = &self.player {
                    self.my_pet = pets::MyPet::new(cfg, p.centre() - 5.0, p.y);
                }
            }
            (_, None) => self.my_pet = None,
        }
    }

    /// The pet was named / restyled in the village (for the app to save).
    pub fn take_pet_change(&mut self) -> Option<crate::config::PetCfg> {
        self.pet_change.take()
    }

    fn pet_changed(&mut self) {
        if let Some(p) = &self.my_pet {
            let c = p.cfg();
            self.pet_cfg = c.clone();
            self.pet_change = Some(c);
        }
    }

    /// The pet's menu in the dialogue box: 0 the main page, 1 its look, 10+n
    /// a page of names.
    fn pet_menu(&mut self, page: u8) {
        use story::{Act, Opt, PetAct as P};
        let Some(pet) = &self.my_pet else { return };
        let kind = pet.kind.name();
        let name = if pet.name.is_empty() { format!("your {kind}") } else { pet.name.clone() };
        let at_home = self.inside.as_ref().is_some_and(|inn| inn.home);
        let (line, opts) = if pet.name.is_empty() && page == 0 {
            // first meeting: straight to naming them
            return self.pet_menu_names(0, &format!("a little {kind} bounces up to you, tail going — they're yours! what will you call them?"));
        } else if page >= 10 {
            return self.pet_menu_names(page - 10, &format!("what shall we call your {kind}?"));
        } else if page == 2 {
            let v = match pet.kind {
                pets::PetKind::Dog => vec![
                    Opt::new("throw the ball", Act::Pet(P::Throw(1))),
                    Opt::new("throw a stick", Act::Pet(P::Throw(2))),
                    Opt::new(&format!("{name}, do a trick!"), Act::Pet(P::Trick)),
                    Opt::new("chase me!", Act::Pet(P::Zoomies)),
                ],
                pets::PetKind::Cat => vec![
                    Opt::new("toss a ball of yarn", Act::Pet(P::Throw(0))),
                    Opt::new("dangle your scarf", Act::Pet(P::Tease)),
                    Opt::new(&format!("a trick, {name}?"), Act::Pet(P::Trick)),
                    Opt::new("chase me!", Act::Pet(P::Zoomies)),
                ],
                pets::PetKind::Bunny => vec![
                    Opt::new("roll a carrot", Act::Pet(P::Throw(3))),
                    Opt::new(&format!("{name}, do a trick!"), Act::Pet(P::Trick)),
                    Opt::new("a little race!", Act::Pet(P::Zoomies)),
                ],
            };
            (format!("{name} looks up at you, all ears. what'll it be?"), v)
        } else if page == 3 {
            let mut v = vec![Opt::new(&format!("brush {name}"), Act::Pet(P::Brush))];
            if at_home && !pet.staying {
                v.push(Opt::new(&format!("stay here, {name}"), Act::Pet(P::Stay)));
            }
            v.push(Opt::new("a new name…", Act::Pet(P::Names(0))));
            v.push(Opt::new("a new look…", Act::Pet(P::Look)));
            v.push(Opt::new("that's all", Act::Pet(P::Done)));
            (format!("{name} tilts their head, listening"), v)
        } else if page == 1 {
            let mut v = vec![];
            for (k, label) in ["a dog", "a cat", "a bunny"].iter().enumerate() {
                if pets::PetKind::from_str(&label[2..]) != Some(pet.kind) {
                    v.push(Opt::new(&format!("make {name} {label}"), Act::Pet(P::Kind(k as u8))));
                }
            }
            v.push(Opt::new(&format!("a different colour (now {})", pet.coat), Act::Pet(P::Coat)));
            v.push(Opt::new("that's perfect", Act::Pet(P::Done)));
            (format!("{name} sits very still, ready for a new look"), v)
        } else {
            let mood = match pet.kind {
                pets::PetKind::Dog => "wags so hard the whole back end goes too",
                pets::PetKind::Cat => "winds round your boots, purring",
                pets::PetKind::Bunny => "does a happy little hop",
            };
            let mut v = vec![
                Opt::new(&format!("give {name} a fuss"), Act::Pet(P::Fuss)),
                Opt::new(&format!("cuddle {name}"), Act::Pet(P::Cuddle)),
                Opt::new(&format!("feed {name}"), Act::Pet(P::Feed)),
                Opt::new("play!", Act::Pet(P::Play)),
            ];
            if pet.asleep() {
                v.push(Opt::new(&format!("wake {name} up"), Act::Pet(P::Wake)));
            } else {
                v.push(Opt::new("nap time", Act::Pet(P::Nap)));
            }
            if pet.staying {
                v.push(Opt::new(&format!("come along, {name}!"), Act::Pet(P::Come)));
            }
            v.push(Opt::new("more…", Act::Pet(P::More)));
            let line = if pet.asleep() { format!("{name} is fast asleep, paws twitching") } else { format!("{name} {mood}") };
            (line, v)
        };
        self.open_pet_box(&name, &line, opts);
    }

    fn pet_menu_names(&mut self, page: u8, line: &str) {
        use story::{Act, Opt, PetAct as P};
        let n = pets::NAMES.len();
        let per = 4;
        let start = (page as usize * per) % n;
        let mut v: Vec<Opt> = (0..per).map(|k| (start + k) % n).map(|i| Opt::new(pets::NAMES[i], Act::Pet(P::Name(i as u8)))).collect();
        v.push(Opt::new("more names…", Act::Pet(P::Names(((page as usize + 1) % n.div_ceil(per)) as u8))));
        self.open_pet_box("a name", line, v);
    }

    /// A line about the pet, in the dialogue box (no answers to pick).
    fn pet_say(&mut self, line: &str) {
        let mut c = story::Convo::menu(PET_ID, "", line, vec![]);
        c.pos = self.convo_pos;
        c.ended_at = Some(self.now());
        self.convo = Some(c);
    }

    fn open_pet_box(&mut self, title: &str, line: &str, opts: Vec<story::Opt>) {
        let mut c = story::Convo::menu(PET_ID, title, line, opts);
        c.pos = self.convo_pos;
        self.convo = Some(c);
    }

    /// The pet's menu answered.
    fn pet_act(&mut self, a: story::PetAct) {
        use story::PetAct as P;
        let t = self.now();
        let Some(pet) = &mut self.my_pet else { return };
        let kind = pet.kind;
        let say = |s: &mut Self, line: &str| {
            let mut c = story::Convo::menu(PET_ID, "", line, vec![]);
            c.pos = s.convo_pos;
            c.ended_at = Some(s.now());
            s.convo = Some(c);
        };
        let sound = match kind {
            pets::PetKind::Dog => Sfx::Bark,
            pets::PetKind::Cat => Sfx::Meow,
            pets::PetKind::Bunny => Sfx::Pet,
        };
        let k = if self.inside.is_some() { 2.0 } else { sprite_k(self.h) };
        let you = self.player.as_ref().map(|p| (p.centre(), p.y, p.dir)).unwrap_or((pet.x, pet.y, 1.0));
        match a {
            P::Play => return self.pet_menu(2),
            P::More => return self.pet_menu(3),
            P::Feed => {
                // the bowl goes down just in front of the villager
                let line = pet.feed(you.0 + you.2 * 4.0 * k, you.1, t);
                self.convo = None;
                self.pet_say(&line);
                return;
            }
            P::Cuddle => {
                let line = pet.cuddle(t);
                let x = pet.x;
                self.convo = None;
                self.pet_say(&line);
                match self.inside.as_mut() {
                    Some(inn) => inn.director.sound(sound, x, 0.6),
                    None => self.director.sound(sound, x, 0.6),
                }
                return;
            }
            P::Nap => {
                let line = pet.nap(t);
                self.convo = None;
                self.pet_say(&line);
                return;
            }
            P::Wake => {
                pet.wake(t);
                pet.happy_until = t + 2.0;
                let line = format!("{} blinks awake, stretches… and wags", pet.name);
                say(self, &line);
            }
            P::Brush => {
                let line = pet.brush(t);
                self.convo = None;
                self.pet_say(&line);
                return;
            }
            P::Throw(i) => {
                let toy = [pets::ToyKind::Yarn, pets::ToyKind::Ball, pets::ToyKind::Stick, pets::ToyKind::Carrot][i as usize % 4];
                pet.throw(toy, you.0, you.1, you.2, t, k);
                let name = pet.name.clone();
                self.convo = None;
                let line = match toy {
                    pets::ToyKind::Yarn => format!("you toss the yarn — {name}'s eyes go huge"),
                    pets::ToyKind::Ball => format!("you throw the ball! {name} is already gone"),
                    pets::ToyKind::Stick => format!("a good stick! {name} watches it fly, quivering"),
                    pets::ToyKind::Carrot => format!("you roll a carrot across the floor. {name}'s nose twitches"),
                };
                self.pet_say(&line);
                return;
            }
            P::Trick => {
                let line = pet.trick(t);
                self.convo = None;
                self.pet_say(&line);
                self.cheer(1);
                return;
            }
            P::Zoomies => {
                pet.zoomies(you.0, t);
                let name = pet.name.clone();
                self.convo = None;
                let line = match kind {
                    pets::PetKind::Dog => format!("{name} tears round and round you, ears flapping"),
                    pets::PetKind::Cat => format!("{name} gets the zoomies — and then sits down like nothing happened"),
                    pets::PetKind::Bunny => format!("{name} races you in a happy little loop. {name} wins"),
                };
                self.pet_say(&line);
                return;
            }
            P::Tease => {
                pet.wake(t);
                pet.happy_until = t + 2.5;
                let name = pet.name.clone();
                let lines = [
                    format!("{name} stalks the scarf end… wiggle… POUNCE!"),
                    format!("{name} bats at the tassel with both paws, very serious about it"),
                    format!("{name} catches the scarf and won't give it back. it's theirs now"),
                ];
                let line = lines[(t * 7.0) as usize % 3].clone();
                self.convo = None;
                self.pet_say(&line);
                return;
            }
            P::Fuss => {
                pet.happy_until = t + 3.0;
                pet.wake(t);
                let (x, y) = (pet.x, pet.y);
                let name = pet.name.clone();
                match self.inside.as_mut() {
                    Some(inn) => {
                        inn.director.emote(x, y * 1.0 - 6.0, '♥', Rgb::hex(0xff6b8a), 4);
                        inn.director.sound(sound, x, 0.8);
                    }
                    None => {
                        self.director.emote(x, y - 3.0, '♥', Rgb::hex(0xff6b8a), 4);
                        self.director.sound(sound, x, 0.8);
                    }
                }
                let pick = (t * 13.0) as usize % 3;
                let line = match kind {
                    pets::PetKind::Dog => [format!("who's a good dog? {name} is. {name} absolutely is"), format!("{name} leans their whole weight on your legs. bliss"), format!("{name} rolls over for belly rubs. obviously")][pick].clone(),
                    pets::PetKind::Cat => [format!("{name} purrs like a little engine"), format!("{name} headbutts your hand. more, please"), format!("{name} allows exactly three strokes, then walks off. fair")][pick].clone(),
                    pets::PetKind::Bunny => [format!("{name}'s nose wiggles happily"), format!("{name} flattens out like a pancake, very content"), format!("{name} grinds their teeth softly — that's a bunny purr")][pick].clone(),
                };
                say(self, &line);
            }
            P::Come => {
                pet.staying = false;
                let line = format!("{} bounds over — ready for an adventure!", pet.name);
                say(self, &line);
            }
            P::Stay => {
                pet.staying = true;
                let line = format!("{} curls up by the fire. they'll be here when you get back", pet.name);
                say(self, &line);
            }
            P::Names(page) => self.pet_menu(10 + page),
            P::Name(i) => {
                let name = pets::NAMES[i as usize % pets::NAMES.len()].to_string();
                pet.name = name.clone();
                pet.happy_until = t + 3.0;
                self.pet_changed();
                self.cheer(1);
                say(self, &format!("{name}! …they love it ✦"));
            }
            P::Look => self.pet_menu(1),
            P::Kind(k) => {
                let kind = [pets::PetKind::Dog, pets::PetKind::Cat, pets::PetKind::Bunny][k as usize % 3];
                let cfg = crate::config::PetCfg { kind: kind.name().into(), coat: kind.coats()[0].0.into(), name: pet.name.clone() };
                pet.restyle(&cfg);
                self.pet_changed();
                self.pet_menu(1);
            }
            P::Coat => {
                let coats = kind.coats();
                let i = coats.iter().position(|c| c.0 == pet.coat).unwrap_or(0);
                let cfg = crate::config::PetCfg { kind: kind.name().into(), coat: coats[(i + 1) % coats.len()].0.into(), name: pet.name.clone() };
                pet.restyle(&cfg);
                self.pet_changed();
                self.pet_menu(1);
            }
            P::Done => self.convo = None,
        }
    }

    /// What poking at something indoors led to.
    fn indoors(&mut self, o: interior::Outcome) {
        use interior::Outcome as O;
        let t = self.now();
        match o {
            O::None => {}
            O::Leave => self.leave(),
            O::Talk(i) => self.start_host_convo(i),
            O::Look(l) => {
                self.look = Some(l);
                self.lean = 0.0;
                self.convo = None;
            }
            O::Gift(item) => {
                self.pockets.add(item);
                self.cheer(1);
            }
            O::Gifts(item, n) => {
                for _ in 0..n {
                    self.pockets.add(item);
                }
                self.cheer(1);
            }
            O::Cozy => self.pockets.cozy_until = self.pockets.cozy_until.max(t + 30.0),
            O::Cheer(n) => self.cheer(n),
        }
    }

    /// Talk with someone in their own home (index `i` in the household).
    fn start_host_convo(&mut self, i: usize) {
        let t = self.now();
        let ctx = self.talk_ctx(t);
        let seed = self.rng.next_u32();
        let Some(inn) = self.inside.as_mut() else { return };
        let Some(info) = inn.about(i) else { return };
        let id = HOST_ID + (inn.house as u64) * 16 + i as u64;
        let visits = self.talks.get(&id).copied().unwrap_or(0);
        *self.talks.entry(id).or_insert(0) += 1;
        // a little job they might ask for
        let mut tasks = vec![interior::Task::Star, interior::Task::Log, interior::Task::Stocking];
        if info.pet.is_some() {
            tasks.push(interior::Task::FindPet);
        }
        if info.has_piano {
            tasks.push(interior::Task::Tune);
        }
        let task = if info.church || inn.task.is_some() { None } else { Some(tasks[(seed as usize) % tasks.len()]) };
        let h = story::HostCtx { theme: info.theme, family: info.family.clone(), pet: info.pet.clone(), years: info.years, church: info.church, piano: info.has_piano, oven: info.has_oven, task };
        let mut c = story::Convo::host(id, info.name, info.kid, info.temper, visits, &ctx, &mut self.story_talk, h, seed);
        c.line = story::by_name(&c.line, &self.you_name, visits == 0, info.kid, seed);
        c.pos = self.convo_pos;
        inn.hold_talk(i, t);
        if let Some((hx, hy)) = inn.head_of(i) {
            inn.director.say(hx, hy, &c.line, t, 3.4, 40);
            inn.director.sound(Sfx::Greet, hx as f32, 0.7);
        }
        self.convo = Some(c);
    }

    /// Open the dialogue box with someone.
    fn start_convo(&mut self, id: u64) {
        let t = self.now();
        let Some(px) = self.player.as_ref().map(|p| p.centre()) else { return };
        let Some((cx, cy)) = self.critters.position(id) else { return };
        let first = !self.pockets.greeted.contains(&id);
        if first {
            self.pockets.greeted.push(id);
            self.cheer(1);
        }
        let ctx = self.talk_ctx(t);
        let name = self.critters.label(id);
        let kid = self.critters.is_kid(id);
        let aware = self.aware_of(id, cx, kid);
        let visits = self.talks.get(&id).copied().unwrap_or(0);
        *self.talks.entry(id).or_insert(0) += 1;
        let mut c = story::Convo::open(id, &name, kid, visits, &ctx, &mut self.story_talk, cx, self.rng.next_u32(), aware);
        c.line = story::by_name(&c.line, &self.you_name, visits == 0, kid, self.rng.next_u32());
        c.pos = self.convo_pos;
        self.director.say(cx.round() as i32, cy as i32 - 4, &c.line, t, 3.0, 40);
        if let Some(p) = &self.player {
            let intro = format!("hello! I'm {}", self.you_name);
            let hi = if first && !self.you_name.is_empty() { intro.as_str() } else if first { "hello there!" } else { "hi again!" };
            self.director.say(p.centre().round() as i32, p.y as i32 - 4, hi, t - 0.01, 1.2, 0);
        }
        self.critters.hold_talk(id, px, t);
        self.director.sound(Sfx::Greet, cx, 0.7);
        self.show_pockets = false;
        self.show_stats = false;
        self.convo = Some(c);
    }

    /// Answer `n` (1-based) in the conversation.
    fn answer(&mut self, n: usize) {
        let t = self.now();
        let ctx = self.talk_ctx(t);
        let cocoa = self.pockets.count(shop::Item::Cocoa) > 0;
        let Some(c) = &mut self.convo else { return };
        let Some((mine, theirs, act)) = c.choose(n, t, &ctx, &self.story_talk, cocoa) else { return };
        let (id, ended, steps) = (c.id, c.ended_at.is_some(), c.steps);
        if let story::Act::Pet(a) = act {
            let _ = (mine, theirs);
            self.pet_act(a);
            return;
        }
        if id >= HOST_ID {
            // at someone's home: bubbles in the room
            let i = ((id - HOST_ID) % 16) as usize;
            if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
                inn.director.say(p.centre().round() as i32, p.y as i32 - 9, &mine, t, 2.0, 40);
                if let Some((hx, hy)) = inn.head_of(i) {
                    inn.director.say(hx, hy, &theirs, t + 1.2, 3.4, 40);
                }
                inn.hold_talk(i, t + 2.0);
                match act {
                    story::Act::Sit => {
                        inn.sit_down(p);
                    }
                    story::Act::Cookie => {
                        self.pockets.add(shop::Item::Gingerbread);
                        inn.director.sound(Sfx::Munch, p.centre(), 0.6);
                    }
                    story::Act::PlayTune => {
                        if let Some((hx, hy)) = inn.head_of(i) {
                            inn.director.notes(hx as f32, hy as f32, 12);
                            inn.director.sound(Sfx::Chime, hx as f32, 0.8);
                        }
                    }
                    story::Act::Task(task) => {
                        inn.task = Some((task, i));
                        match task {
                            interior::Task::FindPet => inn.hide_pet(),
                            interior::Task::Star => inn.drop_star(),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            if ended && steps >= 2 {
                self.pockets.chats += 1;
                self.cheer(1);
            }
            return;
        }
        if let Some(p) = &self.player {
            self.director.say(p.centre().round() as i32, p.y as i32 - 4, &mine, t, 2.0, 40);
        }
        if let Some((cx, cy)) = self.critters.position(id) {
            self.director.say(cx.round() as i32, cy as i32 - 4, &theirs, t + 1.2, 3.2, 40);
        }
        match act {
            story::Act::Invite => {
                if let Some((x, y)) = self.critters.position(id) {
                    self.focus = Some(Focus::Critter(id, x as i32, y as i32 - 4 - self.director.lift));
                }
                if !self.critters.following(id) {
                    if let Some(p) = &mut self.player {
                        self.critters.invite(id, &mut p.berries, t, &mut self.director, &mut self.lines);
                    }
                }
            }
            story::Act::Snowball => {
                self.critters.challenge(id, t);
                self.director.sound(Sfx::Laugh, self.critters.centre_of(id).unwrap_or(0.0), 0.8);
            }
            story::Act::GiveCocoa => {
                if self.pockets.take(shop::Item::Cocoa) {
                    self.critters.gift(id, "cocoa", t, &mut self.director);
                    self.pockets.gifts += 1;
                    self.cheer(2);
                }
            }
            story::Act::HelpBuild => {
                if self.builds.help(id, &mut self.director) {
                    self.cheer(1);
                }
            }
            _ => {}
        }
        if ended && steps >= 2 {
            self.pockets.chats += 1;
            self.cheer(1);
        }
    }

    /// A click on the dialogue box (true if it was on it): on an answer
    /// picks it; anywhere else on the box picks the box up to move it.
    pub fn convo_click(&mut self, x: i32, y: i32) -> bool {
        let (w, h) = (self.w as i32, self.h as i32);
        let Some(c) = &self.convo else { return false };
        if !c.contains(x, y, w, h) {
            return false;
        }
        if let Some(n) = c.option_at(x, y, w, h) {
            self.answer(n);
        } else {
            let (x0, y0) = c.origin(w, h);
            self.convo_drag = Some((x - x0, y - y0));
        }
        true
    }

    pub fn convo_dragging(&self) -> bool {
        self.convo_drag.is_some() && self.convo.is_some()
    }

    /// Moving the dialogue box (it stays put for the next conversation too).
    pub fn convo_drag_to(&mut self, x: i32, y: i32) {
        let (Some((ox, oy)), Some(c)) = (self.convo_drag, &mut self.convo) else { return };
        let (w, h) = (self.w as i32, self.h as i32);
        c.pos = Some((x - ox, y - oy));
        // stored clamped, so it never drifts off screen
        let at = c.origin(w, h);
        c.pos = Some(at);
        self.convo_pos = Some(at);
    }

    pub fn convo_drop(&mut self) {
        self.convo_drag = None;
    }

    /// Whether (x, y) is on the dialogue box but not on an answer (it can be
    /// picked up there).
    pub fn convo_grip(&self, x: i32, y: i32) -> bool {
        let (w, h) = (self.w as i32, self.h as i32);
        self.convo.as_ref().is_some_and(|c| c.contains(x, y, w, h) && c.option_at(x, y, w, h).is_none())
    }

    /// Note something the villager did at column `x` (people nearby saw).
    fn deed(&mut self, d: Deed, x: f32) {
        let t = self.now();
        self.deeds.retain(|e| t - e.2 < 120.0);
        self.deeds.push((d, x, t));
        if self.deeds.len() > 24 {
            self.deeds.remove(0);
        }
    }

    /// What someone at column `x` knows about that's worth mentioning:
    /// things that happened to them first, then what they're busy with,
    /// then what they saw the villager do (most recent first).
    fn aware_of(&self, id: u64, x: f32, kid: bool) -> Vec<story::Aware> {
        use story::Aware as A;
        let t = self.now();
        let recent: Vec<Deed> = self.deeds.iter().rev().filter(|e| t - e.2 < 90.0).map(|e| e.0).collect();
        let seen: Vec<Deed> = self.deeds.iter().rev().filter(|e| t - e.2 < 90.0 && (e.1 - x).abs() < 50.0).map(|e| e.0).collect();
        let mut v = vec![];
        let add = |a: A, v: &mut Vec<A>| {
            if !v.contains(&a) {
                v.push(a);
            }
        };
        for d in &recent {
            match *d {
                Deed::Carried(who) if who == id => add(A::WasCarried, &mut v),
                Deed::Hit(who) if who == id => add(A::GotHit, &mut v),
                Deed::Gift(who) if who == id => add(A::GotGift, &mut v),
                Deed::SantaChat(who) if who == id => add(A::TalkedToSanta, &mut v),
                _ => {}
            }
        }
        if let Some((k, n)) = self.critters.pet_of(id) {
            add(A::HasPet(k, n), &mut v);
        }
        if kid {
            if let Some(igloo) = self.builds.working(id) {
                add(A::Building { igloo }, &mut v);
            }
            if self.critters.in_fight(id) {
                add(A::Fighting, &mut v);
            }
            if let Some(igloo) = self.builds.made_by(id, x) {
                add(A::Built { igloo }, &mut v);
            }
        }
        for d in &seen {
            let a = match *d {
                Deed::Fed(w) => A::SawFeed(w),
                Deed::Pet(w) => A::SawPet(w),
                Deed::Threw => A::SawSnowball,
                Deed::Hit(who) if who != id => A::SawSnowball,
                Deed::Gift(who) if who != id => A::SawGift,
                Deed::Carried(who) if who != id => A::SawCarry,
                Deed::DropIn => A::SawDropIn,
                Deed::Visit => A::SawVisit,
                _ => continue,
            };
            add(a, &mut v);
        }
        if !v.iter().any(|a| matches!(a, A::Building { .. } | A::Built { .. })) {
            if let Some(igloo) = self.builds.near(x) {
                add(A::SeeBuild { igloo }, &mut v);
            }
        }
        v.truncate(3);
        v
    }

    /// The pointer's over the dialogue box (which answer, for the highlight).
    pub fn convo_hover(&mut self, x: i32, y: i32) -> bool {
        let (w, h) = (self.w as i32, self.h as i32);
        match &self.convo {
            Some(c) if c.contains(x, y, w, h) => {
                self.convo_hover = c.option_at(x, y, w, h);
                true
            }
            _ => {
                self.convo_hover = None;
                false
            }
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
        let had = shop_open || self.show_pockets || self.show_stats || self.convo.is_some() || self.look.is_some();
        self.convo = None;
        self.look = None;
        self.show_pockets = false;
        self.show_stats = false;
        had
    }

    /// Q: the next thing to hold out (berries, then whatever's in the
    /// pockets, then empty hands).
    pub fn cycle_held(&mut self) {
        let t = self.now();
        let Some(p) = self.player.as_ref() else { return };
        let (px, py) = (p.centre(), p.y);
        let mut all: Vec<Held> = vec![];
        if p.berries > 0 {
            all.push(Held::Berries);
        }
        all.extend(shop::ITEMS.iter().filter(|&&i| self.pockets.count(i) > 0).map(|&i| Held::Item(i)));
        let next = match self.held.and_then(|h| all.iter().position(|&a| a == h)) {
            Some(i) if i + 1 < all.len() => Some(all[i + 1]),
            Some(_) => None,
            None => all.first().copied(),
        };
        self.held = next;
        let line = match next {
            None if all.is_empty() => "nothing in my pockets yet".to_string(),
            None => "hands in pockets".to_string(),
            Some(Held::Berries) => format!("berries at the ready ({})", self.player.as_ref().map(|p| p.berries).unwrap_or(0)),
            Some(Held::Item(i)) => format!("{} in hand ({})", i.name(), self.pockets.count(i)),
        };
        if self.inside.is_none() {
            self.director.say(px.round() as i32, py as i32 - 4, &line, t, 1.6, 6);
        } else if let Some(inn) = self.inside.as_mut() {
            inn.director.say(px.round() as i32, py as i32 - 10, &line, t, 1.6, 40);
        }
        self.director.sound(Sfx::Rustle, px, 0.4);
    }

    /// 1-5: buy (at the open shop) or use an item.
    pub fn number(&mut self, n: usize) {
        if self.convo.as_ref().is_some_and(|c| c.ended_at.is_none()) {
            self.answer(n);
            return;
        }
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
                self.pockets.cocoa += 1;
                self.director.say(px.round() as i32, py as i32 - 4, "mmm, cocoa!", t, 2.0, 6);
                self.director.emote(px, py - 3.0, '♥', Rgb::hex(0xff6b8a), 2);
                self.director.sound(Sfx::Munch, px, 0.6);
            }
            shop::Item::Gingerbread | shop::Item::CandyCane => match target.filter(|&id| self.critters.is_person(id)) {
                Some(id) => {
                    self.pockets.take(item);
                    self.critters.gift(id, item.name(), t, &mut self.director);
                    self.pockets.gifts += 1;
                    self.cheer(1);
                    self.deed(Deed::Gift(id), px);
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
                    self.pockets.fed += 1;
                    self.cheer(1);
                    let word = animal_word(&self.critters.label(id));
                    self.deed(Deed::Fed(word), px);
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
            self.next_round -= s;
            if let Some(n) = &mut self.newyear {
                n.rebase(s);
            }
            self.builds.rebase(s);
            if let Some(r) = &mut self.rounds {
                r.rebase(s);
            }
            if let Some(p) = &mut self.player {
                p.rebase(s);
            }
            self.zap_reset -= s;
            self.blackout_until -= s;
            self.last_train -= s;
            self.last_show -= s;
            self.last_stars -= s;
            self.last_sleigh -= s;
            self.high.rebase(s);
            self.skylife.rebase(s);
            if let Some(pet) = &mut self.my_pet {
                pet.rebase(s);
            }
            self.land.rebase(s);
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
        self.view_cam().round() as i32
    }

    /// How much the view is magnified on its own: rooms are drawn at
    /// their designed size and scaled up to fill a taller screen.
    pub fn view_zoom(&self) -> f32 {
        if self.inside.is_some() && self.look.is_none() {
            room_scale(self.h)
        } else {
            1.0
        }
    }

    /// The part of the drawing there is to see (cells): a room only
    /// fills its designed size.
    pub fn view_bounds(&self) -> (f32, f32) {
        match (&self.inside, &self.look) {
            (Some(_), None) => {
                let (w, h) = room_size(self.w, self.h);
                (w as f32, h as f32)
            }
            _ => (self.w as f32, self.h as f32),
        }
    }

    /// Where a zoomed view centres (cells, as drawn): on the villager when
    /// they're about, else on the village.
    pub fn zoom_focus(&self) -> (f32, f32) {
        let (bw, bh) = self.view_bounds();
        let Some(p) = &self.player else { return (bw * 0.5, bh * 0.72) };
        if let (Some(inn), None) = (&self.inside, &self.look) {
            return (p.centre(), p.y - 2.0 + inn.view_shift(self.now()));
        }
        if self.inside.is_some() {
            return (bw * 0.5, bh * 0.6);
        }
        let x = p.centre() - self.cam_i() as f32;
        let y = p.y - 2.0 + self.oy_i() as f32;
        if x < 0.0 || x > bw || y < 0.0 || y > bh {
            return (bw * 0.5, bh * 0.72);
        }
        (x, y)
    }

    /// The camera when looking out of a window indoors: (world column at
    /// the left of the screen, rows lifted into the sky). Where the villager
    /// stands in the room, and leaning (A/D), shift the view a little.
    fn look_view(&self) -> Option<(f32, f32)> {
        let look = self.look?;
        let inn = self.inside.as_ref()?;
        let w = self.w as f32;
        let px = self.player.as_ref().map(|p| p.centre()).unwrap_or(w / 2.0);
        let shift = (px - w / 2.0) * 0.12 + self.lean;
        let cam = (inn.outside.0 + look.dx * w + shift - w * 0.5).clamp(0.0, (self.ww - self.w) as f32);
        Some((cam, look.lift.min(self.high.depth as f32)))
    }

    /// The camera the scene is drawn with (out of a window, or the usual).
    fn view_cam(&self) -> f32 {
        self.look_view().map(|v| v.0).unwrap_or(self.cam)
    }

    /// How many rows the view is lifted into the sky.
    fn oy_i(&self) -> i32 {
        if let Some((_, lift)) = self.look_view() {
            return lift.round() as i32;
        }
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
    pub fn set_live(&mut self, walk_speed: f32, drive_left: bool, most_at_once: u32) {
        self.director.drive_left = drive_left;
        self.critters.set_max(most_at_once as usize);
        if let Some(p) = &mut self.player {
            p.speed = walk_speed;
        }
    }

    /// Shift held: the villager runs.
    pub fn set_sprint(&mut self, on: bool) {
        if let Some(p) = &mut self.player {
            p.sprint = on;
        }
    }

    /// The villager's name (empty: "neighbour"), for what people call them.
    pub fn set_name(&mut self, name: &str) {
        if self.you_name != name {
            self.you_name = name.to_string();
        }
    }

    /// What people call the villager.
    pub fn your_name(&self) -> &str {
        if self.you_name.is_empty() {
            "neighbour"
        } else {
            &self.you_name
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

    #[cfg(test)]
    pub fn player_y(&self) -> Option<f32> {
        self.player.as_ref().map(|p| p.y)
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
        // facing the pet, close by: the pet (before people, bushes, doors and
        // things — it's always about, so turning to it is how to say hello)
        let k = sprite_k(self.h);
        if let Some(pet) = self.my_pet.as_ref().filter(|pet| !pet.staying && pet.near(px, py, k) && (pet.x - px) * p.dir > 0.0) {
            return Some(Focus::Pet(pet.x.round() as i32, pet.y.round() as i32 - 3 - self.director.lift));
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
        // nothing else in reach: the pet, at their heels
        if let Some(pet) = self.my_pet.as_ref().filter(|pet| !pet.staying && pet.near(px, py, sprite_k(self.h))) {
            return Some(Focus::Pet(pet.x.round() as i32, pet.y.round() as i32 - 3 - self.director.lift));
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
        if self.look.take().is_some() {
            return;
        }
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
            if !p.free(t) || inn.is_climbing() {
                return;
            }
            p.active_at = t;
            // in a conversation, E picks the first answer
            if self.convo.as_ref().is_some_and(|c| c.ended_at.is_none()) {
                self.answer(1);
                return;
            }
            let pet_near = self.my_pet.as_ref().is_some_and(|pet| (!pet.staying || inn.home) && pet.near(p.centre(), p.y, 2.0));
            let pet_faced = pet_near && self.my_pet.as_ref().is_some_and(|pet| (pet.x - p.centre()) * p.dir > 0.0);
            match inn.nearest(p.centre()) {
                // facing the pet: the pet comes first
                _ if pet_faced => self.pet_menu(0),
                Some((what, ..)) => {
                    let o = inn.poke(what, p, t);
                    self.indoors(o);
                }
                None if pet_near => self.pet_menu(0),
                None => {}
            }
            return;
        }
        if let Some(Focus::Door(i, ..)) = self.focus {
            self.enter(i);
            return;
        }
        if let Some(Focus::Pet(..)) = self.focus {
            if self.convo.as_ref().is_some_and(|c| c.id == PET_ID && c.ended_at.is_none()) {
                self.answer(1);
            } else if let Some(Held::Item(item)) = self.held.filter(|h| matches!(h, Held::Item(shop::Item::Carrot | shop::Item::Gingerbread | shop::Item::CandyCane))) {
                // a treat for the pet
                self.pockets.take(item);
                if self.pockets.count(item) == 0 {
                    self.held = None;
                }
                if let Some(pet) = &mut self.my_pet {
                    pet.wake(t);
                    pet.happy_until = t + 3.0;
                    let n = if pet.name.is_empty() { format!("your {}", pet.kind.name()) } else { pet.name.clone() };
                    let line = match (pet.kind, item) {
                        (pets::PetKind::Bunny, shop::Item::Carrot) => format!("{n} takes the carrot very seriously. *crunch crunch crunch*"),
                        (_, shop::Item::Carrot) => format!("{n} sniffs the carrot, licks it… and eats it anyway"),
                        (pets::PetKind::Cat, _) => format!("{n} inspects the treat, then nibbles a corner, very dignified"),
                        _ => format!("{n} snaps it up and wags for more!"),
                    };
                    self.director.emote(pet.x, pet.y - 3.0, '♥', Rgb::hex(0xff6b8a), 2);
                    self.director.sound(Sfx::Munch, pet.x, 0.6);
                    self.pet_say(&line);
                }
            } else {
                self.pet_menu(0);
            }
            return;
        }
        // something in hand: given to whoever's in front, or used
        if let Some(Held::Item(item)) = self.held {
            let talking = self.convo.as_ref().is_some_and(|c| c.ended_at.is_none());
            if !talking && matches!(self.focus, Some(Focus::Critter(..)) | None) {
                let n = shop::ITEMS.iter().position(|&i| i == item).unwrap_or(0) + 1;
                self.number(n);
                if self.pockets.count(item) == 0 {
                    self.held = None;
                }
                return;
            }
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
                // people: a conversation, story-game style (E picks the first answer)
                if self.critters.is_person(id) {
                    if self.convo.as_ref().is_some_and(|c| c.id == id && c.ended_at.is_none()) {
                        self.answer(1);
                    } else {
                        self.start_convo(id);
                    }
                    return;
                }
                let berries = p.berries;
                let known = self.pockets.greeted.contains(&id);
                let earned = self.critters.meet(id, px, py, &mut p.berries, t, &mut self.director, &mut self.lines);
                let fed = p.berries < berries;
                // cheer: a first hello, a chat run its course, or feeding an animal
                if self.critters.is_person(id) {
                    if !known {
                        self.pockets.greeted.push(id);
                    } else if earned > 0 {
                        self.pockets.chats += 1;
                    }
                }
                if fed {
                    self.pockets.fed += 1;
                }
                if self.critters.is_animal(id) {
                    let word = animal_word(&self.critters.label(id));
                    let x = self.critters.centre_of(id).unwrap_or(px);
                    self.deed(if fed { Deed::Fed(word) } else { Deed::Pet(word) }, x);
                }
                self.cheer(if fed { 2 } else { earned });
            }
            Some(Focus::Shop(..)) => {
                if let Some(s) = &mut self.shop {
                    s.open = !s.open && !s.closed;
                    if s.closed {
                        let (kx, ky) = (s.centre(), s.base - 4);
                        self.director.say(kx, ky, "closed for Christmas Eve — see you in the morning!", t, 3.0, 6);
                    }
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
                    self.pockets.berries += n;
                    let msg = format!("{} ({} ●)", self.lines.pick(), p.berries);
                    self.director.say(px.round() as i32, py as i32 - 4, &msg, t, 2.6, 6);
                } else {
                    self.director.say(px.round() as i32, py as i32 - 4, "no ripe berries yet…", t, 2.4, 6);
                }
            }
            Some(Focus::Pet(..)) => {}
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
        let (rw, rh) = room_size(self.w, self.h);
        let inn = interior::Interior::new(i, h.church, self.home == Some(i), h.wall, rw, rh, seed, (p.x, p.y), t);
        let (ex, ey) = inn.entrance();
        (p.x, p.y, p.dir) = (ex, ey, 1.0);
        let home = inn.home;
        if let Some(pet) = &mut self.my_pet {
            if !pet.staying {
                (pet.x, pet.y) = (ex + 9.0, ey + 1.0);
            } else if home {
                // waiting at home: comes running
                (pet.x, pet.y) = (rw as f32 * 0.4, ey + 2.0);
                pet.happy_until = t + 3.0;
            }
        }
        p.snowball = false;
        self.director.sound(Sfx::Click, h.door() as f32, 0.9);
        self.inside = Some(Box::new(inn));
        if !self.pockets.houses.contains(&i) {
            self.pockets.houses.push(i);
        }
    }

    /// Back out into the snow, on the doorstep.
    pub fn leave(&mut self) {
        self.look = None;
        self.convo = None;
        let Some(inn) = self.inside.take() else { return };
        if let Some(p) = &mut self.player {
            (p.x, p.y) = inn.outside;
            p.dir = 1.0;
            self.director.sound(Sfx::Click, p.centre(), 0.9);
            if let Some(pet) = self.my_pet.as_mut().filter(|pet| !pet.staying) {
                (pet.x, pet.y) = (p.centre() - 4.0, p.y);
            }
        }
        self.deed(Deed::Visit, inn.outside.0);
    }

    /// Previews: talking with the host (indoors), or the pet's menu.
    pub fn preview_ui(&mut self, what: &str) {
        match what {
            "host" => self.indoors(interior::Outcome::Talk(0)),
            "pet" => self.pet_menu(0),
            _ => {}
        }
    }

    /// Previews: the pet fed / cuddled / napping / brushed.
    pub fn preview_pet(&mut self, what: &str) {
        use story::PetAct as P;
        let a = match what {
            "feed" => P::Feed,
            "cuddle" => P::Cuddle,
            "nap" => P::Nap,
            "brush" => P::Brush,
            _ => return,
        };
        self.pet_act(a);
    }

    /// Previews: people out with their pets near the player.
    pub fn preview_pets(&mut self) {
        let t = self.now();
        let Some(px) = self.player.as_ref().map(|p| p.centre()) else { return };
        for k in 0..9 {
            self.critters.spawn_demo_pet(k % 3, k / 3 + k, px - 90.0 + k as f32 * 10.0, t);
        }
    }

    /// Previews: chatting with someone (`kid`), a couple of answers in.
    pub fn preview_talk(&mut self, kid: bool) {
        let t = self.now();
        let Some((px, py)) = self.player.as_ref().map(|p| (p.centre(), p.y)) else { return };
        let kind = if kid { events::Kind::Kid } else { events::Kind::Walker };
        self.critters.spawn_demo(kind, px + 6.0, t);
        let Some(id) = self.critters.people().iter().filter(|p| (p.1 - px - 6.0).abs() < 3.0).map(|p| p.0).next() else { return };
        let _ = py;
        // something for them to have noticed
        self.deed(Deed::Fed("bunny"), px + 2.0);
        self.start_convo(id);
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
        // 900: the villager's own home
        let i = if i == 900 {
            self.home.unwrap_or(0)
        } else if i < self.village.houses.len() {
            i
        } else {
            self.village.houses.iter().position(|h| h.church).unwrap_or(0)
        };
        self.enter(i);
        let t = self.now();
        if let Some(p) = &mut self.player {
            p.active_at = t;
            p.x = room_size(self.w, self.h).0 as f32 * 0.36;
        }
    }

    /// Previews: in another room of the home (0 parlour .. 3 attic), and
    /// optionally looking out of its first window (or the telescope).
    pub fn preview_room(&mut self, room: usize, look: Option<u8>) {
        let t = self.now();
        let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) else { return };
        inn.preview_room(room, p);
        if let Some(k) = look {
            let what = if k == 1 { interior::Thing::Telescope } else { interior::Thing::Window(0) };
            let o = inn.poke(what, p, t);
            self.indoors(o);
        }
    }

    /// Previews: press E on the room's stairs (or the bed: `what` 1, the
    /// sofa: 2, the oven: 3) — the climb (etc.) then plays on.
    pub fn preview_poke(&mut self, what: u8) {
        // 4: throw the pet its toy; 5: a trick
        if what >= 4 {
            let toy = match self.my_pet.as_ref().map(|p| p.kind) {
                Some(pets::PetKind::Cat) => 0,
                Some(pets::PetKind::Bunny) => 3,
                _ => 1,
            };
            self.pet_act(if what == 4 { story::PetAct::Throw(toy) } else { story::PetAct::Trick });
            return;
        }
        let t = self.now();
        let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) else { return };
        let want = |k: &interior::Thing| match what {
            1 => matches!(k, interior::Thing::Bed),
            2 => matches!(k, interior::Thing::Seat(_)),
            3 => matches!(k, interior::Thing::Oven),
            _ => matches!(k, interior::Thing::Stairs(_)),
        };
        if let Some((k, x)) = inn.things_at().into_iter().find(|(k, _)| want(k)) {
            p.x = x as f32 - 4.0;
            let o = inn.poke(k, p, t);
            self.indoors(o);
        }
    }

    #[cfg(test)]
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

    /// How far the sky has drifted with the view (a slow parallax: far
    /// slower than the mountains).
    fn sky_off(&self) -> i32 {
        (self.view_cam() * 0.08).round() as i32
    }

    fn on_moon(&self, x: i32, y: i32) -> bool {
        let (mx, my, r) = self.sky.moon;
        let mx = mx - self.sky_off() as f32;
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
        let carried = self.critters.held_info().filter(|&(id, moved)| moved && self.critters.is_person(id)).map(|(id, _)| id);
        self.critters.release(t, &mut self.director, &mut self.lines);
        if let Some(id) = carried {
            let x = self.critters.centre_of(id).unwrap_or(0.0);
            self.deed(Deed::Carried(id), x);
        }
    }

    /// Let go of whoever's held without dropping or poking them.
    pub fn let_go(&mut self) {
        let t = self.now();
        if let Some(p) = self.player.as_mut().filter(|p| p.held.is_some()) {
            p.release();
            return;
        }
        self.critters.let_go(t);
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
            self.pockets.thrown += 1;
            self.deed(Deed::Threw, x as f32);
        } else {
            p.scoop_until = t + 0.6;
            p.snowball = true;
            self.director.sound(Sfx::Crunch, p.centre(), 0.5);
        }
    }

    /// Is there something under this cell that reacts to a click?
    pub fn clickable(&self, x: i32, y: i32) -> bool {
        if let Some(c) = &self.convo {
            if c.option_at(x, y, self.w as i32, self.h as i32).is_some() {
                return true;
            }
        }
        if let Some(inn) = &self.inside {
            return inn.thing_at(x, y).is_some();
        }
        let y = y - self.oy_i();
        if y < 0 {
            return self.high.clickable(x, y);
        }
        let (sx, x) = (x, x + self.cam_i());
        if self.critters.bush_at(x, y).is_some() || self.critters.pet_at(x, y).is_some() {
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
            || self.sky_thing_at(sx, y, x).is_some()
    }

    /// Something in the sky or on the move at screen column `sx` / world
    /// column `x`, row `y`: Santa's sleigh, a shooting star, the plane, the
    /// owl, a flock, a mountain cabin, a train or a car.
    fn sky_thing_at(&self, sx: i32, y: i32, x: i32) -> Option<Thing> {
        let t = self.now();
        if self.sleigh.hit(sx, y, t) {
            return Some(Thing::Sleigh);
        }
        if let Some(i) = self.director.star_at(sx, y) {
            return Some(Thing::Star(i));
        }
        if let Some(k) = self.skylife.flyer_at(sx + self.sky_off(), y) {
            return Some(Thing::Flyer(k));
        }
        if let Some(i) = self.land.cabin_at(sx, y, self.cam) {
            return Some(Thing::Cabin(i));
        }
        if self.director.train_at(x, y) {
            return Some(Thing::Train);
        }
        self.director.car_at(x, y, &self.ctx).map(Thing::Car)
    }

    /// Click on a moving / far-off thing. True if there was one.
    fn poke_sky_thing(&mut self, sx: i32, y: i32, x: i32) -> bool {
        let Some(thing) = self.sky_thing_at(sx, y, x) else { return false };
        let t = self.now();
        let cam = self.cam;
        match thing {
            Thing::Sleigh => self.santa_clicked(t),
            Thing::Star(i) => {
                // caught one: a burst of sparkles, and a wish
                let wx = sx as f32 + cam;
                self.director.catch_star(i, wx);
                let msg = self.lines.wish();
                self.director.say(wx.round() as i32, y - 1, &format!("caught a falling star! {msg}"), t + 0.2, 3.0, 6);
                self.director.happenings.push((dialogue::Ev::ShootingStar, Some(wx)));
                self.cheer(1);
            }
            Thing::Flyer(k) => {
                let off = self.sky_off() as f32;
                if let Some((fx, fy)) = self.skylife.poke(k, t) {
                    let wx = fx - off + cam;
                    match k {
                        0 => self.director.sound(Sfx::Plane, wx, 0.9),
                        1 => {
                            self.director.sound(Sfx::Hoot, wx, 0.9);
                            self.director.say(wx.round() as i32, fy as i32 - 1, "hoo? hoo-hoo!", t, 1.8, 6);
                        }
                        _ => {
                            self.director.sound(Sfx::Bird, wx, 0.9);
                            self.director.emote(wx, fy - 1.0, '♪', Rgb::hex(0xd8e0f0), 3);
                        }
                    }
                }
            }
            Thing::Cabin(i) => {
                if let Some((cx, cy)) = self.land.light_cabin(i, cam, t) {
                    let wx = cx + cam;
                    self.director.sound(Sfx::Chime, wx, 0.35);
                    self.director.emote(wx, cy - 1.0, '✦', Rgb::hex(0xffb050), 3);
                }
            }
            Thing::Train => {
                self.director.toot(x as f32, y as f32, t);
                self.last_train = t;
            }
            Thing::Car(i) => self.director.honk(i, t),
        }
        true
    }

    /// What's going on right now, as far as conversation goes.
    fn talk_ctx(&self, t: f32) -> dialogue::TalkCtx {
        let d = self.daylight;
        let part = if d.sun < 0.15 {
            dialogue::Part::Night
        } else if d.arc < 0.3 {
            dialogue::Part::Morning
        } else if d.arc > 0.72 || (d.warm > 0.5 && d.arc > 0.5) {
            dialogue::Part::Evening
        } else {
            dialogue::Part::Day
        };
        let days = {
            let today = crate::clock::now().date_naive();
            let mut xmas = chrono::NaiveDate::from_ymd_opt(today.year(), 12, 25).unwrap_or(today);
            if today > xmas {
                xmas = chrono::NaiveDate::from_ymd_opt(today.year() + 1, 12, 25).unwrap_or(today);
            }
            (xmas - today).num_days()
        };
        dialogue::TalkCtx {
            part,
            snowy: self.director.blizzard.active(t) || self.director.gust_wind(t).abs() > 8.0,
            train: t - self.last_train < 90.0,
            show: t - self.last_show < 120.0,
            blackout: t < self.blackout_until + 60.0,
            sleigh: t - self.last_sleigh < 60.0,
            stars: t - self.last_stars < 45.0,
            days,
            tree: self.trees.iter().find(|tr| tr.is_big()).map(|tr| tr.cx as f32),
            church: self.village.houses.iter().find(|h| h.church).map(|h| h.door() as f32),
            shop: self.shop.as_ref().map(|s| s.centre() as f32),
            station: self.ctx.station.map(|(x, w)| (x + w / 2) as f32),
        }
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
        self.director.happenings.push((dialogue::Ev::Blackout, Some(xf)));
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

    }

    /// Something in the village at world (x, y) — a present, the snowman, a
    /// tree, a lamp, a house — reacts as if clicked. Returns whether
    /// anything was there.
    fn poke(&mut self, x: i32, y: i32) -> bool {
        let t = self.now();
        if let Some((kx, ky)) = self.shop.as_ref().filter(|s| s.contains(x, y)).map(|s| (s.centre(), s.base - 4)) {
            let merry = format!("merry Christmas, {}!", self.your_name());
            let lines = ["hot cocoa, gingerbread, candy canes!", "come by the counter for a cocoa!", merry.as_str(), "warm your hands by the window"];
            let line = lines[self.rng.next_u32() as usize % lines.len()].to_string();
            let line = line.as_str();
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
        if self.convo_click(x, y) {
            return;
        }
        if self.look.take().is_some() {
            return;
        }
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
            if self.my_pet.as_ref().is_some_and(|pet| (!pet.staying || inn.home) && pet.hit(x, y, 2.0)) {
                self.pet_menu(0);
                return;
            }
            if let Some(what) = inn.thing_at(x, y) {
                let o = inn.poke(what, p, t);
                self.indoors(o);
            }
            return;
        }
        let y = y - self.oy_i();
        if y < 0 {
            // up in the high sky
            let cam = self.cam;
            let cheer = self.high.click(x, y, t, &mut self.director, cam);
            self.pockets.stars += cheer;
            self.cheer(cheer);
            return;
        }
        let (sx, x) = (x, x + self.cam_i());
        if self.my_pet.as_ref().is_some_and(|pet| !pet.staying && pet.hit(x, y, sprite_k(self.h))) {
            let t = self.now();
            if let Some(pet) = &mut self.my_pet {
                pet.wake(t);
                pet.happy_until = t + 1.5;
            }
            self.pet_menu(0);
            return;
        }
        if let Some(owner) = self.critters.pet_at(x, y) {
            let t = self.now();
            self.critters.pet_the_pet(owner, t, &mut self.director);
            return;
        }
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
        if self.poke_sky_thing(sx, y, x) {
            return;
        }
        let sky_off = self.sky_off() as f32;
        let d = &mut self.director;
        if moon_hit {
            self.sleigh.trigger(t);
            let (mx, my, r) = self.sky.moon;
            // the moon is on screen (drifted a little); bubbles live in the world
            let mx = mx - sky_off + (x - sx) as f32;
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

    /// The sleigh was clicked: sometimes Santa pulls up for a word with
    /// whoever's below, other times it's just a hearty "ho ho ho".
    fn santa_clicked(&mut self, t: f32) {
        let Some((sx, sy)) = self.sleigh.santa_at(t) else { return };
        let cam = self.cam;
        let wx = sx + cam;
        let d = &mut self.director;
        d.sound(Sfx::Magic, wx, 1.0);
        d.burst(wx, sy, 10, &[Rgb::hex(0xffd36b), Rgb::WHITE], 3.0, 1.5);
        // someone down below, in view, to talk to (nearest under the sleigh)
        let (l, r) = (cam, cam + self.w as f32);
        let mut folk: Vec<_> = self.critters.people().into_iter().filter(|p| p.1 > l + 2.0 && p.1 < r - 2.0).collect();
        folk.sort_by(|a, b| (a.1 - wx).abs().total_cmp(&(b.1 - wx).abs()));
        if self.sleigh.halted(t) || folk.is_empty() || self.rng.chance(0.45) {
            let line = *self.rng.pick(santa::HO_HO);
            self.director.say(wx.round() as i32, sy as i32 - 2, line, t, 2.6, 8);
            self.sleigh.halt(t + 1.2);
            self.pockets.santa += 1;
            return;
        }
        let days = self.talk_ctx(t).days;
        let (_, px, py, kid) = folk[0];
        let lines = santa::chat(&mut self.rng, kid, days);
        let (sxi, syi) = (wx.round() as i32, sy as i32 - 2);
        let (pxi, pyi) = (px.round() as i32, py as i32 - 4);
        // the sleigh needs a moment to pull up before Santa starts
        let t0 = t + 1.6;
        let d = &mut self.director;
        d.say(sxi, syi, &lines[0], t0, 2.6, 8);
        d.say(pxi, pyi, &lines[1], t0 + 2.6, 2.8, 8);
        d.say(sxi, syi, &lines[2], t0 + 5.4, 3.0, 8);
        d.say(sxi, syi, &lines[3], t0 + 8.6, 2.4, 8);
        // a wave back from someone else, if there's anyone
        let (wxi, wyi) = folk.get(1).map(|o| (o.1.round() as i32, o.2 as i32 - 4)).unwrap_or((pxi, pyi));
        d.say(wxi, wyi, &lines[4], t0 + 9.4, 2.2, 8);
        d.sound(Sfx::Greet, px, 0.6);
        self.sleigh.halt(t0 + 9.2);
        self.pockets.santa += 1;
        self.cheer(2);
        self.deed(Deed::SantaChat(folk[0].0), px);
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
        // every easter egg gets a little fanfare (and the village notices)
        d.sound(Sfx::Fanfare, CENTER, 1.0);
        d.happenings.push((dialogue::Ev::Magic, None));
        self.typed.clear();
    }


    pub fn update(&mut self, dt: f32, _app_t: f32) {
        self.tick(dt);
        let t = self.now();
        self.eve_night(t);
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
        let festive = matches!(special_now(), Special::EveNight | Special::NewYear);
        let moon_row = self.sky.moon.1 / 2.0;
        self.skylife.update(dt, t, self.snow.wind, self.daylight.sun, festive, (self.cam, self.w as f32), moon_row);
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
        // walking somewhere on their own (from the right-click menu); any
        // direction key takes over again
        if self.mv != (0.0, 0.0) {
            self.goal = None;
        }
        let mv = self.steer(t).unwrap_or(self.mv);
        let before = self.player.as_ref().map(|p| (p.x, p.y));
        let mut walked_out = false;
        // looking out of a window: A/D lean to see along the street, W/S
        // step back
        let mv = if self.look.is_some() {
            self.lean = (self.lean + mv.0 * 24.0 * dt).clamp(-40.0, 40.0);
            if mv.1 != 0.0 {
                self.look = None;
            }
            (0.0, 0.0)
        } else {
            mv
        };
        if let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) {
            walked_out = inn.update(dt, t, mv, p);
        } else if let Some(p) = &mut self.player {
            let (g, h) = (&self.ctx.ground_row, self.h as f32);
            let lo = |x: f32| (g[(x.max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32;
            p.update(dt, t, mv, lo, self.ww as f32, h, &mut self.director);
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
        // the pet out and about: after an animal now and then (a dog chases,
        // a cat stalks, a bunny hops along with rabbits), or playing with
        // another pet
        if let (Some(pet), Some(p), None) = (self.my_pet.as_mut(), self.player.as_ref(), &self.inside) {
            if let Some(id) = self.pet_chasing {
                match self.critters.pos_of(id) {
                    Some((x, _)) if pet.busy.is_some() => pet.chasing(x),
                    _ => self.pet_chasing = None,
                }
            }
            if pet.idle() && !pet.napping && t > pet.social_at && !p.moving {
                pet.social_at = t + 6.0;
                if let Some((owner, vx, _, _)) = self.critters.vpet_near(pet.x, pet.y, 10.0) {
                    pet.play_with(vx, t);
                    self.critters.vpet_play(owner, t);
                    pet.social_at = t + 40.0;
                } else if let Some((id, what, ax, _)) = self.critters.animal_near(pet.x, pet.y, 16.0) {
                    match (pet.kind, what) {
                        (pets::PetKind::Bunny, "rabbit") => {
                            pet.play_with(ax, t);
                            pet.social_at = t + 40.0;
                        }
                        (pets::PetKind::Bunny, _) => {}
                        _ => {
                            pet.chase(ax, t);
                            if pet.kind == pets::PetKind::Dog {
                                self.critters.startle(id, pet.x, t, &mut self.director);
                            }
                            self.pet_chasing = Some(id);
                            pet.social_at = t + 35.0;
                        }
                    }
                }
            }
            // the cat's pounce at the end of the stalk startles its quarry
            if let (Some(id), Some(pets::Busy::Chase { until, stalk: true, .. })) = (self.pet_chasing, pet.busy) {
                if until - t < 0.9 {
                    self.critters.startle(id, pet.x, t, &mut self.director);
                    self.pet_chasing = None;
                }
            }
        }
        // the pet keeps up (or pootles about at home)
        if let (Some(pet), Some(p)) = (self.my_pet.as_mut(), self.player.as_ref()) {
            match &self.inside {
                None if !pet.staying => {
                    let g = &self.ctx.ground_row;
                    let lo = (g[(p.centre().max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32;
                    pet.update(dt, t, p.centre(), p.y, p.dir, p.moving, lo, self.h as f32 - 1.0, sprite_k(self.h));
                }
                Some(inn) if inn.is_climbing() => {
                    // it comes along, and turns up beside them
                    (pet.x, pet.y) = (p.centre() + 6.0 * -p.dir, p.y);
                }
                Some(inn) if !pet.staying || inn.home => {
                    let (lo, hi) = inn.walk_rows();
                    match inn.bed_foot().filter(|_| inn.is_lying()) {
                        // curled up at the foot of the bed
                        Some((bx, by)) => {
                            pet.update(dt, t, bx, by, -1.0, false, lo.min(by), hi, 2.0);
                            if (pet.x - bx).abs() < 3.0 {
                                pet.napping = true;
                            }
                        }
                        None => pet.update(dt, t, p.centre(), p.y, p.dir, p.moving, lo, hi, 2.0),
                    }
                }
                _ => {}
            }
        }
        // what's in hand (gone once it runs out)
        let held = match self.held {
            Some(Held::Berries) if self.player.as_ref().is_some_and(|p| p.berries == 0) => None,
            Some(Held::Item(i)) if self.pockets.count(i) == 0 => None,
            h => h,
        };
        self.held = held;
        if let Some(p) = &mut self.player {
            p.hold = match held {
                Some(Held::Berries) => Some(('⁘', Rgb::hex(0xd8202a))),
                Some(Held::Item(i)) if i != shop::Item::Lantern => Some(i.icon()),
                _ => None,
            };
        }
        // what the pet got up to: hearts, lines, barks
        if let Some(pet) = &mut self.my_pet {
            for ev in pet.take_events() {
                let d = match self.inside.as_mut() {
                    Some(inn) => &mut inn.director,
                    None => &mut self.director,
                };
                match ev {
                    pets::PetEv::Emote(ch, c, x, y) => d.emote(x, y, ch, c, 1),
                    pets::PetEv::Say(line, x, y) => d.say(x.round() as i32, y.round() as i32, &line, t, 3.4, 40),
                    pets::PetEv::Sound(s, x) => d.sound(s, x, 0.6),
                }
            }
        }
        if walked_out {
            self.leave();
        }
        // stats: how far the villager's walked, and whether they've been to
        // both ends of the village
        if let (Some((bx, by)), Some(p), None) = (before, &self.player, &self.inside) {
            self.pockets.steps += ((p.x - bx).powi(2) + (p.y - by).powi(2)).sqrt();
            if p.x < 30.0 {
                self.pockets.ends.0 = true;
            }
            if p.x > self.ww as f32 - 34.0 {
                self.pockets.ends.1 = true;
            }
        }
        // the Christmas list
        for i in self.pockets.newly_done() {
            let (goal, reward) = shop::GOALS[i];
            self.cheer(reward);
            if let Some(p) = &self.player {
                let line = format!("✓ Christmas list: {goal}!");
                self.director.say(p.centre().round() as i32, p.y as i32 - 4, &line, t + 0.3, 3.5, 6);
                self.director.sound(Sfx::Fanfare, p.centre(), 0.5);
            }
        }
        // indoors, anyone following the player waits by the door
        let you = match (&self.inside, &self.player) {
            (Some(inn), Some(p)) => Some((inn.outside.0, inn.outside.1, p.dir, false)),
            (None, Some(p)) => Some((p.x, p.y, p.dir, p.moving)),
            _ => None,
        };
        self.critters.set_player(you);
        self.director.update(dt, t, &self.ctx);
        // what's going on, for what people say (and who reacts to what)
        let arrivals = std::mem::take(&mut self.director.arrivals);
        if !arrivals.is_empty() {
            self.last_train = t;
            self.director.happenings.push((dialogue::Ev::Train, arrivals.first().copied()));
        }
        let flying = self.sleigh.flying();
        if flying && !self.sleigh_was {
            self.director.happenings.push((dialogue::Ev::Sleigh, None));
        }
        if flying {
            self.last_sleigh = t;
        }
        self.sleigh_was = flying;
        if self.director.show.active(t) {
            self.last_show = t;
        }
        for (ev, x) in std::mem::take(&mut self.director.happenings) {
            if matches!(ev, dialogue::Ev::ShootingStar | dialogue::Ev::Comet | dialogue::Ev::Fireworks) {
                self.last_stars = t;
            }
            if self.inside.is_none() {
                self.critters.react(ev, x, t, &mut self.director);
            }
        }
        let tctx = self.talk_ctx(t);
        self.critters.set_talk(tctx);
        for x in arrivals {
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
        for id in self.critters.take_your_hit_ids() {
            let x = self.critters.centre_of(id).unwrap_or(0.0);
            self.deed(Deed::Hit(id), x);
        }
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
                    self.pockets.shows += 1;
                }
            }
        }
        if let Some(until) = self.director.gather.take() {
            // a few more folk come out for the show, then everyone gathers
            for _ in 0..3 {
                self.critters.request(events::Kind::Walker, t);
            }
            let tree = self.trees.iter().find(|tr| tr.is_big()).map(|tr| tr.cx as f32).unwrap_or(self.ww as f32 / 2.0);
            let line = if special_now() == Special::NewYear { "countdown by the big tree — come on!".to_string() } else { self.lines.show_soon() };
            self.critters.gather(tree, until, t, &mut self.director, &line);
        }
        for k in std::mem::take(&mut self.director.spawn_requests) {
            self.critters.request(k, t);
        }
        // the cursor matters to the animals for a while after it last moved
        let cam = self.cam;
        let cur = self.cursor.filter(|_| t - self.cursor_seen < 15.0).map(|(x, y)| (x + cam, y - lift, self.cursor_vel.0, self.cursor_vel.1));
        let snowman = self.village.snowmen.first().map(village::snowman_head);
        self.critters.update(dt, t, &self.ctx, &mut self.director, &mut self.lines, cur, snowman);
        self.update_builds(dt, t);
        // speech bubbles follow their speakers (the player included)
        let mut who = self.critters.anchors();
        if let (None, Some(p)) = (&self.inside, &self.player) {
            who.push((u64::MAX - 1, p.centre(), p.y));
        }
        if let Some((x, y)) = self.sleigh.santa_at(t) {
            who.push((u64::MAX - 2, x + self.cam, y));
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
        // the conversation: they stay put while it lasts; walking off (or
        // a moment after goodbye) ends it
        if let Some(c) = self.convo.as_ref().filter(|c| c.id == PET_ID) {
            if c.ended_at.is_some_and(|e| t - e > 3.0) {
                self.convo = None;
            }
        } else if let Some(c) = self.convo.as_ref().filter(|c| c.id >= HOST_ID) {
            let (id, ended) = (c.id, c.ended_at);
            let i = ((id - HOST_ID) % 16) as usize;
            let gone = match (self.inside.as_mut(), self.player.as_ref()) {
                (Some(inn), Some(p)) => {
                    let near = inn.head_of(i).is_some_and(|(hx, _)| (hx as f32 - p.centre()).abs() < 40.0);
                    if near && ended.is_none() {
                        inn.hold_talk(i, t);
                    }
                    !near
                }
                _ => true,
            };
            if gone || ended.is_some_and(|e| t - e > 2.5) {
                self.convo = None;
            }
        } else if let Some(c) = &self.convo {
            let (id, ended) = (c.id, c.ended_at);
            let px = self.player.as_ref().map(|p| p.centre());
            let far = match (px, self.critters.centre_of(id)) {
                (Some(px), Some(cx)) => (px - cx).abs() > 16.0,
                _ => true,
            };
            if far || self.inside.is_some() || ended.is_some_and(|e| t - e > 2.5) {
                self.convo = None;
            } else if let Some(px) = px {
                if ended.is_none() {
                    self.critters.hold_talk(id, px, t);
                }
            }
        }
        // passers-by sometimes remark on what's going on
        if self.rng.chance(dt * 0.04 * events::chattiness()) {
            self.critters.chatter(t, &mut self.director);
        }
        if self.power_back && t >= self.blackout_until {
            self.power_back = false;
            self.director.happenings.push((dialogue::Ev::PowerBack, None));
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
        self.santa_rounds(dt, t);
        self.new_year(dt, t);
        if self.cfg.snow > 0.0 {
            let fresh = t - self.cursor_seen < 2.5;
            let cur = self.cursor.filter(|_| fresh).map(|(x, y)| (x, y * 2.0, self.cursor_vel.0, self.cursor_vel.1 * 2.0));
            self.snow.update(dt, t, self.director.gust_wind(t), cur);
        }
        self.village.update(dt, self.snow.wind);
        self.sleigh.hold = self.rounds.is_some();
        self.sleigh.update(dt, t);
    }

    /// Santa's rounds: on December nights now and then he drops in on a
    /// house or two near the view; on Christmas Eve he works his way round
    /// the whole village, roof by roof, all night. Every delivery puts
    /// another present under the big tree.
    fn santa_rounds(&mut self, dt: f32, t: f32) {
        let special = special_now();
        let now = crate::clock::now();
        let december_night = now.month() == 12 && now.day() < 24 && self.daylight.sun < 0.12;
        let eve = special == Special::EveNight;
        if self.rounds.is_none() && !self.sleigh.flying() && t > self.next_round && (eve || december_night) && self.inside.is_none() {
            let mut stops: Vec<santa::Stop> = self
                .village
                .houses
                .iter()
                .enumerate()
                .filter_map(|(i, h)| h.chimney_top().map(|(x, y)| santa::Stop { house: i, chimney: (x, (y / 2.0).floor()) }))
                .collect();
            stops.sort_by(|a, b| a.chimney.0.total_cmp(&b.chimney.0));
            let (l, w) = (self.cam, self.w as f32);
            if !stops.is_empty() {
                let stops = if eve {
                    // the whole village: from the house nearest the middle of
                    // the view, working right, then back round from the left
                    let mid = l + w * 0.5;
                    let k = stops.iter().position(|s| s.chimney.0 >= mid).unwrap_or(0);
                    stops.rotate_left(k);
                    stops
                } else {
                    // one or two houses in view
                    let seen: Vec<_> = stops.iter().copied().filter(|s| s.chimney.0 > l + 8.0 && s.chimney.0 < l + w - 30.0).collect();
                    if seen.is_empty() {
                        self.next_round = t + 30.0;
                        vec![]
                    } else {
                        let k = self.rng.int(0, seen.len() as i32) as usize;
                        seen.into_iter().skip(k).take(self.rng.int(1, 3) as usize).collect()
                    }
                };
                if !stops.is_empty() {
                    let from = l + w + 6.0;
                    self.rounds = Some(santa::Rounds::new(stops, from, self.rng.next_u32()));
                    self.director.happenings.push((dialogue::Ev::Sleigh, None));
                    self.director.sound(Sfx::Magic, from, 0.8);
                }
            }
        }
        let Some(r) = &mut self.rounds else { return };
        r.update(dt, t, &mut self.director);
        self.last_sleigh = t;
        let delivered = std::mem::take(&mut r.delivered);
        let done = r.done();
        let team = r.team();
        for i in delivered {
            self.present_from_santa(t);
            // the reindeer have the carrot left out for them
            if let Some(h) = self.village.houses.get_mut(i).filter(|h| h.carrot) {
                h.carrot = false;
                let dx = h.door() as f32 + 2.0;
                let by = h.base as f32;
                self.director.burst(dx, by - 0.5, 6, &[Rgb::hex(0xff8a2a), Rgb::hex(0xffd36b)], 2.0, 1.0);
                self.director.emote(team.0, team.1 - 2.0, '♥', Rgb::hex(0xff6b8a), 2);
                self.director.sound(Sfx::Munch, team.0, 0.35);
            }
        }
        if done {
            self.rounds = None;
            self.next_round = t + if eve { self.rng.range(25.0, 50.0) } else { self.rng.range(360.0, 840.0) };
        }
    }

    /// Christmas Eve: the village goes to bed (dark windows, no traffic, the
    /// cabin shut), and carrots go out on the steps for the reindeer —
    /// set out by the kids through the afternoon, the rest by bedtime.
    fn eve_night(&mut self, t: f32) {
        let special = special_now();
        let eve = special == Special::EveNight;
        self.critters.set_asleep(eve, t, &mut self.director);
        self.director.quiet = eve;
        if let Some(s) = &mut self.shop {
            s.closed = eve;
            if eve {
                s.open = false;
            }
        }
        let now = crate::clock::now();
        let afternoon = now.month() == 12 && now.day() == 24 && now.hour() >= 12 && !eve;
        let fresh = !self.village.houses.iter().any(|h| h.asleep) && eve;
        // Christmas morning: everyone's up at once, the bells ring out and
        // the kids come running to see what Santa left
        let woke = !eve && self.village.houses.iter().any(|h| h.asleep);
        if woke {
            self.director.light_show(t + 2.0, 20.0);
            if let Some((cx, _)) = self.ctx.church {
                self.director.sound(Sfx::ChurchPeal, cx as f32, 1.0);
            }
            for k in 0..6 {
                let kind = if k % 2 == 0 { events::Kind::Kid } else { events::Kind::Walker };
                self.critters.request(kind, t);
            }
        }
        for h in &mut self.village.houses {
            h.asleep = eve && !h.church;
            if fresh && !h.church && h.chimney_top().is_some() {
                h.carrot = true;
            }
            if special == Special::XmasDay || (!eve && !afternoon) {
                h.carrot = false;
            }
        }
        if afternoon {
            // a kid passing a door leaves a carrot on the step
            let kids: Vec<(f32, f32)> = self.critters.people().into_iter().filter(|p| p.3).map(|p| (p.1, p.2)).collect();
            for (kx, ky) in kids {
                if let Some(h) = self.village.houses.iter_mut().find(|h| !h.church && !h.carrot && h.chimney_top().is_some() && (h.door() as f32 - kx).abs() < 1.5) {
                    h.carrot = true;
                    let line = *self.rng.pick(&["a carrot for the reindeer!", "Rudolph gets the biggest one", "for Dasher! and Dancer!", "don't eat it before tonight!"]);
                    self.director.say(kx.round() as i32, ky as i32 - 4, line, t, 2.6, 6);
                    self.director.sound(Sfx::Drop, kx, 0.4);
                }
            }
        }
    }

    /// Kids building snowmen and igloos on the open snow.
    fn update_builds(&mut self, dt: f32, t: f32) {
        if self.inside.is_some() {
            return;
        }
        // what's already standing on the snowfield: (columns, bottom row)
        let mut taken: Vec<(i32, i32, i32)> = self.trees.iter().map(|tr| (tr.cx - tr.hw_base as i32 - 3, tr.cx + tr.hw_base as i32 + 3, tr.base())).collect();
        if let Some(tr) = self.trees.iter().find(|tr| tr.is_big()) {
            // the presents (and on New Year's Eve, the mast)
            let right = if special_now() == Special::NewYear { 22 } else { 12 };
            taken.push((tr.cx - tr.hw_base as i32 - 12, tr.cx + tr.hw_base as i32 + right, tr.base() + 2));
        }
        taken.extend(self.village.snowmen.iter().map(|s| (s.cx - 10, s.cx + 10, s.base)));
        taken.extend(self.village.lamps.iter().map(|l| (l.x - 2, l.x + 3, l.base)));
        taken.extend(self.critters.bushes.iter().map(|b| (b.x - 2, b.x + 12, b.base)));
        if let Some(s) = &self.shop {
            taken.push((s.x - 3, s.x + shop::SHOP_W + 3, s.base));
        }
        // in the way: overlapping columns and not well behind or in front
        let free = |x0: i32, x1: i32, row: i32| !taken.iter().any(|&(a, b, r)| x1 > a && x0 < b && (r - row).abs() < 8);

        let g = &self.ctx.ground_row;
        let h = self.h as f32;
        let lo = |x: f32| (g[(x.max(0.0) as usize).min(g.len() - 1)] + 4).min(self.h as i32 - 2) as f32;
        let can = special_now() != Special::EveNight && self.cfg.critters > 0.0;
        self.builds.update(dt, t, can, (self.cam, self.w as f32), &free, &lo, h - 1.0, &mut self.critters, &mut self.director);
    }

    /// New Year's Eve: the mast goes up by the big tree for the evening.
    fn new_year(&mut self, dt: f32, t: f32) {
        let now = crate::clock::now();
        let left = newyear::to_midnight(&now);
        if special_now() != Special::NewYear || self.inside.is_some() {
            if special_now() != Special::NewYear {
                self.newyear = None;
            }
            return;
        }
        let Some(left) = left else { return };
        if self.newyear.is_none() {
            let Some(tree) = self.trees.iter().find(|tr| tr.is_big()) else { return };
            let mx = tree.cx + tree.hw_base as i32 + 16;
            self.newyear = Some(newyear::NewYear::new((mx, tree.top + 3, tree.base()), self.rng.next_u32()));
        }
        let people: Vec<(f32, f32, bool)> = self.critters.people().into_iter().map(|p| (p.1, p.2, p.3)).collect();
        let view = (self.cam, self.w as f32);
        let Some(ny) = &mut self.newyear else { return };
        if ny.update(dt, t, left, &people, view, &mut self.director) {
            self.director.gather = Some(t + left + 150.0);
        }
    }

    /// A new present appears under the big tree (from Santa).
    fn present_from_santa(&mut self, t: f32) {
        let Some(tree) = self.trees.iter().find(|tr| tr.is_big()) else { return };
        if self.presents.len() > 90 {
            return;
        }
        let (cx, base, spread) = (tree.cx, tree.base(), tree.hw_base as i32 + 6);
        let u = (self.h as f32 / 75.0).clamp(0.45, 1.3);
        let (w, h) = (((self.rng.int(5, 11) as f32 * u).round() as i32).max(4), ((self.rng.int(2, 4) as f32 * u).round() as i32).max(2));
        let x = cx + self.rng.int(-spread, spread) - w / 2;
        let b = base + self.rng.int(0, 4);
        let mut p = Present::new(x, b, w, h, &mut self.rng);
        p.jiggle_until = t + 1.2;
        let (px, py) = p.top_center();
        self.presents.push(p);
        // front ones drawn last
        self.presents.sort_by_key(|p| p.base_row());
        self.director.burst(px as f32, py as f32, 12, &[Rgb::hex(0xffd36b), Rgb::WHITE, Rgb::hex(0x7fe0a0)], 3.0, 2.0);
        self.director.sound(Sfx::Present, px as f32, 0.5);
    }

    #[cfg(test)]
    pub fn draw(&self, f: &mut Frame, app_t: f32) {
        self.draw_world(f, app_t);
        self.draw_overlay(f);
    }

    /// Draw everything, the scene magnified by `zoom` (times a room's own
    /// scale indoors) around `focus` (cells; `None`: the usual spot).
    /// Returns (z, x0, y0): screen cell (x, y) shows (x0 + x/z, y0 + y/z).
    pub fn render(&self, f: &mut Frame, spare: &mut Frame, app_t: f32, zoom: f32, focus: Option<(f32, f32)>) -> (f32, f32, f32) {
        let z = zoom.clamp(1.0, 4.0) * self.view_zoom();
        // out in the village, zoomed in: people are drawn after the zoom
        let outdoors = self.inside.is_none() || self.look.is_some();
        self.zoomed.set(z > 1.001 && outdoors);
        self.draw_world(f, app_t);
        let mut xf = (1.0, 0.0, 0.0);
        if z > 1.001 {
            let (fx, fy) = focus.unwrap_or_else(|| self.zoom_focus());
            let (bw, bh) = self.view_bounds();
            let (x0, y0) = f.zoom(z, fx, fy, (bw, bh), spare);
            xf = (z, x0, y0);
            if self.zoomed.get() {
                self.draw_people(f, View { z, x0, y0, cam: self.cam_i() as f32, oy: self.oy_i() as f32 });
            }
        }
        self.zoomed.set(false);
        self.draw_overlay(f);
        xf
    }

    /// Zoomed in: people, animals and pets drawn straight into the zoomed
    /// frame at the bigger size (crisp pixel art, not magnified glyphs).
    fn draw_people(&self, f: &mut Frame, v: View) {
        let t = self.now();
        let glow = self.cfg.glow;
        (f.ox, f.oy) = (0, 0);
        f.day = self.daylight.sun;
        self.critters.draw(f, t, glow, true, Some(v));
        if let (Some(p), None) = (&self.player, &self.inside) {
            let pet = self.my_pet.as_ref().filter(|pet| !pet.staying);
            // (in the villager's arms: drawn in front of them)
            if let Some(pet) = pet.filter(|pet| !pet.held()) {
                pet.draw(f, t, critters::pet_scale(sprite_k(self.h) * v.z), Some(v));
            }
            p.draw(f, t, glow, Some(v));
            if let Some(pet) = pet.filter(|pet| pet.held()) {
                pet.draw(f, t, critters::pet_scale(sprite_k(self.h) * v.z), Some(v));
            }
        }
        f.day = 0.0;
    }

    /// The panels over the scene (the dialogue box, pockets, the window
    /// frame when looking out): these never zoom.
    pub fn draw_overlay(&self, f: &mut Frame) {
        let t = self.now();
        f.ox = 0;
        f.oy = 0;
        if self.inside.is_some() && self.look.is_none() {
            if let Some(c) = &self.convo {
                c.draw(f, self.convo_hover);
            }
            return;
        }
        // looking out of a window: the room around the view
        if let (Some(inn), Some(look)) = (&self.inside, &self.look) {
            inn.draw_window_frame(f, look, t, self.cfg.glow);
        }
        if let Some(c) = &self.convo {
            c.draw(f, self.convo_hover);
        }
        if self.show_stats {
            shop::draw_stats(f, &self.pockets);
        } else if self.show_pockets {
            shop::draw_pockets(f, &self.pockets);
        }
        // in hand (Q to change)
        if let (Some(h), Some(p)) = (self.held, &self.player) {
            if t - p.active_at < 20.0 {
                let (ch, col, label) = match h {
                    Held::Berries => ('⁘', Rgb::hex(0xd8202a), format!("berries ×{}", p.berries)),
                    Held::Item(i) => (i.icon().0, i.icon().1, format!("{} ×{}", i.name(), self.pockets.count(i))),
                };
                let bg = crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.85);
                let y = f.h as i32 - 2;
                let x = f.text(2, y, " in hand ", crate::color::pal::UI_DIM, bg, 0.0);
                f.put_g(x, y, crate::frame::Glyph::new(ch, col).bg(bg).lit(0.0));
                let x = f.text(x + 1, y, &format!(" {label} "), crate::color::pal::UI_TEXT, bg, 0.0);
                f.text(x, y, " Q ", Rgb::hex(0xffd36b), bg, 0.0);
            }
        }
    }

    /// The scene itself (what zooms).
    pub fn draw_world(&self, f: &mut Frame, _app_t: f32) {
        let t = self.now();
        let glow = self.cfg.glow;
        f.day = 0.0;
        f.day_warm = 0.0;
        if let (Some(inn), Some(p), None) = (&self.inside, &self.player, &self.look) {
            let pet_faced = self.my_pet.as_ref().filter(|pet| (!pet.staying || inn.home) && pet.near(p.centre(), p.y, 2.0) && (pet.x - p.centre()) * p.dir > 0.0);
            let prompt = (t - p.active_at < 20.0 && self.convo.is_none())
                .then(|| match pet_faced {
                    Some(pet) => Some((pet.x.round() as i32, pet.y.round() as i32 - 7)),
                    None => inn.nearest(p.centre()).map(|(_, x, y)| (x, y)),
                })
                .flatten();
            inn.draw(f, t, glow, p, prompt);
            f.ox = 0;
            f.oy = 0;
            if let Some(pet) = self.my_pet.as_ref().filter(|pet| (!pet.staying || inn.home) && !inn.is_climbing()) {
                pet.draw(f, t, 2, None);
            }
            return;
        }
        let cam = self.cam_i();
        let oy = self.oy_i();
        f.oy = oy;
        // pixel layer (sky things stay on screen, the world scrolls)
        f.ox = 0;
        self.sky.draw_px(f, t, self.director.flare.value(t), self.sky_off());
        let dl = self.daylight;
        if oy > 0 {
            f.specks = true;
            self.high.draw(f, t, glow, self.sky.top_color(), dl.sun, self.sky_off());
            f.specks = false;
        }
        daylight::sky_pass(f, &dl, -2 * oy, self.land.horizon as i32, glow);
        // clouds drift with the sky (the same slow parallax)
        f.ox = self.sky_off();
        let (mx, my, _) = self.sky.moon;
        self.skylife.draw_clouds(f, t, dl.sun, (mx, my));
        f.ox = 0;
        (f.day, f.day_warm) = (dl.sun, dl.warm);
        self.director.draw_px(f);
        let vc = self.view_cam();
        self.land.draw_px(f, vc);
        self.land.draw_lights(f, vc, t, glow);
        f.ox = cam;
        self.village.draw_px(f, vc);
        self.builds.draw_px(f, dl.sun);
        // glyph layer, back to front
        f.ox = 0;
        if dl.sun < 0.45 {
            // stars and the moon's glyphs (not in daylight)
            f.specks = true;
            self.sky.draw_glyphs(f, t, self.sky_off());
            f.specks = false;
        }
        f.ox = self.sky_off();
        self.skylife.draw_flyers(f, t, glow, dl.sun);
        f.ox = 0;
        self.sleigh.draw(f, t, glow);
        f.ox = cam;
        self.skylife.draw_lanterns(f, t, glow);
        self.village.draw_glyphs(f, t, glow);
        if let Some(r) = &self.rounds {
            r.draw(f, t, glow);
        }
        self.land.draw_glints(f, t);
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            f.specks = true;
            self.snow.draw_far(f);
            f.specks = false;
        }
        f.ox = cam;
        self.director.draw_back(f, &self.ctx, t);
        self.critters.draw_back(f, t);
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            f.specks = true;
            self.snow.draw_mid(f);
            f.specks = false;
        }
        f.ox = cam;
        let show = self.director.show.value(t);
        // Christmas Eve: the trees and presents breathe with a soft glow
        let eve = special_now() == Special::EveNight;
        for (i, tr) in self.trees.iter().enumerate() {
            let pulse = if eve { 1.0 + 0.5 * (0.5 + 0.5 * (t * 1.4 + i as f32 * 0.9).sin()) } else { 1.0 };
            tr.draw(f, t, glow * pulse, show);
        }
        self.builds.draw_glyphs(f, t, glow);
        for p in &self.presents {
            p.draw(f, t);
            if eve {
                let (x, y) = p.top_center();
                let k = 0.5 + 0.5 * (t * 1.8 + x as f32 * 0.3).sin();
                f.light_cell(x, y + 1, Rgb::hex(0xffc870) * (1.4 * k * glow));
            }
        }
        if let Some(s) = &self.shop {
            s.draw(f, t, glow);
        }
        // (zoomed in, the people come after the zoom, drawn at the bigger
        // size: see `draw_people`)
        let people = !self.zoomed.get();
        self.critters.draw(f, t, glow, people, None);
        if let (Some(p), None, true) = (&self.player, &self.inside, people) {
            let pet = self.my_pet.as_ref().filter(|pet| !pet.staying);
            if let Some(pet) = pet.filter(|pet| !pet.held()) {
                pet.draw(f, t, critters::pet_scale(sprite_k(self.h)), None);
            }
            p.draw(f, t, glow, None);
            if let Some(pet) = pet.filter(|pet| pet.held()) {
                pet.draw(f, t, critters::pet_scale(sprite_k(self.h)), None);
            }
        }
        let ny_left = newyear::to_midnight(&crate::clock::now()).unwrap_or(1e9);
        let ny_year = crate::clock::now().year() + (ny_left > 0.0) as i32;
        if let Some(ny) = &self.newyear {
            ny.draw(f, t, glow, ny_left, ny_year);
        }
        f.ox = 0;
        if self.cfg.snow > 0.0 {
            f.specks = true;
            self.snow.draw_near(f, t);
            f.specks = false;
        }
        f.ox = cam;
        self.director.draw_front(f, t, glow);
        self.critters.draw_top(f, t, glow);
        // what E would do, while the player is out walking
        if let (Some(p), Some(fc)) = (&self.player, self.focus) {
            if t - p.active_at < 20.0 {
                // over the person / bush; for village things, over the player's head
                let (x, y) = match fc {
                    Focus::Spot(..) => (p.centre().round() as i32, p.y.round() as i32 - 5 - self.director.lift),
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
        if let Some(ny) = &self.newyear {
            ny.draw_sky(f, t, ny_left, ny_year);
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

/// What the villager can hold out: berries (for the animals) or something
/// from the pockets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Held {
    Berries,
    Item(shop::Item),
}

/// A zoomed view, for drawing people and animals straight into it at the
/// bigger size (crisp, not magnified): screen cell = ((world column - cam -
/// x0) * z, (row + oy - y0) * z).
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub z: f32,
    pub x0: f32,
    pub y0: f32,
    pub cam: f32,
    pub oy: f32,
}

impl View {
    /// World column → screen column.
    pub fn x(&self, wx: f32) -> f32 {
        (wx - self.cam - self.x0) * self.z
    }
    /// World row (as a coordinate: a row's top edge) → screen row.
    pub fn y(&self, wy: f32) -> f32 {
        (wy + self.oy - self.y0) * self.z
    }
}

/// How much bigger people and animals are drawn on a screen `h` rows tall
/// (the scenery scales with the rows by itself): 1 up to ~94 rows, then in
/// quarter steps (2 at 150 rows).
pub fn sprite_k(h: usize) -> f32 {
    let k = h as f32 / 75.0;
    if k < 1.25 {
        1.0
    } else {
        (k * 4.0).round() / 4.0
    }
}

/// Rooms are designed for a screen this many rows tall.
pub const ROOM_ROWS: f32 = 75.0;

/// How many times bigger than designed a room is drawn on a screen `h`
/// rows tall (in 0.05 steps; never smaller than designed).
pub fn room_scale(h: usize) -> f32 {
    ((h as f32 / ROOM_ROWS * 20.0).round() / 20.0).max(1.0)
}

/// The size a room is built at, for a `w` x `h` screen.
pub fn room_size(w: usize, h: usize) -> (usize, usize) {
    let k = room_scale(h);
    (((w as f32 / k).round() as usize).max(1), ((h as f32 / k).round() as usize).max(1))
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

    /// The villager's pet follows them, asks for a name on first meeting
    /// (E with nothing else in front), and the name is handed to the app to
    /// keep; it comes indoors too.
    #[test]
    fn the_villagers_pet_follows_gets_named_and_comes_inside() {
        let mut s = scene();
        assert!(s.my_pet.is_some(), "no pet by default");
        s.focus = None;
        for _ in 0..20 {
            s.update(DT, 0.0);
        }
        // walk right a while: the pet keeps up
        s.set_move(1.0, 0.0);
        for _ in 0..(3.0 / DT) as usize {
            s.update(DT, 0.0);
        }
        s.set_move(0.0, 0.0);
        for _ in 0..(1.0 / DT) as usize {
            s.update(DT, 0.0);
        }
        let (px, petx) = (s.player.as_ref().unwrap().centre(), s.my_pet.as_ref().unwrap().x);
        assert!((px - petx).abs() < 8.0, "pet left behind: villager {px}, pet {petx}");
        // E with nothing else around: name them
        s.focus = Some(Focus::Pet(0, 0));
        s.interact();
        let c = s.convo.as_ref().expect("no naming menu");
        assert!(c.line.contains("call them"), "{}", c.line);
        s.answer(2);
        let name = s.my_pet.as_ref().unwrap().name.clone();
        assert!(!name.is_empty());
        assert_eq!(s.take_pet_change().map(|p| p.name), Some(name));
        // and in through a front door, the pet comes too
        let i = s.village.houses.iter().position(|h| !h.church).unwrap();
        s.enter(i);
        for _ in 0..30 {
            s.update(DT, 0.0);
        }
        let (px, petx) = (s.player.as_ref().unwrap().centre(), s.my_pet.as_ref().unwrap().x);
        assert!((px - petx).abs() < 30.0, "pet didn't come in: {px} vs {petx}");
    }

    /// The villager's home: rooms to walk between and stairs to climb, and
    /// windows to look out of (the camera moves outside).
    #[test]
    fn home_has_rooms_stairs_and_windows_to_look_out_of() {
        let mut s = scene();
        let home = s.home.expect("no home");
        s.enter(home);
        assert!(s.inside.as_ref().unwrap().home);
        assert_eq!(s.inside.as_ref().unwrap().kind(), interior::RoomKind::Parlour);
        // walk out the right side into the kitchen
        s.set_move(1.0, 0.0);
        for _ in 0..(30.0 / DT) as usize {
            s.update(DT, 0.0);
            if s.inside.as_ref().unwrap().kind() == interior::RoomKind::Kitchen {
                break;
            }
        }
        s.set_move(0.0, 0.0);
        assert_eq!(s.inside.as_ref().unwrap().kind(), interior::RoomKind::Kitchen);
        // up the stairs (walked, the view following), then the ladder to
        // the attic, and look out of its window
        let mut t = s.now();
        {
            let (inn, p) = (s.inside.as_mut().unwrap(), s.player.as_mut().unwrap());
            inn.preview_room(0, p);
            let o = inn.poke(interior::Thing::Stairs(true), p, t);
            assert_eq!(o, interior::Outcome::None);
            assert_eq!(inn.kind(), interior::RoomKind::Bedroom);
            assert!(inn.is_climbing());
            // the view starts on the parlour (the bedroom a screen above)
            // and pans up with the villager
            let h = inn.walk_rows().1 + 1.0;
            assert!((inn.view_shift(t) + h).abs() < 1.0, "{}", inn.view_shift(t));
            let mut last = inn.view_shift(t);
            let mut rows = vec![];
            while inn.is_climbing() {
                t += DT;
                inn.update(DT, t, (1.0, 0.0), p);
                let v = inn.view_shift(t);
                assert!(v >= last - 1e-3, "the view went back down: {last} → {v}");
                last = v;
                rows.push(p.y);
                assert!(t < 20.0, "never got to the top");
            }
            assert!(inn.view_shift(t).abs() < 1e-3);
            assert!(rows.windows(2).any(|w| w[1] < w[0] - 0.1), "the villager never climbed");
            let (lo, hi) = inn.walk_rows();
            assert!(p.y >= lo && p.y <= hi, "landed off the floor: {} not in {lo}..{hi}", p.y);
            inn.poke(interior::Thing::Stairs(true), p, t);
            while inn.is_climbing() {
                t += DT;
                inn.update(DT, t, (0.0, 0.0), p);
            }
            assert_eq!(inn.kind(), interior::RoomKind::Attic);
            let o = inn.poke(interior::Thing::Window(0), p, t);
            assert!(matches!(o, interior::Outcome::Look(_)), "{o:?}");
            s.indoors(o);
        }
        assert!(s.look.is_some());
        let (cam, lift) = s.look_view().unwrap();
        assert!(lift > 0.0 && cam >= 0.0);
        s.interact();
        assert!(s.look.is_none(), "E didn't step back from the window");
    }

    /// Talking with someone in their home: the dialogue box opens with
    /// home topics, and a little job they ask for can be done.
    #[test]
    fn hosts_talk_and_ask_for_help() {
        let mut s = scene();
        let i = s.village.houses.iter().position(|h| !h.church && Some(0) != None).unwrap();
        let i = (i..s.village.houses.len()).find(|&k| !s.village.houses[k].church && Some(k) != s.home).unwrap();
        s.enter(i);
        s.update(DT, 0.0);
        s.indoors(interior::Outcome::Talk(0));
        let c = s.convo.as_ref().expect("no conversation with the host");
        let labels: Vec<String> = c.opts.iter().map(|o| o.label.clone()).collect();
        assert!(labels.iter().any(|l| l.contains("home") || l.contains("beautiful")), "{labels:?}");
        // ask to help, accept the job, do it
        let k = c.opts.iter().position(|o| o.label.contains("help")).map(|k| k + 1);
        if let Some(k) = k {
            s.answer(k);
            let k2 = s.convo.as_ref().unwrap().opts.iter().position(|o| o.label.contains("leave it to me")).map(|k| k + 1);
            if let Some(k2) = k2 {
                s.answer(k2);
                let (task, _) = s.inside.as_ref().unwrap().task.expect("no task taken on");
                let what = match task {
                    interior::Task::Star => interior::Thing::Tree,
                    interior::Task::Log | interior::Task::Stocking => interior::Thing::Fire,
                    interior::Task::FindPet => interior::Thing::Pet,
                    interior::Task::Tune => interior::Thing::Piano,
                };
                let t = s.now();
                let (inn, p) = (s.inside.as_mut().unwrap(), s.player.as_mut().unwrap());
                assert_eq!(inn.poke(what, p, t), interior::Outcome::Cheer(3));
                assert!(inn.task.is_none());
            }
        }
    }

    /// Longer lines stay up longer, and the setting scales it.
    #[test]
    fn longer_lines_stay_up_longer() {
        events::set_talk(1.0, 1.0);
        let short = events::read_time("hi!", 2.0);
        let long = events::read_time("the snow fell softly on the sleeping town, as if it too were waiting", 2.0);
        assert!(long > short + 2.0, "{short} vs {long}");
        events::set_talk(2.0, 1.0);
        assert!((events::read_time("hi!", 2.0) - short * 2.0).abs() < 1e-3);
        events::set_talk(1.0, 1.0);
    }

    /// Cars honk, trains whistle and the plane rumbles when clicked.
    #[test]
    fn traffic_and_the_plane_answer_clicks() {
        let mut s = Scene::new(300, 75, 3, &SceneCfg::default());
        let (mut car, mut train, mut plane) = (false, false, false);
        for step in 0..(240.0 / DT) as usize {
            s.update(DT, 0.0);
            if step % 15 != 0 {
                continue;
            }
            let _ = s.take_sounds();
            let cam = s.cam_i();
            let off = s.sky_off();
            // look for one of each on screen and click it
            'scan: for y in 0..s.h as i32 {
                for sx in 0..s.w as i32 {
                    let thing = s.sky_thing_at(sx, y, sx + cam);
                    let want = match thing {
                        Some(Thing::Car(_)) => !car,
                        Some(Thing::Train) => !train,
                        Some(Thing::Flyer(0)) => !plane,
                        _ => false,
                    };
                    if want {
                        s.click(sx, y, 0.0);
                        let heard: Vec<Sfx> = s.take_sounds().into_iter().map(|e| e.sfx).collect();
                        match thing {
                            Some(Thing::Car(_)) => car = heard.contains(&Sfx::Honk),
                            Some(Thing::Train) => train = heard.contains(&Sfx::TrainWhistle),
                            _ => plane = heard.contains(&Sfx::Plane),
                        }
                        break 'scan;
                    }
                }
            }
            let _ = off;
            if car && train && plane {
                break;
            }
        }
        // a mountain cabin lights up
        let _ = s.take_sounds();
        let cam = s.cam;
        let (i, sx, sy) = (0..s.land.cabins.len())
            .map(|i| (i, s.land.cabins[i].0 - cam * land::PARALLAX, s.land.cabins[i].1 / 2.0))
            .find(|c| c.1 >= 0.0 && c.1 < s.w as f32)
            .expect("no cabin in view");
        assert!(matches!(s.sky_thing_at(sx as i32, sy as i32, sx as i32 + s.cam_i()), Some(Thing::Cabin(j)) if j == i));
        s.click(sx as i32, sy as i32, 0.0);
        assert!(s.take_sounds().iter().any(|e| e.sfx == Sfx::Chime), "cabin didn't answer");
        // a shooting star can be caught
        s.director.shooting_star(100.0, 20.0);
        s.update(DT, 0.0);
        let star = (0..30).flat_map(|y| (0..s.w as i32).map(move |x| (x, y))).find(|&(x, y)| s.director.star_at(x, y).is_some()).expect("no star found");
        s.click(star.0, star.1, 0.0);
        assert!(s.take_sounds().iter().any(|e| e.sfx == Sfx::Starburst), "star not caught");
        assert!(car, "no car honked");
        assert!(train, "no train whistled");
        assert!(plane, "the plane didn't answer");
    }

    /// People mention what they just saw the villager do, and the
    /// dialogue box can be dragged somewhere else (and stays there).
    #[test]
    fn people_notice_what_the_villager_did() {
        let mut s = scene();
        s.update(DT, 0.0);
        let px = s.player.as_ref().unwrap().centre();
        s.deed(Deed::Fed("bunny"), px + 3.0);
        s.critters.spawn_demo(events::Kind::Walker, px + 5.0, s.now());
        let id = s.critters.people().iter().min_by(|a, b| (a.1 - px - 5.0).abs().total_cmp(&(b.1 - px - 5.0).abs())).unwrap().0;
        s.start_convo(id);
        let c = s.convo.as_ref().unwrap();
        assert!(c.line.contains("bunny"), "opener: {}", c.line);
        let labels: Vec<&str> = c.opts.iter().map(|o| o.label.as_str()).collect();
        assert!(labels.iter().any(|l| l.contains("feed")), "{labels:?}");
        // drag it by its title bar
        let (w, h) = (s.w as i32, s.h as i32);
        let (x0, y0) = c.origin(w, h);
        assert!(s.convo_click(x0 + 3, y0));
        s.convo_drag_to(x0 - 10, y0 - 8);
        s.convo_drop();
        assert_eq!(s.convo.as_ref().unwrap().origin(w, h), (x0 - 13, y0 - 8));
        // …and the next conversation opens where it was left
        s.close_panels();
        s.start_convo(id);
        assert_eq!(s.convo.as_ref().unwrap().origin(w, h), (x0 - 13, y0 - 8));
    }

    /// Kids out and about now and then have a snowball fight (and it ends).
    #[test]
    fn kids_have_snowball_fights() {
        let mut s = Scene::new(200, 60, 5, &SceneCfg::default());
        let (mut fought, mut ended) = (false, false);
        for i in 0..(600.0 / DT) as usize {
            s.update(DT, 0.0);
            let n = s.critters.fighting();
            ended |= fought && n == 0;
            fought |= n > 0;
            if i % 200 == 0 && s.critters.people().iter().filter(|p| p.3).count() < 2 {
                let x = s.cam + 80.0;
                s.critters.spawn_demo(events::Kind::Kid, x, s.now());
            }
        }
        assert!(fought, "no snowball fight broke out");
        assert!(ended, "the fight never ended");
    }

    /// Kids out in the snow build something, and anyone who goes into an
    /// igloo comes back out.
    #[test]
    fn kids_build_snowmen_and_igloos() {
        let mut s = Scene::new(200, 60, 11, &SceneCfg { critters: 2.0, ..SceneCfg::default() });
        let mut started = false;
        let mut went_in = false;
        for i in 0..(900.0 / DT) as usize {
            s.update(DT, 0.0);
            let (n, inside) = s.builds.count();
            started |= n > 0;
            went_in |= inside > 0;
            if i % 300 == 0 && s.critters.people().iter().filter(|p| p.3).count() < 2 {
                let x = s.cam + 60.0;
                s.critters.spawn_demo(events::Kind::Kid, x, s.now());
            }
        }
        assert!(started, "nothing got built");
        assert!(s.builds.made > 0, "nothing was ever finished");
        let _ = went_in;
        // nobody's stuck inside forever
        for _ in 0..(120.0 / DT) as usize {
            s.update(DT, 0.0);
        }
        assert!(s.builds.count().1 == 0 || went_in, "kids lost inside an igloo");
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
        // (the sky drifts a little with the camera; look away from the big
        // tree, whose star can sit in front of the moon on a short screen)
        s.jump(-120.0);
        let (mx, my, _) = s.sky.moon;
        s.click(mx as i32 - s.sky_off(), (my / 2.0) as i32, 0.0);
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

    #[test]
    fn right_click_drop_in_lands_the_villager_there() {
        let mut s = walker_scene();
        run(&mut s, 0.2);
        // an open bit of snowfield, on screen
        let sx = 20;
        let row = s.h as i32 - 3;
        let (_, items) = s.menu_at(sx, row);
        let drop = items.iter().find_map(|(_, d)| matches!(d, Do::DropIn(..)).then_some(*d)).expect("no drop-in on open ground");
        let target = sx as f32 + s.cam;
        s.act(drop);
        assert!(s.player.as_ref().unwrap().y < row as f32 - 5.0, "should start up in the sky");
        run(&mut s, 3.0);
        let p = s.player.as_ref().unwrap();
        assert!((p.centre() - target).abs() < 2.0, "landed at {} not {target}", p.centre());
        assert_eq!(p.y, row as f32);
        assert!(p.free(s.now()));
        // right-clicking the villager offers the stats card
        let (px, py) = (p.x.round() as i32 + 1 - s.cam_i(), p.y as i32 - 1);
        let (title, items) = s.menu_at(px, py);
        assert_eq!(title, "you");
        assert!(items.iter().any(|(_, d)| *d == Do::Stats));
    }

    #[test]
    fn menu_hello_walks_over_says_hi_and_ticks_the_list() {
        let mut s = walker_scene();
        let t = s.now();
        let (px, py) = { let p = s.player.as_ref().unwrap(); (p.centre(), p.y) };
        let _ = py;
        s.critters.spawn_demo(events::Kind::Walker, px + 30.0, t);
        let id = s.critters.last_id();
        s.critters.set_walking(id, 0.0);
        s.act(Do::Talk(id));
        run(&mut s, 6.0);
        assert!(s.pockets.greeted.contains(&id), "never got there to say hello");
        // four more hellos tick off the first Christmas-list goal (+5 cheer)
        for k in 0..4 {
            s.pockets.greeted.push(1000 + k);
        }
        let before = s.pockets.cheer;
        run(&mut s, 0.1);
        assert!(s.pockets.done & 1 == 1);
        assert!(s.pockets.cheer >= before + 5);
    }

    /// Rooms: E on the sofa sits the villager on its cushion; E on the bed
    /// tucks them in (the room dims, the pet curls up at the foot of the
    /// bed) until a direction key gets them up; the oven bakes a tray of
    /// gingerbread that E puts in the pockets.
    #[test]
    fn the_sofa_the_bed_and_the_oven_do_what_they_say() {
        let mut s = scene();
        let home = s.home.expect("no home");
        s.enter(home);
        let run = |s: &mut Scene, secs: f32| {
            for _ in 0..(secs / DT) as usize {
                s.update(DT, 0.0);
            }
        };
        run(&mut s, 0.5);
        s.preview_poke(2);
        run(&mut s, 0.5);
        {
            let (inn, p) = (s.inside.as_ref().unwrap(), s.player.as_ref().unwrap());
            assert!(!p.moving);
            assert!(inn.things_at().iter().any(|(k, x)| matches!(k, interior::Thing::Seat(_)) && (*x as f32 - p.centre()).abs() < 3.0), "not by a seat");
        }
        // upstairs to bed
        let t = s.now();
        {
            let (inn, p) = (s.inside.as_mut().unwrap(), s.player.as_mut().unwrap());
            inn.preview_room(2, p);
        }
        let _ = t;
        s.preview_poke(1);
        run(&mut s, 4.0);
        assert!(s.inside.as_ref().unwrap().is_lying());
        let (bx, _) = s.inside.as_ref().unwrap().bed_foot().unwrap();
        let pet = s.my_pet.as_ref().unwrap();
        assert!(pet.napping && (pet.x - bx).abs() < 4.0, "pet not asleep on the bed: {} vs {bx}", pet.x);
        s.set_move(1.0, 0.0);
        run(&mut s, 0.3);
        s.set_move(0.0, 0.0);
        assert!(!s.inside.as_ref().unwrap().is_lying(), "didn't get up");
        // the kitchen: bake, wait, take
        {
            let (inn, p) = (s.inside.as_mut().unwrap(), s.player.as_mut().unwrap());
            inn.preview_room(1, p);
        }
        let before = s.pockets.count(shop::Item::Gingerbread);
        s.preview_poke(3);
        run(&mut s, 9.0);
        s.preview_poke(3);
        assert_eq!(s.pockets.count(shop::Item::Gingerbread), before + 3, "the cookies never reached the pockets");
    }

    /// The pet: a thrown ball is fetched and brought back; tricks; and left
    /// standing about a good while, it naps (and wakes when you move off).
    #[test]
    fn the_pet_fetches_does_tricks_and_naps() {
        let mut s = scene();
        let run = |s: &mut Scene, secs: f32| {
            for _ in 0..(secs / DT) as usize {
                s.update(DT, 0.0);
            }
        };
        run(&mut s, 1.0);
        s.pet_act(story::PetAct::Throw(1));
        let x0 = s.my_pet.as_ref().unwrap().x;
        run(&mut s, 0.6);
        assert!(s.my_pet.as_ref().unwrap().toy.is_some(), "nothing thrown");
        let mut went = 0.0f32;
        for _ in 0..(12.0 / DT) as usize {
            s.update(DT, 0.0);
            went = went.max((s.my_pet.as_ref().unwrap().x - x0).abs());
            if s.my_pet.as_ref().unwrap().busy.is_none() {
                break;
            }
        }
        let pet = s.my_pet.as_ref().unwrap();
        assert!(went > 8.0, "the dog didn't run after it ({went})");
        assert!(pet.busy.is_none() && pet.toy.is_none(), "never brought it back");
        let p = s.player.as_ref().unwrap().centre();
        assert!((pet.x - p).abs() < 8.0, "dropped it far away");
        s.pet_act(story::PetAct::Trick);
        assert!(s.convo.as_ref().is_some_and(|c| !c.line.is_empty()));
        run(&mut s, 3.0);
        assert!(s.my_pet.as_ref().unwrap().busy.is_none());
        // a nap after a while standing still
        s.critters = critters::Critters::new(&s.ctx, 1, 0.0, false, &[]);
        run(&mut s, 34.0);
        assert!(s.my_pet.as_ref().unwrap().napping, "no nap");
        s.set_move(1.0, 0.0);
        run(&mut s, 0.2);
        assert!(!s.my_pet.as_ref().unwrap().napping, "slept through the walk");
    }

    /// The pet's care: fed from a bowl (it trots over and eats, and is full
    /// for a while after), cuddled in the villager's arms (until they walk
    /// off), sent for a nap (until they move), and brushed (a shiny coat).
    #[test]
    fn the_pet_eats_cuddles_naps_and_shines() {
        let mut s = walker_scene();
        let run = |s: &mut Scene, secs: f32| {
            for _ in 0..(secs / DT) as usize {
                s.update(DT, 0.0);
            }
        };
        run(&mut s, 1.0);
        s.pet_act(story::PetAct::Feed);
        assert!(s.my_pet.as_ref().unwrap().bowl.is_some(), "no bowl");
        let bowl = s.my_pet.as_ref().unwrap().bowl.unwrap().0;
        let mut ate = false;
        for _ in 0..(8.0 / DT) as usize {
            s.update(DT, 0.0);
            let pet = s.my_pet.as_ref().unwrap();
            if matches!(pet.busy, Some(pets::Busy::Eat { eating: true, .. })) {
                ate = true;
                assert!((pet.x - bowl).abs() < 4.0, "eating away from the bowl");
            }
        }
        assert!(ate, "never ate");
        assert!(s.my_pet.as_ref().unwrap().busy.is_none(), "still busy eating");
        // straight after: full
        s.pet_act(story::PetAct::Feed);
        assert!(s.convo.as_ref().is_some_and(|c| c.line.contains("full")), "not full after dinner");
        run(&mut s, 5.0);
        // a cuddle: held at the villager, until they walk off
        s.pet_act(story::PetAct::Cuddle);
        run(&mut s, 0.5);
        let px = s.player.as_ref().unwrap().centre();
        assert!(s.my_pet.as_ref().unwrap().held());
        assert!((s.my_pet.as_ref().unwrap().x - px).abs() < 2.0, "not in their arms");
        s.set_move(1.0, 0.0);
        run(&mut s, 0.3);
        s.set_move(0.0, 0.0);
        assert!(!s.my_pet.as_ref().unwrap().held(), "still held while walking");
        // nap time: asleep at once, up when the villager moves
        run(&mut s, 1.0);
        s.pet_act(story::PetAct::Nap);
        assert!(s.my_pet.as_ref().unwrap().asleep());
        run(&mut s, 2.0);
        assert!(s.my_pet.as_ref().unwrap().asleep(), "woke by itself");
        s.set_move(-1.0, 0.0);
        run(&mut s, 0.2);
        s.set_move(0.0, 0.0);
        assert!(!s.my_pet.as_ref().unwrap().asleep());
        // a brush: shines a while
        s.pet_act(story::PetAct::Brush);
        assert!(s.my_pet.as_ref().unwrap().shine_until > s.now() + 5.0);
        // and the menu offers all of it (once it has a name)
        s.my_pet.as_mut().unwrap().name = "Biscuit".into();
        s.pet_menu(0);
        let labels: Vec<String> = s.convo.as_ref().unwrap().opts.iter().map(|o| o.label.clone()).collect();
        for want in ["cuddle", "feed", "play!", "nap time", "more…"] {
            assert!(labels.iter().any(|l| l.contains(want)), "no {want:?} in {labels:?}");
        }
    }

    /// Q: what's in hand cycles through what's in the pockets; E then gives
    /// it to the pet as a treat; people greet the villager by name.
    #[test]
    fn something_in_hand_and_a_name() {
        let mut s = scene();
        s.update(DT, 0.0);
        s.player.as_mut().unwrap().berries = 0;
        s.pockets.add(shop::Item::Carrot);
        s.pockets.add(shop::Item::Cocoa);
        s.cycle_held();
        assert_eq!(s.held, Some(Held::Item(shop::Item::Cocoa)));
        s.cycle_held();
        assert_eq!(s.held, Some(Held::Item(shop::Item::Carrot)));
        s.cycle_held();
        assert_eq!(s.held, None);
        s.cycle_held();
        s.cycle_held();
        s.focus = Some(Focus::Pet(0, 0));
        s.interact();
        assert_eq!(s.pockets.count(shop::Item::Carrot), 0, "the pet didn't get the carrot");
        s.update(DT, 0.0);
        assert_eq!(s.held, None, "still holding a carrot that's gone");
        assert_eq!(story::by_name("hello again!", "", false, false, 0), "hello again!");
        assert!(story::by_name("hello again!", "Robin", false, false, 0).contains("Robin"));
        assert!(story::by_name("hi!", "Robin", true, true, 0).contains("Robin"));
    }

    /// On a screen twice as tall, people and animals are drawn twice as big
    /// (and walk twice as far), and rooms are built at their designed size
    /// and magnified.
    #[test]
    fn a_taller_screen_draws_everyone_to_scale() {
        assert_eq!(sprite_k(75), 1.0);
        assert_eq!(sprite_k(150), 2.0);
        assert_eq!(room_scale(150), 2.0);
        assert_eq!(room_size(1136, 150), (568, 75));
        let mut s = Scene::new(600, 150, 7, &SceneCfg::default());
        assert_eq!(s.critters.k, 2.0);
        assert_eq!(s.player.as_ref().unwrap().k, 2.0);
        s.update(DT, 0.0);
        let x0 = s.player_x().unwrap();
        s.set_move(1.0, 0.0);
        for _ in 0..30 {
            s.update(DT, 0.0);
        }
        let far = s.player_x().unwrap() - x0;
        assert!(far > 15.0, "walked only {far} columns in a second at 2x");
        let home = s.home.unwrap();
        s.enter(home);
        assert_eq!(s.view_zoom(), 2.0);
        assert_eq!(s.view_bounds(), (300.0, 75.0));
    }

    /// The pet answers E when the villager turns to face it, even with
    /// the big tree and presents in reach, and a click on it opens its
    /// menu (as at a 90-row screen with a cat).
    #[test]
    fn the_pet_answers_e_and_clicks() {
        let mut cfg = SceneCfg::default();
        cfg.pet.kind = "cat".into();
        cfg.pet.name = "Kitters".into();
        let mut s = Scene::new(682, 90, 7, &cfg);
        // stand by the big tree, where there's always something to poke
        let tree = s.trees.iter().find(|t| t.is_big()).map(|t| t.cx as f32).unwrap();
        {
            let p = s.player.as_mut().unwrap();
            p.x = tree - 6.0;
        }
        for _ in 0..60 {
            s.update(DT, 0.0);
        }
        let (px, petx) = (s.player.as_ref().unwrap().centre(), s.my_pet.as_ref().unwrap().x);
        // turn round to face the cat
        s.player.as_mut().unwrap().dir = (petx - px).signum();
        s.focus = s.find_focus();
        assert!(matches!(s.focus, Some(Focus::Pet(..))), "{:?}", s.focus);
        s.interact();
        assert!(s.convo.as_ref().is_some_and(|c| c.id == PET_ID), "E didn't open the pet's box");
        s.convo = None;
        // a click on it (screen coordinates: the camera's subtracted)
        let pet = s.my_pet.as_ref().unwrap();
        let (cx, cy) = ((pet.x - s.cam).round() as i32, pet.y.round() as i32 - 1);
        s.click(cx, cy, 0.0);
        assert!(s.convo.as_ref().is_some_and(|c| c.id == PET_ID && !c.opts.is_empty()), "a click didn't open the pet's menu");
    }

    /// At 150 rows nothing in the village draws negative (black) pixels
    /// (the snow flecks down tall roofs did), and zoomed in, people are
    /// drawn crisp into the zoomed frame.
    #[test]
    fn tall_screens_and_zoom_draw_cleanly() {
        let mut cfg = SceneCfg::default();
        cfg.snow = 0.0;
        cfg.events = 0.0;
        let mut s = Scene::new(600, 150, 7, &cfg);
        for _ in 0..30 {
            s.update(DT, 0.0);
        }
        let mut f = Frame::new(600, 150);
        s.draw(&mut f, 0.0);
        let low = f.px.iter().map(|c| c.r.min(c.g).min(c.b)).fold(f32::MAX, f32::min);
        assert!(low > -0.01, "a negative pixel: {low}");
        // zoomed: the villager is drawn after the zoom (sprite pixels)
        let mut f = Frame::new(600, 150);
        let mut spare = Frame::new(1, 1);
        let (z, ..) = s.render(&mut f, &mut spare, 0.0, 1.5, None);
        assert!((z - 1.5).abs() < 1e-3);
        assert!(f.soft.iter().any(|&v| v != 0), "nobody drawn into the zoomed view");
    }
}
