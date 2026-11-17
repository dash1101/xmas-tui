//! Pets: dogs, cats and bunnies (and, for the villager, a young reindeer, a
//! fox or a little bird too). Villagers walk theirs (the dog on a lead),
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
    /// A young reindeer (Rudolph's coat has the nose).
    Reindeer,
    Fox,
    /// A little bird: hops along, and flutters to keep up.
    Bird,
}

/// Every kind the villager can have, in the order the menu steps through.
pub const KINDS: [PetKind; 6] = [PetKind::Dog, PetKind::Cat, PetKind::Bunny, PetKind::Reindeer, PetKind::Fox, PetKind::Bird];

impl PetKind {
    pub fn from_str(s: &str) -> Option<PetKind> {
        match s.trim().to_lowercase().as_str() {
            "dog" | "puppy" => Some(PetKind::Dog),
            "cat" | "kitten" => Some(PetKind::Cat),
            "bunny" | "rabbit" => Some(PetKind::Bunny),
            "reindeer" | "deer" => Some(PetKind::Reindeer),
            "fox" => Some(PetKind::Fox),
            "bird" | "robin" => Some(PetKind::Bird),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PetKind::Dog => "dog",
            PetKind::Cat => "cat",
            PetKind::Bunny => "bunny",
            PetKind::Reindeer => "reindeer",
            PetKind::Fox => "fox",
            PetKind::Bird => "bird",
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
            // (a reindeer's markings: its nose)
            PetKind::Reindeer => &[
                ("brown", 0x8a5a32, 0x1a1414),
                ("rudolph", 0x8a5a32, 0xff2a2a),
                ("snowy", 0xe8e2d6, 0x3a2a24),
                ("dark", 0x5a3a22, 0x1a1414),
                ("caramel", 0xb07a48, 0x1a1414),
            ],
            // (a fox's markings: its ears and socks)
            PetKind::Fox => &[
                ("red", 0xd8682a, 0x2a1a14),
                ("arctic", 0xf2f0ea, 0x8a8a92),
                ("silver", 0x6a6a74, 0x1a1a1e),
                ("golden", 0xe0a040, 0x5a3418),
            ],
            // (a bird's markings: its breast)
            PetKind::Bird => &[
                ("robin", 0x8a6a4a, 0xe0482a),
                ("bluebird", 0x3a6ad8, 0xe89a5a),
                ("cardinal", 0xc8202a, 0xe0483a),
                ("goldfinch", 0xf0c838, 0x2a2a2a),
                ("snow bunting", 0xf2f0ea, 0x8a6a4a),
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
    /// Head down to the bowl.
    Eat,
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

const REINDEER: [&[&str]; 4] = [
    &["    m m ", "     bbd", "m   bbb ", "bbbbbb  ", "b b b b "],
    &["    m m ", "     bbd", "m   bbb ", "bbbbbb  ", " b b  b "],
    &["    m m ", "     bbd", "    bbb ", "mbbbbb  ", " bb bb  "],
    &["        ", "   m m  ", "    bbd ", "mbbbbb  ", "bbbbbbb "],
];
const FOX: [&[&str]; 4] = [
    &["     m m", "     bbd", "w   bbb ", "bbbbbb  ", " m  m   "],
    &["     m m", "     bbd", " w  bbb ", "bbbbbb  ", "  m  m  "],
    &["    m m ", "    bbd ", "    bbw ", "wbbbbb  ", "bb m m  "],
    &["        ", "        ", "    m m ", "wbbbbbbd", "bbbbbbb "],
];
const BIRD: [&[&str]; 4] = [
    &["  bb ", " bbbd", "bmmb ", " bb  ", "  d  "],
    &["  bb ", " bbbd", "bmmb ", " bb  ", " d d "],
    &["  bb ", " bbbd", " mmb ", "bbb  ", "  d  "],
    &["     ", "     ", " bbb ", "bmmbb", " bbb "],
];

/// Draw a pet with its feet on pixel row `feet`, centred on column `x`,
/// facing `dir`; `scale` 1 in the village, 2 indoors. `t` animates the walk
/// and the tail.
#[allow(clippy::too_many_arguments)]
pub fn draw(f: &mut Frame, kind: PetKind, body: Rgb, marks: Rgb, x: f32, feet: i32, dir: f32, pose: Pose, t: f32, k: f32, collar: Option<Rgb>) {
    // (the detailed picture is drawn at exactly `k`; the pixel art, at a
    // whole number of pixels)
    let scale = (k.round() as i32).max(1);
    // the detailed sprite, where the front-end can show pictures
    if crate::frame::pictures_ok() {
        use super::sprites::{self, PetKind as K, PetPose as P};
        let sk = match kind {
            PetKind::Dog => K::Dog,
            PetKind::Cat => K::Cat,
            PetKind::Bunny => K::Bunny,
            PetKind::Reindeer => K::Reindeer,
            PetKind::Fox => K::Fox,
            PetKind::Bird => K::Bird,
        };
        let p = match pose {
            Pose::Walk => P::Walk(((t * 7.0) as i32 % 2) as u8),
            Pose::Sit => P::Sit,
            Pose::Sleep => P::Sleep,
            Pose::Up => P::Up,
            Pose::Eat => P::Eat,
        };
        let wag = pose != Pose::Sleep && (t * if pose == Pose::Up { 7.0 } else { 3.0 }) as i32 % 2 == 0;
        let light = 0.9 + 0.25 * f.day.max(0.0);
        // a bird on the move flutters along, a little off the ground
        let flying = kind == PetKind::Bird && pose == Pose::Walk;
        let p = if flying { P::Walk(((t * 12.0) as i32 % 2) as u8) } else { p };
        let pic = sprites::pet(sk, body, marks, collar, p, wag, dir, light);
        let rows = sprites::rows_of(&pic, k * size_of(kind));
        // a little hop when excited
        let hop = if pose == Pose::Up && (t * 6.0).sin() > 0.0 { 0.25 * k } else { 0.0 };
        let fly = if flying { (0.9 + 0.25 * (t * 5.0).sin()) * k } else { 0.0 };
        f.sprite(x, (feet + 1) as f32 / 2.0 - hop - fly, rows, pic);
        return;
    }
    let pose = if pose == Pose::Eat { if (t * 5.0) as i32 % 2 == 0 { Pose::Walk } else { Pose::Sit } } else { pose };
    let art = match kind {
        PetKind::Dog => &DOG,
        PetKind::Cat => &CAT,
        PetKind::Bunny => &BUNNY,
        PetKind::Reindeer => &REINDEER,
        PetKind::Fox => &FOX,
        PetKind::Bird => &BIRD,
    };
    let frame = match pose {
        Pose::Walk => ((t * 7.0) as i32 % 2) as usize,
        Pose::Sit | Pose::Up | Pose::Eat => 2,
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
                    if i == 0 && matches!(kind, PetKind::Dog | PetKind::Cat) && pose != Pose::Sleep && wag {
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
    if matches!(kind, PetKind::Dog | PetKind::Cat) && pose != Pose::Sleep && !wag {
        let ty = top + scale;
        let tx = if dir >= 0.0 { x0 - scale } else { x0 + w * scale };
        for sy in 0..scale {
            for sx in 0..scale {
                f.px_set(tx + sx, ty + sy, marks);
            }
        }
    }
}

/// A food bowl with its feet on pixel row `feet`: red, with `left` (0..1)
/// of the food still in it (kibble, or greens for a bunny).
fn draw_bowl(f: &mut Frame, kind: PetKind, x: f32, feet: i32, scale: i32, left: f32) {
    let food = match kind {
        PetKind::Bunny | PetKind::Reindeer => Rgb::hex(0x6abf4a),
        PetKind::Bird => Rgb::hex(0xe8c878),
        _ => Rgb::hex(0xb07a40),
    };
    let (red, dark) = (Rgb::hex(0xd8343c), Rgb::hex(0x8a1c24));
    // a little bowl: a narrow foot, a wider rim, the food heaped on top
    let x0 = x.round() as i32 - 2 * scale;
    let px = |f: &mut Frame, cx: i32, cy: i32, c: Rgb| {
        for sy in 0..scale {
            for sx in 0..scale {
                f.px_set(x0 + cx * scale + sx, feet - cy * scale - sy, c);
            }
        }
    };
    px(f, 1, 0, dark);
    px(f, 2, 0, dark);
    for cx in 0..4 {
        px(f, cx, 1, red);
    }
    let n = (left * 2.0).ceil() as i32;
    for cx in 1..1 + n {
        px(f, cx, 2, food);
    }
}

/// Half the drawn width, in pixels at scale 1 (for hit tests).
pub fn half_width(kind: PetKind) -> f32 {
    match kind {
        PetKind::Bunny => 3.0,
        PetKind::Bird => 2.0,
        PetKind::Reindeer => 5.0,
        PetKind::Fox => 4.0,
        _ => 3.5,
    }
}

/// How big a kind's drawn, against its picture (a reindeer's picture is
/// the grown deer's, drawn smaller: a young one).
pub fn size_of(kind: PetKind) -> f32 {
    match kind {
        PetKind::Reindeer => 0.62,
        PetKind::Fox => 0.78,
        PetKind::Bird => 0.72,
        _ => 1.0,
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
    /// Dinner: off to the bowl at that column, then eating until then
    /// (`until` is when it gives up getting there, before it starts).
    Eat { bowl: f32, until: f32, eating: bool },
    /// Held in the villager's arms until then.
    Cuddle { until: f32 },
}

/// What the pet says (in a little bubble, and heard).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Voice {
    /// Just chatting: a meow, a woof, a sniff.
    Hello,
    /// Petted, brushed, cuddled: a purr, a happy pant.
    Purr,
    /// Excited (a toy thrown, a game): a bark, a mrrp, a binky.
    Excited,
    /// At a fox or another animal: a growl, a hiss, a thump.
    Growl,
    Eat,
    Yawn,
    Snore,
}

/// The words for a pet's voice (`pick` chooses among a few).
pub fn words(kind: PetKind, v: Voice, pick: u32) -> &'static str {
    let one = |w: &[&'static str]| w[pick as usize % w.len()];
    match (kind, v) {
        (PetKind::Cat, Voice::Hello) => one(&["meow!", "mrrp?", "mew", "mrow"]),
        (PetKind::Cat, Voice::Purr) => one(&["prrrrr…", "purrr ♥", "prrrp"]),
        (PetKind::Cat, Voice::Excited) => one(&["mrrp!", "MEOW!", "ek-ek-ek"]),
        (PetKind::Cat, Voice::Growl) => one(&["hsssss!", "grrrr…", "mrrrOW!"]),
        (PetKind::Cat, Voice::Eat) => one(&["nom nom", "*crunch*", "mmmrrp"]),
        (PetKind::Cat, Voice::Yawn) => one(&["*big yawn*", "mrrm…"]),
        (PetKind::Cat, Voice::Snore) => one(&["prrr… zz", "*twitch*"]),
        (PetKind::Dog, Voice::Hello) => one(&["woof!", "arf!", "boof", "hmm?"]),
        (PetKind::Dog, Voice::Purr) => one(&["hah hah hah", "*happy pant*", "awoo ♥"]),
        (PetKind::Dog, Voice::Excited) => one(&["WOOF!", "arf arf!", "ruff!"]),
        (PetKind::Dog, Voice::Growl) => one(&["grrrr…", "RUFF!", "grr-woof!"]),
        (PetKind::Dog, Voice::Eat) => one(&["*chomp chomp*", "nom nom nom", "*slurp*"]),
        (PetKind::Dog, Voice::Yawn) => one(&["*yaaawn*", "hmmf…"]),
        (PetKind::Dog, Voice::Snore) => one(&["*snore*", "*wuf* (dreaming)"]),
        (PetKind::Bunny, Voice::Hello) => one(&["*sniff sniff*", "*nose wiggle*", "hrr?"]),
        (PetKind::Bunny, Voice::Purr) => one(&["*teeth purr*", "*happy flop*"]),
        (PetKind::Bunny, Voice::Excited) => one(&["binky!", "*zoom*", "*hop hop*"]),
        (PetKind::Bunny, Voice::Growl) => one(&["*THUMP*", "*thump thump*", "hrrr!"]),
        (PetKind::Bunny, Voice::Eat) => one(&["*munch munch*", "*crunch*"]),
        (PetKind::Bunny, Voice::Yawn) => one(&["*tiny yawn*", "*flop*"]),
        (PetKind::Bunny, Voice::Snore) => one(&["*ear twitch*", "zz…"]),
        (PetKind::Reindeer, Voice::Hello) => one(&["*snort*", "*jingle jingle*", "hmmf?"]),
        (PetKind::Reindeer, Voice::Purr) => one(&["*happy grunt*", "*nuzzle*", "*soft snort* ♥"]),
        (PetKind::Reindeer, Voice::Excited) => one(&["*prance prance*", "*JINGLE!*", "*hooves clatter*"]),
        (PetKind::Reindeer, Voice::Growl) => one(&["*antler shake*", "*stamp stamp*", "HMMF!"]),
        (PetKind::Reindeer, Voice::Eat) => one(&["*munch munch*", "*crunch* (lichen!)"]),
        (PetKind::Reindeer, Voice::Yawn) => one(&["*big reindeer yawn*", "*sleepy snort*"]),
        (PetKind::Reindeer, Voice::Snore) => one(&["*snort… zz*", "*dreams of flying*"]),
        (PetKind::Fox, Voice::Hello) => one(&["*yip!*", "*ack ack*", "hrrm?"]),
        (PetKind::Fox, Voice::Purr) => one(&["*happy chitter*", "*tail swish* ♥", "*soft whine*"]),
        (PetKind::Fox, Voice::Excited) => one(&["YIP YIP!", "*pounce!*", "*ack-ack-ack*"]),
        (PetKind::Fox, Voice::Growl) => one(&["*gekker!*", "grrr…", "*SCREAM*"]),
        (PetKind::Fox, Voice::Eat) => one(&["*chomp*", "nom nom"]),
        (PetKind::Fox, Voice::Yawn) => one(&["*squeaky yawn*", "*curls tail round*"]),
        (PetKind::Fox, Voice::Snore) => one(&["*tiny snore*", "*twitch*"]),
        (PetKind::Bird, Voice::Hello) => one(&["tweet!", "*chirp chirp*", "pip?"]),
        (PetKind::Bird, Voice::Purr) => one(&["*happy trill*", "*fluffs up* ♥", "tweedle-dee"]),
        (PetKind::Bird, Voice::Excited) => one(&["TWEET!", "*flutter flutter*", "chirrup!"]),
        (PetKind::Bird, Voice::Growl) => one(&["*angry chirp*", "tsk tsk tsk!", "*ruffles*"]),
        (PetKind::Bird, Voice::Eat) => one(&["*peck peck*", "*nibble*"]),
        (PetKind::Bird, Voice::Yawn) => one(&["*tucks head under wing*", "*sleepy peep*"]),
        (PetKind::Bird, Voice::Snore) => one(&["peep… zz", "*fluffed up*"]),
    }
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
    /// Picked up with the pointer (grab offset), or dropped and falling
    /// (speed, the row it lands on).
    pub carried: Option<(f32, f32)>,
    fall: Option<(f32, f32)>,
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
    /// The bowl, put down at this column (and row) until then; when it was
    /// last fed; a freshly brushed coat shines until then.
    pub bowl: Option<(f32, f32, f32)>,
    fed_at: f32,
    pub shine_until: f32,
    /// Not before then: something to say unprompted (or a snore).
    chat_at: f32,
    /// How big it's drawn (1 outdoors, 2 indoors), for placing its bubbles.
    k: f32,
    events: Vec<PetEv>,
    seed: u32,
}

impl MyPet {
    /// From the config (None for "none").
    pub fn new(cfg: &crate::config::PetCfg, x: f32, y: f32) -> Option<MyPet> {
        let kind = PetKind::from_str(&cfg.kind)?;
        let (body, marks) = kind.coat(&cfg.coat);
        Some(MyPet {
            carried: None,
            fall: None,
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
            bowl: None,
            fed_at: f32::NEG_INFINITY,
            shine_until: 0.0,
            chat_at: 30.0,
            k: 1.0,
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
            Some(Busy::Fetch { until, .. } | Busy::Chase { until, .. } | Busy::Zoom { until, .. } | Busy::Trick { until, .. } | Busy::Eat { until, .. } | Busy::Cuddle { until }) => *until -= s,
            _ => {}
        }
        if let Some(b) = &mut self.bowl {
            b.2 -= s;
        }
        self.fed_at -= s;
        self.shine_until -= s;
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
            PetKind::Reindeer => crate::audio::Sfx::Deer,
            PetKind::Fox => crate::audio::Sfx::Fox,
            PetKind::Bird => crate::audio::Sfx::Bird,
        }
    }

    /// Say something (in a little bubble over its head, and heard).
    pub fn voice(&mut self, v: Voice, t: f32) {
        let pick = self.roll(12);
        let w = words(self.kind, v, pick);
        self.events.push(PetEv::Say(w.to_string(), self.x, self.y - 2.0 * self.k));
        if !matches!(v, Voice::Snore | Voice::Eat) {
            let s = self.sound();
            self.events.push(PetEv::Sound(s, self.x));
        }
        self.chat_at = self.chat_at.max(t + 25.0);
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
        self.voice(Voice::Excited, t);
    }

    /// "Chase me!": zoomies round the villager.
    pub fn zoomies(&mut self, cx: f32, t: f32) {
        self.wake(t);
        self.busy = Some(Busy::Zoom { cx, until: t + 4.0 });
        self.happy_until = t + 4.0;
        self.voice(Voice::Excited, t);
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
            (PetKind::Reindeer, 0) => format!("{n} prances on the spot, bells jingling ✦"),
            (PetKind::Reindeer, 1) => format!("{n} takes a run-up… and very nearly flies"),
            (PetKind::Reindeer, 2) => format!("{n} bows, antlers and all, very grand"),
            (PetKind::Reindeer, _) => format!("{n} nudges your pocket for a carrot. smart reindeer"),
            (PetKind::Fox, 0) => format!("{n} does a mouse-pounce, nose first into the snow!"),
            (PetKind::Fox, 1) => format!("{n} chases their own tail, round and round"),
            (PetKind::Fox, 2) => format!("{n} sits up, tail curled neatly round their paws"),
            (PetKind::Fox, _) => format!("{n} rolls in the snow and comes up with a white nose"),
            (PetKind::Bird, 0) => format!("{n} sings a little carol, all on their own ♪"),
            (PetKind::Bird, 1) => format!("{n} lands on your mitten, light as a snowflake"),
            (PetKind::Bird, 2) => format!("{n} does a loop-the-loop over your head!"),
            (PetKind::Bird, _) => format!("{n} fluffs up into a perfect round ball"),
        }
    }

    /// Off after an animal at column `x` (a cat stalks first).
    pub fn chase(&mut self, x: f32, t: f32) {
        self.wake(t);
        let stalk = self.kind == PetKind::Cat;
        self.busy = Some(Busy::Chase { x, until: t + if stalk { 4.0 } else { 2.6 }, stalk });
        if !stalk || self.roll(2) == 0 {
            self.voice(Voice::Growl, t);
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

    /// Dinner: a bowl goes down at column `x` (row `y`) and the pet tucks
    /// in. Returns the line to say (it may still be full).
    pub fn feed(&mut self, x: f32, y: f32, t: f32) -> String {
        self.wake(t);
        self.toy = None;
        self.carrying = false;
        let n = self.name_or();
        let full = t - self.fed_at < 90.0;
        self.busy = Some(Busy::Eat { bowl: x, until: t + 6.0, eating: false });
        self.bowl = Some((x, y, t + 12.0));
        if full {
            return format!("{n} sniffs the bowl politely… still full from last time");
        }
        let pick = self.roll(2) as usize;
        match self.kind {
            PetKind::Dog => [format!("dinner! {n} hoovers it up, tail going the whole time"), format!("{n} sits, waits, waits… and dives in")][pick].clone(),
            PetKind::Cat => [format!("{n} eats very daintily… then looks at you for seconds"), format!("{n} picks out the best bits first, naturally")][pick].clone(),
            PetKind::Bunny => [format!("fresh greens! {n} munches, nose going a mile a minute"), format!("{n} nibbles a carrot top, ears twitching happily")][pick].clone(),
            PetKind::Reindeer => [format!("lichen and carrots! {n} crunches away, ears flicking"), format!("{n} eats like they've a sleigh to pull tonight")][pick].clone(),
            PetKind::Fox => [format!("{n} snaps it up, then checks the snow for more"), format!("{n} buries half of it for later. very foxy")][pick].clone(),
            PetKind::Bird => [format!("seeds! {n} pecks happily, one at a time"), format!("{n} picks out the sunflower seeds first, naturally")][pick].clone(),
        }
    }

    /// Picked up for a cuddle (in the villager's arms for a few seconds).
    pub fn cuddle(&mut self, t: f32) -> String {
        self.wake(t);
        self.toy = None;
        self.carrying = false;
        self.busy = Some(Busy::Cuddle { until: t + 5.0 });
        self.happy_until = t + 5.0;
        self.voice(Voice::Purr, t);
        let n = self.name_or();
        let pick = self.roll(2) as usize;
        match self.kind {
            PetKind::Dog => [format!("you scoop {n} up — a warm armful of happy, licking your chin"), format!("{n} snuggles in and lets out a big contented sigh")][pick].clone(),
            PetKind::Cat => [format!("{n} purrs against your scarf, kneading your coat"), format!("{n} puts up with the cuddle for a moment… then melts")][pick].clone(),
            PetKind::Bunny => [format!("{n} tucks into the crook of your arm, soft as new snow"), format!("{n} goes all floppy in your arms, perfectly content")][pick].clone(),
            PetKind::Reindeer => [format!("you hug {n} round the neck — warm, fuzzy, smells of pine"), format!("{n} rests their chin on your shoulder, antlers and all")][pick].clone(),
            PetKind::Fox => [format!("{n} curls round your neck like a warm scarf"), format!("{n} chitters happily and tucks their nose under your chin")][pick].clone(),
            PetKind::Bird => [format!("{n} snuggles into your cupped mittens, a tiny warm ball"), format!("{n} sits on your shoulder and fluffs up against your cheek")][pick].clone(),
        }
    }

    /// Nap time: curls up right where it is (it wakes when the villager walks off).
    pub fn nap(&mut self, t: f32) -> String {
        self.busy = None;
        self.carrying = false;
        self.napping = true;
        self.still_since = t;
        self.voice(Voice::Yawn, t);
        let n = self.name_or();
        match self.kind {
            PetKind::Dog => format!("{n} turns round three times and flops down. sweet dreams"),
            PetKind::Cat => format!("{n} curls up into a perfect little loaf and closes their eyes"),
            PetKind::Bunny => format!("{n} does a sleepy flop onto their side. out like a light"),
            PetKind::Reindeer => format!("{n} folds their legs under and settles in the snow. dreaming of flying"),
            PetKind::Fox => format!("{n} curls up, tail over nose, a little orange swirl"),
            PetKind::Bird => format!("{n} tucks their head under a wing and fluffs right up"),
        }
    }

    /// A good brush: the coat shines for a while.
    pub fn brush(&mut self, t: f32) -> String {
        self.wake(t);
        self.shine_until = t + 12.0;
        self.happy_until = t + 2.0;
        self.events.push(PetEv::Emote('✦', Rgb::hex(0xfff1a0), self.x - 1.5, self.y - 2.0));
        self.events.push(PetEv::Emote('✧', Rgb::hex(0xfff1a0), self.x + 1.5, self.y - 3.0));
        self.events.push(PetEv::Sound(crate::audio::Sfx::Sparkle, self.x));
        self.voice(Voice::Purr, t);
        let n = self.name_or();
        match self.kind {
            PetKind::Dog => format!("you brush {n} till their coat shines like a bauble ✦"),
            PetKind::Cat => format!("{n} leans into every stroke of the brush, purring loudly ✦"),
            PetKind::Bunny => format!("a gentle brush, and {n} is the fluffiest bunny in the village ✦"),
            PetKind::Reindeer => format!("you brush {n} till their coat gleams. sleigh-ready ✦"),
            PetKind::Fox => format!("a brush, and {n}'s tail is the fluffiest thing in the village ✦"),
            PetKind::Bird => format!("you smooth {n}'s feathers with a fingertip. they shine ✦"),
        }
    }

    /// Picked up with the pointer, held `(ox, oy)` from where it's grabbed.
    pub fn carry(&mut self, ox: f32, oy: f32, t: f32) {
        self.carried = Some((ox, oy));
        self.fall = None;
        self.busy = None;
        self.napping = false;
        self.walking = false;
        self.sniff = None;
        self.happy_until = t + 1e6;
    }

    /// Carried along to (x, y) (where the pointer is).
    pub fn carry_to(&mut self, x: f32, y: f32) {
        if let Some((ox, oy)) = self.carried {
            self.x = x - ox;
            self.y = y - oy;
        }
    }

    /// Let go: down onto row `land` (from up high, a little drop).
    pub fn put_down(&mut self, land: f32, t: f32) {
        if self.carried.take().is_some() {
            self.happy_until = t + 1.2;
            self.still_since = t;
            if self.y < land - 0.5 {
                self.fall = Some((0.0, land));
            } else {
                self.y = land;
            }
        }
    }

    /// In the villager's arms right now (drawn in front of them).
    pub fn held(&self) -> bool {
        matches!(self.busy, Some(Busy::Cuddle { .. }))
    }

    /// Asleep now?
    pub fn asleep(&self) -> bool {
        self.napping
    }

    /// Free to start something new?
    pub fn idle(&self) -> bool {
        self.busy.is_none() && !self.staying
    }

    /// Follow `(px, py)` (facing `pdir`, `moving`), keeping to rows lo..=hi;
    /// `k` scales distances (2 indoors, where everything's drawn bigger).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, t: f32, px: f32, py: f32, pdir: f32, moving: bool, lo: f32, hi: f32, k: f32) {
        // in the pointer's grip: dangling, happily
        if self.carried.is_some() {
            self.walking = false;
            return;
        }
        // let go of up high: down it comes, onto its feet
        if let Some((vy, land)) = self.fall {
            let vy = vy + 70.0 * dt;
            self.y += vy * dt;
            if self.y >= land {
                self.y = land;
                self.fall = None;
                self.happy_until = t + 1.0;
                self.voice(Voice::Hello, t);
            } else {
                self.fall = Some((vy, land));
            }
            return;
        }
        if moving {
            self.still_since = t;
            self.sniff = None;
            self.napping = false;
        }
        self.update_toy(dt, t, k);
        self.k = k;
        // now and then, something to say (or a snore, asleep)
        if t > self.chat_at {
            self.chat_at = t + 40.0 + self.roll(50) as f32;
            if self.napping {
                self.voice(Voice::Snore, t);
            } else if self.busy.is_none() {
                self.voice(Voice::Hello, t);
            }
        }
        if self.bowl.is_some_and(|b| t > b.2) {
            self.bowl = None;
        }
        // in the villager's arms: held, hearts now and then; walking off
        // (or time up) puts it down
        if let Some(Busy::Cuddle { until }) = self.busy {
            if moving || t > until {
                self.busy = None;
                self.happy_until = t + 1.5;
                self.x = px - pdir * 2.5 * k;
            } else {
                self.x = px;
                self.y = py;
                self.dir = pdir;
                self.walking = false;
                if self.roll(100) < (dt * 120.0) as u32 {
                    self.events.push(PetEv::Emote('♥', Rgb::hex(0xff6b8a), self.x, self.y - 5.0 * k));
                }
                return;
            }
        }
        // a good long while standing about: a nap (wakes when they move off)
        if !moving && self.busy.is_none() && self.sniff.is_none() && t - self.still_since > 22.0 + (self.seed % 9) as f32 {
            if !self.napping {
                self.napping = true;
                self.voice(Voice::Yawn, t);
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
            Some(Busy::Eat { bowl, until, eating }) => {
                // stand beside the bowl, facing it
                let side = if self.x <= bowl { -1.0 } else { 1.0 };
                let spot = bowl + side * 2.5 * k;
                if !eating && (self.x - spot).abs() < 0.8 * k {
                    let full = t - self.fed_at < 90.0;
                    if !full {
                        self.voice(Voice::Eat, t);
                    }
                    self.busy = Some(Busy::Eat { bowl, until: t + if full { 1.2 } else { 4.0 }, eating: true });
                    if let Some(b) = &mut self.bowl {
                        b.2 = t + if full { 3.0 } else { 6.0 };
                    }
                } else if t > until {
                    self.busy = None;
                    if eating {
                        if t - self.fed_at >= 90.0 {
                            self.fed_at = t;
                            self.happy_until = t + 2.0;
                            self.events.push(PetEv::Emote('♥', Rgb::hex(0xff6b8a), self.x, self.y - 2.0 * k));
                            let s = self.sound();
                            self.events.push(PetEv::Sound(s, self.x));
                        }
                        if let Some(b) = &mut self.bowl {
                            b.2 = t + 2.0;
                        }
                    }
                } else if eating && self.roll(100) < (dt * 180.0) as u32 {
                    self.events.push(PetEv::Sound(crate::audio::Sfx::Munch, self.x));
                    let jitter = self.roll(3) as f32 - 1.0;
                    self.events.push(PetEv::Emote('·', Rgb::hex(0xc8a070), bowl + jitter, self.y - 1.0));
                }
                if eating {
                    self.dir = -side;
                }
                ty = self.bowl.map(|b| b.1).unwrap_or(py);
                if eating { self.x } else { spot }
            }
            Some(Busy::Cuddle { .. }) => self.x,
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
            PetKind::Bunny | PetKind::Reindeer | PetKind::Fox | PetKind::Bird => {
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

    /// Drawn `scale` times bigger, where it is or through a zoomed `view`.
    /// Drawn at size `k` (the village's sprite size; 2 indoors).
    pub fn draw(&self, f: &mut Frame, t: f32, k: f32, view: Option<super::View>) {
        let scale = super::critters::pet_scale(k);
        let (sx, sy) = match view {
            Some(v) => (v.x(self.x), v.y(self.y + 1.0) - 1.0),
            None => (self.x, self.y),
        };
        let tricking = match self.busy {
            Some(Busy::Trick { which, until }) => Some((which, until)),
            _ => None,
        };
        let held = self.held();
        let eating = matches!(self.busy, Some(Busy::Eat { eating: true, .. }));
        let pose = if self.napping || held {
            Pose::Sleep
        } else if eating {
            Pose::Eat
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
        let feet = (sy * 2.0).round() as i32 + 1;
        // the bowl, on the ground (emptying as it eats)
        if let Some((bx, by, _)) = self.bowl {
            let (bxs, bys) = match view {
                Some(v) => (v.x(bx), v.y(by + 1.0) - 1.0),
                None => (bx, by),
            };
            let left = match self.busy {
                Some(Busy::Eat { eating: true, until, .. }) => ((until - t) / 4.0).clamp(0.0, 1.0),
                Some(Busy::Eat { eating: false, .. }) => 1.0,
                _ => 0.0,
            };
            draw_bowl(f, self.kind, bxs, (bys * 2.0).round() as i32 + 1, scale, left);
        }
        // held: curled up at the villager's chest
        // (the villager isn't drawn as much bigger as the pet is when zoomed in)
        let feet = if !held {
            feet
        } else if crate::frame::pictures_ok() {
            // (the detailed villager's chest is a little lower)
            feet - 2 * scale
        } else {
            feet - (1 + 2 * scale)
        };
        // (one brightness while it shines: each shade is a sprite of its own)
        let shine = if t < self.shine_until { 1.3 } else { 1.0 };
        draw(f, self.kind, self.body * shine, self.marks * shine, sx, feet, dir, pose, t, k, Some(Rgb::hex(0x2fae5c)));
        if t < self.shine_until && (t * 2.3).fract() < 0.35 {
            let k = (t * 7.0) as i32;
            f.put_g(sx.round() as i32 + (k % 5 - 2) * scale, (feet - 4 * scale) / 2, crate::frame::Glyph::new('✦', Rgb::hex(0xfff1a0)).lit(0.0));
        }
        if self.napping {
            // z's drifting up, one after another
            let ph = (t * 0.6).fract();
            let (zx, zy) = (sx.round() as i32 + 2 * scale + (ph * 2.0) as i32, (feet - 6 * scale) / 2 - (ph * 3.0) as i32);
            let c = Rgb::hex(0xdfe7f4) * (1.0 - ph * 0.6);
            f.put_g(zx, zy, crate::frame::Glyph::new(if ph < 0.5 { 'z' } else { 'Z' }, c).lit(0.0));
            if ph > 0.35 {
                f.put_g(zx - 1, zy + 2, crate::frame::Glyph::new('z', Rgb::hex(0xdfe7f4) * 0.6).lit(0.0));
            }
        }
        // not named yet: a little "?" asking for one
        if self.name.is_empty() && (t * 1.5) as i32 % 2 == 0 {
            f.put_g(sx.round() as i32, (feet - 6 * scale) / 2, crate::frame::Glyph::new('?', Rgb::hex(0xffd36b)).lit(0.0));
        }
        if let Some(toy) = &self.toy {
            match view {
                Some(v) => {
                    let mut t2 = *toy;
                    t2.x = v.x(toy.x);
                    t2.y = v.y(toy.y + 1.0) - 1.0;
                    t2.h = toy.h * v.z;
                    t2.trail = toy.trail.map(|x| v.x(x));
                    draw_toy(f, &t2, scale);
                }
                None => draw_toy(f, toy, scale),
            }
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
