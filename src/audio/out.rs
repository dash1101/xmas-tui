//! Where the PCM goes: 48 kHz stereo s16le, written in small blocks by the
//! audio thread. Writes block — that's the pacing — so they never happen
//! on the render thread.
//! - Linux: a `pw-cat` child process (PipeWire), falling back to `paplay`
//!   (PulseAudio), fed through a pipe shrunk to ~43 ms so latency stays
//!   low. No ALSA headers or audio crates needed.
//! - Windows: the classic waveOut API (winmm.dll ships with Windows), eight
//!   10 ms buffers in flight.

use std::io;

pub const RATE: u32 = 48_000;

/// How many backends `Output::open` can cycle through.
#[cfg(target_os = "linux")]
pub const BACKENDS: usize = 2;
#[cfg(not(target_os = "linux"))]
pub const BACKENDS: usize = 1;

/// Test hooks (env vars), so the real output path can be exercised without
/// being heard: XMAS_AUDIO_DEV_VOLUME scales the samples (0 = silence) and
/// XMAS_AUDIO_DEV_TARGET picks the sink (e.g. a null sink) on Linux.
#[derive(Clone, Default)]
pub struct Dev {
    pub volume: Option<f32>,
    /// Sink name for the Linux child (Windows always uses the default device).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub target: Option<String>,
}

impl Dev {
    pub fn from_env() -> Dev {
        Dev {
            volume: std::env::var("XMAS_AUDIO_DEV_VOLUME").ok().and_then(|v| v.parse::<f32>().ok()).map(|v| v.clamp(0.0, 1.0)),
            target: std::env::var("XMAS_AUDIO_DEV_TARGET").ok().filter(|t| !t.is_empty()),
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::Output;
#[cfg(windows)]
pub use win::Output;

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::process::{Child, ChildStdin, Command, Stdio};

    pub struct Output {
        child: Child,
        stdin: Option<ChildStdin>,
        bytes: Vec<u8>,
        gain: f32,
        pub name: &'static str,
    }

    impl Output {
        /// `which` picks the backend (cycled after failures).
        pub fn open(which: usize, dev: &Dev) -> io::Result<Output> {
            let rate = RATE.to_string();
            let (name, mut cmd) = if which % BACKENDS == 0 {
                let mut c = Command::new("pw-cat");
                c.args(["--playback", "--raw", "--format", "s16", "--rate", &rate, "--channels", "2", "--latency", "40ms"]);
                c.args(["-P", "{ application.name = \"xmas-tui\" media.name = \"Christmas wallpaper\" }"]);
                if let Some(t) = &dev.target {
                    c.args(["--target", t]);
                }
                c.arg("-");
                ("pw-cat", c)
            } else {
                let mut c = Command::new("paplay");
                c.args(["--raw", "--format=s16le", &format!("--rate={rate}"), "--channels=2", "--latency-msec=40"]);
                c.args(["--client-name=xmas-tui", "--stream-name=Christmas wallpaper"]);
                if let Some(t) = &dev.target {
                    c.arg(format!("--device={t}"));
                }
                ("paplay", c)
            };
            // the child must never scribble on our terminal (the TUI owns it)
            let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
            let stdin = child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?;
            // a default 64 KiB pipe would add ~340 ms of latency; 8 KiB ≈ 43 ms
            unsafe {
                libc::fcntl(stdin.as_raw_fd(), libc::F_SETPIPE_SZ, 8192);
            }
            Ok(Output { child, stdin: Some(stdin), bytes: vec![], gain: dev.volume.unwrap_or(1.0), name })
        }

        pub fn write(&mut self, pcm: &[i16]) -> io::Result<()> {
            self.bytes.clear();
            for s in pcm {
                let s = if self.gain < 1.0 { (*s as f32 * self.gain) as i16 } else { *s };
                self.bytes.extend_from_slice(&s.to_le_bytes());
            }
            match self.stdin.as_mut() {
                Some(w) => w.write_all(&self.bytes),
                None => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
            }
        }
    }

    impl Drop for Output {
        fn drop(&mut self) {
            self.stdin = None;
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Media::Audio::{
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite, CALLBACK_EVENT, HWAVEOUT, WAVEFORMATEX,
        WAVEHDR, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

    const NBUF: usize = 8;

    /// WAVEHDR is packed(1); this wrapper pins it to a 16-byte boundary so
    /// its fields are naturally aligned for the volatile flag reads.
    #[repr(C, align(16))]
    struct Hdr(WAVEHDR);

    pub struct Output {
        h: HWAVEOUT,
        ev: HANDLE,
        /// Heap headers shared with the driver (it sets WHDR_DONE).
        hdrs: Vec<*mut Hdr>,
        bufs: Vec<Vec<i16>>,
        queued: [bool; NBUF],
        next: usize,
        gain: f32,
        pub name: &'static str,
    }

    const HDR_SIZE: u32 = std::mem::size_of::<WAVEHDR>() as u32;

    impl Output {
        pub fn open(_which: usize, dev: &Dev) -> io::Result<Output> {
            let fmt = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 2,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            unsafe {
                let ev = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
                if ev.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let mut h: HWAVEOUT = std::ptr::null_mut();
                let rc = waveOutOpen(&mut h, WAVE_MAPPER, &fmt, ev as usize, 0, CALLBACK_EVENT);
                if rc != 0 {
                    CloseHandle(ev);
                    return Err(io::Error::other(format!("waveOutOpen failed ({rc})")));
                }
                Ok(Output {
                    h,
                    ev,
                    hdrs: vec![],
                    bufs: vec![],
                    queued: [false; NBUF],
                    next: 0,
                    gain: dev.volume.unwrap_or(1.0),
                    name: "waveOut",
                })
            }
        }

        fn prepare(&mut self, len: usize) -> io::Result<()> {
            if self.bufs.first().map(|b| b.len()) == Some(len) {
                return Ok(());
            }
            self.release();
            for _ in 0..NBUF {
                let mut buf = vec![0i16; len];
                let p = Box::into_raw(Box::new(Hdr(unsafe { std::mem::zeroed() })));
                unsafe {
                    (*p).0.lpData = buf.as_mut_ptr() as *mut u8;
                    (*p).0.dwBufferLength = (len * 2) as u32;
                }
                self.bufs.push(buf);
                self.hdrs.push(p);
                let rc = unsafe { waveOutPrepareHeader(self.h, p as *mut WAVEHDR, HDR_SIZE) };
                if rc != 0 {
                    return Err(io::Error::other(format!("waveOutPrepareHeader failed ({rc})")));
                }
            }
            self.queued = [false; NBUF];
            self.next = 0;
            Ok(())
        }

        fn release(&mut self) {
            unsafe {
                waveOutReset(self.h);
                for &p in &self.hdrs {
                    waveOutUnprepareHeader(self.h, p as *mut WAVEHDR, HDR_SIZE);
                    drop(Box::from_raw(p));
                }
            }
            self.hdrs.clear();
            self.bufs.clear();
        }

        /// Queue one block, waiting (on the driver's event) for a free buffer.
        pub fn write(&mut self, pcm: &[i16]) -> io::Result<()> {
            self.prepare(pcm.len())?;
            let i = self.next;
            let p = self.hdrs[i];
            let start = Instant::now();
            while self.queued[i] {
                let flags = unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*p).0.dwFlags)) };
                if flags & WHDR_DONE != 0 {
                    break;
                }
                if start.elapsed() > Duration::from_secs(2) {
                    return Err(io::Error::from(io::ErrorKind::TimedOut));
                }
                unsafe { WaitForSingleObject(self.ev, 50) };
            }
            for (d, s) in self.bufs[i].iter_mut().zip(pcm) {
                *d = (*s as f32 * self.gain) as i16;
            }
            let rc = unsafe {
                let f = std::ptr::addr_of_mut!((*p).0.dwFlags);
                std::ptr::write_volatile(f, std::ptr::read_volatile(f) & !WHDR_DONE);
                waveOutWrite(self.h, p as *mut WAVEHDR, HDR_SIZE)
            };
            if rc != 0 {
                return Err(io::Error::other(format!("waveOutWrite failed ({rc})")));
            }
            self.queued[i] = true;
            self.next = (i + 1) % NBUF;
            Ok(())
        }
    }

    impl Drop for Output {
        fn drop(&mut self) {
            self.release();
            unsafe {
                waveOutClose(self.h);
                CloseHandle(self.ev);
            }
        }
    }
}

/// No sound output on other platforms (macOS etc.) — yet.
#[cfg(not(any(target_os = "linux", windows)))]
pub struct Output {
    pub name: &'static str,
}

#[cfg(not(any(target_os = "linux", windows)))]
impl Output {
    pub fn open(_which: usize, _dev: &Dev) -> io::Result<Output> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
    pub fn write(&mut self, _pcm: &[i16]) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
