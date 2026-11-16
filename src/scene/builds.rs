//! Kids out in the snow building things: a snowman rolled up ball by ball,
//! or an igloo raised course by course — which, once it's finished, they
//! all pile into for a while (giggling) before coming back out. What they
//! build stands for about ten minutes, then slowly melts away.
//!
//! The villager can pitch in (E beside it: pushing the snowball bigger, or
//! setting a block), start one of their own (the kids nearby come over to
//! help), and crawl into a finished igloo. What the villager helped with gets
//! a touch of theirs: their red scarf on the snowman, a flag on the igloo.

use super::critters::{Critters, Hidden};
use super::events::Director;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum What {
    Snowman,
    Igloo,
}

struct Build {
    what: What,
    /// Centre column and bottom row (cells).
    x: f32,
    base: i32,
    /// Size (1 = the 75-row design).
    u: f32,
    /// How far along (0..1).
    p: f32,
    kids: Vec<u64>,
    /// Everyone who worked on it (kept after they wander off).
    builders: Vec<u64>,
    inside: Vec<Hidden>,
    /// Igloo: heading in (until), and when they come back out.
    entering: Option<f32>,
    out_at: f32,
    done_at: Option<f32>,
    /// Last time anyone was working on it.
    worked: f32,
    melt: f32,
    next_line: f32,
    /// The villager lent a hand (or started it).
    helped: bool,
    by_you: bool,
    /// Presses of E on it (for the villager's lines).
    pushes: u32,
    /// The villager's curled up inside (an igloo).
    you_inside: bool,
}

impl Build {
    fn new(what: What, x: f32, base: i32, u: f32, t: f32) -> Self {
        Build { what, x, base, u, p: 0.0, kids: vec![], builders: vec![], inside: vec![], entering: None, out_at: 0.0, done_at: None, worked: t, melt: 0.0, next_line: t + 3.0, helped: false, by_you: false, pushes: 0, you_inside: false }
    }

    /// Half its width in columns.
    fn half(&self) -> f32 {
        match self.what {
            What::Snowman => 5.5 * self.u * SNOW_K + 1.0,
            What::Igloo => IGLOO_R * self.u,
        }
    }
    fn door(&self) -> f32 {
        self.x
    }
}

/// Snowman size (relative to the big one by the tree) and igloo radius
/// (pixels at the 75-row design).
const SNOW_K: f32 = 0.8;
const IGLOO_R: f32 = 9.0;

pub struct Builds {
    list: Vec<Build>,
    next: f32,
    rng: Rng,
    u: f32,
    /// Snowmen and igloos finished (for the stats).
    pub made: u32,
}

const SNOWMAN_WORK: &[&str] = &["roll it bigger!", "push! push!", "it's getting HUGE", "now the middle one!", "careful — don't squash it!", "I'll do the head!", "who's got a carrot?", "he needs a hat!"];
const IGLOO_WORK: &[&str] = &["more blocks!", "pat it down, pat it down", "best igloo EVER", "leave a hole for the door!", "is it big enough for all of us?", "pack it tight!"];
const SNOWMAN_DONE: &[&str] = &["ta-da!", "he's perfect!", "let's call him Mr. Frosty!", "best snowman EVER"];
const IGLOO_DONE: &[&str] = &["it's done! everybody in!", "last one inside's a snowball!", "come on, in we go!"];
const INSIDE: &[&str] = &["it's so cozy in here!", "shh — secret club!", "hee hee hee", "warmer than you'd think!", "knock knock!", "this is our house now"];
const OUT: &[&str] = &["brr — back out into the cold!", "that was the best!", "let's build another one!", "my mittens are soaked!"];
const YOU_SNOWMAN: &[&str] = &["heave!", "rolling, rolling…", "one more push!", "it's getting heavy!", "nice and round"];
const YOU_IGLOO: &[&str] = &["one more block!", "there — pack it in", "nice and snug", "another course!", "mind the doorway"];
const THANKS: &[&str] = &["thanks!!", "you're really good at this!", "yay, a grown-up's helping!", "it's going so fast now!", "push harder!", "teamwork!"];
const JOIN: &[&str] = &["can I help?", "ooh, I want to help!", "wait for me!", "I'll pat the snow down!"];
const YOU_INSIDE: &[&str] = &["it's so cozy in here", "I could stay in here all winter", "shh… it's so quiet", "everything's blue and glowy in here"];

impl Builds {
    pub fn new(h: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0xb11d);
        Builds { list: vec![], next: rng.range(25.0, 70.0), rng, u: (h as f32 / 75.0).clamp(0.5, 1.4), made: 0 }
    }

    pub fn rebase(&mut self, s: f32) {
        self.next -= s;
        for b in &mut self.list {
            b.out_at -= s;
            b.worked -= s;
            b.next_line -= s;
            if let Some(d) = &mut b.done_at {
                *d -= s;
            }
            if let Some(e) = &mut b.entering {
                *e -= s;
            }
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> (usize, usize) {
        (self.list.len(), self.list.iter().map(|b| b.inside.len()).sum())
    }

    /// A kid busy building: is it an igloo?
    pub fn working(&self, id: u64) -> Option<bool> {
        self.list.iter().find(|b| b.done_at.is_none() && b.kids.contains(&id)).map(|b| b.what == What::Igloo)
    }

    /// Something finished that this kid helped build, near column `x`.
    pub fn made_by(&self, id: u64, x: f32) -> Option<bool> {
        self.list.iter().find(|b| b.p >= 1.0 && b.melt < 0.5 && b.builders.contains(&id) && (b.x - x).abs() < 40.0).map(|b| b.what == What::Igloo)
    }

    /// Something (mostly) built near column `x`.
    pub fn near(&self, x: f32) -> Option<bool> {
        self.list.iter().find(|b| b.p >= 0.5 && b.melt < 0.5 && (b.x - x).abs() < 30.0).map(|b| b.what == What::Igloo)
    }

    /// The villager lends a hand with what `id` is building.
    pub fn help(&mut self, id: u64, d: &mut Director) -> bool {
        let Some(b) = self.list.iter_mut().find(|b| b.done_at.is_none() && b.kids.contains(&id)) else { return false };
        b.p = (b.p + 0.2).min(0.99);
        d.snow_puff(b.x, b.base as f32);
        d.sound(Sfx::Crunch, b.x, 0.6);
        true
    }

    /// What the villager at (x, y) can do with a build: pitch in on one being
    /// built (false), or crawl into a finished igloo (true).
    pub fn at(&self, x: f32, y: f32) -> Option<(usize, bool)> {
        self.list.iter().enumerate().find_map(|(i, b)| {
            let near_row = (y - b.base as f32).abs() <= 4.0;
            if b.done_at.is_none() && b.melt == 0.0 && near_row && (x - b.x).abs() <= b.half() + 5.0 {
                Some((i, false))
            } else if b.what == What::Igloo && b.p >= 1.0 && b.melt < 0.4 && b.entering.is_none() && near_row && (x - b.door()).abs() <= 3.5 {
                Some((i, true))
            } else {
                None
            }
        })
    }

    /// The build drawn over cell (x, y), if any.
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        self.list.iter().position(|b| {
            let tall = match b.what {
                What::Snowman => 12.5 * b.u * SNOW_K * b.p.max(0.3),
                What::Igloo => IGLOO_R * b.u * 0.5 * b.p.max(0.2),
            };
            (x as f32 - b.x).abs() <= b.half() && y <= b.base && (y as f32) >= b.base as f32 - tall - 1.0
        })
    }

    /// Half the width (columns) of a snowman or igloo built now.
    pub fn half_for(&self, igloo: bool) -> f32 {
        if igloo {
            IGLOO_R * self.u
        } else {
            5.5 * self.u * SNOW_K + 1.0
        }
    }

    /// The build at column x (as a menu named it).
    pub fn find(&self, x: f32) -> Option<usize> {
        self.list.iter().position(|b| (b.x - x).abs() < 0.5)
    }

    /// A build's centre, bottom row and whether it's a finished igloo (for
    /// walking over to it).
    pub fn spot(&self, i: usize) -> Option<(f32, f32, f32, bool)> {
        self.list.get(i).map(|b| (b.x, b.base as f32 + 1.0, b.half(), b.what == What::Igloo && b.p >= 1.0))
    }

    /// Is it a snowman, and how far along?
    pub fn about(&self, i: usize) -> Option<(bool, f32)> {
        self.list.get(i).map(|b| (b.what == What::Snowman, b.p))
    }

    /// The villager lends a hand with build `i` (E beside it): a push on the
    /// snowball, a block on the igloo. True once it's finished.
    pub fn pitch_in(&mut self, i: usize, px: f32, py: f32, t: f32, c: &Critters, d: &mut Director) -> bool {
        let Some(b) = self.list.get_mut(i).filter(|b| b.done_at.is_none()) else { return false };
        let snowman = b.what == What::Snowman;
        let before = b.p;
        b.p = (b.p + if snowman { 0.07 } else { 0.05 }).min(1.0);
        b.helped = true;
        b.worked = t;
        b.pushes += 1;
        let sx = b.x + self.rng.range(-b.half(), b.half()) * 0.7;
        d.snow_puff(sx, b.base as f32);
        d.sound(Sfx::Crunch, b.x, 0.55);
        // the villager's part, now and then
        if b.pushes % 3 == 1 {
            let line = *self.rng.pick(if snowman { YOU_SNOWMAN } else { YOU_IGLOO });
            d.say(px.round() as i32, py as i32 - 4, line, t, 1.8, 4);
        }
        // a new stage of the snowman
        let stage = |a: f32| before < a && b.p >= a;
        let call = if snowman && stage(0.45) {
            Some("now the middle one!")
        } else if snowman && stage(0.75) {
            Some("now the head!")
        } else if !snowman && stage(0.85) {
            Some("leave a hole for the door!")
        } else if self.rng.chance(0.3) {
            Some(*self.rng.pick(THANKS))
        } else {
            None
        };
        if let Some(line) = call {
            if let Some((kx, ky)) = b.kids.get(self.rng.int(0, b.kids.len().max(1) as i32) as usize).and_then(|&id| c.position(id)) {
                d.say(kx.round() as i32, ky as i32 - 4, line, t + 0.5, 2.2, 4);
            }
        }
        b.p >= 1.0
    }

    /// The villager starts building something at column x, row `base`; free
    /// kids close by come over to help.
    pub fn start(&mut self, what: What, x: f32, base: i32, t: f32, c: &mut Critters, d: &mut Director) {
        let mut b = Build::new(what, x, base, self.u, t);
        b.by_you = true;
        b.helped = true;
        let half = b.half();
        let busy = self.busy();
        // whoever's walking along with the villager pitches in first; then
        // a kid or two from round about
        let mut kids: Vec<(u64, f32, f32)> = c.follower_ids().into_iter().filter_map(|id| c.position(id).map(|(px, py)| (id, px, py))).collect();
        let mut more: Vec<(u64, f32, f32)> = c.people().into_iter().filter(|p| p.3 && !busy.contains(&p.0) && (p.1 - x).abs() < 60.0).map(|p| (p.0, p.1, p.2)).collect();
        more.sort_by(|a, b2| (a.1 - x).abs().total_cmp(&(b2.1 - x).abs()));
        kids.extend(more);
        for (k, &(id, kx, ky)) in kids.iter().take(3).enumerate() {
            let side = if k % 2 == 0 { -1.0 } else { 1.0 };
            c.work_at(id, x + side * (half + 2.0), base as f32, x, t + 90.0);
            b.kids.push(id);
            b.builders.push(id);
            d.say(kx.round() as i32, ky as i32 - 4, self.rng.pick(JOIN), t + 0.8 + k as f32, 2.2, 6);
        }
        if kids.is_empty() && self.rng.chance(0.7) {
            // word gets round: a kid or two comes out to join in
            c.request(super::events::Kind::Kid, t);
        }
        self.list.push(b);
    }

    /// Curl up inside finished igloo `i`: its doorway (x, row), if it's
    /// there to crawl into.
    pub fn crawl_in(&mut self, i: usize) -> Option<(f32, f32)> {
        let b = self.list.get_mut(i).filter(|b| b.what == What::Igloo && b.p >= 1.0 && b.melt < 0.4)?;
        b.you_inside = true;
        b.next_line = 0.0;
        Some((b.door(), b.base as f32 + 1.0))
    }

    /// Who's inside igloo `i` (name, a kid, coat).
    pub fn guests(&self, i: usize) -> Vec<(&'static str, bool, crate::color::Rgb)> {
        self.list.get(i).map(|b| b.inside.iter().map(|h| h.about()).collect()).unwrap_or_default()
    }

    /// Knock build `i` down (no hard feelings): whoever's in an igloo
    /// tumbles out. Its column and row, and whether it was an igloo.
    pub fn knock_down(&mut self, i: usize, t: f32, h: f32, c: &mut Critters, d: &mut Director) -> Option<(f32, i32, bool)> {
        if i >= self.list.len() {
            return None;
        }
        let mut b = self.list.remove(i);
        for id in b.kids.drain(..) {
            c.stop_work(id, t);
        }
        let (n, door, base) = (b.inside.len(), b.door(), b.base);
        for (k, hd) in b.inside.drain(..).enumerate() {
            let x = door + (k as f32 - n as f32 / 2.0) * 3.0;
            c.put_back(hd, x, (base + 1).min(h as i32) as f32, t);
        }
        let half = b.half();
        for k in 0..5 {
            d.snow_puff(b.x - half + half * 0.5 * k as f32, b.base as f32 - 1.0);
        }
        Some((b.x, b.base, b.what == What::Igloo))
    }

    /// Previews: build `i` at progress p (helped by the villager).
    pub fn preview(&mut self, i: usize, p: f32) {
        if let Some(b) = self.list.get_mut(i) {
            b.p = p;
            b.helped = true;
            if p >= 1.0 {
                b.done_at = Some(0.0);
            }
        }
    }

    /// Out of the igloo again.
    pub fn crawl_out(&mut self) {
        for b in &mut self.list {
            b.you_inside = false;
        }
    }

    /// Column spans already taken (so nothing gets built on top of it).
    pub fn spans(&self) -> Vec<(i32, i32)> {
        self.list.iter().map(|b| ((b.x - b.half()) as i32 - 3, (b.x + b.half()) as i32 + 3)).collect()
    }

    /// Kids working on something (not free for anything else).
    fn busy(&self) -> Vec<u64> {
        self.list.iter().flat_map(|b| b.kids.iter().copied()).collect()
    }

    /// `can_start`: kids are out (not bedtime); `view`: (left, width);
    /// `free`: whether columns x0..x1 are clear to build on at a row; `lo`: the
    /// first snowfield row at a column; `h`: rows.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, can_start: bool, view: (f32, f32), free: &dyn Fn(i32, i32, i32) -> bool, lo: &dyn Fn(f32) -> f32, h: f32, c: &mut Critters, d: &mut Director) {
        // something new: when there are kids about and room in view
        if can_start && t > self.next && self.list.len() < 3 {
            self.next = t + self.rng.range(60.0, 150.0);
            let busy = self.busy();
            let kids: Vec<(u64, f32)> = c.people().into_iter().filter(|p| p.3 && !busy.contains(&p.0) && p.1 > view.0 && p.1 < view.0 + view.1).map(|p| (p.0, p.1)).collect();
            if kids.is_empty() {
                // a couple of kids come out to play, and try again shortly
                if self.rng.chance(0.6) {
                    c.request(super::events::Kind::Kid, t);
                    c.request(super::events::Kind::Kid, t);
                }
                self.next = t + self.rng.range(10.0, 20.0);
            } else {
                let what = if self.rng.chance(0.5) { What::Snowman } else { What::Igloo };
                let u = self.u * self.rng.range(0.85, 1.1);
                let half = match what {
                    What::Snowman => 5.5 * u * SNOW_K + 1.0,
                    What::Igloo => IGLOO_R * u,
                };
                for _ in 0..60 {
                    let x = view.0 + self.rng.range(8.0, (view.1 - 8.0).max(9.0));
                    let (x0, x1) = ((x - half) as i32 - 2, (x + half) as i32 + 2);
                    let top = lo(x) + 3.0;
                    if top > h - 2.0 {
                        continue;
                    }
                    let base = self.rng.range(top, h - 1.0).round() as i32;
                    if !free(x0, x1, base) || self.spans().iter().any(|&(a, b)| x1 > a && x0 < b) {
                        continue;
                    }
                    let mut b = Build::new(what, x, base, u, t);
                    let mut near = kids.clone();
                    near.sort_by(|a, b2| (a.1 - x).abs().total_cmp(&(b2.1 - x).abs()));
                    for (k, &(id, _)) in near.iter().take(3).enumerate() {
                        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
                        let tx = x + side * (half + 2.0 + (k / 2) as f32 * 3.0);
                        c.work_at(id, tx, base as f32, x, t + 90.0);
                        b.kids.push(id);
                        b.builders.push(id);
                    }
                    let (kx, ky) = c.position(b.kids[0]).unwrap_or((x, base as f32));
                    let line = if what == What::Snowman { "let's build a snowman!" } else { "let's make an igloo!" };
                    d.say(kx.round() as i32, ky as i32 - 4, line, t, 2.6, 6);
                    self.list.push(b);
                    break;
                }
            }
        }

        let busy = self.busy();
        for b in &mut self.list {
            // the villager's own: kids nearby come and join in, now and then
            if b.done_at.is_none() && b.by_you && b.kids.len() < 2 && self.rng.chance(dt * 0.2) {
                let near = c.people().into_iter().find(|p| p.3 && !busy.contains(&p.0) && !b.kids.contains(&p.0) && (p.1 - b.x).abs() < 50.0);
                if let Some((id, kx, ky, _)) = near {
                    let side = if b.kids.len() % 2 == 0 { -1.0 } else { 1.0 };
                    c.work_at(id, b.x + side * (b.half() + 2.0), b.base as f32, b.x, t + 90.0);
                    b.kids.push(id);
                    b.builders.push(id);
                    d.say(kx.round() as i32, ky as i32 - 4, self.rng.pick(JOIN), t, 2.2, 6);
                }
            }
            // the villager, curled up in the igloo
            if b.you_inside && t > b.next_line {
                b.next_line = t + self.rng.range(5.0, 9.0);
                let top = b.base as f32 - IGLOO_R * b.u / 2.0 - 2.0;
                d.say(b.x.round() as i32, top as i32, self.rng.pick(YOU_INSIDE), t, 2.6, 6);
            }
            if b.done_at.is_none() {
                b.kids.retain(|&id| c.position(id).is_some());
                let n = b.kids.iter().filter(|&&id| c.arrived(id)).count();
                if n > 0 {
                    let dur = if b.what == What::Snowman { 28.0 } else { 40.0 };
                    b.p += dt / dur * (0.6 + 0.2 * n as f32);
                    b.worked = t;
                    if self.rng.chance(dt * 1.2) {
                        d.snow_puff(b.x + self.rng.range(-b.half(), b.half()), b.base as f32);
                        d.sound(Sfx::Crunch, b.x, 0.25);
                    }
                    if t > b.next_line {
                        b.next_line = t + self.rng.range(5.0, 10.0);
                        if let Some((kx, ky)) = b.kids.get(self.rng.int(0, b.kids.len() as i32) as usize).and_then(|&id| c.position(id)) {
                            let line = *self.rng.pick(if b.what == What::Snowman { SNOWMAN_WORK } else { IGLOO_WORK });
                            d.say(kx.round() as i32, ky as i32 - 4, line, t, 2.4, 6);
                        }
                    }
                }
                let done = b.p >= 1.0;
                // everyone wandered off: it stays half-built
                if done || t - b.worked > 40.0 {
                    b.p = b.p.min(1.0);
                    b.done_at = Some(t);
                    if done {
                        self.made += 1;
                        d.burst(b.x, b.base as f32 - 3.0, 12, &[Rgb::WHITE, Rgb::hex(0xbfe0ff), Rgb::hex(0xffd36b)], 3.0, 2.0);
                        d.sound(Sfx::Snowman, b.x, 0.6);
                        if let Some((kx, ky)) = b.kids.first().and_then(|&id| c.position(id)) {
                            let line = if b.helped && self.rng.chance(0.5) { "we did it — together!" } else { *self.rng.pick(if b.what == What::Snowman { SNOWMAN_DONE } else { IGLOO_DONE }) };
                            d.say(kx.round() as i32, ky as i32 - 4, line, t + 0.3, 2.6, 6);
                        }
                    }
                    if done && b.what == What::Igloo {
                        // everyone in!
                        b.entering = Some(t + 15.0);
                        for &id in &b.kids {
                            c.work_at(id, b.door(), b.base as f32 + 1.0, b.door(), t + 15.0);
                        }
                    } else {
                        for &id in &b.kids {
                            c.stop_work(id, t);
                        }
                        b.kids.clear();
                    }
                }
            }
            // into the igloo, one by one as they reach the door
            if let Some(until) = b.entering {
                let mut k = 0;
                while k < b.kids.len() {
                    let id = b.kids[k];
                    if c.arrived(id) {
                        if let Some(hd) = c.take(id) {
                            b.inside.push(hd);
                            d.snow_puff(b.door(), b.base as f32);
                        }
                        b.kids.remove(k);
                    } else {
                        k += 1;
                    }
                }
                if t > until || b.kids.is_empty() {
                    for &id in &b.kids {
                        c.stop_work(id, t);
                    }
                    b.kids.clear();
                    b.entering = None;
                    b.out_at = t + self.rng.range(20.0, 50.0);
                    b.next_line = t + 3.0;
                }
            }
            if !b.inside.is_empty() && b.entering.is_none() {
                let top = b.base as f32 - IGLOO_R * b.u / 2.0 - 2.0;
                if t > b.next_line {
                    b.next_line = t + self.rng.range(4.0, 8.0);
                    d.say(b.x.round() as i32, top as i32, self.rng.pick(INSIDE), t, 2.4, 6);
                    d.sound(Sfx::Laugh, b.x, 0.35);
                }
                if t > b.out_at {
                    let n = b.inside.len();
                    let door = b.door();
                    for (i, hd) in b.inside.drain(..).enumerate() {
                        let x = door + (i as f32 - (n as f32 - 1.0) / 2.0) * 3.0;
                        c.put_back(hd, x, (b.base + 1).min(h as i32) as f32, t);
                    }
                    d.say(b.door().round() as i32, b.base - 3, self.rng.pick(OUT), t + 0.3, 2.6, 6);
                }
            }
            // it stands for about ten minutes, then melts
            if let Some(done) = b.done_at {
                if b.inside.is_empty() && b.entering.is_none() && !b.you_inside && t - done > 600.0 {
                    b.melt += dt / 25.0;
                }
            }
        }
        self.list.retain(|b| b.melt < 1.0);
    }

    /// Pixel layer (world space: draw with the camera offset).
    pub fn draw_px(&self, f: &mut Frame, sun: f32) {
        for b in &self.list {
            let fade = 1.0 - b.melt;
            let bp = b.base as f32 * 2.0 + 2.0;
            match b.what {
                What::Snowman => {
                    let k = b.u * SNOW_K * (1.0 - 0.5 * b.melt);
                    let (r0, r1, r2) = (5.5 * k, 4.0 * k, 3.0 * k);
                    let grow = |a: f32, z: f32| ((b.p - a) / (z - a)).clamp(0.0, 1.0);
                    let g0 = 0.3 + 0.7 * grow(0.0, 0.45);
                    let balls = [
                        (bp - r0 * g0, r0 * g0, true),
                        (bp - 2.0 * r0 - r1 + 1.0, r1 * grow(0.45, 0.75), b.p > 0.45),
                        (bp - 2.0 * r0 - 2.0 * r1 - r2 + 2.0, r2 * grow(0.75, 0.9), b.p > 0.75),
                    ];
                    for (cy, r, on) in balls {
                        if on && r > 0.3 {
                            ball(f, b.x, cy, r, fade, sun);
                        }
                    }
                    // a hat to finish
                    if b.p >= 0.97 && b.melt < 0.5 {
                        let (hy, hr) = (bp - 2.0 * r0 - 2.0 * r1 - r2 + 2.0, r2);
                        let brim = (hy - hr + 0.5) as i32;
                        for x in ((b.x - hr - 0.5) as i32)..=((b.x + hr) as i32) {
                            f.px_blend(x, brim, Rgb::hex(0x15151c), fade);
                        }
                        for y in (brim - (4.0 * k).round() as i32).max(brim - 6)..brim {
                            for x in ((b.x - hr * 0.6) as i32)..=((b.x + hr * 0.6) as i32) {
                                f.px_blend(x, y, if y == brim - 1 { Rgb::hex(0x2a6ad0) } else { Rgb::hex(0x1b1b24) }, fade);
                            }
                        }
                    }
                }
                What::Igloo => {
                    let r = IGLOO_R * b.u * (1.0 - 0.4 * b.melt);
                    let built = b.p * r;
                    let ice = Rgb::hex(0xeaf0fc).lerp(Rgb::hex(0xffffff), sun);
                    let mortar = Rgb::hex(0xbccbe6).lerp(Rgb::hex(0xd0dcf0), sun);
                    for y in ((bp - r) as i32)..(bp as i32) {
                        let up = bp - y as f32 - 0.5;
                        if up > built {
                            continue;
                        }
                        let half = (r * r - up * up).max(0.0).sqrt();
                        let course = (up / 3.0) as i32;
                        for x in ((b.x - half).round() as i32)..((b.x + half).round() as i32) {
                            let dx = (x as f32 + 0.5 - b.x) / r;
                            let joint = (up % 3.0) < 0.7 || (x + course * 3).rem_euclid(6) == 0;
                            let lit = 0.78 + 0.22 * dx - 0.1 * (up / r);
                            let c = if joint { mortar } else { ice * lit.clamp(0.55, 1.1) };
                            f.px_blend(x, y, c, fade);
                        }
                    }
                    // the doorway, once it's all but finished
                    if b.p > 0.85 && b.melt < 0.6 {
                        let warm = !b.inside.is_empty() || b.you_inside;
                        let dr = 3.0 * b.u.max(0.7);
                        for y in ((bp - dr) as i32)..(bp as i32) {
                            let up = bp - y as f32 - 0.5;
                            let hw = (dr * dr - up * up).max(0.0).sqrt() * 0.8;
                            for x in ((b.x - hw).round() as i32)..((b.x + hw).round() as i32) {
                                f.px_blend(x, y, if warm { Rgb::hex(0xffb050) } else { Rgb::hex(0x1c2434) }, fade);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Glyphs: the snowman's face and arms, a glow from an igloo with kids inside.
    pub fn draw_glyphs(&self, f: &mut Frame, t: f32, glow: f32) {
        for b in &self.list {
            let bp = b.base as f32 * 2.0 + 2.0;
            match b.what {
                What::Snowman if b.p >= 0.9 && b.melt < 0.6 => {
                    let k = b.u * SNOW_K * (1.0 - 0.5 * b.melt);
                    let (r0, r1, r2) = (5.5 * k, 4.0 * k, 3.0 * k);
                    let hy = bp - 2.0 * r0 - 2.0 * r1 - r2 + 2.0;
                    let my = bp - 2.0 * r0 - r1 + 1.0;
                    let (cx, hrow, mrow) = (b.x as i32, (hy / 2.0) as i32, (my / 2.0) as i32);
                    let coal = Rgb::hex(0x15151c);
                    f.put_g(cx, hrow, Glyph::new('ˆ', coal).lit(0.3));
                    f.put_g(cx + 1, hrow, Glyph::new('>', Rgb::hex(0xff7a1a)).lit(0.3));
                    f.put_g(cx, mrow, Glyph::new('•', coal).lit(0.3));
                    let wood = Rgb::hex(0x5a3a22);
                    let ar = r1.ceil() as i32;
                    f.put_g(cx - ar - 1, mrow, Glyph::new('╲', wood).lit(0.6));
                    f.put_g(cx + ar + 1, mrow, Glyph::new('╱', wood).lit(0.6));
                    // the villager helped: their red scarf round its neck
                    if b.helped {
                        let scarf = Rgb::hex(0xd8283a);
                        let nrow = ((hy + r2) / 2.0).round() as i32;
                        for dx in -1..=1 {
                            f.put_g(cx + dx, nrow, Glyph::new('▬', scarf).lit(0.5));
                        }
                        f.put_g(cx + 2, nrow + 1, Glyph::new('╲', scarf * 0.85).lit(0.5));
                    }
                }
                What::Igloo if !b.inside.is_empty() || b.you_inside || (b.helped && b.p >= 1.0 && b.melt < 0.6) => {
                    if !b.inside.is_empty() || b.you_inside {
                        let k = 0.75 + 0.25 * (t * 3.0 + b.x).sin();
                        f.light_cell(b.x as i32, b.base, Rgb::hex(0xffa040) * (2.2 * k * glow));
                    }
                    // the villager helped: a little flag on top
                    if b.helped && b.p >= 1.0 {
                        let top = ((bp - IGLOO_R * b.u * (1.0 - 0.4 * b.melt)) / 2.0).floor() as i32;
                        let wave = if (t * 2.0 + b.x).sin() > 0.0 { '⚑' } else { '⚐' };
                        f.put_g(b.x.round() as i32, top - 1, Glyph::new(wave, Rgb::hex(0xd8283a)).lit(0.5));
                    }
                }
                _ => {}
            }
        }
    }
}

/// A shaded snowball on the pixel layer.
fn ball(f: &mut Frame, cx: f32, cy: f32, r: f32, a: f32, sun: f32) {
    let base = Rgb::hex(0xeef2fc).lerp(Rgb::hex(0xffffff), sun);
    for y in ((cy - r - 1.0) as i32)..=((cy + r + 1.0) as i32) {
        for x in ((cx - r - 1.0) as i32)..=((cx + r + 1.0) as i32) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                let lit = 0.72 + 0.28 * (dx / r) - 0.12 * (dy / r);
                f.px_blend(x, y, base * lit.clamp(0.45, 1.1), cov * a);
            }
        }
    }
}
