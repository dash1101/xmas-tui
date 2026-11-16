//! The save file: the village (the seed it's laid out from, so it's the same
//! village, the villager's home where it was, each time) and the villager's
//! own things: pockets, stats and scorecard, where they were. Kept beside the
//! app's other files and written every minute and on the way out; Settings →
//! Village exports it (to share, or to carry to another computer) and
//! imports one.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Save {
    pub version: u32,
    /// The village's layout.
    pub seed: u32,
    /// The villager's name when it was saved (for telling saves apart).
    pub name: String,
    /// When it was saved (local time, for people to read).
    pub saved: String,
    /// Where the villager was (column, row).
    pub player: Option<[f32; 2]>,
    /// Pockets, stats, the scorecard and the Christmas list.
    pub pockets: crate::scene::shop::Pockets,
    /// The villager themself (their name, pet, friends & family, keys).
    pub you: Option<You>,
    /// Online: the friend code, friends (see `crate::net`).
    pub online: Option<crate::net::Book>,
}

/// The villager themself: their name, their pet, friends & family, which
/// keys do what and the panels' look. A save carried to another computer,
/// the web or a phone brings them along.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct You {
    pub name: String,
    pub pet: crate::config::PetCfg,
    pub friends: Vec<crate::config::FriendCfg>,
    pub only_friends: bool,
    /// The panels' and menus' look (Settings → Widgets).
    pub look: String,
    pub keys: Option<crate::config::KeysCfg>,
}

impl You {
    pub fn of(cfg: &crate::config::Config) -> You {
        You {
            name: cfg.scene.name.clone(),
            pet: cfg.scene.pet.clone(),
            friends: cfg.scene.friends.clone(),
            only_friends: cfg.scene.only_friends,
            look: cfg.ui.look.clone(),
            keys: Some(cfg.keys.clone()),
        }
    }

    /// All of it into a config (a save imported).
    pub fn apply(&self, cfg: &mut crate::config::Config) {
        cfg.scene.name = self.name.clone();
        cfg.scene.pet = self.pet.clone();
        cfg.scene.friends = self.friends.clone();
        cfg.scene.only_friends = self.only_friends;
        if !self.look.is_empty() {
            cfg.ui.look = self.look.clone();
        }
        if let Some(k) = &self.keys {
            cfg.keys = k.clone();
        }
    }

    /// Whatever a config leaves unset, from here (the villager's name, the
    /// pet's name, friends & family): picking up where the save left off.
    pub fn fill(&self, cfg: &mut crate::config::Config) {
        if cfg.scene.name.is_empty() {
            cfg.scene.name = self.name.clone();
        }
        if cfg.scene.pet.name.is_empty() && !self.pet.name.is_empty() {
            cfg.scene.pet = self.pet.clone();
        }
        if cfg.scene.friends.is_empty() {
            cfg.scene.friends = self.friends.clone();
        }
    }
}

/// Where it's kept: ~/.local/share/xmas-tui/save.toml (%APPDATA%\xmas-tui
/// on Windows).
pub fn path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("xmas-tui").join("save.toml");
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    data.join("xmas-tui").join("save.toml")
}

static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This run keeps the save file (set once, at the start).
pub fn set_on(on: bool) {
    ON.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn on() -> bool {
    ON.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn read(p: &Path) -> Option<Save> {
    let s = std::fs::read_to_string(p).ok()?;
    let save: Save = toml::from_str(&s).ok()?;
    (save.seed != 0).then_some(save)
}

pub fn load() -> Option<Save> {
    read(&path())
}

/// Write it (whole, then into place: never a half-written save).
pub fn write(p: &Path, s: &Save) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(s).map_err(std::io::Error::other)?;
    let tmp = p.with_extension("toml.tmp");
    std::fs::write(&tmp, format!("# xmas-tui save: the village and the villager's things\n{text}"))?;
    std::fs::rename(&tmp, p)
}

/// A copy to keep or share: in Documents (or the home folder), named for
/// the day. Where it went.
pub fn export(s: &Save) -> std::io::Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let docs = home.join("Documents");
    let dir = if docs.is_dir() { docs } else { home };
    let day = crate::clock::now().format("%Y-%m-%d").to_string();
    let p = dir.join(format!("xmas-tui-save-{day}.toml"));
    write(&p, s)?;
    Ok(p)
}

/// The system's file picker, for a save to import (None: cancelled, or no
/// picker to be had).
pub fn pick() -> Option<PathBuf> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(all(not(test), windows))]
    {
        crate::winshelf::com_init();
        crate::winshelf::pick_file("A save to import")
    }
    #[cfg(all(not(test), not(windows)))]
    {
        use std::process::Command;
        let home = std::env::var("HOME").unwrap_or_default();
        let tries: [(&str, Vec<String>); 2] = [
            ("kdialog", vec!["--title".into(), "A save to import".into(), "--getopenfilename".into(), format!("{home}/Documents"), "*.toml".into()]),
            ("zenity", vec!["--file-selection".into(), "--title=A save to import".into(), "--file-filter=*.toml".into()]),
        ];
        for (cmd, args) in tries {
            let found = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()));
            if !found {
                continue;
            }
            let out = Command::new(cmd).args(&args).output().ok()?;
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return (out.status.success() && !p.is_empty()).then(|| PathBuf::from(p));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_goes_out_and_comes_back() {
        let dir = std::env::temp_dir().join(format!("xmas-save-{}", std::process::id()));
        let p = dir.join("save.toml");
        let mut s = Save { version: 1, seed: 1234, name: "Robin".into(), player: Some([400.5, 60.0]), ..Default::default() };
        s.pockets.cheer = 42;
        s.pockets.fights_won = 3;
        s.pockets.ski_best = Some(61.5);
        s.pockets.houses = vec![2, 5];
        write(&p, &s).unwrap();
        let back = read(&p).expect("unreadable");
        assert_eq!((back.seed, back.pockets.cheer, back.pockets.fights_won, back.pockets.ski_best), (1234, 42, 3, Some(61.5)));
        assert_eq!(back.player, Some([400.5, 60.0]));
        assert_eq!(back.pockets.houses, vec![2, 5]);
        // the villager comes along: name, pet, friends, keys
        let mut cfg = crate::config::Config::default();
        cfg.scene.name = "Robin".into();
        cfg.scene.pet = crate::config::PetCfg { kind: "cat".into(), coat: "ginger".into(), name: "Biscuit".into() };
        cfg.scene.friends = vec![crate::config::FriendCfg { name: "Sam".into(), kid: true, ..Default::default() }];
        cfg.keys.jump = "k".into();
        s.you = Some(You::of(&cfg));
        write(&p, &s).unwrap();
        let back = read(&p).expect("unreadable");
        let mut there = crate::config::Config::default();
        back.you.as_ref().unwrap().apply(&mut there);
        assert_eq!((there.scene.name.as_str(), there.scene.pet.name.as_str(), there.scene.pet.kind.as_str()), ("Robin", "Biscuit", "cat"));
        assert_eq!((there.scene.friends.len(), there.keys.jump.as_str()), (1, "k"));
        // filling in only what's unset
        let mut mine = crate::config::Config::default();
        mine.scene.name = "Alex".into();
        back.you.as_ref().unwrap().fill(&mut mine);
        assert_eq!((mine.scene.name.as_str(), mine.scene.pet.name.as_str()), ("Alex", "Biscuit"));
        // something that isn't a save
        std::fs::write(&p, "fps = 30").unwrap();
        assert!(read(&p).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
