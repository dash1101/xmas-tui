//! Games at the table: chess, checkers, Snowflakes (a card game of colours
//! and numbers: match the colour or the number, skip, turn about, make them
//! draw, a snowflake plays on anything) and Twenty-One. Played with whoever's
//! at home (up to three of them at the cards), or with friends online at
//! the table in the games room; on a panel over the room: the arrow keys
//! (or the mouse, on the board, the cards and the buttons) to choose, Enter
//! to play, Esc to get up from the table. The cards come off a real deck,
//! sliding across the table to whoever's dealt them.

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::Rng;
use std::cell::{Cell, RefCell};

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

    /// Its number online (see `crate::net::ask`).
    pub fn code(self) -> u8 {
        match self {
            Kind::Chess => 1,
            Kind::Checkers => 2,
            Kind::Snowflakes => 3,
            Kind::TwentyOne => 4,
        }
    }

    pub fn from_code(c: u8) -> Option<Kind> {
        Kind::ALL.iter().copied().find(|k| k.code() == c)
    }

    pub fn icon(self) -> char {
        match self {
            Kind::Chess => '♞',
            Kind::Checkers => '◉',
            Kind::Snowflakes => '❄',
            Kind::TwentyOne => '♠',
        }
    }

    /// The most who can sit in (chess and checkers: two).
    pub fn seats(self) -> usize {
        match self {
            Kind::Chess | Kind::Checkers => 2,
            _ => 4,
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
    /// Twenty-One: the deal passes to the next at the table.
    Dealer,
    /// Deal (the first hand, online; or again).
    Deal,
}

/// How a game ended (for the scorecard): won, lost or drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Result {
    Won,
    Lost,
    Drawn,
}

/// Playing with friends online: the table (its number), who's at it seat
/// by seat (seat 0 deals, and deals again), this villager's id and seat,
/// how many moves in and hands dealt, and the game as it stands to send
/// (after a move).
#[derive(Clone, Debug)]
pub struct Seat {
    pub game: u32,
    pub seats: Vec<u32>,
    pub me: u32,
    pub seat: usize,
    pub seq: u32,
    pub hand: u32,
    pub out: Option<String>,
    /// When it was last sent (sent again while waiting for a move).
    pub sent: f32,
    /// Someone's got up from a game of two (or gone): it's off.
    pub left: bool,
}

impl Seat {
    /// Seat 0: the one who deals.
    pub fn owner(&self) -> bool {
        self.seats.first() == Some(&self.me)
    }

    /// Everyone else at the table (ids).
    pub fn others(&self) -> Vec<u32> {
        self.seats.iter().copied().filter(|&s| s != self.me).collect()
    }
}

/// What goes across the table: who's sitting where, how many hands have
/// been dealt, and the game from seat 0's side (None: waiting for the
/// first deal).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Wire {
    seats: Vec<u32>,
    names: Vec<String>,
    hand: u32,
    st: Option<State>,
}

/// Cards on the move: how many each place (the hands, this side's way
/// round, then the pile) had when last drawn, the cards on their way there
/// (place, which, from where in the picture, when they set off), and the
/// dealer's cards turning up.
#[derive(Default)]
struct Motion {
    seen: Vec<usize>,
    going: Vec<(usize, usize, (f32, f32), f32)>,
    shown: bool,
    flip: f32,
}

/// How long a card takes to cross the table (s).
const SLIDE: f32 = 0.32;

pub struct Table {
    pub kind: Kind,
    /// Everyone at the table, this side's way round: [0] the villager.
    pub names: Vec<String>,
    /// The ones the village plays for (whoever's at home).
    ai: Vec<bool>,
    /// Online: friends at the table (None: whoever's at home).
    pub net: Option<Seat>,
    /// Online, waiting for the first deal.
    pub waiting: bool,
    /// Online, dealt but waiting for everyone to sit down (since when:
    /// negative, not looked yet): no moves till then.
    pub to_sit: Option<f32>,
    game: Game,
    rng: Rng,
    /// What's happening (the line under the board).
    pub status: String,
    /// The game's over, and how; and it's gone on the scorecard.
    pub result: Option<Result>,
    pub scored: bool,
    /// The next move comes after a moment's thought (scene time).
    think_until: f32,
    /// Twenty-One: who deals (this side's seat), when the village deals.
    dealer: usize,
    /// Where the board was drawn (for clicks): x, y.
    origin: Cell<(i32, i32)>,
    /// Moved somewhere (its top-left), and where it's held while moving.
    pub pos: Option<(i32, i32)>,
    pub held: Option<(i32, i32)>,
    /// Where it was last drawn: (x, y, w).
    at: Cell<(i32, i32, i32)>,
    /// The pixel-art picture of the game, and what it showed.
    pic: RefCell<Option<(u64, std::sync::Arc<crate::frame::Picture>)>>,
    /// The buttons as drawn (x, y, width, what they do).
    buttons: RefCell<Vec<(i32, i32, i32, Key)>>,
    motion: RefCell<Motion>,
}

enum Game {
    Chess(Chess),
    Checkers(Checkers),
    Snow(Snow),
    TwentyOne(TwentyOne),
}

/// A new game of `kind` for `n` at the table (Twenty-One: `dealer` deals).
fn deal(kind: Kind, n: usize, dealer: usize, rng: &mut Rng) -> Game {
    match kind {
        Kind::Chess => Game::Chess(Chess::new()),
        Kind::Checkers => Game::Checkers(Checkers::new()),
        Kind::Snowflakes => Game::Snow(Snow::new(rng, n.clamp(2, 4))),
        Kind::TwentyOne => Game::TwentyOne(TwentyOne::new(rng, n.clamp(2, 4), dealer)),
    }
}

/// "Agnes", "Agnes & Sam", "Agnes, Sam & Bo".
fn and(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        n => format!("{} & {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

impl Table {
    /// A game with whoever's at home (up to three of them at the cards; one
    /// at chess and checkers).
    pub fn with(kind: Kind, others: &[&str], seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0x6a3e);
        let others: Vec<String> = others.iter().take(kind.seats() - 1).map(|s| s.to_string()).collect();
        let n = others.len() + 1;
        let mut names = vec!["you".to_string()];
        names.extend(others.iter().cloned());
        // (at Twenty-One, one of them deals to begin with)
        let dealer = if kind == Kind::TwentyOne { 1 } else { 0 };
        let game = deal(kind, n, dealer, &mut rng);
        let them = and(&others);
        let status = match kind {
            Kind::Chess => "you're white: arrows to a piece, Enter, then where it goes".to_string(),
            Kind::Checkers => "you're red, moving up: pick a piece, then where (jumps are a must)".to_string(),
            Kind::Snowflakes => "match the colour or the number · ← → choose · Enter play · D draw".to_string(),
            Kind::TwentyOne => format!("nearer 21 than the dealer ({}), without going over · P: pass the deal", short(&names[dealer])),
        };
        let _ = them;
        let mut t = Table::blank(kind, names, vec![false; n], rng, game, status);
        for i in 1..n {
            t.ai[i] = true;
        }
        t.dealer = dealer;
        t
    }

    fn blank(kind: Kind, names: Vec<String>, ai: Vec<bool>, rng: Rng, game: Game, status: String) -> Self {
        Table {
            kind,
            names,
            ai,
            net: None,
            waiting: false,
            to_sit: None,
            game,
            rng,
            status,
            result: None,
            scored: false,
            think_until: 0.0,
            dealer: 0,
            origin: Cell::new((0, 0)),
            pos: None,
            held: None,
            at: Cell::new((0, 0, 0)),
            pic: RefCell::new(None),
            buttons: RefCell::new(vec![]),
            motion: RefCell::new(Motion::default()),
        }
    }

    /// A game with friends online, dealt now: `seats` (their ids, seat 0
    /// first) and their names; this villager is `me`. The same seed deals
    /// the same game on every side.
    pub fn online(kind: Kind, game: u32, seats: Vec<u32>, names: Vec<String>, me: u32, seed: u32) -> Self {
        let n = seats.len().max(1);
        let seat = seats.iter().position(|&s| s == me).unwrap_or(0);
        let mut rng = Rng::new(seed as u64 ^ 0x6a3e);
        let canon = deal(kind, n, 0, &mut rng);
        let view_names = (0..n).map(|j| names.get((j + seat) % n).cloned().unwrap_or_default()).collect();
        let mut tb = Table::blank(kind, view_names, vec![false; n], rng, canon, String::new());
        if seat != 0 {
            let st = turn_round(tb.game.state(), seat);
            tb.game.adopt(st);
        }
        tb.net = Some(Seat { game, seats, me, seat, seq: 0, hand: 1, out: None, sent: 0.0, left: false });
        // (it starts once everyone's sat down at it)
        tb.to_sit = Some(-1.0);
        tb.status = format!("waiting for {} to sit down…", and(tb.names.get(1..).unwrap_or(&[])));
        tb
    }

    /// On its "✕ leave" (up from the table, as Esc).
    pub fn on_leave(&self, x: i32, y: i32) -> bool {
        let (x0, y0, w) = self.at.get();
        y == y0 && x >= x0 + w - 13 && x < x0 + w - 4
    }

    /// Everyone's at the table (or the wait's over): off it goes.
    pub fn all_sat(&mut self) {
        if self.to_sit.take().is_some() {
            self.status = self.opening();
        }
    }

    /// A table online, waiting for friends to sit down and the first deal
    /// (seat 0's to make).
    pub fn lobby(kind: Kind, game: u32, seats: Vec<u32>, names: Vec<String>, me: u32) -> Self {
        let n = seats.len();
        let seat = seats.iter().position(|&s| s == me).unwrap_or(0);
        let mut rng = Rng::new(game as u64 ^ 0x6a3e);
        let g = deal(kind, n.max(2), 0, &mut rng);
        // (not sat down yet, as far as anyone's said: just this villager)
        let view_names = if n == 0 { vec!["you".to_string()] } else { (0..n).map(|j| names.get((j + seat) % n).cloned().unwrap_or_default()).collect() };
        let mut tb = Table::blank(kind, view_names, vec![false; n.max(1)], rng, g, String::new());
        tb.net = Some(Seat { game, seats, me, seat, seq: 0, hand: 0, out: None, sent: 0.0, left: false });
        tb.waiting = true;
        tb.status = tb.lobby_said();
        tb
    }

    fn lobby_said(&self) -> String {
        let owner = self.net.as_ref().is_some_and(|n| n.owner());
        let others = and(self.names.get(1..).unwrap_or(&[]));
        match (owner, self.names.len()) {
            (true, 1) => "waiting for friends to sit down…".into(),
            (true, _) => format!("{others} at the table · Enter (or deal) when everyone's here"),
            (false, _) => format!("sat down · waiting for {} to deal", short(&self.dealer_name())),
        }
    }

    /// Seat 0's name, this side's way round (whoever deals online).
    fn dealer_name(&self) -> String {
        match &self.net {
            Some(n) => self.names.get((self.names.len() + 0 - n.seat % self.names.len().max(1)) % self.names.len().max(1)).cloned().unwrap_or_default(),
            None => self.names.get(self.dealer).cloned().unwrap_or_default(),
        }
    }

    /// What to say as a game begins.
    fn opening(&self) -> String {
        let seat = self.net.as_ref().map(|n| n.seat).unwrap_or(0);
        let others = and(&self.names[1..]);
        let first = short(self.names.get(1).map(|s| s.as_str()).unwrap_or("them"));
        match (self.kind, seat) {
            (Kind::Chess, 0) => format!("you're white, {first}'s black: your move"),
            (Kind::Chess, _) => format!("you're black (at the bottom): {first} moves first"),
            (Kind::Checkers, 0) => "you're red, moving up: your move first".to_string(),
            (Kind::Checkers, _) => format!("you're black (at the bottom): {first} moves first"),
            (Kind::Snowflakes, _) => format!("Snowflakes with {others} · match the colour or the number"),
            (Kind::TwentyOne, _) => match &self.game {
                Game::TwentyOne(g) if g.dealer == 0 => format!("you deal · nearer 21 than the dealer wins · {others} first"),
                Game::TwentyOne(g) => format!("{} deals · nearer 21 than the dealer, without going over", short(&self.names[g.dealer])),
                _ => String::new(),
            },
        }
    }

    /// The game as it stands, from seat 0's side, with who's where (for
    /// friends online).
    pub fn state_json(&self) -> String {
        let Some(net) = &self.net else { return serde_json::to_string(&self.game.state()).unwrap_or_default() };
        let n = self.names.len().max(1);
        let back = (n - net.seat % n) % n;
        let names = (0..n).map(|i| self.names[(i + back) % n].clone()).collect();
        let st = (!self.waiting).then(|| turn_round(self.game.state(), back));
        serde_json::to_string(&Wire { seats: net.seats.clone(), names, hand: net.hand, st }).unwrap_or_default()
    }

    /// The game as a friend has it after their move (from seat 0's side):
    /// taken on, turned round to this side.
    pub fn adopt(&mut self, seq: u32, json: &str) -> bool {
        let Some(net) = &self.net else { return false };
        if seq <= net.seq {
            return false;
        }
        let Ok(w) = serde_json::from_str::<Wire>(json) else { return false };
        let Some(seat) = w.seats.iter().position(|&s| s == net.me) else { return false };
        if w.st.as_ref().is_some_and(|st| st.kind() != self.kind) {
            return false;
        }
        let n = w.seats.len();
        let new_game = w.hand > net.hand;
        self.names = (0..n).map(|j| w.names.get((j + seat) % n).cloned().unwrap_or_default()).collect();
        self.ai = vec![false; n];
        if let Some(net) = &mut self.net {
            (net.seats, net.seat, net.seq, net.hand, net.out) = (w.seats, seat, seq, w.hand, None);
        }
        match w.st {
            None => {
                self.waiting = true;
                self.status = self.lobby_said();
            }
            Some(st) => {
                self.waiting = false;
                self.game.adopt(turn_round(st, seat));
                if new_game {
                    self.result = None;
                    self.scored = false;
                    *self.motion.borrow_mut() = Motion::default();
                    self.status = self.opening();
                } else {
                    self.status = self.last_move_said();
                }
            }
        }
        self.check_over();
        true
    }

    /// What was just done, as best it can be said.
    fn last_move_said(&self) -> String {
        let up = |i: usize| short(self.names.get(i).map(|s| s.as_str()).unwrap_or(""));
        match &self.game {
            Game::Chess(c) if c.in_check(true) => format!("{} moved — check!", up(1)),
            Game::Snow(s) if s.turn == 0 => format!("your turn · the {} {} is up", COLOURS[s.colour.min(3) as usize].0, s.top().label()),
            Game::Snow(s) => format!("{}'s turn · the {} {} is up", up(s.turn), COLOURS[s.colour.min(3) as usize].0, s.top().label()),
            Game::TwentyOne(g) => match g.up() {
                Some(0) => format!("your turn: {} (Enter hit · S stand)", total(&g.hands[0])),
                Some(u) => format!("{}'s turn", up(u)),
                None => String::new(),
            },
            _ => format!("{} moved · your turn", up(1)),
        }
    }

    /// Online: the state to send (once, after a move), and its number.
    pub fn take_out(&mut self) -> Option<(u32, String)> {
        let n = self.net.as_mut()?;
        let out = n.out.take()?;
        Some((n.seq, out))
    }

    /// Online: the move made here goes out.
    fn send(&mut self) {
        let s = self.state_json();
        if let Some(n) = &mut self.net {
            n.seq += 1;
            n.out = Some(s);
        }
    }

    /// Online, seat 0: someone sits down (between hands they're dealt in
    /// next time).
    pub fn sit_in(&mut self, id: u32, name: &str) {
        let Some(net) = &mut self.net else { return };
        if !net.owner() || net.seats.contains(&id) || net.seats.len() >= self.kind.seats() {
            return;
        }
        net.seats.push(id);
        self.names.push(name.to_string());
        self.ai.push(false);
        if self.waiting {
            self.status = self.lobby_said();
            self.send();
        }
    }

    /// Online, seat 0: someone's gone (dealt out from the next hand on).
    pub fn sit_out(&mut self, id: u32) {
        let Some(net) = &mut self.net else { return };
        let Some(k) = net.seats.iter().position(|&s| s == id) else { return };
        if net.seats.len() <= 2 || !net.owner() {
            net.left = true;
            return;
        }
        let name = self.names.get(k).cloned().unwrap_or_default();
        net.seats.remove(k);
        self.names.remove(k);
        self.ai.remove(k);
        self.status = format!("{} got up · a new hand", short(&name));
        self.result = Some(Result::Drawn);
        self.scored = true;
        self.deal_again();
    }

    /// Their turn (not this villager's)?
    pub fn theirs(&self) -> bool {
        if self.waiting {
            return true;
        }
        match &self.game {
            Game::Chess(c) => !c.white && c.over.is_none(),
            Game::Checkers(c) => !c.red && c.over.is_none(),
            Game::Snow(s) => s.turn != 0 && s.won.is_none(),
            Game::TwentyOne(g) => g.up().is_some_and(|u| u != 0),
        }
    }

    /// A new hand (or game): online, seat 0 deals it for everyone (and the
    /// deal passes round at Twenty-One).
    fn deal_again(&mut self) {
        let n = self.names.len().max(1);
        let pos = self.pos;
        match &self.net {
            Some(net) if !net.owner() => {
                self.status = format!("waiting for {} to deal again", short(&self.dealer_name()));
                return;
            }
            Some(net) => {
                let hand = net.hand + 1;
                let dealer = if self.kind == Kind::TwentyOne { (hand as usize - 1) % n } else { 0 };
                self.game = deal(self.kind, n, dealer, &mut self.rng);
                if let Some(net) = &mut self.net {
                    net.hand = hand;
                }
                self.waiting = false;
                self.send();
            }
            None => {
                self.game = deal(self.kind, n, self.dealer, &mut self.rng);
            }
        }
        self.result = None;
        self.scored = false;
        *self.motion.borrow_mut() = Motion::default();
        self.pos = pos;
        self.status = self.opening();
    }

    /// A key at the table.
    pub fn key(&mut self, k: Key, t: f32) {
        if self.to_sit.is_some() {
            return;
        }
        if self.waiting {
            if matches!(k, Key::Pick | Key::Deal) {
                if self.net.as_ref().is_some_and(|n| n.owner()) && self.names.len() >= 2 {
                    self.deal_again();
                } else {
                    self.status = self.lobby_said();
                }
            }
            return;
        }
        if self.net.as_ref().is_some_and(|n| n.left) {
            return;
        }
        // Twenty-One with the village: the deal passes on (a fresh hand)
        if k == Key::Dealer && self.kind == Kind::TwentyOne {
            let fresh = matches!(&self.game, Game::TwentyOne(g) if g.played == 0 && g.hands.iter().all(|h| h.len() == 2)) || self.result.is_some();
            if self.net.is_none() && fresh {
                self.dealer = (self.dealer + 1) % self.names.len().max(1);
                self.deal_again();
                self.status = if self.dealer == 0 { "you deal now: they play first, then you".into() } else { format!("{} deals now", short(&self.names[self.dealer])) };
            } else if self.net.is_some() {
                self.status = "online, the deal passes round with each hand".into();
            }
            return;
        }
        if self.result.is_some() {
            // a game over: Enter (or the button) for another
            if matches!(k, Key::Pick | Key::Deal) {
                self.deal_again();
            }
            return;
        }
        if self.theirs() {
            if self.net.is_some() {
                self.status = "not your turn yet — wait for them".into();
            }
            return;
        }
        let before = self.net.is_some().then(|| self.state_json());
        let names = self.names.clone();
        let msg = match &mut self.game {
            Game::Chess(c) => c.key(k),
            Game::Checkers(c) => c.key(k),
            Game::Snow(s) => s.key(k, &mut self.rng, &names),
            Game::TwentyOne(g) => g.key(k, &mut self.rng, &names),
        };
        if let Some(m) = msg {
            self.status = m;
        }
        self.think_until = t + 0.9;
        self.check_over();
        // online: a move made goes to everyone at the table
        if let Some(before) = before {
            if self.state_json() != before {
                self.send();
            }
        }
    }

    /// Tests: a chess move by the keys (from file, rank to file, rank).
    #[cfg(test)]
    pub fn test_move(&mut self, fx: i32, fy: i32, tx: i32, ty: i32) {
        if let Game::Chess(c) = &mut self.game {
            c.cur = (fx, fy);
        }
        self.key(Key::Pick, 0.0);
        if let Game::Chess(c) = &mut self.game {
            c.cur = (tx, ty);
        }
        self.key(Key::Pick, 0.0);
    }

    /// A move for seat `p` (this side's way round), thought out by the
    /// village: whoever's at home, or the villager on autopilot.
    fn think(&mut self, p: usize) -> Option<String> {
        let names = self.names.clone();
        let who = short(names.get(p).map(|s| s.as_str()).unwrap_or("them"));
        let me = |m: Option<String>| m.map(|s| s.replacen("you moves", "you move", 1).replacen("you takes", "you take", 1).replacen("you jumps", "you jump", 1).replacen("you plays", "you play", 1).replacen("you draws", "you draw", 1).replacen("you skips", "you skip", 1));
        match &mut self.game {
            Game::Chess(c) => me(c.their_move(&mut self.rng, &who)),
            Game::Checkers(c) => me(c.their_move(&mut self.rng, &who)),
            Game::Snow(s) => me(s.their_turn(&mut self.rng, &names)),
            Game::TwentyOne(g) => g.think(&mut self.rng, &names),
        }
    }

    /// The villager's move, made for them (autopilot): the same thinking
    /// as everyone else's, from the villager's side.
    pub fn auto_move(&mut self, t: f32) {
        if self.result.is_some() || self.theirs() || self.net.is_some() {
            return;
        }
        if let Some(m) = self.think(0) {
            self.status = m;
        }
        self.think_until = t + 0.9;
        self.check_over();
    }

    /// On its title bar (picked up there, and moved).
    pub fn on_title(&self, x: i32, y: i32) -> bool {
        let (x0, y0, w) = self.at.get();
        y == y0 && x >= x0 && x < x0 + w && !self.on_leave(x, y)
    }

    /// On the table's panel anywhere (clicks there are the game's).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (x0, y0, w) = self.at.get();
        w > 0 && x >= x0 && x < x0 + w && y >= y0 && y < y0 + 24
    }

    /// Where it's drawn (its top-left).
    pub fn corner(&self) -> (i32, i32) {
        let (x0, y0, _) = self.at.get();
        (x0, y0)
    }

    /// A click (screen cell): a button, a square on the board, a card in
    /// the hand or the deck. Anywhere else, nothing.
    pub fn click(&mut self, x: i32, y: i32, t: f32) {
        if self.to_sit.is_some() {
            return;
        }
        let b = self.buttons.borrow().iter().find(|b| y == b.1 && x >= b.0 && x < b.0 + b.2).map(|b| b.3);
        if let Some(k) = b {
            return self.key(k, t);
        }
        let (ox, oy) = self.origin.get();
        let hit = match &mut self.game {
            Game::Chess(c) => board_hit(x - ox, y - oy).map(|(cx, cy)| {
                c.cur = (cx, cy);
            }),
            Game::Checkers(c) => board_hit(x - ox, y - oy).map(|(cx, cy)| {
                c.cur = (cx, cy);
            }),
            Game::Snow(s) => {
                let pics = crate::frame::pictures_ok();
                let i = if pics { ((x - ox) * 3 - 4) / 12 } else { (x - ox - 2) / 5 };
                let rows = if pics { 12..=16 } else { 12..=14 };
                // (the deck: draw)
                let deck = if pics { (26..=32).contains(&(x - ox)) && (6..=9).contains(&(y - oy)) } else { (26..=29).contains(&(x - ox)) && (4..=6).contains(&(y - oy)) };
                if deck && s.choosing.is_none() {
                    s.sel = s.hands[0].len();
                    Some(())
                } else {
                    (rows.contains(&(y - oy)) && i >= 0 && (i as usize) < s.hands[0].len().min(12)).then(|| s.sel = i as usize)
                }
            }
            Game::TwentyOne(_) => None,
        };
        if hit.is_some() {
            self.key(Key::Pick, t);
        }
    }

    /// The others' moves (whoever's at home), once they've had a think.
    pub fn update(&mut self, t: f32) {
        if self.net.is_some() || self.result.is_some() || !self.theirs() || t < self.think_until {
            return;
        }
        let p = match &self.game {
            Game::Chess(_) | Game::Checkers(_) => 1,
            Game::Snow(s) => s.turn,
            Game::TwentyOne(g) => g.up().unwrap_or(0),
        };
        if !self.ai.get(p).copied().unwrap_or(false) {
            return;
        }
        if let Some(m) = self.think(p) {
            self.status = m;
        }
        self.think_until = t + 0.9;
        self.check_over();
    }

    fn over(&self) -> Option<Result> {
        if self.waiting {
            return None;
        }
        match &self.game {
            Game::Chess(c) => c.over,
            Game::Checkers(c) => c.over,
            Game::Snow(s) => s.won.map(|w| if w == 0 { Result::Won } else { Result::Lost }),
            Game::TwentyOne(g) => g.shown.then(|| g.result_for(0)),
        }
    }

    fn check_over(&mut self) {
        let Some(r) = self.over() else { return };
        if self.result.is_some() {
            return;
        }
        self.result = Some(r);
        let again = match &self.net {
            Some(n) if !n.owner() => format!("{} deals again", short(&self.dealer_name())),
            _ => "Enter: again".to_string(),
        };
        let them = and(&self.names[1..]);
        self.status = match (&self.game, r) {
            (Game::TwentyOne(g), _) => format!("{} · {again}", g.summary(&self.names)),
            (Game::Snow(s), Result::Lost) => format!("{} wins this one! · {again}", short(&self.names[s.won.unwrap_or(1)])),
            (_, Result::Won) => format!("you win! well played, {them} · {again} · Esc: up"),
            (_, Result::Lost) => format!("{} wins this one · {again} · Esc: up", short(&self.names[1])),
            (_, Result::Drawn) => format!("a draw! · {again} · Esc: up"),
        };
    }

    /// The buttons under the game, as things stand.
    fn button_keys(&self) -> Vec<(&'static str, Key)> {
        let owner = self.net.as_ref().is_none_or(|n| n.owner());
        if self.waiting {
            return if owner && self.names.len() >= 2 { vec![("deal", Key::Deal)] } else { vec![] };
        }
        if self.net.as_ref().is_some_and(|n| n.left) {
            return vec![];
        }
        let mut v = vec![];
        if self.result.is_some() {
            if owner {
                v.push(("again", Key::Deal));
            }
            if self.kind == Kind::TwentyOne && self.net.is_none() {
                v.push(("pass the deal", Key::Dealer));
            }
            return v;
        }
        match &self.game {
            Game::TwentyOne(g) => {
                if g.up() == Some(0) {
                    v.push(("hit", Key::Pick));
                    v.push(("stand", Key::Other));
                }
                if self.net.is_none() && g.played == 0 && g.hands.iter().all(|h| h.len() == 2) {
                    v.push(("pass the deal", Key::Dealer));
                }
            }
            Game::Snow(s) if s.turn == 0 && s.choosing.is_none() => v.push(("draw a card", Key::Other)),
            _ => {}
        }
        v
    }

    /// The table: a panel in the middle of the screen (`t`: scene time,
    /// for the cards on the move).
    pub fn draw(&self, f: &mut Frame, look: &str, t: f32) {
        let (w, h) = (54, 24);
        let (x0, y0) = match self.pos {
            Some((x, y)) => (x.clamp(0, (f.w as i32 - w).max(0)), y.clamp(0, (f.h as i32 - h).max(0))),
            None => ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(0)),
        };
        self.at.set((x0, y0, w));
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
        let them = and(&self.names[1..]);
        let title: String = format!(" {} {} with {}{} ", self.kind.icon(), self.kind.name(), if them.is_empty() { "…" } else { &them }, if self.net.is_some() { " (online)" } else { "" }).chars().take((w - 18) as usize).collect();
        f.text(x0 + 2, y0, &title, pal::UI_TITLE, bg, 0.0);
        f.text(x0 + w - 13, y0, " ✕ leave ", pal::UI_TITLE, Bg::Mix(Rgb::hex(0x3a4a7a), 0.9), 0.0);
        f.text(x0 + w - 16, y0, "⠿", pal::UI_DIM, bg, 0.0);
        let (bx, by) = (x0 + 2, y0 + 2);
        self.origin.set((bx, by));
        if self.waiting {
            // who's sat down so far
            f.text(bx, by + 1, "at the table:", pal::UI_DIM, bg, 0.0);
            for (i, n) in self.names.iter().enumerate() {
                f.text(bx + 2, by + 3 + i as i32, &format!("☺ {n}"), Rgb::hex(0xffe9a8), bg, 0.0);
            }
        } else if crate::frame::pictures_ok() {
            // drawn as pixel art (easier on the eye than characters); only
            // made again when something's changed (or a card's on the move)
            self.draw_pixels(f, bx, by, bg, t);
        } else {
            match &self.game {
                Game::Chess(c) => c.draw(f, bx + 9, by, bg),
                Game::Checkers(c) => c.draw(f, bx + 9, by, bg),
                Game::Snow(s) => s.draw(f, bx, by, bg, &self.names),
                Game::TwentyOne(g) => g.draw(f, bx, by, bg, &self.names),
            }
        }
        if matches!(self.game, Game::Chess(_) | Game::Checkers(_)) && !self.waiting {
            self.origin.set((bx + 9, by));
        }
        // the buttons
        let mut btns = vec![];
        let mut x = x0 + 2;
        let row = y0 + h - 4;
        for (label, k) in self.button_keys() {
            let s = format!(" {label} ");
            let n = s.chars().count() as i32;
            let hot = self.at.get() == (x0, y0, w);
            let _ = hot;
            f.text(x, row, &s, Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x34406a), 0.95), 0.0);
            btns.push((x, row, n, k));
            x += n + 2;
        }
        *self.buttons.borrow_mut() = btns;
        // what's happening
        let line: String = self.status.chars().take((w - 4) as usize).collect();
        f.text(x0 + 2, y0 + h - 2, &line, if self.result.is_some() { Rgb::hex(0xffd36b) } else { pal::UI_TEXT }, bg, 0.0);
        if self.theirs() && self.result.is_none() && !self.waiting {
            let who = match &self.game {
                Game::Snow(s) => s.turn,
                Game::TwentyOne(g) => g.up().unwrap_or(1),
                _ => 1,
            };
            let name = short(self.names.get(who).map(|s| s.as_str()).unwrap_or(""));
            let s = if self.net.is_some() { format!("{name}'s move…") } else { format!("{name} thinks…") };
            f.text(x0 + w - 2 - s.chars().count() as i32, y0 + h - 4, &s, pal::UI_DIM, bg, 0.0);
        }
        crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h }, look, bg, 17);
    }
}

impl Table {
    /// Online, the second seat at chess or checkers: this side's pieces
    /// are the dark ones.
    fn dark(&self) -> bool {
        self.net.as_ref().is_some_and(|n| n.seat == 1)
    }

    /// The game in pixel art: the board (32×16 cells), or the cards.
    fn draw_pixels(&self, f: &mut Frame, bx: i32, by: i32, bg: Bg, t: f32) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // the cards: any just dealt or played set off across the table
        let moving = self.track(t);
        let (cols, rows, ox) = match &self.game {
            Game::Chess(c) => {
                (c.b, c.cur, c.picked, self.dark()).hash(&mut h);
                (32, 16, 9)
            }
            Game::Checkers(c) => {
                (c.b, c.cur, c.picked, self.dark()).hash(&mut h);
                (32, 16, 9)
            }
            Game::Snow(s) => {
                (s.hands.iter().map(|hd| hd.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>()).collect::<Vec<_>>(), s.top().0, s.top().1, s.colour, s.sel, s.choosing, s.deck.len()).hash(&mut h);
                (50, 18, 0)
            }
            Game::TwentyOne(g) => {
                (&g.hands, g.shown, g.dealer, g.turn, g.played).hash(&mut h);
                (50, 18, 0)
            }
        };
        if moving {
            ((t * 30.0) as i64).hash(&mut h);
        }
        let key = h.finish();
        let pic = {
            let mut cache = self.pic.borrow_mut();
            match cache.as_ref() {
                Some((k, p)) if *k == key => p.clone(),
                _ => {
                    let p = std::sync::Arc::new(self.paint(t));
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
                let n = s.hands.len();
                let seg = 50 / (n - 1).max(1) as i32;
                for p in 1..n {
                    let name = short(&self.names[p]);
                    let mark = if s.turn == p && s.won.is_none() { "▸" } else { " " };
                    f.text(bx + (p as i32 - 1) * seg, by, &format!("{mark}{name} {}", s.hands[p].len()), if s.turn == p { Rgb::hex(0xffd36b) } else { pal::UI_DIM }, bg, 0.0);
                }
                f.text(bx, by + 17, &format!("your hand · {} cards  (← → choose · Enter play · D draw)", s.hands[0].len()), pal::UI_DIM, bg, 0.0);
            }
            Game::TwentyOne(g) => {
                let d = g.dealer;
                let hide = d != 0 && !g.shown;
                let dt = if hide { "?".to_string() } else { total(&g.hands[d]).to_string() };
                f.text(bx, by, &format!("{} · {dt}", if d == 0 { "you deal".to_string() } else { format!("{} deals", short(&self.names[d])) }), if g.up() == Some(d) { Rgb::hex(0xffd36b) } else { pal::UI_DIM }, bg, 0.0);
                let mid: Vec<usize> = (1..g.hands.len()).filter(|&i| i != d).collect();
                let seg = 50 / mid.len().max(1) as i32;
                for (k, &i) in mid.iter().enumerate() {
                    let mark = if g.up() == Some(i) { "▸" } else { " " };
                    f.text(bx + k as i32 * seg, by + 6, &format!("{mark}{} {}", short(&self.names[i]), total(&g.hands[i])), if g.up() == Some(i) { Rgb::hex(0xffd36b) } else { pal::UI_DIM }, bg, 0.0);
                }
                if d != 0 {
                    f.text(bx, by + 17, &format!("you · {}", total(&g.hands[0])), Rgb::hex(0xffd36b), bg, 0.0);
                }
            }
            _ => {}
        }
    }

    /// Where each card lies in the cards' picture (place: a hand, this
    /// side's way round, or the pile, after the hands), and the deck.
    fn layout(&self) -> (Vec<Vec<(i32, i32, i32, i32)>>, (i32, i32)) {
        match &self.game {
            Game::Snow(s) => {
                let n = s.hands.len();
                let seg = 150 / (n - 1).max(1) as i32;
                let mut v: Vec<Vec<(i32, i32, i32, i32)>> = vec![];
                v.push(s.hands[0].iter().enumerate().map(|(i, _)| (4 + (i.min(11)) as i32 * 12, 78 - if i == s.sel && s.choosing.is_none() { 6 } else { 0 }, 14, 22)).collect());
                for p in 1..n {
                    v.push(s.hands[p].iter().enumerate().map(|(i, _)| ((p as i32 - 1) * seg + 4 + (i.min(5) as i32) * 7, 10, 12, 17)).collect());
                }
                v.push(s.pile.iter().map(|_| (50, 36, 18, 26)).collect());
                (v, (80, 36))
            }
            Game::TwentyOne(g) => {
                let n = g.hands.len();
                let d = g.dealer;
                let mid: Vec<usize> = (1..n).filter(|&i| i != d).collect();
                let seg = 150 / mid.len().max(1) as i32;
                let v = (0..n)
                    .map(|p| {
                        g.hands[p]
                            .iter()
                            .enumerate()
                            .map(|(i, _)| {
                                let i = i as i32;
                                if p == d {
                                    (6 + i * 16, 6, 18, 26)
                                } else if p == 0 {
                                    (6 + i * 22, 80, 20, 30)
                                } else {
                                    let k = mid.iter().position(|&m| m == p).unwrap_or(0) as i32;
                                    (k * seg + 4 + i * 10, 48, 12, 17)
                                }
                            })
                            .collect()
                    })
                    .collect();
                (v, (124, 6))
            }
            _ => (vec![], (0, 0)),
        }
    }

    /// Cards newly in a hand or on the pile set off across the table (from
    /// the deck, or from the hand they were played from); whether any are
    /// still on the move.
    fn track(&self, t: f32) -> bool {
        let counts: Vec<usize> = match &self.game {
            Game::Snow(s) => s.hands.iter().map(|h| h.len()).chain(std::iter::once(s.pile.len())).collect(),
            Game::TwentyOne(g) => g.hands.iter().map(|h| h.len()).collect(),
            _ => return false,
        };
        let (lay, deck) = self.layout();
        let mut m = self.motion.borrow_mut();
        let shown = matches!(&self.game, Game::TwentyOne(g) if g.shown);
        if shown && !m.shown {
            m.flip = t;
        }
        m.shown = shown;
        let fresh = m.seen.len() != counts.len();
        if fresh {
            m.going.clear();
        }
        let old = if fresh { vec![0; counts.len()] } else { m.seen.clone() };
        // the hand a card was played from (the pile grew; a hand shrank)
        let from_hand = (0..counts.len().saturating_sub(1)).find(|&p| counts[p] < old.get(p).copied().unwrap_or(0));
        let mut k = 0;
        // (a whole deal: round the table a card at a time)
        let deal_order: Vec<(usize, usize)> = {
            let mut v = vec![];
            let most = counts.iter().copied().max().unwrap_or(0);
            for i in 0..most {
                for p in 0..counts.len() {
                    if i >= old.get(p).copied().unwrap_or(0) && i < counts[p] {
                        v.push((p, i));
                    }
                }
            }
            v
        };
        for (p, i) in deal_order {
            let pile = matches!(self.game, Game::Snow(_)) && p == counts.len() - 1;
            let from = match (pile, from_hand) {
                (true, Some(h)) => lay.get(h).and_then(|v| v.first()).map(|r| (r.0 as f32, r.1 as f32)).unwrap_or((deck.0 as f32, deck.1 as f32)),
                _ => (deck.0 as f32, deck.1 as f32),
            };
            // (the first card on the pile, dealt: no need to fly)
            if pile && fresh && i == 0 {
                continue;
            }
            m.going.push((p, i, from, t + k as f32 * if fresh { 0.09 } else { 0.0 }));
            k += 1;
        }
        m.seen = counts;
        m.going.retain(|g| t - g.3 < SLIDE);
        !m.going.is_empty() || t - m.flip < SLIDE
    }

    fn paint(&self, t: f32) -> crate::frame::Picture {
        match &self.game {
            Game::Chess(c) => {
                let dark = self.dark();
                let targets: Vec<usize> = c.picked.map(|p| c.moves().into_iter().filter(|m| m.from == p).map(|m| m.to).collect()).unwrap_or_default();
                paint_board(c.cur, c.picked, &targets, |i| {
                    let p = c.b[i];
                    (p != 0).then(|| (CHESS_ART[(p.unsigned_abs() - 1) as usize], (p > 0) != dark))
                })
            }
            Game::Checkers(c) => {
                let dark = self.dark();
                let targets: Vec<usize> = c.picked.map(|p| c.moves().into_iter().filter(|m| m.0 == p).map(|m| m.1).collect()).unwrap_or_default();
                paint_board(c.cur, c.picked, &targets, |i| {
                    let p = c.b[i];
                    (p != 0).then(|| (if p.abs() == 2 { KING_MAN } else { MAN }, (p > 0) != dark))
                })
            }
            Game::Snow(_) | Game::TwentyOne(_) => self.paint_cards(t),
        }
    }

    /// The card games: the felt, the deck (a stack of backs), the hands and
    /// the pile, any card on the move drawn where it's got to.
    fn paint_cards(&self, t: f32) -> crate::frame::Picture {
        let (w, h) = (150, 112);
        let mut p = Px::clear(w, h);
        // the table's felt
        for y in 0..h {
            for x in 0..w {
                let edge = x.min(y).min(w - 1 - x).min(h - 1 - y);
                if edge > 1 {
                    p.set(x, y, Rgb::hex(0x1e4a32).lerp(Rgb::hex(0x16382a), ((x * 7 + y * 13) % 5) as f32 * 0.06));
                }
            }
        }
        let (lay, deck) = self.layout();
        let m = self.motion.borrow();
        // the deck: a few backs, stacked
        let left = match &self.game {
            Game::Snow(s) => s.deck.len(),
            Game::TwentyOne(g) => g.deck.len(),
            _ => 0,
        };
        let (dw, dh) = if matches!(self.game, Game::Snow(_)) { (18, 26) } else { (18, 26) };
        let n = (left / 8).clamp(1, 5) as i32;
        for k in 0..n {
            let on = matches!(&self.game, Game::Snow(s) if s.sel >= s.hands[0].len() && s.choosing.is_none() && s.turn == 0) && k == n - 1;
            back(&mut p, deck.0 + k, deck.1 - k, dw, dh, on);
        }
        // where a card is: its spot, or on its way there
        let at = |pl: usize, i: usize, r: (i32, i32, i32, i32)| -> (i32, i32, f32) {
            match m.going.iter().find(|g| g.0 == pl && g.1 == i) {
                Some(&(_, _, from, start)) => {
                    let u = ((t - start) / SLIDE).clamp(0.0, 1.0);
                    if u <= 0.0 {
                        return (deck.0, deck.1, -1.0);
                    }
                    let e = u * u * (3.0 - 2.0 * u);
                    ((from.0 + (r.0 as f32 - from.0) * e).round() as i32, (from.1 + (r.1 as f32 - from.1) * e).round() as i32, u)
                }
                None => (r.0, r.1, 1.0),
            }
        };
        match &self.game {
            Game::Snow(s) => {
                let np = s.hands.len();
                let lab = |c: Card| match c.1 {
                    0..=9 => c.1.to_string(),
                    10 => "X".into(),
                    11 => "R".into(),
                    12 => "+2".into(),
                    14 => "+4".into(),
                    _ => String::new(),
                };
                // the pile (its colour to match)
                let top = s.top();
                let pi = s.pile.len() - 1;
                if s.pile.len() > 1 {
                    let under = s.pile[pi - 1];
                    paint_card(&mut p, 50, 36, 18, 26, Rgb::hex(COLOURS[under.0.min(3) as usize].1) * 0.8, Rgb::hex(0xffffff), &lab(under), false);
                }
                let (x, y, u) = at(np, pi, (50, 36, 18, 26));
                if u >= 0.0 {
                    paint_card(&mut p, x, y, 18, 26, Rgb::hex(COLOURS[s.colour.min(3) as usize].1), Rgb::hex(0xffffff), &lab(top), false);
                }
                // the others' hands, face down
                for (pl, hand) in s.hands.iter().enumerate().skip(1) {
                    for i in 0..hand.len().min(6) {
                        let r = lay[pl][i];
                        let (x, y, u) = at(pl, i, r);
                        if u >= 0.0 {
                            back(&mut p, x, y, r.2, r.3, false);
                        }
                    }
                }
                // this villager's, face up (face down until it lands)
                for (i, c) in s.hands[0].iter().enumerate().take(12) {
                    let hot = i == s.sel && s.choosing.is_none() && s.turn == 0;
                    let r = lay[0][i];
                    let (x, y, u) = at(0, i, r);
                    if u < 0.0 {
                        continue;
                    }
                    if u < 0.75 {
                        back(&mut p, x, y, r.2, r.3, false);
                        continue;
                    }
                    let face = c.colour() * if c.0 == 4 { 0.7 } else { 1.0 };
                    paint_card(&mut p, x, y, 14, 22, face, Rgb::hex(0xffffff), &lab(*c), hot);
                    if c.0 == 4 {
                        p.art(x + 4, y + 13, &[" + ", "+++", " + "], Rgb::hex(0xffffff), Rgb::hex(0xffffff));
                    }
                }
                // a snowflake's colour being chosen
                if let Some(sel) = s.choosing {
                    for (k, (_, col)) in COLOURS.iter().enumerate() {
                        let hot = k as u8 == sel;
                        let x = 30 + k as i32 * 24;
                        p.rect(x - 1, 102 - 1, 20, 9, if hot { Rgb::hex(0xffd36b) } else { Rgb::hex(0x1a1420) });
                        p.rect(x, 102, 18, 7, Rgb::hex(*col));
                    }
                }
            }
            Game::TwentyOne(g) => {
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
                let d = g.dealer;
                let flip_u = ((t - m.flip) / SLIDE).clamp(0.0, 1.0);
                for (pl, hand) in g.hands.iter().enumerate() {
                    for (i, &c) in hand.iter().enumerate() {
                        let r = lay[pl][i];
                        let (x, y, u) = at(pl, i, r);
                        if u < 0.0 {
                            continue;
                        }
                        // the dealer's cards after the first stay face down
                        // till everyone's played (turning over, then)
                        let hidden = pl == d && d != 0 && i >= 1 && (!g.shown || flip_u < 0.5);
                        if u < 0.75 || hidden {
                            let squash = if pl == d && g.shown && flip_u < 0.5 { 1.0 - flip_u * 2.0 } else { 1.0 };
                            let cw = ((r.2 as f32 * squash).round() as i32).max(2);
                            back(&mut p, x + (r.2 - cw) / 2, y, cw, r.3, false);
                            continue;
                        }
                        let red = c.1 == 1 || c.1 == 2;
                        let ink = if red { Rgb::hex(0xc8202a) } else { Rgb::hex(0x1a1420) };
                        let grow = if pl == d && d != 0 && g.shown && flip_u < 1.0 && i >= 1 { (flip_u - 0.5) * 2.0 } else { 1.0 };
                        let cw = ((r.2 as f32 * grow).round() as i32).max(2);
                        let cx = x + (r.2 - cw) / 2;
                        let label = if cw > 8 { rank(c) } else { String::new() };
                        paint_card(&mut p, cx, y, cw, r.3, Rgb::hex(0xf4efe6), ink, &label, false);
                        if r.2 >= 18 && cw >= 14 {
                            p.art(cx + cw / 2 - 2, y + r.3 - 9, suit(c.1), ink, ink);
                        }
                    }
                }
            }
            _ => {}
        }
        p.picture()
    }
}

/// A card's back: blue, a snowflake on it (`hot`: picked out).
fn back(p: &mut Px, x: i32, y: i32, w: i32, h: i32, hot: bool) {
    paint_card(p, x, y, w, h, Rgb::hex(0x3a4a7a), Rgb::hex(0xbfe4ff), "", hot);
    if w >= 9 {
        p.art(x + w / 2 - 1, y + h / 2 - 1, &[" + ", "+++", " + "], Rgb::hex(0xbfe4ff), Rgb::hex(0xbfe4ff));
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
    /// Hands, round the table: the villager's first.
    hands: Vec<Vec<Card>>,
    turn: usize,
    dir: i32,
    sel: usize,
    /// Choosing a colour for a snowflake just played.
    choosing: Option<u8>,
    /// Out of cards first: who (the game's over).
    won: Option<usize>,
}

impl Snow {
    fn new(rng: &mut Rng, n: usize) -> Self {
        let mut deck = vec![];
        for c in 0..4u8 {
            deck.push(Card(c, 0));
            for k in 1..=12u8 {
                deck.push(Card(c, k));
                deck.push(Card(c, k));
            }
        }
        for _ in 0..4 {
            deck.push(Card(4, 13));
            deck.push(Card(4, 14));
        }
        shuffle(&mut deck, rng);
        let hands: Vec<Vec<Card>> = (0..n).map(|_| deck.split_off(deck.len() - 7)).collect();
        // the first card face up is a number
        let i = deck.iter().position(|c| c.1 <= 9).unwrap_or(0);
        let first = deck.remove(i);
        Snow { deck, pile: vec![first], colour: first.0, hands, turn: 0, dir: 1, sel: 0, choosing: None, won: None }
    }

    fn top(&self) -> Card {
        *self.pile.last().unwrap_or(&Card(0, 0))
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
            self.won = Some(p);
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

    fn key(&mut self, k: Key, rng: &mut Rng, names: &[String]) -> Option<String> {
        let up = |s: &Self| short(names.get(s.turn).map(|s| s.as_str()).unwrap_or(""));
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
                    return Some(format!("snowflake! the colour's {} · {}'s turn", COLOURS[colour as usize].0, up(self)));
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
            let c = self.draw_card(rng)?;
            self.hands[0].push(c);
            if self.fits(c) {
                self.sel = self.hands[0].len() - 1;
                return Some("you draw — and it fits! Enter to play it".into());
            }
            self.turn = self.next(1);
            return Some(format!("you draw a card · {}'s turn", up(self)));
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
        Some(if self.hands[0].len() == 1 { "snowflake! one card left!".into() } else { format!("{}'s turn", up(self)) })
    }

    /// Whoever's turn it is plays (a fitting card, holding the snowflakes
    /// back; or draws).
    fn their_turn(&mut self, rng: &mut Rng, names: &[String]) -> Option<String> {
        let p = self.turn;
        let name = short(names.get(p).map(|s| s.as_str()).unwrap_or("them"));
        let hand = &self.hands[p];
        let fits: Vec<usize> = (0..hand.len()).filter(|&i| self.fits(hand[i])).collect();
        let pick = fits.iter().copied().filter(|&i| hand[i].0 != 4).max_by_key(|&i| hand[i].1).or_else(|| fits.first().copied());
        let you = |s: &Self| if s.turn == 0 { "your turn".to_string() } else { format!("{}'s turn", short(names.get(s.turn).map(|s| s.as_str()).unwrap_or(""))) };
        match pick {
            Some(i) => {
                let c = self.hands[p][i];
                // their most-held colour, for a snowflake
                let colour = (0..4u8).max_by_key(|&k| self.hands[p].iter().filter(|c| c.0 == k).count()).unwrap_or(0);
                self.play(p, i, colour, rng);
                Some(match c.1 {
                    13 | 14 => format!("{name} plays a snowflake: {}! · {}", COLOURS[colour as usize].0, you(self)),
                    10 => format!("{name} skips a turn round! · {}", you(self)),
                    12 => format!("{name}: draw two! · {}", you(self)),
                    _ if self.won.is_none() && self.hands[p].len() == 1 => format!("{name}: snowflake! one card left"),
                    _ => format!("{name} plays a {} {} · {}", COLOURS[c.0.min(3) as usize].0, c.label(), you(self)),
                })
            }
            None => {
                if let Some(c) = self.draw_card(rng) {
                    self.hands[p].push(c);
                }
                self.turn = self.next(1);
                Some(format!("{name} draws a card · {}", you(self)))
            }
        }
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, bg: Bg, names: &[String]) {
        // the others' hands, face down
        let others: Vec<String> = (1..self.hands.len()).map(|p| format!("{} {}", short(&names[p]), self.hands[p].len())).collect();
        f.text(x0, y0, &others.join(" · "), pal::UI_DIM, bg, 0.0);
        for i in 0..self.hands.get(1).map(|h| h.len()).unwrap_or(0).min(16) {
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

// ----------------------------------------------------------- Twenty-One (play)

/// Twenty-One round a table: one deals; the rest play in turn (hit for
/// another card, or stand) and the dealer last; then each is nearer 21 than
/// the dealer, or not. A dealer the village plays for keeps to the house
/// rule (another card under 17); a villager (or a friend) dealing chooses.
struct TwentyOne {
    deck: Vec<(u8, u8)>,
    /// Hands, round the table: the villager's first.
    hands: Vec<Vec<(u8, u8)>>,
    dealer: usize,
    /// Whose turn, and how many have played.
    turn: usize,
    played: usize,
    stood: Vec<bool>,
    /// Everyone's played: the dealer's cards shown, and the reckoning.
    shown: bool,
}

impl TwentyOne {
    fn new(rng: &mut Rng, n: usize, dealer: usize) -> Self {
        let mut deck: Vec<(u8, u8)> = (0..52).map(|i| ((i % 13 + 1) as u8, (i / 13) as u8)).collect();
        shuffle(&mut deck, rng);
        let dealer = dealer % n;
        let mut hands = vec![vec![]; n];
        // two each, a card at a time round the table from the dealer's left
        for _ in 0..2 {
            for k in 1..=n {
                if let Some(c) = deck.pop() {
                    hands[(dealer + k) % n].push(c);
                }
            }
        }
        let mut g = TwentyOne { deck, hands, dealer, turn: (dealer + 1) % n, played: 0, stood: vec![false; n], shown: false };
        g.skip_done();
        g
    }

    /// Whose turn it is (None: everyone's played).
    fn up(&self) -> Option<usize> {
        (!self.shown && self.played < self.hands.len()).then_some(self.turn)
    }

    /// Done: on to the next (or, all done, the reckoning).
    fn stand(&mut self) {
        let n = self.hands.len();
        self.stood[self.turn] = true;
        self.played += 1;
        self.turn = (self.turn + 1) % n;
        if self.played >= n {
            self.shown = true;
        } else {
            self.skip_done();
        }
    }

    /// Anyone with 21 already stands.
    fn skip_done(&mut self) {
        while self.played < self.hands.len() && total(&self.hands[self.turn]) >= 21 {
            self.stand();
        }
    }

    fn hit(&mut self) -> Option<(u8, u8)> {
        let c = self.deck.pop()?;
        self.hands[self.turn].push(c);
        if total(&self.hands[self.turn]) >= 21 {
            self.stand();
        }
        Some(c)
    }

    /// Seat `i` against the dealer: won, lost or drawn (from `i`'s side).
    fn against(&self, i: usize) -> Result {
        let (y, d) = (total(&self.hands[i]), total(&self.hands[self.dealer]));
        if y > 21 {
            Result::Lost
        } else if d > 21 || y > d {
            Result::Won
        } else if y < d {
            Result::Lost
        } else {
            Result::Drawn
        }
    }

    /// How it went for seat `i`: against the dealer; the dealer, against
    /// the table (more beaten than not: won).
    fn result_for(&self, i: usize) -> Result {
        if i != self.dealer {
            return self.against(i);
        }
        let (mut won, mut lost) = (0, 0);
        for j in (0..self.hands.len()).filter(|&j| j != self.dealer) {
            match self.against(j) {
                Result::Won => lost += 1,
                Result::Lost => won += 1,
                Result::Drawn => {}
            }
        }
        match won.cmp(&lost) {
            std::cmp::Ordering::Greater => Result::Won,
            std::cmp::Ordering::Less => Result::Lost,
            std::cmp::Ordering::Equal => Result::Drawn,
        }
    }

    /// How the hand went, in words.
    fn summary(&self, names: &[String]) -> String {
        let say = |i: usize| {
            let n = if i == 0 { "you".to_string() } else { short(&names[i]) };
            let t = total(&self.hands[i]);
            if t > 21 {
                format!("{n} over")
            } else {
                format!("{n} {t}")
            }
        };
        let all: Vec<String> = (0..self.hands.len()).map(say).collect();
        let r = match self.result_for(0) {
            Result::Won => "you win!",
            Result::Lost => "you lose",
            Result::Drawn => "a draw",
        };
        format!("{r} {}", all.join(" · "))
    }

    fn key(&mut self, k: Key, _rng: &mut Rng, names: &[String]) -> Option<String> {
        if self.up() != Some(0) {
            return None;
        }
        let next = |g: &Self| match g.up() {
            Some(0) => "your turn".to_string(),
            Some(u) => format!("{}'s turn", short(&names[u])),
            None => "everyone's played".into(),
        };
        match k {
            Key::Pick | Key::Up => {
                let c = self.hit()?;
                let t = total(&self.hands[0]);
                Some(match t {
                    _ if t > 21 => format!("{} — {t}, over! · {}", card_name(c), next(self)),
                    21 => format!("{} — 21! · {}", card_name(c), next(self)),
                    _ => format!("{} — that's {t}", card_name(c)),
                })
            }
            Key::Other | Key::Down => {
                let t = total(&self.hands[0]);
                self.stand();
                Some(format!("you stand on {t} · {}", next(self)))
            }
            _ => None,
        }
    }

    /// Whoever's turn it is decides (the dealer by the house rule).
    fn think(&mut self, _rng: &mut Rng, names: &[String]) -> Option<String> {
        let u = self.up()?;
        let name = if u == 0 { "you".to_string() } else { short(&names[u]) };
        let t = total(&self.hands[u]);
        let want = if u == self.dealer { 17 } else { 16 };
        if t < want {
            let c = self.hit()?;
            Some(format!("{name} takes another: {}", card_name(c)))
        } else {
            self.stand();
            Some(format!("{name} stands on {t}"))
        }
    }

    fn draw(&self, f: &mut Frame, x0: i32, y0: i32, bg: Bg, names: &[String]) {
        let row = |f: &mut Frame, y: i32, hand: &[(u8, u8)], hide: bool| {
            for (i, &c) in hand.iter().enumerate().take(7) {
                let red = c.1 == 1 || c.1 == 2;
                let (txt, col) = if hide && i >= 1 { ("??".to_string(), Rgb::hex(0x6a7aa8)) } else { (card_name(c), if red { Rgb::hex(0xe0404a) } else { Rgb::hex(0x1a1014) }) };
                let x = x0 + 4 + i as i32 * 7;
                f.text(x, y, "╭────╮", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x, y + 1, "│", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x + 1, y + 1, &format!("{txt:^4}"), col, Bg::Solid(Rgb::hex(0xf4efe6)), 0.0);
                f.text(x + 5, y + 1, "│", Rgb::hex(0xe8e0d0), bg, 0.0);
                f.text(x, y + 2, "╰────╯", Rgb::hex(0xe8e0d0), bg, 0.0);
            }
        };
        let d = self.dealer;
        let hide = d != 0 && !self.shown;
        let dt = if hide { "?".to_string() } else { total(&self.hands[d]).to_string() };
        f.text(x0, y0, &format!("{} · {dt}", if d == 0 { "you deal".to_string() } else { format!("{} deals", short(&names[d])) }), pal::UI_DIM, bg, 0.0);
        row(f, y0 + 1, &self.hands[d], hide);
        let others: Vec<String> = (1..self.hands.len()).filter(|&i| i != d).map(|i| format!("{} {}", short(&names[i]), total(&self.hands[i]))).collect();
        f.text(x0, y0 + 5, &others.join(" · "), pal::UI_DIM, bg, 0.0);
        if d != 0 {
            f.text(x0, y0 + 8, &format!("you · {}", total(&self.hands[0])), Rgb::hex(0xffd36b), bg, 0.0);
            row(f, y0 + 9, &self.hands[0], false);
        }
    }
}


// ------------------------------------------------------------- online

fn res_code(r: Option<Result>) -> Option<u8> {
    r.map(|r| match r {
        Result::Won => 0,
        Result::Lost => 1,
        Result::Drawn => 2,
    })
}

fn res_of(c: Option<u8>) -> Option<Result> {
    c.map(|c| match c {
        0 => Result::Won,
        1 => Result::Lost,
        _ => Result::Drawn,
    })
}


/// A game as it stands, to send to the others at the table (from seat 0's
/// side: "white", "red", hand 0 are seat 0's).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "g")]
enum State {
    Chess { b: Vec<i8>, white: bool, castle: [bool; 4], ep: Option<usize>, quiet: u32, over: Option<u8> },
    Checkers { b: Vec<i8>, red: bool, chain: Option<usize>, over: Option<u8> },
    Snow { deck: Vec<(u8, u8)>, pile: Vec<(u8, u8)>, colour: u8, hands: Vec<Vec<(u8, u8)>>, turn: usize, dir: i32, won: Option<usize> },
    TwentyOne { deck: Vec<(u8, u8)>, hands: Vec<Vec<(u8, u8)>>, dealer: usize, turn: usize, played: usize, stood: Vec<bool>, shown: bool },
}

impl State {
    fn kind(&self) -> Kind {
        match self {
            State::Chess { .. } => Kind::Chess,
            State::Checkers { .. } => Kind::Checkers,
            State::Snow { .. } => Kind::Snowflakes,
            State::TwentyOne { .. } => Kind::TwentyOne,
        }
    }
}

/// The same game from `k` seats round the table: the board turned round
/// for the second at chess and checkers (chess mirrored top to bottom, so
/// the kings keep their files; checkers turned right round, so the pieces
/// stay on the dark squares), the colours swapped, a win a loss; at the
/// cards, the hands, whose turn and who deals moved round.
fn turn_round(st: State, k: usize) -> State {
    let other = |r: Option<u8>| {
        r.map(|c| match c {
            0 => 1,
            1 => 0,
            x => x,
        })
    };
    let rot = |n: usize, i: usize| (i + n - k % n.max(1)) % n.max(1);
    match st {
        State::Chess { b, white, castle, ep, quiet, over } if k % 2 == 1 => {
            let m = |i: usize| (7 - i / 8) * 8 + i % 8;
            let mut nb = vec![0i8; 64];
            for (i, p) in b.iter().enumerate().take(64) {
                nb[m(i)] = -p;
            }
            State::Chess { b: nb, white: !white, castle: [castle[2], castle[3], castle[0], castle[1]], ep: ep.map(m), quiet, over: other(over) }
        }
        State::Checkers { b, red, chain, over } if k % 2 == 1 => {
            let mut nb = vec![0i8; 64];
            for (i, p) in b.iter().enumerate().take(64) {
                nb[63 - i] = -p;
            }
            State::Checkers { b: nb, red: !red, chain: chain.map(|c| 63 - c), over: other(over) }
        }
        State::Snow { deck, pile, colour, hands, turn, dir, won } => {
            let n = hands.len().max(1);
            let hands = (0..n).map(|i| hands[(i + k) % n].clone()).collect();
            State::Snow { deck, pile, colour, hands, turn: rot(n, turn), dir, won: won.map(|w| rot(n, w)) }
        }
        State::TwentyOne { deck, hands, dealer, turn, played, stood, shown } => {
            let n = hands.len().max(1);
            let hands = (0..n).map(|i| hands[(i + k) % n].clone()).collect();
            let stood = (0..n).map(|i| stood.get((i + k) % n).copied().unwrap_or(false)).collect();
            State::TwentyOne { deck, hands, dealer: rot(n, dealer), turn: rot(n, turn), played, stood, shown }
        }
        st => st,
    }
}

impl Game {
    fn state(&self) -> State {
        let cards = |v: &[Card]| v.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>();
        match self {
            Game::Chess(c) => State::Chess { b: c.b.to_vec(), white: c.white, castle: c.castle, ep: c.ep, quiet: c.quiet, over: res_code(c.over) },
            Game::Checkers(c) => State::Checkers { b: c.b.to_vec(), red: c.red, chain: c.chain, over: res_code(c.over) },
            Game::Snow(s) => State::Snow { deck: cards(&s.deck), pile: cards(&s.pile), colour: s.colour, hands: s.hands.iter().map(|h| cards(h)).collect(), turn: s.turn, dir: s.dir, won: s.won },
            Game::TwentyOne(g) => State::TwentyOne { deck: g.deck.clone(), hands: g.hands.clone(), dealer: g.dealer, turn: g.turn, played: g.played, stood: g.stood.clone(), shown: g.shown },
        }
    }

    /// Take on a state (this side's view of it): what's picked out and the
    /// cursor stay as they were.
    fn adopt(&mut self, st: State) {
        match (self, st) {
            (Game::Chess(c), State::Chess { b, white, castle, ep, quiet, over }) => {
                if b.len() == 64 {
                    c.b.copy_from_slice(&b);
                }
                (c.white, c.castle, c.ep, c.quiet, c.over, c.picked) = (white, castle, ep, quiet, res_of(over), None);
            }
            (Game::Checkers(c), State::Checkers { b, red, chain, over }) => {
                if b.len() == 64 {
                    c.b.copy_from_slice(&b);
                }
                (c.red, c.chain, c.over, c.picked) = (red, chain, res_of(over), None);
            }
            (g, State::Snow { deck, pile, colour, hands, turn, dir, won }) => {
                let cards = |v: Vec<(u8, u8)>| v.into_iter().map(|(a, b)| Card(a, b)).collect::<Vec<_>>();
                let sel = if let Game::Snow(s) = g { s.sel } else { 0 };
                let mut s = Snow { deck: cards(deck), pile: cards(pile), colour, hands: hands.into_iter().map(cards).collect(), turn, dir, sel, choosing: None, won };
                if s.pile.is_empty() {
                    s.pile.push(Card(0, 0));
                }
                while s.hands.len() < 2 {
                    s.hands.push(vec![]);
                }
                s.turn = s.turn.min(s.hands.len() - 1);
                s.sel = s.sel.min(s.hands[0].len());
                *g = Game::Snow(s);
            }
            (g, State::TwentyOne { deck, hands, dealer, turn, played, stood, shown }) => {
                let n = hands.len().max(1);
                *g = Game::TwentyOne(TwentyOne { deck, stood: (0..n).map(|i| stood.get(i).copied().unwrap_or(false)).collect(), hands, dealer: dealer.min(n - 1), turn: turn.min(n - 1), played, shown });
            }
            _ => {}
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

    /// Something legal for this side, by the keys.
    fn play_something(tbl: &mut Table, rng: &mut Rng, t: f32) {
        match &tbl.game {
            Game::Chess(c) => {
                let ms = c.moves();
                if ms.is_empty() {
                    return;
                }
                let m = ms[rng.int(0, ms.len() as i32) as usize];
                if let Game::Chess(c) = &mut tbl.game {
                    c.cur = ((m.from % 8) as i32, (m.from / 8) as i32);
                    c.picked = None;
                }
                tbl.key(Key::Pick, t);
                if let Game::Chess(c) = &mut tbl.game {
                    c.cur = ((m.to % 8) as i32, (m.to / 8) as i32);
                }
                tbl.key(Key::Pick, t);
            }
            Game::Checkers(c) => {
                let ms = c.moves();
                if ms.is_empty() {
                    return;
                }
                let m = ms[rng.int(0, ms.len() as i32) as usize];
                if let Game::Checkers(c) = &mut tbl.game {
                    c.cur = ((m.0 % 8) as i32, (m.0 / 8) as i32);
                    c.picked = if c.chain.is_some() { c.chain } else { None };
                }
                if matches!(&tbl.game, Game::Checkers(c) if c.picked.is_none()) {
                    tbl.key(Key::Pick, t);
                }
                if let Game::Checkers(c) = &mut tbl.game {
                    c.cur = ((m.1 % 8) as i32, (m.1 / 8) as i32);
                }
                tbl.key(Key::Pick, t);
            }
            Game::Snow(s) => {
                if s.choosing.is_some() {
                    tbl.key(Key::Pick, t);
                } else if let Some(i) = s.hands[0].iter().position(|&c| s.fits(c)) {
                    if let Game::Snow(s) = &mut tbl.game {
                        s.sel = i;
                    }
                    tbl.key(Key::Pick, t);
                    if matches!(&tbl.game, Game::Snow(s) if s.choosing.is_some()) {
                        tbl.key(Key::Pick, t);
                    }
                } else {
                    tbl.key(Key::Other, t);
                }
            }
            Game::TwentyOne(g) => tbl.key(if total(&g.hands[0]) < 15 { Key::Pick } else { Key::Other }, t),
        }
    }

    #[test]
    fn games_play_out_to_an_end() {
        let mut rng = Rng::new(7);
        for kind in Kind::ALL {
            for (seed, others) in [(0u32, vec!["Agnes"]), (1, vec!["Agnes", "Sam"]), (2, vec!["Agnes", "Sam", "Bo"]), (3, vec!["Agnes"])] {
                let mut tbl = Table::with(kind, &others, seed);
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
                    play_something(&mut tbl, &mut rng, t);
                }
                assert!(tbl.result.is_some() || kind == Kind::Chess, "{kind:?} with {others:?} never ended");
            }
        }
    }

    /// Twenty-One: the deal passes on (the villager dealing plays last,
    /// and chooses); a click off the buttons does nothing.
    #[test]
    fn twenty_one_deals_round_and_clicks_need_a_button() {
        let mut tbl = Table::with(Kind::TwentyOne, &["Agnes", "Sam"], 4);
        let mut f = Frame::new(120, 40);
        tbl.draw(&mut f, "classic", 0.0);
        let before = tbl.state_json();
        // (a click on the felt, away from the buttons: nothing)
        let (x0, y0) = tbl.corner();
        tbl.click(x0 + 20, y0 + 8, 0.0);
        assert_eq!(tbl.state_json(), before, "a stray click played");
        // pass the deal to Sam, then to the villager
        tbl.key(Key::Dealer, 0.0);
        assert!(matches!(&tbl.game, Game::TwentyOne(g) if g.dealer == 2));
        tbl.key(Key::Dealer, 0.0);
        assert!(matches!(&tbl.game, Game::TwentyOne(g) if g.dealer == 0 && g.turn == 1));
        // they play first; then it's the villager's to choose
        let mut t = 0.0;
        while tbl.theirs() && t < 60.0 {
            t += 1.0;
            tbl.update(t);
        }
        assert!(matches!(&tbl.game, Game::TwentyOne(g) if g.up() == Some(0)), "the dealer (the villager) plays last");
        // the stand button (as drawn) stands
        tbl.draw(&mut f, "classic", t);
        let b = tbl.buttons.borrow().iter().find(|b| b.3 == Key::Other).copied().expect("a stand button");
        tbl.click(b.0 + 1, b.1, t);
        assert!(tbl.result.is_some(), "stood, and the hand's reckoned");
    }

    /// Players online: each move goes across (from seat 0's side) and is
    /// taken in, turned round, on every other side; they agree on the game
    /// to the end, and on who won; seat 0 deals again for everyone.
    #[test]
    fn friends_online_play_to_an_end() {
        let mut rng = Rng::new(11);
        for kind in Kind::ALL {
            for players in [2usize, 3, 4] {
                if players > kind.seats() {
                    continue;
                }
                let ids: Vec<u32> = (1..=players as u32).collect();
                let names: Vec<String> = ["Robin", "Sam", "Bo", "Ash"].iter().take(players).map(|s| s.to_string()).collect();
                // (everyone sat down at it)
                let mut all: Vec<Table> = ids.iter().map(|&me| Table::online(kind, 99, ids.clone(), names.clone(), me, 5)).collect();
                all.iter_mut().for_each(Table::all_sat);
                // the same game, seen from each side
                for (s, tb) in all.iter().enumerate().skip(1) {
                    assert_eq!(serde_json::to_string(&turn_round(all[0].game.state(), s)).unwrap(), serde_json::to_string(&tb.game.state()).unwrap(), "{kind:?} dealt differently for seat {s}");
                    assert_eq!(tb.names[0], names[s]);
                }
                let mut t = 0.0;
                let mut moves = 0;
                for _ in 0..6000 {
                    if all.iter().all(|tb| tb.result.is_some()) {
                        break;
                    }
                    t += 1.0;
                    let up: Vec<usize> = (0..players).filter(|&i| !all[i].theirs() && all[i].result.is_none()).collect();
                    assert!(up.len() <= 1, "{kind:?} ×{players}: two think it's their turn: {up:?}");
                    let Some(&i) = up.first() else { break };
                    play_something(&mut all[i], &mut rng, t);
                    if let Some((seq, st)) = all[i].take_out() {
                        for (j, tb) in all.iter_mut().enumerate() {
                            if j != i {
                                assert!(tb.adopt(seq, &st), "{kind:?}: a move refused");
                                assert!(!tb.adopt(seq, &st), "taken twice");
                            }
                        }
                        moves += 1;
                    }
                }
                assert!(all.iter().all(|tb| tb.result.is_some()) || kind == Kind::Chess, "{kind:?} ×{players} never ended ({moves} moves)");
                if kind == Kind::Chess || players != 2 {
                    continue;
                }
                let (ra, rb) = (all[0].result.unwrap(), all[1].result.unwrap());
                assert!(matches!((ra, rb), (Result::Won, Result::Lost) | (Result::Lost, Result::Won) | (Result::Drawn, Result::Drawn)), "{kind:?}: they disagree: {ra:?} {rb:?}");
            }
        }
        // seat 0 deals again; the others take it as a new game
        let ids = vec![1, 2, 3];
        let names: Vec<String> = ["Robin", "Sam", "Bo"].iter().map(|s| s.to_string()).collect();
        let mut all: Vec<Table> = ids.iter().map(|&me| Table::online(Kind::TwentyOne, 7, ids.clone(), names.clone(), me, 9)).collect();
        all.iter_mut().for_each(Table::all_sat);
        for tb in &mut all {
            tb.result = Some(Result::Drawn);
        }
        all[1].key(Key::Deal, 0.0);
        assert!(all[1].take_out().is_none(), "only seat 0 deals");
        all[0].key(Key::Deal, 0.0);
        let (seq, st) = all[0].take_out().expect("the new hand goes out");
        for tb in all.iter_mut().skip(1) {
            assert!(tb.adopt(seq, &st));
            assert!(tb.result.is_none());
        }
        // (the deal passed round: seat 1 deals the second hand)
        assert!(matches!(&all[1].game, Game::TwentyOne(g) if g.dealer == 0));
    }

    /// Online, waiting for players: they sit down at seat 0's table, and
    /// seat 0 deals for everyone.
    #[test]
    fn a_table_fills_up_and_deals() {
        let mut host = Table::lobby(Kind::Snowflakes, 42, vec![1], vec!["Robin".into()], 1);
        let mut sam = Table::lobby(Kind::Snowflakes, 42, vec![], vec![], 2);
        assert!(host.waiting && sam.waiting);
        host.sit_in(2, "Sam");
        let (seq, st) = host.take_out().expect("who's sat down goes out");
        assert!(sam.adopt(seq, &st));
        assert_eq!(sam.names, vec!["Sam".to_string(), "Robin".to_string()]);
        sam.key(Key::Deal, 0.0);
        assert!(sam.take_out().is_none() && sam.waiting, "Sam can't deal");
        host.key(Key::Deal, 0.0);
        let (seq, st) = host.take_out().expect("the deal goes out");
        assert!(sam.adopt(seq, &st));
        assert!(!sam.waiting && !host.waiting);
        assert_eq!(serde_json::to_string(&turn_round(host.game.state(), 1)).unwrap(), serde_json::to_string(&sam.game.state()).unwrap());
    }

    /// Autopilot at the table: every game plays itself out to an end.
    #[test]
    fn autopilot_plays_its_own_games() {
        for kind in Kind::ALL {
            for seed in 0..3u32 {
                let mut tbl = Table::with(kind, &["Agnes", "Sam"], seed);
                let mut t = 0.0;
                for _ in 0..1500 {
                    if tbl.result.is_some() {
                        break;
                    }
                    t += 1.0;
                    tbl.update(t);
                    tbl.auto_move(t);
                }
                assert!(tbl.result.is_some() || kind == Kind::Chess, "{kind:?} never ended");
            }
        }
    }
}
