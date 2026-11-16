//! Windows: window snow & lights on a click-through, always-on-top layered
//! window (per-pixel alpha via UpdateLayeredWindowIndirect) covering one
//! monitor. Window geometry comes straight from EnumWindows + DWM, polled
//! every frame (~30/s, ~60/s while something is being dragged, 4/s while
//! there's nothing to draw). All coordinates are physical pixels.

use super::{merge_damage, Canvas, Fx, FxCfg, Glyphs, Win};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "xmas-tui-snow";

/// Monitor rect (x, y, w, h) in physical screen pixels.
pub type Rect = (i32, i32, i32, i32);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

thread_local! {
    /// Set by the window procedure when our monitor's DPI (scaling) changes.
    static DPI_CHANGED: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_DPICHANGED {
        DPI_CHANGED.with(|c| c.set(true));
        return 0;
    }
    DefWindowProcW(h, msg, w, l)
}

unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
    (*(l as *mut Vec<HWND>)).push(h);
    1
}

/// Windows 11 rounds window corners (8 px at 96 dpi); Windows 10 doesn't.
/// (GetVersionEx tells the truth because the exe's manifest declares
/// Windows 10 support; without it this says "8.0" and we draw square.)
fn corner_radius() -> f32 {
    unsafe {
        let mut v: OSVERSIONINFOW = std::mem::zeroed();
        v.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if GetVersionExW(&mut v) != 0 && v.dwMajorVersion >= 10 && v.dwBuildNumber >= 22000 {
            8.0
        } else {
            0.0
        }
    }
}

/// Visible app windows + the taskbar on our monitor, bottom-to-top, in
/// screen pixels. `skip` is the wallpaper's own window (if it isn't behind
/// the icons, it's a borderless window that would look "fullscreen").
/// The visible windows on this monitor (bottom to top), and the ids of the
/// ones that would be but are cloaked (other virtual desktops, mostly), so
/// their snow can wait for them.
fn enumerate(own: HWND, skip: HWND, mon: Rect, scale: f32) -> (Vec<Win>, Vec<String>) {
    let mut hs: Vec<HWND> = Vec::with_capacity(256);
    let mut wins = vec![];
    let mut away = vec![];
    let (mx, my, mw, mh) = mon;
    unsafe {
        // EnumWindows walks the top-level windows top-to-bottom
        EnumWindows(Some(collect), &mut hs as *mut Vec<HWND> as LPARAM);
        let n = hs.len();
        for (i, &h) in hs.iter().enumerate() {
            if h == own || h == skip || IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
                continue;
            }
            // cheap checks first: nowhere near this monitor (outer rect)
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(h, &mut r);
            if r.right <= mx || r.left >= mx + mw || r.bottom <= my || r.top >= my + mh {
                continue;
            }
            // cloaked = hidden by DWM: other virtual desktops, suspended
            // UWP apps, the closed Start menu / search panels (checked below,
            // once it's clear this would otherwise count as a window)
            let mut cloaked: u32 = 0;
            DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as *mut _, 4);
            let cls = class_of(h);
            let taskbar = cls == "Shell_TrayWnd" || cls == "Shell_SecondaryTrayWnd";
            if !taskbar {
                let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
                // tool windows (tooltips, menus, palettes), click-through
                // overlays (like us) and untitled helpers aren't "windows"
                if ex & (WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT) != 0 || GetWindowTextLengthW(h) == 0 {
                    continue;
                }
                if matches!(cls.as_str(), "Progman" | "WorkerW" | CLASS) {
                    continue;
                }
            }
            // the visible frame, without the invisible resize borders
            // (on failure `r` keeps the outer rect)
            let mut fr: RECT = std::mem::zeroed();
            if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut fr as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32) == 0 {
                r = fr;
            }
            // not on this monitor at all
            if r.right <= mx || r.left >= mx + mw || r.bottom <= my || r.top >= my + mh {
                continue;
            }
            if taskbar {
                // an auto-hidden taskbar is parked off-screen with a sliver
                // showing; only the on-screen part counts
                r.top = r.top.max(my);
                r.bottom = r.bottom.min(my + mh);
                if ((r.bottom - r.top) as f32) < 8.0 * scale {
                    continue;
                }
            }
            let (w, hh) = (r.right - r.left, r.bottom - r.top);
            if w <= 0 || hh <= 0 || (!taskbar && ((w as f32) < 80.0 * scale || (hh as f32) < 40.0 * scale)) {
                continue;
            }
            if cloaked != 0 {
                away.push(format!("{:x}", h as usize));
                continue;
            }
            let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
            let covers = r.left <= mx && r.top <= my && r.right >= mx + mw && r.bottom >= my + mh;
            let kind = if taskbar {
                1
            } else if covers && style & WS_CAPTION != WS_CAPTION {
                2 // borderless + covers the monitor: a game / video
            } else if IsZoomed(h) != 0 {
                3
            } else {
                0
            };
            wins.push(Win {
                id: format!("{:x}", h as usize),
                x: r.left as f32,
                y: r.top as f32,
                w: w as f32,
                h: hh as f32,
                kind,
                z: (n - i) as i64,
            });
        }
    }
    wins.sort_by_key(|w| w.z);
    (wins, away)
}

fn cursor() -> (f32, f32) {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    (pt.x as f32, pt.y as f32)
}

/// Cheap fingerprint of the window layout, to notice drags.
fn fingerprint(wins: &[Win]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
    for w in wins {
        for b in w.id.bytes() {
            mix(b as u64);
        }
        for v in [w.x, w.y, w.w, w.h] {
            mix(v.to_bits() as u64);
        }
        mix(w.kind as u64);
    }
    h
}

/// The running overlay. Dropping it stops the thread, which destroys the
/// window and frees its bitmap.
pub struct Overlay {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Start the overlay for the monitor `mon` (physical screen px). `skip` is
/// the wallpaper's own HWND (0 for none).
pub fn spawn(cfg: Arc<Mutex<FxCfg>>, mon: Rect, skip: usize) -> Overlay {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("xmas-snow".into())
        .spawn(move || {
            if let Err(e) = run(cfg, mon, skip as HWND, &flag) {
                eprintln!("xmas-tui: window snow disabled: {e}");
                // a release exe has no console: leave a note for XMAS_SNOW_DEBUG
                if let Some(dir) = debug_dir() {
                    let _ = std::fs::write(dir.join("snow-debug.txt"), format!("window snow disabled: {e}\n"));
                }
            }
        })
        .ok();
    Overlay { stop, thread }
}

/// GDI objects + the window, released in the right order (on the overlay
/// thread, which owns the window).
struct Res {
    hwnd: HWND,
    screen: HDC,
    mem: HDC,
    dib: HBITMAP,
    old: HGDIOBJ,
}

impl Drop for Res {
    fn drop(&mut self) {
        unsafe {
            if !self.old.is_null() {
                SelectObject(self.mem, self.old);
            }
            if !self.dib.is_null() {
                DeleteObject(self.dib);
            }
            if !self.mem.is_null() {
                DeleteDC(self.mem);
            }
            if !self.screen.is_null() {
                ReleaseDC(std::ptr::null_mut(), self.screen);
            }
            DestroyWindow(self.hwnd);
        }
    }
}

fn new_fx(hwnd: HWND, mon: Rect, cfg: FxCfg) -> Fx {
    let scale = (unsafe { GetDpiForWindow(hwnd) }.max(96) as f32) / 96.0;
    let mut fx = Fx::new(mon.2 as f32, mon.3 as f32, cfg, scale);
    fx.corner = corner_radius() * scale;
    fx.origin = (mon.0 as f32, mon.1 as f32);
    fx
}

/// Where XMAS_SNOW_DEBUG dumps go: the variable's value if it names a
/// directory, else %TEMP%\xmas-tui.
fn debug_dir() -> Option<std::path::PathBuf> {
    let v = std::env::var_os("XMAS_SNOW_DEBUG")?;
    let dir = if v.is_empty() || v == "1" { std::env::temp_dir().join("xmas-tui") } else { std::path::PathBuf::from(v) };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Snapshot of what the overlay draws, over a neutral backdrop.
fn write_png(path: &std::path::Path, c: &Canvas) {
    let Ok(file) = std::fs::File::create(path) else { return };
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), c.w as u32, c.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let Ok(mut wr) = enc.write_header() else { return };
    let mut img = Vec::with_capacity(c.w * c.h * 3);
    for &p in &c.px {
        let a = (p >> 24) as f32 / 255.0;
        for sh in [16, 8, 0] {
            img.push((((p >> sh) & 0xff) as f32 + 48.0 * (1.0 - a)) as u8);
        }
    }
    let _ = wr.write_image_data(&img);
}

fn run(shared: Arc<Mutex<FxCfg>>, mon: Rect, skip: HWND, stop: &AtomicBool) -> Result<(), String> {
    let (mx, my, mw, mh) = mon;
    if mw <= 0 || mh <= 0 {
        return Err("empty monitor rect".into());
    }
    unsafe {
        // physical pixels everywhere, whatever the host process chose
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc); // fails harmlessly if already registered
        // layered + transparent = per-pixel alpha and fully click-through;
        // created hidden, shown only while there's something to draw
        let ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let title = wide("xmas-tui snow");
        let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), WS_POPUP, mx, my, mw, mh, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
        if hwnd.is_null() {
            return Err("CreateWindowEx failed".into());
        }
        let mut res = Res { hwnd, screen: std::ptr::null_mut(), mem: std::ptr::null_mut(), dib: std::ptr::null_mut(), old: std::ptr::null_mut() };
        res.screen = GetDC(std::ptr::null_mut());
        res.mem = CreateCompatibleDC(res.screen);
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = mw;
        bmi.bmiHeader.biHeight = -mh; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        res.dib = CreateDIBSection(res.mem, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if res.dib.is_null() || bits.is_null() {
            return Err("CreateDIBSection failed".into());
        }
        res.old = SelectObject(res.mem, res.dib);
        // 32bpp BGRA in memory == our premultiplied 0xAARRGGBB u32s
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (mw * mh) as usize);
        pixels.fill(0);

        let cfg0 = shared.lock().map(|c| *c).map_err(|e| e.to_string())?;
        let mut fx = new_fx(hwnd, mon, cfg0);
        let mut canvas = Canvas::new(mw as usize, mh as usize);
        let mut glyphs = Glyphs::new();
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let (dst, size, src) = (POINT { x: mx, y: my }, SIZE { cx: mw, cy: mh }, POINT { x: 0, y: 0 });
        let start = Instant::now();
        let mut last = Instant::now();
        let mut next = Instant::now();
        let mut full = true; // upload the whole bitmap on the next present
        let mut visible = false;
        let mut fullscreen = (false, Instant::now() - Duration::from_secs(1));
        let mut layout = 0u64;
        let dbg = debug_dir();
        let mut dbg_at = Instant::now();
        let (mut frames, mut presents, mut failed) = (0u64, 0u64, 0u64);
        // time spent per phase (debug): enumerate, simulate+draw, upload
        let mut cost = [Duration::ZERO; 3];
        let mut msg: MSG = std::mem::zeroed();
        while !stop.load(Ordering::Relaxed) {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let cfg = shared.lock().map(|c| *c).unwrap_or(cfg0);
            if DPI_CHANGED.with(|c| c.replace(false)) {
                // scaling changed: start over at the new size
                fx = new_fx(hwnd, mon, cfg);
                canvas = Canvas::new(mw as usize, mh as usize);
                full = true;
            }
            fx.set_cfg(cfg);
            let t = start.elapsed().as_secs_f32();
            let dt = last.elapsed().as_secs_f32().min(0.1);
            last = Instant::now();
            let t0 = Instant::now();
            let (wins, away) = enumerate(hwnd, skip, mon, fx.scale());
            let fp = fingerprint(&wins);
            let moving = fp != layout;
            layout = fp;
            fx.set_windows(wins, &away, cursor(), t);
            // fullscreen game / presentation (the shell's own verdict);
            // asked twice a second, it isn't free
            if fullscreen.1.elapsed() > Duration::from_millis(500) {
                fullscreen = (crate::winsys::fullscreen_app(hwnd), Instant::now());
            }
            if fullscreen.0 {
                fx.hidden = true;
            }
            let show = (cfg.snow || cfg.lights) && !fx.hidden;
            let t1 = Instant::now();
            if show {
                fx.update(dt, t);
            }
            let mut damage = canvas.clear_touched();
            if show {
                fx.render(&mut canvas, &mut glyphs, t);
            }
            damage.extend(canvas.touched.iter().cloned());
            let t2 = Instant::now();
            if full || !damage.is_empty() {
                let rects = if full { vec![(0, 0, mw, mh)] } else { merge_damage(&damage, mw, mh) };
                let (mut l, mut tp, mut r, mut b) = (i32::MAX, i32::MAX, 0, 0);
                for &(x, y, w, h) in &rects {
                    for yy in y..y + h {
                        let row = (yy * mw) as usize;
                        let span = row + x as usize..row + (x + w) as usize;
                        pixels[span.clone()].copy_from_slice(&canvas.px[span]);
                    }
                    l = l.min(x);
                    tp = tp.min(y);
                    r = r.max(x + w);
                    b = b.max(y + h);
                }
                if r > l && b > tp {
                    // only the dirty box is re-read from our bitmap
                    let dirty = RECT { left: l, top: tp, right: r, bottom: b };
                    let info = UPDATELAYEREDWINDOWINFO {
                        cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                        hdcDst: res.screen,
                        pptDst: &dst,
                        psize: &size,
                        hdcSrc: res.mem,
                        pptSrc: &src,
                        crKey: 0,
                        pblend: &blend,
                        dwFlags: ULW_ALPHA,
                        prcDirty: &dirty,
                    };
                    if UpdateLayeredWindowIndirect(hwnd, &info) != 0 {
                        presents += 1;
                        full = false;
                    } else {
                        failed += 1;
                    }
                }
            }
            // a hidden window costs DWM nothing and can't keep a fullscreen
            // game off its fast (independent flip) path
            if show && !visible {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                visible = true;
            } else if !show && visible {
                ShowWindow(hwnd, SW_HIDE);
                visible = false;
            }
            frames += 1;
            let t3 = Instant::now();
            for (c, d) in cost.iter_mut().zip([t1 - t0, t2 - t1, t3 - t2]) {
                *c += d;
            }
            if let Some(dir) = &dbg {
                if dbg_at.elapsed() > Duration::from_secs(1) {
                    dbg_at = Instant::now();
                    let ms = |d: Duration| d.as_secs_f64() * 1e3 / frames as f64;
                    let head = format!(
                        "monitor {mx},{my} {mw}x{mh} scale {} visible {visible} fullscreen {} frames {frames} presents {presents} failed {failed} ms/frame: enum {:.2} draw {:.2} upload {:.2}\n",
                        fx.scale(),
                        fullscreen.0,
                        ms(cost[0]),
                        ms(cost[1]),
                        ms(cost[2])
                    );
                    let _ = std::fs::write(dir.join("snow-debug.txt"), head + &fx.status());
                    write_png(&dir.join("snow-canvas.png"), &canvas);
                }
            }
            // ~30/s; ~60/s while windows move so piles keep up with drags;
            // 4/s while there's nothing to draw
            let frame = Duration::from_millis(if !show { 250 } else if moving { 16 } else { 33 });
            let now = Instant::now();
            next = (next + frame).max(now).min(now + frame);
            std::thread::sleep(next - now);
        }
        drop(res);
        Ok(())
    }
}
