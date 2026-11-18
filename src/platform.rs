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

/// Take xmas-tui off this computer, as it was installed: Windows, the
/// installer's own uninstaller (it asks, and takes the program, its
/// shortcuts and "run at startup" away); Linux, what the installer set up
/// (the service, the program and its scripts in ~/.local/share/xmas-tui,
/// its link in ~/.local/bin). The settings and the save are kept either
/// way. Returns
/// what was done (or why it couldn't be), and whether the wallpaper should
/// now quit.
pub fn uninstall() -> (String, bool) {
    let Ok(exe) = std::env::current_exe() else { return ("couldn't find the program".into(), false) };
    #[cfg(windows)]
    {
        if let Some(un) = exe.parent().map(|d| d.join("unins000.exe")).filter(|p| p.exists()) {
            // (its own window asks; nothing's removed unless that's said
            // yes to, and then it stops the wallpaper itself)
            return match Command::new(&un).spawn() {
                Ok(_) => ("the uninstaller's open: it asks before it takes anything away".into(), false),
                Err(e) => (format!("couldn't start the uninstaller: {e}"), false),
            };
        }
        // a copy that wasn't installed: off "run at startup" (the file is the user's to delete)
        autostart_set(false);
        (format!("not started with Windows any more; {} can be deleted", exe.display()), true)
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("xmas-tui");
        let units = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("systemd/user");
        let installed = exe.starts_with(&data);
        if !installed {
            return if exe.starts_with("/usr") || exe.starts_with("/opt") {
                ("installed as a package: `sudo apt remove xmas-tui` (or the distro's own way) takes it away".into(), false)
            } else {
                (format!("this copy wasn't put in by the installer ({}): it can simply be deleted", exe.display()), false)
            };
        }
        let link = home.join(".local/bin/xmas-tui");
        // (in a process of its own: stopping the service stops this one)
        // (the program and its scripts only: the save beside them stays)
        let script = format!(
            "sleep 1; systemctl --user disable --now xmas-tui.service; rm -f '{u}'; systemctl --user daemon-reload; rm -rf '{d}/bin' '{d}/scripts'; rm -f '{d}/config.example.toml'; rmdir '{d}' 2>/dev/null; if [ -L '{l}' ]; then rm -f '{l}'; fi",
            u = units.join("xmas-tui.service").display(),
            d = data.display(),
            l = link.display()
        );
        match Command::new("setsid").args(["sh", "-c", &script]).spawn().or_else(|_| Command::new("sh").args(["-c", &script]).spawn()) {
            Ok(_) => ("taken off this computer (the settings and the save are kept: ~/.config/xmas-tui, ~/.local/share/xmas-tui/save.toml)".into(), true),
            Err(e) => (format!("couldn't: {e}"), false),
        }
    }
}

/// The wallpaper's log, if there is one.
pub fn log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let p = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?.join("xmas-tui").join("wallpaper.log");
    #[cfg(not(windows))]
    let p = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state")).join("xmas-tui").join("wallpaper.log");
    p.exists().then_some(p)
}

/// Open a web page (https only) in the browser.
pub fn open_url(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    #[cfg(windows)]
    {
        // (no console window flashing up, as `cmd /c start` would)
        let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
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
