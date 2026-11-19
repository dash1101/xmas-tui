//! The UI's little pictures: pixel art, 12 by 12, the same on every device
//! (drawn as real pixels where the front end can: the Windows window, the
//! web page, the phone, kitty; a plain sign in their place where it can't).
//! Fitted into a few cells: 4 by 2 is about square (a cell's about twice
//! as tall as it's wide), 2 by 1 small, 6 by 3 big.

use crate::color::Rgb;
use crate::frame::{Frame, Glyph, Picture};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Home,
    Village,
    You,
    Pets,
    Online,
    Sound,
    Effects,
    Widgets,
    Shelves,
    Windows,
    Screen,
    Upkeep,
    Friends,
    Chat,
    Todo,
    Pause,
    Play,
    Help,
    Menu,
    Centre,
    Devices,
    Moon,
    Back,
    Restart,
    Globe,
    /// A list (more to choose from).
    List,
    /// The screen's four corners, arrows out (full screen).
    Full,
}

/// The palette: a letter a colour ('.' clear).
fn colour(c: u8) -> Option<[u8; 4]> {
    let hex = |v: u32| Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
    match c {
        b'k' => hex(0x1a1730),
        b'w' => hex(0xf4f0ea),
        b'g' => hex(0xffd36b),
        b'G' => hex(0xc8902a),
        b'r' => hex(0xd8283a),
        b'R' => hex(0x8a1a26),
        b'e' => hex(0x2fae5c),
        b'E' => hex(0x1f6e3a),
        b'b' => hex(0x4a8dff),
        b'B' => hex(0x2a4c8a),
        b'n' => hex(0x9a6a3a),
        b'N' => hex(0x5a3a1e),
        b's' => hex(0xf0d0b0),
        b'p' => hex(0x9a5ad0),
        b'y' => hex(0xffe9a8),
        b'c' => hex(0x8ad0ff),
        b'o' => hex(0xff9030),
        b'h' => hex(0xb8bcc8),
        b'H' => hex(0x6a6e80),
        _ => None,
    }
}

fn art(i: Icon) -> [&'static str; 12] {
    match i {
        Icon::Home => [
            ".....kk.....", "....kRRk....", "...kRrrRk...", "..kRrrrrRk..", ".kRrrrrrrRk.", "kkkkkkkkkkkk", ".kwwwwwwwwk.", ".kwnnwwyywk.", ".kwnnwwyywk.", ".kwnnwwwwwk.", ".kwnnwwwwwk.", ".kkkkkkkkkk.",
        ],
        Icon::Village => [
            ".....gg.....", ".....ee.....", "....eEee....", "...eeeree...", "....eeEe....", "..eeEeebee..", "...eeeeee...", ".eeyeeEeree.", "..eeeeeeee..", "eeeEeeeeEeee", ".....nn.....", ".....nn.....",
        ],
        Icon::You => [
            "....rrrr....", "...rrrrrr...", "..wwwwwwww..", "...ssssss...", "...skssks...", "...ssssss...", "....ssss....", "..eeeeeeee..", ".ebbbbbbbbe.", ".bbbbbbbbbb.", ".bbbbbbbbbb.", ".bbbbbbbbbb.",
        ],
        Icon::Pets => [
            "............", "..nn....nn..", ".nnnn..nnnn.", ".nnnn..nnnn.", "..nn....nn..", "............", ".....nn.....", "...nnnnnn...", "..nnnnnnnn..", "..nnnnnnnn..", "...nn..nn...", "............",
        ],
        Icon::Online | Icon::Globe => [
            "....BBBB....", "..BBeebbBB..", ".BbeeeebbbB.", ".beeeebbeeb.", "BbbeebbbeeeB", "Bbbbbbbbeeeb", "BbbbeebbbeeB", "BbbeeeebbbbB", ".bbeeeebbbb.", ".BbbeebbbbB.", "..BBbbbbBB..", "....BBBB....",
        ],
        Icon::Sound => [
            "............", ".....k...g..", "....kk....g.", "...kwk..g..g", "kkkkwk...g.g", "kwwwwk.g.g.g", "kwwwwk.g.g.g", "kkkkwk...g.g", "...kwk..g..g", "....kk....g.", ".....k...g..", "............",
        ],
        Icon::Effects => [
            ".....c......", "..c..c..c...", "...c.c.c....", "....ccc.....", "cccccwccccc.", "....ccc.....", "...c.c.c....", "..c..c..c...", ".....c......", ".........g..", "........ggg.", ".........g..",
        ],
        Icon::Widgets => [
            "............", ".bbbbb.eeeee", ".bbwbb.eeeee", ".bbbbb.eewee", ".bbbbb.eeeee", "............", ".ggggg.rrrrr", ".ggggg.rrwrr", ".ggggg.rrrrr", ".ggggg.rrrrr", "............", "............",
        ],
        Icon::Shelves => [
            "............", ".GGGG.......", "GggggGGGGGG.", "GgggggggggG.", "GyyyyyyyyyG.", "GyyyyyyyyyG.", "GyyyyyyyyyG.", "GyyyyyyyyyG.", "GyyyyyyyyyG.", "GGGGGGGGGGG.", "............", "............",
        ],
        Icon::Windows => [
            ".wwwwwwwwww.", "wwhhhhhhhhww", "hkkkkkkkkkkh", "hkbbbbbbbbkh", "hkbbwbbbbbkh", "hkbbbbbbwbkh", "hkbwbbbbbbkh", "hkbbbbbwbbkh", "hkkkkkkkkkkh", "hhhhhhhhhhhh", "............", "............",
        ],
        Icon::Screen => [
            "kkkkkkkkkkkk", "kBBBBBBBBBBk", "kBBBBwBBBBBk", "kBBBwwwBBBBk", "kBBeewBeBBBk", "kBeeeeeeeeBk", "keeeeeeeeeek", "kkkkkkkkkkkk", ".....HH.....", ".....HH.....", "...HHHHHH...", "............",
        ],
        Icon::Upkeep => [
            "........hhh.", ".......hh.h.", ".......hh...", "......hhhh..", ".....hhh.hh.", "....hhh.....", "...hhh......", "..nnh.......", ".nnn........", "nnn.........", "nn..........", "............",
        ],
        Icon::Friends => [
            "............", "..rrr...ggg.", ".rrrrr.ggggg", ".wwwww.wwwww", ".sssss.sssss", ".sksks.sksks", ".sssss.sssss", "..sss...sss.", ".eeeee.ppppp", "bbbbbbbbbbbb", "bbbbbbbbbbbb", "bbbbbbbbbbbb",
        ],
        Icon::Chat => [
            "............", ".wwwwwwwwww.", "wwwwwwwwwwww", "wwkwwkwwkwww", "wwwwwwwwwwww", ".wwwwwwwwww.", "..ww........", ".ww.........", "w...........", "............", "............", "............",
        ],
        Icon::Todo => [
            ".....gg.....", ".....gg.....", "....gggg....", "gggggggggggg", ".gggggggggg.", "..gggggggg..", "...gggggg...", "..gggggggg..", "..ggg..ggg..", ".ggg....ggg.", ".gg......gg.", "............",
        ],
        Icon::Pause => [
            "............", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "..cccc.cccc.", "............",
        ],
        Icon::Play | Icon::Back => [
            "............", "..ee........", "..eeee......", "..eeeeee....", "..eeeeeeee..", "..eeeeeeeeee", "..eeeeeeeeee", "..eeeeeeee..", "..eeeeee....", "..eeee......", "..ee........", "............",
        ],
        Icon::Help => [
            "...bbbbbb...", "..bbbbbbbb..", "..bbb..bbb..", "......bbb...", ".....bbb....", "....bbb.....", "....bbb.....", "............", "....bbb.....", "....bbb.....", "............", "............",
        ],
        Icon::Menu => [
            ".....g......", "..g..g..g...", "...g.g.g....", "....ggg.....", "gggggyggggg.", "....ggg.....", "...g.g.g....", "..g..g..g...", ".....g......", "............", "............", "............",
        ],
        Icon::Centre => [
            ".....ee.....", ".....ee.....", "...eeeeee...", "..ee....ee..", "eee..ee..eee", "eee..ee..eee", "..ee....ee..", "...eeeeee...", ".....ee.....", ".....ee.....", "............", "............",
        ],
        Icon::Devices => [
            "kkkkkkkk....", "kBBBBBBk....", "kBBBBBBk.kkk", "kBBBBBBk.kBk", "kBBBBBBk.kBk", "kkkkkkkk.kBk", "...HH....kBk", "..HHHH...kkk", "............", "...cc..cc...", "....cccc....", "............",
        ],
        Icon::Moon => [
            "....yyyy....", "..yyyyyy....", ".yyyyy......", ".yyyy.......", "yyyy........", "yyyy........", "yyyy........", "yyyyy.......", ".yyyyy....g.", ".yyyyyyyy...", "..yyyyyyy...", "....yyyy....",
        ],
        Icon::List => [
            "............", ".gg.wwwwwwww", ".gg.wwwwwwww", "............", ".gg.wwwwwwww", ".gg.wwwwwwww", "............", ".gg.wwwwwwww", ".gg.wwwwwwww", "............", "............", "............",
        ],
        Icon::Full => [
            "wwww....wwww", "ww........ww", "w.w......w.w", "w..w....w..w", "............", "............", "............", "............", "w..w....w..w", "w.w......w.w", "ww........ww", "wwww....wwww",
        ],
        Icon::Restart => [
            "....eeee....", "..ee....ee..", ".e........e.", "e.........e.", "e........eee", "e.........e.", ".e..........", "..ee....ee..", "....eeee....", "............", "............", "............",
        ],
    }
}

/// The icon as a picture (made once).
pub fn picture(i: Icon) -> Arc<Picture> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static KEPT: Mutex<Option<HashMap<Icon, Arc<Picture>>>> = Mutex::new(None);
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let map = kept.get_or_insert_with(HashMap::new);
    if let Some(p) = map.get(&i) {
        return p.clone();
    }
    let rows = art(i);
    let mut rgba = vec![0u8; 12 * 12 * 4];
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().take(12).enumerate() {
            if let Some(px) = colour(c) {
                rgba[(y * 12 + x) * 4..][..4].copy_from_slice(&px);
            }
        }
    }
    let p = Arc::new(Picture::new(12, 12, rgba));
    map.insert(i, p.clone());
    p
}

/// A sign for the icon where pictures can't be drawn, and its colour.
pub fn sign(i: Icon) -> (char, Rgb) {
    match i {
        Icon::Home => ('⌂', Rgb::hex(0xff9a6b)),
        Icon::Village => ('❄', Rgb::hex(0xbfe4ff)),
        Icon::You => ('☃', Rgb::hex(0xf2f6ff)),
        Icon::Pets => ('♥', Rgb::hex(0xff6b8a)),
        Icon::Online | Icon::Globe => ('◉', Rgb::hex(0x9be08a)),
        Icon::Sound => ('♪', Rgb::hex(0xffd36b)),
        Icon::Effects => ('✦', Rgb::hex(0x8ad0ff)),
        Icon::Widgets => ('▦', Rgb::hex(0x8ad0ff)),
        Icon::Shelves => ('▤', Rgb::hex(0xffd36b)),
        Icon::Windows => ('▣', Rgb::hex(0xbfe4ff)),
        Icon::Screen => ('▣', Rgb::hex(0xbfe4ff)),
        Icon::Upkeep => ('⇩', Rgb::hex(0xff9a6b)),
        Icon::Friends => ('✉', Rgb::hex(0xffd36b)),
        Icon::Chat => ('☺', Rgb::hex(0xf4f0ea)),
        Icon::Todo => ('✦', Rgb::hex(0xffd36b)),
        Icon::Pause => ('⏸', Rgb::hex(0xbfe4ff)),
        Icon::Play | Icon::Back => ('▶', Rgb::hex(0x9be08a)),
        Icon::Help => ('?', Rgb::hex(0x8ad0ff)),
        Icon::Menu => ('❄', Rgb::hex(0xffd36b)),
        Icon::Centre => ('⌖', Rgb::hex(0x9be08a)),
        Icon::Devices => ('⇄', Rgb::hex(0x8ad0ff)),
        Icon::Moon => ('☾', Rgb::hex(0xffe9a8)),
        Icon::Restart => ('↻', Rgb::hex(0x9be08a)),
        Icon::List => ('☰', Rgb::hex(0xf4f0ea)),
        Icon::Full => ('⤢', Rgb::hex(0xf4f0ea)),
    }
}

/// The icon in the cells at (x, y), `cols` by `rows` (its sign in the
/// middle where pictures can't be drawn), over the background `bg`.
pub fn put(f: &mut Frame, x: i32, y: i32, cols: i32, rows: i32, i: Icon, bg: crate::frame::Bg) {
    for yy in y..y + rows {
        for xx in x..x + cols {
            f.put_g(xx, yy, Glyph::new(' ', Rgb::BLACK).bg(bg).lit(0.0));
        }
    }
    if crate::frame::pictures_ok() {
        f.picture(x, y, cols, rows, picture(i));
    } else {
        let (ch, c) = sign(i);
        f.put_g(x + (cols - 1) / 2, y + (rows - 1) / 2, Glyph::new(ch, c).bg(bg).lit(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon's art is 12 by 12, in the palette's letters.
    #[test]
    fn every_icon_is_twelve_by_twelve() {
        use Icon::*;
        for i in [Home, Village, You, Pets, Online, Sound, Effects, Widgets, Shelves, Windows, Screen, Upkeep, Friends, Chat, Todo, Pause, Play, Help, Menu, Centre, Devices, Moon, Back, Restart, Globe, List, Full] {
            for (y, row) in art(i).iter().enumerate() {
                let n = row.chars().count();
                assert_eq!(n, 12, "{i:?} row {y}: {row:?}");
                for c in row.bytes() {
                    assert!(c == b'.' || colour(c).is_some(), "{i:?}: {:?}", c as char);
                }
            }
            assert!(picture(i).rgba.iter().skip(3).step_by(4).any(|&a| a > 0), "{i:?} is blank");
        }
    }
}
