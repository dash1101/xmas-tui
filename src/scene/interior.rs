//! Inside a house (or the church, or the villager's own home). Walk up to a
//! front door in the village and press E: the view becomes a cozy room.
//!
//! Every house is its own: a household with a theme (the baker's kitchen,
//! the toymaker's workbench, the bookworm's shelves, the musician's piano,
//! the sailor's telescope, the candle-maker's glow, the farmer's rocking
//! chair), its own wallpaper, floor and furniture layout, one to three
//! people who potter about — warming their hands by the fire, sitting down,
//! looking out of the window — and often a cat or a dog. Chairs can be sat
//! in, the oven bakes, books get read, the tree takes another ornament, and
//! looking out of a window shows the real village outside, from that spot.
//!
//! The villager's own home has several rooms on three floors: the parlour,
//! the kitchen, a bedroom upstairs and an attic with a round window and a
//! telescope. Each window looks out from its own height.
//!
//! The room's static parts are drawn once into a layer; the fire, snow,
//! steam, candles, people and pets animate on top each frame. Speech bubbles
//! and sparkles use a small indoor `Director` of their own.

use super::events::{Ctx, Director};
use super::pets::{self, PetKind, Pose};
use super::player::Player;
use super::shop::Item;
use super::story::{temper_of, Temper};
use super::tree::{LightStyle, Tree};
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Frame, Glyph, Layer};
use crate::rng::{h01, hash, hash2, hash3, noise1, Rng};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Thing {
    Door,
    Fire,
    Tree,
    /// Someone at home (index into the residents).
    Person(usize),
    Pet,
    Cocoa,
    Window(usize),
    Seat(usize),
    Piano,
    Gramophone,
    Radio,
    Clock,
    Shelf,
    Organ,
    Candles,
    Oven,
    CookieJar,
    Workbench,
    Telescope,
    Photo,
    Stocking,
    Bed,
    MusicBox,
    Boxes,
    /// Stairs (true: going up) and doorways (true: to the right).
    Stairs(bool),
    Doorway(bool),
}

/// What poking at something leads to (for the scene to act on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    None,
    /// Out the front door.
    Leave,
    /// Talk with someone at home.
    Talk(usize),
    /// Look out of a window (or through the telescope).
    Look(Look),
    /// Something for the pockets.
    Gift(Item),
    /// Several of something (a tray of cookies).
    Gifts(Item, u32),
    /// Warm and cozy (cocoa, the fire).
    Cozy,
    /// A little job done for the host, or something nice: cheer.
    Cheer(u32),
}

/// A view out of a window: how far along the village it looks (screens,
/// relative to the house), how high (rows lifted into the sky), and the
/// shape of the opening on screen (fractions of the screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub dx: f32,
    pub lift: f32,
    pub hole: Hole,
    pub telescope: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hole {
    /// x0, y0, x1, y1
    Rect(f32, f32, f32, f32),
    /// centre x, centre y, radius (fraction of the screen height)
    Round(f32, f32, f32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Theme {
    Baker,
    Toymaker,
    Bookworm,
    Musician,
    Sailor,
    Candles,
    Farmer,
}

const THEMES: [Theme; 7] = [Theme::Baker, Theme::Toymaker, Theme::Bookworm, Theme::Musician, Theme::Sailor, Theme::Candles, Theme::Farmer];


#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RoomKind {
    Parlour,
    Kitchen,
    Bedroom,
    Attic,
    Church,
}

impl RoomKind {
    pub fn title(self) -> &'static str {
        match self {
            RoomKind::Parlour => "the parlour",
            RoomKind::Kitchen => "the kitchen",
            RoomKind::Bedroom => "the bedroom",
            RoomKind::Attic => "the attic",
            RoomKind::Church => "the church",
        }
    }
}

/// A little job a host asked for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Task {
    /// Put the star back on the tree.
    Star,
    /// Another log on the fire.
    Log,
    /// Find the cat / dog.
    FindPet,
    /// Play a tune on the piano.
    Tune,
    /// Hang the last stocking.
    Stocking,
}

impl Task {
    pub fn done_by(self, what: Thing) -> bool {
        matches!(
            (self, what),
            (Task::Star, Thing::Tree) | (Task::Log, Thing::Fire) | (Task::FindPet, Thing::Pet) | (Task::Tune, Thing::Piano) | (Task::Stocking, Thing::Stocking) | (Task::Stocking, Thing::Fire)
        )
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Doing {
    Stand,
    Walk,
    Sit(usize),
    Warm,
    Look,
    Busy,
    Play,
    Talk,
}

/// Someone at home.
pub struct Resident {
    pub name: &'static str,
    pub kid: bool,
    pub temper: Temper,
    coat: Rgb,
    hair: Rgb,
    x: f32,
    feet: i32,
    target: f32,
    then: Doing,
    doing: Doing,
    until: f32,
    dir: f32,
}

struct Seat {
    x: i32,
    /// Cell row a sitter's feet rest on.
    feet: i32,
    /// Pixel row of the cushion (where a sitter's hips go).
    top: i32,
    /// Who's in it: Some(None) the villager, Some(Some(i)) a resident.
    by: Option<Option<usize>>,
}

/// Walking between floors of the home.
struct Climb {
    start: f32,
    dur: f32,
    /// Where the new room sits, in rows from the old one (-h: above).
    origin: f32,
    /// The villager's way (centre column, feet row), in the old room's
    /// rows, and the distance along it to each point.
    path: Vec<(f32, f32)>,
    lens: Vec<f32>,
    /// The rows over which the view pans across.
    cam: (f32, f32),
    /// The room left behind, as it looked (pixels and light).
    snap_px: Vec<Rgb>,
    snap_light: Vec<Rgb>,
    snap_glyph: Vec<Option<crate::frame::Glyph>>,
}

impl Climb {
    /// (column, row, facing, progress 0..1) at time t.
    fn at(&self, t: f32) -> (f32, f32, f32, f32) {
        let u = ((t - self.start) / self.dur).clamp(0.0, 1.0);
        let total = *self.lens.last().unwrap_or(&1.0);
        let d = u * total;
        let i = self.lens.iter().rposition(|&l| l <= d).unwrap_or(0).min(self.path.len() - 2);
        let (a, b) = (self.path[i], self.path[i + 1]);
        let seg = (self.lens[i + 1] - self.lens[i]).max(1e-3);
        let k = ((d - self.lens[i]) / seg).clamp(0.0, 1.0);
        let dir = if (b.0 - a.0).abs() > 0.5 { (b.0 - a.0).signum() } else { 1.0 };
        (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, dir, u)
    }

    /// How far the view has moved (rows; 0 → origin) with the villager at
    /// row `y`.
    fn view(&self, y: f32) -> f32 {
        let (y0, y1) = self.cam;
        let f = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
        let f = ((f - 0.1) / 0.8).clamp(0.0, 1.0);
        self.origin * f * f * (3.0 - 2.0 * f)
    }
}

/// The house cat or dog (or the villager's own pet at home).
pub struct HousePet {
    pub kind: PetKind,
    pub body: Rgb,
    pub marks: Rgb,
    pub name: String,
    x: f32,
    feet: i32,
    target: f32,
    pose: Pose,
    until: f32,
    happy_until: f32,
    /// A ball rolling across the floor for it to chase: (x, velocity).
    ball: Option<(f32, f32)>,
    hiding: bool,
}

/// Everything about a household that's the same each visit.
pub struct Profile {
    pub theme: Theme,
    wall: Rgb,
    pattern: u8,
    floor_style: u8,
    rug: Rgb,
    curtain: Rgb,
    layout: u8,
    pub years: u32,
}

impl Profile {
    fn new(seed: u32, tint: Rgb) -> Self {
        let h = |k: u32| hash(seed ^ k.wrapping_mul(0x9e37_79b9));
        let walls = [0x7a4a3a, 0x5a7a5a, 0x7a3a44, 0x3a4a6a, 0xb8a888, 0x6a4a6a, 0x3a6a6a, 0x9a7a3a];
        let rugs = [0x8a2030, 0x2a5a3a, 0x2a3a6a, 0x8a6a2a, 0x5a2a5a];
        let curtains = [0xb8202a, 0x2a6a3a, 0xc8a040, 0x4a5aa8, 0x8a3a6a, 0xe0d0b0];
        Profile {
            theme: THEMES[(h(1) % THEMES.len() as u32) as usize],
            wall: Rgb::hex(walls[(h(2) % walls.len() as u32) as usize]).lerp(tint, 0.25),
            pattern: (h(3) % 5) as u8,
            floor_style: (h(4) % 3) as u8,
            rug: Rgb::hex(rugs[(h(5) % rugs.len() as u32) as usize]),
            curtain: Rgb::hex(curtains[(h(6) % curtains.len() as u32) as usize]),
            layout: (h(7) % 3) as u8,
            years: 3 + h(8) % 46,
        }
    }
}

const HOST_HI: &[&str] = &[
    "come in, come in! warm yourself by the fire",
    "oh! a visitor! the kettle's just boiled",
    "merry Christmas! mind the {pet}",
    "we just finished the tree — what do you think?",
    "did you knock? I couldn't hear over the carols",
    "shoes off, snow stays outside! *laughs*",
];
const CHURCH_HI: &[&str] = &[
    "welcome! choir practice is at seven",
    "isn't the glass lovely by candlelight?",
    "come in out of the cold, friend",
    "peace be with you — and merry Christmas",
];
const FIRE: &[&str] = &["toasty!", "ahh, that's the stuff", "my toes are thawing", "*crackle* *pop*", "I could stay here all night"];
const COCOA: &[&str] = &["mmm, cocoa!", "marshmallows, too!", "careful, hot!", "best cocoa in town"];
const BOOKS: &[&str] = &[
    "\"…and the snow fell softly on the sleeping town, as if it too were waiting for morning.\"",
    "\"Chapter One: in which a reindeer gets thoroughly lost.\" — a classic",
    "a cookbook: 101 kinds of fudge. page 37 is smudged with chocolate",
    "a book of carols, dog-eared at \"Silent Night\"",
    "\"The Lighthouse Keeper's Christmas\". someone pressed a holly leaf in it",
    "a photo album: a much younger snowman, the same hat",
    "\"How to Knit a Scarf (Not a Sock)\". there's a sock tucked inside",
    "a star atlas, with the North Star circled twice",
];
const PHOTOS: &[&str] = &[
    "that's us on our wedding day. it snowed then, too",
    "my grandmother, the year the river froze. she skated to church!",
    "that's {kid} with their first snowman. it was mostly carrot",
    "the whole family, the Christmas the lights blew the fuse",
    "my old dog Pudding. best boy there ever was",
];
const CLOCK: &[&str] = &["tick… tock…", "almost time for cocoa", "it chimes on the hour", "it's a few minutes fast. on purpose"];
const CANDLES: &[&str] = &["a quiet moment", "the candles flicker warmly", "a little wish ✦"];
const BOXES: &[&str] = &["old ornaments! a glass bird, a tin star…", "a box marked \"LIGHTS (TANGLED)\"", "a wooden sled, a bit too small now", "a stack of old Christmas cards. lovely"];
const SIT: &[&str] = &["ahh. comfy", "*sinks into the cushions*", "I could get used to this", "so cozy…"];

pub struct Interior {
    pub house: usize,
    pub church: bool,
    /// The villager's own home (several rooms).
    pub home: bool,
    w: i32,
    h: i32,
    /// First floor pixel row.
    floor: i32,
    seed: u32,
    pub profile: Profile,
    pub rooms: Vec<RoomKind>,
    pub room_i: usize,
    room: Layer,
    /// (what, centre column, top row) — for the [E] prompt and interacting.
    things: Vec<(Thing, i32, i32)>,
    /// Window openings (pixel rect) and the view out of each.
    windows: Vec<((i32, i32, i32, i32), Look)>,
    /// Fireplace opening (pixel rect).
    hearth: Option<(i32, i32, i32, i32)>,
    tree: Option<Tree>,
    pub residents: Vec<Resident>,
    pub pet: Option<HousePet>,
    seats: Vec<Seat>,
    /// Where people like to be: (column, what they do there).
    spots: Vec<(f32, Doing)>,
    mug: Option<(i32, i32)>,
    clock: Option<(i32, i32)>,
    candles: Vec<(i32, i32)>,
    oven: Option<(i32, i32)>,
    baking: Option<f32>,
    toy_train: Option<(i32, i32)>,
    /// Fairy lights strung along the walls: (x, y, colour, phase).
    bulbs: Vec<(i32, i32, Rgb, f32)>,
    /// Lamps that glow: (x, y).
    lamps: Vec<(i32, i32)>,
    pub director: Director,
    ctx: Ctx,
    rng: Rng,
    /// Where the player stood outside (world column, row), to step back out.
    pub outside: (f32, f32),
    music_until: f32,
    nap_until: f32,
    greeted: bool,
    /// The villager is sitting in this seat.
    you_sit: Option<usize>,
    /// Tucked up in bed (since when).
    you_lie: Option<f32>,
    /// The bed in this room: (left pixel column, floor row).
    bed: Option<(i32, i32)>,
    /// A tray of cookies out of the oven, waiting to be taken.
    cookies_ready: bool,
    /// Walking up (or down) the stairs between floors of the home.
    climbing: Option<Climb>,
    /// A job someone asked for, and who.
    pub task: Option<(Task, usize)>,
    stocking_hung: bool,
    star_off: bool,
    stocking_found: bool,
    ornaments: u32,
}

impl Interior {
    #[allow(clippy::too_many_arguments)]
    pub fn new(house: usize, church: bool, home: bool, wall_tint: Rgb, w: usize, h: usize, seed: u32, outside: (f32, f32), t: f32) -> Self {
        let (w, h) = (w as i32, h as i32);
        let ph = h * 2;
        let floor = (ph as f32 * 0.70) as i32;
        let mut rng = Rng::new(seed as u64 ^ 0x1a51de);
        let base_row = floor / 2 + 1;
        let mut director = Director::new(seed, 0.0);
        director.indoor = true;
        let ctx = Ctx {
            w,
            h,
            ground_row: vec![base_row; w.max(1) as usize],
            track_row: h - 1,
            occluded: vec![false; (w * h).max(1) as usize],
            church: None,
            sky_rows: 1,
            station: None,
            cam: 0.0,
            vw: w,
        };
        let profile = Profile::new(seed, wall_tint);
        let rooms = if church {
            vec![RoomKind::Church]
        } else if home {
            vec![RoomKind::Parlour, RoomKind::Kitchen, RoomKind::Bedroom, RoomKind::Attic]
        } else {
            vec![RoomKind::Parlour]
        };
        // the household
        let mut residents = vec![];
        if !home {
            let coats = [0xb3202a, 0x1f6e3a, 0x2a4c8a, 0x6a3a8a, 0xc8862a, 0x8a5a2a, 0x3a6a6a];
            let hair = [0x3a2a1a, 0x8a5a2a, 0xd8d0c0, 0x1a1a1a, 0xa8462a, 0xe0c080];
            let n = if church { 1 } else { 1 + rng.chance(0.5) as usize + rng.chance(0.4) as usize };
            for k in 0..n {
                let kid = k == 2 || (k == 1 && rng.chance(0.3));
                let id = hash(seed ^ (k as u32 + 1) * 7919) as u64;
                residents.push(Resident {
                    name: super::dialogue::name(&mut rng, kid),
                    kid,
                    temper: temper_of(id, kid),
                    coat: Rgb::hex(*rng.pick(&coats)),
                    hair: Rgb::hex(*rng.pick(&hair)),
                    x: w as f32 * (0.3 + 0.2 * k as f32),
                    feet: floor / 2 + 5 + (k as i32 % 2) * 2,
                    target: 0.0,
                    then: Doing::Stand,
                    doing: Doing::Stand,
                    until: t + 1.0 + k as f32 * 2.0,
                    dir: -1.0,
                });
            }
        }
        // a cat or a dog (most homes have one)
        let pet = if church || home {
            None
        } else {
            let r = rng.f();
            let kind = if r < 0.45 {
                Some(PetKind::Cat)
            } else if r < 0.75 {
                Some(PetKind::Dog)
            } else {
                None
            };
            kind.map(|kind| {
                let coats = kind.coats();
                let (_, b, m) = coats[rng.int(0, coats.len() as i32) as usize];
                HousePet {
                    kind,
                    body: Rgb::hex(b),
                    marks: Rgb::hex(m),
                    name: rng.pick(pets::NAMES).to_string(),
                    x: w as f32 * 0.45,
                    feet: (floor + (ph - floor) * 42 / 100) / 2,
                    target: w as f32 * 0.45,
                    pose: Pose::Sleep,
                    until: t + rng.range(8.0, 20.0),
                    happy_until: 0.0,
                    ball: None,
                    hiding: false,
                }
            })
        };
        let mut room = Interior {
            house,
            church,
            home,
            w,
            h,
            floor,
            seed,
            profile,
            rooms,
            room_i: 0,
            room: Layer::new(w.max(1) as usize, ph.max(1) as usize),
            things: vec![],
            windows: vec![],
            hearth: None,
            tree: None,
            residents,
            pet,
            seats: vec![],
            spots: vec![],
            mug: None,
            clock: None,
            candles: vec![],
            oven: None,
            baking: None,
            toy_train: None,
            bulbs: vec![],
            lamps: vec![],
            director,
            ctx,
            rng,
            outside,
            music_until: 0.0,
            nap_until: 0.0,
            greeted: false,
            you_sit: None,
            you_lie: None,
            bed: None,
            cookies_ready: false,
            climbing: None,
            task: None,
            stocking_hung: false,
            star_off: false,
            stocking_found: false,
            ornaments: 0,
        };
        room.build();
        room
    }

    pub fn kind(&self) -> RoomKind {
        self.rooms[self.room_i]
    }

    /// Where the player appears: just inside the door.
    pub fn entrance(&self) -> (f32, f32) {
        (17.0, (self.floor / 2 + 8) as f32)
    }

    /// The rows people can walk on.
    pub fn walk_rows(&self) -> (f32, f32) {
        ((self.floor / 2 + 5) as f32, (self.h - 1) as f32)
    }

    pub fn rebase(&mut self, s: f32) {
        self.director.rebase(s);
        if let Some(tr) = &mut self.tree {
            tr.rebase(s);
        }
        self.music_until -= s;
        self.nap_until -= s;
        if let Some(b) = &mut self.baking {
            *b -= s;
        }
        for r in &mut self.residents {
            r.until -= s;
        }
        if let Some(p) = &mut self.pet {
            p.until -= s;
            p.happy_until -= s;
        }
    }

    // ---------------------------------------------------------------- build

    fn build(&mut self) {
        self.room = Layer::new(self.w.max(1) as usize, (self.h * 2).max(1) as usize);
        self.things.clear();
        self.windows.clear();
        self.hearth = None;
        self.tree = None;
        self.seats.clear();
        self.spots.clear();
        self.mug = None;
        self.clock = None;
        self.candles.clear();
        self.oven = None;
        self.toy_train = None;
        self.bulbs.clear();
        self.lamps.clear();
        self.you_sit = None;
        self.you_lie = None;
        self.bed = None;
        let kind = self.kind();
        self.build_shell(kind);
        match kind {
            RoomKind::Church => self.build_church(),
            RoomKind::Parlour => self.build_parlour(),
            RoomKind::Kitchen => self.build_kitchen(),
            RoomKind::Bedroom => self.build_bedroom(),
            RoomKind::Attic => self.build_attic(),
        }
        if kind != RoomKind::Church {
            self.decorate(kind);
        }
        for (i, r) in self.residents.iter().enumerate() {
            self.things.push((Thing::Person(i), r.x as i32 + 1, r.feet - 10));
        }
        // the household goes about its evening from where it stands
        let n = self.residents.len();
        for i in 0..n {
            let x = self.residents[i].x;
            self.residents[i].target = x;
        }
    }

    /// Fill the room: a garland of lights along the top of the walls, cards
    /// on a string, and in the gaps between the furniture lamps, plants,
    /// side tables, a dresser, more pictures (and a coat rack by the door).
    fn decorate(&mut self, kind: RoomKind) {
        let (w, floor) = (self.w, self.floor);
        let seed = self.seed ^ (self.room_i as u32 * 977);
        // what's already standing along the wall (and the windows)
        let mut taken: Vec<(i32, i32)> = self.things.iter().map(|t| (t.1 - 13, t.1 + 13)).collect();
        taken.extend(self.windows.iter().map(|((x0, _, ww, _), _)| (x0 - 8, x0 + ww + 8)));
        taken.push((0, 30));
        taken.push((w - 12, w));
        // the big built-in pieces of the home's rooms
        let fx = |f: f32| (w as f32 * f) as i32;
        match kind {
            RoomKind::Parlour if self.home => taken.push((fx(0.86) - 20, fx(0.86) + 32)),
            RoomKind::Kitchen => taken.push((fx(0.28), fx(0.72))),
            RoomKind::Bedroom => {
                taken.push((fx(0.30) - 4, fx(0.30) + 48));
                taken.push((fx(0.90) - 10, fx(0.90) + 10));
                taken.push((fx(0.10) - 14, fx(0.10) + 16));
            }
            RoomKind::Attic => taken.push((fx(0.22), fx(0.36))),
            _ => {}
        }
        let free = |a: i32, b: i32, taken: &Vec<(i32, i32)>| !taken.iter().any(|&(x0, x1)| b > x0 && a < x1);
        // the garland, swagging between pins along the top of the wall
        if kind != RoomKind::Attic {
            let span = 34;
            let cols = [Rgb::hex(0xff4a4a), Rgb::hex(0xffd36b), Rgb::hex(0x4ad06a), Rgb::hex(0x4aa0ff), Rgb::hex(0xff8ad8)];
            let mut x = 6;
            while x + span < w {
                for k in 0..=span {
                    let u = k as f32 / span as f32;
                    let y = 5 + ((u * std::f32::consts::PI).sin() * 6.0) as i32;
                    self.room.set(x + k, y, Rgb::hex(0x1f5a32) * (0.8 + 0.3 * h01(hash2(x + k, 4))));
                    self.room.set(x + k, y + 1, Rgb::hex(0x174a28));
                    if k % 5 == 2 {
                        self.bulbs.push((x + k, y + 2, cols[((x + k) / 5 % cols.len() as i32) as usize], h01(hash2(x + k, seed as i32)) * 6.3));
                    }
                }
                x += span;
            }
        }
        // Christmas cards on a string over the fireplace
        if let Some((ox, oy, ow, _)) = self.hearth {
            let y0 = oy - 22;
            for k in 0..ow + 16 {
                let x = ox - 8 + k;
                let sag = ((k as f32 / (ow + 16) as f32 * std::f32::consts::PI).sin() * 3.0) as i32;
                self.room.set(x, y0 + sag, Rgb::hex(0xd8d0c0) * 0.7);
                if k % 6 == 3 {
                    let c = [0xc8202a, 0x2a6a3a, 0xf0ece4, 0x2a4a8a][((k / 6) % 4) as usize];
                    for dy in 1..5 {
                        for dx in 0..3 {
                            self.room.set(x + dx - 1, y0 + sag + dy, Rgb::hex(c) * (0.9 + 0.1 * dx as f32));
                        }
                    }
                }
            }
        }
        // a coat rack by the front door, a coat and scarf on it
        if matches!(kind, RoomKind::Parlour) {
            let cx = 28;
            for y in floor - 30..floor {
                self.room.set(cx, y, Rgb::hex(0x4a3020));
            }
            for (dx, c) in [(-2, 0x6a3a8a), (2, 0x2a5aa8)] {
                for y in floor - 28..floor - 16 {
                    self.room.set(cx + dx, y, Rgb::hex(c));
                    self.room.set(cx + dx + dx.signum(), y + 2, Rgb::hex(c) * 0.8);
                }
            }
            for y in floor - 28..floor - 20 {
                self.room.set(cx + 1, y, Rgb::hex(0xc8202a));
            }
        }
        // the gaps: something in each
        let wtop = (floor as f32 * 0.22) as i32;
        let mut x = 34;
        let mut k = 0u32;
        while x < w - 30 {
            let item = (hash(seed ^ k.wrapping_mul(2654435761)) % 6) as u8;
            let width = [10, 8, 12, 20, 12, 12][item as usize];
            if free(x, x + width, &taken) {
                self.decor(item, x, kind, wtop);
                taken.push((x - 6, x + width + 6));
                x += width + 10;
                k += 1;
            } else {
                x += 4;
            }
        }
    }

    /// One piece of decor at column `x`: 0 floor lamp, 1 potted plant, 2
    /// side table with a lamp, 3 dresser, 4 picture, 5 wall shelf.
    fn decor(&mut self, item: u8, x: i32, kind: RoomKind, wtop: i32) {
        let floor = self.floor;
        let wood = Rgb::hex(0x5a3a24);
        let l = &mut self.room;
        match item {
            0 => {
                for y in floor - 26..floor {
                    l.set(x + 4, y, Rgb::hex(0x2a2a2a));
                }
                for xx in x + 2..x + 7 {
                    l.set(xx, floor - 1, Rgb::hex(0x2a2a2a));
                }
                for y in floor - 32..floor - 26 {
                    let r = (y - floor + 32) / 2 + 2;
                    for xx in x + 4 - r..=x + 4 + r {
                        l.set(xx, y, Rgb::hex(0xe8d0a0) * 0.9);
                    }
                }
                self.lamps.push((x + 4, floor - 27));
            }
            1 => {
                for y in floor - 6..floor {
                    for xx in x + 1..x + 7 {
                        l.set(xx, y, Rgb::hex(0xa8502a) * (0.8 + 0.05 * (y - floor + 6) as f32));
                    }
                }
                for k in 0..22 {
                    let (dx, dy) = ((h01(hash2(k, x)) * 10.0) as i32 - 1, (h01(hash2(k, x + 1)) * 12.0) as i32);
                    l.set(x + dx, floor - 7 - dy, Rgb::hex(0x2a7a3a) * (0.7 + 0.5 * h01(hash2(k, 3))));
                }
                // a poinsettia, it's Christmas
                if kind != RoomKind::Kitchen {
                    for k in 0..6 {
                        l.set(x + 2 + k % 4, floor - 14 - k / 2, Rgb::hex(0xd8202a));
                    }
                }
            }
            2 => {
                for xx in x..x + 10 {
                    l.set(xx, floor - 9, wood);
                }
                for y in floor - 8..floor {
                    l.set(x + 1, y, wood * 0.8);
                    l.set(x + 8, y, wood * 0.8);
                }
                for y in floor - 13..floor - 9 {
                    l.set(x + 5, y, Rgb::hex(0x3a2a20));
                }
                for y in floor - 18..floor - 13 {
                    for xx in x + 2..x + 9 {
                        l.set(xx, y, Rgb::hex(0xe8c890));
                    }
                }
                self.lamps.push((x + 5, floor - 15));
            }
            3 => {
                for y in floor - 16..floor {
                    for xx in x..x + 20 {
                        let drawer = (y - floor + 16) % 5 == 0 || xx == x || xx == x + 19;
                        l.set(xx, y, if drawer { wood * 0.7 } else { wood * 1.1 });
                    }
                }
                for k in 0..3 {
                    l.set(x + 10, floor - 13 + k * 5, Rgb::hex(0xc8a040));
                }
                // a little nativity / candles / a photo on top
                for (dx, c) in [(3, 0xf4efe6), (7, 0xc8a040), (14, 0x3a5a7a)] {
                    for y in floor - 20..floor - 16 {
                        l.set(x + dx, y, Rgb::hex(c));
                        l.set(x + dx + 1, y, Rgb::hex(c) * 0.9);
                    }
                }
                self.candles.push((x + 3, floor - 20));
            }
            4 => {
                let c = [0x3a5a7a, 0x6a4a2a, 0x5a3a5a, 0x2a5a4a][(hash2(x, 7) % 4) as usize];
                let top = wtop + (hash2(x, 3) % 10) as i32;
                for y in top..top + 11 {
                    for xx in x..x + 12 {
                        let edge = y == top || y == top + 10 || xx == x || xx == x + 11;
                        l.set(xx, y, if edge { Rgb::hex(0xb8902a) } else { Rgb::hex(c) * (0.7 + 0.4 * h01(hash2(xx / 3, y / 3))) });
                    }
                }
            }
            _ => {
                let y = wtop + 8;
                for xx in x..x + 12 {
                    l.set(xx, y, wood);
                }
                for (dx, c) in [(1, 0xb83a2a), (4, 0x2a6a3a), (6, 0xe0d0b0), (9, 0x4a5aa8)] {
                    for yy in y - 4..y {
                        l.set(x + dx, yy, Rgb::hex(c));
                        l.set(x + dx + 1, yy, Rgb::hex(c) * 0.85);
                    }
                }
            }
        }
    }

    /// Walls, floor, skirting, and (on the ground floor) the front door.
    fn build_shell(&mut self, kind: RoomKind) {
        let (w, ph, floor) = (self.w, self.h * 2, self.floor);
        let seed = self.seed;
        let church = kind == RoomKind::Church;
        let attic = kind == RoomKind::Attic;
        let wall = if church {
            Rgb::hex(0x6a6478)
        } else if attic {
            Rgb::hex(0x7a5a3a)
        } else if kind == RoomKind::Kitchen {
            Rgb::hex(0xc8b890).lerp(self.profile.wall, 0.2)
        } else if kind == RoomKind::Bedroom {
            Rgb::hex(0x5a6a8a).lerp(self.profile.wall, 0.3)
        } else {
            self.profile.wall
        };
        let wood = Rgb::hex(0x5a3a24);
        let pattern = if self.home { [1u8, 4, 2, 0][self.room_i % 4] } else { self.profile.pattern };
        let l = &mut self.room;
        for y in 0..floor {
            for x in 0..w {
                let mut c = wall;
                if church {
                    let row = y / 5;
                    let off = if row % 2 == 0 { 0 } else { 6 };
                    if y % 5 == 0 || (x + off) % 12 == 0 {
                        c = c * 0.82;
                    }
                    c = c * (0.94 + 0.08 * h01(hash2(x / 12 + row * 31, seed as i32)));
                } else if attic {
                    // bare boards
                    if x % 9 == 0 {
                        c = c * 0.78;
                    }
                    c = c * (0.9 + 0.1 * h01(hash2(x / 9, y / 13)));
                } else if kind == RoomKind::Kitchen {
                    // tiles halfway up
                    if y > floor / 2 && (x % 6 == 0 || y % 6 == 0) {
                        c = c * 0.85;
                    }
                } else {
                    match pattern {
                        0 => {
                            if (x / 6) % 2 == 1 {
                                c = c * 0.9;
                            }
                        }
                        1 => {
                            if (x % 10 == 4) && (y % 8 == 3) {
                                c = c * 1.18;
                            }
                        }
                        2 => {
                            let d = ((x % 12) - 6).abs() + ((y % 12) - 6).abs();
                            if d == 6 {
                                c = c * 0.86;
                            }
                        }
                        3 => {
                            // plain, with a picture rail
                            if y == floor / 4 {
                                c = wood * 0.9;
                            }
                        }
                        _ => {
                            // log cabin
                            let row = y / 6;
                            c = Rgb::hex(0x7a5030).lerp(wall, 0.25);
                            if y % 6 == 0 {
                                c = c * 0.6;
                            } else {
                                c = c * (0.85 + 0.2 * h01(hash2(x / 30 + row * 13, seed as i32)));
                            }
                        }
                    }
                }
                l.set(x, y, c * (0.55 + 0.45 * (y as f32 / floor as f32)));
            }
        }
        if attic {
            // the roof slopes down on both sides
            for y in 0..floor {
                let slope = ((floor - y) as f32 * 0.9) as i32;
                for x in 0..w {
                    if x < slope - 20 || x > w - slope + 20 {
                        let beam = (x + y) % 14 == 0;
                        l.set(x, y, if beam { Rgb::hex(0x3a2414) } else { Rgb::hex(0x4a3020) * (0.6 + 0.4 * y as f32 / floor as f32) });
                    }
                }
            }
        } else {
            // crown molding
            for x in 0..w {
                l.set(x, 2, wood * 0.9);
                l.set(x, 3, wood * 0.7);
            }
        }
        let wains = floor - (ph as f32 * 0.13) as i32;
        if !church && !attic && kind != RoomKind::Kitchen && pattern != 4 {
            for y in wains..floor {
                for x in 0..w {
                    let panel = (x % 20 == 0) || y == wains || y == wains + 1;
                    l.set(x, y, if panel { wood * 0.75 } else { wood * (0.95 + 0.05 * h01(hash2(x / 20, y))) });
                }
            }
        }
        // the floor
        let style = if self.home { [0u8, 2, 1, 0][self.room_i % 4] } else { self.profile.floor_style };
        for y in floor..ph {
            for x in 0..w {
                let row = (y - floor) / 3;
                let off = (h01(hash2(row, seed as i32)) * 18.0) as i32;
                let c = if church {
                    let seam = (x + off) % 18 == 0 || (y - floor) % 3 == 0;
                    let c = Rgb::hex(0x4a4652) * (0.9 + 0.12 * h01(hash2((x + off) / 9, row)));
                    if seam {
                        c * 0.7
                    } else {
                        c
                    }
                } else if style == 2 {
                    // checker tiles
                    let tile = ((x / 8) + (y - floor) / 4) % 2 == 0;
                    if tile {
                        Rgb::hex(0xd8d0c0)
                    } else {
                        Rgb::hex(0x4a3a3a)
                    }
                } else {
                    let seam = (x + off) % 18 == 0 || (y - floor) % 3 == 0;
                    let k = h01(hash2((x + off) / 18, row * 7 + seed as i32));
                    let (a, b) = if style == 1 { (Rgb::hex(0x9a7048), Rgb::hex(0xb08458)) } else { (Rgb::hex(0x6a4428), Rgb::hex(0x7a5030)) };
                    let c = a.lerp(b, k);
                    if seam {
                        c * 0.7
                    } else {
                        c
                    }
                };
                let depth = (y - floor) as f32 / (ph - floor).max(1) as f32;
                l.set(x, y, c * (0.75 + 0.3 * depth));
            }
        }
        for x in 0..w {
            l.set(x, floor - 1, wood * 0.5);
        }
        if matches!(kind, RoomKind::Parlour | RoomKind::Church) {
            self.front_door();
        }
    }

    fn front_door(&mut self) {
        let floor = self.floor;
        let (dx0, dx1, dtop) = (4, 22, floor - 32);
        let l = &mut self.room;
        let trim = Rgb::hex(0xe8e0d0) * 0.85;
        for y in dtop - 2..floor {
            for x in dx0 - 2..dx1 + 2 {
                l.set(x, y, trim);
            }
        }
        let door = if self.church {
            Rgb::hex(0x4a2a16)
        } else if self.home {
            Rgb::hex(0x1f5a32)
        } else {
            [Rgb::hex(0x7a2a22), Rgb::hex(0x2a4a7a), Rgb::hex(0x1f5a32), Rgb::hex(0x5a3a22)][(self.seed % 4) as usize]
        };
        for y in dtop..floor {
            for x in dx0..dx1 {
                let (u, v) = (x - dx0, y - dtop);
                let panel = ((3..=7).contains(&u) || (10..=14).contains(&u)) && ((3..=13).contains(&v) || (17..=28).contains(&v));
                let edge_hi = panel && (u == 3 || u == 10 || v == 3 || v == 17);
                let c = if edge_hi {
                    door * 1.25
                } else if panel {
                    door * 1.08
                } else {
                    door
                };
                l.set(x, y, c * (0.95 + 0.05 * h01(hash2(x, y))));
            }
        }
        l.set(dx1 - 3, dtop + 16, Rgb::hex(0xffc23a));
        l.set(dx1 - 3, dtop + 17, Rgb::hex(0xc89a2a));
        let (wx, wy) = ((dx0 + dx1) / 2, dtop + 8);
        for a in 0..24 {
            let ang = a as f32 / 24.0 * std::f32::consts::TAU;
            let (x, y) = (wx as f32 + ang.cos() * 3.2, wy as f32 + ang.sin() * 3.2);
            l.set(x.round() as i32, y.round() as i32, if a % 6 == 0 { Rgb::hex(0xd8202a) } else { Rgb::hex(0x1f6e3a) });
        }
        self.things.push((Thing::Door, (dx0 + dx1) / 2, dtop / 2 - 1));
    }

    fn window_at(&mut self, cx: i32, top: i32, ww: i32, wh: i32, arch: bool, look: Look) {
        let frame = if self.church { Rgb::hex(0x3a3440) } else { Rgb::hex(0xe8e0d0) };
        let curtain = self.profile.curtain;
        let l = &mut self.room;
        let x0 = cx - ww / 2;
        for y in top - 1..=top + wh {
            for x in x0 - 1..=x0 + ww {
                l.set(x, y, frame);
            }
        }
        if arch {
            for k in 0..ww / 2 {
                for x in x0 + k..x0 + ww - k {
                    l.set(x, top - 2 - k, frame);
                }
            }
        } else if !self.church {
            // curtains, tied back
            for y in top - 3..top + wh + 2 {
                let tie = (y - top) as f32 / wh as f32;
                let spread = if tie < 0.55 { 3 } else { 2 };
                for k in 0..spread {
                    l.set(x0 - 3 - k, y, curtain * (0.8 + 0.1 * k as f32));
                    l.set(x0 + ww + 2 + k, y, curtain * (0.8 + 0.1 * k as f32));
                }
            }
            for x in x0 - 6..x0 + ww + 6 {
                l.set(x, top - 3, Rgb::hex(0x3a2214));
            }
        }
        let n = self.windows.len();
        self.windows.push(((x0, top, ww, wh), look));
        self.things.push((Thing::Window(n), cx, top / 2 - 1));
    }

    /// A front window's view: looking out at the street (`dx`: along the
    /// village from the house, in screens).
    fn street_look(dx: f32) -> Look {
        Look { dx, lift: 0.0, hole: Hole::Rect(0.22, 0.34, 0.78, 0.92), telescope: false }
    }

    fn fireplace(&mut self, cx: i32) {
        let floor = self.floor;
        let (fw, fh) = (30, 30);
        let (x0, y0) = (cx - fw / 2, floor - fh);
        let stone = [0x7a6a68, 0x8a5a48, 0x6a6a70][(self.seed % 3) as usize];
        let l = &mut self.room;
        for y in y0..floor {
            for x in x0..x0 + fw {
                let row = (y - y0) / 3;
                let off = if row % 2 == 0 { 0 } else { 3 };
                let mortar = (y - y0) % 3 == 0 || (x - x0 + off) % 6 == 0;
                let c = Rgb::hex(stone) * (0.85 + 0.2 * h01(hash2((x + off) / 6, row)));
                l.set(x, y, if mortar { c * 0.7 } else { c });
            }
        }
        for x in x0 - 3..x0 + fw + 3 {
            l.set(x, y0 - 1, Rgb::hex(0x4a2a1a));
            l.set(x, y0 - 2, Rgb::hex(0x5a3420));
            l.set(x, y0 - 3, Rgb::hex(0x1f5a32) * (0.8 + 0.3 * h01(hash2(x, 9))));
        }
        let (ow, oh) = (16, 16);
        let (ox, oy) = (cx - ow / 2, floor - oh);
        for y in oy..floor {
            for x in ox..ox + ow {
                l.set(x, y, Rgb::hex(0x0e0808));
            }
        }
        self.hearth = Some((ox, oy, ow, oh));
        // stockings: one per person (and one for the pet)
        let n = (self.residents.len() + self.pet.is_some() as usize).clamp(2, 4);
        let socks = [Rgb::hex(0xc8202a), Rgb::hex(0x2a8a4a), Rgb::hex(0x2a5aa8), Rgb::hex(0xc8a040)];
        for i in 0..n {
            let sx = x0 + 2 + i as i32 * 7;
            let c = socks[(i + self.seed as usize) % socks.len()];
            for y in y0..y0 + 5 {
                for x in sx..sx + 3 {
                    l.set(x, y, if y == y0 { Rgb::hex(0xf4efe6) } else { c });
                }
            }
            l.set(sx + 3, y0 + 4, c);
            l.set(sx + 3, y0 + 3, c);
        }
        self.things.push((Thing::Fire, cx, y0 / 2 - 2));
        self.things.push((Thing::Stocking, x0 + fw - 2, y0 / 2 - 1));
        self.spots.push((cx as f32 + 10.0, Doing::Warm));
    }

    fn rug(&mut self, cx: i32, rx: f32) {
        let (floor, ph) = (self.floor, self.h * 2);
        let rug = self.profile.rug;
        let (rcx, rcy, ry) = (cx as f32, floor as f32 + (ph - floor) as f32 * 0.42, 6.0);
        let l = &mut self.room;
        for y in floor..ph {
            for x in (cx - rx as i32 - 2)..(cx + rx as i32 + 2) {
                let (dx, dy) = ((x as f32 - rcx) / rx, (y as f32 - rcy) / ry);
                let d = dx * dx + dy * dy;
                if d <= 1.0 {
                    let border = d > 0.72;
                    let c = if border {
                        Rgb::hex(0xc8a040)
                    } else if (x + y) % 4 == 0 {
                        rug * 0.8
                    } else {
                        rug
                    };
                    l.set(x, y, c);
                }
            }
        }
    }

    /// An armchair (or a rocking chair) facing `dir`; a seat.
    fn armchair(&mut self, ax: i32, color: Rgb, rocking: bool) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 16..floor + 2 {
            for x in ax..ax + 14 {
                let back = y < floor - 6 && x >= ax + 2 && x < ax + 12;
                let seat = y >= floor - 6 && y < floor;
                let arm = (x < ax + 3 || x >= ax + 11) && y >= floor - 9 && y < floor;
                let leg = y >= floor && (x == ax + 1 || x == ax + 12);
                if back || seat || arm {
                    let c = if rocking { Rgb::hex(0x6a4428) } else { color };
                    l.set(x, y, c * if arm { 0.8 } else { 1.0 } * (0.9 + 0.1 * h01(hash2(x, y))));
                } else if leg {
                    l.set(x, y, Rgb::hex(0x2a1a10));
                }
            }
        }
        if rocking {
            for x in ax - 1..ax + 15 {
                let dy = (((x - ax) as f32 - 7.0) / 7.0).powi(2) * 2.0;
                l.set(x, floor + 1 - dy as i32, Rgb::hex(0x3a2214));
            }
            // a patchwork quilt over the back
            for y in floor - 15..floor - 8 {
                for x in ax + 3..ax + 11 {
                    let c = [0xb8402a, 0x2a6a3a, 0xc8a040, 0x2a4a8a][((x / 3 + y / 3) % 4) as usize];
                    l.set(x, y, Rgb::hex(c));
                }
            }
        }
        let n = self.seats.len();
        self.seats.push(Seat { x: ax + 7, feet: floor / 2, top: floor - 6, by: None });
        self.things.push((Thing::Seat(n), ax + 7, (floor - 16) / 2 - 1));
        self.spots.push(((ax + 7) as f32, Doing::Sit(n)));
    }

    /// A sofa with two seats.
    fn couch(&mut self, cx: i32) {
        let floor = self.floor;
        let c = self.profile.rug.lerp(Rgb::hex(0x6a5a4a), 0.4);
        let l = &mut self.room;
        let x0 = cx - 16;
        for y in floor - 14..floor + 2 {
            for x in x0..x0 + 32 {
                let back = y < floor - 6 && x >= x0 + 2 && x < x0 + 30;
                let seat = y >= floor - 6 && y < floor;
                let arm = (x < x0 + 3 || x >= x0 + 29) && y >= floor - 10 && y < floor;
                let leg = y >= floor && (x == x0 + 1 || x == x0 + 30);
                if back || seat || arm {
                    let cushion = seat && (x - x0) % 14 == 1;
                    l.set(x, y, c * if arm { 0.8 } else if cushion { 0.75 } else { 1.0 } * (0.9 + 0.1 * h01(hash2(x, y))));
                } else if leg {
                    l.set(x, y, Rgb::hex(0x2a1a10));
                }
            }
        }
        // a cushion leaning on each arm: a small slanted pillow in a
        // darker shade of the sofa, with a lighter stripe
        for (k, lean) in [(x0 + 3, 1), (x0 + 26, -1)] {
            for dy in 0..4 {
                for dx in 0..3 {
                    let x = k + dx + if lean > 0 { dy / 2 } else { -(dy / 2) };
                    let stripe = dx == 1;
                    l.set(x, floor - 10 + dy, c * if stripe { 0.95 } else { 0.62 } + Rgb::hex(0x1a1008) * 0.5);
                }
            }
        }
        for sx in [x0 + 9, x0 + 23] {
            let n = self.seats.len();
            self.seats.push(Seat { x: sx, feet: floor / 2, top: floor - 6, by: None });
            self.things.push((Thing::Seat(n), sx, (floor - 14) / 2 - 1));
            self.spots.push((sx as f32, Doing::Sit(n)));
        }
    }

    fn shelf(&mut self, bx: i32, toys: bool) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 30..floor {
            for x in bx..bx + 12 {
                let shelf = (floor - y) % 8 == 0 || x == bx || x == bx + 11;
                let c = if shelf {
                    Rgb::hex(0x3a2214)
                } else if toys && (floor - y) % 8 > 3 {
                    Rgb::hex(0x1a1410)
                } else {
                    let books = [0x8a2a2a, 0x2a5a3a, 0x2a3a6a, 0x8a6a2a, 0x5a2a5a, 0x6a6a6a];
                    Rgb::hex(books[(hash2(x, (floor - y) / 8) % 6) as usize]) * (0.75 + 0.3 * h01(hash2(x, 3)))
                };
                l.set(x, y, c);
            }
        }
        if toys {
            // little wooden toys on the shelves
            for (k, col) in [0xc8202a, 0x2a8a4a, 0xc8a040, 0x2a5aa8].iter().enumerate() {
                let y = floor - 4 - 8 * (k as i32 % 3);
                let x = bx + 2 + (k as i32 * 3) % 8;
                for dy in 0..3 {
                    for dx in 0..2 {
                        l.set(x + dx, y - dy, Rgb::hex(*col));
                    }
                }
            }
        }
        self.things.push((Thing::Shelf, bx + 6, (floor - 30) / 2 - 1));
    }

    fn clock_at(&mut self, ck: i32) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 34..floor {
            for x in ck..ck + 7 {
                let edge = x == ck || x == ck + 6;
                l.set(x, y, if edge { Rgb::hex(0x2a1810) } else { Rgb::hex(0x4a2a18) });
            }
        }
        self.clock = Some((ck + 3, floor - 30));
        self.things.push((Thing::Clock, ck + 3, (floor - 34) / 2 - 1));
    }

    fn piano(&mut self, pxp: i32) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 14..floor + 1 {
            for x in pxp..pxp + 22 {
                let keys = y == floor - 8 || y == floor - 7;
                let leg = y > floor - 7 && !(x == pxp + 1 || x == pxp + 20);
                if leg {
                    continue;
                }
                let c = if keys {
                    if (x - pxp) % 2 == 0 {
                        Rgb::hex(0xf0ece0)
                    } else {
                        Rgb::hex(0x1a1a1a)
                    }
                } else {
                    Rgb::hex(0x16141a)
                };
                l.set(x, y, c);
            }
        }
        self.things.push((Thing::Piano, pxp + 11, (floor - 14) / 2 - 1));
        self.spots.push(((pxp + 11) as f32, Doing::Busy));
    }

    fn table(&mut self, tx: i32, wide: i32) -> (i32, i32) {
        let floor = self.floor;
        let l = &mut self.room;
        for x in tx..tx + wide {
            l.set(x, floor - 7, Rgb::hex(0x4a2a1a));
            l.set(x, floor - 8, Rgb::hex(0x5a3420));
        }
        for y in floor - 6..floor + 1 {
            l.set(tx + 1, y, Rgb::hex(0x3a2214));
            l.set(tx + wide - 2, y, Rgb::hex(0x3a2214));
        }
        (tx + wide / 2, floor - 9)
    }

    fn mug_at(&mut self, x: i32, y: i32) {
        let l = &mut self.room;
        for yy in y - 2..y + 1 {
            for xx in x - 1..x + 2 {
                l.set(xx, yy, Rgb::hex(0xe8e0d0));
            }
        }
        l.set(x + 2, y - 1, Rgb::hex(0xe8e0d0));
        self.mug = Some((x, y - 3));
        self.things.push((Thing::Cocoa, x, (y - 3) / 2 - 2));
    }

    fn photo(&mut self, px: i32, top: i32, c: u32) {
        let l = &mut self.room;
        for y in top..top + 9 {
            for x in px..px + 12 {
                let edge = y == top || y == top + 8 || x == px || x == px + 11;
                l.set(x, y, if edge { Rgb::hex(0xb8902a) } else { Rgb::hex(c) * (0.7 + 0.4 * h01(hash2(x / 3, y / 3))) });
            }
        }
        self.things.push((Thing::Photo, px + 6, top / 2 - 1));
    }

    fn oven_at(&mut self, ox: i32) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 20..floor {
            for x in ox..ox + 18 {
                let door = y > floor - 14 && y < floor - 3 && x > ox + 2 && x < ox + 15;
                let glass = y > floor - 12 && y < floor - 6 && x > ox + 4 && x < ox + 13;
                let c = if glass {
                    Rgb::hex(0x2a1a14)
                } else if door {
                    Rgb::hex(0x3a3a40)
                } else if y < floor - 17 {
                    Rgb::hex(0x1a1a20)
                } else {
                    Rgb::hex(0x2a2a30)
                };
                l.set(x, y, c);
            }
        }
        // the knobs and a kettle on top
        for k in 0..3 {
            l.set(ox + 4 + k * 4, floor - 16, Rgb::hex(0xc8c0a8));
        }
        for y in floor - 25..floor - 20 {
            for x in ox + 3..ox + 9 {
                l.set(x, y, Rgb::hex(0xb8202a));
            }
        }
        self.oven = Some((ox + 9, floor - 9));
        self.things.push((Thing::Oven, ox + 9, (floor - 25) / 2 - 1));
        self.spots.push(((ox + 9) as f32, Doing::Busy));
    }

    fn workbench(&mut self, wx: i32) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 10..floor + 1 {
            for x in wx..wx + 22 {
                let top = y <= floor - 9;
                let leg = !top && (x == wx + 1 || x == wx + 20);
                if top {
                    l.set(x, y, Rgb::hex(0x8a6a40));
                } else if leg {
                    l.set(x, y, Rgb::hex(0x4a3020));
                }
            }
        }
        // tools on a pegboard, half-made toys on the bench
        for (k, col) in [(0, 0x8a8a92), (4, 0x6a4428), (8, 0x8a8a92), (12, 0x6a4428)] {
            for y in floor - 30..floor - 22 {
                l.set(wx + 4 + k, y, Rgb::hex(col));
            }
        }
        for (k, col) in [(3, 0xc8202a), (9, 0x2a8a4a), (15, 0xc8a040)] {
            for y in floor - 13..floor - 10 {
                for x in wx + k..wx + k + 3 {
                    l.set(x, y, Rgb::hex(col));
                }
            }
        }
        self.things.push((Thing::Workbench, wx + 11, (floor - 30) / 2 - 1));
        self.spots.push(((wx + 11) as f32, Doing::Busy));
    }

    fn gramophone(&mut self, gx: i32, radio: bool) {
        let floor = self.floor;
        let (_, ty) = self.table(gx - 6, 13);
        let l = &mut self.room;
        if radio {
            for y in ty - 7..ty + 1 {
                for x in gx - 4..gx + 5 {
                    let grille = y > ty - 6 && y < ty - 1 && x > gx - 3 && x < gx + 1 && (x + y) % 2 == 0;
                    l.set(x, y, if grille { Rgb::hex(0xc8b080) } else { Rgb::hex(0x6a3a1a) });
                }
            }
            l.set(gx + 3, ty - 3, Rgb::hex(0xe8c040));
        } else {
            for y in ty - 3..ty + 1 {
                for x in gx - 4..gx + 4 {
                    l.set(x, y, Rgb::hex(0x5a3420));
                }
            }
            // the horn
            for k in 0..8 {
                let r = k / 2;
                for dx in -r..=r {
                    l.set(gx + 2 + k, ty - 4 - k + dx.abs() / 2, Rgb::hex(0xc8a040) * (0.8 + 0.04 * k as f32));
                }
            }
        }
        let _ = floor;
        self.things.push((if radio { Thing::Radio } else { Thing::Gramophone }, gx, (ty - 12) / 2));
    }

    fn telescope(&mut self, tx: i32, top: i32) {
        let floor = self.floor;
        let l = &mut self.room;
        // tripod and tube, pointing up at the window
        for k in 0..12 {
            l.set(tx - k / 2, floor - k, Rgb::hex(0x4a3020));
            l.set(tx + k / 2, floor - k, Rgb::hex(0x4a3020));
        }
        for k in 0..14 {
            for d in 0..2 {
                l.set(tx - 2 + k, floor - 12 - k / 2 - d, Rgb::hex(0xb8902a) * (0.8 + 0.02 * k as f32));
            }
        }
        let _ = top;
        self.things.push((Thing::Telescope, tx + 3, (floor - 22) / 2 - 1));
    }

    fn the_tree(&mut self, tx: i32) {
        let (h, floor) = (self.h, self.floor);
        let base_row = floor / 2 + 1;
        let tall = if self.church { 0.5 } else { 0.4 };
        let style = [LightStyle::Multi, LightStyle::Warm, LightStyle::Icy][(self.seed % 3) as usize];
        let tree = Tree::new(tx, base_row + 1, ((h as f32) * tall) as i32, style, self.seed ^ 0x77ee);
        self.things.push((Thing::Tree, tree.cx, tree.top - 2));
        self.spots.push((tx as f32 - 8.0, Doing::Look));
        self.tree = Some(tree);
    }

    fn build_parlour(&mut self) {
        let (w, floor) = (self.w, self.floor);
        let fx = |f: f32| (w as f32 * f) as i32;
        let wtop = (floor as f32 * 0.22) as i32;
        let wh = (floor as f32 * 0.40) as i32;
        let theme = self.profile.theme;
        let chair = [Rgb::hex(0x7a2a2a), Rgb::hex(0x2a5a3a), Rgb::hex(0x3a4a7a), Rgb::hex(0x6a4a2a)][(self.seed as usize / 7) % 4];
        if self.home {
            // the villager's own parlour: fire, tree, a sofa, the stairs up
            self.window_at(fx(0.18), wtop, 18, wh, false, Self::street_look(-0.4));
            self.fireplace(fx(0.38));
            self.rug(fx(0.38), 30.0);
            self.couch(fx(0.56));
            self.photo(fx(0.47), wtop, 0x3a5a7a);
            self.the_tree(fx(0.70));
            self.stairs_up();
            self.things.push((Thing::Doorway(true), w - 6, floor / 2 - 12));
            self.doorway_right();
            return;
        }
        let layout = self.profile.layout;
        let (win1, fire, chairs, tree, win2, a, b) = match layout {
            0 => (0.17, 0.42, 0.53, 0.71, 0.85, 0.28, 0.93),
            1 => (0.15, 0.56, 0.67, 0.29, 0.80, 0.42, 0.92),
            _ => (0.14, 0.30, 0.45, 0.62, 0.86, 0.75, 0.94),
        };
        self.window_at(fx(win1), wtop, 18, wh, false, Self::street_look(-0.45));
        self.window_at(fx(win2), wtop, 18, wh, false, Self::street_look(0.05));
        self.candles.push((fx(win1) + 6, wtop + wh));
        self.candles.push((fx(win2) + 6, wtop + wh));
        self.fireplace(fx(fire));
        self.rug(fx(fire), 34.0);
        if layout == 2 {
            self.couch(fx(chairs));
        } else {
            self.armchair(fx(chairs), chair, theme == Theme::Farmer);
            let (mx, my) = self.table(fx(chairs) + 17, 8);
            self.mug_at(mx, my);
        }
        self.the_tree(fx(tree));
        self.photo(fx((fire + chairs) / 2.0 + 0.02), wtop, [0x3a5a7a, 0x6a4a2a, 0x5a3a5a][(self.seed % 3) as usize]);
        // what this household is about
        let (ax, bx) = (fx(a).max(30), fx(b).min(w - 26));
        match theme {
            Theme::Baker => {
                self.oven_at(bx - 6);
                let (jx, jy) = self.table(ax - 2, 14);
                self.cookie_jar(jx, jy);
            }
            Theme::Toymaker => {
                self.workbench(ax - 6);
                self.shelf(bx, true);
                let tr = self.tree.as_ref().map(|t| t.cx).unwrap_or(fx(tree));
                self.toy_train = Some((tr, floor + 6));
            }
            Theme::Bookworm => {
                self.shelf(ax - 6, false);
                self.shelf(ax + 8, false);
                self.shelf(bx, false);
            }
            Theme::Musician => {
                self.piano(ax - 10);
                self.gramophone(bx + 4, false);
            }
            Theme::Sailor => {
                self.shelf(ax - 6, false);
                self.telescope(bx + 6, wtop);
                self.photo(ax + 10, wtop + 4, 0x2a4a6a);
            }
            Theme::Candles => {
                self.shelf(ax - 6, false);
                self.clock_at(bx + 6);
                for k in 0..8 {
                    self.candles.push((fx(0.1 + k as f32 * 0.11) + 3, floor - 1 - (k % 2) * 8));
                }
            }
            Theme::Farmer => {
                self.gramophone(ax, true);
                self.clock_at(bx + 6);
            }
        }
        if layout == 0 && theme != Theme::Candles && theme != Theme::Farmer {
            self.clock_at(fx(0.62));
        }
    }

    fn cookie_jar(&mut self, x: i32, y: i32) {
        let l = &mut self.room;
        for yy in y - 5..y + 1 {
            for xx in x - 3..x + 3 {
                l.set(xx, yy, Rgb::hex(0xd8e0e8) * 0.85);
            }
        }
        for xx in x - 2..x + 2 {
            l.set(xx, y - 3, Rgb::hex(0xa86a30));
            l.set(xx, y - 1, Rgb::hex(0xb87a38));
        }
        for xx in x - 3..x + 3 {
            l.set(xx, y - 6, Rgb::hex(0xc8202a));
        }
        self.things.push((Thing::CookieJar, x, (y - 6) / 2 - 2));
    }

    /// The home's staircase: (foot column, top column, steps). It climbs
    /// to the right from the parlour floor right up to the ceiling, where
    /// the bedroom's stairwell opens straight above it.
    fn stair_geom(&self) -> (i32, i32, i32) {
        let n = (self.floor / 5).max(4);
        let xb = (self.w as f32 * 0.78) as i32;
        (xb, xb + 3 * n, n)
    }

    /// The ladder between the bedroom and the attic (its column).
    fn ladder_x(&self) -> i32 {
        (self.w as f32 * 0.10) as i32
    }

    fn stairs_up(&mut self) {
        let floor = self.floor;
        let (xb, xt, n) = self.stair_geom();
        let l = &mut self.room;
        // under the stairs: a panelled wall
        for k in 0..n {
            let (x0, top) = (xb + 3 * k, floor - 5 * (k + 1));
            for x in x0..x0 + 3 {
                for y in top + 2..floor {
                    let panel = (y - top) % 12 == 0 || (x - xb) % 15 == 0;
                    l.set(x, y, Rgb::hex(0x4a3020) * if panel { 0.8 } else { 1.0 });
                }
                // the tread, lit on its edge
                l.set(x, top, Rgb::hex(0x8a5a34));
                l.set(x, top + 1, Rgb::hex(0x6a4428));
            }
            // a spindle every other step, up to the handrail
            if k % 2 == 0 {
                for y in top - 9..top {
                    l.set(x0 + 1, y, Rgb::hex(0xe8e0d0));
                }
            }
        }
        // the handrail
        for k in 0..3 * n {
            let y = floor - 14 - (k * 5) / 3;
            l.set(xb + k, y, Rgb::hex(0x5a3420));
            l.set(xb + k, y + 1, Rgb::hex(0x3a2214));
        }
        // the newel post at the foot
        for y in floor - 16..floor {
            l.set(xb - 1, y, Rgb::hex(0x5a3420));
            l.set(xb, y, Rgb::hex(0x6a4428));
        }
        // the opening in the ceiling
        for x in xt - 16..xt + 3 {
            for y in 0..4 {
                l.set(x, y, Rgb::hex(0x140e0a));
            }
        }
        self.things.push((Thing::Stairs(true), xb + 6, (floor - 30) / 2));
    }

    /// The top of the stairs from below: a stairwell at the front of the
    /// floor (the parlour's stairs come up into it), railed round.
    fn stairwell(&mut self, x: i32) {
        let (h2, floor) = (self.h * 2, self.floor);
        let l = &mut self.room;
        for xx in x - 12..x + 4 {
            for y in h2 - 9..h2 {
                let k = (y - (h2 - 9)) as f32 / 9.0;
                l.set(xx, y, Rgb::hex(0x2a1c12) * (1.0 - 0.6 * k));
            }
            // the top steps going down, just visible
            if (xx - x) % 3 == 0 {
                l.set(xx, h2 - 9 + ((x - xx) / 3).clamp(0, 8), Rgb::hex(0x6a4428));
            }
        }
        for (px, side) in [(x - 13, 0), (x + 4, 1)] {
            for y in h2 - 16..h2 {
                l.set(px, y, Rgb::hex(0x5a3420) * if side == 0 { 1.0 } else { 0.85 });
            }
        }
        for xx in x - 13..x + 5 {
            l.set(xx, h2 - 16, Rgb::hex(0x5a3420));
        }
        let _ = floor;
        self.things.push((Thing::Stairs(false), x - 4, self.h - 8));
    }

    fn doorway_right(&mut self) {
        let (w, floor) = (self.w, self.floor);
        let l = &mut self.room;
        for y in floor - 30..floor {
            for x in w - 9..w {
                l.set(x, y, if x == w - 9 || y == floor - 30 { Rgb::hex(0xe8e0d0) * 0.85 } else { Rgb::hex(0x2a1e18) * (0.8 + 0.2 * (y - floor + 30) as f32 / 30.0) });
            }
        }
    }

    fn doorway_left(&mut self) {
        let floor = self.floor;
        let l = &mut self.room;
        for y in floor - 30..floor {
            for x in 0..9 {
                l.set(x, y, if x == 8 || y == floor - 30 { Rgb::hex(0xe8e0d0) * 0.85 } else { Rgb::hex(0x2a1e18) * (0.8 + 0.2 * (y - floor + 30) as f32 / 30.0) });
            }
        }
        self.things.push((Thing::Doorway(false), 5, floor / 2 - 12));
    }

    fn build_kitchen(&mut self) {
        let (w, floor) = (self.w, self.floor);
        let fx = |f: f32| (w as f32 * f) as i32;
        let wtop = (floor as f32 * 0.20) as i32;
        let wh = (floor as f32 * 0.36) as i32;
        self.doorway_left();
        self.window_at(fx(0.5), wtop, 22, wh, false, Look { dx: 0.25, lift: 0.0, hole: Hole::Rect(0.2, 0.36, 0.8, 0.86), telescope: false });
        self.candles.push((fx(0.5) + 8, wtop + wh));
        // counters along the wall
        let l = &mut self.room;
        for y in floor - 12..floor {
            for x in fx(0.30)..fx(0.70) {
                let top = y < floor - 10;
                let door = (x - fx(0.30)) % 14 == 0;
                l.set(x, y, if top { Rgb::hex(0xd8d0c0) } else if door { Rgb::hex(0x3a2a1a) } else { Rgb::hex(0x5a7a6a) });
            }
        }
        // hanging pots
        for k in 0..4 {
            let px = fx(0.33) + k * 9;
            for y in floor - 30..floor - 26 {
                for x in px..px + 5 {
                    l.set(x, y, Rgb::hex(0x8a5a3a) * (0.8 + 0.1 * k as f32));
                }
            }
            l.set(px + 2, floor - 31, Rgb::hex(0x2a2a2a));
        }
        self.oven_at(fx(0.14));
        let (tx, ty) = self.table(fx(0.78), 22);
        self.cookie_jar(tx - 5, ty);
        self.mug_at(tx + 5, ty);
        for (k, sx) in [fx(0.76), fx(0.78) + 24].into_iter().enumerate() {
            // ladder-back chairs
            let l = &mut self.room;
            for y in floor - 16..floor + 1 {
                for x in sx - 3..sx + 3 {
                    let back = y < floor - 6 && (x == sx - 3 || x == sx + 2 || (y - floor) % 3 == 0);
                    let seat = y == floor - 6 || y == floor - 5;
                    let leg = y > floor - 5 && (x == sx - 3 || x == sx + 2);
                    if back || seat || leg {
                        l.set(x, y, Rgb::hex(0x6a4428) * (0.9 + 0.1 * k as f32));
                    }
                }
            }
            let n = self.seats.len();
            self.seats.push(Seat { x: sx, feet: floor / 2, top: floor - 6, by: None });
            self.things.push((Thing::Seat(n), sx, (floor - 16) / 2 - 1));
        }
    }

    fn build_bedroom(&mut self) {
        let (w, floor) = (self.w, self.floor);
        let fx = |f: f32| (w as f32 * f) as i32;
        let wtop = (floor as f32 * 0.16) as i32;
        let wh = (floor as f32 * 0.40) as i32;
        // the stairs back down (the parlour's stairs come up here)
        let xt = self.stair_geom().1;
        self.stairwell(xt);
        // a ladder up to the attic hatch
        let lx = self.ladder_x();
        let l = &mut self.room;
        for y in 0..floor {
            l.set(lx - 4, y, Rgb::hex(0x6a4428));
            l.set(lx + 4, y, Rgb::hex(0x6a4428));
            if y % 5 == 0 {
                for x in lx - 4..lx + 5 {
                    l.set(x, y, Rgb::hex(0x7a5030));
                }
            }
        }
        for x in lx - 7..lx + 8 {
            for y in 0..4 {
                l.set(x, y, Rgb::hex(0x1a120c));
            }
        }
        self.things.push((Thing::Stairs(true), lx, floor / 2 - 14));
        // the bed, a quilt, a little lamp
        let bx = fx(0.30);
        for y in floor - 12..floor + 2 {
            for x in bx..bx + 44 {
                let head = x < bx + 4 && y < floor + 2;
                let mattress = y >= floor - 7 && y < floor - 2;
                let quilt = y >= floor - 7 && y < floor && x > bx + 12;
                let leg = y >= floor && (x == bx + 1 || x == bx + 42);
                if head {
                    l.set(x, y - 6, Rgb::hex(0x6a4428));
                    l.set(x, y, Rgb::hex(0x6a4428));
                } else if quilt {
                    let c = [0xb8402a, 0x2a6a3a, 0xc8a040, 0x2a4a8a][(((x - bx) / 5 + (y - floor) / 3) % 4).unsigned_abs() as usize];
                    l.set(x, y, Rgb::hex(c));
                } else if mattress {
                    l.set(x, y, Rgb::hex(0xf0ece4));
                } else if leg {
                    l.set(x, y, Rgb::hex(0x3a2214));
                }
            }
        }
        // the pillow
        for y in floor - 10..floor - 7 {
            for x in bx + 5..bx + 12 {
                l.set(x, y, Rgb::hex(0xf8f6f0));
            }
        }
        self.things.push((Thing::Bed, bx + 22, (floor - 12) / 2 - 1));
        self.bed = Some((bx, floor));
        self.window_at(fx(0.62), wtop, 20, wh, false, Look { dx: -0.6, lift: 0.0, hole: Hole::Rect(0.24, 0.3, 0.76, 0.74), telescope: false });
        self.candles.push((fx(0.62) + 7, wtop + wh));
        self.shelf(fx(0.76), true);
        self.rug(fx(0.5), 26.0);
    }

    fn build_attic(&mut self) {
        let (w, floor, h) = (self.w, self.floor, self.h);
        let fx = |f: f32| (w as f32 * f) as i32;
        // the hatch and the ladder down (above the bedroom's ladder)
        let lx = self.ladder_x();
        let l = &mut self.room;
        for x in lx - 7..lx + 8 {
            for y in h * 2 - 8..h * 2 {
                l.set(x, y, Rgb::hex(0x1a120c) * (1.0 - 0.5 * (y - (h * 2 - 8)) as f32 / 8.0));
            }
            l.set(x, h * 2 - 9, Rgb::hex(0x5a3420));
        }
        for y in h * 2 - 8..h * 2 {
            if y % 4 == 0 {
                for x in lx - 4..lx + 5 {
                    l.set(x, y, Rgb::hex(0x7a5030));
                }
            }
        }
        self.things.push((Thing::Stairs(false), lx, h - 8));
        // a round window under the eaves
        let (cx, cy, r) = (fx(0.5), (floor as f32 * 0.42) as i32, (floor as f32 * 0.16) as i32);
        for y in cy - r - 2..=cy + r + 2 {
            for x in cx - r * 2 - 4..=cx + r * 2 + 4 {
                let d = (((x - cx) as f32 / 2.0).powi(2) + ((y - cy) as f32).powi(2)).sqrt();
                if d <= r as f32 + 2.0 {
                    l.set(x, y, Rgb::hex(0xe8e0d0) * 0.9);
                }
            }
        }
        let n = self.windows.len();
        self.windows.push(((cx - r * 2, cy - r, r * 4, r * 2), Look { dx: -0.4, lift: (h as f32 * 0.25).round(), hole: Hole::Round(0.5, 0.42, 0.34), telescope: false }));
        self.things.push((Thing::Window(n), cx, (cy - r) / 2 - 1));
        self.telescope(fx(0.66), cy);
        // boxes of decorations, an old music box
        for (k, bx) in [fx(0.26), fx(0.32), fx(0.8)].into_iter().enumerate() {
            let l = &mut self.room;
            let hgt = 10 + (k as i32 % 2) * 5;
            for y in floor - hgt..floor {
                for x in bx..bx + 12 {
                    let tape = x == bx + 6 || y == floor - hgt;
                    l.set(x, y, if tape { Rgb::hex(0xc8a060) } else { Rgb::hex(0x9a7a50) * (0.85 + 0.15 * h01(hash2(x, k as i32))) });
                }
            }
        }
        self.things.push((Thing::Boxes, fx(0.29), (floor - 15) / 2 - 1));
        let (mx, my) = self.table(fx(0.86), 12);
        let l = &mut self.room;
        for y in my - 4..my + 1 {
            for x in mx - 3..mx + 3 {
                l.set(x, y, Rgb::hex(0x8a3a2a));
            }
        }
        self.things.push((Thing::MusicBox, mx, (my - 4) / 2 - 2));
    }

    fn build_church(&mut self) {
        let (w, floor, ph) = (self.w, self.floor, self.h * 2);
        let fx = |f: f32| (w as f32 * f) as i32;
        let wtop = (floor as f32 * 0.25) as i32;
        let wh = (floor as f32 * 0.55) as i32;
        let mut f = 0.16;
        while f < 0.95 {
            let cx = fx(f);
            self.window_at(cx, wtop, 12, wh, true, Self::street_look(f - 0.5));
            f += 0.17;
        }
        let l = &mut self.room;
        for row in 0..3 {
            let y = floor + 3 + row * ((ph - floor) / 3);
            let mut x = fx(0.18);
            while x < w - 16 {
                if (x - fx(0.5)).abs() > 14 {
                    for yy in y - 4..y + 1 {
                        for xx in x..x + 18 {
                            let back = yy < y - 1;
                            l.set(xx, yy, Rgb::hex(0x5a3420) * if back { 0.85 } else { 1.0 } * (0.9 + 0.1 * h01(hash2(xx, yy))));
                        }
                    }
                }
                x += 24;
            }
        }
        let ax = fx(0.5);
        for y in floor - 8..floor {
            for x in ax - 9..ax + 9 {
                l.set(x, y, if y < floor - 5 { Rgb::hex(0xf0ece0) } else { Rgb::hex(0x6a4a2a) });
            }
        }
        for k in [-6, -2, 2, 6] {
            for y in floor - 12..floor - 8 {
                l.set(ax + k, y, Rgb::hex(0xf4efe6));
            }
            self.candles.push((ax + k, floor - 12));
        }
        self.things.push((Thing::Candles, ax, (floor - 12) / 2 - 2));
        let ox = fx(0.86).min(w - 26);
        for i in 0..11 {
            let pxp = ox + i * 2;
            let hgt = 14 + ((5 - (i - 5).abs()) * 3);
            for y in floor - 10 - hgt..floor - 10 {
                l.set(pxp, y, Rgb::hex(0xc8c0a8) * (0.8 + 0.25 * ((i % 2) as f32)));
            }
        }
        for y in floor - 10..floor {
            for x in ox - 2..ox + 24 {
                l.set(x, y, Rgb::hex(0x3a2214));
            }
        }
        self.things.push((Thing::Organ, ox + 11, (floor - 40) / 2));
        self.the_tree(fx(0.66));
        self.spots.push((ax as f32, Doing::Busy));
        self.spots.push((fx(0.3) as f32, Doing::Look));
    }

    // ---------------------------------------------------------------- live

    /// Walk around inside. Returns true when the player walks out the front
    /// door (rooms of the home are changed in here).
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), p: &mut Player) -> bool {
        if !self.greeted {
            self.greeted = true;
            if let Some(r) = self.residents.first() {
                let pet = self.pet.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "cat".into());
                let line = if self.church { (*self.rng.pick(CHURCH_HI)).to_string() } else { self.rng.pick(HOST_HI).replace("{pet}", &pet) };
                let (hx, hy) = (r.x.round() as i32 + 1, r.feet - 9);
                self.director.say(hx, hy, &line, t + 0.6, 3.4, 40);
                self.director.sound(Sfx::Greet, r.x, 0.8);
            } else if self.home {
                self.director.say(17, self.floor / 2 - 6, "home sweet home ✦", t + 0.4, 2.6, 40);
            }
        }
        // on the stairs: the climb walks the villager (no keys)
        if let Some(c) = &self.climbing {
            let (x, y, dir, u) = c.at(t);
            p.x = x - 1.0;
            p.y = y - c.origin;
            p.dir = dir;
            p.moving = true;
            p.active_at = t;
            if u >= 1.0 {
                self.climbing = None;
                let title = self.kind().title();
                self.director.say(p.centre().round() as i32, p.y as i32 - 9, title, t, 1.6, 40);
            }
            self.update_people(dt, t, p);
            self.update_pet(dt, t, p.centre());
            self.director.update(dt, t, &self.ctx);
            return false;
        }
        // in bed: any direction key gets up again
        if self.you_lie.is_some() {
            if mv.0 != 0.0 || mv.1 != 0.0 {
                self.you_lie = None;
                self.nap_until = 0.0;
                if let Some((bx, floor)) = self.bed {
                    p.x = (bx + 20) as f32;
                    p.y = (floor / 2 + 7) as f32;
                }
            } else {
                p.moving = false;
                if let Some((bx, floor)) = self.bed {
                    p.x = (bx + 8) as f32;
                    p.y = (floor / 2) as f32;
                }
            }
        }
        let (mx, my) = if p.free(t) && t >= self.nap_until && self.you_lie.is_none() { mv } else { (0.0, 0.0) };
        // sitting: any direction key gets up again
        if let Some(i) = self.you_sit {
            if mx != 0.0 || my != 0.0 {
                self.you_sit = None;
                if let Some(s) = self.seats.get_mut(i) {
                    s.by = None;
                    p.y = (s.feet + 4) as f32;
                }
            } else if let Some(s) = self.seats.get(i) {
                p.x = s.x as f32 - 1.0;
                p.y = s.feet as f32;
            }
        }
        if self.you_lie.is_some() {
            self.update_people(dt, t, p);
            self.update_pet(dt, t, p.centre());
            self.director.update(dt, t, &self.ctx);
            return false;
        }
        p.moving = self.you_sit.is_none() && (mx != 0.0 || my != 0.0);
        if p.moving {
            p.active_at = t;
            if mx != 0.0 {
                p.dir = mx.signum();
            }
            let k = p.pace();
            p.x += mx * 11.0 * k * dt;
            let (lo, hi) = self.walk_rows();
            p.y = (p.y + my * 5.0 * k.sqrt() * dt).clamp(lo, hi);
        }
        // the home's rooms run into each other at the edges
        if self.home {
            let k = self.kind();
            if k == RoomKind::Parlour && mx > 0.0 && p.x >= (self.w - 5) as f32 {
                self.go_to(1, p, true);
            } else if k == RoomKind::Kitchen && mx < 0.0 && p.x <= 2.0 {
                self.go_to(0, p, false);
            }
        }
        p.x = p.x.clamp(1.0, (self.w - 4) as f32);
        self.update_people(dt, t, p);
        self.update_pet(dt, t, p.centre());
        if let Some(done) = self.baking {
            if t >= done {
                self.baking = None;
                self.cookies_ready = true;
                if let Some((ox, oy)) = self.oven {
                    self.director.burst(ox as f32, (oy / 2) as f32, 14, &[Rgb::hex(0xffd36b), Rgb::hex(0xc8803a)], 3.0, 1.0);
                    self.director.say(ox, oy / 2 - 6, "ding! the cookies are ready", t, 2.6, 40);
                    self.director.sound(Sfx::Chime, ox as f32, 0.7);
                }
            }
        }
        self.director.update(dt, t, &self.ctx);
        // walking back out through the front door
        matches!(self.kind(), RoomKind::Parlour | RoomKind::Church) && mx < 0.0 && p.x <= 1.5
    }

    /// Into another room of the home (`from_left`: walked in from its left).
    fn go_to(&mut self, room: usize, p: &mut Player, from_left: bool) {
        if room >= self.rooms.len() {
            return;
        }
        self.room_i = room;
        self.build();
        p.x = if from_left { 6.0 } else { (self.w - 8) as f32 };
        self.director.sound(Sfx::Click, p.x, 0.5);
    }

    /// Previews: straight into a room.
    pub fn preview_room(&mut self, room: usize, p: &mut Player) {
        if room < self.rooms.len() && room != self.room_i {
            self.room_i = room;
            self.build();
            p.x = self.w as f32 * 0.4;
            p.y = (self.floor / 2 + 7) as f32;
        }
    }

    /// Up or down the stairs in the home: the villager walks up the steps
    /// (or the ladder) and the view follows them into the room above or
    /// below — the two rooms stacked, the one left behind as it looked.
    fn climb(&mut self, up: bool, p: &mut Player, t: f32) {
        let next = match (self.kind(), up) {
            (RoomKind::Parlour, true) => Some(2),
            (RoomKind::Bedroom, false) => Some(0),
            (RoomKind::Bedroom, true) => Some(3),
            (RoomKind::Attic, false) => Some(2),
            _ => None,
        };
        let Some(room) = next else { return };
        if self.climbing.is_some() {
            return;
        }
        let from = self.kind();
        let (h, floor) = (self.h as f32, self.floor as f32);
        let lo = self.walk_rows().0;
        let (xb, xt, _) = self.stair_geom();
        let (xb, xt, xl) = (xb as f32, xt as f32, self.ladder_x() as f32);
        let start = (p.centre(), p.y);
        // (path in this room's rows; the new room sits `origin` rows away)
        // (arriving up through the floor: a step in from the front edge,
        // on the floor whatever the room's size)
        let (a1, a2) = ((h - 6.0).max(lo) - h, (h - 8.0).max(lo + 1.0).min(h - 1.0) - h);
        let (origin, path) = match (from, up) {
            (RoomKind::Parlour, _) => (-h, vec![start, (xb - 2.0, lo), (xb + 1.0, floor / 2.0 - 2.0), (xt - 1.0, 0.0), (xt - 4.0, a1), (xt - 9.0, a2)]),
            (RoomKind::Bedroom, false) => (h, vec![start, (xt - 4.0, h - 6.0), (xt - 1.0, h), (xb + 1.0, h + floor / 2.0 - 2.0), (xb - 3.0, h + lo + 1.0)]),
            (RoomKind::Bedroom, true) => (-h, vec![start, (xl, lo), (xl, floor / 2.0), (xl, 0.0), (xl, a1), (xl + 6.0, a2)]),
            _ => (h, vec![start, (xl, h - 6.0), (xl, h), (xl, h + floor / 2.0), (xl + 5.0, h + lo + 1.0)]),
        };
        // the room being left, as it looks right now
        let mut snap = Frame::new(self.w.max(1) as usize, self.h.max(1) as usize);
        self.draw_room(&mut snap, t, 1.0, None);
        self.you_sit = None;
        self.you_lie = None;
        self.room_i = room;
        self.build();
        let mut lens = vec![0.0];
        for w in path.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (2.0 * (w[1].1 - w[0].1)).powi(2)).sqrt();
            lens.push(lens.last().unwrap() + d);
        }
        let total = *lens.last().unwrap();
        let pace = p.pace().max(0.5);
        let dur = (total / (24.0 * pace.sqrt())).clamp(1.6, 4.5);
        // the camera pans over the climb proper (after the walk to the foot)
        let (y0, y1) = (path[1].1, path[path.len() - 1].1);
        self.climbing = Some(Climb { start: t, dur, origin, path, lens, cam: (y0, y1), snap_px: snap.px, snap_light: snap.light, snap_glyph: snap.glyph });
        self.director.sound(Sfx::Click, p.x, 0.4);
    }

    /// Mid-climb: how far the view has moved (rows, toward the new room's
    /// `origin`), for drawing and for the zoomed view.
    pub fn view_shift(&self, t: f32) -> f32 {
        self.climbing.as_ref().map(|c| c.origin - c.view(c.at(t).1)).unwrap_or(0.0)
    }

    /// What's in the room to poke at.
    pub fn things_at(&self) -> Vec<(Thing, i32)> {
        self.things.iter().map(|t| (t.0, t.1)).collect()
    }

    /// Walking the stairs right now.
    pub fn is_climbing(&self) -> bool {
        self.climbing.is_some()
    }

    /// In bed.
    pub fn is_lying(&self) -> bool {
        self.you_lie.is_some()
    }

    /// The foot of the bed (column, row), for a pet to curl up at.
    pub fn bed_foot(&self) -> Option<(f32, f32)> {
        self.bed.map(|(bx, floor)| ((bx + 38) as f32, (floor / 2 + 1) as f32))
    }

    /// The household potters about: off to a favourite spot, a while there,
    /// then somewhere else. Anyone talking to the player stays put and
    /// faces them.
    fn update_people(&mut self, dt: f32, t: f32, p: &Player) {
        let px = p.centre();
        let spots = self.spots.clone();
        for i in 0..self.residents.len() {
            let r = &mut self.residents[i];
            if r.doing == Doing::Talk {
                r.dir = (px - r.x).signum();
                if t > r.until {
                    r.doing = Doing::Stand;
                    r.until = t + 2.0;
                }
                continue;
            }
            if r.doing == Doing::Walk {
                let dx = r.target - r.x;
                if dx.abs() < 0.6 {
                    r.x = r.target;
                    r.doing = r.then;
                    r.until = t + if r.kid { 6.0 } else { 9.0 } + h01(hash2(i as i32, t as i32)) * 10.0;
                    if let Doing::Sit(s) = r.doing {
                        match self.seats.get_mut(s) {
                            Some(seat) if seat.by.is_none() => seat.by = Some(Some(i)),
                            _ => r.doing = Doing::Stand,
                        }
                    }
                } else {
                    r.dir = dx.signum();
                    r.x += r.dir * (if r.kid { 6.0 } else { 4.0 }) * dt;
                }
                continue;
            }
            if t < r.until {
                continue;
            }
            // done here: get up, pick somewhere new
            if let Doing::Sit(s) = r.doing {
                if let Some(seat) = self.seats.get_mut(s) {
                    seat.by = None;
                }
            }
            if spots.is_empty() || self.church && i == 0 && h01(hash2(t as i32, 7)) < 0.5 {
                r.doing = Doing::Stand;
                r.until = t + 5.0;
                continue;
            }
            let k = (hash2(t as i32 * 31 + i as i32, self.seed as i32) as usize) % spots.len();
            let (sx, then) = spots[k];
            let then = if r.kid && h01(hash2(t as i32, i as i32)) < 0.4 { Doing::Play } else { then };
            r.target = (sx + (i as f32 - 1.0) * 3.0).clamp(24.0, self.w as f32 - 12.0);
            r.then = then;
            r.doing = Doing::Walk;
        }
        // keep the [E] targets on the people as they move
        for th in &mut self.things {
            if let Thing::Person(i) = th.0 {
                if let Some(r) = self.residents.get(i) {
                    th.1 = r.x.round() as i32 + 1;
                    th.2 = r.feet - 10;
                }
            }
        }
    }

    /// The house cat / dog: naps, wanders, sits by the fire, chases a ball,
    /// comes to say hello.
    fn update_pet(&mut self, dt: f32, t: f32, px: f32) {
        let fire = self.hearth.map(|(x, _, w, _)| (x + w / 2) as f32);
        let wmax = self.w as f32 - 10.0;
        let Some(pet) = &mut self.pet else { return };
        if let Some((bx, vx)) = &mut pet.ball {
            *bx += *vx * dt;
            *vx *= (1.0 - dt * 0.8).max(0.0);
            if *bx < 20.0 || *bx > wmax {
                *vx = -*vx;
                *bx = bx.clamp(20.0, wmax);
            }
            pet.target = *bx;
            pet.pose = Pose::Walk;
            if (pet.x - *bx).abs() < 2.0 && vx.abs() < 4.0 {
                pet.ball = None;
                pet.happy_until = t + 2.0;
                pet.until = t + 3.0;
                pet.pose = Pose::Sit;
            }
        } else if t > pet.until && !pet.hiding {
            let r = h01(hash2(t as i32, pet.name.len() as i32));
            if r < 0.3 {
                pet.pose = Pose::Sleep;
                pet.until = t + 12.0 + r * 20.0;
            } else if r < 0.55 {
                pet.target = px + 6.0;
                pet.pose = Pose::Walk;
                pet.until = t + 8.0;
            } else if r < 0.8 {
                pet.target = fire.map(|f| f - 10.0).unwrap_or(pet.x);
                pet.pose = Pose::Walk;
                pet.until = t + 10.0;
            } else {
                pet.target = 24.0 + r * (wmax - 24.0);
                pet.pose = Pose::Walk;
                pet.until = t + 6.0;
            }
        }
        let dx = pet.target - pet.x;
        if pet.pose == Pose::Walk || pet.ball.is_some() {
            if dx.abs() > 1.0 {
                let v = if pet.ball.is_some() { 26.0 } else { 9.0 };
                pet.x += dx.signum() * (v * dt).min(dx.abs());
            } else if pet.ball.is_none() {
                pet.pose = Pose::Sit;
            }
        }
        for th in &mut self.things {
            if th.0 == Thing::Pet {
                th.1 = pet.x.round() as i32;
            }
        }
        if !self.things.iter().any(|th| th.0 == Thing::Pet) && !pet.hiding {
            self.things.push((Thing::Pet, pet.x.round() as i32, pet.feet - 5));
        }
        self.things.retain(|th| th.0 != Thing::Pet || !self.pet.as_ref().is_some_and(|p| p.hiding));
    }

    /// What's within reach of the player, nearest first.
    pub fn nearest(&self, x: f32) -> Option<(Thing, i32, i32)> {
        self.things
            .iter()
            .filter(|(_, tx, _)| (*tx as f32 - x).abs() <= 7.0)
            .min_by(|a, b| (a.1 as f32 - x).abs().partial_cmp(&(b.1 as f32 - x).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .copied()
    }

    /// Whatever's at (x, y) on screen (for clicks).
    pub fn thing_at(&self, x: i32, y: i32) -> Option<Thing> {
        self.things.iter().find(|(_, tx, ty)| (tx - x).abs() <= 4 && y >= ty - 1 && y <= ty + 10).map(|(k, ..)| *k)
    }

    /// Someone at home stops to talk (facing the villager) for a while.
    pub fn hold_talk(&mut self, i: usize, t: f32) {
        if let Some(r) = self.residents.get_mut(i) {
            if let Doing::Sit(_) = r.doing {
                // stays sitting, just turns to talk
                r.until = r.until.max(t + 3.0);
                return;
            }
            if r.doing == Doing::Walk {
                r.target = r.x;
            }
            r.doing = Doing::Talk;
            r.until = t + 3.0;
        }
    }

    /// (column, head row) of someone at home, for speech bubbles.
    pub fn head_of(&self, i: usize) -> Option<(i32, i32)> {
        self.residents.get(i).map(|r| (r.x.round() as i32 + 1, r.feet - 9 + if matches!(r.doing, Doing::Sit(_)) { 3 } else { 0 }))
    }

    /// Sit the villager down in the nearest free seat (true if there was one).
    pub fn sit_down(&mut self, p: &mut Player) -> bool {
        let x = p.centre();
        let best = self.seats.iter().enumerate().filter(|(_, s)| s.by.is_none()).min_by_key(|(_, s)| (s.x as f32 - x).abs() as i32).map(|(i, _)| i);
        if let Some(i) = best {
            self.sit_in(i, p);
            return true;
        }
        false
    }

    fn sit_in(&mut self, i: usize, p: &mut Player) {
        let Some(s) = self.seats.get_mut(i) else { return };
        if s.by.is_some() {
            return;
        }
        s.by = Some(None);
        self.you_sit = Some(i);
        p.x = s.x as f32 - 1.0;
        p.y = s.feet as f32;
    }

    /// The pet: hide somewhere (a host asked the villager to find it).
    pub fn hide_pet(&mut self) {
        if let Some(p) = &mut self.pet {
            p.hiding = true;
        }
    }

    /// The star falls off the tree (a host asked for it to be put back).
    pub fn drop_star(&mut self) {
        self.star_off = true;
    }

    /// E on something in the room.
    pub fn poke(&mut self, what: Thing, p: &mut Player, t: f32) -> Outcome {
        let (px, py) = (p.centre().round() as i32, p.y as i32 - 9);
        // finishing a job someone asked for
        if let Some((task, who)) = self.task {
            if task.done_by(what) {
                self.task = None;
                if task == Task::FindPet {
                    if let Some(pet) = &mut self.pet {
                        pet.hiding = false;
                        pet.x = px as f32 + 6.0;
                        pet.happy_until = t + 3.0;
                    }
                }
                if task == Task::Star {
                    self.star_off = false;
                }
                if task == Task::Stocking {
                    self.stocking_hung = true;
                }
                let thanks = ["oh, thank you! you're a gem", "perfect! now it feels like Christmas", "you're an angel. a snowy angel", "wonderful — thank you, dear"][hash(t as u32) as usize % 4];
                if let Some((hx, hy)) = self.head_of(who) {
                    self.director.say(hx, hy, thanks, t + 0.6, 3.0, 40);
                }
                self.director.sound(Sfx::Fanfare, px as f32, 0.4);
                self.director.burst(px as f32, py as f32, 16, &[Rgb::hex(0xffd36b), Rgb::WHITE], 4.0, 2.0);
                return Outcome::Cheer(3);
            }
        }
        let d = &mut self.director;
        let say = |d: &mut Director, rng: &mut Rng, pool: &[&str]| {
            let line = *rng.pick(pool);
            d.say(px, py, line, t, 3.0, 40);
        };
        match what {
            Thing::Door => return Outcome::Leave,
            Thing::Doorway(right) => {
                self.go_to(if right { 1 } else { 0 }, p, right);
            }
            Thing::Stairs(up) => self.climb(up, p, t),
            Thing::Person(i) => return Outcome::Talk(i),
            Thing::Fire => {
                if let Some((ox, oy, ow, _)) = self.hearth {
                    let sparks = [Rgb::hex(0xffb040), Rgb::hex(0xffe080), Rgb::hex(0xff6020)];
                    d.burst((ox + ow / 2) as f32, (oy / 2) as f32, 18, &sparks, 4.0, -2.0);
                }
                d.sound(Sfx::Sparkle, px as f32, 0.5);
                say(d, &mut self.rng, FIRE);
                return Outcome::Cozy;
            }
            Thing::Tree => {
                if let Some(tr) = &mut self.tree {
                    tr.boost_until = t + 3.0;
                    self.ornaments += 1;
                    d.sound(Sfx::Starburst, tr.cx as f32, 0.8);
                    let (sx, sy) = tr.star();
                    d.burst(sx as f32, sy as f32, 30, &[Rgb::hex(0xffd65a), Rgb::WHITE], 8.0, 3.0);
                    let line = if self.ornaments == 1 { "I hung a little glass bird ✦" } else { "another ornament — perfect" };
                    d.say(px, py, line, t, 2.6, 40);
                }
            }
            Thing::Pet => {
                if let Some(pet) = &mut self.pet {
                    pet.happy_until = t + 3.0;
                    if pet.pose == Pose::Sleep {
                        pet.pose = Pose::Sit;
                    }
                    d.emote(pet.x, (pet.feet - 6) as f32, '♥', Rgb::hex(0xff6b8a), 3);
                    d.sound(
                        match pet.kind {
                            PetKind::Dog => Sfx::Bark,
                            PetKind::Cat => Sfx::Meow,
                            PetKind::Bunny => Sfx::Pet,
                        },
                        pet.x,
                        0.7,
                    );
                    // the dog gets a ball thrown; the cat chases a bit of string
                    if pet.kind != PetKind::Bunny && self.rng.chance(0.6) {
                        let dir = if pet.x > self.w as f32 / 2.0 { -1.0 } else { 1.0 };
                        pet.ball = Some((pet.x, dir * 40.0));
                        d.say(px, py, if pet.kind == PetKind::Dog { "fetch, {n}!" } else { "get the string, {n}!" }.replace("{n}", &pet.name).as_str(), t, 2.0, 40);
                    } else {
                        let line = match pet.kind {
                            PetKind::Dog => "good dog! yes you are!",
                            PetKind::Cat => "prrrrr…",
                            PetKind::Bunny => "*soft bunny nose*",
                        };
                        d.say(px, py, line, t, 2.0, 40);
                    }
                }
            }
            Thing::Cocoa => {
                d.sound(Sfx::Munch, px as f32, 0.6);
                say(d, &mut self.rng, COCOA);
                return Outcome::Cozy;
            }
            Thing::Window(i) => {
                if let Some((_, look)) = self.windows.get(i) {
                    return Outcome::Look(*look);
                }
            }
            Thing::Telescope => {
                return Outcome::Look(Look { dx: -0.3, lift: (self.h * 3) as f32, hole: Hole::Round(0.5, 0.45, 0.42), telescope: true });
            }
            Thing::Seat(i) => {
                self.sit_in(i, p);
                if self.you_sit.is_some() {
                    say(&mut self.director, &mut self.rng, SIT);
                } else {
                    self.director.say(px, py, "someone's sitting there", t, 2.0, 40);
                }
            }
            Thing::Shelf => {
                let line = *self.rng.pick(BOOKS);
                d.say(px, py, line, t, 5.5, 40);
                d.sound(Sfx::Rustle, px as f32, 0.4);
            }
            Thing::Photo => {
                let kid = self.residents.iter().find(|r| r.kid).map(|r| r.name).unwrap_or("our niece");
                let line = self.rng.pick(PHOTOS).replace("{kid}", kid);
                if let Some((hx, hy)) = self.residents.first().map(|r| (r.x.round() as i32 + 1, r.feet - 9)) {
                    self.director.say(px, py, "who's this in the photo?", t, 1.8, 40);
                    self.director.say(hx, hy, &line, t + 2.0, 3.6, 40);
                } else {
                    self.director.say(px, py, "a happy memory ✦", t, 2.4, 40);
                }
            }
            Thing::Clock => {
                d.sound(Sfx::Click, px as f32, 0.6);
                say(d, &mut self.rng, CLOCK);
            }
            Thing::Piano | Thing::Organ | Thing::Gramophone | Thing::Radio | Thing::MusicBox => {
                self.music_until = t + 8.0;
                let x = self.things.iter().find(|k| k.0 == what).map(|k| k.1).unwrap_or(px);
                d.notes(x as f32, (self.floor / 2 - 8) as f32, 10);
                let sfx = match what {
                    Thing::Organ => Sfx::ChurchPeal,
                    Thing::MusicBox => Sfx::Chime,
                    _ => Sfx::Chime,
                };
                d.sound(sfx, x as f32, 0.9);
                let line = match what {
                    Thing::Radio => "♪ …and now, a little Bing for a snowy night ♪",
                    Thing::Gramophone => "♪ crackle… a waltz ♪",
                    Thing::MusicBox => "♪ a tinkly little carol ♪",
                    _ => "♪ ♫ ♪",
                };
                d.say(px, py, line, t, 2.8, 40);
            }
            Thing::Candles => {
                d.sound(Sfx::Chime, px as f32, 0.5);
                say(d, &mut self.rng, CANDLES);
            }
            Thing::Oven => {
                if self.cookies_ready {
                    self.cookies_ready = false;
                    d.sound(Sfx::Present, px as f32, 0.6);
                    d.burst(px as f32, py as f32 + 6.0, 10, &[Rgb::hex(0xc8803a), Rgb::hex(0xffd36b)], 2.0, 1.5);
                    d.say(px, py, "warm gingerbread! three for my pockets ✦", t, 2.8, 40);
                    return Outcome::Gifts(Item::Gingerbread, 3);
                }
                if self.baking.is_some() {
                    d.say(px, py, "not yet… smells amazing though", t, 2.2, 40);
                } else {
                    self.baking = Some(t + 8.0);
                    d.say(px, py, "into the oven they go!", t, 2.2, 40);
                    d.sound(Sfx::Click, px as f32, 0.6);
                }
            }
            Thing::CookieJar => {
                d.sound(Sfx::Munch, px as f32, 0.6);
                d.say(px, py, "a gingerbread for later ✦", t, 2.4, 40);
                return Outcome::Gift(Item::Gingerbread);
            }
            Thing::Workbench => {
                d.sound(Sfx::Click, px as f32, 0.8);
                d.burst(px as f32 + 4.0, py as f32 + 4.0, 8, &[Rgb::hex(0xc8a060)], 2.0, 2.0);
                d.say(px, py, "I whittled a little wooden star ✦", t, 2.8, 40);
                return Outcome::Cheer(1);
            }
            Thing::Stocking => {
                if !self.stocking_found {
                    self.stocking_found = true;
                    d.say(px, py, "a candy cane peeking out — for me?", t, 2.6, 40);
                    d.sound(Sfx::Present, px as f32, 0.6);
                    return Outcome::Gift(Item::CandyCane);
                }
                d.say(px, py, "the stockings are all hung with care", t, 2.4, 40);
            }
            Thing::Bed => {
                if let Some((bx, floor)) = self.bed {
                    // tucked in: head on the pillow (any direction key gets up)
                    self.you_lie = Some(t);
                    p.x = (bx + 8) as f32;
                    p.y = (floor / 2) as f32;
                    p.dir = 1.0;
                }
                d.say(px, py, "just a little nap… zzz", t, 2.4, 40);
                return Outcome::Cozy;
            }
            Thing::Boxes => {
                say(d, &mut self.rng, BOXES);
                d.sound(Sfx::Rustle, px as f32, 0.5);
            }
        }
        Outcome::None
    }

    // ---------------------------------------------------------------- draw

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, p: &Player, prompt: Option<(i32, i32)>) {
        if let Some(c) = &self.climbing {
            // on the stairs: the room left behind and the one ahead, stacked,
            // the view following the villager between them
            let v = c.view(c.at(t).1);
            f.ox = 0;
            f.paste(&c.snap_px, &c.snap_light, &c.snap_glyph, self.w.max(1) as usize, (self.h * 2).max(1) as usize, (-v).round() as i32);
            f.oy = (c.origin - v).round() as i32;
            self.draw_room(f, t, glow, Some((p, None)));
            f.oy = 0;
            return;
        }
        self.draw_room(f, t, glow, Some((p, prompt)));
    }

    /// The room and everyone in it (`you`: the villager and where the [E]
    /// prompt goes; None for a snapshot of the room without them).
    fn draw_room(&self, f: &mut Frame, t: f32, glow: f32, you: Option<(&Player, Option<(i32, i32)>)>) {
        f.ox = 0;
        self.room.composite_from(f, 0);
        // snow falling past the windows (and a deep night sky)
        for (i, &((x0, y0, ww, wh), look)) in self.windows.iter().enumerate() {
            let round = matches!(look.hole, Hole::Round(..));
            for y in y0..y0 + wh {
                for x in x0..x0 + ww {
                    if round {
                        let (cx, cy, r) = ((x0 + ww / 2) as f32, (y0 + wh / 2) as f32, (wh / 2) as f32);
                        if (((x as f32 - cx) / 2.0).powi(2) + (y as f32 - cy).powi(2)).sqrt() > r {
                            continue;
                        }
                    }
                    let k = (y - y0) as f32 / wh as f32;
                    let c = if self.church {
                        let pal = [0x4a7bff, 0xff4a5a, 0xffc84a, 0x5adf8a, 0xb05aff];
                        let cell = hash3(x / 3, y / 4, i as i32) % 5;
                        Rgb::hex(pal[cell as usize]) * (0.55 + 0.2 * noise1(t * 0.5 + x as f32 * 0.1, 7))
                    } else {
                        Rgb::hex(0x0e1838).lerp(Rgb::hex(0x2a3a6a), k)
                    };
                    f.px_set(x, y, c);
                }
            }
            if self.church {
                for y in y0..y0 + wh {
                    if (y - y0) % 4 == 0 {
                        for x in x0..x0 + ww {
                            f.px_set(x, y, Rgb::hex(0x1a1820));
                        }
                    }
                }
                f.light_px((x0 + ww / 2) as f32, (y0 + wh / 2) as f32, Rgb::hex(0xb090ff) * (2.0 * glow));
                continue;
            }
            if !round {
                for y in y0..y0 + wh {
                    f.px_set(x0 + ww / 2, y, Rgb::hex(0xe8e0d0));
                }
                for x in x0..x0 + ww {
                    f.px_set(x, y0 + wh / 2, Rgb::hex(0xe8e0d0));
                }
            }
            for k in 0..14 {
                let sx = x0 + ((h01(hash2(k, i as i32)) * ww as f32 + t * (1.5 + h01(hash2(k, 5)) * 2.0)) % ww as f32) as i32;
                let sy = y0 + ((h01(hash2(k, 9 + i as i32)) * wh as f32 + t * (3.0 + h01(hash2(k, 3)) * 3.0)) % wh as f32) as i32;
                f.px_set(sx, sy, Rgb::hex(0xeef4ff));
            }
        }
        // the fire
        if let Some((ox, oy, ow, oh)) = self.hearth {
            for y in oy..oy + oh {
                for x in ox..ox + ow {
                    let up = (oy + oh - y) as f32 / oh as f32;
                    let n = noise1(t * 5.0 + x as f32 * 0.9 + y as f32 * 0.35, self.seed) * 0.6 + noise1(t * 9.0 - y as f32 * 0.5 + x as f32 * 0.3, self.seed ^ 3) * 0.4;
                    let mid = 1.0 - ((x - ox) as f32 / ow as f32 - 0.5).abs() * 2.0;
                    let heat = (n * 1.25 + mid * 0.35 - up * 1.1).clamp(0.0, 1.0);
                    if heat > 0.12 {
                        let c = if heat > 0.75 {
                            Rgb::hex(0xffe8a0)
                        } else if heat > 0.45 {
                            Rgb::hex(0xffa030)
                        } else {
                            Rgb::hex(0xd8401a)
                        };
                        f.px_set(x, y, c * (0.7 + heat * 0.5));
                    }
                }
            }
            for x in ox + 2..ox + ow - 2 {
                f.px_set(x, oy + oh - 1, Rgb::hex(0x3a2214));
                if x % 5 != 0 {
                    f.px_set(x, oy + oh - 2, Rgb::hex(0x4a2a18));
                }
            }
            let fl = 0.85 + 0.15 * noise1(t * 8.0, 41) + 0.08 * (t * 23.0).sin();
            let (cx, cy) = ((ox + ow / 2) as f32, (oy + oh / 2) as f32);
            f.light_px(cx, cy, Rgb::hex(0xff8a30) * (14.0 * fl * glow));
            f.light_px(cx, cy - 10.0, Rgb::hex(0xffa040) * (6.0 * fl * glow));
        }
        // the oven glows while baking
        if let (Some((ox, oy)), Some(done)) = (self.oven, self.baking) {
            let k = 0.6 + 0.4 * (t * 3.0).sin();
            f.light_px(ox as f32, oy as f32, Rgb::hex(0xff9030) * (5.0 * k * glow));
            for x in ox - 4..ox + 4 {
                for y in oy - 2..oy + 2 {
                    f.px_set(x, y, Rgb::hex(0xff8a30) * (0.6 + 0.3 * k));
                }
            }
            if (done - t) < 2.0 {
                for k in 0..3 {
                    let age = (t * 0.8 + k as f32 / 3.0).fract();
                    f.px_disc(ox as f32 + (age * 5.0).sin() * 2.0, oy as f32 - 14.0 - age * 8.0, 0.8 + age, Rgb::hex(0xe8eaf0), (1.0 - age) * 0.4);
                }
            }
        }
        if let Some((mx, my)) = self.mug {
            for k in 0..3 {
                let age = (t * 0.7 + k as f32 / 3.0).fract();
                let x = mx as f32 + (age * 6.0 + k as f32).sin() * 1.2;
                f.px_disc(x, my as f32 - 1.0 - age * 8.0, 0.6 + age, Rgb::hex(0xe8eaf0), (1.0 - age) * 0.4);
            }
        }
        for &(x, y, c, ph) in &self.bulbs {
            let k = 0.55 + 0.45 * (t * 1.7 + ph).sin();
            f.px_set(x, y, c * (0.6 + 0.6 * k));
            f.light_px(x as f32 + 0.5, y as f32 + 0.5, c * (0.9 * k * glow));
        }
        for &(x, y) in &self.lamps {
            f.light_px(x as f32 + 0.5, y as f32, Rgb::hex(0xffc878) * (5.0 * glow));
        }
        for (i, &(x, y)) in self.candles.iter().enumerate() {
            let fl = 0.75 + 0.25 * noise1(t * 9.0 + i as f32 * 3.0, 13);
            f.px_set(x, y, Rgb::hex(0xf4efe6));
            f.px_set(x, y - 1, Rgb::hex(0xffd070) * (1.0 + 0.3 * fl));
            f.px_set(x, y - 2, Rgb::hex(0xfff0b0) * fl);
            f.light_px(x as f32 + 0.5, y as f32 - 1.5, Rgb::hex(0xffb050) * (2.2 * fl * glow));
        }
        if let Some((cx, cy)) = self.clock {
            let sw = (t * std::f32::consts::PI).sin() * 2.0;
            for k in 0..14 {
                f.px_set(cx + (sw * k as f32 / 14.0).round() as i32, cy + 6 + k, Rgb::hex(0xb8902a));
            }
            f.put_g(cx, cy / 2, Glyph::new('◷', Rgb::hex(0xf0e8d0)).lit(0.6));
        }
        // the tree (its star knocked off, if a host is waiting for help)
        if let Some(tr) = &self.tree {
            tr.draw(f, t, glow, 0.0);
            if self.star_off {
                let (sx, sy) = tr.star();
                f.put_g(sx, sy, Glyph::new(' ', Rgb::BLACK).alpha(0.0));
                f.put_g(sx + 6, self.floor / 2 + 3, Glyph::new('★', Rgb::hex(0xffd65a)).lit(0.0));
            }
        }
        // a wooden train going round the tree
        if let Some((tx, ty)) = self.toy_train {
            let a = t * 0.8;
            for k in 0..4 {
                let u = a - k as f32 * 0.35;
                let (x, y) = (tx as f32 + u.cos() * 22.0, ty as f32 + u.sin() * 3.0);
                let c = if k == 0 { Rgb::hex(0xc8202a) } else { [Rgb::hex(0x2a5aa8), Rgb::hex(0xc8a040), Rgb::hex(0x2a8a4a)][k % 3] };
                for dx in 0..3 {
                    for dy in 0..2 {
                        f.px_set(x as i32 + dx, y as i32 + dy, c);
                    }
                }
            }
        }
        // the pet
        if let Some(pet) = &self.pet {
            if !pet.hiding {
                let pose = if t < pet.happy_until { Pose::Up } else { pet.pose };
                let dir = if (pet.target - pet.x).abs() > 1.0 { (pet.target - pet.x).signum() } else { 1.0 };
                pets::draw(f, pet.kind, pet.body, pet.marks, pet.x, pet.feet * 2 + 1, dir, pose, t, 2, Some(Rgb::hex(0xc8202a)));
                if let Some((bx, _)) = pet.ball {
                    f.px_disc(bx, (pet.feet * 2 - 1) as f32, 1.4, if pet.kind == PetKind::Dog { Rgb::hex(0xc8e040) } else { Rgb::hex(0xc8202a) }, 1.0);
                }
                if pose == Pose::Sleep && (t * 0.8) as i32 % 2 == 0 {
                    f.put_g(pet.x as i32 + 4, pet.feet - 5, Glyph::new('z', Rgb::hex(0xc8d0e0)).lit(0.0));
                }
            }
        }
        // a tray of cookies cooling on the oven
        if let (true, Some((ox, oy))) = (self.cookies_ready, self.oven) {
            // (the oven's centre column; its top is 11 px above `oy`)
            let (x0, top) = (ox - 6, oy - 11);
            for x in x0..x0 + 13 {
                f.px_set(x, top - 1, Rgb::hex(0x9aa0a8));
            }
            for k in 0..4 {
                let x = x0 + 1 + k * 3;
                f.px_set(x, top - 2, Rgb::hex(0xc8803a));
                f.px_set(x + 1, top - 2, Rgb::hex(0xa86a30));
            }
            if (t * 2.0) as i32 % 2 == 0 {
                f.put_g(ox, top / 2 - 3, Glyph::new('~', Rgb::hex(0xe8e0d0)).alpha(0.6).lit(0.0));
            }
        }
        // people, back to front
        let climbing = self.climbing.is_some();
        let walking = you.is_some_and(|(p, _)| p.moving && if climbing { (t * 7.0) as i32 % 2 == 0 } else { (p.phase() as i32) % 2 == 0 });
        let mut people: Vec<(i32, Big)> = self
            .residents
            .iter()
            .map(|r| {
                let sit = match r.doing {
                    Doing::Sit(n) => self.seats.get(n).map(|s| s.top),
                    _ => None,
                };
                let step = r.doing == Doing::Walk && (t * 6.0) as i32 % 2 == 0;
                let warm = r.doing == Doing::Warm;
                (r.feet, Big { x: r.x + 1.0, feet: r.feet, dir: r.dir, step, hat: None, hair: r.hair, coat: r.coat, scarf: None, sit, kid: r.kid, arms_up: warm && (t * 2.0) as i32 % 2 == 0, outdoor: false })
            })
            .collect();
        if let (Some((p, _)), None) = (you, self.you_lie) {
            let sit = self.you_sit.and_then(|i| self.seats.get(i)).map(|s| s.top);
            people.push((p.y.round() as i32, Big { x: p.centre(), feet: p.y.round() as i32, dir: p.dir, step: walking, hat: Some(Rgb::hex(0x2fae5c)), hair: Rgb::hex(0x3a2a1a), coat: Rgb::hex(0xe0a030), scarf: Some(Rgb::hex(0xd8283a)), sit, kid: false, arms_up: false, outdoor: false }));
        }
        people.sort_by_key(|(feet, _)| *feet);
        for (_, b) in &people {
            b.draw(f, t);
        }
        // tucked up in bed: head on the pillow, a bump under the quilt
        if let (Some(since), Some((bx, floor)), Some(_)) = (self.you_lie, self.bed, you) {
            let fl = floor;
            for (x, y) in [(bx + 5, fl - 11), (bx + 6, fl - 11), (bx + 5, fl - 10), (bx + 6, fl - 10), (bx + 7, fl - 10), (bx + 6, fl - 12)] {
                f.px_set(x, y, Rgb::hex(0x2fae5c));
            }
            f.px_set(bx + 4, fl - 12, Rgb::hex(0xf4f0ea));
            for y in fl - 11..fl - 7 {
                for x in bx + 8..bx + 11 {
                    f.px_set(x, y, Rgb::hex(0xe8c39e));
                }
            }
            f.px_set(bx + 9, fl - 10, Rgb::hex(0x6a4a3a));
            f.px_set(bx + 11, fl - 8, Rgb::hex(0xd8283a));
            for x in bx + 12..bx + 36 {
                let c = [0xb8402a, 0x2a6a3a, 0xc8a040, 0x2a4a8a][(((x - bx) / 5) % 4) as usize];
                let bump = if x > bx + 31 { 2 } else { 1 };
                for k in 1..=bump {
                    f.px_set(x, fl - 7 - k, Rgb::hex(c) * (0.95 + 0.05 * k as f32));
                }
            }
            let k = ((t - since) / 2.0).clamp(0.0, 1.0);
            for i in 0..3 {
                let a = ((t * 0.6 + i as f32 / 3.0).fract()) * k;
                if a > 0.05 {
                    let ch = if i == 1 { 'Z' } else { 'z' };
                    f.put_g(bx + 10 + (a * 4.0) as i32 + i, fl / 2 - 7 - (a * 5.0) as i32, Glyph::new(ch, Rgb::hex(0xc8d0e0)).alpha(1.0 - a * 0.7).lit(0.0));
                }
            }
        }
        if let Some((p, _)) = you {
            f.light_px(p.centre(), p.y * 2.0 - 8.0, Rgb::hex(0xffc878) * (1.2 * glow));
        }
        // napping (in bed): the room dims
        let lie = self.you_lie.map(|since| ((t - since) / 2.0).clamp(0.0, 1.0) * 0.5).unwrap_or(0.0);
        let nap = if t < self.nap_until { ((self.nap_until - t).min(1.0)).clamp(0.0, 1.0) * 0.55 } else { 0.0 };
        let k = lie.max(nap);
        if k > 0.0 {
            for y in 0..self.h * 2 {
                for x in 0..self.w {
                    let c = f.px_get(x, y);
                    f.px_set(x, y, c * (1.0 - k));
                }
            }
        }
        if let Some((_, Some((x, y)))) = you {
            Player::draw_prompt(f, x, y, 'E', t);
        }
        // the room's name in the home
        if self.home {
            let name = format!(" {} ", self.kind().title());
            f.text(2, 1, &name, Rgb::hex(0xffd36b), crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.7), 0.0);
        }
        self.director.draw_px(f);
        self.director.draw_front(f, t, glow);
        let _ = self.music_until;
    }

    /// Looking out of a window: everything on screen but the opening is the
    /// room — wall, frame, curtains, a candle on the sill. The village shows
    /// through the opening (drawn before this).
    pub fn draw_window_frame(&self, f: &mut Frame, look: &Look, t: f32, glow: f32) {
        let (w, h) = (f.w as i32, f.h as i32);
        let wall = if self.church { Rgb::hex(0x4a4658) } else { self.profile.wall * 0.7 };
        let frame = Rgb::hex(0xe8e0d0);
        let inside = |x: i32, y: i32| -> bool {
            match look.hole {
                Hole::Rect(x0, y0, x1, y1) => x as f32 >= x0 * w as f32 && (x as f32) < x1 * w as f32 && y as f32 >= y0 * h as f32 && (y as f32) < y1 * h as f32,
                Hole::Round(cx, cy, r) => {
                    let (dx, dy) = ((x as f32 + 0.5 - cx * w as f32) / 2.0, y as f32 + 0.5 - cy * h as f32);
                    (dx * dx + dy * dy).sqrt() < r * h as f32
                }
            }
        };
        let edge = |x: i32, y: i32| -> bool { inside(x, y) && (!inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1)) };
        for y in 0..h {
            for x in 0..w {
                if inside(x, y) {
                    // the glass: a faint reflection, frost creeping in at the edges
                    if edge(x, y) {
                        f.put_g(x, y, Glyph::new(' ', frame).solid(frame).lit(0.0));
                    }
                    continue;
                }
                let c = if look.telescope {
                    Rgb::hex(0x050508)
                } else {
                    let stripe = if (x / 6) % 2 == 0 { 1.0 } else { 0.92 };
                    let lamp = 1.0 - ((y as f32 / h as f32) - 0.9).abs() * 0.5;
                    wall * stripe * lamp
                };
                f.put_g(x, y, Glyph::new(' ', c).solid(c).lit(0.0));
            }
        }
        if look.telescope {
            f.text(2, h - 2, " looking through the telescope · E or Esc to look away ", Rgb::hex(0x8a8aa0), crate::frame::Bg::Mix(Rgb::hex(0x050508), 1.0), 0.0);
            return;
        }
        // mullions, curtains and the sill
        match look.hole {
            Hole::Rect(x0, y0, x1, y1) => {
                let (x0, y0, x1, y1) = ((x0 * w as f32) as i32, (y0 * h as f32) as i32, (x1 * w as f32) as i32, (y1 * h as f32) as i32);
                let mx = (x0 + x1) / 2;
                for y in y0..y1 {
                    f.put_g(mx, y, Glyph::new('┃', frame).lit(0.0));
                }
                let my = (y0 + y1) / 2;
                for x in x0..x1 {
                    f.put_g(x, my, Glyph::new('━', frame).lit(0.0));
                }
                f.put_g(mx, my, Glyph::new('╋', frame).lit(0.0));
                // frost in the corners
                for k in 0..6 {
                    for (cx, cy, sx, sy) in [(x0 + 1, y0 + 1, 1, 1), (x1 - 2, y0 + 1, -1, 1), (x0 + 1, y1 - 2, 1, -1), (x1 - 2, y1 - 2, -1, -1)] {
                        let (fx, fy) = (cx + sx * (k % 3) * 2, cy + sy * (k / 3));
                        f.put_g(fx, fy, Glyph::new(if k % 2 == 0 { '❄' } else { '·' }, Rgb::hex(0xe8f0ff)).alpha(0.7).lit(0.0));
                    }
                }
                let cur = self.profile.curtain;
                for y in y0 - 1..y1 + 1 {
                    let sway = ((t * 0.7 + y as f32 * 0.3).sin() * 0.6) as i32;
                    for k in 0..5 {
                        let c = cur * (0.75 + 0.06 * k as f32);
                        f.put_g(x0 - 6 + k + sway, y, Glyph::new('▓', c).solid(c * 0.85).lit(0.0));
                        f.put_g(x1 + 1 + k - sway, y, Glyph::new('▓', c).solid(c * 0.85).lit(0.0));
                    }
                }
                let sill = Rgb::hex(0xd8d0c0);
                for x in x0 - 3..x1 + 3 {
                    f.put_g(x, y1, Glyph::new('▀', sill).solid(sill * 0.7).lit(0.0));
                }
                // a candle on the sill
                let fl = 0.8 + 0.2 * noise1(t * 9.0, 3);
                f.put_g(x1 - 4, y1 - 1, Glyph::new('▮', Rgb::hex(0xf4efe6)).lit(0.0));
                f.put_g(x1 - 4, y1 - 2, Glyph::new('♦', Rgb::hex(0xffd070) * fl).lit(0.0));
                f.light_cell(x1 - 4, y1 - 2, Rgb::hex(0xffb050) * (2.5 * fl * glow));
            }
            Hole::Round(..) => {}
        }
        let hint = " looking out · A/D lean · E or Esc to step back ";
        f.text((w - hint.chars().count() as i32) / 2, h - 2, hint, Rgb::hex(0xc8c0b0), crate::frame::Bg::Mix(wall * 0.6, 0.9), 0.0);
    }

    /// Info about someone at home, for a conversation with them.
    pub fn about(&self, i: usize) -> Option<HostInfo> {
        let r = self.residents.get(i)?;
        let others: Vec<&'static str> = self.residents.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, r)| r.name).collect();
        Some(HostInfo {
            name: r.name,
            kid: r.kid,
            temper: r.temper,
            theme: self.profile.theme,
            family: others,
            pet: self.pet.as_ref().map(|p| (p.kind, p.name.clone())),
            years: self.profile.years,
            church: self.church,
            has_piano: self.things.iter().any(|t| t.0 == Thing::Piano),
            has_oven: self.oven.is_some(),
        })
    }

}

/// Everything a conversation with someone at home can draw on.
pub struct HostInfo {
    pub name: &'static str,
    pub kid: bool,
    pub temper: Temper,
    pub theme: Theme,
    pub family: Vec<&'static str>,
    pub pet: Option<(PetKind, String)>,
    pub years: u32,
    pub church: bool,
    pub has_piano: bool,
    pub has_oven: bool,
}

/// A person drawn as pixel art at double the village scale: 8 px wide,
/// 16 tall (12 for a kid), feet on cell row `feet`.
pub(crate) struct Big {
    pub(crate) x: f32,
    pub(crate) feet: i32,
    pub(crate) dir: f32,
    pub(crate) step: bool,
    /// A bobble hat (the player) or hair.
    pub(crate) hat: Option<Rgb>,
    pub(crate) hair: Rgb,
    pub(crate) coat: Rgb,
    pub(crate) scarf: Option<Rgb>,
    /// Sitting, on a cushion at this pixel row.
    pub(crate) sit: Option<i32>,
    pub(crate) kid: bool,
    /// Hands up to the fire.
    pub(crate) arms_up: bool,
    /// Out in the village: daylight shades it, and lights touch it the way
    /// they touch glyphs.
    pub(crate) outdoor: bool,
}

impl Big {
    /// A person in pixel art, 16 pixels tall, feet on row `feet`.
    pub(crate) fn draw(&self, f: &mut Frame, t: f32) {
        let skin = Rgb::hex(0xe8c39e);
        let boots = Rgb::hex(0x2a2a34);
        let bottom = self.feet * 2 + 1;
        let cx = self.x.round() as i32;
        let rows: &[&str] = if self.hat.is_some() {
            &[
                "   w    ", "   hh   ", "  hhh   ", "  hhhh  ", "  ssss  ", "  sses  ", "  ssss  ", " rrrrr  ", "rr cccc ", "  cccc  ", "  ccccc ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ",
                " bb bb  ",
            ]
        } else if self.kid {
            &["  hhh   ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "  cccc  ", " cccccc ", "  cccc s", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  "]
        } else if self.arms_up {
            &[
                "        ", "  hhhh  ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "   ss  s", "  cccccs", " ccccc  ", " cccccc ", "  cccc  ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ",
                " bb bb  ",
            ]
        } else {
            &[
                "        ", "  hhhh  ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "   ss   ", "  cccc  ", " cccccc ", " cccccc ", "  cccc s", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ",
                " bb bb  ",
            ]
        };
        let n = rows.len() as i32;
        let legs = n - 3;
        // sitting: hips sunk into the cushion, the legs hanging down in front
        let top = match self.sit {
            Some(seat) => seat + 2 - (legs - 1),
            None => bottom - n + 1,
        };
        for (ry, row) in rows.iter().enumerate() {
            let ry = ry as i32;
            if self.sit.is_some() && ry >= legs {
                continue;
            }
            for (i, ch) in row.chars().enumerate() {
                let col = match ch {
                    'w' => Rgb::hex(0xf4f0ea),
                    'h' => self.hat.unwrap_or(self.hair),
                    's' => skin,
                    'e' => Rgb::hex(0x2a1a14),
                    'r' => self.scarf.unwrap_or(self.coat),
                    'c' => self.coat,
                    'b' => boots,
                    _ => continue,
                };
                let mut dx = i as i32 - 4;
                let mut y = top + ry;
                if self.step && ry >= legs {
                    dx += if ry % 2 == 0 { 1 } else { -1 };
                }
                if self.step && ry < legs {
                    y -= 1;
                }
                let x = if self.dir > 0.0 { cx + dx } else { cx - dx - 1 };
                let col = if ch == 'r' && i < 2 && (t * 6.0) as i32 % 2 == 0 { col * 0.8 } else { col };
                if self.outdoor {
                    f.px_sprite(x, y, col * (1.0 + 0.7 * f.day.max(0.0)));
                } else {
                    f.px_set(x, y, col);
                }
            }
        }
        if self.sit.is_some() {
            // two legs down from the seat's edge, boots on the floor
            for y in top + legs..=bottom {
                for dx in [-2, 1] {
                    let x = if self.dir > 0.0 { cx + dx } else { cx - dx - 1 };
                    f.px_set(x, y, if y >= bottom - 1 { boots } else { boots * 1.4 });
                }
            }
        }
    }
}

