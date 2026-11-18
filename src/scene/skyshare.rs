//! What's in the sky of a shared village, the same for everyone there: the
//! host says when the sleigh, a balloon, a comet, the aurora… come (a
//! guest's own schedules wait on its word); whoever knocks Santa out of his
//! sleigh says where he runs, and anyone there can catch him; someone up in
//! a balloon is seen in it.

use super::*;
use crate::net::sky;

impl Scene {
    /// A hot-air balloon drifting in to rest at `x` (from the right, or the
    /// left), with a word from the villager.
    pub(super) fn balloon_comes(&mut self, x: f32, right: bool, seed: u32, t: f32) {
        let ground = (self.lo_at(x) + 3.0).min(self.h as f32 - 2.0);
        let from = if right { self.cam + self.w as f32 + 40.0 } else { self.cam - 40.0 };
        self.balloon = Some(super::balloon::Balloon::coming(from, -12.0, x, ground, 0.0, seed));
        if let Some(p) = &self.player {
            self.director.say(p.centre().round() as i32, p.y as i32 - 4, "look — a hot-air balloon!", t, 2.6, 8);
        }
    }

    /// Each frame: a guest's schedules held for the host's word; what's come
    /// up in the sky here, told the others.
    pub(super) fn share_sky(&mut self) {
        let shared = self.online.village().is_some();
        let guest = shared && !self.online.hosting();
        self.director.guest = guest;
        self.sleigh.follow = guest;
        let skies = std::mem::take(&mut self.director.shared);
        let went = std::mem::take(&mut self.sleigh.went);
        if !shared {
            return;
        }
        if !guest {
            for (what, x, y) in skies {
                self.online.sky(what, x, y, 0);
            }
        }
        // (the sleigh: whoever's it started for, the host's schedule or a
        // click on the moon)
        if went {
            self.online.sky(sky::SLEIGH, 0.0, 0.0, 0);
        }
        // someone up in a balloon who's hopped out: it drifts away up
        let now: Vec<(u32, u32, f32, f32)> = self.online.others().iter().filter(|(_, o)| o.av.ride != 0 && o.av.away == 0).map(|(id, o)| (*id, o.av.ride, o.shown.0, o.shown.1)).collect();
        let was = std::mem::take(&mut self.social.riders);
        for (id, seed, x, y) in &was {
            if !now.iter().any(|n| n.0 == *id && n.1 == *seed) && self.balloon.is_none() && self.online.other(*id).is_some() {
                let (lx, ly) = self.local(*x, *y);
                self.balloon = Some(super::balloon::Balloon::leaving(lx + 1.0, ly + 0.8, *seed));
            }
        }
        self.social.riders = now;
    }

    /// Something in the sky from someone in the village (see `net::sky`).
    pub(super) fn on_sky(&mut self, id: u32, what: u8, x: f32, y: f32, seed: u32, t: f32) {
        match what {
            sky::SLEIGH => {
                self.sleigh.heard = true;
                self.sleigh.trigger(t);
            }
            sky::HALT => self.sleigh.halt(t + y.clamp(0.0, 15.0)),
            sky::BALLOON => {
                // (one here already, or busy elsewhere: this one's missed)
                if self.balloon.is_none() && self.inside.is_none() && self.trip.is_none() {
                    let lx = self.local(x, 0.0).0.clamp(8.0, self.ww as f32 - 8.0);
                    self.balloon_comes(lx, y > 0.5, seed, t);
                }
            }
            sky::SANTA => {
                if self.runaway.is_none() {
                    let (lx, ly) = self.local(x, y);
                    let land = (self.lo_at(lx.clamp(0.0, self.ww as f32 - 1.0)) + 3.0).min(self.h as f32 - 2.0);
                    self.runaway = Some(play::Runaway::theirs(lx, ly.min(land - 1.0), land, t));
                    self.sleigh.santa_out = true;
                    self.director.say(lx.round() as i32, ly as i32 - 2, "whoa — WHOA!", t, 1.8, 8);
                    self.director.sound(Sfx::Magic, lx, 0.8);
                    let name = self.other_name(id);
                    self.post(format!("★ {name} knocked Santa out of his sleigh — catch him!"));
                }
            }
            sky::SANTA_AT => {
                let lx = self.local(x, 0.0).0;
                if let Some(r) = self.runaway.as_mut().filter(|r| r.theirs) {
                    r.want = Some(lx);
                    r.dir = if y < 0.0 { -1.0 } else { 1.0 };
                    r.heard = t;
                    if seed == 1 {
                        r.iced = t + 0.5;
                    }
                }
            }
            sky::CAUGHT => {
                if let Some(r) = self.runaway.as_mut().filter(|r| !r.caught_yet()) {
                    let (rx, ry) = (r.x, r.y);
                    r.catch_now(t);
                    self.santa_caught(rx, ry, t, false);
                    let name = self.other_name(seed);
                    self.post(format!("★ {name} caught Santa!"));
                }
            }
            sky::SANTA_GONE => {
                if let Some(r) = self.runaway.as_mut().filter(|r| r.theirs) {
                    r.gone = true;
                }
            }
            _ => {
                self.director.sky(what, x, y, t, &self.ctx);
            }
        }
    }

    /// Whoever's up on the moon just now, seen from the village: tiny
    /// figures on its rim, where they are round it (the side facing the
    /// village only).
    pub(super) fn draw_moon_visitors(&self, f: &mut Frame) {
        let (mx, my, r) = self.sky.moon;
        let mx = mx - self.sky_off() as f32;
        for (id, o) in self.online.others() {
            let Some(at) = o.av.trip.filter(|a| o.av.away == 2 && a.at == 8) else { continue };
            let a = at.x / super::trips::MOON_W * std::f32::consts::TAU;
            if a.cos() < 0.0 {
                continue;
            }
            let s = a.sin() * 0.85;
            let (px, py) = (mx + r * s, my - r * (1.0 - s * s).sqrt() - at.air * 0.3);
            let look = super::online::look_of(&o.av, id);
            let (x, y) = (px.round() as i32, py.round() as i32);
            f.px_set(x, y - 3, look.hat.unwrap_or(look.hair));
            f.px_set(x, y - 2, look.skin);
            f.px_set(x, y - 1, look.coat);
            f.px_set(x, y, look.coat * 0.7);
        }
    }

    /// The others up in balloons, drawn round them (behind them: the
    /// envelope; the basket in front, see `draw_ridden_front`).
    pub(super) fn draw_ridden(&self, f: &mut Frame, t: f32, front: bool) {
        if self.inside.is_some() || self.trip.is_some() {
            return;
        }
        let k = sprite_k(self.h);
        for (_, o) in self.online.others() {
            if o.av.ride == 0 || o.av.away != 0 {
                continue;
            }
            let (x, y) = self.local(o.shown.0, o.shown.1);
            let mut b = super::balloon::Balloon::ridden(x + 1.0, y + 0.8, o.av.ride);
            // (going up: the burner lit)
            if o.av.air {
                b.light(t + 0.1);
            }
            if front {
                b.draw_front(f, k);
            } else {
                b.draw_back(f, t, k);
            }
        }
    }
}
