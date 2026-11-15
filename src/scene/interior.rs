//! Inside a house (or the church). Walk up to a front door in the village
//! and press E: the view becomes a cozy room — fire crackling in the
//! hearth, a decorated tree, snow falling past the windows, someone home to
//! chat with, a cat asleep on the rug. E pokes at things in here too; walk
//! out the door (or press Esc) to go back outside.
//!
//! The room's static parts are drawn once into a layer; the fire, snow,
//! steam, candles and so on animate on top each frame. Speech bubbles and
//! sparkles use a small indoor `Director` of their own.

use super::events::{Ctx, Director};
use super::player::Player;
use super::tree::{LightStyle, Tree};
use crate::audio::Sfx;
use crate::color::Rgb;
use crate::frame::{Frame, Glyph, Layer};
use crate::rng::{h01, hash2, hash3, noise1, Rng};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Thing {
    Door,
    Fire,
    Tree,
    Host,
    Cat,
    Cocoa,
    Window,
    Piano,
    Clock,
    Shelf,
    Organ,
    Candles,
}

const HOST_HI: &[&str] = &[
    "come in, come in! warm yourself by the fire",
    "oh! a visitor! cocoa's on the table",
    "merry Christmas! mind the cat",
    "we just finished the tree — what do you think?",
    "did you knock? I couldn't hear over the carols",
    "shoes off, snow stays outside!",
];
const HOST_CHAT: &[&str] = &[
    "the cookies are for Santa. …mostly.",
    "the cat thinks the tree is hers",
    "my grandmother knitted every stocking",
    "we've had that ornament forty years",
    "have you seen the lights at the church?",
    "stay as long as you like",
    "the kettle's always on in winter",
    "Santa takes the chimney, you take the door",
];
const CHURCH_HI: &[&str] = &[
    "welcome! choir practice is at seven",
    "isn't the glass lovely by candlelight?",
    "come in out of the cold, friend",
    "peace be with you — and merry Christmas",
];
const CHURCH_CHAT: &[&str] = &[
    "the organ's older than the village",
    "midnight mass is my favourite night of the year",
    "the bells? they've rung every hour for a century",
    "shh… listen to the snow on the roof",
];
const FIRE: &[&str] = &["toasty!", "ahh, that's the stuff", "my toes are thawing", "*crackle* *pop*"];
const COCOA: &[&str] = &["mmm, cocoa!", "marshmallows, too!", "careful, hot!", "best cocoa in town"];
const WINDOW: &[&str] = &["the snow's coming down nicely", "look at it out there", "so cozy in here", "I can see the big tree from here"];
const SHELF: &[&str] = &["A Christmas Carol — well loved", "a cookbook: 101 kinds of fudge", "a photo album, very festive", "a book of carols, dog-eared"];
const CLOCK: &[&str] = &["tick… tock…", "almost time for cocoa", "it chimes on the hour"];
const CANDLES: &[&str] = &["a quiet moment", "the candles flicker warmly", "a little wish ✦"];

pub struct Interior {
    #[allow(dead_code)]
    pub house: usize,
    pub church: bool,
    w: i32,
    h: i32,
    /// First floor pixel row.
    floor: i32,
    seed: u32,
    room: Layer,
    /// (what, centre column, top row) — for the [E] prompt and interacting.
    things: Vec<(Thing, i32, i32)>,
    windows: Vec<(i32, i32, i32, i32)>,
    /// Fireplace opening (pixel rect).
    hearth: Option<(i32, i32, i32, i32)>,
    tree: Tree,
    host: (f32, Rgb, Rgb),
    cat: i32,
    mug: Option<(i32, i32)>,
    clock: Option<(i32, i32)>,
    candles: Vec<(i32, i32)>,
    pub director: Director,
    ctx: Ctx,
    rng: Rng,
    /// Where the player stood outside (world column, row), to step back out.
    pub outside: (f32, f32),
    purr_until: f32,
    music_until: f32,
    greeted: bool,
}

impl Interior {
    #[allow(clippy::too_many_arguments)]
    pub fn new(house: usize, church: bool, wall_tint: Rgb, w: usize, h: usize, seed: u32, outside: (f32, f32), t: f32) -> Self {
        let (w, h) = (w as i32, h as i32);
        let ph = h * 2;
        let floor = (ph as f32 * 0.70) as i32;
        let mut rng = Rng::new(seed as u64 ^ 0x1a51de);
        let fx = |f: f32| (w as f32 * f) as i32;
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
        let tree_x = fx(if church { 0.66 } else { 0.70 });
        let tree_h = ((h as f32) * if church { 0.5 } else { 0.4 }) as i32;
        let style = *rng.pick(&[LightStyle::Multi, LightStyle::Warm, LightStyle::Icy]);
        let tree = Tree::new(tree_x, base_row + 1, tree_h, style, seed ^ 0x77ee);
        let coats = [Rgb::hex(0xb3202a), Rgb::hex(0x1f6e3a), Rgb::hex(0x2a4c8a), Rgb::hex(0x6a3a8a), Rgb::hex(0xc8862a)];
        let hair = [Rgb::hex(0x3a2a1a), Rgb::hex(0x8a5a2a), Rgb::hex(0xd8d0c0), Rgb::hex(0x1a1a1a)];
        let mut room = Interior {
            house,
            church,
            w,
            h,
            floor,
            seed,
            room: Layer::new(w.max(1) as usize, ph.max(1) as usize),
            things: vec![],
            windows: vec![],
            hearth: None,
            tree,
            host: (fx(if church { 0.47 } else { 0.27 }) as f32, *rng.pick(&coats), *rng.pick(&hair)),
            cat: fx(0.42),
            mug: None,
            clock: None,
            candles: vec![],
            director,
            ctx,
            rng,
            outside,
            purr_until: 0.0,
            music_until: 0.0,
            greeted: false,
        };
        room.build(wall_tint);
        let _ = t;
        room
    }

    /// Where the player appears: just inside the door.
    pub fn entrance(&self) -> (f32, f32) {
        (17.0, (self.floor / 2 + 8) as f32)
    }

    /// The row the host stands on.
    fn host_feet(&self) -> i32 {
        self.floor / 2 + 6
    }

    pub fn rebase(&mut self, s: f32) {
        self.director.rebase(s);
        self.tree.rebase(s);
        self.purr_until -= s;
        self.music_until -= s;
    }

    // ---------------------------------------------------------------- build

    fn build(&mut self, tint: Rgb) {
        let (w, ph, floor) = (self.w, self.h * 2, self.floor);
        let seed = self.seed;
        let fx = |f: f32| (w as f32 * f) as i32;
        let wall = if self.church { Rgb::hex(0x6a6478) } else { Rgb::hex(0x7a4a3a).lerp(tint, 0.35) };
        let stripe = wall * 0.9;
        let wood = Rgb::hex(0x5a3a24);
        let l = &mut self.room;
        // wallpaper with soft stripes and a little pattern
        for y in 0..floor {
            for x in 0..w {
                let mut c = if (x / 6) % 2 == 0 { wall } else { stripe };
                if !self.church && (x % 12 == 3) && (y % 8 == 4) {
                    c = c * 1.12;
                }
                if self.church {
                    // stone blocks
                    let row = y / 5;
                    let off = if row % 2 == 0 { 0 } else { 6 };
                    if y % 5 == 0 || (x + off) % 12 == 0 {
                        c = c * 0.82;
                    }
                    c = c * (0.94 + 0.08 * h01(hash2(x / 12 + row * 31, seed as i32)));
                }
                l.set(x, y, c * (0.55 + 0.45 * (y as f32 / floor as f32)));
            }
        }
        // crown molding and wainscoting
        for x in 0..w {
            l.set(x, 2, wood * 0.9);
            l.set(x, 3, wood * 0.7);
        }
        let wains = floor - (ph as f32 * 0.13) as i32;
        if !self.church {
            for y in wains..floor {
                for x in 0..w {
                    let panel = (x % 20 == 0) || y == wains || y == wains + 1;
                    l.set(x, y, if panel { wood * 0.75 } else { wood * (0.95 + 0.05 * h01(hash2(x / 20, y))) });
                }
            }
        }
        // the floor: wooden planks (stone flags in the church)
        for y in floor..ph {
            for x in 0..w {
                let row = (y - floor) / 3;
                let off = (h01(hash2(row, seed as i32)) * 18.0) as i32;
                let seam = (x + off) % 18 == 0 || (y - floor) % 3 == 0;
                let c = if self.church {
                    Rgb::hex(0x4a4652) * (0.9 + 0.12 * h01(hash2((x + off) / 9, row)))
                } else {
                    let k = h01(hash2((x + off) / 18, row * 7 + seed as i32));
                    Rgb::hex(0x6a4428).lerp(Rgb::hex(0x7a5030), k)
                };
                let depth = (y - floor) as f32 / (ph - floor).max(1) as f32;
                l.set(x, y, if seam { c * 0.7 } else { c } * (0.75 + 0.3 * depth));
            }
        }
        // skirting board
        for x in 0..w {
            l.set(x, floor - 1, wood * 0.5);
        }
        // the door (left)
        // a proper front door: trimmed frame, two raised panels, brass knob
        // and a wreath
        let (dx0, dx1, dtop) = (4, 22, floor - 32);
        let trim = Rgb::hex(0xe8e0d0) * 0.85;
        for y in dtop - 2..floor {
            for x in dx0 - 2..dx1 + 2 {
                l.set(x, y, trim);
            }
        }
        let door = if self.church { Rgb::hex(0x4a2a16) } else { Rgb::hex(0x7a2a22) };
        for y in dtop..floor {
            for x in dx0..dx1 {
                let (u, v) = (x - dx0, y - dtop);
                let panel = (u >= 3 && u <= 7 || u >= 10 && u <= 14) && (v >= 3 && v <= 13 || v >= 17 && v <= 28);
                let edge_hi = panel && (u == 3 || u == 10 || v == 3 || v == 17);
                let c = if edge_hi { door * 1.25 } else if panel { door * 1.08 } else { door };
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

        if self.church {
            self.build_church(wains);
        } else {
            self.build_home(wains);
        }
        let _ = fx;
    }

    fn window_at(&mut self, cx: i32, top: i32, ww: i32, wh: i32, arch: bool) {
        let frame = if self.church { Rgb::hex(0x3a3440) } else { Rgb::hex(0xe8e0d0) };
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
        }
        self.windows.push((x0, top, ww, wh));
    }

    fn build_home(&mut self, wains: i32) {
        let (w, floor, ph) = (self.w, self.floor, self.h * 2);
        let fx = |f: f32| (w as f32 * f) as i32;
        // windows
        let wtop = (floor as f32 * 0.22) as i32;
        let wh = (floor as f32 * 0.40) as i32;
        for f in [0.17, 0.85] {
            let cx = fx(f);
            self.window_at(cx, wtop, 18, wh, false);
            self.things.push((Thing::Window, cx, wtop / 2 - 1));
            // a candle on the sill
            self.candles.push((cx + 6, wtop + wh));
        }
        if w > 330 {
            let cx = fx(0.97).min(w - 12);
            self.window_at(cx, wtop, 14, wh, false);
        }
        // the fireplace, stockings on the mantel
        let cx = fx(0.42);
        let (fw, fh) = (30, 30);
        let (x0, y0) = (cx - fw / 2, floor - fh);
        let l = &mut self.room;
        for y in y0..floor {
            for x in x0..x0 + fw {
                let row = (y - y0) / 3;
                let off = if row % 2 == 0 { 0 } else { 3 };
                let mortar = (y - y0) % 3 == 0 || (x - x0 + off) % 6 == 0;
                let c = Rgb::hex(0x7a6a68) * (0.85 + 0.2 * h01(hash2((x + off) / 6, row)));
                l.set(x, y, if mortar { c * 0.7 } else { c });
            }
        }
        // mantel
        for x in x0 - 3..x0 + fw + 3 {
            l.set(x, y0 - 1, Rgb::hex(0x4a2a1a));
            l.set(x, y0 - 2, Rgb::hex(0x5a3420));
        }
        // garland along the mantel with little bulbs
        for x in x0 - 3..x0 + fw + 3 {
            l.set(x, y0 - 3, Rgb::hex(0x1f5a32) * (0.8 + 0.3 * h01(hash2(x, 9))));
        }
        // opening
        let (ow, oh) = (16, 16);
        let (ox, oy) = (cx - ow / 2, floor - oh);
        for y in oy..floor {
            for x in ox..ox + ow {
                l.set(x, y, Rgb::hex(0x0e0808));
            }
        }
        self.hearth = Some((ox, oy, ow, oh));
        // stockings
        let socks = [Rgb::hex(0xc8202a), Rgb::hex(0x2a8a4a), Rgb::hex(0xc8202a), Rgb::hex(0x2a5aa8)];
        for (i, c) in socks.iter().enumerate() {
            let sx = x0 + 2 + i as i32 * 7;
            for y in y0..y0 + 5 {
                for x in sx..sx + 3 {
                    l.set(x, y, if y == y0 { Rgb::hex(0xf4efe6) } else { *c });
                }
            }
            l.set(sx + 3, y0 + 4, *c);
            l.set(sx + 3, y0 + 3, *c);
        }
        self.things.push((Thing::Fire, cx, y0 / 2 - 2));
        // the rug in front of it
        let (rcx, rcy, rx, ry) = (cx as f32, (floor as f32 + (ph - floor) as f32 * 0.42), 34.0, 6.0);
        for y in floor..ph {
            for x in (cx - 36)..(cx + 36) {
                let (dx, dy) = ((x as f32 - rcx) / rx, (y as f32 - rcy) / ry);
                let d = dx * dx + dy * dy;
                if d <= 1.0 {
                    let border = d > 0.72;
                    let c = if border { Rgb::hex(0xc8a040) } else if (x + y) % 4 == 0 { Rgb::hex(0x6a1a22) } else { Rgb::hex(0x8a2030) };
                    l.set(x, y, c);
                }
            }
        }
        self.cat = cx - 8;
        self.things.push((Thing::Cat, cx - 7, (rcy as i32) / 2 - 2));
        // armchair and the side table with a mug of cocoa
        let ax = fx(0.53);
        let chair = Rgb::hex(0x7a2a2a);
        for y in floor - 16..floor + 2 {
            for x in ax..ax + 14 {
                let back = y < floor - 6 && x >= ax + 2 && x < ax + 12;
                let seat = y >= floor - 6 && y < floor;
                let arm = (x < ax + 3 || x >= ax + 11) && y >= floor - 9 && y < floor;
                let leg = y >= floor && (x == ax + 1 || x == ax + 12);
                if back || seat || arm {
                    l.set(x, y, chair * if arm { 0.8 } else { 1.0 } * (0.9 + 0.1 * h01(hash2(x, y))));
                } else if leg {
                    l.set(x, y, Rgb::hex(0x2a1a10));
                }
            }
        }
        let tx = ax + 17;
        for x in tx..tx + 8 {
            l.set(x, floor - 7, Rgb::hex(0x4a2a1a));
        }
        for y in floor - 6..floor + 1 {
            l.set(tx + 1, y, Rgb::hex(0x3a2214));
            l.set(tx + 6, y, Rgb::hex(0x3a2214));
        }
        for y in floor - 10..floor - 7 {
            for x in tx + 3..tx + 6 {
                l.set(x, y, Rgb::hex(0xe8e0d0));
            }
        }
        l.set(tx + 6, floor - 9, Rgb::hex(0xe8e0d0));
        self.mug = Some((tx + 4, floor - 10));
        self.things.push((Thing::Cocoa, tx + 4, (floor - 10) / 2 - 2));
        // picture frames
        for (f, c) in [(0.30, 0x3a5a7a), (0.58, 0x6a4a2a)] {
            let px = fx(f);
            for y in wtop..wtop + 9 {
                for x in px..px + 12 {
                    let edge = y == wtop || y == wtop + 8 || x == px || x == px + 11;
                    l.set(x, y, if edge { Rgb::hex(0xb8902a) } else { Rgb::hex(c) * (0.7 + 0.4 * h01(hash2(x / 3, y / 3))) });
                }
            }
        }
        // a bookshelf, a grandfather clock, and (in bigger houses) a piano
        let bx = fx(0.08).max(16);
        for y in floor - 30..floor {
            for x in bx..bx + 12 {
                let shelf = (floor - y) % 8 == 0 || x == bx || x == bx + 11;
                let c = if shelf {
                    Rgb::hex(0x3a2214)
                } else {
                    let books = [0x8a2a2a, 0x2a5a3a, 0x2a3a6a, 0x8a6a2a, 0x5a2a5a, 0x6a6a6a];
                    Rgb::hex(books[(hash2(x, (floor - y) / 8) % 6) as usize]) * (0.75 + 0.3 * h01(hash2(x, 3)))
                };
                l.set(x, y, c);
            }
        }
        self.things.push((Thing::Shelf, bx + 6, (floor - 30) / 2 - 1));
        let ck = fx(0.62);
        for y in floor - 34..floor {
            for x in ck..ck + 7 {
                let edge = x == ck || x == ck + 6;
                l.set(x, y, if edge { Rgb::hex(0x2a1810) } else { Rgb::hex(0x4a2a18) });
            }
        }
        self.clock = Some((ck + 3, floor - 30));
        self.things.push((Thing::Clock, ck + 3, (floor - 34) / 2 - 1));
        if w > 260 {
            let pxp = fx(0.92).min(w - 30);
            for y in floor - 14..floor + 1 {
                for x in pxp..pxp + 22 {
                    let keys = y == floor - 8 || y == floor - 7;
                    let leg = y > floor - 7 && !(x == pxp + 1 || x == pxp + 20);
                    if leg {
                        continue;
                    }
                    let c = if keys { if (x - pxp) % 2 == 0 { Rgb::hex(0xf0ece0) } else { Rgb::hex(0x1a1a1a) } } else { Rgb::hex(0x16141a) };
                    l.set(x, y, c);
                }
            }
            self.things.push((Thing::Piano, pxp + 11, (floor - 14) / 2 - 1));
        }
        let _ = wains;
        // the host and the tree
        self.things.push((Thing::Host, self.host.0 as i32 + 1, self.host_feet() - 10));
        self.things.push((Thing::Tree, self.tree.cx, self.tree.top - 2));
    }

    fn build_church(&mut self, _wains: i32) {
        let (w, floor, ph) = (self.w, self.floor, self.h * 2);
        let fx = |f: f32| (w as f32 * f) as i32;
        // tall arched stained-glass windows
        let wtop = (floor as f32 * 0.25) as i32;
        let wh = (floor as f32 * 0.55) as i32;
        let mut f = 0.16;
        while f < 0.95 {
            let cx = fx(f);
            self.window_at(cx, wtop, 12, wh, true);
            self.things.push((Thing::Window, cx, wtop / 2 - 4));
            f += 0.17;
        }
        // pews, in rows
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
        // altar with a white cloth and candles
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
        // the organ, pipes reaching up the wall
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
        self.things.push((Thing::Host, self.host.0 as i32 + 1, self.host_feet() - 10));
        self.things.push((Thing::Tree, self.tree.cx, self.tree.top - 2));
    }

    // ---------------------------------------------------------------- live

    /// Walk around inside. Returns true when the player walks out the door.
    pub fn update(&mut self, dt: f32, t: f32, mv: (f32, f32), p: &mut Player) -> bool {
        if !self.greeted {
            self.greeted = true;
            let line = *self.rng.pick(if self.church { CHURCH_HI } else { HOST_HI });
            let (hx, hy) = (self.host.0.round() as i32 + 1, self.host_feet() - 9);
            self.director.say(hx, hy, line, t + 0.6, 3.4, 40);
            self.director.sound(Sfx::Greet, self.host.0, 0.8);
        }
        let (mx, my) = if p.free(t) { mv } else { (0.0, 0.0) };
        p.moving = mx != 0.0 || my != 0.0;
        if p.moving {
            p.active_at = t;
            if mx != 0.0 {
                p.dir = mx.signum();
            }
            let k = p.speed.clamp(0.25, 6.0);
            p.x += mx * 11.0 * k * dt;
            p.y = (p.y + my * 5.0 * k.sqrt() * dt).clamp((self.floor / 2 + 5) as f32, (self.h - 1) as f32);
        }
        p.x = p.x.clamp(1.0, (self.w - 4) as f32);
        self.director.update(dt, t, &self.ctx);
        // walking back out through the door
        mx < 0.0 && p.x <= 1.5
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

    /// E on something in the room. Returns true for the door (go outside).
    pub fn poke(&mut self, what: Thing, p: &Player, t: f32) -> bool {
        let (px, py) = (p.centre().round() as i32, p.y as i32 - 9);
        let host_head = self.host_feet() - 9;
        let d = &mut self.director;
        let say = |d: &mut Director, rng: &mut Rng, pool: &[&str]| {
            let line = *rng.pick(pool);
            d.say(px, py, line, t, 2.6, 40);
        };
        match what {
            Thing::Door => return true,
            Thing::Fire => {
                if let Some((ox, oy, ow, _)) = self.hearth {
                    let sparks = [Rgb::hex(0xffb040), Rgb::hex(0xffe080), Rgb::hex(0xff6020)];
                    d.burst((ox + ow / 2) as f32, (oy / 2) as f32, 18, &sparks, 4.0, -2.0);
                }
                d.sound(Sfx::Sparkle, px as f32, 0.5);
                say(d, &mut self.rng, FIRE);
            }
            Thing::Tree => {
                self.tree.boost_until = t + 3.0;
                d.sound(Sfx::Starburst, self.tree.cx as f32, 0.8);
                let (sx, sy) = self.tree.star();
                d.burst(sx as f32, sy as f32, 30, &[Rgb::hex(0xffd65a), Rgb::WHITE], 8.0, 3.0);
            }
            Thing::Host => {
                let line = *self.rng.pick(if self.church { CHURCH_CHAT } else { HOST_CHAT });
                let (hx, hy) = (self.host.0.round() as i32 + 1, host_head);
                d.say(px, py, *self.rng.pick(&["hello!", "lovely home!", "merry Christmas!", "thanks for having me"]), t, 2.0, 40);
                d.say(hx, hy, line, t + 2.2, 3.4, 40);
                d.sound(Sfx::Greet, hx as f32, 0.8);
            }
            Thing::Cat => {
                self.purr_until = t + 4.0;
                d.emote(self.cat as f32 + 3.0, ((self.floor + (self.h * 2 - self.floor) * 42 / 100) / 2 - 2) as f32, '♥', Rgb::hex(0xff6b8a), 3);
                d.sound(Sfx::Pet, self.cat as f32, 0.8);
                d.say(px, py, "prrrrr…", t + 0.2, 2.0, 40);
            }
            Thing::Cocoa => {
                d.sound(Sfx::Munch, px as f32, 0.6);
                say(d, &mut self.rng, COCOA);
            }
            Thing::Window => say(d, &mut self.rng, WINDOW),
            Thing::Shelf => say(d, &mut self.rng, SHELF),
            Thing::Clock => {
                d.sound(Sfx::Click, px as f32, 0.6);
                say(d, &mut self.rng, CLOCK);
            }
            Thing::Piano | Thing::Organ => {
                self.music_until = t + 6.0;
                let x = self.things.iter().find(|k| k.0 == what).map(|k| k.1).unwrap_or(px);
                d.notes(x as f32, (self.floor / 2 - 8) as f32, 8);
                d.sound(if what == Thing::Organ { Sfx::ChurchPeal } else { Sfx::Chime }, x as f32, 0.9);
            }
            Thing::Candles => {
                d.sound(Sfx::Chime, px as f32, 0.5);
                say(d, &mut self.rng, CANDLES);
            }
        }
        false
    }

    // ---------------------------------------------------------------- draw

    pub fn draw(&self, f: &mut Frame, t: f32, glow: f32, p: &Player, prompt: Option<(i32, i32)>) {
        f.ox = 0;
        self.room.composite_from(f, 0);
        let ph = self.h * 2;
        // snow falling past the windows (and a deep night sky)
        for (i, &(x0, y0, ww, wh)) in self.windows.iter().enumerate() {
            for y in y0..y0 + wh {
                for x in x0..x0 + ww {
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
                // leading between the panes, and the light through the glass
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
            // window cross
            for y in y0..y0 + wh {
                f.px_set(x0 + ww / 2, y, Rgb::hex(0xe8e0d0));
            }
            for x in x0..x0 + ww {
                f.px_set(x, y0 + wh / 2, Rgb::hex(0xe8e0d0));
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
                    let up = (oy + oh - y) as f32 / oh as f32; // 0 at the bottom
                    let n = noise1(t * 5.0 + x as f32 * 0.9 + y as f32 * 0.35, self.seed) * 0.6 + noise1(t * 9.0 - y as f32 * 0.5 + x as f32 * 0.3, self.seed ^ 3) * 0.4;
                    let mid = 1.0 - ((x - ox) as f32 / ow as f32 - 0.5).abs() * 2.0;
                    let heat = (n * 1.25 + mid * 0.35 - up * 1.1).clamp(0.0, 1.0);
                    if heat > 0.12 {
                        let c = if heat > 0.75 { Rgb::hex(0xffe8a0) } else if heat > 0.45 { Rgb::hex(0xffa030) } else { Rgb::hex(0xd8401a) };
                        f.px_set(x, y, c * (0.7 + heat * 0.5));
                    }
                }
            }
            // logs
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
        // steam curling off the cocoa
        if let Some((mx, my)) = self.mug {
            for k in 0..3 {
                let age = (t * 0.7 + k as f32 / 3.0).fract();
                let x = mx as f32 + (age * 6.0 + k as f32).sin() * 1.2;
                f.px_disc(x, my as f32 - 1.0 - age * 8.0, 0.6 + age, Rgb::hex(0xe8eaf0), (1.0 - age) * 0.4);
            }
        }
        // candles
        for (i, &(x, y)) in self.candles.iter().enumerate() {
            let fl = 0.75 + 0.25 * noise1(t * 9.0 + i as f32 * 3.0, 13);
            f.px_set(x, y - 1, Rgb::hex(0xffd070) * (1.0 + 0.3 * fl));
            f.px_set(x, y - 2, Rgb::hex(0xfff0b0) * fl);
            f.light_px(x as f32 + 0.5, y as f32 - 1.5, Rgb::hex(0xffb050) * (2.2 * fl * glow));
        }
        // the clock's pendulum
        if let Some((cx, cy)) = self.clock {
            let sw = (t * std::f32::consts::PI).sin() * 2.0;
            for k in 0..14 {
                f.px_set(cx + (sw * k as f32 / 14.0).round() as i32, cy + 6 + k, Rgb::hex(0xb8902a));
            }
            f.put_g(cx, cy / 2, Glyph::new('◷', Rgb::hex(0xf0e8d0)).lit(0.6));
        }
        // the cat, curled up on the rug (purring when petted)
        if !self.church {
            let rug_y = self.floor + (ph - self.floor) * 42 / 100;
            let breathe = ((t * if t < self.purr_until { 5.0 } else { 1.6 }).sin() * 0.5 + 0.5) * 0.6;
            let cat = Rgb::hex(0xd8862a);
            let (cx, cy) = (self.cat, rug_y - 1);
            f.px_disc(cx as f32 + 3.0, cy as f32 + 0.5, 2.6 + breathe * 0.3, cat, 1.0);
            f.px_set(cx + 6, cy - 1, cat);
            f.px_set(cx + 6, cy - 2, cat * 0.9); // ear
            f.px_set(cx + 7, cy - 1, cat);
            for k in 0..4 {
                f.px_set(cx - k, cy + 1, cat * 0.85); // tail
            }
            if t < self.purr_until && (t * 2.0) as i32 % 2 == 0 {
                f.put_g(cx + 7, cy / 2 - 2, Glyph::new('z', Rgb::hex(0xc8d0e0)).lit(0.0));
            }
        }
        // the tree
        self.tree.draw(f, t, glow, 0.0);
        // people, drawn at double size in here (nearer to the eye than the
        // village outside); back to front
        let (hx, coat, hair) = self.host;
        let host_dir = if p.centre() > hx + 1.0 { 1.0 } else { -1.0 };
        let host = Big { x: hx + 1.0, feet: self.host_feet(), dir: host_dir, step: false, hat: None, hair, coat, scarf: None };
        let walking = p.moving && (p.phase() as i32) % 2 == 0;
        let you = Big { x: p.centre(), feet: p.y.round() as i32, dir: p.dir, step: walking, hat: Some(Rgb::hex(0x2fae5c)), hair, coat: Rgb::hex(0xe0a030), scarf: Some(Rgb::hex(0xd8283a)) };
        if you.feet < host.feet {
            you.draw(f, t);
            host.draw(f, t);
        } else {
            host.draw(f, t);
            you.draw(f, t);
        }
        f.light_px(p.centre(), p.y * 2.0 - 8.0, Rgb::hex(0xffc878) * (1.2 * glow));
        if let Some((x, y)) = prompt {
            Player::draw_prompt(f, x, y, 'E', t);
        }
        self.director.draw_px(f);
        self.director.draw_front(f, t, glow);
        let _ = self.music_until;
    }
}

/// A person drawn as pixel art at double the village scale: 8 px wide,
/// 16 tall, feet on cell row `feet`.
struct Big {
    x: f32,
    feet: i32,
    dir: f32,
    step: bool,
    /// A bobble hat (the player) or hair.
    hat: Option<Rgb>,
    hair: Rgb,
    coat: Rgb,
    scarf: Option<Rgb>,
}

impl Big {
    fn draw(&self, f: &mut Frame, t: f32) {
        let skin = Rgb::hex(0xe8c39e);
        let boots = Rgb::hex(0x2a2a34);
        let bottom = self.feet * 2 + 1;
        let top = bottom - 15;
        let cx = self.x.round() as i32;
        // rows of the right-facing figure: one char per pixel, centred on cx
        let rows: [&str; 16] = if self.hat.is_some() {
            [
                "   w    ", "   hh   ", "  hhh   ", "  hhhh  ", "  ssss  ", "  sses  ", "  ssss  ", " rrrrr  ",
                "rr cccc ", "  cccc  ", "  ccccc ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
            ]
        } else {
            [
                "        ", "  hhhh  ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "   ss   ", "  cccc  ",
                " cccccc ", " cccccc ", "  cccc s", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
            ]
        };
        for (ry, row) in rows.iter().enumerate() {
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
                let mut y = top + ry as i32;
                // a little walk cycle: legs swap, the body bobs
                if self.step && ry >= 13 {
                    dx += if ry % 2 == 0 { 1 } else { -1 };
                }
                if self.step && ry < 13 {
                    y -= 1;
                }
                let x = if self.dir > 0.0 { cx + dx } else { cx - dx - 1 };
                // the scarf's loose end flutters
                let col = if ch == 'r' && i < 2 && (t * 6.0) as i32 % 2 == 0 { col * 0.8 } else { col };
                f.px_set(x, y, col);
            }
        }
    }
}
