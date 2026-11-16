//! The Cocoa & Gifts cabin on the snowfield, the player's pockets, and the
//! "cheer" that pays for things (earned by being neighbourly: first hellos,
//! feeding animals, watching light shows, finding constellations…).

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Cocoa,
    Gingerbread,
    CandyCane,
    Carrot,
    Lantern,
}

pub const ITEMS: [Item; 5] = [Item::Cocoa, Item::Gingerbread, Item::CandyCane, Item::Carrot, Item::Lantern];

impl Item {
    pub fn name(self) -> &'static str {
        match self {
            Item::Cocoa => "hot cocoa",
            Item::Gingerbread => "gingerbread",
            Item::CandyCane => "candy cane",
            Item::Carrot => "carrot",
            Item::Lantern => "lantern",
        }
    }
    pub fn price(self) -> u32 {
        match self {
            Item::Cocoa => 3,
            Item::Gingerbread => 2,
            Item::CandyCane => 2,
            Item::Carrot => 3,
            Item::Lantern => 6,
        }
    }
    pub fn icon(self) -> (char, Rgb) {
        match self {
            Item::Cocoa => ('◘', Rgb::hex(0x8a5a3a)),
            Item::Gingerbread => ('◆', Rgb::hex(0xc8803a)),
            Item::CandyCane => ('ʃ', Rgb::hex(0xe8283c)),
            Item::Carrot => ('▾', Rgb::hex(0xff8a2a)),
            Item::Lantern => ('▣', Rgb::hex(0xffd27a)),
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Item::Cocoa => "drink: warm and cozy",
            Item::Gingerbread => "give to someone (or eat)",
            Item::CandyCane => "give to a kid (or eat)",
            Item::Carrot => "give to a deer or rabbit",
            Item::Lantern => "light the way (toggle)",
        }
    }
}

/// What the player carries (and has been up to: kept in the save file).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Pockets {
    pub cheer: u32,
    pub items: [u32; 5],
    pub lantern_on: bool,
    /// Warm from cocoa until then (a cozier glow).
    pub cozy_until: f32,
    /// People already said hello to (cheer for a first hello only).
    pub greeted: Vec<u64>,
    // ---- what the player's been up to (the stats card)
    pub steps: f32,
    pub chats: u32,
    pub fed: u32,
    pub thrown: u32,
    pub hits: u32,
    pub houses: Vec<usize>,
    pub stars: u32,
    pub shows: u32,
    pub berries: u32,
    pub gifts: u32,
    pub cocoa: u32,
    /// Times Santa's sleigh was waved down.
    pub santa: u32,
    /// Reached the far left / far right end of the village.
    pub ends: (bool, bool),
    /// Christmas-list goals done (bit per goal).
    pub done: u32,
    // ---- the scorecard
    /// Team snowball fights won and lost.
    pub fights_won: u32,
    pub fights_lost: u32,
    /// Games of tag; times "it" changed hands in them.
    pub tag_games: u32,
    pub tags: u32,
    pub angels: u32,
    pub carols: u32,
    pub fireworks: u32,
    pub light_shows: u32,
    /// Snowmen and igloos built (or helped with) to the end.
    pub built: u32,
    /// Santa, clicked out of his sleigh and chased (never caught).
    pub santa_chases: u32,
    /// Games at the table (chess, checkers, cards) won and lost.
    pub games_won: u32,
    pub games_lost: u32,
    /// Runs down the mountain; the quickest (s); kilometres driven.
    pub ski_runs: u32,
    pub ski_best: Option<f32>,
    pub km: f32,
}

/// The Christmas list: (goal, how much cheer it's worth).
pub const GOALS: [(&str, u32); 10] = [
    ("say hello to 5 neighbours", 5),
    ("have 3 proper chats", 4),
    ("feed 3 animals", 4),
    ("visit 3 homes", 4),
    ("give someone a gift", 3),
    ("land 5 snowballs", 3),
    ("watch a light show by the big tree", 3),
    ("find 3 constellations", 4),
    ("walk the village from end to end", 5),
    ("wave down Santa's sleigh", 4),
];

impl Pockets {
    /// Progress on goal `i`: (so far, needed).
    pub fn goal(&self, i: usize) -> (u32, u32) {
        let (cur, need) = match i {
            0 => (self.greeted.len() as u32, 5),
            1 => (self.chats, 3),
            2 => (self.fed, 3),
            3 => (self.houses.len() as u32, 3),
            4 => (self.gifts, 1),
            5 => (self.hits, 5),
            6 => (self.shows, 1),
            7 => (self.stars, 3),
            8 => ((self.ends.0 as u32) + (self.ends.1 as u32), 2),
            _ => (self.santa, 1),
        };
        (cur.min(need), need)
    }

    /// Goals newly done since last asked (marks them done).
    pub fn newly_done(&mut self) -> Vec<usize> {
        let mut out = vec![];
        for i in 0..GOALS.len() {
            let (cur, need) = self.goal(i);
            if cur >= need && self.done & (1 << i) == 0 {
                self.done |= 1 << i;
                out.push(i);
            }
        }
        out
    }
}

/// The stats card: what the player's been up to, and the Christmas list.
pub fn draw_stats(f: &mut Frame, p: &Pockets) {
    let w = 58;
    let h = GOALS.len() as i32 + 13;
    let (x0, y0) = (3, (f.h as i32 - h - 4).max(0));
    let bg = Bg::Mix(pal::UI_PANEL, 0.92);
    let border = Rgb::hex(0xffd36b) * 0.8;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
        }
    }
    f.text(x0 + 2, y0, " my village ", pal::UI_TITLE, bg, 0.0);
    let cheer = format!(" ✦ {} cheer ", p.cheer);
    f.text(x0 + w - 2 - cheer.chars().count() as i32, y0, &cheer, Rgb::hex(0xffd36b), bg, 0.0);
    let lines = [
        format!("walked {} steps · met {} · {} chats", (p.steps / 2.0) as u32, p.greeted.len(), p.chats),
        format!("fed {} animals · {} snowballs ({} hits) · {} gifts", p.fed, p.thrown, p.hits, p.gifts),
        format!("{} homes visited · {} constellations · {} cocoas · Santa {}", p.houses.len(), p.stars, p.cocoa, p.santa),
    ];
    for (i, l) in lines.iter().enumerate() {
        f.text(x0 + 2, y0 + 1 + i as i32, l, pal::UI_TEXT, bg, 0.0);
    }
    // the scorecard
    f.text(x0 + 2, y0 + 5, "SCORECARD", Rgb::hex(0xffd36b), bg, 0.0);
    let score = [
        format!("snowball fights won {} · lost {} · tag games {} ({} tags)", p.fights_won, p.fights_lost, p.tag_games, p.tags),
        format!("{} snow angels · {} carols · {} built · {} light shows", p.angels, p.carols, p.built, p.light_shows),
        format!("games won {} · lost {} · {} fireworks · Santa chased {}×", p.games_won, p.games_lost, p.fireworks, p.santa_chases),
        format!("{} ski runs (best {}) · {:.1} km driven", p.ski_runs, p.ski_best.map(|b| format!("{b:.1}s")).unwrap_or_else(|| "—".into()), p.km),
    ];
    for (i, l) in score.iter().enumerate() {
        f.text(x0 + 2, y0 + 6 + i as i32, l, pal::UI_TEXT, bg, 0.0);
    }
    f.text(x0 + 2, y0 + 10, "CHRISTMAS LIST", Rgb::hex(0xffd36b), bg, 0.0);
    for (i, (goal, reward)) in GOALS.iter().enumerate() {
        let y = y0 + 11 + i as i32;
        let done = p.done & (1 << i) != 0;
        let (cur, need) = p.goal(i);
        let (mark, fg) = if done { ('✓', Rgb::hex(0x5adf8a)) } else { ('○', pal::UI_DIM) };
        f.put_g(x0 + 2, y, Glyph::new(mark, fg).bg(bg).lit(0.0));
        f.text(x0 + 4, y, goal, if done { pal::UI_DIM } else { pal::UI_TEXT }, bg, 0.0);
        let right = if done { format!("+{reward} ✦") } else if need > 1 { format!("{cur}/{need}  +{reward} ✦") } else { format!("+{reward} ✦") };
        f.text(x0 + w - 2 - right.chars().count() as i32, y, &right, if done { Rgb::hex(0x5adf8a) } else { pal::UI_DIM }, bg, 0.0);
    }
    f.text(x0 + 2, y0 + h - 1, " Esc or right-click to close ", pal::UI_DIM, bg, 0.0);
}

impl Pockets {
    pub fn new() -> Self {
        Pockets { cheer: 5, ..Default::default() }
    }
    pub fn count(&self, it: Item) -> u32 {
        self.items[ITEMS.iter().position(|&i| i == it).unwrap_or(0)]
    }
    pub fn add(&mut self, it: Item) {
        let i = ITEMS.iter().position(|&i| i == it).unwrap_or(0);
        self.items[i] += 1;
    }
    pub fn take(&mut self, it: Item) -> bool {
        let i = ITEMS.iter().position(|&i| i == it).unwrap_or(0);
        if self.items[i] > 0 {
            self.items[i] -= 1;
            true
        } else {
            false
        }
    }
}

/// The cabin itself: `x` its left column, `base` its bottom row.
pub struct Shop {
    pub x: i32,
    pub base: i32,
    pub open: bool,
    /// Closed for the night (Christmas Eve).
    pub closed: bool,
}

pub const SHOP_W: i32 = 22;
/// How tall it stands (rows, from its base up to the top of the roof).
pub const SHOP_H: i32 = 12;

impl Shop {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + SHOP_W && y > self.base - SHOP_H && y <= self.base
    }

    /// Centre column (the door's a little left of it).
    pub fn centre(&self) -> i32 {
        self.x + SHOP_W / 2
    }

    /// The cabin's door (column): in through it.
    pub fn door(&self) -> i32 {
        self.x + 6
    }

    /// A log cabin in pixel art: courses of logs, a steep snowy roof, a
    /// chimney with smoke, the door, a big warm window with the shopkeeper
    /// behind the counter and a mug steaming on the sill, a sign over the
    /// door, lights along the eaves, a lantern.
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        let (x0, b) = (self.x, self.base);
        let (px0, px1) = (x0 + 1, x0 + SHOP_W - 1);
        let ground = b * 2 + 1;
        let eaves = ground - 14;
        let ridge = eaves - 9;
        let log = |y: i32| if (y - eaves) % 3 == 0 { Rgb::hex(0x4a2a16) } else if (y - eaves) % 3 == 1 { Rgb::hex(0x7a4a28) } else { Rgb::hex(0x6a3e22) };
        // the walls
        for y in eaves..ground {
            for x in px0..px1 {
                f.px_set(x, y, log(y) * (0.9 + 0.12 * (((x * 7 + y * 3) % 5) as f32 / 5.0)));
            }
            f.px_set(px0, y, Rgb::hex(0x3a2214));
            f.px_set(px1 - 1, y, Rgb::hex(0x3a2214));
        }
        // the roof: steep, snow on it, a little overhang
        for y in ridge..eaves {
            let k = (y - ridge) as f32 / (eaves - ridge) as f32;
            let half = 2.0 + k * (SHOP_W as f32 / 2.0 + 1.0);
            let mid = (px0 + px1) as f32 / 2.0;
            for x in (mid - half).floor() as i32..=(mid + half).ceil() as i32 {
                let edge = (x as f32 - mid).abs() > half - 1.5;
                f.px_set(x, y, if edge || y - ridge < 2 { Rgb::hex(0xf4f8ff) } else { Rgb::hex(0x8a2a22) * (0.85 + 0.1 * (y % 2) as f32) });
            }
        }
        for x in px0 - 2..=px1 + 1 {
            f.px_set(x, eaves, Rgb::hex(0xeef2fa));
            f.px_set(x, eaves - 1, Rgb::hex(0xf4f8ff));
        }
        // the chimney, smoke curling off it
        let cx = px1 - 5;
        for y in ridge + 1..eaves - 3 {
            for x in cx..cx + 3 {
                f.px_set(x, y, Rgb::hex(0x7a6a68) * if (x + y) % 3 == 0 { 0.8 } else { 1.0 });
            }
        }
        f.px_set(cx, ridge, Rgb::hex(0xf4f8ff));
        f.px_set(cx + 1, ridge, Rgb::hex(0xf4f8ff));
        f.px_set(cx + 2, ridge, Rgb::hex(0xf4f8ff));
        for k in 0..3 {
            let ph = (t * 0.4 + k as f32 / 3.0).fract();
            let (sx, sy) = (cx as f32 + 1.0 + (t * 1.3 + k as f32).sin() * 2.0 * ph, ridge as f32 - 2.0 - ph * 10.0);
            f.px_blend(sx as i32, sy as i32, Rgb::hex(0xd8dce8), 0.6 * (1.0 - ph));
        }
        // the door
        let dx = self.door();
        for y in ground - 10..ground {
            for x in dx - 2..dx + 2 {
                f.px_set(x, y, if self.closed { Rgb::hex(0x2a1a10) } else { Rgb::hex(0x3a2214) });
            }
        }
        f.px_set(dx + 1, ground - 5, Rgb::hex(0xffc23a));
        // a wreath on it
        for (ddx, ddy) in [(-1, -8), (0, -9), (1, -8), (-1, -7), (1, -7), (0, -6)] {
            f.px_set(dx + ddx, ground + ddy, Rgb::hex(0x1f6e3a));
        }
        f.px_set(dx, ground - 6, Rgb::hex(0xd8202a));
        // the big window: warm light, the counter, the shopkeeper
        let (wx0, wx1, wy0, wy1) = (dx + 4, px1 - 2, ground - 11, ground - 3);
        for y in wy0..wy1 {
            for x in wx0..wx1 {
                let frame = x == wx0 || x == wx1 - 1 || y == wy0 || y == wy1 - 1 || x == (wx0 + wx1) / 2;
                let c = if frame {
                    Rgb::hex(0xe8e0d0) * 0.8
                } else if self.closed {
                    Rgb::hex(0x1a1420)
                } else {
                    Rgb::hex(0xffd27a) * (0.75 + 0.25 * ((y - wy0) as f32 / (wy1 - wy0) as f32))
                };
                f.px_set(x, y, c);
            }
        }
        if !self.closed {
            // the shopkeeper (green apron), and the counter in front
            let kx = (wx0 + wx1) / 2 + 2;
            for y in wy0 + 2..wy0 + 4 {
                f.px_set(kx, y, Rgb::hex(0xe8c39e));
                f.px_set(kx + 1, y, Rgb::hex(0xe8c39e));
            }
            for y in wy0 + 4..wy1 - 2 {
                f.px_set(kx, y, Rgb::hex(0x2a6a3a));
                f.px_set(kx + 1, y, Rgb::hex(0x2a6a3a));
            }
            for x in wx0 + 1..wx1 - 1 {
                f.px_set(x, wy1 - 2, Rgb::hex(0x8a5a32));
            }
            // a mug steaming on the sill
            f.px_set(wx0 + 2, wy1, Rgb::hex(0xf0e8d8));
            let steam = (t * 2.0) as i32 % 2;
            f.px_blend(wx0 + 2 + steam, wy1 - 2, Rgb::hex(0xd8dce8), 0.6);
        }
        // lights along the eaves
        for (k, x) in (px0 - 1..=px1).step_by(2).enumerate() {
            let c = [0xff4a4a, 0xffd36b, 0x4ad06a, 0x4aa0ff, 0xff8ad8][k % 5];
            let tw = 0.7 + 0.3 * ((t * 3.0 + k as f32 * 1.7).sin() * 0.5 + 0.5);
            f.px_set(x, eaves + 1, Rgb::hex(c) * tw);
            if k % 2 == 0 {
                f.light_px(x as f32, (eaves + 1) as f32, Rgb::hex(c) * (0.5 * glow));
            }
        }
        // the lantern by the door
        f.px_set(dx - 4, ground - 8, Rgb::hex(0xffd27a));
        f.px_set(dx - 4, ground - 7, Rgb::hex(0xffd27a));
        let fl = 0.9 + 0.1 * (t * 7.0).sin();
        f.light_px((dx - 4) as f32, (ground - 8) as f32, Rgb::hex(0xffb050) * (3.0 * fl * glow));
        if !self.closed {
            f.light_px(((wx0 + wx1) / 2) as f32, ((wy0 + wy1) / 2) as f32, Rgb::hex(0xffb050) * (5.0 * fl * glow));
        }
        // the sign over the door
        let sign = if self.closed { " CLOSED · SEE YOU XMAS " } else { " COCOA & GIFTS " };
        let n = sign.chars().count() as i32;
        let sx = (x0 + SHOP_W / 2 - n / 2).max(x0 - 2);
        f.text(sx, b - 8, sign, Rgb::hex(0xffd36b), Bg::Solid(Rgb::hex(0x2a1a10)), 0.3);
    }

    /// The shop's menu, over the cabin.
    pub fn draw_menu(&self, f: &mut Frame, p: &Pockets) {
        self.draw_menu_at(f, p, self.centre(), self.base - 7, "classic");
    }

    /// The shop's menu: centred on column `cx`, its bottom edge on row
    /// `bottom` (kept on screen), in the panels' look.
    pub fn draw_menu_at(&self, f: &mut Frame, p: &Pockets, cx: i32, bottom: i32, look: &str) {
        let w = 40;
        let h = ITEMS.len() as i32 + 4;
        let x0 = (cx - w / 2).clamp(1, (f.w as i32 - w - 1).max(1));
        let y0 = (bottom - h).clamp(1, (f.h as i32 - h - 1).max(1));
        let bg = Bg::Mix(pal::UI_PANEL, 0.94);
        let border = Rgb::hex(0xffd36b) * 0.8;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                    (true, _, true, _) => '╭',
                    (_, true, true, _) => '╮',
                    (true, _, _, true) => '╰',
                    (_, true, _, true) => '╯',
                    (_, _, true, _) | (_, _, _, true) => '─',
                    (true, _, _, _) | (_, true, _, _) => '│',
                    _ => ' ',
                };
                f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
            }
        }
        f.text(x0 + 2, y0, " Cocoa & Gifts ", pal::UI_TITLE, bg, 0.0);
        let cheer = format!(" ✦ {} cheer ", p.cheer);
        f.text(x0 + w - 2 - cheer.chars().count() as i32, y0, &cheer, Rgb::hex(0xffd36b), bg, 0.0);
        for (i, it) in ITEMS.iter().enumerate() {
            let y = y0 + 1 + i as i32;
            let afford = p.cheer >= it.price();
            let fg = if afford { pal::UI_TEXT } else { pal::UI_DIM };
            let (ic, icc) = it.icon();
            f.text(x0 + 2, y, &format!("{}", i + 1), Rgb::hex(0xffd36b), bg, 0.0);
            f.put_g(x0 + 4, y, Glyph::new(ic, icc).bg(bg).lit(0.0));
            f.text(x0 + 6, y, it.name(), fg, bg, 0.0);
            let have = p.count(*it);
            if have > 0 {
                f.text(x0 + 19, y, &format!("×{have}"), pal::UI_DIM, bg, 0.0);
            }
            f.text(x0 + w - 8, y, &format!("{} ✦", it.price()), fg, bg, 0.0);
        }
        f.text(x0 + 2, y0 + h - 2, "1-5 buy · walk away or Esc to close", pal::UI_DIM, bg, 0.0);
        crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h }, look, bg, 5);
    }

    /// Which item row of the menu (drawn as `draw_menu_at` would) is at
    /// (x, y), if any (1-based, as the number keys).
    pub fn menu_item_at(&self, f_w: i32, f_h: i32, cx: i32, bottom: i32, x: i32, y: i32) -> Option<usize> {
        let (w, h) = (40, ITEMS.len() as i32 + 4);
        let x0 = (cx - w / 2).clamp(1, (f_w - w - 1).max(1));
        let y0 = (bottom - h).clamp(1, (f_h - h - 1).max(1));
        let i = y - y0 - 1;
        (x > x0 && x < x0 + w - 1 && i >= 0 && (i as usize) < ITEMS.len()).then_some(i as usize + 1)
    }
}

/// The pockets panel (I), bottom-left of the screen.
pub fn draw_pockets(f: &mut Frame, p: &Pockets) {
    let w = 44;
    let h = ITEMS.len() as i32 + 3;
    let (x0, y0) = (3, f.h as i32 - h - 4);
    let bg = Bg::Mix(pal::UI_PANEL, 0.9);
    let border = pal::UI_BORDER;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
        }
    }
    f.text(x0 + 2, y0, " pockets ", pal::UI_TITLE, bg, 0.0);
    let cheer = format!(" ✦ {} cheer ", p.cheer);
    f.text(x0 + w - 2 - cheer.chars().count() as i32, y0, &cheer, Rgb::hex(0xffd36b), bg, 0.0);
    for (i, it) in ITEMS.iter().enumerate() {
        let y = y0 + 1 + i as i32;
        let n = p.count(*it);
        let fg = if n > 0 { pal::UI_TEXT } else { pal::UI_DIM };
        let (ic, icc) = it.icon();
        f.text(x0 + 2, y, &format!("{}", i + 1), Rgb::hex(0xffd36b), bg, 0.0);
        f.put_g(x0 + 4, y, Glyph::new(ic, if n > 0 { icc } else { icc * 0.4 }).bg(bg).lit(0.0));
        let label = if *it == Item::Lantern && p.lantern_on { format!("{} (lit)", it.name()) } else { it.name().to_string() };
        f.text(x0 + 6, y, &format!("{label} ×{n}"), fg, bg, 0.0);
        f.text(x0 + 21, y, it.hint(), pal::UI_DIM, bg, 0.0);
    }
    f.text(x0 + 2, y0 + h - 1, " 1-5 use · I close ", pal::UI_DIM, bg, 0.0);
}
