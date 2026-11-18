//! Linking another device: a card with the link as a QR code (another
//! device's camera opens it: the web page, or the app, linked there and
//! then), the address itself, and the code to type in by hand.

use super::*;
use crate::color::pal;
use crate::frame::{Bg, Glyph};

/// The web page's address with a link code in it (what the QR code holds;
/// the page links itself when it's opened so, once asked).
pub fn link_url(code: &str) -> String {
    format!("https://xmas.novalabs.app/#link={}", crate::net::seal::pretty(code))
}

/// A link code from what was typed or pasted: the code itself, or an
/// address with one in it.
pub fn code_in(text: &str) -> Option<String> {
    let t = text.trim();
    let raw = t.rsplit("link=").next().unwrap_or(t);
    let code = crate::net::seal::norm(raw);
    (code.len() == 12).then_some(code)
}

/// The QR code's modules (dark: true), a row at a time, with its quiet
/// zone (two modules all round).
pub fn qr(text: &str) -> Option<(usize, Vec<bool>)> {
    let q = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let n = q.size() as usize;
    let m = n + 4;
    let mut out = vec![false; m * m];
    for y in 0..n {
        for x in 0..n {
            out[(y + 2) * m + x + 2] = q.get_module(x as i32, y as i32);
        }
    }
    Some((m, out))
}

impl Scene {
    /// The card for linking another device, with this link code.
    pub fn show_link_card(&mut self, code: &str) {
        self.social.link_card = Some(crate::net::seal::norm(code));
    }

    /// The card, middle of the screen: the QR code (two modules a cell,
    /// one over the other: square on the screen), the code, how.
    pub(super) fn draw_link_card(&self, f: &mut Frame) {
        let Some(code) = &self.social.link_card else {
            self.social.link_at.set(None);
            return;
        };
        let url = link_url(code);
        let Some((m, dark)) = qr(&url) else { return };
        let lines = [
            "on the other device (a phone, a laptop…):".to_string(),
            "  · scan this with its camera, or open the address,".to_string(),
            "  · or in the village there: ✉ → my devices → link this device,".to_string(),
            format!("    and type {}", crate::net::seal::pretty(code)),
            String::new(),
            "it becomes this same villager: friends, chats, progress.".to_string(),
        ];
        let text_w = lines.iter().map(|l| l.chars().count()).chain([url.chars().count()]).max().unwrap_or(40) as i32;
        let qr_rows = m.div_ceil(2) as i32;
        let w = (m as i32 + text_w + 7).min(f.w as i32 - 2);
        let h = (qr_rows + 4).max(lines.len() as i32 + 6).min(f.h as i32 - 1);
        let (x0, y0) = ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(0));
        let keep = std::mem::replace(&mut f.keep_ui, false);
        let bg = Bg::Mix(pal::UI_PANEL, 0.98);
        let border = Rgb::hex(0xffd36b) * 0.8;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
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
        f.text(x0 + 2, y0, " ⇄ link another device ", Rgb::hex(0xffd36b), bg, 0.0);
        f.text(x0 + w - 5, y0, " ✕ ", pal::UI_TITLE, Bg::Mix(pal::UI_PANEL, 0.75), 0.0);
        // the code: light round dark, two modules to a cell
        let (light, ink) = (Rgb::hex(0xf4f4f0), Rgb::hex(0x0b1020));
        let (qx, qy) = (x0 + 2, y0 + 2);
        for r in 0..qr_rows {
            for c in 0..m as i32 {
                let top = dark[(2 * r as usize) * m + c as usize];
                let bot = dark.get((2 * r as usize + 1) * m + c as usize).copied().unwrap_or(false);
                let (fg, b) = (if top { ink } else { light }, if bot { ink } else { light });
                f.put_g(qx + c, qy + r, Glyph::new('▀', fg).bg(Bg::Solid(b)).lit(0.0));
            }
        }
        let tx = qx + m as i32 + 3;
        let mut y = y0 + 2;
        f.text(tx, y, &url, Rgb::hex(0x9be08a), bg, 0.0);
        y += 2;
        for l in &lines {
            f.text(tx, y, l, pal::UI_TEXT, bg, 0.0);
            y += 1;
        }
        f.text(tx, y0 + h - 2, "Esc or ✕: done · the code's copied too", pal::UI_DIM, bg, 0.0);
        f.keep_ui = keep;
        self.social.link_at.set(Some((x0, y0, w, h)));
    }

    /// A click on the card: ✕ (or anywhere off it) puts it away. True: it
    /// was the card's.
    pub fn link_card_click(&mut self, x: i32, y: i32) -> bool {
        let Some((x0, y0, w, h)) = self.social.link_at.get() else { return false };
        if self.social.link_card.is_none() {
            return false;
        }
        let inside = x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
        if !inside || (y == y0 && x >= x0 + w - 5) {
            self.social.link_card = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_code_from_an_address_or_typed() {
        let url = link_url("abcd1234efgh");
        assert_eq!(url, "https://xmas.novalabs.app/#link=ABCD-1234-EFGH");
        assert_eq!(code_in(&url).as_deref(), Some("ABCD1234EFGH"));
        assert_eq!(code_in(" abcd-1234-efgh ").as_deref(), Some("ABCD1234EFGH"));
        assert_eq!(code_in("abcd-1234"), None);
        let (m, dark) = qr(&url).unwrap();
        assert!(m >= 25 && dark.len() == m * m);
        // (the quiet zone's light, a finder pattern's corner dark)
        assert!(!dark[0] && dark[2 * m + 2]);
    }
}
