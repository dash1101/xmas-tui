//! `--settings-window`: a small native window (winit + softbuffer + the
//! same cell raster as `window.rs`) showing only the settings panel on a
//! festive backdrop — night sky, a garland of glowing bulbs, drifting snow.
//! Opened from the tray icon. It edits the same config file; the running
//! wallpaper picks the changes up through its live reload (`App::step`).
//!
//! The panel logic is a trimmed `App`: no scene, no widgets (so no data
//! threads, no sound), just `Settings` + config load/apply/save.

use crate::app::{random_seed, Btn, KeyIn, MouseIn};
use crate::color::{pal, Rgb};
use crate::config::{self, Config};
use crate::frame::{Bg, Cell, Frame, Glyph};
use crate::platform;
use crate::raster::Raster;
use crate::rng::{h01, hash, Rng};
use crate::settings::{self, Act, Settings};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Icon, Window, WindowId};

pub const WIN_TITLE: &str = "Christmas wallpaper settings";
/// Backdrop margin around the panel, in cells (sides, top/bottom).
const MARGIN: (usize, usize) = (3, 2);

struct Flake {
    x: f32,
    y: f32,
    vy: f32,
    phase: f32,
    ch: char,
    bright: f32,
}

/// The panel "app": input → settings → config file.
pub struct Panel {
    cfg: Config,
    /// Live reload + saving of the config file (never over a broken file).
    sync: config::FileSync,
    settings: Settings,
    frame: Frame,
    flakes: Vec<Flake>,
    rng: Rng,
    acts: Vec<Act>,
    /// Asking the running wallpaper for its sound levels: when last asked,
    /// and when last read.
    meters_asked: f32,
    meters_read: f32,
    pub w: usize,
    pub h: usize,
    /// Set when the window should close.
    pub close: bool,
}

/// Can this desktop show window snow/lights (so the toggles mean something)?
fn window_fx_supported() -> bool {
    if cfg!(windows) {
        return true;
    }
    let kde = std::env::var("XDG_CURRENT_DESKTOP").map(|d| d.contains("KDE")).unwrap_or(false);
    kde && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

impl Panel {
    pub fn new(cfg: Config, path: &Path) -> Self {
        let mut settings = Settings::default();
        settings.open = true;
        settings.extras = true;
        settings.standalone = true;
        settings.keyboard = true;
        settings.autostart = platform::autostart_get();
        settings.window_fx = window_fx_supported();
        let mut p = Panel {
            sync: config::FileSync::new(path, &cfg, Duration::from_millis(500)),
            cfg,
            settings,
            frame: Frame::new(1, 1),
            flakes: vec![],
            rng: Rng::new(random_seed() as u64),
            acts: vec![],
            meters_asked: -10.0,
            meters_read: -10.0,
            w: 1,
            h: 1,
            close: false,
        };
        let (w, h) = p.grid();
        p.resize(w, h);
        p
    }

    /// Cells needed to show the whole panel plus the backdrop margin.
    /// (Measured by drawing it once, so a taller panel just works.)
    pub fn grid(&mut self) -> (usize, usize) {
        let mut probe = Frame::new(200, 120);
        self.settings.draw(&mut probe, &self.cfg, 0.0);
        let r = self.settings.rect.unwrap_or(crate::widgets::Rect { x: 0, y: 0, w: 66, h: 32 });
        (r.w as usize + 2 * MARGIN.0, r.h as usize + 2 * MARGIN.1)
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == (self.w, self.h) {
            return;
        }
        self.w = w;
        self.h = h;
        self.frame.resize(w, h);
        let n = (w * h / 28).max(12);
        self.flakes = (0..n)
            .map(|_| {
                let r = self.rng.f();
                Flake {
                    x: self.rng.range(0.0, w as f32),
                    y: self.rng.range(0.0, h as f32),
                    vy: self.rng.range(0.8, 2.6),
                    phase: self.rng.range(0.0, 6.28),
                    ch: if r < 0.12 { '❄' } else if r < 0.30 { '*' } else if r < 0.55 { '•' } else { '·' },
                    bright: self.rng.range(0.45, 1.0),
                }
            })
            .collect();
    }

    fn commit_city(&mut self) {
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
        self.settings.hover = (x, y);
        match ev {
            MouseIn::Down(Btn::Left) => match self.settings.click(x, y) {
                Some(Act::EditCity) => self.acts.push(Act::EditCity),
                Some(a) => {
                    self.commit_city();
                    self.acts.push(a);
                }
                None => self.commit_city(),
            },
            MouseIn::Scroll(up) => {
                if let Some(a) = self.settings.scroll(x, y, up) {
                    self.acts.push(a);
                }
            }
            _ => {}
        }
    }

    pub fn key(&mut self, k: KeyIn) {
        if let Some(buf) = self.settings.editing.as_mut() {
            match k {
                KeyIn::Char(c) if !c.is_control() && buf.chars().count() < 28 => buf.push(c),
                KeyIn::Backspace => {
                    buf.pop();
                }
                KeyIn::Enter => self.commit_city(),
                KeyIn::Esc => self.settings.editing = None,
                _ => {}
            }
        } else if k == KeyIn::Esc {
            self.close = true;
        }
    }

    pub fn pointer(&self) -> &'static str {
        let (hx, hy) = self.settings.hover;
        match self.settings.click(hx, hy) {
            Some(Act::EditCity) => "text",
            Some(_) => "pointer",
            None => "default",
        }
    }

    /// Apply queued actions, sync with the config file, draw a frame.
    pub fn step(&mut self, t: f32, dt: f32) -> &[Cell] {
        // the Sound tab's live meters come from the running wallpaper: keep
        // telling it they're wanted, and read what it shares
        if self.settings.tab == 2 {
            if t - self.meters_asked > 1.0 {
                self.meters_asked = t;
                let _ = std::fs::write(crate::platform::meters_want_path(), b"1");
            }
            if t - self.meters_read > 0.05 {
                self.meters_read = t;
                self.settings.meters = crate::platform::read_meters::<{ crate::audio::METERS }>();
            }
        } else {
            self.settings.meters = None;
        }
        for a in std::mem::take(&mut self.acts) {
            match a {
                Act::Close => {
                    self.commit_city();
                    self.close = true;
                }
                Act::EditCity => {
                    // clicking back into the field while typing keeps the text
                    if self.settings.editing.is_none() {
                        self.settings.editing = Some(self.cfg.weather.city.clone());
                    }
                }
                Act::Shuffle => self.cfg.seed = random_seed() as u64,
                Act::Quit => {
                    crate::tray::request_wallpaper_quit();
                    self.close = true;
                }
                Act::Tab(i) => self.settings.tab = i as usize,
                // the wallpaper process runs the updater; this window only
                // has the toggles
                Act::CheckUpdate | Act::InstallUpdate | Act::DismissUpdate => {}
                Act::Autostart => {
                    let on = !self.settings.autostart.unwrap_or(false);
                    platform::autostart_set(on);
                    self.settings.autostart = platform::autostart_get();
                }
                a => {
                    settings::apply(&mut self.cfg, a);
                }
            }
        }
        // pick up changes from the wallpaper (or the tray, or a text
        // editor) and save ours — never over a file that doesn't parse
        self.sync.poll(&mut self.cfg, true);
        self.settings.file_error = self.sync.error.is_some();
        self.frame.clear();
        self.backdrop(t, dt);
        self.settings.draw(&mut self.frame, &self.cfg, t);
        self.retip();
        self.frame.compose();
        &self.frame.out
    }

    /// The panel's footer tips are about the wallpaper (drag widgets,
    /// right-click to open…). Here, say what applies to this window instead.
    /// (Done on the drawn frame so the shared panel code stays untouched;
    /// if the tips change upstream this simply does nothing.)
    fn retip(&mut self) {
        const BG: Bg = Bg::Mix(pal::UI_PANEL, 0.90); // the panel's own background
        let Some(r) = self.settings.rect else { return };
        let f = &mut self.frame;
        let starts = |f: &Frame, y: i32, s: &str| s.chars().enumerate().all(|(i, c)| f.get_g(r.x + 3 + i as i32, y).map(|g| g.ch) == Some(c));
        let Some(y) = (r.y..r.y + r.h).find(|&y| starts(f, y, "drag any widget")) else { return };
        let tips = ["scroll on a value to adjust  ·  the wallpaper updates live", "Esc or ✕ closes this window"];
        for (k, tip) in tips.iter().enumerate() {
            let yy = y + k as i32;
            for x in r.x + 1..r.x + r.w - 1 {
                f.put_g(x, yy, Glyph::new(' ', pal::UI_DIM).bg(BG).lit(0.0));
            }
            // the old tip could spill over the right border: restore it
            f.put_g(r.x + r.w - 1, yy, Glyph::new('│', pal::UI_BORDER).bg(BG).lit(0.0));
            for x in r.x + r.w..f.w as i32 {
                if f.cell_in(x, yy) {
                    f.glyph[yy as usize * f.w + x as usize] = None;
                }
            }
            f.text(r.x + 3, yy, tip, pal::UI_DIM, BG, 0.0);
        }
    }

    /// Night sky, twinkling stars, a bulb garland along the top, a snowy
    /// floor, and snowflakes drifting down behind the panel.
    fn backdrop(&mut self, t: f32, dt: f32) {
        let f = &mut self.frame;
        let (pw, ph) = (f.pw as i32, f.ph as i32);
        for y in 0..ph {
            let v = y as f32 / ph.max(1) as f32;
            let c = if v < 0.55 { pal::SKY_TOP.lerp(pal::SKY_MID, v / 0.55) } else { pal::SKY_MID.lerp(pal::SKY_LOW, (v - 0.55) / 0.45) };
            for x in 0..pw {
                f.px_set(x, y, c);
            }
        }
        // twinkling stars (glyphs: pixel stars would be chunky half-blocks)
        for i in 0..(f.w * f.h / 45).max(6) {
            let h = hash(i as u32 * 7919 + 17);
            let (x, y) = ((h % f.w as u32) as i32, 1 + ((h >> 11) % (f.h as u32 * 2 / 3).max(1)) as i32);
            let tw = 0.5 + 0.5 * (t * (0.8 + h01(h >> 3) * 1.6) + h01(h >> 7) * 6.28).sin();
            let ch = if h01(h >> 5) < 0.2 { '✦' } else { '·' };
            f.put_g(x, y, Glyph::new(ch, Rgb::hex(0xcfd8ff) * (0.25 + 0.5 * tw)).lit(0.0));
        }
        // snowy floor (exactly the last text row) with a warm glow, as if
        // from a fireplace off-screen
        for y in (ph - 2).max(0)..ph {
            for x in 0..pw {
                let shade = 0.75 + 0.25 * h01(hash(x as u32 * 31 + y as u32));
                f.px_set(x, y, pal::SNOW_SHADOW.lerp(pal::SNOW, shade * 0.6));
            }
        }
        f.light_px(pw as f32 * 0.5, ph as f32 - 1.0, pal::WINDOW * 0.8);
        // garland: a sagging wire with twinkling bulbs
        let bulbs = (f.w / 5).max(2);
        for i in 0..bulbs {
            let x = (i * f.w / bulbs + 2) as i32;
            let c = pal::BULBS[i % pal::BULBS.len()];
            let tw = 0.65 + 0.35 * (t * 2.2 + i as f32 * 1.7).sin();
            f.put_g(x, 0, Glyph::new('●', c * (0.8 + 0.5 * tw)).lit(0.0));
            f.light_cell(x, 0, c * (0.55 * tw));
        }
        for x in 0..pw {
            f.px_set(x, 1, Rgb::hex(0x1d4a2c));
        }
        // falling snow
        let (w, h) = (f.w as f32, f.h as f32);
        for fl in &mut self.flakes {
            fl.y += fl.vy * dt;
            fl.x += (0.4 * (t * 0.7 + fl.phase).sin() + 0.25) * dt;
            if fl.y >= h {
                fl.y -= h + 1.0;
            }
            fl.x = fl.x.rem_euclid(w);
            let (cx, cy) = (fl.x as i32, fl.y as i32);
            if cy >= 1 && cy < f.h as i32 - 1 && !f.has_g(cx, cy) {
                f.put_g(cx, cy, Glyph::new(fl.ch, pal::SNOW * (fl.bright * 1.1)).lit(0.3));
            }
        }
    }
}

// ------------------------------------------------------- single instance

/// Hold an exclusive lock for the life of the process; false if another
/// settings window already has it (on Windows it's brought to the front).
fn single_instance() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("xmas-tui");
        let _ = std::fs::create_dir_all(&dir);
        let Ok(file) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("settings-window.lock")) else {
            return true; // can't lock: better two windows than none
        };
        let ok = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
        if ok {
            std::mem::forget(file); // keep the lock until we exit
        }
        ok
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, wide("Local\\xmas-tui-settings").as_ptr());
            if !h.is_null() && GetLastError() != ERROR_ALREADY_EXISTS {
                return true; // handle leaked on purpose: lives as long as we do
            }
            let other = FindWindowW(std::ptr::null(), wide(WIN_TITLE).as_ptr());
            if !other.is_null() {
                ShowWindow(other, SW_RESTORE);
                SetForegroundWindow(other);
            }
            false
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

// ----------------------------------------------------------------- window

struct Gfx {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    raster: Raster,
    fb: Vec<u32>,
    prev: Vec<Cell>,
    size: (u32, u32),
    offset: (usize, usize),
}

struct Runner {
    cfg: Option<Config>,
    path: PathBuf,
    gfx: Option<Gfx>,
    panel: Option<Panel>,
    start: Instant,
    last: Instant,
    next: Instant,
    cursor: (f64, f64),
    pointer: &'static str,
    mods: ModifiersState,
    focused: bool,
}

/// Cell size for a display scale factor (≈ the wallpaper's 10pt kitty cells).
fn cell_for(scale: f64) -> (usize, usize) {
    let ch = (18.0 * scale).round().clamp(12.0, 48.0) as usize;
    (((ch as f32 * 0.474).round() as usize).max(5), ch)
}

impl Runner {
    fn layout(&mut self, size: PhysicalSize<u32>) {
        let (Some(g), Some(p)) = (self.gfx.as_mut(), self.panel.as_mut()) else { return };
        let (w, h) = (size.width.max(1), size.height.max(1));
        let (cw, ch) = cell_for(g.window.scale_factor());
        if (cw, ch) != (g.raster.cw, g.raster.ch) {
            g.raster = Raster::new(cw, ch);
        }
        let (cols, rows) = ((w as usize / cw).max(1), (h as usize / ch).max(1));
        g.offset = ((w as usize - cols * cw) / 2, (h as usize - rows * ch) / 2);
        g.size = (w, h);
        g.fb = vec![0x0003_050f; (w * h) as usize];
        g.prev.clear();
        let _ = g.surface.resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        p.resize(cols, rows);
    }

    /// Ask for a window that fits the panel (at the current cell size).
    fn fit(&mut self) {
        let (Some(g), Some(p)) = (self.gfx.as_ref(), self.panel.as_mut()) else { return };
        let (cols, rows) = p.grid();
        let (cw, ch) = cell_for(g.window.scale_factor());
        let want = PhysicalSize::new((cols * cw) as u32, (rows * ch) as u32);
        g.window.set_min_inner_size(Some(want));
        if g.size.0 < want.width || g.size.1 < want.height {
            if let Some(s) = g.window.request_inner_size(want) {
                self.layout(s);
            }
        }
    }

    fn to_cell(&self, x: f64, y: f64) -> (i32, i32) {
        let Some(g) = self.gfx.as_ref() else { return (-1, -1) };
        let cx = ((x - g.offset.0 as f64) / g.raster.cw as f64).floor() as i32;
        let cy = ((y - g.offset.1 as f64) / g.raster.ch as f64).floor() as i32;
        (cx, cy)
    }

    fn draw(&mut self) {
        let (Some(g), Some(p)) = (self.gfx.as_mut(), self.panel.as_mut()) else { return };
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.25);
        self.last = now;
        let (cols, rows) = (p.w, p.h);
        let cells = p.step(self.start.elapsed().as_secs_f32(), dt);
        let (cw, ch) = (g.raster.cw, g.raster.ch);
        let stride = g.size.0 as usize;
        let full = g.prev.len() != cells.len();
        for cy in 0..rows {
            for cx in 0..cols {
                let i = cy * cols + cx;
                if full || cells[i] != g.prev[i] {
                    g.raster.draw_cell(&mut g.fb, stride, g.offset.0 + cx * cw, g.offset.1 + cy * ch, &cells[i]);
                }
            }
        }
        g.prev = cells.to_vec();
        if let Ok(mut buf) = g.surface.buffer_mut() {
            if buf.len() == g.fb.len() {
                buf.copy_from_slice(&g.fb);
                let _ = buf.present();
            }
        }
        let want = p.pointer();
        if want != self.pointer {
            self.pointer = want;
            g.window.set_cursor(match want {
                "pointer" => CursorIcon::Pointer,
                "text" => CursorIcon::Text,
                _ => CursorIcon::Default,
            });
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let Some(cfg) = self.cfg.take() else { return };
        let mut panel = Panel::new(cfg, &self.path);
        let (cols, rows) = panel.grid();
        // cell size from the primary monitor's scale until the window knows its own
        let scale = el.primary_monitor().map(|m| m.scale_factor()).unwrap_or(1.0);
        let (cw, ch) = cell_for(scale);
        let icon = Icon::from_rgba(crate::tray::icon_rgba(64), 64, 64).ok();
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title(WIN_TITLE)
            .with_window_icon(icon)
            .with_inner_size(PhysicalSize::new((cols * cw) as u32, (rows * ch) as u32));
        #[cfg(target_os = "linux")]
        {
            use winit::platform::startup_notify::{self, EventLoopExtStartupNotify, WindowAttributesExtStartupNotify};
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs = attrs.with_name("xmas-tui-settings", "xmas-tui-settings");
            // the tray host's activation token lets us take focus
            if let Some(token) = el.read_token_from_env() {
                startup_notify::reset_activation_token_env();
                attrs = attrs.with_activation_token(token);
            }
        }
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("xmas-tui: can't create the settings window: {e}");
                el.exit();
                return;
            }
        };
        let context = Context::new(window.clone()).expect("softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("softbuffer surface");
        let size = window.inner_size();
        panel.resize(cols, rows);
        self.panel = Some(panel);
        self.gfx = Some(Gfx { window: window.clone(), surface, raster: Raster::new(cw, ch), fb: vec![], prev: vec![], size: (0, 0), offset: (0, 0) });
        self.layout(size);
        self.fit();
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => el.exit(),
            WindowEvent::Resized(size) => self.layout(size),
            WindowEvent::ScaleFactorChanged { .. } => self.fit(),
            WindowEvent::Focused(f) => self.focused = f,
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                let (cx, cy) = self.to_cell(position.x, position.y);
                if let Some(p) = self.panel.as_mut() {
                    p.mouse(MouseIn::Move, cx, cy);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(p) = self.panel.as_mut() {
                    p.mouse(MouseIn::Move, -1, -1);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (cx, cy) = self.to_cell(self.cursor.0, self.cursor.1);
                if let Some(p) = self.panel.as_mut() {
                    p.mouse(MouseIn::Down(Btn::Left), cx, cy);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let up = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y > 0.0,
                    MouseScrollDelta::PixelDelta(p) => p.y > 0.0,
                };
                let (cx, cy) = self.to_cell(self.cursor.0, self.cursor.1);
                if let Some(p) = self.panel.as_mut() {
                    p.mouse(MouseIn::Scroll(up), cx, cy);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let ctrl = self.mods.control_key();
                let Some(p) = self.panel.as_mut() else { return };
                match &event.logical_key {
                    // Ctrl+W / Ctrl+Q close, like any small dialog
                    Key::Character(s) if ctrl => {
                        if s.eq_ignore_ascii_case("w") || s.eq_ignore_ascii_case("q") {
                            p.close = true;
                        }
                    }
                    Key::Named(NamedKey::Enter) => p.key(KeyIn::Enter),
                    Key::Named(NamedKey::Backspace) => p.key(KeyIn::Backspace),
                    Key::Named(NamedKey::Escape) => p.key(KeyIn::Esc),
                    Key::Named(NamedKey::Space) => p.key(KeyIn::Char(' ')),
                    Key::Character(s) => {
                        for c in s.chars() {
                            p.key(KeyIn::Char(c));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.panel.as_ref().map(|p| p.close).unwrap_or(false) {
            el.exit();
            return;
        }
        // smooth while in use, gentle in the background
        let dt = Duration::from_millis(if self.focused { 33 } else { 100 });
        let now = Instant::now();
        if now >= self.next {
            self.next = if now - self.next > dt { now + dt } else { self.next + dt };
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}

/// Run the standalone settings window until it's closed.
pub fn run(cfg: Config, path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if !single_instance() {
        return Ok(()); // one is already open
    }
    let el = EventLoop::new()?;
    let mut r = Runner {
        cfg: Some(cfg),
        path,
        gfx: None,
        panel: None,
        start: Instant::now(),
        last: Instant::now(),
        next: Instant::now(),
        cursor: (-1.0, -1.0),
        pointer: "",
        mods: ModifiersState::empty(),
        focused: true,
    };
    el.run_app(&mut r)?;
    Ok(())
}

/// Dev tool (`--settings-window --export DIR`): render the window's content
/// and the tray icons to PNGs instead of opening anything.
pub fn export(cfg: Config, path: &Path, dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut p = Panel::new(cfg, path);
    // don't read or write the user's config from a headless render
    p.sync.frozen = true;
    let mut t = 0.0;
    for _ in 0..90 {
        p.step(t, 1.0 / 30.0);
        t += 1.0 / 30.0;
    }
    // hover a slider so the "hot" state shows up too
    if let Some(r) = p.settings.rect {
        p.mouse(MouseIn::Move, r.x + 20, r.y + 3);
    }
    let (w, h) = (p.w, p.h);
    let cells = p.step(t, 1.0 / 30.0).to_vec();
    let out = Path::new(dir);
    crate::export::Exporter::new().render(&cells, w, h, &out.join("settings-window.png"))?;
    // tray icons, each upscaled (nearest) to 256 px so they're easy to inspect
    for n in [16usize, 22, 32, 64] {
        let rgba = crate::tray::icon_rgba(n);
        let k = 256 / n;
        let s = n * k;
        let mut img = vec![0u8; s * s * 4];
        for y in 0..s {
            for x in 0..s {
                let (si, di) = (((y / k) * n + x / k) * 4, (y * s + x) * 4);
                img[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
            }
        }
        let file = std::fs::File::create(out.join(format!("tray-icon-{n}.png")))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), s as u32, s as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().and_then(|mut wr| wr.write_image_data(&img)).map_err(std::io::Error::other)?;
    }
    eprintln!("exported the settings window ({w}x{h} cells) and tray icons to {dir}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Find the cell whose glyph row starts with `label` and return (x, y).
    fn find(p: &Panel, label: &str) -> (i32, i32) {
        let f = &p.frame;
        for y in 0..f.h as i32 {
            for x in 0..f.w as i32 {
                if label.chars().enumerate().all(|(i, c)| f.get_g(x + i as i32, y).map(|g| g.ch) == Some(c)) {
                    return (x, y);
                }
            }
        }
        panic!("{label} not on screen");
    }

    #[test]
    fn clicks_and_typing_reach_the_config_file() {
        let dir = std::env::temp_dir().join(format!("xmas-setwin-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Panel::new(Config::default(), &path);
        p.step(0.0, 0.03);
        // Snowfall ▸ (the arrow after the bar on the Snowfall row)
        let (_, y) = find(&p, "Snowfall");
        let x = (0..p.w as i32).rev().find(|&x| p.frame.get_g(x, y).map(|g| g.ch) == Some('▸')).unwrap();
        p.mouse(MouseIn::Down(Btn::Left), x, y);
        p.step(0.1, 0.03);
        assert_eq!(config::load(Some(&path.to_string_lossy())).scene.snow, 1.25);
        // the city field lives on the Display tab
        let (x, y) = find(&p, "Display");
        p.mouse(MouseIn::Down(Btn::Left), x, y);
        p.step(0.15, 0.03);
        // click the city field, type, Enter
        let (x, y) = find(&p, "(click to set)");
        p.mouse(MouseIn::Down(Btn::Left), x, y);
        p.step(0.2, 0.03);
        for c in "Tromsø".chars() {
            p.key(KeyIn::Char(c));
        }
        p.key(KeyIn::Enter);
        p.step(0.3, 0.03);
        assert_eq!(config::load(Some(&path.to_string_lossy())).weather.city, "Tromsø");
        // an outside edit (the tray / the wallpaper) is picked up, not overwritten
        let mut c = config::load(Some(&path.to_string_lossy()));
        c.scene.wind = 2.0;
        std::thread::sleep(Duration::from_millis(20));
        config::save(&c, &path).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        p.step(0.4, 0.03);
        assert_eq!(p.cfg.scene.wind, 2.0);
        assert_eq!(config::load(Some(&path.to_string_lossy())).scene.snow, 1.25);
        // Esc closes; ✕ closes
        p.key(KeyIn::Esc);
        assert!(p.close);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tray_icon_has_a_tree_shape() {
        for n in [16, 22, 32, 64] {
            let px = crate::tray::icon_rgba(n);
            assert_eq!(px.len(), n * n * 4);
            let alpha = |x: usize, y: usize| px[(y * n + x) * 4 + 3];
            assert_eq!(alpha(0, n - 1), 0, "corners transparent");
            assert!(alpha(n / 2, n * 3 / 4) > 200, "solid trunk/tree in the middle");
        }
    }
}
