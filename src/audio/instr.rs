//! Instrument timbres, each a recipe of partials (+ noise) for a Voice.
//! Frequencies in Hz, `vel` 0..1. Levels here are relative; the mixer's
//! bus gains set the final loudness.

use super::dsp::{midi_hz, Mode};
use super::voice::{Bus, NoiseLayer, Voice};

/// The instruments the music arranger can use.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Instr {
    MusicBox,
    Celesta,
    Glock,
    Piano,
    Pad,
    Bass,
    Pluck,
    Jingle,
    Clop,
}

/// A music note → voice. `dur` is the written length in seconds.
pub fn note(instr: Instr, midi: f32, dur: f32, vel: f32, seed: u32) -> Voice {
    let hz = midi_hz(midi);
    match instr {
        Instr::MusicBox => music_box(hz, vel, seed),
        Instr::Celesta => celesta(hz, vel).hold(dur + 0.6, 0.5),
        Instr::Glock => glock(hz, vel),
        Instr::Piano => piano(hz, vel).hold(dur.max(0.12), 0.35),
        Instr::Pad => pad(hz, vel, dur),
        Instr::Bass => bass(hz, vel).hold(dur.max(0.1), 0.12),
        Instr::Pluck => pluck(hz, vel).hold(dur.max(0.08), 0.1),
        Instr::Jingle => jingle(vel, seed),
        Instr::Clop => clop(midi, vel, seed),
    }
    .done()
}

/// Music-box comb tine: bright, glassy, a little metallic "tick" on top.
pub fn music_box(hz: f32, vel: f32, seed: u32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    // higher tines ring shorter, like a real comb
    let tau = (1.5 * (523.0 / hz).powf(0.4)).clamp(0.5, 2.4);
    Voice::new(Bus::Music)
        .part(hz, 0.55 * v, tau)
        .part(hz * 2.0, 0.08 * v, tau * 0.3)
        .part(hz * 3.01, 0.10 * v * v, tau * 0.16)
        .part(hz * 5.98, 0.06 * v * v, 0.05)
        .part(hz * 8.93, 0.025 * v, 0.025)
        .noise(NoiseLayer::new(seed, Mode::Hp, 7000.0, 0.7, 0.03 * v, 0.004))
        .attack(0.0015)
        .send(0.35)
}

/// Celesta: softer and rounder than the music box, longer ring.
pub fn celesta(hz: f32, vel: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    let tau = (1.8 * (523.0 / hz).powf(0.35)).clamp(0.6, 2.8);
    Voice::new(Bus::Music)
        .part(hz, 0.55 * v, tau)
        .part(hz * 2.0, 0.06 * v, tau * 0.35)
        .part(hz * 4.0, 0.09 * v * v, tau * 0.12)
        .part(hz * 7.0, 0.02 * v, 0.03)
        .attack(0.003)
        .send(0.4)
}

/// Glockenspiel bar: inharmonic overtones (free-free bar modes).
pub fn glock(hz: f32, vel: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    Voice::new(Bus::Music)
        .part(hz, 0.45 * v, 1.6)
        .part(hz * 2.76, 0.16 * v, 0.35)
        .part(hz * 5.40, 0.07 * v, 0.12)
        .part(hz * 8.93, 0.03 * v, 0.05)
        .attack(0.001)
        .send(0.45)
}

/// Soft felt piano: harmonics with height-dependent decay, a slightly
/// detuned second string for warmth, gentler highs at low velocity.
pub fn piano(hz: f32, vel: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    let tau1 = (2.6 * (262.0 / hz).powf(0.5)).clamp(0.6, 5.0);
    let mut vc = Voice::new(Bus::Music).attack(0.004).send(0.3);
    for n in 1..=7 {
        let nf = n as f32;
        // slight stiffness: upper partials run a little sharp
        let f = hz * nf * (1.0 + 0.0004 * nf * nf);
        let bright = (v * 1.2).min(1.0).powf(nf * 0.35);
        let a = 0.62 * v / nf.powf(1.4) * bright;
        vc = vc.part(f, a, tau1 / nf.powf(0.6));
    }
    vc.part(hz * 1.0012, 0.26 * v, tau1 * 1.3)
}

/// Warm sustained pad: two detuned soft-saw stacks, slow swell.
pub fn pad(hz: f32, vel: f32, dur: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    let mut vc = Voice::new(Bus::Music).attack(0.9).hold(dur.max(0.5), 1.8).send(0.55);
    for n in 1..=5 {
        let nf = n as f32;
        let a = 0.16 * v / nf.powf(1.6);
        vc = vc.part(hz * nf * 1.0025, a, 0.0).part(hz * nf * 0.9975, a, 0.0);
    }
    vc
}

/// Round sine-ish bass.
pub fn bass(hz: f32, vel: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    Voice::new(Bus::Music)
        .part(hz, 0.6 * v, 1.1)
        .part(hz * 2.0, 0.18 * v, 0.4)
        .part(hz * 3.0, 0.06 * v, 0.2)
        .attack(0.006)
        .send(0.12)
}

/// Short plucked chord tone (sleigh-ride off-beats).
pub fn pluck(hz: f32, vel: f32) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    Voice::new(Bus::Music)
        .part(hz, 0.4 * v, 0.35)
        .part(hz * 2.0, 0.16 * v, 0.16)
        .part(hz * 3.0, 0.08 * v, 0.08)
        .part(hz * 4.0, 0.04 * v, 0.05)
        .attack(0.003)
        .send(0.25)
}

/// One sleigh bell: a small closed bell with a rattling pellet, so a
/// cluster of bright inharmonic partials plus a burst of high noise.
pub fn sleigh_bell(seed: u32, vel: f32, bus: Bus) -> Voice {
    let h = crate::rng::hash(seed);
    let f = 2300.0 + (h % 1400) as f32;
    let v = vel.clamp(0.02, 1.0);
    Voice::new(bus)
        .part(f, 0.20 * v, 0.16)
        .part(f * 1.47, 0.14 * v, 0.11)
        .part(f * 2.09, 0.10 * v, 0.08)
        .part(f * 2.56, 0.06 * v, 0.06)
        .noise(NoiseLayer::new(seed, Mode::Bp, 7500.0, 1.2, 0.10 * v, 0.025))
        .attack(0.0008)
        .send(0.3)
}

/// A shake of a sleigh-bell strap: several bells within ~50 ms.
pub fn jingle(vel: f32, seed: u32) -> Voice {
    // one voice can't hold a whole shake, so this is the loudest bell;
    // `jingle_shake` adds the rest
    sleigh_bell(seed, vel, Bus::Music)
}

pub fn jingle_shake(out: &mut Vec<Voice>, bus: Bus, at: f32, vel: f32, pan: f32, seed: u32) {
    let n = 3 + (crate::rng::hash(seed) % 3) as usize;
    for k in 0..n {
        let s = crate::rng::hash(seed.wrapping_add(k as u32 * 7919));
        let dt = (s % 1000) as f32 / 1000.0 * 0.05;
        let p = (pan + ((s >> 10) % 100) as f32 / 100.0 * 0.12 - 0.06).clamp(0.0, 1.0);
        out.push(sleigh_bell(s, vel * if k == 0 { 1.0 } else { 0.6 }, bus).at(at + dt).pan(p).done());
    }
}

/// Hoof "clop" — a hollow wooden knock (two resonant band-pass taps).
pub fn clop(midi: f32, vel: f32, seed: u32) -> Voice {
    let f = midi_hz(midi);
    Voice::new(Bus::Music)
        .part(f, 0.25 * vel, 0.03)
        .noise(NoiseLayer::new(seed, Mode::Bp, f * 1.6, 4.0, 0.5 * vel, 0.018))
        .attack(0.0005)
        .send(0.15)
}

/// Church bell: hum, prime, minor-third tierce, quint, nominal and a few
/// upper partials, each with its own decay; the strike adds a clang.
pub fn church_bell(hz: f32, vel: f32, seed: u32, bus: Bus) -> Voice {
    let v = vel.clamp(0.05, 1.0);
    Voice::new(bus)
        .part(hz * 0.5, 0.30 * v, 5.5)
        .part(hz, 0.34 * v, 3.6)
        .part(hz * 1.19, 0.22 * v, 2.6)
        .part(hz * 1.50, 0.12 * v, 2.0)
        .part(hz * 2.0, 0.22 * v, 2.4)
        .part(hz * 2.51, 0.09 * v, 1.2)
        .part(hz * 2.66, 0.07 * v, 1.0)
        .part(hz * 3.01, 0.06 * v, 0.7)
        .part(hz * 4.17, 0.04 * v, 0.4)
        .noise(NoiseLayer::new(seed, Mode::Bp, hz * 5.0, 2.0, 0.12 * v, 0.03))
        .attack(0.002)
        .send(0.7)
}

/// Wind-chime tube: 1 : 2.76 : 5.40 overtone series, long ring.
pub fn chime(hz: f32, vel: f32, bus: Bus) -> Voice {
    let v = vel.clamp(0.02, 1.0);
    Voice::new(bus)
        .part(hz, 0.4 * v, 2.6)
        .part(hz * 2.76, 0.16 * v, 1.1)
        .part(hz * 5.40, 0.06 * v, 0.4)
        .attack(0.002)
        .send(0.7)
}

/// A tiny high "ping" for sparkles and twinkles.
pub fn ping(hz: f32, vel: f32, tau: f32, bus: Bus) -> Voice {
    Voice::new(bus).part(hz, 0.4 * vel, tau).part(hz * 2.0, 0.08 * vel, tau * 0.5).attack(0.001).send(0.5)
}
