//! The right-click menu: what can be done with whatever's under the
//! pointer (and the settings, always at the bottom).

use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::scene::Do;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuAct {
    Scene(Do),
    Shelf(crate::widgets::shelf::MenuAct),
    /// Windows desktop: show Windows' own desktop menu instead.
    Desktop,
    Settings,
    /// Widget i: its next look; hide it; (un)lock the widgets.
    Look(u8),
    Hide(u8),
    LockWidgets,
}

pub struct CtxMenu {
    pub x: i32,
    pub y: i32,
    pub title: String,
    pub items: Vec<(String, MenuAct)>,
    pub hover: Option<usize>,
}

impl CtxMenu {
    /// A menu at (x, y), kept on a `w`×`h` screen.
    pub fn new(x: i32, y: i32, title: String, items: Vec<(String, Do)>, w: i32, h: i32) -> Self {
        let items: Vec<(String, MenuAct)> = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        Self::with(x, y, title, items, w, h)
    }

    /// A menu of any actions (the settings are added at the bottom).
    pub fn with(x: i32, y: i32, title: String, mut items: Vec<(String, MenuAct)>, w: i32, h: i32) -> Self {
        items.push(("settings…".into(), MenuAct::Settings));
        let mut m = CtxMenu { x, y, title, items, hover: None };
        let (mw, mh) = m.size();
        m.x = x.min(w - mw - 1).max(0);
        m.y = (y + 1).min(h - mh - 1).max(0);
        m
    }

    /// Keep clear of the open windows (x0, y0, x1, y1 in cells): the menu
    /// is drawn on the wallpaper, under them. Below the click if that's
    /// clear, else above it, else to the left; failing all of those, where
    /// the least of it is hidden.
    pub fn avoid(&mut self, wins: &[(i32, i32, i32, i32)], click: (i32, i32), w: i32, h: i32) {
        if wins.is_empty() {
            return;
        }
        let (mw, mh) = self.size();
        let (cx, cy) = click;
        let hidden = |x: i32, y: i32| -> i32 {
            wins.iter()
                .map(|&(x0, y0, x1, y1)| {
                    let ow = (x + mw).min(x1) - x.max(x0);
                    let oh = (y + mh).min(y1) - y.max(y0);
                    ow.max(0) * oh.max(0)
                })
                .sum()
        };
        let fit = |x: i32, y: i32| (x.min(w - mw - 1).max(0), y.min(h - mh - 1).max(0));
        let spots = [fit(cx, cy + 1), fit(cx, cy - mh), fit(cx - mw, cy + 1), fit(cx - mw, cy - mh)];
        let best = spots.iter().copied().min_by_key(|&(x, y)| hidden(x, y)).unwrap_or((self.x, self.y));
        if hidden(self.x, self.y) > hidden(best.0, best.1) {
            (self.x, self.y) = best;
        }
    }

    fn size(&self) -> (i32, i32) {
        let w = self.items.iter().map(|i| i.0.chars().count()).chain(std::iter::once(self.title.chars().count() + 2)).max().unwrap_or(10) as i32 + 6;
        (w, self.items.len() as i32 + 2)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.size();
        x >= self.x && x < self.x + w && y >= self.y && y < self.y + h
    }

    /// The item at (x, y).
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        let (w, _) = self.size();
        let i = y - self.y - 1;
        (x > self.x && x < self.x + w - 1 && i >= 0 && (i as usize) < self.items.len()).then_some(i as usize)
    }

    pub fn draw(&self, f: &mut Frame) {
        let (w, h) = self.size();
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
        let border = Rgb::hex(0xffd36b) * 0.8;
        for y in self.y..self.y + h {
            for x in self.x..self.x + w {
                let ch = match (x == self.x, x == self.x + w - 1, y == self.y, y == self.y + h - 1) {
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
        f.text(self.x + 2, self.y, &format!(" {} ", self.title), pal::UI_TITLE, bg, 0.0);
        for (i, (label, act)) in self.items.iter().enumerate() {
            let y = self.y + 1 + i as i32;
            let hot = self.hover == Some(i);
            let ibg = if hot { Bg::Mix(Rgb::hex(0x34406a), 0.95) } else { bg };
            for x in self.x + 1..self.x + w - 1 {
                f.put_g(x, y, Glyph::new(' ', border).bg(ibg).lit(0.0));
            }
            let fg = match act {
                MenuAct::Settings | MenuAct::Desktop => pal::UI_DIM,
                _ if hot => Rgb::hex(0xffd36b),
                _ => pal::UI_TEXT,
            };
            f.text(self.x + 3, y, label, fg, ibg, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_keeps_out_from_under_a_window() {
        let items = vec![("drop in up here".to_string(), MenuAct::Settings); 3];
        // clicked just above a window's top edge: it opens above instead
        let mut m = CtxMenu::with(50, 20, "the sky".into(), items.clone(), 200, 60);
        m.avoid(&[(10, 21, 150, 50)], (50, 20), 200, 60);
        let (_, mh) = m.size();
        assert!(m.y + mh <= 21, "{} + {mh}", m.y);
        // nothing in the way: where it was
        let mut m = CtxMenu::with(50, 20, "the sky".into(), items, 200, 60);
        m.avoid(&[(100, 0, 150, 10)], (50, 20), 200, 60);
        assert_eq!((m.x, m.y), (50, 21));
    }
}
