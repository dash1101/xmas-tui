//! Folk going in and out of the church: every so often the bell rings for a
//! service and a few walk over and go in (to come out a while later), and
//! now and then someone just pops in on their own.

use super::critters::{Critters, Hidden};
use super::events::Director;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::rng::Rng;

const GOING: &[&str] = &["the bells! service is starting", "off to choir practice", "carols at the church tonight!", "can't be late for the service", "let's go and light a candle"];
const OUT: &[&str] = &["what lovely carols", "the choir was wonderful", "so many candles in there!", "a beautiful service", "I'll be humming that all night", "it's warm in there!"];
const POP_IN: &[&str] = &["just popping in to light a candle", "I left my scarf in the church", "a quiet moment inside"];

pub struct Chapel {
    rng: Rng,
    /// Heading to the door: id, and when they give up.
    going: Vec<(u64, f32)>,
    /// Inside, and when each comes back out.
    inside: Vec<(Hidden, f32)>,
    next_service: f32,
    next_visit: f32,
    /// People went in or came out lately (the door glows).
    pub door_glow: f32,
}

impl Chapel {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0xc4a9);
        let next_service = rng.range(60.0, 150.0);
        let next_visit = rng.range(30.0, 90.0);
        Chapel { rng, going: vec![], inside: vec![], next_service, next_visit, door_glow: 0.0 }
    }

    /// How many are in the church just now.
    pub fn inside(&self) -> usize {
        self.inside.len()
    }

    /// `door`: the church door's column and the row its step is on;
    /// `asleep`: Christmas Eve night (nobody's about).
    pub fn update(&mut self, dt: f32, t: f32, door: Option<(f32, f32)>, asleep: bool, c: &mut Critters, d: &mut Director) {
        let Some((dx, dy)) = door else { return };
        self.door_glow = (self.door_glow - dt).max(0.0);
        // a service: the bell, and a few who were nearby head over
        if t > self.next_service && !asleep {
            self.next_service = t + self.rng.range(150.0, 320.0);
            let mut near: Vec<(u64, f32, f32, bool)> = c.people().into_iter().filter(|p| (p.1 - dx).abs() < 140.0).collect();
            near.sort_by(|a, b| (a.1 - dx).abs().total_cmp(&(b.1 - dx).abs()));
            let n = (2 + self.rng.int(0, 3) as usize).min(near.len());
            if n > 0 {
                d.sound(Sfx::ChurchBell, dx, 0.8);
                for (k, p) in near.iter().take(n).enumerate() {
                    c.work_at(p.0, dx + (k as f32 - 1.0) * 1.5, dy, dx, t + 50.0);
                    self.going.push((p.0, t + 50.0));
                }
                let (id, x, y, _) = near[0];
                let _ = id;
                d.say(x.round() as i32, y as i32 - 4, self.rng.pick(GOING), t, 2.6, 6);
            }
        }
        // or just someone popping in
        if t > self.next_visit && !asleep {
            self.next_visit = t + self.rng.range(60.0, 160.0);
            if let Some(p) = c.people().into_iter().filter(|p| (p.1 - dx).abs() < 60.0).min_by(|a, b| (a.1 - dx).abs().total_cmp(&(b.1 - dx).abs())) {
                c.work_at(p.0, dx, dy, dx, t + 30.0);
                self.going.push((p.0, t + 30.0));
                if self.rng.chance(0.5) {
                    d.say(p.1.round() as i32, p.2 as i32 - 4, self.rng.pick(POP_IN), t, 2.4, 6);
                }
            }
        }
        // through the door as they reach it
        let mut k = 0;
        while k < self.going.len() {
            let (id, until) = self.going[k];
            if c.arrived(id) {
                if let Some(hd) = c.take(id) {
                    let stay = self.rng.range(30.0, 90.0);
                    self.inside.push((hd, t + stay));
                    self.door_glow = 1.2;
                    d.emote(dx, dy - 4.0, '✦', Rgb::hex(0xffd88a), 1);
                }
                self.going.remove(k);
            } else if t > until || c.position(id).is_none() {
                c.stop_work(id, t);
                self.going.remove(k);
            } else {
                k += 1;
            }
        }
        // and back out again, one at a time
        if let Some(i) = self.inside.iter().position(|(_, out)| t > *out) {
            let (hd, _) = self.inside.remove(i);
            let side = if self.rng.chance(0.5) { -1.0 } else { 1.0 };
            c.put_back(hd, dx + side * 1.5, dy, t);
            self.door_glow = 1.2;
            if self.rng.chance(0.6) {
                d.say(dx.round() as i32, dy as i32 - 4, self.rng.pick(OUT), t + 0.3, 2.6, 6);
            }
            // the next one out a moment later
            if let Some(n) = self.inside.first_mut() {
                n.1 = n.1.max(t + 1.5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::events::{Ctx, Kind};

    #[test]
    fn folk_go_into_the_church_and_come_back_out() {
        let (w, h) = (200, 40);
        let ctx = Ctx { w, h, ground_row: vec![24; w as usize], track_row: 20, occluded: vec![false; (w * h) as usize], church: None, sky_rows: 10, station: None, cam: 0.0, vw: w };
        let mut d = Director::new(3, 0.0);
        let mut l = crate::scene::lines::Lines::new(3);
        let mut c = Critters::new(&ctx, 1, 0.0, false, &[]);
        for x in [80.0, 90.0, 110.0] {
            c.spawn_demo(Kind::Walker, x, 0.0);
        }
        let mut ch = Chapel::new(1);
        ch.next_service = 0.5;
        ch.next_visit = 1e9;
        let door = Some((100.0, 30.0));
        let (mut t, dt) = (0.0, 1.0 / 30.0);
        let mut most_inside = 0;
        while t < 200.0 {
            c.update(dt, t, &ctx, &mut d, &mut l, None, None);
            ch.update(dt, t, door, false, &mut c, &mut d);
            most_inside = most_inside.max(ch.inside());
            t += dt;
        }
        assert!(most_inside >= 2, "nobody went in ({most_inside})");
        assert_eq!(ch.inside(), 0, "still inside after the service");
        assert!(c.positions().len() <= 3);
    }
}
