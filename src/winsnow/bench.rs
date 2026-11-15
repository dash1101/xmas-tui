//! `--bench` for the window snow & lights: the overlay's per-frame work on
//! a made-up 4452x1252 desktop, minus the compositor. Reports CPU per frame
//! and how much the compositor would be asked to repaint.

use super::{coalesce, copy_rect, merge_damage, Canvas, Changes, Fx, FxCfg, Glyphs, Win};
use std::time::Instant;

fn desktop() -> Vec<Win> {
    let win = |id: &str, x: f32, y: f32, w: f32, h: f32, kind: u8, z: i64| Win { id: id.into(), x, y, w, h, kind, z };
    vec![
        win("browser", 3329.0, 40.0, 1123.0, 1150.0, 0, 15),
        win("editor", 1687.0, 88.0, 1204.0, 996.0, 0, 16),
        win("terminal", 60.0, 120.0, 1297.0, 1000.0, 0, 17),
        win("panel", 1774.0, 1188.0, 903.0, 64.0, 1, 18),
        win("notes", 2900.0, 300.0, 600.0, 400.0, 0, 19),
    ]
}

pub fn run(frames: usize, cfg: FxCfg) -> String {
    let (w, h) = (4452usize, 1252usize);
    let mut report = String::new();
    // idle: a static desktop at the regular 30 fps; drag: one window being
    // dragged, with the overlay keeping up at 240 fps
    for (name, hz, drag) in [("idle", 30.0f32, false), ("drag", 240.0, true)] {
        let mut wins = desktop();
        let mut fx = Fx::new(w as f32, h as f32, cfg, 1.0);
        let mut canvas = Canvas::new(w, h);
        let mut glyphs = Glyphs::new();
        let mut changes = Changes::new(w, h);
        let mut buf = vec![0u8; w * h * 4];
        let mut last_changed = vec![];
        let (mut t_draw, mut t_diff, mut t_copy) = (0.0f64, 0.0f64, 0.0f64);
        let (mut area, mut rects, mut copied) = (0usize, 0usize, 0usize);
        let dt = 1.0 / hz;
        // let some snow settle first (20 s at 30 fps)
        let warm = 600;
        for i in 0..warm + frames {
            let t = if i < warm { i as f32 / 30.0 } else { warm as f32 / 30.0 + (i - warm) as f32 * dt };
            if drag && i >= warm {
                let n = wins.len() - 1;
                wins[n].x = 2900.0 + ((i - warm) as f32 * 6.0) % 900.0;
            }
            let a = Instant::now();
            fx.set_windows(wins.clone(), &[], (2000.0, 700.0), t);
            fx.update(if i < warm { 1.0 / 30.0 } else { dt }, t);
            let mut damage = canvas.clear_touched();
            fx.render(&mut canvas, &mut glyphs, t);
            damage.extend(canvas.touched.iter().cloned());
            let b = Instant::now();
            let changed = changes.find(&canvas, &damage);
            let merged = merge_damage(&changed, w as i32, h as i32);
            let c = Instant::now();
            // a double-buffered overlay fills in this frame's and last frame's changes
            let pending: Vec<_> = changed.iter().chain(&last_changed).cloned().collect();
            for r in coalesce(&pending, w as i32, h as i32) {
                copy_rect(&canvas, &mut buf, r);
            }
            let d = Instant::now();
            if i >= warm {
                t_draw += (b - a).as_secs_f64();
                t_diff += (c - b).as_secs_f64();
                t_copy += (d - c).as_secs_f64();
                area += merged.iter().map(|r| (r.2 * r.3) as usize).sum::<usize>();
                rects += merged.len();
                copied += changed.iter().chain(&last_changed).map(|r| (r.2 * r.3) as usize).sum::<usize>();
            }
            last_changed = changed;
        }
        // copying only the changed tiles must leave the buffer equal to the canvas
        let exact = buf.chunks_exact(4).zip(&canvas.px).all(|(b, &p)| b == p.to_le_bytes());
        let n = frames as f64;
        report += &format!(
            "window fx {name} @{hz:.0}fps: draw {:.2} ms, diff {:.2} ms, copy {:.2} ms per frame; damage {:.0} kpx in {:.1} rects, copied {:.0} kpx per frame{}\n",
            t_draw / n * 1e3,
            t_diff / n * 1e3,
            t_copy / n * 1e3,
            area as f64 / n / 1e3,
            rects as f64 / n,
            copied as f64 / n / 1e3,
            if exact { "" } else { " — BUFFER MISMATCH" },
        );
    }
    report
}
