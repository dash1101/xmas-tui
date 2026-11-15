//! The wall clock everything seasonal goes by. `XMAS_NOW` (e.g.
//! "2026-12-24T22:30") pretends it's that moment instead — the time then
//! runs on normally from there. For previews and testing Christmas Eve or
//! New Year's in November.

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone};
use std::sync::OnceLock;

fn offset() -> Duration {
    static OFF: OnceLock<Duration> = OnceLock::new();
    *OFF.get_or_init(|| {
        let Ok(s) = std::env::var("XMAS_NOW") else { return Duration::zero() };
        let parsed = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M"));
        match parsed.ok().and_then(|n| Local.from_local_datetime(&n).single()) {
            Some(target) => target - Local::now(),
            None => {
                eprintln!("xmas-tui: XMAS_NOW={s:?} isn't YYYY-MM-DDTHH:MM[:SS]; ignoring it");
                Duration::zero()
            }
        }
    })
}

/// Now (local time), or the pretend now.
pub fn now() -> DateTime<Local> {
    Local::now() + offset()
}
