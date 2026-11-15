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

/// What the player carries.
#[derive(Clone, Debug, Default)]
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
    /// Reached the far left / far right end of the village.
    pub ends: (bool, bool),
    /// Christmas-list goals done (bit per goal).
    pub done: u32,
}

/// The Christmas list: (goal, how much cheer it's worth).
pub const GOALS: [(&str, u32); 9] = [
    ("say hello to 5 neighbours", 5),
    ("have 3 proper chats", 4),
    ("feed 3 animals", 4),
    ("visit 3 homes", 4),
    ("give someone a gift", 3),
    ("land 5 snowballs", 3),
    ("watch a light show by the big tree", 3),
    ("find 3 constellations", 4),
    ("walk the village from end to end", 5),
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
            _ => ((self.ends.0 as u32) + (self.ends.1 as u32), 2),
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
    let h = GOALS.len() as i32 + 8;
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
        format!("{} homes visited · {} constellations · {} cocoas", p.houses.len(), p.stars, p.cocoa),
    ];
    for (i, l) in lines.iter().enumerate() {
        f.text(x0 + 2, y0 + 1 + i as i32, l, pal::UI_TEXT, bg, 0.0);
    }
    f.text(x0 + 2, y0 + 5, "CHRISTMAS LIST", Rgb::hex(0xffd36b), bg, 0.0);
    for (i, (goal, reward)) in GOALS.iter().enumerate() {
        let y = y0 + 6 + i as i32;
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
}

pub const SHOP_W: i32 = 16;

impl Shop {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + SHOP_W && y > self.base - 7 && y <= self.base
    }

    /// Centre column (where the shopkeeper stands).
    pub fn centre(&self) -> i32 {
        self.x + SHOP_W / 2
    }

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32) {
        let (x0, b) = (self.x, self.base);
        let wood = Rgb::hex(0x6a3e22);
        let dark = Rgb::hex(0x3a2214);
        let snow = Rgb::hex(0xeef2fa);
        // snowy roof
        for i in 0..SHOP_W {
            f.put_g(x0 + i, b - 6, Glyph::new('▄', snow * (0.9 + 0.1 * ((i * 7) % 3) as f32)).lit(0.6));
        }
        // striped awning
        for i in 0..SHOP_W {
            let c = if i % 2 == 0 { pal::CANDY_RED } else { Rgb::hex(0xf4efe6) };
            f.put_g(x0 + i, b - 5, Glyph::new('▀', c).solid(wood).lit(0.6));
        }
        // the sign
        let sign = " COCOA & GIFTS ";
        for i in 0..SHOP_W {
            f.put_g(x0 + i, b - 4, Glyph::new(' ', dark).solid(dark).lit(0.6));
        }
        let sx = x0 + (SHOP_W - sign.chars().count() as i32) / 2;
        f.text(sx, b - 4, sign, Rgb::hex(0xffd36b), Bg::Solid(dark), 0.3);
        // the window with the shopkeeper and a steaming mug
        for i in 0..SHOP_W {
            let edge = i == 0 || i == SHOP_W - 1;
            for dy in [3, 2] {
                let g = if edge { Glyph::new('▐', wood).solid(dark) } else { Glyph::new(' ', dark).solid(Rgb::hex(0x7a4a22) * 1.2) };
                f.put_g(x0 + i, b - dy, g.lit(0.4));
            }
        }
        let kx = self.centre();
        f.put_g(kx, b - 3, Glyph::new('●', Rgb::hex(0xe8c39e)).solid(Rgb::hex(0x8a5a2a)).lit(0.4));
        f.put_g(kx, b - 2, Glyph::new('█', Rgb::hex(0x2a6a3a)).solid(Rgb::hex(0x8a5a2a)).lit(0.4));
        f.put_g(kx - 4, b - 2, Glyph::new('◘', Rgb::hex(0xf0e8d8)).solid(Rgb::hex(0x8a5a2a)).lit(0.4));
        let steam = if (t * 2.0) as i32 % 2 == 0 { '∘' } else { '°' };
        f.put_g(kx - 4, b - 3, Glyph::new(steam, Rgb::hex(0xd8dce8)).solid(Rgb::hex(0x8a5a2a)).lit(0.4));
        f.put_g(kx + 4, b - 2, Glyph::new('◆', Rgb::hex(0xc8803a)).solid(Rgb::hex(0x8a5a2a)).lit(0.4));
        // counter and legs
        for i in 0..SHOP_W {
            f.put_g(x0 + i, b - 1, Glyph::new('▀', Rgb::hex(0x8a5a32)).solid(dark).lit(0.6));
            if i == 1 || i == SHOP_W - 2 {
                f.put_g(x0 + i, b, Glyph::new('║', dark).lit(0.6));
            }
        }
        let fl = 0.9 + 0.1 * (t * 7.0).sin();
        f.light_cell(kx, b - 3, Rgb::hex(0xffb050) * (5.0 * fl * glow));
    }

    /// The shop's menu, over the cabin.
    pub fn draw_menu(&self, f: &mut Frame, p: &Pockets) {
        let w = 40;
        let x0 = self.centre() - w / 2;
        let y0 = self.base - 7 - (ITEMS.len() as i32 + 4);
        let bg = Bg::Mix(pal::UI_PANEL, 0.94);
        let border = Rgb::hex(0xffd36b) * 0.8;
        let h = ITEMS.len() as i32 + 4;
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
        f.text(x0 + 2, y0 + h - 2, "1-5 buy · E or Esc to close", pal::UI_DIM, bg, 0.0);
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
