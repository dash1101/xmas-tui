//! Time of day. "auto" follows the real clock against the day's actual
//! sunrise and sunset (from the weather lookup, or a sensible default);
//! the other settings pin it: night, dawn, morning, noon, afternoon, dusk.
//!
//! The scene is painted for night; daylight is layered on top: the sky
//! blends toward blue (pink and orange around sunrise and sunset) with the
//! sun crossing it, the land and everything on it brightens, and stars,
//! aurora and the glow of lights fade back.

use crate::color::Rgb;
use crate::frame::Frame;

pub const MODES: [&str; 7] = ["auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"];

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Daylight {
    /// 0 night … 1 full day.
    pub sun: f32,
    /// 0 … 1: how much sunrise / sunset colour there is.
    pub warm: f32,
    /// The sun's way across the sky: 0 at sunrise, 1 at sunset.
    pub arc: f32,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `now`, `rise`, `set`: minutes since local midnight.
pub fn compute(mode: &str, now: f32, rise: f32, set: f32) -> Daylight {
    match mode {
        "night" => Daylight { sun: 0.0, warm: 0.0, arc: -1.0 },
        "dawn" => Daylight { sun: 0.45, warm: 1.0, arc: 0.03 },
        "morning" => Daylight { sun: 0.9, warm: 0.25, arc: 0.22 },
        "noon" => Daylight { sun: 1.0, warm: 0.0, arc: 0.5 },
        "afternoon" => Daylight { sun: 0.95, warm: 0.12, arc: 0.75 },
        "dusk" => Daylight { sun: 0.4, warm: 1.0, arc: 0.97 },
        _ => {
            let (r0, r1, s0, s1) = (rise - 50.0, rise + 50.0, set - 50.0, set + 60.0);
            let sun = if now < r0 || now > s1 {
                0.0
            } else if now < r1 {
                smooth((now - r0) / (r1 - r0))
            } else if now <= s0 {
                1.0
            } else {
                1.0 - smooth((now - s0) / (s1 - s0))
            };
            let warm = (1.0 - (now - rise).abs() / 70.0).max(1.0 - (now - set).abs() / 80.0).clamp(0.0, 1.0);
            Daylight { sun, warm, arc: (now - rise) / (set - rise).max(1.0) }
        }
    }
}

/// Blend the sky (pixel rows `top..horizon`, scene coordinates) toward the
/// daytime sky, and put the sun in it.
pub fn sky_pass(f: &mut Frame, d: &Daylight, top: i32, horizon: i32, glow: f32) {
    if d.sun <= 0.0 && d.warm <= 0.0 {
        return;
    }
    let w = f.pw as i32;
    let (top_c, bot_c) = (
        Rgb::hex(0x2a5aa8).lerp(Rgb::hex(0x3a3a7a), d.warm),
        Rgb::hex(0xa8cce8).lerp(Rgb::hex(0xffa070), d.warm),
    );
    let k_sun = (d.sun * 0.92 + d.warm * 0.25).min(0.95);
    // every row (the land is painted over this afterwards)
    let bottom = f.ph as i32 - 2 * f.oy;
    for y in top..bottom {
        let k = ((y - top) as f32 / (horizon - top).max(1) as f32).clamp(0.0, 1.0);
        let day = top_c.lerp(bot_c, k.powf(1.3));
        for x in 0..w {
            let c = f.px_get(x, y);
            f.px_set(x, y, c.lerp(day, k_sun));
        }
    }
    // the sun
    if (-0.06..1.06).contains(&d.arc) {
        let a = d.arc.clamp(0.0, 1.0);
        let sx = w as f32 * (0.08 + 0.84 * d.arc);
        let sy = horizon as f32 * (0.95 - 0.78 * (std::f32::consts::PI * a).sin());
        let c = Rgb::hex(0xfff4d0).lerp(Rgb::hex(0xffa050), d.warm);
        f.px_disc(sx, sy, 6.0, c * 1.2, 1.0);
        f.px_disc(sx, sy, 10.0, c, 0.25);
        f.light_px(sx, sy, c * (16.0 * glow));
    }
}
