//! The app icon (a Christmas tree made of terminal glyphs, drawn by
//! packaging/icons/make-icons.py), built in: for the windows, the tray, and
//! the icon / launcher files the Linux desktop looks them up by.

const PNGS: [(usize, &[u8]); 8] = [
    (16, include_bytes!("../assets/icon/xmas-tui-16.png")),
    (24, include_bytes!("../assets/icon/xmas-tui-24.png")),
    (32, include_bytes!("../assets/icon/xmas-tui-32.png")),
    (48, include_bytes!("../assets/icon/xmas-tui-48.png")),
    (64, include_bytes!("../assets/icon/xmas-tui-64.png")),
    (128, include_bytes!("../assets/icon/xmas-tui-128.png")),
    (256, include_bytes!("../assets/icon/xmas-tui-256.png")),
    (512, include_bytes!("../assets/icon/xmas-tui-512.png")),
];

/// Decode one of the built-in PNGs to straight RGBA.
fn decode(data: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        _ => return None,
    };
    Some((px, w, h))
}

/// The icon at `n` x `n` pixels (straight RGBA, row-major): the drawn size
/// if there is one, else the next bigger one scaled down.
pub fn rgba(n: usize) -> Vec<u8> {
    let n = n.max(1);
    let (_, data) = PNGS.iter().find(|(s, _)| *s >= n).copied().unwrap_or(PNGS[PNGS.len() - 1]);
    let Some((src, sw, sh)) = decode(data) else { return vec![0; n * n * 4] };
    if sw == n && sh == n {
        return src;
    }
    // area-average down (premultiplied, so edges don't darken)
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        let (y0, y1) = (y * sh / n, ((y + 1) * sh / n).max(y * sh / n + 1));
        for x in 0..n {
            let (x0, x1) = (x * sw / n, ((x + 1) * sw / n).max(x * sw / n + 1));
            let mut acc = [0f32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &src[(sy * sw + sx) * 4..][..4];
                    let a = p[3] as f32 / 255.0;
                    for k in 0..3 {
                        acc[k] += p[k] as f32 * a;
                    }
                    acc[3] += a;
                }
            }
            let cnt = ((y1 - y0) * (x1 - x0)) as f32;
            let i = (y * n + x) * 4;
            if acc[3] > 0.0 {
                for k in 0..3 {
                    out[i + k] = (acc[k] / acc[3]).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[i + 3] = (acc[3] / cnt * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Linux: put the icon (hicolor theme sizes) and launchers where the
/// desktop looks them up, so the windows (app ids "xmas-tui" and
/// "xmas-tui-settings") get the tree in the taskbar instead of a generic
/// one. Only writes what's missing or different; quiet on failure.
#[cfg(target_os = "linux")]
pub fn install_desktop_files() {
    let Some(data) = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))) else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.display().to_string();
    // a binary that's been replaced (an update) shows as "… (deleted)"
    let exe = exe.trim_end_matches(" (deleted)").to_string();
    let write = |path: std::path::PathBuf, bytes: &[u8]| {
        if std::fs::read(&path).ok().as_deref() != Some(bytes) {
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&path, bytes);
        }
    };
    for (n, png) in PNGS {
        write(data.join(format!("icons/hicolor/{n}x{n}/apps/xmas-tui.png")), png);
    }
    let q = |s: &str| if s.contains(' ') { format!("\"{s}\"") } else { s.to_string() };
    let game = format!(
        "[Desktop Entry]\nType=Application\nName=xmas-tui\nComment=The Christmas village, in a window of its own\nExec={} --windowed\nIcon=xmas-tui\nStartupWMClass=xmas-tui\nCategories=Game;\nTerminal=false\n",
        q(&exe)
    );
    write(data.join("applications/xmas-tui.desktop"), game.as_bytes());
    let settings = format!(
        "[Desktop Entry]\nType=Application\nName=xmas-tui settings\nComment=Settings for the xmas-tui Christmas wallpaper\nExec={} --settings-window\nIcon=xmas-tui\nStartupWMClass=xmas-tui-settings\nCategories=Settings;DesktopSettings;\nTerminal=false\n",
        q(&exe)
    );
    write(data.join("applications/xmas-tui-settings.desktop"), settings.as_bytes());
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_comes_in_every_size() {
        for n in [16, 22, 32, 64, 100, 256] {
            let px = super::rgba(n);
            assert_eq!(px.len(), n * n * 4);
            // a dark tile with a green tree on it: opaque middle, some green
            let mid = &px[(n / 2 * n + n / 2) * 4..][..4];
            assert!(mid[3] > 200, "{n}: transparent middle");
            assert!(px.chunks_exact(4).any(|p| p[1] > 120 && p[0] < 100 && p[3] > 200), "{n}: no green");
        }
    }
}
