//! The interactive app, independent of how it's displayed. The terminal
//! front-end (kitty wallpaper) and the native window front-end (Windows /
//! `--windowed`) both translate their input into these calls and draw the
//! cells `step()` returns.

use crate::audio::{Audio, Sfx, SoundEvent};
use crate::config::{self, Config, SoundCfg};
use crate::frame::{Cell, Frame};
use crate::platform;
use crate::scene::Scene;
use crate::settings::{self, Act, Settings, F};
use crate::widgets::Widgets;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

/// `--fps N` from the command line: used for this run only, never written
/// into config.toml (and not lost when the file is reloaded).
static FPS_OVERRIDE: OnceLock<f32> = OnceLock::new();

pub fn set_fps_override(fps: f32) {
    let _ = FPS_OVERRIDE.set(config::clean_fps(fps));
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Btn {
    Left,
    Right,
}

/// What a folder picker is for: a new shelf (at this cell), or shelf s.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Pick {
    New(i32, i32),
    Change(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ShelfPress {
    Title(usize),
    Item(usize, usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MouseIn {
    Move,
    Down(Btn),
    Drag,
    Up(Btn),
    Scroll(bool),
    /// Ctrl+wheel: look up into the sky (true) or back down.
    ScrollSky(bool),
    /// Scrolled sideways (true: to the right): a trackpad, or a tilting
    /// wheel.
    ScrollSide(bool),
    /// A trackpad's smooth scrolling: cells along (positive: the view moves
    /// left) and rows (positive: up into the sky).
    Pad(f32, f32),
    /// Zoom in (> 1) or out (< 1) by this much (Ctrl+wheel, a pinch).
    Zoom(f32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KeyIn {
    Char(char),
    Backspace,
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
}

/// A layer of the UI drawn at a size of its own over the village.
struct UiPart {
    frame: Frame,
    k: f32,
}

/// The parts of the UI, bottom to top: the widgets, the buttons up top,
/// the windows (menus, panels, dialogs, games).
const WIDGETS: usize = 0;
const BUTTONS: usize = 1;
const WINDOWS: usize = 2;

pub struct App {
    pub cfg: Config,
    /// The config the scene / widgets / overlay were last updated to;
    /// changes are detected against this every frame.
    applied: Config,
    seed: u32,
    /// Live reload + saving of the config file (never over a broken file).
    sync: config::FileSync,
    /// `--fps` for this run (see `FPS_OVERRIDE`); cleared once the frame
    /// rate is changed in the panel.
    fps_override: Option<f32>,
    pub frame: Frame,
    scene: Scene,
    widgets: Widgets,
    settings: Settings,
    drag: Option<(usize, i32, i32)>,
    /// Resizing a widget (the map or a shelf) by its corner grip.
    resizing: Option<(usize, u8)>,
    /// Dragging shelf `s`'s scrollbar.
    bar_drag: Option<usize>,
    /// A folder picker open (on its own thread): what it's for, and its answer.
    picking: Option<std::sync::mpsc::Receiver<(Pick, Option<std::path::PathBuf>)>>,
    /// A press on a shelf's title (a click rolls it up) or on one of its
    /// items (a click selects, a double-click opens, a drag moves it).
    shelf_press: Option<ShelfPress>,
    /// The last click on a shelf item: (shelf, item, when), for double-clicks.
    last_click: Option<(usize, usize, f32)>,
    /// Windows desktop mode: the grid's origin on screen and the cell size
    /// (pixels), to put a file dragged off a shelf where it was dropped.
    pub px_map: Option<(i32, i32, i32, i32)>,
    /// The wallpaper's pixel grid: the screen's size, where the cells start
    /// on it, and a cell's size (px). Open windows (whose tops the villager
    /// can climb onto) are placed on the grid with it.
    pub screen_map: Option<(f32, f32, f32, f32, f32, f32)>,
    /// Each UI layer's cells as the front-end draws them (bottom to top,
    /// as `ui_layers`), in the village's cells: whole pixels, so a little
    /// off the layer's own size, and that adds up down the screen. The
    /// pointer's found on the layers with these.
    pub layer_px: Vec<(f32, f32)>,
    /// Why the desktop wallpaper rests (a game in front, the computer busy,
    /// locked), as the front end sees it: the village stands still, the
    /// widgets and online go on.
    rest_why: Option<String>,
    /// Paused by hand (the ⏸ button, or P).
    paused: bool,
    /// Resting: the village's picture as it stood, put back under the
    /// widgets each time.
    rest_base: Option<Frame>,
    /// The open windows (where they show), in cells: menus keep out from
    /// under them.
    window_rects: Vec<(i32, i32, i32, i32)>,
    /// (XMAS_TEST_DROP_ON_WINDOW: the villager's been dropped on a window)
    test_dropped: u8,
    /// Windows desktop: our menu's "Windows desktop menu…" was picked (the
    /// window sends the right-click on to the desktop).
    pub desktop_menu: bool,
    /// The right-click menu, while it's open.
    menu: Option<crate::ctxmenu::CtxMenu>,
    /// A left press on the scene: where and when (app time). Held still
    /// long enough, it opens the right-click menu (for where right-click
    /// isn't ours — the Windows desktop — or a touchpad).
    press: Option<(i32, i32, f32)>,
    /// Where the current widget press started (a press that doesn't move
    /// on the map is a click: look there).
    press_at: (i32, i32),
    start: Instant,
    last: Instant,
    acts: Vec<Act>,
    /// The UI's grid (menus, panels, widgets, the mouse): the village's,
    /// or (`ui.scale` bigger, a front-end drawing its own letters) fewer,
    /// bigger cells over it.
    pub w: usize,
    pub h: usize,
    /// The village's own grid.
    sw: usize,
    sh: usize,
    /// The UI's own layers over the village, bottom to top, each at its
    /// own size (none: all of it at the village's size, in its frame).
    layers: Vec<UiPart>,
    /// Which layer each part of the UI goes on (None: the village's frame)
    /// and its size: the widgets, the buttons up top, the windows.
    part_layer: [Option<usize>; 3],
    part_k: [f32; 3],
    /// The widgets' grid and the buttons' (each part has its own).
    wid_wh: (usize, usize),
    btn_wh: (usize, usize),
    /// The windows' size (how much bigger than the village's cells), and
    /// whether the front-end can show a UI of its own sizes.
    ui_k: f32,
    ui_ok: bool,
    /// The pointer, in the village's cells (fractions): each part of the UI
    /// works out its own cell from it.
    ptr: (f32, f32),
    /// Running as a wallpaper: keys never quit.
    wallpaper: bool,
    /// Whether typed keys can reach us (false behind the Windows desktop icons).
    keyboard: bool,
    pub quit: bool,
    /// Quit at this app time (after uninstalling: once it's been said).
    quit_at: Option<f32>,
    /// When this side was last offered to the linked devices.
    synced_at: f32,
    /// Direction keys (left, right, up, down): held until this app time.
    held: [f32; 4],
    /// The front-end reports key releases (otherwise a press holds a key
    /// for a moment and auto-repeat keeps it held).
    exact_keys: bool,
    /// The jump key's held (a jump keeps rising): until let go, or (without
    /// releases) until auto-repeat stops refreshing it.
    jump_down: bool,
    jump_until: f32,
    /// Where the last menu opened (a further list opens there too).
    last_menu_at: (i32, i32),
    /// A menu picked up by its title bar: where it was held.
    menu_drag: Option<(i32, i32)>,
    /// Keeping the save file (a real session, not a tool or a test); when
    /// it was last written; a save being imported (picked, then applied).
    saves: bool,
    saved_at: f32,
    import_pick: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// ffmpeg being installed (Settings → Desktop): its result.
    ffmpeg_job: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    importing_save: Option<crate::save::Save>,
    /// The link to the relay (online: friends, shared villages).
    link: Option<crate::net::tcp::Tcp>,
    /// Devices linked to share this villager (see `crate::sync`).
    linked: Option<crate::sync::Linked>,
    /// The village to be made anew (another villager, the village changed
    /// on a linked device): next frame.
    reseed_soon: bool,
    /// A line being typed (a friend's code, a message): what for, the
    /// question, the text.
    typing: Option<(crate::scene::online::TextFor, String, String)>,
    /// A line's to be typed (or a setting edited): the front-end should
    /// bring the keyboard here (Windows, behind the icons: take the focus).
    pub want_keys: bool,
    /// The keyboard can be had by asking (`want_keys`), even while it isn't
    /// here now (Windows desktop mode).
    can_focus: bool,
    /// Rows at the bottom that something else covers (the taskbar, a
    /// panel): menus, tips and boxes keep above them. From the front-end,
    /// and from the window-snow overlay's view of the docks.
    safe_front: usize,
    /// In someone else's village: their settings, over this villager's own
    /// (see `crate::scene::online::Rules`).
    rules_on: Option<crate::scene::online::Rules>,
    /// Whose those are.
    rules_lock: Option<crate::scene::online::Lock>,
    /// Linked devices open at once share one village, unless it's been
    /// closed (or left) by hand; when that's next looked at.
    apart: bool,
    together_at: f32,
    /// Scrolling's come from a trackpad (sideways scrolling seen): up and
    /// down look into the sky, sideways along the village.
    trackpad: bool,
    pad_acc: f32,
    #[cfg(any(target_os = "linux", windows))]
    fx: Option<std::sync::Arc<std::sync::Mutex<crate::winsnow::FxCfg>>>,
    /// The sound thread (None until `start_audio`, e.g. in tools).
    audio: Option<Audio>,
    /// Sharing sound levels with a settings window: when it was last
    /// checked whether one wants them, whether it does, when last written.
    meters_check: f32,
    meters_wanted: bool,
    meters_sent: f32,
    /// This is a popped-out window (it lets the wallpaper know it's open).
    popout: bool,
    popout_touched: f32,
    /// The wallpaper: a popped-out window is open (so it rests, quietly).
    hushed: bool,
    hush_check: Option<Instant>,
    /// Running as a normal window; the window should toggle full screen.
    pub windowed: bool,
    pub fullscreen_toggle: bool,
    /// Quiet because another app is playing media; and since when the
    /// media state last disagreed with that (hysteresis).
    media_quiet: bool,
    media_flip: Option<Instant>,
    /// Sound settings as last handed to the audio thread.
    sound_sent: SoundCfg,
    /// Sounds from the UI itself (volume previews).
    ui_sounds: Vec<SoundEvent>,
    /// A resolution change waiting to settle (scale, app time of the change).
    scale_pending: Option<(f32, f32)>,
    /// The zoomed view: (z, x0, y0) of the last frame (screen → drawing),
    /// where it centres (eased), and a scratch frame for it.
    zoom_xf: (f32, f32, f32),
    zoom_focus: Option<(f32, f32)>,
    /// A pinch's (or the wheel's) zoom so far, before rounding, and the
    /// zoom it last set: a touchpad's many tiny steps add up instead of
    /// each being rounded away.
    zoom_raw: f32,
    zoom_set: f32,
    /// The zoom as shown (easing toward the setting).
    zoom_now: f32,
    spare: Frame,
    /// The wallpaper is showing full screen; and a change the front-end
    /// should make (Some(true): go full screen, Some(false): back).
    pub wall_full: bool,
    pub wall_full_req: Option<bool>,
    /// Shift is held (the villager runs).
    shift: bool,
}

pub fn random_seed() -> u32 {
    let n = SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    crate::rng::hash((n & 0xffff_ffff) as u32 ^ (n >> 32) as u32) | 1
}

/// The kitty wallpaper's control socket. kitty adds "-<its pid>" to a
/// socket path given as `listen_on` (older launchers did that), so look for
/// one of those too.
fn kitty_socket() -> Option<String> {
    let sock = std::env::var("XMAS_KITTY_SOCKET").ok()?;
    let path = sock.strip_prefix("unix:")?;
    if std::path::Path::new(path).exists() {
        return Some(sock);
    }
    let p = std::path::Path::new(path);
    let (dir, name) = (p.parent()?, p.file_name()?.to_str()?.to_string() + "-");
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(&name))).map(|e| format!("unix:{}", e.path().display()))
}

/// Ask the kitty wallpaper to do something through its control socket
/// (in the background). `done` gets whether it worked.
fn kitty_cmd(args: Vec<String>, done: impl FnOnce(bool) + Send + 'static) {
    let Some(sock) = kitty_socket() else {
        done(false);
        return;
    };
    let kitten = std::env::var("XMAS_KITTEN").unwrap_or_else(|_| "kitten".into());
    std::thread::spawn(move || {
        let ok = std::process::Command::new(kitten)
            .args(["@", "--to", &sock])
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        done(ok);
    });
}

/// The resolution changed: in the kitty wallpaper, ask kitty for the
/// matching font size. If that doesn't work, a restart would (the launcher
/// reads the new scale), so the app asks whether to.
fn apply_scale(scale: f32) {
    if std::env::var_os("XMAS_KITTY_SOCKET").is_none() {
        return;
    }
    let base: f32 = std::env::var("XMAS_FONT_BASE").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0);
    let size = format!("{:.2}", base * scale.clamp(0.5, 2.0));
    kitty_cmd(vec!["set-font-size".into(), size], |ok| {
        if !ok && std::env::var_os("INVOCATION_ID").is_some() {
            SCALE_NEEDS_RESTART.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

/// Set (once asked, and the answer was yes) when the wallpaper should exit
/// so its service starts it again with the new resolution.
pub static RESTART_FOR_SCALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The font couldn't be changed live: ask about restarting.
static SCALE_NEEDS_RESTART: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl App {
    pub fn new(cfg: Config, seed: u32, path: &Path, w: usize, h: usize, wallpaper: bool, keyboard: bool, extras: bool) -> Self {
        let mut frame = Frame::new(w, h);
        frame.glow_near = cfg.scene.glow;
        frame.glow_far = cfg.scene.glow;
        let scene = Scene::new(w, h, seed, &cfg.scene);
        let widgets = Widgets::new(&cfg);
        let mut settings = Settings::default();
        settings.extras = extras;
        // run-at-startup lives with the wallpaper (not previews / tests)
        settings.autostart = if extras || wallpaper { platform::autostart_get() } else { None };
        settings.keyboard = keyboard;
        // only the wallpaper itself looks for updates (not previews/exports)
        if wallpaper {
            crate::update::spawn(cfg.updates.check, cfg.updates.auto_install);
        }
        App {
            sync: config::FileSync::new(path, &cfg, Duration::from_secs(1)),
            fps_override: FPS_OVERRIDE.get().copied(),
            applied: cfg.clone(),
            meters_check: -10.0,
            meters_wanted: false,
            meters_sent: -10.0,
            popout: std::env::var_os("XMAS_POPOUT").is_some(),
            popout_touched: -10.0,
            hushed: false,
            hush_check: None,
            windowed: false,
            fullscreen_toggle: false,
            cfg,
            seed,
            frame,
            scene,
            widgets,
            settings,
            drag: None,
            start: Instant::now(),
            last: Instant::now(),
            acts: vec![],
            w,
            h,
            sw: w,
            sh: h,
            layers: vec![],
            part_layer: [None; 3],
            part_k: [1.0; 3],
            wid_wh: (w, h),
            btn_wh: (w, h),
            ui_k: 1.0,
            ui_ok: false,
            ptr: (0.0, 0.0),
            wallpaper,
            keyboard,
            quit: false,
            quit_at: None,
            synced_at: 0.0,
            resizing: None,
            bar_drag: None,
            picking: None,
            shelf_press: None,
            last_click: None,
            px_map: None,
            screen_map: None,
            layer_px: vec![],
            rest_why: None,
            paused: false,
            rest_base: None,
            window_rects: vec![],
            test_dropped: 0,
            desktop_menu: false,
            menu: None,
            press: None,
            press_at: (0, 0),
            held: [f32::NEG_INFINITY; 4],
            exact_keys: false,
            jump_down: false,
            jump_until: 0.0,
            last_menu_at: (0, 0),
            menu_drag: None,
            saves: false,
            saved_at: 0.0,
            import_pick: None,
            ffmpeg_job: None,
            importing_save: None,
            link: None,
            linked: None,
            reseed_soon: false,
            typing: None,
            want_keys: false,
            can_focus: false,
            safe_front: 0,
            rules_on: None,
            rules_lock: None,
            apart: false,
            together_at: 0.0,
            trackpad: false,
            pad_acc: 0.0,
            #[cfg(any(target_os = "linux", windows))]
            fx: None,
            audio: None,
            sound_sent: SoundCfg::default(),
            media_quiet: false,
            media_flip: None,
            ui_sounds: vec![],
            scale_pending: None,
            zoom_xf: (1.0, 0.0, 0.0),
            zoom_focus: None,
            zoom_now: 1.0,
            zoom_raw: 1.0,
            zoom_set: -1.0,
            spare: Frame::new(1, 1),
            wall_full: false,
            wall_full_req: None,
            shift: false,
        }
    }

    /// Screen cell → the cell of the drawing it shows (they differ while
    /// zoomed in, or indoors on a tall screen).
    /// A UI cell to the village's cell under it (the same, without a UI
    /// layer of its own).
    #[cfg(test)]
    fn scene_pt(&self, x: i32, y: i32) -> (i32, i32) {
        if self.ui_k <= 1.0 {
            return (x, y);
        }
        (((x as f32 + 0.5) * self.ui_k) as i32, ((y as f32 + 0.5) * self.ui_k) as i32)
    }

    /// The village's grid (its cells, under the UI's).
    pub fn scene_size(&self) -> (usize, usize) {
        (self.sw, self.sh)
    }

    /// The UI's own layers, bottom to top (and how much bigger each one's
    /// cells): none when it's all the village's size.
    pub fn ui_layers(&self) -> Vec<(&Frame, f32)> {
        self.layers.iter().map(|l| (&l.frame, l.k)).collect()
    }

    /// Where the pointer is in part `p`'s own cells.
    fn part_pt(&self, p: usize) -> (i32, i32) {
        let k = self.part_k[p].max(1.0);
        let (kx, ky) = self.part_layer[p].and_then(|i| self.layer_px.get(i).copied()).filter(|&(x, y)| x > 0.0 && y > 0.0).unwrap_or((k, k));
        ((self.ptr.0 / kx).floor() as i32, (self.ptr.1 / ky).floor() as i32)
    }

    /// The village's cell under the pointer, through the zoom.
    fn world_here(&self) -> (i32, i32) {
        self.world_of(self.ptr.0, self.ptr.1)
    }

    /// The village's cell under (fx, fy) on the screen (its cells), through the zoom.
    fn world_of(&self, x: f32, y: f32) -> (i32, i32) {
        let (z, x0, y0) = self.zoom_xf;
        if z <= 1.001 {
            return (x.floor() as i32, y.floor() as i32);
        }
        ((x0 + x / z).floor() as i32, (y0 + y / z).floor() as i32)
    }

    /// Draw from here on into part `p`'s frame (its layer's, swapped in as
    /// `self.frame`, or the village's own); `part_out` puts it back.
    fn part_in(&mut self, p: usize) -> Option<usize> {
        let i = self.part_layer[p]?;
        std::mem::swap(&mut self.frame, &mut self.layers[i].frame);
        Some(i)
    }

    fn part_out(&mut self, i: Option<usize>) {
        if let Some(i) = i {
            std::mem::swap(&mut self.frame, &mut self.layers[i].frame);
        }
    }

    /// The front-end draws its own letters (a UI layer of bigger ones can
    /// go over the village).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn set_ui_layer_ok(&mut self, on: bool) {
        self.ui_ok = on;
        let (w, h) = (self.sw, self.sh);
        self.regrid(w, h, true);
    }

    #[cfg(test)]
    fn world_pt(&self, x: i32, y: i32) -> (i32, i32) {
        let (x, y) = self.scene_pt(x, y);
        let (z, x0, y0) = self.zoom_xf;
        if z <= 1.001 {
            return (x, y);
        }
        ((x0 + (x as f32 + 0.5) / z).floor() as i32, (y0 + (y as f32 + 0.5) / z).floor() as i32)
    }

    /// Whether typed keys reach us (the Windows wallpaper gets them only
    /// while it's full screen).
    #[cfg_attr(not(windows), allow(dead_code))]
    /// Windows desktop mode: the keyboard comes when asked for.
    pub fn set_can_focus(&mut self, on: bool) {
        self.can_focus = on;
    }

    /// Rows at the bottom the taskbar covers (from the front-end).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn set_safe_bottom(&mut self, rows: usize) {
        self.safe_front = rows;
    }

    /// Rows at the bottom to keep the UI above: what the front-end says,
    /// or a dock the window-snow overlay can see.
    fn safe_bottom(&self) -> usize {
        #[allow(unused_mut)]
        let mut n = self.safe_front;
        #[cfg(any(target_os = "linux", windows))]
        if let (Some(top), Some((_, sh, _, oy, _, ch))) = (crate::winsnow::dock_top(), self.screen_map.filter(|m| m.5 > 0.0)) {
            let row = ((top * sh - oy) / ch).floor();
            if row > 0.0 && (row as usize) < self.sh {
                n = n.max(self.sh - row as usize);
            }
        }
        n.min(self.sh / 4)
    }

    /// The height the UI keeps within (above the taskbar).
    fn ui_h(&self) -> usize {
        self.h - (self.safe_bottom() as f32 / self.ui_k).ceil() as usize
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn set_keyboard(&mut self, on: bool) {
        self.keyboard = on;
        self.settings.keyboard = on;
    }

    /// F1: the controls card.
    pub fn toggle_help(&mut self) {
        self.settings.help = !self.settings.help;
    }

    /// Shift held or not (the front-ends report it with each key).
    pub fn set_shift(&mut self, on: bool) {
        if on != self.shift {
            self.shift = on;
            self.scene.set_sprint(on);
        }
    }

    /// Ctrl + a key: = / + zoom in, - zoom out, 0 back to 1x; with Shift,
    /// the resolution instead (finer / coarser detail).
    pub fn ctrl_key(&mut self, c: char, shift: bool) {
        let t = self.t();
        // Ctrl+V while typing: the clipboard (a friend's code)
        if let (Some((_, ask, text)), 'v' | 'V') = (self.typing.as_mut(), c) {
            text.push_str(crate::net::paste_text().trim());
            if let Some(more) = crate::net::seal::asks_code(ask) {
                *text = crate::net::seal::typing_code(text, more);
            }
            return;
        }
        let (up, down) = (matches!(c, '=' | '+'), matches!(c, '-' | '_'));
        if shift && (up || down) {
            settings::apply(&mut self.cfg, if up { Act::Inc(F::Scale) } else { Act::Dec(F::Scale) });
            self.settings.toast = Some((format!("resolution {:.2}× detail", settings::resolution(self.cfg.scale)), t + 1.4));
        } else if up || down {
            self.zoom_step(if up { 1 } else { -1 });
        } else if c == '0' {
            self.cfg.zoom = 1.0;
            self.settings.toast = Some(("zoom 1.00×".into(), t + 1.4));
        }
    }

    /// The save as things stand.
    fn current_save(&self) -> crate::save::Save {
        crate::save::Save {
            version: 1,
            seed: self.seed,
            name: self.cfg.scene.name.clone(),
            saved: crate::clock::now().format("%Y-%m-%d %H:%M").to_string(),
            player: self.scene.player_at(),
            pockets: self.scene.pockets().clone(),
            you: Some(crate::save::You::of(&self.cfg)),
            online: Some(self.scene.online.book.clone()),
            sync: self.linked.clone(),
        }
    }

    /// Write the save file (a real session only).
    fn write_save(&mut self) {
        self.saved_at = self.t();
        if self.saves {
            self.send_sync();
            let _ = crate::save::write(&crate::save::path(), &self.current_save());
        }
    }

    /// Linked devices: this one's side, for the others.
    fn send_sync(&mut self) {
        let Some(l) = &mut self.linked else { return };
        let now = crate::clock::now().timestamp();
        let you = crate::save::You::of(&self.cfg);
        let st = l.state(&you, self.seed, self.scene.pockets(), &self.scene.online.book, now);
        self.scene.online.sync_send(now, st);
        let devs = l.devices(now);
        self.scene.set_devices(Some((l.label.clone(), devs)));
    }

    /// The village made anew (another seed, or a setting that needs it):
    /// the villager's things, the online side and their card carried over.
    fn new_scene(&mut self) {
        let pockets = self.scene.take_pockets();
        let online = self.scene.take_online();
        let social = self.scene.take_social();
        // a guest stays a guest: the host's village made again
        let guest = online.guest_seed().filter(|_| self.scene.seed() != self.seed);
        // (the same village made again: the villager stays where they were)
        let at = (guest.unwrap_or(self.seed) == self.scene.seed()).then(|| self.scene.player_at()).flatten();
        let cfg = self.scene_cfg();
        self.scene = Scene::new(self.sw, self.sh, guest.unwrap_or(self.seed), &cfg);
        self.scene.set_pockets(pockets);
        self.scene.put_online(online);
        self.scene.put_social(social);
        self.scene.set_own_seed(self.seed);
        if at.is_some() {
            self.scene.place_player(at);
        } else if guest.is_some() {
            self.scene.arrive_by_host();
        }
    }

    /// The scene's settings: this villager's own, but in someone else's
    /// village, theirs (see `Rules`).
    fn scene_cfg(&self) -> crate::config::SceneCfg {
        let mut c = self.cfg.scene.clone();
        if let Some(r) = &self.rules_on {
            r.apply(&mut c);
        }
        c
    }

    /// The whole config as the settings show it (a guest: the village's
    /// settings are the host's).
    fn shown_cfg(&self) -> std::borrow::Cow<'_, Config> {
        match &self.rules_on {
            Some(r) => {
                let mut c = self.cfg.clone();
                r.apply(&mut c.scene);
                std::borrow::Cow::Owned(c)
            }
            None => std::borrow::Cow::Borrowed(&self.cfg),
        }
    }

    /// The villagers kept on this device (for the menu).
    fn list_villagers(&mut self) {
        let names = crate::save::villagers(&crate::save::path()).into_iter().map(|(n, _)| n).collect();
        self.scene.set_villagers(names);
    }

    /// Another villager on this device (`to`, one kept), or a new one
    /// (`name`): this one's put away, theirs is taken out.
    fn switch_villager(&mut self, to: Option<std::path::PathBuf>, name: Option<String>) {
        if !self.saves {
            return;
        }
        let t = self.t();
        self.write_save();
        let p = crate::save::path();
        if let Err(e) = crate::save::switch(&p, to.as_deref()) {
            self.scene.note(&format!("couldn't switch: {e}"));
            return;
        }
        // off the relay as this villager
        self.scene.online.goodbye();
        if let Some(l) = self.link.take() {
            l.close(self.scene.online.take_out());
        }
        let s = crate::save::load();
        let mut pockets = Default::default();
        let mut place = None;
        let mut book = crate::net::Book::default();
        self.linked = None;
        match &s {
            Some(s) => {
                self.seed = s.seed;
                if let Some(you) = &s.you {
                    you.apply(&mut self.cfg);
                }
                pockets = s.pockets.clone();
                place = s.player;
                book = s.online.clone().unwrap_or_default();
                self.linked = s.sync.clone();
            }
            None => {
                self.seed = random_seed();
                let fresh = crate::config::SceneCfg::default();
                self.cfg.scene.name = name.unwrap_or_default();
                self.cfg.scene.pet = fresh.pet;
                self.cfg.scene.friends = vec![];
                self.cfg.scene.wear = fresh.wear;
                self.cfg.scene.motto = String::new();
            }
        }
        if self.cfg.seed != 0 {
            self.cfg.seed = self.seed as u64;
        }
        self.scene = Scene::new(self.sw, self.sh, self.seed, &self.cfg.scene);
        self.scene.set_pockets(pockets);
        self.scene.place_player(place);
        self.scene.set_own_seed(self.seed);
        self.scene.put_online(crate::net::Online::new(book, &self.cfg.scene.name));
        if let Some(l) = &self.linked {
            self.scene.online.sync_on(Some((&l.code, &l.dev)));
        }
        self.scene.set_devices(None);
        self.applied = self.cfg.clone();
        self.write_save();
        self.list_villagers();
        let who = if self.cfg.scene.name.is_empty() { "a new villager".to_string() } else { self.cfg.scene.name.clone() };
        self.scene.note(&format!("☺ {who} now (their own village)"));
        self.settings.toast = Some((format!("☺ {who}"), t + 3.0));
    }

    /// What the village asks of the app (the card's line, linking devices,
    /// switching villagers).
    fn do_front(&mut self) {
        use crate::scene::online::Front;
        for f in self.scene.take_front() {
            match f {
                Front::Motto(m) => self.cfg.scene.motto = m,
                Front::Apart => self.apart = true,
                Front::SetRule(k, v) => {
                    let mut r = crate::scene::online::Rules::of(&self.cfg.scene);
                    if r.set(&k, &v) {
                        r.apply(&mut self.cfg.scene);
                    }
                }
                Front::LinkShow => {
                    let l = self.linked.get_or_insert_with(|| crate::sync::Linked::new(&crate::net::seal::new_code(), false));
                    let code = l.pretty_code();
                    self.scene.online.sync_on(Some((&l.code.clone(), &l.dev.clone())));
                    crate::net::copy_text(&code);
                    self.scene.note(&format!("⇄ link code {code} (copied) · on the other device: ✉ → my devices → link this device, and type it"));
                    self.write_save();
                }
                Front::Link(code) => {
                    let l = crate::sync::Linked::new(&code, true);
                    self.scene.online.sync_on(Some((&l.code, &l.dev)));
                    self.linked = Some(l);
                    self.scene.note("⇄ linking… (the other device needs to be online; it can take a minute)");
                    self.write_save();
                }
                Front::Unlink => {
                    self.linked = None;
                    self.scene.online.sync_on(None);
                    self.scene.set_devices(None);
                    self.scene.note("⇄ this device isn't linked any more");
                    self.write_save();
                }
                Front::Villager(i) => {
                    let to = crate::save::villagers(&crate::save::path()).into_iter().nth(i).map(|(_, p)| p);
                    if to.is_some() {
                        self.switch_villager(to, None);
                    }
                }
                Front::NewVillager(name) => self.switch_villager(None, Some(name)),
                Front::Unfriended(code) => {
                    if let Some(l) = &mut self.linked {
                        l.unfriended(&code);
                    }
                }
            }
        }
    }

    /// Linked devices' sides, taken in.
    fn take_synced(&mut self) {
        let now = crate::clock::now().timestamp();
        for (dev, at, st) in self.scene.take_synced() {
            let Some(l) = &mut self.linked else { continue };
            let you = crate::save::You::of(&self.cfg);
            let a = l.take(&dev, at, &st, now, &you, self.seed, self.scene.pockets(), &self.scene.online.book);
            if let Some(p) = a.pockets {
                self.scene.set_pockets(p);
            }
            if let Some(y) = &a.you {
                y.apply(&mut self.cfg);
                self.scene.note(&format!("⇄ {} from {}", if y.name.is_empty() { "the villager" } else { &y.name }, a.from));
            }
            if let Some(b) = a.book {
                if crate::net::seal::norm(&b.code) != crate::net::seal::norm(&self.scene.online.book.code) {
                    self.scene.online.rekey(b);
                } else {
                    self.scene.online.merge_book(b);
                }
            }
            if let Some(seed) = a.seed {
                self.seed = seed;
                if self.cfg.seed != 0 {
                    self.cfg.seed = seed as u64;
                }
                self.reseed_soon = true;
                self.scene.note(&format!("⇄ the village from {}", a.from));
            }
            self.saved_at = self.saved_at.min(self.t() - 55.0);
        }
    }

    /// Keep the save file from now on, picking up where the last one left
    /// off (its things, if it's this village).
    pub fn keep_saves(&mut self) {
        self.saves = true;
        self.saved_at = self.t();
        if let Some(s) = crate::save::load().filter(|s| s.seed == self.seed) {
            self.scene.set_pockets(s.pockets);
            self.scene.place_player(s.player);
            // the villager's name, the pet's, friends: from the save where
            // the config has none (it was carried here from elsewhere)
            if let Some(you) = &s.you {
                you.fill(&mut self.cfg);
            }
        }
        // online: the friend code and friends (in any save: they're the
        // villager's, whichever village)
        let saved = crate::save::load();
        let book = saved.as_ref().and_then(|s| s.online.clone()).unwrap_or_default();
        self.scene.put_online(crate::net::Online::new(book, &self.cfg.scene.name));
        self.scene.online.dirty = true;
        // linked devices: this one's link
        self.linked = saved.and_then(|s| s.sync);
        if let Some(l) = &self.linked {
            self.scene.online.sync_on(Some((&l.code, &l.dev)));
            let now = crate::clock::now().timestamp();
            self.scene.set_devices(Some((l.label.clone(), l.devices(now))));
        }
        self.list_villagers();
    }

    /// Once a frame: bytes to and from the relay, a line to type, a code to
    /// copy, into a shared village (or home again).
    fn update_online(&mut self) {
        if !self.saves {
            return;
        }
        crate::net::tcp::pump(&mut self.scene.online, &mut self.link);
        if std::mem::take(&mut self.scene.online.dirty) {
            // (the friends list changed: saved soon)
            self.saved_at = self.saved_at.min(self.t() - 55.0);
        }
        if let Some((what, ask)) = self.scene.take_text_ask() {
            self.typing = Some((what, ask, String::new()));
            self.menu = None;
            self.want_keys = true;
        }
        // a chat open: what's come in since, as it comes
        if let Some((crate::scene::online::TextFor::Mail(i), ask, _)) = &mut self.typing {
            *ask = self.scene.chat_prompt(*i as usize);
        }
        if let Some(text) = self.scene.take_copy() {
            crate::net::copy_text(&text);
        }
        if let Some(to) = self.scene.take_village() {
            let seed = to.unwrap_or(self.seed);
            let pockets = self.scene.take_pockets();
            let online = self.scene.take_online();
            let social = self.scene.take_social();
            // a guest: the host's village, as they've set it
            self.rules_on = to.and(social.rules_in.as_ref().map(|r| r.0.clone()));
            let cfg = self.scene_cfg();
            self.scene = Scene::new(self.sw, self.sh, seed, &cfg);
            self.scene.set_pockets(pockets);
            self.scene.put_online(online);
            self.scene.put_social(social);
            self.scene.set_own_seed(self.seed);
            self.scene.arrive_by_host();
        }
        self.do_front();
        self.take_synced();
        self.together();
    }

    /// Two of this villager's devices open at once: one village for both
    /// (one opens theirs, the other comes in), the same folk about in it.
    fn together(&mut self) {
        let t = self.t();
        let Some(l) = &mut self.linked else { return };
        l.live = true;
        l.open = match self.scene.online.village() {
            Some((code, true)) => crate::net::seal::norm(&code),
            _ => String::new(),
        };
        if self.apart || t < self.together_at || self.scene.online.link() != crate::net::Link::On || !self.scene.out_and_about() || self.scene.online.village().is_some() {
            return;
        }
        self.together_at = t + 3.0;
        match l.together(crate::clock::now().timestamp()) {
            crate::sync::Together::Join(code, label) => {
                if self.scene.online.join(&code).is_ok() {
                    self.scene.note(&format!("⇄ into the village on {label}: one villager, one village"));
                }
            }
            crate::sync::Together::Host => {
                self.scene.online.host();
                self.scene.note("⇄ your other device is open too: the village opened for both");
                self.send_sync();
            }
            crate::sync::Together::Alone => {}
        }
    }

    /// The line being typed, over the bottom of the village.
    fn draw_typing(&mut self) {
        let blink = (self.t() * 2.0) as i32 % 2 == 0;
        let Some((what, ask, text)) = &self.typing else { return };
        use crate::color::pal;
        use crate::frame::{Bg, Glyph};
        // a chat: what's been said so far, under its title
        let mut lines = ask.lines();
        let title = lines.next().unwrap_or("").to_string();
        let said: Vec<String> = lines.map(|l| l.to_string()).collect();
        let chat = matches!(what, crate::scene::online::TextFor::Mail(_));
        let f = &mut self.frame;
        let widest = said.iter().map(|l| l.chars().count()).chain([title.chars().count(), text.chars().count() + 2]).max().unwrap_or(0);
        let w = (widest as i32 + 6).clamp(30, f.w as i32 - 4);
        let n = said.len() as i32;
        let h = 4 + n;
        let (x0, y0) = ((f.w as i32 - w) / 2, (f.h as i32 - 8 - n).max(0));
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                    (true, _, true, _) => '╭',
                    (_, true, true, _) => '╮',
                    (true, _, _, true) => '╰',
                    (_, true, _, true) => '╯',
                    (_, _, true, _) | (_, _, _, true) => '─',
                    (true, _, _, _) | (_, true, _, _) => '│',
                    _ => ' ',
                };
                f.put_g(x, y, Glyph::new(ch, crate::color::Rgb::hex(0xffd36b) * 0.8).bg(bg).lit(0.0));
            }
        }
        f.text(x0 + 2, y0, &format!(" {title} "), pal::UI_TITLE, bg, 0.0);
        for (k, l) in said.iter().enumerate() {
            let mine = l.trim_start().starts_with("you:");
            let l: String = l.chars().take((w - 4) as usize).collect();
            f.text(x0 + 2, y0 + 1 + k as i32, &l, if mine { pal::UI_DIM * 1.3 } else { pal::UI_TEXT }, bg, 0.0);
        }
        let shown: String = text.chars().rev().take((w - 6) as usize).collect::<Vec<_>>().into_iter().rev().collect();
        f.text(x0 + 2, y0 + 1 + n, &format!("{shown}{}", if blink { "▏" } else { " " }), pal::UI_TEXT, bg, 0.0);
        let help = if chat { "Enter: send · Esc: close the chat · Ctrl+V: paste" } else { "Enter: send · Esc: never mind · Ctrl+V: paste" };
        f.text(x0 + 2, y0 + 2 + n, help, pal::UI_DIM, bg, 0.0);
        crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w, h }, &self.cfg.ui.look, bg, 3);
    }

    /// A click on the friends widget: a friend's menu, the messages, the
    /// talk line, adding a friend.
    fn friends_click(&mut self, h: crate::widgets::FriendsHit, x: i32, y: i32) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        use crate::scene::online::{NetDo, MORE_FRIEND, MORE_MESSAGES, MORE_PUBLIC};
        use crate::widgets::FriendsHit;
        let more = match h {
            FriendsHit::GoOnline => {
                self.scene.act(crate::scene::Do::Net(NetDo::On));
                return;
            }
            FriendsHit::Public => MORE_PUBLIC,
            FriendsHit::Friend(i) => MORE_FRIEND + i.min(31) as u8,
            FriendsHit::Messages => {
                self.scene.messages_read();
                MORE_MESSAGES
            }
            FriendsHit::Talk => {
                self.scene.open_chat();
                return;
            }
            FriendsHit::Add => {
                self.scene.act(crate::scene::Do::Net(NetDo::Add));
                return;
            }
        };
        let (title, items) = self.scene.more_menu(more);
        let items: Vec<(String, MenuAct)> = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        self.menu = Some(CtxMenu::plain(x + 2, y, title, items, w, h));
        self.ui_sounds.push(SoundEvent { sfx: Sfx::Click, pan: 0.5, gain: 0.6 });
    }

    /// Something online, from the menu (its Online tab, its home page).
    fn net_act(&mut self, n: u8) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        use crate::scene::online::{NetDo, MORE_DEVICES, MORE_PUBLIC};
        use crate::scene::Do;
        let on = self.scene.online.wanted();
        let more = match n {
            settings::NET_ONLINE => return self.scene.act(Do::Net(if on { NetDo::Off } else { NetDo::On })),
            settings::NET_COPY => return self.scene.act(Do::Net(NetDo::Code)),
            settings::NET_LIST => return self.scene.act(Do::Net(NetDo::ListMine(!self.scene.online.book.listed))),
            settings::NET_OPEN => return self.scene.act(Do::Net(NetDo::Host)),
            settings::NET_FRIENDS => {
                self.settings.open = false;
                return self.open_online();
            }
            settings::NET_ADD => {
                self.settings.open = false;
                return self.scene.act(Do::Net(NetDo::Add));
            }
            settings::NET_CARD => {
                self.settings.open = false;
                return self.scene.act(Do::Net(NetDo::MyCard));
            }
            settings::NET_PUBLIC => MORE_PUBLIC,
            settings::NET_DEVICES => MORE_DEVICES,
            _ => return,
        };
        // a menu of its own, in place of the panel
        self.settings.open = false;
        let (title, items) = self.scene.more_menu(more);
        let items: Vec<(String, MenuAct)> = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        let (x, y) = self.scene.zoom_focus();
        let (x, y) = (x / self.ui_k, y / self.ui_k);
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        self.menu = Some(CtxMenu::plain(x as i32 + 4, y as i32 - 10, title, items, w, h));
        self.ui_sounds.push(SoundEvent { sfx: Sfx::Click, pan: 0.5, gain: 0.6 });
    }

    /// What the menu's Online tab shows, as things stand.
    fn fill_net(&mut self) {
        use crate::net::Link;
        let o = &self.scene.online;
        let state = match (o.wanted(), o.link()) {
            (false, _) | (_, Link::Off) => 0,
            (_, Link::Connecting) => 1,
            (_, Link::On) => 2,
        };
        let friends = o.friends();
        self.settings.net = settings::NetInfo {
            state,
            via: self.link.as_ref().map(|l| l.via()).unwrap_or_default(),
            code: crate::net::seal::pretty(&o.code()),
            friends: friends.len(),
            about: friends.iter().filter(|(_, h)| h.is_some()).count(),
            listed: o.book.listed,
            open: o.village().filter(|(_, host)| *host && o.square().is_none()).map(|(c, _)| c),
            devices: self.linked.as_ref().map(|l| l.others.len()).unwrap_or(0),
        };
    }

    /// What a click (or key) in friends & chats asked for: something
    /// online, or a menu (a friend's ⋯ at (x, y); back to friends & online).
    fn chats_do(&mut self, d: crate::scene::chats::ChatsDo, x: i32, y: i32) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        use crate::scene::chats::ChatsDo;
        match d {
            ChatsDo::Net(n) => {
                self.scene.woke();
                self.scene.act(crate::scene::Do::Net(n));
            }
            ChatsDo::Menu(crate::scene::online::MORE_ONLINE) => self.open_online(),
            ChatsDo::Menu(k) => {
                let (title, items) = self.scene.more_menu(k);
                let items = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
                let (w, h) = (self.w as i32, self.ui_h() as i32);
                self.menu = Some(CtxMenu::plain(x, y, title, items, w, h));
            }
            ChatsDo::Done => {}
        }
    }

    /// O: friends & online.
    fn open_online(&mut self) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        let (title, items) = self.scene.more_menu(crate::scene::online::MORE_ONLINE);
        let (x, y) = self.scene.zoom_focus();
        let (x, y) = (x / self.ui_k, y / self.ui_k);
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        let items = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        let m = CtxMenu::plain(x as i32 + 4, y as i32 - 10, title, items, w, h);
        self.last_menu_at = (m.x, m.y);
        self.menu = Some(m);
    }

    /// Zoom in (k > 1) or out, centred on the villager; eased into on screen.
    pub fn zoom_by(&mut self, k: f32) {
        let base = if (self.cfg.zoom - self.zoom_set).abs() < 1e-4 { self.zoom_raw } else { self.cfg.zoom };
        let raw = (base * k).clamp(1.0, 4.0);
        self.zoom_raw = raw;
        self.cfg.zoom = if raw < 1.01 { 1.0 } else { (raw * 100.0).round() / 100.0 };
        self.zoom_set = self.cfg.zoom;
        self.zoom_toast();
    }

    /// The next zoom along (`dir` 1: closer, -1: back out): the buttons,
    /// Ctrl + and Ctrl −.
    pub fn zoom_step(&mut self, dir: i32) {
        self.cfg.zoom = settings::zoom_step(self.cfg.zoom, dir);
        self.zoom_toast();
    }

    fn zoom_toast(&mut self) {
        let t = self.t();
        self.settings.toast = Some((format!("zoom {:.2}×  (Ctrl+wheel or pinch · Ctrl 0: back)", settings::zoom_settled(self.cfg.zoom)), t + 1.4));
    }

    /// The wallpaper itself goes full screen (or back behind the desktop):
    /// the same village, nothing restarts. F11 does it too.
    pub fn toggle_wall_full(&mut self) {
        if self.windowed {
            self.fullscreen_toggle = true;
            return;
        }
        if !self.wallpaper {
            return;
        }
        self.set_wall_full(!self.wall_full);
    }

    fn set_wall_full(&mut self, on: bool) {
        self.commit_city();
        self.settings.open = false;
        self.wall_full = on;
        self.settings.wall_full = on;
        self.wall_full_req = Some(on);
        // the kitty wallpaper: move its panel up front (or back down)
        if std::env::var_os("XMAS_KITTY_SOCKET").is_some() {
            let args = if on {
                "edge=center layer=top focus-policy=exclusive"
            } else {
                "edge=background layer=background focus-policy=on-demand"
            };
            let mut cmd: Vec<String> = ["resize-os-window", "--action=os-panel", "--incremental"].iter().map(|s| s.to_string()).collect();
            cmd.extend(args.split(' ').map(String::from));
            kitty_cmd(cmd, |_| {});
            self.wall_full_req = None;
        }
    }

    /// Start the sound thread (interactive front-ends only).
    pub fn start_audio(&mut self) {
        self.audio = Audio::start(&self.cfg.sound);
        self.sound_sent = self.cfg.sound.clone();
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn fx_cfg(&self) -> crate::winsnow::FxCfg {
        let w = &self.cfg.windows;
        let _ = w;
        crate::winsnow::FxCfg::from_cfg(&self.cfg.windows)
    }

    /// Hook up the window-snow overlay so settings changes reach it.
    #[cfg(any(target_os = "linux", windows))]
    pub fn set_fx(&mut self, fx: std::sync::Arc<std::sync::Mutex<crate::winsnow::FxCfg>>) {
        self.fx = Some(fx);
        self.settings.window_fx = true;
    }

    pub fn t(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    pub fn frame_dt(&self) -> Duration {
        // resting: twice a second is plenty (the clock, online)
        if self.resting() {
            return Duration::from_millis(500);
        }
        // clean_fps: NaN.clamp() is NaN, and from_secs_f32(NaN) panics
        Duration::from_secs_f32(1.0 / config::clean_fps(self.fps_override.unwrap_or(self.cfg.fps)))
    }

    /// Resting (paused by hand, or the wallpaper resting): the village
    /// stands still and goes quiet; the widgets, the menus and online go on.
    pub fn resting(&self) -> bool {
        self.paused || self.rest_why.is_some()
    }

    /// The front end's verdict on resting (`None`: carry on).
    pub fn set_rest(&mut self, why: Option<String>) {
        self.rest_why = why;
    }

    /// The wallpaper may rest while a game runs or the computer's busy
    /// (the menu's setting).
    pub fn rest_for_games(&self) -> bool {
        self.cfg.rest_for_games
    }

    /// The ⏸ button (or P): pause, or carry on.
    pub fn toggle_pause(&mut self) {
        self.paused ^= true;
        self.ui_sounds.push(SoundEvent { sfx: Sfx::Click, pan: 0.5, gain: 0.6 });
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if (w, h) == (self.sw, self.sh) || w == 0 || h == 0 {
            return;
        }
        self.regrid(w, h, true);
    }

    /// The village's grid `w` × `h`, and the UI's over it (bigger cells,
    /// if it's set bigger and the front-end draws its own letters); the
    /// village made again for it (`remake`), where it stands.
    fn regrid(&mut self, w: usize, h: usize, remake: bool) {
        let ok = self.ui_ok;
        let fix = |k: f32| {
            let k = if ok && k.is_finite() { k.clamp(1.0, 2.5) } else { 1.0 };
            if k < 1.05 {
                1.0
            } else {
                (k * 20.0).round() / 20.0
            }
        };
        let ks = [fix(self.cfg.ui.widgets_k()), fix(self.cfg.ui.buttons_k()), fix(self.cfg.ui.scale)];
        let grid = |k: f32| if k > 1.0 { (((w as f32 / k).floor() as usize).max(20), ((h as f32 / k).floor() as usize).max(12)) } else { (w, h) };
        // the layers, bottom to top (the widgets, the buttons, the windows):
        // parts next to each other at one size share one; the village's
        // own frame takes those at its size from the bottom
        let mut layers: Vec<UiPart> = vec![];
        let mut part_layer = [None; 3];
        let (mut cur, mut cur_k) = (None, 1.0f32);
        for (p, &k) in ks.iter().enumerate() {
            if (k - cur_k).abs() > 1e-3 {
                let (gw, gh) = grid(k);
                layers.push(UiPart { frame: Frame::new(gw, gh), k });
                (cur, cur_k) = (Some(layers.len() - 1), k);
            }
            part_layer[p] = cur;
        }
        let changed = (w, h) != (self.sw, self.sh);
        (self.sw, self.sh, self.ui_k) = (w, h, ks[WINDOWS]);
        (self.w, self.h) = grid(ks[WINDOWS]);
        self.wid_wh = grid(ks[WIDGETS]);
        self.btn_wh = grid(ks[BUTTONS]);
        (self.layers, self.part_layer, self.part_k) = (layers, part_layer, ks);
        self.layer_px.clear();
        self.frame.resize(w, h);
        self.menu = None;
        if remake && changed {
            // (still online, still in a friend's village, where the villager was)
            self.new_scene();
        }
    }

    /// Put whatever's being typed in a settings field where it belongs.
    fn commit_city(&mut self) {
        self.settings.commit(&mut self.cfg);
    }

    /// The pointer at a cell of the windows' grid (a terminal, tests: all
    /// one size there).
    pub fn mouse(&mut self, ev: MouseIn, x: i32, y: i32) {
        let k = self.ui_k.max(1.0);
        self.mouse_at(ev, (x as f32 + 0.5) * k, (y as f32 + 0.5) * k);
    }

    /// The pointer at (fx, fy) in the village's cells (fractions): each
    /// part of the UI takes it in its own cells (x, y: the windows'; gx,
    /// gy: the widgets'; bx, by: the buttons').
    pub fn mouse_at(&mut self, ev: MouseIn, fx: f32, fy: f32) {
        self.ptr = (fx, fy);
        let (x, y) = self.part_pt(WINDOWS);
        let (gx, gy) = self.part_pt(WIDGETS);
        let (bx, by) = self.part_pt(BUTTONS);
        let (gw, gh) = (self.wid_wh.0 as i32, self.wid_wh.1 as i32);
        let bw = self.btn_wh.0 as i32;
        let t = self.t();
        self.settings.hover = (x, y);
        self.settings.btn_hover = (bx, by);
        match ev {
            MouseIn::Move => {
                if let Some(m) = &mut self.menu {
                    m.hover = m.hit(x, y);
                }
                let (wx, wy) = self.world_here();
                self.scene.set_cursor(Some((wx, wy)));
                self.scene.convo_hover(x, y);
                self.widgets.hover = if self.settings.open { None } else { self.widgets.hit(gx, gy) };
                self.widgets.friends_hover = if self.settings.open { None } else { self.widgets.friends_hit(gx, gy) };
                if !self.settings.open {
                    self.widgets.shelf_hover(gx, gy);
                }
            }
            // picked up by its title bar: moved, not closed
            MouseIn::Down(Btn::Left) if self.menu.as_ref().is_some_and(|m| m.on_title(x, y)) => {
                let m = self.menu.as_ref().unwrap();
                self.menu_drag = Some((x - m.x, y - m.y));
            }
            MouseIn::Down(Btn::Left) if self.menu.is_some() => {
                // a click picks from the menu (or, outside it, just closes it)
                let m = self.menu.take().unwrap();
                if let Some(a) = m.hit(x, y).map(|i| m.items[i].1) {
                    self.menu_do(a);
                }
            }
            MouseIn::Down(Btn::Left) => {
                let in_panel = self.settings.rect.map(|r| r.contains(x, y)).unwrap_or(false);
                if let Some(a) = self.settings.banner_click(x, y).filter(|_| !self.settings.open) {
                    self.acts.push(a);
                } else if self.settings.open && in_panel && self.settings.on_title(x, y) {
                    // pick the panel up by its title bar
                    self.settings.start_drag(x, y);
                } else if self.settings.open && in_panel {
                    match self.settings.click(x, y) {
                        // clicking into a text field (or back into the one
                        // being typed in) doesn't put away what's typed
                        Some(Act::Edit(w)) => self.acts.push(Act::Edit(w)),
                        // along a slider: set it, and follow a drag
                        Some(a @ Act::Set(f, _)) => {
                            self.commit_city();
                            self.settings.slider_drag = Some(f);
                            self.acts.push(a);
                        }
                        Some(a) => {
                            self.commit_city();
                            self.acts.push(a)
                        }
                        None => self.commit_city(),
                    }
                } else if self.settings.open {
                    self.commit_city();
                    self.settings.open = false;
                } else if self.settings.help {
                    // a click on one of the villager's [keys] changes it;
                    // anywhere else closes the controls card
                    let (w, h) = (self.w as i32, self.ui_h() as i32);
                    if !self.settings.help_click(x, y, w, h, &self.cfg.keys) {
                        self.settings.help = false;
                    }
                } else if settings::pause_rect(bw).contains(bx, by) {
                    self.toggle_pause();
                } else if settings::zoom_rects(bw).0.contains(bx, by) {
                    self.zoom_step(-1);
                } else if settings::zoom_rects(bw).1.contains(bx, by) {
                    self.zoom_step(1);
                } else if settings::centre_rect(bw).contains(bx, by) {
                    self.scene.recenter();
                } else if settings::help_rect(bw).contains(bx, by) {
                    self.settings.help = true;
                } else if settings::online_rect(bw).contains(bx, by) {
                    self.menu = None;
                    self.scene.open_chats(None);
                } else if settings::gear_rect(bw).contains(bx, by) {
                    // (the buttons up there always answer: a game, a
                    // conversation or a card open doesn't take them)
                    self.settings.open = true;
                } else if self.scene.close_click(x, y) {
                    // (a ✕ or ⌂ button: as Esc)
                } else if self.scene.on_chats(x, y) {
                    // friends & chats: a friend picked, their ⋯, a line to write…
                    let d = self.scene.chats_click(x, y);
                    self.chats_do(d, x, y);
                } else if self.scene.prompt_click(x, y) || self.scene.close_card() {
                    // (a question answered, a profile card put away)
                } else if self.scene.post_click(x, y) {
                    self.open_online();
                } else if self.scene.on_table(x, y) {
                    // (its title bar picks it up to move it)
                    if !self.scene.table_grab(x, y) {
                        self.scene.table_click(x, y);
                    }
                } else if self.scene.convo_click(x, y) {
                    // answered in the dialogue box (by hand: autopilot stands down)
                    self.scene.woke();
                } else if let Some(i) = self.widgets.on_grip(gx, gy).filter(|_| !self.cfg.layout.locked) {
                    self.resizing = Some((i, crate::widgets::shelf::EDGE_R | crate::widgets::shelf::EDGE_B));
                    self.widgets.dragging = Some(i);
                } else if let Some((s, hit)) = self.widgets.shelf_hit(gx, gy) {
                    self.shelf_down(s, hit, gx, gy);
                } else if let Some(h) = self.widgets.friends_hit(gx, gy) {
                    self.friends_click(h, x, y);
                } else if let (true, Some(wx)) = (self.cfg.layout.locked, self.widgets.map_world_x(gx, gy)) {
                    self.scene.look_at(wx);
                } else if let (false, Some(i)) = (self.cfg.layout.locked, self.widgets.hit(gx, gy)) {
                    let r = self.widgets.rects[i].unwrap();
                    self.drag = Some((i, gx - r.x, gy - r.y));
                    self.press_at = (gx, gy);
                    self.widgets.dragging = Some(i);
                } else if self.scene.at_table() {
                    // (sat at a game, a click off the table leaves the
                    // village alone)
                } else {
                    self.press = Some((x, y, self.t()));
                    let (wx, wy) = self.world_here();
                    if self.scene.grab(wx, wy) {
                        self.scene.woke();
                    } else {
                        // nobody to pick up here: an ordinary click on the scene
                        self.scene.click(wx, wy, t);
                    }
                }
            }
            MouseIn::Drag if self.settings.slider_drag.is_some() => {
                if let Some(a) = self.settings.drag_slider(x, y) {
                    self.acts.push(a);
                }
            }
            MouseIn::Drag => {
                // moved off: not a long press after all
                if self.press.is_some_and(|(px, py, _)| (px - x).abs() > 1 || (py - y).abs() > 1) {
                    self.press = None;
                }
                if self.settings.drag.is_some() {
                    self.settings.drag_to(x, y);
                    return;
                }
                if let (Some((dx, dy)), Some(m)) = (self.menu_drag, self.menu.as_mut()) {
                    m.move_to(x - dx, y - dy, self.w as i32, self.h as i32);
                    self.last_menu_at = (m.x, m.y);
                    return;
                }
                if self.scene.table_dragging() {
                    self.scene.table_drag_to(x, y);
                    return;
                }
                if self.scene.convo_dragging() {
                    self.scene.convo_drag_to(x, y);
                    return;
                }
                if self.scene.holding() {
                    let (wx, wy) = self.world_here();
                    self.scene.drag(wx, wy);
                    return;
                }
                if let Some(ShelfPress::Item(s, k)) = self.shelf_press {
                    // an item pulled off its slot: carried to wherever it's let go
                    if self.widgets.lift.is_some() || (gx - self.press_at.0).abs() > 1 || (gy - self.press_at.1).abs() > 1 {
                        self.widgets.lift = Some((s, k));
                        self.widgets.lift_at = (gx, gy);
                    }
                    return;
                }
                let (wx, wy) = self.world_here();
                self.scene.set_cursor(Some((wx, wy)));
                if let Some(s) = self.bar_drag {
                    self.shelf_bar(s, gy);
                    return;
                }
                if let Some((i, e)) = self.resizing {
                    self.widgets.resize(i, e, gx, gy, gw, gh);
                    return;
                }
                if let Some((i, ox, oy)) = self.drag {
                    self.widgets.move_to(i, gx - ox, gy - oy, gw, gh);
                }
            }
            MouseIn::Up(Btn::Left) if self.press.take().is_some() && self.menu.is_some() => {
                // the end of a long press: the menu it opened stays up
                if self.scene.holding() {
                    self.scene.let_go();
                }
                self.shelf_press = None;
                self.widgets.lift = None;
                if self.drag.take().is_some() {
                    self.widgets.dragging = None;
                }
            }
            MouseIn::Up(Btn::Left) if self.shelf_press.is_some() => {
                let p = self.shelf_press.take().unwrap();
                match p {
                    ShelfPress::Item(s, k) => {
                        if let Some(lift) = self.widgets.lift.take() {
                            self.shelf_drop(lift, gx, gy);
                        } else {
                            // a click selects; a second one soon after opens
                            let now = self.t();
                            if self.last_click.is_some_and(|(ls, lk, lt)| ls == s && lk == k && now - lt < 0.5) {
                                self.last_click = None;
                                if let Some(path) = self.widgets.item_path(s, k) {
                                    open_detached(path);
                                }
                            } else {
                                self.last_click = Some((s, k, now));
                            }
                        }
                    }
                    ShelfPress::Title(s) => {
                        if (gx, gy) == self.press_at {
                            // a click on the title rolls it up (or down again)
                            if let Some(c) = self.cfg.shelves.get_mut(s) {
                                c.rolled = !c.rolled;
                            }
                            if self.drag.take().is_some() {
                                self.widgets.dragging = None;
                            }
                        } else if let Some((i, ox, oy)) = self.drag.take() {
                            self.widgets.move_to(i, gx - ox, gy - oy, gw, gh);
                            self.widgets.dragging = None;
                            crate::widgets::set_pos(&mut self.cfg, i, crate::widgets::pos_of(&self.widgets.cfg, i));
                        }
                    }
                }
            }
            MouseIn::Up(Btn::Left) if self.settings.drag.is_some() => {
                self.settings.drag_to(x, y);
                self.settings.drag = None;
            }
            MouseIn::Up(Btn::Left) if self.menu_drag.is_some() => self.menu_drag = None,
            MouseIn::Up(Btn::Left) if self.scene.table_dragging() => self.scene.table_drop(),
            MouseIn::Up(Btn::Left) if self.scene.convo_dragging() => {
                self.scene.convo_drag_to(x, y);
                self.scene.convo_drop();
            }
            MouseIn::Up(Btn::Left) if self.scene.holding() => {
                let (wx, wy) = self.world_here();
                self.scene.drag(wx, wy);
                self.scene.release();
            }
            MouseIn::Up(Btn::Left) if self.bar_drag.is_some() => {
                self.bar_drag = None;
            }
            MouseIn::Up(Btn::Left) if self.resizing.is_some() => {
                let (i, e) = self.resizing.take().unwrap();
                self.widgets.resize(i, e, gx, gy, gw, gh);
                self.widgets.dragging = None;
                if i == 4 {
                    self.cfg.layout.map_size = self.widgets.cfg.layout.map_size;
                    self.cfg.layout.map = self.widgets.cfg.layout.map;
                } else if i < crate::widgets::BUILTIN {
                    // system, music, weather, the visualizer
                    self.cfg.layout.sizes = self.widgets.cfg.layout.sizes.clone();
                    self.cfg.layout.set(i, self.widgets.cfg.layout.get(i));
                } else if let (Some(c), Some(w)) = (self.cfg.shelves.get_mut(i - crate::widgets::BUILTIN), self.widgets.cfg.shelves.get(i - crate::widgets::BUILTIN)) {
                    (c.size, c.pos) = (w.size, w.pos);
                }
            }
            MouseIn::Up(Btn::Left) if self.settings.slider_drag.take().is_some() => {}
            MouseIn::Up(Btn::Left) => {
                if let Some((4, ..)) = self.drag.filter(|_| (gx, gy) == self.press_at) {
                    // a click on the map (no drag): look over there
                    self.drag = None;
                    self.widgets.dragging = None;
                    if let Some(wx) = self.widgets.map_world_x(gx, gy) {
                        self.scene.look_at(wx);
                    }
                    return;
                }
                if let Some((i, ox, oy)) = self.drag.take() {
                    // drop where the button came up (there may have been no
                    // Drag event at this spot), and commit only the dragged
                    // widget's slot: the others may have changed in the file
                    // meanwhile and must not be reverted
                    self.widgets.move_to(i, gx - ox, gy - oy, gw, gh);
                    self.widgets.dragging = None;
                    crate::widgets::set_pos(&mut self.cfg, i, crate::widgets::pos_of(&self.widgets.cfg, i));
                }
            }
            MouseIn::Down(Btn::Right) => {
                // right-click: options for whatever's under the pointer (the
                // settings are always at the bottom); again to close
                self.commit_city();
                if self.settings.open {
                    self.settings.open = false;
                } else if self.menu.take().is_none() {
                    self.open_menu(x, y);
                }
            }
            MouseIn::Scroll(up) => {
                if self.settings.open {
                    if let Some(a) = self.settings.wheel(x, y, up) {
                        self.acts.push(a);
                    }
                } else if self.scene.on_chats(x, y) {
                    self.scene.chats_scroll(!up);
                } else if let Some((s, _)) = self.widgets.shelf_hit(gx, gy) {
                    self.widgets.shelf_scroll(s, if up { -1 } else { 1 });
                } else if self.trackpad {
                    // a trackpad: up and down look up into the sky and back
                    self.scene.pan_up(up);
                } else {
                    // the wheel looks around the village
                    let step = (self.sw as f32 * 0.06).max(4.0);
                    self.scene.pan(if up { -step } else { step });
                }
            }
            MouseIn::ScrollSide(right) => {
                // sideways scrolling: a trackpad (from now on, up and down
                // look into the sky)
                self.trackpad = true;
                if !self.settings.open {
                    let step = (self.sw as f32 * 0.03).max(2.0);
                    self.scene.pan(if right { step } else { -step });
                }
            }
            MouseIn::Pad(dx, dy) => {
                self.trackpad = true;
                if self.settings.open {
                    self.pad_acc += dy;
                    if self.pad_acc.abs() >= 1.0 {
                        if let Some(a) = self.settings.wheel(x, y, self.pad_acc > 0.0) {
                            self.acts.push(a);
                        }
                        self.pad_acc = 0.0;
                    }
                } else if let Some((s, _)) = self.widgets.shelf_hit(gx, gy) {
                    self.pad_acc += dy;
                    if self.pad_acc.abs() >= 1.0 {
                        self.widgets.shelf_scroll(s, if self.pad_acc > 0.0 { -1 } else { 1 });
                        self.pad_acc = 0.0;
                    }
                } else {
                    self.scene.pan(-dx);
                    self.scene.lift_by(dy);
                }
            }
            MouseIn::Zoom(k) => {
                if !self.settings.open {
                    self.zoom_by(k);
                }
            }
            MouseIn::ScrollSky(up) => {
                if !self.settings.open {
                    self.scene.pan_up(up);
                }
            }
            MouseIn::Up(Btn::Right) => {}
        }
    }

    /// Which direction slot a key drives (WASD or the arrows).
    fn dir_slot(k: KeyIn) -> Option<usize> {
        match k {
            KeyIn::Left | KeyIn::Char('a' | 'A') => Some(0),
            KeyIn::Right | KeyIn::Char('d' | 'D') => Some(1),
            KeyIn::Up | KeyIn::Char('w' | 'W') => Some(2),
            KeyIn::Down | KeyIn::Char('s' | 'S') => Some(3),
            _ => None,
        }
    }

    fn hold(&mut self, k: KeyIn) {
        if self.settings.editing.is_some() || !self.scene.has_player() {
            return;
        }
        if let Some(i) = Self::dir_slot(k) {
            // with releases reported a key stays held until it's let go (no
            // timeout: only the last key pressed auto-repeats, so a key held
            // under another one — W then D, walking diagonally — never gets
            // refreshed). Without them a press holds it for a moment and
            // auto-repeat keeps it going.
            self.held[i] = if self.exact_keys { f32::INFINITY } else { self.t() + 0.6 };
        }
    }

    /// The front-end reports releases (winit, the desktop key poll, kitty's
    /// keyboard protocol): keys are held exactly until let go.
    pub fn set_exact_keys(&mut self) {
        if !self.exact_keys {
            self.exact_keys = true;
            let t = self.t();
            for h in &mut self.held {
                if *h > t {
                    *h = f32::INFINITY;
                }
            }
        }
    }

    /// Let go of everything (focus lost: releases may never arrive).
    pub fn release_all(&mut self) {
        self.held = [f32::NEG_INFINITY; 4];
    }

    /// A key's auto-repeat.
    pub fn key_repeat(&mut self, k: KeyIn) {
        if self.menu.is_some() || self.scene.convo_choosing() {
            return self.key(k);
        }
        self.hold(k);
        if self.key_action(k) == Some(1) && !self.exact_keys {
            self.jump_until = self.t() + 0.25;
        }
    }

    /// A key was let go (front-ends that can tell).
    pub fn key_up(&mut self, k: KeyIn) {
        self.set_exact_keys();
        if let Some(i) = Self::dir_slot(k) {
            self.held[i] = f32::NEG_INFINITY;
        }
        if self.key_action(k) == Some(1) && self.jump_down {
            self.jump_down = false;
            self.scene.release_jump();
        }
    }

    /// Held direction keys → (x, y) in -1..=1.
    fn move_dir(&self) -> (f32, f32) {
        let t = self.t();
        let on = |i: usize| (self.held[i] > t) as i32 as f32;
        (on(1) - on(0), on(3) - on(2))
    }

    pub fn key(&mut self, k: KeyIn) {
        // a line being typed (a friend's code, a message)
        if let Some((what, ask, text)) = self.typing.as_mut() {
            let code = crate::net::seal::asks_code(ask);
            match k {
                KeyIn::Char(c) if !c.is_control() && text.chars().count() < 200 => {
                    text.push(c);
                    // a code: capitals and dashes, as it's typed
                    if let Some(more) = code {
                        *text = crate::net::seal::typing_code(text, more);
                    }
                }
                KeyIn::Backspace => {
                    text.pop();
                    if let Some(more) = code {
                        *text = crate::net::seal::typing_code(text, more);
                    }
                }
                KeyIn::Enter => {
                    let (what, text) = (*what, std::mem::take(text));
                    self.typing = None;
                    self.scene.give_text(what, &text);
                }
                KeyIn::Esc => self.typing = None,
                _ => {}
            }
            return;
        }
        // friends & chats open: up and down the friends, Enter to write,
        // Esc to put it away
        if self.settings.editing.is_none() && !self.settings.open && self.menu.is_none() && self.scene.chats_open() {
            let d = match k {
                KeyIn::Up => self.scene.chats_key(Some(true), false),
                KeyIn::Down => self.scene.chats_key(Some(false), false),
                KeyIn::Enter => self.scene.chats_key(None, true),
                KeyIn::Esc => {
                    self.scene.close_chats();
                    Some(crate::scene::chats::ChatsDo::Done)
                }
                _ => None,
            };
            if let Some(d) = d {
                self.chats_do(d, self.w as i32 / 2, self.ui_h() as i32 / 3);
                return;
            }
        }
        // a question asked by someone online: Y yes, N no thanks
        if self.settings.editing.is_none() && !self.settings.open && self.scene.asked_now() {
            if let KeyIn::Char(c @ ('y' | 'Y' | 'n' | 'N')) = k {
                self.scene.prompt_key(c.eq_ignore_ascii_case(&'y'));
                return;
            }
        }
        // a key being set for something (Settings → You → Keys)
        if let Some(i) = self.settings.binding {
            match k {
                KeyIn::Esc => self.settings.binding = None,
                KeyIn::Char(c) if !c.is_control() && !"wasdWASD?".contains(c) => {
                    let spec = if c == ' ' { "space".to_string() } else { c.to_ascii_lowercase().to_string() };
                    self.cfg.keys.set(i, spec);
                    self.settings.binding = None;
                }
                KeyIn::Enter => {
                    self.cfg.keys.set(i, "enter".into());
                    self.settings.binding = None;
                }
                _ => {}
            }
            return;
        }
        // a menu open: the arrows (or W/S) go up and down it, Enter (or
        // Space, or E) picks, 1-9 picks straight off
        if self.menu.is_some() && self.settings.editing.is_none() {
            let act = self.key_action(k);
            let m = self.menu.as_mut().unwrap();
            let pick = match k {
                KeyIn::Up | KeyIn::Char('w' | 'W') => {
                    m.step(-1);
                    return;
                }
                KeyIn::Down | KeyIn::Char('s' | 'S') => {
                    m.step(1);
                    return;
                }
                KeyIn::Enter | KeyIn::Char(' ') => m.hover,
                KeyIn::Char(c @ '1'..='9') => Some(c as usize - '1' as usize).filter(|&i| i < m.items.len()),
                _ if act == Some(0) => m.hover,
                _ if act == Some(8) => {
                    self.menu = None;
                    return;
                }
                _ => None,
            };
            if let Some(i) = pick {
                let a = m.items[i].1;
                self.menu = None;
                self.menu_do(a);
                return;
            }
        }
        // at the table for a game: the arrows, Enter, D/S, Esc to get up
        if self.settings.editing.is_none() && !self.settings.open && self.scene.at_table() {
            use crate::scene::games::Key as G;
            // on a board WASD moves about too; at cards D draws, S stands
            let board = self.scene.table_board();
            let g = match k {
                KeyIn::Up => Some(Some(G::Up)),
                KeyIn::Down => Some(Some(G::Down)),
                KeyIn::Left => Some(Some(G::Left)),
                KeyIn::Right => Some(Some(G::Right)),
                KeyIn::Char('w' | 'W') if board => Some(Some(G::Up)),
                KeyIn::Char('s' | 'S') if board => Some(Some(G::Down)),
                KeyIn::Char('a' | 'A') if board => Some(Some(G::Left)),
                KeyIn::Char('d' | 'D') if board => Some(Some(G::Right)),
                KeyIn::Char('p' | 'P') => Some(Some(G::Dealer)),
                KeyIn::Char('d' | 'D' | 's' | 'S') => Some(Some(G::Other)),
                KeyIn::Enter | KeyIn::Char(' ' | 'e' | 'E') => Some(Some(G::Pick)),
                KeyIn::Esc => Some(None),
                _ => None,
            };
            if let Some(g) = g {
                self.scene.table_key(g);
            }
            return;
        }
        // a conversation: the arrows go through the answers, Enter picks
        if self.settings.editing.is_none() && !self.settings.open && self.scene.convo_choosing() {
            match k {
                KeyIn::Up => return self.scene.convo_step(-1),
                KeyIn::Down => return self.scene.convo_step(1),
                KeyIn::Enter if self.scene.convo_pick() => {
                    self.scene.woke();
                    return;
                }
                _ => {}
            }
        }
        self.hold(k);
        if self.settings.editing.is_none() && !self.settings.open && self.scene.has_player() {
            // the villager's keys (as set): steered by hand again (autopilot
            // stands down)
            let act = self.key_action(k).or((k == KeyIn::Enter).then_some(0));
            if act.is_some_and(|a| a != 6) || matches!(k, KeyIn::Char('1'..='5')) {
                self.scene.woke();
            }
            match act {
                Some(0) => self.scene.interact(),
                Some(1) => {
                    self.scene.jump();
                    self.jump_down = true;
                    self.jump_until = if self.exact_keys { f32::INFINITY } else { self.t() + 0.25 };
                }
                Some(2) => self.scene.snowball(),
                Some(3) => self.scene.invite(),
                Some(4) => self.scene.toggle_pockets(),
                Some(5) => self.scene.cycle_held(),
                Some(6) => self.scene.recenter(),
                Some(7) => self.scene.rest(),
                Some(8) => self.open_todo(),
                Some(9) => self.open_online(),
                Some(10) => self.scene.open_chat(),
                Some(11) => self.toggle_pause(),
                _ => {
                    if let KeyIn::Char(c @ '1'..='5') = k {
                        self.scene.number(c as usize - '0' as usize);
                    }
                }
            }
        }
        if let Some(buf) = self.settings.editing.as_mut() {
            match k {
                KeyIn::Char(c) if buf.chars().count() < if self.settings.edit == Some(settings::Field::Motto) { 60 } else { 28 } => buf.push(c),
                KeyIn::Backspace => {
                    buf.pop();
                }
                KeyIn::Enter => self.commit_city(),
                KeyIn::Esc => self.settings.cancel_edit(),
                _ => {}
            }
        } else if self.settings.help && k == KeyIn::Esc {
            self.settings.help = false;
        } else if k == KeyIn::Char('?') {
            self.settings.help = !self.settings.help;
        } else if self.settings.open && k == KeyIn::Esc {
            self.settings.open = false;
        } else if k == KeyIn::Esc && self.menu.take().is_some() {
        } else if k == KeyIn::Esc && self.scene.close_panels() {
        } else if k == KeyIn::Esc {
            // nothing else to close: the settings
            self.settings.open = true;
        } else {
            let c = match k {
                KeyIn::Char(c) => Some(c),
                KeyIn::Up => Some('u'),
                KeyIn::Down => Some('d'),
                KeyIn::Left => Some('l'),
                KeyIn::Right => Some('r'),
                _ => None,
            };
            if let Some(c) = c {
                let t = self.t();
                self.scene.typed(c, t);
            }
        }
    }

    /// What the mouse pointer should look like right now.
    pub fn pointer(&self) -> &'static str {
        let (hx, hy) = self.settings.hover;
        if let Some(m) = self.menu.as_ref().filter(|m| m.contains(hx, hy)) {
            return if m.hit(hx, hy).is_some() { "pointer" } else { "default" };
        }
        if self.drag.is_some() || self.scene.holding() {
            "grabbing"
        } else if self.settings.drag.is_some() {
            "grabbing"
        } else if self.settings.open && self.settings.on_title(hx, hy) {
            "grab"
        } else if self.settings.open && self.settings.rect.map(|r| r.contains(hx, hy)).unwrap_or(false) {
            match self.settings.click(hx, hy) {
                Some(Act::Edit(_)) if self.keyboard => "text",
                Some(_) => "pointer",
                None => "default",
            }
        } else if {
            let ((bx, by), bw) = (self.settings.btn_hover, self.btn_wh.0 as i32);
            settings::online_rect(bw).contains(bx, by) || settings::pause_rect(bw).contains(bx, by) || settings::gear_rect(bw).contains(bx, by) || settings::help_rect(bw).contains(bx, by) || settings::centre_rect(bw).contains(bx, by) || settings::zoom_rects(bw).0.contains(bx, by) || settings::zoom_rects(bw).1.contains(bx, by)
        } || self.scene.post_click(hx, hy)
        {
            "pointer"
        } else if !self.settings.open && !self.cfg.layout.locked && self.widgets.hover.is_some() {
            "grab"
        } else if self.scene.convo_dragging() {
            "grabbing"
        } else if !self.settings.open && self.scene.convo_grip(hx, hy) {
            "grab"
        } else if !self.settings.open && self.scene.grabbable(self.world_here().0, self.world_here().1) {
            "grab"
        } else if !self.settings.open && self.scene.clickable(self.world_here().0, self.world_here().1) {
            "pointer"
        } else {
            "default"
        }
    }

    /// Advance one frame and return the composed cells.
    /// Open the village in a window of its own (or full screen), like a
    /// game. The wallpaper rests (and goes quiet) while it's open.
    fn pop_out(&mut self, fullscreen: bool) {
        self.commit_city();
        self.settings.open = false;
        let Ok(exe) = std::env::current_exe() else { return };
        let mut cmd = std::process::Command::new(exe);
        cmd.arg(if fullscreen { "--fullscreen" } else { "--windowed" }).arg("--config").arg(self.sync.path()).env("XMAS_POPOUT", "1");
        // not tied to the wallpaper's terminal
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        if cmd.spawn().is_err() {
            self.ui_sounds.push(crate::audio::SoundEvent { sfx: Sfx::Click, pan: 0.5, gain: 0.3 });
        }
    }

    /// Running as a normal window (not the wallpaper).
    pub fn set_windowed(&mut self, on: bool) {
        self.windowed = on;
        self.settings.windowed = on;
        // Linux, in a window (not the wallpaper's own pop-out): offer to make
        // it the desktop wallpaper, if it isn't already
        #[cfg(target_os = "linux")]
        if on && !self.popout && !crate::linwall::wallpaper_running() {
            self.settings.wall_offer = true;
        }
    }

    /// The wallpaper side: is a popped-out window open? (Checked about once
    /// a second; while it is, the wallpaper rests and its sound is off.)
    pub fn popped_out(&mut self) -> bool {
        if self.popout {
            return false;
        }
        let now = Instant::now();
        if self.hush_check.is_none_or(|c| now.duration_since(c).as_secs_f32() > 1.0) {
            self.hush_check = Some(now);
            let open = crate::platform::fresh(&crate::platform::popout_path(), 3.0);
            if open != self.hushed {
                self.hushed = open;
                // sound off (or back) right away: a resting wallpaper doesn't step
                if let Some(a) = &self.audio {
                    let mut want = self.cfg.sound.clone();
                    if open || self.media_quiet {
                        want.enabled = false;
                    }
                    if a.config(&want) {
                        self.sound_sent = want;
                    }
                }
            }
        }
        self.hushed
    }

    /// Something picked from a menu.
    fn menu_do(&mut self, a: crate::ctxmenu::MenuAct) {
        use crate::ctxmenu::MenuAct;
        match a {
            MenuAct::Settings => self.settings.open = true,
            MenuAct::Look(i) => self.acts.push(Act::Look(i, 1)),
            MenuAct::Hide(i) => {
                let f = [settings::F::Clock, settings::F::System, settings::F::Music, settings::F::Weather, settings::F::Map, settings::F::Viz][(i as usize).min(5)];
                self.acts.push(Act::Toggle(f));
            }
            MenuAct::LockWidgets => self.acts.push(Act::Toggle(settings::F::Locked)),
            MenuAct::Scene(crate::scene::Do::More(k)) => {
                // a further list, where this one was
                let (title, items) = self.scene.more_menu(k);
                let (x, y) = self.last_menu_at;
                let (w, h) = (self.w as i32, self.ui_h() as i32);
                let items = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
                self.menu = Some(crate::ctxmenu::CtxMenu::plain(x, y, title, items, w, h));
            }
            MenuAct::Scene(d) => {
                self.scene.woke();
                self.scene.act(d);
            }
            MenuAct::Shelf(a) => self.shelf_act(a),
            MenuAct::Desktop => self.desktop_menu = true,
        }
    }

    /// T: what there is to do right here, by the villager.
    fn open_todo(&mut self) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        let (title, items) = self.scene.todo_menu();
        if items.is_empty() {
            return;
        }
        let (x, y) = self.scene.zoom_focus();
        let (x, y) = (x / self.ui_k, y / self.ui_k);
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        let items = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        let m = CtxMenu::plain(x as i32 + 4, y as i32 - 6, title, items, w, h);
        self.last_menu_at = (m.x, m.y);
        self.menu = Some(m);
    }

    /// Whether key `k` is the one set for something (see `KeysCfg`).
    fn bound(spec: &str, k: KeyIn) -> bool {
        match (spec.to_ascii_lowercase().as_str(), k) {
            ("space" | " ", KeyIn::Char(' ')) => true,
            ("enter", KeyIn::Enter) => true,
            ("tab", KeyIn::Char('\t')) => true,
            (s, KeyIn::Char(c)) => s.chars().count() == 1 && s.chars().next().is_some_and(|x| x.eq_ignore_ascii_case(&c)),
            _ => false,
        }
    }

    /// Which of the villager's keys `k` is (index into `KEY_ACTIONS`).
    fn key_action(&self, k: KeyIn) -> Option<usize> {
        (0..config::KEY_ACTIONS.len()).find(|&i| Self::bound(self.cfg.keys.get(i), k))
    }

    /// Open the right-click menu at (x, y).
    fn open_menu(&mut self, x: i32, y: i32) {
        self.build_menu(x, y);
        // (drawn on the wallpaper: out from under the open windows)
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        if let Some(m) = &mut self.menu {
            m.avoid(&self.window_rects, (x, y), w, h);
        }
    }

    fn build_menu(&mut self, x: i32, y: i32) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        self.last_menu_at = (x, y + 1);
        use crate::widgets::shelf::{Hit, MenuAct as S};
        self.commit_city();
        let (w, h) = (self.w as i32, self.ui_h() as i32);
        // (the widgets and shelves: in their own cells)
        let (gx, gy) = self.part_pt(WIDGETS);
        if let Some((s, hit)) = self.widgets.shelf_hit(gx, gy) {
            let c = &self.cfg.shelves[s];
            let mut items: Vec<(String, MenuAct)> = vec![];
            let mut title = c.name.clone();
            if let Hit::Item(k) = hit {
                if let Some(it) = self.widgets.contents(s).and_then(|c| c.items.get(k).cloned()) {
                    title = it.name.clone();
                    items.push(("open".into(), MenuAct::Shelf(S::Open(s, k))));
                    items.push(("show in its folder".into(), MenuAct::Shelf(S::Reveal(s, k))));
                    items.push(("take off the shelf (to the desktop)".into(), MenuAct::Shelf(S::TakeOff(s, k))));
                }
            }
            let style = match c.style.as_str() {
                "wood" => "wooden shelf",
                "gift" => "gift box",
                _ => "frosted",
            };
            items.push(("open the shelf's folder".into(), MenuAct::Shelf(S::OpenFolder(s))));
            items.push(("choose its folder…".into(), MenuAct::Shelf(S::PickFolder(s))));
            items.push((format!("look: {style} ▸"), MenuAct::Shelf(S::Style(s))));
            items.push((format!("icons: {} ▸", if c.icons == "real" { "real" } else { "pixel art" }), MenuAct::Shelf(S::Icons(s))));
            items.push((if c.rolled { "unroll" } else { "roll up" }.into(), MenuAct::Shelf(S::Roll(s))));
            items.push(("remove the shelf (its files stay)".into(), MenuAct::Shelf(S::Remove(s))));
            self.settings.set_tab(settings::TAB_SHELVES);
            if cfg!(windows) && !self.keyboard {
                items.push(("Windows desktop menu…".into(), MenuAct::Desktop));
            }
            self.menu = Some(CtxMenu::with(x, y, title, items, w, h));
            return;
        }
        // a widget: its look, hiding it, its settings
        if let Some(i) = self.widgets.hit(gx, gy).filter(|&i| i < crate::widgets::BUILTIN) {
            let names = ["the clock", "system", "now playing", "weather", "map", "visualizer", "friends & messages"];
            let mut items: Vec<(String, MenuAct)> = vec![];
            if let Some(look) = self.cfg.widgets.looks.get(i) {
                let shown = match look {
                    "frosted" => "frosted",
                    "wood" => "wooden",
                    "gift" => "gift box",
                    _ => "classic",
                };
                items.push((format!("look: {shown} ▸"), MenuAct::Look(i as u8)));
            }
            items.push(("hide it".into(), MenuAct::Hide(i as u8)));
            items.push((if self.cfg.layout.locked { "unlock the widgets" } else { "lock the widgets in place" }.into(), MenuAct::LockWidgets));
            // ("menu…", at the bottom, opens on the Widgets tab)
            self.settings.set_tab(settings::TAB_WIDGETS);
            if cfg!(windows) && !self.keyboard {
                items.push(("Windows desktop menu…".into(), MenuAct::Desktop));
            }
            self.menu = Some(CtxMenu::with(x, y, names[i].to_string(), items, w, h));
            return;
        }
        let (wx, wy) = self.world_here();
        let (title, items) = self.scene.menu_at(wx, wy);
        let mut items: Vec<(String, MenuAct)> = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        if self.widgets.hit(gx, gy).is_none() {
            items.push(("new shelf here".into(), MenuAct::Shelf(S::New(gx, gy))));
            items.push(("new shelf from a folder…".into(), MenuAct::Shelf(S::NewFrom(gx, gy))));
        }
        if cfg!(windows) && !self.keyboard {
            items.push(("Windows desktop menu…".into(), MenuAct::Desktop));
        }
        self.menu = Some(CtxMenu::with(x, y, title, items, w, h));
    }

    /// Would a right-click at (x, y) be the village's (something of its own
    /// there, or a panel open that it closes)? Windows desktop: worked out
    /// as the pointer moves, so the mouse hook knows at once.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn right_click_ours(&self, fx: f32, fy: f32) -> bool {
        if self.settings.open || self.menu.is_some() {
            return true;
        }
        let pt = |k: f32| ((fx / k.max(1.0)).floor() as i32, (fy / k.max(1.0)).floor() as i32);
        let ((gx, gy), (bx, by)) = (pt(self.part_k[WIDGETS]), pt(self.part_k[BUTTONS]));
        self.widgets.hit(gx, gy).is_some_and(|i| i != 0) || settings::gear_rect(self.btn_wh.0 as i32).contains(bx, by) || {
            let (wx, wy) = self.world_of(fx, fy);
            let (title, items) = self.scene.menu_at(wx, wy);
            !matches!(title.as_str(), "here" | "the sky" | "the high sky" | "the village") || items.iter().any(|(_, d)| matches!(d, crate::scene::Do::DropOn(..)))
        }
    }

    /// Windows desktop: a right-click the hook took from the desktop. Opens
    /// the village's menu if something of its own is there (someone, a
    /// house, a widget, a shelf) and returns true; false lets Windows'
    /// desktop menu show.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn right_click(&mut self, fx: f32, fy: f32) -> bool {
        self.ptr = (fx, fy);
        let (x, y) = self.part_pt(WINDOWS);
        if self.settings.open {
            self.commit_city();
            self.settings.open = false;
            return true;
        }
        if self.menu.take().is_some() {
            return true;
        }
        // (the clock is mostly see-through: right-clicks there are the desktop's)
        let ours = self.right_click_ours(fx, fy);
        if ours {
            self.open_menu(x, y);
        }
        ours
    }

    /// Windows desktop: a desktop icon is being dragged (at this cell), or
    /// not any more. A shelf under it lights up.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn icon_drag(&mut self, at: Option<(f32, f32)>) {
        let k = self.part_k[WIDGETS].max(1.0);
        self.widgets.ext_drag = at.map(|(fx, fy)| ((fx / k).floor() as i32, (fy / k).floor() as i32));
    }

    /// Windows desktop: desktop icons were let go at (x, y): onto a shelf,
    /// they go in its folder.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn desktop_drop(&mut self, fx: f32, fy: f32) {
        let k = self.part_k[WIDGETS].max(1.0);
        let (x, y) = ((fx / k).floor() as i32, (fy / k).floor() as i32);
        #[cfg(windows)]
        if let Some((s, _)) = self.widgets.shelf_hit(x, y) {
            if let Some(to) = self.widgets.shelf_folder(s) {
                self.widgets.send(crate::widgets::shelf::Cmd::DesktopDrop { to });
            }
        }
        let _ = (x, y);
    }

    /// Shelf `s`'s scrollbar pressed or dragged at row `y`.
    fn shelf_bar(&mut self, s: usize, y: i32) {
        let Some(r) = self.widgets.rects.get(crate::widgets::BUILTIN + s).copied().flatten() else { return };
        let n = self.widgets.contents(s).map(|c| c.items.len()).unwrap_or(0);
        let to = crate::widgets::shelf::scroll_at(r, n, y - r.y - 1);
        if let Some(ui) = self.widgets.shelf_ui.get_mut(s) {
            ui.scroll = to;
        }
    }

    /// Open the system's folder picker (on a thread of its own) for a new
    /// shelf or to change shelf s's folder.
    fn pick_folder(&mut self, why: Pick) {
        if self.picking.is_some() {
            return;
        }
        let start = match why {
            Pick::Change(s) => self.cfg.shelves.get(s).map(crate::widgets::shelf::folder_of).unwrap_or_else(crate::widgets::shelf::home_dir),
            Pick::New(..) => crate::widgets::shelf::home_dir(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.picking = Some(rx);
        std::thread::spawn(move || {
            let got = crate::widgets::shelf::pick_folder(&start);
            let _ = tx.send((why, got));
        });
    }

    /// The folder picker answered.
    fn picked(&mut self, why: Pick, dir: Option<std::path::PathBuf>) {
        let Some(dir) = dir else { return };
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Shelf".into());
        match why {
            Pick::New(x, y) => {
                let (w, h) = (self.w.max(1) as f32, self.h.max(1) as f32);
                crate::widgets::shelf::add(&mut self.cfg, Some([x as f32 / w, y as f32 / h]));
                if let Some(c) = self.cfg.shelves.last_mut() {
                    c.folder = dir.to_string_lossy().to_string();
                    c.name = name;
                }
            }
            Pick::Change(s) => {
                if let Some(c) = self.cfg.shelves.get_mut(s) {
                    c.folder = dir.to_string_lossy().to_string();
                    c.order.clear();
                    // a shelf still called "Shelf…" takes the folder's name
                    if c.name.starts_with("Shelf") {
                        c.name = name;
                    }
                }
            }
        }
    }

    /// A press on shelf `s`.
    fn shelf_down(&mut self, s: usize, hit: crate::widgets::shelf::Hit, x: i32, y: i32) {
        use crate::widgets::shelf::Hit;
        let i = crate::widgets::BUILTIN + s;
        self.press_at = (x, y);
        match hit {
            Hit::Bar(_) => {
                self.bar_drag = Some(s);
                self.shelf_bar(s, y);
            }
            Hit::Edge(e) if !self.cfg.layout.locked => {
                self.resizing = Some((i, e));
                self.widgets.dragging = Some(i);
            }
            Hit::Item(k) => {
                self.shelf_press = Some(ShelfPress::Item(s, k));
                // held still, it's the item's menu
                self.press = Some((x, y, self.t()));
                for (j, ui) in self.widgets.shelf_ui.iter_mut().enumerate() {
                    ui.selected = (j == s).then_some(k);
                }
            }
            Hit::Title | Hit::Body | Hit::Edge(_) => {
                if hit == Hit::Title {
                    self.shelf_press = Some(ShelfPress::Title(s));
                } else {
                    self.press = Some((x, y, self.t()));
                }
                for ui in self.widgets.shelf_ui.iter_mut() {
                    ui.selected = None;
                }
                if !self.cfg.layout.locked {
                    if let Some(r) = self.widgets.rects.get(i).copied().flatten() {
                        self.drag = Some((i, x - r.x, y - r.y));
                        self.widgets.dragging = Some(i);
                    }
                }
            }
        }
    }

    /// An item carried off shelf `s` let go at (x, y): onto another shelf,
    /// or off the shelves onto the desktop (on Windows, where it was let go).
    fn shelf_drop(&mut self, (s, k): (usize, usize), x: i32, y: i32) {
        use crate::widgets::shelf::Cmd;
        let Some(path) = self.widgets.item_path(s, k) else { return };
        for ui in self.widgets.shelf_ui.iter_mut() {
            ui.selected = None;
        }
        match self.widgets.shelf_hit(x, y) {
            Some((t, _)) if t == s => {
                // back onto its own shelf: rearranged to where it was let go
                if let Some(r) = self.widgets.rects.get(crate::widgets::BUILTIN + s).copied().flatten() {
                    let n = self.widgets.contents(s).map(|c| c.items.len()).unwrap_or(0);
                    let scroll = self.widgets.shelf_ui.get(s).map(|u| u.scroll).unwrap_or(0);
                    let to = crate::widgets::shelf::slot_at(r, n, scroll, x, y);
                    if let (Some(order), Some(c)) = (self.widgets.reordered(s, k, to), self.cfg.shelves.get_mut(s)) {
                        c.order = order;
                    }
                }
            }
            Some((t, _)) => {
                if let Some(to) = self.widgets.shelf_folder(t) {
                    self.widgets.send(Cmd::Move { paths: vec![path], to, place: None });
                }
            }
            // onto another widget: nothing
            None if self.widgets.hit(x, y).is_some() => {}
            None => {
                // (a shelf's cells: the widgets' size, over the village's)
                let k = self.part_k[WIDGETS].max(1.0);
                let place = self.px_map.map(|(ox, oy, cw, ch)| (ox + (x as f32 * k * cw as f32) as i32 - 36, oy + (y as f32 * k * ch as f32) as i32 - 24));
                self.widgets.send(Cmd::Move { paths: vec![path], to: crate::widgets::shelf::desktop_dir(), place });
            }
        }
    }

    fn shelf_act(&mut self, a: crate::widgets::shelf::MenuAct) {
        use crate::widgets::shelf::{self, Cmd, MenuAct as S};
        match a {
            S::Open(s, k) => {
                if let Some(p) = self.widgets.item_path(s, k) {
                    open_detached(p);
                }
            }
            S::Reveal(s, k) => {
                if let Some(p) = self.widgets.item_path(s, k) {
                    shelf::reveal(&p);
                }
            }
            S::TakeOff(s, k) => {
                if let Some(p) = self.widgets.item_path(s, k) {
                    self.widgets.send(Cmd::Move { paths: vec![p], to: shelf::desktop_dir(), place: None });
                }
            }
            S::OpenFolder(s) => {
                if let Some(f) = self.widgets.shelf_folder(s) {
                    open_detached(f);
                }
            }
            S::Style(s) => {
                if let Some(c) = self.cfg.shelves.get_mut(s) {
                    let i = shelf::STYLES.iter().position(|&x| x == c.style).map(|i| (i + 1) % shelf::STYLES.len()).unwrap_or(1);
                    c.style = shelf::STYLES[i].into();
                }
            }
            S::Icons(s) => {
                if let Some(c) = self.cfg.shelves.get_mut(s) {
                    c.icons = if c.icons == "real" { "pixel".into() } else { "real".into() };
                }
            }
            S::Roll(s) => {
                if let Some(c) = self.cfg.shelves.get_mut(s) {
                    c.rolled = !c.rolled;
                }
            }
            S::Remove(s) => {
                if s < self.cfg.shelves.len() {
                    self.cfg.shelves.remove(s);
                    self.widgets.shelf_ui.clear();
                }
            }
            S::New(x, y) => {
                let (w, h) = (self.w.max(1) as f32, self.h.max(1) as f32);
                shelf::add(&mut self.cfg, Some([x as f32 / w, y as f32 / h]));
            }
            S::NewFrom(x, y) => self.pick_folder(Pick::New(x, y)),
            S::PickFolder(s) => self.pick_folder(Pick::Change(s)),
        }
    }

    /// The last frame: its cells and the pictures over them.
    pub fn output(&self) -> (&[Cell], &[crate::frame::Placed]) {
        (&self.frame.out, &self.frame.pictures)
    }

    pub fn step(&mut self) -> &[Cell] {
        let t = self.t();
        // testing: a preview set up once things have settled
        // (XMAS_TEST_PREVIEW=netcards: a card game at a table, say)
        if self.test_dropped < 100 && t > 3.0 {
            if let Ok(what) = std::env::var("XMAS_TEST_PREVIEW") {
                self.scene.preview_perch(&what);
            }
            self.test_dropped += 100;
        }
        // a press held still for a moment: the right-click menu
        if let Some((px, py, at)) = self.press {
            if t - at > 0.6 && self.menu.is_none() && !self.settings.open {
                if self.scene.holding() {
                    self.scene.let_go();
                }
                self.open_menu(px, py);
            }
        }
        // the folder picker answered
        if let Some(rx) = &self.picking {
            match rx.try_recv() {
                Ok((why, dir)) => {
                    self.picking = None;
                    self.picked(why, dir);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.picking = None,
                Err(_) => {}
            }
        }
        if self.settings.open {
            self.fill_net();
        }
        let old = self.applied.clone();
        let mut reseed = false;
        if self.quit_at.is_some_and(|q| self.t() >= q) {
            self.quit = true;
        }
        for a in std::mem::take(&mut self.acts) {
            match a {
                Act::Close => {
                    self.commit_city();
                    self.settings.open = false;
                }
                Act::ShelfOpen(i) => {
                    if let Some(c) = self.cfg.shelves.get(i as usize) {
                        open_detached(crate::widgets::shelf::folder_of(c));
                    }
                }
                Act::ShelfFolder(i) => self.pick_folder(Pick::Change(i as usize)),
                Act::ShelfNewFrom => self.pick_folder(Pick::New(self.w as i32 / 3, self.h as i32 / 3)),
                Act::Edit(w) => {
                    if self.keyboard || self.can_focus {
                        self.settings.begin_edit(&mut self.cfg, w);
                        self.want_keys = true;
                    } else {
                        // no keyboard behind the desktop icons: edit the file instead
                        self.commit_city();
                        if !self.sync.path().exists() {
                            self.sync.save_now(&self.cfg);
                        }
                        platform::open_in_editor(self.sync.path());
                    }
                }
                Act::Shuffle => {
                    self.seed = random_seed();
                    self.cfg.seed = self.seed as u64;
                    reseed = true;
                }
                Act::ExportSave => {
                    let t = self.t();
                    let s = self.current_save();
                    match crate::save::export(&s) {
                        Ok(p) => {
                            self.settings.toast = Some((format!("a copy saved: {}", p.display()), t + 4.0));
                            if let Some(dir) = p.parent() {
                                crate::widgets::shelf::open(dir);
                            }
                        }
                        Err(e) => self.settings.toast = Some((format!("couldn't save a copy ({e})"), t + 4.0)),
                    }
                }
                Act::ImportSave => {
                    if self.import_pick.is_none() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.import_pick = Some(rx);
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::save::pick());
                        });
                    }
                }
                Act::Quit => self.quit = true,
                Act::Uninstall | Act::ResetSettings => self.settings.confirm = Some(a),
                Act::NotNow => self.settings.confirm = None,
                Act::UninstallNow => {
                    self.settings.confirm = None;
                    let (msg, quit) = platform::uninstall();
                    let t = self.t();
                    self.settings.toast = Some((msg, t + 6.0));
                    if quit {
                        self.quit_at = Some(t + 2.0);
                    }
                }
                Act::ResetNow => {
                    self.settings.confirm = None;
                    self.cfg = self.cfg.reset_keeping_own();
                    let t = self.t();
                    self.settings.toast = Some(("settings reset".into(), t + 3.0));
                }
                Act::OpenConfig => {
                    self.commit_city();
                    if !self.sync.path().exists() {
                        self.sync.save_now(&self.cfg);
                    }
                    platform::open_in_editor(self.sync.path());
                }
                Act::OpenLog => {
                    if let Some(p) = platform::log_path() {
                        platform::open_in_editor(&p);
                    }
                }
                Act::PopOut => self.pop_out(false),
                Act::FullScreen if self.windowed => self.fullscreen_toggle = true,
                Act::FullScreen if self.wallpaper => self.set_wall_full(true),
                Act::FullScreen => self.pop_out(true),
                Act::BackToWallpaper => self.set_wall_full(false),
                Act::RestartNow => {
                    self.settings.restart_ask = false;
                    self.sync.save_now(&self.cfg);
                    RESTART_FOR_SCALE.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Act::RestartLater => self.settings.restart_ask = false,
                Act::Tab(i) => self.settings.set_tab(i as usize),
                Act::Restart => {
                    self.commit_city();
                    self.sync.save_now(&self.cfg);
                    self.write_save();
                    crate::update::restart_now();
                }
                Act::Help => {
                    self.settings.open = false;
                    self.settings.help = true;
                }
                Act::Pause => self.toggle_pause(),
                Act::Net(n) => self.net_act(n),
                Act::FriendSel(i) => self.settings.friend_sel = if self.settings.friend_sel == Some(i) { None } else { Some(i) },
                Act::CheckUpdate => crate::update::check_now(),
                Act::InstallUpdate => crate::update::install_now(),
                Act::DismissUpdate => crate::update::dismiss(),
                Act::SetWallpaper => {
                    // show how it's going (in the Desktop tab)
                    self.settings.wall_offer = false;
                    self.settings.open = true;
                    self.settings.set_tab(settings::TAB_DESKTOP);
                    #[cfg(target_os = "linux")]
                    crate::linwall::start_wallpaper();
                }
                Act::DismissWallOffer => self.settings.wall_offer = false,
                Act::LockScreen => {
                    #[cfg(target_os = "linux")]
                    crate::linwall::start_lock(!crate::linwall::info().lock_on);
                }
                Act::LockRedraw => {
                    #[cfg(target_os = "linux")]
                    crate::linwall::start_lock(true);
                }
                Act::InstallFfmpeg => {
                    #[cfg(target_os = "linux")]
                    if self.ffmpeg_job.is_none() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.ffmpeg_job = Some(rx);
                        let t = self.t();
                        self.settings.toast = Some(("installing ffmpeg… (the system asks for your password)".into(), t + 30.0));
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::linwall::install_ffmpeg());
                        });
                    }
                }
                Act::Autostart => {
                    let on = !self.settings.autostart.unwrap_or(false);
                    platform::autostart_set(on);
                    self.settings.autostart = platform::autostart_get();
                }
                a => {
                    if let (Act::Inc(F::Fps) | Act::Dec(F::Fps), Some(f)) = (a, self.fps_override.take()) {
                        // stepping the frame rate replaces the --fps override (and is saved)
                        self.cfg.fps = f;
                    }
                    // a guest: the village's own settings are the host's;
                    // a change is asked of them (made, by an admin)
                    let asked = match (&self.rules_on, self.rules_lock) {
                        // (a square's own settings can't be changed here; the
                        // rest are this villager's)
                        (Some(_), Some(crate::scene::online::Lock::Keeper)) => vec![],
                        (Some(r), _) => {
                            let mut probe = self.cfg.clone();
                            r.apply(&mut probe.scene);
                            settings::apply(&mut probe, a);
                            r.changes(&crate::scene::online::Rules::of(&probe.scene))
                        }
                        (None, _) => vec![],
                    };
                    if asked.is_empty() {
                        settings::apply(&mut self.cfg, a);
                    }
                    for (k, v) in asked {
                        self.scene.ask_rule(&k, &v);
                    }
                    if let Act::FriendRemove(_) = a {
                        self.settings.friend_sel = None;
                    }
                    // let the new level be heard
                    if let Act::Inc(f) | Act::Dec(f) = a {
                        if matches!(f, F::SoundMaster | F::SoundSfx) {
                            self.ui_sounds.push(SoundEvent { sfx: Sfx::Chime, pan: 0.5, gain: 1.0 });
                        }
                    }
                }
            }
        }
        // pick up edits to the config file (by hand, the settings window,
        // the tray) and save ours; nothing is written while a drag is in
        // progress or while the file doesn't parse
        // the pet named / recoloured in the village: keep it in the config
        if let Some(p) = self.scene.take_pet_change() {
            self.cfg.scene.pet = p;
        }
        if self.sync.poll(&mut self.cfg, self.drag.is_none()) && self.cfg.seed != 0 && self.cfg.seed as u32 != self.seed {
            self.seed = self.cfg.seed as u32;
            reseed = true;
        }
        if std::mem::take(&mut self.reseed_soon) {
            reseed = true;
        }
        // a save to import, picked: its village and its things
        if let Some(rx) = &self.import_pick {
            match rx.try_recv() {
                Ok(p) => {
                    self.import_pick = None;
                    let t = self.t();
                    match p.as_deref().and_then(crate::save::read) {
                        Some(s) => {
                            self.seed = s.seed;
                            if self.cfg.seed != 0 {
                                self.cfg.seed = s.seed as u64;
                            }
                            let who = if s.name.is_empty() { "the".to_string() } else { format!("{}'s", s.name) };
                            // the villager too: their name, pet, friends, keys
                            if let Some(you) = &s.you {
                                you.apply(&mut self.cfg);
                            }
                            self.settings.toast = Some((format!("imported {who} village"), t + 3.0));
                            self.importing_save = Some(s);
                            reseed = true;
                        }
                        None if p.is_some() => self.settings.toast = Some(("that isn't an xmas-tui save".into(), t + 3.0)),
                        None => {}
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.import_pick = None,
                Err(_) => {}
            }
        }
        // ffmpeg being installed: how it went
        if let Some(rx) = &self.ffmpeg_job {
            if let Ok(r) = rx.try_recv() {
                self.ffmpeg_job = None;
                let t = self.t();
                self.settings.toast = Some((r.unwrap_or_else(|e| e), t + 5.0));
            }
        }
        self.update_online();
        // saved now and then (and on the way out)
        // linked devices: a change here reaches the others in a moment
        // (an unchanged side isn't sent again)
        if self.linked.is_some() && self.t() - self.synced_at > 2.0 {
            self.synced_at = self.t();
            self.send_sync();
        }
        if self.saves && self.t() - self.saved_at > 60.0 {
            self.write_save();
        }
        self.frame.glow_near = self.cfg.scene.glow;
        self.frame.glow_far = self.cfg.scene.glow;
        self.settings.file_error = self.sync.error.is_some();
        self.settings.ui_layer_ok = self.ui_ok;
        self.settings.fps_override = self.fps_override;
        // `applied` always advances (also mid-drag), so a change is acted on
        // once instead of rebuilding the scene every frame until the drop
        if reseed || !config::same(&self.cfg, &old) {
            // walking speed and the traffic side apply live; anything else
            // about the scene rebuilds it
            if reseed || self.cfg.scene.build_key() != old.scene.build_key() {
                self.new_scene();
                // a save imported: its things, in its village
                if let Some(s) = self.importing_save.take() {
                    self.scene.set_pockets(s.pockets);
                    self.scene.place_player(s.player);
                    self.write_save();
                }
            }
            #[cfg(any(target_os = "linux", windows))]
            if let Some(fx) = &self.fx {
                let c = self.fx_cfg();
                if let Ok(mut f) = fx.lock() {
                    *f = c;
                }
            }
            if self.drag.is_some() {
                // keep the widget being dragged where the pointer has it
                let mut c = self.cfg.clone();
                c.layout = self.widgets.cfg.layout.clone();
                for (a, b) in c.shelves.iter_mut().zip(&self.widgets.cfg.shelves) {
                    (a.pos, a.size) = (b.pos, b.size);
                }
                self.widgets.apply(&c);
            } else {
                self.widgets.apply(&self.cfg);
            }
            if (self.cfg.ui.scale - old.ui.scale).abs() > 1e-3 || (self.cfg.ui.widgets_k() - old.ui.widgets_k()).abs() > 1e-3 || (self.cfg.ui.buttons_k() - old.ui.buttons_k()).abs() > 1e-3 {
                let (w, h) = (self.sw, self.sh);
                self.regrid(w, h, false);
            }
            if (self.cfg.scale - old.scale).abs() > 1e-3 {
                // wait for the arrows / keys to settle before resizing
                self.scale_pending = Some((self.cfg.scale, t));
            }
            if self.cfg.updates != old.updates {
                crate::update::configure(self.cfg.updates.check, self.cfg.updates.auto_install);
            }
            self.applied = self.cfg.clone();
        }

        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.25);
        self.last = now;
        let (mx, my) = self.move_dir();
        self.scene.set_move(mx, my);
        self.scene.set_ui_look(&self.cfg.ui.look);
        if self.jump_down && self.t() > self.jump_until {
            self.jump_down = false;
            self.scene.release_jump();
        }
        let left = match self.cfg.scene.traffic.as_str() {
            "left" => true,
            "right" => false,
            _ => crate::widgets::data::drives_left(&crate::widgets::data::country()),
        };
        self.scene.set_live(self.cfg.scene.walk_speed, left, self.cfg.scene.most_at_once);
        crate::scene::events::set_talk(self.cfg.scene.talk_time, self.cfg.scene.chattiness);
        self.scene.set_pet(&self.cfg.scene.pet);
        self.scene.set_more_pets(&self.cfg.scene.pets);
        self.scene.set_name(&self.cfg.scene.name);
        self.scene.set_friends(&self.cfg.scene.friends);
        self.scene.set_wear(&self.cfg.scene.wear, &self.cfg.scene.motto);
        self.scene.set_autopilot(self.cfg.scene.autopilot);
        self.scene.set_only_friends(self.cfg.scene.only_friends);
        // hosting: the village's settings, for the guests; a guest: the
        // host's, the village made again when they change
        self.scene.set_rules(&crate::scene::online::Rules::of(&self.cfg.scene));
        let whose = self.scene.rules_now(&self.cfg.scene);
        self.rules_lock = whose.as_ref().map(|r| r.1);
        // (a public square: its own settings greyed out in the menu)
        self.settings.locked = self.rules_lock == Some(crate::scene::online::Lock::Keeper);
        let rules = whose.map(|r| r.0);
        if rules != self.rules_on {
            self.rules_on = rules;
            self.new_scene();
        }
        if let Some((sc, at)) = self.scale_pending {
            if t - at > 0.6 {
                self.scale_pending = None;
                apply_scale(sc);
            }
        }
        if SCALE_NEEDS_RESTART.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.settings.restart_ask = true;
        }
        {
            use chrono::Timelike;
            let now = crate::clock::now();
            let mins = now.hour() as f32 * 60.0 + now.minute() as f32 + now.second() as f32 / 60.0;
            let (rise, set) = crate::widgets::data::sun_times().unwrap_or((450.0, 1020.0));
            // a guest: the host's clock (the same sky for everyone there)
            self.scene.online.set_clock(mins, rise, set);
            let (mins, rise, set) = self.scene.online.village_clock().unwrap_or((mins, rise, set));
            // (the village's time of day: the host's, or a square's, when it's theirs)
            let time = self.rules_on.as_ref().map(|r| r.time.as_str()).unwrap_or(self.cfg.scene.time.as_str());
            self.scene.set_daylight(crate::scene::daylight::compute(time, mins, rise, set));
        }
        // resting: only the online side goes on (the village held as it
        // was, its picture kept); back again, it carries on from there
        let resting = self.resting();
        if resting != self.scene.resting() {
            self.scene.set_resting(resting);
            self.rest_base = None;
        }
        self.settings.resting = resting.then(|| if self.paused { String::new() } else { self.rest_why.clone().unwrap_or_default() });
        if resting {
            self.scene.rest_tick(dt);
        } else {
            self.scene.update(dt, t);
        }
        let mut sounds = self.scene.take_sounds();
        let mut ui = std::mem::take(&mut self.ui_sounds);
        if let Some(a) = &self.audio {
            if !self.cfg.sound.cursor {
                sounds.retain(|e| e.sfx != Sfx::Cursor);
            }
            sounds.append(&mut ui);
            a.frame(sounds, self.scene.audio_state(t));
            // go quiet ~0.6 s after other media starts; come back 3 s after
            // it stops (so skipping tracks doesn't make us pop in and out)
            let playing = self.cfg.sound.pause_for_media && self.widgets.media_playing();
            if playing != self.media_quiet {
                let since = *self.media_flip.get_or_insert(now);
                let wait = if playing { 0.6 } else { 3.0 };
                if now.duration_since(since).as_secs_f32() >= wait {
                    self.media_quiet = playing;
                    self.media_flip = None;
                }
            } else {
                self.media_flip = None;
            }
            self.settings.media_quiet = self.media_quiet;
            let mut want = self.cfg.sound.clone();
            if self.media_quiet || self.hushed || resting {
                want.enabled = false;
            }
            if want != self.sound_sent && a.config(&want) {
                self.sound_sent = want;
            }
            // a shared village's music: the host's is sent to the guests;
            // a guest plays the same, unless their music's off or set to
            // a style of their own
            let host = self.scene.online.village().is_some_and(|(_, h)| h);
            self.scene.online.set_tune(host.then(|| a.cue()).flatten().map(|c| crate::net::Tune { style: c.style, carol: c.carol, seed: c.seed, t: c.t, at: crate::clock::now().timestamp_millis() }));
            if let Some(m) = self.scene.online.take_tune() {
                if crate::audio::follows_host(&self.cfg.sound) && !resting {
                    a.follow(crate::audio::Cue { style: m.style, carol: m.carol, seed: m.seed, t: m.t });
                }
            }
        }
        self.frame.clear();
        self.frame.safe_bottom = self.safe_bottom();
        // the zoomed view eases after the villager
        let want = self.scene.zoom_focus();
        let f = match self.zoom_focus {
            Some((fx, fy)) if (fx - want.0).abs() < self.sw as f32 * 0.5 => {
                let k = 1.0 - (-dt * 5.0).exp();
                (fx + (want.0 - fx) * k, fy + (want.1 - fy) * k)
            }
            _ => want,
        };
        self.zoom_focus = Some(f);
        // the zoom eases to where it's been set
        // (it settles on a twentieth: 1.1, 1.25 … wherever it was set)
        let want = settings::zoom_settled(self.cfg.zoom);
        let dz = want - self.zoom_now;
        self.zoom_now = if dz.abs() < 0.01 { want } else { self.zoom_now + dz * (1.0 - (-dt * 9.0).exp()) };
        // (the scene's own panels go on the windows' layer, when they have one)
        let windows_apart = self.part_layer[WINDOWS].is_some();
        self.scene.ui_wh = windows_apart.then_some((self.w as i32, self.h as i32));
        self.scene.ui_k = self.ui_k;
        match self.rest_base.as_ref().filter(|b| resting && (b.w, b.h) == (self.frame.w, self.frame.h)) {
            // resting: the village's picture as it stood
            Some(b) => self.frame.clone_from(b),
            None => {
                self.zoom_xf = self.scene.render(&mut self.frame, &mut self.spare, t, self.zoom_now, Some(f));
                if resting {
                    self.rest_base = Some(self.frame.clone());
                }
            }
        }
        // the UI from here: each part on its own layer at its own size, if
        // it has one (bottom to top: the widgets, the buttons, the windows)
        let (aspect, safe) = (self.frame.cell_aspect, self.safe_bottom());
        for l in &mut self.layers {
            l.frame.clear();
            l.frame.cell_aspect = aspect;
            l.frame.safe_bottom = (safe as f32 / l.k).ceil() as usize;
        }
        let i = self.part_in(WIDGETS);
        self.widgets.map = Some(self.scene.minimap());
        self.widgets.friends = self.scene.friends_view();
        self.widgets.draw(&mut self.frame, t);
        self.part_out(i);
        // the buttons up top
        let i = self.part_in(BUTTONS);
        self.settings.draw_gear(&mut self.frame, t);
        {
            let o = &self.scene.online;
            let about = o.friends().iter().filter(|(_, h)| h.as_ref().is_some_and(|h| h.on)).count();
            let state = match (o.wanted(), o.link()) {
                (false, _) => 0,
                (true, crate::net::Link::On) => 2,
                _ => 1,
            };
            let asked = self.scene.asked_now() || !o.book.asking.is_empty();
            self.settings.draw_online(&mut self.frame, state, about, asked, t);
        }
        self.part_out(i);
        // the windows: the UI from here on keeps above the taskbar
        let windows_layer = self.part_in(WINDOWS);
        let full_h = self.frame.ui_in();
        if windows_apart {
            self.scene.draw_ui(&mut self.frame);
        }
        // (the post, questions asked, cards and games with friends: over the widgets)
        self.scene.draw_top(&mut self.frame);
        // the widgets' tops are something to climb up onto (next frame)
        // (in the village's cells: the widgets' may be bigger, on a layer of their own)
        let k = self.part_k[WIDGETS];
        let sc = |v: i32| (v as f32 * k).round() as i32;
        self.scene.set_widget_ledges(self.widgets.rects.iter().enumerate().filter_map(|(i, r)| r.map(|r| (i, sc(r.x), sc(r.y), sc(r.w), sc(r.h)))).collect());
        // and the open windows' tops (the window-snow overlay sees them)
        #[cfg(any(target_os = "linux", windows))]
        if let Some((sw, sh, ox, oy, cw, ch)) = self.screen_map.filter(|m| m.4 > 0.0 && m.5 > 0.0) {
            let tops = crate::winsnow::tops();
            let cells: Vec<(u64, f32, f32, f32, f32)> = tops.iter().map(|t| (t.id, (t.x0 * sw - ox) / cw, (t.x1 * sw - ox) / cw, (t.y * sh - oy) / ch, (t.bottom * sh - oy) / ch)).collect();
            // (menus keep clear of them: in the UI's cells)
            let k = self.ui_k;
            self.window_rects = cells.iter().map(|&(_, x0, x1, y0, y1)| ((x0 / k).floor() as i32, (y0 / k).floor() as i32, (x1 / k).ceil() as i32, (y1 / k).ceil() as i32)).collect();
            // testing: drop the villager in on the first window to show up
            // (a frame after it's seen, once the village has it as a ledge)
            if let Some(&(_, x0, x1, y0, _)) = cells.first().filter(|_| std::env::var_os("XMAS_TEST_DROP_ON_WINDOW").is_some() && self.test_dropped < 2) {
                self.test_dropped += 1;
                if self.test_dropped == 2 {
                    self.scene.drop_on_screen((x0 + x1) / 2.0, y0);
                }
            }
            self.scene.set_window_ledges(cells);
        }
        self.settings.meters = self.audio.as_ref().filter(|_| self.settings.open).and_then(|a| a.meters.lock().ok().map(|m| *m));
        // a popped-out window tells the wallpaper it's open
        if self.popout && t - self.popout_touched > 1.0 {
            self.popout_touched = t;
            let _ = std::fs::write(crate::platform::popout_path(), b"1");
        }
        // the tray's settings window (another process) shows the levels and
        // the update status too, and can ask for an update check
        if t - self.meters_check > 1.0 {
            self.meters_check = t;
            self.meters_wanted = self.audio.is_some() && crate::platform::fresh(&crate::platform::meters_want_path(), 3.0);
            if !self.popout {
                crate::update::serve();
            }
        }
        if self.audio.is_some() {
            if self.meters_wanted && t - self.meters_sent > 0.08 {
                self.meters_sent = t;
                if let Some(m) = self.audio.as_ref().and_then(|a| a.meters.lock().ok().map(|m| *m)) {
                    crate::platform::write_meters(&m);
                }
            }
        }
        self.settings.draw_banner(&mut self.frame, t);
        {
            let shown = self.shown_cfg().into_owned();
            self.settings.draw(&mut self.frame, &shown, t);
        }
        self.settings.draw_help(&mut self.frame, &self.cfg.keys, &self.cfg.ui.look);
        if let Some(m) = &self.menu {
            m.draw(&mut self.frame);
            crate::widgets::looks::apply(&mut self.frame, m.rect(), &self.cfg.ui.look, crate::frame::Bg::Mix(crate::color::pal::UI_PANEL, 0.95), 5);
        }
        self.draw_typing();
        self.frame.ui_out(full_h);
        self.part_out(windows_layer);
        // (the layers aren't composed: what's under their panels cut away here)
        for l in &mut self.layers {
            l.frame.cut_hidden();
        }
        self.frame.compose();
        &self.frame.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_widget_resizes_by_its_corner() {
        let dir = std::env::temp_dir().join(format!("xmas-widget-size-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        cfg.sound.enabled = false;
        (cfg.widgets.music, cfg.widgets.weather, cfg.widgets.map, cfg.clock.enabled) = (false, false, false, false);
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        app.step();
        let r = app.widgets.rects[1].expect("the system widget");
        let (cx, cy) = (r.x + r.w - 1, r.y + r.h - 1);
        app.mouse(MouseIn::Move, cx, cy);
        app.mouse(MouseIn::Down(Btn::Left), cx, cy);
        app.mouse(MouseIn::Drag, cx + 12, cy + 3);
        app.mouse(MouseIn::Up(Btn::Left), cx + 12, cy + 3);
        app.step();
        assert_eq!(app.cfg.layout.sizes.get("system"), Some(&[r.w + 12, r.h + 3]));
        let r2 = app.widgets.rects[1].unwrap();
        assert_eq!((r2.x, r2.y, r2.w, r2.h), (r.x, r.y, r.w + 12, r.h + 3), "same place, bigger");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_widget_has_its_own_menu_with_its_look() {
        let dir = std::env::temp_dir().join(format!("xmas-widget-menu-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        cfg.sound.enabled = false;
        (cfg.widgets.music, cfg.widgets.weather, cfg.widgets.map, cfg.clock.enabled) = (false, false, false, false);
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        app.step();
        let r = app.widgets.rects[1].expect("the system widget");
        assert!(app.right_click((r.x + 3) as f32 + 0.5, (r.y + 2) as f32 + 0.5));
        let m = app.menu.as_ref().expect("a menu");
        assert_eq!(m.title, "system");
        assert!(m.items[0].0.starts_with("look: classic"), "{:?}", m.items.iter().map(|i| &i.0).collect::<Vec<_>>());
        let (mx, my) = (m.x + 2, m.y + 1);
        app.mouse(MouseIn::Down(Btn::Left), mx, my);
        app.mouse(MouseIn::Up(Btn::Left), mx, my);
        app.step();
        assert_eq!(app.cfg.widgets.looks.system, "frosted");
        // its "menu…" opens on the Widgets tab
        assert!(app.right_click((r.x + 3) as f32 + 0.5, (r.y + 2) as f32 + 0.5));
        let m = app.menu.as_ref().unwrap();
        let (mx, my) = (m.x + 2, m.y + m.items.len() as i32);
        assert_eq!(m.items.last().unwrap().0, "menu…");
        app.mouse(MouseIn::Down(Btn::Left), mx, my);
        app.mouse(MouseIn::Up(Btn::Left), mx, my);
        assert!(app.settings.open && app.settings.tab == settings::TAB_WIDGETS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Press on a character, drag them up into the sky and let go: the
    /// app routes it to the scene (not to a scene click or a widget drag),
    /// the pointer shows "grabbing", and they end up back down, let go.
    #[test]
    fn mouse_picks_up_and_drops_a_character() {
        let dir = std::env::temp_dir().join(format!("xmas-app-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.widgets.music = false;
        cfg.widgets.weather = false;
        cfg.widgets.system = false;
        cfg.clock.enabled = false;
        cfg.widgets.map = false; // (a widget under the pointer takes the press)
        let mut app = App::new(cfg, 9, &path, 300, 60, true, true, false);
        app.scene.demo(0.0);
        app.step();
        // find someone to pick up
        let spot = (0..60).rev().flat_map(|y| (0..300).map(move |x| (x, y))).find(|&(x, y)| app.scene.grabbable(x, y));
        let (x, y) = spot.expect("nobody on screen to pick up");
        app.mouse(MouseIn::Move, x, y);
        assert_eq!(app.pointer(), "grab");
        app.mouse(MouseIn::Down(Btn::Left), x, y);
        assert!(app.scene.holding());
        assert_eq!(app.pointer(), "grabbing");
        app.mouse(MouseIn::Drag, x + 6, 8);
        app.step();
        app.mouse(MouseIn::Up(Btn::Left), x + 6, 8);
        assert!(!app.scene.holding());
        // the app runs on real time: give them ~1.5 s to fall
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_millis(12));
            app.step();
        }
        // landed: nobody is hovering in the sky where we let go
        assert!(!app.scene.grabbable(x + 6, 8));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Shelves: items show up from their folders; a double-click opens one;
    /// dragging an item onto another shelf moves the file there; a click on
    /// the title rolls the shelf up; right-clicking the bare scene offers a
    /// new shelf, and a shelf's own menu can remove it.
    #[test]
    fn shelves_show_open_and_move_files() {
        use crate::widgets::{shelf, BUILTIN};
        let dir = std::env::temp_dir().join(format!("xmas-app-shelves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("note.txt"), "hi").unwrap();
        std::fs::write(a.join("zeta.txt"), "z").unwrap();
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        (cfg.widgets.music, cfg.widgets.weather, cfg.widgets.system, cfg.widgets.map, cfg.clock.enabled) = (false, false, false, false, false);
        cfg.sound.enabled = false;
        let shelf_at = |name: &str, folder: &std::path::Path, x: f32| crate::config::ShelfCfg { name: name.into(), folder: folder.to_string_lossy().into(), pos: Some([x, 0.2]), size: [40, 15], ..Default::default() };
        cfg.shelves = vec![shelf_at("A", &a, 0.05), shelf_at("B", &b, 0.55)];
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        let wait = |app: &mut App, done: &dyn Fn(&App) -> bool| {
            for _ in 0..300 {
                app.step();
                if done(app) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        assert!(wait(&mut app, &|app| app.widgets.contents(0).is_some_and(|c| c.items.len() == 2)), "shelf A never showed its files");
        let ra = app.widgets.rects[BUILTIN].unwrap();
        let rb = app.widgets.rects[BUILTIN + 1].unwrap();
        let (x0, y0, _, _) = shelf::grid(ra);
        let (ix, iy) = (x0 + 2, y0 + 1); // on the first item's icon
        assert_eq!(app.widgets.shelf_hit(ix, iy), Some((0, shelf::Hit::Item(0))));
        // double-click: opens it
        for _ in 0..2 {
            app.mouse(MouseIn::Down(Btn::Left), ix, iy);
            app.mouse(MouseIn::Up(Btn::Left), ix, iy);
        }
        assert!(OPENED.lock().unwrap().iter().any(|p| p.ends_with("note.txt")), "a double-click didn't open the item");
        // drag it onto shelf B
        app.mouse(MouseIn::Down(Btn::Left), ix, iy);
        app.mouse(MouseIn::Drag, ix + 5, iy + 1);
        app.mouse(MouseIn::Drag, rb.x + 5, rb.y + 4);
        app.step();
        app.mouse(MouseIn::Up(Btn::Left), rb.x + 5, rb.y + 4);
        assert!(wait(&mut app, &|_| b.join("note.txt").exists()), "the file didn't move to shelf B's folder");
        assert!(!a.join("note.txt").exists());
        assert!(wait(&mut app, &|app| app.widgets.contents(1).is_some_and(|c| c.items.len() == 1)));
        // a click on the title rolls it up
        app.mouse(MouseIn::Down(Btn::Left), ra.x + 6, ra.y);
        app.mouse(MouseIn::Up(Btn::Left), ra.x + 6, ra.y);
        assert!(app.cfg.shelves[0].rolled);
        // right-click on the bare scene: "new shelf here"
        app.mouse(MouseIn::Down(Btn::Right), 150, 50);
        let m = app.menu.as_ref().expect("no menu");
        let k = m.items.iter().position(|(l, _)| l == "new shelf here").expect("no 'new shelf here'");
        let (mx, my) = (m.x + 3, m.y + 1 + k as i32);
        app.mouse(MouseIn::Down(Btn::Left), mx, my);
        assert_eq!(app.cfg.shelves.len(), 3);
        // a shelf's menu removes it (its files stay)
        app.step();
        app.mouse(MouseIn::Down(Btn::Right), rb.x + 3, rb.y + rb.h - 2);
        let m = app.menu.as_ref().expect("no shelf menu");
        let k = m.items.iter().position(|(l, _)| l.starts_with("remove the shelf")).expect("no remove");
        let (mx, my) = (m.x + 3, m.y + 1 + k as i32);
        app.mouse(MouseIn::Down(Btn::Left), mx, my);
        assert_eq!(app.cfg.shelves.len(), 2);
        assert!(b.join("note.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Shelves: an item dragged to another spot on its own shelf moves
    /// there (and stays there); a shelf's left edge resizes it.
    #[test]
    fn shelves_rearrange_and_resize_by_an_edge() {
        use crate::widgets::{shelf, BUILTIN};
        let dir = std::env::temp_dir().join(format!("xmas-app-arrange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(dir.join(f), f).unwrap();
        }
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        (cfg.widgets.music, cfg.widgets.weather, cfg.widgets.system, cfg.widgets.map, cfg.clock.enabled) = (false, false, false, false, false);
        cfg.sound.enabled = false;
        cfg.shelves = vec![crate::config::ShelfCfg { name: "S".into(), folder: dir.to_string_lossy().into(), pos: Some([0.3, 0.2]), size: [40, 15], ..Default::default() }];
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        for _ in 0..300 {
            app.step();
            if app.widgets.contents(0).is_some_and(|c| c.items.len() == 3) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let names = |app: &App| app.widgets.contents(0).unwrap().items.iter().map(|i| i.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&app), ["a.txt", "b.txt", "c.txt"]);
        let r = app.widgets.rects[BUILTIN].unwrap();
        let (x0, y0, _, _) = shelf::grid(r);
        // carry a.txt over to the third slot
        app.mouse(MouseIn::Down(Btn::Left), x0 + 2, y0 + 1);
        app.mouse(MouseIn::Drag, x0 + 6, y0 + 2);
        app.mouse(MouseIn::Drag, x0 + 2 * shelf::SLOT_W + 3, y0 + 1);
        app.step();
        app.mouse(MouseIn::Up(Btn::Left), x0 + 2 * shelf::SLOT_W + 3, y0 + 1);
        app.step();
        assert_eq!(names(&app), ["b.txt", "c.txt", "a.txt"], "not rearranged");
        assert_eq!(app.cfg.shelves[0].order, ["b.txt", "c.txt", "a.txt"]);
        // the left edge, dragged 6 to the left: wider, same right side
        let right = r.x + r.w;
        app.mouse(MouseIn::Down(Btn::Left), r.x, r.y + 5);
        app.mouse(MouseIn::Drag, r.x - 6, r.y + 5);
        app.mouse(MouseIn::Up(Btn::Left), r.x - 6, r.y + 5);
        app.step();
        let r2 = app.widgets.rects[BUILTIN].unwrap();
        assert_eq!((r2.x, r2.x + r2.w), (r.x - 6, right), "the left edge didn't resize it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A left press held still opens the right-click menu (which stays up
    /// after letting go); a quick click doesn't; dragging the settings panel
    /// by its title bar moves it.
    #[test]
    fn long_press_opens_the_menu_and_the_panel_moves() {
        let dir = std::env::temp_dir().join(format!("xmas-app-press-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        cfg.widgets.music = false;
        cfg.widgets.weather = false;
        cfg.widgets.system = false;
        cfg.widgets.map = false;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        let pause = |app: &mut App, ms: u64| {
            for _ in 0..ms / 10 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                app.step();
            }
        };
        app.step();
        // a quick click on the snow: no menu
        app.mouse(MouseIn::Down(Btn::Left), 20, 55);
        pause(&mut app, 100);
        app.mouse(MouseIn::Up(Btn::Left), 20, 55);
        pause(&mut app, 700);
        assert!(app.menu.is_none());
        // held still: the menu
        app.mouse(MouseIn::Down(Btn::Left), 20, 55);
        pause(&mut app, 800);
        assert!(app.menu.is_some(), "long press didn't open the menu");
        app.mouse(MouseIn::Up(Btn::Left), 20, 55);
        pause(&mut app, 100);
        assert!(app.menu.is_some(), "menu closed on letting go");
        app.mouse(MouseIn::Down(Btn::Right), 20, 55);
        assert!(app.menu.is_none());
        // the settings button opens the panel; its title bar moves it
        let g = settings::gear_rect(300);
        app.mouse(MouseIn::Down(Btn::Left), g.x + 3, g.y);
        app.mouse(MouseIn::Up(Btn::Left), g.x + 3, g.y);
        app.step();
        assert!(app.settings.open);
        let r = app.settings.rect.unwrap();
        app.mouse(MouseIn::Down(Btn::Left), r.x + 4, r.y);
        app.mouse(MouseIn::Drag, r.x - 20, r.y + 5);
        app.mouse(MouseIn::Up(Btn::Left), r.x - 20, r.y + 5);
        app.step();
        let r2 = app.settings.rect.unwrap();
        assert_eq!((r2.x, r2.y), (r.x - 24, r.y + 5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Holding D walks right; letting go (a reported release) stops at once;
    /// without releases a tap walks only a moment.
    #[test]
    fn held_keys_walk_the_player() {
        let dir = std::env::temp_dir().join(format!("xmas-app-keys-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.scene.critters = 0.0;
        cfg.widgets.music = false;
        cfg.widgets.weather = false;
        cfg.widgets.system = false;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        let pause = |app: &mut App, ms: u64| {
            for _ in 0..ms / 10 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                app.step();
            }
        };
        app.step();
        let x0 = app.scene.player_x().unwrap();
        // no releases reported (yet): a tap moves a little, then stops
        app.key(KeyIn::Char('d'));
        pause(&mut app, 1000);
        let x1 = app.scene.player_x().unwrap();
        assert!(x1 > x0 + 3.0 && x1 < x0 + 9.0, "tap walked {x0} → {x1}");
        // with releases: held until let go
        app.key(KeyIn::Right);
        pause(&mut app, 400);
        app.key_up(KeyIn::Right);
        let x2 = app.scene.player_x().unwrap();
        pause(&mut app, 300);
        let x3 = app.scene.player_x().unwrap();
        assert!(x2 > x1 + 2.0, "held walked {x1} → {x2}");
        assert!((x3 - x2).abs() < 0.01, "kept walking after release: {x2} → {x3}");
        // two keys at once (W then D): only the last one auto-repeats, and
        // the first stays held anyway — diagonal for as long as both are down
        let y0 = app.scene.player_y().unwrap();
        app.key(KeyIn::Char('w'));
        pause(&mut app, 200);
        app.key(KeyIn::Char('d'));
        for _ in 0..12 {
            pause(&mut app, 150);
            app.key_repeat(KeyIn::Char('d'));
        }
        let (x4, y4) = (app.scene.player_x().unwrap(), app.scene.player_y().unwrap());
        assert!(x4 > x3 + 3.0 && y4 < y0 - 1.0, "diagonal: ({x3},{y0}) → ({x4},{y4})");
        // a long hold with slow (or no) repeats keeps walking
        pause(&mut app, 1500);
        assert!(app.scene.player_x().unwrap() > x4 + 2.0, "a held key timed out");
        app.key_up(KeyIn::Char('D'));
        app.key_up(KeyIn::Char('w'));
        let x5 = app.scene.player_x().unwrap();
        pause(&mut app, 300);
        assert!((app.scene.player_x().unwrap() - x5).abs() < 0.01, "kept walking after both released");
        let x3 = x5;
        // typing the city in settings never moves anyone
        app.settings.edit = Some(settings::Field::City);
        app.settings.editing = Some(String::new());
        app.key(KeyIn::Char('a'));
        pause(&mut app, 300);
        assert!((app.scene.player_x().unwrap() - x3).abs() < 0.01);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Zoom: Ctrl + / - steps it, clicks land on what's drawn under the
    /// pointer; Ctrl Shift + steps the resolution by 0.05; Esc opens the
    /// settings when nothing else is open, and the controls card comes and
    /// goes with "?".
    /// The widgets, the buttons up top and the windows each at a size of
    /// their own: each on its own layer (the village's own frame for
    /// those at its size), and a click finds each at its own size.
    #[test]
    fn each_part_of_the_ui_has_a_size_of_its_own() {
        let dir = std::env::temp_dir().join(format!("xmas-app-parts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.ui.scale = 1.5;
        cfg.ui.widgets = Some(1.0);
        cfg.ui.buttons = Some(2.0);
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        app.set_ui_layer_ok(true);
        app.step();
        assert_eq!(app.part_layer, [None, Some(0), Some(1)], "widgets on the village's frame, buttons and windows on layers");
        {
            let ls = app.ui_layers();
            assert_eq!((ls.len(), ls[0].0.w, ls[0].1, ls[1].0.w, ls[1].1), (2, 150, 2.0, 200, 1.5));
            // the buttons drawn on theirs
            let f = ls[0].0;
            let g = settings::gear_rect(150);
            assert!(f.glyph[g.y as usize * f.w + g.x as usize + 1].is_some_and(|c| c.ch == '❄'));
        }
        // the settings button, where it is at the buttons' size
        let g = settings::gear_rect(150);
        app.mouse_at(MouseIn::Down(Btn::Left), ((g.x + 3) as f32 + 0.5) * 2.0, (g.y as f32 + 0.5) * 2.0);
        app.mouse_at(MouseIn::Up(Btn::Left), ((g.x + 3) as f32 + 0.5) * 2.0, (g.y as f32 + 0.5) * 2.0);
        assert!(app.settings.open);
        app.step();
        // and the panel on the windows' layer, within its grid
        let r = app.settings.rect.expect("the panel's drawn");
        assert!(r.x + r.w <= 200 && r.y + r.h <= 40, "{r:?}");
        // the same size all round: no layers at all
        app.cfg.ui = crate::config::UiCfg::default();
        app.regrid(300, 60, false);
        assert!(app.ui_layers().is_empty() && app.part_layer == [None; 3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A layer's cells are drawn whole pixels big, a little off its size
    /// (72 rows of 15 px, the windows at 1.3: 19 px, not 19.5): a click
    /// finds the row drawn under it, all the way down the screen.
    #[test]
    fn a_click_lands_on_the_row_drawn_under_it() {
        let dir = std::env::temp_dir().join(format!("xmas-app-px-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.ui.scale = 1.3;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 240, 72, true, true, false);
        app.set_ui_layer_ok(true);
        app.step();
        let li = app.part_layer[WINDOWS].expect("the windows on a layer of their own");
        let (lw, lh) = { let ls = app.ui_layers(); (ls[li].0.w, ls[li].0.h) };
        // the front-end's cells for it: 8 x 15 px under, whole pixels over
        let (cw, ch) = (8usize, 15usize);
        let (ucw, uch) = (240 * cw / lw, 72 * ch / lh);
        assert_eq!(uch, 19);
        app.layer_px = vec![(ucw as f32 / cw as f32, uch as f32 / ch as f32)];
        // row 20 as drawn (380 to 399 px down): row 20 all the way, not 19
        // at its top half
        for (px, row) in [(381.0, 20), (386.0, 20), (398.0, 20), (400.0, 21), (12.0, 0)] {
            app.mouse_at(MouseIn::Move, 100.0 / cw as f32, px / ch as f32);
            assert_eq!(app.settings.hover.1, row, "{px} px down");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Paused (⏸ or P), or resting for a game: the village stands still,
    /// drawn twice a second from the picture it stood at; carrying on, it
    /// moves again. The ⏸ button up top does it too.
    #[test]
    fn pausing_holds_the_village_still() {
        let dir = std::env::temp_dir().join(format!("xmas-app-pause-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.widgets.weather = false;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 240, 60, true, true, false);
        for _ in 0..10 {
            app.step();
        }
        let fast = app.frame_dt();
        let p = settings::pause_rect(app.w as i32);
        app.mouse(MouseIn::Down(Btn::Left), p.x + 1, p.y);
        app.mouse(MouseIn::Up(Btn::Left), p.x + 1, p.y);
        assert!(app.resting(), "the ⏸ button didn't pause");
        app.step();
        let t0 = app.scene.now();
        let px0 = app.frame.px.clone();
        for _ in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            app.step();
        }
        assert_eq!(app.scene.now(), t0, "the village moved while paused");
        assert!(app.frame.px == px0, "the picture changed while paused");
        assert!(app.frame_dt() > fast * 4, "still drawn as often");
        assert!(app.settings.resting.as_deref() == Some(""));
        // P carries on (the key as set)
        app.key(KeyIn::Char('p'));
        assert!(!app.resting());
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            app.step();
        }
        assert!(app.scene.now() > t0, "it didn't carry on");
        // the wallpaper resting (a game): the same, with the reason up top
        app.set_rest(Some("a game is running".into()));
        app.step();
        assert_eq!(app.settings.resting.as_deref(), Some("a game is running"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sat at a game (autopilot's chess with a neighbour, say), the
    /// buttons up top still answer.
    #[test]
    fn the_top_buttons_answer_at_a_table() {
        let dir = std::env::temp_dir().join(format!("xmas-app-table-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        app.step();
        app.scene.sit_at_test_table();
        assert!(app.scene.at_table());
        let g = settings::gear_rect(app.w as i32);
        app.mouse(MouseIn::Down(Btn::Left), g.x + 3, g.y);
        app.mouse(MouseIn::Up(Btn::Left), g.x + 3, g.y);
        assert!(app.settings.open, "the settings opened");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zoom_resolution_and_keys() {
        let dir = std::env::temp_dir().join(format!("xmas-app-zoom-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut cfg = Config::default();
        cfg.scene.events = 0.0;
        cfg.widgets.music = false;
        cfg.widgets.weather = false;
        cfg.widgets.system = false;
        cfg.widgets.map = false;
        cfg.clock.enabled = false;
        let mut app = App::new(cfg, 9, &dir.join("config.toml"), 300, 60, true, true, false);
        app.step();
        assert_eq!(app.world_pt(100, 30), (100, 30));
        app.ctrl_key('=', false);
        app.ctrl_key('=', false);
        assert!((app.cfg.zoom - 1.2).abs() < 1e-3, "{}", app.cfg.zoom);
        // (the view eases in to it, settling there)
        for _ in 0..60 {
            app.step();
        }
        let (z, x0, y0) = app.zoom_xf;
        assert!(z > 1.1 && z <= 1.2 + 1e-3, "{z}");
        let (wx, wy) = app.world_pt(150, 30);
        assert_eq!((wx, wy), ((x0 + 150.5 / z) as i32, (y0 + 30.5 / z) as i32));
        app.ctrl_key('0', false);
        assert_eq!(app.cfg.zoom, 1.0);
        // a touchpad's pinch: many tiny steps, adding up (none rounded away)
        for _ in 0..30 {
            app.mouse(MouseIn::Zoom(1.004), 150, 30);
        }
        assert!(app.cfg.zoom > 1.1 && app.cfg.zoom < 1.15, "{}", app.cfg.zoom);
        for _ in 0..30 {
            app.mouse(MouseIn::Zoom(1.0 / 1.004), 150, 30);
        }
        assert_eq!(app.cfg.zoom, 1.0);
        // resolution: 0.05 steps (as 1 / scale)
        app.ctrl_key('+', true);
        assert!((settings::resolution(app.cfg.scale) - 1.05).abs() < 1e-3, "{}", app.cfg.scale);
        settings::apply(&mut app.cfg, Act::Inc(F::Scale));
        assert!((settings::resolution(app.cfg.scale) - 1.10).abs() < 1e-3);
        // keys
        app.key(KeyIn::Esc);
        assert!(app.settings.open, "Esc didn't open the settings");
        app.key(KeyIn::Esc);
        assert!(!app.settings.open);
        app.key(KeyIn::Char('?'));
        assert!(app.settings.help);
        app.key(KeyIn::Esc);
        assert!(!app.settings.help && !app.settings.open, "Esc should close the card first");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Open a shelf item (or folder) off the render thread: launching can take
/// a moment.
fn open_detached(path: std::path::PathBuf) {
    // (tests note what would have opened instead of opening it)
    #[cfg(test)]
    {
        OPENED.lock().unwrap().push(path);
        return;
    }
    #[allow(unreachable_code)]
    std::thread::spawn(move || {
        #[cfg(windows)]
        crate::winshelf::com_init();
        crate::widgets::shelf::open(&path);
    });
}

#[cfg(test)]
static OPENED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

impl Drop for App {
    /// On the way out: the save, written.
    fn drop(&mut self) {
        if self.saves {
            self.write_save();
        }
        // off the relay politely ("gone", for friends)
        if let Some(l) = self.link.take() {
            self.scene.online.goodbye();
            l.close(self.scene.online.take_out());
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
}
