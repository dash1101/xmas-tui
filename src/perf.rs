//! How much processor time the wallpaper costs: its own, and on Linux the
//! terminal drawing it (kitty, its parent), which without a working
//! graphics driver can cost far more than the village itself. Read now
//! and then by the app to ease off the frame rate on a computer that
//! struggles (see `App::govern`).

/// Processor seconds used so far (all threads): (this process, the
/// terminal drawing it: kitty, when kitty is what runs it). `None` where
/// it can't be read.
pub fn cpu_parts() -> Option<(f64, f64)> {
    #[cfg(target_os = "linux")]
    {
        let own = linux::cpu_of("self")?;
        let parent = std::os::unix::process::parent_id();
        let kitty = std::fs::read_to_string(format!("/proc/{parent}/comm")).ok().filter(|c| c.trim().starts_with("kitty")).and_then(|_| linux::cpu_of(&parent.to_string())).unwrap_or(0.0);
        Some((own, kitty))
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) == 0 {
            return None;
        }
        let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 1e7;
        Some((t(k) + t(u), 0.0))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// The memory it holds (resident), in megabytes. `None` where it can't be
/// read.
pub fn mem_mb() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as f64;
        Some((pages * page / 1048576.0) as f32)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return None;
        }
        Some((c.WorkingSetSize as f64 / 1048576.0) as f32)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
mod linux {
    /// utime + stime of /proc/<pid>/stat, in seconds.
    pub fn cpu_of(pid: &str) -> Option<f64> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // (after the command's name, which may hold spaces, in brackets)
        let rest = &s[s.rfind(')')? + 2..];
        let f: Vec<&str> = rest.split_whitespace().collect();
        let (u, k): (f64, f64) = (f.get(11)?.parse().ok()?, f.get(12)?.parse().ok()?);
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        Some((u + k) / hz)
    }
}

#[cfg(test)]
mod tests {
    /// The processor time grows as the process works, and reads back sane.
    #[test]
    fn processor_time_is_read() {
        let Some(a) = super::cpu_parts().map(|(o, k)| o + k) else { return };
        let t = std::time::Instant::now();
        let mut x = 0u64;
        while t.elapsed().as_millis() < 200 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        std::hint::black_box(x);
        let b = super::cpu_parts().map(|(o, k)| o + k).unwrap();
        assert!(b > a && b - a < 5.0, "{a} → {b}");
    }
}
