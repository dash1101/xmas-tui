//! What the app's Java side calls (class `io.github.dash1101.xmastui.Native`).
//! A game is a handle (a pointer, as a long); its frames are drawn straight
//! into the game screen's surface (an ANativeWindow), and the sound thread's
//! samples are handed to the app's AudioTrack. A panic is logged and caught
//! here, never let out into Java.

use crate::config::Config;
use crate::game::{self, Game};
use jni::objects::{JClass, JObject, JShortArray, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, jobjectArray};
use jni::JNIEnv;
use std::ffi::{c_char, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

#[repr(C)]
struct ANativeWindow {
    _p: [u8; 0],
}

#[repr(C)]
struct WindowBuffer {
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    bits: *mut c_void,
    reserved: [u32; 6],
}

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut ANativeWindow;
    fn ANativeWindow_release(w: *mut ANativeWindow);
    fn ANativeWindow_setBuffersGeometry(w: *mut ANativeWindow, width: i32, height: i32, format: i32) -> i32;
    fn ANativeWindow_lock(w: *mut ANativeWindow, out: *mut WindowBuffer, dirty: *mut c_void) -> i32;
    fn ANativeWindow_unlockAndPost(w: *mut ANativeWindow) -> i32;
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: i32, tag: *const c_char, text: *const c_char) -> i32;
}

const RGBX_8888: i32 = 2;

pub fn log(s: &str) {
    if let (Ok(tag), Ok(text)) = (CString::new("xmas-tui"), CString::new(s.replace('\0', " "))) {
        unsafe { __android_log_write(5, tag.as_ptr(), text.as_ptr()) };
    }
}

struct Win(*mut ANativeWindow);
// (only ever used while holding the handle's lock)
unsafe impl Send for Win {}

struct Handle {
    game: Mutex<Game>,
    win: Mutex<Option<Win>>,
}

fn handle<'a>(h: jlong) -> Option<&'a Handle> {
    (h != 0).then(|| unsafe { &*(h as *const Handle) })
}

fn guard<T>(what: &str, fallback: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_default();
            log(&format!("{what}: panicked: {msg}"));
            fallback
        }
    }
}

fn with_game<T>(h: jlong, what: &str, fallback: T, f: impl FnOnce(&mut Game) -> T) -> T {
    let Some(h) = handle(h) else { return fallback };
    guard(what, fallback, || {
        let mut g = h.game.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    })
}

/// A new village for a w×h px screen; `size` scales the cells (1 = about 18
/// px a row); `cfg` is TOML in the config file's shape (anything left out is
/// the default).
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_create(mut env: JNIEnv, _: JClass, w: jint, h: jint, size: jfloat, cfg: JString) -> jlong {
    let text: String = env.get_string(&cfg).map(|s| s.into()).unwrap_or_default();
    guard("create", 0, || {
        std::panic::set_hook(Box::new(|p| log(&format!("panic: {p}"))));
        let cfg: Config = toml::from_str(&text).unwrap_or_else(|e| {
            log(&format!("config: {e}"));
            Config::default()
        });
        let g = Game::new(w.max(64) as usize, h.max(64) as usize, size, cfg);
        Box::into_raw(Box::new(Handle { game: Mutex::new(g), win: Mutex::new(None) })) as jlong
    })
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_destroy(_: JNIEnv, _: JClass, h: jlong) {
    if h == 0 {
        return;
    }
    guard("destroy", (), || {
        let b = unsafe { Box::from_raw(h as *mut Handle) };
        if let Some(Win(w)) = b.win.lock().unwrap_or_else(|e| e.into_inner()).take() {
            unsafe { ANativeWindow_release(w) };
        }
        drop(b);
    })
}

/// The game screen's surface is ready (or changed): frames go there.
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_attach(env: JNIEnv, _: JClass, h: jlong, surface: JObject) {
    let Some(hd) = handle(h) else { return };
    let raw = env.get_raw();
    guard("attach", (), || {
        let w = unsafe { ANativeWindow_fromSurface(raw, surface.as_raw()) };
        if w.is_null() {
            log("attach: no window");
            return;
        }
        let (pw, ph) = hd.game.lock().unwrap_or_else(|e| e.into_inner()).size();
        unsafe { ANativeWindow_setBuffersGeometry(w, pw as i32, ph as i32, RGBX_8888) };
        if let Some(Win(old)) = hd.win.lock().unwrap_or_else(|e| e.into_inner()).replace(Win(w)) {
            unsafe { ANativeWindow_release(old) };
        }
    })
}

/// The surface is going away: stop drawing to it.
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_detach(_: JNIEnv, _: JClass, h: jlong) {
    let Some(hd) = handle(h) else { return };
    guard("detach", (), || {
        if let Some(Win(w)) = hd.win.lock().unwrap_or_else(|e| e.into_inner()).take() {
            unsafe { ANativeWindow_release(w) };
        }
    })
}

/// A frame: the village moves on `dt` seconds and is drawn onto the surface.
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_frame(_: JNIEnv, _: JClass, h: jlong, dt: jfloat) {
    let Some(hd) = handle(h) else { return };
    guard("frame", (), || {
        let mut g = hd.game.lock().unwrap_or_else(|e| e.into_inner());
        g.frame(dt);
        let win = hd.win.lock().unwrap_or_else(|e| e.into_inner());
        let Some(Win(w)) = win.as_ref() else { return };
        let mut buf = WindowBuffer { width: 0, height: 0, stride: 0, format: 0, bits: std::ptr::null_mut(), reserved: [0; 6] };
        if unsafe { ANativeWindow_lock(*w, &mut buf, std::ptr::null_mut()) } != 0 || buf.bits.is_null() {
            return;
        }
        let (pw, ph) = g.size();
        let (bw, bh, stride) = (buf.width.max(0) as usize, buf.height.max(0) as usize, buf.stride.max(0) as usize);
        let out = unsafe { std::slice::from_raw_parts_mut(buf.bits as *mut u32, stride * bh) };
        // 0x00RRGGBB → the window's R, G, B, X bytes
        for y in 0..bh.min(ph) {
            let src = &g.fb[y * pw..y * pw + pw.min(bw)];
            let dst = &mut out[y * stride..y * stride + pw.min(bw)];
            for (d, &v) in dst.iter_mut().zip(src) {
                *d = 0xff00_0000 | ((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff);
            }
        }
        unsafe { ANativeWindow_unlockAndPost(*w) };
    })
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_steer(_: JNIEnv, _: JClass, h: jlong, mx: jfloat, my: jfloat, run: jboolean) {
    with_game(h, "steer", (), |g| g.steer(mx, my, run != 0));
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_button(_: JNIEnv, _: JClass, h: jlong, b: jint) {
    with_game(h, "button", (), |g| g.button(b));
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_touch(_: JNIEnv, _: JClass, h: jlong, action: jint, x: jfloat, y: jfloat) {
    with_game(h, "touch", (), |g| g.touch(action, x, y));
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_pan(_: JNIEnv, _: JClass, h: jlong, dx: jfloat) {
    with_game(h, "pan", (), |g| g.pan(dx));
}

/// A long press: [title, choice, choice…], or null when there's nothing to do.
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_menu(mut env: JNIEnv, _: JClass, h: jlong, x: jfloat, y: jfloat) -> jobjectArray {
    let Some((title, items)) = with_game(h, "menu", None, |g| g.menu(x, y)) else { return std::ptr::null_mut() };
    let all: Vec<String> = std::iter::once(title).chain(items).collect();
    let Ok(arr) = env.new_object_array(all.len() as i32, "java/lang/String", JObject::null()) else { return std::ptr::null_mut() };
    for (i, s) in all.iter().enumerate() {
        if let Ok(js) = env.new_string(s) {
            let _ = env.set_object_array_element(&arr, i as i32, js);
        }
    }
    arr.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_pick(_: JNIEnv, _: JClass, h: jlong, i: jint) {
    with_game(h, "pick", (), |g| g.pick(i.max(0) as usize));
}

#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_pause(_: JNIEnv, _: JClass, h: jlong, on: jboolean) {
    with_game(h, "pause", (), |g| g.pause(on != 0));
}

/// Fill `buf` with sound (48 kHz stereo s16, interleaved): how many samples
/// were real (the rest is silence).
#[no_mangle]
pub extern "system" fn Java_io_github_dash1101_xmastui_Native_audio(env: JNIEnv, _: JClass, buf: JShortArray) -> jint {
    guard("audio", 0, || {
        let n = env.get_array_length(&buf).unwrap_or(0).max(0) as usize;
        let mut tmp = vec![0i16; n];
        let got = game::audio_take(&mut tmp);
        let _ = env.set_short_array_region(&buf, 0, &tmp);
        got as jint
    })
}
