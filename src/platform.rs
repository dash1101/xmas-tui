//! Small OS-specific helpers.

use std::path::Path;
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
        None
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
        let _ = on;
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
