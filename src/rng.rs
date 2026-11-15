//! Small deterministic RNG, hashing and value noise (no external deps).

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

pub fn hash2(a: i32, b: i32) -> u32 {
    hash((a as u32).wrapping_mul(0x9E37_79B1) ^ hash(b as u32 ^ 0x5bd1_e995))
}

pub fn hash3(a: i32, b: i32, c: i32) -> u32 {
    hash(hash2(a, b) ^ (c as u32).wrapping_mul(0x27d4_eb2d))
}

/// Uniform 0..1 from a hash.
pub fn h01(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    pub fn f(&mut self) -> f32 {
        h01(self.next_u32())
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn int(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            return a;
        }
        a + (self.next_u32() % (b - a) as u32) as i32
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next_u32() as usize) % xs.len()]
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 1D value noise in 0..1.
pub fn noise1(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = h01(hash2(i as i32, seed as i32));
    let b = h01(hash2(i as i32 + 1, seed as i32));
    a + (b - a) * smooth(f)
}

/// 2D value noise in 0..1.
pub fn noise2(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(x - ix), smooth(y - iy));
    let (ix, iy) = (ix as i32, iy as i32);
    let s = seed as i32;
    let a = h01(hash3(ix, iy, s));
    let b = h01(hash3(ix + 1, iy, s));
    let c = h01(hash3(ix, iy + 1, s));
    let d = h01(hash3(ix + 1, iy + 1, s));
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fy
}

/// Fractal 1D noise, 0..1.
pub fn fbm1(x: f32, oct: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..oct {
        sum += noise1(x * freq, seed.wrapping_add(o * 101)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

pub fn fbm2(x: f32, y: f32, oct: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..oct {
        sum += noise2(x * freq, y * freq, seed.wrapping_add(o * 131)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.07;
    }
    sum / norm
}
