//! `--export-audio DIR`: render every effect, the travelling sounds, the
//! ambience and each music style to 32-bit float WAV files — exactly what
//! would go to the speakers at the configured volumes, but nothing is
//! played.
//! Check them with `scripts/check-audio.py DIR`.

use super::ambience::AmbState;
use super::dsp::SR;
use super::mixer::Mixer;
use super::music::STYLES;
use super::{carols, Sfx, ALL_SFX};
use crate::config::SoundCfg;
use std::io::Write;
use std::path::Path;

fn write_wav(path: &Path, frames: &[[f32; 2]]) -> std::io::Result<()> {
    let data = (frames.len() * 8) as u32;
    let mut b: Vec<u8> = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&(SR as u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for f in frames {
        b.extend_from_slice(&f[0].to_le_bytes());
        b.extend_from_slice(&f[1].to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&b)
}

/// Render `secs` of audio, calling `drive(mixer, t)` every 10 ms.
fn render(mx: &mut Mixer, secs: f32, mut drive: impl FnMut(&mut Mixer, f32)) -> Vec<[f32; 2]> {
    let n = (secs * SR) as usize;
    let mut out = vec![[0.0f32; 2]; n];
    let block = 480;
    let mut i = 0;
    while i < n {
        drive(mx, i as f32 / SR);
        let end = (i + block).min(n);
        mx.render(&mut out[i..end]);
        i = end;
    }
    out
}

fn mixer(cfg: &SoundCfg, music: bool, sfx: bool, amb: bool) -> Mixer {
    let mut mx = Mixer::new(0x5eed);
    let c = SoundCfg {
        music: if music { cfg.music } else { 0.0 },
        sfx: if sfx { cfg.sfx } else { 0.0 },
        ambience: if amb { cfg.ambience } else { 0.0 },
        ..cfg.clone()
    };
    mx.configure(&c);
    mx.snap_volumes();
    mx.set_state(AmbState::default());
    mx
}

/// Renders at the given `[sound]` volumes (`enabled` and `track` are ignored).
pub fn export_all(dir: &str, cfg: &SoundCfg) -> std::io::Result<()> {
    let dir = Path::new(dir);
    std::fs::create_dir_all(dir)?;
    let cfg = SoundCfg { enabled: true, ..cfg.clone() };
    let mut files = vec![];
    let mut save = |name: String, mx: &Mixer, frames: Vec<[f32; 2]>| -> std::io::Result<()> {
        write_wav(&dir.join(format!("{name}.wav")), &frames)?;
        files.push((name, frames.len() as f32 / SR, mx.nonfinite));
        Ok(())
    };
    // every effect on its own, centred
    for sfx in ALL_SFX {
        let secs = match sfx {
            Sfx::ChurchPeal | Sfx::Comet => 14.0,
            Sfx::ChurchBell | Sfx::Lamp => 8.0,
            _ => 5.0,
        };
        let mut mx = mixer(&cfg, false, true, false);
        let frames = render(&mut mx, secs, |m, t| {
            if t == 0.0 {
                m.event(sfx, 0.5, 1.0)
            }
        });
        save(format!("sfx_{}", sfx.name()), &mx, frames)?;
    }
    // Santa's sleigh crossing right → left, and the distant train
    let mut mx = mixer(&cfg, false, true, false);
    let frames = render(&mut mx, 16.0, |m, t| {
        let x = 1.15 - t / 12.0;
        let g = (1.0 - ((x - 0.5).abs() - 0.5).max(0.0) / 0.15).clamp(0.0, 1.0);
        m.set_state(AmbState { sleigh: (t < 13.0).then_some((x.clamp(0.0, 1.0), g)), ..Default::default() })
    });
    save("travel_sleigh".into(), &mx, frames)?;
    let mut mx = mixer(&cfg, false, true, false);
    let frames = render(&mut mx, 20.0, |m, t| {
        if t == 0.0 {
            m.event(Sfx::TrainWhistle, 0.0, 1.0);
        }
        m.set_state(AmbState { train: (t < 17.0).then_some((t / 17.0, 1.0)), ..Default::default() })
    });
    save("travel_train".into(), &mx, frames)?;
    // ambience: calm, a gust, a blizzard, an aurora flare
    let mut mx = mixer(&cfg, false, false, true);
    let frames = render(&mut mx, 30.0, |m, t| {
        let gust = if (8.0..14.0).contains(&t) { 0.9 } else { 0.0 };
        let bliz = if (18.0..26.0).contains(&t) { 1.0 } else { 0.0 };
        m.set_state(AmbState { wind: 0.35 + gust + bliz * 1.6, snow: 1.0 + bliz, aurora: if t > 22.0 { 1.0 } else { 0.0 }, ..Default::default() })
    });
    save("ambience".into(), &mx, frames)?;
    // each music style: an original piece and a carol
    let carol_for = |s| match s {
        super::music::Style::MusicBox => 0,
        super::music::Style::Celesta => 3,
        super::music::Style::Piano => 6,
        super::music::Style::SleighRide => 1,
    };
    for style in STYLES {
        for carol in [None, Some(carol_for(style))] {
            let mut mx = mixer(&cfg, true, false, false);
            mx.music.start(style, carol);
            let frames = render(&mut mx, 30.0, |_, _| {});
            let kind = if carol.is_some() { "carol" } else { "original" };
            save(format!("music_{}_{kind}", style.name()), &mx, frames)?;
        }
    }
    // every carol once (music box), to catch harmonization slips
    for (i, t) in carols::TUNES.iter().enumerate() {
        let mut mx = mixer(&cfg, true, false, false);
        mx.music.start(STYLES[i % 4], Some(i));
        let frames = render(&mut mx, 30.0, |_, _| {});
        let slug: String = t.name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        save(format!("carol_{slug}"), &mx, frames)?;
    }
    // the whole lot at default settings, with a busy minute's worth of events
    let mut mx = mixer(&cfg, true, true, true);
    mx.music.start(STYLES[1], None);
    let frames = render(&mut mx, 40.0, |m, t| {
        let k = (t * 100.0).round() as i32;
        if k % 300 == 150 {
            let sfx = ALL_SFX[((k / 300) as usize * 7) % ALL_SFX.len()];
            m.event(sfx, (k % 700) as f32 / 700.0, 1.0);
        }
        let sleigh = (10.0..22.0).contains(&t).then(|| (1.0 - (t - 10.0) / 12.0, 1.0));
        m.set_state(AmbState { wind: 0.4, snow: 1.0, sleigh, ..Default::default() })
    });
    save("mix_default".into(), &mx, frames)?;
    for (name, secs, bad) in &files {
        eprintln!("  {name:<34} {secs:5.1} s{}", if *bad > 0 { format!("  ({bad} non-finite samples zeroed!)") } else { String::new() });
    }
    eprintln!("exported {} WAV files to {}", files.len(), dir.display());
    Ok(())
}
