//! Transparent, click-through Wayland overlay (wlr-layer-shell, overlay
//! layer, empty input region) that draws the window snow and lights.
//! Runs on its own thread; only re-uploads the regions that changed.

use super::{coalesce, copy_rect, kde::Bridge, merge_damage, parse, Canvas, Changes, Fx, FxCfg, Glyphs};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
    Connection, EventQueue, QueueHandle,
};

struct Slot {
    buf: Buffer,
    /// regions changed since this buffer was last filled
    pending: Vec<(i32, i32, i32, i32)>,
    full: bool,
}

struct State {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    layer: LayerSurface,
    size: (u32, u32),
    configured: bool,
    origin: (f32, f32),
    closed: bool,
    /// When the last commit's frame callback is still outstanding (the
    /// compositor hasn't shown it yet), committing again is wasted work.
    waiting_frame: Option<Instant>,
}

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.waiting_frame = None;
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
        if let Some(info) = self.outputs.info(output) {
            if let Some((x, y)) = info.logical_position {
                self.origin = (x as f32, y as f32);
            }
        }
    }
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, cfg: LayerSurfaceConfigure, _: u32) {
        if cfg.new_size.0 > 0 && cfg.new_size.1 > 0 {
            self.size = cfg.new_size;
        }
        self.configured = true;
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_layer!(State);
delegate_registry!(State);

/// Flush requests, read whatever events arrive within `timeout_ms` (0: just
/// what's already there) and dispatch them.
fn pump(queue: &mut EventQueue<State>, st: &mut State, timeout_ms: i32) -> Result<(), String> {
    let e = |e: &dyn std::fmt::Display| e.to_string();
    queue.flush().map_err(|x| e(&x))?;
    if let Some(guard) = queue.prepare_read() {
        use std::os::fd::AsRawFd;
        let mut pfd = libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut pfd, 1, timeout_ms) } > 0 {
            guard.read().map_err(|x| e(&x))?;
        }
    }
    queue.dispatch_pending(st).map_err(|x| e(&x))?;
    Ok(())
}

/// The next frame tick after a wake at `now`. A tick only moves on once it
/// has passed: an early wake (a window moved) leaves it alone, or the ticks
/// would run ahead of the clock during a drag and stall the animation after.
/// A tick missed by more than a frame isn't caught up on.
fn next_tick(next: Instant, now: Instant, frame: Duration) -> Instant {
    if now < next {
        return next;
    }
    let n = next + frame;
    if n < now {
        now + frame
    } else {
        n
    }
}

/// Start the overlay thread. Gives up quietly (with a log line) if this
/// isn't a KDE Wayland session.
pub fn spawn(cfg: Arc<Mutex<FxCfg>>) {
    std::thread::spawn(move || {
        if let Err(e) = run(cfg) {
            eprintln!("xmas-tui: window snow disabled: {e}");
        }
    });
}

fn run(shared: Arc<Mutex<FxCfg>>) -> Result<(), String> {
    let e = |e: &dyn std::fmt::Display| e.to_string();
    let conn = Connection::connect_to_env().map_err(|x| e(&x))?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|x| e(&x))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|x| e(&x))?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|x| e(&x))?;
    let shm = Shm::bind(&globals, &qh).map_err(|x| e(&x))?;
    let mut bridge = Bridge::start()?;

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("xmas-snow"), None);
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer.set_size(0, 0);
    // empty input region: every click goes straight through to the windows
    let region = Region::new(&compositor).map_err(|x| e(&x))?;
    layer.wl_surface().set_input_region(Some(region.wl_region()));
    layer.commit();

    let mut st = State {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        shm,
        layer,
        size: (0, 0),
        configured: false,
        origin: (0.0, 0.0),
        closed: false,
        waiting_frame: None,
    };

    let mut fx: Option<Fx> = None;
    let mut canvas = Canvas::new(1, 1);
    let mut changes = Changes::new(1, 1);
    let mut pool: Option<SlotPool> = None;
    let mut slots: Vec<Slot> = vec![];
    let mut glyphs = Glyphs::new();
    let start = Instant::now();
    let mut last = Instant::now();
    let frame = Duration::from_millis(33);
    let mut next = Instant::now();
    let debug = std::env::var_os("XMAS_SNOW_DEBUG").is_some();
    let mut dbg_at = Instant::now();
    let dbg_path = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("xmas-tui").join("snow-debug.txt");
    // how long the next Wayland pump may block (only while idle)
    let mut idle_ms = 0;
    // plasmashellrc is re-read when it changes (panel thickness / floating)
    let mut panels_at = Instant::now();
    let mut panels_mtime = super::kde::plasmashellrc_mtime();
    // (debug) loop passes, simulation steps and commits so far
    let (mut passes, mut steps, mut commits) = (0u64, 0u64, 0u64);

    loop {
        // pump Wayland events (without blocking, unless idle)
        pump(&mut queue, &mut st, std::mem::take(&mut idle_ms))?;
        if st.closed {
            return Ok(());
        }
        if !st.configured || st.size.0 == 0 {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        let (w, h) = st.size;
        let cfg = shared.lock().map(|c| *c).unwrap_or(FxCfg { snow: false, lights: false, ..FxCfg::default() });
        // both effects off: the KWin script is unloaded, so it stops building
        // and sending reports; turned back on, it's loaded again
        let active = cfg.snow || cfg.lights;
        if active != bridge.running() {
            if active {
                bridge.set_running(true)?;
            } else if let Err(err) = bridge.set_running(false) {
                eprintln!("xmas-tui: {err}");
            }
        }
        // (re)build on first configure or resize
        if canvas.w != w as usize || canvas.h != h as usize || fx.is_none() {
            canvas = Canvas::new(w as usize, h as usize);
            changes = Changes::new(w as usize, h as usize);
            let mut p = SlotPool::new((w * h * 4 * 2) as usize, &st.shm).map_err(|x| e(&x))?;
            slots.clear();
            for _ in 0..2 {
                let (buf, bytes) = p.create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888).map_err(|x| e(&x))?;
                bytes.fill(0);
                slots.push(Slot { buf, pending: vec![], full: true });
            }
            pool = Some(p);
            let mut f = Fx::new(w as f32, h as f32, cfg, 1.0);
            f.docks = super::kde::floating_panels();
            fx = Some(f);
        }
        let (Some(fx), Some(pool)) = (fx.as_mut(), pool.as_mut()) else { continue };
        let t = start.elapsed().as_secs_f32();
        let dt = last.elapsed().as_secs_f32().min(0.1);
        last = Instant::now();
        fx.set_cfg(cfg);
        fx.origin = st.origin;
        if panels_at.elapsed() > Duration::from_secs(5) {
            panels_at = Instant::now();
            let m = super::kde::plasmashellrc_mtime();
            if m != panels_mtime {
                panels_mtime = m;
                fx.docks = super::kde::floating_panels();
            }
        }
        let report = bridge.latest.0.lock().ok().and_then(|mut l| {
            l.wake = false;
            l.data.take()
        });
        if let Some((wins, away, cur)) = report.as_deref().and_then(parse) {
            fx.set_windows(wins, &away, cur, t);
        }
        passes += 1;
        if active {
            fx.update(dt, t);
            steps += 1;
        }
        // Only draw a frame the compositor can take now. While the last
        // commit hasn't been shown yet (frame callback outstanding, with a
        // fallback in case callbacks stop, e.g. while the output is off) or
        // the compositor still holds both buffers — about every other tick —
        // a drawn frame would be replaced before anyone saw it. The
        // simulation above still steps every tick.
        let shown = st.waiting_frame.is_none_or(|at| at.elapsed() > Duration::from_millis(250));
        let free = (0..slots.len()).find(|&i| slots[i].buf.canvas(pool).is_some());
        if let (true, Some(i)) = (shown, free) {
            let mut damage = canvas.clear_touched();
            if active {
                fx.render(&mut canvas, &mut glyphs, t);
            }
            damage.extend(canvas.touched.iter().cloned());
            // only what actually looks different goes to the compositor (the
            // wire and settled snow are redrawn identically every frame)
            let changed = changes.find(&canvas, &damage);
            for s in slots.iter_mut() {
                if s.pending.len() > 4000 {
                    s.full = true;
                    s.pending.clear();
                }
                if !s.full {
                    s.pending.extend(changed.iter().cloned());
                }
            }
            let first = slots.iter().all(|s| s.full);
            if first || !changed.is_empty() {
                // fill the free buffer and commit it
                let s = &mut slots[i];
                let bytes = s.buf.canvas(pool).unwrap();
                let surf = st.layer.wl_surface();
                if s.full {
                    copy_rect(&canvas, bytes, (0, 0, w as i32, h as i32));
                } else {
                    // this frame's changes plus any older ones this buffer missed
                    for r in coalesce(&s.pending, w as i32, h as i32) {
                        copy_rect(&canvas, bytes, r);
                    }
                }
                if first {
                    surf.damage_buffer(0, 0, w as i32, h as i32);
                } else {
                    for (x, y, rw, rh) in merge_damage(&changed, w as i32, h as i32) {
                        surf.damage_buffer(x, y, rw, rh);
                    }
                }
                s.full = false;
                s.pending.clear();
                surf.frame(&qh, surf.clone());
                st.waiting_frame = Some(Instant::now());
                s.buf.attach_to(surf).map_err(|x| e(&x))?;
                st.layer.commit();
                commits += 1;
            }
        }
        if debug && dbg_at.elapsed() > Duration::from_secs(2) {
            dbg_at = Instant::now();
            let head = format!("passes {passes} steps {steps} commits {commits} script {}\n", bridge.running());
            let _ = std::fs::write(&dbg_path, head + &fx.status());
            // snapshot of what the overlay is drawing, over a neutral backdrop
            let png = dbg_path.with_file_name("snow-canvas.png");
            if let Ok(file) = std::fs::File::create(&png) {
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                if let Ok(mut wr) = enc.write_header() {
                    let mut img = Vec::with_capacity((w * h * 3) as usize);
                    for &p in &canvas.px {
                        let a = (p >> 24) as f32 / 255.0;
                        let bg = 0x30 as f32;
                        for sh in [16, 8, 0] {
                            img.push((((p >> sh) & 0xff) as f32 + bg * (1.0 - a)) as u8);
                        }
                    }
                    let _ = wr.write_image_data(&img);
                }
            }
        }
        if !active && canvas.touched.is_empty() {
            // nothing to draw and nothing left on screen: no ticking, just
            // wait for Wayland events and look at the settings 4x a second
            idle_ms = 250;
            continue;
        }
        // sleep until the next frame tick, but wake early (≤240/s) when the
        // compositor reports movement, so piles and lights keep up with drags
        let rendered = Instant::now();
        next = next_tick(next, rendered, frame);
        let (lock, cv) = &*bridge.latest;
        if let Ok(g) = lock.lock() {
            // (a report that came in while we were drawing wakes us at once)
            let _ = cv.wait_timeout_while(g, next.saturating_duration_since(rendered), |p| !p.wake);
        }
        let min_gap = Duration::from_micros(4100);
        if rendered.elapsed() < min_gap {
            std::thread::sleep(min_gap - rendered.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_wakes_dont_push_the_tick_ahead() {
        let frame = Duration::from_millis(33);
        let t0 = Instant::now();
        let mut next = t0 + frame;
        // a 2 s drag waking the loop every 4.1 ms
        let mut now = t0;
        let mut ticks = 0;
        while now < t0 + Duration::from_secs(2) {
            now += Duration::from_micros(4100);
            let n = next_tick(next, now, frame);
            if n != next {
                ticks += 1;
                assert!(n > now && n <= now + frame);
            }
            next = n;
        }
        // the regular tick is never more than a frame away, and it kept
        // ticking at about its own rate
        assert!(next <= now + frame);
        assert!((55..=65).contains(&ticks), "{ticks}");
        // after a long stall the tick restarts from now
        let late = now + Duration::from_secs(3);
        assert_eq!(next_tick(next, late, frame), late + frame);
        // an on-time wake moves it by exactly one frame
        assert_eq!(next_tick(next, next, frame), next + frame);
    }
}
