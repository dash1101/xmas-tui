//! The faces of the widgets the web page and the phone app show too (the
//! clock and its countdown, the weather, the friends): drawn the same
//! everywhere. (The desktop's widgets module draws them with these; the
//! others include this file on its own.)

#![allow(dead_code)]

use super::Rect;
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::noise1;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};

pub use crate::scene::online::FriendsView;

pub const PANEL: Bg = Bg::Mix(pal::UI_PANEL, 0.80);

/// The weather as last fetched (Open-Meteo).
#[derive(Clone, Debug, Default)]
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

/// Open-Meteo's forecast for a place (`fahrenheit`: °F and mph), the URL.
pub fn forecast_url(la: f64, lo: f64, fahrenheit: bool) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={la:.3}&longitude={lo:.3}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset&forecast_days=4&timezone=auto{}",
        if fahrenheit { "&temperature_unit=fahrenheit&wind_speed_unit=mph" } else { "" }
    )
}

/// The weather from Open-Meteo's forecast (JSON) for `place`, and today's
/// sunrise and sunset (minutes into the day), if it says.
pub fn weather_from(v: &serde_json::Value, place: &str, fahrenheit: bool) -> (Weather, Option<(f32, f32)>) {
    let mut w = Weather { unit: if fahrenheit { "°F".into() } else { "°C".into() }, wind_unit: if fahrenheit { "mph".into() } else { "km/h".into() }, ..Default::default() };
    let c = &v["current"];
    if c.is_null() {
        w.status = "no weather".into();
        return (w, None);
    }
    w.place = place.to_string();
    w.temp = c["temperature_2m"].as_f64().unwrap_or(0.0);
    w.feels = c["apparent_temperature"].as_f64().unwrap_or(0.0);
    w.code = c["weather_code"].as_i64().unwrap_or(0);
    w.wind = c["wind_speed_10m"].as_f64().unwrap_or(0.0);
    w.humidity = c["relative_humidity_2m"].as_f64().unwrap_or(0.0);
    w.is_day = c["is_day"].as_i64().unwrap_or(0) == 1;
    let d = &v["daily"];
    // today's sunrise / sunset (local "YYYY-MM-DDTHH:MM")
    let mins = |s: &serde_json::Value| -> Option<f32> {
        let t = s.as_str()?.split('T').nth(1)?;
        let (h, m) = t.split_once(':')?;
        Some(h.parse::<f32>().ok()? * 60.0 + m.get(..2)?.parse::<f32>().ok()?)
    };
    let sun = match (mins(&d["sunrise"][0]), mins(&d["sunset"][0])) {
        (Some(r), Some(s)) => Some((r, s)),
        _ => None,
    };
    if let (Some(days), Some(codes), Some(mx), Some(mn)) = (d["time"].as_array(), d["weather_code"].as_array(), d["temperature_2m_max"].as_array(), d["temperature_2m_min"].as_array()) {
        for i in 0..days.len().min(4).min(codes.len()).min(mx.len()).min(mn.len()) {
            let day = days[i].as_str().unwrap_or("").to_string();
            w.daily.push((day, codes[i].as_i64().unwrap_or(0), mx[i].as_f64().unwrap_or(0.0), mn[i].as_f64().unwrap_or(0.0)));
        }
    }
    w.ok = true;
    (w, sun)
}

pub fn draw_clock(f: &mut Frame, x: i32, y: i32, clock: &crate::config::ClockCfg, t: f32) -> i32 {
    let now = crate::clock::now();
    let (h, ampm) = if clock.hour24 {
        (now.hour(), "")
    } else {
        let h = now.hour() % 12;
        (if h == 0 { 12 } else { h }, if now.hour() < 12 { "AM" } else { "PM" })
    };
    let s = format!("{}:{:02}", h, now.minute());
    let warm = Rgb::hex(0xfff1d6);
    // keep the clock's backdrop clean: no stars or snow between the
    // digits (trees, houses, their lights and speech bubbles stay)
    for yy in y..(y + 11) {
        for xx in x..(x + 66) {
            f.clear_speck(xx, yy);
        }
    }
    let mut cx = x;
    for ch in s.chars() {
        let rows = digit(ch);
        let wcols = if ch == ':' { 1 } else { 5 };
        for (ry, bits) in rows.iter().enumerate() {
            for bx in 0..wcols {
                if bits >> (wcols - 1 - bx) & 1 == 1 {
                    let shimmer = 0.92 + 0.08 * noise1(t * 0.8 + (cx + bx as i32 * 2) as f32 * 0.15, 3);
                    let col = warm * shimmer;
                    for k in 0..2 {
                        let gx = cx + bx as i32 * 2 + k;
                        let gy = y + ry as i32;
                        f.put_g(gx, gy, Glyph::new('█', col).lit(0.0));
                        f.light_cell(gx, gy, Rgb::hex(0xffc878) * 0.16);
                    }
                }
            }
        }
        cx += wcols as i32 * 2 + 2;
    }
    let tx = cx;
    if clock.seconds {
        // the seconds: small block digits, as warm as the big ones, along
        // the bottom (AM / PM above them)
        if !ampm.is_empty() {
            f.text(tx, y + 1, ampm, warm * 0.8, Bg::Scene, 0.0);
        }
        let sec = format!("{:02}", now.second());
        for (k, ch) in sec.chars().enumerate() {
            draw_small_digit(f, tx + k as i32 * 4, y + 4, ch, warm);
        }
    } else if !ampm.is_empty() {
        f.text(tx, y + 5, ampm, pal::UI_BORDER, Bg::Scene, 0.0);
    }
    let date = now.format("%A, %B %-d").to_string();
    f.text(x, y + 8, &date, pal::UI_TEXT, Bg::Scene, 0.0);
    let mut row = y + 9;
    if clock.countdown {
        draw_countdown(f, x, row, t);
        row += 1;
    }
    row + 1
}

/// A small 3×5 digit drawn in half-block pixels: three cells wide, three
/// tall (its top at row y).
pub fn draw_small_digit(f: &mut Frame, x: i32, y: i32, ch: char, col: Rgb) {
    const FONT: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 2, 2],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    let Some(d) = ch.to_digit(10) else { return };
    let rows = FONT[d as usize];
    let px = |r: usize, c: i32| r < 5 && rows[r] >> (2 - c) & 1 == 1;
    for cy in 0..3 {
        for c in 0..3 {
            let (top, bot) = (px(cy * 2, c), px(cy * 2 + 1, c));
            let g = match (top, bot) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                _ => continue,
            };
            f.put_g(x + c, y + cy as i32, Glyph::new(g, col).lit(0.0));
            f.light_cell(x + c, y + cy as i32, Rgb::hex(0xffc878) * 0.1);
        }
    }
}

pub fn digit(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        ':' => [0, 0, 1, 0, 1, 0, 0],
        _ => [0; 7],
    }
}

pub fn draw_countdown(f: &mut Frame, x: i32, y: i32, t: f32) {
    let now = crate::clock::now();
    let year = now.year();
    let xmas = |y: i32| Local.from_local_datetime(&NaiveDate::from_ymd_opt(y, 12, 25).unwrap().and_hms_opt(0, 0, 0).unwrap()).single();
    let red = pal::CANDY_RED;
    let gold = Rgb::hex(0xffd36b);
    let sparkle = if (t * 2.0) as i32 % 2 == 0 { '✦' } else { '✧' };
    if now.month() == 12 && now.day() == 25 {
        let msg = format!("{sparkle} Merry Christmas! {sparkle}");
        for (i, ch) in msg.chars().enumerate() {
            let c = if (i as i32 + (t * 4.0) as i32) % 2 == 0 { red * 1.2 } else { Rgb::hex(0x3cd070) };
            f.put_g(x + i as i32, y, Glyph::new(ch, c).lit(0.0));
        }
        return;
    }
    let target = match xmas(year) {
        Some(d) if d > now => d,
        _ => match xmas(year + 1) {
            Some(d) => d,
            None => return,
        },
    };
    let left = target - now;
    let secs = left.num_seconds().max(0);
    let (d, h, m, s) = (secs / 86400, (secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let mut cx = f.text(x, y, &format!("{sparkle} "), gold, Bg::Scene, 0.0);
    cx = f.text(cx, y, &format!("{d}"), red * 1.25, Bg::Scene, 0.0);
    cx = f.text(cx, y, if d == 1 { " day " } else { " days " }, pal::UI_TEXT, Bg::Scene, 0.0);
    cx = f.text(cx, y, &format!("{h:02}:{m:02}:{s:02}"), gold, Bg::Scene, 0.0);
    f.text(cx, y, " until Christmas", pal::UI_TEXT, Bg::Scene, 0.0);
}

/// The friends widget's rows: (row, what a click there does). The title
/// row is left alone (it picks the widget up).
pub fn friends_rows(r: Rect, v: &FriendsView) -> Vec<(i32, FriendsHit)> {
    let mut out = vec![];
    // offline: going online, the public villages, and the friends as they
    // were (nobody's about)
    if !v.online {
        let mut y = r.y + 1;
        for h in [FriendsHit::GoOnline, FriendsHit::Public] {
            if y < r.y + r.h - 1 {
                out.push((y, h));
                y += 1;
            }
        }
        for (_, _, i) in &v.friends {
            if y >= r.y + r.h - 1 {
                break;
            }
            out.push((y, FriendsHit::Friend(*i)));
            y += 1;
        }
        return out;
    }
    let inner = (r.h - 2).max(0);
    // the messages take the bottom rows (up to three), the friends the rest
    let lines = (v.lines.len() as i32).min(3).min(inner / 3);
    let rows = (inner - lines - 1 - (lines > 0) as i32).max(0);
    let mut y = r.y + 1;
    for (_, _, i) in v.friends.iter().take(rows as usize) {
        out.push((y, FriendsHit::Friend(*i)));
        y += 1;
    }
    if (v.friends.len() as i32) < rows {
        out.push((y, FriendsHit::Add));
        y += 1;
    }
    if (v.friends.len() as i32) + 1 < rows {
        out.push((y, FriendsHit::Public));
    }
    // (a rule, then the lines, just above the talk line)
    let y = r.y + r.h - 3 - lines;
    if lines > 0 {
        for k in 0..=lines {
            out.push((y + k, FriendsHit::Messages));
        }
    }
    out.push((r.y + r.h - 2, FriendsHit::Talk));
    out
}

/// Friends & messages: who's about (● online, ⌂ a village open), the last
/// few lines said, and a line to talk.
pub fn draw_friends(f: &mut Frame, r: Rect, v: &FriendsView, hover: Option<FriendsHit>, t: f32) {
    let title = match (v.online, v.unread, v.here) {
        (false, ..) => "friends · offline".to_string(),
        (_, 0, 0) => "friends & messages".to_string(),
        (_, 0, h) => format!("friends · {h} here"),
        (_, n, _) => format!("friends · {n} new"),
    };
    panel(f, r.x, r.y, r.w, r.h, '✉', &title);
    if v.online && !v.connected {
        txt(f, r.w + r.x - 13, r.y, " connecting ", pal::UI_DIM);
    }
    let width = (r.w - 4).max(4) as usize;
    let hot = |h: FriendsHit| hover == Some(h);
    let rows = friends_rows(r, v);
    let mut shown_lines = false;
    for (y, what) in &rows {
        let bg = if hot(*what) { Bg::Mix(Rgb::hex(0x34406a), 0.9) } else { PANEL };
        match *what {
            FriendsHit::Friend(i) => {
                let Some((name, state, _)) = v.friends.iter().find(|fr| fr.2 == i) else { continue };
                let (mark, c) = match state {
                    1 => ('●', Rgb::hex(0x9be08a)),
                    2 => ('⌂', Rgb::hex(0xffd36b)),
                    3 => ('…', pal::UI_DIM),
                    _ => ('○', pal::UI_DIM),
                };
                for x in r.x + 1..r.x + r.w - 1 {
                    f.put_g(x, *y, Glyph::new(' ', pal::UI_TEXT).bg(bg).lit(0.0));
                }
                f.put_g(r.x + 2, *y, Glyph::new(mark, c).bg(bg).lit(0.0));
                let what = match state {
                    1 => "online",
                    2 => "village open",
                    3 => "asked",
                    _ => "",
                };
                let name = clip(name, width.saturating_sub(what.len() + 4));
                f.text(r.x + 4, *y, &name, if *state == 0 { pal::UI_DIM } else { pal::UI_TEXT }, bg, 0.0);
                f.text(r.x + r.w - 2 - what.chars().count() as i32, *y, what, pal::UI_DIM, bg, 0.0);
                if hot(FriendsHit::Friend(i)) {
                    f.text(r.x + r.w - 4, *y, " ▸", Rgb::hex(0xffd36b), bg, 0.0);
                }
            }
            FriendsHit::Add => {
                let s = if v.friends.is_empty() { "✚ add a friend (their code)" } else { "✚ add a friend" };
                f.text(r.x + 2, *y, &clip(s, width), if hot(FriendsHit::Add) { Rgb::hex(0xffd36b) } else { pal::UI_DIM }, bg, 0.0);
            }
            FriendsHit::Messages if !shown_lines => {
                shown_lines = true;
                let room = rows.iter().filter(|(_, h)| *h == FriendsHit::Messages).count().saturating_sub(1);
                let lines: Vec<&String> = v.lines.iter().rev().take(room).collect::<Vec<_>>().into_iter().rev().collect();
                let y0 = rows.iter().find(|(_, h)| *h == FriendsHit::Messages).map(|(y, _)| *y).unwrap_or(*y);
                let rule: String = "┄".repeat(width);
                f.text(r.x + 2, y0, &rule, pal::UI_BORDER, PANEL, 0.0);
                for (k, l) in lines.iter().enumerate() {
                    let yy = y0 + 1 + k as i32;
                    let lbg = if hot(FriendsHit::Messages) { Bg::Mix(Rgb::hex(0x34406a), 0.9) } else { PANEL };
                    for x in r.x + 1..r.x + r.w - 1 {
                        f.put_g(x, yy, Glyph::new(' ', pal::UI_TEXT).bg(lbg).lit(0.0));
                    }
                    f.text(r.x + 2, yy, &clip(l, width), pal::UI_TEXT * 0.9, lbg, 0.0);
                }
            }
            FriendsHit::Messages => {}
            FriendsHit::GoOnline => {
                let c = if hot(FriendsHit::GoOnline) { Rgb::hex(0xffd36b) } else { Rgb::hex(0x9be08a) };
                f.text(r.x + 2, *y, &clip("● go online: friends, talk, villages", width), c, bg, 0.0);
            }
            FriendsHit::Public => {
                f.text(r.x + 2, *y, &clip("⌂ public villages: drop in ▸", width), if hot(FriendsHit::Public) { Rgb::hex(0xffd36b) } else { pal::UI_TEXT * 0.85 }, bg, 0.0);
            }
            FriendsHit::Talk => {
                let blink = (t * 1.5) as i32 % 2 == 0 && hot(FriendsHit::Talk);
                let s = if v.unread > 0 { format!("✎ talk (/) · ✉ {} new", v.unread) } else { "✎ talk (/) · @name: to one".to_string() };
                f.text(r.x + 2, *y, &clip(&s, width), if hot(FriendsHit::Talk) || blink { Rgb::hex(0xffd36b) } else { pal::UI_DIM }, bg, 0.0);
            }
        }
    }
}

pub fn panel(f: &mut Frame, x: i32, y: i32, w: i32, h: i32, icon: char, title: &str) {
    let b = pal::UI_BORDER;
    for yy in y..y + h {
        for xx in x..x + w {
            let ch = match (xx == x, xx == x + w - 1, yy == y, yy == y + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(xx, yy, Glyph::new(ch, b).bg(PANEL).lit(0.0));
        }
    }
    let mut cx = x + 2;
    f.put_g(cx, y, Glyph::new(' ', b).bg(PANEL).lit(0.0));
    f.put_g(cx + 1, y, Glyph::new(icon, Rgb::hex(0xffd36b)).bg(PANEL).lit(0.0));
    cx += 2;
    cx = txt(f, cx, y, " ", b);
    cx = txt(f, cx, y, title, pal::UI_TITLE);
    txt(f, cx, y, " ", b);
}

/// At most `n` chars, ending in … when cut.
pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    if n > 0 {
        out.push('…');
    }
    out
}

pub fn txt(f: &mut Frame, x: i32, y: i32, s: &str, c: Rgb) -> i32 {
    f.text(x, y, s, c, PANEL, 0.0)
}

#[derive(Clone, Copy)]
pub enum Sky {
    Clear,
    Partly,
    Cloud,
    Fog,
    Rain,
    Snow,
    Thunder,
}

pub fn wmo(code: i64) -> (Sky, &'static str) {
    match code {
        0 => (Sky::Clear, "Clear"),
        1 => (Sky::Clear, "Mainly clear"),
        2 => (Sky::Partly, "Partly cloudy"),
        3 => (Sky::Cloud, "Overcast"),
        45 | 48 => (Sky::Fog, "Fog"),
        51..=57 => (Sky::Rain, "Drizzle"),
        61..=67 => (Sky::Rain, "Rain"),
        71..=77 => (Sky::Snow, "Snow"),
        80..=82 => (Sky::Rain, "Showers"),
        85 | 86 => (Sky::Snow, "Snow showers"),
        95..=99 => (Sky::Thunder, "Thunderstorm"),
        _ => (Sky::Cloud, "—"),
    }
}

pub fn small_icon(s: Sky, day: bool) -> char {
    match s {
        Sky::Clear if day => '☀',
        Sky::Clear => '☾',
        Sky::Partly => '◐',
        Sky::Cloud => '☁',
        Sky::Fog => '≡',
        Sky::Rain => '☂',
        Sky::Snow => '❄',
        Sky::Thunder => 'ϟ',
    }
}

pub fn draw_icon(f: &mut Frame, x: i32, y: i32, s: Sky, day: bool, t: f32) {
    let sun = Rgb::hex(0xffd36b);
    let cloud = Rgb::hex(0xc8cede);
    let moon = Rgb::hex(0xf2ead0);
    let fall = (t * 2.0) as usize % 2;
    let (lines, cols): (Vec<String>, Vec<Rgb>) = match s {
        Sky::Clear if day => (
            vec!["  \\ │ /  ".into(), " ─  ☀  ─ ".into(), "  / │ \\  ".into(), "".into()],
            vec![sun, sun * 1.2, sun, sun],
        ),
        Sky::Clear => (
            vec!["  ✦   ·  ".into(), "   ▄▄▖   ".into(), "  ▐█     ".into(), "   ▀▀▘ ✧ ".into()],
            vec![moon * 0.7, moon, moon, moon],
        ),
        Sky::Partly => (
            vec![" \\ /     ".into(), "─ ☀ .--. ".into(), "  .(    ).".into(), " (___.__)".into()],
            vec![sun, sun, cloud, cloud],
        ),
        Sky::Fog => (
            vec!["         ".into(), " ─ ─ ─ ─ ".into(), "  ─ ─ ─  ".into(), " ─ ─ ─ ─ ".into()],
            vec![cloud; 4],
        ),
        _ => {
            let drop = match s {
                Sky::Rain => if fall == 0 { "  ╎ ╎ ╎ ╎" } else { " ╎ ╎ ╎ ╎ " },
                Sky::Snow => if fall == 0 { "  * ❄ * ❄" } else { " ❄ * ❄ * " },
                Sky::Thunder => if fall == 0 { "  ϟ ╎ ϟ  " } else { " ╎ ϟ ╎ ϟ " },
                _ => "",
            };
            let dc = match s {
                Sky::Rain => Rgb::hex(0x6aa8ff),
                Sky::Snow => Rgb::hex(0xeef4ff),
                _ => sun,
            };
            (vec!["   .--.  ".into(), " .(    ).".into(), "(___.__)_)".into(), drop.into()], vec![cloud, cloud, cloud, dc])
        }
    };
    for (i, l) in lines.iter().enumerate() {
        txt(f, x, y + i as i32, l, cols[i]);
    }
}

pub fn draw_weather(f: &mut Frame, r: Rect, wx: &Weather, t: f32) -> i32 {
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    let title = if wx.place.is_empty() { "weather".to_string() } else { format!("weather · {}", wx.place) };
    // the title starts at x + 5; leave the corner and one ─ before it
    panel(f, x, y, w, h, '☃', &clip(&title, (w - 8).max(0) as usize));
    if !wx.ok {
        // errors can be long (ureq includes the whole URL): wrap them over
        // the panel's empty rows, clipped to the inner width
        let inner = (w - 4).max(1) as usize;
        let chars: Vec<char> = wx.status.chars().collect();
        let rows = (h - 3) as usize; // y+2 .. above the bottom border
        for (i, line) in chars.chunks(inner).take(rows).enumerate() {
            let mut line: String = line.iter().collect();
            if i == rows - 1 && chars.len() > inner * rows {
                line = clip(&format!("{line}…"), inner);
            }
            txt(f, x + 2, y + 2 + i as i32, &line, pal::UI_DIM);
        }
        return y + h;
    }
    let (sky, desc) = wmo(wx.code);
    draw_icon(f, x + 2, y + 1, sky, wx.is_day, t);
    let tx = x + 14;
    let c = txt(f, tx, y + 1, &format!("{:.0}{}", wx.temp, wx.unit), Rgb::hex(0xfff1d6));
    txt(f, c + 2, y + 1, desc, pal::UI_TEXT);
    txt(f, tx, y + 2, &format!("feels like {:.0}{}", wx.feels, wx.unit), pal::UI_DIM);
    txt(f, tx, y + 3, &format!("wind {:.0} {}  ·  humidity {:.0}%", wx.wind, wx.wind_unit, wx.humidity), pal::UI_DIM);
    // forecast row
    let mut fx = x + 2;
    // as many days as fit (four at the usual width)
    let days = (((w - 4) / 14).max(1)) as usize;
    for (i, (day, code, mx, mn)) in wx.daily.iter().enumerate().take(days) {
        let name = NaiveDate::parse_from_str(day, "%Y-%m-%d").map(|d| d.format("%a").to_string()).unwrap_or_default();
        let (s, _) = wmo(*code);
        let nc = if i == 0 { Rgb::hex(0xffd36b) } else { pal::UI_TEXT };
        let fy = y + h - 2;
        let c = txt(f, fx, fy, &format!("{name} "), nc);
        f.put_g(c, fy, Glyph::new(small_icon(s, true), Rgb::hex(0xffd36b)).bg(PANEL).lit(0.0));
        let c = txt(f, c + 2, fy, &format!("{:.0}°", mx), pal::UI_TEXT);
        txt(f, c, fy, &format!("/{:.0}°", mn), pal::UI_DIM);
        fx += 14;
    }
    y + h
}

/// Where a click on the friends widget landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FriendsHit {
    /// A friend (their place in the friends list).
    Friend(usize),
    Messages,
    Talk,
    Add,
    /// Offline: going online.
    GoOnline,
    /// The public villages' menu.
    Public,
}
