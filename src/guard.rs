//! Keeping the Windows wallpaper standing: a panic is written to the log
//! with where it happened, and the wallpaper starts itself again; a main
//! loop that stops turning (stuck somewhere) is noticed by a watchdog,
//! logged and restarted the same way. Never more than a few times in a row
//! (a crash at start-up mustn't become a loop).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
/// When the main loop last went round (ms since START).
static BEAT: AtomicU64 = AtomicU64::new(0);
/// The main loop's quitting on purpose (the watchdog stands down).
static LEAVING: AtomicBool = AtomicBool::new(false);
/// How many restarts are allowed in a row (passed on to the next instance).
const MAX_RESTARTS: u32 = 4;

fn ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Seconds since the guard started.
pub fn uptime() -> f32 {
    ms() as f32 / 1000.0
}

/// The main loop went round.
pub fn beat() {
    BEAT.store(ms(), Ordering::Relaxed);
}

/// Quitting (or restarting for an update): no watchdog from here.
pub fn leaving() {
    LEAVING.store(true, Ordering::SeqCst);
}

/// Restarts so far, in this run of restarts.
fn restarts() -> u32 {
    std::env::var("XMAS_RESTARTS").ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Start this program again (the same arguments), once this one's gone,
/// and leave.
pub fn restart(why: &str) -> ! {
    let n = restarts();
    crate::tray::shutdown();
    if n >= MAX_RESTARTS {
        eprintln!("xmas-tui: {why}; not restarting (that's {n} times in a row)");
        std::process::exit(3);
    }
    eprintln!("xmas-tui: {why}; starting again");
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .env("XMAS_RESTARTS", (n + 1).to_string())
            .env("XMAS_AFTER", std::process::id().to_string())
            .spawn();
    }
    std::process::exit(3);
}

/// A restarted wallpaper: wait for the one before to be gone (it holds the
/// single-instance lock until it is). A run that's lasted a while counts as
/// a fresh start again.
pub fn after_restart() {
    #[cfg(windows)]
    if let Some(pid) = std::env::var("XMAS_AFTER").ok().and_then(|v| v.parse::<u32>().ok()) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if !h.is_null() {
                WaitForSingleObject(h, 10_000);
                CloseHandle(h);
            }
        }
        std::env::remove_var("XMAS_AFTER");
    }
    // after ten good minutes, a crash starts the count afresh
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(600));
        std::env::remove_var("XMAS_RESTARTS");
    });
}

/// Panics logged with a backtrace; one on the main thread (the wallpaper's
/// gone) restarts it. A watchdog restarts a main loop that's stuck.
pub fn install() {
    START.get_or_init(Instant::now);
    beat();
    std::panic::set_hook(Box::new(|info| {
        let main = std::thread::current().name() == Some("main");
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("xmas-tui: panic on thread {:?}: {info}\n{bt}", std::thread::current().name().unwrap_or("?"));
        if main && !LEAVING.load(Ordering::SeqCst) {
            restart("the wallpaper crashed");
        }
    }));
    let _ = std::thread::Builder::new().name("watchdog".into()).spawn(|| {
        let mut last = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(5));
            // (the PC slept: the loop hasn't had a chance yet)
            let slept = last.elapsed() > Duration::from_secs(15);
            last = Instant::now();
            if slept {
                beat();
                continue;
            }
            if LEAVING.load(Ordering::SeqCst) {
                return;
            }
            let quiet = ms().saturating_sub(BEAT.load(Ordering::Relaxed));
            if quiet > 40_000 {
                restart(&format!("the wallpaper stopped responding ({} s)", quiet / 1000));
            }
        }
    });
}
