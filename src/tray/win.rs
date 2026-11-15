//! Windows tray icon: Shell_NotifyIconW, owned by a hidden window on its
//! own thread. Left click opens the settings window; right click pops up
//! Settings…, Window snow, Sound, Start with Windows, Quit wallpaper.
//!
//! The owner is a never-shown top-level tool window rather than a
//! message-only one: only top-level windows get Explorer's "TaskbarCreated"
//! broadcast (to re-add the icon after Explorer restarts) and can be made
//! foreground, which TrackPopupMenu needs to close on a click elsewhere.

use super::{flag, open_settings, quit_wallpaper, toggle_flag, SOUND, TITLE, WINDOW_SNOW};
use crate::platform;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, TRUE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "xmas-tui-tray";
const WM_TRAY: u32 = WM_APP + 1;
const ID_SETTINGS: usize = 1;
const ID_SNOW: usize = 2;
const ID_SOUND: usize = 3;
const ID_AUTOSTART: usize = 4;
const ID_QUIT: usize = 5;

static HWND_TRAY: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static ICON: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static CONFIG: OnceLock<PathBuf> = OnceLock::new();

// kernel32; declared here to avoid another windows-sys feature
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The procedural tree as an HICON (32-bit colour + alpha).
fn make_icon(n: i32) -> HICON {
    let rgba = super::icon_rgba(n as usize);
    unsafe {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = n;
        bmi.bmiHeader.biHeight = -n; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dc = GetDC(std::ptr::null_mut());
        let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        ReleaseDC(std::ptr::null_mut(), dc);
        if color.is_null() || bits.is_null() {
            return std::ptr::null_mut();
        }
        // straight-alpha BGRA, like an .ico
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
        for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
        // all-zero AND mask: the alpha channel decides
        let row = (n as usize).div_ceil(16) * 2;
        let zeros = vec![0u8; row * n as usize];
        let mask = CreateBitmap(n, n, 1, 1, zeros.as_ptr() as *const _);
        let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

fn notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = 1;
    d
}

fn add_icon(hwnd: HWND) {
    let mut d = notify_data(hwnd);
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = ICON.load(Ordering::SeqCst);
    for (dst, src) in d.szTip.iter_mut().zip(TITLE.encode_utf16().take(127)) {
        *dst = src;
    }
    unsafe { Shell_NotifyIconW(NIM_ADD, &d) };
}

fn command(id: usize) {
    let Some(config) = CONFIG.get() else { return };
    match id {
        ID_SETTINGS => open_settings(config),
        ID_SNOW => {
            toggle_flag(config, WINDOW_SNOW);
        }
        ID_SOUND => {
            toggle_flag(config, SOUND);
        }
        ID_AUTOSTART => platform::autostart_set(!platform::autostart_get().unwrap_or(false)),
        ID_QUIT => quit_wallpaper(),
        _ => {}
    }
}

fn show_menu(hwnd: HWND) {
    let Some(config) = CONFIG.get() else { return };
    let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
    unsafe {
        let m = CreatePopupMenu();
        AppendMenuW(m, MF_STRING, ID_SETTINGS, wide("Settings…").as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING | check(flag(config, WINDOW_SNOW)), ID_SNOW, wide("Window snow").as_ptr());
        AppendMenuW(m, MF_STRING | check(flag(config, SOUND)), ID_SOUND, wide("Sound").as_ptr());
        AppendMenuW(m, MF_STRING | check(platform::autostart_get().unwrap_or(false)), ID_AUTOSTART, wide("Start with Windows").as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING, ID_QUIT, wide("Quit wallpaper").as_ptr());
        SetMenuDefaultItem(m, ID_SETTINGS as u32, 0);
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // the documented dance so the menu closes on a click elsewhere
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(m);
        if cmd > 0 {
            command(cmd as usize);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lp as u32 {
                WM_LBUTTONUP => command(ID_SETTINGS),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            command(wp & 0xffff);
            0
        }
        WM_DESTROY => {
            Shell_NotifyIconW(NIM_DELETE, &notify_data(hwnd));
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            add_icon(hwnd); // Explorer restarted
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn run() {
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = inst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::SeqCst);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide(TITLE).as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            eprintln!("xmas-tui: tray icon disabled (no window)");
            return;
        }
        ICON.store(make_icon(GetSystemMetrics(SM_CXSMICON).clamp(16, 64)), Ordering::SeqCst);
        HWND_TRAY.store(hwnd, Ordering::SeqCst);
        add_icon(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn spawn(config: PathBuf) {
    let _ = CONFIG.set(config);
    let _ = std::thread::Builder::new().name("tray".into()).spawn(run);
}

/// Remove the icon now (Shell_NotifyIcon works from any thread) and let
/// the tray thread wind down.
pub fn shutdown() {
    let hwnd = HWND_TRAY.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !hwnd.is_null() {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &notify_data(hwnd));
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

/// Another process (the settings window) asks the wallpaper to quit.
pub fn remote_quit() {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), std::ptr::null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_COMMAND, ID_QUIT, 0);
        }
    }
}
