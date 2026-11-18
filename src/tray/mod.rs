//! System-tray / status icon: a little glowing tree that opens the
//! standalone settings window (`--settings-window`) and offers a few quick
//! toggles. KDE / any StatusNotifierItem host on Linux (`sni.rs`, over
//! zbus), the notification area on Windows (`win.rs`).
//!
//! Quick toggles edit the config file directly; the running wallpaper
//! live-reloads it (see `App::step`), so the tray never touches the App.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "linux")]
mod sni;
#[cfg(windows)]
mod win;

/// Tooltip / title everywhere.
pub const TITLE: &str = "Christmas wallpaper";

/// Set by the tray's "Quit wallpaper" when we're not under systemd (or on
/// Windows); the front-end's main loop exits cleanly when it sees it.
static QUIT: AtomicBool = AtomicBool::new(false);
/// "Pause effects" (Linux tray): freeze the animation and hide window
/// snow/lights until unticked. Runtime only, never saved.
static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn quit_requested() -> bool {
    QUIT.load(Ordering::SeqCst)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn paused() -> bool {
    PAUSED.load(Ordering::SeqCst)
}

/// Start the tray icon on its own thread (no-op where unsupported).
pub fn spawn(config: PathBuf) {
    #[cfg(target_os = "linux")]
    sni::spawn(config);
    #[cfg(windows)]
    win::spawn(config);
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = config;
}

/// Remove the icon (call on every exit path; safe to call more than once).
pub fn shutdown() {
    #[cfg(target_os = "linux")]
    sni::shutdown();
    #[cfg(windows)]
    win::shutdown();
}

// ---------------------------------------------------------------- actions

/// Our own executable, even if it was rebuilt (replaced) while running —
/// Linux then reports "/path/xmas-tui (deleted)".
fn exe() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?;
    let s = p.to_string_lossy();
    Some(match s.strip_suffix(" (deleted)") {
        Some(orig) => PathBuf::from(orig),
        None => p,
    })
}

/// Launch `<exe> --settings-window --config <path>` detached (the settings
/// window itself makes sure only one is open).
pub fn open_settings(config: &Path) {
    open_settings_activated(config, None);
}

/// Same, handing over an XDG activation token (from the tray host) so the
/// new window may take focus on Wayland instead of opening behind others.
pub fn open_settings_activated(config: &Path, token: Option<String>) {
    let Some(exe) = exe() else { return };
    let mut cmd = Command::new(exe);
    cmd.arg("--settings-window").arg("--config").arg(config);
    cmd.env_remove("XDG_ACTIVATION_TOKEN").env_remove("DESKTOP_STARTUP_ID");
    if let Some(t) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", t);
    }
    // never let it scribble on the wallpaper's terminal
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // its own process group, so a wallpaper restart (SIGHUP to our group)
    // doesn't close a settings window the user has open
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    match cmd.spawn() {
        // reap it when it exits so it doesn't linger as a zombie
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("xmas-tui: can't open settings: {e}"),
    }
}

/// Our own systemd user unit (xmas-tui.service from scripts/install.sh),
/// if that's what started us. Only units named xmas* count, so running
/// from e.g. a Konsole (itself an app-….service) never stops the terminal.
#[cfg(target_os = "linux")]
fn systemd_unit() -> Option<String> {
    let cg = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    cg.lines()
        .filter_map(|l| l.rsplit('/').find(|seg| seg.starts_with("xmas") && seg.ends_with(".service")))
        .next()
        .map(str::to_string)
}

/// Where a quit on purpose is noted (Linux): the launcher reads it, so a
/// wallpaper restarted straight after a Quit (if systemd couldn't be asked
/// to stop it) stops itself instead of coming back.
#[cfg(target_os = "linux")]
pub fn quit_note() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state"))
        .join("xmas-tui")
        .join("quit")
}

/// Stop the wallpaper for good (the tray's Quit, the menu's Quit). Under
/// systemd, which restarts it whenever it exits, the service is stopped
/// (ours by the cgroup we're in, or by its name if it's running), and the
/// quit is noted for the launcher; then this exits as well.
pub fn quit_wallpaper() {
    #[cfg(target_os = "linux")]
    {
        // (tests: a stand-in)
        let systemctl = std::env::var("XMAS_TEST_SYSTEMCTL").unwrap_or_else(|_| "systemctl".into());
        let unit = systemd_unit().or_else(|| {
            let active = Command::new(&systemctl).args(["--user", "is-active", "xmas-tui.service"]).stderr(Stdio::null()).output().ok().is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "active");
            active.then(|| "xmas-tui.service".to_string())
        });
        if let Some(unit) = unit {
            let note = quit_note();
            if let Some(dir) = note.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = std::fs::write(&note, now.to_string());
            // --no-block: systemctl lives in our cgroup and gets stopped with us
            let ok = Command::new(&systemctl)
                .args(["--user", "--no-block", "stop", &unit])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            eprintln!("xmas-tui: quitting on purpose{}", if ok { "" } else { " (systemd couldn't be asked to stop the service: the launcher won't start it again for a minute)" });
        }
    }
    QUIT.store(true, Ordering::SeqCst);
}

/// From another process (the settings window's "quit wallpaper"): ask the
/// running wallpaper, through its tray, to quit. No-op if none is running.
pub fn request_wallpaper_quit() {
    #[cfg(target_os = "linux")]
    sni::remote_quit();
    #[cfg(windows)]
    win::remote_quit();
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn toggle_pause() -> bool {
    !PAUSED.fetch_xor(true, Ordering::SeqCst)
}

// ------------------------------------------------------------ config edits

/// Config switches the tray can flip: (section, key, default when absent).
pub const WINDOW_SNOW: (&str, &str, bool) = ("windows", "snow", true);
/// The sound on/off switch (the `[sound]` section of the config).
pub const SOUND: (&str, &str, bool) = ("sound", "enabled", true);

fn read_table(path: &Path) -> toml::Table {
    std::fs::read_to_string(path).ok().and_then(|s| s.parse::<toml::Table>().ok()).unwrap_or_default()
}

/// Current value of a config switch.
pub fn flag(path: &Path, (sec, key, def): (&str, &str, bool)) -> bool {
    read_table(path).get(sec).and_then(|s| s.get(key)).and_then(|v| v.as_bool()).unwrap_or(def)
}

/// Flip a config switch in the file, keeping every other key as it is.
/// Works on the raw TOML so sections this build doesn't know survive.
/// Returns the new value.
pub fn toggle_flag(path: &Path, (sec, key, def): (&str, &str, bool)) -> bool {
    // never rewrite a config the user is mid-way through editing (or that
    // has a typo): that would replace their whole file with one setting
    match std::fs::read_to_string(path) {
        Ok(text) if text.parse::<toml::Table>().is_err() => {
            eprintln!("xmas-tui: {} has an error; not changing it from the tray", path.display());
            return def;
        }
        // unreadable (e.g. saved as ANSI/Latin-1 by Notepad, or permissions):
        // writing would replace the user's whole file with one setting
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            eprintln!("xmas-tui: can't read {} ({e}); not changing it from the tray", path.display());
            return def;
        }
        _ => {}
    }
    let mut t = read_table(path);
    let cur = t.get(sec).and_then(|s| s.get(key)).and_then(|v| v.as_bool()).unwrap_or(def);
    let section = t.entry(sec.to_string()).or_insert_with(|| toml::Value::Table(Default::default()));
    if !section.is_table() {
        *section = toml::Value::Table(Default::default());
    }
    if let Some(s) = section.as_table_mut() {
        s.insert(key.to_string(), toml::Value::Boolean(!cur));
    }
    let body = toml::to_string_pretty(&t).unwrap_or_default();
    let text = format!("# xmas-tui config — written by the settings panel / tray icon.\n\n{body}");
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    };
    if let Err(e) = write() {
        eprintln!("xmas-tui: can't write {}: {e}", path.display());
        return cur;
    }
    !cur
}

// ------------------------------------------------------------------- icon

/// The tray icon: the app icon (a terminal-glyph Christmas tree) at
/// `n` x `n` pixels, straight (non-premultiplied) RGBA, row-major.
pub fn icon_rgba(n: usize) -> Vec<u8> {
    crate::icon::rgba(n)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    /// Quit on purpose, with the wallpaper's service running: the service
    /// is stopped (by name, though this isn't in its cgroup), the quit is
    /// noted for the launcher, and the app's told to quit too.
    #[test]
    fn quit_stops_the_service_and_notes_it() {
        let dir = std::env::temp_dir().join(format!("xmas-quit-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let fake = dir.join("systemctl");
        let log = dir.join("calls");
        std::fs::write(&fake, format!("#!/bin/sh\necho \"$*\" >> '{}'\n[ \"$2\" = is-active ] && echo active\nexit 0\n", log.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("XMAS_TEST_SYSTEMCTL", &fake);
        std::env::set_var("XDG_STATE_HOME", dir.join("state"));
        super::quit_wallpaper();
        let calls = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(calls.contains("--user is-active xmas-tui.service"), "{calls}");
        assert!(calls.contains("--user --no-block stop xmas-tui.service"), "never stopped: {calls}");
        let at: u64 = std::fs::read_to_string(super::quit_note()).unwrap().trim().parse().unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        assert!(now - at < 5, "the note's time is off");
        assert!(super::quit_requested());
        std::env::remove_var("XMAS_TEST_SYSTEMCTL");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
