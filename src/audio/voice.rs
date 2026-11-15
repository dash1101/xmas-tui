//! One sounding note. Every sound the mixer plays — a music-box tine, a
//! sleigh bell, a firework crackle — is a Voice: up to 12 damped sine
//! partials plus an optional filtered-noise layer, under one envelope, with
//! optional pitch glide and vibrato. Built with a small builder API.

use super::dsp::{pan, Mode, Noise, Rotor, Svf, SR};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    Sfx,
    Music,
    Amb,
}

const MAX_PARTS: usize = 12;
/// Modulation (glide, vibrato, filter sweeps) is updated this often.
const MOD_EVERY: u32 = 32;

/// Filtered noise: continuous hiss, or sparse random impulses ("grain" =
/// impulse probability per sample) for crackles and crunches.
#[derive(Clone)]
pub struct NoiseLayer {
    src: Noise,
    filt: Svf,
    amp: f32,
    level: f32,
    decay: f32,
    attack: u32,
    grain: f32,
    sweep: Option<(f32, f32, f32)>,
    q: f32,
}

impl NoiseLayer {
    /// `tau` = decay time constant in seconds (0 = sustain).
    pub fn new(seed: u32, mode: Mode, hz: f32, q: f32, amp: f32, tau: f32) -> Self {
        NoiseLayer {
            src: Noise::new(seed),
            filt: Svf::new(mode, hz, q),
            amp,
            level: 0.0,
            decay: if tau > 0.0 { (-1.0 / (tau * SR)).exp() } else { 1.0 },
            attack: 0,
            grain: 0.0,
            sweep: None,
            q,
        }
    }
    pub fn attack(mut self, secs: f32) -> Self {
        self.attack = (secs * SR) as u32;
        self
    }
    /// Sparse impulses instead of continuous noise.
    pub fn grains(mut self, per_sec: f32) -> Self {
        self.grain = (per_sec / SR).clamp(0.0, 1.0);
        self
    }
    /// Cutoff glides exponentially from `hz` to `to` over `secs`.
    pub fn sweep(mut self, from: f32, to: f32, secs: f32) -> Self {
        self.sweep = Some((from, to, secs * SR));
        self
    }
    #[inline]
    fn tick(&mut self, age: u32) -> f32 {
        if age < self.attack {
            self.level = self.amp * age as f32 / self.attack as f32;
        } else if age == self.attack {
            self.level = self.amp;
        } else {
            self.level *= self.decay;
        }
        let w = self.src.next();
        let x = if self.grain > 0.0 {
            if w.abs() < self.grain {
                w / self.grain
            } else {
                0.0
            }
        } else {
            w
        };
        self.filt.tick(x) * self.level
    }
    fn modulate(&mut self, age: u32) {
        if let Some((a, b, len)) = self.sweep {
            let u = (age as f32 / len).min(1.0);
            self.filt.set(a * (b / a).powf(u), self.q);
        }
    }
}

#[derive(Clone)]
pub struct Voice {
    pub bus: Bus,
    /// Music pieces fade as a group when they cross-fade (0 = no group).
    pub group: u8,
    delay: u32,
    age: u32,
    len: u32,
    attack: u32,
    release_at: u32,
    release: u32,
    gain: f32,
    pan: f32,
    send: f32,
    parts: [Rotor; MAX_PARTS],
    hz: [f32; MAX_PARTS],
    tau: [f32; MAX_PARTS],
    amp: [f32; MAX_PARTS],
    np: usize,
    glide: (f32, f32),
    vib: (f32, f32, f32),
    noise: Option<NoiseLayer>,
    sustained: bool,
}

impl Voice {
    pub fn new(bus: Bus) -> Self {
        Voice {
            bus,
            group: 0,
            delay: 0,
            age: 0,
            len: 0,
            attack: (0.002 * SR) as u32,
            release_at: u32::MAX,
            release: (0.05 * SR) as u32,
            gain: 1.0,
            pan: 0.5,
            send: 0.0,
            parts: [Rotor::default(); MAX_PARTS],
            hz: [0.0; MAX_PARTS],
            tau: [0.0; MAX_PARTS],
            amp: [0.0; MAX_PARTS],
            np: 0,
            glide: (1.0, 1.0),
            vib: (0.0, 0.0, 0.0),
            noise: None,
            sustained: false,
        }
    }

    /// Add a sine partial; `tau` = decay time constant (0 = sustain until
    /// the envelope releases it).
    pub fn part(mut self, hz: f32, amp: f32, tau: f32) -> Self {
        if self.np < MAX_PARTS && hz > 15.0 && hz < SR * 0.45 && amp != 0.0 {
            let i = self.np;
            self.parts[i] = Rotor::new(amp, hz, tau);
            self.hz[i] = hz;
            self.tau[i] = tau;
            self.amp[i] = amp;
            self.sustained |= tau <= 0.0;
            self.np += 1;
        }
        self
    }
    pub fn noise(mut self, n: NoiseLayer) -> Self {
        self.sustained |= n.decay >= 1.0;
        self.noise = Some(n);
        self
    }
    pub fn attack(mut self, secs: f32) -> Self {
        self.attack = ((secs * SR) as u32).max(1);
        self
    }
    /// Hold for `secs` after the attack, then release over `rel` seconds.
    pub fn hold(mut self, secs: f32, rel: f32) -> Self {
        self.release_at = self.attack + (secs * SR) as u32;
        self.release = ((rel * SR) as u32).max(1);
        self
    }
    pub fn gain(mut self, g: f32) -> Self {
        self.gain = g;
        self
    }
    /// Scale an already-built voice's level.
    pub fn scale(&mut self, k: f32) {
        self.gain *= k;
    }
    /// 0 = left .. 1 = right.
    pub fn pan(mut self, p: f32) -> Self {
        self.pan = p;
        self
    }
    /// Reverb send (0..1).
    pub fn send(mut self, s: f32) -> Self {
        self.send = s;
        self
    }
    pub fn at(mut self, delay_secs: f32) -> Self {
        self.delay = (delay_secs.max(0.0) * SR) as u32;
        self
    }
    pub fn bus(mut self, b: Bus) -> Self {
        self.bus = b;
        self
    }
    /// Pitch slides by `ratio` over `secs` (exponentially), then stays.
    pub fn glide(mut self, ratio: f32, secs: f32) -> Self {
        self.glide = (ratio, (secs * SR).max(1.0));
        self
    }
    /// Vibrato: rate in Hz, depth as a frequency ratio (0.01 ≈ 17 cents).
    pub fn vibrato(mut self, rate: f32, depth: f32) -> Self {
        self.vib = (rate, depth, 0.0);
        self
    }

    /// Finish building: work out how long the voice can possibly ring.
    pub fn done(mut self) -> Self {
        let tail = if self.release_at != u32::MAX {
            self.release_at as f32 + self.release as f32
        } else {
            let longest = self.tau[..self.np].iter().cloned().fold(0.0, f32::max);
            let n = self.noise.as_ref().map(|n| if n.decay < 1.0 { -1.0 / (n.decay.ln() * SR) } else { 0.0 }).unwrap_or(0.0);
            // ~ -60 dB after 7 time constants
            self.attack as f32 + longest.max(n) * 7.0 * SR
        };
        self.len = (tail as u32).clamp(1, (14.0 * SR) as u32);
        self
    }

    /// Not started yet (spawned this block).
    pub fn is_fresh(&self) -> bool {
        self.age == 0
    }

    #[inline]
    fn env(&self) -> f32 {
        let a = self.age;
        let e = if a < self.attack { a as f32 / self.attack as f32 } else { 1.0 };
        if a >= self.release_at {
            let r = 1.0 - (a - self.release_at) as f32 / self.release as f32;
            e * r.max(0.0) * r.max(0.0)
        } else {
            e
        }
    }

    fn modulate(&mut self) {
        let a = self.age as f32;
        let mut k = 1.0;
        if self.glide.0 != 1.0 {
            k *= self.glide.0.powf((a / self.glide.1).min(1.0));
        }
        if self.vib.1 > 0.0 {
            self.vib.2 += self.vib.0 * MOD_EVERY as f32 / SR;
            // vibrato fades in over the first half second
            let depth = self.vib.1 * (a / (0.5 * SR)).min(1.0);
            k *= 1.0 + depth * (self.vib.2 * std::f32::consts::TAU).sin();
        }
        if k != 1.0 || self.glide.0 != 1.0 {
            for i in 0..self.np {
                self.parts[i].tune(self.hz[i] * k, self.tau[i]);
            }
        }
        if let Some(n) = &mut self.noise {
            n.modulate(self.age);
        }
    }

    /// Mix into `dry` (stereo) and `rev` (mono reverb send), scaled by
    /// `g`. Returns false once the voice has finished.
    pub fn render(&mut self, dry: &mut [[f32; 2]], rev: &mut [f32], g: f32) -> bool {
        let n = dry.len();
        let mut i = 0;
        if self.delay > 0 {
            let skip = (self.delay as usize).min(n);
            self.delay -= skip as u32;
            i = skip;
        }
        let (pl, pr) = pan(self.pan);
        let g = g * self.gain;
        let (gl, gr, gs) = (g * pl, g * pr, g * self.send);
        let modulated = self.glide.0 != 1.0 || self.vib.1 > 0.0 || self.noise.as_ref().map(|n| n.sweep.is_some()).unwrap_or(false);
        while i < n {
            if self.age >= self.len {
                return false;
            }
            if modulated && self.age % MOD_EVERY == 0 {
                self.modulate();
            }
            let mut s = 0.0;
            for p in &mut self.parts[..self.np] {
                s += p.tick();
            }
            if let Some(nl) = &mut self.noise {
                s += nl.tick(self.age);
            }
            let v = s * self.env();
            dry[i][0] += v * gl;
            dry[i][1] += v * gr;
            rev[i] += v * gs;
            self.age += 1;
            i += 1;
        }
        if self.sustained && self.age % 1024 < n as u32 {
            for k in 0..self.np {
                if self.tau[k] <= 0.0 {
                    self.parts[k].renorm(self.amp[k]);
                }
            }
        }
        if self.delay == 0 && self.age > self.attack {
            // drop partials and noise that have died away (bells and tines
            // keep only their long fundamental ringing)
            let mut k = 0;
            while k < self.np {
                if self.tau[k] > 0.0 && self.parts[k].amp() < 1e-5 {
                    self.np -= 1;
                    let last = self.np;
                    self.parts.swap(k, last);
                    self.hz.swap(k, last);
                    self.tau.swap(k, last);
                    self.amp.swap(k, last);
                } else {
                    k += 1;
                }
            }
            if self.noise.as_ref().map(|n| n.decay < 1.0 && n.level < 1e-6 && self.age > n.attack).unwrap_or(false) {
                self.noise = None;
            }
            // decayed to silence before its nominal end?
            if !self.sustained {
                let level: f32 = self.parts[..self.np].iter().map(|p| p.amp()).sum::<f32>() + self.noise.as_ref().map(|n| n.level).unwrap_or(0.0);
                if level * self.gain < 1e-4 {
                    return false;
                }
            }
        }
        true
    }
}
