//! Autopilot (Settings → You): left alone a while, the villager gets on with
//! a day of their own — a hand with the kids' snowmen and igloos (or one of
//! their own), snowballs at the neighbours, berries for the animals, a chat,
//! a game with the pet, tag with the kids, a look round from a rooftop.
//! Anything done by hand takes over again at once.

use super::ledges::Kind as LKind;
use super::story::PetAct;
use super::{pets, Do, Scene};

/// How long the villager is left alone before getting on with things (s).
pub const IDLE: f32 = 20.0;

#[derive(Default)]
pub(super) struct Auto {
    pub on: bool,
    /// When the villager was last steered by hand (scene time).
    pub touched: f32,
    pub plan: Option<Plan>,
    /// Nothing new is started before this.
    pub next_at: f32,
    /// The goal being walked to is autopilot's own.
    pub goal_mine: bool,
    /// How many times each kind of thing was taken up (tests).
    #[cfg(test)]
    pub tried: [u32; 13],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Plan {
    /// Over there to do something (the goal does it on arrival); then E
    /// there, for berries off a bush.
    Errand { then_e: bool, until: f32 },
    /// A snowman or igloo at column `bx`: a hand with it every so often,
    /// till it's done.
    Build { bx: f32, next: f32, until: f32 },
    /// Snowballs at someone: how many more, and when the next.
    Snowballs { id: u64, left: u8, next: f32, until: f32 },
    /// A chat with someone: an answer every so often.
    Chat { id: u64, next: f32, until: f32 },
    /// A game with the pet, once alongside.
    Pet { act: PetAct, until: f32 },
    /// Tag with a kid (the game itself is the scene's `tag`): `swaps` seen
    /// so far; running off, out of puff after `tire_at`; dodging back past
    /// them (`dodge`, until `commit`) when cornered at the end of the world.
    Tag { swaps: u32, tire_at: f32, dodge: f32, commit: f32 },
    /// A roof: walking under it (0), leapt (1, landing by `at`), up there
    /// till `at` (2).
    Roof { stage: u8, at: f32 },
    /// Standing about (curled up in an igloo, maybe).
    Linger { until: f32 },
}

impl Auto {
    pub fn rebase(&mut self, s: f32) {
        self.touched -= s;
        self.next_at -= s;
        self.plan = match self.plan {
            Some(Plan::Errand { then_e, until }) => Some(Plan::Errand { then_e, until: until - s }),
            Some(Plan::Build { bx, next, until }) => Some(Plan::Build { bx, next: next - s, until: until - s }),
            Some(Plan::Snowballs { id, left, next, until }) => Some(Plan::Snowballs { id, left, next: next - s, until: until - s }),
            Some(Plan::Chat { id, next, until }) => Some(Plan::Chat { id, next: if next == 0.0 { 0.0 } else { next - s }, until: until - s }),
            Some(Plan::Pet { act, until }) => Some(Plan::Pet { act, until: until - s }),
            Some(Plan::Tag { swaps, tire_at, dodge, commit }) => Some(Plan::Tag { swaps, tire_at: tire_at - s, dodge, commit: commit - s }),
            Some(Plan::Roof { stage, at }) => Some(Plan::Roof { stage, at: at - s }),
            Some(Plan::Linger { until }) => Some(Plan::Linger { until: until - s }),
            None => None,
        };
    }
}

impl Scene {
    /// Autopilot on or off (Settings → You).
    pub fn set_autopilot(&mut self, on: bool) {
        if self.auto.on != on {
            self.auto.on = on;
            let t = self.now();
            self.auto.touched = t;
            if !on {
                self.auto_stop(t);
            }
        }
    }

    /// Something done by hand: the villager is steered by hand again (and
    /// autopilot waits a while before taking over).
    pub fn woke(&mut self) {
        let t = self.now();
        self.auto.touched = t;
        if self.auto.plan.is_some() || self.auto.goal_mine {
            self.auto_stop(t);
        }
    }

    /// Autopilot is steering the villager right now.
    #[cfg(test)]
    pub fn on_autopilot(&self) -> bool {
        self.auto.on && self.player.is_some() && self.now() - self.auto.touched >= IDLE
    }

    fn auto_stop(&mut self, t: f32) {
        if let (Some(Plan::Tag { .. }), Some(g)) = (self.auto.plan, self.tag.take()) {
            self.critters.tag_over(g.id, t, &mut self.director);
        }
        if self.auto.goal_mine {
            self.goal = None;
            self.auto.goal_mine = false;
        }
        self.auto.plan = None;
        self.auto.next_at = t + 2.0;
        if let Some(p) = &mut self.player {
            p.sprint = false;
        }
    }

    /// Done with that: something else in a little while.
    fn auto_done(&mut self, t: f32, a: f32, b: f32) {
        self.auto.plan = None;
        self.auto.next_at = t + self.rng.range(a, b);
        if let Some(p) = &mut self.player {
            p.sprint = false;
        }
    }

    fn auto_goal(&mut self, d: Do) {
        self.act(d);
        self.auto.goal_mine = true;
    }

    fn auto_say(&mut self, line: &str, t: f32) {
        if let Some(p) = &self.player {
            self.director.say(p.centre().round() as i32, p.y as i32 - 4, line, t, 2.4, 6);
        }
    }

    /// A frame of autopilot (before the villager moves): the direction keys
    /// it holds, if any (walking to a goal steers itself).
    pub(super) fn autopilot(&mut self, t: f32) -> Option<(f32, f32)> {
        if !self.auto.on || self.player.is_none() || t - self.auto.touched < IDLE {
            return None;
        }
        // inside an igloo for a rest: back out when it's time
        if self.inside.is_some() {
            if self.inside.as_ref().is_some_and(|i| i.is_igloo()) {
                match self.auto.plan {
                    Some(Plan::Linger { until }) if t < until => {}
                    Some(Plan::Linger { .. }) => {
                        self.leave();
                        self.auto_done(t, 2.0, 5.0);
                    }
                    _ => self.auto.plan = Some(Plan::Linger { until: t + self.rng.range(10.0, 18.0) }),
                }
            }
            return None;
        }
        if self.goal.is_none() {
            self.auto.goal_mine = false;
        }
        let Some(plan) = self.auto.plan else {
            if t >= self.auto.next_at {
                self.auto_pick(t);
            }
            return None;
        };
        let walking = self.goal.is_some();
        let (px, py) = self.player.as_ref().map(|p| (p.centre(), p.y))?;
        match plan {
            Plan::Errand { then_e, until } => {
                if walking && t < until {
                    return None;
                }
                if then_e && !walking {
                    self.interact();
                }
                self.auto_done(t, 2.0, 5.0);
            }
            Plan::Build { bx, next, until } => {
                let i = self.builds.find(bx);
                let about = i.and_then(|i| self.builds.about(i));
                if walking && t < until {
                    return None;
                }
                if t > until || about.is_none() || about.is_some_and(|(_, p)| p >= 1.0) {
                    // a finished igloo: in for a rest, now and then
                    if about.is_some_and(|(snowman, p)| !snowman && p >= 1.0) && self.rng.chance(0.5) {
                        self.auto_goal(Do::Crawl(bx));
                        self.auto.plan = Some(Plan::Linger { until: t + self.rng.range(8.0, 14.0) });
                    } else {
                        self.auto_done(t, 3.0, 8.0);
                    }
                    return None;
                }
                if t >= next {
                    self.auto_goal(Do::HelpBuild(bx));
                    self.auto.plan = Some(Plan::Build { bx, next: t + self.rng.range(0.4, 0.9), until });
                }
            }
            Plan::Snowballs { id, left, next, until } => {
                if walking && t < until {
                    return None;
                }
                if t > until || left == 0 || self.critters.position(id).is_none() {
                    self.auto_done(t, 3.0, 7.0);
                } else if t >= next {
                    self.auto_goal(Do::Snowball(id));
                    self.auto.plan = Some(Plan::Snowballs { id, left: left - 1, next: t + self.rng.range(1.2, 2.5), until });
                }
            }
            Plan::Chat { id, next, until } => {
                if walking && t < until {
                    return None;
                }
                let open = self.convo.as_ref().filter(|c| c.id == id && c.ended_at.is_none()).map(|c| c.opts.len());
                match open {
                    Some(n) if n > 0 && t < until => {
                        if next == 0.0 {
                            self.auto.plan = Some(Plan::Chat { id, next: t + self.rng.range(2.5, 4.0), until });
                        } else if t >= next {
                            let k = (self.rng.range(0.0, n as f32) as usize).min(n - 1) + 1;
                            self.answer(k);
                            self.auto.plan = Some(Plan::Chat { id, next: t + self.rng.range(2.8, 4.5), until });
                        }
                    }
                    Some(_) if t < until => {}
                    _ => self.auto_done(t, 3.0, 8.0),
                }
            }
            Plan::Pet { act, until } => {
                if walking && t < until {
                    return None;
                }
                if !walking && self.my_pet.is_some() {
                    self.pet_act(act);
                }
                self.auto_done(t, 4.0, 9.0);
            }
            Plan::Tag { swaps, tire_at, dodge, commit } => return self.auto_tag(t, (px, py), swaps, tire_at, dodge, commit),
            Plan::Roof { stage, at } => {
                let perched = self.player.as_ref().is_some_and(|p| p.perch.is_some());
                match stage {
                    _ if walking => {}
                    0 => {
                        self.jump();
                        self.auto.plan = Some(Plan::Roof { stage: 1, at: t + 3.0 });
                    }
                    1 if perched => {
                        let line = self.rng_line(&["what a view!", "I can see the whole village!", "hello up here!", "the lights look lovely from here"]);
                        self.auto_say(line, t);
                        self.auto.plan = Some(Plan::Roof { stage: 2, at: t + self.rng.range(6.0, 12.0) });
                    }
                    1 if t > at => self.auto_done(t, 2.0, 4.0),
                    2 if t > at || !perched => {
                        self.auto_done(t, 2.0, 5.0);
                        // S: hop back down
                        return perched.then_some((0.0, 1.0));
                    }
                    _ => {}
                }
            }
            Plan::Linger { until } => {
                if t >= until {
                    if self.in_igloo {
                        self.leave_igloo();
                    }
                    // up out of the snow
                    if self.player.as_ref().is_some_and(|p| p.lying()) {
                        self.rest();
                    }
                    self.auto_done(t, 2.0, 5.0);
                }
            }
        }
        None
    }

    fn rng_line(&mut self, lines: &[&'static str]) -> &'static str {
        self.rng.pick(lines)
    }

    /// Tag: after the kid (who runs off), or away from them (the game
    /// itself, tags and the end of it, is the scene's). Cornered at either
    /// end of the world, a dodge back past them the open way, kept to for a
    /// moment rather than flipped about.
    fn auto_tag(&mut self, t: f32, (px, py): (f32, f32), swaps: u32, tire_at: f32, dodge: f32, commit: f32) -> Option<(f32, f32)> {
        let Some(g) = self.tag else {
            self.auto_done(t, 3.0, 6.0);
            return None;
        };
        let Some((kx, ky)) = self.critters.position(g.id) else {
            self.auto_done(t, 3.0, 6.0);
            return None;
        };
        let tire_at = if g.swaps != swaps && !g.you_it { t + self.rng.range(4.0, 7.0) } else { tire_at };
        let (dx, dy) = (kx - px, ky - py);
        if let Some(p) = &mut self.player {
            p.sprint = true;
        }
        if g.you_it {
            self.auto.plan = Some(Plan::Tag { swaps: g.swaps, tire_at, dodge: 0.0, commit: 0.0 });
            let my = if dy.abs() > 0.6 { dy.signum() } else { 0.0 };
            return Some((if dx.abs() > 1.0 { dx.signum() } else { 0.0 }, my));
        }
        // running off (out of puff after a while: caught soon enough)
        if t > tire_at {
            if let Some(p) = &mut self.player {
                p.sprint = false;
            }
            self.auto.plan = Some(Plan::Tag { swaps: g.swaps, tire_at, dodge, commit });
            return Some((0.0, 0.0));
        }
        let away = -dx.signum();
        let room = if away < 0.0 { px } else { self.ww as f32 - px };
        let (mx, dodge, commit) = if t < commit {
            (dodge, dodge, commit)
        } else if room < 26.0 {
            (-away, -away, t + 1.6)
        } else {
            (away, 0.0, 0.0)
        };
        self.auto.plan = Some(Plan::Tag { swaps: g.swaps, tire_at, dodge, commit });
        let my = if dy.abs() < 2.0 { -dy.signum() } else { 0.0 };
        Some((mx, my))
    }

    /// Something to do next, from what's about.
    fn auto_pick(&mut self, t: f32) {
        let Some((px, py, berries)) = self.player.as_ref().map(|p| (p.centre(), p.y, p.berries)) else { return };
        let near = |x: f32| (x - px).abs() < 60.0;
        let people: Vec<(u64, f32, f32, bool)> = self.critters.people().into_iter().filter(|p| near(p.1)).collect();
        let kids: Vec<u64> = people.iter().filter(|p| p.3).map(|p| p.0).collect();
        let animals: Vec<(u64, f32, f32)> = self.critters.animals().into_iter().filter(|a| near(a.1)).collect();
        let bushes: Vec<(f32, f32)> = self.critters.ripe_bushes(t).into_iter().filter(|b| near(b.0)).collect();
        let mut going = None;
        let mut i = 0;
        while let Some((bx, ..)) = self.builds.spot(i) {
            if self.builds.about(i).is_some_and(|(_, p)| p < 1.0) && near(bx) {
                going = Some(bx);
            }
            i += 1;
        }
        let roofs: Vec<f32> = self.ledges.iter().filter(|l| l.kind == LKind::House && l.id == 0 && near(l.x0 as f32)).map(|l| l.x0 as f32 + l.tops.len() as f32 / 2.0).collect();
        let pet = self.my_pet.as_ref().filter(|p| !p.staying).map(|p| (p.x, p.y, p.kind));
        // what's possible, and how likely
        let mut opts: Vec<(f32, u8)> = vec![(1.0, 0)];
        if going.is_some() {
            opts.push((4.0, 1));
        }
        opts.push((1.2, 2));
        if !people.is_empty() {
            opts.push((2.0, 3));
            opts.push((1.2, 5));
        }
        if !animals.is_empty() && (berries > 0 || !bushes.is_empty()) {
            opts.push((2.5, 4));
        } else if !bushes.is_empty() {
            opts.push((1.0, 4));
        }
        if pet.is_some() {
            opts.push((1.5, 6));
        }
        if !kids.is_empty() {
            opts.push((2.0, 7));
        }
        if !roofs.is_empty() {
            opts.push((1.0, 8));
        }
        let night = self.daylight.sun < 0.25;
        let on_snow = self.player.as_ref().is_some_and(|p| p.perch.is_none());
        if on_snow {
            opts.push((1.2, 9));
            opts.push((if night { 1.2 } else { 0.4 }, 10));
        }
        if !people.is_empty() && self.carol.is_none() {
            opts.push((0.8, 11));
        }
        let mut igloo = None;
        let mut k = 0;
        while let Some((bx, _, _, ig)) = self.builds.spot(k) {
            if ig && near(bx) && self.builds.about(k).is_some_and(|(_, p)| p >= 1.0) {
                igloo = Some(bx);
            }
            k += 1;
        }
        if igloo.is_some() {
            opts.push((1.0, 12));
        }
        let sum: f32 = opts.iter().map(|o| o.0).sum();
        let mut r = self.rng.range(0.0, sum);
        let pick = opts.iter().find(|o| {
            r -= o.0;
            r < 0.0
        });
        let lo = |s: &Self, x: f32| s.lo_at(x);
        let which = pick.map(|o| o.1).unwrap_or(0);
        #[cfg(test)]
        {
            self.auto.tried[which as usize] += 1;
        }
        match which {
            // a hand with a build going up
            1 => {
                let bx = going.unwrap_or(px);
                let line = self.rng_line(&["I'll help!", "need a hand?", "ooh, let me help!"]);
                self.auto_say(line, t);
                self.auto_goal(Do::HelpBuild(bx));
                self.auto.plan = Some(Plan::Build { bx, next: t + 1.0, until: t + 60.0 });
            }
            // a snowman or igloo of their own, somewhere near with room
            2 => {
                let igloo = self.rng.chance(0.35);
                let half = self.builds.half_for(igloo);
                let mut offs = [14.0f32, -14.0, 22.0, -22.0, 30.0, -30.0];
                if self.rng.chance(0.5) {
                    offs.iter_mut().for_each(|o| *o = -*o);
                }
                let spot = offs.iter().map(|o| px + o).find_map(|x| {
                    let base = (lo(self, x) + 3.0).min(self.h as f32 - 1.0);
                    self.build_spot_free(x, base as i32, half).then_some((x, base))
                });
                match spot {
                    Some((x, base)) => {
                        self.auto_goal(Do::Build(igloo, x, base));
                        self.auto.plan = Some(Plan::Build { bx: x, next: t + 1.5, until: t + 90.0 });
                    }
                    None => self.auto.next_at = t + 1.0,
                }
            }
            // snowballs at someone (a kid, mostly)
            3 => {
                let id = if !kids.is_empty() && self.rng.chance(0.65) { *self.rng.pick(&kids) } else { self.rng.pick(&people).0 };
                if self.rng.chance(0.5) {
                    let line = self.rng_line(&["snowball fight!", "heads up!", "incoming!"]);
                    self.auto_say(line, t);
                }
                let left = self.rng.range(1.0, 3.99) as u8;
                self.auto_goal(Do::Snowball(id));
                self.auto.plan = Some(Plan::Snowballs { id, left, next: t + 2.0, until: t + 30.0 });
            }
            // the animals: berries first (off a bush, if none in the
            // pocket), then one for an animal
            4 => {
                if berries == 0 && !bushes.is_empty() {
                    let (bx, base) = *self.rng.pick(&bushes);
                    self.auto_goal(Do::WalkTo(bx, base.max(lo(self, bx))));
                    self.auto.plan = Some(Plan::Errand { then_e: true, until: t + 25.0 });
                } else if let Some(&(id, ..)) = (!animals.is_empty()).then(|| self.rng.pick(&animals)) {
                    self.auto_goal(Do::Pet(id));
                    self.auto.plan = Some(Plan::Errand { then_e: false, until: t + 25.0 });
                }
            }
            // a chat
            5 => {
                let id = self.rng.pick(&people).0;
                self.auto_goal(Do::Talk(id));
                self.auto.plan = Some(Plan::Chat { id, next: 0.0, until: t + 60.0 });
            }
            // a game with the pet
            6 => {
                let Some((x, y, kind)) = pet else { return };
                let act = *self.rng.pick(match kind {
                    pets::PetKind::Dog => &[PetAct::Throw(1), PetAct::Throw(2), PetAct::Trick, PetAct::Zoomies, PetAct::Cuddle, PetAct::Brush][..],
                    pets::PetKind::Cat => &[PetAct::Throw(0), PetAct::Tease, PetAct::Trick, PetAct::Zoomies, PetAct::Cuddle, PetAct::Brush][..],
                    pets::PetKind::Bunny => &[PetAct::Throw(3), PetAct::Trick, PetAct::Cuddle, PetAct::Brush][..],
                });
                if (x - px).abs() > 6.0 || (y - py).abs() > 3.0 {
                    self.auto_goal(Do::WalkTo(x, y.max(lo(self, x))));
                }
                self.auto.plan = Some(Plan::Pet { act, until: t + 20.0 });
            }
            // tag with a kid
            7 => {
                let id = *self.rng.pick(&kids);
                let secs = self.rng.range(25.0, 40.0);
                self.start_tag(id, secs);
                self.auto.plan = Some(Plan::Tag { swaps: 0, tire_at: 0.0, dodge: 0.0, commit: 0.0 });
            }
            // up on a roof for a look round
            8 => {
                let x = *self.rng.pick(&roofs);
                self.auto_goal(Do::WalkTo(x, lo(self, x)));
                self.auto.plan = Some(Plan::Roof { stage: 0, at: t + 30.0 });
            }
            // a snow angel, right here
            9 => {
                self.snow_angel();
                self.auto.plan = Some(Plan::Linger { until: t + self.rng.range(4.0, 6.0) });
            }
            // a rest in the snow (looking up at the stars, after dark)
            10 => {
                self.rest();
                self.auto.plan = Some(Plan::Linger { until: t + self.rng.range(10.0, 22.0) });
            }
            // a carol (the neighbours gather round)
            11 => {
                self.sing();
                self.auto.plan = Some(Plan::Linger { until: t + 13.0 });
            }
            // into an igloo for a sit by the lamp
            12 => {
                if let Some(bx) = igloo {
                    self.auto_goal(Do::Crawl(bx));
                    self.auto.plan = Some(Plan::Linger { until: t + self.rng.range(14.0, 24.0) });
                }
            }
            // a wander
            _ => {
                let x = (px + self.rng.range(12.0, 35.0) * if self.rng.chance(0.5) { 1.0 } else { -1.0 }).clamp(4.0, self.ww as f32 - 6.0);
                let lo = lo(self, x);
                let y = self.rng.range(lo, (lo + 6.0).min(self.h as f32 - 2.0)).round();
                self.auto_goal(Do::WalkTo(x, y));
                self.auto.plan = Some(Plan::Errand { then_e: false, until: t + 25.0 });
            }
        }
    }
}
