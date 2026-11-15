//! New Year's Eve: a ball-drop mast goes up beside the big tree, the
//! village gathers round, the ball slides down over the last minute, a big
//! countdown fills the sky for the last ten seconds, and at midnight it's
//! fireworks, confetti, party blowers and "HAPPY NEW YEAR" all round.

use super::events::Director;
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::Rng;

/// Seconds until midnight (negative after it), on New Year's Eve / Day.
pub fn to_midnight(now: &chrono::DateTime<chrono::Local>) -> Option<f32> {
    use chrono::{Datelike, Timelike};
    let s = (now.hour() * 3600 + now.minute() * 60 + now.second()) as f32 + now.nanosecond() as f32 * 1e-9;
    match (now.month(), now.day()) {
        (12, 31) => Some(86400.0 - s),
        (1, 1) => Some(-s),
        _ => None,
    }
}

struct Blower {
    x: f32,
    y: f32,
    dir: f32,
    age: f32,
    col: Rgb,
}

pub struct NewYear {
    /// The mast: column, top row, foot row.
    mast: (i32, i32, i32),
    blowers: Vec<Blower>,
    next_blow: f32,
    /// Last countdown second announced; midnight celebrated.
    last_sec: i32,
    pub celebrated: bool,
    gathered: bool,
    /// When midnight happened (scene time), for the banner.
    midnight_at: Option<f32>,
    rng: Rng,
}

const PARTY: [Rgb; 6] = [Rgb::hex(0xff4d6d), Rgb::hex(0xffd166), Rgb::hex(0x4ad0ff), Rgb::hex(0x7aef7a), Rgb::hex(0xc77dff), Rgb::hex(0xff9f40)];

impl NewYear {
    pub fn new(mast: (i32, i32, i32), seed: u32) -> Self {
        NewYear { mast, blowers: vec![], next_blow: 0.0, last_sec: 99, celebrated: false, gathered: false, midnight_at: None, rng: Rng::new(seed as u64 ^ 0x2027) }
    }

    pub fn rebase(&mut self, s: f32) {
        self.next_blow -= s;
        if let Some(m) = &mut self.midnight_at {
            *m -= s;
        }
    }

    /// `left`: seconds to midnight; `people`: (centre, row, kid) of the
    /// villagers about (for blowers and cheering); `view`: (left, width).
    /// Returns true the moment the village should gather round.
    pub fn update(&mut self, dt: f32, t: f32, left: f32, people: &[(f32, f32, bool)], view: (f32, f32), d: &mut Director) -> bool {
        for b in &mut self.blowers {
            b.age += dt;
        }
        self.blowers.retain(|b| b.age < 0.9);
        let seen: Vec<_> = people.iter().copied().filter(|p| p.0 > view.0 && p.0 < view.0 + view.1).collect();
        // party blowers now and then all evening, lots of them at midnight
        let busy = (-120.0..0.0).contains(&left) || (0.0..15.0).contains(&left);
        if t > self.next_blow && !seen.is_empty() {
            self.next_blow = t + if busy { self.rng.range(0.15, 0.5) } else { self.rng.range(1.5, 5.0) };
            let p = *self.rng.pick(&seen);
            self.blow(p);
        }
        // everyone gathers round ten minutes before
        let mut gather = false;
        if !self.gathered && (0.0..600.0).contains(&left) {
            self.gathered = true;
            gather = true;
        }
        // chanting the last ten
        let sec = left.ceil() as i32;
        if (1..=10).contains(&sec) && sec != self.last_sec {
            self.last_sec = sec;
            let n = if sec <= 3 { 3 } else { 1 };
            for _ in 0..n.min(seen.len()) {
                let p = *self.rng.pick(&seen);
                d.say(p.0.round() as i32, p.1 as i32 - 4, &format!("{sec}!"), t, 0.9, 3);
            }
            d.sound(Sfx::Chime, view.0 + view.1 / 2.0, if sec <= 3 { 0.8 } else { 0.45 });
        }
        // midnight!
        if !self.celebrated && left <= 0.0 && left > -1800.0 {
            self.celebrated = true;
            self.midnight_at = Some(t);
            d.sound(Sfx::Fanfare, view.0 + view.1 / 2.0, 1.0);
            d.sound(Sfx::ChurchPeal, view.0 + view.1 / 2.0, 0.9);
            for k in 0..14 {
                let x = view.0 + view.1 * (k as f32 + 0.5) / 14.0;
                d.burst(x, self.rng.range(4.0, 16.0), 16, &PARTY, 5.0, 3.0);
            }
            d.happenings.push((super::dialogue::Ev::Fireworks, None));
            for (i, p) in seen.iter().enumerate() {
                let line = *self.rng.pick(&["HAPPY NEW YEAR!", "happy new year!!", "woooo!", "cheers, everyone!", "here's to a good one!", "HAPPY NEW YEAR!!", "best year yet!"]);
                d.say(p.0.round() as i32, p.1 as i32 - 4, line, t + 0.4 + i as f32 * 0.25, 2.8, 3);
                self.blow(*p);
            }
        }
        gather
    }

    fn blow(&mut self, p: (f32, f32, bool)) {
        let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        let col = *self.rng.pick(&PARTY);
        let head = if p.2 { p.1 - 2.0 } else { p.1 - 3.0 };
        self.blowers.push(Blower { x: p.0 + dir, y: head, dir, age: 0.0, col });
    }

    /// The mast, the ball and the blowers (world space).
    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, left: f32, year: i32) {
        let (mx, top, foot) = self.mast;
        // a lattice tower with a lit platform on top
        let steel = Rgb::hex(0x9aa0b8);
        for y in top + 1..=foot {
            f.put_g(mx - 1, y, Glyph::new('▐', steel * 0.7).lit(0.5));
            f.put_g(mx, y, Glyph::new(if y % 2 == 0 { '╳' } else { '┼' }, steel).lit(0.5));
            f.put_g(mx + 1, y, Glyph::new('▌', steel * 0.7).lit(0.5));
        }
        for dx in -3..=3 {
            f.put_g(mx + dx, top, Glyph::new('▀', steel).lit(0.4));
            if dx % 2 == 0 {
                let c = PARTY[((dx + 3) as usize + (t * 2.0) as usize) % PARTY.len()];
                f.put_g(mx + dx, top - 1, Glyph::new('•', c).lit(0.0));
                f.light_cell(mx + dx, top - 1, c * (1.2 * glow));
            }
        }
        // the ball: up top until the last minute, then down it comes
        let u = if left >= 60.0 { 0.0 } else { (1.0 - left / 60.0).clamp(0.0, 1.0) };
        let by = (top as f32 + 2.0 + u * (foot - top - 6) as f32).round() as i32;
        let rows: [&[(i32, char)]; 3] = [
            &[(-2, '▗'), (-1, '█'), (0, '█'), (1, '█'), (2, '▖')],
            &[(-3, '█'), (-2, '█'), (-1, '█'), (0, '█'), (1, '█'), (2, '█'), (3, '█')],
            &[(-2, '▝'), (-1, '█'), (0, '█'), (1, '█'), (2, '▘')],
        ];
        for (r, row) in rows.iter().enumerate() {
            for &(dx, ch) in row.iter() {
                let y = by + r as i32;
                let k = 0.55 + 0.45 * ((t * 6.0 + dx as f32 * 1.9 + r as f32 * 2.3).sin() * 0.5 + 0.5);
                let c = PARTY[((t * 2.5) as i32 + dx + r as i32 + 6) as usize % PARTY.len()].lerp(Rgb::WHITE, 0.5);
                f.put_g(mx + dx, y, Glyph::new(ch, c * (1.15 * k)).lit(0.0));
                f.light_cell(mx + dx, y, c * (1.6 * k * glow));
            }
        }
        // after midnight the new year lights up at the foot of the mast
        if left <= 0.0 {
            let s = year.to_string();
            let x0 = mx - s.len() as i32 / 2;
            for (i, ch) in s.chars().enumerate() {
                let c = PARTY[(i + (t * 4.0) as usize) % PARTY.len()];
                f.put_g(x0 + i as i32, foot - 2, Glyph::new(ch, c * 1.3).lit(0.0));
                f.light_cell(x0 + i as i32, foot - 2, c * (2.0 * glow));
            }
        }
        for b in &self.blowers {
            // unrolls and rolls back up
            let len = ((b.age / 0.9 * std::f32::consts::PI).sin() * 4.0).round() as i32;
            for k in 0..len {
                let x = (b.x + b.dir * k as f32).round() as i32;
                let ch = if k == len - 1 { '⊸' } else if k % 2 == 0 { '━' } else { '╍' };
                let ch = if b.dir < 0.0 && k == len - 1 { '⟜' } else { ch };
                f.put_g(x, b.y.round() as i32, Glyph::new(ch, b.col).lit(0.0));
            }
        }
    }

    /// The big countdown (last ten seconds) and the midnight banner,
    /// in screen space over the sky.
    pub fn draw_sky(&self, f: &mut Frame, t: f32, left: f32, year: i32) {
        let w = f.w as i32;
        let sx = if w >= 240 { 2 } else { 1 };
        let row = 2;
        // off to the left of the big tree's star (the default view centres it)
        let cx = w * 3 / 10;
        if left > 0.0 && left <= 10.0 {
            let n = left.ceil() as i32;
            // each number pops in big and settles
            let pop = 1.0 - left.fract();
            let s = n.to_string();
            let big = sx * 2;
            let k = 0.7 + 0.5 * (1.0 - pop);
            big_text(f, &s, cx, row, big, Rgb::hex(0xffd36b) * k);
        }
        if let Some(m) = self.midnight_at {
            let age = t - m;
            if left <= 0.0 && age < 90.0 {
                let flash = 0.8 + 0.3 * (t * 5.0).sin();
                // one line, the year in party colours after the greeting
                let (msg, ys) = ("HAPPY NEW YEAR ", year.to_string());
                let total = (msg.len() + ys.len()) as i32 * 6 * sx;
                let x0 = w / 2 - total / 2;
                let mw = msg.len() as i32 * 6 * sx;
                big_text(f, msg.trim_end(), x0 + (mw - 6 * sx) / 2, row, sx, Rgb::hex(0xffd36b) * flash);
                big_text(f, &ys, x0 + mw + (ys.len() as i32 * 6 * sx) / 2, row, sx, PARTY[((t * 2.0) as usize) % PARTY.len()].lerp(Rgb::WHITE, 0.3));
                let _ = cx;
            }
        }
    }
}

/// 5×7 letters for the countdown and the banner.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        _ => [0; 7],
    }
}

/// Big block letters centred on column `cx`, top at `y`; each dot is
/// `s` cells wide (and `s/2`-ish tall, cells being twice as tall as wide).
pub fn big_text(f: &mut Frame, s: &str, cx: i32, y: i32, sc: i32, col: Rgb) {
    let sc = sc.max(1);
    let (dw, dh) = (sc, (sc / 2).max(1));
    let n = s.chars().count() as i32;
    let width = n * 6 * dw - dw;
    let x0 = cx - width / 2;
    for (i, c) in s.chars().enumerate() {
        let g = glyph(c);
        let gx = x0 + i as i32 * 6 * dw;
        for (r, bits) in g.iter().enumerate() {
            for b in 0..5 {
                if bits & (0x10 >> b) == 0 {
                    continue;
                }
                for yy in 0..dh {
                    for xx in 0..dw {
                        let (x, yv) = (gx + b * dw + xx, y + r as i32 * dh + yy);
                        f.put_g(x, yv, Glyph::new('█', col).lit(0.0));
                        f.light_cell(x, yv, col * 0.12);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn counts_down_to_midnight() {
        let eve = chrono::Local.with_ymd_and_hms(2026, 12, 31, 23, 59, 50).unwrap();
        assert!((to_midnight(&eve).unwrap() - 10.0).abs() < 0.01);
        let after = chrono::Local.with_ymd_and_hms(2027, 1, 1, 0, 0, 5).unwrap();
        assert!((to_midnight(&after).unwrap() + 5.0).abs() < 0.01);
        assert!(to_midnight(&chrono::Local.with_ymd_and_hms(2026, 12, 30, 23, 0, 0).unwrap()).is_none());
    }
}
