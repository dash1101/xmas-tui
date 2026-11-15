//! Float RGB color (0..1 nominal, may exceed 1 before tone mapping).

use std::ops::{Add, AddAssign, Mul};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Rgb = Rgb { r: 1.0, g: 1.0, b: 1.0 };

    /// From a 0xRRGGBB literal.
    pub const fn hex(h: u32) -> Self {
        Rgb {
            r: ((h >> 16) & 0xff) as f32 / 255.0,
            g: ((h >> 8) & 0xff) as f32 / 255.0,
            b: (h & 0xff) as f32 / 255.0,
        }
    }

    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
        }
    }

    pub fn scale(self, k: f32) -> Rgb {
        Rgb { r: self.r * k, g: self.g * k, b: self.b * k }
    }

    /// Soft tone map: keeps hue when a channel overflows by bleeding the
    /// excess into the other channels (so hot lights go toward white).
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Rgb {
        let h = h.rem_euclid(1.0) * 6.0;
        let i = h.floor() as i32;
        let f = h - i as f32;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        let (r, g, b) = match i {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Rgb { r, g, b }
    }

    pub fn tonemap(self) -> [u8; 3] {
        let m = self.r.max(self.g).max(self.b);
        let (mut r, mut g, mut b) = (self.r, self.g, self.b);
        if m > 1.0 {
            let excess = ((m - 1.0) * 0.6).min(1.0);
            r = r / m + (1.0 - r / m) * excess;
            g = g / m + (1.0 - g / m) * excess;
            b = b / m + (1.0 - b / m) * excess;
        }
        [to8(r), to8(g), to8(b)]
    }

}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl Add for Rgb {
    type Output = Rgb;
    fn add(self, o: Rgb) -> Rgb {
        Rgb { r: self.r + o.r, g: self.g + o.g, b: self.b + o.b }
    }
}

impl AddAssign for Rgb {
    fn add_assign(&mut self, o: Rgb) {
        self.r += o.r;
        self.g += o.g;
        self.b += o.b;
    }
}

impl Mul for Rgb {
    type Output = Rgb;
    fn mul(self, o: Rgb) -> Rgb {
        Rgb { r: self.r * o.r, g: self.g * o.g, b: self.b * o.b }
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;
    fn mul(self, k: f32) -> Rgb {
        self.scale(k)
    }
}

/// The cozy palette everything draws from.
pub mod pal {
    use super::Rgb;
    pub const SKY_TOP: Rgb = Rgb::hex(0x03050f);
    pub const SKY_MID: Rgb = Rgb::hex(0x0a1030);
    pub const SKY_LOW: Rgb = Rgb::hex(0x1d1a44);
    pub const SKY_HORIZON: Rgb = Rgb::hex(0x3a2a52);
    pub const TOWN_GLOW: Rgb = Rgb::hex(0x6a3a2a);

    pub const SNOW: Rgb = Rgb::hex(0xb9c6e6);
    pub const SNOW_SHADOW: Rgb = Rgb::hex(0x5d6a93);

    pub const RED: Rgb = Rgb::hex(0xff3b3b);
    pub const GOLD: Rgb = Rgb::hex(0xffc23a);
    pub const GREEN: Rgb = Rgb::hex(0x3cff73);
    pub const BLUE: Rgb = Rgb::hex(0x4a8dff);
    pub const ORANGE: Rgb = Rgb::hex(0xff7a2a);
    pub const WARM_WHITE: Rgb = Rgb::hex(0xfff0c8);

    pub const BULBS: [Rgb; 5] = [RED, GOLD, GREEN, BLUE, ORANGE];

    pub const WINDOW: Rgb = Rgb::hex(0xffb347);

    pub const UI_BORDER: Rgb = Rgb::hex(0xc9a25a);
    pub const UI_TITLE: Rgb = Rgb::hex(0xfff0d0);
    pub const UI_TEXT: Rgb = Rgb::hex(0xe6dcc4);
    pub const UI_DIM: Rgb = Rgb::hex(0x7d7a8c);
    pub const UI_PANEL: Rgb = Rgb::hex(0x070a18);
    pub const CANDY_RED: Rgb = Rgb::hex(0xe8323c);
    pub const CANDY_WHITE: Rgb = Rgb::hex(0xf4efe6);
    pub const PINE: Rgb = Rgb::hex(0x2fae5c);
}
