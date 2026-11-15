//! In-wallpaper settings panel. Opened by the ⚙ button (top-right corner)
//! or a right-click anywhere. Mouse-driven: click ◂ ▸ or scroll on a row to
//! adjust, click checkboxes to toggle; the city field takes keyboard input.

use crate::color::{pal, Rgb};
use crate::config::Config;
use crate::frame::{Bg, Frame, Glyph};
use crate::widgets::Rect;

const BG: Bg = Bg::Mix(pal::UI_PANEL, 0.90);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum F {
    Snow,
    Wind,
    Glow,
    Sleigh,
    WalkSpeed,
    Traffic,
    TimeOfDay,
    Fps,
    Events,
    Aurora,
    Clock,
    Hour24,
    Seconds,
    Countdown,
    System,
    Music,
    Weather,
    Units,
    Locked,
    WinSnow,
    WinLights,
    Sound,
    SoundMaster,
    SoundMusic,
    SoundSfx,
    SoundAmb,
    SoundTrack,
    SoundCursor,
    SoundMedia,
    Critters,
    Bushes,
    Player,
    UpdCheck,
    Map,
    UpdAuto,
    WinFront,
    WinDocks,
    WinPile,
    WinDockPile,
    WinFall,
    WinWind,
    WinShake,
    WinMelt,
    WinBrush,
    WinDensity,
    WinSettle,
    WinLightStyle,
    WinLightCount,
    WinLightGlow,
    /// One of `EffectVolumes::ROWS`.
    Fx(u8),
    /// Ambience layer: wind, snowfall, fire, chimes, aurora.
    Amb(u8),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    Dec(F),
    Inc(F),
    Toggle(F),
    EditCity,
    Close,
    ResetLayout,
    Shuffle,
    Quit,
    Autostart,
    /// Switch the panel to another tab (UI state only, not config).
    Tab(u8),
    CheckUpdate,
    InstallUpdate,
    DismissUpdate,
}

#[derive(Default)]
pub struct Settings {
    pub open: bool,
    /// Some(text) while the city field is being edited.
    pub editing: Option<String>,
    regions: Vec<(Rect, Act)>,
    banner: Vec<(Rect, Act)>,
    pub rect: Option<Rect>,
    pub hover: (i32, i32),
    /// Native-window extras (quit button, start-with-OS toggle).
    pub extras: bool,
    /// Some(state) where start-with-OS is supported.
    pub autostart: Option<bool>,
    /// Live sound levels (from the audio thread), when there is one.
    pub meters: Option<[f32; crate::audio::METERS]>,
    /// Typing works (false behind the Windows desktop icons).
    pub keyboard: bool,
    /// The window snow/lights overlay is running.
    pub window_fx: bool,
    /// The config file doesn't parse: changes here aren't being saved.
    pub file_error: bool,
    /// `--fps` given on the command line (shown instead of the config value).
    pub fps_override: Option<f32>,
    /// Which tab is showing.
    pub tab: usize,
    /// Sound is currently faded out for other media.
    pub media_quiet: bool,
}

pub const TABS: [&str; 5] = ["Scene", "Display", "Sound", "Effects", "Windows"];
const LIGHT_COUNTS: [f32; 8] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 20.0];
const AMB_ROWS: [&str; 5] = ["Wind", "Snowfall hiss", "Fireplace", "Wind chimes", "Aurora shimmer"];
const PANEL_W: i32 = 66;
const PANEL_H: i32 = 28;

const SLEIGH_STEPS: [f32; 9] = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 10.0, 15.0, 30.0];
const FPS_STEPS: [f32; 6] = [15.0, 20.0, 24.0, 30.0, 45.0, 60.0];

/// A live level bar: `level` is a 0..1 peak, shown on a -48..0 dB scale.
fn meter_bar(f: &mut Frame, x: i32, y: i32, level: f32, w: i32) {
    let db = if level > 0.0 { 20.0 * level.log10() } else { -99.0 };
    let fill = ((db + 48.0) / 48.0).clamp(0.0, 1.0) * w as f32;
    const PART: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    for k in 0..w {
        let c = (fill - k as f32).clamp(0.0, 1.0);
        let col = if (k as f32) < w as f32 * 0.6 { Rgb::hex(0x5adf8a) } else if (k as f32) < w as f32 * 0.85 { Rgb::hex(0xffd36b) } else { Rgb::hex(0xff6b5a) };
        let ch = if c >= 1.0 { '█' } else { PART[(c * 8.0) as usize] };
        f.put_g(x + k, y, Glyph::new(ch, col).bg(Bg::Mix(Rgb::hex(0x0e1428), 0.9)).lit(0.0));
    }
}

/// A labelled meter row: name, bar and level in dB.
fn meter_row(f: &mut Frame, lx: i32, vx: i32, y: i32, label: &str, level: f32, w: i32) {
    let fg = if level > 0.004 { pal::UI_TEXT } else { pal::UI_DIM };
    f.text(lx, y, label, fg, BG, 0.0);
    meter_bar(f, vx, y, level, w);
    let db = if level > 0.0005 { format!("{:>4.0} dB", 20.0 * level.log10()) } else { "   — ".to_string() };
    f.text(vx + w + 2, y, &db, pal::UI_DIM, BG, 0.0);
}

fn step_list(list: &[f32], v: f32, dir: i32) -> f32 {
    if let Some(i) = list.iter().position(|&x| (x - v).abs() < 0.01) {
        return list[(i as i32 + dir).clamp(0, list.len() as i32 - 1) as usize];
    }
    // a hand-edited value between/outside the steps: go to the nearest step
    // in the arrow's direction (and stay put past the ends), never backwards
    let next = if dir > 0 { list.iter().find(|&&x| x > v) } else { list.iter().rev().find(|&&x| x < v) };
    next.copied().unwrap_or(v)
}

/// Apply an action to the config. Returns true if anything changed.
pub fn apply(cfg: &mut Config, act: Act) -> bool {
    let step = |v: &mut f32, d: f32, lo: f32, hi: f32| {
        *v = ((*v + d) * 100.0).round() / 100.0;
        *v = v.clamp(lo, hi);
    };
    match act {
        Act::Inc(f) | Act::Dec(f) => {
            let dir = if matches!(act, Act::Inc(_)) { 1.0 } else { -1.0 };
            match f {
                F::Snow => step(&mut cfg.scene.snow, 0.25 * dir, 0.0, 3.0),
                F::Wind => step(&mut cfg.scene.wind, 0.25 * dir, 0.0, 3.0),
                F::Glow => step(&mut cfg.scene.glow, 0.1 * dir, 0.0, 2.0),
                F::Events => step(&mut cfg.scene.events, 0.25 * dir, 0.0, 3.0),
                F::Sleigh => cfg.scene.sleigh_minutes = step_list(&SLEIGH_STEPS, cfg.scene.sleigh_minutes, dir as i32),
                F::Fps => cfg.fps = step_list(&FPS_STEPS, cfg.fps, dir as i32),
                F::SoundMaster => step(&mut cfg.sound.master, 0.05 * dir, 0.0, 1.0),
                F::SoundMusic => step(&mut cfg.sound.music, 0.05 * dir, 0.0, 1.0),
                F::SoundSfx => step(&mut cfg.sound.sfx, 0.05 * dir, 0.0, 1.0),
                F::SoundAmb => step(&mut cfg.sound.ambience, 0.05 * dir, 0.0, 1.0),
                F::SoundTrack => {
                    let names = crate::audio::track_names();
                    let i = names.iter().position(|n| *n == cfg.sound.track).unwrap_or(0) as i32;
                    cfg.sound.track = names[(i + dir as i32).rem_euclid(names.len() as i32) as usize].to_string();
                }
                F::Critters => step(&mut cfg.scene.critters, 0.25 * dir, 0.0, 3.0),
                F::WalkSpeed => step(&mut cfg.scene.walk_speed, 0.25 * dir, 0.25, 3.0),
                F::TimeOfDay => {
                    let m = crate::scene::daylight::MODES;
                    let i = m.iter().position(|o| *o == cfg.scene.time).unwrap_or(0) as i32;
                    cfg.scene.time = m[(i + dir as i32).rem_euclid(m.len() as i32) as usize].to_string();
                }
                F::Traffic => {
                    let opts = ["auto", "right", "left"];
                    let i = opts.iter().position(|o| *o == cfg.scene.traffic).unwrap_or(0) as i32;
                    cfg.scene.traffic = opts[(i + dir as i32).rem_euclid(3) as usize].to_string();
                }
                F::WinPile => step(&mut cfg.windows.pile_rows, 0.25 * dir, 0.25, 4.0),
                F::WinDensity => step(&mut cfg.windows.density, 0.25 * dir, 0.0, 3.0),
                F::WinSettle => step(&mut cfg.windows.settle, 0.25 * dir, 0.0, 4.0),
                F::WinDockPile => step(&mut cfg.windows.dock_rows, 0.25 * dir, 0.0, 4.0),
                F::WinFall => step(&mut cfg.windows.fall, 0.25 * dir, 0.25, 3.0),
                F::WinWind => step(&mut cfg.windows.wind, 0.25 * dir, 0.0, 3.0),
                F::WinShake => step(&mut cfg.windows.shake, 0.25 * dir, 0.0, 3.0),
                F::WinMelt => step(&mut cfg.windows.melt, 0.25 * dir, 0.0, 3.0),
                F::WinLightGlow => step(&mut cfg.windows.light_glow, 0.1 * dir, 0.0, 2.0),
                F::WinLightCount => cfg.windows.light_windows = step_list(&LIGHT_COUNTS, cfg.windows.light_windows as f32, dir as i32) as u32,
                F::WinLightStyle => {
                    let styles = crate::config::LIGHT_STYLES;
                    let i = styles.iter().position(|n| *n == cfg.windows.light_style).unwrap_or(0) as i32;
                    cfg.windows.light_style = styles[(i + dir as i32).rem_euclid(styles.len() as i32) as usize].to_string();
                }
                F::Fx(i) => {
                    let key = crate::config::EffectVolumes::ROWS[(i as usize).min(15)].0;
                    if let Some(v) = cfg.sound.effects.get_mut(key) {
                        step(v, 0.05 * dir, 0.0, 1.5);
                    }
                }
                F::Amb(i) => {
                    let m = &mut cfg.sound.ambience_mix;
                    let v = match i {
                        0 => &mut m.wind,
                        1 => &mut m.snowfall,
                        2 => &mut m.fire,
                        3 => &mut m.chimes,
                        _ => &mut m.aurora,
                    };
                    step(v, 0.05 * dir, 0.0, 1.5);
                }
                F::Units => return apply(cfg, Act::Toggle(F::Units)),
                _ => return apply(cfg, Act::Toggle(f)),
            }
            true
        }
        Act::Toggle(f) => {
            match f {
                F::Aurora => cfg.scene.aurora ^= true,
                F::Clock => cfg.clock.enabled ^= true,
                F::Hour24 => cfg.clock.hour24 ^= true,
                F::Seconds => cfg.clock.seconds ^= true,
                F::Countdown => cfg.clock.countdown ^= true,
                F::System => cfg.widgets.system ^= true,
                F::Music => cfg.widgets.music ^= true,
                F::Weather => cfg.widgets.weather ^= true,
                F::Locked => cfg.layout.locked ^= true,
                F::WinSnow => cfg.windows.snow ^= true,
                F::WinLights => cfg.windows.lights ^= true,
                F::Sound => cfg.sound.enabled ^= true,
                F::SoundCursor => cfg.sound.cursor ^= true,
                F::SoundMedia => cfg.sound.pause_for_media ^= true,
                F::Bushes => cfg.scene.bushes ^= true,
                F::Player => cfg.scene.player ^= true,
                F::UpdCheck => cfg.updates.check ^= true,
                F::Map => cfg.widgets.map ^= true,
                F::UpdAuto => cfg.updates.auto_install ^= true,
                F::WinFront => cfg.windows.front ^= true,
                F::WinBrush => cfg.windows.brush ^= true,
                F::WinDocks => cfg.windows.docks ^= true,
                F::Units => {
                    cfg.weather.units = if cfg.weather.units.to_lowercase().starts_with('f') { "celsius".into() } else { "fahrenheit".into() }
                }
                _ => return false,
            }
            true
        }
        Act::ResetLayout => {
            for i in 0..4 {
                cfg.layout.set(i, None);
            }
            true
        }
        Act::Shuffle | Act::EditCity | Act::Close | Act::Quit | Act::Autostart | Act::Tab(_) | Act::CheckUpdate | Act::InstallUpdate | Act::DismissUpdate => false,
    }
}

pub fn gear_rect(w: i32) -> Rect {
    Rect { x: w - 4, y: 0, w: 3, h: 1 }
}

impl Settings {
    pub fn click(&self, x: i32, y: i32) -> Option<Act> {
        self.regions.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, a)| *a)
    }

    /// Scroll over a value row → step it.
    pub fn scroll(&self, x: i32, y: i32, up: bool) -> Option<Act> {
        let f = self.regions.iter().find_map(|(r, a)| {
            if r.y == y && x >= self.rect?.x && x < self.rect?.x + self.rect?.w {
                match a {
                    Act::Inc(f) | Act::Dec(f) => Some(*f),
                    _ => None,
                }
            } else {
                None
            }
        })?;
        Some(if up { Act::Inc(f) } else { Act::Dec(f) })
    }

    /// "A new version is out — install?" (when updates ask first).
    pub fn draw_banner(&mut self, f: &mut Frame, t: f32) {
        self.banner.clear();
        let Some(v) = crate::update::offer() else { return };
        let msg = format!(" ✦ xmas-tui {v} is out ");
        let (install, later) = (" install ", " later ");
        let n = (msg.chars().count() + install.len() + later.len() + 2) as i32;
        let (x, y) = ((f.w as i32 - n) / 2, 1);
        let bg = Bg::Mix(pal::UI_PANEL, 0.88);
        let tw = 0.9 + 0.1 * (t * 2.0).sin();
        let mut cx = f.text(x, y, &msg, pal::UI_TEXT * tw, bg, 0.0);
        for (label, act, col) in [(install, Act::InstallUpdate, Rgb::hex(0xffd36b)), (later, Act::DismissUpdate, pal::UI_DIM)] {
            let r = Rect { x: cx + 1, y, w: label.chars().count() as i32, h: 1 };
            let hot = r.contains(self.hover.0, self.hover.1);
            let b = Bg::Mix(if hot { Rgb::hex(0x34406a) } else { Rgb::hex(0x1a2242) }, 0.95);
            cx = f.text(r.x, y, label, if hot { col * 1.2 } else { col }, b, 0.0);
            self.banner.push((r, act));
        }
    }

    /// A click on the update banner?
    pub fn banner_click(&self, x: i32, y: i32) -> Option<Act> {
        self.banner.iter().find(|(r, _)| r.contains(x, y)).map(|(_, a)| *a)
    }

    pub fn draw_gear(&self, f: &mut Frame, t: f32) {
        let r = gear_rect(f.w as i32);
        let hot = r.contains(self.hover.0, self.hover.1) || self.open;
        let c = if hot { Rgb::hex(0xffd36b) * (1.1 + 0.1 * (t * 5.0).sin()) } else { pal::UI_DIM * 0.9 };
        f.put_g(r.x + 1, r.y, Glyph::new('⚙', c).bg(Bg::Mix(pal::UI_PANEL, if hot { 0.7 } else { 0.0 })).lit(0.0));
        if hot && !self.open {
            f.text(r.x - 9, r.y, " settings", pal::UI_TEXT, Bg::Mix(pal::UI_PANEL, 0.8), 0.0);
        }
    }

    pub fn draw(&mut self, f: &mut Frame, cfg: &Config, t: f32) {
        self.regions.clear();
        self.rect = None;
        if !self.open {
            return;
        }
        let (w, h) = (PANEL_W, PANEL_H);
        let x = (f.w as i32 - w) / 2;
        let y = ((f.h as i32 - h) / 2).max(0);
        let rect = Rect { x, y, w, h };
        self.rect = Some(rect);
        // frame
        let b = pal::UI_BORDER;
        for yy in y..y + h {
            for xx in x..x + w {
                let ch = match (xx == x, xx == x + w - 1, yy == y, yy == y + h - 1) {
                    (true, _, true, _) => '╭',
                    (_, true, true, _) => '╮',
                    (true, _, _, true) => '╰',
                    (_, true, _, true) => '╯',
                    (_, _, true, _) | (_, _, _, true) => '─',
                    (true, _, _, _) | (_, true, _, _) => '│',
                    _ => ' ',
                };
                f.put_g(xx, yy, Glyph::new(ch, b).bg(BG).lit(0.0));
            }
        }
        f.text(x + 2, y, " ⚙ settings ", pal::UI_TITLE, BG, 0.0);
        let close = Rect { x: x + w - 5, y, w: 3, h: 1 };
        let hot = close.contains(self.hover.0, self.hover.1);
        f.text(close.x, y, " ✕ ", if hot { pal::CANDY_RED * 1.3 } else { pal::UI_TEXT }, BG, 0.0);
        self.regions.push((close, Act::Close));

        // tab bar
        let tabs: Vec<usize> = (0..TABS.len()).filter(|&i| i != 4 || self.window_fx).collect();
        if !tabs.contains(&self.tab) {
            self.tab = 0;
        }
        let mut tx = x + 3;
        for &i in &tabs {
            let label = format!(" {} ", TABS[i]);
            let r = Rect { x: tx, y: y + 1, w: label.chars().count() as i32, h: 1 };
            let on = i == self.tab;
            let hot = r.contains(self.hover.0, self.hover.1);
            let (fg, bg) = if on {
                (Rgb::hex(0xffd36b) * 1.2, Bg::Solid(Rgb::hex(0x3a2a14)))
            } else if hot {
                (pal::UI_TITLE, Bg::Mix(Rgb::hex(0x2a2440), 0.95))
            } else {
                (pal::UI_DIM * 1.2, BG)
            };
            f.text(tx, y + 1, &label, fg, bg, 0.0);
            self.regions.push((r, Act::Tab(i as u8)));
            tx += r.w + 1;
        }
        for xx in x + 1..x + w - 1 {
            f.put_g(xx, y + 2, Glyph::new('┄', pal::UI_BORDER * 0.45).bg(BG).lit(0.0));
        }

        let lx = x + 3;
        let vx = x + 20;
        let yy = y + 4;
        match self.tab {
            0 => self.tab_scene(f, cfg, lx, vx, yy, t),
            1 => self.tab_display(f, cfg, lx, vx, yy, t),
            2 => self.tab_sound(f, cfg, lx, vx, yy, t),
            3 => self.tab_effects(f, cfg, lx, x + 28, yy, t),
            _ => self.tab_windows(f, cfg, lx, vx, yy, t),
        }

        let tips = [
            "drag any widget to move it  ·  scroll on a value to adjust",
            if self.extras && !self.keyboard {
                "⚙ (top-right) opens/closes this  ·  pick up the critters!"
            } else {
                // (≤ 62 cells: the inner width from lx to the right border)
                "right-click to open/close  ·  pick up animals & people!"
            },
        ];
        for (k, tip) in tips.iter().enumerate() {
            // clipped so a longer tip can never run over the border
            let tip: String = tip.chars().take((x + w - 1 - lx) as usize).collect();
            f.text(lx, y + h - 3 + k as i32, &tip, pal::UI_DIM, BG, 0.0);
        }
        if self.file_error {
            // on the bottom border, like the title on the top one
            f.text(x + 2, y + h - 1, " ⚠ config file has an error · changes aren't saved ", pal::CANDY_RED * 1.3, BG, 0.0);
        }
    }

    fn tab_scene(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        self.section(f, lx, yy, "SCENE");
        yy += 1;
        let pct = |v: f32, hi: f32| v / hi;
        self.slider(f, lx, vx, yy, "Snowfall", F::Snow, pct(cfg.scene.snow, 3.0), &format!("{:.2}×", cfg.scene.snow), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Wind", F::Wind, pct(cfg.scene.wind, 3.0), &format!("{:.2}×", cfg.scene.wind), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Glow", F::Glow, pct(cfg.scene.glow, 2.0), &format!("{:.2}×", cfg.scene.glow), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Surprises", F::Events, pct(cfg.scene.events, 3.0), &format!("{:.2}×", cfg.scene.events), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Animals & people", F::Critters, pct(cfg.scene.critters, 3.0), &format!("{:.2}×", cfg.scene.critters), t);
        yy += 1;
        let sl = if cfg.scene.sleigh_minutes <= 0.0 { "never".to_string() } else { format!("every ~{:.0} min", cfg.scene.sleigh_minutes) };
        self.stepper(f, lx, vx, yy, "Santa's sleigh", F::Sleigh, &sl);
        yy += 1;
        self.slider(f, lx, vx, yy, "Walk speed", F::WalkSpeed, pct(cfg.scene.walk_speed, 3.0), &format!("{:.2}×", cfg.scene.walk_speed), t);
        yy += 1;
        let side = match cfg.scene.traffic.as_str() {
            "left" => "keep left",
            "right" => "keep right",
            _ => "auto (weather city)",
        };
        self.stepper(f, lx, vx, yy, "Traffic", F::Traffic, side);
        yy += 1;
        let tod = if cfg.scene.time == "auto" { "auto (real sunrise & sunset)".to_string() } else { cfg.scene.time.clone() };
        self.stepper(f, lx, vx, yy, "Time of day", F::TimeOfDay, &tod);
        yy += 1;
        let fps = match self.fps_override {
            Some(o) => format!("{o:.0} fps (--fps)"),
            None => format!("{:.0} fps", cfg.fps),
        };
        self.stepper(f, lx, vx, yy, "Frame rate", F::Fps, &fps);
        yy += 2;
        let cx = self.check(f, lx, yy, "Aurora", Act::Toggle(F::Aurora), cfg.scene.aurora) + 3;
        let cx = self.check(f, cx, yy, "Berry bushes", Act::Toggle(F::Bushes), cfg.scene.bushes) + 3;
        self.check(f, cx, yy, "Villager (WASD)", Act::Toggle(F::Player), cfg.scene.player);
        yy += 2;
        self.button(f, lx, yy, "↻ shuffle scene", Act::Shuffle);
    }

    fn tab_display(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        self.section(f, lx, yy, "CLOCK");
        yy += 1;
        let mut cx = self.check(f, lx, yy, "Show clock", Act::Toggle(F::Clock), cfg.clock.enabled) + 3;
        cx = self.check(f, cx, yy, "24-hour", Act::Toggle(F::Hour24), cfg.clock.hour24) + 3;
        cx = self.check(f, cx, yy, "Seconds", Act::Toggle(F::Seconds), cfg.clock.seconds) + 3;
        self.check(f, cx, yy, "Countdown", Act::Toggle(F::Countdown), cfg.clock.countdown);
        yy += 2;
        self.section(f, lx, yy, "WIDGETS");
        yy += 1;
        let mut cx = self.check(f, lx, yy, "System", Act::Toggle(F::System), cfg.widgets.system) + 3;
        cx = self.check(f, cx, yy, "Music", Act::Toggle(F::Music), cfg.widgets.music) + 3;
        cx = self.check(f, cx, yy, "Weather", Act::Toggle(F::Weather), cfg.widgets.weather) + 3;
        self.check(f, cx, yy, "Map", Act::Toggle(F::Map), cfg.widgets.map);
        yy += 1;
        f.text(lx, yy, "Weather city", pal::UI_TEXT, BG, 0.0);
        let field = Rect { x: vx, y: yy, w: 30, h: 1 };
        let (txt, editing) = match &self.editing {
            Some(s) => (s.clone(), true),
            None => (if cfg.weather.city.is_empty() { "(click to set)".into() } else { cfg.weather.city.clone() }, false),
        };
        for i in 0..field.w {
            let fg = if editing { pal::UI_TITLE } else { pal::UI_TEXT };
            let bgc = if editing { Rgb::hex(0x24305a) } else { Rgb::hex(0x1a2242) };
            let ch = txt.chars().nth(i as usize).unwrap_or(' ');
            f.put_g(field.x + i, yy, Glyph::new(ch, fg).solid(bgc).lit(0.0));
        }
        if editing && (t * 2.0) as i32 % 2 == 0 {
            let cx = field.x + txt.chars().count().min(field.w as usize - 1) as i32;
            f.put_g(cx, yy, Glyph::new('▏', Rgb::hex(0xffd36b)).solid(Rgb::hex(0x1a2240)).lit(0.0));
        }
        self.regions.push((field, Act::EditCity));
        if editing {
            f.text(field.x + field.w + 1, yy, "⏎ save", pal::UI_DIM, BG, 0.0);
        } else if !self.keyboard {
            f.text(field.x + field.w + 1, yy, "edits config", pal::UI_DIM, BG, 0.0);
        }
        yy += 1;
        let units = if cfg.weather.units.to_lowercase().starts_with('f') { "°F  ·  mph" } else { "°C  ·  km/h" };
        self.stepper(f, lx, vx, yy, "Units", F::Units, units);
        yy += 2;
        self.section(f, lx, yy, "LAYOUT");
        yy += 1;
        let cx = self.check(f, lx, yy, "Lock widget positions", Act::Toggle(F::Locked), cfg.layout.locked) + 4;
        self.button(f, cx, yy, "⟲ reset positions", Act::ResetLayout);
        yy += 2;
        self.section(f, lx, yy, "UPDATES");
        yy += 1;
        let cx = self.check(f, lx, yy, "Check for updates", Act::Toggle(F::UpdCheck), cfg.updates.check) + 3;
        self.check(f, cx, yy, "Install automatically", Act::Toggle(F::UpdAuto), cfg.updates.auto_install);
        yy += 1;
        use crate::update::Status;
        let ver = crate::update::current();
        let (msg, act) = match crate::update::status() {
            Status::Unsupported => (format!("v{ver} · built from source: updates come from git"), None),
            Status::Managed => (format!("v{ver} · system-wide install: update via the package manager"), None),
            _ if !crate::update::running() => (format!("v{ver} · the wallpaper checks for updates"), None),
            Status::Idle => (format!("v{ver}"), Some(("check now", Act::CheckUpdate))),
            Status::Checking => (format!("v{ver} · checking…"), None),
            Status::UpToDate => (format!("v{ver} · up to date ✓"), Some(("check now", Act::CheckUpdate))),
            Status::Available(v) => (format!("v{ver} · {v} is available"), Some(("⬇ install", Act::InstallUpdate))),
            Status::Installing(v) => (format!("installing {v}…"), None),
            Status::Installed(v) => (format!("{v} installed · restarting…"), None),
            Status::Failed(e) => (format!("update: {e}"), Some(("retry", Act::CheckUpdate))),
        };
        let msg: String = msg.chars().take(44).collect();
        let cx = f.text(lx, yy, &msg, pal::UI_DIM, BG, 0.0);
        if let Some((label, a)) = act {
            self.button(f, cx + 2, yy, label, a);
        }
        yy += 2;
        let mut cx = lx;
        if let Some(on) = self.autostart {
            cx = self.check(f, lx, yy, "Run at startup", Act::Autostart, on) + 4;
        }
        if self.extras {
            self.button(f, cx, yy, "✕ quit wallpaper", Act::Quit);
        }
    }

    fn tab_sound(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        self.section(f, lx, yy, "SOUND");
        yy += 1;
        let cx = self.check(f, lx, yy, "Sound on", Act::Toggle(F::Sound), cfg.sound.enabled) + 3;
        self.check(f, cx, yy, "Cursor sparkle", Act::Toggle(F::SoundCursor), cfg.sound.cursor);
        yy += 1;
        let vol = |v: f32| format!("{:.0}%", v * 100.0);
        self.slider(f, lx, vx, yy, "Master volume", F::SoundMaster, cfg.sound.master, &vol(cfg.sound.master), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Music", F::SoundMusic, cfg.sound.music, &vol(cfg.sound.music), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Effects", F::SoundSfx, cfg.sound.sfx, &vol(cfg.sound.sfx), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Ambience", F::SoundAmb, cfg.sound.ambience, &vol(cfg.sound.ambience), t);
        yy += 1;
        let track = if cfg.sound.track == "shuffle" { "shuffle all styles".to_string() } else { cfg.sound.track.replace('-', " ") };
        self.stepper(f, lx, vx, yy, "Music style", F::SoundTrack, &track);
        yy += 1;
        let cx = self.check(f, lx, yy, "Quiet while other media plays", Act::Toggle(F::SoundMedia), cfg.sound.pause_for_media) + 2;
        if self.media_quiet && cfg.sound.pause_for_media {
            f.text(cx, yy, "♪ paused for media", Rgb::hex(0xffd36b), BG, 0.0);
        }
        yy += 2;
        self.section(f, lx, yy, "AMBIENCE MIX");
        yy += 1;
        let m = &cfg.sound.ambience_mix;
        for (i, v) in [m.wind, m.snowfall, m.fire, m.chimes, m.aurora].iter().enumerate() {
            self.slider(f, lx, vx, yy, AMB_ROWS[i], F::Amb(i as u8), v / 1.5, &vol(*v), t);
            yy += 1;
        }
        yy += 1;
        self.section(f, lx, yy, "WHAT'S PLAYING");
        yy += 1;
        match self.meters {
            None => {
                f.text(lx, yy, "live levels show in the wallpaper's own settings (right-click)", pal::UI_DIM, BG, 0.0);
            }
            Some(m) => {
                // music and ambience, then whichever effects are sounding,
                // loudest first
                use crate::audio::{meter_label, METER_AMB, METER_MUSIC, METER_OTHER};
                let mut fx: Vec<usize> = (0..=METER_OTHER).filter(|&i| m[i] > 0.004).collect();
                fx.sort_by(|a, b| m[*b].partial_cmp(&m[*a]).unwrap_or(std::cmp::Ordering::Equal));
                let mut rows = vec![METER_MUSIC, METER_AMB];
                rows.extend(fx.into_iter().take(2));
                for i in rows {
                    meter_row(f, lx, lx + 24, yy, meter_label(i), m[i], 20);
                    yy += 1;
                }
            }
        }
    }

    fn tab_effects(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        self.section(f, lx, yy, "EFFECT VOLUMES");
        yy += 1;
        for (i, (key, label)) in crate::config::EffectVolumes::ROWS.iter().enumerate() {
            let v = cfg.sound.effects.get(key);
            self.slider(f, lx, vx, yy, label, F::Fx(i as u8), v / 1.5, &format!("{:.0}%", v * 100.0), t);
            // what this group is playing right now
            if let Some(m) = self.meters {
                meter_bar(f, vx + 32, yy, m[i], 4);
            }
            yy += 1;
        }
    }

    fn tab_windows(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        let wc = &cfg.windows;
        self.section(f, lx, yy, "SNOW ON WINDOWS");
        yy += 1;
        let mut cx = self.check(f, lx, yy, "Snow on windows", Act::Toggle(F::WinSnow), wc.snow) + 3;
        cx = self.check(f, cx, yy, "In front", Act::Toggle(F::WinFront), wc.front) + 3;
        self.check(f, cx, yy, "Cursor sweeps", Act::Toggle(F::WinBrush), wc.brush);
        yy += 1;
        let pct = |v: f32, max: f32| v / max;
        self.slider(f, lx, vx, yy, "Pile height", F::WinPile, pct(wc.pile_rows, 4.0), &format!("{:.2} rows", wc.pile_rows), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Snowfall", F::WinDensity, pct(wc.density, 3.0), &format!("{:.2}×", wc.density), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Settling speed", F::WinSettle, pct(wc.settle, 4.0), &format!("{:.2}×", wc.settle), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Fall speed", F::WinFall, pct(wc.fall, 3.0), &format!("{:.2}×", wc.fall), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Wind", F::WinWind, pct(wc.wind, 3.0), &format!("{:.2}×", wc.wind), t);
        yy += 1;
        let shake = if wc.shake <= 0.0 { "off".to_string() } else { format!("{:.2}×", wc.shake) };
        self.slider(f, lx, vx, yy, "Shake-off", F::WinShake, pct(wc.shake, 3.0), &shake, t);
        yy += 1;
        let melt = if wc.melt <= 0.0 { "never".to_string() } else { format!("{:.2}×", wc.melt) };
        self.slider(f, lx, vx, yy, "Melting", F::WinMelt, pct(wc.melt, 3.0), &melt, t);
        yy += 1;
        let hint = if wc.front { "flakes drift over open windows" } else { "piles still grow; nothing drifts over the work underneath" };
        f.text(lx, yy, hint, pal::UI_DIM, BG, 0.0);
        yy += 2;
        self.section(f, lx, yy, "TASKBAR");
        yy += 1;
        self.check(f, lx, yy, "Snow on the taskbar", Act::Toggle(F::WinDocks), wc.docks);
        yy += 1;
        self.slider(f, lx, vx, yy, "Taskbar pile", F::WinDockPile, pct(wc.dock_rows, 4.0), &format!("{:.2} rows", wc.dock_rows), t);
        yy += 2;
        self.section(f, lx, yy, "LIGHTS AROUND WINDOWS");
        yy += 1;
        self.check(f, lx, yy, "Lights around windows", Act::Toggle(F::WinLights), wc.lights);
        yy += 1;
        let style = match wc.light_style.as_str() {
            "warm" => "warm white",
            "icy" => "icy blue",
            "candy" => "candy cane",
            _ => "multicolour",
        };
        self.stepper(f, lx, vx, yy, "Colours", F::WinLightStyle, style);
        yy += 1;
        let n = if wc.light_windows >= 20 { "all windows".to_string() } else if wc.light_windows == 1 { "top window only".into() } else { format!("top {} windows", wc.light_windows) };
        self.stepper(f, lx, vx, yy, "On", F::WinLightCount, &n);
        yy += 1;
        self.slider(f, lx, vx, yy, "Brightness", F::WinLightGlow, wc.light_glow / 2.0, &format!("{:.1}×", wc.light_glow), t);
    }

    fn section(&self, f: &mut Frame, x: i32, y: i32, s: &str) {
        let c = f.text(x, y, s, Rgb::hex(0xffd36b), BG, 0.0);
        for i in 0..(58 - s.len() as i32) {
            f.put_g(c + 1 + i, y, Glyph::new('┄', pal::UI_BORDER * 0.45).bg(BG).lit(0.0));
        }
    }

    fn arrow(&mut self, f: &mut Frame, x: i32, y: i32, ch: char, act: Act) {
        let r = Rect { x, y, w: 1, h: 1 };
        let hot = r.contains(self.hover.0, self.hover.1);
        f.put_g(x, y, Glyph::new(ch, if hot { Rgb::hex(0xffd36b) * 1.3 } else { pal::UI_BORDER }).bg(BG).lit(0.0));
        self.regions.push((Rect { x: x - 1, y, w: 3, h: 1 }, act));
    }

    #[allow(clippy::too_many_arguments)]
    fn slider(&mut self, f: &mut Frame, lx: i32, vx: i32, y: i32, label: &str, field: F, frac: f32, val: &str, t: f32) {
        f.text(lx, y, label, pal::UI_TEXT, BG, 0.0);
        self.arrow(f, vx, y, '◂', Act::Dec(field));
        let bw = 20;
        let n = (frac.clamp(0.0, 1.0) * bw as f32).round() as i32;
        let shift = (t * 3.0) as i32;
        for i in 0..bw {
            let g = if i < n {
                Glyph::new('▰', if (i + shift).rem_euclid(2) == 0 { pal::CANDY_RED } else { pal::CANDY_WHITE })
            } else {
                Glyph::new('▱', pal::UI_DIM * 0.7)
            };
            f.put_g(vx + 2 + i, y, g.bg(BG).lit(0.0));
        }
        self.arrow(f, vx + 3 + bw, y, '▸', Act::Inc(field));
        f.text(vx + 6 + bw, y, val, pal::UI_TITLE, BG, 0.0);
    }

    fn stepper(&mut self, f: &mut Frame, lx: i32, vx: i32, y: i32, label: &str, field: F, val: &str) {
        f.text(lx, y, label, pal::UI_TEXT, BG, 0.0);
        self.arrow(f, vx, y, '◂', Act::Dec(field));
        let c = f.text(vx + 2, y, val, pal::UI_TITLE, BG, 0.0);
        self.arrow(f, c + 1, y, '▸', Act::Inc(field));
    }

    /// Returns the x after the label.
    fn check(&mut self, f: &mut Frame, x: i32, y: i32, label: &str, act: Act, on: bool) -> i32 {
        let len = 4 + label.chars().count() as i32;
        let r = Rect { x, y, w: len, h: 1 };
        let hot = r.contains(self.hover.0, self.hover.1);
        let box_c = if hot { Rgb::hex(0xffd36b) * 1.2 } else { pal::UI_BORDER };
        f.text(x, y, "[", box_c, BG, 0.0);
        f.put_g(x + 1, y, Glyph::new(if on { '✓' } else { ' ' }, pal::PINE * 1.4).bg(BG).lit(0.0));
        f.text(x + 2, y, "]", box_c, BG, 0.0);
        let end = f.text(x + 4, y, label, if on { pal::UI_TEXT } else { pal::UI_DIM }, BG, 0.0);
        self.regions.push((r, act));
        end
    }

    fn button(&mut self, f: &mut Frame, x: i32, y: i32, label: &str, act: Act) {
        let len = label.chars().count() as i32 + 2;
        let r = Rect { x, y, w: len, h: 1 };
        let hot = r.contains(self.hover.0, self.hover.1);
        let bg = if hot { Bg::Solid(Rgb::hex(0x3a2a14)) } else { Bg::Mix(Rgb::hex(0x1c1a2c), 0.95) };
        f.text(x, y, &format!(" {label} "), if hot { Rgb::hex(0xffd36b) * 1.2 } else { pal::UI_TEXT }, bg, 0.0);
        self.regions.push((r, act));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steppers_move_the_way_the_arrow_points_for_off_list_values() {
        assert_eq!(step_list(&FPS_STEPS, 120.0, 1), 120.0); // past the end: stays
        assert_eq!(step_list(&FPS_STEPS, 120.0, -1), 60.0);
        assert_eq!(step_list(&FPS_STEPS, 10.0, -1), 10.0);
        assert_eq!(step_list(&FPS_STEPS, 10.0, 1), 15.0);
        assert_eq!(step_list(&FPS_STEPS, 25.0, 1), 30.0);
        assert_eq!(step_list(&FPS_STEPS, 25.0, -1), 24.0);
        assert_eq!(step_list(&SLEIGH_STEPS, 60.0, 1), 60.0);
        assert_eq!(step_list(&FPS_STEPS, 30.0, 1), 45.0); // on-list: unchanged behaviour
    }
}
