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

/// The save, as text (the page keeps it in the browser): its length.
#[no_mangle]
pub extern "C" fn save(w: *mut Web) -> usize {
    let w = unsafe { &mut *w };
    w.text = w.game.save_text().into_bytes();
    w.text.len()
}

/// Where the text buffer is (after `menu`, `todo`, `pick`, `save`).
#[no_mangle]
pub extern "C" fn text_ptr(w: *mut Web) -> *const u8 {
    unsafe { &*w }.text.as_ptr()
}
