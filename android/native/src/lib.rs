//! The village as a native library for the Android app. The scene, the
//! frame it's drawn into, the cell renderer and the sound are the desktop's
//! own source files (included below by path, so there's one village, not
//! two); `game` drives them for the app's game screen, and `jni_api` is what
//! the Java side calls.

// (the shared files have plenty only the desktop uses)
#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_assignments, clippy::all)]

#[path = "../../../src/audio/mod.rs"]
pub mod audio;
#[path = "../../../src/clock.rs"]
pub mod clock;
#[path = "../../../src/color.rs"]
pub mod color;
#[path = "../../../src/config.rs"]
pub mod config;
#[path = "../../../src/frame.rs"]
pub mod frame;
#[path = "../../../src/raster.rs"]
pub mod raster;
#[path = "../../../src/rng.rs"]
pub mod rng;
#[path = "../../../src/scene/mod.rs"]
pub mod scene;
#[path = "../../../src/save.rs"]
pub mod save;
#[path = "../../../src/net/mod.rs"]
pub mod net;
#[path = "../../../src/sync.rs"]
pub mod sync;
#[path = "../../../src/settings.rs"]
pub mod settings;
#[path = "../../../src/pixicons.rs"]
pub mod pixicons;
#[path = "stubs.rs"]
mod stubs;
pub use stubs::{platform, update};

/// The widgets' looks (the scene's panels are drawn in them too).
pub mod widgets;

/// What the shared files expect of the desktop app.
pub mod app {
    pub fn random_seed() -> u32 {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
        crate::rng::hash((n & 0xffff_ffff) as u32 ^ (n >> 32) as u32) | 1
    }
}

mod game;
mod jni_api;
