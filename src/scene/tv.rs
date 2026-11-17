//! The television in the parlour: a few channels to watch from the couch.
//! Each channel is drawn afresh as a little picture (160×90), with a line
//! or two about what's on under it; E (or a tap on the set) goes to the
//! next channel, and after the last it's off again.

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph, Picture};
use crate::rng::noise1;

fn hash(a: u32, b: u32) -> u32 {
    crate::rng::hash(a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B))
}
use chrono::{Datelike, Timelike};
use std::sync::Arc;

pub const CHANNELS: [&str; 6] = ["North Pole Cam", "Santa Tracker", "Train Cam", "Board Game Championship", "Ski Championship", "Yule Log"];

/// The set's on, on this channel since then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tv {
    pub channel: u8,
    pub since: f32,
}

impl Tv {
    /// The next channel; None after the last (the set goes off).
    pub fn next(self, t: f32) -> Option<Tv> {
        let c = self.channel as usize + 1;
        (c < CHANNELS.len()).then_some(Tv { channel: c as u8, since: t })
    }
}

const W: usize = 160;
const H: usize = 90;

/// A little picture to paint.
struct Px(Vec<u8>);

impl Px {
    fn new() -> Px {
        Px(vec![0; W * H * 4])
    }

    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
            return;
        }
        let i = (y as usize * W + x as usize) * 4;
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (r, g, b) = (q(c.r), q(c.g), q(c.b));
        self.0[i..i + 4].copy_from_slice(&[r, g, b, 255]);
    }

    fn get(&self, x: i32, y: i32) -> Rgb {
        if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
            return Rgb::BLACK;
        }
        let i = (y as usize * W + x as usize) * 4;
        Rgb { r: self.0[i] as f32 / 255.0, g: self.0[i + 1] as f32 / 255.0, b: self.0[i + 2] as f32 / 255.0 }
    }

    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        let o = self.get(x, y);
        self.set(x, y, o.lerp(c, a.clamp(0.0, 1.0)));
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d <= r {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn sky(&mut self, top: Rgb, bottom: Rgb, to: i32) {
        for y in 0..to {
            let c = top.lerp(bottom, y as f32 / to.max(1) as f32);
            for x in 0..W as i32 {
                self.set(x, y, c);
            }
        }
    }

    fn snowfall(&mut self, t: f32, n: u32, speed: f32) {
        for k in 0..n {
            let x = (hash(k, 11) % W as u32) as f32 + (t * 0.7 + k as f32).sin() * 3.0;
            let y = ((hash(k, 23) % H as u32) as f32 + t * speed * (0.6 + (hash(k, 5) % 10) as f32 / 20.0)) % H as f32;
            self.set(x as i32, y as i32, Rgb::hex(0xeef4ff));
        }
    }

    fn picture(self) -> Arc<Picture> {
        Arc::new(Picture::new(W, H, self.0))
    }
}

/// What's on channel `c` at `t` (the scene's clock; `now`: the wall clock).
pub fn picture(c: u8, t: f32, now: chrono::DateTime<chrono::Local>) -> Arc<Picture> {
    let mut p = Px::new();
    match c {
        0 => north_pole(&mut p, t),
        1 => santa_tracker(&mut p, t, now),
        2 => train(&mut p, t),
        3 => board_game(&mut p, t),
        4 => ski(&mut p, t),
        _ => yule_log(&mut p, t),
    }
    // a faint scanline, and the tube's glow at the edges
    for y in (0..H as i32).step_by(2) {
        for x in 0..W as i32 {
            p.blend(x, y, Rgb::BLACK, 0.08);
        }
    }
    p.picture()
}

/// North Pole Cam: Santa's workshop at night, the aurora overhead, elves
/// bustling about with presents.
fn north_pole(p: &mut Px, t: f32) {
    p.sky(Rgb::hex(0x060a22), Rgb::hex(0x24345e), 66);
    for k in 0..40 {
        let (x, y) = ((hash(k, 3) % W as u32) as i32, (hash(k, 7) % 50) as i32);
        let tw = 0.6 + 0.4 * (t * 2.0 + k as f32).sin();
        p.set(x, y, Rgb::hex(0xfff6d8) * tw);
    }
    // the aurora: two ribbons swaying
    for x in 0..W as i32 {
        for (k, (base, col)) in [(18.0, Rgb::hex(0x3cff9a)), (26.0, Rgb::hex(0x7a8cff))].iter().enumerate() {
            let yc = base + 6.0 * (x as f32 * 0.045 + t * 0.5 + k as f32).sin() + 3.0 * noise1(x as f32 * 0.03 + t * 0.2, k as u32);
            for dy in 0..14 {
                let a = (1.0 - dy as f32 / 14.0) * 0.35 * (0.6 + 0.4 * noise1(x as f32 * 0.1 + t, 9 + k as u32));
                p.blend(x, (yc + dy as f32) as i32, *col, a);
            }
        }
    }
    // snow on the ground
    for y in 66..H as i32 {
        for x in 0..W as i32 {
            p.set(x, y, Rgb::hex(0xdfe6f6) * (0.86 + 0.08 * ((x + y) % 3) as f32 / 2.0));
        }
    }
    // the workshop: red walls, a snowy roof, lit windows, smoke
    p.rect(48, 42, 64, 25, Rgb::hex(0x9a2a24));
    for k in 0..14 {
        p.rect(44 + k * 2, 42 - k, 72 - k * 4, 1, Rgb::hex(0xf2f4fa));
    }
    for wx in [54, 70, 92, 102] {
        let flick = 0.85 + 0.15 * noise1(t * 3.0 + wx as f32, 4);
        p.rect(wx, 48, 7, 7, Rgb::hex(0xffcf6a) * flick);
        p.rect(wx + 3, 48, 1, 7, Rgb::hex(0x7a1a14));
    }
    p.rect(80, 54, 9, 13, Rgb::hex(0x4a2a1a));
    p.rect(98, 26, 6, 10, Rgb::hex(0x6a2a20));
    for k in 0..6 {
        let a = (t * 0.6 + k as f32 / 6.0) % 1.0;
        p.disc(101.0 + a * 10.0 + (a * 9.0).sin() * 2.0, 24.0 - a * 22.0, 1.5 + a * 3.0, Rgb::hex(0x9aa0b4) * (1.0 - a * 0.5));
    }
    // the pole, striped
    for y in 46..67 {
        p.rect(128, y, 3, 1, if (y / 2) % 2 == 0 { pal::CANDY_RED } else { pal::CANDY_WHITE });
    }
    p.disc(129.5, 45.0, 2.5, Rgb::hex(0xffd36b));
    // elves, to and fro with presents
    for k in 0..4 {
        let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
        let span = 150.0;
        let x = ((t * (9.0 + k as f32 * 2.0) * dir + k as f32 * 37.0).rem_euclid(span)) as i32 + 5;
        let y = 64 + (k % 2) as i32 * 2;
        let step = ((t * 6.0) as i32 + k) % 2;
        p.rect(x, y - 4, 3, 4, Rgb::hex(0x2fae5c));
        p.set(x + 1, y - 5, Rgb::hex(0xf0c8a8));
        p.rect(x, y - 7, 3, 2, Rgb::hex(0xc8202a));
        p.set(x + step * 2, y, Rgb::hex(0x2a1a12));
        p.rect(x + if dir > 0.0 { 3 } else { -3 }, y - 4, 3, 3, [Rgb::hex(0x4a8dff), Rgb::hex(0xffc23a), Rgb::hex(0xff7ae0), Rgb::hex(0xffffff)][k as usize]);
    }
    p.snowfall(t, 50, 8.0);
}

/// Where Santa is on Christmas Eve, by UTC: (fraction of the trip, or
/// None: not out yet / home again).
fn santa_trip(now: chrono::DateTime<chrono::Local>) -> Option<f32> {
    let u = now.with_timezone(&chrono::Utc);
    if u.month() != 12 || !(24..=25).contains(&u.day()) {
        return None;
    }
    // out from 10:00 UTC on the 24th, home by 12:00 UTC on the 25th
    let h = (u.day() as f32 - 24.0) * 24.0 + u.hour() as f32 + u.minute() as f32 / 60.0;
    let f = (h - 10.0) / 26.0;
    (0.0..1.0).contains(&f).then_some(f)
}

const ROUTE: [(f32, f32, &str); 9] = [
    (80.0, 6.0, "the North Pole"),
    (148.0, 60.0, "the Pacific islands"),
    (132.0, 64.0, "Australia"),
    (114.0, 34.0, "Asia"),
    (86.0, 50.0, "Africa"),
    (84.0, 26.0, "Europe"),
    (52.0, 64.0, "South America"),
    (34.0, 32.0, "North America"),
    (80.0, 6.0, "the North Pole"),
];

/// Santa Tracker: the world, his route, the sleigh where it's got to (or
/// at the North Pole, getting ready, until Christmas Eve).
fn santa_tracker(p: &mut Px, t: f32, now: chrono::DateTime<chrono::Local>) {
    p.rect(0, 0, W as i32, H as i32, Rgb::hex(0x0a1834));
    for x in (0..W as i32).step_by(16) {
        for y in 0..H as i32 {
            p.blend(x, y, Rgb::hex(0x2a4a7a), 0.4);
        }
    }
    for y in (0..H as i32).step_by(15) {
        for x in 0..W as i32 {
            p.blend(x, y, Rgb::hex(0x2a4a7a), 0.4);
        }
    }
    let land = Rgb::hex(0x2f7a4a);
    for (cx, cy, rx, ry) in [(35.0, 30.0, 18.0, 12.0), (52.0, 62.0, 8.0, 14.0), (84.0, 26.0, 8.0, 6.0), (86.0, 50.0, 10.0, 14.0), (114.0, 30.0, 22.0, 12.0), (132.0, 64.0, 9.0, 6.0), (60.0, 12.0, 7.0, 4.0)] {
        for y in (cy - ry) as i32..=(cy + ry) as i32 {
            for x in (cx - rx) as i32..=(cx + rx) as i32 {
                let d = ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2);
                if d <= 1.0 + 0.15 * noise1(x as f32 * 0.3 + y as f32 * 0.2, 3) {
                    p.set(x, y, land * (0.85 + 0.15 * noise1(x as f32 * 0.2, y as u32)));
                }
            }
        }
    }
    // the North Pole's workshop, at the top
    p.rect(77, 2, 7, 5, Rgb::hex(0xc8202a));
    p.rect(76, 1, 9, 1, Rgb::WHITE);
    let trip = santa_trip(now);
    let (sx, sy) = match trip {
        Some(f) => {
            let seg = f * (ROUTE.len() - 1) as f32;
            let i = (seg as usize).min(ROUTE.len() - 2);
            let u = seg - i as f32;
            // the route behind him, solid; ahead, dotted
            for (k, w) in ROUTE.windows(2).enumerate() {
                let n = 40;
                for s in 0..n {
                    let v = s as f32 / n as f32;
                    let done = k < i || (k == i && v < u);
                    if done || s % 4 == 0 {
                        let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * v, w[0].1 + (w[1].1 - w[0].1) * v);
                        p.set(x as i32, y as i32, if done { Rgb::hex(0xffd36b) } else { Rgb::hex(0x8a90a8) });
                    }
                }
            }
            (ROUTE[i].0 + (ROUTE[i + 1].0 - ROUTE[i].0) * u, ROUTE[i].1 + (ROUTE[i + 1].1 - ROUTE[i].1) * u)
        }
        // getting ready: bobbing over the workshop
        None => (80.0 + 8.0 * (t * 0.8).sin(), 14.0 + 2.0 * (t * 2.0).sin()),
    };
    // the sleigh, the reindeer ahead, Rudolph's nose
    p.rect(sx as i32 - 3, sy as i32, 6, 3, Rgb::hex(0xc8202a));
    p.rect(sx as i32 - 4, sy as i32 + 3, 8, 1, Rgb::hex(0xffd36b));
    for k in 0..3 {
        p.rect(sx as i32 + 4 + k * 3, sy as i32 - 1 + (k + (t * 6.0) as i32) % 2, 2, 2, Rgb::hex(0x8a5a32));
    }
    let glow = 0.6 + 0.4 * (t * 5.0).sin();
    p.disc(sx + 14.0, sy - 0.5, 1.6, Rgb::hex(0xff2a2a) * glow);
}

/// Santa Tracker's caption: where he is (or how long till he sets out).
fn santa_caption(now: chrono::DateTime<chrono::Local>, t: f32) -> (String, String) {
    match santa_trip(now) {
        Some(f) => {
            let seg = f * (ROUTE.len() - 1) as f32;
            let (_, _, place) = ROUTE[((seg + 0.5) as usize).min(ROUTE.len() - 1)];
            let gifts = (f as f64 * 7_800_000_000.0) as u64;
            (format!("Santa's over {place}!"), format!("gifts delivered: {}", grouped(gifts)))
        }
        None => {
            let today = now.date_naive();
            let year = if (today.month(), today.day()) > (12, 25) { today.year() + 1 } else { today.year() };
            let eve = chrono::NaiveDate::from_ymd_opt(year, 12, 24).unwrap_or(today);
            let days = (eve - today).num_days().max(0);
            let wrapped = 3_400_000_000u64 + (((365 - days.min(365)) as f64 / 365.0) * 4_000_000_000.0) as u64 + (t * 37.0) as u64;
            let when = match days {
                0 => "home again: a sleep, then next year's list".to_string(),
                1 => "he sets out tomorrow night!".to_string(),
                n => format!("he sets out in {n} days"),
            };
            (format!("Santa's at the North Pole, getting ready · {when}"), format!("presents wrapped so far: {}", grouped(wrapped)))
        }
    }
}

fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Train Cam: the Village Express through the snowy hills at dusk.
fn train(p: &mut Px, t: f32) {
    p.sky(Rgb::hex(0x2a2a5a), Rgb::hex(0xc87a6a), 60);
    p.disc(122.0, 22.0, 7.0, Rgb::hex(0xffe0a8));
    // far hills, then near pines, scrolling (the camera rides along)
    for x in 0..W as i32 {
        let h = 44.0 + 8.0 * noise1(x as f32 * 0.03 + t * 0.05, 7);
        for y in h as i32..62 {
            p.set(x, y, Rgb::hex(0x6a6a9a) * (0.9 + 0.1 * (y as f32 - h) / 18.0));
        }
        let snow = h as i32;
        p.set(x, snow, Rgb::hex(0xeef0fa));
    }
    for k in 0..12 {
        let x = (k as f32 * 19.0 - t * 18.0).rem_euclid(W as f32 + 20.0) - 10.0;
        let h = 10 + (hash(k, 4) % 8) as i32;
        for r in 0..h {
            let half = r / 3 + 1;
            p.rect(x as i32 - half, 62 - h + r, half * 2 + 1, 1, Rgb::hex(0x1f4a32));
        }
    }
    for y in 62..H as i32 {
        for x in 0..W as i32 {
            p.set(x, y, Rgb::hex(0xdde4f2));
        }
    }
    // the rails
    for x in 0..W as i32 {
        p.set(x, 74, Rgb::hex(0x5a5a64));
        if (x + (t * 40.0) as i32) % 5 == 0 {
            p.rect(x, 75, 2, 1, Rgb::hex(0x6a4a2a));
        }
    }
    // the train: the engine and three cars, lit windows, smoke
    let x0 = 150.0 - (t * 22.0).rem_euclid(300.0);
    let bob = ((t * 8.0) as i32 % 2) as i32;
    let ex = x0 as i32;
    p.rect(ex, 63 - bob, 22, 10, Rgb::hex(0xc8202a));
    p.rect(ex + 14, 57 - bob, 8, 6, Rgb::hex(0x2a2a30));
    p.rect(ex + 3, 58 - bob, 3, 5, Rgb::hex(0x2a2a30));
    p.rect(ex - 1, 70, 24, 1, Rgb::hex(0xffd36b));
    for (k, col) in [Rgb::hex(0x2a6a3a), Rgb::hex(0x2a4c8a), Rgb::hex(0xc8862a)].iter().enumerate() {
        let cx = ex + 25 + k as i32 * 24;
        p.rect(cx, 63 - bob, 22, 10, *col);
        for w in 0..3 {
            p.rect(cx + 3 + w * 6, 65 - bob, 4, 3, Rgb::hex(0xffcf6a));
        }
    }
    for w in 0..12 {
        p.disc((ex + 4 + w * 8) as f32, 73.0, 1.5, Rgb::hex(0x1a1a20));
    }
    for k in 0..7 {
        let a = (t * 1.2 + k as f32 / 7.0) % 1.0;
        p.disc(ex as f32 + 4.0 + a * 30.0, 55.0 - a * 14.0, 2.0 + a * 4.0, Rgb::hex(0xe0e4ee) * (1.0 - a * 0.6));
    }
    p.snowfall(t, 30, 6.0);
}

/// The Opera Game (Morphy, Paris 1858): (from, to) squares, files a–h as
/// 0–7, ranks 1–8 as 0–7; castling also moves its rook.
const GAME: [((u8, u8), (u8, u8)); 33] = [
    ((4, 1), (4, 3)),
    ((4, 6), (4, 4)),
    ((6, 0), (5, 2)),
    ((3, 6), (3, 5)),
    ((3, 1), (3, 3)),
    ((2, 7), (6, 3)),
    ((3, 3), (4, 4)),
    ((6, 3), (5, 2)),
    ((3, 0), (5, 2)),
    ((3, 5), (4, 4)),
    ((5, 0), (2, 3)),
    ((6, 7), (5, 5)),
    ((5, 2), (1, 2)),
    ((3, 7), (4, 6)),
    ((1, 0), (2, 2)),
    ((2, 6), (2, 5)),
    ((2, 0), (6, 4)),
    ((1, 6), (1, 4)),
    ((2, 2), (1, 4)),
    ((2, 5), (1, 4)),
    ((2, 3), (1, 4)),
    ((1, 7), (3, 6)),
    ((4, 0), (2, 0)),
    ((0, 7), (3, 7)),
    ((3, 0), (3, 6)),
    ((3, 7), (3, 6)),
    ((7, 0), (3, 0)),
    ((4, 6), (4, 5)),
    ((1, 4), (3, 6)),
    ((5, 5), (3, 6)),
    ((1, 2), (1, 7)),
    ((3, 6), (1, 7)),
    ((3, 0), (3, 7)),
];

/// The pieces after the first `n` moves: board[file][rank] = (white, kind:
/// 1 pawn, 2 knight, 3 bishop, 4 rook, 5 queen, 6 king).
fn board_after(n: usize) -> [[Option<(bool, u8)>; 8]; 8] {
    let mut b = [[None; 8]; 8];
    let back = [4u8, 2, 3, 5, 6, 3, 2, 4];
    for f in 0..8 {
        b[f][0] = Some((true, back[f]));
        b[f][1] = Some((true, 1));
        b[f][6] = Some((false, 1));
        b[f][7] = Some((false, back[f]));
    }
    for &((ff, fr), (tf, tr)) in GAME.iter().take(n) {
        let piece = b[ff as usize][fr as usize].take();
        b[tf as usize][tr as usize] = piece;
        // castling queen-side (the king two squares along): its rook too
        if let Some((w, 6)) = piece {
            if ff == 4 && tf == 2 {
                let r = if w { 0 } else { 7 };
                b[3][r] = b[0][r].take();
            }
        }
    }
    b
}

const PLAYERS: [(&str, &str); 4] = [("Holly Frost", "Jasper Pine"), ("Mika Snow", "Ivy Bell"), ("Noel Berry", "Clara Star"), ("Rudy Cole", "Wren Holly")];

/// Board Game Championship: the players either side, the board between,
/// the game playing out a move every couple of seconds (then again).
fn board_game(p: &mut Px, t: f32) {
    p.sky(Rgb::hex(0x1a1428), Rgb::hex(0x2a2040), H as i32);
    // the stage's spotlight
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let d = (((x - 80) as f32 / 70.0).powi(2) + ((y - 50) as f32 / 50.0).powi(2)).sqrt();
            p.blend(x, y, Rgb::hex(0x6a5a8a), (1.0 - d).max(0.0) * 0.4);
        }
    }
    let step = 2.4;
    let n = ((t / step) as usize) % (GAME.len() + 4);
    let n = n.min(GAME.len());
    let b = board_after(n);
    let (bx, by, sq) = (52, 9, 7);
    for f in 0..8 {
        for r in 0..8 {
            let (x, y) = (bx + f * sq, by + (7 - r) * sq);
            let light = (f + r) % 2 == 1;
            p.rect(x, y, sq, sq, if light { Rgb::hex(0xe8d8b0) } else { Rgb::hex(0x6a4a2a) });
            if let Some((white, kind)) = b[f as usize][r as usize] {
                let c = if white { Rgb::hex(0xfff4d8) } else { Rgb::hex(0x1a1418) };
                let e = if white { Rgb::hex(0x8a7a5a) } else { Rgb::hex(0x8a8aa0) };
                let (cx, cy) = (x + 3, y + 3);
                match kind {
                    1 => p.rect(cx - 1, cy - 1, 2, 3, c),
                    2 => {
                        p.rect(cx - 1, cy - 2, 3, 2, c);
                        p.rect(cx - 1, cy, 2, 3, c);
                    }
                    3 => {
                        p.rect(cx, cy - 2, 1, 5, c);
                        p.rect(cx - 1, cy - 1, 3, 3, c);
                    }
                    4 => p.rect(cx - 2, cy - 2, 5, 5, c),
                    5 => {
                        p.rect(cx - 2, cy - 1, 5, 4, c);
                        p.set(cx - 2, cy - 2, c);
                        p.set(cx, cy - 2, c);
                        p.set(cx + 2, cy - 2, c);
                    }
                    _ => {
                        p.rect(cx - 2, cy - 1, 5, 4, c);
                        p.rect(cx, cy - 3, 1, 2, c);
                    }
                }
                p.set(cx - 2, cy + 2, e);
            }
        }
    }
    // the move just made, picked out
    if n > 0 && n <= GAME.len() {
        let ((ff, fr), (tf, tr)) = GAME[n - 1];
        for (f, r) in [(ff, fr), (tf, tr)] {
            let (x, y) = (bx + f as i32 * sq, by + (7 - r as i32) * sq);
            for k in 0..sq {
                p.blend(x + k, y, Rgb::hex(0xffd36b), 0.8);
                p.blend(x + k, y + sq - 1, Rgb::hex(0xffd36b), 0.8);
                p.blend(x, y + k, Rgb::hex(0xffd36b), 0.8);
                p.blend(x + sq - 1, y + k, Rgb::hex(0xffd36b), 0.8);
            }
        }
    }
    // the players, at their desks, thinking (whoever's to move)
    let white_to_move = n % 2 == 0;
    for (k, x) in [(0, 22), (1, 128)] {
        let thinking = (k == 0) == white_to_move;
        let nod = if thinking { ((t * 2.0) as i32 % 2) as f32 } else { 0.0 };
        p.rect(x - 8, 62, 16, 20, Rgb::hex(0x4a2a1a));
        p.rect(x - 5, 44, 10, 18, if k == 0 { Rgb::hex(0xc8202a) } else { Rgb::hex(0x2a4c8a) });
        p.disc(x as f32, 38.0 + nod, 5.0, Rgb::hex(0xf0c8a8));
        p.rect(x - 5, 32 + nod as i32, 10, 3, if k == 0 { Rgb::hex(0x6a4a2a) } else { Rgb::hex(0x2a1c14) });
        if thinking {
            p.disc(x as f32 + 8.0, 28.0, 1.0, Rgb::WHITE);
            p.disc(x as f32 + 11.0, 24.0, 1.5, Rgb::WHITE);
        }
    }
}

fn board_caption(t: f32) -> (String, String) {
    let round = ((t / (2.4 * (GAME.len() + 4) as f32)) as usize) % PLAYERS.len();
    let (a, b) = PLAYERS[round];
    let n = (((t / 2.4) as usize) % (GAME.len() + 4)).min(GAME.len());
    let line = if n == GAME.len() { format!("checkmate! {a} wins the round ✦") } else { format!("move {} · {} to play", n / 2 + 1, if n % 2 == 0 { a } else { b }) };
    (format!("Board Game Championship · {a} (gold) vs. {b} (silver)"), line)
}

/// The skier's run: how far down (0..1) at `t`, and which run.
fn ski_run(t: f32) -> (f32, usize) {
    let len = 14.0;
    let k = (t / (len + 3.0)) as usize;
    (((t % (len + 3.0)) / len).min(1.0), k)
}

const SKIERS: [&str; 6] = ["Mika", "Sven", "Astrid", "Noel", "Kari", "Tove"];

/// Ski Championship: the slope, its gates, a racer down through them.
fn ski(p: &mut Px, t: f32) {
    p.sky(Rgb::hex(0x5a8ad8), Rgb::hex(0xbfe0ff), H as i32);
    // the mountain behind, the slope down across the screen
    for x in 0..W as i32 {
        let top = 18.0 + 10.0 * noise1(x as f32 * 0.04, 3);
        for y in top as i32..H as i32 {
            p.set(x, y, Rgb::hex(0x9aa8c8));
        }
    }
    for x in 0..W as i32 {
        let slope = 10.0 + x as f32 * 0.45;
        for y in slope as i32..H as i32 {
            p.set(x, y, Rgb::hex(0xf2f6ff) * (0.92 + 0.08 * noise1(x as f32 * 0.2 + y as f32 * 0.1, 5)));
        }
    }
    // pines along the side
    for k in 0..8 {
        let x = 6 + k * 20;
        let base = (10.0 + x as f32 * 0.45) as i32 - 1;
        for r in 0..9 {
            let half = r / 3 + 1;
            p.rect(x - half, base - 9 + r, half * 2 + 1, 1, Rgb::hex(0x1f5a3a));
        }
    }
    // the gates, red and blue in turn
    let path = |s: f32| -> (f32, f32) {
        let x = 14.0 + s * 132.0;
        (x + 7.0 * (s * 18.0).sin(), 14.0 + x * 0.45 + 6.0)
    };
    for g in 0..8 {
        let s = (g as f32 + 0.5) / 8.0;
        let (x, y) = path(s);
        let col = if g % 2 == 0 { pal::CANDY_RED } else { Rgb::hex(0x2a6ad8) };
        p.rect(x as i32, y as i32 - 6, 1, 6, Rgb::hex(0x3a3a44));
        p.rect(x as i32 + 1, y as i32 - 6, 3, 3, col);
    }
    // the racer, and the spray behind
    let (s, run) = ski_run(t);
    let (x, y) = path(s);
    for k in 1..8 {
        let (bx, byy) = path((s - k as f32 * 0.006).max(0.0));
        p.blend(bx as i32, byy as i32 + 1, Rgb::WHITE, 0.8 - k as f32 * 0.1);
    }
    let suit = [Rgb::hex(0xc8202a), Rgb::hex(0xffc23a), Rgb::hex(0x2fae5c)][run % 3];
    p.rect(x as i32 - 1, y as i32 - 5, 3, 4, suit);
    p.disc(x, y - 6.5, 1.5, Rgb::hex(0xf0c8a8));
    p.rect(x as i32 - 3, y as i32 - 1, 7, 1, Rgb::hex(0x2a2a30));
}

fn ski_caption(t: f32) -> (String, String) {
    let (s, run) = ski_run(t);
    let who = SKIERS[run % SKIERS.len()];
    let secs = 38.0 + s * 33.0 + (hash(run as u32, 9) % 40) as f32 / 10.0 * s;
    let best = 70.9;
    let line = if s >= 1.0 {
        if secs < best { format!("{who} finishes: {:.1}s · a new leader! ✦", secs) } else { format!("{who} finishes: {:.1}s · leader: Sven {best}s", secs) }
    } else {
        format!("run {} · {who} · {:.1}s · leader: Sven {best}s", run + 1, secs)
    };
    ("Ski Championship · the downhill, live from Pine Peak".to_string(), line)
}

/// Yule Log: the fire, crackling on.
fn yule_log(p: &mut Px, t: f32) {
    p.rect(0, 0, W as i32, H as i32, Rgb::hex(0x1a0e0a));
    // the hearth's bricks
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let row = y / 6;
            let edge = y % 6 == 0 || (x + if row % 2 == 0 { 0 } else { 6 }) % 12 == 0;
            let inside = (24..136).contains(&x) && y > 18;
            if !inside {
                p.set(x, y, if edge { Rgb::hex(0x3a1a12) } else { Rgb::hex(0x7a3424) * (0.8 + 0.2 * noise1(x as f32 * 0.5, y as u32)) });
            }
        }
    }
    // the flames: tall in the middle, flickering
    for x in 32..128 {
        let mid = 1.0 - ((x - 80) as f32 / 50.0).abs();
        let h = (22.0 + 26.0 * mid) * (0.7 + 0.45 * noise1(x as f32 * 0.12 + t * 2.4, 3) + 0.15 * (t * 9.0 + x as f32).sin());
        for k in 0..h as i32 {
            let u = k as f32 / h;
            let c = Rgb::hex(0xffe08a).lerp(Rgb::hex(0xff7a1a), u.min(1.0)).lerp(Rgb::hex(0xc81a0a), (u - 0.6).max(0.0) * 2.0);
            p.blend(x, 72 - k, c, 1.0 - u * 0.7);
        }
    }
    // the logs, and the embers rising
    for (x, y, w) in [(36, 72, 50), (74, 74, 52), (52, 77, 56)] {
        p.rect(x, y, w, 5, Rgb::hex(0x5a3420));
        p.rect(x, y, w, 1, Rgb::hex(0x7a4a2a));
        p.disc(x as f32 + w as f32, y as f32 + 2.5, 2.5, Rgb::hex(0xc89a6a));
    }
    for k in 0..16 {
        let a = (t * 0.4 + k as f32 / 16.0) % 1.0;
        let x = 50.0 + (hash(k, 3) % 60) as f32 + (a * 12.0 + k as f32).sin() * 4.0;
        p.set(x as i32, (70.0 - a * 60.0) as i32, Rgb::hex(0xffb050) * (1.0 - a));
    }
}

/// What's on, in words: the title, and a line about it.
pub fn caption(c: u8, t: f32, now: chrono::DateTime<chrono::Local>) -> (String, String) {
    match c {
        0 => ("North Pole Cam · live from Santa's workshop".to_string(), format!("the elves are busy: {} days to go", days_to_christmas(now))),
        1 => santa_caption(now, t),
        2 => ("Train Cam · the Village Express".to_string(), "on time · next stop: the ski lodge".to_string()),
        3 => board_caption(t),
        4 => ski_caption(t),
        _ => ("Yule Log · all night, every night".to_string(), "*crackle* *pop*".to_string()),
    }
}

fn days_to_christmas(now: chrono::DateTime<chrono::Local>) -> i64 {
    let d = now.date_naive();
    let year = if (d.month(), d.day()) > (12, 25) { d.year() + 1 } else { d.year() };
    chrono::NaiveDate::from_ymd_opt(year, 12, 25).map(|c| (c - d).num_days()).unwrap_or(0)
}

/// The set, over the room: a wooden cabinet round the picture, the channel
/// and what's on under it, and how to change it. Returns where it is (x, y,
/// w, h), for a tap on it.
pub fn draw(f: &mut Frame, tv: &Tv, t: f32, look: &str) -> (i32, i32, i32, i32) {
    let now = crate::clock::now();
    let (title, line) = caption(tv.channel, t - tv.since, now);
    let (fw, fh) = (f.w as i32, f.h as i32);
    // the picture's cells, keeping its shape (16:9)
    let rows = ((fh - 10).min(24)).max(6);
    let cols = ((rows as f32 * 16.0 / 9.0 * f.cell_aspect).round() as i32).min(fw - 8).max(16);
    let rows = ((cols as f32 * 9.0 / 16.0 / f.cell_aspect).round() as i32).max(4);
    let (w, h) = (cols + 4, rows + 6);
    let (x, y) = ((fw - w) / 2, ((fh - h) / 2).max(0));
    let wood = Rgb::hex(0x5a3420);
    let bg = Bg::Solid(wood * 0.8);
    for yy in y..y + h {
        for xx in x..x + w {
            let edge = yy == y || yy == y + h - 1 || xx == x || xx == x + w - 1;
            f.put_g(xx, yy, Glyph::new(if edge { '▓' } else { ' ' }, wood * 1.2).bg(if edge { Bg::Solid(wood) } else { bg }).lit(0.0));
        }
    }
    // the screen
    if crate::frame::pictures_ok() {
        for yy in y + 1..y + 1 + rows {
            for xx in x + 2..x + 2 + cols {
                f.put_g(xx, yy, Glyph::new(' ', Rgb::BLACK).bg(Bg::Solid(Rgb::hex(0x05060a))).lit(0.0));
            }
        }
        f.picture_on_panel(x + 2, y + 1, cols, rows, picture(tv.channel, t - tv.since, now));
    } else {
        // (a terminal: the channel's colours, and what's on)
        for yy in y + 1..y + 1 + rows {
            for xx in x + 2..x + 2 + cols {
                let n = noise1(xx as f32 * 0.2 + t, yy as u32 + tv.channel as u32 * 7);
                f.put_g(xx, yy, Glyph::new(' ', Rgb::BLACK).bg(Bg::Solid(Rgb::hex(0x1a2a4a) * (0.6 + 0.6 * n))).lit(0.0));
            }
        }
    }
    let clip = |s: &str| -> String { s.chars().take((w - 4).max(4) as usize).collect() };
    let cy = y + rows + 2;
    f.text(x + 2, cy, &clip(&format!("▣ {} · {title}", tv.channel + 1)), Rgb::hex(0xffd36b), bg, 0.0);
    f.text(x + 2, cy + 1, &clip(&line), pal::UI_TEXT, bg, 0.0);
    f.text(x + 2, cy + 2, &clip("E or a tap: the next channel · Esc or a step away: off"), pal::UI_DIM * 1.2, bg, 0.0);
    crate::widgets::looks::apply(f, crate::widgets::Rect { x, y, w, h }, look, bg, 5);
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_channel_draws_and_says_whats_on() {
        let now = crate::clock::now();
        for c in 0..CHANNELS.len() as u8 {
            for t in [0.0, 3.3, 61.0] {
                let p = picture(c, t, now);
                assert_eq!((p.w, p.h), (W, H));
                // something on the screen (not just black)
                assert!(p.rgba.chunks(4).filter(|px| px[0] as u32 + px[1] as u32 + px[2] as u32 > 120).count() > 500, "channel {c} at {t}");
                let (title, line) = caption(c, t, now);
                assert!(!title.is_empty() && !line.is_empty());
            }
        }
        // the channels go round, and then the set's off
        let mut tv = Some(Tv { channel: 0, since: 0.0 });
        let mut seen = 0;
        while let Some(v) = tv {
            seen += 1;
            tv = v.next(1.0);
        }
        assert_eq!(seen, CHANNELS.len());
        // the championship's game is a real one: the last move mates
        let b = board_after(GAME.len());
        assert_eq!(b[3][7], Some((true, 4)), "the rook on d8");
        assert_eq!(b[4][7], Some((false, 6)), "their king on e8");
    }

    /// Each channel as a picture, for a look (XMAS_TV_SHOTS=folder):
    /// `cargo test --release tv_pictures -- --ignored`
    #[test]
    #[ignore]
    fn tv_pictures() {
        let dir = std::env::var("XMAS_TV_SHOTS").unwrap_or_else(|_| ".".into());
        let now = crate::clock::now();
        for c in 0..CHANNELS.len() as u8 {
            let p = picture(c, 7.3, now);
            let file = std::fs::File::create(format!("{dir}/tv-{c}.png")).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), p.w as u32, p.h as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&p.rgba).unwrap();
        }
    }

    #[test]
    fn santa_sets_out_on_christmas_eve() {
        use chrono::TimeZone;
        let at = |m: u32, d: u32, h: u32| chrono::Utc.with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap().with_timezone(&chrono::Local);
        assert_eq!(santa_trip(at(11, 17, 12)), None);
        assert_eq!(santa_trip(at(12, 24, 9)), None);
        assert!(santa_trip(at(12, 24, 20)).is_some_and(|f| f > 0.3 && f < 0.5));
        assert_eq!(santa_trip(at(12, 25, 13)), None);
        assert!(santa_caption(at(12, 24, 20), 0.0).0.starts_with("Santa's over"));
        assert!(santa_caption(at(11, 17, 12), 0.0).0.contains("getting ready"));
    }
}
