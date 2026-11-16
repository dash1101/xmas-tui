//! Windows: the desktop's own icons, for the shelves. Through the shell's
//! documented view of the desktop (ShellWindows → the desktop's browser →
//! its IFolderView): which files are on it and where, which are selected,
//! moving files between the Desktop and a shelf's folder (IFileOperation,
//! so Explorer's Ctrl+Z can undo it), and placing an icon at a point.
//!
//! Every call talks to Explorer across processes, so they run on a shelf
//! worker thread, never the render thread. The caller initializes COM
//! (apartment-threaded) on that thread first: see `com_init`.

use windows::core::{Interface, HSTRING, PCWSTR, VARIANT};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IFolderView, IShellBrowser, IShellItem, IShellWindows, SHCreateItemFromParsingName, SHGetNameFromIDList, ShellWindows, CSIDL_DESKTOP, FOFX_ADDUNDORECORD, FOF_ALLOWUNDO, FOF_NOCONFIRMMKDIR,
    FOF_RENAMEONCOLLISION, FOF_SILENT, SID_STopLevelBrowser, SIGDN_FILESYSPATH, SVGIO_ALLVIEW, SVGIO_SELECTION, SVSI_POSITIONITEM, SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW};

/// COM for this thread (the shell's objects want an apartment thread).
pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// A file icon on the desktop: its path and where it sits (screen pixels,
/// the top-left of the icon's cell).
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub path: String,
    pub at: (i32, i32),
}

/// The desktop's folder view.
pub struct Desktop {
    fv: IFolderView,
    /// The view's client origin on screen (item positions are relative to it).
    origin: (i32, i32),
}

/// The desktop list view's client origin on screen.
fn list_origin() -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    unsafe {
        let wide = |s: &str| HSTRING::from(s);
        // SHELLDLL_DefView sits under Progman, or under a WorkerW once a
        // wallpaper engine has split the desktop
        let mut defview = windows::Win32::Foundation::HWND::default();
        if let Ok(progman) = FindWindowW(&wide("Progman"), PCWSTR::null()) {
            defview = FindWindowExW(progman, None, &wide("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default();
        }
        if defview.is_invalid() {
            let mut w = FindWindowExW(None, None, &wide("WorkerW"), PCWSTR::null()).unwrap_or_default();
            while !w.is_invalid() {
                if let Ok(d) = FindWindowExW(w, None, &wide("SHELLDLL_DefView"), PCWSTR::null()) {
                    defview = d;
                    break;
                }
                w = FindWindowExW(None, w, &wide("WorkerW"), PCWSTR::null()).unwrap_or_default();
            }
        }
        let list = FindWindowExW(defview, None, &wide("SysListView32"), PCWSTR::null()).unwrap_or_default();
        let mut p = POINT::default();
        if !list.is_invalid() {
            let _ = ClientToScreen(list, &mut p);
        }
        (p.x, p.y)
    }
}

impl Desktop {
    pub fn open() -> windows::core::Result<Desktop> {
        unsafe {
            let sw: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
            let loc = VARIANT::from(CSIDL_DESKTOP as i32);
            let empty = VARIANT::default();
            let mut hwnd = 0i32;
            let disp = sw.FindWindowSW(&loc, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
            let sp: IServiceProvider = disp.cast()?;
            let browser: IShellBrowser = sp.QueryService(&SID_STopLevelBrowser)?;
            let view = browser.QueryActiveShellView()?;
            let fv: IFolderView = view.cast()?;
            Ok(Desktop { fv, origin: list_origin() })
        }
    }

    /// File icons on the desktop (all of them, or just the selected ones).
    pub fn icons(&self, selected: bool) -> Vec<Icon> {
        let mut out = vec![];
        unsafe {
            let which = if selected { SVGIO_SELECTION } else { SVGIO_ALLVIEW };
            let n = self.fv.ItemCount(which).unwrap_or(0);
            for i in 0..n {
                let pidl = if selected {
                    // the selection's i-th item: walk the selection enumerator
                    match self.selected_pidl(i) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    match self.fv.Item(i) {
                        Ok(p) => p,
                        Err(_) => continue,
                    }
                };
                if let Some(path) = path_of(pidl) {
                    let at = self.fv.GetItemPosition(pidl).map(|p| (p.x + self.origin.0, p.y + self.origin.1)).unwrap_or((0, 0));
                    out.push(Icon { path, at });
                }
                CoTaskMemFree(Some(pidl as *const _));
            }
        }
        out
    }

    unsafe fn selected_pidl(&self, i: i32) -> Option<*mut ITEMIDLIST> {
        use windows::Win32::UI::Shell::IEnumIDList;
        let en: IEnumIDList = self.fv.Items(SVGIO_SELECTION).ok()?;
        let mut k = 0;
        loop {
            let mut one = [std::ptr::null_mut(); 1];
            let mut got = 0u32;
            if en.Next(&mut one, Some(&mut got)).is_err() || got == 0 {
                return None;
            }
            if k == i {
                return Some(one[0]);
            }
            CoTaskMemFree(Some(one[0] as *const _));
            k += 1;
        }
    }

    /// Put the icon for `path` at a screen point (no effect with "Auto
    /// arrange icons" on). False if it isn't on the desktop (yet).
    pub fn place(&self, path: &str, at: (i32, i32)) -> bool {
        unsafe {
            let n = self.fv.ItemCount(SVGIO_ALLVIEW).unwrap_or(0);
            for i in 0..n {
                let Ok(pidl) = self.fv.Item(i) else { continue };
                let hit = path_of(pidl).is_some_and(|p| p.eq_ignore_ascii_case(path));
                if hit {
                    let pt = POINT { x: at.0 - self.origin.0, y: at.1 - self.origin.1 };
                    let list = [pidl as *const ITEMIDLIST];
                    let ok = self.fv.SelectAndPositionItems(1, list.as_ptr(), Some(&pt), SVSI_POSITIONITEM.0 as u32).is_ok();
                    CoTaskMemFree(Some(pidl as *const _));
                    return ok;
                }
                CoTaskMemFree(Some(pidl as *const _));
            }
        }
        false
    }

    /// The icon cell size (pixels).
    pub fn spacing(&self) -> (i32, i32) {
        unsafe {
            let mut p = POINT::default();
            let _ = self.fv.GetSpacing(&mut p);
            (p.x, p.y)
        }
    }
}

fn path_of(pidl: *const ITEMIDLIST) -> Option<String> {
    unsafe {
        let p = SHGetNameFromIDList(pidl, SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.filter(|s| !s.is_empty())
    }
}

/// Move files into a folder (made if missing), as Explorer would: a name
/// already taken gets " (2)", and Ctrl+Z on the desktop undoes it. Returns
/// how many moved.
pub fn move_into(paths: &[String], dir: &str) -> windows::core::Result<usize> {
    let _ = std::fs::create_dir_all(dir);
    unsafe {
        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
        op.SetOperationFlags(FOF_RENAMEONCOLLISION | FOF_NOCONFIRMMKDIR | FOF_ALLOWUNDO | FOF_SILENT | FOFX_ADDUNDORECORD)?;
        let dest: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(dir), None)?;
        let mut n = 0;
        for p in paths {
            if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(p.as_str()), None) {
                if op.MoveItem(&item, &dest, PCWSTR::null(), None).is_ok() {
                    n += 1;
                }
            }
        }
        if n > 0 {
            op.PerformOperations()?;
        }
        Ok(n)
    }
}

/// The user's Desktop folder.
pub fn desktop_dir() -> Option<String> {
    use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// The user's Documents folder.
pub fn documents_dir() -> Option<String> {
    use windows::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// `--shelf-probe [move PATH DIR | place PATH X Y]`: try the desktop
/// calls from the command line. The exe has no console, so it writes to
/// %TEMP%\xmas-shelf-probe.txt.
pub fn probe(args: &[String]) {
    let path = std::env::temp_dir().join("xmas-shelf-probe.txt");
    let mut out = String::new();
    probe_into(args, &mut out);
    let _ = std::fs::write(path, out);
}

fn probe_into(args: &[String], out: &mut String) {
    use std::fmt::Write;
    macro_rules! println {
        ($($t:tt)*) => {{ let _ = writeln!(out, $($t)*); }};
    }
    com_init();
    let d = match Desktop::open() {
        Ok(d) => d,
        Err(e) => {
            println!("can't reach the desktop view: {e}");
            return;
        }
    };
    println!("view origin {:?}, spacing {:?}, desktop {:?}, documents {:?}", d.origin, d.spacing(), desktop_dir(), documents_dir());
    match args.first().map(|s| s.as_str()) {
        Some("move") if args.len() >= 3 => println!("moved {:?}", move_into(&[args[1].clone()], &args[2])),
        Some("place") if args.len() >= 4 => {
            let (x, y) = (args[2].parse().unwrap_or(0), args[3].parse().unwrap_or(0));
            println!("placed: {}", d.place(&args[1], (x, y)));
        }
        _ => {}
    }
    for i in d.icons(false) {
        println!("icon {:?} at {:?}", i.path, i.at);
    }
    for i in d.icons(true) {
        println!("selected {:?}", i.path);
    }
}

/// An item's icon as the desktop shows it (or a picture's thumbnail), as
/// straight-alpha RGBA, about `size` pixels.
pub fn icon(path: &std::path::Path, size: i32) -> Option<crate::frame::Picture> {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SIIGBF_BIGGERSIZEOK};
    unsafe {
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let hbm = item.GetImage(SIZE { cx: size, cy: size }, SIIGBF_BIGGERSIZEOK).ok()?;
        let mut bm = BITMAP::default();
        GetObjectW(hbm, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut core::ffi::c_void));
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        if w <= 0 || h <= 0 || w > 1024 || h > 1024 {
            let _ = DeleteObject(hbm);
            return None;
        }
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let n = GetDIBits(dc, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut core::ffi::c_void), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbm);
        if n == 0 {
            return None;
        }
        // BGRA, premultiplied (or no alpha at all) → straight RGBA
        let any_alpha = buf.chunks(4).any(|p| p[3] != 0);
        for p in buf.chunks_mut(4) {
            let (b, g, r, a) = (p[0], p[1], p[2], if any_alpha { p[3] } else { 255 });
            let un = |c: u8| if a == 0 || a == 255 || !any_alpha { c } else { ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
            p[0] = un(r);
            p[1] = un(g);
            p[2] = un(b);
            p[3] = a;
        }
        Some(crate::frame::Picture::new(w as usize, h as usize, buf))
    }
}

/// Windows' file picker, for one file (modal; COM initialized).
pub fn pick_file(title: &str) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH};
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let opts = dlg.GetOptions().ok()?;
        dlg.SetOptions(opts | FOS_FORCEFILESYSTEM).ok()?;
        let _ = dlg.SetTitle(&HSTRING::from(title));
        dlg.Show(None).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(std::path::PathBuf::from)
    }
}

/// Windows' folder picker (modal; on a thread with COM initialized).
pub fn pick_folder(start: &std::path::Path) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let opts = dlg.GetOptions().ok()?;
        dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let _ = dlg.SetTitle(&HSTRING::from("A folder for the shelf"));
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(start.as_os_str()), None) {
            let _ = dlg.SetFolder(&item);
        }
        dlg.Show(None).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(std::path::PathBuf::from)
    }
}
