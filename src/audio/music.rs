//! Generated ambient Christmas music. A piece is either an original tune
//! composed on the spot (seeded chord progression + melody in AA'BA' form)
//! or one of the public-domain carols in `carols.rs`, harmonized
//! automatically. Either way it's then arranged for one of four styles and
//! played through the mixer, with gentle overlaps/fades between pieces.

use super::carols::{self, TUNES};
use super::dsp::SR;
use super::instr::{jingle_shake, note, Instr};
use super::voice::{Bus, Voice};
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    MusicBox,
    Celesta,
    Piano,
    SleighRide,
}

pub const STYLES: [Style; 4] = [Style::MusicBox, Style::Celesta, Style::Piano, Style::SleighRide];

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::MusicBox => "music-box",
            Style::Celesta => "celesta",
            Style::Piano => "piano",
            Style::SleighRide => "sleigh-ride",
        }
    }
    /// "shuffle" (or anything unknown) → None.
    pub fn parse(s: &str) -> Option<Style> {
        STYLES.iter().copied().find(|st| st.name() == s.trim().to_lowercase())
    }
}

/// A melody with its harmony, in beats.
pub struct Mel {
    /// (start, length, MIDI note)
    pub notes: Vec<(f32, f32, i32)>,
    pub chords: Vec<Chord>,
    pub bar: f32,
    pub pickup: f32,
    pub len: f32,
}

#[derive(Clone)]
pub struct Chord {
    pub at: f32,
    pub len: f32,
    /// Pitch class of the root.
    pub root: i32,
    /// Semitones above the root (root, third, fifth[, seventh]).
    pub tones: Vec<i32>,
}

/// One scheduled note of an arranged piece, in seconds.
#[derive(Clone, Copy, Debug)]
pub struct NoteEv {
    pub at: f32,
    pub dur: f32,
    pub midi: f32,
    pub vel: f32,
    pub instr: Instr,
    pub pan: f32,
}

// ---------------------------------------------------------------------
// harmonizing a carol

/// Diatonic chord candidates in a major key: (root above tonic, chord
/// tones above root, prior preference).
const CANDIDATES: [(i32, &[i32], f32); 8] = [
    (0, &[0, 4, 7], 0.35),
    (5, &[0, 4, 7], 0.25),
    (7, &[0, 4, 7], 0.3),
    (7, &[0, 4, 7, 10], 0.1),
    (9, &[0, 3, 7], 0.12),
    (2, &[0, 3, 7], 0.08),
    (4, &[0, 3, 7], 0.0),
    (2, &[0, 4, 7], -0.1), // V of V, for raised fourths
];

pub fn harmonize(t: &carols::Tune) -> Mel {
    let notes = carols::parse(t);
    let len = notes.last().map(|n| n.0 + n.1).unwrap_or(0.0);
    let seg = if t.bar == 4.0 { 2.0 } else { t.bar };
    let mut chords: Vec<Chord> = vec![];
    let mut at = t.pickup;
    let mut prev: Option<usize> = None;
    let n_segs = ((len - t.pickup) / seg).ceil() as usize;
    for si in 0..n_segs {
        let end = (at + seg).min(len);
        let mut best = (f32::MIN, 0usize);
        for (ci, (root, tones, prior)) in CANDIDATES.iter().enumerate() {
            let mut score = *prior;
            for &(nat, nlen, m) in &notes {
                let (s, e) = (nat.max(at), (nat + nlen).min(end));
                if e <= s {
                    continue;
                }
                let rel = (m - t.tonic - root).rem_euclid(12);
                let strong = (nat - at).abs() < 0.01 || ((nat - t.pickup) % t.bar).abs() < 0.01;
                let w = (e - s) * if strong { 2.0 } else { 1.0 };
                score += if tones.contains(&rel) { w } else { -0.6 * w };
            }
            if prev == Some(ci) {
                score += 0.25;
            }
            if si + 1 == n_segs {
                score += if ci == 0 { 5.0 } else { 0.0 };
            } else if si + 2 == n_segs && (*root == 7) {
                score += 0.5;
            }
            if score > best.0 {
                best = (score, ci);
            }
        }
        let (root, tones, _) = CANDIDATES[best.1];
        let root = (t.tonic + root).rem_euclid(12);
        match chords.last_mut() {
            Some(c) if prev == Some(best.1) && c.root == root => c.len += end - at,
            _ => chords.push(Chord { at, len: end - at, root, tones: tones.to_vec() }),
        }
        prev = Some(best.1);
        at = end;
    }
    Mel { notes, chords, bar: t.bar, pickup: t.pickup, len }
}

// ---------------------------------------------------------------------
// composing an original tune

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

const PROG_A: [[i32; 8]; 6] = [
    [0, 5, 3, 4, 0, 5, 3, 4],
    [0, 3, 0, 4, 0, 3, 4, 0],
    [0, 2, 5, 3, 1, 4, 0, 4],
    [0, 4, 5, 2, 3, 0, 3, 4],
    [5, 3, 0, 4, 5, 3, 1, 4],
    [0, 0, 3, 3, 1, 4, 0, 4],
];
const PROG_B: [[i32; 8]; 3] = [[3, 4, 2, 5, 1, 4, 0, 4], [3, 0, 3, 4, 5, 1, 4, 4], [5, 2, 3, 0, 1, 1, 4, 4]];

fn rhythms(bar: f32, end: bool) -> &'static [&'static [f32]] {
    match (bar as i32, end) {
        (3, false) => &[&[1.0, 1.0, 1.0], &[2.0, 1.0], &[1.0, 2.0], &[1.5, 0.5, 1.0], &[1.0, 0.5, 0.5, 1.0], &[0.5, 0.5, 1.0, 1.0]],
        (3, true) => &[&[3.0], &[1.0, 2.0], &[2.0, 1.0]],
        (_, false) => &[
            &[1.0, 1.0, 1.0, 1.0],
            &[2.0, 1.0, 1.0],
            &[1.0, 1.0, 2.0],
            &[1.5, 0.5, 1.0, 1.0],
            &[1.5, 0.5, 2.0],
            &[0.5, 0.5, 1.0, 2.0],
            &[1.0, 0.5, 0.5, 1.0, 1.0],
            &[2.0, 2.0],
        ],
        (_, true) => &[&[2.0, 2.0], &[1.0, 1.0, 2.0], &[4.0], &[1.0, 3.0]],
    }
}

pub fn compose(style: Style, rng: &mut Rng) -> (Mel, f32) {
    let minor = style != Style::SleighRide && rng.chance(0.2);
    let bar: f32 = if style == Style::SleighRide || rng.chance(0.5) { 4.0 } else { 3.0 };
    let tonic = rng.int(0, 12);
    let scale = if minor { MINOR } else { MAJOR };
    let pa = *rng.pick(&PROG_A);
    let pb = *rng.pick(&PROG_B);
    let mut a2 = pa;
    a2[6] = 4;
    a2[7] = 0;
    // chord on scale degree d: stacked thirds (minor keys use a major V)
    let chord_pcs = |d: i32| -> Vec<i32> {
        let mut v: Vec<i32> = [0, 2, 4].iter().map(|k| scale[((d + k) % 7) as usize]).collect();
        if minor && d == 4 {
            v[1] += 1;
        }
        v
    };
    let mut chords = vec![];
    for (si, prog) in [pa, a2, pb, a2].iter().enumerate() {
        for (bi, &d) in prog.iter().enumerate() {
            let at = ((si * 8 + bi) as f32) * bar;
            let pcs = chord_pcs(d);
            let root = (tonic + pcs[0]).rem_euclid(12);
            chords.push(Chord { at, len: bar, root, tones: pcs.iter().map(|p| (p - pcs[0]).rem_euclid(12)).collect() });
        }
    }
    let deg_midi = |d: i32| 60 + tonic + scale[d.rem_euclid(7) as usize] + 12 * d.div_euclid(7);
    let is_chord_tone = |deg: i32, cd: i32| [0, 2, 4].iter().any(|k| (cd + k).rem_euclid(7) == deg.rem_euclid(7));
    // one 8-bar phrase of melody over the given chord degrees
    let phrase = |rng: &mut Rng, prog: &[i32; 8], start: &mut i32, cadence: bool| -> Vec<(f32, f32, i32)> {
        let mut out = vec![];
        let mut cur = *start;
        for (bi, &cd) in prog.iter().enumerate() {
            let end = bi == 3 || bi == 7;
            let pat = *rng.pick(rhythms(bar, end));
            let pat: &[f32] = if cadence && bi == 7 { &[bar] } else { pat };
            let mut pos = 0.0;
            for (ni, &len) in pat.iter().enumerate() {
                let strong = ni == 0 || (bar == 4.0 && (pos - 2.0f32).abs() < 0.01);
                if cadence && bi == 7 {
                    cur = if (cur - 7).abs() < cur.abs() { 7 } else { 0 };
                } else if strong {
                    // nearest chord tones, a little random
                    let mut cands: Vec<(i32, f32)> = ((cur - 4)..=(cur + 4)).filter(|&d| is_chord_tone(d, cd) && (-3..=10).contains(&d)).map(|d| (d, 1.0 / (1.0 + (d - cur).abs() as f32) + if d == cur { -0.15 } else { 0.0 })).collect();
                    if cands.is_empty() {
                        cands.push((cd, 1.0));
                    }
                    let total: f32 = cands.iter().map(|c| c.1.max(0.01)).sum();
                    let mut r = rng.f() * total;
                    for (d, w) in &cands {
                        r -= w.max(0.01);
                        if r <= 0.0 {
                            cur = *d;
                            break;
                        }
                    }
                } else {
                    let up = if cur > 7 { 0.3 } else if cur < 1 { 0.7 } else { 0.5 };
                    let step = if rng.chance(0.15) { 2 } else if rng.chance(0.1) { 0 } else { 1 };
                    cur += if rng.chance(up) { step } else { -step };
                    cur = cur.clamp(-3, 10);
                }
                out.push(((bi as f32) * bar + pos, len, deg_midi(cur)));
                pos += len;
            }
        }
        *start = cur;
        out
    };
    let mut cur = 2;
    let a = phrase(rng, &pa, &mut cur, false);
    let mut cur2 = cur;
    let a_end = phrase(rng, &a2, &mut cur2, true);
    // A' = A's first six bars, then a cadence onto the tonic
    let mut a_prime: Vec<(f32, f32, i32)> = a.iter().filter(|n| n.0 < 6.0 * bar).copied().collect();
    a_prime.extend(a_end.iter().filter(|n| n.0 >= 6.0 * bar).copied());
    let mut cur3 = 4;
    let b = phrase(rng, &pb, &mut cur3, false);
    let mut notes = vec![];
    for (si, sec) in [&a, &a_prime, &b, &a_prime].iter().enumerate() {
        let off = si as f32 * 8.0 * bar;
        notes.extend(sec.iter().map(|&(at, l, m)| (at + off, l, m)));
    }
    let tempo = match style {
        Style::MusicBox => rng.range(76.0, 92.0),
        Style::Celesta => rng.range(68.0, 82.0),
        Style::Piano => rng.range(62.0, 74.0),
        Style::SleighRide => rng.range(118.0, 132.0),
    };
    (Mel { notes, chords, bar, pickup: 0.0, len: 32.0 * bar }, tempo)
}

// ---------------------------------------------------------------------
// arranging

/// Lowest MIDI note ≥ `lo` with pitch class `pc`.
fn near(pc: i32, lo: i32) -> i32 {
    lo + (pc - lo).rem_euclid(12)
}

pub fn arrange(mel: &Mel, style: Style, tempo: f32, rng: &mut Rng) -> Vec<NoteEv> {
    let spb = 60.0 / tempo;
    let bar = mel.bar;
    // an intro bar of accompaniment before the tune (completing the pickup)
    let intro = if mel.pickup > 0.0 { bar - mel.pickup } else { bar };
    let total = intro + mel.len + bar;
    // gentle ritardando over the last two bars
    let r0 = (total - 2.0 * bar).max(0.0);
    let sec = |b: f32| -> f32 {
        if b <= r0 {
            b * spb
        } else {
            let x = b - r0;
            r0 * spb + spb * (x + 0.3 * x * x / (2.0 * (total - r0)))
        }
    };
    // transpose melody into the style's register
    let target = match style {
        Style::MusicBox => 79.0,
        Style::Celesta => 76.0,
        Style::Piano => 70.0,
        Style::SleighRide => 74.0,
    };
    let key_shift = rng.int(-3, 4);
    let mean = mel.notes.iter().map(|n| n.2 as f32).sum::<f32>() / mel.notes.len().max(1) as f32 + key_shift as f32;
    let oct = ((target - mean) / 12.0).round() as i32 * 12;
    let mshift = key_shift + oct;
    let mut out = vec![];
    let push = |out: &mut Vec<NoteEv>, b: f32, dur_b: f32, midi: i32, vel: f32, instr: Instr, pan: f32, jitter: f32| {
        let at = (sec(b) + jitter).max(0.0);
        let dur = (sec(b + dur_b) - sec(b)).max(0.05);
        out.push(NoteEv { at, dur, midi: midi as f32, vel, instr, pan });
    };
    // melody
    let human = if style == Style::Piano { 0.015 } else { 0.006 };
    for &(at, len, m) in &mel.notes {
        let b = intro + at;
        let strong = ((at - mel.pickup).rem_euclid(bar)) < 0.01;
        let vel = (if strong { 0.8 } else { 0.68 }) + rng.range(-0.05, 0.05);
        let m = m + mshift;
        let j = rng.range(-human, human);
        match style {
            Style::MusicBox => push(&mut out, b, len, m, vel, Instr::MusicBox, 0.5, j),
            Style::Celesta => push(&mut out, b, len, m, vel * 0.95, Instr::Celesta, 0.55, j),
            Style::Piano => push(&mut out, b, len, m, vel * 0.8, Instr::Piano, 0.55, j),
            Style::SleighRide => {
                push(&mut out, b, len, m, vel * 0.85, Instr::Celesta, 0.5, j);
                push(&mut out, b, len, m + 12, vel * 0.32, Instr::Glock, 0.6, j);
            }
        }
    }
    // accompaniment, chord by chord (the intro uses the first full chord)
    let mut chords: Vec<Chord> = mel.chords.iter().map(|c| Chord { at: c.at + intro, ..c.clone() }).collect();
    if let Some(first) = chords.first().cloned() {
        chords.insert(0, Chord { at: 0.0, len: first.at, ..first });
    }
    if let Some(last) = chords.last_mut() {
        last.len = total - last.at;
    }
    let ks = key_shift;
    for (ci, c) in chords.iter().enumerate() {
        let root = c.root + ks;
        let last = ci + 1 == chords.len();
        let beats = c.len.round().max(1.0) as i32;
        let tone = |k: usize| c.tones.get(k).copied().unwrap_or(7);
        match style {
            Style::MusicBox => {
                let r = near(root, 55);
                let pat = [r, r + tone(2), r + 12 + tone(1), r + tone(2)];
                let sub = if spb > 0.55 { 2 } else { 1 };
                push(&mut out, c.at, 1.0, near(root, 43), 0.4, Instr::MusicBox, 0.45, 0.0);
                let steps = if last { 4 } else { beats * sub };
                for k in 0..steps {
                    let b = c.at + k as f32 / sub as f32;
                    push(&mut out, b, 1.0 / sub as f32, pat[k as usize % 4] + if last { 12 * (k / 4) } else { 0 }, 0.34, Instr::MusicBox, 0.45 + 0.1 * (k % 2) as f32, 0.0);
                }
            }
            Style::Celesta => {
                for (k, &tn) in c.tones.iter().take(3).enumerate() {
                    push(&mut out, c.at, c.len, near(root + tn, 55), 0.42, Instr::Pad, 0.3 + 0.2 * k as f32, 0.0);
                }
                push(&mut out, c.at, c.len, near(root, 38), 0.4, Instr::Pad, 0.5, 0.0);
                for k in 1..beats {
                    if k % 2 == 1 || bar == 3.0 {
                        let m = near(root + tone(k as usize % 3), 67);
                        push(&mut out, c.at + k as f32, 1.0, m, 0.22, Instr::Celesta, 0.35, 0.0);
                    }
                }
                if rng.chance(0.3) && !last {
                    push(&mut out, c.at, 1.0, near(root + tone(1), 84), 0.18, Instr::Glock, 0.7, 0.0);
                }
            }
            Style::Piano => {
                let r = near(root, 36);
                push(&mut out, c.at, c.len, r, 0.42, Instr::Piano, 0.4, 0.0);
                let pat: Vec<i32> = if bar == 3.0 {
                    vec![r + 12 + tone(1), r + 12 + tone(2)]
                } else {
                    vec![r + tone(2), r + 12 + tone(1), r + tone(2) + 12, r + 12 + tone(1)]
                };
                let sub = if bar == 3.0 || bar == 6.0 { 1.0 } else { 0.5 };
                let mut b = c.at + if bar == 3.0 { 1.0 } else { sub };
                let mut k = 0;
                while b < c.at + c.len - 0.01 && !(last && k > 3) {
                    let held = c.at + c.len - b;
                    push(&mut out, b, held, pat[k % pat.len()], 0.26, Instr::Piano, 0.42, rng.range(-0.01, 0.01));
                    b += sub;
                    k += 1;
                }
            }
            Style::SleighRide => {
                for k in 0..beats {
                    let b = c.at + k as f32;
                    let down = if bar == 3.0 { k % 3 == 0 } else { k % 2 == 0 };
                    if down {
                        let m = near(root + if (k / 2) % 2 == 1 { tone(2) } else { 0 }, 36);
                        push(&mut out, b, 0.9, m, 0.5, Instr::Bass, 0.5, 0.0);
                        push(&mut out, b, 0.2, 72, 0.22, Instr::Clop, 0.42, 0.0);
                    } else {
                        for (t, &tn) in c.tones.iter().take(3).enumerate() {
                            push(&mut out, b, 0.4, near(root + tn, 57), 0.3, Instr::Pluck, 0.4 + 0.1 * t as f32, 0.0);
                        }
                        push(&mut out, b, 0.2, 77, 0.2, Instr::Clop, 0.58, 0.0);
                    }
                    for h in 0..2 {
                        let acc = if h == 0 { 0.32 } else { 0.18 };
                        push(&mut out, b + h as f32 * 0.5, 0.25, 0, acc, Instr::Jingle, 0.62, 0.0);
                    }
                    if last {
                        break;
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    out
}

// ---------------------------------------------------------------------
// the player

struct Piece {
    notes: Vec<NoteEv>,
    idx: usize,
    t: f64,
    group: u8,
    style: Style,
}

pub struct Music {
    rng: Rng,
    /// None = shuffle.
    pref: Option<Style>,
    cur: Option<Piece>,
    gap: f64,
    group: u8,
    last_tune: Option<usize>,
    last_style: Option<Style>,
    /// Group fade targets the mixer should apply (index = group).
    pub fades: [f32; 3],
    /// A group that was just (re)started: the mixer drops its old voices.
    pub restarted: Option<u8>,
    /// What's playing, for logs.
    pub title: String,
}

impl Music {
    pub fn new(seed: u32) -> Self {
        Music {
            rng: Rng::new(seed as u64 ^ 0x3051c),
            pref: None,
            cur: None,
            gap: 2.5,
            group: 1,
            last_tune: None,
            last_style: None,
            fades: [1.0; 3],
            restarted: None,
            title: String::new(),
        }
    }

    pub fn set_pref(&mut self, pref: Option<Style>) {
        if pref == self.pref {
            return;
        }
        self.pref = pref;
        if let (Some(p), Some(cur)) = (pref, &self.cur) {
            if cur.style != p {
                // fade the current piece out; the new style starts shortly
                self.fades[cur.group as usize] = 0.0;
                self.cur = None;
                self.gap = 1.2;
            }
        }
    }

    /// Start a specific piece now (used by the WAV export).
    pub fn start(&mut self, style: Style, carol: Option<usize>) {
        if let Some(c) = &self.cur {
            self.fades[c.group as usize] = 0.0;
        }
        let (mel, tempo, title) = match carol {
            Some(i) => {
                let t = &TUNES[i % TUNES.len()];
                let tempo = self.rng.range(t.tempo.0, t.tempo.1)
                    * match style {
                        Style::Piano => 0.85,
                        Style::Celesta => 0.9,
                        _ => 1.0,
                    };
                (harmonize(t), tempo, t.name.to_string())
            }
            None => {
                let (m, tempo) = compose(style, &mut self.rng);
                (m, tempo, "an original tune".to_string())
            }
        };
        let mut notes = arrange(&mel, style, tempo, &mut self.rng);
        // short tunes play twice, the second time an octave brighter
        let one = notes.iter().map(|n| n.at + n.dur).fold(0.0, f32::max);
        if one < 40.0 {
            let off = one + 60.0 / tempo;
            let up = if matches!(style, Style::Piano | Style::Celesta) { 12.0 } else { 0.0 };
            let melody = |i: Instr| matches!(i, Instr::Piano | Instr::Celesta | Instr::MusicBox);
            let second: Vec<NoteEv> = notes.iter().map(|n| NoteEv { at: n.at + off, midi: if melody(n.instr) && n.midi > 66.0 { n.midi + up } else { n.midi }, ..*n }).collect();
            notes.extend(second);
        }
        self.group = if self.group == 1 { 2 } else { 1 };
        self.fades[self.group as usize] = 1.0;
        self.restarted = Some(self.group);
        self.title = format!("{title} ({})", style.name());
        self.cur = Some(Piece { notes, idx: 0, t: 0.0, group: self.group, style });
        self.last_style = Some(style);
    }

    fn next_piece(&mut self) {
        let style = self.pref.unwrap_or_else(|| {
            let mut s = *self.rng.pick(&STYLES);
            if Some(s) == self.last_style && self.rng.chance(0.7) {
                s = *self.rng.pick(&STYLES);
            }
            s
        });
        let carol = if self.rng.chance(0.5) {
            let ok: Vec<usize> = (0..TUNES.len()).filter(|&i| (style != Style::SleighRide || TUNES[i].lively) && Some(i) != self.last_tune).collect();
            ok.get(self.rng.int(0, ok.len() as i32) as usize).copied()
        } else {
            None
        };
        if carol.is_some() {
            self.last_tune = carol;
        }
        self.start(style, carol);
    }

    /// Advance by `n` samples, spawning the notes that fall in this block.
    pub fn render(&mut self, n: usize, voices: &mut Vec<Voice>) {
        let dt = n as f64 / SR as f64;
        if self.cur.is_none() {
            self.gap -= dt;
            if self.gap <= 0.0 {
                self.next_piece();
            }
            return;
        }
        let Some(p) = self.cur.as_mut() else { return };
        let t1 = p.t + dt;
        while p.idx < p.notes.len() && (p.notes[p.idx].at as f64) < t1 {
            let ne = p.notes[p.idx];
            p.idx += 1;
            let delay = (ne.at as f64 - p.t).max(0.0) as f32;
            let seed = self.rng.next_u32();
            let start = voices.len();
            if ne.instr == Instr::Jingle {
                jingle_shake(voices, Bus::Music, delay, ne.vel, ne.pan, seed);
            } else {
                voices.push(note(ne.instr, ne.midi, ne.dur, ne.vel, seed).pan(ne.pan).at(delay));
            }
            for v in &mut voices[start..] {
                v.group = p.group;
            }
        }
        p.t = t1;
        if p.idx >= p.notes.len() {
            // let the last notes ring, then a short breath before the next
            self.cur = None;
            self.gap = self.rng.range(3.0, 6.0) as f64;
        }
    }
}
