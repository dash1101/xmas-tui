//! The village in a web page (built for WebAssembly). The scene, the frame
//! it's drawn into, the cell renderer, the save file, and the Android app's
//! game driver (joystick, buttons, touches, menus) are the same source files
//! as the desktop's and the app's, included below by path. The page
//! (site/xmas.js) calls the plain functions at the bottom: no bindings
//! generator, just numbers and a shared buffer for text.

#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_assignments, clippy::all)]

#[path = "../../src/audio/mod.rs"]
pub mod audio;
#[path = "../../src/color.rs"]
pub mod color;
#[path = "../../src/config.rs"]
pub mod config;
#[path = "../../src/frame.rs"]
pub mod frame;
#[path = "../../src/raster.rs"]
pub mod raster;
#[path = "../../src/rng.rs"]
pub mod rng;
#[path = "../../src/scene/mod.rs"]
pub mod scene;
#[path = "../../src/save.rs"]
pub mod save;
#[path = "../../src/net/mod.rs"]
pub mod net;
#[path = "../../src/sync.rs"]
pub mod sync;
#[path = "../../src/settings.rs"]
pub mod settings;
#[path = "../../android/native/src/stubs.rs"]
mod stubs;
pub use stubs::{platform, update};
#[path = "../../android/native/src/widgets.rs"]
pub mod widgets;
#[path = "../../android/native/src/game.rs"]
mod game;

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

/// The time, from the page: the browser has the clock and the time zone.
pub mod clock {
    use chrono::{DateTime, Local, TimeZone};
    pub(crate) static LOCAL_MS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

    /// Local wall-clock time (kept as if it were UTC: there's no time zone
    /// database here, and none is needed).
    pub fn now() -> DateTime<Local> {
        let ms = LOCAL_MS.load(std::sync::atomic::Ordering::Relaxed);
        Local.timestamp_millis_opt(ms).single().unwrap_or_else(|| Local.timestamp_millis_opt(0).unwrap())
    }
}

/// What the shared files expect of the desktop app.
pub mod app {
    pub(crate) static SEED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    pub fn random_seed() -> u32 {
        let s = SEED.load(std::sync::atomic::Ordering::Relaxed);
        crate::rng::hash(s) | 1
    }
}

/// A village in the page, its picture (RGBA) and a buffer for text both
/// ways (the config and save in; menus and the save out).
pub struct Web {
    game: game::Game,
    rgba: Vec<u8>,
    text: Vec<u8>,
}

/// Room for `n` bytes of text from the page (the buffer's address).
#[no_mangle]
pub extern "C" fn text_buf(w: *mut Web, n: usize) -> *mut u8 {
    static mut SCRATCH: Vec<u8> = Vec::new();
    unsafe {
        if w.is_null() {
            let s = &mut *std::ptr::addr_of_mut!(SCRATCH);
            s.resize(n, 0);
            return s.as_mut_ptr();
        }
        let w = &mut *w;
        w.text.resize(n, 0);
        w.text.as_mut_ptr()
    }
}

/// A village `w`×`h` pixels; the config (TOML) and the save (TOML, or
/// empty) are in the scratch text buffer as "config\u{0}save".
#[no_mangle]
pub extern "C" fn create(w: u32, h: u32, size: f32, seed: u32, local_ms: f64, len: usize) -> *mut Web {
    clock::LOCAL_MS.store(local_ms as i64, Ordering::Relaxed);
    app::SEED.store(seed, Ordering::Relaxed);
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    let (cfg_text, save_text) = text.split_once('\u{0}').unwrap_or((&text, ""));
    let cfg: config::Config = toml::from_str(cfg_text).unwrap_or_default();
    let saved: Option<save::Save> = toml::from_str(save_text).ok().filter(|s: &save::Save| s.seed != 0);
    let game = game::Game::new_saved(w.max(64) as usize, h.max(64) as usize, size, cfg, saved, None);
    let n = (w.max(64) * h.max(64) * 4) as usize;
    Box::into_raw(Box::new(Web { game, rgba: vec![255; n], text: vec![] }))
}

/// New settings (the config text in the scratch buffer), taken in where
/// things stand: no new village, still online, still in a friend's.
#[no_mangle]
pub extern "C" fn configure(w: *mut Web, len: usize, size: f32) {
    let w = unsafe { &mut *w };
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    if let Ok(cfg) = toml::from_str::<config::Config>(&text) {
        w.game.configure(cfg, size);
    }
}

/// The page resized (or a phone turned): the village keeps going.
#[no_mangle]
pub extern "C" fn resize(w: *mut Web, pw: u32, ph: u32, size: f32) {
    let w = unsafe { &mut *w };
    let (pw, ph) = (pw.max(64), ph.max(64));
    w.game.resize(pw as usize, ph as usize, size);
    w.rgba = vec![255; (pw * ph * 4) as usize];
}

#[no_mangle]
pub extern "C" fn destroy(w: *mut Web) {
    if !w.is_null() {
        unsafe { drop(Box::from_raw(w)) };
    }
}

/// One frame: the village moves on `dt` seconds and is drawn; the picture's
/// address (RGBA, w×h).
#[no_mangle]
pub extern "C" fn frame(w: *mut Web, dt: f32, local_ms: f64) -> *const u8 {
    clock::LOCAL_MS.store(local_ms as i64, Ordering::Relaxed);
    let w = unsafe { &mut *w };
    w.game.frame(dt);
    for (px, out) in w.game.fb.iter().zip(w.rgba.chunks_exact_mut(4)) {
        out[0] = (px >> 16) as u8;
        out[1] = (px >> 8) as u8;
        out[2] = *px as u8;
    }
    w.rgba.as_ptr()
}

#[no_mangle]
pub extern "C" fn steer(w: *mut Web, mx: f32, my: f32, run: u32) {
    unsafe { &mut *w }.game.steer(mx, my, run != 0);
}

#[no_mangle]
pub extern "C" fn button(w: *mut Web, b: i32) {
    unsafe { &mut *w }.game.button(b);
}

/// A finger (or the mouse) on the village: 0 down, 1 moving, 2 up, 3 cancel.
#[no_mangle]
pub extern "C" fn touch(w: *mut Web, action: i32, x: f32, y: f32) {
    unsafe { &mut *w }.game.touch(action, x, y);
}

#[no_mangle]
pub extern "C" fn pan(w: *mut Web, dx: f32) {
    unsafe { &mut *w }.game.pan(dx);
}

/// A list into the text buffer ("title\nchoice\n…"): its length (0: none).
fn put_str(w: &mut Web, s: &str) -> usize {
    w.text = s.as_bytes().to_vec();
    w.text.len()
}

fn put_list(w: &mut Web, m: Option<(String, Vec<String>)>) -> usize {
    let Some((title, items)) = m else { return 0 };
    let s = std::iter::once(title).chain(items).collect::<Vec<_>>().join("\n");
    w.text = s.into_bytes();
    w.text.len()
}

/// What can be done at (x, y) (a long press, a right-click).
#[no_mangle]
pub extern "C" fn menu(w: *mut Web, x: f32, y: f32) -> usize {
    let w = unsafe { &mut *w };
    let m = w.game.menu(x, y);
    put_list(w, m)
}

/// Things to do right here.
#[no_mangle]
pub extern "C" fn todo(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    let m = w.game.todo();
    put_list(w, m)
}

/// A choice from the list shown: 0, or a further list's length.
#[no_mangle]
pub extern "C" fn pick(w: *mut Web, i: u32) -> usize {
    let w = unsafe { &mut *w };
    let m = w.game.pick(i as usize);
    put_list(w, m)
}

/// Where the villager is, in a few words (for the page's title).
#[no_mangle]
pub extern "C" fn place(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    let p = w.game.place();
    put_str(w, &p)
}

/// The talk line (/).
#[no_mangle]
pub extern "C" fn chat(w: *mut Web) {
    unsafe { &mut *w }.game.chat();
}

/// The ☰ menu, in the village.
#[no_mangle]
pub extern "C" fn more(w: *mut Web) {
    unsafe { &mut *w }.game.more(false);
}

/// The controls card (`keys`: the keyboard's; else the touch controls').
#[no_mangle]
pub extern "C" fn help(w: *mut Web, keys: u32) {
    unsafe { &mut *w }.game.help(keys != 0);
}

/// Back (Esc): whether it closed something.
#[no_mangle]
pub extern "C" fn back(w: *mut Web) -> u32 {
    unsafe { &mut *w }.game.back() as u32
}

/// Whether a menu or card is open in the village.
#[no_mangle]
pub extern "C" fn menu_open(w: *mut Web) -> u32 {
    unsafe { &*w }.game.menu_open() as u32
}

/// The pointer over the village (not pressed).
#[no_mangle]
pub extern "C" fn hover(w: *mut Web, x: f32, y: f32) {
    unsafe { &mut *w }.game.hover(x, y);
}

/// Something the page is asked to do (from the village's menus) into the
/// text buffer ("settings"): its length (0: nothing).
#[no_mangle]
pub extern "C" fn ask(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    w.text = w.game.take_ask().unwrap_or_default().into_bytes();
    w.text.len()
}

/// The line typed for what was asked (in the scratch buffer, `len` bytes).
#[no_mangle]
pub extern "C" fn give_text(w: *mut Web, len: usize) {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    unsafe { &mut *w }.game.give_text(&String::from_utf8_lossy(&bytes));
}

/// Randomness from the page (its crypto), for codes and sealing.
#[no_mangle]
pub extern "C" fn entropy(a: u32, b: u32, c: u32, d: u32) {
    net::seal::stir((a as u64) << 32 | b as u64);
    net::seal::stir((c as u64) << 32 | d as u64);
}

/// A preview (the page's `?preview=…`: a look at something, e.g. "chats").
#[no_mangle]
pub extern "C" fn preview(w: *mut Web, len: usize) {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    unsafe { &mut *w }.game.preview(&String::from_utf8_lossy(&bytes));
}

/// Friends & chats (the ✉ button).
#[no_mangle]
pub extern "C" fn friends(w: *mut Web) {
    unsafe { &mut *w }.game.friends();
}

/// Friends & online, as a menu in the village (O).
#[no_mangle]
pub extern "C" fn online(w: *mut Web) {
    unsafe { &mut *w }.game.online_menu();
}

/// Off the relay politely (the page closing): its last bytes to send.
#[no_mangle]
pub extern "C" fn net_bye(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    w.game.scene.online.goodbye();
    w.text = w.game.scene.online.take_out();
    w.text.len()
}

/// Whether the village wants to be online (the page opens a WebSocket): 0
/// no, 1 yes, 2 yes, with a new link (another friend code).
#[no_mangle]
pub extern "C" fn net_want(w: *mut Web) -> u32 {
    let o = &mut unsafe { &mut *w }.game.scene.online;
    // (too old for the relay: not connected anywhere till the page is
    // reloaded with the new version)
    if !o.wanted() || o.too_old() {
        return 0;
    }
    if o.take_relink() {
        2
    } else {
        1
    }
}

/// Y or N: what someone online asked, answered (1 if there was a question).
#[no_mangle]
pub extern "C" fn answer(w: *mut Web, yes: u32) -> u32 {
    unsafe { &mut *w }.game.answer(yes != 0) as u32
}

/// The other villagers kept in the browser: their names (JSON, `len` bytes
/// in the scratch buffer).
#[no_mangle]
pub extern "C" fn set_villagers(w: *mut Web, len: usize) {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    let names: Vec<String> = serde_json::from_slice(&bytes).unwrap_or_default();
    unsafe { &mut *w }.game.set_villagers(names);
}

/// The relay's open but going nowhere (no answer to the hello in a good
/// while, or it turned us down): the page drops it for the next one.
#[no_mangle]
pub extern "C" fn net_stuck(w: *mut Web) -> u32 {
    unsafe { &*w }.game.scene.online.link_stuck() as u32
}

/// The page's WebSocket to the relay: 1 open, 0 closed.
#[no_mangle]
pub extern "C" fn net_link(w: *mut Web, up: u32) {
    let o = &mut unsafe { &mut *w }.game.scene.online;
    if up != 0 {
        // (2: through the village's own relay: what it says as itself is believed)
        o.set_trusted(up == 2);
        o.connected();
    } else {
        o.disconnected();
    }
}

/// Bytes from the relay (in the scratch buffer, `len` of them).
#[no_mangle]
pub extern "C" fn net_in(w: *mut Web, len: usize) {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    unsafe { &mut *w }.game.scene.online.feed(&bytes);
}

/// Bytes for the relay, into the text buffer: how many.
#[no_mangle]
pub extern "C" fn net_out(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    w.text = w.game.scene.online.take_out();
    w.text.len()
}

/// The next `n` frames of sound (48 kHz stereo, interleaved f32): where
/// they are (2n floats).
#[no_mangle]
pub extern "C" fn audio(w: *mut Web, n: u32) -> *const f32 {
    unsafe { &mut *w }.game.audio_pull(n as usize).as_ptr()
}

/// The sound's settings: on or off, and the volumes (0..1).
#[no_mangle]
pub extern "C" fn sound(w: *mut Web, on: u32, master: f32, music: f32, sfx: f32, ambience: f32) {
    let g = &mut unsafe { &mut *w }.game;
    let cfg = config::SoundCfg { enabled: on != 0, master, music, sfx, ambience, ..Default::default() };
    g.set_sound(&cfg);
}

/// The save, as text (the page keeps it in the browser): its length.
#[no_mangle]
pub extern "C" fn save(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    w.text = w.game.save_text().into_bytes();
    w.text.len()
}

/// The villager in a save (its text in the scratch buffer, `len` bytes),
/// as JSON into the text buffer (an import: their name, pet, friends):
/// its length (0: none in it).
#[no_mangle]
pub extern "C" fn you_json(w: *mut Web, len: usize) -> usize {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    let w = unsafe { &mut *w };
    let save: Option<save::Save> = toml::from_str(&String::from_utf8_lossy(&bytes)).ok();
    w.text = save.and_then(|s| s.you).and_then(|y| serde_json::to_string(&y).ok()).unwrap_or_default().into_bytes();
    w.text.len()
}

/// Where the text buffer is (after `menu`, `todo`, `pick`, `save`).
#[no_mangle]
pub extern "C" fn text_ptr(w: *mut Web) -> *const u8 {
    unsafe { &*w }.text.as_ptr()
}

/// The menu (home page and settings): open, or shut if it's open.
#[no_mangle]
pub extern "C" fn open_panel(w: *mut Web) {
    unsafe { &mut *w }.game.open_panel();
}

/// The mouse wheel at (x, y): whether the menu took it (it scrolls, or
/// steps the value under the pointer).
#[no_mangle]
pub extern "C" fn panel_wheel(w: *mut Web, x: f32, y: f32, up: u32) -> u32 {
    unsafe { &mut *w }.game.panel_wheel(x, y, up != 0) as u32
}

/// Whether the sound's on.
#[no_mangle]
pub extern "C" fn sound_on(w: *mut Web) -> u32 {
    unsafe { &*w }.game.sound_on() as u32
}

/// Sound on or off (and kept: a "config:" ask follows).
#[no_mangle]
pub extern "C" fn toggle_sound(w: *mut Web) {
    unsafe { &mut *w }.game.toggle_sound();
}

/// Pause the village, or carry on (the page's ⏸, P).
#[no_mangle]
pub extern "C" fn toggle_pause(w: *mut Web) {
    unsafe { &mut *w }.game.toggle_pause();
}

/// Paused (1) or not (0).
#[no_mangle]
pub extern "C" fn paused(w: *mut Web) -> i32 {
    unsafe { &*w }.game.paused() as i32
}

/// How big the page's own buttons should be (the settings' buttons size).
#[no_mangle]
pub extern "C" fn buttons_k(w: *mut Web) -> f32 {
    unsafe { &*w }.game.buttons_k()
}

/// A row of the menus and talk, in the canvas's pixels: the page's own
/// boxes are sized to match.
#[no_mangle]
pub extern "C" fn ui_row_px(w: *mut Web) -> f32 {
    unsafe { &*w }.game.ui_row_px().0
}

/// The menus' size setting (the row above includes it).
#[no_mangle]
pub extern "C" fn ui_menus_k(w: *mut Web) -> f32 {
    unsafe { &*w }.game.ui_row_px().1
}

/// What weather to fetch ("city|f" or "city|c"; empty: none): its length.
#[no_mangle]
pub extern "C" fn weather_want(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    let s = w.game.weather_want();
    put_str(w, &s)
}

/// The weather the page fetched: "place\u{0}json" in the scratch buffer.
#[no_mangle]
pub extern "C" fn weather_in(w: *mut Web, len: usize) {
    let bytes = unsafe {
        let p = text_buf(std::ptr::null_mut(), len.max(1));
        std::slice::from_raw_parts(p, len).to_vec()
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    let (place, json) = text.split_once('\u{0}').unwrap_or(("", &text));
    unsafe { &mut *w }.game.set_weather(json, place);
}

/// Whether the menu's open (the page's touch pad steps out of its way).
#[no_mangle]
pub extern "C" fn panel_open(w: *mut Web) -> u32 {
    unsafe { &*w }.game.panel_open() as u32
}
