//! Whether the desktop wallpaper should rest: a game running, or the
//! computer working hard (the processors or the graphics card busy for a
//! while). Looked at every couple of seconds on a thread of its own; the
//! front end asks `why` and decides (the desktop itself having the focus
//! keeps the village going).

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How busy counts as busy (a share of everything there is), and how far
/// it has to fall again before the village carries on.
const BUSY: f32 = 0.85;
const CALM: f32 = 0.6;
/// Looks in a row it takes (2 s apart): busy for about 6 s before resting,
/// calm for about 4 s before carrying on.
const BUSY_LOOKS: u32 = 3;
const CALM_LOOKS: u32 = 2;

pub struct Watch {
    why: Arc<Mutex<Option<String>>>,
}

impl Watch {
    /// Why the wallpaper might rest right now (`None`: no reason to).
    pub fn why(&self) -> Option<String> {
        self.why.lock().ok().and_then(|w| w.clone())
    }
}

/// Start watching.
pub fn spawn() -> Watch {
    let why = Arc::new(Mutex::new(None));
    let out = why.clone();
    let _ = std::thread::Builder::new().name("rest-watch".into()).spawn(move || {
        let mut w = Looker::default();
        loop {
            let r = w.look();
            if let Ok(mut o) = out.lock() {
                *o = r;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    Watch { why }
}

/// Busy for a while, calm for a while: a reading that holds before it
/// counts (a moment's spike isn't a game).
#[derive(Default)]
struct Held {
    on: bool,
    run: u32,
}

impl Held {
    fn feed(&mut self, v: Option<f32>) -> bool {
        let Some(v) = v else {
            *self = Held::default();
            return false;
        };
        let flip = if self.on { v < CALM } else { v >= BUSY };
        self.run = if flip { self.run + 1 } else { 0 };
        if self.run >= if self.on { CALM_LOOKS } else { BUSY_LOOKS } {
            self.on ^= true;
            self.run = 0;
        }
        self.on
    }
}

#[derive(Default)]
struct Looker {
    cpu: Held,
    gpu: Held,
    last_cpu: Option<(u64, u64, u64)>,
    game: Option<String>,
    looks: u32,
    /// The last look's readings (processors, graphics card).
    seen: (Option<f32>, Option<f32>),
    #[cfg(windows)]
    pdh: Option<win::Gpu>,
}

impl Looker {
    fn look(&mut self) -> Option<String> {
        // a game: looked for every few seconds (going through every process)
        if self.looks % 3 == 0 {
            self.game = game();
        }
        self.looks += 1;
        let cpu = self.cpu_share();
        let gpu = self.gpu_share();
        self.seen = (cpu, gpu);
        let (c, g) = (self.cpu.feed(cpu), self.gpu.feed(gpu));
        if let Some(name) = &self.game {
            return Some(format!("{name} is running"));
        }
        if g {
            return Some(format!("the graphics card's busy ({:.0}%)", gpu.unwrap_or(1.0) * 100.0));
        }
        if c {
            return Some(format!("the computer's busy ({:.0}%)", cpu.unwrap_or(1.0) * 100.0));
        }
        None
    }

    /// How busy the processors were since the last look, leaving out this
    /// program's own share.
    fn cpu_share(&mut self) -> Option<f32> {
        let now = cpu_times()?;
        let last = self.last_cpu.replace(now)?;
        let (total, busy, own) = (now.0.saturating_sub(last.0), now.1.saturating_sub(last.1), now.2.saturating_sub(last.2));
        (total > 0).then(|| (busy.saturating_sub(own) as f32 / total as f32).clamp(0.0, 1.0))
    }

    fn gpu_share(&mut self) -> Option<f32> {
        #[cfg(target_os = "linux")]
        {
            gpu_busy()
        }
        #[cfg(windows)]
        {
            if self.pdh.is_none() {
                self.pdh = win::Gpu::open();
            }
            self.pdh.as_mut().and_then(|g| g.busy())
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        None
    }
}

/// (everything, busy, this program's own), in the same units.
#[cfg(target_os = "linux")]
fn cpu_times() -> Option<(u64, u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return None;
    }
    // (user nice system idle iowait irq softirq steal: guests are counted in user)
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v[4];
    // this program's own (utime + stime, the 14th and 15th fields, after
    // the name in brackets, which can hold spaces)
    let me = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = &me[me.rfind(')')? + 2..];
    let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let own = f.get(11)? + f.get(12)?;
    Some((total, total - idle, own))
}

#[cfg(windows)]
fn cpu_times() -> Option<(u64, u64, u64)> {
    win::cpu_times()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn cpu_times() -> Option<(u64, u64, u64)> {
    None
}

/// The busiest graphics card's load (AMD and Intel say so in sysfs).
#[cfg(target_os = "linux")]
fn gpu_busy() -> Option<f32> {
    let dir = std::fs::read_dir("/sys/class/drm").ok()?;
    let mut most: Option<f32> = None;
    for e in dir.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        // (card0, card1: not their connectors, nor the render nodes, which
        // are the same cards again)
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        if let Some(v) = std::fs::read_to_string(e.path().join("device/gpu_busy_percent")).ok().and_then(|s| s.trim().parse::<f32>().ok()) {
            most = Some(most.unwrap_or(0.0).max(v / 100.0));
        }
    }
    most
}

/// What marks a running game in a process's command line or path: Steam's
/// (Proton's too: they run from the library), and the other launchers'
/// libraries.
const GAME_MARKS: [&str; 6] = ["/steamapps/common/", "\\steamapps\\common\\", "SteamLaunch", "\\Epic Games\\", "\\GOG Games\\", "/gamescope"];

/// The game running, if one is (its folder's name).
#[cfg(target_os = "linux")]
fn game() -> Option<String> {
    let me = std::process::id().to_string();
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let pid = e.file_name();
        let pid = pid.to_string_lossy();
        if !pid.bytes().all(|b| b.is_ascii_digit()) || pid == me {
            continue;
        }
        let Ok(raw) = std::fs::read(e.path().join("cmdline")) else { continue };
        let cmd = String::from_utf8_lossy(&raw).replace('\0', " ");
        if let Some(name) = game_in(&cmd) {
            return Some(name);
        }
    }
    None
}

#[cfg(windows)]
fn game() -> Option<String> {
    win::game()
}

#[cfg(not(any(target_os = "linux", windows)))]
fn game() -> Option<String> {
    None
}

/// The game a command line (or a program's path) is, if it is one.
fn game_in(cmd: &str) -> Option<String> {
    let mark = GAME_MARKS.iter().find(|m| cmd.contains(**m))?;
    // the tools Steam runs games with aren't the game (Proton, the
    // runtime): only what runs inside them
    let lower = cmd.to_ascii_lowercase();
    if mark.contains("steamapps") {
        let after = &cmd[cmd.find(mark)? + mark.len()..];
        let folder: String = after.chars().take_while(|c| *c != '/' && *c != '\\').collect();
        let f = folder.to_ascii_lowercase();
        if f.starts_with("proton") || f.starts_with("steamlinuxruntime") || f.starts_with("steamworks") || f.is_empty() {
            return None;
        }
        return Some(folder);
    }
    if mark.contains("gamescope") {
        return lower.split_whitespace().next().filter(|p| p.ends_with("gamescope")).map(|_| "a game (gamescope)".to_string());
    }
    if *mark == "SteamLaunch" {
        // (the game's own process says which: this is only Steam's wrapper)
        return None;
    }
    let after = &cmd[cmd.find(mark)? + mark.len()..];
    let folder: String = after.chars().take_while(|c| *c != '\\').collect();
    (!folder.is_empty()).then_some(folder)
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::Performance::{PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};

    fn ticks(f: FILETIME) -> u64 {
        (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64
    }

    pub fn cpu_times() -> Option<(u64, u64, u64)> {
        unsafe {
            let z = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let (mut idle, mut kernel, mut user) = (z(), z(), z());
            if GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 {
                return None;
            }
            // (kernel time includes the idle time)
            let total = ticks(kernel) + ticks(user);
            let busy = total.saturating_sub(ticks(idle));
            let (mut c, mut e, mut k, mut u) = (z(), z(), z(), z());
            let own = if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) != 0 { ticks(k) + ticks(u) } else { 0 };
            // (a process's time is summed over every processor already)
            Some((total, busy, own))
        }
    }

    /// The running game, if any: a program from a game library.
    pub fn game() -> Option<String> {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap as isize == -1 {
                return None;
            }
            let me = std::process::id();
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found = None;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok && found.is_none() {
                if e.th32ProcessID != me && e.th32ProcessID > 4 {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                    if !h.is_null() {
                        let mut buf = [0u16; 1024];
                        let mut n = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0 {
                            let path = String::from_utf16_lossy(&buf[..n as usize]);
                            found = super::game_in(&path);
                        }
                        CloseHandle(h);
                    }
                }
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
            found
        }
    }

    /// The graphics cards' 3D engines, as Windows counts them (each
    /// program's share): the busiest program's, not ours.
    pub struct Gpu {
        query: isize,
        counter: isize,
    }

    // (the handles are only used from the watching thread)
    unsafe impl Send for Gpu {}

    impl Gpu {
        pub fn open() -> Option<Gpu> {
            unsafe {
                let mut q = 0isize;
                if PdhOpenQueryW(std::ptr::null(), 0, &mut q as *mut isize as *mut _) != 0 {
                    return None;
                }
                let path: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage\0".encode_utf16().collect();
                let mut c = 0isize;
                if PdhAddEnglishCounterW(q as _, path.as_ptr(), 0, &mut c as *mut isize as *mut _) != 0 {
                    return None;
                }
                PdhCollectQueryData(q as _);
                Some(Gpu { query: q, counter: c })
            }
        }

        pub fn busy(&mut self) -> Option<f32> {
            unsafe {
                if PdhCollectQueryData(self.query as _) != 0 {
                    return None;
                }
                let (mut size, mut count) = (0u32, 0u32);
                PdhGetFormattedCounterArrayW(self.counter as _, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
                if size == 0 {
                    return None;
                }
                let mut buf = vec![0u8; size as usize];
                if PdhGetFormattedCounterArrayW(self.counter as _, PDH_FMT_DOUBLE, &mut size, &mut count, buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W) != 0 {
                    return None;
                }
                let items = std::slice::from_raw_parts(buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W, count as usize);
                let me = format!("pid_{}_", std::process::id());
                let mut per: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
                for it in items {
                    let mut n = 0;
                    while *it.szName.add(n) != 0 {
                        n += 1;
                    }
                    let name = String::from_utf16_lossy(std::slice::from_raw_parts(it.szName, n));
                    if name.contains(&me) {
                        continue;
                    }
                    let pid = name.split('_').nth(1).unwrap_or("").to_string();
                    *per.entry(pid).or_default() += it.FmtValue.Anonymous.doubleValue;
                }
                let most = per.values().copied().fold(0.0f64, f64::max);
                Some((most / 100.0).clamp(0.0, 1.0) as f32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_are_told_from_the_tools_they_run_in() {
        assert_eq!(game_in("/home/a/.local/share/Steam/steamapps/common/RaceRoom Racing Experience/Game/x64/RRRE64.exe -foo").as_deref(), Some("RaceRoom Racing Experience"));
        assert_eq!(game_in("/home/a/.local/share/Steam/steamapps/common/Proton 9.0/proton waitforexitandrun"), None);
        assert_eq!(game_in("/home/a/.local/share/Steam/steamapps/common/SteamLinuxRuntime_sniper/pressure-vessel/bin/pv-adverb"), None);
        assert_eq!(game_in("/home/a/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId=211500 -- x"), None);
        assert_eq!(game_in("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Celeste\\Celeste.exe").as_deref(), Some("Celeste"));
        assert_eq!(game_in("C:\\Program Files\\Epic Games\\Fortnite\\FortniteGame\\Binaries\\Win64\\x.exe").as_deref(), Some("Fortnite"));
        assert_eq!(game_in("/usr/bin/gamescope -W 1920 -- game").as_deref(), Some("a game (gamescope)"));
        assert_eq!(game_in("/usr/bin/kitty"), None);
    }

    /// What it sees on this computer right now (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn what_it_sees_here() {
        let mut l = Looker::default();
        for _ in 0..3 {
            let why = l.look();
            println!("cpu {:?} gpu {:?} game {:?} → {why:?}", l.seen.0, l.seen.1, l.game);
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    #[test]
    fn busy_has_to_last_and_calm_too() {
        let mut h = Held::default();
        assert!(!h.feed(Some(0.95)) && !h.feed(Some(0.95)), "a moment's not enough");
        assert!(h.feed(Some(0.95)), "busy for three looks");
        assert!(h.feed(Some(0.7)), "still busy-ish: carries on resting");
        assert!(h.feed(Some(0.3)), "one calm look isn't enough");
        assert!(!h.feed(Some(0.3)), "calm for two");
        assert!(!h.feed(None));
    }
}
