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
    /// Resizing the map by its corner grip.
    resizing: bool,
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
}

pub fn random_seed() -> u32 {
    let n = SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    crate::rng::hash((n & 0xffff_ffff) as u32 ^ (n >> 32) as u32) | 1
}

/// The render scale changed: in the kitty wallpaper, ask kitty (through the
/// control socket the launcher opened) for the matching font size. If that
/// doesn't work and the systemd service is running us, exit: the service
/// starts the wallpaper again, and the launcher reads the new scale.
fn apply_scale(scale: f32) {
    let Ok(sock) = std::env::var("XMAS_KITTY_SOCKET") else { return };
    let base: f32 = std::env::var("XMAS_FONT_BASE").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0);
    let kitten = std::env::var("XMAS_KITTEN").unwrap_or_else(|_| "kitten".into());
    let size = format!("{:.2}", base * scale.clamp(0.5, 2.0));
    std::thread::spawn(move || {
        let ok = std::process::Command::new(kitten).args(["@", "--to", &sock, "set-font-size", &size]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if !ok && std::env::var_os("INVOCATION_ID").is_some() {
            // give the config a moment to be saved, then restart
            std::thread::sleep(std::time::Duration::from_millis(600));
            RESTART_FOR_SCALE.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

/// Set when the wallpaper should restart to pick up a new render scale.
pub static RESTART_FOR_SCALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
            resizing: false,
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
                self.scene.set_cursor(Some((x, y)));
                self.scene.convo_hover(x, y);
                self.widgets.hover = if self.settings.open { None } else { self.widgets.hit(x, y) };
            }
            MouseIn::Down(Btn::Left) if self.menu.is_some() => {
                // a click picks from the menu (or, outside it, just closes it)
                let m = self.menu.take().unwrap();
                match m.hit(x, y).map(|i| m.items[i].1) {
                    Some(crate::ctxmenu::MenuAct::Settings) => self.settings.open = true,
                    Some(crate::ctxmenu::MenuAct::Scene(d)) => self.scene.act(d),
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
                        Some(a) => {
                            self.commit_city();
                            self.acts.push(a)
                        }
                        None => self.commit_city(),
                    }
                } else if self.settings.open {
                    self.commit_city();
                    self.settings.open = false;
                } else if self.scene.convo_click(x, y) {
                    // answered in the dialogue box
                } else if settings::gear_rect(self.w as i32).contains(x, y) {
                    self.settings.open = true;
                } else if !self.cfg.layout.locked && self.widgets.on_grip(x, y) {
                    self.resizing = true;
                    self.widgets.dragging = Some(4);
                } else if let (true, Some(wx)) = (self.cfg.layout.locked, self.widgets.map_world_x(x, y)) {
                    self.scene.look_at(wx);
                } else if let (false, Some(i)) = (self.cfg.layout.locked, self.widgets.hit(x, y)) {
                    let r = self.widgets.rects[i].unwrap();
                    self.drag = Some((i, x - r.x, y - r.y));
                    self.press_at = (x, y);
                    self.widgets.dragging = Some(i);
                } else {
                    self.press = Some((x, y, self.t()));
                    if !self.scene.grab(x, y) {
                        // nobody to pick up here: an ordinary click on the scene
                        self.scene.click(x, y, t);
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
                    self.scene.drag(x, y);
                    return;
                }
                self.scene.set_cursor(Some((x, y)));
                if self.resizing {
                    self.widgets.resize_map(x, y, self.w as i32, self.h as i32);
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
                self.scene.drag(x, y);
                self.scene.release();
            }
            MouseIn::Up(Btn::Left) if self.resizing => {
                self.resizing = false;
                self.widgets.resize_map(x, y, self.w as i32, self.h as i32);
                self.widgets.dragging = None;
                self.cfg.layout.map_size = self.widgets.cfg.layout.map_size;
                self.cfg.layout.map = self.widgets.cfg.layout.map;
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
                    self.cfg.layout.set(i, self.widgets.cfg.layout.get(i));
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
                KeyIn::Char('e' | 'E' | ' ') | KeyIn::Enter => self.scene.interact(),
                KeyIn::Char('f' | 'F') => self.scene.invite(),
                KeyIn::Char('r' | 'R') => self.scene.snowball(),
                KeyIn::Char('i' | 'I') => self.scene.toggle_pockets(),
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
        } else if self.settings.open && k == KeyIn::Esc {
            self.settings.open = false;
        } else if k == KeyIn::Esc && self.menu.take().is_some() {
        } else if k == KeyIn::Esc && self.scene.close_panels() {
        } else if k == KeyIn::Esc && self.scene.is_inside() {
            self.scene.leave();
        } else if !self.wallpaper && (k == KeyIn::Char('q') || k == KeyIn::Esc) {
            self.quit = true;
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
        } else if settings::gear_rect(self.w as i32).contains(hx, hy) {
            "pointer"
        } else if !self.settings.open && !self.cfg.layout.locked && self.widgets.hover.is_some() {
            "grab"
        } else if self.scene.convo_dragging() {
            "grabbing"
        } else if !self.settings.open && self.scene.convo_grip(hx, hy) {
            "grab"
        } else if !self.settings.open && self.scene.grabbable(hx, hy) {
            "grab"
        } else if !self.settings.open && self.scene.clickable(hx, hy) {
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
        self.commit_city();
        let (title, items) = self.scene.menu_at(x, y);
        self.menu = Some(crate::ctxmenu::CtxMenu::new(x, y, title, items, self.w as i32, self.h as i32));
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
                Act::EditCity => {
                    if self.settings.editing_pet {
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
                Act::FullScreen => self.pop_out(true),
                Act::Tab(i) => self.settings.tab = i as usize,
                Act::CheckUpdate => crate::update::check_now(),
                Act::InstallUpdate => crate::update::install_now(),
                Act::DismissUpdate => crate::update::dismiss(),
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
                c.traffic.clear();
                c.time.clear();
                c.talk_time = 0.0;
                c.chattiness = 0.0;
                c.pet = Default::default();
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
                self.widgets.apply(&c);
            } else {
                self.widgets.apply(&self.cfg);
            }
            if (self.cfg.scale - old.scale).abs() > 1e-3 {
                apply_scale(self.cfg.scale);
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
        self.scene.set_live(self.cfg.scene.walk_speed, left);
        crate::scene::events::set_talk(self.cfg.scene.talk_time, self.cfg.scene.chattiness);
        self.scene.set_pet(&self.cfg.scene.pet);
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
        self.scene.draw(&mut self.frame, t);
        self.widgets.map = Some(self.scene.minimap());
        self.widgets.draw(&mut self.frame, t);
        self.settings.draw_gear(&mut self.frame, t);
        self.settings.meters = self.audio.as_ref().filter(|_| self.settings.open).and_then(|a| a.meters.lock().ok().map(|m| *m));
        // a popped-out window tells the wallpaper it's open
        if self.popout && t - self.popout_touched > 1.0 {
            self.popout_touched = t;
            let _ = std::fs::write(crate::platform::popout_path(), b"1");
        }
        // the tray's settings window (another process) shows the levels too
        if self.audio.is_some() {
            if t - self.meters_check > 1.0 {
                self.meters_check = t;
                self.meters_wanted = crate::platform::fresh(&crate::platform::meters_want_path(), 3.0);
            }
            if self.meters_wanted && t - self.meters_sent > 0.08 {
                self.meters_sent = t;
                if let Some(m) = self.audio.as_ref().and_then(|a| a.meters.lock().ok().map(|m| *m)) {
                    crate::platform::write_meters(&m);
                }
            }
        }
        self.settings.draw_banner(&mut self.frame, t);
        self.settings.draw(&mut self.frame, &self.cfg, t);
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
}
