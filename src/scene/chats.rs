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
    /// The window's own buttons: pin it (or unpin), fold it to its title
    /// bar (or open it out); a friend's pinned chat folded, or unpinned.
    Pin,
    Fold,
    FriendFold(usize),
    FriendClose(usize),
}

/// A pin changed here, for the settings to keep (see `config::PinCfg`).
#[derive(Clone, Debug, PartialEq)]
pub enum PinEdit {
    Set(crate::config::PinCfg),
    Remove(String),
}

/// A pin changed in the village, kept in the settings' pins (set: in
/// place of the one there, or added; at most 12 in all).
pub fn pin_edit(pins: &mut Vec<crate::config::PinCfg>, e: PinEdit) {
    match e {
        PinEdit::Set(p) => match pins.iter().position(|q| q.what == p.what) {
            Some(i) => pins[i] = p,
            None if pins.len() < 12 => pins.push(p),
            None => {}
        },
        PinEdit::Remove(what) => pins.retain(|q| q.what != what),
    }
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
    /// Moved somewhere (its top-left); pinned (kept open there, and kept
    /// in the settings); folded down to its title bar; picked up by its
    /// title bar (where it was taken).
    pub pos: Option<(i32, i32)>,
    pub pinned: bool,
    pub min: bool,
    grab: Option<(i32, i32)>,
    /// Friends' own chats pinned over the village (their code, where,
    /// folded), where each was last drawn, and one picked up (which,
    /// where it was taken).
    pub friend_pins: Vec<(String, (i32, i32), bool)>,
    pub(super) friend_at: RefCell<Vec<(usize, (i32, i32, i32, i32))>>,
    pub(super) friend_writes: RefCell<Vec<(i32, i32, i32, usize)>>,
    friend_grab: Option<(usize, i32, i32)>,
    /// Pins changed here, for the settings to keep.
    edits: Vec<PinEdit>,
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
        // (pinned: folded down to its title bar, not put away)
        let c = &mut self.social.chats;
        if c.pinned {
            c.min = true;
            self.chats_pin_saved();
        } else {
            c.open = false;
        }
    }

    /// Over the window, or a friend's pinned chat (clicks there are theirs).
    pub fn on_chats(&self, x: i32, y: i32) -> bool {
        let c = &self.social.chats;
        let (x0, y0, w, h) = c.at.get();
        let inside = |(x0, y0, w, h): (i32, i32, i32, i32)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
        (c.open && inside((x0, y0, w, h))) || c.friend_at.borrow().iter().any(|(_, r)| inside(*r))
    }

    /// The pins as the settings have them (each frame: cheap when nothing's
    /// changed): friends & chats pinned or not, and friends' own chats.
    pub fn set_pins(&mut self, pins: &[crate::config::PinCfg]) {
        let c = &mut self.social.chats;
        match pins.iter().find(|p| p.what == "chats") {
            Some(p) => {
                if !c.pinned || c.grab.is_none() {
                    c.pos = Some((p.x, p.y));
                }
                c.pinned = true;
                c.min = p.min;
                c.open = true;
            }
            None if c.pinned => {
                c.pinned = false;
                c.min = false;
            }
            None => {}
        }
        let fr: Vec<(String, (i32, i32), bool)> = pins.iter().filter_map(|p| p.what.strip_prefix("friend:").map(|code| (code.to_string(), (p.x, p.y), p.min))).collect();
        if c.friend_grab.is_none() && fr != c.friend_pins {
            c.friend_pins = fr;
        }
    }

    /// The pins changed here since last asked (for the settings).
    pub fn take_pin_edits(&mut self) -> Vec<PinEdit> {
        std::mem::take(&mut self.social.chats.edits)
    }

    /// Friends & chats as it is now, kept (pinned) or let go (not).
    fn chats_pin_saved(&mut self) {
        let c = &mut self.social.chats;
        let edit = if c.pinned {
            let (x, y) = c.pos.unwrap_or_else(|| (c.at.get().0, c.at.get().1));
            PinEdit::Set(crate::config::PinCfg { what: "chats".into(), x, y, min: c.min })
        } else {
            PinEdit::Remove("chats".into())
        };
        c.edits.push(edit);
    }

    /// A friend's own chat pinned over the village (friend `i`).
    pub fn pin_friend(&mut self, i: usize) {
        let Some(code) = self.online.book.friends.get(i).map(|p| crate::net::seal::norm(&p.code)) else { return };
        let c = &mut self.social.chats;
        if c.friend_pins.iter().any(|p| p.0 == code) {
            return;
        }
        let k = c.friend_pins.len() as i32;
        let at = (3 + 3 * k, 4 + 2 * k);
        c.friend_pins.push((code.clone(), at, false));
        c.edits.push(PinEdit::Set(crate::config::PinCfg { what: format!("friend:{code}"), x: at.0, y: at.1, min: false }));
    }

    /// Over the window itself (open, and drawn over friends' pinned chats).
    fn in_chats_window(&self, x: i32, y: i32) -> bool {
        let c = &self.social.chats;
        let (x0, y0, w, h) = c.at.get();
        c.open && x >= x0 && x < x0 + w && y >= y0 && y < y0 + h
    }

    /// The button at (x, y): the window's over a friend's pinned chat
    /// under it.
    fn chats_button_at(&self, x: i32, y: i32) -> Option<ChatsDo> {
        let main = self.in_chats_window(x, y);
        let friends = |d: &ChatsDo| matches!(d, ChatsDo::FriendFold(_) | ChatsDo::FriendClose(_));
        self.social.chats.buttons.borrow().iter().find(|(bx, by, bw, d)| y == *by && x >= *bx && x < bx + bw && !(main && friends(d))).map(|b| b.3)
    }

    /// Picked up by a title bar (the window's, or a friend's pinned chat):
    /// it moves with the pointer until let go.
    pub fn chats_grab(&mut self, x: i32, y: i32) -> bool {
        if self.chats_button_at(x, y).is_some() {
            return false;
        }
        let main = self.in_chats_window(x, y);
        let c = &mut self.social.chats;
        let (x0, y0, w, _) = c.at.get();
        if c.open && y == y0 && x >= x0 && x < x0 + w {
            c.grab = Some((x - x0, y - y0));
            return true;
        }
        let hit = c.friend_at.borrow().iter().rev().find(|(_, (fx, fy, fw, _))| !main && y == *fy && x >= *fx && x < fx + fw).map(|(i, (fx, fy, ..))| (*i, x - fx, y - fy));
        if let Some(g) = hit {
            c.friend_grab = Some(g);
            return true;
        }
        false
    }

    pub fn chats_dragging(&self) -> bool {
        self.social.chats.grab.is_some() || self.social.chats.friend_grab.is_some()
    }

    pub fn chats_drag_to(&mut self, x: i32, y: i32) {
        let c = &mut self.social.chats;
        if let Some((dx, dy)) = c.grab {
            c.pos = Some((x - dx, (y - dy).max(0)));
        }
        if let Some((i, dx, dy)) = c.friend_grab {
            if let Some(p) = c.friend_pins.get_mut(i) {
                p.1 = (x - dx, (y - dy).max(0));
            }
        }
    }

    /// Let go: kept where it was put (pinned ones, in the settings).
    pub fn chats_drop(&mut self) {
        let c = &mut self.social.chats;
        if c.grab.take().is_some() && c.pinned {
            self.chats_pin_saved();
            return;
        }
        if let Some((i, ..)) = c.friend_grab.take() {
            if let Some((code, (x, y), min)) = c.friend_pins.get(i).cloned() {
                c.edits.push(PinEdit::Set(crate::config::PinCfg { what: format!("friend:{code}"), x, y, min }));
            }
        }
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
        let main = self.in_chats_window(x, y);
        let c = &self.social.chats;
        if let Some(d) = self.chats_button_at(x, y) {
            let c = &mut self.social.chats;
            match d {
                // (back, when pinned: folded, kept where it is)
                ChatsDo::Menu(MORE_ONLINE) if c.pinned => {
                    c.min = true;
                    self.chats_pin_saved();
                }
                // (back, or ✕: the window's put away; pinned, ✕ unpins it)
                ChatsDo::Menu(MORE_ONLINE) | ChatsDo::Done => {
                    c.open = false;
                    if c.pinned {
                        (c.pinned, c.min) = (false, false);
                        self.chats_pin_saved();
                    }
                }
                ChatsDo::Pin => {
                    c.pinned ^= true;
                    if c.pinned && c.pos.is_none() {
                        let (x0, y0, ..) = c.at.get();
                        c.pos = Some((x0, y0));
                    }
                    c.min &= c.pinned;
                    self.chats_pin_saved();
                    return ChatsDo::Done;
                }
                ChatsDo::Fold => {
                    c.min ^= true;
                    if c.pinned {
                        self.chats_pin_saved();
                    }
                    return ChatsDo::Done;
                }
                ChatsDo::FriendFold(i) => {
                    if let Some(p) = c.friend_pins.get_mut(i) {
                        p.2 ^= true;
                        let (code, (x, y), min) = p.clone();
                        c.edits.push(PinEdit::Set(crate::config::PinCfg { what: format!("friend:{code}"), x, y, min }));
                    }
                    return ChatsDo::Done;
                }
                ChatsDo::FriendClose(i) => {
                    if i < c.friend_pins.len() {
                        let (code, ..) = c.friend_pins.remove(i);
                        c.edits.push(PinEdit::Remove(format!("friend:{code}")));
                    }
                    return ChatsDo::Done;
                }
                _ => {}
            }
            return d;
        }
        // a friend's pinned chat: its line to write on
        let fw = self.social.chats.friend_writes.borrow().iter().find(|(wx, wy, ww, _)| !main && y == *wy && x >= *wx && x < wx + ww).map(|w| w.3);
        if let Some(i) = fw {
            return ChatsDo::Net(NetDo::Mail(i as u8));
        }
        if !main {
            // (somewhere else on a friend's pinned chat)
            return ChatsDo::Done;
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
        // (a friend's pinned chat, open: read as it comes)
        let shown: Vec<usize> = self.social.chats.friend_pins.iter().filter(|p| !p.2).filter_map(|p| self.online.book.friends.iter().position(|f| crate::net::seal::norm(&f.code) == p.0)).collect();
        for i in shown {
            self.online.mark_read(i);
        }
        let c = &self.social.chats;
        if !c.open || c.min {
            return;
        }
        if let Some(i) = c.sel {
            self.online.mark_read(i);
        }
    }

    pub(super) fn draw_chats(&self, f: &mut Frame) {
        let mut buttons = vec![];
        self.draw_friend_pins(f, &mut buttons);
        self.draw_chats_window(f, &mut buttons);
        *self.social.chats.buttons.borrow_mut() = buttons;
    }

    /// Friends' own chats pinned over the village: each a little window
    /// (their name, what's new), the last lines, a line to write on.
    fn draw_friend_pins(&self, f: &mut Frame, buttons: &mut Vec<(i32, i32, i32, ChatsDo)>) {
        let c = &self.social.chats;
        let o = &self.online;
        let (mut at, mut writes) = (vec![], vec![]);
        let keep = std::mem::replace(&mut f.keep_ui, false);
        let bg = Bg::Mix(pal::UI_PANEL, 0.96);
        let border = Rgb::hex(0xffd36b) * 0.7;
        for (k, p) in c.friend_pins.iter().enumerate() {
            let (code, (px, py), min) = (&p.0, p.1, p.2);
            let Some(i) = o.book.friends.iter().position(|p| crate::net::seal::norm(&p.code) == *code) else { continue };
            let name = o.book.friends[i].name.clone();
            let (w, h) = (44.min(f.w as i32 - 2).max(20), if min { 1 } else { 12 });
            let (x0, y0) = (px.clamp(0, (f.w as i32 - w).max(0)), py.clamp(0, (f.h as i32 - h).max(0)));
            at.push((k, (x0, y0, w, h)));
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                        _ if h == 1 => '─',
                        (true, _, true, _) => '╭',
                        (_, true, true, _) => '╮',
                        (true, _, _, true) => '╰',
                        (_, true, _, true) => '╯',
                        (_, _, true, _) | (_, _, _, true) => '─',
                        (true, _, _, _) | (_, true, _, _) => '│',
                        _ => ' ',
                    };
                    f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
                }
            }
            let unread = o.unread(i);
            let title: String = format!(" ✉ {name}{} ", if unread > 0 && min { format!(" ✉{unread}") } else { String::new() }).chars().take((w - 10) as usize).collect();
            f.text(x0 + 1, y0, &title, Rgb::hex(0xffd36b), bg, 0.0);
            for (bx, label, d) in [(x0 + w - 8, if min { " ▢ " } else { " _ " }, ChatsDo::FriendFold(k)), (x0 + w - 5, " ✕ ", ChatsDo::FriendClose(k))] {
                f.text(bx, y0, label, pal::UI_TITLE, Bg::Mix(pal::UI_PANEL, 0.75), 0.0);
                buttons.push((bx, y0, 3, d));
            }
            if min {
                continue;
            }
            // the last lines, the latest at the bottom
            let cw = (w - 4) as usize;
            let mut out: Vec<(String, Rgb)> = vec![];
            for l in o.chat(i).iter().rev().take(20) {
                let mut body = wrap(&l.text, cw.saturating_sub(4).max(8));
                body.reverse();
                let who = if l.mine { "you" } else { name.as_str() };
                let col = if l.mine { pal::UI_TEXT * 0.85 } else { pal::UI_TEXT };
                let n = body.len();
                for (j, b) in body.into_iter().enumerate() {
                    out.push((if j + 1 == n { format!("{who}: {b}") } else { format!("  {b}") }, col));
                }
            }
            let room = (h - 4).max(1) as usize;
            if out.is_empty() {
                f.text(x0 + 2, y0 + 2, "nothing yet: say hello!", pal::UI_DIM, bg, 0.0);
            }
            for (j, (line, col)) in out.iter().take(room).enumerate() {
                let line: String = line.chars().take(cw).collect();
                f.text(x0 + 2, y0 + h - 3 - j as i32, &line, *col, bg, 0.0);
            }
            let wy = y0 + h - 2;
            let wbg = Bg::Mix(Rgb::hex(0x0b1020), 0.9);
            for x in x0 + 2..x0 + w - 2 {
                f.put_g(x, wy, Glyph::new(' ', pal::UI_TEXT).bg(wbg).lit(0.0));
            }
            f.text(x0 + 3, wy, "✎ write…", pal::UI_DIM, wbg, 0.0);
            writes.push((x0 + 2, wy, w - 4, i));
        }
        f.keep_ui = keep;
        *c.friend_at.borrow_mut() = at;
        *c.friend_writes.borrow_mut() = writes;
    }

    fn draw_chats_window(&self, f: &mut Frame, buttons: &mut Vec<(i32, i32, i32, ChatsDo)>) {
        let c = &self.social.chats;
        if !c.open {
            c.at.set((0, 0, 0, 0));
            return;
        }
        let (w, h0) = size(f.w as i32, f.h as i32);
        // (folded: just its title bar)
        let h = if c.min { 1 } else { h0 };
        let (x0, y0) = match c.pos {
            Some((px, py)) => (px.clamp(0, (f.w as i32 - w).max(0)), py.clamp(0, (f.h as i32 - h).max(0))),
            None => ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(1)),
        };
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
        let mut button = |f: &mut Frame, x: i32, y: i32, label: &str, d: ChatsDo, col: Rgb| -> i32 {
            let n = label.chars().count() as i32;
            f.text(x, y, label, col, Bg::Mix(pal::UI_PANEL, 0.75), 0.0);
            buttons.push((x, y, n, d));
            x + n
        };
        // the title, back, pin, fold and close
        let x = button(f, x0 + 1, y0, " ◂ back ", ChatsDo::Menu(MORE_ONLINE), pal::UI_TITLE);
        f.text(x + 1, y0, if w >= 58 { " ✉ friends & chats " } else { " ✉ chats " }, Rgb::hex(0xffd36b), bg, 0.0);
        button(f, x0 + w - 19, y0, if c.pinned { " ▣ pinned " } else { " ▣ pin    " }, ChatsDo::Pin, if c.pinned { Rgb::hex(0xffd36b) } else { pal::UI_TITLE });
        button(f, x0 + w - 8, y0, if c.min { " ▢ " } else { " _ " }, ChatsDo::Fold, pal::UI_TITLE);
        button(f, x0 + w - 5, y0, " ✕ ", ChatsDo::Done, pal::UI_TITLE);
        if c.min {
            f.keep_ui = keep;
            *c.rows.borrow_mut() = vec![];
            *c.dots.borrow_mut() = vec![];
            c.write.set(None);
            return;
        }
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
