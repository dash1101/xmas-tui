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
    pub w: usize,
    pub h: usize,
    /// Running as a wallpaper: keys never quit.
    wallpaper: bool,
    /// Whether typed keys can reach us (false behind the Windows desktop icons).
    keyboard: bool,
    pub quit: bool,
    /// Direction keys (left, right, up, down): held until this app time.
    held: [f32; 4],
    /// The front-end reports key releases (otherwise a press holds a key
    /// for a moment and auto-repeat keeps it held).
    exact_keys: bool,
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
            wallpaper,
            keyboard,
            quit: false,
            resizing: None,
            bar_drag: None,
            picking: None,
            shelf_press: None,
            last_click: None,
            px_map: None,
            desktop_menu: false,
            menu: None,
            press: None,
            press_at: (0, 0),
            held: [f32::NEG_INFINITY; 4],
            exact_keys: false,
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
            spare: Frame::new(1, 1),
            wall_full: false,
            wall_full_req: None,
            shift: false,
        }
    }

    /// Screen cell → the cell of the drawing it shows (they differ while
    /// zoomed in, or indoors on a tall screen).
    fn world_pt(&self, x: i32, y: i32) -> (i32, i32) {
        let (z, x0, y0) = self.zoom_xf;
        if z <= 1.001 {
            return (x, y);
        }
        ((x0 + (x as f32 + 0.5) / z).floor() as i32, (y0 + (y as f32 + 0.5) / z).floor() as i32)
    }

    /// Whether typed keys reach us (the Windows wallpaper gets them only
    /// while it's full screen).
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
        let (up, down) = (matches!(c, '=' | '+'), matches!(c, '-' | '_'));
        if shift && (up || down) {
            settings::apply(&mut self.cfg, if up { Act::Inc(F::Scale) } else { Act::Dec(F::Scale) });
            self.settings.toast = Some((format!("resolution {:.2}× detail", settings::resolution(self.cfg.scale)), t + 1.4));
        } else if up || down {
            let z = ((self.cfg.zoom + if up { 0.1 } else { -0.1 }) * 20.0).round() / 20.0;
            self.cfg.zoom = z.clamp(1.0, 3.0);
            self.settings.toast = Some((format!("zoom {:.2}×", self.cfg.zoom), t + 1.4));
        } else if c == '0' {
            self.cfg.zoom = 1.0;
            self.settings.toast = Some(("zoom 1.00×".into(), t + 1.4));
        }
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
        // clean_fps: NaN.clamp() is NaN, and from_secs_f32(NaN) panics
        Duration::from_secs_f32(1.0 / config::clean_fps(self.fps_override.unwrap_or(self.cfg.fps)))
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if (w, h) == (self.w, self.h) || w == 0 || h == 0 {
            return;
        }
        self.w = w;
        self.h = h;
        self.frame.resize(w, h);
        let pockets = self.scene.take_pockets();
        self.scene = Scene::new(w, h, self.seed, &self.cfg.scene);
        self.scene.set_pockets(pockets);
    }

    fn commit_city(&mut self) {
        if let Some(i) = self.settings.editing_shelf.take() {
            if let Some(text) = self.settings.editing.take() {
                crate::widgets::shelf::rename(&mut self.cfg, i, &text);
            }
            return;
        }
        if self.settings.editing_name {
            self.settings.editing_name = false;
            if let Some(text) = self.settings.editing.take() {
                self.cfg.scene.name = text.trim().chars().take(16).collect();
            }
            return;
        }
        if self.settings.editing_pet {
            self.settings.editing_pet = false;
            if let Some(text) = self.settings.editing.take() {
                let name: String = text.trim().chars().take(16).collect();
                if !name.is_empty() {
                    self.cfg.scene.pet.name = name;
                }
            }
            return;
        }
        if let Some(text) = self.settings.editing.take() {
            let city = text.trim().to_string();
            if city != self.cfg.weather.city {
                self.cfg.weather.city = city;
                self.cfg.weather.latitude = None;
                self.cfg.weather.longitude = None;
            }
        }
    }

    pub fn mouse(&mut self, ev: MouseIn, x: i32, y: i32) {
        let t = self.t();
        self.settings.hover = (x, y);
        match ev {
            MouseIn::Move => {
                if let Some(m) = &mut self.menu {
                    m.hover = m.hit(x, y);
                }
                let (wx, wy) = self.world_pt(x, y);
                self.scene.set_cursor(Some((wx, wy)));
                self.scene.convo_hover(x, y);
                self.widgets.hover = if self.settings.open { None } else { self.widgets.hit(x, y) };
                if !self.settings.open {
                    self.widgets.shelf_hover(x, y);
                }
            }
            MouseIn::Down(Btn::Left) if self.menu.is_some() => {
                // a click picks from the menu (or, outside it, just closes it)
                let m = self.menu.take().unwrap();
                match m.hit(x, y).map(|i| m.items[i].1) {
                    Some(crate::ctxmenu::MenuAct::Settings) => self.settings.open = true,
                    Some(crate::ctxmenu::MenuAct::Scene(d)) => self.scene.act(d),
                    Some(crate::ctxmenu::MenuAct::Shelf(a)) => self.shelf_act(a),
                    Some(crate::ctxmenu::MenuAct::Desktop) => self.desktop_menu = true,
                    None => {}
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
                        Some(Act::EditCity) => self.acts.push(Act::EditCity),
                        // clicking back into the shelf name being typed keeps it going
                        Some(Act::ShelfName(i)) if self.settings.editing_shelf == Some(i as usize) && self.settings.editing.is_some() => {}
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
                    // any click closes the controls card
                    self.settings.help = false;
                } else if settings::help_rect(self.w as i32).contains(x, y) {
                    self.settings.help = true;
                } else if self.scene.convo_click(x, y) {
                    // answered in the dialogue box
                } else if settings::gear_rect(self.w as i32).contains(x, y) {
                    self.settings.open = true;
                } else if let Some(i) = self.widgets.on_grip(x, y).filter(|_| !self.cfg.layout.locked) {
                    self.resizing = Some((i, crate::widgets::shelf::EDGE_R | crate::widgets::shelf::EDGE_B));
                    self.widgets.dragging = Some(i);
                } else if let Some((s, hit)) = self.widgets.shelf_hit(x, y) {
                    self.shelf_down(s, hit, x, y);
                } else if let (true, Some(wx)) = (self.cfg.layout.locked, self.widgets.map_world_x(x, y)) {
                    self.scene.look_at(wx);
                } else if let (false, Some(i)) = (self.cfg.layout.locked, self.widgets.hit(x, y)) {
                    let r = self.widgets.rects[i].unwrap();
                    self.drag = Some((i, x - r.x, y - r.y));
                    self.press_at = (x, y);
                    self.widgets.dragging = Some(i);
                } else {
                    self.press = Some((x, y, self.t()));
                    let (wx, wy) = self.world_pt(x, y);
                    if !self.scene.grab(wx, wy) {
                        // nobody to pick up here: an ordinary click on the scene
                        self.scene.click(wx, wy, t);
                    }
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
                if self.scene.convo_dragging() {
                    self.scene.convo_drag_to(x, y);
                    return;
                }
                if self.scene.holding() {
                    let (wx, wy) = self.world_pt(x, y);
                    self.scene.drag(wx, wy);
                    return;
                }
                if let Some(ShelfPress::Item(s, k)) = self.shelf_press {
                    // an item pulled off its slot: carried to wherever it's let go
                    if self.widgets.lift.is_some() || (x - self.press_at.0).abs() > 1 || (y - self.press_at.1).abs() > 1 {
                        self.widgets.lift = Some((s, k));
                        self.widgets.lift_at = (x, y);
                    }
                    return;
                }
                let (wx, wy) = self.world_pt(x, y);
                self.scene.set_cursor(Some((wx, wy)));
                if let Some(s) = self.bar_drag {
                    self.shelf_bar(s, y);
                    return;
                }
                if let Some((i, e)) = self.resizing {
                    self.widgets.resize(i, e, x, y, self.w as i32, self.h as i32);
                    return;
                }
                if let Some((i, ox, oy)) = self.drag {
                    self.widgets.move_to(i, x - ox, y - oy, self.w as i32, self.h as i32);
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
                            self.shelf_drop(lift, x, y);
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
                        if (x, y) == self.press_at {
                            // a click on the title rolls it up (or down again)
                            if let Some(c) = self.cfg.shelves.get_mut(s) {
                                c.rolled = !c.rolled;
                            }
                            if self.drag.take().is_some() {
                                self.widgets.dragging = None;
                            }
                        } else if let Some((i, ox, oy)) = self.drag.take() {
                            self.widgets.move_to(i, x - ox, y - oy, self.w as i32, self.h as i32);
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
            MouseIn::Up(Btn::Left) if self.scene.convo_dragging() => {
                self.scene.convo_drag_to(x, y);
                self.scene.convo_drop();
            }
            MouseIn::Up(Btn::Left) if self.scene.holding() => {
                let (wx, wy) = self.world_pt(x, y);
                self.scene.drag(wx, wy);
                self.scene.release();
            }
            MouseIn::Up(Btn::Left) if self.bar_drag.is_some() => {
                self.bar_drag = None;
            }
            MouseIn::Up(Btn::Left) if self.resizing.is_some() => {
                let (i, e) = self.resizing.take().unwrap();
                self.widgets.resize(i, e, x, y, self.w as i32, self.h as i32);
                self.widgets.dragging = None;
                if i == 4 {
                    self.cfg.layout.map_size = self.widgets.cfg.layout.map_size;
                    self.cfg.layout.map = self.widgets.cfg.layout.map;
                } else if let (Some(c), Some(w)) = (self.cfg.shelves.get_mut(i - crate::widgets::BUILTIN), self.widgets.cfg.shelves.get(i - crate::widgets::BUILTIN)) {
                    (c.size, c.pos) = (w.size, w.pos);
                }
            }
            MouseIn::Up(Btn::Left) => {
                if let Some((4, ..)) = self.drag.filter(|_| (x, y) == self.press_at) {
                    // a click on the map (no drag): look over there
                    self.drag = None;
                    self.widgets.dragging = None;
                    if let Some(wx) = self.widgets.map_world_x(x, y) {
                        self.scene.look_at(wx);
                    }
                    return;
                }
                if let Some((i, ox, oy)) = self.drag.take() {
                    // drop where the button came up (there may have been no
                    // Drag event at this spot), and commit only the dragged
                    // widget's slot: the others may have changed in the file
                    // meanwhile and must not be reverted
                    self.widgets.move_to(i, x - ox, y - oy, self.w as i32, self.h as i32);
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
                    if let Some(a) = self.settings.scroll(x, y, up) {
                        self.acts.push(a);
                    }
                } else if let Some((s, _)) = self.widgets.shelf_hit(x, y) {
                    self.widgets.shelf_scroll(s, if up { -1 } else { 1 });
                } else {
                    // the wheel looks around the village
                    let step = (self.w as f32 * 0.06).max(4.0);
                    self.scene.pan(if up { -step } else { step });
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
        self.hold(k);
    }

    /// A key was let go (front-ends that can tell).
    pub fn key_up(&mut self, k: KeyIn) {
        self.set_exact_keys();
        if let Some(i) = Self::dir_slot(k) {
            self.held[i] = f32::NEG_INFINITY;
        }
    }

    /// Held direction keys → (x, y) in -1..=1.
    fn move_dir(&self) -> (f32, f32) {
        let t = self.t();
        let on = |i: usize| (self.held[i] > t) as i32 as f32;
        (on(1) - on(0), on(3) - on(2))
    }

    pub fn key(&mut self, k: KeyIn) {
        self.hold(k);
        if self.settings.editing.is_none() && !self.settings.open && self.scene.has_player() {
            match k {
                KeyIn::Char('e' | 'E') | KeyIn::Enter => self.scene.interact(),
                KeyIn::Char(' ') => self.scene.jump(),
                KeyIn::Char('f' | 'F') => self.scene.invite(),
                KeyIn::Char('r' | 'R') => self.scene.snowball(),
                KeyIn::Char('i' | 'I') => self.scene.toggle_pockets(),
                KeyIn::Char('q' | 'Q') => self.scene.cycle_held(),
                KeyIn::Char(c @ '1'..='5') => self.scene.number(c as usize - '0' as usize),
                _ => {}
            }
        }
        if let Some(buf) = self.settings.editing.as_mut() {
            match k {
                KeyIn::Char(c) if buf.chars().count() < 28 => buf.push(c),
                KeyIn::Backspace => {
                    buf.pop();
                }
                KeyIn::Enter => self.commit_city(),
                KeyIn::Esc => self.settings.editing = None,
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
                Some(Act::EditCity) if self.keyboard => "text",
                Some(_) => "pointer",
                None => "default",
            }
        } else if settings::gear_rect(self.w as i32).contains(hx, hy) || settings::help_rect(self.w as i32).contains(hx, hy) {
            "pointer"
        } else if !self.settings.open && !self.cfg.layout.locked && self.widgets.hover.is_some() {
            "grab"
        } else if self.scene.convo_dragging() {
            "grabbing"
        } else if !self.settings.open && self.scene.convo_grip(hx, hy) {
            "grab"
        } else if !self.settings.open && self.scene.grabbable(self.world_pt(hx, hy).0, self.world_pt(hx, hy).1) {
            "grab"
        } else if !self.settings.open && self.scene.clickable(self.world_pt(hx, hy).0, self.world_pt(hx, hy).1) {
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

    /// Open the right-click menu at (x, y).
    fn open_menu(&mut self, x: i32, y: i32) {
        use crate::ctxmenu::{CtxMenu, MenuAct};
        use crate::widgets::shelf::{Hit, MenuAct as S};
        self.commit_city();
        let (w, h) = (self.w as i32, self.h as i32);
        if let Some((s, hit)) = self.widgets.shelf_hit(x, y) {
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
            if cfg!(windows) && !self.keyboard {
                items.push(("Windows desktop menu…".into(), MenuAct::Desktop));
            }
            self.menu = Some(CtxMenu::with(x, y, title, items, w, h));
            return;
        }
        let (wx, wy) = self.world_pt(x, y);
        let (title, items) = self.scene.menu_at(wx, wy);
        let mut items: Vec<(String, MenuAct)> = items.into_iter().map(|(l, d)| (l, MenuAct::Scene(d))).collect();
        if self.widgets.hit(x, y).is_none() {
            items.push(("new shelf here".into(), MenuAct::Shelf(S::New(x, y))));
            items.push(("new shelf from a folder…".into(), MenuAct::Shelf(S::NewFrom(x, y))));
        }
        if cfg!(windows) && !self.keyboard {
            items.push(("Windows desktop menu…".into(), MenuAct::Desktop));
        }
        self.menu = Some(CtxMenu::with(x, y, title, items, w, h));
    }

    /// Windows desktop: a right-click the hook took from the desktop. Opens
    /// our menu if something of ours is there (someone, a house, a widget,
    /// a shelf) and returns true; false lets Windows' desktop menu show.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn right_click(&mut self, x: i32, y: i32) -> bool {
        if self.settings.open {
            self.commit_city();
            self.settings.open = false;
            return true;
        }
        if self.menu.take().is_some() {
            return true;
        }
        // (the clock is mostly see-through: right-clicks there are the desktop's)
        let ours = self.widgets.hit(x, y).is_some_and(|i| i != 0) || settings::gear_rect(self.w as i32).contains(x, y) || {
            let (wx, wy) = self.world_pt(x, y);
            let (title, items) = self.scene.menu_at(wx, wy);
            !matches!(title.as_str(), "here" | "the sky" | "the high sky" | "the village") || items.iter().any(|(_, d)| matches!(d, crate::scene::Do::DropOn(..)))
        };
        if ours {
            self.open_menu(x, y);
        }
        ours
    }

    /// Windows desktop: a desktop icon is being dragged (at this cell), or
    /// not any more. A shelf under it lights up.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn icon_drag(&mut self, at: Option<(i32, i32)>) {
        self.widgets.ext_drag = at;
    }

    /// Windows desktop: desktop icons were let go at (x, y): onto a shelf,
    /// they go in its folder.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn desktop_drop(&mut self, x: i32, y: i32) {
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
                let place = self.px_map.map(|(ox, oy, cw, ch)| (ox + x * cw - 36, oy + y * ch - 24));
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
        let old = self.applied.clone();
        let mut reseed = false;
        for a in std::mem::take(&mut self.acts) {
            match a {
                Act::Close => {
                    self.commit_city();
                    self.settings.open = false;
                }
                Act::EditPetName => {
                    if self.keyboard {
                        if !self.settings.editing_pet || self.settings.editing.is_none() {
                            self.commit_city();
                            self.settings.editing_pet = true;
                            self.settings.editing = Some(self.cfg.scene.pet.name.clone());
                        }
                    } else {
                        if !self.sync.path().exists() {
                            self.sync.save_now(&self.cfg);
                        }
                        platform::open_in_editor(self.sync.path());
                    }
                }
                Act::ShelfName(i) => {
                    if self.keyboard {
                        if self.settings.editing_shelf != Some(i as usize) || self.settings.editing.is_none() {
                            self.commit_city();
                            self.settings.editing_shelf = Some(i as usize);
                            self.settings.editing = self.cfg.shelves.get(i as usize).map(|c| c.name.clone());
                        }
                    } else {
                        if !self.sync.path().exists() {
                            self.sync.save_now(&self.cfg);
                        }
                        platform::open_in_editor(self.sync.path());
                    }
                }
                Act::ShelfOpen(i) => {
                    if let Some(c) = self.cfg.shelves.get(i as usize) {
                        open_detached(crate::widgets::shelf::folder_of(c));
                    }
                }
                Act::ShelfFolder(i) => self.pick_folder(Pick::Change(i as usize)),
                Act::ShelfNewFrom => self.pick_folder(Pick::New(self.w as i32 / 3, self.h as i32 / 3)),
                Act::EditName => {
                    if self.keyboard {
                        if !self.settings.editing_name || self.settings.editing.is_none() {
                            self.commit_city();
                            self.settings.editing_name = true;
                            self.settings.editing = Some(self.cfg.scene.name.clone());
                        }
                    } else {
                        if !self.sync.path().exists() {
                            self.sync.save_now(&self.cfg);
                        }
                        platform::open_in_editor(self.sync.path());
                    }
                }
                Act::EditCity => {
                    if self.settings.editing_pet || self.settings.editing_name || self.settings.editing_shelf.is_some() {
                        self.commit_city();
                    }
                    if self.keyboard {
                        // clicking back into the field while typing keeps the text
                        if self.settings.editing.is_none() {
                            self.settings.editing = Some(self.cfg.weather.city.clone());
                        }
                    } else {
                        // no keyboard behind the desktop icons: edit the file instead
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
                Act::Quit => self.quit = true,
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
                Act::Tab(i) => self.settings.tab = i as usize,
                Act::CheckUpdate => crate::update::check_now(),
                Act::InstallUpdate => crate::update::install_now(),
                Act::DismissUpdate => crate::update::dismiss(),
                Act::SetWallpaper => {
                    // show how it's going (in the Desktop tab)
                    self.settings.wall_offer = false;
                    self.settings.open = true;
                    self.settings.tab = 6;
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
                    settings::apply(&mut self.cfg, a);
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
        self.frame.glow_near = self.cfg.scene.glow;
        self.frame.glow_far = self.cfg.scene.glow;
        self.settings.file_error = self.sync.error.is_some();
        self.settings.fps_override = self.fps_override;
        // `applied` always advances (also mid-drag), so a change is acted on
        // once instead of rebuilding the scene every frame until the drop
        if reseed || !config::same(&self.cfg, &old) {
            // walking speed and the traffic side apply live; anything else
            // about the scene rebuilds it
            let live = |c: &crate::config::SceneCfg| {
                let mut c = c.clone();
                c.walk_speed = 0.0;
                c.most_at_once = 0;
                c.traffic.clear();
                c.time.clear();
                c.talk_time = 0.0;
                c.chattiness = 0.0;
                c.pet = Default::default();
                c.name.clear();
                c
            };
            if reseed || live(&self.cfg.scene) != live(&old.scene) {
                let pockets = self.scene.take_pockets();
                self.scene = Scene::new(self.w, self.h, self.seed, &self.cfg.scene);
                self.scene.set_pockets(pockets);
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
        let left = match self.cfg.scene.traffic.as_str() {
            "left" => true,
            "right" => false,
            _ => crate::widgets::data::drives_left(&crate::widgets::data::country()),
        };
        self.scene.set_live(self.cfg.scene.walk_speed, left, self.cfg.scene.most_at_once);
        crate::scene::events::set_talk(self.cfg.scene.talk_time, self.cfg.scene.chattiness);
        self.scene.set_pet(&self.cfg.scene.pet);
        self.scene.set_name(&self.cfg.scene.name);
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
            self.scene.set_daylight(crate::scene::daylight::compute(&self.cfg.scene.time, mins, rise, set));
        }
        self.scene.update(dt, t);
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
            if self.media_quiet || self.hushed {
                want.enabled = false;
            }
            if want != self.sound_sent && a.config(&want) {
                self.sound_sent = want;
            }
        }
        self.frame.clear();
        // the zoomed view eases after the villager
        let want = self.scene.zoom_focus();
        let f = match self.zoom_focus {
            Some((fx, fy)) if (fx - want.0).abs() < self.w as f32 * 0.5 => {
                let k = 1.0 - (-dt * 5.0).exp();
                (fx + (want.0 - fx) * k, fy + (want.1 - fy) * k)
            }
            _ => want,
        };
        self.zoom_focus = Some(f);
        self.zoom_xf = self.scene.render(&mut self.frame, &mut self.spare, t, self.cfg.zoom, Some(f));
        self.widgets.map = Some(self.scene.minimap());
        self.widgets.draw(&mut self.frame, t);
        // the widgets' tops are something to climb up onto (next frame)
        self.scene.set_widget_ledges(self.widgets.rects.iter().flatten().map(|r| (r.x, r.y, r.w, r.h)).collect());
        self.settings.draw_gear(&mut self.frame, t);
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
        self.settings.draw(&mut self.frame, &self.cfg, t);
        self.settings.draw_help(&mut self.frame);
        if let Some(m) = &self.menu {
            m.draw(&mut self.frame);
        }
        self.frame.compose();
        &self.frame.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        app.step();
        let (z, x0, y0) = app.zoom_xf;
        assert!((z - 1.2).abs() < 1e-3);
        let (wx, wy) = app.world_pt(150, 30);
        assert_eq!((wx, wy), ((x0 + 150.5 / z) as i32, (y0 + 30.5 / z) as i32));
        app.ctrl_key('0', false);
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
