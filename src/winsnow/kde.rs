//! KDE bridge: own the D-Bus name the KWin script reports to, and load the
//! script into KWin (unloading any stale copy first). The script can be
//! paused (unloaded) while there's nothing to draw. Dropping `Bridge`
//! unloads it again.

use std::sync::{Arc, Mutex};

const NAME: &str = "org.xmastui.Snow";
const SCRIPT: &str = "xmas-snow";
const JS: &str = include_str!("../../kwin/xmas-snow.js");

/// The latest report, and whether it should wake the overlay early.
#[derive(Default)]
pub struct Pending {
    pub data: Option<String>,
    /// Windows changed since the overlay last looked (cursor-only reports
    /// don't set it). Kept as state, so a report arriving while the overlay
    /// is busy isn't lost the way a bare notify would be.
    pub wake: bool,
}

/// Latest report + a condvar so the overlay can redraw the moment a window
/// moves instead of waiting for its next frame tick.
pub type Signal = Arc<(Mutex<Pending>, std::sync::Condvar)>;

struct Receiver {
    latest: Signal,
    last_windows: Mutex<String>,
}

#[zbus::interface(name = "org.xmastui.Snow")]
impl Receiver {
    /// Called by the KWin script with a JSON snapshot of windows + cursor.
    fn update(&self, data: String) {
        // wake the overlay early only when windows moved; cursor-only
        // reports are picked up on the next regular frame
        let windows = data.split(",\"c\":").next().unwrap_or("").to_string();
        let moved = self.last_windows.lock().map(|mut w| {
            let changed = *w != windows;
            *w = windows;
            changed
        });
        let (lock, cv) = &*self.latest;
        if let Ok(mut l) = lock.lock() {
            l.data = Some(data);
            if moved.unwrap_or(true) {
                l.wake = true;
                cv.notify_one();
            }
        }
    }
}

pub struct Bridge {
    conn: zbus::blocking::Connection,
    pub latest: Signal,
    running: bool,
}

fn kwin(conn: &zbus::blocking::Connection, path: &str, iface: &str, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> zbus::Result<zbus::message::Message> {
    conn.call_method(Some("org.kde.KWin"), path, Some(iface), method, body)
}

impl Bridge {
    pub fn start() -> Result<Bridge, String> {
        let latest: Signal = Arc::new((Mutex::new(Pending::default()), std::sync::Condvar::new()));
        let conn = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(NAME))
            .and_then(|b| b.serve_at("/Snow", Receiver { latest: latest.clone(), last_windows: Mutex::new(String::new()) }))
            .and_then(|b| b.build())
            .map_err(|e| format!("d-bus: {e}"))?;
        // the script itself is loaded by `set_running` once there's
        // something to draw
        Ok(Bridge { conn, latest, running: false })
    }

    /// Is the KWin script loaded (reporting)?
    pub fn running(&self) -> bool {
        self.running
    }

    /// Load (and start) or unload the KWin script. Unloaded, it costs KWin
    /// nothing and no reports arrive.
    pub fn set_running(&mut self, on: bool) -> Result<(), String> {
        self.running = on;
        if !on {
            kwin(&self.conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT,)).map_err(|e| format!("kwin unloadScript: {e}"))?;
            if let Ok(mut l) = self.latest.0.lock() {
                *l = Pending::default();
            }
            return Ok(());
        }
        let conn = &self.conn;
        // write the script where KWin can read it, then (re)load and start it
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("xmas-tui");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("xmas-snow.js");
        std::fs::write(&path, JS).map_err(|e| e.to_string())?;
        let _ = kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT,));
        kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path.to_string_lossy().to_string(), SCRIPT))
            .map_err(|e| format!("kwin loadScript: {e}"))?;
        kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "start", &()).map_err(|e| format!("kwin start: {e}"))?;
        Ok(())
    }
}

fn plasmashellrc() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
        .join("plasmashellrc")
}

/// When plasmashellrc last changed (to notice panel edits).
pub fn plasmashellrc_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(plasmashellrc()).and_then(|m| m.modified()).ok()
}

/// Floating Plasma panels from ~/.config/plasmashellrc as
/// (min length, max length, thickness). A floating panel's window includes
/// an invisible margin around the bar; this lets snow sit on the bar itself.
pub fn floating_panels() -> Vec<(f32, f32, f32)> {
    let Ok(text) = std::fs::read_to_string(plasmashellrc()) else { return vec![] };
    // panel id -> (floating, min, max, thickness)
    let mut panels: std::collections::BTreeMap<String, (bool, f32, f32, f32)> = Default::default();
    let mut cur: Option<(String, bool)> = None; // (panel id, in [Defaults])
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            cur = line.strip_prefix("[PlasmaViews][Panel ").and_then(|r| {
                let (id, rest) = r.split_once(']')?;
                Some((id.to_string(), rest == "[Defaults]"))
            });
            if let Some((id, _)) = &cur {
                panels.entry(id.clone()).or_insert((false, 0.0, 0.0, 0.0));
            }
            continue;
        }
        let (Some((id, defaults)), Some((k, v))) = (&cur, line.split_once('=')) else { continue };
        let e = panels.get_mut(id).unwrap();
        let num = v.trim().parse::<f32>().unwrap_or(0.0);
        match (k.trim(), *defaults) {
            ("floating", false) => e.0 = v.trim() == "1",
            ("minLength", true) => e.1 = num,
            ("maxLength", true) => e.2 = num,
            ("thickness", true) => e.3 = num,
            _ => {}
        }
    }
    panels.into_values().filter(|p| p.0 && p.3 > 0.0).map(|p| (p.1, p.2, p.3)).collect()
}

/// Unload the KWin script (used on shutdown paths where `Drop` won't run).
pub fn unload_script() {
    if let Ok(conn) = zbus::blocking::Connection::session() {
        let _ = kwin(&conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT,));
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = kwin(&self.conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(SCRIPT,));
    }
}
