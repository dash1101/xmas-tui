//! Self-update from the project's GitHub releases.
//!
//! A background thread checks the latest release now and then (a little
//! after start, then every few hours). When there's a newer version it
//! either asks (the default: a small banner and a button in settings) or,
//! with `[updates] auto_install = true`, installs it straight away.
//!
//! Installing downloads this platform's binary from the release, checks it
//! against the release's `SHA256SUMS`, and swaps it in place of the running
//! executable:
//! * Linux: written next to it and renamed over it (atomic; the running
//!   process keeps the old file). Then the app exits — the systemd service
//!   starts the new one — or, run by hand, re-executes itself.
//! * Windows: a running .exe can't be overwritten but can be renamed, so the
//!   old one becomes `xmas-tui.old.exe` (deleted on the next start), the new
//!   one takes its name, and is started with `--after-update <pid>` so it
//!   waits for this one to exit first.
//!
//! Only release builds update themselves (CI sets `XMAS_RELEASE`); a build
//! from source never replaces itself.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const REPO: &str = "dash1101/xmas-tui";

/// This platform's binary in a release.
#[cfg(windows)]
pub const ASSET: &str = "xmas-tui-windows-x86_64.exe";
#[cfg(not(windows))]
pub const ASSET: &str = "xmas-tui-linux-x86_64";

const FIRST_CHECK: Duration = Duration::from_secs(25);
const EVERY: Duration = Duration::from_secs(6 * 3600);
const MAX_SIZE: u64 = 96 << 20;

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Does this build update itself?
pub fn supported() -> bool {
    option_env!("XMAS_RELEASE").is_some() || std::env::var_os("XMAS_UPDATE_TEST").is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// A build from source: no self-updates.
    Unsupported,
    /// Installed system-wide (e.g. the .deb): the package manager updates it.
    Managed,
    Idle,
    Checking,
    UpToDate,
    /// A newer version is out (asking first).
    Available(String),
    Installing(String),
    /// Installed; restarting into it.
    Installed(String),
    Failed(String),
}

struct Shared {
    status: Status,
    check: bool,
    auto: bool,
    check_now: bool,
    install_now: bool,
    /// The user said "later" to this version: don't show the banner again.
    dismissed: Option<String>,
}

fn shared() -> &'static Mutex<Shared> {
    static S: OnceLock<Mutex<Shared>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Shared {
            status: if supported() { Status::Idle } else { Status::Unsupported },
            check: true,
            auto: false,
            check_now: false,
            install_now: false,
            dismissed: None,
        })
    })
}

static RESTART: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

fn with<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    let mut g = shared().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

pub fn status() -> Status {
    with(|s| s.status.clone())
}

/// A version waiting for a yes, unless the user said "later" to it.
pub fn offer() -> Option<String> {
    with(|s| match &s.status {
        Status::Available(v) if s.dismissed.as_deref() != Some(v.as_str()) => Some(v.clone()),
        _ => None,
    })
}

pub fn dismiss() {
    with(|s| {
        if let Status::Available(v) = &s.status {
            s.dismissed = Some(v.clone());
        }
    });
}

pub fn configure(check: bool, auto: bool) {
    with(|s| {
        s.check = check;
        s.auto = auto;
    });
}

pub fn check_now() {
    with(|s| s.check_now = true);
}

pub fn install_now() {
    with(|s| s.install_now = true);
}

/// The new version is in place: the front-end should shut down cleanly and
/// call [`relaunch`].
pub fn restart_requested() -> bool {
    RESTART.load(Ordering::SeqCst)
}

/// Is the checker running in this process (the wallpaper, not the
/// separate settings window)?
pub fn running() -> bool {
    STARTED.load(Ordering::SeqCst)
}

/// Start the background checker (once per process; release builds only).
pub fn spawn(check: bool, auto: bool) {
    configure(check, auto);
    if !supported() || STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if !writable() {
        with(|s| s.status = Status::Managed);
        return;
    }
    cleanup_old();
    let _ = std::thread::Builder::new().name("updates".into()).spawn(run);
}

fn run() {
    let start = Instant::now();
    let mut last: Option<Instant> = None;
    let mut found: Option<Release> = None;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let (check, auto, now_c, now_i) = with(|s| (s.check, s.auto, std::mem::take(&mut s.check_now), std::mem::take(&mut s.install_now)));
        let due = match last {
            None => start.elapsed() >= FIRST_CHECK,
            Some(l) => l.elapsed() >= EVERY,
        };
        if now_c || (check && due) {
            last = Some(Instant::now());
            with(|s| s.status = Status::Checking);
            match latest() {
                Ok(Some(r)) => {
                    with(|s| s.status = Status::Available(r.version.clone()));
                    found = Some(r);
                }
                Ok(None) => {
                    found = None;
                    with(|s| s.status = Status::UpToDate);
                }
                Err(e) => with(|s| s.status = Status::Failed(e)),
            }
        }
        if let Some(r) = &found {
            if now_i || auto {
                with(|s| s.status = Status::Installing(r.version.clone()));
                match install(r) {
                    Ok(()) => {
                        with(|s| s.status = Status::Installed(r.version.clone()));
                        RESTART.store(true, Ordering::SeqCst);
                        return;
                    }
                    Err(e) => {
                        // don't retry an automatic install in a tight loop
                        found = None;
                        with(|s| s.status = Status::Failed(e));
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ GitHub

struct Release {
    version: String,
    binary: String,
    sums: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(60)).user_agent(concat!("xmas-tui/", env!("CARGO_PKG_VERSION"))).build()
}

/// The latest release, if it's newer than this build and has our binary.
fn latest() -> Result<Option<Release>, String> {
    // XMAS_UPDATE_API: a stand-in for api.github.com, for testing only
    let api = std::env::var("XMAS_UPDATE_API").ok().filter(|_| std::env::var_os("XMAS_UPDATE_TEST").is_some());
    let url = format!("{}/repos/{REPO}/releases/latest", api.as_deref().unwrap_or("https://api.github.com"));
    let body = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("couldn't reach GitHub ({e})"))?
        .into_string()
        .map_err(|e| format!("odd reply from GitHub ({e})"))?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("odd reply from GitHub ({e})"))?;
    let tag = v["tag_name"].as_str().unwrap_or("");
    let version = tag.trim_start_matches('v').to_string();
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) || !newer(&version, current()) {
        return Ok(None);
    }
    let asset = |name: &str| {
        v["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(name))).and_then(|x| x["browser_download_url"].as_str()).map(String::from)
    };
    match (asset(ASSET), asset("SHA256SUMS")) {
        (Some(binary), Some(sums)) => Ok(Some(Release { version, binary, sums })),
        _ => Ok(None), // nothing for this platform in that release
    }
}

/// `xmas-tui --update`: check and install right now, from a terminal.
pub fn cli() -> i32 {
    if !supported() {
        println!("xmas-tui {} was built from source — update it with git pull + rebuild.", current());
        return 0;
    }
    println!("xmas-tui {} — checking {REPO} for a newer release…", current());
    match latest() {
        Ok(None) => {
            println!("up to date.");
            0
        }
        Ok(Some(r)) => {
            println!("installing {}…", r.version);
            match install(&r) {
                Ok(()) => {
                    println!("installed {} — restart xmas-tui to use it (the wallpaper service does this on its own).", r.version);
                    0
                }
                Err(e) => {
                    println!("not installed: {e}");
                    1
                }
            }
        }
        Err(e) => {
            println!("{e}");
            1
        }
    }
}

/// Is `a` a newer version than `b`? (major.minor.patch; anything after a
/// `-` makes it a pre-release, which never counts as newer.)
pub fn newer(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        if v.contains('-') {
            return None;
        }
        let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
        Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
    }
    match (parse(a), parse(b.split('-').next().unwrap_or(b))) {
        (Some(x), Some(y)) => x > y,
        _ => false,
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let resp = agent().get(url).call().map_err(|e| format!("download failed ({e})"))?;
    let mut buf = vec![];
    resp.into_reader().take(MAX_SIZE).read_to_end(&mut buf).map_err(|e| format!("download failed ({e})"))?;
    Ok(buf)
}

fn install(r: &Release) -> Result<(), String> {
    let sums = String::from_utf8(download(&r.sums)?).map_err(|_| "SHA256SUMS isn't text".to_string())?;
    let want = expected_hash(&sums, ASSET).ok_or_else(|| format!("{ASSET} isn't listed in SHA256SUMS"))?;
    let bin = download(&r.binary)?;
    let got = hex(&sha256(&bin));
    if got != want {
        return Err("the download didn't match its checksum".into());
    }
    let exe = current_exe()?;
    replace(&exe, &bin)
}

/// The hash for `name` in a `sha256sum`-style listing.
pub fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n.trim_start_matches('*') == name && h.len() == 64).then(|| h.to_ascii_lowercase())
    })
}

fn current_exe() -> Result<PathBuf, String> {
    let p = std::env::current_exe().map_err(|e| format!("can't find myself ({e})"))?;
    // Linux: a binary already swapped out reads as "… (deleted)"
    let s = p.to_string_lossy();
    Ok(PathBuf::from(s.strip_suffix(" (deleted)").unwrap_or(&s).to_string()))
}

#[cfg(not(windows))]
fn replace(exe: &Path, bin: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = exe.with_file_name(".xmas-tui.update");
    let fail = |e: std::io::Error| format!("couldn't write {} ({e})", exe.display());
    std::fs::write(&tmp, bin).map_err(fail)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    std::fs::rename(&tmp, exe).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        fail(e)
    })
}

#[cfg(windows)]
fn old_path(exe: &Path) -> PathBuf {
    exe.with_file_name("xmas-tui.old.exe")
}

#[cfg(windows)]
fn replace(exe: &Path, bin: &[u8]) -> Result<(), String> {
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| format!("couldn't move the old version aside ({e})"))?;
    if let Err(e) = std::fs::write(exe, bin) {
        let _ = std::fs::rename(&old, exe);
        return Err(format!("couldn't write {} ({e})", exe.display()));
    }
    Ok(())
}

/// Can this process replace its own executable? (Not when installed
/// system-wide, e.g. /usr/bin from the .deb.)
fn writable() -> bool {
    let Ok(exe) = current_exe() else { return false };
    let probe = exe.with_file_name(".xmas-tui.write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Remove what a previous update left behind.
fn cleanup_old() {
    #[cfg(windows)]
    if let Ok(exe) = current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

/// Start the new version (call after shutting down cleanly). Returns when
/// the caller should exit; on Linux outside systemd it doesn't return.
pub fn relaunch() {
    let Ok(exe) = current_exe() else { return };
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--after-update").collect();
    #[cfg(unix)]
    {
        // under the systemd service: just exit, it starts the new binary
        if std::env::var_os("INVOCATION_ID").is_some() {
            return;
        }
        use std::os::unix::process::CommandExt;
        let e = std::process::Command::new(&exe).args(&args).exec();
        eprintln!("xmas-tui: restart after update failed: {e}");
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new(&exe).args(&args).arg("--after-update").arg(std::process::id().to_string()).spawn();
    }
}

/// `--after-update <pid>`: wait (a few seconds at most) for the previous
/// version to exit before taking over.
pub fn wait_for(pid: u32) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, 10_000);
            CloseHandle(h);
        }
    }
    #[cfg(not(windows))]
    {
        for _ in 0..100 {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

// ------------------------------------------------------------------ SHA-256

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SHA-256 (FIPS 180-4), small and dependency-free.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74,
        0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d,
        0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e,
        0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(hex(&sha256(long)), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        let million = vec![b'a'; 1_000_000];
        assert_eq!(hex(&sha256(&million)), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn versions_compare_and_sums_parse() {
        assert!(newer("1.0.1", "1.0.0"));
        assert!(newer("1.10.0", "1.9.9"));
        assert!(newer("2.0", "1.9.9"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("0.9.9", "1.0.0"));
        assert!(!newer("1.1.0-beta", "1.0.0"));
        assert!(!newer("garbage", "1.0.0"));
        let sums = format!("{}  xmas-tui-linux-x86_64\n{} *xmas-tui-windows-x86_64.exe\n", "a".repeat(64), "B".repeat(64));
        assert_eq!(expected_hash(&sums, "xmas-tui-linux-x86_64"), Some("a".repeat(64)));
        assert_eq!(expected_hash(&sums, "xmas-tui-windows-x86_64.exe"), Some("b".repeat(64)));
        assert_eq!(expected_hash(&sums, "nope"), None);
    }
}
