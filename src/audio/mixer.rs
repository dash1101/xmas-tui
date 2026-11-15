//! The mixer: owns every voice, the ambience beds and the music player,
//! sums them per bus (each with its own smoothed volume) plus a shared
//! reverb, then DC-blocks and peak-limits the result. Pure computation —
//! the audio thread and the WAV export both drive it the same way.

use super::ambience::{AmbState, Ambience, BLOCK};
use super::dsp::{DcBlock, Limiter, OnePole, Reverb, SR};
use super::music::{Music, Style};
use super::sfx;
use super::voice::{Bus, Voice};
use super::Sfx;
use crate::config::SoundCfg;
use crate::rng::Rng;

/// Level of each bus at full slider, before the master. Balanced with
/// `--export-audio` + scripts/check-audio.py so music sits around
/// -24 dBFS RMS at default settings, ambience well below it, and effects
/// read clearly over both without ever reaching the limiter.
const BASE_MUSIC: f32 = 2.5;
const BASE_SFX: f32 = 4.0;
const BASE_AMB: f32 = 6.0;
const MAX_VOICES: usize = 192;

pub struct Mixer {
    voices: Vec<Voice>,
    pub amb: Ambience,
    pub music: Music,
    reverb: Reverb,
    lim: Limiter,
    dc: [DcBlock; 2],
    rng: Rng,
    /// master, music, sfx, ambience: targets and smoothed values
    target: [f32; 4],
    vol: [OnePole; 4],
    awake: bool,
    group: [f32; 3],
    dry: Vec<[f32; 2]>,
    rev: Vec<f32>,
    t: f64,
    last: [f64; 2],
    /// Non-finite samples caught by the output guard (should stay 0).
    pub nonfinite: u64,
    /// Per-group effect volumes from the config.
    fx_vol: crate::config::EffectVolumes,
}

/// Slider position (0..1) → gain: squared, so the slider feels even.
fn curve(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    v * v
}

impl Mixer {
    pub fn new(seed: u32) -> Self {
        let mut vol = [OnePole::smoother(0.15, BLOCK); 4];
        for v in &mut vol {
            v.y = 0.0;
        }
        Mixer {
            voices: Vec::with_capacity(MAX_VOICES),
            amb: Ambience::new(seed),
            music: Music::new(seed),
            reverb: Reverb::new(),
            lim: Limiter::new(-1.5),
            dc: [DcBlock::default(); 2],
            rng: Rng::new(seed as u64 ^ 0x5fc),
            target: [0.0; 4],
            vol,
            awake: true,
            group: [1.0; 3],
            dry: vec![[0.0; 2]; BLOCK],
            rev: vec![0.0; BLOCK],
            t: 0.0,
            last: [-1.0; 2],
            nonfinite: 0,
            fx_vol: Default::default(),
        }
    }

    pub fn configure(&mut self, c: &SoundCfg) {
        let on = if c.enabled { 1.0 } else { 0.0 };
        self.target = [on * curve(c.master), BASE_MUSIC * curve(c.music), BASE_SFX * curve(c.sfx), BASE_AMB * curve(c.ambience)];
        self.music.set_pref(Style::parse(&c.track));
        self.fx_vol = c.effects.clone();
        let (m, e) = (&c.ambience_mix, &c.effects);
        self.amb.mix = [m.wind, m.snowfall, m.fire, m.chimes, m.aurora, e.sleigh, e.train];
    }

    /// Asleep (paused front-end / sound off): everything fades to silence.
    pub fn set_awake(&mut self, awake: bool) {
        self.awake = awake;
    }

    /// Jump straight to the configured volumes (no fade-in), for export.
    pub fn snap_volumes(&mut self) {
        for i in 0..4 {
            self.vol[i].y = self.target[i];
        }
    }

    pub fn set_state(&mut self, s: AmbState) {
        self.amb.state = s;
    }

    /// The master gain right now (0 once faded out).
    pub fn level(&self) -> f32 {
        self.vol[0].y
    }

    pub fn event(&mut self, sfx: Sfx, pan: f32, gain: f32) {
        // the cursor sparkle and snow crunches can fire every frame: rate-limit
        let slot = match sfx {
            Sfx::Cursor => Some((0, 0.14)),
            Sfx::Crunch => Some((1, 0.11)),
            _ => None,
        };
        if let Some((i, gap)) = slot {
            if self.t - self.last[i] < gap {
                return;
            }
            self.last[i] = self.t;
        }
        if self.voices.len() > MAX_VOICES - 24 && matches!(sfx, Sfx::Cursor | Sfx::Crunch | Sfx::ShootingStar) {
            return;
        }
        let group = sfx.group();
        let g = if group.is_empty() { 1.0 } else { self.fx_vol.get(group) };
        if g <= 0.0 {
            return;
        }
        sfx::spawn(sfx, pan, gain * g, &mut self.rng, &mut self.voices);
    }

    /// Fill `out` with the next frames (post-limiter, within ±1).
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        for chunk in out.chunks_mut(BLOCK) {
            self.block(chunk);
        }
    }

    fn block(&mut self, out: &mut [[f32; 2]]) {
        let n = out.len();
        let (dry, rev) = (&mut self.dry[..n], &mut self.rev[..n]);
        dry.fill([0.0; 2]);
        rev.fill(0.0);
        for (i, v) in self.vol.iter_mut().enumerate() {
            let tgt = if i == 0 && !self.awake { 0.0 } else { self.target[i] };
            v.tick(tgt);
        }
        let (gm, gs, ga) = (self.vol[1].y, self.vol[2].y, self.vol[3].y);
        // music: keep playing while audible at all
        if self.target[1] > 0.0 && self.awake {
            self.music.render(n, &mut self.voices);
        }
        if let Some(g) = self.music.restarted.take() {
            self.voices.retain(|v| !(v.bus == Bus::Music && v.group == g && !v.is_fresh()));
            self.group[g as usize] = 1.0;
        }
        // group cross-fades (~2.5 s)
        let k = n as f32 / (0.8 * SR);
        for g in 1..3 {
            let tgt = self.music.fades[g];
            self.group[g] += (tgt - self.group[g]) * k.min(1.0);
            if tgt == 0.0 && self.group[g] < 1e-3 {
                self.voices.retain(|v| !(v.bus == Bus::Music && v.group == g as u8));
            }
        }
        self.amb.render(dry, rev, ga, &mut self.voices);
        let group = self.group;
        self.voices.retain_mut(|v| {
            let g = match v.bus {
                Bus::Music => gm * group[v.group as usize],
                Bus::Sfx => gs,
                Bus::Amb => ga,
            };
            v.render(dry, rev, g)
        });
        if self.voices.len() > MAX_VOICES {
            let extra = self.voices.len() - MAX_VOICES;
            self.voices.drain(0..extra);
        }
        let m = self.vol[0].y;
        for i in 0..n {
            let (mut dl, mut dr, mut sv) = (dry[i][0], dry[i][1], rev[i]);
            // guard: a NaN/inf must never reach the reverb, limiter or the
            // sound server (it would stick there)
            if !(dl.is_finite() && dr.is_finite() && sv.is_finite()) {
                (dl, dr, sv) = (0.0, 0.0, 0.0);
                self.nonfinite += 1;
            }
            let (wl, wr) = self.reverb.tick(sv);
            let l = self.dc[0].tick(dl + wl) * m;
            let r = self.dc[1].tick(dr + wr) * m;
            let (l, r) = self.lim.tick(l, r);
            out[i] = [l, r];
        }
        self.t += n as f64 / SR as f64;
    }
}
