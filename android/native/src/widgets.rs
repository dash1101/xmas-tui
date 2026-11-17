//! The widgets' looks, which the scene's own panels (conversations, the
//! games' table) are drawn in too; and the rectangle they take.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[path = "../../../src/widgets/looks.rs"]
pub mod looks;
/// The clock, the weather and the friends, drawn as the desktop's are.
#[path = "../../../src/widgets/face.rs"]
pub mod face;

/// The desktop's shelves (folders on the desktop): none here, but the menu
/// knows of them.
#[allow(dead_code)]
pub mod shelf {
    use std::path::PathBuf;

    pub const STYLES: [&str; 3] = ["frosted", "wood", "gift"];

    pub fn home_dir() -> PathBuf {
        PathBuf::new()
    }

    pub fn add(cfg: &mut crate::config::Config, pos: Option<[f32; 2]>) {
        cfg.shelves.push(crate::config::ShelfCfg { name: "Shelf".into(), pos, ..Default::default() });
    }

    pub fn rename(cfg: &mut crate::config::Config, i: usize, name: &str) {
        if let Some(c) = cfg.shelves.get_mut(i) {
            c.name = name.trim().chars().take(24).collect();
        }
    }

    pub fn folder_of(cfg: &crate::config::ShelfCfg) -> PathBuf {
        PathBuf::from(cfg.folder.trim())
    }
}
