//! Friends & chats: a window with each friend down the left (a little
//! figure in their colours, their name, online or not, what's new, a ⋯
//! for their menu) and the chat with the one picked on the right, with a
//! line at the bottom to write the next. Drawn over everything (see
//! `draw_top`), the same on the desktop, the web and a phone.

use super::online::{ago_short, NetDo, MORE_FRIEND, MORE_ONLINE};
use super::*;
use crate::color::pal;
use crate::frame::{Bg, Glyph};
use std::cell::{Cell, RefCell};

/// What a click on the window does (the front end does it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatsDo {
    /// Something online (write to them, join them…).
    Net(NetDo),
    /// A friend's own menu (the ⋯), or back to friends & online.
    Menu(u8),
    /// Nothing more (it was the window's own: a friend picked, closed).
    Done,
}

#[derive(Default)]
pub struct Chats {
    pub open: bool,
    /// The friend picked (their place in the book).
    pub sel: Option<usize>,
    /// Scrolled down the friends this many.
    top: usize,
    /// As last drawn: the window (x, y, w, h); the friends' rows (top row,
    /// their place); the ⋯ of each; the line to write on; the buttons up
    /// top and along the chat's head.
    pub(super) at: Cell<(i32, i32, i32, i32)>,
    pub(super) rows: RefCell<Vec<(i32, usize)>>,
    pub(super) dots: RefCell<Vec<(i32, i32, usize)>>,
    pub(super) write: Cell<Option<(i32, i32, i32)>>,
    pub(super) buttons: RefCell<Vec<(i32, i32, i32, ChatsDo)>>,
}

/// The window's size on a screen `w` × `h` (cells).
fn size(w: i32, h: i32) -> (i32, i32) {
    ((w - 4).clamp(40, 100), (h - 4).clamp(14, 30))
}

impl Scene {
    /// Open friends & chats (with friend `sel` picked, if given).
    pub fn open_chats(&mut self, sel: Option<usize>) {
        let c = &mut self.social.chats;
        c.open = true;
        if sel.is_some() {
            c.sel = sel;
        }
        let n = self.online.book.friends.len();
        if c.sel.is_none_or(|i| i >= n) {
            // (the one with something new, else the first)
            c.sel = (0..n).find(|&i| self.online.unread(i) > 0).or((n > 0).then_some(0));
        }
    }

    pub fn chats_open(&self) -> bool {
        self.social.chats.open
    }

    pub fn close_chats(&mut self) {
        self.social.chats.open = false;
    }

    /// Over the window (clicks there are its own).
    pub fn on_chats(&self, x: i32, y: i32) -> bool {
        let (x0, y0, w, h) = self.social.chats.at.get();
        self.social.chats.open && x >= x0 && x < x0 + w && y >= y0 && y < y0 + h
    }

    /// The wheel over the window: down (or up) the friends.
    pub fn chats_scroll(&mut self, down: bool) {
        let n = self.online.book.friends.len();
        let c = &mut self.social.chats;
        c.top = if down { (c.top + 1).min(n.saturating_sub(1)) } else { c.top.saturating_sub(1) };
    }

    /// A key while it's open: up and down the friends, Enter to write to
    /// the one picked, Esc to close.
    pub fn chats_key(&mut self, up: Option<bool>, enter: bool) -> Option<ChatsDo> {
        let n = self.online.book.friends.len();
        let c = &mut self.social.chats;
        if let Some(up) = up {
            if n > 0 {
                let i = c.sel.unwrap_or(0) as i32 + if up { -1 } else { 1 };
                c.sel = Some(i.clamp(0, n as i32 - 1) as usize);
            }
            return Some(ChatsDo::Done);
        }
        if enter {
            return c.sel.filter(|&i| i < n).map(|i| ChatsDo::Net(NetDo::Mail(i as u8)));
        }
        None
    }

    /// A click on the window at (x, y).
    pub fn chats_click(&mut self, x: i32, y: i32) -> ChatsDo {
        let c = &self.social.chats;
        let button = c.buttons.borrow().iter().find(|(bx, by, bw, _)| y == *by && x >= *bx && x < bx + bw).map(|b| b.3);
        if let Some(d) = button {
            // (back, or ✕: the window's put away)
            if matches!(d, ChatsDo::Menu(MORE_ONLINE) | ChatsDo::Done) {
                self.social.chats.open = false;
            }
            return d;
        }
        let dot = c.dots.borrow().iter().find(|(dx, dy, _)| y == *dy && x >= *dx - 1 && x <= dx + 1).map(|d| d.2);
        if let Some(i) = dot {
            self.social.chats.sel = Some(i);
            return ChatsDo::Menu(MORE_FRIEND + i as u8);
        }
        let c = &self.social.chats;
        if let Some((wx0, wy, wx1)) = c.write.get() {
            if y == wy && x >= wx0 && x < wx1 {
                if let Some(i) = c.sel {
                    return ChatsDo::Net(NetDo::Mail(i as u8));
                }
            }
        }
        let pick = c.rows.borrow().iter().rev().find(|(ry, _)| y >= *ry && y < ry + 3).map(|r| r.1);
        let (x0, _, w, _) = c.at.get();
        if let Some(i) = pick.filter(|_| x < x0 + list_w(w)) {
            self.social.chats.sel = Some(i);
        }
        ChatsDo::Done
    }

    /// Each frame it's open: the chat picked, read.
    pub(super) fn update_chats(&mut self) {
        if !self.social.chats.open {
            return;
        }
        if let Some(i) = self.social.chats.sel {
            self.online.mark_read(i);
        }
    }

    pub(super) fn draw_chats(&self, f: &mut Frame) {
        let c = &self.social.chats;
        if !c.open {
            return;
        }
        let (w, h) = size(f.w as i32, f.h as i32);
        let (x0, y0) = ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(1));
        c.at.set((x0, y0, w, h));
        // (over everything: the post's lines too)
        let keep = std::mem::replace(&mut f.keep_ui, false);
        let bg = Bg::Mix(pal::UI_PANEL, 0.96);
        let border = Rgb::hex(0xffd36b) * 0.8;
        let lw = list_w(w);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                    (true, _, true, _) => '╭',
                    (_, true, true, _) => '╮',
                    (true, _, _, true) => '╰',
                    (_, true, _, true) => '╯',
                    (_, _, true, _) | (_, _, _, true) => '─',
                    (true, _, _, _) | (_, true, _, _) => '│',
                    _ if x == x0 + lw && y > y0 + 1 => '│',
                    _ if y == y0 + 2 => '─',
                    _ => ' ',
                };
                f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
            }
        }
        let mut buttons = vec![];
        let mut button = |f: &mut Frame, x: i32, y: i32, label: &str, d: ChatsDo, col: Rgb| -> i32 {
            let n = label.chars().count() as i32;
            f.text(x, y, label, col, Bg::Mix(pal::UI_PANEL, 0.75), 0.0);
            buttons.push((x, y, n, d));
            x + n
        };
        // the title, back and close
        let x = button(f, x0 + 1, y0, " ◂ back ", ChatsDo::Menu(MORE_ONLINE), pal::UI_TITLE);
        f.text(x + 1, y0, " ✉ friends & chats ", Rgb::hex(0xffd36b), bg, 0.0);
        button(f, x0 + w - 5, y0, " ✕ ", ChatsDo::Done, pal::UI_TITLE);
        // the friend code, and adding one
        let o = &self.online;
        let code = if o.book.code.is_empty() { "…".to_string() } else { o.book.code.clone() };
        let x = f.text(x0 + 2, y0 + 1, "my friend code ", pal::UI_DIM, bg, 0.0);
        let x = button(f, x, y0 + 1, &format!(" {code} ⧉ "), ChatsDo::Net(NetDo::Code), Rgb::hex(0x9be08a));
        button(f, x + 2, y0 + 1, " ✚ add a friend ", ChatsDo::Net(NetDo::Add), pal::UI_TITLE);
        if !o.wanted() {
            button(f, x0 + w - 16, y0 + 1, " ● go online ", ChatsDo::Net(NetDo::On), Rgb::hex(0x9be08a));
        }
        // the friends, down the left: asked first, then online, then the rest
        let mut rows = vec![];
        let mut dots = vec![];
        let mut y = y0 + 3;
        let bottom = y0 + h - 1;
        for (i, p) in o.book.asking.iter().enumerate().take(3) {
            if y + 1 >= bottom {
                break;
            }
            let name: String = p.name.chars().take((lw - 4) as usize).collect();
            f.text(x0 + 2, y, &format!("✦ {name}"), Rgb::hex(0xffd36b), bg, 0.0);
            let x = button(f, x0 + 2, y + 1, " ✓ yes ", ChatsDo::Net(NetDo::Accept(i as u8)), Rgb::hex(0x9be08a));
            button(f, x + 1, y + 1, " no ", ChatsDo::Net(NetDo::Decline(i as u8)), pal::UI_DIM);
            y += 3;
        }
        let fr = o.friends();
        let mut order: Vec<usize> = (0..fr.len()).collect();
        order.sort_by_key(|&i| {
            let (p, h) = &fr[i];
            (p.waiting, !h.as_ref().is_some_and(|h| h.on), o.unread(i) == 0, p.name.to_lowercase())
        });
        let top = c.top.min(order.len().saturating_sub(1));
        for &i in order.iter().skip(top) {
            if y + 2 >= bottom {
                f.text(x0 + 2, bottom - 1, "  ▾ more (wheel)", pal::UI_DIM, bg, 0.0);
                break;
            }
            let (p, here) = &fr[i];
            let picked = c.sel == Some(i);
            let rbg = if picked { Bg::Mix(Rgb::hex(0x3a4a7a), 0.9) } else { bg };
            for yy in y..y + 3 {
                for xx in x0 + 1..x0 + lw {
                    f.put_g(xx, yy, Glyph::new(' ', pal::UI_TEXT).bg(rbg).lit(0.0));
                }
            }
            // their figure: hat over face, scarf over coat, coat over boots
            let look = here.as_ref().and_then(|h| h.card.as_ref()).map(|c| c.look).unwrap_or([0xc0392b, 0x2e7d32, 0x3a2a1a, 0xe8c39e, 0xf2f6ff]);
            figure(f, x0 + 2, y, look, p.waiting, rbg);
            let unread = o.unread(i);
            let name: String = p.name.chars().take((lw - 13) as usize).collect();
            let nx = f.text(x0 + 6, y, &name, if picked { pal::UI_TITLE } else { pal::UI_TEXT }, rbg, 0.0);
            if unread > 0 {
                f.text(nx + 1, y, &format!("✉{unread}"), Rgb::hex(0xffd36b), rbg, 0.0);
            }
            let (dot, col, what) = match here {
                _ if p.waiting => ('…', pal::UI_DIM, "asked"),
                Some(h) if h.village.is_some() => ('⌂', Rgb::hex(0xffd36b), "village open"),
                Some(h) if h.on => ('●', Rgb::hex(0x9be08a), "online"),
                _ => ('○', pal::UI_DIM, "offline"),
            };
            f.text(x0 + 6, y + 1, &format!("{dot} {what}"), col, rbg, 0.0);
            // its ⋯: their menu
            let dx = x0 + lw - 3;
            f.text(dx - 1, y, " ⋯ ", pal::UI_TITLE, Bg::Mix(pal::UI_PANEL, 0.7), 0.0);
            dots.push((dx, y, i));
            rows.push((y, i));
            y += 3;
        }
        if fr.is_empty() && o.book.asking.is_empty() {
            f.text(x0 + 2, y0 + 3, "no friends yet:", pal::UI_DIM, bg, 0.0);
            f.text(x0 + 2, y0 + 4, "give them your code,", pal::UI_DIM, bg, 0.0);
            f.text(x0 + 2, y0 + 5, "or add theirs (✚)", pal::UI_DIM, bg, 0.0);
        }
        // the chat with the one picked, on the right
        let (cx, cw) = (x0 + lw + 2, w - lw - 4);
        let mut write = None;
        match c.sel.and_then(|i| fr.get(i).map(|f| (i, f))) {
            Some((i, (p, here))) => {
                let name: String = p.name.chars().take((cw - 24).max(4) as usize).collect();
                let x = f.text(cx, y0 + 3, &name, Rgb::hex(0xffd36b), bg, 0.0);
                let (what, col) = match here {
                    _ if p.waiting => ("· asked, no answer yet", pal::UI_DIM),
                    Some(h) if h.village.is_some() => ("· ⌂ their village is open", Rgb::hex(0xffd36b) * 0.85),
                    Some(h) if h.on => ("· ● online", Rgb::hex(0x9be08a)),
                    _ => ("· ○ offline: lines reach them only while they're online", pal::UI_DIM),
                };
                let x = f.text(x + 1, y0 + 3, what, col, bg, 0.0);
                if here.as_ref().is_some_and(|h| h.village.is_some()) {
                    button(f, x + 2, y0 + 3, " ⌂ join ", ChatsDo::Net(NetDo::Join(i as u8)), Rgb::hex(0x9be08a));
                }
                let lines = o.chat(i);
                let now = crate::clock::now().timestamp();
                // the lines, wrapped, the latest at the bottom
                let mut out: Vec<(String, Rgb)> = vec![];
                for l in lines.iter().rev().take(40) {
                    let who = if l.mine { "you" } else { &p.name };
                    let head = format!("{who} · {}", ago_short(now - l.at));
                    let mut body = wrap(&l.text, (cw - 2).max(10) as usize);
                    body.reverse();
                    for b in body {
                        out.push((format!("  {b}"), if l.mine { pal::UI_TEXT * 0.9 } else { pal::UI_TEXT }));
                    }
                    out.push((head, if l.mine { pal::UI_DIM } else { Rgb::hex(0x9be08a) }));
                }
                let room = (bottom - 3 - (y0 + 5)).max(1) as usize;
                if out.is_empty() {
                    f.text(cx, y0 + 5, "nothing yet: say hello!", pal::UI_DIM, bg, 0.0);
                }
                for (k, (s, col)) in out.iter().take(room).enumerate() {
                    let s: String = s.chars().take(cw as usize).collect();
                    f.text(cx, bottom - 3 - k as i32, &s, *col, bg, 0.0);
                }
                // the line to write on
                if !p.waiting {
                    let wy = bottom - 1;
                    for xx in cx..cx + cw {
                        f.put_g(xx, wy, Glyph::new(' ', pal::UI_TEXT).bg(Bg::Mix(Rgb::hex(0x0b1020), 0.9)).lit(0.0));
                    }
                    let hint: String = format!("✎ write to {}…  (Enter)", p.name).chars().take(cw as usize).collect();
                    f.text(cx + 1, wy, &hint, pal::UI_DIM, Bg::Mix(Rgb::hex(0x0b1020), 0.9), 0.0);
                    write = Some((cx, wy, cx + cw));
                } else {
                    f.text(cx, bottom - 1, "waiting for them to say yes", pal::UI_DIM, bg, 0.0);
                }
            }
            None => {
                f.text(cx, y0 + 3, "pick a friend on the left to chat", pal::UI_DIM, bg, 0.0);
            }
        }
        f.keep_ui = keep;
        c.write.set(write);
        *c.rows.borrow_mut() = rows;
        *c.dots.borrow_mut() = dots;
        *c.buttons.borrow_mut() = buttons;
    }
}

/// The friends' column's width in a window `w` wide.
fn list_w(w: i32) -> i32 {
    (w * 2 / 5).clamp(22, 34)
}

/// A little figure, three cells wide and three high (half blocks): a hat
/// over a face, the scarf between coat-sleeved shoulders, the coat over
/// two boots.
fn figure(f: &mut Frame, x: i32, y: i32, look: [u32; 5], faded: bool, bg: Bg) {
    let k = if faded { 0.5 } else { 1.0 };
    let c = |v: u32, or: u32| Rgb::hex(if v == 0 { or } else { v }) * k;
    let (coat, hat, hair, skin, scarf) = (c(look[0], 0xc0392b), look[1], c(look[2], 0x3a2a1a), c(look[3], 0xe8c39e), look[4]);
    let top = if hat != 0 { Rgb::hex(hat) * k } else { hair };
    let neck = if scarf != 0 { Rgb::hex(scarf) * k } else { coat };
    let boots = Rgb::hex(0x2a1e18) * k;
    let put = |f: &mut Frame, dx: i32, dy: i32, ch: char, fg: Rgb, b: Bg| f.put_g(x + dx, y + dy, Glyph::new(ch, fg).bg(b).lit(0.0));
    // the head
    put(f, 1, 0, '▀', top, Bg::Solid(skin));
    // the scarf at the neck, the coat's sleeves either side of it
    put(f, 0, 1, '▗', coat, bg);
    put(f, 1, 1, '▀', neck, Bg::Solid(coat));
    put(f, 2, 1, '▖', coat, bg);
    // the arms hanging down, the coat over the boots
    put(f, 0, 2, '▝', coat, bg);
    put(f, 1, 2, '▀', coat, Bg::Solid(boots));
    put(f, 2, 2, '▘', coat, bg);
}

/// `text` in lines at most `w` long (at the spaces where it can).
fn wrap(text: &str, w: usize) -> Vec<String> {
    let mut out = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        let wl = word.chars().count();
        if !line.is_empty() && line.chars().count() + 1 + wl > w {
            out.push(std::mem::take(&mut line));
        }
        if wl > w {
            for ch in word.chars() {
                if line.chars().count() >= w {
                    out.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
            continue;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}
