//! Windows data sources for the widgets: CPU/RAM/disk/network from Win32,
//! GPU load + VRAM from Performance Counters and DXGI, and now-playing from
//! the system media session API (Spotify, browsers, media players…).

#![cfg(windows)]

use super::data::{push, Music, SysStats};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

fn ft(f: &FILETIME) -> u64 {
    (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64
}

fn cpu_times() -> Option<(u64, u64)> {
    let mut idle = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut kernel, mut user) = (idle, idle);
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    // kernel time includes idle time
    Some((ft(&kernel) + ft(&user), ft(&idle)))
}

fn net_bytes() -> (u64, u64) {
    let (mut rx, mut tx) = (0u64, 0u64);
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return (0, 0);
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        for r in rows {
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            // 24 = loopback, OperStatus 1 = up
            if hardware && r.Type != 24 && r.OperStatus == 1 {
                rx += r.InOctets;
                tx += r.OutOctets;
            }
        }
        FreeMibTable(table as *const _);
    }
    (rx, tx)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A wildcard performance counter, summed or maxed over its instances.
struct Counter {
    h: isize,
}

impl Counter {
    fn add(query: isize, path: &str) -> Option<Counter> {
        let mut h: isize = 0;
        let p = wide(path);
        if unsafe { PdhAddEnglishCounterW(query, p.as_ptr(), 0, &mut h) } != 0 {
            return None;
        }
        Some(Counter { h })
    }

    fn values(&self) -> Vec<(String, f64)> {
        unsafe {
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(self.h, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
            if size == 0 {
                return vec![];
            }
            let mut buf = vec![0u8; size as usize];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.h, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
                return vec![];
            }
            (0..count as usize)
                .map(|i| {
                    let it = &*items.add(i);
                    let mut len = 0;
                    while *it.szName.add(len) != 0 {
                        len += 1;
                    }
                    let name = String::from_utf16_lossy(std::slice::from_raw_parts(it.szName, len));
                    (name, it.FmtValue.Anonymous.doubleValue)
                })
                .collect()
        }
    }
}

/// Largest dedicated VRAM of any adapter (the discrete GPU), via DXGI.
fn vram_total() -> f64 {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let Ok(f) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return 0.0 };
    let mut best = 0usize;
    let mut i = 0;
    while let Ok(a) = unsafe { f.EnumAdapters1(i) } {
        if let Ok(d) = unsafe { a.GetDesc1() } {
            best = best.max(d.DedicatedVideoMemory);
        }
        i += 1;
    }
    best as f64
}

pub fn spawn_sys(disk: String) -> Arc<Mutex<SysStats>> {
    let shared = Arc::new(Mutex::new(SysStats::default()));
    let out = shared.clone();
    thread::spawn(move || {
        // "/" means the system drive on Windows
        let root = if disk == "/" || disk.is_empty() { std::env::var("SystemDrive").unwrap_or("C:".into()) + "\\" } else { disk };
        let root_w = wide(&root);
        let mut query: isize = 0;
        let pdh_ok = unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } == 0;
        let gpu_util = if pdh_ok { Counter::add(query, "\\GPU Engine(*engtype_3D)\\Utilization Percentage") } else { None };
        let gpu_mem = if pdh_ok { Counter::add(query, "\\GPU Adapter Memory(*)\\Dedicated Usage") } else { None };
        if pdh_ok {
            unsafe { PdhCollectQueryData(query) };
        }
        let vram_total = vram_total();
        let mut last_cpu = cpu_times();
        let mut last_net = (net_bytes(), Instant::now());
        let mut st = SysStats::default();
        loop {
            thread::sleep(Duration::from_millis(1000));
            if let (Some((t0, i0)), Some((t1, i1))) = (last_cpu, cpu_times()) {
                let dt = (t1 - t0).max(1) as f32;
                st.cpu = (1.0 - (i1 - i0) as f32 / dt).clamp(0.0, 1.0) * 100.0;
                last_cpu = Some((t1, i1));
                let v = st.cpu;
                push(&mut st.cpu_hist, v);
            }
            unsafe {
                let mut m: MEMORYSTATUSEX = std::mem::zeroed();
                m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
                if GlobalMemoryStatusEx(&mut m) != 0 {
                    st.mem_total = m.ullTotalPhys as f64;
                    st.mem_used = (m.ullTotalPhys - m.ullAvailPhys) as f64;
                }
                let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
                if GetDiskFreeSpaceExW(root_w.as_ptr(), &mut avail, &mut total, &mut free) != 0 {
                    st.disk_total = total as f64;
                    st.disk_used = (total - free) as f64;
                }
            }
            if pdh_ok {
                unsafe { PdhCollectQueryData(query) };
                if let Some(c) = &gpu_util {
                    let sum: f64 = c.values().iter().map(|v| v.1).sum();
                    st.gpu = Some(sum.clamp(0.0, 100.0) as f32);
                    push(&mut st.gpu_hist, sum.clamp(0.0, 100.0) as f32);
                }
                if let Some(c) = &gpu_mem {
                    st.vram_used = c.values().iter().map(|v| v.1).fold(0.0, f64::max);
                    st.vram_total = vram_total;
                }
            }
            let now_net = net_bytes();
            let secs = last_net.1.elapsed().as_secs_f64().max(0.1);
            st.rx = now_net.0.saturating_sub(last_net.0 .0) as f64 / secs;
            st.tx = now_net.1.saturating_sub(last_net.0 .1) as f64 / secs;
            last_net = (now_net, Instant::now());
            let (rx, tx) = (st.rx as f32, st.tx as f32);
            push(&mut st.rx_hist, rx);
            push(&mut st.tx_hist, tx);
            st.ready = true;
            if let Ok(mut o) = out.lock() {
                *o = st.clone();
            }
        }
    });
    shared
}

fn read_music() -> windows::core::Result<Music> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let mgr = Manager::RequestAsync()?.get()?;
    let s = mgr.GetCurrentSession()?;
    let props = s.TryGetMediaPropertiesAsync()?.get()?;
    let playing = s.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
    let tl = s.GetTimelineProperties()?;
    let mut position = tl.Position()?.Duration as f64 / 1e7;
    let length = (tl.EndTime()?.Duration - tl.StartTime()?.Duration) as f64 / 1e7;
    if playing {
        // the timeline is a snapshot from LastUpdatedTime: advance it to now
        let updated = tl.LastUpdatedTime()?.UniversalTime;
        let now = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() / 100) as i64
            + 116_444_736_000_000_000;
        if updated > 0 && now > updated {
            position += (now - updated) as f64 / 1e7;
        }
    }
    let app = s.SourceAppUserModelId()?.to_string_lossy();
    let player = app
        .rsplit(['!', '\\'])
        .next()
        .unwrap_or(&app)
        .trim_end_matches(".exe")
        .split('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let title = props.Title()?.to_string_lossy();
    Ok(Music {
        present: !title.is_empty(),
        title,
        artist: props.Artist()?.to_string_lossy(),
        player,
        playing,
        length,
        // a player can report Playing but stop updating its timeline (e.g. a
        // browser at the end of a track): never run past the track length
        position: if length > 0.0 { position.clamp(0.0, length) } else { position.max(0.0) },
        at: Some(Instant::now()),
    })
}

pub fn spawn_music() -> Arc<Mutex<Music>> {
    let shared = Arc::new(Mutex::new(Music::default()));
    let out = shared.clone();
    thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        loop {
            let m = read_music().unwrap_or_default();
            if let Ok(mut o) = out.lock() {
                *o = m;
            }
            thread::sleep(Duration::from_millis(1500));
        }
    });
    shared
}
