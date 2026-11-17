//! Things to do, started by hand (T opens the list, or the right-click
//! menu): a game of tag with one of the kids, a carol (the neighbours
//! gather round and join in), a snow angel, a rest in the snow, building a
//! snowman or an igloo, knocking one down again. Autopilot plays tag through
//! the same game.

use super::{Do, Scene};
use crate::audio::Sfx;
use crate::color::Rgb;

/// A game of tag with a kid: who's "it", no tagging back until `grace`, and
/// when it ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TagGame {
    pub id: u64,
    pub you_it: bool,
    pub grace: f32,
    pub until: f32,
    /// How many times "it" changed hands.
    pub swaps: u32,
}

/// Singing a carol: until then, and the next note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carol {
    pub until: f32,
    pub next: f32,
}

impl TagGame {
    pub fn rebase(&mut self, s: f32) {
        self.grace -= s;
        self.until -= s;
    }
}

impl Carol {
    pub fn rebase(&mut self, s: f32) {
        self.until -= s;
        self.next -= s;
    }
}

/// A change of place, made while the view's shut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pending {
    Enter(usize),
    Igloo(usize),
    Leave,
    /// In at the cocoa cabin's door.
    Cabin,
    /// A trip out (true: skiing), or back from one.
    Trip(bool),
    TripEnd,
    /// Along to where someone online is on their trip (their id).
    Join(u32),
    /// A race down the mountain with friends: the course, when it starts.
    Race(u32, f32),
    /// Round the table in the games room (the table's number).
    GameRoom(u32),
}

/// How long the view takes to close (and again to open).
pub const IRIS: f32 = 0.35;

const TAG_START: &[&str] = &["tag! bet I can catch you!", "let's play tag!", "ready or not — tag!"];
const TAG_END: &[&str] = &["good game!", "phew, you win!", "that was fun!", "you're fast!"];

impl Scene {
    /// Go somewhere: the view closes round the villager, and opens on it.
    pub(super) fn soon(&mut self, what: Pending) {
        if self.pending.is_some() {
            return;
        }
        let t = self.now();
        let (x, y) = self.zoom_focus();
        self.iris = Some((t, x, y));
        self.pending = Some((t + IRIS, what));
        self.goal = None;
    }

    /// The change, once the view's shut; the view open again after.
    pub(super) fn update_iris(&mut self, t: f32) {
        if let Some((at, what)) = self.pending {
            if t >= at {
                self.pending = None;
                match what {
                    Pending::Enter(i) => self.enter(i),
                    Pending::Igloo(i) => self.enter_igloo(i, t),
                    Pending::Leave => self.leave(),
                    Pending::Cabin => self.enter_cabin(t),
                    Pending::Trip(ski) => self.start_trip(ski),
                    Pending::TripEnd => self.finish_trip(),
                    Pending::Join(id) => self.join_trip(id),
                    Pending::Race(seed, at) => self.start_race(seed, at),
                    Pending::GameRoom(game) => self.enter_games_room(game),
                }
                // (opening round where the villager is now)
                let (x, y) = self.zoom_focus();
                if let Some(ir) = &mut self.iris {
                    (ir.1, ir.2) = (x, y);
                }
            }
        }
        if self.iris.is_some_and(|ir| t - ir.0 > IRIS * 2.0) {
            self.iris = None;
        }
    }

    /// The view closing (or opening): dark beyond a circle round the
    /// villager, a gold ring at its edge.
    pub fn draw_iris(&self, f: &mut crate::frame::Frame) {
        let Some((at, cx, cy)) = self.iris else { return };
        let t = self.now();
        let u = ((t - at) / IRIS).clamp(0.0, 2.0);
        let k = if u < 1.0 { 1.0 - u } else { u - 1.0 };
        let k = k * k * (3.0 - 2.0 * k);
        let (w, h) = (f.w as f32, f.h as f32);
        let r = k * (w * w + 4.0 * h * h).sqrt() * 0.6;
        let (ox, oy) = (f.ox, f.oy);
        (f.ox, f.oy) = (0, 0);
        let (pcx, pcy) = (cx, cy * 2.0);
        for py in 0..f.ph as i32 {
            for px in 0..f.pw as i32 {
                let d = ((px as f32 + 0.5 - pcx).powi(2) + (py as f32 + 0.5 - pcy).powi(2)).sqrt();
                if d > r {
                    f.px_set(px, py, Rgb::hex(0x03050f));
                } else if d > r - 1.5 && r > 2.0 {
                    f.px_set(px, py, Rgb::hex(0xffd36b) * 0.7);
                }
            }
        }
        for cy2 in 0..f.h as i32 {
            for cx2 in 0..f.w as i32 {
                let d = ((cx2 as f32 + 0.5 - pcx).powi(2) + ((cy2 as f32 + 0.5) * 2.0 - pcy).powi(2)).sqrt();
                if d > r {
                    f.clear_glyph(cx2, cy2);
                }
            }
        }
        f.pictures.retain(|p| {
            let (x, y) = ((p.x + p.cols / 2) as f32, ((p.y + p.rows / 2) as f32) * 2.0);
            ((x - pcx).powi(2) + (y - pcy).powi(2)).sqrt() < r
        });
        (f.ox, f.oy) = (ox, oy);
    }

    /// Tag with kid `id`: they run off, and the villager's "it".
    pub(super) fn start_tag(&mut self, id: u64, secs: f32) {
        let t = self.now();
        if !self.critters.is_kid(id) || self.critters.position(id).is_none() {
            return;
        }
        if let Some(g) = self.tag.take() {
            self.critters.tag_over(g.id, t, &mut self.director);
        }
        let until = t + secs;
        self.critters.tag(id, false, until);
        self.tag = Some(TagGame { id, you_it: true, grace: t + 1.0, until, swaps: 0 });
        self.pockets.tag_games += 1;
        let line = *self.rng.pick(TAG_START);
        self.you_say(line, t);
    }

    /// The game of tag, each frame: a tag either way, or the end of it.
    pub(super) fn update_tag(&mut self, t: f32) {
        let Some(mut g) = self.tag else { return };
        let caught = self.critters.take_tagged();
        let kid = self.critters.position(g.id).filter(|_| self.critters.tag_state(g.id).is_some());
        let (Some((kx, ky)), Some(p)) = (kid, self.player.as_ref()) else {
            self.tag = None;
            return;
        };
        let (px, py) = (p.centre(), p.y);
        if t > g.until || self.inside.is_some() {
            self.critters.tag_over(g.id, t, &mut self.director);
            self.tag = None;
            let line = *self.rng.pick(TAG_END);
            self.you_say(line, t);
            if g.swaps >= 2 {
                self.cheer(1);
            }
            return;
        }
        if g.you_it {
            if t > g.grace && (kx - px).abs() < 2.5 && (ky - py).abs() < 1.5 {
                self.you_say("tag! you're it!", t);
                self.director.sound(Sfx::Pickup, px, 0.5);
                self.critters.tag(g.id, true, g.until);
                g.you_it = false;
                g.grace = t + 1.5;
                g.swaps += 1;
                self.pockets.tags += 1;
            }
        } else if caught && t > g.grace {
            self.director.say(kx.round() as i32, ky as i32 - 4, "tag! you're it!", t, 2.2, 6);
            self.director.sound(Sfx::Laugh, kx, 0.5);
            self.critters.tag(g.id, false, g.until);
            g.you_it = true;
            g.grace = t + 1.5;
            g.swaps += 1;
        }
        self.tag = Some(g);
    }

    /// A line from the villager.
    pub(super) fn you_say(&mut self, line: &str, t: f32) {
        if let Some(p) = &self.player {
            self.director.say(p.centre().round() as i32, p.y as i32 - 4, line, t, 2.4, 6);
        }
    }

    /// A carol: the villager sings, and the neighbours about gather round
    /// and join in.
    pub fn sing(&mut self) {
        let t = self.now();
        let Some((px, py)) = self.player.as_ref().map(|p| (p.centre(), p.y)) else { return };
        if self.inside.is_some() || self.carol.is_some_and(|c| t < c.until) {
            return;
        }
        let secs = 12.0;
        self.carol = Some(Carol { until: t + secs, next: t });
        self.pockets.carols += 1;
        let line = *self.rng.pick(&["♪ deck the halls… ♪", "♪ silent night… ♪", "♪ jingle bells, jingle bells… ♪", "♪ o christmas tree… ♪", "♪ we wish you a merry christmas… ♪"]);
        self.you_say(line, t);
        let join = *self.rng.pick(&["ooh, I know this one!", "a carol! wait for me!", "♪ la la laaa ♪", "everyone, sing along!"]);
        self.critters.gather(px, t + secs, t, &mut self.director, join);
        self.share_show(2, 0);
        self.director.sound(Sfx::Chime, px, 0.6);
        let _ = py;
    }

    /// The carol under way: notes rising from the singers.
    pub(super) fn update_carol(&mut self, t: f32) {
        let Some(mut c) = self.carol else { return };
        let Some((px, py)) = self.player.as_ref().map(|p| (p.centre(), p.y)) else {
            self.carol = None;
            return;
        };
        if t > c.until || self.inside.is_some() {
            self.carol = None;
            let n = self.critters.people().iter().filter(|p| (p.1 - px).abs() < 16.0).count();
            if n > 0 {
                self.director.happenings.push((super::dialogue::Ev::Magic, Some(px)));
                self.cheer(1 + n.min(3) as u32);
                let line = *self.rng.pick(&["bravo!", "lovely!", "encore!", "merry christmas!"]);
                if let Some((id, x, y, _)) = self.critters.people().into_iter().find(|p| (p.1 - px).abs() < 16.0) {
                    let _ = id;
                    self.director.say(x.round() as i32, y as i32 - 4, line, t, 2.4, 6);
                }
            }
            return;
        }
        if t >= c.next {
            c.next = t + 0.55;
            let notes = ['♪', '♫'];
            let k = (t * 3.0) as usize % 2;
            self.director.emote(px, py - 4.5, notes[k], Rgb::hex(0xffd36b), 1);
            for (_, x, y, _) in self.critters.people().into_iter().filter(|p| (p.1 - px).abs() < 16.0).take(5) {
                if self.rng.chance(0.5) {
                    self.director.emote(x, y - 4.5, notes[1 - k], Rgb::hex(0xbfe4ff), 1);
                }
            }
            if self.rng.chance(0.35) {
                self.director.sound(Sfx::Chime, px, 0.25);
            }
        }
        self.carol = Some(c);
    }

    /// Knock down the snowman or igloo at column `bx` (nobody minds).
    pub(super) fn knock_down(&mut self, bx: f32) {
        let t = self.now();
        let Some(i) = self.builds_near(bx) else { return };
        let h = self.h as f32;
        if let Some((x, base, igloo)) = self.builds.knock_down(i, t, h, &mut self.critters, &mut self.director) {
            self.director.sound(Sfx::Puff, x, 0.8);
            self.director.burst(x, base as f32 - 3.0, 18, &[Rgb::WHITE, Rgb::hex(0xdfe8f6)], 5.0, 4.0);
            let line = if igloo { *self.rng.pick(&["timber!", "and down it comes", "oops — all gone"]) } else { *self.rng.pick(&["poof!", "bye-bye, snowman", "back to snow"]) };
            self.you_say(line, t);
        }
    }

    /// A light show of the villager's own, `secs` long: everyone's called
    /// to the big tree first, and it starts once they've had time to get
    /// there.
    pub(super) fn start_light_show(&mut self, secs: f32) {
        let t = self.now();
        if self.director.plan_light_show(t, super::events::SHOW_LEAD * 0.6, secs) {
            self.pockets.light_shows += 1;
            self.share_show(0, secs as u16);
            self.you_say("light show by the big tree — everyone come and see!", t);
            self.director.sound(Sfx::Chime, self.player.as_ref().map(|p| p.centre()).unwrap_or(0.0), 0.6);
        } else {
            self.you_say("there's a show on already!", t);
        }
    }

    /// Fireworks, set off near the villager.
    pub(super) fn set_off_fireworks(&mut self, n: usize) {
        let t = self.now();
        let Some(px) = self.player.as_ref().map(|p| p.centre()) else { return };
        self.director.fireworks(t + 0.6, n, px);
        self.share_show(1, n as u16);
        self.pockets.fireworks += n as u32;
        let line = *self.rng.pick(&["stand back!", "light the fuse…", "three, two, one…", "ooh, here they go!"]);
        self.you_say(line, t);
        self.cheer(if n >= 10 { 2 } else { 1 });
    }

    /// The further choices behind a "▸" (title, choices).
    pub fn more_menu(&self, k: u8) -> (String, Vec<(String, Do)>) {
        if let Some(m) = self.online_more(k) {
            return m;
        }
        match k {
            0 => (
                "a light show".into(),
                vec![
                    ("a short one (30 s)".into(), Do::LightShow(30)),
                    ("a good long one (2 min)".into(), Do::LightShow(120)),
                    ("all evening (5 min)".into(), Do::LightShow(300)),
                    ("on and on (15 min)".into(), Do::LightShow(900)),
                ],
            ),
            _ => (
                "fireworks".into(),
                vec![("one rocket".into(), Do::Fireworks(1)), ("a handful".into(), Do::Fireworks(5)), ("a big display".into(), Do::Fireworks(14)), ("the grand finale".into(), Do::Fireworks(30))],
            ),
        }
    }

    /// A team snowball fight: the villager and whoever's along against the
    /// folk round about.
    pub(super) fn start_team_fight(&mut self) {
        let t = self.now();
        if self.critters.team_fight(t, &mut self.director) {
            let n = self.critters.follower_ids().len();
            let line = if n == 0 { "you lot against me — snowball fight!" } else { "team snowball fight — with me, everyone!" };
            self.you_say(line, t);
        } else {
            self.you_say("nobody about to take us on…", t);
        }
    }

    /// A team fight over: how it went (the scorecard), and a word.
    pub(super) fn fight_results(&mut self, t: f32) {
        for (won, ours, theirs) in std::mem::take(&mut self.critters.fight_results) {
            if won {
                self.pockets.fights_won += 1;
                self.cheer(3);
                self.you_say(&format!("we won, {ours} to {theirs}!"), t + 1.0);
            } else {
                self.pockets.fights_lost += 1;
                self.you_say(&format!("{theirs} to {ours}… rematch!"), t + 1.0);
            }
        }
    }

    /// Games to play with whoever's at home (none in an empty house or an
    /// igloo on your own).
    pub(super) fn game_choices(&self) -> Vec<(String, Do)> {
        let Some(inn) = self.inside.as_ref().filter(|i| !i.is_games()) else { return vec![] };
        let names: Vec<&str> = inn.residents.iter().filter(|r| !inn.is_cabin() || r.name != inn.residents[0].name).map(|r| r.name).collect();
        let Some(first) = names.first().copied() else { return vec![] };
        // (the cards: up to three of them round the table)
        let all = match names.len() {
            1 => first.to_string(),
            2 => format!("{} & {}", names[0], names[1]),
            _ => format!("{}, {} & {}", names[0], names[1], names[2]),
        };
        super::games::Kind::ALL.iter().map(|&k| (format!("{} {} with {}", k.icon(), k.name(), if k.seats() > 2 { &all } else { first }), Do::Game(k))).collect()
    }

    /// Tests: sat at a game of chess, wherever the villager is.
    #[cfg(test)]
    pub fn sit_at_test_table(&mut self) {
        self.table = Some(super::games::Table::with(super::games::Kind::Chess, &["Ivy"], 1));
    }

    /// Sit down at the table for a game with whoever's at home.
    pub(super) fn start_game(&mut self, kind: super::games::Kind) {
        if let Some(old) = self.table.take() {
            self.left_table(&old);
        }
        // round a table: the villager sits down, and whoever's playing too
        let (Some(inn), Some(p)) = (self.inside.as_mut(), self.player.as_mut()) else { return };
        let names = inn.sit_for_game(p, kind.seats() - 1);
        if names.is_empty() {
            return;
        }
        let seed = self.rng.next_u32();
        self.table = Some(super::games::Table::with(kind, &names, seed));
        self.convo = None;
    }

    /// At the table (keys and clicks go to the game).
    pub fn at_table(&self) -> bool {
        self.table.is_some()
    }

    /// A click on the table's panel (the game's, not what's behind it).
    pub fn on_table(&self, x: i32, y: i32) -> bool {
        self.table.as_ref().filter(|_| self.table_shows()).is_some_and(|tb| tb.contains(x, y))
    }

    /// The table picked up by its title bar (true): it moves with the
    /// pointer until let go.
    pub fn table_grab(&mut self, x: i32, y: i32) -> bool {
        let Some(tb) = &mut self.table else { return false };
        if !tb.on_title(x, y) {
            return false;
        }
        let (x0, y0) = tb.corner();
        tb.held = Some((x - x0, y - y0));
        true
    }

    pub fn table_dragging(&self) -> bool {
        self.table.as_ref().is_some_and(|t| t.held.is_some())
    }

    pub fn table_drag_to(&mut self, x: i32, y: i32) {
        if let Some(tb) = &mut self.table {
            if let Some((dx, dy)) = tb.held {
                tb.pos = Some((x - dx, y - dy));
            }
        }
    }

    pub fn table_drop(&mut self) {
        if let Some(tb) = &mut self.table {
            tb.held = None;
        }
    }

    /// The game at the table is on a board (chess, checkers).
    pub fn table_board(&self) -> bool {
        self.table.as_ref().is_some_and(|t| matches!(t.kind, super::games::Kind::Chess | super::games::Kind::Checkers))
    }

    /// A key at the table; Esc (`None`) gets up from it.
    pub fn table_key(&mut self, k: Option<super::games::Key>) {
        let t = self.now();
        match (k, self.table.as_mut()) {
            (Some(k), Some(tb)) => tb.key(k, t),
            (None, _) => {
                if let Some(old) = self.table.take() {
                    self.left_table(&old);
                }
                // up from the table, everyone
                if let Some(inn) = self.inside.as_mut() {
                    inn.game_over();
                }
            }
            _ => {}
        }
    }

    pub fn table_click(&mut self, x: i32, y: i32) {
        let t = self.now();
        if let Some(tb) = &mut self.table {
            tb.click(x, y, t);
        }
    }

    /// The game: their moves; a game over goes on the scorecard (once).
    pub(super) fn update_table(&mut self, t: f32) {
        // (a game with a friend online goes on anywhere; with a neighbour,
        // only in their home)
        let online = self.table.as_ref().is_some_and(|tb| tb.net.is_some());
        if self.inside.is_none() && !online {
            self.table = None;
            return;
        }
        let Some(tb) = &mut self.table else { return };
        tb.update(t);
        if let (Some(r), false) = (tb.result, tb.scored) {
            tb.scored = true;
            match r {
                super::games::Result::Won => {
                    self.pockets.games_won += 1;
                    self.cheer(2);
                }
                super::games::Result::Lost => self.pockets.games_lost += 1,
                super::games::Result::Drawn => {}
            }
        }
    }

    /// Where a trip starts: the station (skiing) or the car by home (a
    /// drive): (column, row) to walk to.
    pub(super) fn trip_spot(&self, ski: bool) -> Option<(f32, f32)> {
        let x = if ski {
            self.ctx.station.map(|(sx, sw)| (sx + sw / 2) as f32)?
        } else {
            self.home.and_then(|h| self.village.houses.get(h)).map(|h| h.door() as f32 + 7.0)?
        };
        Some((x, self.lo_at(x)))
    }

    /// Off on a trip (the station, or the car by home, reached).
    pub(super) fn start_trip(&mut self, ski: bool) {
        let t = self.now();
        // (in a shared village, the same train and mountain for everyone)
        let seed = if self.online.village().is_some() { self.seed ^ 0x51ed } else { self.rng.next_u32() };
        self.goal = None;
        self.trip = Some(if ski { super::trips::Trip::ski(t, seed) } else { super::trips::Trip::drive(t, seed) });
        if ski {
            self.share_trip();
        }
        self.director.sound(if ski { Sfx::TrainWhistle } else { Sfx::Honk }, self.player.as_ref().map(|p| p.centre()).unwrap_or(0.0), 0.8);
    }

    /// Along to where someone online is on their trip.
    pub(super) fn join_trip(&mut self, id: u32) {
        let t = self.now();
        let Some(at) = self.online.other(id).and_then(|o| o.av.trip).filter(|a| (1..=5).contains(&a.at)) else {
            self.post("they're not on a trip just now");
            return;
        };
        self.goal = None;
        self.trip = Some(super::trips::Trip::join(t, at));
        self.director.sound(Sfx::TrainWhistle, self.player.as_ref().map(|p| p.centre()).unwrap_or(0.0), 0.6);
    }

    /// A race with friends: up the chairlift, and down from the top at
    /// `at` (scene time).
    pub(super) fn start_race(&mut self, seed: u32, at: f32) {
        let t = self.now();
        self.goal = None;
        self.trip = Some(super::trips::Trip::racing(t, seed, at - t));
    }

    /// The trip, each frame: and back to the village once it's over.
    pub(super) fn update_trip(&mut self, dt: f32, t: f32) {
        self.update_trip_online();
        let mv = self.mv;
        let Some(tr) = &mut self.trip else { return };
        tr.update(dt, t, mv);
        for (sfx, v) in tr.take_sounds() {
            self.director.sound(sfx, self.cam + self.w as f32 / 2.0, v);
        }
        if tr.done.is_none() {
            return;
        }
        self.soon(Pending::TripEnd);
    }

    /// Back from the trip (the view shut): where it started.
    pub(super) fn finish_trip(&mut self) {
        let t = self.now();
        let Some(tr) = &self.trip else { return };
        let Some(back) = tr.done else { return };
        let (runs, best, km, score) = (tr.runs, tr.best, tr.km, tr.best_score);
        self.trip = None;
        self.pockets.ski_score = self.pockets.ski_score.max(score);
        self.pockets.ski_runs += runs;
        if let Some(b) = best {
            self.pockets.ski_best = Some(self.pockets.ski_best.map_or(b, |o: f32| o.min(b)));
        }
        self.pockets.km += km;
        // back where it started
        let spot = self.trip_spot(back == super::trips::Back::Station);
        if let (Some((x, y)), Some(p)) = (spot, self.player.as_mut()) {
            p.x = x - 1.0;
            p.y = y;
            p.perch = None;
            p.lie = None;
            self.look_at(x);
            self.cam = self.cam_target;
            self.ctx.cam = self.cam;
        }
        let line = if back == super::trips::Back::Station { "home again — what a day!" } else { "nice drive. home sweet home" };
        self.you_say(line, t + 0.3);
    }

    /// The shuttle stop on the snowfield below the station: a post and a
    /// sign (the shuttle sleigh runs from here up to the trains).
    pub(super) fn draw_stop(&self, f: &mut crate::frame::Frame, glow: f32) {
        let Some((x, y)) = self.trip_spot(true) else { return };
        let (x, y) = (x.round() as i32 + 3, y as i32 - 1);
        for row in y - 5..=y {
            f.put_g(x, row, crate::frame::Glyph::new('┃', Rgb::hex(0x4a3a2a)).lit(0.5));
        }
        let bg = crate::frame::Bg::Solid(Rgb::hex(0x2a4c8a));
        f.text(x - 4, y - 6, " ⇄ SHUTTLE ", Rgb::hex(0xffd36b), bg, 0.3);
        f.text(x - 4, y - 5, " to trains ", Rgb::hex(0xdfe7f4), bg, 0.3);
        f.light_cell(x, y - 6, Rgb::hex(0xffb050) * (0.8 * glow));
    }

    /// The villager's car, parked by their home.
    pub(super) fn draw_car(&self, f: &mut crate::frame::Frame, glow: f32) {
        let Some((x, y)) = self.trip_spot(false) else { return };
        let (x0, y0) = (x.round() as i32 - 5, (y as i32 - 2) * 2);
        let body = Rgb::hex(0x9a1c28);
        let rows: [&str; 6] = ["  ######   ", " #==#==#   ", "###########", "###########", " @@     @@ ", "           "];
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let c = match ch {
                    '#' => body * if dy == 0 { 1.1 } else { 1.0 },
                    '=' => Rgb::hex(0x9ac8f0),
                    '@' => Rgb::hex(0x1a1a22),
                    _ => continue,
                };
                f.px_set(x0 + dx as i32, y0 + dy as i32, c);
            }
        }
        // snow on the roof, a warm sidelight
        for dx in 2..8 {
            f.px_set(x0 + dx, y0 - 1, Rgb::hex(0xf4f8ff));
        }
        f.light_px((x0 + 10) as f32, (y0 + 2) as f32, Rgb::hex(0xffb050) * (1.2 * glow));
    }

    /// T: what there is to do right here (title, choices).
    pub fn todo_menu(&self) -> (String, Vec<(String, Do)>) {
        let mut v: Vec<(String, Do)> = vec![];
        let Some(p) = self.player.as_ref() else { return ("things to do".into(), v) };
        let (px, py) = (p.centre(), p.y);
        // with friends online (first: it's what's new)
        if !self.online.others().is_empty() {
            v.push((format!("☺ with the {} here online: play, wave, race ▸", self.online.others().len() + 1), Do::More(super::online::MORE_PEOPLE)));
        }
        v.push(("✉ friends & online: play together ▸".into(), Do::More(super::online::MORE_ONLINE)));
        if self.inside.is_some() {
            v.extend(self.game_choices());
            return ("things to do".into(), v);
        }
        let t = self.now();
        // a build going up, or a finished igloo, nearby
        let mut i = 0;
        let mut seen_igloo = false;
        while let Some((bx, _, _, igloo)) = self.builds.spot(i) {
            if (bx - px).abs() < 30.0 {
                if let Some((snowman, prog)) = self.builds.about(i) {
                    let what = if snowman { "snowman" } else { "igloo" };
                    if prog < 1.0 {
                        v.push((format!("☃ help build the {what}"), Do::HelpBuild(bx)));
                    } else if igloo && !seen_igloo {
                        seen_igloo = true;
                        v.push(("◠ crawl into the igloo".into(), Do::Crawl(bx)));
                    }
                }
            }
            i += 1;
        }
        // somewhere near with room for one
        let lo = self.lo_at(px);
        let on_snow = p.perch.is_none();
        if on_snow {
            for igloo in [false, true] {
                let half = self.builds.half_for(igloo);
                let spot = [half + 3.0, -(half + 3.0), half + 9.0, -(half + 9.0)].iter().map(|o| px + o * p.dir).find_map(|x| {
                    let base = py.max(lo + 3.0).min(self.h as f32 - 1.0);
                    self.build_spot_free(x, base as i32, half).then_some((x, base))
                });
                if let Some((x, base)) = spot {
                    v.push((if igloo { "⌂ build an igloo here".into() } else { "☃ build a snowman here".into() }, Do::Build(igloo, x, base)));
                }
            }
            v.push(("✻ make a snow angel".into(), Do::Angel));
        }
        v.push((if p.lying() { "☾ get up again".into() } else { "☾ lie down and rest".into() }, Do::Rest));
        v.push(("♪ sing a carol".into(), Do::Sing));
        let along = self.critters.follower_ids().len();
        if self.critters.team_score().is_none() && !self.critters.people().is_empty() {
            let label = if along == 0 { "● a snowball fight (me against everyone)".to_string() } else { format!("● a team snowball fight (us {} vs them)", along + 1) };
            v.push((label, Do::TeamFight));
        }
        v.push(("✦ a light show ▸".into(), Do::More(0)));
        v.push(("✸ set off fireworks ▸".into(), Do::More(1)));
        // who's about
        let people = self.critters.people();
        let mut near: Vec<&(u64, f32, f32, bool)> = people.iter().filter(|q| (q.1 - px).abs() < 40.0).collect();
        near.sort_by(|a, b| (a.1 - px).abs().total_cmp(&(b.1 - px).abs()));
        if let Some(&&(id, ..)) = near.first() {
            let name = self.critters.label(id);
            v.push((format!("♥ say hi to {name}"), Do::Talk(id)));
            v.push((format!("● snowball at {name}"), Do::Snowball(id)));
        }
        if let Some(&&(id, ..)) = near.iter().find(|q| q.3) {
            if self.tag.is_none() {
                let name = self.critters.label(id);
                v.push((format!("⚑ play tag with {name}"), Do::Tag(id)));
            }
        }
        if let Some(&(id, ax, ..)) = self.critters.animals().iter().filter(|a| (a.1 - px).abs() < 30.0).min_by(|a, b| (a.1 - px).abs().total_cmp(&(b.1 - px).abs())) {
            let what = self.critters.label(id);
            let feed = p.berries > 0;
            let _ = ax;
            v.push((format!("✿ {} {what}", if feed { "feed" } else { "say hello to" }), Do::Pet(id)));
        }
        if let Some(&(bx, base)) = self.critters.ripe_bushes(t).iter().filter(|b| (b.0 - px).abs() < 40.0).min_by(|a, b| (a.0 - px).abs().total_cmp(&(b.0 - px).abs())) {
            v.push(("✿ pick some berries".into(), Do::Pick(bx, base.max(self.lo_at(bx)))));
        }
        if self.shop.is_some() {
            v.push(("◡ cocoa at the cabin".into(), Do::Shop));
        }
        if self.ctx.station.is_some() {
            v.push(("⇄ the shuttle to the station, and the train up to the slopes".into(), Do::Trip(true)));
        }
        if self.home.is_some() {
            v.push(("⚐ a drive in the car".into(), Do::Trip(false)));
        }
        if let Some(h) = self.home {
            v.push(("⚑ go home".into(), Do::Visit(h)));
        }
        if self.daylight.sun < 0.25 {
            v.push(("✦ wish on a star".into(), Do::Wish));
        }
        ("things to do".into(), v)
    }
}

/// Santa, tumbled out of his sleigh (clicked once too often): down into the
/// snow, a moment seeing stars, then off at a run, away from the villager,
/// a touch faster than whoever's chasing him, until he's run clean off the
/// screen. Almost never catchable: now and then (not every time) he slips
/// on a patch of ice once he's been running a while, and someone right on
/// his heels just then has him.
pub struct Runaway {
    pub x: f32,
    pub y: f32,
    vy: f32,
    /// Where he lands; dizzy until; running (since).
    land: f32,
    dizzy_until: f32,
    running: Option<f32>,
    pub dir: f32,
    phase: f32,
    next_line: f32,
    pub gone: bool,
    /// Whether there's a patch of ice ahead for him this time, when he
    /// slips on it, and when he was caught.
    pub(super) slips: bool,
    slip: Option<f32>,
    caught: Option<f32>,
}

impl Runaway {
    pub fn rebase(&mut self, s: f32) {
        self.dizzy_until -= s;
        self.next_line -= s;
        self.running = self.running.map(|r| r - s);
        self.slip = self.slip.map(|v| v - s);
        self.caught = self.caught.map(|v| v - s);
    }
}

const SANTA_RUN: &[&str] = &["ho ho — can't catch me!", "ho ho ho! too slow!", "the reindeer will be wondering…", "catch me if you can!", "ho ho HO!"];

impl Scene {
    /// The sleigh clicked again and again: out he tumbles.
    pub(super) fn santa_spammed(&mut self, t: f32) -> bool {
        self.santa_clicks.retain(|&c| t - c < 2.5);
        self.santa_clicks.push(t);
        if self.santa_clicks.len() < 5 || self.runaway.is_some() {
            return false;
        }
        let Some((sx, sy)) = self.sleigh.santa_at(t) else { return false };
        self.santa_clicks.clear();
        let x = sx + self.cam;
        let land = match self.player.as_ref() {
            Some(p) if p.perch.is_none() => p.y,
            _ => (self.lo_at(x) + 3.0).min(self.h as f32 - 2.0),
        };
        let away = self.player.as_ref().map(|p| (x - p.centre()).signum()).unwrap_or(1.0);
        self.runaway = Some(Runaway { x, y: sy - self.oy_i() as f32, vy: -4.0, land, dizzy_until: 0.0, running: None, dir: if away == 0.0 { 1.0 } else { away }, phase: 0.0, next_line: 0.0, gone: false, slips: crate::rng::h01(crate::rng::hash((t * 1000.0) as u32 ^ 0x5a1c)) < 0.25, slip: None, caught: None });
        self.sleigh.santa_out = true;
        self.director.say(x.round() as i32, sy as i32 - 2, "whoa — WHOA!", t, 1.8, 8);
        self.director.sound(Sfx::Magic, x, 0.8);
        true
    }

    /// Santa on the run, each frame.
    pub(super) fn update_runaway(&mut self, dt: f32, t: f32) {
        let Some(mut r) = self.runaway.take() else { return };
        let k = super::sprite_k(self.h);
        let you = self.player.as_ref().map(|p| (p.centre(), p.y, p.pace(), p.moving));
        if r.running.is_none() && r.dizzy_until == 0.0 {
            // falling
            r.vy += 70.0 * dt;
            r.y += r.vy * dt;
            if r.y >= r.land {
                r.y = r.land;
                r.dizzy_until = t + 1.6;
                self.director.snow_puff(r.x, r.y);
                self.director.burst(r.x, r.y - 1.0, 14, &[Rgb::WHITE, Rgb::hex(0xffd36b)], 4.0, 3.0);
                self.director.sound(Sfx::Drop, r.x, 1.0);
                self.director.say(r.x.round() as i32, r.y as i32 - 4, "oof! ho… ho?", t, 1.6, 8);
            }
        } else if r.running.is_none() {
            if t >= r.dizzy_until {
                r.running = Some(t);
                if let Some((px, ..)) = you {
                    r.dir = if (r.x - px).abs() < 0.5 { 1.0 } else { (r.x - px).signum() };
                }
                self.director.say(r.x.round() as i32, r.y as i32 - 4, "ho ho — you'll never catch me!", t, 2.4, 8);
                self.director.sound(Sfx::Laugh, r.x, 0.9);
                r.next_line = t + 4.0;
                // (one time in four or so, a patch of ice somewhere ahead)
                if r.slips {
                    r.slip = Some(t + 6.0 + 8.0 * crate::rng::h01(crate::rng::hash((t * 977.0) as u32)));
                }
            }
        } else if let Some(at) = r.caught {
            // caught! a word, a gift, then the sleigh's back for him
            if t - at > 4.5 {
                r.gone = true;
            }
        } else {
            // away from the villager: walking pace to begin with; chased,
            // always a tenth faster than they are (and faster still if
            // they've somehow got right up to him)
            let walk = 11.0 * k;
            let mut speed = walk;
            // down on the ice a moment: there for the catching
            let slipping = r.slip.is_some_and(|s| t >= s && t < s + 1.4);
            if r.slip.is_some_and(|s| t >= s && t - dt < s) {
                self.director.say(r.x.round() as i32, r.y as i32 - 4, "whoa — ice!", t, 1.4, 8);
                self.director.snow_puff(r.x, r.y);
                self.director.sound(Sfx::Drop, r.x, 0.8);
            }
            if let (true, Some((px, py, ..))) = (slipping, you) {
                if (r.x - px).abs() < 2.2 && (r.y - py).abs() < 3.0 {
                    r.caught = Some(t);
                    self.santa_caught(r.x, r.y, t);
                    self.runaway = Some(r);
                    return;
                }
            }
            if let Some(s) = r.slip.filter(|s| t >= s + 1.4 && t - dt < s + 1.4) {
                let _ = s;
                self.director.say(r.x.round() as i32, r.y as i32 - 4, "ho ho — a close one!", t, 1.8, 8);
            }
            if slipping {
                speed = 0.0;
            } else if let Some((px, _, pace, moving)) = you {
                let chasing = moving && (r.x - px).signum() == r.dir;
                if chasing {
                    speed = speed.max(1.1 * 11.0 * pace * k);
                }
                if (r.x - px).abs() < 4.0 {
                    speed = speed.max(1.1 * 11.0 * pace * k + 6.0);
                }
            }
            r.x += r.dir * speed * dt;
            r.phase += dt * 12.0 * (speed > 0.0) as i32 as f32;
            if t > r.next_line {
                r.next_line = t + 4.5;
                let line = *self.rng.pick(SANTA_RUN);
                self.director.say(r.x.round() as i32, r.y as i32 - 4, line, t, 2.2, 8);
            }
            if (t * 6.0).fract() < dt * 6.0 {
                self.director.snow_puff(r.x - r.dir * 2.0, r.y);
            }
            // clean off the screen (or off the end of the world): gone
            let (l, rr) = (self.cam - 8.0, self.cam + self.w as f32 + 8.0);
            if r.x < l || r.x > rr || r.x < -6.0 || r.x > self.ww as f32 + 6.0 {
                r.gone = true;
            }
        }
        if r.gone {
            self.sleigh.santa_out = false;
            self.pockets.santa_chases += 1;
            return;
        }
        self.runaway = Some(r);
    }

    /// Santa caught: he laughs, a present for the villager (and one under
    /// the tree), the stats say so.
    fn santa_caught(&mut self, x: f32, y: f32, t: f32) {
        self.director.say(x.round() as i32, y as i32 - 4, "ho ho HO! you caught me! the nice list, for sure", t, 4.0, 8);
        self.director.burst(x, y - 2.0, 30, &[Rgb::hex(0xffd36b), Rgb::WHITE, Rgb::hex(0xd8283a), Rgb::hex(0x7fe0a0)], 6.0, 3.0);
        self.director.sound(Sfx::Fanfare, x, 0.9);
        self.pockets.santa_caught += 1;
        self.cheer(25);
        self.present_from_santa(t);
        if let Some(p) = &mut self.player {
            p.wave_until = t + 2.0;
        }
    }

    /// The hot-air balloon: now and then one comes down over the village
    /// and waits a while; one comes for whoever's up on the big tree's
    /// star; aboard, it's steered (see `balloon.rs`).
    pub(super) fn update_balloon(&mut self, dt: f32, t: f32) {
        let k = super::sprite_k(self.h);
        if self.balloon.is_none() && self.inside.is_none() && self.trip.is_none() && self.online.guest_seed().is_none() {
            if let Some(p) = &self.player {
                let star = self.trees.iter().find(|tr| tr.is_big()).map(|tr| tr.star());
                let on_star = star.is_some_and(|(sx, sy)| p.perch.is_some() && (p.centre() - sx as f32).abs() < 3.0 && p.y <= sy as f32 + 3.0);
                let (pc, py) = (p.centre(), p.y);
                if on_star && !self.balloon_star {
                    // up on the star: one drifts over, its ladder down to them
                    self.balloon_star = true;
                    let from = self.cam - 40.0;
                    let seed = self.rng.next_u32();
                    self.balloon = Some(super::balloon::Balloon::coming(from, py - 30.0, pc + 5.0 * k, py, 5.0 * k, seed));
                    self.director.say(pc.round() as i32, py as i32 - 4, "is that… a balloon?!", t, 2.6, 8);
                } else if !on_star && t > self.balloon_next && p.perch.is_none() {
                    self.balloon_next = t + self.rng.range(700.0, 1600.0);
                    // somewhere in view, on the snow (not on a house)
                    let x = (self.cam + self.w as f32 * self.rng.range(0.2, 0.8)).clamp(8.0, self.ww as f32 - 8.0);
                    if !self.village.houses.iter().any(|h| h.contains(x as i32, (self.lo_at(x) + 2.0) as i32)) {
                        let ground = (self.lo_at(x) + 3.0).min(self.h as f32 - 2.0);
                        let from = if x > pc { self.cam + self.w as f32 + 40.0 } else { self.cam - 40.0 };
                        let seed = self.rng.next_u32();
                        self.balloon = Some(super::balloon::Balloon::coming(from, -12.0, x, ground, 0.0, seed));
                        self.director.say(pc.round() as i32, py as i32 - 4, "look — a hot-air balloon!", t, 2.6, 8);
                    }
                }
                if !on_star {
                    self.balloon_star = false;
                }
            }
        }
        let ww = self.ww as f32;
        let Some(mut b) = self.balloon.take() else { return };
        let ground = (self.lo_at(b.x.clamp(0.0, ww - 1.0)) + 3.0).min(self.h as f32 - 2.0);
        let sky = -(self.h as f32) * 0.8;
        let mv = if b.aboard { self.mv } else { (0.0, 0.0) };
        if b.update(dt, t, mv, ground, sky, ww) {
            self.director.sound(Sfx::Puff, b.x, 0.5);
        }
        if b.aboard {
            if let Some(p) = &mut self.player {
                p.active_at = t;
                p.ride(b.x - 1.0, b.y - 0.8);
            }
        }
        if !b.gone(ww) {
            self.balloon = Some(b);
        }
    }

    /// E by (or in) the balloon: aboard, or (low over the snow) off again.
    pub(super) fn balloon_e(&mut self, t: f32) -> bool {
        let Some(bx) = self.balloon.as_ref().map(|b| b.x) else { return false };
        let ground = (self.lo_at(bx.clamp(0.0, self.ww as f32 - 1.0)) + 3.0).min(self.h as f32 - 2.0);
        let (Some(b), Some(p)) = (self.balloon.as_mut(), self.player.as_mut()) else { return false };
        if b.aboard {
            if b.landed(ground) {
                b.hop_off();
                p.ride(b.x + 3.0, ground);
                self.director.say(p.centre().round() as i32, p.y as i32 - 4, "what a view! thanks, balloon", t, 2.4, 8);
                self.pockets.balloon += 1;
            } else {
                self.director.say(p.centre().round() as i32, p.y as i32 - 4, "too high to hop off — ↓ to come down", t, 2.0, 8);
            }
            return true;
        }
        if b.reachable(p.centre(), p.y) {
            b.board();
            p.ride(b.x - 1.0, b.y - 0.8);
            self.director.say(p.centre().round() as i32, p.y as i32 - 6, "up, up and away! ← → drift · ↑ burner · ↓ down · E on the snow: off", t, 4.5, 8);
            self.director.sound(Sfx::Magic, b.x, 0.7);
            return true;
        }
        false
    }

    /// Santa running about in the snow (in front of everything in the
    /// village).
    pub(super) fn draw_runaway(&self, f: &mut crate::frame::Frame, t: f32) {
        let Some(r) = &self.runaway else { return };
        use super::sprites::{self, Look, Pose};
        let k = super::sprite_k(self.h);
        let slipping = r.slip.is_some_and(|s| t >= s && t < s + 1.4);
        let pose = if r.caught.is_some() {
            if (t * 2.0) as i32 % 2 == 0 { Pose::Wave } else { Pose::Stand }
        } else if slipping {
            Pose::Lie
        } else if r.running.is_some() {
            Pose::Walk((r.phase as i32 % 2) as u8)
        } else if r.dizzy_until > 0.0 {
            Pose::Stand
        } else {
            Pose::ArmsUp
        };
        if crate::frame::pictures_ok() {
            let red = Rgb::hex(0xc8202a);
            let look = Look { kid: false, coat: red, hat: Some(red), hair: Rgb::hex(0xf2efe8), scarf: Some(Rgb::hex(0xf2efe8)), skin: Rgb::hex(0xf0c8a8), mittens: Rgb::hex(0x2a2a34), holding: None, lantern: false };
            let pic = sprites::person(&look, pose, r.dir, 1.0);
            let rows = sprites::rows_of(&pic, k);
            f.sprite(r.x, r.y + 1.0, rows, pic);
        } else {
            super::sleigh::draw_santa(f, r.x.round() as i32, r.y.round() as i32 - 2);
            let legs = if (r.phase as i32) % 2 == 0 { '╱' } else { '╲' };
            f.put_g(r.x.round() as i32, r.y.round() as i32, crate::frame::Glyph::new(legs, Rgb::hex(0x2a2a34)).lit(0.5));
        }
        if r.running.is_none() && r.dizzy_until > 0.0 && t < r.dizzy_until {
            for i in 0..3 {
                let a = t * 5.0 + i as f32 * 2.09;
                let (x, y) = (r.x + a.cos() * 2.0, r.y - 5.5 * k + a.sin() * 0.5);
                f.put_g(x.round() as i32, y.round() as i32, crate::frame::Glyph::new('✦', Rgb::hex(0xffe08a)).lit(0.0));
            }
        }
    }
}
