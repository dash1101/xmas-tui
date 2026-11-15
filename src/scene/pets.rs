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


/// The villager's own pet: follows them about (a step behind, sitting when
/// they stop, off for a sniff when they've stood still a while), comes
/// indoors too — or stays home, if asked.
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
}

impl MyPet {
    /// From the config (None for "none").
    pub fn new(cfg: &crate::config::PetCfg, x: f32, y: f32) -> Option<MyPet> {
        let kind = PetKind::from_str(&cfg.kind)?;
        let (body, marks) = kind.coat(&cfg.coat);
        Some(MyPet { kind, coat: cfg.coat.clone(), name: cfg.name.clone(), body, marks, x, y, dir: 1.0, walking: false, happy_until: 0.0, staying: false, sniff: None, still_since: 0.0 })
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
        if let Some((_, u)) = &mut self.sniff {
            *u -= s;
        }
    }

    /// Follow `(px, py)` (facing `pdir`, `moving`), keeping to rows lo..=hi;
    /// `k` scales distances (2 indoors, where everything's drawn bigger).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, px: f32, py: f32, pdir: f32, moving: bool, lo: f32, hi: f32, k: f32) {
        if moving {
            self.still_since = t;
            self.sniff = None;
        }
        // stood still a while: off for a little sniff around, then back
        if !moving && self.sniff.is_none() && t - self.still_since > 9.0 && ((t * 0.37).sin() > 0.95) {
            let side = if (t * 1.3).sin() > 0.0 { 1.0 } else { -1.0 };
            self.sniff = Some((px + side * 9.0 * k, t + 4.0));
        }
        let tx = match self.sniff {
            Some((sx, until)) if t < until => sx,
            _ => {
                self.sniff = None;
                px - pdir * 3.5 * k
            }
        };
        let dx = tx - self.x;
        self.walking = dx.abs() > 0.6 * k;
        if self.walking {
            // trots to keep up, sprints if left behind
            let v = (dx.abs() * 3.0).clamp(3.0 * k, 45.0 * k);
            self.x += dx.signum() * (v * dt).min(dx.abs());
            self.dir = dx.signum();
        } else if !moving {
            self.dir = (px - self.x).signum();
        }
        let ty = py.clamp(lo, hi);
        self.y += (ty - self.y) * (1.0 - (-dt * 4.0).exp());
        // far behind (dropped in from the sky, came out of a door): catch up
        if (self.x - px).abs() > 80.0 * k {
            self.x = px - pdir * 4.0 * k;
            self.y = ty;
        }
    }

    pub fn draw(&self, f: &mut Frame, t: f32, scale: i32) {
        let pose = if t < self.happy_until {
            Pose::Up
        } else if self.walking {
            Pose::Walk
        } else {
            Pose::Sit
        };
        let feet = (self.y * 2.0).round() as i32 + 1;
        draw(f, self.kind, self.body, self.marks, self.x, feet, self.dir, pose, t, scale, Some(Rgb::hex(0x2fae5c)));
        // not named yet: a little "?" asking for one
        if self.name.is_empty() && (t * 1.5) as i32 % 2 == 0 {
            f.put_g(self.x.round() as i32, (feet - 6 * scale) / 2, crate::frame::Glyph::new('?', Rgb::hex(0xffd36b)).lit(0.0));
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
