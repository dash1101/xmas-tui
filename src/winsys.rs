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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, GetAsyncKeyState, SetFocus, KEYEVENTF_KEYUP, VK_DOWN, VK_LBUTTON, VK_LEFT, VK_MENU, VK_RBUTTON, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_UP};
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
/// invite, snowball, pockets, items 1-5]. None while any other window is focused, so typing elsewhere
/// never moves anyone.
pub fn desktop_keys() -> Option<[bool; 15]> {
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
            down(b'E' as u16) || down(VK_SPACE) || down(VK_RETURN),
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
        ])
    }
}

/// Is the cursor over a desktop *icon* (rather than the bare desktop)?
/// Best effort: the icon layer is Explorer's SysListView32, which
/// hot-tracks the item under the mouse, so LVM_GETHOTITEM (no
/// cross-process buffer needed, unlike LVM_HITTEST) tells us whether an
/// icon is hovered. Only called on a button press, with a short timeout so
/// a hung Explorer can't stall us. Rubber-band selections that start on
/// empty desktop still count as desktop clicks.
pub fn over_icon(x: i32, y: i32) -> bool {
    const LVM_GETHOTITEM: u32 = 0x103D;
    unsafe {
        let under = WindowFromPoint(POINT { x, y });
        if under.is_null() || class_of(under) != "SysListView32" {
            return false;
        }
        let mut res: usize = usize::MAX;
        let ok = SendMessageTimeoutW(under, LVM_GETHOTITEM, 0, 0, SMTO_ABORTIFHUNG, 50, &mut res);
        ok != 0 && (res as isize) >= 0
    }
}

/// Should the wallpaper pause? While a fullscreen game / presentation is
/// in front (nobody can see us), and while the session isn't visible at
/// all: locked, screen saver, user switched away, or a secure desktop
/// (lock screen / UAC prompt) has the input. `own` is our window: in the
/// top-level fallback it is itself a borderless monitor-sized window, which
/// Windows reports as a "fullscreen app" once it's in the foreground.
pub fn should_pause(own: HWND) -> bool {
    unsafe {
        // the input desktop isn't ours (Winlogon / secure desktop)
        let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desk.is_null() {
            return true;
        }
        CloseDesktop(desk);
        let mut st: QUERY_USER_NOTIFICATION_STATE = 0;
        if SHQueryUserNotificationState(&mut st) != 0 {
            return false;
        }
        match st {
            // QUNS_NOT_PRESENT: locked, screen saver, fast user switching
            1 => true,
            // QUNS_BUSY (fullscreen app), D3D fullscreen, presentation mode
            2..=4 => own.is_null() || GetForegroundWindow() != own,
            _ => false,
        }
    }
}

/// True while a fullscreen game / presentation is running (used by the
/// window-snow overlay, which hides itself then).
pub fn fullscreen_app() -> bool {
    unsafe {
        let mut st: QUERY_USER_NOTIFICATION_STATE = 0;
        if SHQueryUserNotificationState(&mut st) != 0 {
            return false;
        }
        // 2 = QUNS_BUSY (fullscreen app), 3 = D3D fullscreen, 4 = presentation mode
        matches!(st, 2..=4)
    }
}
