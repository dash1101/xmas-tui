//! Sound: everything synthesized in code — no audio files, no samples.
//! - Ambience beds (wind, snowfall hiss, fireplace, distant chimes).
//! - Effects for scene events (presents, snowman, sleigh bells, bells,
//!   fireworks, shooting stars, the train…), queued by the scene as
//!   `SoundEvent`s and drained by the App each frame.
//! - Generated music: original pieces and public-domain carols in four
//!   arrangement styles.
//!
//! The App talks to a dedicated audio thread over a bounded channel; the
//! thread mixes 48 kHz stereo in 10 ms blocks and blocks on the output
//! (pw-cat/paplay pipe on Linux, waveOut on Windows), so the render loop is
//! never held up. If the front-end stops sending frames (paused, e.g. a
//! fullscreen game on Windows) the sound fades out and the output closes.

mod ambience;
mod carols;
mod dsp;
pub mod export;
mod instr;
mod mixer;
mod music;
mod out;
mod sfx;
mod voice;

pub use ambience::AmbState;
pub use music::STYLES;

use crate::config::SoundCfg;
use mixer::Mixer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Every effect the scene can ask for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sfx {
    Present,
    Snowman,
    ShootingStar,
    Comet,
    ChurchBell,
    ChurchPeal,
    Launch,
    Firework,
    Puff,
    Crunch,
    LightShow,
    Click,
    Lamp,
    Fanfare,
    TrainWhistle,
    Cursor,
    Sparkle,
    Starburst,
    Magic,
    Chime,
    // animals, people and handling them
    Deer,
    Fox,
    Rabbit,
    Munch,
    Pet,
    Greet,
    Laugh,
    Pickup,
    Drop,
    Snowball,
    Rustle,
    Bird,
    // clicked on: a car's horn, a plane's rumble, the owl
    Honk,
    Plane,
    Hoot,
    // pets
    Bark,
    Meow,
}

pub const ALL_SFX: [Sfx; 37] = [
    Sfx::Present,
    Sfx::Snowman,
    Sfx::ShootingStar,
    Sfx::Comet,
    Sfx::ChurchBell,
    Sfx::ChurchPeal,
    Sfx::Launch,
    Sfx::Firework,
    Sfx::Puff,
    Sfx::Crunch,
    Sfx::LightShow,
    Sfx::Click,
    Sfx::Lamp,
    Sfx::Fanfare,
    Sfx::TrainWhistle,
    Sfx::Cursor,
    Sfx::Sparkle,
    Sfx::Starburst,
    Sfx::Magic,
    Sfx::Chime,
    Sfx::Deer,
    Sfx::Fox,
    Sfx::Rabbit,
    Sfx::Munch,
    Sfx::Pet,
    Sfx::Greet,
    Sfx::Laugh,
    Sfx::Pickup,
    Sfx::Drop,
    Sfx::Snowball,
    Sfx::Rustle,
    Sfx::Bird,
    Sfx::Honk,
    Sfx::Plane,
    Sfx::Hoot,
    Sfx::Bark,
    Sfx::Meow,
];

impl Sfx {
    pub fn name(self) -> String {
        format!("{self:?}").to_lowercase()
    }

    /// The `[sound.effects]` volume this effect follows ("" = none).
    pub fn group(self) -> &'static str {
        match self {
            Sfx::Present => "presents",
            Sfx::Snowman => "snowman",
            Sfx::ShootingStar | Sfx::Comet => "stars",
            Sfx::ChurchBell | Sfx::ChurchPeal => "bells",
            Sfx::Launch | Sfx::Firework => "fireworks",
            Sfx::Puff | Sfx::Crunch => "snow",
            Sfx::LightShow | Sfx::Sparkle | Sfx::Starburst | Sfx::Magic => "sparkles",
            Sfx::Click | Sfx::Lamp => "switches",
            Sfx::Fanfare => "fanfare",
            Sfx::TrainWhistle | Sfx::Honk | Sfx::Plane => "train",
            Sfx::Cursor => "cursor",
            Sfx::Chime => "",
            Sfx::Deer | Sfx::Fox | Sfx::Rabbit | Sfx::Munch => "animals",
            Sfx::Pet | Sfx::Greet | Sfx::Laugh => "people",
            Sfx::Pickup | Sfx::Drop | Sfx::Snowball => "handling",
            Sfx::Rustle | Sfx::Bird => "bushes",
            Sfx::Hoot | Sfx::Bark | Sfx::Meow => "animals",
        }
    }
}

/// Live level meters: one per effect group (`EffectVolumes::ROWS`), then
/// other effects, the music and the ambience.
pub const METERS: usize = crate::config::EffectVolumes::ROWS.len() + 3;
pub const METER_OTHER: usize = METERS - 3;
pub const METER_MUSIC: usize = METERS - 2;
pub const METER_AMB: usize = METERS - 1;

/// What each meter is called.
pub fn meter_label(i: usize) -> &'static str {
    match i {
        _ if i < METER_OTHER => crate::config::EffectVolumes::ROWS[i].1,
        _ if i == METER_OTHER => "Other effects",
        _ if i == METER_MUSIC => "Music",
        _ => "Ambience",
    }
}

/// Which meter an effect shows on.
pub fn meter_of(sfx: Sfx) -> usize {
    let g = sfx.group();
    crate::config::EffectVolumes::ROWS.iter().position(|r| r.0 == g).unwrap_or(METER_OTHER)
}

/// Peak levels (0..1, before the master volume), refreshed by the audio
/// thread a couple of dozen times a second.
pub type Meters = Arc<std::sync::Mutex<[f32; METERS]>>;

/// A queued effect. The scene fills `pan` with the event's x in cells and
/// normalizes it to 0 (left) .. 1 (right) in `Scene::take_sounds`.
#[derive(Clone, Copy, Debug)]
pub struct SoundEvent {
    pub sfx: Sfx,
    pub pan: f32,
    pub gain: f32,
}

/// Names for the settings stepper: "shuffle" then each style.
pub fn track_names() -> Vec<&'static str> {
    std::iter::once("shuffle").chain(STYLES.iter().map(|s| s.name())).collect()
}

enum Msg {
    Events(Vec<SoundEvent>),
    State(AmbState),
    Config(SoundCfg),
}

/// Counters the audio thread keeps (for `--audio-test` and debugging).
#[derive(Default)]
pub struct Stats {
    pub frames: AtomicU64,
    pub opens: AtomicU64,
    pub failures: AtomicU64,
}

/// Handle to the audio thread. Dropping it stops the thread (and the
/// output child process).
pub struct Audio {
    tx: Option<SyncSender<Msg>>,
    /// What's playing and how loud (for the settings panel).
    pub meters: Meters,
    /// Disconnects when the audio thread has finished (output closed).
    done: Receiver<()>,
    pub stats: Arc<Stats>,
}

impl Audio {
    pub fn start(cfg: &SoundCfg) -> Option<Audio> {
        let (tx, rx) = sync_channel(256);
        let stats = Arc::new(Stats::default());
        let (cfg, st) = (cfg.clone(), stats.clone());
        let meters: Meters = Arc::new(std::sync::Mutex::new([0.0; METERS]));
        let m = meters.clone();
        let (done_tx, done) = sync_channel::<()>(1);
        std::thread::Builder::new()
            .name("xmas-audio".into())
            .spawn(move || {
                run(rx, cfg, st, m);
                drop(done_tx);
            })
            .ok()?;
        Some(Audio { tx: Some(tx), meters, done, stats })
    }

    /// Once per frame: this frame's effects plus the ambience state. Never
    /// blocks; if the audio thread is stuck, frames are simply dropped.
    pub fn frame(&self, events: Vec<SoundEvent>, state: AmbState) {
        let Some(tx) = &self.tx else { return };
        if !events.is_empty() {
            let _ = tx.try_send(Msg::Events(events));
        }
        let _ = tx.try_send(Msg::State(state));
    }

    /// Returns false if it couldn't be queued (try again next frame).
    pub fn config(&self, cfg: &SoundCfg) -> bool {
        match &self.tx {
            Some(tx) => !matches!(tx.try_send(Msg::Config(cfg.clone())), Err(TrySendError::Full(_))),
            None => true,
        }
    }
}

impl Drop for Audio {
    /// Stop: the thread fades out, closes the output and exits. Waits a
    /// moment for that (bounded, in case the sound server is stuck).
    fn drop(&mut self) {
        self.tx = None;
        let _ = self.done.recv_timeout(Duration::from_millis(300));
    }
}

/// Android: the app's AudioTrack takes what's been made (see `out::android`).
#[cfg(target_os = "android")]
pub fn take_android(out: &mut [i16]) -> usize {
    out::android::take(out)
}

fn audible(c: &SoundCfg) -> bool {
    c.enabled && c.master > 0.0 && (c.music > 0.0 || c.sfx > 0.0 || c.ambience > 0.0)
}

/// 10 ms blocks.
const FRAMES: usize = 480;

/// Float frames → interleaved s16.
fn to_pcm(buf: &[[f32; 2]], pcm: &mut [i16]) {
    for (i, f) in buf.iter().enumerate() {
        pcm[i * 2] = (f[0] * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
        pcm[i * 2 + 1] = (f[1] * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
    }
}

fn run(rx: Receiver<Msg>, mut cfg: SoundCfg, stats: Arc<Stats>, meters: Meters) {
    let mut meters_at = Instant::now();
    let debug = std::env::var_os("XMAS_AUDIO_DEBUG").is_some();
    let dev = out::Dev::from_env();
    let mut mx = Mixer::new(crate::app::random_seed());
    mx.configure(&cfg);
    let mut out: Option<out::Output> = None;
    let mut which = 0usize;
    let mut opened = Instant::now();
    let mut retry_at = Instant::now();
    let mut backoff = Duration::from_secs(1);
    let mut last_state = Instant::now();
    let mut quiet_since: Option<Instant> = None;
    let mut buf = vec![[0f32; 2]; FRAMES];
    let mut pcm = vec![0i16; FRAMES * 2];
    let log = |s: String| {
        if debug {
            eprintln!("xmas-tui audio: {s}");
        }
    };
    let mut title = String::new();
    loop {
        let mut msgs = vec![];
        loop {
            match rx.try_recv() {
                Ok(m) => msgs.push(m),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // shutting down: a quick fade so it doesn't click off
                    if let Some(o) = out.as_mut() {
                        mx.set_awake(false);
                        for _ in 0..8 {
                            mx.render(&mut buf);
                            to_pcm(&buf, &mut pcm);
                            if o.write(&pcm).is_err() {
                                break;
                            }
                        }
                    }
                    return;
                }
            }
        }
        let live = last_state.elapsed() < Duration::from_secs(1) && audible(&cfg);
        if out.is_none() && !(live && Instant::now() >= retry_at) && msgs.is_empty() {
            // idle: nothing to play (or waiting to retry) — sleep on the channel
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(m) => msgs.push(m),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        for m in msgs {
            match m {
                Msg::State(s) => {
                    last_state = Instant::now();
                    mx.set_state(s);
                }
                Msg::Events(evs) => {
                    if out.is_some() && live {
                        for e in evs {
                            mx.event(e.sfx, e.pan, e.gain);
                        }
                    }
                }
                Msg::Config(c) => {
                    cfg = c;
                    mx.configure(&cfg);
                }
            }
        }
        let live = last_state.elapsed() < Duration::from_secs(1) && audible(&cfg);
        mx.set_awake(live);
        if live {
            quiet_since = None;
        } else {
            let since = *quiet_since.get_or_insert_with(Instant::now);
            // let the fade finish, then let go of the sound device
            if out.is_some() && since.elapsed() > Duration::from_millis(1500) && mx.level() < 1e-4 {
                log("idle, closing output".into());
                out = None;
            }
        }
        if out.is_none() {
            if !live || Instant::now() < retry_at {
                continue;
            }
            match out::Output::open(which, &dev) {
                Ok(o) => {
                    log(format!("opened {}", o.name));
                    stats.opens.fetch_add(1, Ordering::Relaxed);
                    out = Some(o);
                    opened = Instant::now();
                }
                Err(e) => {
                    log(format!("can't open output #{which}: {e}"));
                    stats.failures.fetch_add(1, Ordering::Relaxed);
                    which = (which + 1) % out::BACKENDS;
                    retry_at = Instant::now() + backoff;
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    continue;
                }
            }
        }
        mx.render(&mut buf);
        to_pcm(&buf, &mut pcm);
        if meters_at.elapsed() >= Duration::from_millis(45) {
            meters_at = Instant::now();
            if let Ok(mut m) = meters.lock() {
                *m = if out.is_some() { mx.meters } else { [0.0; METERS] };
            }
        }
        if debug && mx.music.title != title {
            title = mx.music.title.clone();
            log(format!("now playing {title}"));
        }
        let Some(o) = out.as_mut() else { continue };
        match o.write(&pcm) {
            Ok(()) => {
                stats.frames.fetch_add(FRAMES as u64, Ordering::Relaxed);
                if opened.elapsed() > Duration::from_secs(10) {
                    backoff = Duration::from_secs(1);
                }
            }
            Err(e) => {
                log(format!("output {} failed: {e}", o.name));
                stats.failures.fetch_add(1, Ordering::Relaxed);
                // died right away? try the other backend next
                if opened.elapsed() < Duration::from_secs(3) {
                    which = (which + 1) % out::BACKENDS;
                }
                out = None;
                retry_at = Instant::now() + backoff;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

/// `--audio-test SECS`: run the real output path headless, firing every
/// effect and a sleigh pass, then report how much audio the output took.
/// Keep it inaudible with XMAS_AUDIO_DEV_VOLUME=0 (silent samples) and/or
/// XMAS_AUDIO_DEV_TARGET=<a null sink>.
pub fn run_test(secs: f32, cfg: &SoundCfg) {
    let Some(a) = Audio::start(cfg) else {
        eprintln!("audio-test: couldn't start the audio thread");
        return;
    };
    let start = Instant::now();
    let mut k = 0;
    let mut next_fx = 0.5;
    while start.elapsed().as_secs_f32() < secs {
        let t = start.elapsed().as_secs_f32();
        let mut evs = vec![];
        if t > next_fx {
            evs.push(SoundEvent { sfx: ALL_SFX[k % ALL_SFX.len()], pan: (k % 5) as f32 / 4.0, gain: 1.0 });
            k += 1;
            next_fx += 0.4;
        }
        let sleigh = ((t % 8.0) < 5.0).then(|| (1.0 - (t % 8.0) / 5.0, 1.0));
        a.frame(evs, AmbState { wind: 0.6 + 0.4 * (t * 0.5).sin(), snow: 1.0, sleigh, train: Some((0.3, 1.0)), aurora: 0.5 });
        std::thread::sleep(Duration::from_millis(33));
    }
    let el = start.elapsed().as_secs_f64();
    let frames = a.stats.frames.load(Ordering::Relaxed);
    println!(
        "audio-test: {:.1} s, output took {} frames = {:.2} s of audio ({:.0} frames/s), {} open(s), {} failure(s), {} effects fired",
        el,
        frames,
        frames as f64 / out::RATE as f64,
        frames as f64 / el,
        a.stats.opens.load(Ordering::Relaxed),
        a.stats.failures.load(Ordering::Relaxed),
        k
    );
}
