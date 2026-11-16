//! Detailed sprites for people and pets, drawn over the scene as pictures
//! where the front-end can show them (the window, kitty, the exporter):
//! about three times finer than the grid's half-block pixels. People have
//! faces, mittened hands that swing, wave and pet, and scarves; pets are
//! proper cats, dogs and bunnies, smaller than people, with a clear
//! curled-up sleeping pose. Everything is made from simple shapes, posed,
//! given a thin dark outline (to read against the snow) and cached.

use crate::color::Rgb;
use crate::frame::Picture;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

/// Art pixels per grid row: a grown-up (24 pixels) is 4 rows tall.
pub const PX_PER_ROW: f32 = 6.0;

/// How many rows tall a sprite is (`k`: the scene's sprite scale).
pub fn rows_of(p: &Picture, k: f32) -> f32 {
    p.h as f32 / PX_PER_ROW * k
}

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<Option<Rgb>>,
}

impl Canvas {
    fn new(w: i32, h: i32) -> Self {
        Canvas { w, h, px: vec![None; (w * h) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = Some(c);
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: Rgb) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.set(x, y, c);
            }
        }
    }

    /// A filled ellipse centred on (cx, cy) (pixel centres at +0.5).
    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb) {
        for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx.max(0.01), (y as f32 + 0.5 - cy) / ry.max(0.01));
                if dx * dx + dy * dy <= 1.0 {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// A line `thick` pixels wide.
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: i32, c: Rgb) {
        let n = ((x1 - x0).abs().max((y1 - y0).abs()) * 2.0).ceil().max(1.0) as i32;
        for i in 0..=n {
            let u = i as f32 / n as f32;
            let (x, y) = (x0 + (x1 - x0) * u, y0 + (y1 - y0) * u);
            for dy in 0..thick {
                for dx in 0..thick {
                    self.set(x.floor() as i32 + dx, y.floor() as i32 + dy, c);
                }
            }
        }
    }

    /// Mirrored left-right (to face left).
    fn mirror(&mut self) {
        for y in 0..self.h {
            let row = &mut self.px[(y * self.w) as usize..((y + 1) * self.w) as usize];
            row.reverse();
        }
    }

    /// The picture: each drawn pixel, with a soft dark outline round the
    /// whole shape; `light` brightens or dims it.
    fn picture(&self, light: f32) -> Picture {
        let (w, h) = (self.w as usize + 2, self.h as usize + 2);
        let mut rgba = vec![0u8; w * h * 4];
        let at = |x: i32, y: i32| -> Option<Rgb> {
            if x >= 0 && y >= 0 && x < self.w && y < self.h {
                self.px[(y * self.w + x) as usize]
            } else {
                None
            }
        };
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = (y as usize * w + x as usize) * 4;
                match at(x - 1, y - 1) {
                    Some(c) => {
                        let c = c * light;
                        rgba[i..i + 4].copy_from_slice(&[to8(c.r), to8(c.g), to8(c.b), 255]);
                    }
                    None => {
                        let near = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| at(x - 1 + dx, y - 1 + dy).is_some());
                        if near {
                            rgba[i..i + 4].copy_from_slice(&[18, 16, 34, 170]);
                        }
                    }
                }
            }
        }
        Picture::new(w, h, rgba)
    }
}

fn cache() -> &'static Mutex<HashMap<u64, Arc<Picture>>> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<Picture>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(key: u64, make: impl FnOnce() -> Picture) -> Arc<Picture> {
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = c.get(&key) {
        return p.clone();
    }
    // plenty of room; start afresh rather than grow without end
    if c.len() > 4000 {
        c.clear();
    }
    let p = Arc::new(make());
    c.insert(key, p.clone());
    p
}

fn key(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

fn rgb_key(c: Rgb) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 4.0) * 60.0) as u32;
    b(c.r) << 20 | b(c.g) << 10 | b(c.b)
}

// ---------------------------------------------------------------------------
// people

/// What a person is doing with their arms and legs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pose {
    Stand,
    /// Walking: one of two strides.
    Walk(u8),
    /// A hand up, waving (hello).
    Wave,
    /// Both arms up (carried, falling).
    ArmsUp,
    /// Crouched, a hand to the snow (scooping a snowball).
    Scoop,
    /// An arm back over the shoulder (throwing).
    Throw,
    /// Leaning, a hand down in front (petting the pet).
    Pet,
    /// Arms round something held in front (a cuddle).
    Hold,
}

/// How a person looks.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub kid: bool,
    pub coat: Rgb,
    /// A bobble hat (None: bare-headed, showing the hair).
    pub hat: Option<Rgb>,
    pub hair: Rgb,
    pub scarf: Option<Rgb>,
    pub skin: Rgb,
    pub mittens: Rgb,
    /// Something in the front hand (a lantern glows; anything else is a
    /// little coloured thing).
    pub holding: Option<Rgb>,
    pub lantern: bool,
}

/// A person, posed and facing `dir` (>0 right), lit by `light`.
pub fn person(l: &Look, pose: Pose, dir: f32, light: f32) -> Arc<Picture> {
    let k = key((
        ("person", l.kid, rgb_key(l.coat), l.hat.map(rgb_key), rgb_key(l.hair), l.scarf.map(rgb_key)),
        (rgb_key(l.skin), rgb_key(l.mittens), l.holding.map(rgb_key), l.lantern),
        (pose, dir > 0.0, (light * 20.0) as i32),
    ));
    cached(k, || {
        let mut c = draw_person(l, pose);
        if dir < 0.0 {
            c.mirror();
        }
        c.picture(light)
    })
}

fn draw_person(l: &Look, pose: Pose) -> Canvas {
    let mut c = Canvas::new(16, if l.kid { 19 } else { 25 });
    // body plan (facing right; feet on the bottom row)
    let h = c.h;
    let kid = l.kid;
    let crouch = if pose == Pose::Scoop { if kid { 3 } else { 4 } } else { 0 };
    let boots_y = h - 2;
    let leg_len = if kid { 3 } else { 4 } - crouch / 2;
    let hip_y = boots_y - leg_len;
    let torso_h = if kid { 5 } else { 7 } - crouch / 2;
    let shoulder_y = hip_y - torso_h;
    let head_r = if kid { 2.6 } else { 2.8 };
    let head_cy = shoulder_y as f32 - head_r - 0.4;
    let cx = 7.0f32;
    let pants = l.coat * 0.45 + Rgb::hex(0x1e2238) * 0.5;
    let boot = Rgb::hex(0x3a2418);
    let coat = l.coat;
    let coat_dark = coat * 0.72;
    let sleeve = coat * 0.88;
    let lean = if pose == Pose::Pet { 1.0 } else { 0.0 };
    // legs and boots (the far leg darker); strides when walking
    let (front_dx, back_dx): (i32, i32) = match pose {
        Pose::Walk(0) => (2, -2),
        Pose::Walk(_) => (-1, 1),
        Pose::Scoop => (2, -1),
        _ => (1, -1),
    };
    for (dx, col, b) in [(back_dx, pants * 0.75, boot * 0.75), (front_dx, pants, boot)] {
        let x = (cx - 1.0) as i32 + dx.signum() * dx.abs().min(1);
        c.line(cx - 1.0 + (dx as f32) * 0.25, hip_y as f32, x as f32 + (dx as f32) * 0.5, (boots_y - 1) as f32, 2, col);
        let bx = x + dx / 2;
        c.rect(bx, boots_y, 3, 2, b);
    }
    // the coat: a body, a little wider at the hem, buttons down the front
    let top = shoulder_y;
    for y in top..=hip_y {
        let flare = if y >= hip_y - 1 { 1 } else { 0 };
        let w = if kid { 5 } else { 6 } + flare;
        let x0 = (cx - w as f32 / 2.0 + lean * ((y - top) as f32 / torso_h as f32 - 1.0)).round() as i32;
        c.rect(x0, y, w, 1, coat);
        // a darker back edge
        c.set(x0, y, coat_dark);
    }
    if !kid {
        for y in (top + 2..hip_y).step_by(2) {
            c.set((cx + 1.0).round() as i32, y, Rgb::hex(0xf2d27a));
        }
    }
    // the head: skin, hair at the back, an eye, a rosy cheek, a nose
    let hx = cx + 0.4 + lean;
    c.ellipse(hx, head_cy, head_r, head_r + 0.2, l.skin);
    for y in (head_cy - 1.5) as i32..=(head_cy + 1.4) as i32 {
        c.set((hx - head_r + 0.3) as i32, y, l.hair);
        c.set((hx - head_r + 1.2) as i32, y - 1, l.hair);
    }
    let (ex, ey) = ((hx + 1.2) as i32, (head_cy - 0.2) as i32);
    c.set(ex, ey, Rgb::hex(0x241a18));
    c.set(ex, ey + 2, Rgb::hex(0xf09a8a));
    c.set((hx + head_r) as i32, ey + 1, l.skin * 0.86);
    // the hat: a bobble hat with a turned-up brim and a pompom
    match l.hat {
        Some(hat) => {
            let brim = (head_cy - head_r * 0.45) as i32;
            c.ellipse(hx, brim as f32 - 0.6, head_r + 0.3, 2.0, hat);
            c.rect((hx - head_r - 0.3) as i32, brim, (head_r * 2.0 + 1.4) as i32, 1, hat * 0.75);
            c.ellipse(hx - 0.3, brim as f32 - 2.8, 1.1, 1.0, Rgb::hex(0xf6f2ea));
        }
        None => {
            c.ellipse(hx - 0.3, head_cy - head_r * 0.7, head_r, 1.4, l.hair);
        }
    }
    // the scarf, its end flying behind
    if let Some(sc) = l.scarf {
        let y = shoulder_y;
        c.rect((cx - 2.6) as i32, y - 1, 6, 2, sc);
        let tail = if matches!(pose, Pose::Walk(_)) { 2 } else { 1 };
        c.line(cx - 2.6, y as f32, cx - 2.6 - tail as f32, y as f32 + 2.5, 1, sc * 0.85);
    }
    // arms: from the shoulders to mittened hands
    let sh = shoulder_y as f32 + 1.0;
    let rest = hip_y as f32 - 1.0;
    let (back_hand, front_hand): ((f32, f32), (f32, f32)) = match pose {
        Pose::Stand => ((cx - 2.6, rest), (cx + 2.6, rest)),
        Pose::Walk(0) => ((cx - 3.6, rest - 0.5), (cx + 3.4, rest - 0.8)),
        Pose::Walk(_) => ((cx + 2.2, rest - 0.4), (cx - 2.8, rest - 0.4)),
        Pose::Wave => ((cx - 2.6, rest), (cx + 3.6, head_cy - 2.6)),
        Pose::ArmsUp => ((cx - 3.8, head_cy - 2.8), (cx + 3.8, head_cy - 2.8)),
        Pose::Scoop => ((cx - 1.8, rest), (cx + 4.8, boots_y as f32 + 0.5)),
        Pose::Throw => ((cx - 2.6, rest), (cx - 2.4, head_cy - 2.0)),
        Pose::Pet => ((cx - 1.6, rest), (cx + 6.0, boots_y as f32 - 1.0)),
        Pose::Hold => ((cx + 3.4, sh + 2.6), (cx + 4.2, sh + 1.8)),
    };
    let shoulder_back = (cx - 1.8 + lean, sh);
    let shoulder_front = (cx + 1.2 + lean, sh);
    c.line(shoulder_back.0, shoulder_back.1, back_hand.0, back_hand.1, 2, sleeve * 0.8);
    c.rect(back_hand.0 as i32, back_hand.1 as i32, 2, 2, l.mittens * 0.8);
    // something in the front hand
    if l.lantern {
        let (lx, ly) = (front_hand.0 as i32, front_hand.1 as i32);
        c.rect(lx, ly + 1, 1, 1, Rgb::hex(0x3a3a44));
        c.rect(lx - 1, ly + 2, 3, 3, Rgb::hex(0xffd27a) * 1.3);
        c.rect(lx - 1, ly + 2, 3, 1, Rgb::hex(0x3a3a44));
    } else if let Some(t) = l.holding {
        c.rect(front_hand.0 as i32 - 1, front_hand.1 as i32 - 1, 3, 3, t);
    }
    c.line(shoulder_front.0, shoulder_front.1, front_hand.0, front_hand.1, 2, sleeve);
    c.rect(front_hand.0 as i32, front_hand.1 as i32, 2, 2, l.mittens);
    c
}

// ---------------------------------------------------------------------------
// pets

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PetKind {
    Dog,
    Cat,
    Bunny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PetPose {
    Walk(u8),
    Sit,
    /// Curled up asleep (eyes shut, tail round).
    Sleep,
    /// Up on its hind legs (excited, begging).
    Up,
    /// Head down to the bowl.
    Eat,
}

/// A pet: its kind, coat (body and markings), collar; posed, facing `dir`.
pub fn pet(kind: PetKind, body: Rgb, marks: Rgb, collar: Option<Rgb>, pose: PetPose, wag: bool, dir: f32, light: f32) -> Arc<Picture> {
    let k = key(("pet", kind, rgb_key(body), rgb_key(marks), collar.map(rgb_key), pose, wag, dir > 0.0, (light * 20.0) as i32));
    cached(k, || {
        let mut c = match kind {
            PetKind::Cat => draw_cat(body, marks, collar, pose, wag),
            PetKind::Dog => draw_dog(body, marks, collar, pose, wag),
            PetKind::Bunny => draw_bunny(body, marks, pose),
        };
        if dir < 0.0 {
            c.mirror();
        }
        c.picture(light)
    })
}

const EYE: Rgb = Rgb::hex(0x1a1418);
const PINK: Rgb = Rgb::hex(0xf2a0a8);
const WHITE: Rgb = Rgb::hex(0xf6f2ea);

fn legs(c: &mut Canvas, xs: [i32; 4], top: i32, ground: i32, frame: Option<u8>, col: Rgb, far: Rgb) {
    for (i, &x) in xs.iter().enumerate() {
        let swing = match frame {
            Some(f) => (if (i + f as usize) % 2 == 0 { 1 } else { -1 }) * if i < 2 { 1 } else { 1 },
            None => 0,
        };
        let colr = if i % 2 == 1 { far } else { col };
        c.line(x as f32, top as f32, (x + swing) as f32, ground as f32, 1, colr);
    }
}

fn draw_cat(body: Rgb, marks: Rgb, collar: Option<Rgb>, pose: PetPose, wag: bool) -> Canvas {
    let mut c = Canvas::new(17, 11);
    let g = 10; // the ground row
    match pose {
        PetPose::Sleep => {
            // a curled-up loaf: body round, head tucked in, tail wrapped
            // round the front, eyes shut
            c.ellipse(7.5, 7.6, 5.6, 2.9, body);
            for x in (4..11).step_by(2) {
                c.set(x, 5, marks);
                c.set(x, 6, marks);
            }
            c.ellipse(11.6, 7.4, 2.4, 2.1, body);
            c.set(10, 5, body);
            c.set(13, 5, body);
            c.set(10, 4, marks);
            c.set(13, 4, marks);
            c.line(11.0, 7.0, 12.6, 7.0, 1, EYE);
            c.set(14, 8, PINK);
            c.line(2.0, 9.4, 12.5, 9.6, 1, marks);
            c.set(13, 9, marks * 0.8);
        }
        PetPose::Sit | PetPose::Up | PetPose::Eat => {
            let up = pose == PetPose::Up;
            let eat = pose == PetPose::Eat;
            // sitting tall: body upright, front legs straight, tail round the paws
            c.ellipse(7.0, 7.0 - up as i32 as f32, 3.6, 3.2, body);
            c.rect(8, 7 - up as i32, 2, 4, body);
            c.set(8, g, WHITE);
            c.set(9, g, WHITE);
            for y in (5..9).step_by(2) {
                c.set(5, y, marks);
                c.set(6, y, marks);
            }
            let (hx, hy) = if eat { (11.4, 7.5) } else if up { (9.6, 2.4) } else { (9.4, 3.4) };
            c.ellipse(hx, hy, 2.6, 2.3, body);
            c.set(hx as i32 - 1, hy as i32 - 2, body);
            c.set(hx as i32 - 1, hy as i32 - 3, marks);
            c.set(hx as i32 + 2, hy as i32 - 2, body);
            c.set(hx as i32 + 2, hy as i32 - 3, marks);
            c.set(hx as i32 - 1, hy as i32 - 2, PINK);
            if !eat {
                c.set(hx as i32 + 1, hy as i32, EYE);
                c.set(hx as i32 + 2, hy as i32 + 1, PINK);
                c.set(hx as i32 + 3, hy as i32 + 1, WHITE * 0.9);
            }
            if let Some(k) = collar {
                c.rect(hx as i32 - 1, hy as i32 + 2, 3, 1, k);
            }
            // the tail curls round the front (or flicks up)
            if wag || up {
                c.line(3.6, 8.0, 1.6, 4.0, 1, body);
                c.set(1, 3, marks);
            } else {
                c.line(3.4, 9.0, 1.6, 9.6, 1, body);
                c.line(1.6, 9.6, 6.0, 10.0, 1, marks);
            }
        }
        PetPose::Walk(f) => {
            // on all fours: a long body, head up, tail up
            c.ellipse(7.5, 6.0, 4.8, 2.2, body);
            for x in (4..11).step_by(2) {
                c.set(x, 4, marks);
            }
            legs(&mut c, [10, 9, 5, 4], 7, g, Some(f), body, body * 0.78);
            c.ellipse(12.8, 4.2, 2.4, 2.2, body);
            c.set(11, 1, body);
            c.set(14, 1, body);
            c.set(11, 2, PINK);
            c.set(13, 4, EYE);
            c.set(15, 5, PINK);
            if let Some(k) = collar {
                c.rect(11, 6, 2, 1, k);
            }
            let tip = if wag { (1.0, 0.8) } else { (0.6, 2.0) };
            c.line(3.0, 5.0, tip.0, tip.1, 1, body);
            c.set(tip.0 as i32, tip.1 as i32, marks);
        }
    }
    c
}

fn draw_dog(body: Rgb, marks: Rgb, collar: Option<Rgb>, pose: PetPose, wag: bool) -> Canvas {
    let mut c = Canvas::new(19, 12);
    let g = 11;
    let head = |c: &mut Canvas, hx: f32, hy: f32, down: bool, pant: bool| {
        c.ellipse(hx, hy, 2.6, 2.4, body);
        // the muzzle, a nose, an ear that flops
        let (mx, my) = if down { (hx + 2.0, hy + 2.0) } else { (hx + 2.4, hy + 0.8) };
        c.rect(mx as i32, my as i32, 2, 2, body * 1.05);
        c.set(mx as i32 + 2, my as i32, EYE);
        if !down {
            c.set(hx as i32 + 1, hy as i32 - 1, EYE);
        }
        c.line(hx - 1.4, hy - 2.0, hx - 2.0, hy + 1.6, 2, marks);
        if pant && !down {
            c.set(mx as i32 + 1, my as i32 + 2, PINK);
            c.set(mx as i32 + 1, my as i32 + 3, PINK);
        }
    };
    match pose {
        PetPose::Sleep => {
            c.ellipse(9.0, 8.8, 6.6, 2.6, body);
            for x in (5..12).step_by(3) {
                c.set(x, 7, marks);
            }
            c.ellipse(14.0, 8.6, 2.6, 2.0, body);
            c.rect(16, 9, 2, 1, body);
            c.line(12.4, 7.2, 13.4, 9.8, 2, marks);
            c.line(14.0, 8.0, 15.4, 8.0, 1, EYE);
            c.set(17, 9, EYE);
            c.line(2.0, 10.4, 6.0, 10.8, 1, body);
        }
        PetPose::Sit | PetPose::Up | PetPose::Eat => {
            let up = pose == PetPose::Up;
            let eat = pose == PetPose::Eat;
            c.ellipse(7.4, 7.6 - up as i32 as f32, 3.8, 3.4, body);
            c.rect(9, 7 - up as i32, 2, 5, body);
            c.set(9, g, body * 1.1);
            c.set(10, g, body * 1.1);
            c.ellipse(9.0, 6.6, 1.4, 1.8, WHITE * 0.9);
            if eat {
                head(&mut c, 12.0, 7.4, true, false);
            } else {
                head(&mut c, 10.2, if up { 2.4 } else { 3.4 }, false, up);
            }
            if let Some(k) = collar {
                c.rect(8, if up { 5 } else { 6 }, 3, 1, k);
            }
            let tail = if wag { (2.0, 4.0) } else { (1.6, 7.0) };
            c.line(3.8, 8.6, tail.0, tail.1, 2, body);
        }
        PetPose::Walk(f) => {
            c.ellipse(8.0, 6.6, 5.4, 2.4, body);
            c.ellipse(7.0, 7.8, 3.0, 1.2, WHITE * 0.85);
            legs(&mut c, [11, 10, 5, 4], 8, g, Some(f), body, body * 0.78);
            head(&mut c, 13.6, 4.0, false, wag);
            if let Some(k) = collar {
                c.rect(12, 6, 2, 1, k);
            }
            let tail = if wag { (1.0, 1.6) } else { (1.0, 3.4) };
            c.line(3.0, 5.4, tail.0, tail.1, 2, body);
        }
    }
    c
}

fn draw_bunny(body: Rgb, marks: Rgb, pose: PetPose) -> Canvas {
    let mut c = Canvas::new(15, 12);
    let g = 11;
    match pose {
        PetPose::Sleep => {
            // a round loaf, ears laid back flat, eyes shut
            c.ellipse(7.0, 8.8, 5.0, 2.6, body);
            c.ellipse(10.6, 8.4, 2.2, 2.0, body);
            c.line(9.0, 7.0, 4.0, 6.4, 2, body);
            c.line(9.0, 7.2, 5.0, 6.8, 1, PINK);
            c.line(10.0, 8.0, 11.4, 8.0, 1, EYE);
            c.set(13, 9, PINK);
            c.ellipse(2.4, 9.0, 1.2, 1.2, WHITE);
        }
        PetPose::Walk(f) => {
            // mid-hop: stretched out
            let lift = if f % 2 == 0 { 1.0 } else { 0.0 };
            c.ellipse(6.6, 7.8 - lift, 4.4, 2.4, body);
            c.ellipse(10.4, 5.8 - lift, 2.2, 2.0, body);
            c.line(10.0, 4.0 - lift, 8.6, 0.4, 2, body);
            c.line(9.6, 3.6 - lift, 8.8, 1.2, 1, PINK);
            c.set(11, (5.4 - lift) as i32, EYE);
            c.set(12, (6.4 - lift) as i32, PINK);
            c.line(9.0, 9.0 - lift, 11.0 + f as f32, g as f32, 1, body);
            c.line(4.0, 9.0 - lift, 2.0, g as f32, 2, body * 0.85);
            c.ellipse(2.2, 6.6 - lift, 1.2, 1.2, WHITE);
        }
        _ => {
            let up = pose == PetPose::Up;
            let eat = pose == PetPose::Eat;
            c.ellipse(6.0, 8.2 - up as i32 as f32 * 1.5, 3.8, 2.8, body);
            c.ellipse(5.4, 9.0, 2.4, 1.4, marks * 1.05);
            let (hx, hy) = if eat { (9.6, 8.2) } else if up { (8.4, 3.6) } else { (8.8, 5.0) };
            c.ellipse(hx, hy, 2.2, 2.0, body);
            // long ears, pink inside
            c.line(hx - 0.6, hy - 1.6, hx - 1.4, hy - 5.6, 2, body);
            c.line(hx - 0.4, hy - 1.8, hx - 1.0, hy - 4.6, 1, PINK);
            c.line(hx + 0.8, hy - 1.6, hx + 1.4, hy - 5.0, 1, body * 0.9);
            if !eat {
                c.set(hx as i32 + 1, hy as i32, EYE);
            }
            c.set(hx as i32 + 2, hy as i32 + 1, PINK);
            c.rect(8, g - 1, 2, 2, body * 1.05);
            c.ellipse(2.0, 8.0, 1.3, 1.3, WHITE);
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_and_pets_are_drawn_cached_and_mirrored() {
        let l = Look { kid: false, coat: Rgb::hex(0xd8a040), hat: Some(Rgb::hex(0x2fae5c)), hair: Rgb::hex(0x3a2a1a), scarf: Some(Rgb::hex(0xc8323c)), skin: Rgb::hex(0xe8c39e), mittens: Rgb::hex(0xc8323c), holding: None, lantern: false };
        let a = person(&l, Pose::Wave, 1.0, 1.0);
        let b = person(&l, Pose::Wave, 1.0, 1.0);
        assert!(Arc::ptr_eq(&a, &b), "not cached");
        assert_eq!((a.w, a.h), (18, 27));
        // facing left is the mirror image
        let m = person(&l, Pose::Wave, -1.0, 1.0);
        let row = |p: &Picture, y: usize| (0..p.w).map(|x| p.rgba[(y * p.w + x) * 4 + 3]).collect::<Vec<_>>();
        let (mut r1, r2) = (row(&a, 10), row(&m, 10));
        r1.reverse();
        assert_eq!(r1, r2);
        // a grown-up is 4 rows tall; a cat about half that, sitting
        assert!((rows_of(&a, 1.0) - 4.5).abs() < 0.6);
        let cat = pet(PetKind::Cat, Rgb::hex(0xe08a3a), Rgb::hex(0xa85a20), None, PetPose::Sit, false, 1.0, 1.0);
        assert!(rows_of(&cat, 1.0) < rows_of(&a, 1.0) * 0.6);
        // asleep, it's lower and longer than sitting
        let asleep = pet(PetKind::Cat, Rgb::hex(0xe08a3a), Rgb::hex(0xa85a20), None, PetPose::Sleep, false, 1.0, 1.0);
        let opaque_rows = |p: &Picture| (0..p.h).filter(|&y| row(p, y).iter().any(|&a| a == 255)).count();
        assert!(opaque_rows(&asleep) < opaque_rows(&cat));
    }
}

/// `--sprite-sheet OUT.png`: every pose of a grown-up, a kid and each pet,
/// 4× size on the snow, to look the art over.
pub fn sheet(path: &std::path::Path) -> std::io::Result<()> {
    let adult = Look { kid: false, coat: Rgb::hex(0xd8a040), hat: Some(Rgb::hex(0x2fae5c)), hair: Rgb::hex(0x3a2a1a), scarf: Some(Rgb::hex(0xc8323c)), skin: Rgb::hex(0xe8c39e), mittens: Rgb::hex(0xc8323c), holding: None, lantern: false };
    let walker = Look { coat: Rgb::hex(0x2a4c8a), hat: Some(Rgb::hex(0xe8323c)), scarf: None, lantern: true, mittens: Rgb::hex(0x6a4a2a), ..adult };
    let kid = Look { kid: true, coat: Rgb::hex(0x6a3a8a), hat: Some(Rgb::hex(0xffc23a)), scarf: Some(Rgb::hex(0x2fae5c)), ..adult };
    let poses = [Pose::Stand, Pose::Walk(0), Pose::Walk(1), Pose::Wave, Pose::ArmsUp, Pose::Scoop, Pose::Throw, Pose::Pet, Pose::Hold];
    let mut rows: Vec<Vec<Arc<Picture>>> = vec![];
    for l in [&adult, &walker, &kid] {
        rows.push(poses.iter().map(|&p| person(l, p, 1.0, 1.0)).collect());
    }
    let pp = [PetPose::Walk(0), PetPose::Walk(1), PetPose::Sit, PetPose::Up, PetPose::Eat, PetPose::Sleep];
    for (kind, b, m) in [(PetKind::Cat, 0xe08a3a, 0xa85a20), (PetKind::Dog, 0xd8a050, 0x8a5a28), (PetKind::Bunny, 0xe8e0d4, 0xc8b8a8)] {
        rows.push(pp.iter().map(|&p| pet(kind, Rgb::hex(b), Rgb::hex(m), Some(Rgb::hex(0x2fae5c)), p, false, 1.0, 1.0)).collect());
    }
    let s = 4;
    let cell = (22 * s, 29 * s);
    let (w, h) = (cell.0 * 9, cell.1 * rows.len());
    let mut buf = vec![0xd8e0eeu32; w * h];
    for (ry, row) in rows.iter().enumerate() {
        for (cx, p) in row.iter().enumerate() {
            let x = (cx * cell.0 + (cell.0 - p.w * s) / 2) as i32;
            let y = (ry * cell.1 + cell.1 - p.h * s - 4) as i32;
            p.blit_scaled(&mut buf, w, x, y, s);
        }
    }
    let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().map_err(std::io::Error::other)?.write_image_data(&rgb).map_err(std::io::Error::other)
}
