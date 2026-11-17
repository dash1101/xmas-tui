//! In-wallpaper settings panel (and the tray's settings window, which draws
//! the same one). Opened by the ⚙ button (top-right corner) or from a
//! right-click menu. Mouse-driven: click or drag along a slider, click ◂ ▸
//! or scroll on a row to step it, click checkboxes to toggle; the text
//! fields take the keyboard. Hovering over a setting says what it does.

use crate::color::{pal, Rgb};
use crate::config::Config;
use crate::frame::{Bg, Frame, Glyph};
use crate::widgets::Rect;

const BG: Bg = Bg::Mix(pal::UI_PANEL, 0.90);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum F {
    TalkTime,
    Chatty,
    PetKind,
    PetCoat,
    Snow,
    Wind,
    Glow,
    Sleigh,
    WalkSpeed,
    Traffic,
    TimeOfDay,
    Scale,
    UiScale,
    Zoom,
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
    MostAtOnce,
    Viz,
    VizKhz,
    FriendsW,
    Bushes,
    Player,
    Autopilot,
    OnlyFriends,
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
    /// A slider set by clicking (or dragging) along its bar: to this
    /// fraction of its range.
    Set(F, f32),
    /// Start typing in a text field.
    Edit(Field),
    Close,
    ResetLayout,
    /// Open the village in its own window / full screen (as a game).
    PopOut,
    FullScreen,
    /// The wallpaper went full screen: back behind the desktop.
    BackToWallpaper,
    /// A new resolution needs the wallpaper to restart: now, or not yet.
    RestartNow,
    RestartLater,
    Shuffle,
    /// The save file: a copy out (to Documents), or one in.
    ExportSave,
    ImportSave,
    Quit,
    Autostart,
    /// Switch the panel to another tab (UI state only, not config).
    Tab(u8),
    CheckUpdate,
    InstallUpdate,
    DismissUpdate,
    /// The shelves (Shelves tab): rename shelf i, change its look or icons,
    /// open its folder, remove it; or make a new one.
    ShelfStyle(u8, i8),
    ShelfIcons(u8),
    ShelfOpen(u8),
    ShelfRemove(u8),
    ShelfNew,
    /// Pick shelf i's folder; a new shelf from a picked folder.
    ShelfFolder(u8),
    ShelfNewFrom,
    /// Linux: make the village the desktop wallpaper (kitty panel + service).
    SetWallpaper,
    /// The "make it the wallpaper?" banner: not now.
    DismissWallOffer,
    /// Linux (KDE): the village on the lock screen, or off it again; or
    /// drawn afresh (the scene's changed).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    LockScreen,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    LockRedraw,
    /// Install ffmpeg (through the system's password prompt): the lock
    /// screen moves. (Linux)
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    InstallFfmpeg,
    /// Widget i's look, one along (or back).
    Look(u8, i8),
    /// Friend i: comes as a kid / a grown-up; off the list.
    FriendKid(u8),
    FriendRemove(u8),
    /// Friend i's look: open (or close) its editor; a part (0 coat, 1 hat,
    /// 2 hair, 3 skin) one along (or back); the scarf on or off.
    FriendSel(u8),
    FriendLook(u8, u8, i8),
    FriendScarf(u8),
}

/// The text fields.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Field {
    City,
    Name,
    PetName,
    Shelf(u8),
    Friend(u8),
    /// A new name for the friends & family list.
    NewFriend,
    /// The line on the villager's profile card.
    Motto,
}

/// How many friends & family fit on the list.
pub const MAX_FRIENDS: usize = 10;
/// The look editor's "friend" number for the villager's own look.
pub const YOU_LOOK: u8 = 255;

#[derive(Default)]
pub struct Settings {
    pub open: bool,
    /// Some(text) while a text field is being typed in, and which one.
    pub editing: Option<String>,
    /// Waiting for a key to set for one of the villager's actions (an
    /// index into `config::KEY_ACTIONS`).
    pub binding: Option<usize>,
    pub edit: Option<Field>,
    regions: Vec<(Rect, Act)>,
    /// Labels (not clickable): hovering one explains its setting.
    hints: Vec<(Rect, Act)>,
    /// A slider being dragged along.
    pub slider_drag: Option<F>,
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
    /// The front-end can draw the UI bigger than the village (its own
    /// letters): the UI size setting works here.
    pub ui_layer_ok: bool,
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
    /// Where the panel was dragged to (top-left), and the grab offset while
    /// it's being dragged by its title bar.
    pub pos: Option<(i32, i32)>,
    pub drag: Option<(i32, i32)>,
    /// Running as a normal window (popped out), not the wallpaper.
    pub windowed: bool,
    /// The settings window (no village of its own to pop out).
    pub standalone: bool,
    /// The wallpaper itself is showing full screen (not behind the icons).
    pub wall_full: bool,
    /// Ask whether to restart for a new resolution.
    pub restart_ask: bool,
    /// Whose look is being chosen (You tab).
    pub friend_sel: Option<u8>,
    /// A brief note at the top (zoom / resolution changes): text, until.
    pub toast: Option<(String, f32)>,
    /// Linux, in a window of its own: offer to make it the desktop
    /// wallpaper (a banner, until it's done or dismissed).
    pub wall_offer: bool,
    /// The controls card is showing.
    pub help: bool,
}

pub const TABS: [&str; 8] = ["Village", "You", "Widgets", "Shelves", "Sound", "Effects", "Windows", "Desktop"];
pub const TAB_VILLAGE: usize = 0;
pub const TAB_YOU: usize = 1;
pub const TAB_WIDGETS: usize = 2;
pub const TAB_SHELVES: usize = 3;
pub const TAB_SOUND: usize = 4;
pub const TAB_EFFECTS: usize = 5;
pub const TAB_WINDOWS: usize = 6;
pub const TAB_DESKTOP: usize = 7;
const LIGHT_COUNTS: [f32; 8] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 20.0];
const AMB_ROWS: [&str; 5] = ["Wind", "Snowfall hiss", "Fireplace", "Wind chimes", "Aurora shimmer"];
const PANEL_W: i32 = 76;
const PANEL_H: i32 = 30;

const SLEIGH_STEPS: [f32; 9] = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 10.0, 15.0, 30.0];
const FPS_STEPS: [f32; 6] = [15.0, 20.0, 24.0, 30.0, 45.0, 60.0];
const MOST_STEPS: [f32; 14] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0];

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

/// Resolution as shown in settings: 1 / scale, to the nearest 0.05.
pub fn resolution(scale: f32) -> f32 {
    ((1.0 / scale.max(0.1)) * 20.0).round() / 20.0
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
                F::MostAtOnce => cfg.scene.most_at_once = step_list(&MOST_STEPS, cfg.scene.most_at_once as f32, dir as i32).round().max(1.0) as u32,
                F::WalkSpeed => step(&mut cfg.scene.walk_speed, 0.25 * dir, 0.25, 6.0),
                F::TalkTime => step(&mut cfg.scene.talk_time, 0.25 * dir, 0.5, 3.0),
                F::Chatty => step(&mut cfg.scene.chattiness, 0.25 * dir, 0.0, 2.0),
                F::PetKind => {
                    let opts = ["dog", "cat", "bunny", "none"];
                    let i = opts.iter().position(|o| *o == cfg.scene.pet.kind).unwrap_or(0) as i32;
                    cfg.scene.pet.kind = opts[(i + dir as i32).rem_euclid(4) as usize].to_string();
                    // a colour this kind comes in
                    if let Some(k) = crate::scene::pets::PetKind::from_str(&cfg.scene.pet.kind) {
                        if !k.coats().iter().any(|c| c.0 == cfg.scene.pet.coat) {
                            cfg.scene.pet.coat = k.coats()[0].0.to_string();
                        }
                    }
                }
                F::PetCoat => {
                    if let Some(k) = crate::scene::pets::PetKind::from_str(&cfg.scene.pet.kind) {
                        let c = k.coats();
                        let i = c.iter().position(|o| o.0 == cfg.scene.pet.coat).unwrap_or(0) as i32;
                        cfg.scene.pet.coat = c[(i + dir as i32).rem_euclid(c.len() as i32) as usize].0.to_string();
                    }
                }
                F::Scale => {
                    // shown as resolution (1 / scale), in 0.05 steps
                    let res = (resolution(cfg.scale) + 0.05 * dir).clamp(0.5, 2.0);
                    cfg.scale = ((1.0 / res) * 10000.0).round() / 10000.0;
                }
                F::Zoom => step(&mut cfg.zoom, 0.05 * dir, 1.0, 3.0),
                F::UiScale => {
                    cfg.ui.scale = ((cfg.ui.scale + 0.1 * dir).clamp(1.0, 2.5) * 10.0).round() / 10.0;
                }
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
                F::Autopilot => cfg.scene.autopilot ^= true,
                F::OnlyFriends => cfg.scene.only_friends ^= true,
                F::UpdCheck => cfg.updates.check ^= true,
                F::Map => cfg.widgets.map ^= true,
                F::Viz => cfg.widgets.visualizer ^= true,
                F::VizKhz => cfg.widgets.visualizer_khz ^= true,
                F::FriendsW => cfg.widgets.friends ^= true,
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
        Act::Set(f, frac) => {
            let Some((v, max, lo, st)) = slider_val(cfg, f) else { return false };
            let new = ((frac.clamp(0.0, 1.0) * max / st).round() * st).clamp(lo, max);
            let new = (new * 100.0).round() / 100.0;
            if (new - *v).abs() < 1e-4 {
                return false;
            }
            *v = new;
            true
        }
        Act::Look(9, d) => {
            cfg.ui.look = crate::widgets::looks::step(&cfg.ui.look, d as i32).into();
            true
        }
        Act::Look(i, d) => {
            let Some(l) = cfg.widgets.looks.get_mut(i as usize) else { return false };
            *l = crate::widgets::looks::step(l, d as i32).into();
            true
        }
        Act::FriendKid(i) => {
            let Some(fr) = cfg.scene.friends.get_mut(i as usize) else { return false };
            fr.kid ^= true;
            true
        }
        // (255: the villager's own look)
        Act::FriendLook(YOU_LOOK, part, d) => {
            cfg.scene.wear.wear_step(part, d as i32);
            true
        }
        Act::FriendScarf(YOU_LOOK) => {
            cfg.scene.wear.scarf ^= true;
            true
        }
        Act::FriendLook(i, part, d) => {
            let Some(fr) = cfg.scene.friends.get_mut(i as usize) else { return false };
            fr.step(part, d as i32);
            true
        }
        Act::FriendScarf(i) => {
            let Some(fr) = cfg.scene.friends.get_mut(i as usize) else { return false };
            fr.scarf ^= true;
            true
        }
        Act::FriendRemove(i) => {
            if (i as usize) < cfg.scene.friends.len() {
                cfg.scene.friends.remove(i as usize);
                true
            } else {
                false
            }
        }
        Act::ResetLayout => {
            for i in [0, 1, 2, 3, 5] {
                cfg.layout.set(i, None);
            }
            true
        }
        Act::ShelfStyle(i, d) => {
            let Some(c) = cfg.shelves.get_mut(i as usize) else { return false };
            let styles = crate::widgets::shelf::STYLES;
            let k = styles.iter().position(|&x| x == c.style).unwrap_or(0) as i32;
            c.style = styles[(k + d as i32).rem_euclid(styles.len() as i32) as usize].into();
            true
        }
        Act::ShelfIcons(i) => {
            let Some(c) = cfg.shelves.get_mut(i as usize) else { return false };
            c.icons = if c.icons == "real" { "pixel".into() } else { "real".into() };
            true
        }
        Act::ShelfRemove(i) => {
            if (i as usize) < cfg.shelves.len() {
                cfg.shelves.remove(i as usize);
                true
            } else {
                false
            }
        }
        Act::ShelfNew => {
            crate::widgets::shelf::add(cfg, None);
            true
        }
        Act::Shuffle | Act::ExportSave | Act::ImportSave | Act::Edit(_) | Act::BackToWallpaper | Act::RestartNow | Act::RestartLater | Act::Close | Act::Quit | Act::Autostart | Act::Tab(_) | Act::CheckUpdate | Act::InstallUpdate | Act::DismissUpdate | Act::PopOut | Act::FullScreen | Act::ShelfOpen(_) | Act::ShelfFolder(_) | Act::ShelfNewFrom | Act::SetWallpaper | Act::DismissWallOffer | Act::LockScreen | Act::LockRedraw | Act::InstallFfmpeg | Act::FriendSel(_) => false,
    }
}

/// A slider's value: (the value, the bar's full scale, its lowest, its step).
fn slider_val(cfg: &mut Config, f: F) -> Option<(&mut f32, f32, f32, f32)> {
    let w = &mut cfg.windows;
    Some(match f {
        F::Snow => (&mut cfg.scene.snow, 3.0, 0.0, 0.25),
        F::Wind => (&mut cfg.scene.wind, 3.0, 0.0, 0.25),
        F::Glow => (&mut cfg.scene.glow, 2.0, 0.0, 0.1),
        F::Events => (&mut cfg.scene.events, 3.0, 0.0, 0.25),
        F::Critters => (&mut cfg.scene.critters, 3.0, 0.0, 0.25),
        F::WalkSpeed => (&mut cfg.scene.walk_speed, 6.0, 0.25, 0.25),
        F::TalkTime => (&mut cfg.scene.talk_time, 3.0, 0.5, 0.25),
        F::Chatty => (&mut cfg.scene.chattiness, 2.0, 0.0, 0.25),
        F::SoundMaster => (&mut cfg.sound.master, 1.0, 0.0, 0.05),
        F::SoundMusic => (&mut cfg.sound.music, 1.0, 0.0, 0.05),
        F::SoundSfx => (&mut cfg.sound.sfx, 1.0, 0.0, 0.05),
        F::SoundAmb => (&mut cfg.sound.ambience, 1.0, 0.0, 0.05),
        F::Amb(i) => {
            let m = &mut cfg.sound.ambience_mix;
            let v = match i {
                0 => &mut m.wind,
                1 => &mut m.snowfall,
                2 => &mut m.fire,
                3 => &mut m.chimes,
                _ => &mut m.aurora,
            };
            (v, 1.5, 0.0, 0.05)
        }
        F::Fx(i) => {
            let key = crate::config::EffectVolumes::ROWS[(i as usize).min(15)].0;
            (cfg.sound.effects.get_mut(key)?, 1.5, 0.0, 0.05)
        }
        F::WinPile => (&mut w.pile_rows, 4.0, 0.25, 0.25),
        F::WinDensity => (&mut w.density, 3.0, 0.0, 0.25),
        F::WinSettle => (&mut w.settle, 4.0, 0.0, 0.25),
        F::WinFall => (&mut w.fall, 3.0, 0.25, 0.25),
        F::WinWind => (&mut w.wind, 3.0, 0.0, 0.25),
        F::WinShake => (&mut w.shake, 3.0, 0.0, 0.25),
        F::WinMelt => (&mut w.melt, 3.0, 0.0, 0.25),
        F::WinDockPile => (&mut w.dock_rows, 4.0, 0.0, 0.25),
        F::WinLightGlow => (&mut w.light_glow, 2.0, 0.0, 0.1),
        _ => return None,
    })
}

/// What a setting does, shown at the bottom of the panel while the pointer
/// is over it.
fn tip(a: Act) -> Option<&'static str> {
    let f = match a {
        Act::Inc(f) | Act::Dec(f) | Act::Toggle(f) | Act::Set(f, _) => f,
        Act::Edit(Field::City) => return Some("The weather widget's city (and the real sunrise, sunset and side of the road)."),
        Act::Edit(Field::Name) => return Some("What the village calls the villager. Word gets round once it's set."),
        Act::Edit(Field::PetName) => return Some("The pet's name. People mention it, and it answers to it."),
        Act::Edit(Field::Shelf(_)) => return Some("The shelf's name (its folder is renamed with it)."),
        Act::Edit(Field::Friend(_)) => return Some("Their name in the village. Clear it to take them off the list."),
        Act::Edit(Field::Motto) => return Some("A line about you, on your profile card (friends online see it)."),
        Act::FriendSel(YOU_LOOK) => return Some("How you look in the village (and to friends online): coat, hat, hair, skin, scarf."),
        Act::Edit(Field::NewFriend) => return Some("A friend or family member: some visitors take these names and know the villager."),
        Act::FriendKid(_) => return Some("Whether they come to the village as a grown-up or as a kid."),
        Act::FriendRemove(_) => return Some("Take them off the list."),
        Act::FriendSel(_) => return Some("How they look in the village: click to choose their coat, hat, hair and skin."),
        Act::FriendLook(_, 0, _) => return Some("Their coat."),
        Act::FriendLook(_, 1, _) => return Some("Their bobble hat (or none: bare-headed)."),
        Act::FriendLook(_, 2, _) => return Some("Their hair."),
        Act::FriendLook(..) => return Some("Their skin."),
        Act::FriendScarf(_) => return Some("A scarf, to match their hat."),
        Act::Look(9, _) => return Some("How this panel, the menus, conversations and the controls card look: classic, frosted with snow, a wooden frame, or a gift box."),
        Act::Look(..) => return Some("The widget's look: classic, frosted with snow, a wooden frame, or a gift box."),
        Act::ShelfStyle(..) => return Some("The shelf's look: frosted with snow, a wooden shelf, or a gift box."),
        Act::ShelfIcons(_) => return Some("Each item's real icon (album art, thumbnails), or its icon as pixel art."),
        Act::ShelfOpen(_) => return Some("Open the shelf's folder in the file manager."),
        Act::ShelfFolder(_) => return Some("Pick another folder for this shelf to show."),
        Act::ShelfRemove(_) => return Some("Take the shelf away (its folder and files stay)."),
        Act::ShelfNew | Act::ShelfNewFrom => return Some("A new shelf: in its own new folder, or showing a folder picked now."),
        Act::Shuffle => return Some("A new village: the houses, trees and folk are laid out afresh."),
        Act::ExportSave => return Some("A copy of the save (this village, your pockets, stats and scorecard) in Documents: to keep, or to take to another computer."),
        Act::ImportSave => return Some("Load a save: its village (houses where they were) and its pockets, stats and scorecard."),
        Act::ResetLayout => return Some("Put the widgets back where they started."),
        Act::PopOut => return Some("Open the village in a window of its own, to play in."),
        Act::FullScreen => return Some("The village full screen, to play in (F11 again comes back)."),
        Act::BackToWallpaper => return Some("Back behind the desktop icons."),
        Act::Autostart => return Some("Start the wallpaper when the computer starts."),
        Act::Quit => return Some("Stop the wallpaper (the desktop goes back to normal)."),
        Act::SetWallpaper => return Some("Make the village the desktop wallpaper (sets up the service that keeps it running)."),
        Act::LockScreen | Act::LockRedraw => return Some("Show a looping video of the village on KDE's lock screen."),
        Act::InstallFfmpeg => return Some("Install ffmpeg with the system's package manager (it asks for your password first). Then redraw the lock screen and it moves."),
        Act::CheckUpdate | Act::InstallUpdate => return Some("Look for a newer xmas-tui on GitHub (downloads are checked before installing)."),
        _ => return None,
    };
    Some(match f {
        F::Snow => "How much snow falls.",
        F::Wind => "How hard the wind blows the snow (and the smoke from the chimneys).",
        F::Glow => "The glow round lights, windows and the tree.",
        F::Events => "How often something happens: the train, fireworks, shooting stars, a sleigh…",
        F::Critters => "How many people and animals come and go.",
        F::MostAtOnce => "The most people and animals in the village at once.",
        F::Sleigh => "How often Santa's sleigh flies over.",
        F::Traffic => "Which side of the road the cars keep to (auto: from the weather city's country).",
        F::TimeOfDay => "Day and night: following the real sun (auto), or one time of day always.",
        F::Fps => "Frames a second. Lower is lighter on the computer.",
        F::Aurora => "The northern lights in the sky.",
        F::Bushes => "Berry bushes to pick berries from (animals love them).",
        F::Player => "The villager: walk about with WASD or the arrow keys.",
        F::OnlyFriends => "Only the friends & family named here come by as people (the animals still wander through). Off: the village's own folk come by too.",
        F::Autopilot => "Left alone a while, the villager goes about their own day: snowmen and igloos, snowballs, feeding the animals, chats, tag with the kids. Any key takes over again.",
        F::WalkSpeed => "How fast the villager walks (Shift runs).",
        F::TalkTime => "How long speech bubbles stay up.",
        F::Chatty => "How often people stop to chat and remark on things.",
        F::PetKind => "The villager's pet: a dog, a cat, a bunny, or none.",
        F::PetCoat => "The pet's colour.",
        F::Clock => "The big clock.",
        F::Hour24 => "A 24-hour clock.",
        F::Seconds => "Seconds on the clock.",
        F::Countdown => "The countdown to Christmas under the clock.",
        F::System => "CPU, GPU, memory, disk and network.",
        F::Music => "What's playing, from any music player.",
        F::Weather => "The weather in the weather city.",
        F::Map => "A strip map of the village: where the villager is.",
        F::Viz => "Bars dancing to whatever's playing (it listens to the sound output; nothing is recorded).",
        F::FriendsW => "While online: friends (who's about), the last few messages, a click for each friend's menu (ping, invite, write), and a line to talk.",
        F::VizKhz => "Each bar's frequency under it.",
        F::Units => "°F and mph, or °C and km/h.",
        F::Locked => "Stop the widgets being dragged about by accident.",
        F::Scale => "Finer or coarser detail (more or fewer, smaller or bigger characters).",
        F::UiScale => "The menus, panels, widgets and writing drawn bigger; the village stays its own size. (The window and the Windows wallpaper; a terminal draws one size of letter.)",
        F::Zoom => "Magnify the whole village.",
        F::Sound => "All of the sound.",
        F::SoundCursor => "A soft sparkle as the pointer moves.",
        F::SoundMedia => "Fade out while other music or video plays.",
        F::SoundMaster => "The overall volume.",
        F::SoundMusic => "The music's volume.",
        F::SoundSfx => "All the sound effects (each one's in the Effects tab).",
        F::SoundAmb => "The background: wind, snowfall, the fire, chimes.",
        F::SoundTrack => "Which music plays.",
        F::Amb(_) => "This part of the background sound.",
        F::Fx(_) => "This sound effect's volume.",
        F::UpdCheck => "Look for new versions now and then.",
        F::UpdAuto => "Install new versions without asking.",
        F::WinSnow => "Snow settles on top of open windows.",
        F::WinFront => "The snow falls in front of windows too.",
        F::WinBrush => "Moving the pointer through the snow sweeps it away.",
        F::WinDocks => "Snow on the taskbar.",
        F::WinLights => "Christmas lights round the open windows.",
        F::WinPile => "How deep the snow piles up on windows.",
        F::WinDockPile => "How deep the snow piles up on the taskbar.",
        F::WinDensity => "How much snow falls over the windows.",
        F::WinSettle => "How fast the snow settles on windows.",
        F::WinFall => "How fast the flakes fall.",
        F::WinWind => "How hard the wind blows the flakes.",
        F::WinShake => "How much snow falls off when a window is moved.",
        F::WinMelt => "How fast settled snow melts away.",
        F::WinLightStyle => "The lights' colours.",
        F::WinLightCount => "How many windows get lights (the ones in front).",
        F::WinLightGlow => "How bright the lights are.",
    })
}

/// The settings button: always there, top right.
pub fn gear_rect(w: i32) -> Rect {
    Rect { x: w - 14, y: 0, w: 13, h: 1 }
}

/// The controls button, just left of it.
pub fn help_rect(w: i32) -> Rect {
    Rect { x: w - 26, y: 0, w: 11, h: 1 }
}

/// The re-centre button (back to the villager), left of the controls one.
pub fn centre_rect(w: i32) -> Rect {
    Rect { x: w - 36, y: 0, w: 9, h: 1 }
}

/// The friends & online button, left of the zoom ones.
pub fn online_rect(w: i32) -> Rect {
    Rect { x: w - 57, y: 0, w: 13, h: 1 }
}

/// The zoom buttons (out, in), left of the re-centre one.
pub fn zoom_rects(w: i32) -> (Rect, Rect) {
    (Rect { x: w - 43, y: 0, w: 3, h: 1 }, Rect { x: w - 40, y: 0, w: 3, h: 1 })
}

/// The controls card: (keys, what they do, which of the villager's keys
/// it is: those can be clicked to change them).
fn controls(k: &crate::config::KeysCfg) -> Vec<(String, &'static str, Option<usize>)> {
    use crate::config::KeysCfg as K;
    let key = |i: usize| K::label(k.get(i));
    vec![
        ("W A S D / arrows".into(), "walk about", None),
        ("Shift (held)".into(), "run", None),
        (format!("{} / Enter", key(0)), "talk, use, sit, pick up", Some(0)),
        (key(1), "jump: hold it to go higher (up to 4×)", Some(1)),
        (format!("{} at a wall", key(1)), "leap up onto a house, a tree, a widget…", None),
        ("W up on something".into(), "on up it (a roof's ridge, a tree's star)", None),
        (key(7), "lie down in the snow (then jump: a snow angel)", Some(7)),
        (key(8), "things to do: build, sing, tag, lights…", Some(8)),
        (key(3), "invite someone along / send them home", Some(3)),
        (key(2), "scoop a snowball, then throw it", Some(2)),
        (key(5), "what's in hand: berries, a carrot, cocoa…", Some(5)),
        ("1 - 5".into(), "use an item (or buy one at the cabin)", None),
        (key(4), "your pockets, scorecard and Christmas list", Some(4)),
        ("↑ ↓ Enter".into(), "choose in menus and conversations", None),
        ("Esc".into(), "close what's open · else the settings", None),
        (key(6), "the view back on the villager", Some(6)),
        ("F11".into(), "full screen / back to the wallpaper", None),
        ("Ctrl + / -, Ctrl wheel".into(), "zoom in / out, or pinch (Ctrl 0: back)", None),
        ("Ctrl Shift + / -".into(), "resolution: finer / coarser detail", None),
        ("click".into(), "people, animals, the sky, the train…", None),
        ("drag".into(), "pick up people & animals; move widgets", None),
        ("wheel · Shift wheel".into(), "look along the village · up into the sky", None),
        ("trackpad".into(), "two fingers: along, and up into the sky", None),
        ("hold a click".into(), "the right-click menu", None),
    ]
}

/// Where the controls card sits on a `w`×`h` screen: (x, y, w, h).
fn help_box(w: i32, h: i32, rows: usize) -> (i32, i32, i32, i32) {
    let (bw, bh) = (72, rows as i32 + 6);
    ((w - bw) / 2, ((h - bh) / 2).max(1), bw, bh)
}

impl Settings {
    /// Start typing in `which` (whatever was being typed elsewhere is put
    /// in place first).
    pub fn begin_edit(&mut self, cfg: &mut Config, which: Field) {
        if self.edit == Some(which) && self.editing.is_some() {
            return;
        }
        self.commit(cfg);
        let text = match which {
            Field::City => cfg.weather.city.clone(),
            Field::Name => cfg.scene.name.clone(),
            Field::PetName => cfg.scene.pet.name.clone(),
            Field::Shelf(i) => cfg.shelves.get(i as usize).map(|c| c.name.clone()).unwrap_or_default(),
            Field::Friend(i) => cfg.scene.friends.get(i as usize).map(|c| c.name.clone()).unwrap_or_default(),
            Field::NewFriend => String::new(),
            Field::Motto => cfg.scene.motto.clone(),
        };
        self.edit = Some(which);
        self.editing = Some(text);
    }

    /// Put what's been typed where it belongs (and stop typing).
    pub fn commit(&mut self, cfg: &mut Config) {
        let (which, text) = (self.edit.take(), self.editing.take());
        let (Some(which), Some(text)) = (which, text) else { return };
        let text = text.trim();
        let short = || text.chars().take(16).collect::<String>();
        match which {
            Field::City => {
                if text != cfg.weather.city {
                    cfg.weather.city = text.to_string();
                    cfg.weather.latitude = None;
                    cfg.weather.longitude = None;
                }
            }
            Field::Name => cfg.scene.name = short(),
            Field::PetName => {
                if !text.is_empty() {
                    cfg.scene.pet.name = short();
                }
            }
            Field::Shelf(i) => crate::widgets::shelf::rename(cfg, i as usize, text),
            Field::Friend(i) => {
                let i = i as usize;
                if text.is_empty() {
                    if i < cfg.scene.friends.len() {
                        cfg.scene.friends.remove(i);
                    }
                } else if let Some(fr) = cfg.scene.friends.get_mut(i) {
                    fr.name = short();
                }
            }
            Field::NewFriend => {
                if !text.is_empty() && cfg.scene.friends.len() < MAX_FRIENDS {
                    cfg.scene.friends.push(crate::config::FriendCfg { name: short(), ..Default::default() });
                }
            }
            Field::Motto => cfg.scene.motto = text.chars().take(60).collect(),
        }
    }

    /// Stop typing, keeping nothing.
    pub fn cancel_edit(&mut self) {
        self.edit = None;
        self.editing = None;
    }

    /// A slider being dragged along: the setting under (x, y) on its row.
    pub fn drag_slider(&self, x: i32, y: i32) -> Option<Act> {
        let f = self.slider_drag?;
        let on_row: Vec<(Rect, f32)> = self.regions.iter().filter_map(|(r, a)| match a {
            Act::Set(g, frac) if *g == f => Some((*r, *frac)),
            _ => None,
        }).collect();
        let (first, last) = (on_row.first()?, on_row.last()?);
        let _ = y;
        // past either end: the end
        let frac = if x < first.0.x {
            0.0
        } else if x >= last.0.x {
            1.0
        } else {
            on_row.iter().find(|(r, _)| r.x == x).map(|(_, v)| *v)?
        };
        Some(Act::Set(f, frac))
    }

    /// On the title bar (where the panel can be picked up), not its ✕.
    pub fn on_title(&self, x: i32, y: i32) -> bool {
        self.rect.is_some_and(|r| y == r.y && x >= r.x && x < r.x + r.w - 5)
    }

    /// Start moving the panel (grabbed at (x, y) on its title bar).
    pub fn start_drag(&mut self, x: i32, y: i32) {
        if let Some(r) = self.rect {
            self.drag = Some((x - r.x, y - r.y));
        }
    }

    pub fn drag_to(&mut self, x: i32, y: i32) {
        if let Some((ox, oy)) = self.drag {
            self.pos = Some((x - ox, y - oy));
        }
    }

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
        if let Some((text, until)) = self.toast.as_ref().filter(|(_, u)| t < *u) {
            let k = ((until - t) / 0.4).clamp(0.0, 1.0);
            let n = text.chars().count() as i32 + 4;
            let (x, y) = ((f.w as i32 - n) / 2, 1);
            f.text(x, y, &format!("  {text}  "), Rgb::hex(0xffd36b) * (0.4 + 0.8 * k), Bg::Mix(pal::UI_PANEL, 0.9 * k), 0.0);
            return;
        }
        let (msg, install, later, yes, no) = if self.restart_ask {
            (" ✦ the new resolution needs a restart ".to_string(), " restart now ", " later ", Act::RestartNow, Act::RestartLater)
        } else if let Some(v) = crate::update::offer() {
            (format!(" ✦ xmas-tui {v} is out "), " install ", " later ", Act::InstallUpdate, Act::DismissUpdate)
        } else if self.wall_offer && !self.open {
            (" ✦ make the village the desktop wallpaper? ".to_string(), " set it up ", " not now ", Act::SetWallpaper, Act::DismissWallOffer)
        } else {
            return;
        };
        let n = (msg.chars().count() + install.len() + later.len() + 2) as i32;
        let (x, y) = ((f.w as i32 - n) / 2, 1);
        let bg = Bg::Mix(pal::UI_PANEL, 0.88);
        let tw = 0.9 + 0.1 * (t * 2.0).sin();
        let mut cx = f.text(x, y, &msg, pal::UI_TEXT * tw, bg, 0.0);
        for (label, act, col) in [(install, yes, Rgb::hex(0xffd36b)), (later, no, pal::UI_DIM)] {
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

    /// The controls card (? button, F1, or "?").
    pub fn draw_help(&mut self, f: &mut Frame, keys: &crate::config::KeysCfg, look: &str) {
        if !self.help {
            return;
        }
        let rows = controls(keys);
        let (x, y, w, h) = help_box(f.w as i32, f.h as i32, rows.len());
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
                f.put_g(xx, yy, Glyph::new(ch, pal::UI_BORDER).bg(BG).lit(0.0));
            }
        }
        f.text(x + 2, y, " ⌨ controls ", pal::UI_TITLE, BG, 0.0);
        f.text(x + w - 5, y, " ✕ ", pal::UI_TEXT, BG, 0.0);
        for (i, (k, what, slot)) in rows.iter().enumerate() {
            let yy = y + 2 + i as i32;
            let waiting = slot.is_some() && *slot == self.binding;
            let hot = slot.is_some() && self.hover.1 == yy && self.hover.0 >= x + 2 && self.hover.0 < x + 24;
            let (txt, bg) = if waiting { ("press a key…".to_string(), Bg::Mix(Rgb::hex(0x34406a), 0.95)) } else { (k.clone(), if hot { Bg::Mix(Rgb::hex(0x34406a), 0.95) } else { BG }) };
            if slot.is_some() {
                f.text(x + 2, yy, &format!("[{txt}]"), Rgb::hex(0xffd36b), bg, 0.0);
            } else {
                f.text(x + 3, yy, &txt, Rgb::hex(0xffd36b), BG, 0.0);
            }
            f.text(x + 25, yy, what, pal::UI_TEXT, BG, 0.0);
        }
        let foot = if self.binding.is_some() { "press the new key · Esc: keep the old one" } else { "click a [key] to change it · anywhere else to close" };
        f.text(x + 3, y + h - 2, foot, pal::UI_DIM, BG, 0.0);
        crate::widgets::looks::apply(f, Rect { x, y, w, h }, look, BG, 13);
    }

    /// A click on the controls card: on one of the villager's [keys], wait
    /// for a new one (true); anywhere else, close it (false).
    pub fn help_click(&mut self, x: i32, y: i32, w: i32, h: i32, keys: &crate::config::KeysCfg) -> bool {
        let rows = controls(keys);
        let (bx, by, _, _) = help_box(w, h, rows.len());
        let i = y - by - 2;
        if x >= bx + 2 && x < bx + 24 && i >= 0 {
            if let Some((_, _, Some(slot))) = rows.get(i as usize) {
                self.binding = Some(*slot);
                return true;
            }
        }
        self.binding = None;
        false
    }

    /// The settings button (always showing, so the panel's always one
    /// click away — right-click isn't available everywhere).
    pub fn draw_gear(&self, f: &mut Frame, t: f32) {
        // zoom out / in
        let (zo, zi) = zoom_rects(f.w as i32);
        for (r, ch) in [(zo, '−'), (zi, '+')] {
            let hot = r.contains(self.hover.0, self.hover.1);
            let bg = Bg::Mix(pal::UI_PANEL, if hot { 0.9 } else { 0.62 });
            f.put_g(r.x, r.y, Glyph::new(' ', pal::UI_TEXT).bg(bg).lit(0.0));
            f.put_g(r.x + 1, r.y, Glyph::new(ch, if hot { pal::UI_TITLE } else { Rgb::hex(0xbfe4ff) * 0.9 }).bg(bg).lit(0.0));
            f.put_g(r.x + 2, r.y, Glyph::new(' ', pal::UI_TEXT).bg(bg).lit(0.0));
        }
        // the re-centre button: back to the villager
        let r = centre_rect(f.w as i32);
        let hot = r.contains(self.hover.0, self.hover.1);
        let bg = Bg::Mix(pal::UI_PANEL, if hot { 0.9 } else { 0.62 });
        f.put_g(r.x, r.y, Glyph::new('▐', pal::UI_PANEL * 0.0).bg(bg).lit(0.0));
        f.put_g(r.x + 1, r.y, Glyph::new('⌖', Rgb::hex(0x9be08a) * if hot { 1.2 } else { 0.85 }).bg(bg).lit(0.0));
        f.text(r.x + 2, r.y, " centre", if hot { pal::UI_TITLE } else { pal::UI_TEXT * 0.85 }, bg, 0.0);
        // and the controls button beside it
        let r = help_rect(f.w as i32);
        let hot = r.contains(self.hover.0, self.hover.1) || self.help;
        let bg = Bg::Mix(pal::UI_PANEL, if hot { 0.9 } else { 0.62 });
        f.put_g(r.x, r.y, Glyph::new('▐', pal::UI_PANEL * 0.0).bg(bg).lit(0.0));
        f.put_g(r.x + 1, r.y, Glyph::new('?', Rgb::hex(0x8ad0ff) * if hot { 1.2 } else { 0.85 }).bg(bg).lit(0.0));
        f.text(r.x + 2, r.y, " controls", if hot { pal::UI_TITLE } else { pal::UI_TEXT * 0.85 }, bg, 0.0);
        let r = gear_rect(f.w as i32);
        let hot = r.contains(self.hover.0, self.hover.1) || self.open;
        let bg = Bg::Mix(pal::UI_PANEL, if hot { 0.9 } else { 0.62 });
        let (gc, tc) = if hot { (Rgb::hex(0xffd36b) * (1.1 + 0.1 * (t * 5.0).sin()), pal::UI_TITLE) } else { (Rgb::hex(0xffd36b) * 0.8, pal::UI_TEXT * 0.85) };
        f.put_g(r.x, r.y, Glyph::new('▐', pal::UI_PANEL * 0.0).bg(bg).lit(0.0));
        f.put_g(r.x + 1, r.y, Glyph::new('⚙', gc).bg(bg).lit(0.0));
        f.text(r.x + 2, r.y, " settings ", tc, bg, 0.0);
    }

    /// The friends & online button (`state`: 0 offline, 1 connecting, 2
    /// online; how many friends are about; something waiting).
    pub fn draw_online(&self, f: &mut Frame, state: u8, about: usize, waiting: bool, t: f32) {
        let r = online_rect(f.w as i32);
        let hot = r.contains(self.hover.0, self.hover.1);
        let bg = Bg::Mix(pal::UI_PANEL, if hot { 0.9 } else { 0.62 });
        f.put_g(r.x, r.y, Glyph::new('▐', pal::UI_PANEL * 0.0).bg(bg).lit(0.0));
        let pulse = if waiting { 1.0 + 0.3 * (t * 4.0).sin() } else { 1.0 };
        f.put_g(r.x + 1, r.y, Glyph::new('✉', Rgb::hex(0xffd36b) * if hot { 1.2 } else { 0.85 * pulse }).bg(bg).lit(0.0));
        let x = f.text(r.x + 2, r.y, " friends", if hot { pal::UI_TITLE } else { pal::UI_TEXT * 0.85 }, bg, 0.0);
        let (dot, c) = match state {
            2 if about > 0 => (format!(" ●{about}"), Rgb::hex(0x9be08a)),
            2 => (" ●".to_string(), Rgb::hex(0x9be08a)),
            1 => (" ◌".to_string(), pal::UI_DIM),
            _ => (" ○".to_string(), pal::UI_DIM),
        };
        let x = f.text(x, r.y, &dot, c, bg, 0.0);
        for xx in x..r.x + r.w {
            f.put_g(xx, r.y, Glyph::new(' ', pal::UI_TEXT).bg(bg).lit(0.0));
        }
    }

    pub fn draw(&mut self, f: &mut Frame, cfg: &Config, t: f32) {
        self.draw_panel(f, cfg, t);
        // dressed in the look chosen for the app's own panels
        if let Some(r) = self.rect {
            crate::widgets::looks::apply(f, r, &cfg.ui.look, BG, 11);
        }
    }

    fn draw_panel(&mut self, f: &mut Frame, cfg: &Config, t: f32) {
        self.regions.clear();
        self.hints.clear();
        self.rect = None;
        if !self.open {
            return;
        }
        let (w, h) = (PANEL_W, PANEL_H);
        // centred, unless it's been dragged somewhere (kept on screen)
        let (x, y) = match self.pos {
            Some((px, py)) => (px.clamp(0, (f.w as i32 - w).max(0)), py.clamp(0, (f.h as i32 - h).max(0))),
            None => ((f.w as i32 - w) / 2, ((f.h as i32 - h) / 2).max(0)),
        };
        if self.pos.is_some() {
            self.pos = Some((x, y));
        }
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
        if !self.extras {
            // a grip: drag the title bar to move the panel
            f.text(x + w - 9, y, " ⠿ ", pal::UI_DIM, BG, 0.0);
        }
        let close = Rect { x: x + w - 5, y, w: 3, h: 1 };
        let hot = close.contains(self.hover.0, self.hover.1);
        f.text(close.x, y, " ✕ ", if hot { pal::CANDY_RED * 1.3 } else { pal::UI_TEXT }, BG, 0.0);
        self.regions.push((close, Act::Close));

        // tab bar
        let tabs: Vec<usize> = (0..TABS.len()).filter(|&i| i != TAB_WINDOWS || self.window_fx).collect();
        if !tabs.contains(&self.tab) {
            self.tab = TAB_VILLAGE;
        }
        let mut tx = x + 3;
        // a gap between the tabs, while they fit with one
        let need: i32 = tabs.iter().map(|&i| TABS[i].chars().count() as i32 + 3).sum();
        let gap = (need - 1 <= w - 5) as i32;
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
            tx += r.w + gap;
        }
        for xx in x + 1..x + w - 1 {
            f.put_g(xx, y + 2, Glyph::new('┄', pal::UI_BORDER * 0.45).bg(BG).lit(0.0));
        }

        let lx = x + 3;
        let vx = x + 20;
        let yy = y + 4;
        match self.tab {
            TAB_YOU => self.tab_you(f, cfg, lx, vx, yy, x + w - 1, t),
            TAB_WIDGETS => self.tab_widgets(f, cfg, lx, vx, yy, t),
            TAB_SHELVES => self.tab_shelves(f, cfg, lx, yy, x + w - 1, t),
            TAB_SOUND => self.tab_sound(f, cfg, lx, vx, yy, t),
            TAB_EFFECTS => self.tab_effects(f, cfg, lx, x + 28, yy, t),
            TAB_WINDOWS => self.tab_windows(f, cfg, lx, vx, yy, t),
            TAB_DESKTOP => self.tab_desktop(f, cfg, lx, yy, x + w - 1, t),
            _ => self.tab_village(f, cfg, lx, vx, yy, t),
        }
        // what the setting under the pointer does, in place of the tips
        let (hx, hy) = self.hover;
        let under = self.regions.iter().rev().chain(self.hints.iter()).find(|(r, _)| r.contains(hx, hy)).map(|(_, a)| *a);
        if let Some(text) = under.and_then(tip) {
            let width = (x + w - 1 - lx) as usize;
            let mut lines: Vec<String> = vec![String::new()];
            for word in text.split(' ') {
                let cur = lines.last_mut().unwrap();
                if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width - 2 {
                    lines.push(word.to_string());
                } else {
                    if !cur.is_empty() {
                        cur.push(' ');
                    }
                    cur.push_str(word);
                }
            }
            for (k, l) in lines.iter().take(2).enumerate() {
                let yy = y + h - 3 + k as i32;
                for xx in lx..x + w - 1 {
                    f.put_g(xx, yy, Glyph::new(' ', pal::UI_TEXT).bg(BG).lit(0.0));
                }
                if k == 0 {
                    f.put_g(lx, yy, Glyph::new('✦', Rgb::hex(0xffd36b)).bg(BG).lit(0.0));
                }
                f.text(lx + 2, yy, l, pal::UI_TEXT * 0.95, BG, 0.0);
            }
            return;
        }

        let tips = [
            "point at a setting to see what it does  ·  click or drag along a slider",
            if self.standalone {
                "changes reach the wallpaper straight away  ·  Esc closes this window"
            } else if self.extras {
                "drag widgets to move them  ·  hold a click for a right-click menu"
            } else {
                // (≤ 72 cells: the inner width from lx to the right border)
                "drag widgets to move them  ·  drag the title bar to move this panel"
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

    fn tab_village(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        let pct = |v: f32, hi: f32| v / hi;
        self.section(f, lx, yy, "THE VILLAGE");
        yy += 1;
        self.slider(f, lx, vx, yy, "Snowfall", F::Snow, pct(cfg.scene.snow, 3.0), &format!("{:.2}×", cfg.scene.snow), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Wind", F::Wind, pct(cfg.scene.wind, 3.0), &format!("{:.2}×", cfg.scene.wind), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Glow", F::Glow, pct(cfg.scene.glow, 2.0), &format!("{:.2}×", cfg.scene.glow), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Surprises", F::Events, pct(cfg.scene.events, 3.0), &format!("{:.2}×", cfg.scene.events), t);
        yy += 1;
        let sl = if cfg.scene.sleigh_minutes <= 0.0 { "never".to_string() } else { format!("every ~{:.0} min", cfg.scene.sleigh_minutes) };
        self.stepper(f, lx, vx, yy, "Santa's sleigh", F::Sleigh, &sl);
        yy += 1;
        let tod = if cfg.scene.time == "auto" { "auto (real sunrise & sunset)".to_string() } else { cfg.scene.time.clone() };
        self.stepper(f, lx, vx, yy, "Time of day", F::TimeOfDay, &tod);
        yy += 1;
        let side = match cfg.scene.traffic.as_str() {
            "left" => "keep left",
            "right" => "keep right",
            _ => "auto (weather city)",
        };
        self.stepper(f, lx, vx, yy, "Traffic", F::Traffic, side);
        yy += 1;
        let cx = self.check(f, lx, yy, "Aurora", Act::Toggle(F::Aurora), cfg.scene.aurora) + 3;
        self.check(f, cx, yy, "Berry bushes", Act::Toggle(F::Bushes), cfg.scene.bushes);
        yy += 2;
        self.section(f, lx, yy, "VISITORS");
        yy += 1;
        self.slider(f, lx, vx, yy, "Animals & people", F::Critters, pct(cfg.scene.critters, 3.0), &format!("{:.2}×", cfg.scene.critters), t);
        yy += 1;
        let most = cfg.scene.most_at_once.max(1);
        self.stepper(f, lx, vx, yy, "Most at once", F::MostAtOnce, &format!("{most} {}", if most == 1 { "visitor" } else { "visitors" }));
        yy += 1;
        f.text(lx, yy, "(friends & family, who visit too, are in the You tab)", pal::UI_DIM, BG, 0.0);
        yy += 2;
        self.button(f, lx, yy, "↻ a new village", Act::Shuffle);
        self.button(f, lx + 19, yy, "⇩ export save", Act::ExportSave);
        self.button(f, lx + 36, yy, "⇧ import save", Act::ImportSave);
        f.text(lx + 53, yy, "(saves itself)", pal::UI_DIM, BG, 0.0);
    }

    fn tab_you(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, right: i32, t: f32) {
        let pct = |v: f32, hi: f32| v / hi;
        self.section(f, lx, yy, "THE VILLAGER");
        yy += 1;
        f.text(lx, yy, "Your name", pal::UI_TEXT, BG, 0.0);
        let editing = self.edit == Some(Field::Name) && self.editing.is_some();
        let txt = if editing { self.editing.clone().unwrap_or_default() } else if cfg.scene.name.is_empty() { "(click to set)".into() } else { cfg.scene.name.clone() };
        self.field(f, Rect { x: vx, y: yy, w: 18, h: 1 }, &txt, editing, Act::Edit(Field::Name), t);
        // their look: click the colours to dress them
        {
            let (coat, hat, hair, _, _) = cfg.scene.wear.wear_colours();
            let (x, y) = (vx + 21, yy);
            let sel = self.friend_sel == Some(YOU_LOOK);
            let r = Rect { x, y, w: 14, h: 1 };
            let hot = r.contains(self.hover.0, self.hover.1);
            f.put_g(x, y, Glyph::new('▀', Rgb::hex(hat.unwrap_or(hair))).bg(Bg::Solid(Rgb::hex(coat))).lit(0.0));
            f.text(x + 2, y, if sel { "your look ◂" } else { "your look ✎" }, if hot || sel { pal::UI_TITLE } else { pal::UI_TEXT }, BG, 0.0);
            self.regions.push((r, Act::FriendSel(YOU_LOOK)));
        }
        self.check(f, vx + 37, yy, "Walk about", Act::Toggle(F::Player), cfg.scene.player);
        yy += 1;
        f.text(lx, yy, "Card's line", pal::UI_TEXT, BG, 0.0);
        self.hints.push((Rect { x: lx, y: yy, w: 11, h: 1 }, Act::Edit(Field::Motto)));
        let editing = self.edit == Some(Field::Motto) && self.editing.is_some();
        let txt = if editing { self.editing.clone().unwrap_or_default() } else if cfg.scene.motto.is_empty() { "(a line friends see on your profile card)".into() } else { cfg.scene.motto.clone() };
        self.field(f, Rect { x: vx, y: yy, w: (right - vx - 2).clamp(18, 48), h: 1 }, &txt, editing, Act::Edit(Field::Motto), t);
        yy += 1;
        f.text(lx, yy, "Autopilot", pal::UI_TEXT, BG, 0.0);
        self.hints.push((Rect { x: lx, y: yy, w: 9, h: 1 }, Act::Toggle(F::Autopilot)));
        self.check(f, vx, yy, "left alone, off doing their own thing", Act::Toggle(F::Autopilot), cfg.scene.autopilot);
        yy += 1;
        self.slider(f, lx, vx, yy, "Walk speed", F::WalkSpeed, pct(cfg.scene.walk_speed, 6.0), &format!("{:.2}×", cfg.scene.walk_speed), t);
        yy += 1;
        self.slider(f, lx, vx, yy, "Speech bubbles", F::TalkTime, pct(cfg.scene.talk_time, 3.0), &format!("{:.2}× as long", cfg.scene.talk_time), t);
        yy += 1;
        let chat = if cfg.scene.chattiness <= 0.0 { "quiet".to_string() } else { format!("{:.2}×", cfg.scene.chattiness) };
        self.slider(f, lx, vx, yy, "Chattiness", F::Chatty, pct(cfg.scene.chattiness, 2.0), &chat, t);
        yy += 2;
        self.section(f, lx, yy, "THE PET");
        yy += 1;
        self.stepper(f, lx, vx, yy, "Pet", F::PetKind, &cfg.scene.pet.kind);
        if cfg.scene.pet.kind != "none" {
            let cx = vx + 13;
            f.text(cx, yy, "colour", pal::UI_TEXT, BG, 0.0);
            self.arrow(f, cx + 7, yy, '◂', Act::Dec(F::PetCoat));
            let c = f.text(cx + 9, yy, &cfg.scene.pet.coat, pal::UI_TITLE, BG, 0.0);
            self.arrow(f, c + 1, yy, '▸', Act::Inc(F::PetCoat));
            yy += 1;
            f.text(lx, yy, "Pet's name", pal::UI_TEXT, BG, 0.0);
            let editing = self.edit == Some(Field::PetName) && self.editing.is_some();
            let txt = if editing { self.editing.clone().unwrap_or_default() } else if cfg.scene.pet.name.is_empty() { "(not named yet)".into() } else { cfg.scene.pet.name.clone() };
            self.field(f, Rect { x: vx, y: yy, w: 18, h: 1 }, &txt, editing, Act::Edit(Field::PetName), t);
        }
        yy += 2;
        self.section(f, lx, yy, "FRIENDS & FAMILY");
        yy += 1;
        f.text(lx, yy, "Each comes by as a villager of their own.", pal::UI_DIM, BG, 0.0);
        self.check(f, right - 22, yy, "only them", Act::Toggle(F::OnlyFriends), cfg.scene.only_friends);
        yy += 1;
        // two columns of five
        let colw = ((right - lx) / 2).max(30);
        let slot = |k: usize| (lx + (k / 5) as i32 * colw, yy + (k % 5) as i32);
        let n = cfg.scene.friends.len().min(MAX_FRIENDS);
        for (k, fr) in cfg.scene.friends.iter().take(MAX_FRIENDS).enumerate() {
            let (x, y) = slot(k);
            let k8 = k as u8;
            // their colours (hat or hair over coat): click to dress them
            let (coat, hat, hair, _) = fr.colours();
            let sel = self.friend_sel == Some(k8);
            let r = Rect { x, y, w: 2, h: 1 };
            let hot = r.contains(self.hover.0, self.hover.1);
            f.put_g(x, y, Glyph::new('▀', Rgb::hex(hat.unwrap_or(hair))).bg(Bg::Solid(Rgb::hex(coat))).lit(0.0));
            f.put_g(x + 1, y, Glyph::new(if sel { '◂' } else if hot { '✎' } else { ' ' }, Rgb::hex(0xffd36b)).bg(BG).lit(0.0));
            self.regions.push((r, Act::FriendSel(k8)));
            let editing = self.edit == Some(Field::Friend(k8)) && self.editing.is_some();
            let txt = if editing { self.editing.clone().unwrap_or_default() } else { fr.name.clone() };
            self.field(f, Rect { x: x + 2, y, w: 13, h: 1 }, &txt, editing, Act::Edit(Field::Friend(k8)), t);
            let age = if fr.kid { "kid     " } else { "grown-up" };
            self.button(f, x + 16, y, age, Act::FriendKid(k8));
            self.button(f, x + 27, y, "✕", Act::FriendRemove(k8));
        }
        if n < MAX_FRIENDS {
            let (x, y) = slot(n);
            if self.edit == Some(Field::NewFriend) && self.editing.is_some() {
                f.put_g(x, y, Glyph::new('♥', Rgb::hex(0xff7a9a)).bg(BG).lit(0.0));
                let txt = self.editing.clone().unwrap_or_default();
                self.field(f, Rect { x: x + 2, y, w: 13, h: 1 }, &txt, true, Act::Edit(Field::NewFriend), t);
                f.text(x + 16, y, "⏎ add", pal::UI_DIM, BG, 0.0);
            } else {
                self.button(f, x, y, "+ add someone", Act::Edit(Field::NewFriend));
            }
        }
        // the chosen friend's look, and how they'll appear
        let yy = yy + 7;
        if self.friend_sel == Some(YOU_LOOK) {
            let w = &cfg.scene.wear;
            f.text(lx, yy, "Your look", Rgb::hex(0xffd36b), BG, 0.0);
            for (part, label, px, py) in [(0u8, "coat", vx, yy), (1, "hat", vx + 18, yy), (2, "hair", vx, yy + 1), (3, "skin", vx + 18, yy + 1)] {
                let c = f.text(px, py, label, pal::UI_TEXT, BG, 0.0);
                self.arrow(f, c + 1, py, '◂', Act::FriendLook(YOU_LOOK, part, -1));
                let e = f.text(c + 3, py, w.wear_name(part), pal::UI_TITLE, BG, 0.0);
                self.arrow(f, e + 1, py, '▸', Act::FriendLook(YOU_LOOK, part, 1));
            }
            self.check(f, vx + 34, yy, "scarf", Act::FriendScarf(YOU_LOOK), w.scarf);
            f.text(lx, yy + 2, "(friends online see you this way, and on your profile card)", pal::UI_DIM, BG, 0.0);
            let (coat, hat, hair, skin, scarf) = w.wear_colours();
            let look = crate::scene::sprites::Look { kid: false, coat: Rgb::hex(coat), hat: hat.map(Rgb::hex), hair: Rgb::hex(hair), scarf: scarf.map(Rgb::hex), skin: Rgb::hex(skin), mittens: Rgb::hex(scarf.or(hat).unwrap_or(coat)), holding: None, lantern: false };
            let pic = crate::scene::sprites::person(&look, crate::scene::sprites::Pose::Wave, 1.0, 1.0);
            let rows = crate::scene::sprites::rows_of(&pic, 2.5);
            f.sprite((right - 6) as f32, (yy + 2) as f32, rows, pic);
            return;
        }
        let Some(i) = self.friend_sel.filter(|&i| (i as usize) < n) else {
            if n > 0 {
                f.text(lx, yy, "Click the colours by a name to choose how they look.", pal::UI_DIM, BG, 0.0);
            }
            return;
        };
        let fr = &cfg.scene.friends[i as usize];
        let title: String = format!("{}'s look", fr.name).chars().take((vx - lx - 1).max(4) as usize).collect();
        f.text(lx, yy, &title, Rgb::hex(0xffd36b), BG, 0.0);
        for (part, label, px, py) in [(0u8, "coat", vx, yy), (1, "hat", vx + 18, yy), (2, "hair", vx, yy + 1), (3, "skin", vx + 18, yy + 1)] {
            let c = f.text(px, py, label, pal::UI_TEXT, BG, 0.0);
            self.hints.push((Rect { x: px, y: py, w: label.len() as i32, h: 1 }, Act::FriendLook(i, part, 1)));
            self.arrow(f, c + 1, py, '◂', Act::FriendLook(i, part, -1));
            let e = f.text(c + 3, py, fr.part_name(part), pal::UI_TITLE, BG, 0.0);
            self.arrow(f, e + 1, py, '▸', Act::FriendLook(i, part, 1));
        }
        self.check(f, vx + 34, yy, "scarf", Act::FriendScarf(i), fr.scarf);
        // (drawn as they'll be seen, where pictures can be shown)
        let fl = crate::scene::critters::FriendLook::of(fr);
        let look = crate::scene::sprites::Look { kid: fr.kid, coat: fl.coat, hat: fl.hat, hair: fl.hair, scarf: fl.scarf, skin: fl.skin, mittens: fl.hat.unwrap_or(fl.coat * 0.8), holding: None, lantern: false };
        let pic = crate::scene::sprites::person(&look, crate::scene::sprites::Pose::Wave, 1.0, 1.0);
        let rows = crate::scene::sprites::rows_of(&pic, 2.5);
        f.sprite((right - 6) as f32, (yy + 2) as f32, rows, pic);
    }

    fn tab_widgets(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32, t: f32) {
        self.section(f, lx, yy, "CLOCK");
        yy += 1;
        let mut cx = self.check(f, lx, yy, "Show clock", Act::Toggle(F::Clock), cfg.clock.enabled) + 3;
        cx = self.check(f, cx, yy, "24-hour", Act::Toggle(F::Hour24), cfg.clock.hour24) + 3;
        cx = self.check(f, cx, yy, "Seconds", Act::Toggle(F::Seconds), cfg.clock.seconds) + 3;
        self.check(f, cx, yy, "Countdown", Act::Toggle(F::Countdown), cfg.clock.countdown);
        yy += 2;
        self.section(f, lx, yy, "WIDGETS");
        yy += 1;
        let w = &cfg.widgets;
        for (i, label, field, on) in [(1u8, "System", F::System, w.system), (2, "Music", F::Music, w.music), (3, "Weather", F::Weather, w.weather), (4, "Map", F::Map, w.map), (5, "Visualizer", F::Viz, w.visualizer), (6, "Friends", F::FriendsW, w.friends)] {
            self.check(f, lx, yy, label, Act::Toggle(field), on);
            let look = w.looks.get(i as usize).unwrap_or("classic");
            let name = match look {
                "frosted" => "frosted",
                "wood" => "wooden",
                "gift" => "gift box",
                _ => "classic",
            };
            let r = Rect { x: vx, y: yy, w: 4, h: 1 };
            f.text(vx, yy, "look", if on { pal::UI_TEXT } else { pal::UI_DIM }, BG, 0.0);
            self.hints.push((r, Act::Look(i, 1)));
            self.arrow(f, vx + 5, yy, '◂', Act::Look(i, -1));
            let e = f.text(vx + 7, yy, &format!("{name:<8}"), if on { pal::UI_TITLE } else { pal::UI_DIM }, BG, 0.0);
            self.arrow(f, e + 1, yy, '▸', Act::Look(i, 1));
            if i == 5 && on {
                self.check(f, e + 4, yy, "kHz labels", Act::Toggle(F::VizKhz), w.visualizer_khz);
            }
            yy += 1;
        }
        // the app's own panels and menus, in the same looks
        let name = match cfg.ui.look.as_str() {
            "frosted" => "frosted",
            "wood" => "wooden",
            "gift" => "gift box",
            _ => "classic",
        };
        f.text(lx, yy, "Panels & menus", pal::UI_TEXT, BG, 0.0);
        let r = Rect { x: vx, y: yy, w: 4, h: 1 };
        f.text(vx, yy, "look", pal::UI_TEXT, BG, 0.0);
        self.hints.push((r, Act::Look(9, 1)));
        self.arrow(f, vx + 5, yy, '◂', Act::Look(9, -1));
        let e = f.text(vx + 7, yy, &format!("{name:<8}"), pal::UI_TITLE, BG, 0.0);
        self.arrow(f, e + 1, yy, '▸', Act::Look(9, 1));
        f.text(e + 4, yy, "(shelves: Shelves tab)", pal::UI_DIM, BG, 0.0);
        yy += 1;
        let size = if self.ui_layer_ok { format!("{:.0}%", cfg.ui.scale * 100.0) } else { format!("{:.0}% (not in a terminal)", cfg.ui.scale * 100.0) };
        self.stepper(f, lx, vx, yy, "UI size", F::UiScale, &size);
        yy += 2;
        self.section(f, lx, yy, "WEATHER");
        yy += 1;
        f.text(lx, yy, "City", pal::UI_TEXT, BG, 0.0);
        let field = Rect { x: vx, y: yy, w: 30, h: 1 };
        let editing = self.edit == Some(Field::City) && self.editing.is_some();
        let txt = if editing { self.editing.clone().unwrap_or_default() } else if cfg.weather.city.is_empty() { "(click to set)".into() } else { cfg.weather.city.clone() };
        self.field(f, field, &txt, editing, Act::Edit(Field::City), t);
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
    }

    /// Resolution, zoom, frame rate, and playing it in a window.
    fn display_rows(&mut self, f: &mut Frame, cfg: &Config, lx: i32, vx: i32, mut yy: i32) -> i32 {
        let res = format!("{:.2}× detail", resolution(cfg.scale));
        self.stepper(f, lx, vx, yy, "Resolution", F::Scale, &res);
        let zx = vx + 22;
        let r = Rect { x: zx, y: yy, w: 4, h: 1 };
        f.text(zx, yy, "Zoom", pal::UI_TEXT, BG, 0.0);
        self.hints.push((r, Act::Inc(F::Zoom)));
        self.arrow(f, zx + 5, yy, '◂', Act::Dec(F::Zoom));
        let c = f.text(zx + 7, yy, &format!("{:.2}×", cfg.zoom), pal::UI_TITLE, BG, 0.0);
        self.arrow(f, c + 1, yy, '▸', Act::Inc(F::Zoom));
        yy += 1;
        let fps = match self.fps_override {
            Some(o) => format!("{o:.0} fps (--fps)"),
            None => format!("{:.0} fps", cfg.fps),
        };
        self.stepper(f, lx, vx, yy, "Frame rate", F::Fps, &fps);
        yy += 1;
        // play it like a game: in a window of its own, or full screen
        if !self.standalone {
            let bx = vx;
            if self.windowed {
                self.button(f, bx, yy, "▣ full screen (F11)", Act::FullScreen);
            } else if self.wall_full {
                self.button(f, bx, yy, "▢ back to wallpaper (F11)", Act::BackToWallpaper);
            } else {
                let nx = bx + "▸ pop out".chars().count() as i32 + 3;
                self.button(f, bx, yy, "▸ pop out", Act::PopOut);
                self.button(f, nx, yy, "▣ full screen (F11)", Act::FullScreen);
            }
            f.text(lx, yy, "Play", pal::UI_TEXT, BG, 0.0);
            yy += 1;
        }
        yy
    }

    /// The wallpaper itself: setting it up (Linux), the lock screen (KDE),
    /// starting with the computer, updates.
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn tab_desktop(&mut self, f: &mut Frame, cfg: &Config, lx: i32, mut yy: i32, right: i32, t: f32) {
        let width = (right - lx - 1).max(10) as usize;
        #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
        let clip = |s: &str| -> String { s.chars().take(width).collect() };
        self.section(f, lx, yy, "WALLPAPER");
        yy += 1;
        #[cfg(target_os = "linux")]
        {
            use crate::linwall::Job;
            let info = crate::linwall::info();
            let spin = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'][(t * 10.0) as usize % 10];
            match crate::linwall::wall_job() {
                Job::Busy(m) => {
                    f.text(lx, yy, &clip(&format!("{spin} {m}")), pal::UI_TITLE, BG, 0.0);
                    yy += 1;
                }
                job => {
                    if info.running {
                        f.text(lx, yy, "the village is the desktop wallpaper ✓", pal::UI_TEXT, BG, 0.0);
                        yy += 1;
                    } else if info.known {
                        self.button(f, lx, yy, "✦ set as the desktop wallpaper", Act::SetWallpaper);
                        yy += 1;
                        if let Some(n) = &info.needs {
                            f.text(lx + 2, yy, &clip(n), pal::UI_DIM, BG, 0.0);
                            yy += 1;
                        }
                    } else {
                        f.text(lx, yy, &format!("{spin} looking…"), pal::UI_DIM, BG, 0.0);
                        yy += 1;
                    }
                    match job {
                        Job::Failed(e) => {
                            f.text(lx, yy, &clip(&format!("✗ {e}")), pal::CANDY_RED * 1.3, BG, 0.0);
                            yy += 1;
                        }
                        Job::Done(m) if !info.running => {
                            f.text(lx, yy, &clip(&m), pal::UI_DIM, BG, 0.0);
                            yy += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut cx = lx;
        if let Some(on) = self.autostart {
            cx = self.check(f, lx, yy, "Run at startup", Act::Autostart, on) + 4;
        }
        if self.extras {
            self.button(f, cx, yy, "✕ quit wallpaper", Act::Quit);
        }
        if self.autostart.is_some() || self.extras {
            yy += 1;
        }
        yy += 1;
        self.section(f, lx, yy, "DISPLAY");
        yy += 1;
        yy = self.display_rows(f, cfg, lx, lx + 17, yy);
        #[cfg(target_os = "linux")]
        {
            use crate::linwall::Job;
            let info = crate::linwall::info();
            if info.lock_ok {
                yy += 1;
                self.section(f, lx, yy, "LOCK SCREEN");
                yy += 1;
                match crate::linwall::lock_job() {
                    Job::Busy(m) => {
                        let spin = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'][(t * 10.0) as usize % 10];
                        f.text(lx, yy, &clip(&format!("{spin} {m}")), pal::UI_TITLE, BG, 0.0);
                    }
                    job => {
                        let cx = self.check(f, lx, yy, "The village on the lock screen", Act::LockScreen, info.lock_on) + 3;
                        if info.lock_on {
                            self.button(f, cx, yy, "⟲ redraw it", Act::LockRedraw);
                        }
                        match job {
                            Job::Failed(e) => {
                                yy += 1;
                                f.text(lx, yy, &clip(&format!("✗ {e}")), pal::CANDY_RED * 1.3, BG, 0.0);
                            }
                            Job::Done(m) => {
                                yy += 1;
                                f.text(lx, yy, &clip(&m), pal::UI_DIM, BG, 0.0);
                            }
                            _ => {}
                        }
                    }
                }
                yy += 1;
                let note = if info.ffmpeg { "a looping video of the village (KDE Plasma's lock screen)" } else { "a still of the village (with ffmpeg installed, it moves)" };
                f.text(lx, yy, &clip(note), pal::UI_DIM, BG, 0.0);
                yy += 1;
                if !info.ffmpeg {
                    self.button(f, lx, yy, "⇩ install ffmpeg (asks for your password)", Act::InstallFfmpeg);
                    yy += 1;
                }
            }
        }
        yy += 1;
        self.section(f, lx, yy, "UPDATES");
        yy += 1;
        let cx = self.check(f, lx, yy, "Check for updates", Act::Toggle(F::UpdCheck), cfg.updates.check) + 3;
        self.check(f, cx, yy, "Install automatically", Act::Toggle(F::UpdAuto), cfg.updates.auto_install);
        yy += 1;
        use crate::update::Status;
        let ver = crate::update::current();
        // in the tray's settings window, the wallpaper (another process)
        // shares its status and takes requests
        let local = crate::update::status();
        let st = if crate::update::running() || matches!(local, Status::Unsupported | Status::Managed) { Some(local) } else { crate::update::remote_status() };
        let (msg, act) = match st {
            None => (format!("v{ver} · start the wallpaper to check for updates"), None),
            Some(Status::Unsupported) => (format!("v{ver} · built from source: updates come from git"), None),
            Some(Status::Managed) => (format!("v{ver} · system-wide install: update via the package manager"), None),
            Some(Status::Idle) => (format!("v{ver}"), Some(("check now", Act::CheckUpdate))),
            Some(Status::Checking) => (format!("v{ver} · checking…"), None),
            Some(Status::UpToDate) => (format!("v{ver} · up to date ✓"), Some(("check now", Act::CheckUpdate))),
            Some(Status::Available(v)) => (format!("v{ver} · {v} is available"), Some(("⬇ install", Act::InstallUpdate))),
            Some(Status::Installing(v)) => (format!("installing {v}…"), None),
            Some(Status::Installed(v)) => (format!("{v} installed · restarting…"), None),
            Some(Status::Failed(e)) => (format!("update: {e}"), Some(("retry", Act::CheckUpdate))),
        };
        let msg: String = msg.chars().take(58).collect();
        let cx = f.text(lx, yy, &msg, pal::UI_DIM, BG, 0.0);
        if let Some((label, a)) = act {
            self.button(f, cx + 2, yy, label, a);
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

    fn tab_shelves(&mut self, f: &mut Frame, cfg: &Config, lx: i32, mut yy: i32, right: i32, t: f32) {
        self.section(f, lx, yy, "SHELVES");
        yy += 1;
        let tip = if cfg!(windows) {
            "Each shows a folder. Drag desktop icons onto one to tidy them away."
        } else {
            "Each shows a folder: move files and app shortcuts into it."
        };
        f.text(lx, yy, tip, pal::UI_DIM, BG, 0.0);
        yy += 2;
        let room = (right - lx - 1).max(10) as usize;
        for (i, c) in cfg.shelves.iter().enumerate().take(5) {
            let i8 = i as u8;
            let editing = self.edit == Some(Field::Shelf(i8)) && self.editing.is_some();
            let txt = if editing { self.editing.clone().unwrap_or_default() } else { c.name.clone() };
            self.field(f, Rect { x: lx, y: yy, w: 14, h: 1 }, &txt, editing, Act::Edit(Field::Shelf(i8)), t);
            let style = match c.style.as_str() {
                "wood" => "wooden",
                "gift" => "gift box",
                _ => "frosted",
            };
            let mut cx = lx + 16;
            self.arrow(f, cx, yy, '◂', Act::ShelfStyle(i8, -1));
            let e = f.text(cx + 2, yy, &format!("{style:<8}"), pal::UI_TITLE, BG, 0.0);
            self.arrow(f, e + 1, yy, '▸', Act::ShelfStyle(i8, 1));
            cx = e + 4;
            let icons = if c.icons == "real" { "real icons" } else { "pixel art" };
            self.button(f, cx, yy, icons, Act::ShelfIcons(i8));
            cx += icons.chars().count() as i32 + 3;
            self.button(f, cx, yy, "open", Act::ShelfOpen(i8));
            self.button(f, right - 4, yy, "✕", Act::ShelfRemove(i8));
            yy += 1;
            let folder = crate::widgets::shelf::folder_of(c);
            let shown = folder.to_string_lossy().replace(&*crate::widgets::shelf::home_dir().to_string_lossy(), "~");
            let shown: String = if shown.chars().count() > room { format!("…{}", shown.chars().rev().take(room - 1).collect::<Vec<_>>().into_iter().rev().collect::<String>()) } else { shown };
            let room = room.saturating_sub(11);
            let shown: String = if shown.chars().count() > room { format!("…{}", shown.chars().rev().take(room.saturating_sub(1)).collect::<Vec<_>>().into_iter().rev().collect::<String>()) } else { shown };
            let e = f.text(lx + 1, yy, &shown, pal::UI_DIM * 0.9, BG, 0.0);
            self.button(f, e + 1, yy, "folder…", Act::ShelfFolder(i8));
            yy += 2;
        }
        if cfg.shelves.len() > 5 {
            f.text(lx, yy, &format!("…and {} more (in the config file)", cfg.shelves.len() - 5), pal::UI_DIM, BG, 0.0);
            yy += 2;
        }
        self.button(f, lx, yy, "+ new shelf", Act::ShelfNew);
        self.button(f, lx + 15, yy, "+ from a folder…", Act::ShelfNewFrom);
        if cfg.shelves.is_empty() {
            f.text(lx, yy + 1, "(or right-click the desktop: new shelf here)", pal::UI_DIM, BG, 0.0);
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

    /// A one-line text field (a click edits it).
    fn field(&mut self, f: &mut Frame, r: Rect, txt: &str, editing: bool, act: Act, t: f32) {
        for i in 0..r.w {
            let fg = if editing { pal::UI_TITLE } else { pal::UI_TEXT };
            let bgc = if editing { Rgb::hex(0x24305a) } else { Rgb::hex(0x1a2242) };
            let ch = txt.chars().nth(i as usize).unwrap_or(' ');
            f.put_g(r.x + i, r.y, Glyph::new(ch, fg).solid(bgc).lit(0.0));
        }
        if editing && (t * 2.0) as i32 % 2 == 0 {
            let cx = r.x + txt.chars().count().min(r.w as usize - 1) as i32;
            f.put_g(cx, r.y, Glyph::new('▏', Rgb::hex(0xffd36b)).solid(Rgb::hex(0x1a2240)).lit(0.0));
        }
        self.regions.push((r, act));
    }

    fn section(&self, f: &mut Frame, x: i32, y: i32, s: &str) {
        let c = f.text(x, y, s, Rgb::hex(0xffd36b), BG, 0.0);
        // a rule along to the panel's right side
        let end = self.rect.map(|r| r.x + r.w - 3).unwrap_or(x + 60);
        for xx in c + 1..end {
            f.put_g(xx, y, Glyph::new('┄', pal::UI_BORDER * 0.45).bg(BG).lit(0.0));
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
        self.hints.push((Rect { x: lx, y, w: (vx - lx - 1).max(1), h: 1 }, Act::Inc(field)));
        self.arrow(f, vx, y, '◂', Act::Dec(field));
        let bw = 20;
        // click (or drag) anywhere along the bar
        for i in 0..bw {
            self.regions.push((Rect { x: vx + 2 + i, y, w: 1, h: 1 }, Act::Set(field, i as f32 / (bw - 1) as f32)));
        }
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
        self.hints.push((Rect { x: lx, y, w: (vx - lx - 1).max(1), h: 1 }, Act::Inc(field)));
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

    /// The text on row y of a frame.
    fn row(f: &Frame, y: i32) -> String {
        (0..f.w as i32).map(|x| f.get_g(x, y).map(|g| g.ch).unwrap_or(' ')).collect()
    }

    #[test]
    fn pointing_at_a_setting_says_what_it_does_and_its_bar_can_be_clicked() {
        let mut cfg = Config::default();
        let mut s = Settings { open: true, ..Default::default() };
        let mut f = Frame::new(200, 50);
        s.draw(&mut f, &cfg, 0.0);
        let r = s.rect.unwrap();
        // the Snowfall row: its label, then its bar
        let y = (0..f.h as i32).find(|&y| row(&f, y).contains("Snowfall")).unwrap();
        let lx = row(&f, y).find("Snowfall").unwrap() as i32;
        s.hover = (lx + 2, y);
        s.draw(&mut f, &cfg, 0.0);
        assert!(row(&f, r.y + r.h - 3).contains("How much snow falls"), "{}", row(&f, r.y + r.h - 3));
        // the end of its bar: all the way up
        let end = s.regions.iter().filter_map(|(rr, a)| matches!(a, Act::Set(F::Snow, _)).then_some(*rr)).max_by_key(|rr| rr.x).unwrap();
        let a = s.click(end.x, end.y).unwrap();
        assert!(apply(&mut cfg, a));
        assert_eq!(cfg.scene.snow, 3.0);
        // and dragged off the start: none
        s.slider_drag = Some(F::Snow);
        let a = s.drag_slider(r.x, end.y).unwrap();
        assert!(apply(&mut cfg, a));
        assert_eq!(cfg.scene.snow, 0.0);
    }

    #[test]
    fn friends_are_added_renamed_and_removed_through_their_fields() {
        let mut cfg = Config::default();
        let mut s = Settings::default();
        s.begin_edit(&mut cfg, Field::NewFriend);
        s.editing = Some("  Sam ".into());
        s.begin_edit(&mut cfg, Field::NewFriend);
        // (starting another field puts the first one in place)
        s.begin_edit(&mut cfg, Field::City);
        assert_eq!(cfg.scene.friends.len(), 1);
        assert_eq!(cfg.scene.friends[0].name, "Sam");
        s.cancel_edit();
        assert!(apply(&mut cfg, Act::FriendKid(0)));
        assert!(cfg.scene.friends[0].kid);
        // dressed: each part steps round its list, the scarf goes on
        let coat = cfg.scene.friends[0].part_name(0);
        assert!(apply(&mut cfg, Act::FriendLook(0, 0, 1)));
        assert_ne!(cfg.scene.friends[0].part_name(0), coat);
        assert!(apply(&mut cfg, Act::FriendLook(0, 0, -1)));
        assert_eq!(cfg.scene.friends[0].part_name(0), coat);
        for _ in 0..crate::config::FRIEND_HATS.len() {
            if cfg.scene.friends[0].hat == "none" {
                break;
            }
            apply(&mut cfg, Act::FriendLook(0, 1, 1));
        }
        assert_eq!(cfg.scene.friends[0].colours().1, None, "bare-headed");
        assert!(apply(&mut cfg, Act::FriendScarf(0)));
        assert!(cfg.scene.friends[0].scarf);
        let saved: Config = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(saved.scene.friends, cfg.scene.friends, "kept in the config file");
        assert!(!apply(&mut cfg, Act::FriendLook(5, 0, 1)));
        s.begin_edit(&mut cfg, Field::Friend(0));
        assert_eq!(s.editing.as_deref(), Some("Sam"));
        s.editing = Some(String::new());
        s.commit(&mut cfg);
        assert!(cfg.scene.friends.is_empty(), "cleared: off the list");
        assert!(!apply(&mut cfg, Act::FriendRemove(0)));
    }

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
