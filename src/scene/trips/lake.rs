//! Aurora Lake: a frozen lake far to the north, under the northern lights,
//! to skate on. The horizon across the middle of the screen: curtains of
//! light hanging over a far shore of low hills and dark spruce, and below
//! it the ice, a black glass mirror of it all (the aurora again, rippled,
//! the trees upside down), broken by drifts of snow and pale floes, its
//! cracks catching the light. The villager skates (← → push off, and it
//! glides on, slowing only a little), hops (Space), twirls (E); a few
//! others skate round the lake, and anyone else online who's come; each of
//! them mirrored in the ice under their blades. ↓ or Esc: home.
//!
//! The cinematic look: the aurora's rays in letters, and their reflection
//! in letters too (dimmer, rippled), and a faint grid of dots over the sky
//! and the clear ice.

use super::*;
use crate::rng::{fbm1, fbm2};
use crate::scene::aurora::Aurora;

/// Round the lake (columns): skating on from here comes back round.
pub const LAKE_W: f32 = 720.0;

/// Skating: top speed, the push, and how little it slows gliding.
const TOP: f32 = 30.0;
const PUSH: f32 = 26.0;
const GLIDE: f32 = 0.22;

/// On the lake: where round it, how fast and which way, off the ice (a
/// hop), twirling until, the stride (for the pose), the blades' lines in
/// the ice (where, when), the others skating round (who, where they
/// started, how fast, how far off: 0 by the far shore .. 1 near), and the
/// aurora's light (drawn each frame).
pub(super) struct Skate {
    x: f32,
    v: f32,
    air: f32,
    vy: f32,
    held: bool,
    twirl: f32,
    stride: f32,
    trail: Vec<(f32, f32)>,
    now: f32,
    others: Vec<(Passenger, f32, f32, f32)>,
    sky: std::cell::RefCell<Aurora>,
    /// The snow and cracks on the ice, worked out once for the screen.
    field: std::cell::RefCell<Option<Field>>,
}

/// The ice's snow and cracks, row by row below the horizon, all the way
/// round the lake (two samples a column, joined up where it comes round):
/// worked out once for a screen `ph` pixels tall, read as the view moves.
struct Field {
    ph: usize,
    hz: f32,
    snow: Vec<Vec<f32>>,
    crack: Vec<Vec<f32>>,
}

/// Samples a column of the lake, in the field.
const PER: f32 = 2.0;

impl Field {
    fn new(ph: usize, seed: u32) -> Field {
        let hz = horizon(ph);
        let below = (ph as f32 - hz).max(1.0);
        let n = (LAKE_W * PER) as usize;
        let (mut snow, mut crack) = (vec![], vec![]);
        for y in hz as usize..ph {
            let d = ((y as f32 - hz) / below).clamp(0.001, 1.0);
            let wz = 9.0 / (d + 0.06);
            // (round the lake and back to the start without a seam)
            let wrap = |f: &dyn Fn(f32) -> f32, i: usize| {
                let wx = i as f32 / PER;
                let u = wx / LAKE_W;
                f(wx) * (1.0 - u) + f(wx - LAKE_W) * u
            };
            let sn = |wx: f32| fbm2(wx * 0.028, wz * 0.28, 3, seed ^ 0x1ce);
            let cr = |wx: f32| fbm2(wx * 0.06 + 50.0, wz * 0.5, 2, seed ^ 0xc4);
            snow.push((0..n).map(|i| wrap(&sn, i)).collect());
            crack.push((0..n).map(|i| wrap(&cr, i)).collect());
        }
        Field { ph, hz, snow, crack }
    }

    /// (snow, crack) at screen column `x` and pixel row `y` (below the
    /// horizon), the villager at `cam` in the middle `mid`.
    fn at(&self, x: f32, y: usize, cam: f32, mid: f32) -> (f32, f32) {
        let Some(r) = y.checked_sub(self.hz as usize).filter(|&r| r < self.snow.len()) else { return (0.0, 0.5) };
        let d = ((y as f32 - self.hz) / (self.ph as f32 - self.hz).max(1.0)).clamp(0.001, 1.0);
        let wx = (cam + (x - mid) / (0.32 + 1.5 * d)).rem_euclid(LAKE_W);
        let i = ((wx * PER) as usize).min(self.snow[r].len() - 1);
        (self.snow[r][i], self.crack[r][i])
    }
}

/// How far round the lake from `a` to `b`, the short way (columns).
fn gap(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(LAKE_W);
    if d > LAKE_W / 2.0 {
        d - LAKE_W
    } else {
        d
    }
}

/// The horizon (pixel row) on a screen `ph` pixels tall.
fn horizon(ph: usize) -> f32 {
    (ph as f32 * 0.46).round()
}

/// The villager's feet (row) on a screen `h` rows tall.
pub(super) fn feet_row(h: usize) -> f32 {
    h as f32 * 0.84
}

impl Trip {
    /// Off to Aurora Lake (`seed`: the lake's floes and the others there).
    pub fn lake(t: f32, seed: u32) -> Self {
        let mut tr = Trip::new(seed);
        tr.at = Where::Lake { since: t };
        let mut rng = Rng::new(seed as u64 ^ 0x1a4e);
        let folk = Self::folk_for(&mut rng);
        let n = folk.len().min(4);
        let others = folk
            .into_iter()
            .take(4)
            .enumerate()
            .map(|(i, p)| {
                let way = if rng.chance(0.5) { 1.0 } else { -1.0 };
                (p, rng.range(0.0, LAKE_W), rng.range(5.0, 12.0) * way, 0.1 + 0.55 * i as f32 / n.max(1) as f32)
            })
            .collect();
        let x = (seed % 97) as f32 * 3.0;
        tr.lake = Some(Skate { x, v: 0.0, air: 0.0, vy: 0.0, held: false, twirl: 0.0, stride: 0.0, trail: vec![], now: t, others, sky: Default::default(), field: Default::default() });
        tr.say("Aurora Lake · ← → skate · Space: hop · E: twirl · ↓ or Esc: home", t, 6.0);
        tr.sounds.push((Sfx::Magic, 0.8));
        tr
    }

    /// Each frame on the lake: pushing off and gliding, a hop, the blades'
    /// lines; ↓ home.
    pub(super) fn lake_step(&mut self, dt: f32, t: f32, mv: (f32, f32)) {
        let Some(k) = &mut self.lake else { return };
        k.now = t;
        if mv.1 > 0.5 && k.air <= 0.0 {
            self.done = Some(Back::Lake);
            self.say("back to the village…", t, 2.0);
            self.sounds.push((Sfx::Chime, 0.5));
            return;
        }
        if mv.0 != 0.0 {
            // (pushing against the glide: a stop, quickly)
            let against = k.v * mv.0 < 0.0;
            k.v += mv.0 * PUSH * if against { 2.0 } else { 1.0 } * dt;
            self.dir = mv.0.signum();
            k.stride += dt * 2.2;
        } else {
            k.stride = 0.0;
        }
        k.v = k.v.clamp(-TOP, TOP) * (-dt * GLIDE).exp();
        if k.v.abs() < 0.05 && mv.0 == 0.0 {
            k.v = 0.0;
        }
        k.x = (k.x + k.v * dt).rem_euclid(LAKE_W);
        // the blades' line in the ice, every little way
        if k.air <= 0.0 && k.v.abs() > 1.0 && k.trail.last().is_none_or(|&(lx, _)| gap(lx, k.x).abs() > 0.8) {
            k.trail.push((k.x, t));
            if k.trail.len() > 400 {
                k.trail.remove(0);
            }
        }
        if k.air > 0.0 || k.vy > 0.0 {
            k.vy -= if k.held && k.vy > 0.0 { 22.0 } else { 42.0 } * dt;
            k.air += k.vy * dt;
            if k.air <= 0.0 {
                k.air = 0.0;
                k.vy = 0.0;
                self.sounds.push((Sfx::Crunch, 0.25));
            }
        }
    }

    /// Space on the lake: a hop (held, a little higher).
    pub(super) fn lake_jump(&mut self, held: bool) {
        let Some(k) = &mut self.lake else { return };
        if held && k.air <= 0.0 {
            k.vy = 13.0;
            k.air = 0.01;
            self.sounds.push((Sfx::Pickup, 0.3));
        }
        k.held = held;
    }

    /// E on the lake: a twirl.
    pub(super) fn lake_twirl(&mut self, t: f32) {
        let Some(k) = &mut self.lake else { return };
        if t < k.twirl {
            return;
        }
        k.twirl = t + 1.4;
        self.sounds.push((Sfx::Sparkle, 0.5));
    }

    /// Where the villager is round the lake, how high, for friends to see.
    pub(super) fn lake_here(&self) -> (f32, f32, bool) {
        self.lake.as_ref().map_or((0.0, 0.0, false), |k| (k.x, k.air, k.twirl > k.now))
    }

    pub(super) fn lake_rebase(&mut self, s: f32) {
        if let Some(k) = &mut self.lake {
            k.twirl -= s;
            k.now -= s;
            for p in &mut k.trail {
                p.1 -= s;
            }
        }
    }

    /// The lake: the sky and its aurora, the far shore, the ice mirroring
    /// it all, the snow on it, the skaters (and their reflections), and,
    /// cinematic, the letters.
    pub(super) fn draw_lake(&self, f: &mut Frame, t: f32, el: f32, look: &crate::scene::sprites::Look) {
        use crate::scene::sprites::Pose;
        let Some(k) = &self.lake else { return };
        let (pw, ph) = (f.pw, f.ph);
        if pw == 0 || ph < 4 {
            return;
        }
        let hz = horizon(ph);
        let hzi = hz as usize;
        let mid = pw as f32 / 2.0;
        let cam = k.x;
        let seed = self.seed;
        // (the snow and cracks on the ice, for this screen)
        if k.field.borrow().as_ref().is_none_or(|fl| fl.ph != ph) {
            *k.field.borrow_mut() = Some(Field::new(ph, seed));
        }
        let field = k.field.borrow();
        let Some(field) = field.as_ref() else { return };
        // ---- the sky: deep blue, lighter toward the horizon
        for y in 0..hzi.min(ph) {
            let v = y as f32 / hz.max(1.0);
            let c = if v < 0.6 { Rgb::hex(0x050b22).lerp(Rgb::hex(0x0b1a3c), v / 0.6) } else { Rgb::hex(0x0b1a3c).lerp(Rgb::hex(0x1a3158), (v - 0.6) / 0.4) };
            for x in 0..pw {
                f.px_set(x as i32, y as i32, c);
            }
        }
        // the stars (drifting a little as the villager skates)
        for i in 0..(pw * hzi / 70).clamp(40, 1400) as i32 {
            let sx = (h01(hash2(i, 17)) * pw as f32 - cam * 0.05).rem_euclid(pw as f32);
            let sy = h01(hash2(i, 29)) * (hz - 4.0);
            let tw = 0.55 + 0.45 * (t * (0.8 + h01(hash2(i, 3)) * 2.0) + i as f32).sin();
            let b = h01(hash2(i, 41));
            // (light added: a faint one never darker than the sky)
            f.px_add(sx as i32, sy as i32, Rgb::hex(0xdfe7f4) * ((0.12 + 0.7 * b * b) * tw));
        }
        // the aurora: the curtains, swelling now and then
        let flare = (0.5 + 0.5 * (t * 0.045 + seed as f32).sin()).powi(6) * 0.9;
        {
            let mut a = k.sky.borrow_mut();
            a.render(pw, hz, t, seed ^ 0xa0a, cam * 0.12, flare, 3);
            a.add_to(f, 1.0, 1.0, None);
        }
        // ---- the far shore: low hills, a line of dark spruce
        let shore = |x: usize| -> f32 {
            let wx = cam * 0.25 + (x as f32 - mid);
            hz - (1.5 + 4.0 * fbm1(wx * 0.012, 3, seed ^ 0x5e0))
        };
        for x in 0..pw {
            let wx = cam * 0.25 + (x as f32 - mid);
            let base = shore(x);
            // the trees round here (one every 3 columns or so, some gaps)
            let mut top = base;
            let j0 = (wx / 3.0).floor() as i32;
            for j in j0 - 3..=j0 + 3 {
                let r = h01(hash2(j, seed as i32 ^ 0x7ee));
                if r < 0.3 {
                    continue;
                }
                let c = j as f32 * 3.0 + h01(hash2(j, 5)) * 2.0;
                let tall = 3.0 + 7.0 * h01(hash2(j, 9)) * (0.6 + 0.4 * fbm1(c * 0.02, 2, seed ^ 0x7ef));
                let d = (wx - c).abs();
                // (a spruce: a point at the top, wider down)
                let at = base - tall + d / 0.38;
                top = top.min(at);
            }
            for y in (top.max(0.0) as i32)..(hzi as i32) {
                let land = y as f32 >= base;
                f.px_set(x as i32, y, if land { Rgb::hex(0x0b1222) } else { Rgb::hex(0x060a14) });
            }
            // (snow along the far bank, catching a little light)
            f.px_set(x as i32, hzi as i32 - 1, Rgb::hex(0x22304c));
        }
        // ---- the ice: a mirror of everything above the horizon, rippled
        let rows_below = (ph - hzi).max(1) as f32;
        // (the aurora's colour overall: the snow on the ice takes it on)
        let tint = {
            let a = k.sky.borrow();
            let (mut c, mut n) = (Rgb::BLACK, 0.0f32);
            for i in (0..a.lum.len()).step_by(7) {
                c += a.col[i];
                n += a.lum[i];
            }
            if n > 0.0 {
                c * (1.0 / n)
            } else {
                Rgb::hex(0x2cf08e)
            }
        };
        let glow = (k.sky.borrow().lum.iter().step_by(11).sum::<f32>() / (k.sky.borrow().lum.len() / 11).max(1) as f32 * 3.0).min(1.0);
        let snow_at = |x: f32, y: f32| -> f32 { field.at(x, y as usize, cam, mid).0 };
        let src: Vec<Rgb> = f.px[..hzi.min(ph) * pw].to_vec();
        for y in hzi..ph {
            let dy = (y - hzi) as f32;
            let d = dy / rows_below;
            let sy = (hzi as f32 - 1.0 - dy).max(0.0) as usize;
            let sy2 = sy.saturating_sub(1);
            for x in 0..pw {
                let xf = x as f32;
                let rip = ((y as f32 * 1.3 + t * 1.1).sin() * 0.6 + (y as f32 * 0.37 - t * 0.7 + xf * 0.05).sin() * 0.8) * (0.3 + 1.8 * d);
                let sx = ((xf + rip).round() as i32).clamp(0, pw as i32 - 1) as usize;
                let refl = src[sy * pw + sx] * 0.65 + src[sy2 * pw + sx] * 0.35;
                // darker nearer (looking down into it), a deep blue
                let mut c = refl * (0.66 - 0.26 * d) + Rgb::hex(0x040914) * (0.6 + 0.4 * d);
                // the snow and the floes on it, and thin frost at their edges
                let (n, cr) = field.at(xf, y, cam, mid);
                if n > 0.56 {
                    let edge = ((n - 0.56) / 0.04).clamp(0.0, 1.0);
                    let grain = 0.92 + 0.08 * h01(hash2((cam + xf) as i32 * 3 + x as i32, y as i32));
                    let lit = Rgb::hex(0x7f98bc) * (0.5 + 0.4 * d) * grain + tint * (0.1 + 0.18 * glow);
                    c = c.lerp(lit, edge * 0.92);
                    // a glint now and then
                    if edge >= 1.0 && h01(hash2((cam * 0.5 + xf) as i32, y as i32 ^ (t * 1.5) as i32)) > 0.9975 {
                        c = Rgb::hex(0xeaf4ff);
                    }
                } else if (cr - 0.5).abs() < 0.004 * (0.5 + d) {
                    // a crack in the clear ice, catching the light
                    c += Rgb::hex(0x1c2c44) * (0.6 + 0.4 * glow);
                }
                f.px_set(x as i32, y as i32, c);
            }
        }
        // the blades' lines in the ice (fading)
        let feet = feet_row(f.h);
        let fy = (feet * 2.0) as i32 + 1;
        for &(tx, born) in &k.trail {
            let age = t - born;
            if age > 9.0 {
                continue;
            }
            let sx = mid + gap(cam, tx);
            if (0.0..pw as f32).contains(&sx) {
                f.px_blend(sx as i32, fy, Rgb::hex(0xa8c0e0), (1.0 - age / 9.0) * 0.45);
            }
        }
        // ---- cinematic: the aurora in letters, and its reflection
        if self.cinematic {
            let a = k.sky.borrow();
            a.letters(f, 1.0, None);
            self.lake_letters(f, &a, hz, t, &snow_at);
        }
        // ---- the skaters, far ones first; each mirrored in the ice
        let sk = crate::scene::sprite_k(f.h);
        let skating = |stride: f32, air: f32| if air > 0.0 { Pose::Jump } else if stride.fract() > 0.5 { Pose::Walk(1) } else { Pose::Stand };
        let far_feet = hz / 2.0 + 3.0;
        let mut drawn: Vec<(f32, f32, crate::scene::sprites::Look, Pose, f32, f32, Option<&str>)> = vec![];
        for (p, x0, speed, depth) in &k.others {
            let wx = (x0 + speed * t).rem_euclid(LAKE_W);
            let scale = 0.5 + 0.5 * depth;
            let sx = mid + gap(cam, wx) * scale;
            if !(-12.0..pw as f32 + 12.0).contains(&sx) {
                continue;
            }
            let row = far_feet + (feet - far_feet) * (0.15 + 0.7 * depth);
            let twirl = h01(hash2((t * 0.2) as i32, (x0 * 10.0) as i32)) > 0.85;
            let dir = if twirl { if (t * 7.0) as i32 % 2 == 0 { 1.0 } else { -1.0 } } else { speed.signum() };
            drawn.push((row, sx, p.look(), skating(t * 1.6 + x0, 0.0), dir, 1.6 * sk * (0.55 + 0.45 * depth), None));
        }
        for fw in self.fellows.iter().filter(|fw| fw.at.at == 9) {
            let sx = mid + gap(cam, fw.at.x);
            if !(-12.0..pw as f32 + 12.0).contains(&sx) {
                continue;
            }
            let dir = if fw.at.sat { if (t * 7.0) as i32 % 2 == 0 { 1.0 } else { -1.0 } } else { fw.at.dir };
            drawn.push((feet - 2.5 - fw.at.air, sx, fw.look, if fw.at.air > 0.2 { Pose::Jump } else { Pose::Stand }, dir, 1.6 * sk * 0.9, Some(fw.name.as_str())));
        }
        drawn.sort_by(|p, q| p.0.total_cmp(&q.0));
        for (row, sx, l, pose, dir, kk, name) in &drawn {
            reflection(f, l, *pose, *dir, *sx, *row, *kk);
            person(f, l, *pose, *dir, *sx, *row, *kk);
            if let Some(n) = name {
                name_tag(f, n, *sx, (*row - 11.0 * sk) as i32);
            }
        }
        // the villager, in the middle (sliding in from the side at first)
        let come = (el / 0.8).clamp(0.0, 1.0);
        let x = mid - (1.0 - come) * mid * 0.6;
        let twirling = t < k.twirl;
        let dir = if twirling { if (t * 8.0) as i32 % 2 == 0 { 1.0 } else { -1.0 } } else { self.dir };
        let pose = if twirling { Pose::Stand } else { skating(k.stride, k.air) };
        let at = feet - k.air;
        reflection(f, look, pose, dir, x, feet + k.air, 1.6 * sk);
        person(f, look, pose, dir, x, at, 1.6 * sk);
        if twirling {
            for i in 0..5 {
                let a = t * 6.0 + i as f32 * 1.26;
                f.light_px(x + a.cos() * 3.0, (at - 6.0 * sk) * 2.0 + a.sin() * 2.0, Rgb::hex(0xbfe8ff) * 1.2);
            }
        }
    }

    /// The aurora's reflection in letters (cinematic): mirrored about the
    /// horizon, rippled, dimmer, not on the snow; and the faint dots over
    /// the sky and the clear ice where there's nothing else.
    fn lake_letters(&self, f: &mut Frame, a: &Aurora, hz: f32, t: f32, snow_at: &dyn Fn(f32, f32) -> f32) {
        if a.h < 2 || a.w == 0 {
            return;
        }
        let hzc = (hz / 2.0) as i32;
        for cy in hzc..f.h as i32 {
            let y = cy as f32 * 2.0 + 1.0;
            let dy = y - hz;
            let sy = (hz - 1.0 - dy).max(0.0) as usize;
            if sy >= a.h {
                continue;
            }
            let d = dy / (f.ph as f32 - hz).max(1.0);
            for cx in 0..(f.w.min(a.w)) as i32 {
                let rip = ((y * 1.3 + t * 1.1).sin() * 0.6 + (y * 0.37 - t * 0.7 + cx as f32 * 0.05).sin() * 0.8) * (0.3 + 1.8 * d);
                let sx = ((cx as f32 + rip).round() as i32).clamp(0, a.w as i32 - 1) as usize;
                let (c, l) = a.at(sy * a.w + sx);
                let l = l * (0.55 - 0.2 * d);
                if l < 0.06 || snow_at(cx as f32, y) > 0.55 {
                    continue;
                }
                let ch = if l < 0.12 {
                    '.'
                } else if l < 0.22 {
                    ':'
                } else if l < 0.36 {
                    '¦'
                } else {
                    '|'
                };
                f.put_g(cx, cy, Glyph::new(ch, c * (0.4 + 0.7 * l.min(1.2))).lit(0.0));
            }
        }
        // the dots: the sky, and (fainter) the clear ice
        let hz_row = hz;
        for cy in 0..f.h as i32 {
            if cy == hzc || cy == hzc - 1 {
                continue;
            }
            for cx in 0..f.w as i32 {
                if f.has_g(cx, cy) {
                    continue;
                }
                let y = cy as f32 * 2.0 + 1.0;
                if y > hz && snow_at(cx as f32, y) > 0.54 {
                    continue;
                }
                let (a, b) = (f.px_get(cx, cy * 2), f.px_get(cx, cy * 2 + 1));
                let (dark, star) = if a.r + a.g + a.b > b.r + b.g + b.b { (b, a) } else { (a, b) };
                if (a.r + a.g + a.b - b.r - b.g - b.b).abs() > 0.12 || star.r + star.g + star.b > 0.6 {
                    // (a star, or its reflection: a bright dot of its own)
                    f.put_g(cx, cy, Glyph::new('·', star * 1.3).bg(Bg::Solid(dark)).lit(1.25));
                    continue;
                }
                let lift = if y > hz_row { Rgb::hex(0x04070d) } else { Rgb::hex(0x080d18) };
                f.put_g(cx, cy, Glyph::new('·', (a + b) * 0.65 + lift).lit(0.0));
            }
        }
    }
}

thread_local! {
    /// Pictures turned upside down and darkened (reflections), by the
    /// picture they're of.
    static MIRRORED: std::cell::RefCell<std::collections::HashMap<u64, std::sync::Arc<crate::frame::Picture>>> = Default::default();
}

/// Someone's reflection in the ice: upside down under their feet, dark and
/// blue and see-through, as they'd be drawn standing at `feet`.
fn reflection(f: &mut Frame, look: &crate::scene::sprites::Look, pose: crate::scene::sprites::Pose, dir: f32, cx: f32, feet: f32, k: f32) {
    use crate::scene::sprites;
    if !crate::frame::pictures_ok() {
        return;
    }
    let pic = sprites::person(look, pose, dir, 1.0);
    let rows = sprites::rows_of(&pic, k);
    let flipped = MIRRORED.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 256 {
            m.clear();
        }
        m.entry(pic.id)
            .or_insert_with(|| {
                let (w, h) = (pic.w, pic.h);
                let mut rgba = vec![0u8; w * h * 4];
                for y in 0..h {
                    for x in 0..w {
                        let (s, d) = (((h - 1 - y) * w + x) * 4, (y * w + x) * 4);
                        // (fading the further down it goes, as it does in ice)
                        let fade = 1.0 - y as f32 / h as f32 * 0.6;
                        rgba[d] = (pic.rgba[s] as f32 * 0.38) as u8;
                        rgba[d + 1] = (pic.rgba[s + 1] as f32 * 0.42) as u8;
                        rgba[d + 2] = (pic.rgba[s + 2] as f32 * 0.55 + 18.0).min(255.0) as u8;
                        rgba[d + 3] = (pic.rgba[s + 3] as f32 * 0.7 * fade) as u8;
                    }
                }
                std::sync::Arc::new(crate::frame::Picture::new(w, h, rgba))
            })
            .clone()
    });
    f.sprite(cx, feet + rows, rows, flipped);
}
