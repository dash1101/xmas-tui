//! Terminal backend: diffs against what's on screen and only emits changed
//! cells, wrapped in synchronized-output so kitty never shows a torn frame.
//!
//! The output is kept small because kitty has to parse every byte (at 30
//! fps on a 568x75 grid this is the bulk of kitty's CPU):
//! - half-block cells are compared by how they *look* (a top and a bottom
//!   colour), so ' ', '█' and '▀' are interchangeable spellings;
//! - a colour within `tolerance` of the current pen is reused instead of
//!   switching pens (the same invisible slack already allowed for deciding
//!   a cell is unchanged);
//! - fg and bg go in one SGR, and short hops along a row use relative moves.

use crate::frame::Cell;
use std::io::{self, Write};

pub struct Term {
    /// What is on screen (as last emitted, not as last requested).
    prev: Vec<Cell>,
    w: usize,
    h: usize,
    buf: Vec<u8>,
    force: bool,
    /// Colors closer than this (per channel, 0..255) count as unchanged.
    pub tolerance: u8,
    pub bytes_last: usize,
}

/// How a cell looks: half-block cells are just two colours, whichever of
/// ' ', '█' or '▀' spelled them.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Look {
    Halves([u8; 3], [u8; 3]),
    Glyph(char, [u8; 3], [u8; 3]),
}

fn look(c: Cell) -> Look {
    match c.ch {
        ' ' => Look::Halves(c.bg, c.bg),
        '█' => Look::Halves(c.fg, c.fg),
        '▀' => Look::Halves(c.fg, c.bg),
        ch => Look::Glyph(ch, c.fg, c.bg),
    }
}

/// Characters that certainly take one column in kitty, so the cursor
/// position after printing them is known. (Anything else — e.g. an emoji in
/// a song title — makes the next cell use an absolute move.)
fn narrow(c: char) -> bool {
    let u = c as u32;
    u < 0x1100
        || (0x2000..0x231a).contains(&u)
        || (0x2500..0x25fd).contains(&u)
        || (0x2600..0x2614).contains(&u)
        || (0x2654..0x267f).contains(&u)
        || (0x2730..0x274c).contains(&u)
}

fn push_num(b: &mut Vec<u8>, mut v: usize) {
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    loop {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    b.extend_from_slice(&tmp[i..]);
}

fn push_rgb(b: &mut Vec<u8>, c: [u8; 3]) {
    for (k, v) in c.into_iter().enumerate() {
        if k > 0 {
            b.push(b';');
        }
        push_num(b, v as usize);
    }
}

impl Term {
    pub fn new() -> Self {
        Term { prev: vec![], w: 0, h: 0, buf: Vec::with_capacity(1 << 20), force: true, tolerance: 2, bytes_last: 0 }
    }

    pub fn enter() -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        let mut o = io::stdout();
        // alt screen, hide cursor, no autowrap, clear
        o.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J")?;
        crossterm::execute!(o, crossterm::event::EnableMouseCapture)?;
        // kitty keyboard protocol: report key releases too, so holding a
        // direction key walks smoothly and letting go stops at once
        // (terminals without it ignore this; see App::key_up)
        let flags = crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
        let _ = crossterm::execute!(o, crossterm::event::PushKeyboardEnhancementFlags(flags));
        o.flush()
    }

    /// Set the mouse pointer shape (kitty OSC 22: default, pointer, grab, grabbing, text…).
    pub fn pointer(shape: &str) {
        let mut o = io::stdout();
        let _ = write!(o, "\x1b]22;{shape}\x1b\\");
        let _ = o.flush();
    }

    pub fn leave() {
        let mut o = io::stdout();
        let _ = crossterm::execute!(o, crossterm::event::PopKeyboardEnhancementFlags);
        let _ = crossterm::execute!(o, crossterm::event::DisableMouseCapture);
        let _ = o.write_all(b"\x1b]22;default\x1b\\");
        let _ = o.write_all(b"\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l");
        let _ = o.flush();
        let _ = crossterm::terminal::disable_raw_mode();
    }

    pub fn invalidate(&mut self) {
        self.force = true;
    }

    pub fn draw(&mut self, cells: &[Cell], w: usize, h: usize) -> io::Result<()> {
        self.render(cells, w, h);
        let mut o = io::stdout().lock();
        o.write_all(&self.buf)?;
        o.flush()
    }

    /// Build the escape-sequence update for this frame into `self.buf`.
    pub fn render(&mut self, cells: &[Cell], w: usize, h: usize) -> usize {
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            self.prev = vec![Cell::default(); w * h];
            self.force = true;
        }
        let tol = self.tolerance as i16;
        let close = |a: [u8; 3], b: [u8; 3]| {
            (a[0] as i16 - b[0] as i16).abs() <= tol
                && (a[1] as i16 - b[1] as i16).abs() <= tol
                && (a[2] as i16 - b[2] as i16).abs() <= tol
        };
        // the current pen colour if it's close enough to stand in for `want`
        let near = |pen: Option<[u8; 3]>, want: [u8; 3]| pen.filter(|&p| close(p, want));
        let b = &mut self.buf;
        b.clear();
        b.extend_from_slice(b"\x1b[?2026h");
        if self.force {
            b.extend_from_slice(b"\x1b[0m\x1b[2J");
        }
        let mut cur_fg: Option<[u8; 3]> = None;
        let mut cur_bg: Option<[u8; 3]> = None;
        // where the terminal cursor is, if known
        let mut cursor: Option<(usize, usize)> = None;
        let mut chb = [0u8; 4];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let want = look(cells[i]);
                if !self.force {
                    let same = match (want, look(self.prev[i])) {
                        (Look::Halves(t, u), Look::Halves(pt, pu)) => close(t, pt) && close(u, pu),
                        (Look::Glyph(c, f, g), Look::Glyph(pc, pf, pg)) => c == pc && close(f, pf) && close(g, pg),
                        _ => false,
                    };
                    if same {
                        continue;
                    }
                }
                // ---- move there ----
                match cursor {
                    Some((cx, cy)) if cy == y && cx == x => {}
                    Some((cx, cy)) if cy == y && cx < x => {
                        // a one- or two-cell gap of plain spaces in the
                        // current bg is cheaper to reprint than to skip
                        let gap = &self.prev[y * w + cx..i];
                        if gap.len() <= 2 && gap.iter().all(|p| look(*p) == Look::Halves(p.bg, p.bg) && Some(p.bg) == cur_bg) {
                            b.resize(b.len() + gap.len(), b' ');
                        } else {
                            b.extend_from_slice(b"\x1b[");
                            if x - cx > 1 {
                                push_num(b, x - cx);
                            }
                            b.push(b'C');
                        }
                    }
                    _ => {
                        b.extend_from_slice(b"\x1b[");
                        push_num(b, y + 1);
                        if x > 0 {
                            b.push(b';');
                            push_num(b, x + 1);
                        }
                        b.push(b'H');
                    }
                }
                // ---- pick the spelling that needs the fewest pen changes ----
                let (ch, fg, bg) = match want {
                    Look::Halves(t, u) if close(t, u) => {
                        if let Some(p) = near(cur_bg, t).filter(|&p| close(p, u)) {
                            (' ', None, Some(p))
                        } else if let Some(p) = near(cur_fg, t).filter(|&p| close(p, u)) {
                            ('█', Some(p), None)
                        } else {
                            (' ', None, Some(u))
                        }
                    }
                    Look::Halves(t, u) => ('▀', Some(near(cur_fg, t).unwrap_or(t)), Some(near(cur_bg, u).unwrap_or(u))),
                    Look::Glyph(c, f, g) => (c, Some(near(cur_fg, f).unwrap_or(f)), Some(near(cur_bg, g).unwrap_or(g))),
                };
                let set_fg = fg.filter(|&f| cur_fg != Some(f));
                let set_bg = bg.filter(|&g| cur_bg != Some(g));
                if set_fg.is_some() || set_bg.is_some() {
                    b.extend_from_slice(b"\x1b[");
                    if let Some(f) = set_fg {
                        b.extend_from_slice(b"38;2;");
                        push_rgb(b, f);
                        cur_fg = Some(f);
                    }
                    if let Some(g) = set_bg {
                        if set_fg.is_some() {
                            b.push(b';');
                        }
                        b.extend_from_slice(b"48;2;");
                        push_rgb(b, g);
                        cur_bg = Some(g);
                    }
                    b.push(b'm');
                }
                b.extend_from_slice(ch.encode_utf8(&mut chb).as_bytes());
                cursor = (x + 1 < w && narrow(ch)).then_some((x + 1, y));
                // remember what's actually on screen now
                let (f, g) = (fg.or(cur_fg).unwrap_or_default(), bg.or(cur_bg).unwrap_or_default());
                self.prev[i] = Cell { ch, fg: if ch == ' ' { g } else { f }, bg: if ch == '█' { f } else { g } };
            }
        }
        b.extend_from_slice(b"\x1b[?2026l");
        self.force = false;
        self.bytes_last = b.len();
        b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny terminal for exactly the sequences `render` emits.
    struct Screen {
        w: usize,
        cells: Vec<Cell>,
        x: usize,
        y: usize,
        fg: [u8; 3],
        bg: [u8; 3],
    }

    impl Screen {
        fn feed(&mut self, bytes: &[u8]) {
            let s = std::str::from_utf8(bytes).unwrap();
            let mut it = s.chars().peekable();
            while let Some(c) = it.next() {
                if c != '\x1b' {
                    let i = self.y * self.w + self.x;
                    self.cells[i] = Cell { ch: c, fg: self.fg, bg: self.bg };
                    self.x += 1;
                    continue;
                }
                assert_eq!(it.next(), Some('['));
                let mut p = String::new();
                let fin = loop {
                    let c = it.next().unwrap();
                    if c.is_ascii_alphabetic() {
                        break c;
                    }
                    p.push(c);
                };
                let nums: Vec<usize> = p.trim_start_matches('?').split(';').map(|v| v.parse().unwrap_or(0)).collect();
                match fin {
                    'H' => {
                        self.y = nums[0].max(1) - 1;
                        self.x = nums.get(1).copied().unwrap_or(1).max(1) - 1;
                    }
                    'C' => self.x += nums[0].max(1),
                    'm' if p != "0" => {
                        let mut k = 0;
                        while k < nums.len() {
                            let c = [nums[k + 2] as u8, nums[k + 3] as u8, nums[k + 4] as u8];
                            if nums[k] == 38 { self.fg = c } else { self.bg = c }
                            k += 5;
                        }
                    }
                    _ => {} // 0m, 2J, sync on/off
                }
            }
        }
    }

    #[test]
    fn screen_tracks_scene_within_tolerance() {
        let (w, h) = (160, 40);
        let cfg = crate::config::Config::default();
        let mut frame = crate::frame::Frame::new(w, h);
        let mut scene = crate::scene::Scene::new(w, h, 7, &cfg.scene);
        let mut term = Term::new();
        let mut scr = Screen { w, cells: vec![Cell::default(); w * h], x: 0, y: 0, fg: [0; 3], bg: [0; 3] };
        let mut t = 0.0;
        for _ in 0..90 {
            scene.update(1.0 / 30.0, t);
            frame.clear();
            scene.draw(&mut frame, t);
            frame.compose();
            term.render(&frame.out, w, h);
            scr.feed(&term.buf);
            for (i, (&want, &got)) in frame.out.iter().zip(&scr.cells).enumerate() {
                let ok = match (look(want), look(got)) {
                    (Look::Halves(a, b), Look::Halves(c, d)) => {
                        let d1 = (0..3).all(|k| (a[k] as i16 - c[k] as i16).abs() <= 2 && (b[k] as i16 - d[k] as i16).abs() <= 2);
                        d1
                    }
                    (Look::Glyph(a, f, g), Look::Glyph(b, f2, g2)) => {
                        a == b && (0..3).all(|k| (f[k] as i16 - f2[k] as i16).abs() <= 2 && (g[k] as i16 - g2[k] as i16).abs() <= 2)
                    }
                    _ => false,
                };
                assert!(ok, "cell {i} at t={t}: want {want:?} got {got:?}");
            }
            // and the encoder's idea of the screen is the real one
            assert!(term.prev.iter().zip(&scr.cells).all(|(a, b)| look(*a) == look(*b)));
            t += 1.0 / 30.0;
        }
    }
}
