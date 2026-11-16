//! Small OS-specific helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
fn quiet(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Is "start with the OS" enabled? None where we don't manage it (on Linux
/// the systemd user service handles that).
pub fn autostart_get() -> Option<bool> {
    #[cfg(windows)]
    {
        let ok = quiet(Command::new("reg").args(["query", RUN_KEY, "/v", "xmas-tui"]))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Some(ok)
    }
    #[cfg(not(windows))]
    {
        // the systemd user service the installers set up; None if there
        // isn't one (then there's nothing to toggle)
        let out = Command::new("systemctl").args(["--user", "is-enabled", "xmas-tui.service"]).output().ok()?;
        match String::from_utf8_lossy(&out.stdout).trim() {
            "enabled" | "enabled-runtime" | "linked" => Some(true),
            "disabled" => Some(false),
            _ => None,
        }
    }
}

pub fn autostart_set(on: bool) {
    #[cfg(windows)]
    {
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let val = format!("\"{}\"", exe.display());
                let _ = quiet(Command::new("reg").args(["add", RUN_KEY, "/v", "xmas-tui", "/t", "REG_SZ", "/d", &val, "/f"])).status();
            }
        } else {
            let _ = quiet(Command::new("reg").args(["delete", RUN_KEY, "/v", "xmas-tui", "/f"])).status();
        }
    }
    #[cfg(not(windows))]
    {
        // enable / disable for the next log-in; never stops the running one
        let _ = Command::new("systemctl").args(["--user", if on { "enable" } else { "disable" }, "xmas-tui.service"]).output();
    }
}

/// Open the config in the user's text editor (used where we can't take
/// keyboard input ourselves).
pub fn open_in_editor(path: &Path) {
    #[cfg(windows)]
    {
        let _ = Command::new("notepad.exe").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

/// Windows: make sure only one wallpaper instance runs. Returns false if
/// another is already running. (The handle is intentionally leaked so the
/// mutex lives as long as the process.)
pub fn single_instance() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = "Local\\xmas-tui-wallpaper\0".encode_utf16().collect();
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            !(h.is_null() || GetLastError() == ERROR_ALREADY_EXISTS)
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// A spot both the running wallpaper and its settings window can reach
/// (the per-user runtime dir on Linux, the temp dir elsewhere).
fn shared_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(d);
    }
    std::env::temp_dir()
}

/// The wallpaper's live sound levels, for the settings window's meters:
/// written by the wallpaper only while a settings window keeps
/// `meters_wanted` fresh.
pub fn meters_path() -> PathBuf {
    shared_dir().join("xmas-tui-meters")
}

/// The wallpaper's update status, for the settings window (one line).
pub fn update_status_path() -> PathBuf {
    shared_dir().join("xmas-tui-update")
}

/// The settings window asking the wallpaper to check / install / dismiss.
pub fn update_request_path() -> PathBuf {
    shared_dir().join("xmas-tui-update.request")
}

/// Kept fresh by a popped-out window while it's open.
pub fn popout_path() -> PathBuf {
    shared_dir().join("xmas-tui-popout")
}

pub fn meters_want_path() -> PathBuf {
    shared_dir().join("xmas-tui-meters.want")
}

/// Whether a file was modified within the last `secs` seconds.
pub fn fresh(path: &Path, secs: f32) -> bool {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).is_some_and(|e| e.as_secs_f32() < secs)
}

/// Share the sound levels (whole-file replace, so a reader never sees half).
pub fn write_meters(m: &[f32]) {
    let bytes: Vec<u8> = m.iter().flat_map(|v| v.to_le_bytes()).collect();
    let (tmp, dst) = (meters_path().with_extension("tmp"), meters_path());
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &dst);
    }
}

/// The wallpaper's levels, if it's sharing them (and they're current).
pub fn read_meters<const N: usize>() -> Option<[f32; N]> {
    let p = meters_path();
    if !fresh(&p, 1.0) {
        return None;
    }
    let b = std::fs::read(&p).ok()?;
    if b.len() != N * 4 {
        return None;
    }
    let mut out = [0.0f32; N];
    for (i, c) in b.chunks_exact(4).enumerate() {
        out[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn meters_round_trip() {
        let m = [0.1f32, 0.5, 0.0, 0.9];
        super::write_meters(&m);
        assert_eq!(super::read_meters::<4>(), Some(m));
        // the wrong size is never misread
        assert_eq!(super::read_meters::<3>(), None);
        let _ = std::fs::remove_file(super::meters_path());
    }
}
