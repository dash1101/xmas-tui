//! A line typed in the village itself, the same on every device: the chat
//! box (bottom left, as in a game's chat: what's been said lately, over the
//! line being typed; open with /), a friend's chat (on its own line in
//! friends & chats), or a short field (a code, a name: a little box at the
//! bottom). The desktop types into it straight; a phone or a browser
//! through its own keyboard, kept to the line (`set_typed`), with the line
//! drawn above that keyboard (`set_inset`).

use super::online::TextFor;
use super::*;
use crate::color::pal;
use crate::frame::{Bg, Glyph};

/// A line being typed (see the module).
#[derive(Clone, Debug)]
pub struct Typing {
    pub what: TextFor,
    /// What it's for, in a few words (the box's title; the chat box's hint).
    pub ask: String,
    pub text: String,
    /// Where the caret is (characters in).
    pub caret: usize,
    /// A code (capitals and dashes as it's typed; `true`: a name may follow
    /// it after a space).
    code: Option<bool>,
    /// Which line this is (each opened, a new number), and how many times
    /// it's been changed here, not by the front end's own box (which takes
    /// it back then).
    pub id: u32,
    pub rev: u32,
    /// Up through what was said before (the chat box): how far, and what
    /// was being typed before.
    back: Option<(usize, String)>,
}

/// A key while typing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypeKey {
    Char(char),
    Back,
    Del,
    Left,
    Right,
    Home,
    End,
    Enter,
    Esc,
    Up,
    Down,
}

/// The most that can be typed.
const MOST: usize = 280;

/// How long a line said shows in the chat box with it closed (seconds).
pub const CHAT_FADE: i64 = 12;

impl Typing {
    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, c: usize) -> usize {
        self.text.char_indices().nth(c).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    fn insert(&mut self, s: &str) {
        for ch in s.chars().filter(|c| !c.is_control()) {
            if self.chars() >= MOST {
                break;
            }
            let b = self.byte(self.caret);
            self.text.insert(b, ch);
            self.caret += 1;
        }
        self.tidy();
    }

    /// A code: capitals and a dash between each four (the caret kept after
    /// the same letters).
    fn tidy(&mut self) {
        let Some(more) = self.code else { return };
        let before: usize = self.text.chars().take(self.caret).filter(|c| c.is_ascii_alphanumeric() || *c == ' ').count();
        let t = crate::net::seal::typing_code(&self.text, more);
        if t != self.text {
            self.text = t;
            let mut seen = 0;
            let mut caret = self.text.chars().count();
            for (i, c) in self.text.chars().enumerate() {
                if seen == before {
                    caret = i;
                    break;
                }
                if c.is_ascii_alphanumeric() || c == ' ' {
                    seen += 1;
                }
            }
            self.caret = caret;
        }
        self.caret = self.caret.min(self.chars());
    }

    /// Talk: said to everyone, a whisper, in a group (the chat box).
    pub fn is_talk(&self) -> bool {
        matches!(self.what, TextFor::Chat | TextFor::Say | TextFor::Whisper(_) | TextFor::GroupSay(_))
    }

    /// A code being typed (the front end's keyboard in capitals).
    pub fn is_code(&self) -> bool {
        self.code.is_some()
    }
}

impl Scene {
    /// Something to type (see `TextFor`): the line opens, empty.
    pub(super) fn open_typing(&mut self, what: TextFor, ask: String) {
        let code = crate::net::seal::asks_code(&ask);
        self.typing_n = self.typing_n.wrapping_add(1);
        self.typing = Some(Typing { what, ask, text: String::new(), caret: 0, code, id: self.typing_n, rev: 0, back: None });
    }

    /// The line being typed, if one is.
    pub fn typing(&self) -> Option<&Typing> {
        self.typing.as_ref()
    }

    /// A key while typing (false: not typing, the key's someone else's).
    pub fn type_key(&mut self, k: TypeKey) -> bool {
        let Some(ty) = self.typing.as_mut() else { return false };
        match k {
            TypeKey::Char(c) => {
                ty.insert(&c.to_string());
                ty.back = None;
            }
            TypeKey::Back => {
                if ty.caret > 0 {
                    let b = ty.byte(ty.caret - 1);
                    ty.text.remove(b);
                    ty.caret -= 1;
                    ty.tidy();
                }
            }
            TypeKey::Del => {
                if ty.caret < ty.chars() {
                    let b = ty.byte(ty.caret);
                    ty.text.remove(b);
                    ty.tidy();
                }
            }
            TypeKey::Left => ty.caret = ty.caret.saturating_sub(1),
            TypeKey::Right => ty.caret = (ty.caret + 1).min(ty.chars()),
            TypeKey::Home => ty.caret = 0,
            TypeKey::End => ty.caret = ty.chars(),
            TypeKey::Up | TypeKey::Down if ty.is_talk() => {
                // (back through what was said before, and forward again)
                let said = &self.said_before;
                let up = k == TypeKey::Up;
                let (i, kept) = match ty.back.take() {
                    None if up && !said.is_empty() => (Some(said.len() - 1), ty.text.clone()),
                    None => (None, ty.text.clone()),
                    Some((i, kept)) if up => (Some(i.saturating_sub(1)), kept),
                    Some((i, kept)) if i + 1 < said.len() => (Some(i + 1), kept),
                    Some((_, kept)) => {
                        ty.text = kept;
                        ty.caret = ty.chars();
                        ty.rev += 1;
                        return true;
                    }
                };
                if let Some(i) = i {
                    ty.text = said[i].clone();
                    ty.caret = ty.chars();
                    ty.back = Some((i, kept));
                    ty.rev += 1;
                }
                return true;
            }
            TypeKey::Up | TypeKey::Down => {}
            TypeKey::Enter => {
                let ty = self.typing.take().unwrap();
                let text = ty.text.trim().to_string();
                if ty.is_talk() && !text.is_empty() {
                    self.said_before.retain(|s| *s != text);
                    self.said_before.push(text.clone());
                    if self.said_before.len() > 30 {
                        self.said_before.remove(0);
                    }
                }
                self.give_text(ty.what, &text);
                return true;
            }
            TypeKey::Esc => {
                self.typing = None;
                return true;
            }
        }
        if let Some(ty) = self.typing.as_mut() {
            if matches!(k, TypeKey::Char(_) | TypeKey::Back | TypeKey::Del) && ty.code.is_some() {
                ty.rev += 1;
            }
        }
        true
    }

    /// Text pasted (or put in at once) at the caret.
    pub fn type_text(&mut self, s: &str) {
        if let Some(ty) = self.typing.as_mut() {
            let s: String = s.chars().take(MOST).collect();
            ty.insert(&s);
            ty.back = None;
            ty.rev += 1;
        }
    }

    /// The front end's own box, as it stands now (a phone's or a browser's
    /// keyboard types there): the line here kept to it.
    pub fn set_typed(&mut self, s: &str, caret: Option<usize>) {
        if let Some(ty) = self.typing.as_mut() {
            let s: String = s.chars().filter(|c| !c.is_control()).take(MOST).collect();
            let n = s.chars().count();
            ty.caret = caret.unwrap_or(n).min(n);
            if s != ty.text {
                ty.text = s;
                ty.caret = caret.unwrap_or(n).min(n);
                ty.back = None;
                let was = ty.text.clone();
                ty.tidy();
                if ty.text != was {
                    ty.rev += 1;
                }
            }
        }
    }

    /// The bottom rows a phone's keyboard covers (what's typed is drawn
    /// above them).
    pub fn set_inset(&mut self, rows: i32) {
        self.inset = rows.max(0);
    }

    pub fn inset(&self) -> i32 {
        self.inset
    }

    /// The chat box's lines: what's been said (and whispered, mailed…),
    /// the latest last; with it closed, only the last few moments'.
    fn chat_lines(&self, open: bool, n: usize) -> Vec<(String, Rgb)> {
        let now = crate::clock::now().timestamp();
        let mut out = vec![];
        for l in self.social.log.iter().rev() {
            if !open && now - l.at > CHAT_FADE {
                break;
            }
            // (closed: just what's said here, in groups, to one person, and
            // the host's word; mail and the rest are in the post)
            let talk = l.text.starts_with('☺') || l.text.starts_with('#') || l.text.contains("(just ") || l.text.starts_with("✦ (the host)");
            if !open && !talk {
                continue;
            }
            let fade = if open { 1.0 } else { (1.0 - ((now - l.at) as f32 - (CHAT_FADE as f32 - 3.0)).max(0.0) / 3.0).clamp(0.2, 1.0) };
            let col = if l.text.starts_with("☺ you") || l.text.starts_with("✉ to") || l.text.contains(" you: ") {
                pal::UI_TEXT * 0.8
            } else if l.text.starts_with('✉') || l.text.starts_with('♪') {
                Rgb::hex(0xffd36b)
            } else if l.text.starts_with('#') {
                Rgb::hex(0x8ac6ff)
            } else if l.text.starts_with('✦') {
                Rgb::hex(0xffe9a8)
            } else {
                pal::UI_TEXT
            };
            out.push((l.text.clone(), col * fade));
            if out.len() >= n {
                break;
            }
        }
        out.reverse();
        out
    }

    /// The line being typed (and, typing talk, the chat box round it; a
    /// field: its little box), over everything, above a phone's keyboard.
    pub(super) fn draw_typing(&self, f: &mut Frame) {
        let Some(ty) = &self.typing else { return };
        // (a friend's chat in friends & chats: typed on its own line there)
        if let TextFor::Mail(i) = ty.what {
            if self.chats_shown() && self.social.chats.sel == Some(i as usize) && self.social.chats.write.get().is_some() {
                return;
            }
            if self.social.chats.friend_writes.borrow().iter().any(|w| w.3 == i as usize) {
                return;
            }
        }
        let blink = (self.now() * 2.0) as i32 % 2 == 0;
        let (w, h) = (f.w as i32, f.h as i32);
        let bottom = (h - 1 - self.inset).max(3);
        let keep = std::mem::replace(&mut f.keep_ui, false);
        if ty.is_talk() || matches!(ty.what, TextFor::Mail(_)) {
            // the chat box: bottom left, the lines over the line typed
            let bw = (w * 11 / 20).clamp(30.min(w - 2), 96.min(w - 2));
            let x0 = 1;
            let room = (bottom - 4).clamp(1, 14) as usize;
            let lines = if matches!(ty.what, TextFor::Mail(_)) { self.mail_lines(ty, room, (bw - 4) as usize) } else { self.wrapped(self.chat_lines(true, room * 2), (bw - 4) as usize, room) };
            let n = lines.len() as i32;
            let top = bottom - 2 - n;
            let bg = Bg::Mix(Rgb::hex(0x05070f), 0.72);
            for y in top..=bottom {
                for x in x0..x0 + bw {
                    f.put_g(x, y, Glyph::new(' ', pal::UI_TEXT).bg(bg).lit(0.0));
                }
            }
            let title = match ty.what {
                TextFor::Mail(i) => format!(" ✉ {} ", self.online.book.friends.get(i as usize).map(|p| p.name.as_str()).unwrap_or("a friend")),
                TextFor::Whisper(id) => format!(" ✉ just to {} ", self.other_name(id)),
                TextFor::GroupSay(i) => format!(" # {} ", self.online.book.groups.get(i as usize).map(|g| g.name.as_str()).unwrap_or("group")),
                _ if self.online.village().is_some() => " ☺ village chat ".to_string(),
                _ => " ☺ chat ".to_string(),
            };
            f.text(x0 + 1, top, &title, Rgb::hex(0xffd36b), Bg::Mix(Rgb::hex(0x05070f), 0.9), 0.0);
            if lines.is_empty() {
                f.text(x0 + 2, top + 1, "(nothing said yet)", pal::UI_DIM, bg, 0.0);
            }
            for (k, (s, col)) in lines.iter().enumerate() {
                f.text(x0 + 2, top + 1 + k as i32, s, *col, bg, 0.0);
            }
            // the line typed
            let ly = bottom - 1;
            let lbg = Bg::Mix(Rgb::hex(0x1a2340), 0.95);
            for x in x0..x0 + bw {
                f.put_g(x, ly, Glyph::new(' ', pal::UI_TEXT).bg(lbg).lit(0.0));
            }
            self.draw_field(f, x0 + 1, ly, bw - 2, ty, "› ", blink, lbg);
            let hint = match ty.what {
                TextFor::Chat if self.online.village().is_some() => "Enter: say it · Esc: close · ↑ said before · @name: just to them",
                TextFor::Chat => "@name: to a friend · @group: to a group · Esc: close",
                _ => "Enter: send · Esc: close",
            };
            let hint: String = hint.chars().take((bw - 2) as usize).collect();
            f.text(x0 + 1, bottom, &hint, pal::UI_DIM, bg, 0.0);
            self.social.chat_box.set(Some((x0, top, bw, bottom - top + 1)));
        } else {
            // a field: a little box, bottom middle
            let bw = (ty.ask.chars().count() as i32 + 6).clamp(36.min(w - 2), 72.min(w - 2));
            let x0 = (w - bw) / 2;
            let top = bottom - 3;
            let bg = Bg::Mix(pal::UI_PANEL, 0.97);
            let border = Rgb::hex(0xffd36b) * 0.8;
            for y in top..=bottom {
                for x in x0..x0 + bw {
                    let ch = match (x == x0, x == x0 + bw - 1, y == top, y == bottom) {
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
            let ask: String = format!(" {} ", ty.ask).chars().take((bw - 4) as usize).collect();
            f.text(x0 + 2, top, &ask, pal::UI_TITLE, bg, 0.0);
            let lbg = Bg::Mix(Rgb::hex(0x0b1020), 0.95);
            for x in x0 + 2..x0 + bw - 2 {
                f.put_g(x, top + 1, Glyph::new(' ', pal::UI_TEXT).bg(lbg).lit(0.0));
            }
            self.draw_field(f, x0 + 2, top + 1, bw - 4, ty, "", blink, lbg);
            f.text(x0 + 2, top + 2, "Enter: OK · Esc: never mind · Ctrl+V: paste", pal::UI_DIM, bg, 0.0);
            self.social.chat_box.set(Some((x0, top, bw, 4)));
        }
        f.keep_ui = keep;
    }

    /// The text typed and its caret, along a line `w` wide at (x, y) (the
    /// caret kept in sight).
    pub(super) fn draw_field(&self, f: &mut Frame, x: i32, y: i32, w: i32, ty: &Typing, lead: &str, blink: bool, bg: Bg) {
        let lead_n = lead.chars().count() as i32;
        let room = (w - lead_n - 1).max(4) as usize;
        let chars: Vec<char> = ty.text.chars().collect();
        let from = ty.caret.saturating_sub(room.saturating_sub(1));
        let shown: String = chars.iter().skip(from).take(room).collect();
        let x1 = f.text(x, y, lead, Rgb::hex(0xffd36b), bg, 0.0);
        if ty.text.is_empty() && !lead.is_empty() {
            f.text(x1 + 1, y, "…", pal::UI_DIM, bg, 0.0);
        }
        f.text(x1, y, &shown, pal::UI_TEXT, bg, 0.0);
        if blink {
            let cx = x1 + (ty.caret - from) as i32;
            let under = chars.get(ty.caret).copied().unwrap_or(' ');
            f.put_g(cx, y, Glyph::new(under, Rgb::hex(0x0b1020)).bg(Bg::Solid(Rgb::hex(0xffd36b))).lit(0.0));
        }
    }

    /// A friend's chat as lines for the chat box.
    fn mail_lines(&self, ty: &Typing, room: usize, w: usize) -> Vec<(String, Rgb)> {
        let TextFor::Mail(i) = ty.what else { return vec![] };
        let name = self.online.book.friends.get(i as usize).map(|p| p.name.clone()).unwrap_or_default();
        let lines: Vec<(String, Rgb)> = self
            .online
            .chat(i as usize)
            .iter()
            .rev()
            .take(room * 2)
            .rev()
            .map(|l| {
                let who = if l.mine { "you" } else { name.as_str() };
                let mark = chat_mark(l);
                (format!("{who}: {}{mark}", l.text), if l.mine { pal::UI_TEXT * 0.8 } else { pal::UI_TEXT })
            })
            .collect();
        self.wrapped(lines, w, room)
    }

    /// Lines wrapped to `w`, the last `room` of them.
    fn wrapped(&self, lines: Vec<(String, Rgb)>, w: usize, room: usize) -> Vec<(String, Rgb)> {
        let mut out = vec![];
        for (s, col) in lines {
            for (k, part) in super::chats::wrap(&s, w.max(8)).into_iter().enumerate() {
                out.push((if k == 0 { part } else { format!("  {part}") }, col));
            }
        }
        let skip = out.len().saturating_sub(room);
        out.into_iter().skip(skip).collect()
    }

    /// The chat box closed: the last few moments' lines, bottom left (top
    /// left on a phone), fading.
    pub(super) fn draw_chat_feed(&self, f: &mut Frame) {
        if self.typing.as_ref().is_some_and(|t| t.is_talk() || matches!(t.what, TextFor::Mail(_))) {
            return;
        }
        let lines = self.chat_lines(false, 6);
        if lines.is_empty() {
            return;
        }
        let w = (f.w as i32 * 11 / 20).clamp(30.min(f.w as i32 - 2), 96.min(f.w as i32 - 2));
        let lines = self.wrapped(lines, (w - 3) as usize, 6);
        let n = lines.len() as i32;
        // (over the post: below it on a phone, where the post's at the top)
        let post = self.social.post_at.get();
        let top = if self.social.post_top {
            post.map(|p| p.1 + p.3 + 1).unwrap_or(2)
        } else {
            post.map(|p| p.1 - 1 - n).unwrap_or(f.h as i32 - 3 - n - self.inset).max(1)
        };
        let keep = std::mem::replace(&mut f.keep_ui, false);
        for (k, (s, col)) in lines.iter().enumerate() {
            let a = (col.r + col.g + col.b) / 3.0;
            let bg = Bg::Mix(Rgb::hex(0x05070f), (0.6 * a / 0.8).clamp(0.15, 0.6));
            f.text(1, top + k as i32, &format!(" {s} "), *col, bg, 0.0);
        }
        f.keep_ui = keep;
    }
}

/// How a line of one's own is getting on: on its way, never got there.
pub fn chat_mark(l: &crate::net::ChatLine) -> &'static str {
    match l.state {
        crate::net::sent::WAITING if l.mine => "  ⧗ waiting to send",
        crate::net::sent::LOST if l.mine => "  ✗ not sent",
        _ => "",
    }
}
