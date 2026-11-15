//! Small DSP building blocks, all f32 at one fixed sample rate: white
//! noise, a state-variable filter, a damped "rotor" oscillator (a sine that
//! costs four multiplies per sample and can decay by itself), a
//! Freeverb-style reverb, a DC blocker and the output peak limiter.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const SR: f32 = 48_000.0;

/// xorshift white noise in -1..1.
#[derive(Clone)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Noise(crate::rng::hash(seed) | 1)
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as i32) as f32 * (1.0 / 2_147_483_648.0)
    }
}

/// Which output of the state-variable filter to use.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Lp,
    Bp,
    Hp,
}

/// Topology-preserving state-variable filter: stable at any cutoff, and its
/// band-pass output peaks at unity gain whatever the Q.
#[derive(Clone)]
pub struct Svf {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
    pub mode: Mode,
}

impl Svf {
    pub fn new(mode: Mode, hz: f32, q: f32) -> Self {
        let mut s = Svf { k: 1.0, a1: 0.0, a2: 0.0, a3: 0.0, ic1: 0.0, ic2: 0.0, mode };
        s.set(hz, q);
        s
    }
    /// Retune (costs a tan(), so do it per block, not per sample).
    pub fn set(&mut self, hz: f32, q: f32) {
        let g = (PI * hz.clamp(10.0, SR * 0.45) / SR).tan();
        self.k = 1.0 / q.max(0.05);
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }
    #[inline]
    pub fn tick(&mut self, v0: f32) -> f32 {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match self.mode {
            Mode::Lp => v2,
            Mode::Bp => v1 * self.k,
            Mode::Hp => v0 - self.k * v1 - v2,
        }
    }
}

/// One-pole smoother for parameters (ticked once per block).
#[derive(Clone, Copy)]
pub struct OnePole {
    a: f32,
    pub y: f32,
}

impl OnePole {
    /// A smoother that settles in roughly `secs` (time constant secs/3),
    /// meant to be ticked once per `block` samples.
    pub fn smoother(secs: f32, block: usize) -> Self {
        let steps = (secs * SR / block as f32 / 3.0).max(1.0);
        OnePole { a: 1.0 - (-1.0 / steps).exp(), y: 0.0 }
    }
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

/// A damped complex rotor: (re, im) is rotated by w and scaled by r every
/// sample, so `im` is amp·r^n·sin(n·w) — a sine with exponential decay for
/// four multiplies, no sin() calls. r = 1 sustains (renormalize now and
/// then; f32 rounding slowly drifts the amplitude).
#[derive(Clone, Copy, Default)]
pub struct Rotor {
    re: f32,
    im: f32,
    cr: f32,
    ci: f32,
}

impl Rotor {
    /// `tau` = seconds to fall to 1/e (≈ -8.7 dB); 0 or less sustains.
    pub fn new(amp: f32, hz: f32, tau: f32) -> Self {
        let amp = if hz < SR * 0.45 { amp } else { 0.0 };
        let mut r = Rotor { re: amp, im: 0.0, cr: 1.0, ci: 0.0 };
        r.tune(hz, tau);
        r
    }
    /// Change pitch/decay, keeping phase and current amplitude.
    pub fn tune(&mut self, hz: f32, tau: f32) {
        let w = TAU * hz.min(SR * 0.45) / SR;
        let d = if tau > 0.0 { (-1.0 / (tau * SR)).exp() } else { 1.0 };
        self.cr = d * w.cos();
        self.ci = d * w.sin();
    }
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let re = self.re * self.cr - self.im * self.ci;
        self.im = self.re * self.ci + self.im * self.cr;
        self.re = re;
        self.im
    }
    pub fn amp(&self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
    pub fn renorm(&mut self, amp: f32) {
        let a = self.amp();
        if a > 1e-12 {
            let k = amp / a;
            self.re *= k;
            self.im *= k;
        }
    }
}

/// Equal-power pan gains for p in 0 (left) .. 1 (right); centre ≈ 0.707 each.
pub fn pan(p: f32) -> (f32, f32) {
    let a = p.clamp(0.0, 1.0) * FRAC_PI_2;
    (a.cos(), a.sin())
}

pub fn db(d: f32) -> f32 {
    10f32.powf(d / 20.0)
}

pub fn midi_hz(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

// ---------------------------------------------------------------------
// Reverb: Jezar's Freeverb (8 damped combs + 4 allpasses per channel),
// delay lengths rescaled from 44.1 kHz.

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Comb { buf: vec![0.0; len], i: 0, store: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * fb;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass { buf: vec![0.0; len], i: 0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        b - x
    }
}

pub struct Reverb {
    cl: Vec<Comb>,
    cr: Vec<Comb>,
    al: Vec<Allpass>,
    ar: Vec<Allpass>,
    fb: f32,
    damp: f32,
    hp: Svf,
}

impl Reverb {
    pub fn new() -> Self {
        let s = SR / 44_100.0;
        let len = |n: usize| ((n as f32 * s) as usize).max(1);
        let combs = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let aps = [556, 441, 341, 225];
        Reverb {
            cl: combs.iter().map(|&n| Comb::new(len(n))).collect(),
            cr: combs.iter().map(|&n| Comb::new(len(n + 23))).collect(),
            al: aps.iter().map(|&n| Allpass::new(len(n))).collect(),
            ar: aps.iter().map(|&n| Allpass::new(len(n + 23))).collect(),
            fb: 0.86,
            damp: 0.4,
            // keep rumble out of the tail
            hp: Svf::new(Mode::Hp, 160.0, 0.7),
        }
    }
    /// Mono send in, stereo wet out.
    #[inline]
    pub fn tick(&mut self, x: f32) -> (f32, f32) {
        let x = self.hp.tick(x) * 0.03;
        let (mut l, mut r) = (0.0, 0.0);
        for c in &mut self.cl {
            l += c.tick(x, self.fb, self.damp);
        }
        for c in &mut self.cr {
            r += c.tick(x, self.fb, self.damp);
        }
        for a in &mut self.al {
            l = a.tick(l);
        }
        for a in &mut self.ar {
            r = a.tick(r);
        }
        (l, r)
    }
}

/// One-pole DC blocker (~10 Hz).
#[derive(Clone, Copy, Default)]
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + 0.9987 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Look-ahead peak limiter: the signal is delayed 2 ms while a held peak
/// of what's coming steers a smoothly ramped gain, so transients are
/// caught without clicks; a final clamp makes the ceiling absolute. Only a
/// safety net — at default settings the mix stays well under it.
pub struct Limiter {
    delay: Vec<[f32; 2]>,
    pos: usize,
    held: f32,
    hold: usize,
    gain: f32,
    att: f32,
    rel: f32,
    ceil: f32,
}

impl Limiter {
    pub fn new(ceiling_db: f32) -> Self {
        let la = (0.002 * SR) as usize;
        Limiter {
            delay: vec![[0.0; 2]; la],
            pos: 0,
            held: 0.0,
            hold: 0,
            gain: 1.0,
            att: 1.0 - (-4.0 / la as f32).exp(),
            rel: 1.0 - (-1.0 / (0.25 * SR)).exp(),
            ceil: db(ceiling_db),
        }
    }
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs());
        if peak >= self.held {
            self.held = peak;
            self.hold = self.delay.len();
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.held = peak.max(self.held * 0.9995);
        }
        let want = if self.held > self.ceil { self.ceil / self.held } else { 1.0 };
        let k = if want < self.gain { self.att } else { self.rel };
        self.gain += (want - self.gain) * k;
        let [dl, dr] = std::mem::replace(&mut self.delay[self.pos], [l, r]);
        self.pos = (self.pos + 1) % self.delay.len();
        let c = self.ceil;
        ((dl * self.gain).clamp(-c, c), (dr * self.gain).clamp(-c, c))
    }
}
