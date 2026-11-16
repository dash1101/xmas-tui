//! Shelves: widgets that each show a folder's files and shortcuts, so the
//! desktop can stay tidy. Everything on a shelf is an ordinary file in its
//! folder ("Desktop Shelves/<name>" in Documents unless another folder is
//! picked): open the folder in Explorer or Dolphin and it's all there.
//!
//! A worker thread scans the folders (every couple of seconds, and right
//! after a move), loads icons, and does the moves. On Windows it also talks
//! to the desktop (see winshelf.rs) for dragging icons onto and off shelves.

use super::{clip, Rect, PANEL};
use crate::color::{pal, Rgb};
use crate::config::ShelfCfg;
use crate::frame::{Bg, Frame, Glyph, Picture};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime};

/// An item's slot on the shelf: an 8×8 icon (8 columns × 4 rows of half
/// blocks), its name under it, and a row to breathe (or a plank).
pub const SLOT_W: i32 = 11;
pub const SLOT_H: i32 = 6;
pub const ICON: usize = 8;
/// The smallest a shelf can be made (cells).
pub const MIN_W: i32 = 15;
pub const MIN_H: i32 = 8;
pub const STYLES: [&str; 3] = ["frosted", "wood", "gift"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Folder,
    App,
    Doc,
    Image,
    Music,
    Video,
    Archive,
    Link,
}

pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// The real icon, when there is one (Windows: the shell's; Linux: an
    /// app's PNG icon).
    pub icon: Option<Arc<Picture>>,
    /// The icon as 8×8 pixel art (None: see-through).
    pub pixels: [Option<Rgb>; ICON * ICON],
    stamp: (Option<SystemTime>, u64),
}

/// What's on a shelf.
#[derive(Clone, Default)]
pub struct Contents {
    pub folder: PathBuf,
    pub items: Vec<Arc<Item>>,
    pub ready: bool,
    pub error: Option<String>,
}

pub enum Cmd {
    /// The shelves' folders changed (index = shelf).
    Folders(Vec<PathBuf>),
    /// Move files into a folder; on Windows, then place them on the
    /// desktop at a screen point (when moving onto the desktop).
    Move { paths: Vec<PathBuf>, to: PathBuf, place: Option<(i32, i32)> },
    /// Windows: desktop icons were dropped on shelf `shelf` (whatever is
    /// selected on the desktop goes in).
    #[cfg(windows)]
    DesktopDrop { to: PathBuf },
}

/// The worker: its command channel and what it found.
pub struct Worker {
    pub tx: mpsc::Sender<Cmd>,
    pub data: Arc<Mutex<Vec<Contents>>>,
}

pub fn spawn() -> Worker {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let data = Arc::new(Mutex::new(Vec::<Contents>::new()));
    let d = data.clone();
    let _ = std::thread::Builder::new().name("shelves".into()).spawn(move || run(rx, d));
    Worker { tx, data }
}

fn run(rx: mpsc::Receiver<Cmd>, data: Arc<Mutex<Vec<Contents>>>) {
    #[cfg(windows)]
    crate::winshelf::com_init();
    let mut folders: Vec<PathBuf> = vec![];
    // items by path, kept while unchanged (icons are the slow part)
    let mut cache: std::collections::HashMap<PathBuf, Arc<Item>> = Default::default();
    loop {
        let cmd = match rx.recv_timeout(Duration::from_millis(2000)) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        match cmd {
            Some(Cmd::Folders(f)) => folders = f,
            None => {}
            Some(Cmd::Move { paths, to, place }) => {
                let moved = move_files(&paths, &to);
                #[cfg(windows)]
                if let (Some(at), Some(first)) = (place, moved.first()) {
                    place_on_desktop(first, at);
                }
                let _ = place;
                let _ = moved;
            }
            #[cfg(windows)]
            Some(Cmd::DesktopDrop { to }) => desktop_drop(&to),
        }
        let mut out = Vec::with_capacity(folders.len());
        let mut seen = std::collections::HashSet::new();
        for f in &folders {
            out.push(scan(f, &mut cache, &mut seen));
        }
        cache.retain(|p, _| seen.contains(p));
        if let Ok(mut d) = data.lock() {
            *d = out;
        }
    }
}

/// The folder's items, folders first, then by name.
fn scan(folder: &Path, cache: &mut std::collections::HashMap<PathBuf, Arc<Item>>, seen: &mut std::collections::HashSet<PathBuf>) -> Contents {
    let rd = match std::fs::read_dir(folder) {
        Ok(rd) => rd,
        Err(e) => return Contents { folder: folder.to_path_buf(), items: vec![], ready: true, error: Some(format!("{}: {e}", folder.display())) },
    };
    let mut items = vec![];
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if hidden(&name, &e) {
            continue;
        }
        let meta = e.metadata().ok();
        let stamp = (meta.as_ref().and_then(|m| m.modified().ok()), meta.as_ref().map(|m| m.len()).unwrap_or(0));
        seen.insert(path.clone());
        let item = match cache.get(&path) {
            Some(it) if it.stamp == stamp => it.clone(),
            _ => {
                let dir = meta.as_ref().is_some_and(|m| m.is_dir());
                let it = Arc::new(make_item(&path, &name, dir, stamp));
                cache.insert(path.clone(), it.clone());
                it
            }
        };
        items.push(item);
    }
    items.sort_by(|a, b| (a.kind != Kind::Folder).cmp(&(b.kind != Kind::Folder)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Contents { folder: folder.to_path_buf(), items, ready: true, error: None }
}

fn hidden(name: &str, _e: &std::fs::DirEntry) -> bool {
    if name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // hidden or system
        if _e.metadata().is_ok_and(|m| m.file_attributes() & 0x6 != 0) {
            return true;
        }
    }
    false
}

fn make_item(path: &Path, file: &str, dir: bool, stamp: (Option<SystemTime>, u64)) -> Item {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let kind = if dir { Kind::Folder } else { kind_of(&ext) };
    let mut name = file.to_string();
    if matches!(ext.as_str(), "lnk" | "url") {
        name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name);
    }
    #[cfg(windows)]
    let icon: Option<Arc<Picture>> = crate::winshelf::icon(path, 48).map(Arc::new);
    #[cfg(not(windows))]
    let mut icon: Option<Arc<Picture>> = None;
    #[cfg(not(windows))]
    if ext == "desktop" {
        if let Some((n, i)) = desktop_entry(path) {
            if !n.is_empty() {
                name = n;
            }
            icon = i.and_then(|i| find_icon(&i)).map(Arc::new);
        }
    } else {
        // pictures, music with cover art, album folders with a cover
        icon = super::art::thumb_or_folder(path, dir).map(Arc::new);
    }
    let pixels = match &icon {
        Some(p) => pixel_art(p),
        None => sprite(kind),
    };
    Item { name, path: path.to_path_buf(), kind, icon, pixels, stamp }
}

fn kind_of(ext: &str) -> Kind {
    match ext {
        "exe" | "lnk" | "desktop" | "appimage" | "bat" | "cmd" | "msi" | "sh" => Kind::App,
        "url" | "webloc" => Kind::Link,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "heic" => Kind::Image,
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "opus" | "aac" => Kind::Music,
        "mp4" | "mkv" | "webm" | "mov" | "avi" => Kind::Video,
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "zst" | "bz2" => Kind::Archive,
        _ => Kind::Doc,
    }
}

/// An icon shrunk to 8×8 pixel art: each pixel the alpha-weighted average
/// of its block, see-through where the block mostly is.
pub fn pixel_art(p: &Picture) -> [Option<Rgb>; ICON * ICON] {
    let mut out = [None; ICON * ICON];
    for ty in 0..ICON {
        for tx in 0..ICON {
            let (x0, x1) = (tx * p.w / ICON, ((tx + 1) * p.w / ICON).max(tx * p.w / ICON + 1));
            let (y0, y1) = (ty * p.h / ICON, ((ty + 1) * p.h / ICON).max(ty * p.h / ICON + 1));
            let (mut r, mut g, mut b, mut a, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for y in y0..y1.min(p.h) {
                for x in x0..x1.min(p.w) {
                    let i = (y * p.w + x) * 4;
                    let al = p.rgba[i + 3] as f32 / 255.0;
                    r += p.rgba[i] as f32 * al;
                    g += p.rgba[i + 1] as f32 * al;
                    b += p.rgba[i + 2] as f32 * al;
                    a += al;
                    n += 1.0;
                }
            }
            if n > 0.0 && a / n >= 0.42 {
                out[ty * ICON + tx] = Some(Rgb { r: r / a / 255.0, g: g / a / 255.0, b: b / a / 255.0 });
            }
        }
    }
    out
}

/// A small drawing for items without an icon of their own.
fn sprite(k: Kind) -> [Option<Rgb>; ICON * ICON] {
    let (rows, c): ([&str; 8], u32) = match k {
        Kind::Folder => (["........", "xxx.....", "xxxxxxxx", "xooooooxx", "xxxxxxxx", "xxxxxxxx", "xxxxxxxx", "........"], 0xe8c278),
        Kind::App => (["xxxxxxxx", "xoxoxxxx", "xxxxxxxx", "x......x", "x.xx...x", "x......x", "xxxxxxxx", "........"], 0x7fd8ff),
        Kind::Image => (["xxxxxxxx", "x.....ox", "x......x", "x..x...x", "x.xxx.xx", "xxxxxxxx", "xxxxxxxx", "........"], 0x5adf8a),
        Kind::Music => (["....xxx.", "....x.x.", "....x...", "....x...", "..xxx...", ".xxxx...", ".xxx....", "........"], 0xff7ae0),
        Kind::Video => (["xxxxxxxx", "x.x.x.x.", "xxxxxxxx", "x..x...x", "x..xx..x", "x..x...x", "xxxxxxxx", "........"], 0xff8a5a),
        Kind::Archive => (["xxxxxxxx", "x..o...x", "xxxoxxxx", "x..o...x", "x..o...x", "x......x", "xxxxxxxx", "........"], 0xc8a070),
        Kind::Link => ([".xxxxxx.", "xx....xx", "x..xx..x", "x.x..x.x", "x..xx..x", "xx....xx", ".xxxxxx.", "........"], 0x6aa8ff),
        Kind::Doc => ([".xxxxx..", ".x...xx.", ".x.oo.x.", ".x....x.", ".x.oo.x.", ".x....x.", ".xxxxxx.", "........"], 0xdfe7f4),
    };
    let base = Rgb::hex(c);
    let mut out = [None; ICON * ICON];
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().take(ICON).enumerate() {
            out[y * ICON + x] = match ch {
                'x' => Some(base),
                'o' => Some(base * 0.55),
                _ => None,
            };
        }
    }
    out
}

// ---------------------------------------------------------------------------
// folders and files

/// Documents and Desktop (Linux: from user-dirs.dirs).
pub fn documents_dir() -> PathBuf {
    #[cfg(windows)]
    {
        crate::winshelf::documents_dir().map(PathBuf::from).unwrap_or_else(|| home().join("Documents"))
    }
    #[cfg(not(windows))]
    {
        xdg_dir("XDG_DOCUMENTS_DIR").unwrap_or_else(|| home().join("Documents"))
    }
}

pub fn desktop_dir() -> PathBuf {
    #[cfg(windows)]
    {
        crate::winshelf::desktop_dir().map(PathBuf::from).unwrap_or_else(|| home().join("Desktop"))
    }
    #[cfg(not(windows))]
    {
        xdg_dir("XDG_DESKTOP_DIR").unwrap_or_else(|| home().join("Desktop"))
    }
}

pub fn home_dir() -> PathBuf {
    home()
}

/// A new shelf (named "Shelf", "Shelf 2"…), at a spot given as fractions
/// of the screen, or the default one.
pub fn add(cfg: &mut crate::config::Config, pos: Option<[f32; 2]>) {
    let mut name = "Shelf".to_string();
    let mut n = 2;
    while cfg.shelves.iter().any(|c| c.name == name) {
        name = format!("Shelf {n}");
        n += 1;
    }
    cfg.shelves.push(ShelfCfg { name, pos, ..Default::default() });
}

/// Rename shelf `i`. A shelf in its default folder takes the folder along
/// (renamed on disk), so its files stay on it.
pub fn rename(cfg: &mut crate::config::Config, i: usize, name: &str) {
    let name: String = name.trim().chars().take(24).collect();
    let Some(c) = cfg.shelves.get_mut(i) else { return };
    if name.is_empty() || name == c.name {
        return;
    }
    if c.folder.trim().is_empty() {
        let old = folder_of(c);
        let new = folder_of(&ShelfCfg { name: name.clone(), ..c.clone() });
        // folder_of made the new one: put the old one in its place
        let moved = std::fs::read_dir(&new).map(|mut d| d.next().is_none()).unwrap_or(true) && std::fs::remove_dir(&new).is_ok() && std::fs::rename(&old, &new).is_ok();
        if !moved {
            // keep showing the old folder
            c.folder = old.to_string_lossy().to_string();
        }
    }
    c.name = name;
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

#[cfg(not(windows))]
fn xdg_dir(key: &str) -> Option<PathBuf> {
    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    let text = std::fs::read_to_string(cfg.join("user-dirs.dirs")).ok()?;
    let line = text.lines().find(|l| l.trim_start().starts_with(key))?;
    let v = line.split_once('=')?.1.trim().trim_matches('"');
    Some(PathBuf::from(v.replace("$HOME", &home().to_string_lossy())))
}

/// The folder a shelf shows (made if it's the default one and missing).
pub fn folder_of(cfg: &ShelfCfg) -> PathBuf {
    if !cfg.folder.trim().is_empty() {
        return PathBuf::from(cfg.folder.trim());
    }
    let safe: String = cfg.name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let safe = if safe.trim().is_empty() { "Shelf".to_string() } else { safe.trim().to_string() };
    let p = documents_dir().join("Desktop Shelves").join(safe);
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Move files into `to`; a taken name gets " (2)", " (3)"…. Returns where
/// each went.
fn move_files(paths: &[PathBuf], to: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(to);
    #[cfg(windows)]
    {
        // through the shell: Explorer's own renaming, and Ctrl+Z undoes it
        let list: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let before: Vec<PathBuf> = std::fs::read_dir(to).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        let _ = crate::winshelf::move_into(&list, &to.to_string_lossy());
        let after: Vec<PathBuf> = std::fs::read_dir(to).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        after.into_iter().filter(|p| !before.contains(p)).collect()
    }
    #[cfg(not(windows))]
    {
        let mut out = vec![];
        for p in paths {
            let Some(file) = p.file_name() else { continue };
            if p.parent() == Some(to) {
                continue;
            }
            let mut dest = to.join(file);
            let (stem, ext) = (p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());
            let mut k = 2;
            while dest.exists() {
                dest = to.join(format!("{stem} ({k}){ext}"));
                k += 1;
            }
            let ok = std::fs::rename(p, &dest).is_ok() || (p.is_file() && std::fs::copy(p, &dest).is_ok() && std::fs::remove_file(p).is_ok());
            if ok {
                out.push(dest);
            }
        }
        out
    }
}

#[cfg(windows)]
fn place_on_desktop(path: &Path, at: (i32, i32)) {
    // the desktop picks a new file up a moment after it appears
    for _ in 0..20 {
        if let Ok(d) = crate::winshelf::Desktop::open() {
            if d.place(&path.to_string_lossy(), at) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Windows: what's selected on the desktop (the icons just dragged) goes
/// onto the shelf. Only the user's own Desktop: the shared one needs admin.
#[cfg(windows)]
fn desktop_drop(to: &Path) {
    let Ok(d) = crate::winshelf::Desktop::open() else { return };
    let mine = desktop_dir();
    let paths: Vec<PathBuf> = d.icons(true).into_iter().map(|i| PathBuf::from(i.path)).filter(|p| p.parent().is_some_and(|q| q == mine)).collect();
    if !paths.is_empty() {
        let moved = move_files(&paths, to);
        eprintln!("xmas-tui: shelved {} of {} from the desktop into {}", moved.len(), paths.len(), to.display());
    }
}

/// The system's folder picker (blocks until it's answered; None when
/// cancelled, or when there's no picker to be had).
pub fn pick_folder(start: &Path) -> Option<PathBuf> {
    #[cfg(test)]
    {
        let _ = start;
        None
    }
    #[cfg(all(not(test), windows))]
    {
        crate::winshelf::com_init();
        crate::winshelf::pick_folder(start)
    }
    #[cfg(all(not(test), not(windows)))]
    {
        use std::process::Command;
        let s = start.to_string_lossy().to_string();
        let tries: [(&str, Vec<String>); 2] = [
            ("kdialog", vec!["--title".into(), "A folder for the shelf".into(), "--getexistingdirectory".into(), s.clone()]),
            ("zenity", vec!["--file-selection".into(), "--directory".into(), "--title=A folder for the shelf".into(), format!("--filename={s}/")]),
        ];
        for (cmd, args) in tries {
            if !which(cmd) {
                continue;
            }
            let out = Command::new(cmd).args(&args).output().ok()?;
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return (out.status.success() && !p.is_empty()).then(|| PathBuf::from(p));
        }
        None
    }
}

/// Open an item the way the desktop would.
pub fn open(path: &Path) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = |s: &std::ffi::OsStr| -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;
            s.encode_wide().chain(std::iter::once(0)).collect()
        };
        let (verb, file) = (wide("open".as_ref()), wide(path.as_os_str()));
        let dir = path.parent().map(|p| wide(p.as_os_str()));
        unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), dir.as_ref().map(|d| d.as_ptr()).unwrap_or(std::ptr::null()), SW_SHOWNORMAL);
        }
    }
    #[cfg(not(windows))]
    launch(&[path.as_os_str()]);
}

/// Show an item in its folder.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        if which("dolphin") {
            run_scoped(&[std::ffi::OsStr::new("dolphin"), std::ffi::OsStr::new("--select"), path.as_os_str()]);
        } else if let Some(dir) = path.parent() {
            launch(&[dir.as_os_str()]);
        }
    }
}

/// Linux: open a file or launch a .desktop entry, through KDE's launcher
/// (or gio, or xdg-open), in a systemd scope of its own, so it lives on
/// when the wallpaper restarts (its service's processes are stopped with it).
#[cfg(not(windows))]
fn launch(args: &[&std::ffi::OsStr]) {
    let Some(target) = args.first() else { return };
    let desktop = Path::new(target).extension().is_some_and(|e| e == "desktop");
    use std::ffi::OsStr;
    let mut cmd: Vec<&OsStr> = vec![];
    if which("kioclient") {
        cmd.extend([OsStr::new("kioclient"), OsStr::new("exec")]);
    } else if which("gio") {
        cmd.extend([OsStr::new("gio"), OsStr::new(if desktop { "launch" } else { "open" })]);
    } else {
        cmd.push(OsStr::new("xdg-open"));
    }
    cmd.push(target);
    run_scoped(&cmd);
}

#[cfg(not(windows))]
fn run_scoped(cmd: &[&std::ffi::OsStr]) {
    use std::process::{Command, Stdio};
    let mut c = if which("systemd-run") {
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--scope", "--collect", "--quiet", "--"]);
        c.args(cmd);
        c
    } else {
        let mut c = Command::new(cmd[0]);
        c.args(&cmd[1..]);
        c
    };
    let _ = c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

#[cfg(not(windows))]
fn which(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// Linux: a .desktop entry's name and icon.
#[cfg(not(windows))]
fn desktop_entry(path: &Path) -> Option<(String, Option<String>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let (mut name, mut icon, mut in_entry) = (String::new(), None, false);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = l.strip_prefix("Name=") {
            if name.is_empty() {
                name = v.to_string();
            }
        } else if let Some(v) = l.strip_prefix("Icon=") {
            icon = Some(v.to_string());
        }
    }
    Some((name, icon))
}

/// Linux: an icon by name (or path), as a PNG from the icon themes.
#[cfg(not(windows))]
fn find_icon(name: &str) -> Option<Picture> {
    if name.starts_with('/') {
        return load_png(Path::new(name));
    }
    let home = home();
    let data_dirs = [
        home.join(".local/share/icons"),
        home.join(".icons"),
        home.join(".local/share/flatpak/exports/share/icons"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/icons"),
    ];
    for size in ["48x48", "64x64", "32x32", "96x96", "128x128", "256x256", "24x24"] {
        for d in &data_dirs {
            for theme in ["hicolor", "breeze", "Adwaita"] {
                let p = d.join(theme).join(size).join("apps").join(format!("{name}.png"));
                if let Some(pic) = load_png(&p) {
                    return Some(pic);
                }
            }
        }
    }
    load_png(&PathBuf::from("/usr/share/pixmaps").join(format!("{name}.png")))
}

#[cfg(not(windows))]
fn load_png(path: &Path) -> Option<Picture> {
    let f = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&c| [c, c, c, 255]).collect(),
        _ => return None,
    };
    Some(Picture::new(w, h, rgba))
}

// ---------------------------------------------------------------------------
// layout and drawing

/// What the shelf menus can do: (shelf, item) or shelf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuAct {
    Open(usize, usize),
    Reveal(usize, usize),
    TakeOff(usize, usize),
    OpenFolder(usize),
    Style(usize),
    Icons(usize),
    Roll(usize),
    Remove(usize),
    /// Pick shelf s's folder with the system's folder picker.
    PickFolder(usize),
    /// A new shelf with its top-left at this cell (in a folder of its own,
    /// or one picked).
    New(i32, i32),
    NewFrom(i32, i32),
}

/// Where a press on a shelf landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// The title row (drag: move; click: roll up / down).
    Title,
    /// An edge or corner to resize by: a mix of the EDGE_* bits.
    Edge(u8),
    /// The scrollbar, this many rows down it.
    Bar(i32),
    Item(usize),
    Body,
}

pub const EDGE_L: u8 = 1;
pub const EDGE_R: u8 = 2;
pub const EDGE_T: u8 = 4;
pub const EDGE_B: u8 = 8;

/// The item grid inside a shelf: top-left of the first slot, columns, and
/// rows that fit.
pub fn grid(r: Rect) -> (i32, i32, i32, i32) {
    let (iw, ih) = (r.w - 4, r.h - 2);
    let cols = (iw / SLOT_W).max(1);
    let rows = (ih / SLOT_H).max(1);
    let x0 = r.x + 2 + (iw - cols * SLOT_W).max(0) / 2;
    (x0, r.y + 1, cols, rows)
}

/// How many rows of items need scrolling past.
pub fn max_scroll(r: Rect, n: usize) -> usize {
    let (_, _, cols, rows) = grid(r);
    let need = n.div_ceil(cols as usize);
    need.saturating_sub(rows as usize)
}

/// The scrollbar's column, and its length (rows), for a shelf that needs one.
pub fn bar(r: Rect, n: usize) -> Option<(i32, i32)> {
    (max_scroll(r, n) > 0 && r.h >= 5).then_some((r.x + r.w - 2, r.h - 2))
}

/// The scrollbar's thumb: first row (from the bar's top) and length.
pub fn thumb(r: Rect, n: usize, scroll: usize) -> (i32, i32) {
    let (_, _, cols, rows) = grid(r);
    let total = n.div_ceil(cols as usize).max(1) as i32;
    let len = r.h - 2;
    let size = (len * rows / total).clamp(1, len);
    let max = max_scroll(r, n).max(1) as i32;
    ((len - size) * scroll.min(max as usize) as i32 / max, size)
}

/// The scroll position for a press `row` rows down the bar (centring the thumb there).
pub fn scroll_at(r: Rect, n: usize, row: i32) -> usize {
    let (_, size) = thumb(r, n, 0);
    let len = r.h - 2;
    let max = max_scroll(r, n) as i32;
    if len <= size {
        return 0;
    }
    (((row - size / 2) as f32 / (len - size) as f32 * max as f32).round() as i32).clamp(0, max) as usize
}

/// The slot an item dropped at (x, y) goes to (for rearranging), 0..n-1.
pub fn slot_at(r: Rect, n: usize, scroll: usize, x: i32, y: i32) -> usize {
    let (x0, y0, cols, rows) = grid(r);
    let col = ((x - x0).max(0) / SLOT_W).min(cols - 1);
    let row = ((y - y0).max(0) / SLOT_H).min(rows - 1);
    ((row as usize + scroll) * cols as usize + col as usize).min(n.saturating_sub(1))
}

pub fn hit(r: Rect, rolled: bool, n: usize, scroll: usize, x: i32, y: i32) -> Option<Hit> {
    if !r.contains(x, y) {
        return None;
    }
    let (left, right, bottom) = (x == r.x, x == r.x + r.w - 1, y == r.y + r.h - 1);
    if y == r.y {
        // the corners of the title row resize; the rest moves / rolls up
        return Some(match (rolled, left, right) {
            (false, true, _) => Hit::Edge(EDGE_L | EDGE_T),
            (false, _, true) => Hit::Edge(EDGE_R | EDGE_T),
            _ => Hit::Title,
        });
    }
    let e = (left as u8 * EDGE_L) | (right as u8 * EDGE_R) | (bottom as u8 * EDGE_B);
    if e != 0 {
        return Some(Hit::Edge(e));
    }
    if let Some((bx, _)) = bar(r, n) {
        if x == bx {
            return Some(Hit::Bar(y - r.y - 1));
        }
    }
    let (x0, y0, cols, rows) = grid(r);
    let (cx, cy) = (x - x0, y - y0);
    if cx >= 0 && cy >= 0 && cx < cols * SLOT_W && cy < rows * SLOT_H && cy % SLOT_H < SLOT_H - 1 {
        let k = (cy / SLOT_H + scroll as i32) as usize * cols as usize + (cx / SLOT_W) as usize;
        if k < n && cx % SLOT_W < SLOT_W - 1 {
            return Some(Hit::Item(k));
        }
    }
    Some(Hit::Body)
}

/// How the shelf is being shown this frame.
pub struct View<'a> {
    pub cfg: &'a ShelfCfg,
    pub contents: Option<&'a Contents>,
    pub scroll: usize,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
    /// An item being dragged off (drawn faded in its slot).
    pub lifted: Option<usize>,
    /// Something's being dragged over the shelf (it lights up).
    pub target: bool,
    /// An item of its own being dragged about: where it would go.
    pub insert: Option<usize>,
    pub locked: bool,
    pub seed: u32,
}

const SNOW: Rgb = Rgb::hex(0xe6edf8);
const WOOD: Rgb = Rgb::hex(0x7a4a28);
const WOOD_DARK: Rgb = Rgb::hex(0x4a2a16);
const GOLD: Rgb = Rgb::hex(0xffd36b);
const GIFT: Rgb = Rgb::hex(0x9a1c28);

fn h(x: i32, seed: u32) -> u32 {
    let mut v = (x as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2c1b_3c6d);
    v ^ (v >> 12)
}

pub fn draw(f: &mut Frame, r: Rect, v: &View, t: f32) {
    let style = if STYLES.contains(&v.cfg.style.as_str()) { v.cfg.style.as_str() } else { "frosted" };
    let n = v.contents.map(|c| c.items.len()).unwrap_or(0);
    let body = match style {
        "wood" => Bg::Mix(Rgb::hex(0x24150c), 0.88),
        "gift" => Bg::Mix(GIFT * 0.55, 0.9),
        _ => PANEL,
    };
    let body = if v.target { Bg::Mix(Rgb::hex(0x3a3060), 0.9) } else { body };
    // the box
    for yy in r.y..r.y + r.h {
        for xx in r.x..r.x + r.w {
            f.put_g(xx, yy, Glyph::new(' ', pal::UI_TEXT).bg(body).lit(0.0));
        }
    }
    match style {
        "wood" => frame_wood(f, r, v, body),
        "gift" => frame_gift(f, r, v, body, t),
        _ => frame_frosted(f, r, v, body),
    }
    // the title (and how many items), on the top edge
    let count = if n > 0 { format!(" {n} ") } else { String::new() };
    let mark = if v.cfg.rolled { '▸' } else if style == "gift" { '✿' } else { '❄' };
    let title_bg = match style {
        "wood" => Bg::Solid(WOOD),
        "gift" => Bg::Solid(GIFT),
        _ => body,
    };
    let room = (r.w - 8 - count.chars().count() as i32).max(1) as usize;
    let title = clip(&v.cfg.name, room);
    let tx = if style == "gift" { r.x + 3 } else { r.x + 2 };
    let c = f.text(tx, r.y, " ", pal::UI_BORDER, title_bg, 0.0);
    f.put_g(c, r.y, Glyph::new(mark, GOLD).bg(title_bg).lit(0.0));
    let c = f.text(c + 1, r.y, " ", pal::UI_BORDER, title_bg, 0.0);
    let c = f.text(c, r.y, &title, if style == "frosted" { pal::UI_TITLE } else { Rgb::hex(0xfff1d6) }, title_bg, 0.0);
    f.text(c, r.y, " ", pal::UI_BORDER, title_bg, 0.0);
    if v.cfg.rolled {
        if !count.is_empty() {
            f.text(r.x + r.w - 2 - count.chars().count() as i32, r.y, &count, pal::UI_DIM, title_bg, 0.0);
        }
        return;
    }
    let scroll = v.scroll.min(max_scroll(r, n));
    if !count.is_empty() && r.w > 16 {
        f.text(r.x + r.w - 2 - count.chars().count() as i32, r.y, &count, pal::UI_DIM, title_bg, 0.0);
    }
    // more than fits: a scrollbar inside the right edge, and which items
    // are showing on the bottom edge
    if let Some((bx, len)) = bar(r, n) {
        let (t0, size) = thumb(r, n, scroll);
        for k in 0..len {
            let on = k >= t0 && k < t0 + size;
            let g = if on { Glyph::new('┃', GOLD) } else { Glyph::new('│', pal::UI_DIM * 0.6) };
            f.put_g(bx, r.y + 1 + k, g.bg(body).lit(0.0));
        }
        let (_, _, cols, rows) = grid(r);
        let first = scroll * cols as usize + 1;
        let last = ((scroll + rows as usize) * cols as usize).min(n);
        let range = format!(" {first}–{last} of {n} ");
        let rx = r.x + r.w - 3 - range.chars().count() as i32;
        if rx > r.x + 2 {
            let bottom_bg = if style == "wood" { Bg::Solid(WOOD_DARK) } else { body };
            f.text(rx, r.y + r.h - 1, &range, pal::UI_DIM * 1.2, bottom_bg, 0.0);
        }
    }
    if !v.locked {
        f.put_g(r.x + r.w - 1, r.y + r.h - 1, Glyph::new('◢', GOLD * 0.8).bg(title_bg).lit(0.0));
    }
    // the items
    let (x0, y0, cols, rows) = grid(r);
    let Some(c) = v.contents else { return };
    if let Some(e) = &c.error {
        f.text(r.x + 2, r.y + 2, &clip(e, (r.w - 4).max(1) as usize), pal::UI_DIM, body, 0.0);
        return;
    }
    if c.items.is_empty() && c.ready {
        let msg = if cfg!(windows) { "empty · drag desktop icons here" } else { "empty · move files into its folder" };
        let msg = clip(msg, (r.w - 4).max(1) as usize);
        let mx = r.x + (r.w - msg.chars().count() as i32) / 2;
        f.text(mx, r.y + r.h / 2, &msg, pal::UI_DIM, body, 0.0);
        return;
    }
    for (k, it) in c.items.iter().enumerate().skip(scroll * cols as usize).take((rows * cols) as usize) {
        let j = k as i32 - (scroll as i32 * cols);
        let (sx, sy) = (x0 + (j % cols) * SLOT_W, y0 + (j / cols) * SLOT_H);
        let hot = v.hover == Some(k);
        let sel = v.selected == Some(k);
        let lifted = v.lifted == Some(k);
        let slot_bg = if hot || sel { Bg::Mix(Rgb::hex(0x3a3060), 0.88) } else { body };
        if hot || sel {
            for yy in sy..sy + SLOT_H - 1 {
                for xx in sx..sx + SLOT_W - 1 {
                    f.put_g(xx, yy, Glyph::new(' ', pal::UI_TEXT).bg(slot_bg).lit(0.0));
                }
            }
        }
        let (ix, iy) = (sx + (SLOT_W - 1 - ICON as i32) / 2, sy);
        let dim = if lifted { 0.35 } else { 1.0 };
        if v.cfg.icons == "real" && it.icon.is_some() && !lifted && crate::frame::pictures_ok() {
            // the icon itself, drawn by the window / terminal over these cells
            f.picture(ix, iy, ICON as i32, (ICON / 2) as i32, it.icon.clone().unwrap());
        } else {
            draw_pixels(f, ix, iy, &it.pixels, slot_bg, dim);
        }
        let label = clip(&it.name, (SLOT_W - 1) as usize);
        let lx = sx + (SLOT_W - 1 - label.chars().count() as i32) / 2;
        let fg = if sel { GOLD } else if hot { Rgb::hex(0xfff1d6) } else { Rgb::hex(0xc8d0e4) * dim };
        f.text(lx, sy + 4, &label, fg, slot_bg, 0.0);
    }
    if let Some(k) = v.insert {
        draw_insert(f, r, n, scroll, k);
    }
}

/// Where a dragged item would land among its own shelf's slots: a gold
/// bar at the left of that slot.
pub fn draw_insert(f: &mut Frame, r: Rect, n: usize, scroll: usize, k: usize) {
    let (x0, y0, cols, rows) = grid(r);
    let j = k as i32 - scroll as i32 * cols;
    if j < 0 || j >= cols * rows || k >= n {
        return;
    }
    let (sx, sy) = (x0 + (j % cols) * SLOT_W, y0 + (j / cols) * SLOT_H);
    for yy in sy..sy + SLOT_H - 1 {
        f.put_g(sx - 1, yy, Glyph::new('▎', GOLD * 1.2).lit(0.0));
    }
}

/// 8×8 pixels as 8 columns × 4 rows of half blocks.
pub fn draw_pixels(f: &mut Frame, x: i32, y: i32, px: &[Option<Rgb>; ICON * ICON], bg: Bg, k: f32) {
    for row in 0..ICON / 2 {
        for col in 0..ICON {
            let (top, bot) = (px[row * 2 * ICON + col], px[(row * 2 + 1) * ICON + col]);
            let g = match (top, bot) {
                (Some(a), Some(b)) => Glyph::new('▀', a * k).bg(Bg::Solid(b * k)),
                (Some(a), None) => Glyph::new('▀', a * k).bg(bg),
                (None, Some(b)) => Glyph::new('▄', b * k).bg(bg),
                (None, None) => Glyph::new(' ', pal::UI_TEXT).bg(bg),
            };
            f.put_g(x + col as i32, y + row as i32, g.lit(0.0));
        }
    }
}

/// Snow piled along a row: a soft drift, different for every shelf.
fn snow_cap(f: &mut Frame, x0: i32, x1: i32, y: i32, seed: u32) {
    const PILE: [char; 4] = ['▁', '▂', '▃', '▄'];
    let ph = (seed % 1000) as f32 * 0.37;
    for x in x0..x1 {
        let u = x as f32;
        let mut n = 0.55 + 0.3 * (u * 0.31 + ph).sin() + 0.15 * (u * 0.97 + ph * 2.1).sin();
        // thinner towards the ends
        let edge = ((x - x0).min(x1 - 1 - x)) as f32;
        n *= (edge / 3.0).min(1.0);
        let k = (n * 4.0).floor().clamp(0.0, 3.0) as usize;
        f.put_g(x, y, Glyph::new(PILE[k], SNOW * (0.9 + 0.05 * (h(x, seed) % 3) as f32)).lit(0.0));
    }
}

fn frame_frosted(f: &mut Frame, r: Rect, v: &View, body: Bg) {
    let b = pal::UI_BORDER;
    let rolled = v.cfg.rolled;
    for xx in r.x..r.x + r.w {
        let top = if xx == r.x { '╭' } else if xx == r.x + r.w - 1 { '╮' } else { '─' };
        f.put_g(xx, r.y, Glyph::new(top, b).bg(body).lit(0.0));
        if !rolled {
            let bot = if xx == r.x { '╰' } else if xx == r.x + r.w - 1 { '╯' } else { '─' };
            f.put_g(xx, r.y + r.h - 1, Glyph::new(bot, b).bg(body).lit(0.0));
        }
    }
    if rolled {
        f.put_g(r.x, r.y, Glyph::new('╶', b).bg(body).lit(0.0));
        f.put_g(r.x + r.w - 1, r.y, Glyph::new('╴', b).bg(body).lit(0.0));
    }
    for yy in r.y + 1..r.y + r.h - 1 {
        f.put_g(r.x, yy, Glyph::new('│', b).bg(body).lit(0.0));
        f.put_g(r.x + r.w - 1, yy, Glyph::new('│', b).bg(body).lit(0.0));
    }
    snow_cap(f, r.x + 1, r.x + r.w - 1, r.y - 1, v.seed);
}

fn frame_wood(f: &mut Frame, r: Rect, v: &View, body: Bg) {
    let grain = |x: i32, y: i32| if h(x * 7 + y * 131, v.seed) % 5 == 0 { WOOD_DARK } else { WOOD };
    // the header board, the sides, and a plank under every row of items
    for xx in r.x..r.x + r.w {
        f.put_g(xx, r.y, Glyph::new('█', grain(xx, r.y)).bg(Bg::Solid(WOOD)).lit(0.0));
    }
    if v.cfg.rolled {
        snow_cap(f, r.x + 1, r.x + r.w - 1, r.y - 1, v.seed);
        return;
    }
    for yy in r.y + 1..r.y + r.h {
        f.put_g(r.x, yy, Glyph::new('█', grain(r.x, yy) * 0.85).bg(body).lit(0.0));
        f.put_g(r.x + r.w - 1, yy, Glyph::new('█', grain(r.x + r.w - 1, yy) * 0.85).bg(body).lit(0.0));
    }
    let (_, y0, _, rows) = grid(r);
    let mut planks: Vec<i32> = (1..=rows).map(|k| y0 + k * SLOT_H - 1).filter(|&y| y < r.y + r.h - 1).collect();
    planks.push(r.y + r.h - 1);
    for py in planks {
        for xx in r.x + 1..r.x + r.w - 1 {
            // a dusting of snow on the plank here and there
            let snowy = h(xx / 2 + py * 17, v.seed) % 4 == 0;
            let top = if snowy { SNOW * 0.9 } else { grain(xx, py) };
            f.put_g(xx, py, Glyph::new('▀', top).bg(Bg::Solid(WOOD_DARK)).lit(0.0));
        }
    }
    snow_cap(f, r.x + 1, r.x + r.w - 1, r.y - 1, v.seed);
}

fn frame_gift(f: &mut Frame, r: Rect, v: &View, body: Bg, _t: f32) {
    // a gold ribbon all round, crossing on the lid, and a bow on top
    let rolled = v.cfg.rolled;
    for xx in r.x..r.x + r.w {
        let top = if xx == r.x { '╔' } else if xx == r.x + r.w - 1 { '╗' } else { '═' };
        f.put_g(xx, r.y, Glyph::new(top, GOLD).bg(Bg::Solid(GIFT)).lit(0.0));
        if !rolled {
            let bot = if xx == r.x { '╚' } else if xx == r.x + r.w - 1 { '╝' } else { '═' };
            f.put_g(xx, r.y + r.h - 1, Glyph::new(bot, GOLD).bg(body).lit(0.0));
        }
    }
    if !rolled {
        for yy in r.y + 1..r.y + r.h - 1 {
            f.put_g(r.x, yy, Glyph::new('║', GOLD).bg(body).lit(0.0));
            f.put_g(r.x + r.w - 1, yy, Glyph::new('║', GOLD).bg(body).lit(0.0));
        }
    }
    // the bow, sitting on the lid in the middle
    let mid = r.x + r.w / 2;
    if r.y > 0 {
        for (i, ch) in ['◖', '✿', '◗'].iter().enumerate() {
            let c = if i == 1 { Rgb::hex(0xfff1a0) } else { GOLD };
            f.put_g(mid - 1 + i as i32, r.y - 1, Glyph::new(*ch, c).lit(0.0));
        }
    }
    f.put_g(mid, r.y, Glyph::new('╦', GOLD).bg(Bg::Solid(GIFT)).lit(0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_land_on_items_title_and_grip() {
        let r = Rect { x: 10, y: 5, w: 40, h: 15 };
        let (x0, y0, cols, rows) = grid(r);
        assert_eq!((cols, rows), (3, 2));
        assert_eq!(hit(r, false, 5, 0, x0 + 1, y0 + 1), Some(Hit::Item(0)));
        assert_eq!(hit(r, false, 5, 0, x0 + SLOT_W + 1, y0 + 1), Some(Hit::Item(1)));
        assert_eq!(hit(r, false, 5, 0, x0 + 1, y0 + SLOT_H + 1), Some(Hit::Item(3)));
        // past the last item, the gap under a name, the corner, the top
        assert_eq!(hit(r, false, 5, 0, x0 + 2 * SLOT_W + 1, y0 + SLOT_H + 1), Some(Hit::Body));
        assert_eq!(hit(r, false, 5, 0, x0 + 1, y0 + SLOT_H - 1), Some(Hit::Body));
        // edges and corners resize; the title row's middle moves it
        assert_eq!(hit(r, false, 5, 0, r.x + r.w - 1, r.y + r.h - 1), Some(Hit::Edge(EDGE_R | EDGE_B)));
        assert_eq!(hit(r, false, 5, 0, r.x, r.y + 3), Some(Hit::Edge(EDGE_L)));
        assert_eq!(hit(r, false, 5, 0, r.x + 9, r.y + r.h - 1), Some(Hit::Edge(EDGE_B)));
        assert_eq!(hit(r, false, 5, 0, r.x, r.y), Some(Hit::Edge(EDGE_L | EDGE_T)));
        assert_eq!(hit(r, true, 5, 0, r.x, r.y), Some(Hit::Title));
        assert_eq!(hit(r, false, 5, 0, r.x + 5, r.y), Some(Hit::Title));
        assert_eq!(hit(r, false, 5, 0, r.x - 1, r.y), None);
        // scrolled down a row: the first slot is item 3
        assert_eq!(hit(r, false, 9, 1, x0 + 1, y0 + 1), Some(Hit::Item(3)));
        assert_eq!(max_scroll(r, 9), 1);
        // the scrollbar, inside the right edge, only when it's needed
        assert!(bar(r, 5).is_none());
        let (bx, len) = bar(r, 9).unwrap();
        assert_eq!(hit(r, false, 9, 0, bx, r.y + 4), Some(Hit::Bar(3)));
        assert_eq!(scroll_at(r, 9, 0), 0);
        assert_eq!(scroll_at(r, 9, len - 1), 1);
        // where a dragged item lands when rearranging
        assert_eq!(slot_at(r, 9, 0, x0 + SLOT_W + 2, y0 + SLOT_H + 2), 4);
        assert_eq!(slot_at(r, 4, 0, x0 + 2 * SLOT_W + 2, y0 + SLOT_H + 2), 3);
    }

    #[test]
    fn pixel_art_keeps_shape_and_see_through() {
        // a 16×16 red square in the top-left of a 32×32 clear picture
        let mut rgba = vec![0u8; 32 * 32 * 4];
        for y in 0..16 {
            for x in 0..16 {
                rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let px = pixel_art(&Picture::new(32, 32, rgba));
        assert!(px[0].is_some_and(|c| c.r > 0.9 && c.g < 0.1));
        assert!(px[3 * ICON + 3].is_some());
        assert!(px[4 * ICON + 4].is_none() && px[7].is_none());
    }

    #[test]
    fn scanning_lists_folders_first_and_skips_hidden_files() {
        let d = std::env::temp_dir().join(format!("xmas-shelf-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Zed folder")).unwrap();
        for f in ["b.txt", "A.png", ".hidden", "desktop.ini"] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        let c = scan(&d, &mut Default::default(), &mut Default::default());
        let names: Vec<&str> = c.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Zed folder", "A.png", "b.txt"]);
        assert_eq!(c.items[0].kind, Kind::Folder);
        assert_eq!(c.items[1].kind, Kind::Image);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(not(windows))]
    #[test]
    fn moving_renames_on_a_clash() {
        let d = std::env::temp_dir().join(format!("xmas-shelf-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (a, b) = (d.join("a"), d.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("n.txt"), "1").unwrap();
        std::fs::write(b.join("n.txt"), "2").unwrap();
        let out = move_files(&[a.join("n.txt")], &b);
        assert_eq!(out, vec![b.join("n (2).txt")]);
        assert_eq!(std::fs::read_to_string(b.join("n (2).txt")).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(b.join("n.txt")).unwrap(), "2");
        let _ = std::fs::remove_dir_all(&d);
    }
}
