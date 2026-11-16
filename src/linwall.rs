//! Linux: making the village the desktop wallpaper, and the KDE lock screen.
//!
//! The wallpaper runs in kitty's background panel, started by a systemd
//! user service (scripts/xmas-wallpaper.sh). "Set as wallpaper" sets that up
//! from inside the app: it checks the desktop can do it, finds kitty (or
//! fetches the official build into ~/.local/kitty.app when there's none, or
//! it's too old), writes the launcher and the service, and starts it.
//!
//! The lock screen can't run a terminal, so it gets a looping video of the
//! village (or a still, without ffmpeg) through a small Plasma wallpaper
//! plugin, set as the lock screen's wallpaper in kscreenlockerrc.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LAUNCHER: &str = include_str!("../scripts/xmas-wallpaper.sh");
const UNIT: &str = include_str!("../scripts/xmas-tui.service");
const PLUGIN: &str = "org.xmastui.loop";

/// A job running in the background (setting up the wallpaper / the lock
/// screen), as the settings show it.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Job {
    #[default]
    Idle,
    Busy(String),
    Done(String),
    Failed(String),
}

static WALL: Mutex<Job> = Mutex::new(Job::Idle);
static LOCK: Mutex<Job> = Mutex::new(Job::Idle);

fn set(m: &Mutex<Job>, j: Job) {
    if let Ok(mut g) = m.lock() {
        *g = j;
    }
}

/// What the Desktop tab shows: looked up in the background every few
/// seconds (it runs systemctl and kitty), so drawing never waits on it.
#[derive(Clone, Debug, Default)]
pub struct Info {
    /// Looked at least once.
    pub known: bool,
    pub running: bool,
    pub needs: Option<String>,
    pub lock_ok: bool,
    pub lock_on: bool,
    pub ffmpeg: bool,
}

static INFO: Mutex<Option<(Info, Instant)>> = Mutex::new(None);
static LOOKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn info() -> Info {
    use std::sync::atomic::Ordering;
    let (cur, stale) = match INFO.lock().as_deref() {
        Ok(Some((i, at))) => (i.clone(), at.elapsed() > Duration::from_secs(4)),
        _ => (Info::default(), true),
    };
    if stale && !LOOKING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            let i = Info { known: true, running: wallpaper_running(), needs: wall_needs(), lock_ok: lock_supported(), lock_on: lock_on(), ffmpeg: on_path("ffmpeg").is_some() };
            if let Ok(mut g) = INFO.lock() {
                *g = Some((i, Instant::now()));
            }
            LOOKING.store(false, Ordering::SeqCst);
        });
    }
    cur
}

/// Look again soon (a job just finished).
fn stale() {
    if let Ok(mut g) = INFO.lock() {
        if let Some((_, at)) = g.as_mut() {
            *at = Instant::now() - Duration::from_secs(60);
        }
    }
}

/// The wallpaper's launcher in ~/.local/share/xmas-tui (set up by
/// get.sh or "set as wallpaper") follows the app when it updates itself:
/// the new one is used from the next start.
pub fn refresh_launcher() {
    let script = data_home().join("xmas-tui/scripts/xmas-wallpaper.sh");
    if std::fs::read_to_string(&script).is_ok_and(|s| s != LAUNCHER) {
        let tmp = script.with_extension("sh.new");
        if std::fs::write(&tmp, LAUNCHER).is_ok() && set_exec(&tmp).is_ok() {
            let _ = std::fs::rename(&tmp, &script);
        }
    }
}

/// The `xmas-tui` command on PATH (~/.local/bin/xmas-tui) for an app
/// installed into ~/.local/share/xmas-tui (older installs, and installers
/// that only unpack it there, left it off): a link to the app, unless
/// something else already goes by that name.
pub fn ensure_command() {
    let Ok(exe) = std::env::current_exe() else { return };
    let ours = data_home().join("xmas-tui/bin/xmas-tui");
    if exe.canonicalize().ok() != ours.canonicalize().ok() {
        return;
    }
    let link = home().join(".local/bin/xmas-tui");
    if link.symlink_metadata().is_err() {
        let _ = std::fs::create_dir_all(home().join(".local/bin"));
        let _ = std::os::unix::fs::symlink(&ours, &link);
    }
}

/// ffmpeg, from the system's package manager, through pkexec (the system
/// asks for the password; nothing runs as root without it). Blocks.
pub fn install_ffmpeg() -> Result<String, String> {
    let pms: [(&str, &[&str]); 4] = [("apt-get", &["install", "-y", "ffmpeg"]), ("dnf", &["install", "-y", "ffmpeg-free"]), ("pacman", &["-S", "--needed", "--noconfirm", "ffmpeg"]), ("zypper", &["install", "-y", "ffmpeg"])];
    let Some((pm, args)) = pms.iter().find_map(|(pm, a)| on_path(pm).map(|p| (p, *a))) else {
        return Err("no package manager found: install ffmpeg by hand".into());
    };
    if on_path("pkexec").is_none() {
        return Err(format!("needs pkexec — or run: sudo {} {}", pm.display(), args.join(" ")));
    }
    let ok = std::process::Command::new("pkexec").arg(&pm).args(args).status().map(|s| s.success()).unwrap_or(false);
    stale();
    if ok && on_path("ffmpeg").is_some() {
        Ok("ffmpeg's in: redraw the lock screen and it moves".into())
    } else {
        Err("ffmpeg wasn't installed".into())
    }
}

pub fn wall_job() -> Job {
    WALL.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn lock_job() -> Job {
    LOCK.lock().map(|g| g.clone()).unwrap_or_default()
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"))
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"))
}

fn on_path(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(cmd)).find(|f| f.is_file()))
}

fn run_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).stderr(Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ------------------------------------------------------------------ kitty

/// A kitty (its version; the launcher finds the same one).
#[derive(Clone, Debug)]
pub struct Kitty {
    pub ver: (u32, u32, u32),
}

fn version_of(bin: &Path) -> Option<(u32, u32, u32)> {
    let o = Command::new(bin).arg("--version").stderr(Stdio::null()).output().ok()?;
    parse_version(&String::from_utf8_lossy(&o.stdout))
}

/// "kitty 0.41.1 created by Kovid Goyal" → (0, 41, 1)
fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let v = s.split_whitespace().nth(1)?;
    let mut it = v.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

/// The official build fetched for the wallpaper (see `fetch_kitty`).
fn own_kitty_dir() -> PathBuf {
    home().join(".local/kitty.app")
}

/// The newest kitty around: on the PATH, or the official build in
/// ~/.local/kitty.app.
pub fn find_kitty() -> Option<Kitty> {
    let mut found: Vec<Kitty> = vec![];
    for bin in [on_path("kitty"), Some(own_kitty_dir().join("bin/kitty"))].into_iter().flatten() {
        if let Some(ver) = version_of(&bin) {
            found.push(Kitty { ver });
        }
    }
    found.into_iter().max_by_key(|k| k.ver)
}

fn x11() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "x11") || (std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some())
}

/// The oldest kitty whose panel can be the desktop background here: 0.34
/// on Wayland, 0.42 on X11.
fn min_kitty() -> (u32, u32, u32) {
    if x11() {
        (0, 42, 0)
    } else {
        (0, 34, 0)
    }
}

fn gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_uppercase().contains("GNOME"))
}

/// What setting up the wallpaper needs first, if anything: an explanation
/// for a desktop it can't work on, or that kitty will be fetched.
pub fn wall_needs() -> Option<String> {
    if gnome() {
        return Some("GNOME can't show it as the wallpaper (no layer-shell)".into());
    }
    match find_kitty() {
        Some(k) if k.ver >= min_kitty() => None,
        Some(k) => Some(format!("gets a newer kitty (yours is {}.{}.{})", k.ver.0, k.ver.1, k.ver.2)),
        None => Some("gets kitty, which draws it (about 30 MB)".into()),
    }
}

/// Is the wallpaper service running (or this the wallpaper itself)?
pub fn wallpaper_running() -> bool {
    std::env::var_os("XMAS_KITTY_SOCKET").is_some() || output("systemctl", &["--user", "is-active", "xmas-tui.service"]).is_some_and(|s| s == "active")
}

/// The official kitty build for this machine, from kitty's GitHub releases
/// (checked against the digest GitHub gives for it), into ~/.local/kitty.app.
fn fetch_kitty(job: &Mutex<Job>) -> Result<Kitty, String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        a => return Err(format!("no kitty build for {a}: install kitty 0.42 or newer yourself")),
    };
    set(job, Job::Busy("finding the latest kitty…".into()));
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).user_agent(concat!("xmas-tui/", env!("CARGO_PKG_VERSION"))).build();
    let body = agent
        .get("https://api.github.com/repos/kovidgoyal/kitty/releases/latest")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("couldn't reach GitHub ({e})"))?
        .into_string()
        .map_err(|e| format!("odd reply from GitHub ({e})"))?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("odd reply from GitHub ({e})"))?;
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.starts_with("kitty-") && n.ends_with(&format!("-{arch}.txz")))))
        .ok_or("no kitty build in its latest release")?;
    let url = asset["browser_download_url"].as_str().ok_or("no kitty download link")?;
    // GitHub lists a sha256 for each release file: no match, no install
    let want = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).ok_or("kitty's release has no checksum to check it against")?.to_lowercase();
    let size = asset["size"].as_u64().unwrap_or(40 << 20).max(1);
    let resp = agent.get(url).call().map_err(|e| format!("kitty download failed ({e})"))?;
    let mut r = resp.into_reader().take(300 << 20);
    let mut buf = Vec::with_capacity(size as usize);
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        let n = r.read(&mut chunk).map_err(|e| format!("kitty download failed ({e})"))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        set(job, Job::Busy(format!("getting kitty… {}%", (buf.len() as u64 * 100 / size).min(100))));
    }
    if crate::update::hex(&crate::update::sha256(&buf)) != want {
        return Err("the kitty download didn't match its checksum".into());
    }
    set(job, Job::Busy("unpacking kitty…".into()));
    let dir = own_kitty_dir();
    let tmp = dir.with_extension("app.new");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("can't make {} ({e})", tmp.display()))?;
    let mut tar = Command::new("tar").args(["-xJf", "-", "-C"]).arg(&tmp).stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("can't run tar ({e})"))?;
    tar.stdin.take().ok_or("tar")?.write_all(&buf).map_err(|e| format!("unpacking failed ({e})"))?;
    let out = tar.wait_with_output().map_err(|e| format!("unpacking failed ({e})"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("unpacking failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let ver = version_of(&tmp.join("bin/kitty")).ok_or("the kitty that was fetched doesn't run")?;
    let old = dir.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&old);
    if dir.exists() {
        std::fs::rename(&dir, &old).map_err(|e| format!("can't replace {} ({e})", dir.display()))?;
    }
    std::fs::rename(&tmp, &dir).map_err(|e| format!("can't put kitty in {} ({e})", dir.display()))?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(Kitty { ver })
}

// -------------------------------------------------------------- wallpaper

/// Set the village up as the wallpaper, in the background (see `wall_job`).
pub fn start_wallpaper() {
    if matches!(wall_job(), Job::Busy(_)) {
        return;
    }
    set(&WALL, Job::Busy("checking…".into()));
    std::thread::spawn(|| {
        let r = install_wallpaper(&WALL);
        set(&WALL, match r {
            Ok(m) => Job::Done(m),
            Err(e) => Job::Failed(e),
        });
        stale();
    });
}

/// `xmas-tui --set-wallpaper`: the same, from a terminal.
pub fn cli_set_wallpaper() -> i32 {
    let job = Mutex::new(Job::Idle);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            let _ = tx.send(install_wallpaper(&job));
        });
        let mut last = String::new();
        loop {
            if let Ok(r) = rx.recv_timeout(Duration::from_millis(200)) {
                return match r {
                    Ok(m) => {
                        println!("{m}");
                        0
                    }
                    Err(e) => {
                        println!("not set up: {e}");
                        1
                    }
                };
            }
            if let Ok(Job::Busy(m)) = job.lock().map(|g| g.clone()) {
                if m != last {
                    println!("{m}");
                    last = m;
                }
            }
        }
    })
}

fn install_wallpaper(job: &Mutex<Job>) -> Result<String, String> {
    if gnome() {
        return Err("GNOME doesn't let apps draw the desktop background (it has no layer-shell); xmas-tui can only run in a window there".into());
    }
    if on_path("systemctl").is_none() {
        return Err("needs systemd (systemctl) to keep the wallpaper running".into());
    }
    let kitty = match find_kitty() {
        Some(k) if k.ver >= min_kitty() => k,
        _ => fetch_kitty(job)?,
    };
    set(job, Job::Busy("setting up the wallpaper…".into()));
    let data = data_home().join("xmas-tui");
    // the binary: a system install (/usr) stays put; anything else (a
    // download, a build) is copied where the app updates itself
    let exe = std::env::current_exe().map_err(|e| format!("can't find this program ({e})"))?;
    let exe = exe.canonicalize().unwrap_or(exe);
    if !exe.starts_with("/usr/") {
        let dest = data.join("bin/xmas-tui");
        if dest.canonicalize().ok() != Some(exe.clone()) {
            std::fs::create_dir_all(data.join("bin")).map_err(|e| format!("can't make {} ({e})", data.display()))?;
            let tmp = dest.with_extension("new");
            std::fs::copy(&exe, &tmp).map_err(|e| format!("can't copy xmas-tui to {} ({e})", dest.display()))?;
            set_exec(&tmp)?;
            std::fs::rename(&tmp, &dest).map_err(|e| format!("can't copy xmas-tui to {} ({e})", dest.display()))?;
        }
    }
    let script = data.join("scripts/xmas-wallpaper.sh");
    std::fs::create_dir_all(data.join("scripts")).map_err(|e| format!("can't make {} ({e})", data.display()))?;
    std::fs::write(&script, LAUNCHER).map_err(|e| format!("can't write {} ({e})", script.display()))?;
    set_exec(&script)?;
    let units = config_home().join("systemd/user");
    std::fs::create_dir_all(&units).map_err(|e| format!("can't make {} ({e})", units.display()))?;
    let unit = UNIT.replace("@DIR@", &data.display().to_string());
    std::fs::write(units.join("xmas-tui.service"), unit).map_err(|e| format!("can't write the service ({e})"))?;
    for args in [&["--user", "daemon-reload"][..], &["--user", "enable", "xmas-tui.service"], &["--user", "restart", "xmas-tui.service"]] {
        if !run_ok("systemctl", args) {
            return Err(format!("systemctl {} failed", args[1..].join(" ")));
        }
    }
    // up and drawing? (the panel is kitty, with the xmas-tui class)
    set(job, Job::Busy("starting it…".into()));
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(500));
        if panel_running() {
            return Ok(format!("the village is the wallpaper now ✓ (kitty {}.{}.{})", kitty.ver.0, kitty.ver.1, kitty.ver.2));
        }
    }
    let why = output("journalctl", &["--user", "-u", "xmas-tui.service", "-n", "3", "-o", "cat", "--no-pager"]).and_then(|s| s.lines().filter(|l| !l.trim().is_empty()).last().map(String::from));
    Err(match why {
        Some(w) => format!("it didn't start: {w}"),
        None => "it didn't start (see: journalctl --user -u xmas-tui)".into(),
    })
}

fn set_exec(p: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("can't make {} runnable ({e})", p.display()))
}

/// A kitty panel running xmas-tui (its window class)?
fn panel_running() -> bool {
    let Ok(rd) = std::fs::read_dir("/proc") else { return false };
    rd.flatten().any(|e| {
        let Ok(cmd) = std::fs::read(e.path().join("cmdline")) else { return false };
        let s = String::from_utf8_lossy(&cmd).replace('\0', " ");
        s.contains("panel") && s.contains("xmas-tui") && s.contains("--wallpaper")
    })
}

// ------------------------------------------------------------ lock screen

fn kwrite() -> Option<&'static str> {
    ["kwriteconfig6", "kwriteconfig5"].into_iter().find(|c| on_path(c).is_some())
}

fn kread() -> Option<&'static str> {
    ["kreadconfig6", "kreadconfig5"].into_iter().find(|c| on_path(c).is_some())
}

/// Plasma 6 (whose lock screen takes wallpaper plugins written for Qt 6).
pub fn lock_supported() -> bool {
    on_path("kwriteconfig6").is_some() && on_path("kreadconfig6").is_some()
}

/// The village is the lock screen's wallpaper now?
pub fn lock_on() -> bool {
    kread().and_then(|k| output(k, &["--file", "kscreenlockerrc", "--group", "Greeter", "--key", "WallpaperPlugin"])).is_some_and(|p| p == PLUGIN)
}

/// Put the village on the lock screen (or back to what it was), in the
/// background (see `lock_job`).
pub fn start_lock(on: bool) {
    if matches!(lock_job(), Job::Busy(_)) {
        return;
    }
    set(&LOCK, Job::Busy("…".into()));
    std::thread::spawn(move || {
        let r = if on { install_lock(&LOCK) } else { remove_lock() };
        set(&LOCK, match r {
            Ok(m) => Job::Done(m),
            Err(e) => Job::Failed(e),
        });
        stale();
    });
}

/// `xmas-tui --lock-screen on|off`, from a terminal.
pub fn cli_lock(on: bool) -> i32 {
    let job = Mutex::new(Job::Idle);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            let _ = tx.send(if on { install_lock(&job) } else { remove_lock() });
        });
        let mut last = String::new();
        loop {
            if let Ok(r) = rx.recv_timeout(Duration::from_millis(300)) {
                return match r {
                    Ok(m) => {
                        println!("{m}");
                        0
                    }
                    Err(e) => {
                        println!("{e}");
                        1
                    }
                };
            }
            if let Ok(Job::Busy(m)) = job.lock().map(|g| g.clone()) {
                if m != last {
                    println!("{m}");
                    last = m;
                }
            }
        }
    })
}

fn plugin_dir() -> PathBuf {
    data_home().join("plasma/wallpapers").join(PLUGIN)
}

const METADATA: &str = r#"{
    "KPackageStructure": "Plasma/Wallpaper",
    "KPlugin": {
        "Authors": [ { "Name": "xmas-tui" } ],
        "Category": "",
        "Description": "The xmas-tui Christmas village, as a seamless looping video (made for the lock screen)",
        "Icon": "video-x-generic",
        "Id": "org.xmastui.loop",
        "License": "MIT",
        "Name": "xmas-tui village",
        "Version": "1.1"
    },
    "X-KDE-ParentApp": "org.kde.plasmashell",
    "X-Plasma-API-Minimum-Version": "6.0"
}
"#;

const MAIN_QML: &str = r##"/*
    xmas-tui lock screen wallpaper: the village as a seamless looping video.
    The first frame shows at once while the video spins up (and stays, if
    there's no video); the video is silent and loops forever.
*/
import QtQuick
import QtMultimedia
import org.kde.plasma.plasmoid

WallpaperItem {
    id: root

    Rectangle {
        anchors.fill: parent
        color: "#03050f"
    }

    Image {
        anchors.fill: parent
        source: Qt.resolvedUrl("../images/first.png")
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }

    Video {
        id: video
        anchors.fill: parent
        source: Qt.resolvedUrl("../video/xmas-tui-loop.mp4")
        fillMode: VideoOutput.PreserveAspectCrop
        loops: MediaPlayer.Infinite
        muted: true
        autoPlay: true
        // only show it once frames are coming (no black flash)
        opacity: playbackState === MediaPlayer.PlayingState ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 400 } }
    }
}
"##;

const CONFIG_QML: &str = r#"import QtQuick
import QtQuick.Controls as QQC2

Item {
    implicitHeight: label.implicitHeight
    QQC2.Label {
        id: label
        text: "The xmas-tui village, looping. Set it up from xmas-tui's settings (Desktop tab)."
    }
}
"#;

const MAIN_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kcfg xmlns="http://www.kde.org/standards/kcfg/1.0"
      xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
      xsi:schemaLocation="http://www.kde.org/standards/kcfg/1.0 http://www.kde.org/standards/kcfg/1.0/kcfg.xsd">
  <kcfgfile name=""/>
  <group name="General">
    <entry name="Muted" type="Bool"><default>true</default></entry>
  </group>
</kcfg>
"#;

/// The biggest connected screen's mode (from the kernel's DRM info).
fn screen_size() -> (u32, u32) {
    let mut best = (0, 0);
    if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
        for e in rd.flatten() {
            let p = e.path();
            if std::fs::read_to_string(p.join("status")).map(|s| s.trim() == "connected").unwrap_or(false) {
                if let Some((w, h)) = std::fs::read_to_string(p.join("modes")).ok().and_then(|m| {
                    let l = m.lines().next()?.to_string();
                    let (w, h) = l.split_once('x')?;
                    Some((w.parse::<u32>().ok()?, h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok()?))
                }) {
                    if w * h > best.0 * best.1 {
                        best = (w, h);
                    }
                }
            }
        }
    }
    if best.0 == 0 {
        (1920, 1080)
    } else {
        best
    }
}

/// The export's cell, in pixels (see export.rs).
const CELL: (u32, u32) = (9, 19);

fn install_lock(job: &Mutex<Job>) -> Result<String, String> {
    let kw = kwrite().filter(|_| lock_supported()).ok_or("the lock screen can only be set on KDE Plasma 6")?;
    let dir = plugin_dir();
    for d in ["contents/ui", "contents/config", "contents/images", "contents/video"] {
        std::fs::create_dir_all(dir.join(d)).map_err(|e| format!("can't make {} ({e})", dir.display()))?;
    }
    for (f, s) in [("metadata.json", METADATA), ("contents/ui/main.qml", MAIN_QML), ("contents/ui/config.qml", CONFIG_QML), ("contents/config/main.xml", MAIN_XML)] {
        std::fs::write(dir.join(f), s).map_err(|e| format!("can't write the lock screen plugin ({e})"))?;
    }
    // the picture: the village as the screen is shaped, at the wallpaper's
    // usual detail (75 rows)
    let (sw, sh) = screen_size();
    let rows = 75u32;
    let cols = ((sw as f32 / sh as f32) * (rows * CELL.1) as f32 / CELL.0 as f32).round() as u32;
    let exe = std::env::current_exe().map_err(|e| format!("can't find this program ({e})"))?;
    let tmp = std::env::temp_dir().join(format!("xmas-tui-lock-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    set(job, Job::Busy("drawing the village…".into()));
    let ok = Command::new("nice")
        .args(["-n", "19"])
        .arg(&exe)
        .arg("--export")
        .arg(tmp.join("still"))
        .args(["--size", &format!("{cols}x{rows}"), "--frames", "1", "--at", "60", "--clean"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let still = tmp.join("still/frame_0000.png");
    if !ok || !still.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("couldn't draw the village for the lock screen".into());
    }
    std::fs::copy(&still, dir.join("contents/images/first.png")).map_err(|e| format!("can't write the lock screen picture ({e})"))?;
    // and the loop, if ffmpeg is around to make it
    let video = dir.join("contents/video/xmas-tui-loop.mp4");
    let moving = match on_path("ffmpeg") {
        Some(_) => match make_video(job, &exe, &tmp, cols, rows, (sw, sh), &video) {
            Ok(()) => true,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(e);
            }
        },
        None => {
            let _ = std::fs::remove_file(&video);
            false
        }
    };
    let _ = std::fs::remove_dir_all(&tmp);
    // what it was before (put back when it's switched off)
    if let Some(prev) = kread().and_then(|k| output(k, &["--file", "kscreenlockerrc", "--group", "Greeter", "--key", "WallpaperPlugin"])) {
        if prev != PLUGIN && !prev.is_empty() {
            let _ = std::fs::write(data_home().join("xmas-tui/lock-screen-before"), prev);
        }
    }
    if !run_ok(kw, &["--file", "kscreenlockerrc", "--group", "Greeter", "--key", "WallpaperPlugin", PLUGIN]) {
        return Err("couldn't set the lock screen's wallpaper".into());
    }
    Ok(if moving { "the lock screen shows the village ✓ (Meta+L)".into() } else { "the lock screen shows the village ✓ (a still: ffmpeg makes it move)".into() })
}

/// Render the village as a seamless loop: 24 s at 24 fps, its last 3 s
/// crossfading into its first.
fn make_video(job: &Mutex<Job>, exe: &Path, tmp: &Path, cols: u32, rows: u32, screen: (u32, u32), out: &Path) -> Result<(), String> {
    const L: u32 = 24;
    const F: u32 = 3;
    const FPS: u32 = 24;
    let frames = (L + F) * FPS;
    let (pw, ph) = (cols * CELL.0, rows * CELL.1);
    let raw = tmp.join("raw.mkv");
    let mut render = Command::new("nice")
        .args(["-n", "19"])
        .arg(exe)
        .arg("--export")
        .arg(tmp.join("unused"))
        .args(["--size", &format!("{cols}x{rows}"), "--fps", &FPS.to_string(), "--at", "60", "--frames", &frames.to_string(), "--clean", "--raw"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't draw the village ({e})"))?;
    let scale = format!("scale={}:{}:flags=lanczos,format=yuv420p", screen.0, screen.1);
    let mut enc = Command::new("nice")
        .args(["-n", "19", "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{pw}x{ph}"), "-r", &FPS.to_string(), "-i", "-", "-vf", &scale, "-c:v", "libx264", "-preset", "ultrafast", "-crf", "8"])
        .arg(&raw)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run ffmpeg ({e})"))?;
    {
        let mut src = render.stdout.take().ok_or("render")?;
        let mut dst = enc.stdin.take().ok_or("ffmpeg")?;
        let total = pw as u64 * ph as u64 * 3 * frames as u64;
        let mut done = 0u64;
        let mut buf = vec![0u8; 1 << 20];
        let mut shown = 101;
        loop {
            let n = src.read(&mut buf).map_err(|e| format!("drawing failed ({e})"))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).map_err(|_| "ffmpeg stopped early".to_string())?;
            done += n as u64;
            let pct = (done * 100 / total.max(1)).min(100);
            if pct != shown {
                shown = pct;
                set(job, Job::Busy(format!("making the lock screen video… {}%", pct * 8 / 10)));
            }
        }
    }
    let _ = render.wait();
    let o = enc.wait_with_output().map_err(|e| format!("ffmpeg failed ({e})"))?;
    if !o.status.success() {
        return Err(format!("ffmpeg failed: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")));
    }
    set(job, Job::Busy("making the lock screen video… 85% (looping it)".into()));
    let fc = format!(
        "[0]trim=start={F},setpts=PTS-STARTPTS,fps={FPS}[body];[1]trim=start={a}:end={L},setpts=PTS-STARTPTS,fps={FPS}[head];[body][head]xfade=transition=fade:duration={F}:offset={a}[v]",
        a = L - F
    );
    let part = out.with_extension("mp4.part");
    let o = Command::new("nice")
        .args(["-n", "19", "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&raw)
        .args(["-itsoffset", &(L - F).to_string(), "-i"])
        .arg(&raw)
        .args(["-filter_complex", &fc, "-map", "[v]", "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-pix_fmt", "yuv420p", "-an", "-movflags", "+faststart", "-f", "mp4"])
        .arg(&part)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ffmpeg failed ({e})"))?;
    if !o.status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("ffmpeg failed: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")));
    }
    std::fs::rename(&part, out).map_err(|e| format!("can't write the lock screen video ({e})"))
}

fn remove_lock() -> Result<String, String> {
    let kw = kwrite().ok_or("no kwriteconfig")?;
    let prev = std::fs::read_to_string(data_home().join("xmas-tui/lock-screen-before")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != PLUGIN).unwrap_or_else(|| "org.kde.image".into());
    if !run_ok(kw, &["--file", "kscreenlockerrc", "--group", "Greeter", "--key", "WallpaperPlugin", &prev]) {
        return Err("couldn't change the lock screen back".into());
    }
    Ok("the lock screen is back to its usual wallpaper".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_versions_parse() {
        assert_eq!(parse_version("kitty 0.41.1 created by Kovid Goyal"), Some((0, 41, 1)));
        assert_eq!(parse_version("kitty 0.50.0 created by Kovid Goyal\n"), Some((0, 50, 0)));
        assert_eq!(parse_version("nonsense"), None);
        assert!((0, 41, 1) < (0, 42, 0) && (0, 41, 1) >= (0, 34, 0));
    }
}
