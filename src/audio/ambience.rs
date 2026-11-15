//! Continuous sound beds, smoothly following the scene: wind (louder in
//! gusts and blizzards), soft snowfall hiss, a gentle fireplace, very soft
//! distant wind chimes, an aurora shimmer — plus the two travelling sounds,
//! Santa's sleigh bells and the distant train, which pan with their x.

use super::dsp::{midi_hz, Mode, Noise, OnePole, Rotor, Svf};
use super::instr::{chime, jingle_shake};
use super::voice::{Bus, NoiseLayer, Voice};
use crate::rng::Rng;

/// What the scene is doing right now, sent ~30×/s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AmbState {
    /// Wind strength: ~0.3 calm, 1 gusty, 2+ in a blizzard.
    pub wind: f32,
    /// Snowfall density (0 = none, 1 = normal).
    pub snow: f32,
    /// Santa's sleigh: pan 0..1 and audibility 0..1, while it flies.
    pub sleigh: Option<(f32, f32)>,
    /// The distant train: pan and audibility.
    pub train: Option<(f32, f32)>,
    /// Aurora flare 0..1.
    pub aurora: f32,
}

pub const BLOCK: usize = 256;

pub struct Ambience {
    pub state: AmbState,
    /// Per-layer volumes: wind, snowfall, fire, chimes, aurora, sleigh, train.
    pub mix: [f32; 7],
    n: [Noise; 4],
    wind_bp: [Svf; 2],
    wind_lp: [Svf; 2],
    whistle: Svf,
    hiss: [Svf; 2],
    fire_lp: Svf,
    brown: f32,
    wind_lvl: OnePole,
    snow_lvl: OnePole,
    sleigh_gain: OnePole,
    sleigh_pan: OnePole,
    train_gain: OnePole,
    train_pan: OnePole,
    aurora_lvl: OnePole,
    aurora: [Rotor; 3],
    t: f64,
    next_crackle: f64,
    next_pop: f64,
    next_chime: f64,
    next_jingle: f64,
    next_chug: f64,
    step: u32,
    rng: Rng,
}

impl Ambience {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed as u64 ^ 0xa3b1);
        let s = |k: u32| seed.wrapping_mul(31).wrapping_add(k);
        Ambience {
            state: AmbState { wind: 0.3, snow: 1.0, ..Default::default() },
            mix: [1.0; 7],
            n: [Noise::new(s(1)), Noise::new(s(2)), Noise::new(s(3)), Noise::new(s(4))],
            wind_bp: [Svf::new(Mode::Bp, 400.0, 0.9), Svf::new(Mode::Bp, 430.0, 0.9)],
            wind_lp: [Svf::new(Mode::Lp, 170.0, 0.7), Svf::new(Mode::Lp, 160.0, 0.7)],
            whistle: Svf::new(Mode::Bp, 800.0, 9.0),
            hiss: [Svf::new(Mode::Hp, 5200.0, 0.7), Svf::new(Mode::Hp, 5600.0, 0.7)],
            fire_lp: Svf::new(Mode::Lp, 260.0, 0.7),
            brown: 0.0,
            wind_lvl: OnePole::smoother(2.0, BLOCK),
            snow_lvl: OnePole::smoother(3.0, BLOCK),
            sleigh_gain: OnePole::smoother(2.0, BLOCK),
            sleigh_pan: OnePole::smoother(0.3, BLOCK),
            train_gain: OnePole::smoother(4.0, BLOCK),
            train_pan: OnePole::smoother(0.5, BLOCK),
            aurora_lvl: OnePole::smoother(4.0, BLOCK),
            aurora: [Rotor::new(1.0, midi_hz(72.0), 0.0), Rotor::new(1.0, midi_hz(76.0), 0.0), Rotor::new(1.0, midi_hz(79.0), 0.0)],
            t: 0.0,
            next_crackle: 0.5,
            next_pop: rng.range(2.0, 6.0) as f64,
            next_chime: rng.range(4.0, 10.0) as f64,
            next_jingle: 0.0,
            next_chug: 0.0,
            step: 0,
            rng,
        }
    }

    /// Render one block of beds into `dry`/`rev` (gain `g`), and spawn
    /// the event-like parts (crackles, chimes, bells, chugs) as voices.
    pub fn render(&mut self, dry: &mut [[f32; 2]], rev: &mut [f32], g: f32, voices: &mut Vec<Voice>) {
        let n = dry.len();
        let dt = n as f64 / super::dsp::SR as f64;
        let st = self.state;
        let wind = self.wind_lvl.tick(st.wind.clamp(0.0, 3.0));
        let snow = self.snow_lvl.tick(st.snow.clamp(0.0, 3.0));
        let aur = self.aurora_lvl.tick(st.aurora.clamp(0.0, 1.0));
        let t = self.t as f32;
        // slow, irregular wind movement
        let lfo = |p: f32| (t * 0.13 + p).sin() * 0.5 + (t * 0.071 + 1.0 + p).sin() * 0.3 + (t * 0.29 + 2.0 + p * 2.0).sin() * 0.2;
        let (la, lb) = (lfo(0.0), lfo(1.7));
        self.wind_bp[0].set(260.0 + 480.0 * wind + 160.0 * la, 0.9);
        self.wind_bp[1].set(260.0 + 480.0 * wind + 160.0 * lb, 0.9);
        self.whistle.set(650.0 + 350.0 * lfo(3.1) + 250.0 * wind, 9.0);
        let mx = self.mix;
        let wa = 0.16 * (wind + 0.15).powf(1.25) * (1.0 + 0.35 * la) * mx[0];
        let wb = 0.16 * (wind + 0.15).powf(1.25) * (1.0 + 0.35 * lb) * mx[0];
        let whistle = 0.05 * (wind - 0.9).max(0.0) * (0.6 + 0.4 * lfo(5.0)).max(0.0) * mx[0];
        let hiss = (0.006 * snow + 0.006 * (wind - 0.5).max(0.0)) * mx[1];
        let fire = 0.035 * mx[2];
        let wp = 0.5 + 0.3 * lfo(7.0);
        let ag = 0.006 * aur * mx[4];
        if g > 0.0 {
            for i in 0..n {
                let (w0, w1, w2, w3) = (self.n[0].next(), self.n[1].next(), self.n[2].next(), self.n[3].next());
                let mut l = self.wind_bp[0].tick(w0) * wa + self.wind_lp[0].tick(w0) * wa * 0.5;
                let mut r = self.wind_bp[1].tick(w1) * wb + self.wind_lp[1].tick(w1) * wb * 0.5;
                let wh = self.whistle.tick(w2) * whistle;
                l += wh * (1.0 - wp) * 1.4;
                r += wh * wp * 1.4;
                l += self.hiss[0].tick(w2) * hiss;
                r += self.hiss[1].tick(w3) * hiss;
                // the fire: brown-noise rumble
                self.brown = (self.brown * 0.996 + w3 * 0.04).clamp(-1.0, 1.0);
                let f = self.fire_lp.tick(self.brown) * fire;
                l += f;
                r += f;
                if ag > 1e-5 {
                    let a = self.aurora[0].tick() + self.aurora[1].tick() * 0.8 + self.aurora[2].tick() * 0.6;
                    let wob = 1.0 + 0.4 * (t * 0.5).sin();
                    l += a * ag * wob;
                    r += a * ag * (2.0 - wob);
                    rev[i] += a * ag * 0.8 * g;
                }
                dry[i][0] += l * g;
                dry[i][1] += r * g;
                rev[i] += (l + r) * 0.08 * g;
            }
            if self.step % 64 == 0 {
                for r in &mut self.aurora {
                    r.renorm(1.0);
                }
            }
        }
        self.events(dt, wind, voices);
        self.t += dt;
        self.step = self.step.wrapping_add(1);
    }

    fn events(&mut self, dt: f64, wind: f32, voices: &mut Vec<Voice>) {
        let (t0, t1) = (self.t, self.t + dt);
        let off = |at: f64| (at - t0) as f32;
        // fireplace crackles and the odd pop
        while self.next_crackle < t1 {
            let at = self.next_crackle.max(t0);
            let f = self.rng.range(1500.0, 5200.0);
            let a = self.rng.range(0.05, 0.28) * self.mix[2];
            let tau = self.rng.range(0.003, 0.012);
            let p = self.rng.range(0.35, 0.5);
            voices.push(Voice::new(Bus::Amb).noise(NoiseLayer::new(self.rng.next_u32(), Mode::Bp, f, 1.4, a, tau)).attack(0.0003).pan(p).send(0.1).at(off(at)).done());
            self.next_crackle = at + self.rng.range(0.03, 0.5) as f64;
        }
        while self.next_pop < t1 {
            let at = self.next_pop.max(t0);
            let f = self.rng.range(700.0, 1400.0);
            voices.push(Voice::new(Bus::Amb).part(f * 0.25, 0.08, 0.02).noise(NoiseLayer::new(self.rng.next_u32(), Mode::Bp, f, 1.2, 0.5, 0.02)).pan(0.42).send(0.15).gain(self.mix[2]).at(off(at)).done());
            self.next_pop = at + self.rng.range(2.5, 9.0) as f64;
        }
        // distant wind chimes, a little more often when it's breezy
        while self.next_chime < t1 {
            let at = self.next_chime.max(t0);
            let notes = [84.0, 86.0, 88.0, 91.0, 93.0, 96.0];
            let k = if self.rng.chance(0.4) { self.rng.int(2, 4) } else { 1 };
            let mut dt2 = 0.0;
            for _ in 0..k {
                let m = *self.rng.pick(&notes);
                let p = self.rng.range(0.2, 0.8);
                let v = self.rng.range(0.15, 0.3) * self.mix[3];
                voices.push(chime(midi_hz(m), v, Bus::Amb).pan(p).gain(0.35).at(off(at) + dt2).done());
                dt2 += self.rng.range(0.25, 0.8);
            }
            self.next_chime = at + (self.rng.range(7.0, 18.0) / (0.6 + 0.5 * wind.min(2.0))) as f64;
        }
        // Santa's sleigh bells: a galloping jingle, panned with the sleigh
        let (sp, sg) = self.state.sleigh.unwrap_or((self.sleigh_pan.y, 0.0));
        let sg = self.sleigh_gain.tick(sg);
        let sp = self.sleigh_pan.tick(sp);
        if sg > 0.003 {
            self.next_jingle = self.next_jingle.max(t0);
            while self.next_jingle < t1 {
                let at = self.next_jingle;
                let beat = (at / 0.19) as u64;
                let accent = [1.0, 0.45, 0.75, 0.45][(beat % 4) as usize];
                let seed = self.rng.next_u32();
                jingle_shake(voices, Bus::Sfx, off(at), 0.8 * accent * sg * self.mix[5], sp, seed);
                self.next_jingle = at + 0.19;
            }
        }
        // the distant train: soft chuffs and the odd rail clack
        let (tp, tg) = self.state.train.unwrap_or((self.train_pan.y, 0.0));
        let tg = self.train_gain.tick(tg) * self.mix[6];
        let tp = self.train_pan.tick(tp);
        if tg > 0.003 {
            self.next_chug = self.next_chug.max(t0);
            while self.next_chug < t1 {
                let at = self.next_chug;
                let k = (at / 0.17) as u64;
                let accent = [1.0, 0.55, 0.8, 0.55][(k % 4) as usize];
                voices.push(
                    Voice::new(Bus::Sfx)
                        .noise(NoiseLayer::new(self.rng.next_u32(), Mode::Lp, 650.0, 0.8, 0.5, 0.07).attack(0.012))
                        .pan(tp)
                        .gain(0.06 * accent * tg)
                        .send(0.5)
                        .at(off(at))
                        .done(),
                );
                if k % 8 == 0 {
                    for c in 0..2 {
                        voices.push(
                            Voice::new(Bus::Sfx)
                                .noise(NoiseLayer::new(self.rng.next_u32(), Mode::Bp, 2200.0, 2.0, 0.4, 0.004))
                                .pan(tp)
                                .gain(0.06 * tg)
                                .send(0.4)
                                .at(off(at) + 0.05 + c as f32 * 0.09)
                                .done(),
                        );
                    }
                }
                self.next_chug = at + 0.17;
            }
        }
    }
}
