//! Things going on up in the sky: soft clouds drifting past (moonlit at
//! night, white by day), an airplane blinking across now and then, flocks
//! of birds in a V by day, an owl gliding past the moon, and paper
//! lanterns floating up from the village (lots of them on Christmas Eve and
//! New Year's).

use crate::color::Rgb;
use crate::frame::{Frame, Glyph};
use crate::rng::{fbm2, noise1, Rng};

struct Cloud {
    x: f32,
    y: f32,
    len: f32,
    h: f32,
    speed: f32,
    seed: u32,
    dense: f32,
}

struct Flyer {
    x: f32,
    y: f32,
    dir: f32,
    speed: f32,
    age: f32,
    /// Clicked: landing lights blazing (plane), a hoot (owl), the flock
    /// scattering — until then.
    poke: f32,
    /// How far the flock has scattered (0 = a tidy V).
    spread: f32,
}

struct Lantern {
    /// World column, row (cells).
    x: f32,
    y: f32,
    vx: f32,
    age: f32,
    life: f32,
    phase: f32,
}

pub struct SkyLife {
    w: f32,
    /// Pixel row of the horizon (the sky is above it).
    horizon: f32,
    clouds: Vec<Cloud>,
    plane: Option<Flyer>,
    next_plane: f32,
    flock: Option<(Flyer, usize)>,
    next_flock: f32,
    owl: Option<Flyer>,
    next_owl: f32,
    lanterns: Vec<Lantern>,
    next_lantern: f32,
    rng: Rng,
}

impl SkyLife {
    pub fn new(w: usize, horizon: f32, seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0xc10d);
        let w = w as f32;
        let n = ((w / 110.0) as usize).clamp(3, 8);
        let clouds = (0..n)
            .map(|_| Cloud {
                x: rng.range(0.0, w),
                y: rng.range(horizon * 0.12, horizon * 0.55),
                len: rng.range(40.0, 120.0),
                h: rng.range(5.0, 12.0),
                speed: rng.range(0.6, 1.6),
                seed: rng.int(0, 1 << 20) as u32,
                dense: rng.range(0.35, 0.8),
            })
            .collect();
        SkyLife {
            w,
            horizon,
            clouds,
            plane: None,
            next_plane: rng.range(30.0, 90.0),
            flock: None,
            next_flock: rng.range(20.0, 80.0),
            owl: None,
            next_owl: rng.range(60.0, 180.0),
            lanterns: vec![],
            next_lantern: rng.range(20.0, 60.0),
            rng,
        }
    }

    pub fn rebase(&mut self, s: f32) {
        for f in [self.plane.as_mut(), self.owl.as_mut(), self.flock.as_mut().map(|f| &mut f.0)].into_iter().flatten() {
            f.poke -= s;
        }
        self.next_plane -= s;
        self.next_flock -= s;
        self.next_owl -= s;
        self.next_lantern -= s;
    }

    /// `sun`: daylight 0..1; `festive`: lanterns galore (Christmas Eve, New
    /// Year's); `view`: (left column, width) of the village in view, for
    /// where lanterns go up from; `moon_y`: the moon's row (the owl flies
    /// across it).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, wind: f32, sun: f32, festive: bool, view: (f32, f32), moon_y: f32) {
        let w = self.w;
        for c in &mut self.clouds {
            c.x += (c.speed + wind * 0.05) * dt;
            if c.x - c.len > w {
                c.x = -c.len;
            } else if c.x + c.len < -10.0 {
                c.x = w + c.len;
            }
        }
        // an airplane, high up, now and then
        if self.plane.is_none() && t > self.next_plane {
            let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            let y = self.rng.range(self.horizon * 0.06, self.horizon * 0.22) / 2.0;
            self.plane = Some(Flyer { x: if dir > 0.0 { -6.0 } else { w + 6.0 }, y, dir, speed: self.rng.range(5.0, 8.0), age: 0.0, poke: -1.0, spread: 0.0 });
            self.next_plane = t + self.rng.range(90.0, 240.0);
        }
        // birds by day, an owl by night
        if self.flock.is_none() && sun > 0.25 && t > self.next_flock {
            let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            let y = self.rng.range(self.horizon * 0.2, self.horizon * 0.6) / 2.0;
            let n = self.rng.int(5, 12) as usize;
            self.flock = Some((Flyer { x: if dir > 0.0 { -20.0 } else { w + 20.0 }, y, dir, speed: self.rng.range(9.0, 14.0), age: 0.0, poke: -1.0, spread: 0.0 }, n));
            self.next_flock = t + self.rng.range(60.0, 180.0);
        }
        if self.owl.is_none() && sun < 0.2 && t > self.next_owl {
            let dir = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            self.owl = Some(Flyer { x: if dir > 0.0 { -4.0 } else { w + 4.0 }, y: moon_y + self.rng.range(-1.5, 1.5), dir, speed: self.rng.range(10.0, 14.0), age: 0.0, poke: -1.0, spread: 0.0 });
            self.next_owl = t + self.rng.range(150.0, 360.0);
        }
        for f in [self.plane.as_mut(), self.owl.as_mut(), self.flock.as_mut().map(|f| &mut f.0)].into_iter().flatten() {
            // a startled flock (or owl) puts on a burst of speed
            let hurry = if t < f.poke { 1.6 } else { 1.0 };
            f.x += f.dir * f.speed * hurry * dt;
            f.age += dt;
            if t < f.poke {
                f.spread = (f.spread + dt * 2.5).min(3.0);
            } else {
                f.spread = (f.spread - dt * 0.4).max(0.0);
            }
        }
        let gone = |f: &Flyer, pad: f32| f.x < -pad - 2.0 || f.x > w + pad + 2.0;
        if self.plane.as_ref().is_some_and(|p| gone(p, 8.0)) {
            self.plane = None;
        }
        if self.owl.as_ref().is_some_and(|o| gone(o, 6.0)) {
            self.owl = None;
        }
        if self.flock.as_ref().is_some_and(|(f, _)| gone(f, 24.0)) {
            self.flock = None;
        }
        // paper lanterns floating up from the village
        if sun < 0.4 && t > self.next_lantern {
            let n = if festive { self.rng.int(2, 6) } else { 1 };
            for _ in 0..n {
                let x = view.0 + self.rng.range(0.05, 0.95) * view.1;
                self.lanterns.push(Lantern { x, y: self.horizon / 2.0 + self.rng.range(4.0, 10.0), vx: self.rng.range(-0.4, 0.4), age: 0.0, life: self.rng.range(35.0, 60.0), phase: self.rng.range(0.0, 6.3) });
            }
            self.next_lantern = t + if festive { self.rng.range(4.0, 10.0) } else { self.rng.range(50.0, 140.0) };
        }
        for l in &mut self.lanterns {
            l.age += dt;
            l.y -= 0.8 * dt;
            l.x += (l.vx + wind * 0.03 + (l.age * 0.7 + l.phase).sin() * 0.3) * dt;
        }
        self.lanterns.retain(|l| l.age < l.life && l.y > -60.0);
    }

    /// What's flying at (x, y) on the sky's own columns (screen column
    /// plus the sky's drift), if anything: 0 the plane, 1 the owl, 2 the
    /// flock.
    pub fn flyer_at(&self, x: i32, y: i32) -> Option<u8> {
        let (x, y) = (x as f32, y as f32);
        if self.plane.as_ref().is_some_and(|p| (p.x - x).abs() <= 3.5 && (p.y.round() - y).abs() <= 1.0) {
            return Some(0);
        }
        if self.owl.as_ref().is_some_and(|o| (o.x - x).abs() <= 2.5 && (o.y.round() - y).abs() <= 1.0) {
            return Some(1);
        }
        if let Some((fl, n)) = &self.flock {
            let ranks = (*n as f32 / 2.0).ceil() + 1.0;
            let (x0, x1) = if fl.dir > 0.0 { (fl.x - ranks * 2.2 - 1.0, fl.x + 1.0) } else { (fl.x - 1.0, fl.x + ranks * 2.2 + 1.0) };
            if x >= x0 && x <= x1 && (y - fl.y).abs() <= ranks * 0.55 + 1.5 {
                return Some(2);
            }
        }
        None
    }

    /// Click on a flyer: (its column and row, for the sound and sparkle).
    pub fn poke(&mut self, which: u8, t: f32) -> Option<(f32, f32)> {
        let f = match which {
            0 => self.plane.as_mut(),
            1 => self.owl.as_mut(),
            _ => self.flock.as_mut().map(|f| &mut f.0),
        }?;
        f.poke = t + if which == 0 { 2.5 } else { 1.5 };
        Some((f.x, f.y))
    }

    /// Clouds, on the pixel layer (screen space, drifted by `off`).
    pub fn draw_clouds(&self, f: &mut Frame, t: f32, sun: f32, moon: (f32, f32)) {
        let night = Rgb::hex(0x56608e);
        let day = Rgb::hex(0xf2f4fa);
        let base = night.lerp(day, sun);
        for c in &self.clouds {
            let (x0, x1) = ((c.x - c.len) as i32, (c.x + c.len) as i32);
            let (y0, y1) = ((c.y - c.h) as i32, (c.y + c.h) as i32);
            for y in y0..=y1 {
                let dy = (y as f32 - c.y) / c.h;
                for x in x0..=x1 {
                    let dx = (x as f32 - c.x) / c.len;
                    let fall = (1.0 - dx * dx - dy * dy * 1.4).max(0.0);
                    if fall <= 0.0 {
                        continue;
                    }
                    let n = fbm2(x as f32 * 0.06 + t * 0.01, y as f32 * 0.12, 3, c.seed);
                    let a = ((n - 0.36) * 3.0 * fall.sqrt()).clamp(0.0, 1.0) * c.dense * 0.8;
                    if a <= 0.01 {
                        continue;
                    }
                    // moonlit (or sunlit) on top, darker below
                    let near_moon = 1.0 - ((x as f32 - moon.0).hypot((y as f32 - moon.1) * 1.4) / 120.0).clamp(0.0, 1.0);
                    let lit = 0.8 + 0.25 * (-dy) + 0.5 * near_moon * (1.0 - sun);
                    f.px_blend(x, y, base * lit, a);
                }
            }
        }
    }

    /// The plane, birds and owl (screen space).
    pub fn draw_flyers(&self, f: &mut Frame, t: f32, glow: f32, sun: f32) {
        if let Some(p) = &self.plane {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            let grey = Rgb::hex(0x8a90a8).lerp(Rgb::hex(0xe0e4ee), sun);
            f.put_g(x, y, Glyph::new(if p.dir > 0.0 { '›' } else { '‹' }, grey).lit(0.0));
            f.put_g(x - p.dir as i32, y, Glyph::new('─', grey).lit(0.0));
            f.put_g(x - 2 * p.dir as i32, y, Glyph::new('─', grey * 0.8).lit(0.0));
            // nav lights: red and green steady, a white strobe
            let (r, g) = if p.dir > 0.0 { (x - 1, x + 1) } else { (x + 1, x - 1) };
            f.light_cell(r, y, Rgb::hex(0xff3030) * (0.7 * glow));
            f.light_cell(g, y, Rgb::hex(0x30ff60) * (0.7 * glow));
            if (t * 1.2).fract() < 0.08 {
                f.put_g(x - p.dir as i32, y, Glyph::new('✦', Rgb::WHITE).lit(0.0));
                f.light_cell(x, y, Rgb::WHITE * (2.0 * glow));
            }
            // clicked: landing lights on, blinking a greeting
            if t < p.poke {
                let on = ((p.poke - t) * 4.0) as i32 % 2 == 0;
                if on {
                    f.put_g(x + p.dir as i32, y, Glyph::new('◆', Rgb::hex(0xfff6dc)).lit(0.0));
                    f.light_cell(x + p.dir as i32, y, Rgb::hex(0xfff6dc) * (4.0 * glow));
                    f.light_cell(x + 2 * p.dir as i32, y, Rgb::hex(0xfff6dc) * (1.5 * glow));
                }
            }
        }
        if let Some((fl, n)) = &self.flock {
            let ink = Rgb::hex(0x2a2a3a).lerp(Rgb::hex(0x404858), 1.0 - sun);
            for k in 0..*n {
                // a V: the leader in front, the rest trailing back on both arms
                let rank = (k + 1) / 2;
                let side = if k % 2 == 0 { -1.0 } else { 1.0 };
                // (scattered when startled: each bird off on its own line)
                let sc = fl.spread * (((k * 37) % 11) as f32 / 11.0 - 0.5);
                let x = fl.x - fl.dir * rank as f32 * (2.2 + fl.spread * 0.6) + sc * 3.0;
                let y = fl.y + side * rank as f32 * 0.55 + (fl.age * 3.0 + k as f32).sin() * 0.15 + sc * 2.5 - fl.spread * 0.3;
                let flap = ((fl.age * 6.0 + k as f32 * 0.7) as i32) % 2 == 0;
                if k == 0 || side != 0.0 {
                    f.put_g(x.round() as i32, y.round() as i32, Glyph::new(if flap { 'v' } else { '‿' }, ink).lit(0.0));
                }
            }
        }
        if let Some(o) = &self.owl {
            let (x, y) = (o.x.round() as i32, o.y.round() as i32);
            // (flapping hard when startled)
            let flap = ((o.age * if t < o.poke { 12.0 } else { 5.0 }) as i32) % 3 == 0;
            let wing = if flap { '⌄' } else { '⌃' };
            let body = Rgb::hex(0x1a1410);
            f.put_g(x - 1, y, Glyph::new(wing, body).lit(0.0));
            f.put_g(x, y, Glyph::new('●', body).lit(0.0));
            f.put_g(x + 1, y, Glyph::new(wing, body).lit(0.0));
            let _ = (glow, noise1(t, 3));
        }
    }

    /// Lanterns (world space: draw with the frame's camera offset).
    pub fn draw_lanterns(&self, f: &mut Frame, t: f32, glow: f32) {
        for l in &self.lanterns {
            let fade = (1.0 - (l.age - l.life + 6.0).max(0.0) / 6.0).clamp(0.0, 1.0) * (l.age / 2.0).min(1.0);
            let fl = 0.8 + 0.2 * noise1(t * 6.0 + l.phase * 10.0, 17);
            let (x, y) = (l.x.round() as i32, l.y.round() as i32);
            f.put_g(x, y, Glyph::new('▆', Rgb::hex(0xffb050) * (1.2 * fl * fade)).alpha(fade).lit(0.0));
            f.light_cell(x, y, Rgb::hex(0xff9a40) * (2.2 * fl * fade * glow));
        }
    }
}
