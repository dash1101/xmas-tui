//! Games at the table, in someone's home: chess, checkers, Snowflakes (a
//! card game of colours and numbers: match the colour or the number, skip,
//! turn about, make them draw, a snowflake plays on anything) and
//! Twenty-One. Played against whoever lives there, on a panel over the room:
//! the arrow keys (or the mouse) to choose, Enter to play, Esc to get up
//! from the table.

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Chess,
    Checkers,
    Snowflakes,
    TwentyOne,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Chess, Kind::Checkers, Kind::Snowflakes, Kind::TwentyOne];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Chess => "chess",
            Kind::Checkers => "checkers",
            Kind::Snowflakes => "Snowflakes",
            Kind::TwentyOne => "Twenty-One",
        }
    }

    pub fn icon(self) -> char {
        match self {
            Kind::Chess => '♞',
            Kind::Checkers => '◉',
            Kind::Snowflakes => '❄',
            Kind::TwentyOne => '♠',
        }
    }
}

/// What the keys (or a click) do at the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Pick,
    /// Twenty-One: stand. Snowflakes: draw a card.
    Other,
}

/// How a game ended (for the scorecard): won, lost or drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Result {
    Won,
    Lost,
    Drawn,
}

pub struct Table {
    pub kind: Kind,
    /// Who's playing against the villager.
    pub them: String,
    game: Game,
    rng: Rng,
    /// What's happening (the line under the board).
    pub status: String,
    /// The game's over, and how; and it's gone on the scorecard.
    pub result: Option<Result>,
    pub scored: bool,
    /// Their move comes after a moment's thought (scene time).
    think_until: f32,
    /// Where the board was drawn (for clicks): x, y.
    origin: std::cell::Cell<(i32, i32)>,
    /// The pixel-art picture of the game, and what it showed.
    pic: std::cell::RefCell<Option<(u64, std::sync::Arc<crate::frame::Picture>)>>,
}

enum Game {
    Chess(Chess),
    Checkers(Checkers),
    Snow(Snow),
    TwentyOne(TwentyOne),
}

impl Table {
    pub fn new(kind: Kind, them: &str, seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x6a3e);
        let game = match kind {
            Kind::Chess => Game::Chess(Chess::new()),
            Kind::Checkers => Game::Checkers(Checkers::new()),
            Kind::Snowflakes => Game::Snow(Snow::new(&mut rng)),
            Kind::TwentyOne => Game::TwentyOne(TwentyOne::new(&mut rng)),
        };
        let status = match kind {
            Kind::Chess => "you're white: arrows to a piece, Enter, then where it goes".to_string(),
            Kind::Checkers => "you're red, moving up: pick a piece, then where (jumps are a must)".to_string(),
            Kind::Snowflakes => "match the colour or the number · ← → choose · Enter play · D draw".to_string(),
            Kind::TwentyOne => "nearer 21 than them, without going over".to_string(),
        };
        Table { kind, them: them.to_string(), game, rng, status, result: None, scored: false, think_until: 0.0, origin: std::cell::Cell::new((0, 0)), pic: std::cell::RefCell::new(None) }
    }

    /// Their turn?
    fn theirs(&self) -> bool {
        match &self.game {
            Game::Chess(c) => !c.white && c.over.is_none(),
            Game::Checkers(c) => !c.red && c.over.is_none(),
            Game::Snow(s) => s.turn != 0 && s.over.is_none(),
            Game::TwentyOne(_) => false,
        }
    }

    /// A key at the table.
    pub fn key(&mut self, k: Key, t: f32) {
        if self.result.is_some() {
            // a game over: Enter for another
            if k == Key::Pick {
                let them = self.them.clone();
                let seed = self.rng.next_u32();
                *self = Table::new(self.kind, &them, seed);
            }
            return;
        }
        if self.theirs() {
            return;
        }
        let them = self.them.clone();
        let msg = match &mut self.game {
            Game::Chess(c) => c.key(k),
            Game::Checkers(c) => c.key(k),
            Game::Snow(s) => s.key(k, &mut self.rng, &them),
            Game::TwentyOne(g) => g.key(k, &mut self.rng, &them),
        };
        if let Some(m) = msg {
            self.status = m;
        }
        self.think_until = t + 0.9;
        self.check_over();
    }

    /// A click on the board (screen cell): picks what's there.
    pub fn click(&mut self, x: i32, y: i32, t: f32) {
        let (ox, oy) = self.origin.get();
        let hit = match &mut self.game {
            Game::Chess(c) => board_hit(x - ox, y - oy).map(|(cx, cy)| {
                c.cur = (cx, cy);
            }),
            Game::Checkers(c) => board_hit(x - ox, y - oy).map(|(cx, cy)| {
                c.cur = (cx, cy);
            }),
            Game::Snow(s) => {
                let i = if crate::frame::pictures_ok() { ((x - ox) * 3 - 4) / 12 } else { (x - ox - 2) / 5 };
                let rows = if crate::frame::pictures_ok() { 11..=16 } else { 12..=14 };
                (rows.contains(&(y - oy)) && i >= 0 && (i as usize) <= s.hands[0].len()).then(|| s.sel = i as usize)
            }
            Game::TwentyOne(_) => Some(()),
        };
        if hit.is_some() {
            self.key(Key::Pick, t);
        }
    }

    /// Their move, once they've had a think.
    pub fn update(&mut self, t: f32) {
        if self.result.is_some() || !self.theirs() || t < self.think_until {
            return;
        }
        let them = self.them.clone();
        let msg = match &mut self.game {
            Game::Chess(c) => c.their_move(&mut self.rng, &them),
            Game::Checkers(c) => c.their_move(&mut self.rng, &them),
            Game::Snow(s) => s.their_turn(&mut self.rng, &them),
            Game::TwentyOne(_) => None,
        };
        if let Some(m) = msg {
            self.status = m;
        }
        self.think_until = t + 0.9;
        self.check_over();
    }

    fn check_over(&mut self) {
        let over = match &self.game {
            Game::Chess(c) => c.over,
            Game::Checkers(c) => c.over,
            Game::Snow(s) => s.over,
            Game::TwentyOne(g) => g.over,
        };
        if let Some(r) = over {
            if self.result.is_none() {
                self.result = Some(r);
                let them = &self.them;
                self.status = match r {
                    Result::Won => format!("you win! {them} shakes your hand · Enter: again · Esc: up from the table"),
                    Result::Lost => format!("{them} wins this one · Enter: a rematch · Esc: up from the table"),
                    Result::Drawn => "a draw! · Enter: again · Esc: up from the table".to_string(),
                };
            }
        }
    }

    /// The table: a panel in the middle of the screen.
    pub fn draw(&self, f: &mut Frame, look: &str) {
        let (w, h) = (54, 24);
        let (x0, y0) = ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(0));
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
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
        let title = format!(" {} {} with {} ", self.kind.icon(), self.kind.name(), self.them);
        f.text(x0 + 2, y0, &title, pal::UI_TITLE, bg, 0.0);
        f.text(x0 + w - 13, y0, " Esc: leave ", pal::UI_DIM, bg, 0.0);
        let (bx, by) = (x0 + 2, y0 + 2);
        self.origin.set((bx, by));
        if crate::frame::pictures_ok() {
            // drawn as pixel art (easier on the eye than characters); only
            // made again when something's changed
            self.draw_pixels(f, bx, by, bg);
        } else {
            match &self.game {
                Game::Chess(c) => c.draw(f, bx + 9, by, bg),
                Game::Checkers(c) => c.draw(f, bx + 9, by, bg),
                Game::Snow(s) => s.draw(f, bx, by, bg, &self.them),
                Game::TwentyOne(g) => g.draw(f, bx, by, bg, &self.them),
            }
        }
        if matches!(self.game, Game::Chess(_) | Game::Checkers(_)) {
            self.origin.set((bx + 9, by));
        }
        // what's happening
        let line: String = self.status.chars().take((w - 4) as usize).collect();
        f.text(x0 + 2, y0 + h - 2, &line, if self.result.is_some() { Rgb::hex(0xffd36b) } else { pal::UI_TEXT }, bg, 0.0);
        if self.theirs() && self.result.is_none() {
            f.text(x0 + w - 16, y0 + h - 3, &format!("{} thinks…", short(&self.them)), pal::UI_DIM, bg, 0.0);
        }
        crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h }, look, bg, 17);
    }
}

impl Table {
    /// The game in pixel art: the board (32×16 cells), or the cards.
    fn draw_pixels(&self, f: &mut Frame, bx: i32, by: i32, bg: Bg) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let (cols, rows, ox) = match &self.game {
            Game::Chess(c) => {
                (c.b, c.cur, c.picked).hash(&mut h);
                (32, 16, 9)
            }
            Game::Checkers(c) => {
                (c.b, c.cur, c.picked).hash(&mut h);
                (32, 16, 9)
            }
            Game::Snow(s) => {
                (s.hands.iter().map(|hd| hd.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>()).collect::<Vec<_>>(), s.top().0, s.top().1, s.colour, s.sel, s.choosing).hash(&mut h);
                (50, 18, 0)
            }
            Game::TwentyOne(g) => {
                (&g.you, &g.them, g.standing).hash(&mut h);
                (50, 18, 0)
            }
        };
        let key = h.finish();
        let pic = {
            let mut cache = self.pic.borrow_mut();
            match cache.as_ref() {
                Some((k, p)) if *k == key => p.clone(),
                _ => {
                    let p = std::sync::Arc::new(self.paint());
                    *cache = Some((key, p.clone()));
                    p
                }
            }
        };
        f.picture(bx + ox, by, cols, rows, pic);
        if ox > 0 {
            self.origin.set((bx + ox, by));
        }
        // words alongside the pictures
        match &self.game {
            Game::Snow(s) => {
                f.text(bx, by, &format!("{} · {} cards", short(&self.them), s.hands[1].len()), pal::UI_DIM, bg, 0.0);
                f.text(bx, by + 17, &format!("your hand · {} cards  (← → choose · Enter play · D draw)", s.hands[0].len()), pal::UI_DIM, bg, 0.0);
            }
            Game::TwentyOne(g) => {
                let theirs = if g.standing { format!("{} · {}", short(&self.them), total(&g.them)) } else { format!("{} · ?", short(&self.them)) };
                f.text(bx, by, &theirs, pal::UI_DIM, bg, 0.0);
                f.text(bx, by + 17, &format!("you · {}   (Enter hit · S stand)", total(&g.you)), Rgb::hex(0xffd36b), bg, 0.0);
            }
            _ => {}
        }
    }

    fn paint(&self) -> crate::frame::Picture {
        match &self.game {
            Game::Chess(c) => {
                let targets: Vec<usize> = c.picked.map(|p| c.moves().into_iter().filter(|m| m.from == p).map(|m| m.to).collect()).unwrap_or_default();
                paint_board(c.cur, c.picked, &targets, |i| {
                    let p = c.b[i];
                    (p != 0).then(|| (CHESS_ART[(p.unsigned_abs() - 1) as usize], p > 0))
                })
            }
            Game::Checkers(c) => {
                let targets: Vec<usize> = c.picked.map(|p| c.moves().into_iter().filter(|m| m.0 == p).map(|m| m.1).collect()).unwrap_or_default();
                paint_board(c.cur, c.picked, &targets, |i| {
                    let p = c.b[i];
                    (p != 0).then(|| (if p.abs() == 2 { KING_MAN } else { MAN }, p > 0))
                })
            }
            Game::Snow(s) => s.paint(),
            Game::TwentyOne(g) => g.paint(),
        }
    }
}

/// A pixel canvas, to make the games' pictures with.
struct Px {
    w: i32,
    h: i32,
    buf: Vec<u8>,
}

impl Px {
    fn new(w: i32, h: i32, c: Rgb) -> Self {
        let mut p = Px { w, h, buf: vec![0; (w * h * 4) as usize] };
        p.rect(0, 0, w, h, c);
        p
    }
    fn clear(w: i32, h: i32) -> Self {
        Px { w, h, buf: vec![0; (w * h * 4) as usize] }
    }
    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            let i = ((y * self.w + x) * 4) as usize;
            let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            self.buf[i..i + 4].copy_from_slice(&[to8(c.r), to8(c.g), to8(c.b), 255]);
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }
    /// A little bitmap ('#' the colour, 'o' the outline), at (x, y).
    fn art(&mut self, x: i32, y: i32, art: &[&str], c: Rgb, edge: Rgb) {
        for (dy, row) in art.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                match ch {
                    '#' => self.set(x + dx as i32, y + dy as i32, c),
                    'o' => self.set(x + dx as i32, y + dy as i32, edge),
                    '+' => self.set(x + dx as i32, y + dy as i32, c * 1.25),
                    _ => {}
                }
            }
        }
    }
    /// Text in the 3×5 pixel font.
    fn text(&mut self, x: i32, y: i32, s: &str, c: Rgb) {
        let mut cx = x;
        for ch in s.chars() {
            if let Some(g) = glyph3x5(ch) {
                for (dy, row) in g.iter().enumerate() {
                    for dx in 0..3 {
                        if row & (4 >> dx) != 0 {
                            self.set(cx + dx, y + dy as i32, c);
                        }
                    }
                }
            }
            cx += 4;
        }
    }
    fn picture(self) -> crate::frame::Picture {
        crate::frame::Picture::new(self.w as usize, self.h as usize, self.buf)
    }
}

/// The 3×5 pixel font: digits, a few letters, the suits' marks.
fn glyph3x5(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 3, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 2, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'A' => [2, 5, 7, 5, 5],
        'J' => [1, 1, 1, 5, 7],
        'Q' => [7, 5, 5, 7, 1],
        'K' => [5, 6, 4, 6, 5],
        '+' => [0, 2, 7, 2, 0],
        'X' => [5, 5, 2, 5, 5],
        'R' => [6, 5, 6, 5, 5],
        '?' => [7, 1, 2, 0, 2],
        _ => return None,
    })
}

/// Chess pieces, 10×10 (pawn, knight, bishop, rook, queen, king).
const CHESS_ART: [&[&str]; 6] = [
    &["", "", "    oo", "   o##o", "   o##o", "    oo", "   o##o", "  o####o", "  oooooo", ""],
    &["   oo", "  o##o", " o####o", "o##o###o", " oo o##o", "   o###o", "  o####o", " o######o", " oooooooo", ""],
    &["    o", "   o#o", "  o###o", "  o#o#o", "  o###o", "   o#o", "  o###o", " o#####o", " ooooooo", ""],
    &["", " o o o o", " o#o#o#o", " o#####o", "  o###o", "  o###o", "  o###o", " o#####o", " ooooooo", ""],
    &[" o  o  o", "o#oo#oo#o", "o#######o", " o#####o", "  o###o", "  o###o", "  o###o", " o#####o", " ooooooo", ""],
    &["    o", "   o#o", "  oo#oo", " o#####o", " o##+##o", "  o###o", "  o###o", " o#####o", " ooooooo", ""],
];
const MAN: &[&str] = &["", "   oooo", "  o####o", " o######o", " o#++++#o", " o######o", "  o####o", "   oooo", "", ""];
const KING_MAN: &[&str] = &["", "   oooo", "  o#o#o#o", " o######o", " o#++++#o", " o######o", "  o####o", "   oooo", "", ""];

/// A board as pixel art: 8×8 squares of 12 pixels, pieces on them.
fn paint_board(cur: (i32, i32), picked: Option<usize>, targets: &[usize], piece: impl Fn(usize) -> Option<(&'static [&'static str], bool)>) -> crate::frame::Picture {
    let sq = 12;
    let mut p = Px::new(8 * sq, 8 * sq, Rgb::BLACK);
    for ry in 0..8 {
        for fx in 0..8 {
            let i = (ry * 8 + fx) as usize;
            let light = (fx + ry) % 2 == 1;
            let mut c = if light { Rgb::hex(0xe8d4b0) } else { Rgb::hex(0x8a5a3a) };
            if (fx, ry) == cur {
                c = c.lerp(CURSOR, 0.55);
            } else if Some(i) == picked {
                c = c.lerp(PICKED, 0.6);
            }
            let (x, y) = (fx * sq, (7 - ry) * sq);
            p.rect(x, y, sq, sq, c);
            if targets.contains(&i) {
                p.rect(x + 5, y + 5, 2, 2, TARGET);
            }
            if let Some((art, mine)) = piece(i) {
                let (body, edge) = if mine { (Rgb::hex(0xfff8ec), Rgb::hex(0x2a1a14)) } else { (Rgb::hex(0x231a24), Rgb::hex(0x6a5a68)) };
                p.art(x + 1, y + 1, art, body, edge);
            }
            // the square the cursor's on: a bright frame
            if (fx, ry) == cur {
                for k in 0..sq {
                    p.set(x + k, y, CURSOR * 1.3);
                    p.set(x + k, y + sq - 1, CURSOR * 1.3);
                    p.set(x, y + k, CURSOR * 1.3);
                    p.set(x + sq - 1, y + k, CURSOR * 1.3);
                }
            }
        }
    }
    p.picture()
}

/// A playing card, w×h pixels, its face colour and what's on it.
fn paint_card(p: &mut Px, x: i32, y: i32, w: i32, h: i32, face: Rgb, ink: Rgb, label: &str, hot: bool) {
    let edge = if hot { Rgb::hex(0xffd36b) } else { Rgb::hex(0x1a1420) };
    p.rect(x, y, w, h, edge);
    p.rect(x + 1, y + 1, w - 2, h - 2, face);
    // a pale oval through the middle, as the cards have
    for yy in 0..h - 4 {
        for xx in 0..w - 4 {
            let (u, v) = ((xx as f32 - (w - 5) as f32 / 2.0) / ((w - 4) as f32 / 2.0), (yy as f32 - (h - 5) as f32 / 2.0) / ((h - 4) as f32 / 2.0));
            if (u * u + v * v) < 0.55 && (u + v * 0.5).abs() < 0.9 {
                p.set(x + 2 + xx, y + 2 + yy, face.lerp(Rgb::hex(0xffffff), 0.35));
            }
        }
    }
    let n = label.chars().count() as i32;
    p.text(x + (w - n * 4 + 1) / 2, y + (h - 5) / 2, label, ink);
    p.text(x + 2, y + 2, &label.chars().take(1).collect::<String>(), ink);
}

fn short(s: &str) -> String {
    s.chars().take(8).collect()
}

/// A board square under (x, y), relative to its top-left (squares 4 wide,
/// 2 tall).
fn board_hit(x: i32, y: i32) -> Option<(i32, i32)> {
    (x >= 0 && y >= 0 && x < 32 && y < 16).then(|| (x / 4, 7 - y / 2))
}

/// A board drawn: each square 4×2 cells; `piece(x, y)`: the glyph and its
/// colour there (rank 0 at the bottom); `mark(x, y)`: highlighted.
fn draw_board(f: &mut Frame, x0: i32, y0: i32, piece: impl Fn(i32, i32) -> Option<(char, Rgb)>, mark: impl Fn(i32, i32) -> Option<Rgb>) {
    for ry in 0..8 {
        for fx in 0..8 {
            let light = (fx + ry) % 2 == 1;
            let base = if light { Rgb::hex(0xe8d4b0) } else { Rgb::hex(0x8a5a3a) };
            let c = mark(fx, ry).map(|m| base.lerp(m, 0.55)).unwrap_or(base);
            let (sx, sy) = (x0 + fx * 4, y0 + (7 - ry) * 2);
            for dy in 0..2 {
                for dx in 0..4 {
                    f.put_g(sx + dx, sy + dy, Glyph::new(' ', c).bg(Bg::Solid(c)).lit(0.0));
                }
            }
            if let Some((ch, col)) = piece(fx, ry) {
                f.put_g(sx + 1, sy + 1, Glyph::new(ch, col).bg(Bg::Solid(c)).lit(0.0));
                f.put_g(sx + 2, sy + 1, Glyph::new(' ', col).bg(Bg::Solid(c)).lit(0.0));
            }
        }
    }
    for i in 0..8 {
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
        f.put_g(x0 - 2, y0 + (7 - i) * 2 + 1, Glyph::new((b'1' + i as u8) as char, pal::UI_DIM).bg(bg).lit(0.0));
        f.put_g(x0 + i * 4 + 1, y0 + 16, Glyph::new((b'a' + i as u8) as char, pal::UI_DIM).bg(bg).lit(0.0));
    }
}

const CURSOR: Rgb = Rgb::hex(0x4aa0ff);
const PICKED: Rgb = Rgb::hex(0xffd36b);
const TARGET: Rgb = Rgb::hex(0x5adf8a);

fn step_cursor(cur: &mut (i32, i32), k: Key) {
    match k {
        Key::Up => cur.1 = (cur.1 + 1).min(7),
        Key::Down => cur.1 = (cur.1 - 1).max(0),
        Key::Left => cur.0 = (cur.0 - 1).max(0),
        Key::Right => cur.0 = (cur.0 + 1).min(7),
        _ => {}
    }
}

// ---------------------------------------------------------------- chess

/// Pieces: 1 pawn, 2 knight, 3 bishop, 4 rook, 5 queen, 6 king; white
/// (the villager) positive, black negative. Square = rank * 8 + file.
#[derive(Clone)]
struct Chess {
    b: [i8; 64],
    white: bool,
    /// Castling still allowed: white short, white long, black short, black
    /// long.
    castle: [bool; 4],
    /// A pawn just went two: the square behind it (en passant).
    ep: Option<usize>,
    cur: (i32, i32),
    picked: Option<usize>,
    over: Option<Result>,
    /// Moves without a capture or a pawn move (fifty: a draw).
    quiet: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Mv {
    from: usize,
    to: usize,
}

const KNIGHT: [(i32, i32); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
const KING: [(i32, i32); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
const DIAG: [(i32, i32); 4] = [(1, 1), (-1, 1), (1, -1), (-1, -1)];
const LINE: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

fn at(x: i32, y: i32) -> Option<usize> {
    ((0..8).contains(&x) && (0..8).contains(&y)).then_some((y * 8 + x) as usize)
}

impl Chess {
    fn new() -> Self {
        let mut b = [0i8; 64];
        let back = [4, 2, 3, 5, 6, 3, 2, 4];
        for x in 0..8 {
            b[x] = back[x];
            b[8 + x] = 1;
            b[48 + x] = -1;
            b[56 + x] = -back[x];
        }
        Chess { b, white: true, castle: [true; 4], ep: None, cur: (4, 1), picked: None, over: None, quiet: 0 }
    }

    /// Is square `sq` attacked by the side `white`?
    fn attacked(&self, sq: usize, white: bool) -> bool {
        let (x, y) = ((sq % 8) as i32, (sq / 8) as i32);
        let s = if white { 1 } else { -1 };
        let is = |p: i8, kind: i8| p == kind * s;
        // pawns
        let py = if white { y - 1 } else { y + 1 };
        for dx in [-1, 1] {
            if at(x + dx, py).is_some_and(|i| is(self.b[i], 1)) {
                return true;
            }
        }
        for (dx, dy) in KNIGHT {
            if at(x + dx, y + dy).is_some_and(|i| is(self.b[i], 2)) {
                return true;
            }
        }
        for (dx, dy) in KING {
            if at(x + dx, y + dy).is_some_and(|i| is(self.b[i], 6)) {
                return true;
            }
        }
        for (dirs, kinds) in [(DIAG, [3i8, 5]), (LINE, [4, 5])] {
            for (dx, dy) in dirs {
                let (mut cx, mut cy) = (x + dx, y + dy);
                while let Some(i) = at(cx, cy) {
                    let p = self.b[i];
                    if p != 0 {
                        if kinds.iter().any(|&k| is(p, k)) {
                            return true;
                        }
                        break;
                    }
                    cx += dx;
                    cy += dy;
                }
            }
        }
        false
    }

    fn king(&self, white: bool) -> usize {
        self.b.iter().position(|&p| p == if white { 6 } else { -6 }).unwrap_or(0)
    }

    fn in_check(&self, white: bool) -> bool {
        self.attacked(self.king(white), !white)
    }

    /// Every move for the side to move that doesn't leave its king in check.
    fn moves(&self) -> Vec<Mv> {
        let w = self.white;
        let s: i8 = if w { 1 } else { -1 };
        let mut out = vec![];
        for from in 0..64 {
            let p = self.b[from] * s;
            if p <= 0 {
                continue;
            }
            let (x, y) = ((from % 8) as i32, (from / 8) as i32);
            let mut add = |to: usize| out.push(Mv { from, to });
            let enemy = |i: usize| self.b[i] * s < 0;
            match p {
                1 => {
                    let dy = if w { 1 } else { -1 };
                    if let Some(i) = at(x, y + dy).filter(|&i| self.b[i] == 0) {
                        add(i);
                        let start = if w { 1 } else { 6 };
                        if y == start {
                            if let Some(j) = at(x, y + 2 * dy).filter(|&j| self.b[j] == 0) {
                                add(j);
                            }
                        }
                    }
                    for dx in [-1, 1] {
                        if let Some(i) = at(x + dx, y + dy) {
                            if enemy(i) || self.ep == Some(i) {
                                add(i);
                            }
                        }
                    }
                }
                2 | 6 => {
                    for (dx, dy) in if p == 2 { KNIGHT } else { KING } {
                        if let Some(i) = at(x + dx, y + dy).filter(|&i| self.b[i] * s <= 0) {
                            add(i);
                        }
                    }
                    if p == 6 {
                        // castling: king and rook unmoved, nothing between,
                        // not out of, through or into check
                        let (k0, short, long) = if w { (4, 0, 1) } else { (60, 2, 3) };
                        if from == k0 && !self.attacked(k0, !w) {
                            if self.castle[short] && self.b[k0 + 1] == 0 && self.b[k0 + 2] == 0 && self.b[k0 + 3] == 4 * s && !self.attacked(k0 + 1, !w) {
                                add(k0 + 2);
                            }
                            if self.castle[long] && self.b[k0 - 1] == 0 && self.b[k0 - 2] == 0 && self.b[k0 - 3] == 0 && self.b[k0 - 4] == 4 * s && !self.attacked(k0 - 1, !w) {
                                add(k0 - 2);
                            }
                        }
                    }
                }
                _ => {
                    let dirs: &[(i32, i32)] = match p {
                        3 => &DIAG,
                        4 => &LINE,
                        _ => &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)],
                    };
                    for &(dx, dy) in dirs {
                        let (mut cx, mut cy) = (x + dx, y + dy);
                        while let Some(i) = at(cx, cy) {
                            if self.b[i] * s > 0 {
                                break;
                            }
                            add(i);
                            if self.b[i] != 0 {
                                break;
                            }
                            cx += dx;
                            cy += dy;
                        }
                    }
                }
            }
        }
        out.retain(|&m| {
            let mut c = self.clone();
            c.play(m);
            !c.in_check(w)
        });
        out
    }

    /// Make a move (pawns reaching the end become queens).
    fn play(&mut self, m: Mv) {
        let p = self.b[m.from];
        let s: i8 = if p > 0 { 1 } else { -1 };
        let capture = self.b[m.to] != 0;
        // en passant: the pawn taken is beside, not on, the square
        if p.abs() == 1 && Some(m.to) == self.ep {
            let behind = if s > 0 { m.to - 8 } else { m.to + 8 };
            self.b[behind] = 0;
        }
        // castling: the rook comes round too
        if p.abs() == 6 && (m.to as i32 - m.from as i32).abs() == 2 {
            let (rf, rt) = if m.to > m.from { (m.from + 3, m.from + 1) } else { (m.from - 4, m.from - 1) };
            self.b[rt] = self.b[rf];
            self.b[rf] = 0;
        }
        self.ep = (p.abs() == 1 && (m.to as i32 - m.from as i32).abs() == 16).then(|| (m.from + m.to) / 2);
        self.b[m.to] = p;
        self.b[m.from] = 0;
        if p.abs() == 1 && (m.to / 8 == 7 || m.to / 8 == 0) {
            self.b[m.to] = 5 * s;
        }
        for (i, sq) in [(0, 7usize), (1, 0), (2, 63), (3, 56)] {
            if m.from == sq || m.to == sq {
                self.castle[i] = false;
            }
        }
        if p == 6 {
            self.castle[0] = false;
            self.castle[1] = false;
        } else if p == -6 {
            self.castle[2] = false;
            self.castle[3] = false;
        }
        self.quiet = if capture || p.abs() == 1 { 0 } else { self.quiet + 1 };
        self.white = !self.white;
    }

    /// After a move: is it over?
    fn settle(&mut self) {
        let any = !self.moves().is_empty();
        if !any {
            self.over = Some(if !self.in_check(self.white) {
                Result::Drawn
            } else if self.white {
                Result::Lost
            } else {
                Result::Won
            });
        } else if self.quiet >= 100 || self.b.iter().filter(|&&p| p != 0).count() == 2 {
            self.over = Some(Result::Drawn);
        }
    }

    /// Material, and a little for the centre and for moving the pawns on
    /// (from white's side).
    fn eval(&self) -> i32 {
        const V: [i32; 7] = [0, 100, 300, 320, 500, 900, 0];
        let mut e = 0;
        for (i, &p) in self.b.iter().enumerate() {
            if p == 0 {
                continue;
            }
            let (x, y) = ((i % 8) as i32, (i / 8) as i32);
            let centre = 6 - ((2 * x - 7).abs() + (2 * y - 7).abs()) / 2;
            let mut v = V[p.unsigned_abs() as usize] + if p.abs() == 2 || p.abs() == 3 { centre * 4 } else { 0 };
            if p.abs() == 1 {
                v += if p > 0 { y * 6 } else { (7 - y) * 6 } + if (3..=4).contains(&x) { 8 } else { 0 };
            }
            e += if p > 0 { v } else { -v };
        }
        e
    }

    /// Best score for the side to move, `depth` moves deep.
    fn search(&self, depth: u32, mut alpha: i32, beta: i32) -> i32 {
        let moves = self.moves();
        if moves.is_empty() {
            return if self.in_check(self.white) { -100_000 - depth as i32 } else { 0 };
        }
        if depth == 0 {
            return if self.white { self.eval() } else { -self.eval() };
        }
        for m in moves {
            let mut c = self.clone();
            c.play(m);
            let v = -c.search(depth - 1, -beta, -alpha);
            if v > alpha {
                alpha = v;
                if alpha >= beta {
                    break;
                }
            }
        }
        alpha
    }

    fn key(&mut self, k: Key) -> Option<String> {
        step_cursor(&mut self.cur, k);
        if k != Key::Pick {
            return None;
        }
        let sq = (self.cur.1 * 8 + self.cur.0) as usize;
        match self.picked {
            Some(from) if from == sq => {
                self.picked = None;
                None
            }
            Some(from) => {
                let mv = Mv { from, to: sq };
                if self.moves().contains(&mv) {
                    self.picked = None;
                    self.play(mv);
                    self.settle();
                    Some(if self.in_check(false) { "check!".into() } else { "your move's made".into() })
                } else if self.b[sq] > 0 {
                    self.picked = Some(sq);
                    None
                } else {
                    Some("that piece can't go there".into())
                }
            }
            None if self.b[sq] > 0 => {
                self.picked = Some(sq);
                None
            }
            None => Some("pick one of your pieces (the light ones)".into()),
        }
    }

    fn their_move(&mut self, rng: &mut Rng, them: &str) -> Option<String> {
        let moves = self.moves();
        if moves.is_empty() {
            self.settle();
            return None;
        }
        // a friendly game: two moves ahead, and not always the very best
        let mut scored: Vec<(i32, Mv)> = moves
            .iter()
            .map(|&m| {
                let mut c = self.clone();
                c.play(m);
                (-c.search(1, -1_000_000, 1_000_000) + rng.int(0, 30), m)
            })
            .collect();
        scored.sort_by_key(|s| -s.0);
        let m = scored[0].1;
        let took = self.b[m.to] != 0;
        self.play(m);
        self.settle();
        let name = |f: usize| ["", "pawn", "knight", "bishop", "rook", "queen", "king"][f];
        let sq = |i: usize| format!("{}{}", (b'a' + (i % 8) as u8) as char, i / 8 + 1);
        let what = name(self.b[m.to].unsigned_abs() as usize);
        Some(if self.in_check(true) {
            format!("{} moves the {what} to {} — check!", short(them), sq(m.to))
        } else if took {
            format!("{} takes on {} with the {what}", short(them), sq(m.to))
        } else {
            format!("{} moves the {what} to {}", short(them), sq(m.to))
        })
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, _bg: Bg) {
        let targets: Vec<usize> = self.picked.map(|p| self.moves().into_iter().filter(|m| m.from == p).map(|m| m.to).collect()).unwrap_or_default();
        draw_board(
            f,
            x0,
            y0,
            |x, y| {
                let p = self.b[(y * 8 + x) as usize];
                (p != 0).then(|| {
                    let ch = ['♟', '♞', '♝', '♜', '♛', '♚'][(p.unsigned_abs() - 1) as usize];
                    (ch, if p > 0 { Rgb::hex(0xfff8ec) } else { Rgb::hex(0x1a1014) })
                })
            },
            |x, y| {
                let i = (y * 8 + x) as usize;
                if (x, y) == self.cur {
                    Some(CURSOR)
                } else if Some(i) == self.picked {
                    Some(PICKED)
                } else if targets.contains(&i) {
                    Some(TARGET)
                } else {
                    None
                }
            },
        );
    }
}

// ------------------------------------------------------------- checkers

/// 1 a red man (the villager's, moving up), 2 a red king; -1 / -2 theirs.
struct Checkers {
    b: [i8; 64],
    red: bool,
    cur: (i32, i32),
    picked: Option<usize>,
    /// Partway through a run of jumps: the piece must jump on from here.
    chain: Option<usize>,
    over: Option<Result>,
}

impl Checkers {
    fn new() -> Self {
        let mut b = [0i8; 64];
        for y in 0..8 {
            for x in 0..8 {
                if (x + y) % 2 == 0 {
                    if y < 3 {
                        b[y * 8 + x] = 1;
                    } else if y > 4 {
                        b[y * 8 + x] = -1;
                    }
                }
            }
        }
        Checkers { b, red: true, cur: (1, 2), picked: None, chain: None, over: None }
    }

    /// Moves for the side to move: (from, to, jumped). Jumps if any can.
    fn moves(&self) -> Vec<(usize, usize, Option<usize>)> {
        let s: i8 = if self.red { 1 } else { -1 };
        let (mut steps, mut jumps) = (vec![], vec![]);
        for from in 0..64 {
            if self.chain.is_some_and(|c| c != from) {
                continue;
            }
            let p = self.b[from] * s;
            if p <= 0 {
                continue;
            }
            let (x, y) = ((from % 8) as i32, (from / 8) as i32);
            let fwd = s as i32;
            let dirs: Vec<(i32, i32)> = if p == 2 { vec![(1, 1), (-1, 1), (1, -1), (-1, -1)] } else { vec![(1, fwd), (-1, fwd)] };
            for (dx, dy) in dirs {
                if let Some(i) = at(x + dx, y + dy) {
                    if self.b[i] == 0 {
                        steps.push((from, i, None));
                    } else if self.b[i] * s < 0 {
                        if let Some(j) = at(x + 2 * dx, y + 2 * dy).filter(|&j| self.b[j] == 0) {
                            jumps.push((from, j, Some(i)));
                        }
                    }
                }
            }
        }
        if !jumps.is_empty() || self.chain.is_some() {
            jumps
        } else {
            steps
        }
    }

    /// Make a move; true if the same piece must jump again.
    fn play(&mut self, (from, to, jumped): (usize, usize, Option<usize>)) -> bool {
        let p = self.b[from];
        self.b[to] = p;
        self.b[from] = 0;
        let crowned = (p == 1 && to / 8 == 7) || (p == -1 && to / 8 == 0);
        if crowned {
            self.b[to] = p * 2;
        }
        if let Some(j) = jumped {
            self.b[j] = 0;
            self.chain = Some(to);
            if !crowned && !self.moves().is_empty() {
                return true;
            }
        }
        self.chain = None;
        self.red = !self.red;
        if self.moves().is_empty() {
            self.over = Some(if self.red { Result::Lost } else { Result::Won });
        }
        false
    }

    fn key(&mut self, k: Key) -> Option<String> {
        step_cursor(&mut self.cur, k);
        if k != Key::Pick {
            return None;
        }
        let sq = (self.cur.1 * 8 + self.cur.0) as usize;
        let moves = self.moves();
        match self.picked {
            Some(from) => {
                if let Some(&m) = moves.iter().find(|m| m.0 == from && m.1 == sq) {
                    let again = self.play(m);
                    self.picked = again.then_some(sq);
                    Some(if again { "and again — keep jumping!".into() } else { "your move's made".into() })
                } else if self.b[sq] > 0 && self.chain.is_none() {
                    self.picked = Some(sq);
                    None
                } else {
                    self.picked = self.chain;
                    Some(if moves.iter().any(|m| m.2.is_some()) { "there's a jump to make".into() } else { "it can't go there".into() })
                }
            }
            None if self.b[sq] > 0 => {
                self.picked = Some(sq);
                None
            }
            None => Some("pick one of your pieces (the red ones)".into()),
        }
    }

    fn their_move(&mut self, rng: &mut Rng, them: &str) -> Option<String> {
        let mut jumped = 0;
        loop {
            let moves = self.moves();
            if moves.is_empty() {
                break;
            }
            // a jump that crowns, a jump, a step that crowns, a step that's
            // safe, anything
            let safe = |m: &(usize, usize, Option<usize>)| {
                let (x, y) = ((m.1 % 8) as i32, (m.1 / 8) as i32);
                ![(1, -1), (-1, -1)].iter().any(|(dx, dy)| at(x + dx, y + dy).is_some_and(|i| self.b[i] > 0) && at(x - dx, y - dy).is_some_and(|j| self.b[j] == 0 || j == m.0))
            };
            let crowns = |m: &(usize, usize, Option<usize>)| self.b[m.0] == -1 && m.1 / 8 == 0;
            let pick = moves.iter().find(|m| m.2.is_some() && crowns(m)).or_else(|| moves.iter().find(|m| crowns(m))).or_else(|| {
                let good: Vec<_> = moves.iter().filter(|m| safe(m)).collect();
                (!good.is_empty()).then(|| *rng.pick(&good))
            });
            let m = *pick.unwrap_or_else(|| rng.pick(&moves));
            if m.2.is_some() {
                jumped += 1;
            }
            if !self.play(m) {
                break;
            }
        }
        Some(match jumped {
            0 => format!("{} moves", short(them)),
            1 => format!("{} jumps one of yours", short(them)),
            n => format!("{} jumps {n} in a row!", short(them)),
        })
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, _bg: Bg) {
        let targets: Vec<usize> = self.picked.map(|p| self.moves().into_iter().filter(|m| m.0 == p).map(|m| m.1).collect()).unwrap_or_default();
        draw_board(
            f,
            x0,
            y0,
            |x, y| {
                let p = self.b[(y * 8 + x) as usize];
                (p != 0).then(|| (if p.abs() == 2 { '◉' } else { '●' }, if p > 0 { Rgb::hex(0xe8384a) } else { Rgb::hex(0x1a1014) }))
            },
            |x, y| {
                let i = (y * 8 + x) as usize;
                if (x, y) == self.cur {
                    Some(CURSOR)
                } else if Some(i) == self.picked {
                    Some(PICKED)
                } else if targets.contains(&i) {
                    Some(TARGET)
                } else {
                    None
                }
            },
        );
    }
}

// ----------------------------------------------------------- Snowflakes

/// A card: colour 0..4 (red, green, blue, gold; 4: a snowflake, any colour)
/// and what it is: 0-9, 10 skip, 11 turn about, 12 draw two, 13 snowflake,
/// 14 snowflake draw four.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Card(u8, u8);

const COLOURS: [(&str, u32); 4] = [("red", 0xe0404a), ("green", 0x3ab86a), ("blue", 0x4a8aff), ("gold", 0xffc23a)];

impl Card {
    fn label(self) -> String {
        match self.1 {
            0..=9 => format!("{}", self.1),
            10 => "⊘".into(),
            11 => "⇄".into(),
            12 => "+2".into(),
            13 => "❄".into(),
            _ => "❄4".into(),
        }
    }
    fn colour(self) -> Rgb {
        COLOURS.get(self.0 as usize).map(|c| Rgb::hex(c.1)).unwrap_or(Rgb::hex(0xf4f8ff))
    }
}

struct Snow {
    deck: Vec<Card>,
    pile: Vec<Card>,
    /// The colour to match (a snowflake sets it).
    colour: u8,
    /// Hands: the villager's first.
    hands: Vec<Vec<Card>>,
    turn: usize,
    dir: i32,
    sel: usize,
    /// Choosing a colour for a snowflake just played.
    choosing: Option<u8>,
    over: Option<Result>,
}

impl Snow {
    fn new(rng: &mut Rng) -> Self {
        let mut deck = vec![];
        for c in 0..4u8 {
            deck.push(Card(c, 0));
            for n in 1..=12u8 {
                deck.push(Card(c, n));
                deck.push(Card(c, n));
            }
        }
        for _ in 0..4 {
            deck.push(Card(4, 13));
            deck.push(Card(4, 14));
        }
        shuffle(&mut deck, rng);
        let hands: Vec<Vec<Card>> = (0..2).map(|_| deck.split_off(deck.len() - 7)).collect();
        // the first card face up is a number
        let i = deck.iter().position(|c| c.1 <= 9).unwrap_or(0);
        let first = deck.remove(i);
        Snow { deck, pile: vec![first], colour: first.0, hands, turn: 0, dir: 1, sel: 0, choosing: None, over: None }
    }

    fn top(&self) -> Card {
        *self.pile.last().unwrap()
    }

    fn fits(&self, c: Card) -> bool {
        c.0 == 4 || c.0 == self.colour || c.1 == self.top().1
    }

    fn draw_card(&mut self, rng: &mut Rng) -> Option<Card> {
        if self.deck.is_empty() {
            let top = self.pile.pop()?;
            self.deck = std::mem::take(&mut self.pile);
            self.pile.push(top);
            shuffle(&mut self.deck, rng);
        }
        self.deck.pop()
    }

    fn next(&self, k: i32) -> usize {
        (self.turn as i32 + self.dir * k).rem_euclid(self.hands.len() as i32) as usize
    }

    /// Player `p` plays card `i` (colour already chosen for a snowflake).
    fn play(&mut self, p: usize, i: usize, colour: u8, rng: &mut Rng) {
        let c = self.hands[p].remove(i);
        self.pile.push(c);
        self.colour = if c.0 == 4 { colour } else { c.0 };
        if self.hands[p].is_empty() {
            self.over = Some(if p == 0 { Result::Won } else { Result::Lost });
            return;
        }
        let mut skip = 1;
        match c.1 {
            10 => skip = 2,
            11 => {
                self.dir = -self.dir;
                if self.hands.len() == 2 {
                    skip = 2;
                }
            }
            12 | 14 => {
                let n = if c.1 == 12 { 2 } else { 4 };
                let victim = self.next(1);
                for _ in 0..n {
                    if let Some(d) = self.draw_card(rng) {
                        self.hands[victim].push(d);
                    }
                }
                skip = 2;
            }
            _ => {}
        }
        self.turn = self.next(skip);
    }

    fn key(&mut self, k: Key, rng: &mut Rng, them: &str) -> Option<String> {
        if let Some(c) = self.choosing.as_mut() {
            match k {
                Key::Left => *c = (*c + 3) % 4,
                Key::Right => *c = (*c + 1) % 4,
                Key::Pick => {
                    let colour = *c;
                    self.choosing = None;
                    let i = self.sel;
                    self.play(0, i, colour, rng);
                    self.sel = self.sel.min(self.hands[0].len().saturating_sub(1));
                    return Some(format!("snowflake! the colour's {} · {}'s turn", COLOURS[colour as usize].0, short(them)));
                }
                _ => {}
            }
            return None;
        }
        let n = self.hands[0].len();
        match k {
            Key::Left => self.sel = (self.sel + n) % (n + 1),
            Key::Right => self.sel = (self.sel + 1) % (n + 1),
            Key::Other => self.sel = n,
            _ => {}
        }
        if !matches!(k, Key::Pick | Key::Other) {
            return None;
        }
        if self.sel >= n {
            // the deck: draw one (and play it, if it fits)
            let Some(c) = self.draw_card(rng) else { return None };
            self.hands[0].push(c);
            if self.fits(c) {
                self.sel = self.hands[0].len() - 1;
                return Some("you draw — and it fits! Enter to play it".into());
            }
            self.turn = self.next(1);
            return Some(format!("you draw a card · {}'s turn", short(them)));
        }
        let c = self.hands[0][self.sel];
        if !self.fits(c) {
            return Some(format!("that doesn't match — a {} card, or a {}", COLOURS[self.colour as usize].0, self.top().label()));
        }
        if c.0 == 4 {
            self.choosing = Some(self.colour);
            return Some("snowflake: ← → choose the colour, Enter".into());
        }
        let i = self.sel;
        self.play(0, i, 0, rng);
        self.sel = self.sel.min(self.hands[0].len().saturating_sub(1));
        Some(if self.hands[0].len() == 1 { "snowflake! one card left!".into() } else { format!("{}'s turn", short(them)) })
    }

    fn their_turn(&mut self, rng: &mut Rng, them: &str) -> Option<String> {
        let p = self.turn;
        // the card that fits, holding the snowflakes back
        let hand = &self.hands[p];
        let fits: Vec<usize> = (0..hand.len()).filter(|&i| self.fits(hand[i])).collect();
        let pick = fits.iter().copied().filter(|&i| hand[i].0 != 4).max_by_key(|&i| hand[i].1).or_else(|| fits.first().copied());
        let name = short(them);
        match pick {
            Some(i) => {
                let c = self.hands[p][i];
                // their most-held colour, for a snowflake
                let colour = (0..4u8).max_by_key(|&k| self.hands[p].iter().filter(|c| c.0 == k).count()).unwrap_or(0);
                self.play(p, i, colour, rng);
                Some(match c.1 {
                    13 | 14 => format!("{name} plays a snowflake: {}!", COLOURS[colour as usize].0),
                    10 => format!("{name} skips you!"),
                    12 => format!("{name}: draw two!"),
                    _ if self.over.is_none() && self.hands[p].len() == 1 => format!("{name}: snowflake! one card left"),
                    _ => format!("{name} plays a {} {}", COLOURS[c.0.min(3) as usize].0, c.label()),
                })
            }
            None => {
                if let Some(c) = self.draw_card(rng) {
                    self.hands[p].push(c);
                }
                self.turn = self.next(1);
                Some(format!("{name} draws a card · your turn"))
            }
        }
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, bg: Bg, them: &str) {
        // their hand, face down
        f.text(x0, y0, &format!("{} · {} cards", short(them), self.hands[1].len()), pal::UI_DIM, bg, 0.0);
        for i in 0..self.hands[1].len().min(16) {
            f.text(x0 + 2 + i as i32 * 3, y0 + 1, "▒▒", Rgb::hex(0x6a7aa8), bg, 0.0);
        }
        // the pile and the deck
        let top = self.top();
        card_box(f, x0 + 18, y0 + 4, &top.label(), Rgb::hex(COLOURS[self.colour as usize].1), false, bg);
        card_box(f, x0 + 26, y0 + 4, "❄", Rgb::hex(0x6a7aa8), self.sel >= self.hands[0].len() && self.choosing.is_none(), bg);
        f.text(x0 + 26, y0 + 8, "draw", pal::UI_DIM, bg, 0.0);
        f.text(x0 + 16, y0 + 8, &format!("({})", COLOURS[self.colour as usize].0), Rgb::hex(COLOURS[self.colour as usize].1), bg, 0.0);
        // the villager's hand
        f.text(x0, y0 + 10, &format!("your hand · {} cards", self.hands[0].len()), pal::UI_DIM, bg, 0.0);
        for (i, c) in self.hands[0].iter().enumerate().take(9) {
            // the one chosen sits up out of the hand, marked
            let hot = i == self.sel && self.choosing.is_none();
            let x = x0 + 2 + i as i32 * 5;
            card_box(f, x, y0 + 12 - hot as i32, &c.label(), c.colour(), hot, bg);
            if hot {
                f.text(x + 1, y0 + 15, "▲", Rgb::hex(0xffd36b), bg, 0.0);
            }
        }
        if self.hands[0].len() > 9 {
            f.text(x0 + 47, y0 + 13, &format!("+{}", self.hands[0].len() - 9), pal::UI_DIM, bg, 0.0);
        }
        if let Some(c) = self.choosing {
            for (k, (name, col)) in COLOURS.iter().enumerate() {
                let hot = k as u8 == c;
                f.text(x0 + 4 + k as i32 * 10, y0 + 17, &format!("{}{name}", if hot { "▸" } else { " " }), Rgb::hex(*col) * if hot { 1.2 } else { 0.7 }, bg, 0.0);
            }
        }
    }
}

impl Snow {
    /// The table as pixel art: their cards face down, the pile and the
    /// deck, the villager's hand (the chosen one lifted).
    fn paint(&self) -> crate::frame::Picture {
        let (w, h) = (150, 112);
        let mut p = Px::clear(w, h);
        // their hand, face down
        let n1 = self.hands[1].len().min(14) as i32;
        for i in 0..n1 {
            paint_card(&mut p, 4 + i * 9, 8, 12, 17, Rgb::hex(0x3a4a7a), Rgb::hex(0xbfe4ff), "", false);
            p.art(4 + i * 9 + 3, 13, &[" + ", "+++", " + "], Rgb::hex(0xbfe4ff), Rgb::hex(0xbfe4ff));
        }
        // the pile (its colour to match) and the deck
        let top = self.top();
        let face = Rgb::hex(COLOURS[self.colour as usize].1);
        let lab = |c: Card| match c.1 {
            0..=9 => c.1.to_string(),
            10 => "X".into(),
            11 => "R".into(),
            12 => "+2".into(),
            14 => "+4".into(),
            _ => String::new(),
        };
        paint_card(&mut p, 50, 36, 18, 26, face, Rgb::hex(0xffffff), &lab(top), false);
        let on_deck = self.sel >= self.hands[0].len() && self.choosing.is_none();
        paint_card(&mut p, 80, 36, 18, 26, Rgb::hex(0x3a4a7a), Rgb::hex(0xbfe4ff), "", on_deck);
        p.art(85, 45, &["  +  ", " +++ ", "+++++", " +++ ", "  +  "], Rgb::hex(0xbfe4ff), Rgb::hex(0xbfe4ff));
        // the hand
        for (i, c) in self.hands[0].iter().enumerate().take(12) {
            let hot = i == self.sel && self.choosing.is_none();
            let face = c.colour() * if c.0 == 4 { 0.7 } else { 1.0 };
            paint_card(&mut p, 4 + i as i32 * 12, 78 - if hot { 6 } else { 0 }, 14, 22, face, Rgb::hex(0xffffff), &lab(*c), hot);
            if c.0 == 4 {
                p.art(4 + i as i32 * 12 + 4, 78 - if hot { 6 } else { 0 } + 13, &[" + ", "+++", " + "], Rgb::hex(0xffffff), Rgb::hex(0xffffff));
            }
        }
        // a snowflake's colour being chosen
        if let Some(sel) = self.choosing {
            for (k, (_, col)) in COLOURS.iter().enumerate() {
                let hot = k as u8 == sel;
                let x = 30 + k as i32 * 24;
                p.rect(x - 1, 102 - 1, 20, 9, if hot { Rgb::hex(0xffd36b) } else { Rgb::hex(0x1a1420) });
                p.rect(x, 102, 18, 7, Rgb::hex(*col));
            }
        }
        p.picture()
    }
}

impl TwentyOne {
    fn paint(&self) -> crate::frame::Picture {
        let (w, h) = (150, 112);
        let mut p = Px::clear(w, h);
        let suit = |s: u8| -> &'static [&'static str] {
            match s % 4 {
                0 => &["  #  ", " ### ", "#####", "  #  ", " ### "],
                1 => &[" # # ", "#####", "#####", " ### ", "  #  "],
                2 => &["  #  ", " ### ", "#####", " ### ", "  #  "],
                _ => &[" ### ", " ### ", "#####", "##+##", "  #  "],
            }
        };
        let rank = |c: (u8, u8)| match c.0 {
            1 => "A".to_string(),
            11 => "J".into(),
            12 => "Q".into(),
            13 => "K".into(),
            n => n.to_string(),
        };
        let row = |p: &mut Px, y: i32, hand: &[(u8, u8)], hide: bool| {
            for (i, &c) in hand.iter().enumerate() {
                let x = 6 + i as i32 * 24;
                if hide && i == 1 {
                    paint_card(p, x, y, 20, 30, Rgb::hex(0x3a4a7a), Rgb::hex(0xbfe4ff), "?", false);
                    continue;
                }
                let red = c.1 == 1 || c.1 == 2;
                let ink = if red { Rgb::hex(0xc8202a) } else { Rgb::hex(0x1a1420) };
                paint_card(p, x, y, 20, 30, Rgb::hex(0xf4efe6), ink, &rank(c), false);
                p.art(x + 8, y + 21, suit(c.1), ink, ink);
            }
        };
        row(&mut p, 8, &self.them, !self.standing);
        row(&mut p, 66, &self.you, false);
        p.picture()
    }
}

fn card_box(f: &mut Frame, x: i32, y: i32, label: &str, c: Rgb, hot: bool, bg: Bg) {
    let edge = if hot { Rgb::hex(0xffd36b) } else { c * 0.8 };
    f.text(x, y, "╭──╮", edge, bg, 0.0);
    f.text(x, y + 1, "│", edge, bg, 0.0);
    let face = Bg::Solid(c * 0.55);
    let lab: String = format!("{label:^2}").chars().take(2).collect();
    f.text(x + 1, y + 1, &lab, Rgb::hex(0xfff8ec), face, 0.0);
    f.text(x + 3, y + 1, "│", edge, bg, 0.0);
    f.text(x, y + 2, "╰──╯", edge, bg, 0.0);
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = rng.int(0, i as i32 + 1) as usize;
        v.swap(i, j.min(i));
    }
}

// ----------------------------------------------------------- Twenty-One

struct TwentyOne {
    deck: Vec<(u8, u8)>,
    you: Vec<(u8, u8)>,
    them: Vec<(u8, u8)>,
    /// Standing (their turn, then the reckoning).
    standing: bool,
    over: Option<Result>,
}

fn total(hand: &[(u8, u8)]) -> u32 {
    let mut t: u32 = hand.iter().map(|c| (c.0 as u32).min(10)).sum();
    if hand.iter().any(|c| c.0 == 1) && t + 10 <= 21 {
        t += 10;
    }
    t
}

fn card_name(c: (u8, u8)) -> String {
    let r = match c.0 {
        1 => "A".to_string(),
        11 => "J".into(),
        12 => "Q".into(),
        13 => "K".into(),
        n => n.to_string(),
    };
    format!("{r}{}", ['♠', '♥', '♦', '♣'][c.1 as usize % 4])
}

impl TwentyOne {
    fn new(rng: &mut Rng) -> Self {
        let mut deck: Vec<(u8, u8)> = (0..52).map(|i| ((i % 13 + 1) as u8, (i / 13) as u8)).collect();
        shuffle(&mut deck, rng);
        let you = vec![deck.pop().unwrap(), deck.pop().unwrap()];
        let them = vec![deck.pop().unwrap(), deck.pop().unwrap()];
        let mut g = TwentyOne { deck, you, them, standing: false, over: None };
        if total(&g.you) == 21 {
            g.reckon();
        }
        g
    }

    /// The dealer's turn (to 17), then who's nearer 21.
    fn reckon(&mut self) {
        self.standing = true;
        while total(&self.them) < 17 {
            match self.deck.pop() {
                Some(c) => self.them.push(c),
                None => break,
            }
        }
        let (y, t) = (total(&self.you), total(&self.them));
        self.over = Some(if y > 21 {
            Result::Lost
        } else if t > 21 || y > t {
            Result::Won
        } else if y < t {
            Result::Lost
        } else {
            Result::Drawn
        });
    }

    fn key(&mut self, k: Key, _rng: &mut Rng, them: &str) -> Option<String> {
        match k {
            Key::Pick | Key::Up => {
                let c = self.deck.pop()?;
                self.you.push(c);
                let t = total(&self.you);
                if t > 21 {
                    self.standing = true;
                    self.over = Some(Result::Lost);
                    return Some(format!("{} — {t}, over!", card_name(c)));
                }
                if t == 21 {
                    self.reckon();
                }
                Some(format!("{} — that's {t}", card_name(c)))
            }
            Key::Other | Key::Down => {
                self.reckon();
                Some(format!("you stand on {} · {} has {}", total(&self.you), short(them), total(&self.them)))
            }
            _ => None,
        }
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, bg: Bg, them: &str) {
        let row = |f: &mut Frame, y: i32, hand: &[(u8, u8)], hide: bool| {
            for (i, &c) in hand.iter().enumerate() {
                let red = c.1 == 1 || c.1 == 2;
                let (txt, col) = if hide && i == 1 { ("??".to_string(), Rgb::hex(0x6a7aa8)) } else { (card_name(c), if red { Rgb::hex(0xe0404a) } else { Rgb::hex(0x1a1014) }) };
                let x = x0 + 4 + i as i32 * 7;
                f.text(x, y, "╭────╮", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x, y + 1, "│", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x + 1, y + 1, &format!("{txt:^4}"), col, Bg::Solid(Rgb::hex(0xf4efe6)), 0.0);
                f.text(x + 5, y + 1, "│", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x, y + 2, "╰────╯", Rgb::hex(0xe8e0d0), bg, 0.0);
            }
        };
        let theirs = if self.standing { format!("{} · {}", short(them), total(&self.them)) } else { format!("{} · ?", short(them)) };
        f.text(x0, y0, &theirs, pal::UI_DIM, bg, 0.0);
        row(f, y0 + 1, &self.them, !self.standing);
        f.text(x0, y0 + 8, &format!("you · {}", total(&self.you)), Rgb::hex(0xffd36b), bg, 0.0);
        row(f, y0 + 9, &self.you, false);
        if self.over.is_none() {
            f.text(x0 + 4, y0 + 14, "[Enter] hit    [S] stand", pal::UI_TEXT, bg, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chess_knows_its_rules() {
        let c = Chess::new();
        assert_eq!(c.moves().len(), 20, "twenty opening moves");
        // fool's mate: white's checkmated
        let mut c = Chess::new();
        for (f, t) in [(13, 21), (52, 36), (14, 30), (59, 31)] {
            c.play(Mv { from: f, to: t });
        }
        c.settle();
        assert_eq!(c.over, Some(Result::Lost));
        // the opponent finds a mate in one (back rank)
        let mut c = Chess { b: [0; 64], white: false, castle: [false; 4], ep: None, cur: (0, 0), picked: None, over: None, quiet: 0 };
        c.b[6] = 6; // white king g1
        c.b[13] = 1;
        c.b[14] = 1;
        c.b[15] = 1;
        c.b[60] = -6;
        c.b[59] = -4; // black rook d8
        let mut rng = Rng::new(3);
        c.their_move(&mut rng, "Agnes");
        assert_eq!(c.over, Some(Result::Lost), "missed the mate");
    }

    #[test]
    fn games_play_out_to_an_end() {
        let mut rng = Rng::new(7);
        for kind in Kind::ALL {
            for seed in 0..4u32 {
                let mut tbl = Table::new(kind, "Agnes", seed);
                let mut t = 0.0;
                for _ in 0..3000 {
                    if tbl.result.is_some() {
                        break;
                    }
                    t += 1.0;
                    tbl.update(t);
                    if tbl.theirs() || tbl.result.is_some() {
                        continue;
                    }
                    // the villager: something legal, by the keys
                    match &tbl.game {
                        Game::Chess(c) => {
                            let ms = c.moves();
                            let m = ms[rng.int(0, ms.len() as i32) as usize];
                            if let Game::Chess(c) = &mut tbl.game {
                                c.cur = ((m.from % 8) as i32, (m.from / 8) as i32);
                            }
                            tbl.key(Key::Pick, t);
                            if let Game::Chess(c) = &mut tbl.game {
                                c.cur = ((m.to % 8) as i32, (m.to / 8) as i32);
                            }
                            tbl.key(Key::Pick, t);
                        }
                        Game::Checkers(c) => {
                            let ms = c.moves();
                            let m = ms[rng.int(0, ms.len() as i32) as usize];
                            if let Game::Checkers(c) = &mut tbl.game {
                                c.cur = ((m.0 % 8) as i32, (m.0 / 8) as i32);
                                c.picked = None;
                            }
                            tbl.key(Key::Pick, t);
                            if let Game::Checkers(c) = &mut tbl.game {
                                c.cur = ((m.1 % 8) as i32, (m.1 / 8) as i32);
                            }
                            tbl.key(Key::Pick, t);
                        }
                        Game::Snow(s) => {
                            let k = if s.choosing.is_some() { Key::Pick } else { s.hands[0].iter().position(|&c| s.fits(c)).map(|_| Key::Right).unwrap_or(Key::Other) };
                            if k == Key::Right {
                                let i = s.hands[0].iter().position(|&c| s.fits(c)).unwrap();
                                if let Game::Snow(s) = &mut tbl.game {
                                    s.sel = i;
                                }
                                tbl.key(Key::Pick, t);
                            } else {
                                tbl.key(k, t);
                            }
                        }
                        Game::TwentyOne(g) => tbl.key(if total(&g.you) < 15 { Key::Pick } else { Key::Other }, t),
                    }
                }
                assert!(tbl.result.is_some() || kind == Kind::Chess, "{kind:?} never ended");
            }
        }
    }
}
