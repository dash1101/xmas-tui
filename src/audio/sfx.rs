//! Sound-effect recipes: each scene event becomes a handful of voices.
//! All of it is synthesized here — no samples.

use super::dsp::{midi_hz, Mode};
use super::instr::{celesta, chime, church_bell, glock, jingle_shake, ping};
use super::voice::{Bus, NoiseLayer, Voice};
use super::Sfx;
use crate::rng::Rng;

/// High pentatonic notes (C major pentatonic), MIDI, for sparkles.
const PENTA: [f32; 10] = [84.0, 86.0, 88.0, 91.0, 93.0, 96.0, 98.0, 100.0, 103.0, 105.0];

/// Spawn the voices for one effect. `pan` 0..1 (left..right), `gain`
/// scales the whole effect.
pub fn spawn(sfx: Sfx, pan: f32, gain: f32, rng: &mut Rng, out: &mut Vec<Voice>) {
    // keep scene positions off the hard edges: easier on the ears
    let p = 0.12 + 0.76 * pan.clamp(0.0, 1.0);
    let mut seed = || rng.next_u32();
    let start = out.len();
    match sfx {
        Sfx::Present => {
            // the box shakes three times, bells jingle inside, then a little magic
            for k in 0..3 {
                let t = k as f32 * 0.09;
                let sk = seed();
                out.push(
                    Voice::new(Bus::Sfx)
                        .part(170.0 + k as f32 * 15.0, 0.25, 0.035)
                        .noise(NoiseLayer::new(sk, Mode::Bp, 650.0, 1.4, 0.55, 0.03))
                        .pan(p + if k % 2 == 0 { -0.03 } else { 0.03 })
                        .at(t)
                        .done(),
                );
                jingle_shake(out, Bus::Sfx, t + 0.015, 0.55, p, sk ^ 0x55);
            }
            for (i, m) in [84.0, 88.0, 91.0, 96.0].iter().enumerate() {
                out.push(celesta(midi_hz(*m), 0.35).bus(Bus::Sfx).pan(p).at(0.32 + i as f32 * 0.06).done());
            }
        }
        Sfx::Snowman => {
            // a cute little vocal "hm-hm?" — additive vowels with gliding pitch
            let n = 2 + (seed() % 2) as usize;
            let f0 = 300.0 + (seed() % 70) as f32;
            let mut t = 0.0;
            for k in 0..n {
                let vowel = [(350.0, 850.0), (700.0, 1150.0), (320.0, 2200.0), (460.0, 900.0)][(seed() % 4) as usize];
                let dur = 0.10 + (seed() % 70) as f32 / 1000.0;
                let last = k + 1 == n;
                let up = if last { 1.25 + (seed() % 30) as f32 / 100.0 } else { 0.92 + (seed() % 12) as f32 / 100.0 };
                let f = f0 * if k == 1 { 1.12 } else { 1.0 };
                out.push(vocal(f, vowel, dur, up).pan(p).gain(0.9).at(t).done());
                t += dur + 0.06;
            }
        }
        Sfx::ShootingStar => {
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .noise(NoiseLayer::new(sk, Mode::Bp, 7000.0, 1.6, 0.22, 0.22).attack(0.06).sweep(7000.0, 2400.0, 0.7))
                    .part(3300.0, 0.05, 0.3)
                    .glide(0.55, 0.6)
                    .pan(p)
                    .send(0.5)
                    .done(),
            );
            for (i, m) in [100.0, 98.0, 95.0].iter().enumerate() {
                out.push(ping(midi_hz(*m), 0.32, 0.22, Bus::Sfx).pan(p).at(i as f32 * 0.07).done());
            }
        }
        Sfx::Comet => {
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .noise(NoiseLayer::new(sk, Mode::Bp, 3200.0, 0.8, 0.10, 0.0))
                    .part(1568.0, 0.012, 0.0)
                    .part(2093.0, 0.010, 0.0)
                    .vibrato(0.4, 0.003)
                    .attack(3.0)
                    .hold(4.0, 4.0)
                    .pan(p)
                    .send(0.8)
                    .done(),
            );
        }
        Sfx::ChurchBell => {
            out.push(church_bell(233.0, 0.85, seed(), Bus::Sfx).pan(p).gain(0.5).done());
        }
        Sfx::ChurchPeal => {
            // two rounds down a six-bell ring
            let bells = [74.0, 72.0, 71.0, 69.0, 67.0, 65.0];
            for round in 0..2 {
                for (i, m) in bells.iter().enumerate() {
                    let t = (round * 6 + i) as f32 * 0.33;
                    let bp = (p + (i as f32 - 2.5) * 0.02).clamp(0.0, 1.0);
                    out.push(church_bell(midi_hz(*m), 0.6, seed(), Bus::Sfx).pan(bp).gain(0.3).at(t).done());
                }
            }
        }
        Sfx::Launch => {
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .part(700.0, 0.07, 0.0)
                    .noise(NoiseLayer::new(sk, Mode::Hp, 3000.0, 0.7, 0.05, 0.0))
                    .glide(2.4, 0.9)
                    .attack(0.05)
                    .hold(0.75, 0.15)
                    .pan(p)
                    .send(0.3)
                    .done(),
            );
        }
        Sfx::Firework => {
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .part(85.0, 0.7, 0.22)
                    .glide(0.5, 0.25)
                    .noise(NoiseLayer::new(sk, Mode::Lp, 900.0, 0.7, 0.5, 0.08))
                    .attack(0.003)
                    .pan(p)
                    .gain(0.6)
                    .send(0.45)
                    .done(),
            );
            for k in 0..2 {
                out.push(
                    Voice::new(Bus::Sfx)
                        .noise(NoiseLayer::new(sk ^ (k + 1), Mode::Bp, 3800.0 + k as f32 * 900.0, 1.0, 1.4, 0.45).attack(0.15).grains(70.0))
                        .pan(p + if k == 0 { -0.08 } else { 0.08 })
                        .at(0.15)
                        .gain(0.35)
                        .send(0.5)
                        .done(),
                );
            }
        }
        Sfx::Puff => {
            let sk = seed();
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Lp, 2200.0, 0.7, 0.22, 0.07).attack(0.01)).pan(p).gain(1.6).done());
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk ^ 9, Mode::Bp, 1800.0, 0.8, 1.6, 0.08).grains(260.0)).pan(p).gain(1.0).done());
        }
        Sfx::Crunch => {
            let sk = seed();
            let f = 1300.0 + (sk % 900) as f32;
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Bp, f, 0.8, 1.4, 0.05).grains(200.0)).pan(p).gain(0.9).done());
        }
        Sfx::LightShow => {
            for (i, m) in PENTA.iter().enumerate() {
                let pp = 0.2 + 0.6 * i as f32 / 9.0;
                out.push(glock(midi_hz(*m), 0.3).bus(Bus::Sfx).pan(pp).at(i as f32 * 0.09).done());
            }
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .noise(NoiseLayer::new(sk, Mode::Hp, 6500.0, 0.7, 0.05, 0.0))
                    .attack(0.4)
                    .hold(0.6, 0.8)
                    .send(0.6)
                    .done(),
            );
        }
        Sfx::Click => {
            for k in 0..2 {
                out.push(
                    Voice::new(Bus::Sfx)
                        .part(140.0, 0.18, 0.01)
                        .noise(NoiseLayer::new(seed(), Mode::Bp, 3000.0 - k as f32 * 600.0, 2.0, 0.7, 0.002))
                        .attack(0.0003)
                        .pan(p)
                        .at(k as f32 * 0.03)
                        .gain(1.0)
                        .done(),
                );
            }
        }
        Sfx::Lamp => {
            // the gas mantle fizzles and buzzes, then catches again
            let mut t = 0.0;
            for _ in 0..10 {
                let d = 0.03 + (seed() % 90) as f32 / 1000.0;
                out.push(
                    Voice::new(Bus::Sfx)
                        .part(120.0, 0.12, 0.0)
                        .part(240.0, 0.08, 0.0)
                        .part(360.0, 0.05, 0.0)
                        .noise(NoiseLayer::new(seed(), Mode::Bp, 2500.0, 1.0, 1.0, 0.0).grains(900.0))
                        .attack(0.005)
                        .hold(d, 0.02)
                        .pan(p)
                        .at(t)
                        .gain(0.6)
                        .done(),
                );
                t += d + 0.06 + (seed() % 200) as f32 / 1000.0;
            }
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(seed(), Mode::Lp, 420.0, 0.7, 0.4, 0.15).attack(0.05)).pan(p).at(t.max(2.3)).gain(0.7).done());
        }
        Sfx::Fanfare => {
            for (i, m) in [79.0, 84.0, 88.0].iter().enumerate() {
                out.push(celesta(midi_hz(*m), 0.45).bus(Bus::Sfx).pan(p).at(i as f32 * 0.1).done());
            }
            for (i, m) in [84.0, 88.0, 91.0, 96.0].iter().enumerate() {
                out.push(glock(midi_hz(*m), 0.3).bus(Bus::Sfx).pan(0.3 + i as f32 * 0.13).at(0.35).done());
            }
            let sk = seed();
            jingle_shake(out, Bus::Sfx, 0.35, 0.45, 0.4, sk);
            jingle_shake(out, Bus::Sfx, 0.5, 0.35, 0.6, sk ^ 3);
        }
        Sfx::TrainWhistle => {
            // a distant two-toot steam whistle (a three-pipe chord)
            for (k, (at, hold, rel)) in [(0.0, 0.35, 0.2), (0.65, 1.0, 0.5)].iter().enumerate() {
                let mut v = Voice::new(Bus::Sfx);
                for r in [1.0, 1.26, 1.5] {
                    let f = 466.0 * r;
                    v = v.part(f, 0.09, 0.0).part(f * 2.0, 0.025, 0.0).part(f * 3.0, 0.008, 0.0);
                }
                out.push(
                    v.noise(NoiseLayer::new(seed() ^ k as u32, Mode::Bp, 1500.0, 1.0, 0.05, 0.0))
                        .glide(1.02, 0.15)
                        .vibrato(5.0, 0.004)
                        .attack(0.12)
                        .hold(*hold, *rel)
                        .pan(p)
                        .at(*at)
                        .gain(0.6)
                        .send(0.8)
                        .done(),
                );
            }
        }
        Sfx::Cursor => {
            let sk = seed();
            let f = 3000.0 + (sk % 2600) as f32;
            out.push(
                ping(f, 0.25, 0.08, Bus::Sfx)
                    .noise(NoiseLayer::new(sk, Mode::Hp, 7000.0, 0.7, 0.05, 0.03))
                    .pan(p)
                    .gain(0.3)
                    .done(),
            );
        }
        Sfx::Sparkle => {
            let base = (seed() % 4) as usize;
            for i in 0..4 {
                out.push(ping(midi_hz(PENTA[base + i]), 0.3, 0.18, Bus::Sfx).pan(p).at(i as f32 * 0.05).gain(0.8).done());
            }
        }
        Sfx::Starburst => {
            for m in [96.0, 100.0, 103.0] {
                out.push(glock(midi_hz(m), 0.3).bus(Bus::Sfx).pan(p).gain(0.6).done());
            }
            for i in 0..8 {
                let m = PENTA[9 - i];
                let pp = p + (i as f32 - 3.5) * 0.03;
                out.push(ping(midi_hz(m), 0.25, 0.2, Bus::Sfx).pan(pp).at(0.12 + i as f32 * 0.05).gain(0.5).done());
            }
        }
        Sfx::Magic => {
            for (i, m) in [96.0, 93.0, 91.0, 88.0, 86.0, 84.0].iter().enumerate() {
                out.push(celesta(midi_hz(*m), 0.3).bus(Bus::Sfx).pan(p).at(i as f32 * 0.07).done());
            }
            out.push(chime(midi_hz(72.0), 0.4, Bus::Sfx).pan(p).at(0.45).done());
            let sk = seed();
            out.push(
                Voice::new(Bus::Sfx)
                    .noise(NoiseLayer::new(sk, Mode::Bp, 5000.0, 1.2, 0.08, 0.3).attack(0.1).sweep(5000.0, 1800.0, 0.6))
                    .pan(p)
                    .send(0.6)
                    .done(),
            );
        }
        Sfx::Chime => {
            out.push(chime(1568.0, 0.5, Bus::Sfx).pan(p).gain(0.5).done());
        }
        Sfx::Deer => {
            // a soft, breathy snort and a low "mm"
            let sk = seed();
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Bp, 900.0, 0.9, 0.35, 0.09).attack(0.01)).pan(p).gain(0.9).done());
            out.push(vocal(150.0, (500.0, 900.0), 0.22, 0.85).pan(p).gain(0.7).at(0.08).done());
        }
        Sfx::Fox => {
            // two bright little yips
            for k in 0..2 {
                let f = 560.0 + (seed() % 90) as f32;
                out.push(vocal(f, (900.0, 1700.0), 0.075, 1.45).pan(p).gain(1.3).at(k as f32 * 0.14).done());
            }
        }
        Sfx::Rabbit => {
            // a soft foot-thump and a tiny squeak
            out.push(Voice::new(Bus::Sfx).part(95.0, 0.9, 0.06).part(140.0, 0.3, 0.04).pan(p).gain(0.8).done());
            out.push(ping(2600.0, 0.12, 0.05, Bus::Sfx).pan(p).at(0.1).done());
        }
        Sfx::Munch => {
            // crunchy nibbles
            let n = 3 + (seed() % 2) as usize;
            for k in 0..n {
                let sk = seed();
                let f = 1400.0 + (sk % 1200) as f32;
                out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Bp, f, 1.1, 0.9, 0.035).grains(320.0)).pan(p).gain(7.0).at(k as f32 * 0.13).done());
                out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk ^ 7, Mode::Lp, 1100.0, 0.8, 0.5, 0.04).attack(0.004)).pan(p).gain(0.8).at(k as f32 * 0.13).done());
            }
        }
        Sfx::Pet => {
            // an "aww" and a little twinkle
            let f0 = 240.0 + (seed() % 60) as f32;
            out.push(vocal(f0, (700.0, 1150.0), 0.34, 0.82).pan(p).gain(0.75).done());
            out.push(celesta(midi_hz(88.0), 0.18).bus(Bus::Sfx).pan(p).at(0.18).done());
            out.push(celesta(midi_hz(93.0), 0.15).bus(Bus::Sfx).pan(p).at(0.3).done());
        }
        Sfx::Greet => {
            // a cheerful "hey!"
            let f0 = 210.0 + (seed() % 90) as f32;
            out.push(vocal(f0, (480.0, 1850.0), 0.17, 1.22).pan(p).gain(0.8).done());
        }
        Sfx::Laugh => {
            // a quick giggle: falling "hee-hee-hee"
            let f0 = 360.0 + (seed() % 80) as f32;
            for k in 0..4 {
                out.push(vocal(f0 * (1.0 - k as f32 * 0.05), (380.0, 2200.0), 0.065, 0.95).pan(p).gain(0.6).at(k as f32 * 0.105).done());
            }
        }
        Sfx::Pickup => {
            // "whoop!" — a rising glide
            out.push(Voice::new(Bus::Sfx).part(330.0, 0.32, 0.0).part(660.0, 0.08, 0.0).glide(2.6, 0.2).attack(0.01).hold(0.17, 0.06).pan(p).send(0.25).done());
        }
        Sfx::Drop => {
            // landing softly in the snow
            let sk = seed();
            out.push(Voice::new(Bus::Sfx).part(78.0, 0.8, 0.09).noise(NoiseLayer::new(sk, Mode::Lp, 1600.0, 0.7, 0.4, 0.12).attack(0.004)).pan(p).gain(0.9).done());
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk ^ 5, Mode::Bp, 2000.0, 0.8, 1.2, 0.09).grains(240.0)).pan(p).gain(0.6).at(0.03).done());
        }
        Sfx::Snowball => {
            // whoosh … puff
            let sk = seed();
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Bp, 1200.0, 1.2, 0.3, 0.0).sweep(900.0, 2400.0, 0.3).attack(0.05)).hold(0.25, 0.05).pan(p).gain(0.5).done());
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk ^ 3, Mode::Lp, 2000.0, 0.7, 0.4, 0.08).attack(0.005)).pan(p).gain(1.1).at(0.34).done());
        }
        Sfx::Rustle => {
            // leaves and snow shaken loose
            let sk = seed();
            out.push(Voice::new(Bus::Sfx).noise(NoiseLayer::new(sk, Mode::Bp, 2600.0, 0.9, 1.4, 0.0).grains(140.0).attack(0.02)).hold(0.35, 0.15).pan(p).gain(4.0).done());
        }
        Sfx::Bird => {
            // a cardinal's quick chirps
            let n = 2 + (seed() % 3) as usize;
            for k in 0..n {
                let f = 2600.0 + (seed() % 900) as f32;
                out.push(Voice::new(Bus::Sfx).part(f, 0.12, 0.0).glide(1.35, 0.05).attack(0.004).hold(0.05, 0.02).pan(p).send(0.3).at(k as f32 * 0.09).done());
            }
        }
    }
    for v in &mut out[start..] {
        v.scale(gain);
    }
}

/// Additive "vowel": harmonics of f0 weighted by two formant resonances,
/// pitch gliding by `glide` over the syllable.
fn vocal(f0: f32, (f1, f2): (f32, f32), dur: f32, glide: f32) -> Voice {
    let mut v = Voice::new(Bus::Sfx);
    for h in 1..=12 {
        let f = f0 * h as f32;
        let res = |c: f32, bw: f32| 1.0 / (1.0 + ((f - c) / bw).powi(2));
        let a = (res(f1, 130.0) + 0.6 * res(f2, 220.0)) * 0.35 / (h as f32).sqrt();
        if a > 0.004 {
            v = v.part(f, a, 0.0);
        }
    }
    v.glide(glide, dur).vibrato(6.0, 0.01).attack(0.02).hold(dur, 0.06).send(0.25)
}
