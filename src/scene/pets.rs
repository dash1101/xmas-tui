//! Pets: dogs, cats and bunnies. Villagers walk theirs (the dog on a lead),
//! house cats and dogs potter about indoors, and the villager has one of
//! their own — named, coloured, and following them everywhere, indoors too.
//!
//! Drawn as tiny pixel art on the half-block layer: at village scale one
//! pixel per cell column (two per cell row), indoors at double size.

use crate::color::Rgb;
use crate::frame::Frame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PetKind {
    Dog,
    Cat,
    Bunny,
}

impl PetKind {
    pub fn from_str(s: &str) -> Option<PetKind> {
        match s.trim().to_lowercase().as_str() {
            "dog" | "puppy" => Some(PetKind::Dog),
            "cat" | "kitten" => Some(PetKind::Cat),
            "bunny" | "rabbit" => Some(PetKind::Bunny),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PetKind::Dog => "dog",
            PetKind::Cat => "cat",
            PetKind::Bunny => "bunny",
        }
    }

    /// The colours this kind comes in: (name, body, markings).
    pub fn coats(self) -> &'static [(&'static str, u32, u32)] {
        match self {
            PetKind::Dog => &[
                ("golden", 0xd8a050, 0x8a5a28),
                ("chocolate", 0x6a3e22, 0x3a2012),
                ("black", 0x2a2a30, 0x111114),
                ("snowy", 0xeeeae0, 0xb8b0a0),
                ("spotty", 0xeeeae0, 0x2a2a30),
                ("grey", 0x8a8a92, 0x55555c),
                ("red", 0xb85a2a, 0x7a3418),
            ],
            PetKind::Cat => &[
                ("ginger", 0xe08a3a, 0xa85a20),
                ("black", 0x26262c, 0x101012),
                ("snowy", 0xf0ece4, 0xc8c0b4),
                ("tabby", 0x9a9aa0, 0x5a5a62),
                ("calico", 0xf0ece4, 0xd07a2a),
                ("cream", 0xe8d2a8, 0xc0a070),
                ("tuxedo", 0x26262c, 0xf0ece4),
            ],
            PetKind::Bunny => &[
                ("snowy", 0xf2eee6, 0xc8b8b0),
                ("brown", 0x9a6a40, 0x6a4424),
                ("grey", 0x9a9aa0, 0x6a6a72),
                ("black", 0x2a2a30, 0x141418),
                ("cream", 0xe8d8b8, 0xb8a080),
            ],
        }
    }

    /// Body and markings colours for a named coat (the first if unknown).
    pub fn coat(self, name: &str) -> (Rgb, Rgb) {
        let c = self.coats();
        let (_, a, b) = c.iter().find(|c| c.0.eq_ignore_ascii_case(name.trim())).copied().unwrap_or(c[0]);
        (Rgb::hex(a), Rgb::hex(b))
    }
}

/// Names to choose from (and for the villagers' pets).
pub const NAMES: &[&str] = &[
    "Biscuit", "Pudding", "Maple", "Pepper", "Clementine", "Nutmeg", "Juniper", "Mittens", "Sprout", "Bean", "Waffles", "Chestnut", "Ginger", "Pickle",
    "Pumpkin", "Olive", "Toffee", "Cocoa", "Snowball", "Pip", "Button", "Tinsel", "Holly", "Jingles", "Frosty", "Noodle", "Pretzel", "Marshmallow",
    "Cinnamon", "Bramble", "Figgy", "Sprocket", "Dumpling", "Comet", "Mistletoe", "Crumpet",
];

/// What a pet's up to (for the drawing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pose {
    Walk,
    Sit,
    Sleep,
    /// Begging / excited (on its hind legs, or a bunny's binky).
    Up,
}

// Right-facing pixel art, one char per pixel: b body, m markings, d dark
// (nose, eyes), w light (belly, cotton tail), k collar, ' ' clear.
// Two walk frames, then sitting and sleeping.
const DOG: [&[&str]; 4] = [
    &["    bm ", "m   bbd", " bbbbk ", " bbbbb ", " b  b  "],
    &["    bm ", " m  bbd", " bbbbk ", " bbbbb ", "  b  b "],
    &["   bm  ", "   bbd ", "m  bk  ", "mbbbb  ", " bb b  "],
    &["       ", "       ", "    bm ", "mbbbbbd", " bbbbb "],
];
const CAT: [&[&str]; 4] = [
    &["    m m", "m   bbb", "m   bdb", " bbbbb ", " b   b "],
    &["    m m", " m  bbb", "m   bdb", " bbbbb ", "  b b  "],
    &["   m m ", "   bbb ", "m  bdb ", "m bbbb ", " bb bb "],
    &["       ", "       ", "    m m", " mbbbbb", "mbbbbbd"],
];
const BUNNY: [&[&str]; 4] = [
    &["   mm ", "   bb ", "  bbbd", "wbbbb ", " b  b "],
    &["   mm ", "   bb ", " bbbbd", "wbbbb ", "  bb  "],
    &["  mm  ", "  bb  ", "  bbd ", " bbbb ", "wbbbb "],
    &["      ", "      ", "   mm ", " bbbbd", "wbbbbb"],
];

/// Draw a pet with its feet on pixel row `feet`, centred on column `x`,
/// facing `dir`; `scale` 1 in the village, 2 indoors. `t` animates the walk
/// and the tail.
#[allow(clippy::too_many_arguments)]
pub fn draw(f: &mut Frame, kind: PetKind, body: Rgb, marks: Rgb, x: f32, feet: i32, dir: f32, pose: Pose, t: f32, scale: i32, collar: Option<Rgb>) {
    let art = match kind {
        PetKind::Dog => &DOG,
        PetKind::Cat => &CAT,
        PetKind::Bunny => &BUNNY,
    };
    let frame = match pose {
        Pose::Walk => ((t * 7.0) as i32 % 2) as usize,
        Pose::Sit | Pose::Up => 2,
        Pose::Sleep => 3,
    };
    let rows = art[frame];
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
    let h = rows.len() as i32;
    // a happy little hop when excited, a breath when asleep
    let lift = match pose {
        Pose::Up => (((t * 6.0).sin() * 0.5 + 0.5) * 1.5) as i32,
        _ => 0,
    };
    let x0 = x.round() as i32 - w * scale / 2;
    let top = feet - h * scale + 1 - lift * scale;
    let wag = (t * if pose == Pose::Up { 14.0 } else { 6.0 }) as i32 % 2 == 0;
    for (ry, row) in rows.iter().enumerate() {
        for (i, ch) in row.chars().enumerate() {
            let col = match ch {
                'b' => body,
                'm' => {
                    // the tail (the leftmost markings) wags
                    if i == 0 && kind != PetKind::Bunny && pose != Pose::Sleep && wag {
                        continue;
                    }
                    marks
                }
                'd' => Rgb::hex(0x141014),
                'w' => Rgb::hex(0xf8f6f0),
                'k' => collar.unwrap_or(body),
                _ => continue,
            };
            // shading: a touch lighter on top
            let col = col * (1.06 - 0.06 * ry as f32 / h as f32);
            let px = if dir >= 0.0 { i as i32 } else { w - 1 - i as i32 };
            for sy in 0..scale {
                for sx in 0..scale {
                    f.px_set(x0 + px * scale + sx, top + ry as i32 * scale + sy, col);
                }
            }
        }
    }
    // a wagging tail on the alternate frame (dogs and cats)
    if kind != PetKind::Bunny && pose != Pose::Sleep && !wag {
        let ty = top + scale;
        let tx = if dir >= 0.0 { x0 - scale } else { x0 + w * scale };
        for sy in 0..scale {
            for sx in 0..scale {
                f.px_set(tx + sx, ty + sy, marks);
            }
        }
    }
}

/// Half the drawn width, in pixels at scale 1 (for hit tests).
pub fn half_width(kind: PetKind) -> f32 {
    match kind {
        PetKind::Bunny => 3.0,
        _ => 3.5,
    }
}


/// Something thrown for the villager's pet to chase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToyKind {
    Yarn,
    Ball,
    Stick,
    Carrot,
}

impl ToyKind {
    pub fn name(self) -> &'static str {
        match self {
            ToyKind::Yarn => "ball of yarn",
            ToyKind::Ball => "ball",
            ToyKind::Stick => "stick",
            ToyKind::Carrot => "carrot",
        }
    }
}

/// A toy in flight or on the ground: column, row (where it rests), height
/// above the ground (pixels) and their speeds.
#[derive(Clone, Copy, Debug)]
pub struct Toy {
    pub kind: ToyKind,
    pub x: f32,
    pub y: f32,
    h: f32,
    vx: f32,
    vh: f32,
    /// Where the yarn started unravelling from (a trail to there).
    trail: Option<f32>,
    /// Lying about since then (picked up by the villager again later).
    rest_at: f32,
}

/// What the pet is busy with (instead of just following).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Busy {
    /// After the toy; how it'll go (0..4), and the step it's on.
    Fetch { how: u8, step: u8, until: f32 },
    /// Bringing it back (dog), maybe the long way round.
    Bring { lap: bool },
    /// After an animal at this column (or stalking it: a cat), until then.
    Chase { x: f32, until: f32, stalk: bool },
    /// Zooming round the villager / another pet.
    Zoom { cx: f32, until: f32 },
    /// A trick: which (0..), until then.
    Trick { which: u8, until: f32 },
}

/// Something the pet did, for the scene to show and play.
pub enum PetEv {
    Emote(char, Rgb, f32, f32),
    Say(String, f32, f32),
    Sound(crate::audio::Sfx, f32),
}

/// The villager's own pet: follows them about (a step behind, sitting when
/// they stop, off for a sniff when they've stood still a while, napping
/// after a longer while), comes indoors too — or stays home, if asked;
/// fetches (or bats about, or nibbles) what's thrown, does tricks, and
/// plays with other animals.
pub struct MyPet {
    pub kind: PetKind,
    pub coat: String,
    pub name: String,
    body: Rgb,
    marks: Rgb,
    pub x: f32,
    pub y: f32,
    dir: f32,
    walking: bool,
    pub happy_until: f32,
    /// Staying at home instead of following.
    pub staying: bool,
    /// Off sniffing something until then (column it went to).
    sniff: Option<(f32, f32)>,
    still_since: f32,
    /// Curled up asleep (the villager stood still a good while, or is in bed).
    pub napping: bool,
    /// A thrown toy, and what the pet's up to.
    pub toy: Option<Toy>,
    pub busy: Option<Busy>,
    carrying: bool,
    /// Not before then: chasing / playing with other animals.
    pub social_at: f32,
    events: Vec<PetEv>,
    seed: u32,
}

impl MyPet {
    /// From the config (None for "none").
    pub fn new(cfg: &crate::config::PetCfg, x: f32, y: f32) -> Option<MyPet> {
        let kind = PetKind::from_str(&cfg.kind)?;
        let (body, marks) = kind.coat(&cfg.coat);
        Some(MyPet {
            kind,
            coat: cfg.coat.clone(),
            name: cfg.name.clone(),
            body,
            marks,
            x,
            y,
            dir: 1.0,
            walking: false,
            happy_until: 0.0,
            staying: false,
            sniff: None,
            still_since: 0.0,
            napping: false,
            toy: None,
            busy: None,
            carrying: false,
            social_at: 20.0,
            events: vec![],
            seed: crate::rng::hash(x.to_bits() ^ 0x9e37),
        })
    }

    /// The config changed (kind, colour or name).
    pub fn restyle(&mut self, cfg: &crate::config::PetCfg) {
        if let Some(k) = PetKind::from_str(&cfg.kind) {
            self.kind = k;
        }
        self.coat = cfg.coat.clone();
        let (b, m) = self.kind.coat(&self.coat);
        (self.body, self.marks) = (b, m);
        self.name = cfg.name.clone();
    }

    pub fn cfg(&self) -> crate::config::PetCfg {
        crate::config::PetCfg { kind: self.kind.name().to_string(), coat: self.coat.clone(), name: self.name.clone() }
    }

    pub fn rebase(&mut self, s: f32) {
        self.happy_until -= s;
        self.still_since -= s;
        self.social_at -= s;
        if let Some((_, u)) = &mut self.sniff {
            *u -= s;
        }
        if let Some(t) = &mut self.toy {
            t.rest_at -= s;
        }
        match &mut self.busy {
            Some(Busy::Fetch { until, .. } | Busy::Chase { until, .. } | Busy::Zoom { until, .. } | Busy::Trick { until, .. }) => *until -= s,
            _ => {}
        }
    }

    /// What it did since last asked.
    pub fn take_events(&mut self) -> Vec<PetEv> {
        std::mem::take(&mut self.events)
    }

    fn roll(&mut self, n: u32) -> u32 {
        self.seed = crate::rng::hash(self.seed.wrapping_add(0x6d2b_79f5));
        self.seed % n.max(1)
    }

    fn name_or(&self) -> String {
        if self.name.is_empty() { format!("your {}", self.kind.name()) } else { self.name.clone() }
    }

    fn sound(&self) -> crate::audio::Sfx {
        match self.kind {
            PetKind::Dog => crate::audio::Sfx::Bark,
            PetKind::Cat => crate::audio::Sfx::Meow,
            PetKind::Bunny => crate::audio::Sfx::Pet,
        }
    }

    /// Wake up (and forget any nap).
    pub fn wake(&mut self, t: f32) {
        self.napping = false;
        self.still_since = t;
    }

    /// The villager throws a toy from (x, y) the way they face; `k` 2
    /// indoors (everything bigger). The pet goes after it.
    pub fn throw(&mut self, kind: ToyKind, x: f32, y: f32, dir: f32, t: f32, k: f32) {
        self.wake(t);
        let far = (14.0 + self.roll(10) as f32) * k;
        let flight = 0.8;
        self.toy = Some(Toy { kind, x, y, h: 6.0 * k, vx: dir * far / flight, vh: 14.0 * k, trail: None, rest_at: f32::INFINITY });
        let how = self.roll(4) as u8;
        self.busy = Some(Busy::Fetch { how, step: 0, until: t + 12.0 });
        self.carrying = false;
        let s = self.sound();
        self.events.push(PetEv::Sound(s, self.x));
    }

    /// "Chase me!": zoomies round the villager.
    pub fn zoomies(&mut self, cx: f32, t: f32) {
        self.wake(t);
        self.busy = Some(Busy::Zoom { cx, until: t + 4.0 });
        self.happy_until = t + 4.0;
    }

    /// A trick (which one depends on the pet, and on its mood).
    pub fn trick(&mut self, t: f32) -> String {
        self.wake(t);
        let which = self.roll(4) as u8;
        self.busy = Some(Busy::Trick { which, until: t + 2.6 });
        let n = self.name_or();
        let s = self.sound();
        self.events.push(PetEv::Sound(s, self.x));
        match (self.kind, which) {
            (PetKind::Dog, 0) => format!("{n} rolls over — paws in the air, tongue out ✦"),
            (PetKind::Dog, 1) => format!("{n} spins a circle. then another. then forgets which way is up"),
            (PetKind::Dog, 2) => format!("{n} sits up and begs, very politely"),
            (PetKind::Dog, _) => format!("bang! {n} flops over… and peeks to see if it worked"),
            (PetKind::Cat, 0) => format!("{n} gives you a slow blink. the highest honour"),
            (PetKind::Cat, 1) => format!("{n} sits up on their hind legs and bats the air. a high five!"),
            (PetKind::Cat, 2) => format!("{n} considers it… and washes a paw instead"),
            (PetKind::Cat, _) => format!("{n} rolls onto their back. it's a trap — but a cute one"),
            (PetKind::Bunny, 0) => format!("{n} does a binky — a twisty leap of pure joy!"),
            (PetKind::Bunny, 1) => format!("{n} stands up tall, ears straight up, very proud"),
            (PetKind::Bunny, 2) => format!("{n} boops your boot with their nose"),
            (PetKind::Bunny, _) => format!("{n} does a little flop onto their side. so relaxed"),
        }
    }

    /// Off after an animal at column `x` (a cat stalks first).
    pub fn chase(&mut self, x: f32, t: f32) {
        self.wake(t);
        let stalk = self.kind == PetKind::Cat;
        self.busy = Some(Busy::Chase { x, until: t + if stalk { 4.0 } else { 2.6 }, stalk });
        if !stalk {
            let s = self.sound();
            self.events.push(PetEv::Sound(s, self.x));
        }
    }

    /// Where the animal being chased has got to.
    pub fn chasing(&mut self, x: f32) {
        if let Some(Busy::Chase { x: cx, .. }) = &mut self.busy {
            *cx = x;
        }
    }

    /// Playing with another pet at column `x`: a happy bounce and a zoom.
    pub fn play_with(&mut self, x: f32, t: f32) {
        self.wake(t);
        self.happy_until = t + 3.0;
        self.busy = Some(Busy::Zoom { cx: x, until: t + 3.0 });
        self.events.push(PetEv::Emote('♥', Rgb::hex(0xff6b8a), (self.x + x) / 2.0, self.y - 2.0));
    }

    /// Free to start something new?
    pub fn idle(&self) -> bool {
        self.busy.is_none() && !self.staying
    }

    /// Follow `(px, py)` (facing `pdir`, `moving`), keeping to rows lo..=hi;
    /// `k` scales distances (2 indoors, where everything's drawn bigger).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, px: f32, py: f32, pdir: f32, moving: bool, lo: f32, hi: f32, k: f32) {
        if moving {
            self.still_since = t;
            self.sniff = None;
            self.napping = false;
        }
        self.update_toy(dt, t, k);
        // a good long while standing about: a nap (wakes when they move off)
        if !moving && self.busy.is_none() && self.sniff.is_none() && t - self.still_since > 22.0 + (self.seed % 9) as f32 {
            if !self.napping {
                self.napping = true;
                self.events.push(PetEv::Emote('z', Rgb::hex(0xc8d0e0), self.x, self.y - 2.0 * k));
            }
        }
        if self.napping {
            self.walking = false;
            let ty = py.clamp(lo, hi);
            self.y += (ty - self.y) * (1.0 - (-dt * 2.0).exp());
            return;
        }
        // stood still a while: off for a little sniff around, then back
        if !moving && self.busy.is_none() && self.sniff.is_none() && t - self.still_since > 9.0 && ((t * 0.37).sin() > 0.95) {
            let side = if (t * 1.3).sin() > 0.0 { 1.0 } else { -1.0 };
            self.sniff = Some((px + side * 9.0 * k, t + 4.0));
        }
        let mut speed = 1.0;
        let mut ty = py;
        let tx = match self.busy {
            Some(Busy::Fetch { until, .. }) if t > until => {
                self.busy = None;
                px - pdir * 3.5 * k
            }
            Some(Busy::Fetch { how, step, until }) => {
                let toy = self.toy;
                match toy {
                    // still in the air: run to where it'll land
                    Some(toy) if toy.h > 0.0 => {
                        speed = 1.6;
                        ty = toy.y;
                        toy.x + toy.vx * (toy.vh / 30.0).max(0.0)
                    }
                    Some(toy) => {
                        ty = toy.y;
                        speed = 1.6;
                        if (toy.x - self.x).abs() <= 1.5 * k {
                            self.reach_toy(how, step, until, px, t, k);
                        }
                        toy.x
                    }
                    None => {
                        self.busy = None;
                        px - pdir * 3.5 * k
                    }
                }
            }
            Some(Busy::Bring { lap }) => {
                speed = 1.4;
                if lap {
                    // a victory lap first, out and back
                    let side = if self.x < px { -1.0 } else { 1.0 };
                    let out = px + side * 22.0 * k;
                    if (self.x - out).abs() < 1.5 * k {
                        self.busy = Some(Busy::Bring { lap: false });
                    }
                    out
                } else {
                    if (self.x - px).abs() < 3.0 * k {
                        // dropped at the villager's feet (back in hand)
                        self.carrying = false;
                        self.toy = None;
                        self.busy = None;
                        self.happy_until = t + 2.0;
                        self.events.push(PetEv::Emote('♥', Rgb::hex(0xff6b8a), self.x, self.y - 2.0 * k));
                    }
                    px - pdir * 2.5 * k
                }
            }
            Some(Busy::Chase { until, .. }) if t > until => {
                self.busy = None;
                self.social_at = t + 25.0;
                px - pdir * 3.5 * k
            }
            Some(Busy::Chase { x, stalk, until }) => {
                // a cat creeps up, then pounces in the last second
                let pounce = stalk && until - t < 1.0;
                speed = if stalk && !pounce { 0.35 } else { 1.5 };
                if pounce && t < self.happy_until.max(0.0) + 0.0 {
                    self.happy_until = t + 0.6;
                }
                x
            }
            Some(Busy::Zoom { until, .. }) if t > until => {
                self.busy = None;
                px - pdir * 3.5 * k
            }
            Some(Busy::Zoom { cx, until }) => {
                speed = 1.8;
                cx + (t * 3.2).sin() * 10.0 * k + (until - t).sin() * 2.0
            }
            Some(Busy::Trick { until, .. }) => {
                if t > until {
                    self.busy = None;
                    self.happy_until = t + 1.0;
                }
                self.x
            }
            None => match self.sniff {
                Some((sx, until)) if t < until => sx,
                _ => {
                    self.sniff = None;
                    px - pdir * 3.5 * k
                }
            },
        };
        let dx = tx - self.x;
        self.walking = dx.abs() > 0.6 * k;
        if self.walking {
            // trots to keep up, sprints if left behind
            let v = (dx.abs() * 3.0).clamp(3.0 * k, 45.0 * k) * speed;
            self.x += dx.signum() * (v * dt).min(dx.abs());
            self.dir = dx.signum();
        } else if !moving && self.busy.is_none() {
            self.dir = (px - self.x).signum();
        }
        let ty = ty.clamp(lo, hi);
        self.y += (ty - self.y) * (1.0 - (-dt * 4.0).exp());
        // far behind (dropped in from the sky, came out of a door): catch up
        if (self.x - px).abs() > 80.0 * k {
            self.x = px - pdir * 4.0 * k;
            self.y = ty;
            self.busy = None;
            self.toy = None;
            self.carrying = false;
        }
    }

    /// The toy's flight: an arc, a bounce, a roll to a stop.
    fn update_toy(&mut self, dt: f32, t: f32, k: f32) {
        let carrying = self.carrying;
        let (x, y, dir) = (self.x, self.y, self.dir);
        let Some(toy) = &mut self.toy else { return };
        if carrying {
            toy.x = x + dir * 2.0 * k;
            toy.y = y;
            toy.h = 3.0 * k;
            return;
        }
        if toy.h > 0.0 || toy.vh > 0.0 {
            toy.vh -= 36.0 * k * dt;
            toy.h += toy.vh * dt;
            toy.x += toy.vx * dt;
            if toy.h <= 0.0 {
                toy.h = 0.0;
                if toy.vh < -8.0 * k && toy.kind != ToyKind::Stick {
                    toy.vh = -toy.vh * 0.35;
                } else {
                    toy.vh = 0.0;
                }
                toy.vx *= 0.55;
            }
        } else {
            toy.vx *= (1.0 - 3.0 * dt).max(0.0);
            toy.x += toy.vx * dt;
            if toy.rest_at.is_infinite() && toy.vx.abs() < 0.5 {
                toy.rest_at = t;
            }
        }
        // lying about a while: tidied away
        if t - toy.rest_at > 25.0 && self.busy.is_none() {
            self.toy = None;
        }
    }

    /// Caught up with the toy: what happens depends on the pet and the throw.
    fn reach_toy(&mut self, how: u8, step: u8, until: f32, px: f32, t: f32, k: f32) {
        let n = self.name_or();
        let (x, y) = (self.x, self.y);
        let s = self.sound();
        let Some(toy) = &mut self.toy else { return };
        let what = toy.kind.name();
        match self.kind {
            PetKind::Dog => {
                self.carrying = true;
                let lap = how == 1;
                let line = match how {
                    0 => format!("{n} brings the {what} back, very pleased with themself"),
                    1 => format!("{n} grabs the {what} and does a victory lap first"),
                    2 => {
                        toy.kind = ToyKind::Stick;
                        format!("{n} comes back with… a stick. close enough!")
                    }
                    _ => format!("{n} trots back and drops it — then nudges it closer. again!"),
                };
                self.events.push(PetEv::Say(line, x, y - 4.0 * k));
                self.events.push(PetEv::Sound(s, x));
                self.busy = Some(Busy::Bring { lap });
            }
            PetKind::Cat => {
                // bats it about a couple of times, then settles
                if step < 2 {
                    toy.vx = (if step % 2 == 0 { 1.0 } else { -1.0 }) * (8.0 + 4.0 * step as f32) * k * if px > x { -1.0 } else { 1.0 };
                    toy.vh = 6.0 * k;
                    toy.h = 0.5;
                    if how == 1 && toy.trail.is_none() {
                        toy.trail = Some(toy.x);
                    }
                    self.happy_until = t + 0.6;
                    self.busy = Some(Busy::Fetch { how, step: step + 1, until });
                    self.events.push(PetEv::Emote('✦', Rgb::hex(0xffd36b), x, y - 2.0 * k));
                } else {
                    let line = match how {
                        0 => format!("{n} pounces, bats the {what} about, and curls up round it"),
                        1 => format!("{n} has unravelled the whole {what}. a masterpiece"),
                        2 => format!("{n} chases the {what}… then decides it was never interesting"),
                        _ => format!("{n} wrestles the {what} into submission. it won't trouble anyone again"),
                    };
                    self.events.push(PetEv::Say(line, x, y - 4.0 * k));
                    self.busy = None;
                    if how == 0 {
                        self.napping = true;
                    }
                    toy.rest_at = t;
                }
            }
            PetKind::Bunny => {
                if step < 1 && how != 2 {
                    toy.vx = self.dir * 6.0 * k;
                    self.happy_until = t + 0.6;
                    self.busy = Some(Busy::Fetch { how, step: step + 1, until });
                } else {
                    let line = match how {
                        0 | 3 => {
                            self.toy = None;
                            self.events.push(PetEv::Sound(crate::audio::Sfx::Munch, x));
                            format!("{n} nibbles the {what} down to the leaves. *munch munch*")
                        }
                        _ => {
                            self.happy_until = t + 1.5;
                            format!("{n} nudges the {what} along with their nose, then does a happy hop")
                        }
                    };
                    self.events.push(PetEv::Say(line, x, y - 4.0 * k));
                    self.busy = None;
                }
            }
        }
    }

    pub fn draw(&self, f: &mut Frame, t: f32, scale: i32) {
        let tricking = match self.busy {
            Some(Busy::Trick { which, until }) => Some((which, until)),
            _ => None,
        };
        let pose = if self.napping {
            Pose::Sleep
        } else if let Some((which, until)) = tricking {
            // rolling over / playing dead: on its side; spinning; sitting up
            match (self.kind, which) {
                (PetKind::Dog, 0 | 3) | (PetKind::Cat, 3) | (PetKind::Bunny, 3) => Pose::Sleep,
                (_, 1) if ((until - t) * 6.0) as i32 % 2 == 0 => Pose::Walk,
                (PetKind::Cat, 2) => Pose::Sit,
                _ => Pose::Up,
            }
        } else if t < self.happy_until {
            Pose::Up
        } else if self.walking {
            Pose::Walk
        } else {
            Pose::Sit
        };
        let dir = match tricking {
            Some((1, until)) => if ((until - t) * 4.0) as i32 % 2 == 0 { 1.0 } else { -1.0 },
            _ => self.dir,
        };
        let feet = (self.y * 2.0).round() as i32 + 1;
        draw(f, self.kind, self.body, self.marks, self.x, feet, dir, pose, t, scale, Some(Rgb::hex(0x2fae5c)));
        if self.napping && (t * 0.8) as i32 % 2 == 0 {
            f.put_g(self.x.round() as i32 + 2 * scale, (feet - 7 * scale) / 2, crate::frame::Glyph::new('z', Rgb::hex(0xc8d0e0)).lit(0.0));
        }
        // not named yet: a little "?" asking for one
        if self.name.is_empty() && (t * 1.5) as i32 % 2 == 0 {
            f.put_g(self.x.round() as i32, (feet - 6 * scale) / 2, crate::frame::Glyph::new('?', Rgb::hex(0xffd36b)).lit(0.0));
        }
        if let Some(toy) = &self.toy {
            draw_toy(f, toy, scale);
        }
    }

    /// Within reach of (x, y)? (`k` as for `update`)
    pub fn near(&self, x: f32, y: f32, k: f32) -> bool {
        (self.x - x).abs() <= 5.0 * k && (self.y - y).abs() <= 3.0 * k
    }

    /// Clicked on? (cell column, row)
    pub fn hit(&self, x: i32, y: i32, k: f32) -> bool {
        (self.x - x as f32 - 0.5).abs() <= half_width(self.kind) * k && y as f32 <= self.y + 0.5 && y as f32 >= self.y - 2.0 * k
    }
}

/// A toy: a ball of yarn (with its trail, once unravelled), a ball, a
/// stick or a carrot, `scale` times as big indoors.
fn draw_toy(f: &mut Frame, toy: &Toy, scale: i32) {
    let s = scale as f32;
    let (x, ground) = (toy.x, toy.y * 2.0 + 1.0);
    let y = ground - toy.h - s;
    match toy.kind {
        ToyKind::Yarn => {
            if let Some(from) = toy.trail {
                let n = (x - from).abs().max(1.0) as i32;
                for i in 0..=n {
                    let u = i as f32 / n as f32;
                    let xx = from + (x - from) * u;
                    let wob = ((xx * 1.7).sin() * 0.6).round();
                    f.px_set(xx.round() as i32, (ground + wob).round() as i32, Rgb::hex(0xd84a6a));
                }
            }
            f.px_disc(x, y, 1.1 * s, Rgb::hex(0xd84a6a), 1.0);
            f.px_set(x.round() as i32 - 1, y.round() as i32 - 1, Rgb::hex(0xf08aa0));
        }
        ToyKind::Ball => {
            f.px_disc(x, y, 1.0 * s, Rgb::hex(0xc8e040), 1.0);
            f.px_set(x.round() as i32, y.round() as i32 - 1, Rgb::hex(0xf0f8a0));
        }
        ToyKind::Stick => {
            for i in 0..(5.0 * s) as i32 {
                f.px_set(x.round() as i32 - (2.5 * s) as i32 + i, y.round() as i32 + if i % 3 == 0 { 0 } else { 1 } - 1, Rgb::hex(0x6a4428));
            }
        }
        ToyKind::Carrot => {
            for i in 0..(4.0 * s) as i32 {
                f.px_set(x.round() as i32 - i, y.round() as i32, Rgb::hex(0xf07a28));
            }
            f.px_set(x.round() as i32 + 1, y.round() as i32 - 1, Rgb::hex(0x3aa04a));
            f.px_set(x.round() as i32 + 1, y.round() as i32, Rgb::hex(0x3aa04a));
        }
    }
}
