//! The looks a widget's panel can have, like the shelves': the classic
//! panel, a frosted one with snow piled on top, a wooden frame, or a gift
//! box tied up with a gold ribbon. A widget draws itself as a classic panel
//! (a border of box-drawing lines with its title on the top edge, the panel
//! colour behind its contents); `apply` then dresses that up.

use super::Rect;
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};

pub const LOOKS: [&str; 4] = ["classic", "frosted", "wood", "gift"];

const SNOW: Rgb = Rgb::hex(0xe6edf8);
const ICE: Rgb = Rgb::hex(0xa9cdee);
const WOOD: Rgb = Rgb::hex(0x7a4a28);
const WOOD_DARK: Rgb = Rgb::hex(0x4a2a16);
const GOLD: Rgb = Rgb::hex(0xffd36b);
const GIFT: Rgb = Rgb::hex(0x9a1c28);

/// The next look along (or back), for the ◂ ▸ arrows and menus.
pub fn step(look: &str, dir: i32) -> &'static str {
    let i = LOOKS.iter().position(|l| *l == look).unwrap_or(0) as i32;
    LOOKS[(i + dir).rem_euclid(LOOKS.len() as i32) as usize]
}

fn h(x: i32, seed: u32) -> u32 {
    let mut v = (x as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2c1b_3c6d);
    v ^ (v >> 12)
}

fn border(ch: char) -> bool {
    matches!(ch, '─' | '│' | '╭' | '╮' | '╰' | '╯')
}

/// Snow piled along the row above a panel, thinning out at its ends.
pub fn snow_cap(f: &mut Frame, x0: i32, x1: i32, y: i32, seed: u32) {
    const PILE: [char; 4] = ['▁', '▂', '▃', '▄'];
    let ph = (seed % 1000) as f32 * 0.37;
    for x in x0..x1 {
        // (not over a speech bubble or anything else drawn there already)
        if f.get_g(x, y).is_some_and(|g| !matches!(g.bg, Bg::Scene) || g.ui) {
            continue;
        }
        let u = x as f32;
        let mut n = 0.55 + 0.3 * (u * 0.31 + ph).sin() + 0.15 * (u * 0.97 + ph * 2.1).sin();
        let edge = ((x - x0).min(x1 - 1 - x)) as f32;
        n *= (edge / 3.0).min(1.0);
        let k = (n * 4.0).floor().clamp(0.0, 3.0) as usize;
        f.put_g(x, y, Glyph::new(PILE[k], SNOW * (0.9 + 0.05 * (h(x, seed) % 3) as f32)).lit(0.0));
    }
}

/// Dress up the classic panel drawn at `r` (`panel` is the colour behind
/// its contents) in `look`.
pub fn apply(f: &mut Frame, r: Rect, look: &str, panel: Bg, seed: u32) {
    if r.w < 4 || r.h < 2 || !matches!(look, "frosted" | "wood" | "gift") {
        return;
    }
    // the panel's colour (in any of the shades widgets use for it)
    let panelish = |a: Bg| match (a, panel) {
        (Bg::Mix(c, _), Bg::Mix(d, _)) => c == d,
        (Bg::Solid(c), Bg::Solid(d)) => c == d,
        _ => false,
    };
    let body = match look {
        "wood" => Bg::Mix(Rgb::hex(0x24150c), 0.88),
        "gift" => Bg::Mix(GIFT * 0.55, 0.9),
        _ => Bg::Mix(Rgb::hex(0x0b1630), 0.84),
    };
    let grain = |x: i32, y: i32| if h(x * 7 + y * 131, seed) % 5 == 0 { WOOD_DARK } else { WOOD };
    let (x1, y1) = (r.x + r.w - 1, r.y + r.h - 1);
    // (restyling the panel's own cells covers nothing new: whatever was
    // under the panel was cut away when it was drawn, and a picture it
    // shows itself — a game's board — stays)
    let cover = (f.cover_pictures, f.cover_panels);
    (f.cover_pictures, f.cover_panels) = (false, false);
    for y in r.y..=y1 {
        for x in r.x..=x1 {
            let Some(g) = f.get_g(x, y) else { continue };
            if g.ui {
                continue;
            }
            let (top, bottom, side) = (y == r.y, y == y1, x == r.x || x == x1);
            let edge = top || bottom || side;
            let new = if edge && border(g.ch) {
                // the frame itself
                Some(match look {
                    "wood" => {
                        if top {
                            Glyph::new('█', grain(x, y)).bg(Bg::Solid(WOOD))
                        } else if bottom {
                            let snowy = h(x / 2 + y * 17, seed) % 4 == 0;
                            Glyph::new('▀', if snowy { SNOW * 0.9 } else { grain(x, y) }).bg(Bg::Solid(WOOD_DARK))
                        } else {
                            Glyph::new('█', grain(x, y) * 0.85).bg(body)
                        }
                    }
                    "gift" => {
                        let ch = match (x == r.x, x == x1, top, bottom) {
                            (true, _, true, _) => '╔',
                            (_, true, true, _) => '╗',
                            (true, _, _, true) => '╚',
                            (_, true, _, true) => '╝',
                            (_, _, true, _) | (_, _, _, true) => '═',
                            _ => '║',
                        };
                        // the ribbon crossing on the lid
                        let ch = if top && x == r.x + r.w / 2 { '╦' } else { ch };
                        Glyph::new(ch, GOLD).bg(if top { Bg::Solid(GIFT) } else { body })
                    }
                    _ => Glyph::new(g.ch, ICE * 0.85).bg(body),
                })
            } else if edge {
                // the title (and anything else on the frame): on the frame's colour
                let bg = match look {
                    "wood" if top => Bg::Solid(WOOD),
                    "wood" if bottom => Bg::Solid(WOOD_DARK),
                    "gift" if top => Bg::Solid(GIFT),
                    _ => body,
                };
                let fg = if look != "frosted" && g.fg == pal::UI_TITLE { Rgb::hex(0xfff1d6) } else { g.fg };
                Some(Glyph { bg, fg, ..g })
            } else if panelish(g.bg) {
                Some(Glyph { bg: body, ..g })
            } else {
                None
            };
            if let Some(n) = new {
                f.put_g(x, y, n.lit(0.0));
            }
        }
    }
    (f.cover_pictures, f.cover_panels) = cover;
    // on top: snow (frosted, wood), or the bow (gift)
    match look {
        "gift" => {
            let mid = r.x + r.w / 2;
            for (i, ch) in ['◖', '✿', '◗'].iter().enumerate() {
                let (x, y) = (mid - 1 + i as i32, r.y - 1);
                if f.get_g(x, y).is_none_or(|g| matches!(g.bg, Bg::Scene) && !g.ui) {
                    f.put_g(x, y, Glyph::new(*ch, if i == 1 { Rgb::hex(0xfff1a0) } else { GOLD }).lit(0.0));
                }
            }
        }
        _ => snow_cap(f, r.x + 1, x1, r.y - 1, seed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_step_round_and_dress_the_frame() {
        assert_eq!(step("classic", 1), "frosted");
        assert_eq!(step("classic", -1), "gift");
        assert_eq!(step("nonsense", 1), "frosted");
        let mut f = Frame::new(30, 12);
        let panel = Bg::Mix(pal::UI_PANEL, 0.8);
        let r = Rect { x: 2, y: 3, w: 20, h: 6 };
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let ch = if y == r.y || y == r.y + r.h - 1 { '─' } else if x == r.x || x == r.x + r.w - 1 { '│' } else { ' ' };
                f.put_g(x, y, Glyph::new(ch, pal::UI_BORDER).bg(panel));
            }
        }
        f.put_g(5, 3, Glyph::new('s', pal::UI_TITLE).bg(panel));
        apply(&mut f, r, "gift", panel, 1);
        assert_eq!(f.get_g(r.x, r.y).unwrap().ch, '╔');
        assert_eq!(f.get_g(r.x + 3, r.y + r.h - 1).unwrap().ch, '═');
        assert_eq!(f.get_g(r.x, r.y + 2).unwrap().ch, '║');
        assert_eq!(f.get_g(r.x + 1, r.y + 2).unwrap().ch, ' ');
        assert!(matches!(f.get_g(r.x + 1, r.y + 2).unwrap().bg, Bg::Mix(..)));
        assert_eq!(f.get_g(5, 3).unwrap().ch, 's', "the title stays");
        assert_eq!(f.get_g(r.x + r.w / 2, r.y - 1).unwrap().ch, '✿', "a bow on top");
    }

    /// A game's board is a picture on its panel: dressing the panel up
    /// afterwards mustn't cut it away (the frosted look left an empty
    /// table).
    #[test]
    fn a_panels_own_picture_survives_its_look() {
        for look in ["frosted", "wood", "gift"] {
            let mut f = Frame::new(40, 20);
            f.cover_pictures = true;
            let panel = Bg::Mix(pal::UI_PANEL, 0.95);
            let r = Rect { x: 2, y: 2, w: 30, h: 14 };
            // something of the village's under the panel (hidden by it)
            let pic = std::sync::Arc::new(crate::frame::Picture::new(2, 2, vec![255; 16]));
            f.picture(5, 5, 2, 2, pic.clone());
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    f.put_g(x, y, Glyph::new(' ', pal::UI_BORDER).bg(panel));
                }
            }
            // the board, then the look
            f.picture(10, 4, 8, 6, pic);
            apply(&mut f, r, look, panel, 1);
            assert!(!f.pictures[0].hidden.is_empty(), "{look}: what's under the panel stays hidden");
            assert!(f.pictures[1].hidden.is_empty(), "{look}: the board was cut away");
        }
    }
}
