//! The visualizer's data: what's playing on the default output, heard
//! through a loopback capture and turned into levels in `BANDS`
//! log-spaced bands (40 Hz to 16 kHz), about 60 times a second.
//! - Linux: the default sink's monitor, through a `pw-record` child
//!   (PipeWire), falling back to `parec` (PulseAudio). Nothing is linked
//!   in; it's the same approach as the sound output (out.rs).
//! - Windows: WASAPI loopback on the default render device (followed when
//!   it changes).
//!
//! It only listens: nothing is stored or sent anywhere, and the capture
//! stops (the child exits) as soon as the widget is switched off.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const BANDS: usize = 64;
const RATE: u32 = 48_000;
/// FFT size (85 ms at 48 kHz: enough resolution for the bass bands).
const N: usize = 4096;
/// A new analysis every 800 samples (60 a second).
const HOP: usize = 800;
const LO_HZ: f32 = 40.0;
const HI_HZ: f32 = 16_000.0;
/// Treble boost, dB per octave above 1 kHz (and cut below), so a typical
/// mix comes out roughly level across the bars instead of all bass.
const TILT: f32 = 1.5;
/// The span of the bars, dB below the loudest band of late, and a curve on
/// top so the bars move more (tuned on real music: busy mixes would
/// otherwise sit near the top).
const RANGE: f32 = 36.0;
const GAMMA: f32 = 1.3;
/// Below this (dBFS, any band) it counts as silence.
const QUIET_DB: f32 = -80.0;

/// The centre frequency (Hz) of bands a..b.
pub fn centre_hz(a: usize, b: usize) -> f32 {
    let f = |i: usize| LO_HZ * (HI_HZ / LO_HZ).powf(i as f32 / BANDS as f32);
    (f(a) * f(b)).sqrt()
}

/// A frequency in kHz, in at most 3 characters: .04, .37, 1.4, 12.
pub fn khz_label(hz: f32) -> String {
    let k = hz / 1000.0;
    if k < 0.995 {
        let s = format!("{k:.2}");
        s.trim_start_matches('0').to_string()
    } else if k < 9.95 {
        format!("{k:.1}")
    } else {
        format!("{k:.0}")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    /// 0..1 per band: the loudest since the widget last took them.
    pub bands: Vec<f32>,
    /// New levels since the widget last took them.
    pub fresh: bool,
    /// Listening right now.
    pub live: bool,
    /// Why it can't listen (empty while it can, or while it's starting).
    pub status: String,
    /// Seconds of silence so far (0 while there's sound).
    pub quiet_for: f32,
}

impl Spectrum {
    /// The latest levels into `into` (and false when there were none new).
    pub fn take(&mut self, into: &mut [f32]) -> bool {
        if !self.fresh {
            return false;
        }
        for (o, b) in into.iter_mut().zip(&self.bands) {
            *o = *b;
        }
        self.fresh = false;
        true
    }
}

/// Start listening. The thread (and the capture) stops once `gen` moves on
/// from `mine`.
pub fn spawn(gen: Arc<AtomicU64>, mine: u64) -> Arc<Mutex<Spectrum>> {
    let out = Arc::new(Mutex::new(Spectrum { bands: vec![0.0; BANDS], ..Default::default() }));
    let o = out.clone();
    let _ = std::thread::Builder::new().name("visualizer".into()).spawn(move || run(o, gen, mine));
    out
}

fn run(out: Arc<Mutex<Spectrum>>, gen: Arc<AtomicU64>, mine: u64) {
    let alive = || gen.load(Ordering::SeqCst) == mine;
    let mut an = Analyzer::new();
    let mut which = 0usize;
    let mut buf = vec![0.0f32; 4096];
    while alive() {
        match cap::Capture::open(which) {
            Ok(mut c) => {
                if let Ok(mut o) = out.lock() {
                    o.live = true;
                    o.status.clear();
                }
                let mut got_any = false;
                while alive() {
                    match c.read(&mut buf) {
                        Ok(0) => std::thread::sleep(Duration::from_millis(8)),
                        Ok(n) => {
                            got_any = true;
                            an.push(&buf[..n], &out);
                        }
                        Err(e) => {
                            if let Ok(mut o) = out.lock() {
                                o.status = e;
                            }
                            break;
                        }
                    }
                }
                if let Ok(mut o) = out.lock() {
                    o.live = false;
                }
                // a backend that never delivered anything: try the next
                if !got_any {
                    which += 1;
                }
            }
            Err(e) => {
                if let Ok(mut o) = out.lock() {
                    o.status = e;
                    o.live = false;
                }
                which += 1;
            }
        }
        // after a failure (or a device change), a short pause before trying
        // again, and a longer one once every backend has failed
        let pause = if which % cap::BACKENDS == 0 && which > 0 { 50 } else { 5 };
        for _ in 0..pause {
            if !alive() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if let Ok(mut o) = out.lock() {
        o.live = false;
    }
}

/// Mono samples in, band levels out.
struct Analyzer {
    ring: Vec<f32>,
    pos: usize,
    since: usize,
    win: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Twiddles for the FFT: cos/sin of -2πk/N.
    tw: Vec<(f32, f32)>,
    /// Each band's edges in (fractional) FFT bins, and its tilt in dB.
    edges: Vec<(f32, f32, f32)>,
    /// The loudest band of late (dB, tilted): the top of the bars.
    top: f32,
    quiet: f32,
    levels: Vec<f32>,
}

impl Analyzer {
    fn new() -> Self {
        let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
        let tw = (0..N / 2)
            .map(|k| {
                let a = -std::f32::consts::TAU * k as f32 / N as f32;
                (a.cos(), a.sin())
            })
            .collect();
        let bin = RATE as f32 / N as f32;
        let edges = (0..BANDS)
            .map(|i| {
                let f0 = LO_HZ * (HI_HZ / LO_HZ).powf(i as f32 / BANDS as f32);
                let f1 = LO_HZ * (HI_HZ / LO_HZ).powf((i + 1) as f32 / BANDS as f32);
                let fc = (f0 * f1).sqrt();
                (f0 / bin, f1 / bin, TILT * (fc / 1000.0).log2())
            })
            .collect();
        Analyzer { ring: vec![0.0; N], pos: 0, since: 0, win, re: vec![0.0; N], im: vec![0.0; N], tw, edges, top: -50.0, quiet: 0.0, levels: vec![0.0; BANDS] }
    }

    fn push(&mut self, s: &[f32], out: &Mutex<Spectrum>) {
        for &x in s {
            self.ring[self.pos] = x;
            self.pos = (self.pos + 1) % N;
            self.since += 1;
            if self.since >= HOP {
                self.since = 0;
                self.analyze();
                if let Ok(mut o) = out.lock() {
                    if o.fresh {
                        for (b, l) in o.bands.iter_mut().zip(&self.levels) {
                            *b = b.max(*l);
                        }
                    } else {
                        o.bands.copy_from_slice(&self.levels);
                    }
                    o.fresh = true;
                    o.quiet_for = self.quiet;
                }
            }
        }
    }

    fn analyze(&mut self) {
        for i in 0..N {
            self.re[i] = self.ring[(self.pos + i) % N] * self.win[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im, &self.tw);
        // one-sided power, scaled so a full-scale sine peaks at 0 dB
        let norm = 2.0 / (N as f32 * 0.5);
        let power = |k: usize, re: &[f32], im: &[f32]| (re[k] * re[k] + im[k] * im[k]) * norm * norm;
        let dt = HOP as f32 / RATE as f32;
        let mut loudest = f32::MIN;
        let mut raw_max = f32::MIN;
        let mut dbs = [0.0f32; BANDS];
        for (i, &(a, b, tilt)) in self.edges.iter().enumerate() {
            // integrate the power over the band: each bin covers k ± ½
            let mut p = 0.0;
            let k0 = (a - 0.5).floor().max(0.0) as usize;
            let k1 = ((b + 0.5).ceil() as usize).min(N / 2 - 1);
            for k in k0..=k1 {
                let lo = (k as f32 - 0.5).max(a);
                let hi = (k as f32 + 0.5).min(b);
                if hi > lo {
                    p += power(k, &self.re, &self.im) * (hi - lo);
                }
            }
            let db = 10.0 * (p + 1e-14).log10();
            raw_max = raw_max.max(db);
            dbs[i] = db + tilt;
            loudest = loudest.max(dbs[i]);
        }
        if raw_max < QUIET_DB {
            self.quiet += dt;
        } else {
            self.quiet = 0.0;
        }
        // the top follows the loudest band up at once and drifts back down
        // slowly, so quiet songs still fill the bars after a while
        if loudest > self.top {
            self.top = loudest;
        } else {
            self.top = (self.top - 3.0 * dt).max(loudest);
        }
        self.top = self.top.max(-50.0);
        let floor = self.top - RANGE;
        for (l, db) in self.levels.iter_mut().zip(dbs) {
            *l = if raw_max < QUIET_DB { 0.0 } else { ((db - floor) / RANGE).clamp(0.0, 1.0).powf(GAMMA) };
        }
    }
}

/// In-place iterative radix-2 FFT (N a power of two).
fn fft(re: &mut [f32], im: &mut [f32], tw: &[(f32, f32)]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (c, s) = tw[k * step];
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

#[cfg(target_os = "linux")]
mod cap {
    //! The default sink's monitor through a recording child. Test hook:
    //! XMAS_VIZ_SOURCE names a sink to listen to instead (e.g. a null sink).
    use super::RATE;
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::process::{Child, ChildStdout, Command, Stdio};

    pub const BACKENDS: usize = 2;

    pub struct Capture {
        child: Child,
        out: ChildStdout,
        bytes: Vec<u8>,
        /// Leftover bytes of a sample split across reads.
        part: Vec<u8>,
    }

    impl Capture {
        pub fn open(which: usize) -> Result<Capture, String> {
            let rate = RATE.to_string();
            let target = std::env::var("XMAS_VIZ_SOURCE").ok().filter(|t| !t.is_empty());
            let mut cmd = if which % BACKENDS == 0 {
                let mut c = Command::new("pw-record");
                c.args(["--raw", "--format", "f32", "--rate", &rate, "--channels", "1", "--latency", "20ms"]);
                // capture what the sink plays (its monitor), not a microphone
                c.args(["-P", "{ stream.capture.sink = true application.name = \"xmas-tui\" media.name = \"Christmas wallpaper visualizer\" }"]);
                if let Some(t) = &target {
                    c.args(["--target", t]);
                }
                c.arg("-");
                c
            } else {
                let mut c = Command::new("parec");
                c.args(["--raw", "--format=float32le", &format!("--rate={rate}"), "--channels=1", "--latency-msec=20"]);
                c.args(["--client-name=xmas-tui", "--stream-name=Christmas wallpaper visualizer"]);
                c.arg(format!("--device={}", target.map(|t| format!("{t}.monitor")).unwrap_or_else(|| "@DEFAULT_MONITOR@".into())));
                c
            };
            let name = if which % BACKENDS == 0 { "pw-record" } else { "parec" };
            let mut child = cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "can't listen: needs PipeWire (pw-record) or PulseAudio (parec)".to_string())?;
            let out = child.stdout.take().ok_or_else(|| format!("{name}: no output"))?;
            // non-blocking, so the thread can notice it's been switched off
            // even when no audio is flowing
            unsafe {
                let fd = out.as_raw_fd();
                let fl = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
            }
            Ok(Capture { child, out, bytes: vec![0; 16384], part: Vec::new() })
        }

        /// Samples read (0: none waiting right now).
        pub fn read(&mut self, into: &mut [f32]) -> Result<usize, String> {
            let want = (into.len() * 4).min(self.bytes.len()) - self.part.len();
            match self.out.read(&mut self.bytes[..want]) {
                Ok(0) => {
                    let _ = self.child.try_wait();
                    Err("the audio capture stopped".into())
                }
                Ok(n) => {
                    self.part.extend_from_slice(&self.bytes[..n]);
                    let whole = self.part.len() / 4;
                    for (i, s) in self.part.chunks_exact(4).enumerate() {
                        into[i] = f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
                    }
                    self.part.drain(..whole * 4);
                    Ok(whole)
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::Interrupted => Ok(0),
                Err(e) => Err(format!("audio capture: {e}")),
            }
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(windows)]
mod cap {
    //! WASAPI loopback: what the default render device is playing.
    use windows::core::PWSTR;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

    pub const BACKENDS: usize = 1;

    enum Fmt {
        F32,
        I16,
        I24,
        I32,
    }

    pub struct Capture {
        en: IMMDeviceEnumerator,
        client: IAudioClient,
        cap: IAudioCaptureClient,
        id: String,
        fmt: Fmt,
        channels: usize,
        /// Input frames per output sample (resampling to 48 kHz by picking).
        step: f64,
        acc: f64,
        last_check: std::time::Instant,
        last_data: std::time::Instant,
    }

    unsafe fn default_id(en: &IMMDeviceEnumerator) -> Option<String> {
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let p: PWSTR = dev.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }

    impl Capture {
        pub fn open(_which: usize) -> Result<Capture, String> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("audio devices: {e}"))?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|_| "no audio output device".to_string())?;
                let id = default_id(&en).unwrap_or_default();
                let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(|e| format!("audio output: {e}"))?;
                let wf = client.GetMixFormat().map_err(|e| format!("audio format: {e}"))?;
                let w: WAVEFORMATEX = *wf;
                let (tag, bits, channels, rate) = (w.wFormatTag as u32, w.wBitsPerSample, w.nChannels as usize, w.nSamplesPerSec);
                // WAVE_FORMAT_EXTENSIBLE: the real format is the SubFormat
                // GUID's first field (at byte 24): 1 = PCM, 3 = float
                let kind = if tag == 0xFFFE { std::ptr::read_unaligned((wf as *const u8).add(24) as *const u32) } else { tag };
                let fmt = match (kind, bits) {
                    (3, 32) => Fmt::F32,
                    (1, 16) => Fmt::I16,
                    (1, 24) => Fmt::I24,
                    (1, 32) => Fmt::I32,
                    _ => {
                        CoTaskMemFree(Some(wf as *const _));
                        return Err(format!("unsupported audio format ({kind}, {bits}-bit)"));
                    }
                };
                // a 200 ms buffer, polled every few ms
                let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, wf, None);
                CoTaskMemFree(Some(wf as *const _));
                r.map_err(|e| format!("audio loopback: {e}"))?;
                let cap: IAudioCaptureClient = client.GetService().map_err(|e| format!("audio loopback: {e}"))?;
                client.Start().map_err(|e| format!("audio loopback: {e}"))?;
                let now = std::time::Instant::now();
                Ok(Capture { en, client, cap, id, fmt, channels: channels.max(1), step: rate as f64 / super::RATE as f64, acc: 0.0, last_check: now, last_data: now })
            }
        }

        pub fn read(&mut self, into: &mut [f32]) -> Result<usize, String> {
            let now = std::time::Instant::now();
            // follow the default device when it changes
            if now.duration_since(self.last_check).as_secs_f32() > 2.0 {
                self.last_check = now;
                if unsafe { default_id(&self.en) }.is_some_and(|id| id != self.id) {
                    return Err(String::new());
                }
            }
            let mut n = 0;
            unsafe {
                loop {
                    let size = self.cap.GetNextPacketSize().map_err(|e| format!("audio loopback: {e}"))?;
                    if size == 0 {
                        break;
                    }
                    let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                    self.cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None).map_err(|e| format!("audio loopback: {e}"))?;
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                    let ch = self.channels;
                    let mut f = 0.0f64;
                    // pick frames at 48 kHz (the analysis rate) and mix to mono
                    f += self.acc;
                    while (f as usize) < frames as usize && n < into.len() {
                        let i = f as usize;
                        let mut s = 0.0f32;
                        if !silent {
                            for c in 0..ch {
                                let k = i * ch + c;
                                s += match self.fmt {
                                    Fmt::F32 => *(data as *const f32).add(k),
                                    Fmt::I16 => *(data as *const i16).add(k) as f32 / 32768.0,
                                    Fmt::I24 => {
                                        let p = (data as *const u8).add(k * 3);
                                        (((*p as i32) << 8 | (*p.add(1) as i32) << 16 | (*p.add(2) as i32) << 24) >> 8) as f32 / 8388608.0
                                    }
                                    Fmt::I32 => *(data as *const i32).add(k) as f32 / 2147483648.0,
                                };
                            }
                        }
                        into[n] = s / ch as f32;
                        n += 1;
                        f += self.step;
                    }
                    self.acc = (f - frames as f64).max(0.0);
                    self.cap.ReleaseBuffer(frames).map_err(|e| format!("audio loopback: {e}"))?;
                    if n >= into.len() {
                        break;
                    }
                }
            }
            if n > 0 {
                self.last_data = now;
            } else {
                // nothing's playing (loopback sends no packets then): feed
                // silence so the bars settle
                let gap = now.duration_since(self.last_data).as_secs_f64();
                if gap > 0.03 {
                    let k = ((gap * super::RATE as f64) as usize).min(into.len());
                    into[..k].fill(0.0);
                    self.last_data = now;
                    return Ok(k);
                }
            }
            Ok(n)
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            unsafe {
                let _ = self.client.Stop();
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod cap {
    pub const BACKENDS: usize = 1;
    pub struct Capture;
    impl Capture {
        pub fn open(_which: usize) -> Result<Capture, String> {
            Err("can't listen to the audio output here".into())
        }
        pub fn read(&mut self, _into: &mut [f32]) -> Result<usize, String> {
            Err(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure tone lights its own band (and not the far ones).
    #[test]
    fn a_tone_shows_in_its_band() {
        let mut an = Analyzer::new();
        let out = Mutex::new(Spectrum { bands: vec![0.0; BANDS], ..Default::default() });
        let hz = 1000.0f32;
        let s: Vec<f32> = (0..N * 2).map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin()).collect();
        an.push(&s, &out);
        let mut b = vec![0.0; BANDS];
        assert!(out.lock().unwrap().take(&mut b));
        let peak = (0..BANDS).max_by(|&i, &j| b[i].total_cmp(&b[j])).unwrap();
        let want = ((hz / LO_HZ).ln() / (HI_HZ / LO_HZ).ln() * BANDS as f32) as usize;
        assert!(peak.abs_diff(want) <= 1, "peak band {peak}, expected about {want}");
        assert!(b[peak] > 0.95);
        assert!(b[0] < 0.05 && b[BANDS - 1] < 0.05, "far bands: {} {}", b[0], b[BANDS - 1]);
    }

    #[test]
    fn silence_is_flat_and_counted() {
        let mut an = Analyzer::new();
        let out = Mutex::new(Spectrum { bands: vec![0.0; BANDS], ..Default::default() });
        an.push(&vec![0.0; RATE as usize], &out);
        let o = out.lock().unwrap();
        assert!(o.bands.iter().all(|&b| b == 0.0));
        assert!(o.quiet_for > 0.9);
    }
}
