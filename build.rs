//! Windows builds: embed packaging/windows/xmas-tui.manifest into the exe
//! (Windows 8-11 compatibility, needed for the 24H2 desktop attach). No
//! extra crates: MSVC links it directly; GNU toolchains (llvm-mingw, the
//! cross build) compile a one-line .rc with windres into an object file.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=packaging/windows/xmas-tui.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let manifest = dir.join("packaging").join("windows").join("xmas-tui.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("manifest.rc");
    let obj = out.join("manifest.o");
    // 1 = CREATEPROCESS_MANIFEST_RESOURCE_ID, 24 = RT_MANIFEST
    let path = manifest.to_string_lossy().replace('\\', "/");
    std::fs::write(&rc, format!("1 24 \"{path}\"\n")).unwrap();
    for windres in windres_candidates() {
        let ok = Command::new(&windres).arg("-O").arg("coff").arg("-i").arg(&rc).arg("-o").arg(&obj).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            println!("cargo:rustc-link-arg-bins={}", obj.display());
            return;
        }
    }
    println!("cargo:warning=no windres found: building without the app manifest (the Windows 11 24H2 desktop attach needs it)");
}

/// $WINDRES, then the windres next to the configured C compiler
/// (CC_<target>, e.g. llvm-mingw's x86_64-w64-mingw32-clang), then PATH.
fn windres_candidates() -> Vec<PathBuf> {
    let target = std::env::var("TARGET").unwrap_or_default();
    let mut v = vec![];
    if let Some(w) = std::env::var_os("WINDRES") {
        v.push(PathBuf::from(w));
    }
    for key in [format!("CC_{}", target.replace('-', "_")), format!("CC_{target}"), "CC".into()] {
        if let Some(cc) = std::env::var_os(&key) {
            let cc = Path::new(&cc);
            let name = cc.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if let Some(prefix) = name.strip_suffix("clang").or(name.strip_suffix("gcc")) {
                v.push(cc.with_file_name(format!("{prefix}windres")));
            }
        }
    }
    let arch = target.split('-').next().unwrap_or("x86_64");
    v.push(PathBuf::from(format!("{arch}-w64-mingw32-windres")));
    v.push(PathBuf::from("windres"));
    v
}
