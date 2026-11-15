//! Background data collectors: system stats (/proc + amdgpu sysfs), MPRIS
//! now-playing (via busctl), and weather (Open-Meteo). Each runs on its own
//! thread and publishes into a shared struct.

use crate::config::WeatherCfg;
use serde_json::Value;
use std::collections::VecDeque;
#[cfg(not(windows))]
use std::fs;
#[cfg(not(windows))]
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const HIST: usize = 48;

#[derive(Default, Clone)]
pub struct SysStats {
    pub cpu: f32,
    pub cpu_temp: Option<f32>,
    pub cpu_hist: VecDeque<f32>,
    pub mem_used: f64,
    pub mem_total: f64,
    pub disk_used: f64,
    pub disk_total: f64,
    pub gpu: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub gpu_hist: VecDeque<f32>,
    pub vram_used: f64,
    pub vram_total: f64,
    pub rx: f64,
    pub tx: f64,
    pub rx_hist: VecDeque<f32>,
    pub tx_hist: VecDeque<f32>,
    pub ready: bool,
}

#[derive(Default, Clone)]
pub struct Music {
    pub title: String,
    pub artist: String,
    pub player: String,
    pub playing: bool,
    pub length: f64,
    pub position: f64,
    pub at: Option<Instant>,
    pub present: bool,
}

#[derive(Default, Clone)]
pub struct Weather {
    pub place: String,
    pub temp: f64,
    pub feels: f64,
    pub code: i64,
    pub wind: f64,
    pub humidity: f64,
    pub is_day: bool,
    pub daily: Vec<(String, i64, f64, f64)>,
    pub unit: String,
    pub wind_unit: String,
    pub status: String,
    pub ok: bool,
}

#[cfg(not(windows))]
fn read(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok()
}

#[cfg(not(windows))]
fn read_num(p: impl AsRef<Path>) -> Option<f64> {
    read(p)?.trim().parse().ok()
}

pub fn push(h: &mut VecDeque<f32>, v: f32) {
    if h.len() >= HIST {
        h.pop_front();
    }
    h.push_back(v);
}

#[cfg(not(windows))]
fn cpu_times() -> Option<(u64, u64)> {
    let s = read("/proc/stat")?;
    let line = s.lines().next()?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return None;
    }
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v[4];
    Some((total, idle))
}

#[cfg(not(windows))]
fn net_bytes() -> (u64, u64) {
    let mut rx = 0;
    let mut tx = 0;
    if let Some(s) = read("/proc/net/dev") {
        for line in s.lines().skip(2) {
            let Some((name, rest)) = line.split_once(':') else { continue };
            let name = name.trim();
            if name == "lo" {
                continue;
            }
            let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() >= 9 {
                rx += v[0];
                tx += v[8];
            }
        }
    }
    (rx, tx)
}

#[cfg(not(windows))]
fn find_hwmon(name: &str) -> Option<PathBuf> {
    for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        if read(e.path().join("name")).map(|s| s.trim() == name).unwrap_or(false) {
            return Some(e.path());
        }
    }
    None
}

#[cfg(not(windows))]
/// The discrete GPU = the amdgpu card with the most VRAM.
fn find_gpu() -> Option<PathBuf> {
    let mut best: Option<(f64, PathBuf)> = None;
    for e in fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if !n.starts_with("card") || n.contains('-') {
            continue;
        }
        let dev = e.path().join("device");
        if let Some(v) = read_num(dev.join("mem_info_vram_total")) {
            if best.as_ref().map(|b| v > b.0).unwrap_or(true) {
                best = Some((v, dev));
            }
        }
    }
    best.map(|b| b.1)
}

#[cfg(not(windows))]
fn first_hwmon_temp(dev: &Path) -> Option<f32> {
    for e in fs::read_dir(dev.join("hwmon")).ok()?.flatten() {
        if let Some(v) = read_num(e.path().join("temp1_input")) {
            return Some((v / 1000.0) as f32);
        }
    }
    None
}

#[cfg(not(windows))]
fn statvfs(path: &str) -> Option<(f64, f64)> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let total = s.f_blocks as f64 * s.f_frsize as f64;
    let avail = s.f_bavail as f64 * s.f_frsize as f64;
    let free = s.f_bfree as f64 * s.f_frsize as f64;
    // match `df`: used = total - free, shown against used + avail
    Some((total - free, total - free + avail))
}

#[cfg(not(windows))]
pub fn spawn_sys(disk: String) -> Arc<Mutex<SysStats>> {
    let shared = Arc::new(Mutex::new(SysStats::default()));
    let out = shared.clone();
    thread::spawn(move || {
        let k10 = find_hwmon("k10temp").or_else(|| find_hwmon("coretemp"));
        let gpu = find_gpu();
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
            st.cpu_temp = k10.as_ref().and_then(|p| read_num(p.join("temp1_input"))).map(|v| (v / 1000.0) as f32);
            if let Some(m) = read("/proc/meminfo") {
                let get = |k: &str| -> f64 {
                    m.lines()
                        .find(|l| l.starts_with(k))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0)
                        * 1024.0
                };
                st.mem_total = get("MemTotal:");
                st.mem_used = st.mem_total - get("MemAvailable:");
            }
            if let Some((u, t)) = statvfs(&disk) {
                st.disk_used = u;
                st.disk_total = t;
            }
            if let Some(g) = &gpu {
                st.gpu = read_num(g.join("gpu_busy_percent")).map(|v| v as f32);
                st.vram_used = read_num(g.join("mem_info_vram_used")).unwrap_or(0.0);
                st.vram_total = read_num(g.join("mem_info_vram_total")).unwrap_or(0.0);
                st.gpu_temp = first_hwmon_temp(g);
                let v = st.gpu.unwrap_or(0.0);
                push(&mut st.gpu_hist, v);
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

// ---------------------------------------------------------------------------

#[cfg(not(windows))]
fn busctl(args: &[&str]) -> Option<Value> {
    let out = Command::new("busctl").arg("--user").arg("--json=short").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

#[cfg(not(windows))]
fn player_props(name: &str) -> Option<Value> {
    let v = busctl(&[
        "call",
        name,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        "s",
        "org.mpris.MediaPlayer2.Player",
    ])?;
    v.get("data")?.get(0).cloned()
}

#[cfg(not(windows))]
fn read_music() -> Music {
    let m = Music::default();
    let Some(names) = busctl(&["call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ListNames"]) else {
        return m;
    };
    let mut players: Vec<String> = names["data"][0]
        .as_array()
        .map(|a| a.iter().filter_map(|n| n.as_str()).filter(|n| n.starts_with("org.mpris.MediaPlayer2.")).map(String::from).collect())
        .unwrap_or_default();
    // KDE's browser integration has the cleanest metadata: prefer it
    players.sort_by_key(|n| if n.contains("plasma-browser-integration") { 0 } else { 1 });
    let mut best: Option<(i32, Music)> = None;
    for name in players {
        let Some(p) = player_props(&name) else { continue };
        let status = p["PlaybackStatus"]["data"].as_str().unwrap_or("Stopped");
        let rank = match status {
            "Playing" => 2,
            "Paused" => 1,
            _ => 0,
        };
        let meta = &p["Metadata"]["data"];
        let title = meta["xesam:title"]["data"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }
        let artist = meta["xesam:artist"]["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
            .or_else(|| meta["xesam:artist"]["data"].as_str().map(String::from))
            .unwrap_or_default();
        let length = meta["mpris:length"]["data"].as_f64().unwrap_or(0.0) / 1e6;
        let position = p["Position"]["data"].as_f64().unwrap_or(0.0) / 1e6;
        let short = name.trim_start_matches("org.mpris.MediaPlayer2.");
        let player = if short.contains("plasma-browser-integration") {
            "browser".to_string()
        } else {
            short.split('.').next().unwrap_or(short).to_string()
        };
        let cand = Music { title, artist, player, playing: rank == 2, length, position, at: Some(Instant::now()), present: true };
        if best.as_ref().map(|b| rank > b.0).unwrap_or(true) && rank > 0 {
            best = Some((rank, cand));
        }
    }
    best.map(|b| b.1).unwrap_or(m)
}

impl Music {
    /// Position extrapolated since the last poll.
    pub fn pos_now(&self) -> f64 {
        match (self.playing, self.at) {
            // extrapolate while playing, but never past the end of the track
            (true, Some(at)) if self.length > 0.0 => (self.position + at.elapsed().as_secs_f64()).clamp(0.0, self.length),
            (true, Some(at)) => self.position + at.elapsed().as_secs_f64(),
            _ if self.length > 0.0 => self.position.clamp(0.0, self.length),
            _ => self.position.max(0.0),
        }
    }
}

#[cfg(not(windows))]
pub fn spawn_music() -> Arc<Mutex<Music>> {
    let shared = Arc::new(Mutex::new(Music::default()));
    let out = shared.clone();
    thread::spawn(move || loop {
        let m = read_music();
        if let Ok(mut o) = out.lock() {
            *o = m;
        }
        thread::sleep(Duration::from_millis(1500));
    });
    shared
}

// ---------------------------------------------------------------------------

fn http_json(url: &str) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).user_agent("xmas-tui/0.1").build();
    let s = agent.get(url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn fetch_weather(cfg: &WeatherCfg, coords: &mut Option<(f64, f64, String)>) -> Weather {
    let mut w = Weather::default();
    let f = cfg.units.to_lowercase().starts_with('f');
    w.unit = if f { "°F".into() } else { "°C".into() };
    w.wind_unit = if f { "mph".into() } else { "km/h".into() };
    if coords.is_none() {
        if let (Some(la), Some(lo)) = (cfg.latitude, cfg.longitude) {
            *coords = Some((la, lo, cfg.city.clone()));
        } else if !cfg.city.trim().is_empty() {
            let url = format!("https://geocoding-api.open-meteo.com/v1/search?count=1&language=en&format=json&name={}", urlenc(cfg.city.trim()));
            match http_json(&url) {
                Ok(v) => {
                    if let Some(r) = v["results"].get(0) {
                        let name = r["name"].as_str().unwrap_or(&cfg.city).to_string();
                        *coords = Some((r["latitude"].as_f64().unwrap_or(0.0), r["longitude"].as_f64().unwrap_or(0.0), name));
                    } else {
                        w.status = format!("city not found: {}", cfg.city);
                        return w;
                    }
                }
                Err(e) => {
                    w.status = format!("geocode failed: {e}");
                    return w;
                }
            }
        } else {
            w.status = "set [weather] city in config".into();
            return w;
        }
    }
    let Some((la, lo, name)) = coords.clone() else { return w };
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={la:.3}&longitude={lo:.3}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min&forecast_days=4&timezone=auto{}",
        if f { "&temperature_unit=fahrenheit&wind_speed_unit=mph" } else { "" }
    );
    match http_json(&url) {
        Ok(v) => {
            let c = &v["current"];
            w.place = name;
            w.temp = c["temperature_2m"].as_f64().unwrap_or(0.0);
            w.feels = c["apparent_temperature"].as_f64().unwrap_or(0.0);
            w.code = c["weather_code"].as_i64().unwrap_or(0);
            w.wind = c["wind_speed_10m"].as_f64().unwrap_or(0.0);
            w.humidity = c["relative_humidity_2m"].as_f64().unwrap_or(0.0);
            w.is_day = c["is_day"].as_i64().unwrap_or(0) == 1;
            let d = &v["daily"];
            if let (Some(days), Some(codes), Some(mx), Some(mn)) = (
                d["time"].as_array(),
                d["weather_code"].as_array(),
                d["temperature_2m_max"].as_array(),
                d["temperature_2m_min"].as_array(),
            ) {
                for i in 0..days.len().min(4) {
                    let day = days[i].as_str().unwrap_or("").to_string();
                    w.daily.push((day, codes[i].as_i64().unwrap_or(0), mx[i].as_f64().unwrap_or(0.0), mn[i].as_f64().unwrap_or(0.0)));
                }
            }
            w.ok = true;
        }
        Err(e) => w.status = format!("weather failed: {e}"),
    }
    w
}

pub fn spawn_weather(cfg: WeatherCfg, gen: Arc<AtomicU64>, mine: u64) -> Arc<Mutex<Weather>> {
    let shared = Arc::new(Mutex::new(Weather { status: "loading weather…".into(), ..Default::default() }));
    let out = shared.clone();
    thread::spawn(move || {
        let mut coords = None;
        loop {
            if gen.load(Ordering::SeqCst) != mine {
                return; // superseded by a newer weather config
            }
            let w = fetch_weather(&cfg, &mut coords);
            let ok = w.ok;
            let retry = w.status.starts_with("set ");
            if let Ok(mut o) = out.lock() {
                // keep showing the last good reading if a refresh fails
                if ok || !o.ok {
                    *o = w;
                }
            }
            if retry {
                return;
            }
            let mins = if ok { cfg.refresh_minutes.max(5) } else { 2 };
            for _ in 0..(mins * 12) {
                if gen.load(Ordering::SeqCst) != mine {
                    return;
                }
                thread::sleep(Duration::from_secs(5));
            }
        }
    });
    shared
}
