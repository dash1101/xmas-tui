//! Windows desktop integration: put our window behind the desktop icons
//! (the same trick Wallpaper Engine / Lively use), poll the mouse (the icon
//! layer sits on top of us, so we never receive mouse messages), and detect
//! fullscreen apps so we can pause while gaming. Also the bits needed to
//! survive Explorer restarts and display changes (liveness / geometry
//! checks, polled by window.rs: as a *child* of Explorer's desktop windows
//! we get neither "TaskbarCreated" nor WM_DISPLAYCHANGE / WM_DPICHANGED).

#![cfg(windows)]

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, RedrawWindow, ScreenToClient, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop, DESKTOP_SWITCHDESKTOP};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, GetAsyncKeyState, GetFocus, SetFocus, VK_CONTROL, KEYEVENTF_KEYUP, VK_DOWN, VK_ESCAPE, VK_LBUTTON, VK_LEFT, VK_MENU, VK_RBUTTON, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_UP};
use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe extern "system" fn find_worker(top: HWND, l: LPARAM) -> BOOL {
    let defview = FindWindowExW(top, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null());
    if !defview.is_null() {
        let out = &mut *(l as *mut HWND);
        *out = FindWindowExW(std::ptr::null_mut(), top, wide("WorkerW").as_ptr(), std::ptr::null());
    }
    TRUE
}

/// The biggest `WorkerW` child of Progman (24H2 also parents small helper
/// WorkerWs there; the wallpaper one covers the desktop).
unsafe fn progman_worker(progman: HWND) -> HWND {
    let class = wide("WorkerW");
    let (mut best, mut area) = (std::ptr::null_mut(), -1i64);
    let mut child = FindWindowExW(progman, std::ptr::null_mut(), class.as_ptr(), std::ptr::null());
    while !child.is_null() {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(child, &mut r);
        let a = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
        if a > area {
            (best, area) = (child, a);
        }
        child = FindWindowExW(progman, child, class.as_ptr(), std::ptr::null());
    }
    best
}

/// (major version, build number), e.g. (10, 26100) for Windows 11 24H2.
/// (GetVersionEx tells the truth: the exe's manifest lists Windows 10.)
pub fn windows_version() -> (u32, u32) {
    use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};
    unsafe {
        let mut v: OSVERSIONINFOW = std::mem::zeroed();
        v.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if GetVersionExW(&mut v) != 0 {
            (v.dwMajorVersion, v.dwBuildNumber)
        } else {
            (0, 0)
        }
    }
}

/// A window's class, title and rect (for the log).
fn window_label(h: HWND) -> String {
    if h.is_null() {
        return "none".into();
    }
    let mut buf = [0u16; 64];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(h, &mut r) };
    format!("{} \"{title}\" ({},{})-({},{})", class_of(h), r.left, r.top, r.right, r.bottom)
}

/// One line about a window, for the log.
fn describe_window(h: HWND) -> String {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        let st = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        let mut lay = String::new();
        if ex & WS_EX_LAYERED != 0 {
            let (mut key, mut alpha, mut flags) = (0u32, 0u8, 0u32);
            lay = if GetLayeredWindowAttributes(h, &mut key, &mut alpha, &mut flags) != 0 { format!(" layered(alpha {alpha}, flags {flags})") } else { " layered(no attributes)".into() };
        }
        format!(
            "{:#x} {} vis={} style={st:#x} ex={ex:#x}{lay} rect=({},{})-({},{})",
            h as usize,
            class_of(h),
            IsWindowVisible(h),
            r.left,
            r.top,
            r.right,
            r.bottom
        )
    }
}

/// The desktop's window layout and where `ours` sits in it, for the log
/// (what to look at when the wallpaper doesn't show up on someone's PC).
pub fn describe(ours: HWND) -> String {
    let mut out = String::new();
    unsafe {
        let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        if progman.is_null() {
            return "no Progman window".into();
        }
        out += &format!("Progman {}\n", describe_window(progman));
        let mut c = GetWindow(progman, GW_CHILD);
        while !c.is_null() {
            let me = if c == ours { "  <- us" } else { "" };
            out += &format!("  child {}{me}\n", describe_window(c));
            c = GetWindow(c, GW_HWNDNEXT);
        }
        // top-level WorkerWs (the classic layout's wallpaper host is one)
        let class = wide("WorkerW");
        let mut w = FindWindowExW(std::ptr::null_mut(), std::ptr::null_mut(), class.as_ptr(), std::ptr::null());
        let mut n = 0;
        while !w.is_null() && n < 12 {
            let icons = !FindWindowExW(w, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null()).is_null();
            out += &format!("  top-level {}{}\n", describe_window(w), if icons { " (has the icons)" } else { "" });
            let mut k = GetWindow(w, GW_CHILD);
            while !k.is_null() {
                let me = if k == ours { "  <- us" } else { "" };
                out += &format!("    child {}{me}\n", describe_window(k));
                k = GetWindow(k, GW_HWNDNEXT);
            }
            w = FindWindowExW(std::ptr::null_mut(), w, class.as_ptr(), std::ptr::null());
            n += 1;
        }
        if !ours.is_null() {
            let parent = GetAncestor(ours, GA_PARENT);
            out += &format!("  us {} parent={:#x} ({})", describe_window(ours), parent as usize, class_of(parent));
        }
    }
    out
}

/// Make `hwnd` a layered window with constant alpha, from scratch (the
/// style bit cleared and set again, then the attributes). On the raised
/// desktop this must happen once the window is Progman's child: layering set
/// while it was still top-level doesn't carry over the re-parenting, and DWM
/// then composites the child as empty, so the wallpaper (or black) shows
/// through. Verified on Windows 11 26H2 (build 26300).
unsafe fn relayer(hwnd: HWND) {
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
    SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW);
}

/// Attached to the raised desktop (a child of Progman)?
fn on_raised_desktop(hwnd: HWND) -> bool {
    unsafe {
        let parent = GetAncestor(hwnd, GA_PARENT);
        !parent.is_null() && class_of(parent) == "Progman" && GetWindowLongPtrW(parent, GWL_EXSTYLE) as u32 & WS_EX_NOREDIRECTIONBITMAP != 0
    }
}

/// Keep a raised-desktop child visible: if something cleared its layered
/// style (it would vanish again), put it back. Cheap; polled about once a
/// second. Returns true if it had to.
pub fn keep_layered(hwnd: HWND) -> bool {
    unsafe {
        if !on_raised_desktop(hwnd) || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0 {
            return false;
        }
        relayer(hwnd);
        true
    }
}

/// Place a child at screen rect (x, y, w, h), converted to `parent`'s
/// client coordinates, after `after` in the sibling z-order (None = keep).
unsafe fn place(hwnd: HWND, parent: HWND, after: Option<HWND>, x: i32, y: i32, w: i32, h: i32) {
    let mut pt = POINT { x, y };
    ScreenToClient(parent, &mut pt);
    let mut flags = SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED;
    if after.is_none() {
        flags |= SWP_NOZORDER;
    }
    SetWindowPos(hwnd, after.unwrap_or(std::ptr::null_mut()), pt.x, pt.y, w, h, flags);
}

/// Re-parent `hwnd` behind the desktop icons, covering the given monitor
/// rect (physical screen coordinates). Returns false if the desktop windows
/// couldn't be found (then we stay a normal borderless window).
///
/// Two desktop layouts exist (same approach as Lively Wallpaper):
/// - classic (Windows 10 .. 11 23H2): message 0x052C makes Progman split
///   off a top-level WorkerW behind the top-level window holding the icons
///   (SHELLDLL_DefView); we become a child of that WorkerW.
/// - "raised desktop" (Windows 11 24H2+, Progman has
///   WS_EX_NOREDIRECTIONBITMAP): DefView and the wallpaper WorkerW are both
///   *children* of Progman. A child of the WorkerW is never composited, so
///   we become a layered child of Progman itself, z-ordered between the
///   icons and the WorkerW: [DefView, us, WorkerW]. Layered child windows
///   need the exe manifest to declare Windows 8+ support (see build.rs).
pub fn attach_to_desktop(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    unsafe {
        let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        if progman.is_null() {
            return false;
        }
        // ask Explorer to spawn the WorkerW that sits behind the icons. Only
        // this variant: on 24H2 a follow-up (0, 0) can tear the new layer down
        let mut res: usize = 0;
        SendMessageTimeoutW(progman, 0x052C, 0xD, 0x1, SMTO_NORMAL, 1000, &mut res);
        let style = (WS_CHILD | WS_VISIBLE) as isize;

        let raised = GetWindowLongPtrW(progman, GWL_EXSTYLE) as u32 & WS_EX_NOREDIRECTIONBITMAP != 0;
        let mut worker: HWND = std::ptr::null_mut();
        if !raised {
            EnumWindows(Some(find_worker), &mut worker as *mut HWND as LPARAM);
            if worker.is_null() {
                // some older Windows 10 builds only answer the plain form
                SendMessageTimeoutW(progman, 0x052C, 0, 0, SMTO_NORMAL, 1000, &mut res);
                EnumWindows(Some(find_worker), &mut worker as *mut HWND as LPARAM);
            }
        }
        if !worker.is_null() {
            // classic
            eprintln!("xmas-tui: desktop attach: classic (child of top-level WorkerW {:#x}; Progman raised={raised})", worker as usize);
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            SetParent(hwnd, worker);
            place(hwnd, worker, None, x, y, w, h);
            return true;
        }

        // raised desktop (or a layout that looks like it)
        let pworker = progman_worker(progman);
        let defview = FindWindowExW(progman, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null());
        if !raised && pworker.is_null() {
            eprintln!("xmas-tui: desktop attach: no wallpaper WorkerW found (Progman raised={raised})");
            return false;
        }
        eprintln!("xmas-tui: desktop attach: raised desktop (layered child of Progman; raised={raised}, icons {:#x}, WorkerW {:#x})", defview as usize, pworker as usize);
        // a child of the redirection-less Progman only shows if it's a
        // layered window — and the layering has to be (re)done once it *is*
        // the child (see relayer)
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        SetParent(hwnd, progman);
        // just below the icons (or on top, if there's no icon view at all)
        place(hwnd, progman, Some(if defview.is_null() { HWND_TOP } else { defview }), x, y, w, h);
        relayer(hwnd);
        // and the wallpaper WorkerW must stay the bottom child, under us
        let first = GetWindow(progman, GW_CHILD);
        if !pworker.is_null() && !first.is_null() && GetWindow(first, GW_HWNDLAST) != pworker {
            SetWindowPos(pworker, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        true
    }
}

/// The wallpaper full screen: out from behind the icons, an ordinary
/// window in front covering the monitor (x, y, w, h), with the keyboard.
/// `attach_to_desktop` puts it back.
pub fn bring_to_front(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        SetParent(hwnd, std::ptr::null_mut());
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((ex & !(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)) | WS_EX_APPWINDOW) as isize);
        SetWindowPos(hwnd, HWND_TOP, x, y, w, h, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        // a background process may not take the foreground; a tap of Alt
        // lets it (the usual way round that rule)
        keybd_event(VK_MENU as u8, 0, 0, 0);
        keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
    }
}

/// Back behind the icons after `bring_to_front` (the ex-style the attach
/// expects: skip the taskbar).
pub fn back_to_desktop(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((ex & !WS_EX_APPWINDOW) | WS_EX_TOOLWINDOW) as isize);
    }
    attach_to_desktop(hwnd, x, y, w, h)
}

/// Move/resize `hwnd` to the screen rect (x, y, w, h) again, e.g. after a
/// resolution or monitor-layout change. Works both attached (child of
/// Explorer's desktop: converted to the parent's client coordinates, which
/// also absorbs a changed virtual-screen origin) and in the top-level
/// fallback.
pub fn reposition(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        if GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CHILD != 0 {
            let parent = GetAncestor(hwnd, GA_PARENT);
            place(hwnd, parent, None, x, y, w, h);
        } else {
            SetWindowPos(hwnd, std::ptr::null_mut(), x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }
}

/// Screen position of our parent's client origin ((0, 0) for a top-level
/// window). Explorer moves/resizes Progman / WorkerW to the virtual screen
/// when monitors are added, moved or re-arranged, which shifts us with it.
pub fn parent_origin(hwnd: HWND) -> (i32, i32) {
    unsafe {
        if GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CHILD == 0 {
            return (0, 0);
        }
        let mut pt = POINT { x: 0, y: 0 };
        ClientToScreen(GetAncestor(hwnd, GA_PARENT), &mut pt);
        (pt.x, pt.y)
    }
}

/// Is the attached wallpaper window where it shows: visible, inside a visible parent,
/// and with no other wallpaper layer in front of it? Explorer rebuilds its
/// desktop layers now and then (a new wallpaper, a display change, waking
/// from sleep), and a layer it puts in front hides the wallpaper without its window
/// going anywhere. Err(why) when something's wrong.
pub fn attach_ok(hwnd: HWND) -> Result<(), String> {
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return Err("the wallpaper window is hidden".into());
        }
        let parent = GetAncestor(hwnd, GA_PARENT);
        if parent.is_null() || IsWindowVisible(parent) == 0 {
            return Err(format!("its parent ({}) is hidden", class_of(parent)));
        }
        let visible_worker = |h: HWND| IsWindowVisible(h) != 0 && class_of(h) == "WorkerW" && FindWindowExW(h, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null()).is_null();
        if on_raised_desktop(hwnd) {
            // [icons, the wallpaper, Windows' WorkerW]: no WorkerW in front of it
            let mut h = GetWindow(hwnd, GW_HWNDPREV);
            while !h.is_null() {
                if visible_worker(h) {
                    return Err(format!("a WorkerW ({:#x}) is in front of it", h as usize));
                }
                h = GetWindow(h, GW_HWNDPREV);
            }
        } else if class_of(parent) == "WorkerW" {
            // classic: between the icons' window and the wallpaper's WorkerW, no other
            // wallpaper WorkerW
            let mut h = GetWindow(parent, GW_HWNDPREV);
            while !h.is_null() {
                if !FindWindowExW(h, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null()).is_null() {
                    break;
                }
                if visible_worker(h) {
                    return Err(format!("another WorkerW ({:#x}) is in front of ours", h as usize));
                }
                h = GetWindow(h, GW_HWNDPREV);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Things that can leave the wallpaper stuck or hidden: waking from sleep
// (closing a laptop's lid), unlocking, the display changing or switching
// back on, DWM restarting, a new Windows wallpaper. The wallpaper window is a child
// of Explorer's and never hears of them, so a hidden top-level window of
// xmas-tui's own listens, and window.rs attaches afresh shortly after.

static DESK_EVENT: Mutex<Option<&'static str>> = Mutex::new(None);
static DESK_WATCH: AtomicBool = AtomicBool::new(false);
static DISPLAY_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(255);

/// What happened (once), if anything.
pub fn take_desktop_event() -> Option<&'static str> {
    DESK_EVENT.lock().ok()?.take()
}

fn desktop_event(what: &'static str) {
    if let Ok(mut e) = DESK_EVENT.lock() {
        *e = Some(what);
    }
}

/// Start listening (once).
pub fn watch_desktop_events() {
    if DESK_WATCH.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("desktop events".into()).spawn(|| unsafe {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::System::Power::RegisterPowerSettingNotification;
        use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
        use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide("xmas-tui-desktop-events");
        let wc = WNDCLASSW { lpfnWndProc: Some(desk_events_proc), hInstance: inst, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        // top-level (message-only windows don't get the broadcasts), never shown
        let h = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), class.as_ptr(), WS_POPUP, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
        if h.is_null() {
            DESK_WATCH.store(false, Ordering::SeqCst);
            return;
        }
        WTSRegisterSessionNotification(h, NOTIFY_FOR_THIS_SESSION);
        RegisterPowerSettingNotification(h as _, &GUID_CONSOLE_DISPLAY_STATE, 0);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn desk_events_proc(h: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    use windows_sys::Win32::System::Power::POWERBROADCAST_SETTING;
    use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
    const WM_DWMCOMPOSITIONCHANGED: u32 = 0x031E;
    const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
    const WTS_CONSOLE_CONNECT: usize = 1;
    const WTS_SESSION_UNLOCK: usize = 8;
    const PBT_APMRESUMESUSPEND: usize = 7;
    const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
    const PBT_POWERSETTINGCHANGE: usize = 0x8013;
    const SPI_SETDESKWALLPAPER: usize = 0x14;
    match msg {
        WM_DISPLAYCHANGE => desktop_event("the display changed"),
        WM_DWMCOMPOSITIONCHANGED => desktop_event("DWM restarted"),
        WM_SETTINGCHANGE if wp == SPI_SETDESKWALLPAPER => desktop_event("the Windows wallpaper changed"),
        WM_WTSSESSION_CHANGE if wp == WTS_SESSION_UNLOCK || wp == WTS_CONSOLE_CONNECT => desktop_event("the session was unlocked"),
        WM_POWERBROADCAST => match wp {
            PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => desktop_event("woke from sleep"),
            PBT_POWERSETTINGCHANGE if lp != 0 => {
                let s = &*(lp as *const POWERBROADCAST_SETTING);
                let g = &s.PowerSetting;
                let d = &GUID_CONSOLE_DISPLAY_STATE;
                // the screen back on after being off (0 = off, 1 = on, 2 =
                // dimmed; the state as it is comes once on registering)
                if g.data1 == d.data1 && g.data2 == d.data2 && g.data3 == d.data3 && g.data4 == d.data4 && s.DataLength >= 1 {
                    let was = DISPLAY_STATE.swap(s.Data[0], Ordering::SeqCst);
                    if was == 0 && s.Data[0] == 1 {
                        desktop_event("the screen came back on");
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
    DefWindowProcW(h, msg, wp, lp)
}

/// Is our attached window still alive inside the *current* Explorer's
/// desktop? False once Explorer crashed / restarted or tore down the
/// window we were parented to (then window.rs creates a fresh window and
/// attaches it again).
pub fn still_attached(hwnd: HWND) -> bool {
    unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        let parent = GetAncestor(hwnd, GA_PARENT);
        if parent.is_null() || parent == GetDesktopWindow() || IsWindow(parent) == 0 {
            return false;
        }
        let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        if progman.is_null() {
            return false;
        }
        // the parent (Progman or a WorkerW) must belong to the running Explorer
        let (mut a, mut b) = (0u32, 0u32);
        GetWindowThreadProcessId(parent, &mut a);
        GetWindowThreadProcessId(progman, &mut b);
        a != 0 && a == b
    }
}

unsafe extern "system" fn find_defview(top: HWND, l: LPARAM) -> BOOL {
    if !FindWindowExW(top, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null()).is_null() {
        *(l as *mut bool) = true;
        return 0; // found: stop enumerating
    }
    TRUE
}

/// Has (a restarted) Explorer finished building its desktop yet? Progman
/// plus the icon view (SHELLDLL_DefView, under Progman or a WorkerW).
pub fn desktop_ready() -> bool {
    unsafe {
        if FindWindowW(wide("Progman").as_ptr(), std::ptr::null()).is_null() {
            return false;
        }
        let mut found = false;
        EnumWindows(Some(find_defview), &mut found as *mut bool as LPARAM);
        found
    }
}

/// Desktop mode: take the keyboard into the wallpaper's own window (a
/// click on the village, a line to type). Our window is a child of
/// Explorer's desktop, so the two share input: once the desktop's in front
/// (it is, after a click on it), the keys can come to us instead of to the
/// icons. They go back the moment anything else is clicked.
pub fn take_focus(hwnd: HWND) -> bool {
    unsafe {
        if GetFocus() == hwnd {
            return true;
        }
        let fg = GetForegroundWindow();
        let cls = class_of(fg);
        if cls != "Progman" && cls != "WorkerW" && GetAncestor(hwnd, GA_ROOT) != fg {
            return false;
        }
        SetFocus(hwnd);
        // (and still under the icons)
        keep_under_icons(hwnd);
        GetFocus() == hwnd
    }
}

/// Keep the wallpaper under the desktop icons (a child of Progman beside
/// the icons' view): taking the keyboard, or Explorer shuffling its
/// children, can lift it over them, and the icons vanish behind the
/// village. Cheap; returns true if it had to.
pub fn keep_under_icons(hwnd: HWND) -> bool {
    unsafe {
        let parent = GetAncestor(hwnd, GA_PARENT);
        if parent.is_null() || class_of(parent) != "Progman" {
            return false;
        }
        let defview = FindWindowExW(parent, std::ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), std::ptr::null());
        if defview.is_null() {
            return false;
        }
        // which comes first, top down: us (wrong) or the icons
        let mut w = GetWindow(parent, GW_CHILD);
        while !w.is_null() && w != hwnd && w != defview {
            w = GetWindow(w, GW_HWNDNEXT);
        }
        if w != hwnd {
            return false;
        }
        SetWindowPos(hwnd, defview, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        true
    }
}

/// Does the wallpaper's window have the keyboard? (Asked straight: the
/// focus events don't always come for a child of the desktop.)
pub fn has_focus(hwnd: HWND) -> bool {
    unsafe { GetFocus() == hwnd }
}

/// How much of the bottom of `hwnd`'s monitor the taskbar covers (the
/// monitor's height less its work area's), in pixels. 0 for one that hides
/// itself, or sits at the side or top.
pub fn taskbar_bottom(hwnd: HWND) -> i32 {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    unsafe {
        let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if m.is_null() || GetMonitorInfoW(m, &mut mi) == 0 {
            return 0;
        }
        (mi.rcMonitor.bottom - mi.rcWork.bottom).clamp(0, (mi.rcMonitor.bottom - mi.rcMonitor.top) / 3)
    }
}

const CF_UNICODETEXT: u32 = 13;

/// The clipboard's text, if it holds any (the system's own clipboard: no
/// helper programs, no console windows flashing up).
pub fn clipboard_text() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        // (another program may have it open for a moment)
        if !(0..5).any(|i| {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            OpenClipboard(std::ptr::null_mut()) != 0
        }) {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(CF_UNICODETEXT);
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let max = (GlobalSize(h) / 2).min(1 << 16);
                let mut n = 0;
                while n < max && *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }
}

/// Put text on the clipboard.
pub fn set_clipboard_text(text: &str) -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if !(0..5).any(|i| {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            OpenClipboard(std::ptr::null_mut()) != 0
        }) {
            return false;
        }
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        let mut ok = false;
        if !h.is_null() {
            let p = GlobalLock(h) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                GlobalUnlock(h);
                // the clipboard owns it from here (unless it said no)
                ok = !SetClipboardData(CF_UNICODETEXT, h).is_null();
            }
            if !ok {
                GlobalFree(h);
            }
        }
        CloseClipboard();
        ok
    }
}

pub struct Polled {
    pub x: i32,
    pub y: i32,
    pub on_desktop: bool,
    /// Logical primary / secondary buttons (already swapped for
    /// left-handed setups).
    pub left: bool,
    pub right: bool,
}

/// Cursor position (physical screen px) and button state, plus whether the
/// pointer is over the bare desktop (not over some app window).
pub fn poll() -> Polled {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let under = WindowFromPoint(pt);
        let root = GetAncestor(under, GA_ROOT);
        let cls = class_of(root);
        let on_desktop = cls == "Progman" || cls == "WorkerW";
        // GetAsyncKeyState reports *physical* buttons: with "switch primary
        // and secondary buttons" on, the user's primary is physical right
        let (prim, sec) = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { (VK_RBUTTON, VK_LBUTTON) } else { (VK_LBUTTON, VK_RBUTTON) };
        Polled {
            x: pt.x,
            y: pt.y,
            on_desktop,
            left: (GetAsyncKeyState(prim as i32) as u16 & 0x8000) != 0,
            right: (GetAsyncKeyState(sec as i32) as u16 & 0x8000) != 0,
        }
    }
}

/// Walking keys, read only while the desktop itself has keyboard focus
/// (the user clicked the bare desktop): [left, right, up, down, interact,
/// invite, snowball, pockets, items 1-5, shift, cycle, jump, esc, centre].
/// None while any other window is focused, so typing elsewhere never moves
/// anyone.
pub fn desktop_keys() -> Option<[bool; 18]> {
    unsafe {
        let fg = GetForegroundWindow();
        let cls = class_of(fg);
        if cls != "Progman" && cls != "WorkerW" {
            return None;
        }
        let down = |vk: u16| (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0;
        Some([
            down(VK_LEFT) || down(b'A' as u16),
            down(VK_RIGHT) || down(b'D' as u16),
            down(VK_UP) || down(b'W' as u16),
            down(VK_DOWN) || down(b'S' as u16),
            down(b'E' as u16) || down(VK_RETURN),
            down(b'F' as u16),
            down(b'R' as u16),
            down(b'I' as u16),
            down(b'1' as u16),
            down(b'2' as u16),
            down(b'3' as u16),
            down(b'4' as u16),
            down(b'5' as u16),
            down(VK_SHIFT),
            down(b'Q' as u16),
            down(VK_SPACE),
            down(VK_ESCAPE),
            down(b'C' as u16),
        ])
    }
}

/// Is the cursor over a desktop *icon* (rather than the bare desktop)?
/// Best effort: the icon layer is Explorer's SysListView32, which
/// hot-tracks the item under the mouse, so LVM_GETHOTITEM (no
/// cross-process buffer needed, unlike LVM_HITTEST) tells us whether an
/// icon is hovered. Called on a press, and while the pointer is over
/// something of the village's, with a short timeout so a hung Explorer
/// can't stall the wallpaper. Rubber-band selections that start on empty
/// desktop still count as desktop clicks.
pub fn over_icon(x: i32, y: i32) -> bool {
    const LVM_GETHOTITEM: u32 = 0x103D;
    unsafe {
        let under = WindowFromPoint(POINT { x, y });
        if under.is_null() || class_of(under) != "SysListView32" {
            return false;
        }
        let mut res: usize = usize::MAX;
        let ok = SendMessageTimeoutW(under, LVM_GETHOTITEM, 0, 0, SMTO_ABORTIFHUNG, 25, &mut res);
        ok != 0 && (res as isize) >= 0
    }
}

/// Should the wallpaper pause? While a fullscreen game / presentation is
/// in front (nobody can see us), and while the session isn't visible at
/// all: locked, screen saver, user switched away, or a secure desktop
/// (lock screen / UAC prompt) has the input. `own` is our window: in the
/// top-level fallback it is itself a borderless monitor-sized window, which
/// Windows reports as a "fullscreen app" once it's in the foreground.
/// Why the wallpaper should rest right now, if it should, and how long
/// that has to hold first (seconds). Locked / secure desktop: at once. A
/// fullscreen app: only once it's lasted a moment, since the shell's verdict
/// can flicker (some overlays and capture tools set it off briefly), and a
/// wallpaper that rests on every flicker stutters.
pub fn pause_reason(own: HWND) -> Option<(String, f32)> {
    unsafe {
        // the input desktop isn't ours (Winlogon / secure desktop)
        let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desk.is_null() {
            return Some(("another desktop has the input (locked or a UAC prompt)".into(), 0.0));
        }
        CloseDesktop(desk);
        let mut st: QUERY_USER_NOTIFICATION_STATE = 0;
        if SHQueryUserNotificationState(&mut st) != 0 {
            return None;
        }
        match st {
            // QUNS_NOT_PRESENT: locked, screen saver, fast user switching
            1 => Some(("the session isn't present (locked or screen saver)".into(), 0.0)),
            // QUNS_PRESENTATION_MODE: the user asked for quiet
            4 => Some(("presentation mode".into(), 0.0)),
            // QUNS_BUSY (fullscreen app), QUNS_RUNNING_D3D_FULL_SCREEN
            2 | 3 => {
                let fg = GetForegroundWindow();
                fullscreen_over(fg, own).then(|| (format!("a fullscreen app in front (state {st}: {})", window_label(fg)), 1.5))
            }
            _ => None,
        }
    }
}

/// Does `fg` cover the monitor `own` is on (so nothing of the wallpaper
/// shows)? The shell says "busy" for a fullscreen window on any monitor.
/// With no `own` to compare against, any fullscreen verdict counts.
fn fullscreen_over(fg: HWND, own: HWND) -> bool {
    unsafe {
        if own.is_null() {
            return true;
        }
        if fg.is_null() || fg == own {
            return false;
        }
        // the desktop and taskbar are full-width, but they're not an app
        let class = class_of(fg);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return false;
        }
        let (mut a, mut b): (RECT, RECT) = (std::mem::zeroed(), std::mem::zeroed());
        if GetWindowRect(own, &mut a) == 0 || GetWindowRect(fg, &mut b) == 0 {
            return true;
        }
        b.left <= a.left + 2 && b.top <= a.top + 2 && b.right >= a.right - 2 && b.bottom >= a.bottom - 2
    }
}

/// True while a fullscreen game / presentation covers `own`'s monitor
/// (used by the window-snow overlay, which hides itself then).
pub fn fullscreen_app(own: HWND) -> bool {
    unsafe {
        let mut st: QUERY_USER_NOTIFICATION_STATE = 0;
        if SHQueryUserNotificationState(&mut st) != 0 {
            return false;
        }
        // 2 = QUNS_BUSY (fullscreen app), 3 = D3D fullscreen, 4 = presentation mode
        st == 4 || (matches!(st, 2 | 3) && fullscreen_over(GetForegroundWindow(), own))
    }
}

// ---------------------------------------------------------------------------
// Right-clicks on the village. The desktop's own window (above ours) gets
// every right-click and shows Windows' desktop menu. A low-level mouse hook
// takes the right-clicks that land on something of the village's (someone
// in the village, a house, a widget, a shelf; never an icon or a window)
// and hands them to the wallpaper, which shows its own menu; every other
// right-click goes straight on to Windows. Whether a click is the village's
// is worked out ahead of time by the wallpaper as the pointer moves (see
// `rclick_target`): the hook itself only reads that, since Windows quietly
// drops a mouse hook that's slow to answer (and every mouse event on the
// system passes through it). It runs on a thread of its own that does
// nothing but pump messages; if Windows drops it anyway, the wallpaper
// notices and puts it back (`rclick_hook_quiet_ms`, `rclick_rehook`).

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

static RC_HOOKED: AtomicBool = AtomicBool::new(false);
static RC_PENDING: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static RC_EATEN: AtomicBool = AtomicBool::new(false);
/// The wallpaper is drawing (not resting, not full screen, attached): only
/// then are right-clicks taken from the desktop.
static RC_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The screen rect (the cell) last looked at under the pointer, whether a
/// right-click there is the village's, whether it's the bare desktop (no
/// icon, no window: the wheel there is the village's), and when
/// (GetTickCount).
static RC_TARGET: Mutex<((i32, i32, i32, i32), bool, bool, u32)> = Mutex::new(((0, 0, 0, 0), false, false, 0));
/// Wheel turns taken from the desktop: (notches × 120, sideways, Ctrl,
/// Shift), waiting for the wallpaper.
static WHEEL: Mutex<Vec<(i32, bool, bool, bool)>> = Mutex::new(Vec::new());
/// When the hook last saw any mouse event (GetTickCount).
static RC_SEEN: AtomicU32 = AtomicU32::new(0);
static RC_THREAD: AtomicU32 = AtomicU32::new(0);
static RC_HOOK: AtomicIsize = AtomicIsize::new(0);
const WM_REHOOK: u32 = WM_APP + 1;

fn ticks() -> u32 {
    unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() }
}

pub fn rclick_active(on: bool) {
    RC_ACTIVE.store(on, Ordering::Relaxed);
}

/// The pointer is over screen rect `r` (x0, y0, x1, y1), and a right-click
/// there is (or isn't) the village's.
pub fn rclick_target(r: (i32, i32, i32, i32), ours: bool, bare: bool) {
    if let Ok(mut t) = RC_TARGET.lock() {
        *t = (r, ours, bare, ticks());
    }
}

/// Wheel turns (and touchpad scrolls and pinches, which arrive as wheel
/// turns, a pinch with Ctrl) over the bare desktop, since the last look.
pub fn take_wheel() -> Vec<(i32, bool, bool, bool)> {
    WHEEL.lock().map(|mut w| std::mem::take(&mut *w)).unwrap_or_default()
}

/// Start taking the village's right-clicks (once).
pub fn rclick_hook() {
    if RC_HOOKED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("right-click hook".into()).spawn(|| unsafe {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::System::Threading::{GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
        // answered promptly even with the CPU busy
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        RC_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let hook = || SetWindowsHookExW(WH_MOUSE_LL, Some(rclick_proc), GetModuleHandleW(std::ptr::null()), 0);
        let h = hook();
        if h.is_null() {
            eprintln!("xmas-tui: no right-click hook (SetWindowsHookEx failed): long-press for the menu");
            RC_HOOKED.store(false, Ordering::SeqCst);
            return;
        }
        RC_HOOK.store(h as isize, Ordering::Relaxed);
        RC_SEEN.store(ticks(), Ordering::Relaxed);
        // testing: lose the hook after 10 s, the way Windows drops one
        let drop_test = std::env::var_os("XMAS_TEST_DROP_HOOK").is_some().then(|| SetTimer(std::ptr::null_mut(), 0, 10_000, None));
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_TIMER && drop_test.is_some_and(|t| t == msg.wParam) {
                KillTimer(std::ptr::null_mut(), msg.wParam);
                UnhookWindowsHookEx(RC_HOOK.load(Ordering::Relaxed) as HHOOK);
                eprintln!("xmas-tui: (test) right-click hook dropped");
                continue;
            }
            if msg.message == WM_REHOOK {
                UnhookWindowsHookEx(RC_HOOK.load(Ordering::Relaxed) as HHOOK);
                let h = hook();
                RC_HOOK.store(h as isize, Ordering::Relaxed);
                RC_SEEN.store(ticks(), Ordering::Relaxed);
                eprintln!("xmas-tui: right-click hook put back ({})", if h.is_null() { "failed" } else { "ok" });
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// How long since the hook last saw a mouse event (ms). Long, while the
/// pointer is moving, means Windows has dropped it.
pub fn rclick_hook_quiet_ms() -> Option<u32> {
    RC_HOOKED.load(Ordering::Relaxed).then(|| ticks().wrapping_sub(RC_SEEN.load(Ordering::Relaxed)))
}

/// Put the hook back.
pub fn rclick_rehook() {
    let t = RC_THREAD.load(Ordering::SeqCst);
    if t != 0 {
        RC_SEEN.store(ticks(), Ordering::Relaxed);
        unsafe { PostThreadMessageW(t, WM_REHOOK, 0, 0) };
    }
}

/// A right-click taken from the desktop, waiting for the wallpaper.
pub fn take_rclick() -> Option<(i32, i32)> {
    RC_PENDING.lock().ok()?.take()
}

/// Send a right-click on to whatever's under the cursor (the desktop's own
/// menu): injected input, which the hook lets through.
pub fn pass_rclick() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT};
    let ev = |flags| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } };
    let inputs = [ev(MOUSEEVENTF_RIGHTDOWN), ev(MOUSEEVENTF_RIGHTUP)];
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

/// Is a right-click at (x, y) the village's? Only what the wallpaper worked
/// out lately for that spot; nothing that could wait.
fn rclick_ours(x: i32, y: i32) -> bool {
    if !RC_ACTIVE.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(t) = RC_TARGET.try_lock() else { return false };
    let ((x0, y0, x1, y1), ours, _, at) = *t;
    ours && x >= x0 && x < x1 && y >= y0 && y < y1 && ticks().wrapping_sub(at) < 1500
}

/// Is a wheel turn at (x, y) over the bare desktop (the village's)?
fn wheel_ours(x: i32, y: i32) -> bool {
    if !RC_ACTIVE.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(t) = RC_TARGET.try_lock() else { return false };
    let ((x0, y0, x1, y1), _, bare, at) = *t;
    bare && x >= x0 && x < x1 && y >= y0 && y < y1 && ticks().wrapping_sub(at) < 1500
}

unsafe extern "system" fn rclick_proc(code: i32, wp: usize, lp: isize) -> isize {
    if code == HC_ACTION as i32 && lp != 0 {
        RC_SEEN.store(ticks(), Ordering::Relaxed);
        let m = &*(lp as *const MSLLHOOKSTRUCT);
        let injected = m.flags & LLMHF_INJECTED != 0;
        // the wheel over the bare desktop (a touchpad's scrolls and pinches
        // too, which may come as injected input): the village's
        if matches!(wp as u32, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) && wheel_ours(m.pt.x, m.pt.y) {
            let delta = (m.mouseData >> 16) as u16 as i16 as i32;
            let down = |vk: u16| (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0;
            if let Ok(mut w) = WHEEL.try_lock() {
                if w.len() < 64 {
                    w.push((delta, wp as u32 == WM_MOUSEHWHEEL, down(VK_CONTROL), down(VK_SHIFT)));
                }
                return 1;
            }
        }
        if !injected {
            match wp as u32 {
                WM_RBUTTONDOWN if rclick_ours(m.pt.x, m.pt.y) => {
                    if let Ok(mut p) = RC_PENDING.try_lock() {
                        *p = Some((m.pt.x, m.pt.y));
                        RC_EATEN.store(true, Ordering::SeqCst);
                        return 1;
                    }
                }
                WM_RBUTTONUP if RC_EATEN.swap(false, Ordering::SeqCst) => return 1,
                _ => {}
            }
        }
    }
    CallNextHookEx(RC_HOOK.load(Ordering::Relaxed) as HHOOK, code, wp, lp)
}
