//! The Linux wallpaper drawn straight onto the desktop: a surface on the
//! Wayland compositor's background layer (wlr-layer-shell: KDE Plasma,
//! Sway, Hyprland and the like), under the windows, drawn in pixels by the
//! village itself (the same letters and pictures as on Windows and the web
//! page, see `blit`), with only what changed each frame sent to the
//! compositor. No terminal in between: kitty (see the launcher) only draws
//! it where this can't (an X11 session, a compositor without the layer
//! shell), which costs far more (every frame written out as text for
//! kitty to read back, and its whole window drawn again each time).
//!
//! Clicks, the wheel and a trackpad go to the village as they do on
//! Windows; the keyboard comes once the desktop is clicked (the compositor
//! gives it on demand), and is read through the desktop's own keyboard
//! layout (libxkbcommon, loaded when it's there).

use crate::app::{App, Btn, KeyIn, MouseIn};
use crate::config::Config;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use std::path::Path;
use std::time::{Duration, Instant};
use wayland_client::{
    globals::{registry_queue_init, GlobalList},
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_device_v1 as shape_dev, wp_cursor_shape_manager_v1 as shape_mgr};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1 as frac_mgr, wp_fractional_scale_v1 as frac};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

/// Why it couldn't draw on this desktop (the launcher has kitty do it).
#[derive(Debug)]
pub struct CantDraw(pub String);

/// Input as it came, for the frame loop.
enum Input {
    /// The pointer at (x, y) (pixels of the buffer).
    At(f64, f64),
    Down(Btn),
    Up(Btn),
    /// The wheel: steps (a wheel) or pixels (a trackpad), across and down.
    Wheel { steps: (i32, i32), px: (f64, f64), finger: bool },
    Key { k: KeyIn, down: bool },
    /// Ctrl + a letter (zoom keys and the like).
    Ctrl(char),
    Help,
    /// The keyboard here (or gone).
    Focus(bool),
}

/// The desktop's keyboard layout (libxkbcommon, loaded at run time).
struct Xkb {
    lib: &'static xkbcommon_dl::XkbCommon,
    ctx: *mut xkbcommon_dl::xkb_context,
    keymap: *mut xkbcommon_dl::xkb_keymap,
    state: *mut xkbcommon_dl::xkb_state,
}

impl Xkb {
    /// A keymap the compositor sent (its text, in a file to map).
    fn from_fd(fd: std::os::fd::OwnedFd, size: u32) -> Option<Xkb> {
        use std::os::fd::AsRawFd;
        let lib = xkbcommon_dl::xkbcommon_option()?;
        unsafe {
            let p = libc::mmap(std::ptr::null_mut(), size as usize, libc::PROT_READ, libc::MAP_PRIVATE, fd.as_raw_fd(), 0);
            if p == libc::MAP_FAILED {
                return None;
            }
            let ctx = (lib.xkb_context_new)(xkbcommon_dl::xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
            let keymap = if ctx.is_null() {
                std::ptr::null_mut()
            } else {
                (lib.xkb_keymap_new_from_string)(ctx, p as *const _, xkbcommon_dl::xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1, xkbcommon_dl::xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS)
            };
            libc::munmap(p, size as usize);
            if keymap.is_null() {
                if !ctx.is_null() {
                    (lib.xkb_context_unref)(ctx);
                }
                return None;
            }
            let state = (lib.xkb_state_new)(keymap);
            if state.is_null() {
                (lib.xkb_keymap_unref)(keymap);
                (lib.xkb_context_unref)(ctx);
                return None;
            }
            Some(Xkb { lib, ctx, keymap, state })
        }
    }

    fn mods(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        unsafe {
            (self.lib.xkb_state_update_mask)(self.state, depressed, latched, locked, 0, 0, group);
        }
    }

    fn active(&self, name: &[u8]) -> bool {
        unsafe { (self.lib.xkb_state_mod_name_is_active)(self.state, name.as_ptr() as *const _, xkbcommon_dl::xkb_state_component::XKB_STATE_MODS_EFFECTIVE) > 0 }
    }

    /// A key (the compositor's code): its symbol, and the text it types.
    fn key(&self, code: u32) -> (u32, Option<char>) {
        let kc = code + 8;
        unsafe {
            let sym = (self.lib.xkb_state_key_get_one_sym)(self.state, kc);
            let u = (self.lib.xkb_state_key_get_utf32)(self.state, kc);
            (sym, char::from_u32(u).filter(|c| *c != '\0'))
        }
    }

    /// A symbol's own letter (what Ctrl+it means).
    fn letter(&self, sym: u32) -> Option<char> {
        unsafe { char::from_u32((self.lib.xkb_keysym_to_utf32)(sym)).filter(|c| *c != '\0') }
    }
}

impl Drop for Xkb {
    fn drop(&mut self) {
        unsafe {
            (self.lib.xkb_state_unref)(self.state);
            (self.lib.xkb_keymap_unref)(self.keymap);
            (self.lib.xkb_context_unref)(self.ctx);
        }
    }
}

/// What a key is to the village (None: nothing it takes), from its symbol
/// and the text it types.
fn key_in(sym: u32, text: Option<char>) -> Option<KeyIn> {
    use xkbcommon_dl::keysyms as k;
    Some(match sym {
        k::Return | k::KP_Enter => KeyIn::Enter,
        k::BackSpace => KeyIn::Backspace,
        k::Escape => KeyIn::Esc,
        k::Up | k::KP_Up => KeyIn::Up,
        k::Down | k::KP_Down => KeyIn::Down,
        k::Left | k::KP_Left => KeyIn::Left,
        k::Right | k::KP_Right => KeyIn::Right,
        k::space => KeyIn::Char(' '),
        _ => match text {
            Some(c) if !c.is_control() => KeyIn::Char(c),
            _ => return None,
        },
    })
}

struct State {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    shm: Shm,
    layer: LayerSurface,
    /// The surface's size (the compositor's units) and its scale (120ths:
    /// 138 is 1.15), configured yet, closed.
    size: (u32, u32),
    scale120: u32,
    int_scale: i32,
    configured: bool,
    closed: bool,
    /// A frame shown, the compositor not yet said so (since then).
    waiting_frame: Option<Instant>,
    input: Vec<Input>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    xkb: Option<Xkb>,
    ctrl: bool,
    shift: bool,
    /// Keys repeat (a held key, while the keyboard's here): every so many
    /// ms after so many; the key held and when it's next again.
    repeat: (i32, i32),
    held: Option<(KeyIn, Instant)>,
    /// The pointer's shape (the compositor draws it), and where it came in.
    shapes: Option<shape_mgr::WpCursorShapeManagerV1>,
    shape: Option<shape_dev::WpCursorShapeDeviceV1>,
    enter_serial: u32,
    shape_now: &'static str,
}

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, s: i32) {
        self.int_scale = s.max(1);
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.waiting_frame = None;
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
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

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            if let Ok(p) = self.seats.get_pointer(qh, &seat) {
                self.shape = self.shapes.as_ref().map(|m| m.get_pointer(&p, qh, ()));
                self.pointer = Some(p);
            }
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            if let Some(s) = self.shape.take() {
                s.destroy();
            }
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
            self.held = None;
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl State {
    fn px(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let s = self.scale120 as f64 / 120.0;
        (x * s, y * s)
    }
}

impl PointerHandler for State {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            let (x, y) = self.px(e.position);
            match e.kind {
                PointerEventKind::Enter { serial } => {
                    self.enter_serial = serial;
                    self.shape_now = "";
                    self.input.push(Input::At(x, y));
                }
                PointerEventKind::Leave { .. } => {}
                PointerEventKind::Motion { .. } => self.input.push(Input::At(x, y)),
                PointerEventKind::Press { button, .. } | PointerEventKind::Release { button, .. } => {
                    let down = matches!(e.kind, PointerEventKind::Press { .. });
                    // (BTN_LEFT, BTN_RIGHT)
                    let b = match button {
                        0x110 => Btn::Left,
                        0x111 => Btn::Right,
                        _ => continue,
                    };
                    self.input.push(Input::At(x, y));
                    self.input.push(if down { Input::Down(b) } else { Input::Up(b) });
                }
                PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                    let finger = matches!(source, Some(wl_pointer::AxisSource::Finger) | Some(wl_pointer::AxisSource::Continuous));
                    self.input.push(Input::Wheel { steps: (horizontal.discrete, vertical.discrete), px: (horizontal.absolute, vertical.absolute), finger });
                }
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(st: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Keymap { format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1), fd, size } => {
                st.xkb = Xkb::from_fd(fd, size);
                if st.xkb.is_none() {
                    eprintln!("xmas-tui: the keyboard's layout couldn't be read (no libxkbcommon?): keys won't reach the village");
                }
            }
            wl_keyboard::Event::Enter { .. } => st.input.push(Input::Focus(true)),
            wl_keyboard::Event::Leave { .. } => {
                st.held = None;
                st.input.push(Input::Focus(false));
            }
            wl_keyboard::Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } => {
                if let Some(x) = st.xkb.as_mut() {
                    x.mods(mods_depressed, mods_latched, mods_locked, group);
                    st.ctrl = x.active(xkbcommon_dl::XKB_MOD_NAME_CTRL);
                    st.shift = x.active(xkbcommon_dl::XKB_MOD_NAME_SHIFT);
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => st.repeat = (rate, delay),
            wl_keyboard::Event::Key { key, state, .. } => {
                let Some(x) = st.xkb.as_ref() else { return };
                let down = matches!(state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                let (sym, text) = x.key(key);
                if sym == xkbcommon_dl::keysyms::F1 {
                    if down {
                        st.input.push(Input::Help);
                    }
                    return;
                }
                // (Ctrl + a letter: the zoom keys and such; a key let go is
                // let go whatever's held, or it'd stick)
                if st.ctrl && down {
                    if let Some(c) = x.letter(sym) {
                        st.input.push(Input::Ctrl(c.to_ascii_lowercase()));
                    }
                    return;
                }
                // (with Ctrl held a letter types a control code: its own
                // letter's what was let go)
                let text = if st.ctrl { x.letter(sym) } else { text };
                let Some(k) = key_in(sym, text) else { return };
                st.input.push(Input::Key { k, down });
                if down && st.repeat.0 > 0 {
                    st.held = Some((k, Instant::now() + Duration::from_millis(st.repeat.1.max(0) as u64)));
                } else if st.held.is_some_and(|(h, _)| h == k) || !down {
                    st.held = None;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<frac::WpFractionalScaleV1, ()> for State {
    fn event(st: &mut Self, _: &frac::WpFractionalScaleV1, event: frac::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let frac::Event::PreferredScale { scale } = event {
            st.scale120 = scale.clamp(60, 600);
        }
    }
}

impl Dispatch<frac_mgr::WpFractionalScaleManagerV1, ()> for State {
    fn event(_: &mut Self, _: &frac_mgr::WpFractionalScaleManagerV1, _: frac_mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for State {
    fn event(_: &mut Self, _: &wp_viewporter::WpViewporter, _: wp_viewporter::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wp_viewport::WpViewport, ()> for State {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<shape_mgr::WpCursorShapeManagerV1, ()> for State {
    fn event(_: &mut Self, _: &shape_mgr::WpCursorShapeManagerV1, _: shape_mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<shape_dev::WpCursorShapeDeviceV1, ()> for State {
    fn event(_: &mut Self, _: &shape_dev::WpCursorShapeDeviceV1, _: shape_dev::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_layer!(State);
delegate_seat!(State);
delegate_pointer!(State);
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

/// The cell size for a screen `ph` pixels high at `scale` (the desktop's)
/// and the menu's render scale: about what kitty's 10-point letters would
/// be, and never fewer than the 75 rows the village is drawn for.
pub fn cell_size(ph: u32, scale: f32, render: f32) -> (usize, usize) {
    let ch = (16.7 * scale * render.clamp(0.5, 2.0)).round().min(ph as f32 / 75.0).floor().clamp(6.0, 64.0) as usize;
    let cw = ((ch as f32 * 0.474).round() as usize).max(3);
    (cw, ch)
}

/// Optional globals: bound if the compositor has them.
fn bind_opt<I, D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Option<I>
where
    I: wayland_client::Proxy + 'static,
    D: Dispatch<I, ()> + 'static,
{
    globals.bind::<I, D, ()>(qh, 1..=1, ()).ok()
}

/// One pair of buffers the compositor reads, and what each missed since it
/// was last filled (the cells drawn again since).
struct Slot {
    buf: Buffer,
    pending: Vec<bool>,
    full: bool,
}

/// Copy rects of the village's pixels (0xRRGGBB) into a buffer (XRGB8888,
/// the same layout on a little-endian CPU).
fn copy_rects(fb: &[u32], w: usize, dst: &mut [u8], rects: &[(i32, i32, i32, i32)]) {
    for &(x, y, rw, rh) in rects {
        for yy in y.max(0) as usize..(y + rh).max(0) as usize {
            let i = yy * w + x.max(0) as usize;
            let n = (rw.max(0) as usize).min(w.saturating_sub(x.max(0) as usize));
            if (i + n) * 4 > dst.len() || i + n > fb.len() {
                continue;
            }
            let src = &fb[i..i + n];
            let d = &mut dst[i * 4..(i + n) * 4];
            #[cfg(target_endian = "little")]
            d.copy_from_slice(unsafe { std::slice::from_raw_parts(src.as_ptr().cast::<u8>(), n * 4) });
            #[cfg(not(target_endian = "little"))]
            for (d, s) in d.chunks_exact_mut(4).zip(src) {
                d.copy_from_slice(&s.to_le_bytes());
            }
        }
    }
}

/// Draw the village on the desktop until stopped (`stop`), or the
/// compositor takes the surface away (a monitor gone: the service starts it
/// again). `Err`: this desktop can't have it drawn this way.
pub fn run(cfg: Config, seed: u32, path: &Path, stop: &dyn Fn() -> bool) -> Result<(), CantDraw> {
    let cant = |e: &dyn std::fmt::Display| CantDraw(e.to_string());
    let conn = Connection::connect_to_env().map_err(|x| cant(&x))?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|x| cant(&x))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|x| cant(&x))?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|x| cant(&x))?;
    let shm = Shm::bind(&globals, &qh).map_err(|x| cant(&x))?;
    let fracs: Option<frac_mgr::WpFractionalScaleManagerV1> = bind_opt(&globals, &qh);
    let viewporter: Option<wp_viewporter::WpViewporter> = bind_opt(&globals, &qh);
    let shapes: Option<shape_mgr::WpCursorShapeManagerV1> = bind_opt(&globals, &qh);

    let surface = compositor.create_surface(&qh);
    // a fractional scale (1.15…): the buffer at the screen's own pixels,
    // shown at the surface's size, so letters stay sharp
    let frac_obj = match (&fracs, &viewporter) {
        (Some(f), Some(_)) => Some(f.get_fractional_scale(&surface, &qh, ())),
        _ => None,
    };
    let viewport = match (&frac_obj, &viewporter) {
        (Some(_), Some(v)) => Some(v.get_viewport(&surface, &qh, ())),
        _ => None,
    };
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Background, Some("xmas-tui"), None);
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    // (the keyboard once the desktop's clicked: the city, a chat line)
    layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    layer.set_exclusive_zone(-1);
    layer.set_size(0, 0);
    layer.commit();

    let mut st = State {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        shm,
        layer,
        size: (0, 0),
        scale120: 120,
        int_scale: 1,
        configured: false,
        closed: false,
        waiting_frame: None,
        input: vec![],
        pointer: None,
        keyboard: None,
        xkb: None,
        ctrl: false,
        shift: false,
        repeat: (25, 600),
        held: None,
        shapes,
        shape: None,
        enter_serial: 0,
        shape_now: "",
    };
    // (the first configure, and the scale, before anything's laid out)
    let started = Instant::now();
    while !st.configured {
        pump(&mut queue, &mut st, 100).map_err(|x| cant(&x))?;
        if st.closed || started.elapsed() > Duration::from_secs(10) {
            return Err(CantDraw("the compositor never placed the wallpaper".into()));
        }
    }
    eprintln!("xmas-tui: drawing straight onto the desktop (Wayland layer shell; fractional scale {})", if frac_obj.is_some() { "yes" } else { "no" });

    let mut app: Option<App> = None;
    let mut blit = crate::blit::Blit::new();
    crate::frame::set_pictures_ok(true);
    let mut laid: Option<((u32, u32), u32, i32, f32)> = None;
    let mut pool: Option<SlotPool> = None;
    let mut slots: Vec<Slot> = vec![];
    // (the buffer filled last: the compositor's usually let it go by the
    // next frame, and it then needs only this frame's changes)
    let mut last_slot = 0usize;
    let mut cursor = (0.0f64, 0.0f64);
    let mut left = false;
    let mut focused = false;
    let mut next = Instant::now();
    let mut was_resting = false;
    let mut was_quiet = false;
    let watch = crate::rest::spawn();
    // the window snow and lights: on KDE (its overlay reads the windows
    // from KWin)
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("KDE"));
    let fx = kde.then(|| std::sync::Arc::new(std::sync::Mutex::new(crate::winsnow::FxCfg { snow: false, lights: false, ..Default::default() })));
    if let Some(f) = &fx {
        crate::winsnow::overlay::spawn(f.clone());
    }
    crate::tray::spawn(path.to_path_buf());
    // (XMAS_WALL_DEBUG: frames drawn and sent, now and then, and one
    // frame's pixels kept as a picture, for checking without a screen)
    let debug = std::env::var_os("XMAS_WALL_DEBUG");
    let (mut frames, mut commits, mut dbg_at, mut dumped) = (0u64, 0u64, Instant::now(), false);
    let (mut t_step, mut t_draw, mut t_copy) = (0.0f64, 0.0f64, 0.0f64);
    loop {
        // the compositor's news (input, a frame shown, a new size), waiting
        // a little if the next frame isn't due yet
        let wait = next.saturating_duration_since(Instant::now()).as_millis().min(50) as i32;
        pump(&mut queue, &mut st, wait).map_err(|x| cant(&x))?;
        if st.closed {
            eprintln!("xmas-tui: the compositor took the wallpaper away (a monitor gone?)");
            return Ok(());
        }
        if stop() || crate::tray::quit_requested() || crate::update::restart_requested() || app.as_ref().is_some_and(|a| a.quit) {
            return Ok(());
        }
        // the buffer: the screen's own pixels
        let s = if frac_obj.is_some() { st.scale120 as f32 / 120.0 } else { st.int_scale as f32 };
        let (pw, ph) = (((st.size.0 as f32 * s).round() as u32).max(1), ((st.size.1 as f32 * s).round() as u32).max(1));
        let render = app.as_ref().map(|a| a.cfg.scale).unwrap_or(cfg.scale);
        let want = ((pw, ph), st.scale120, st.int_scale, render);
        if laid != Some(want) {
            laid = Some(want);
            let (cw, ch) = cell_size(ph, s, render);
            let (cols, rows) = blit.layout(pw, ph, cw, ch);
            match app.as_mut() {
                Some(a) => {
                    a.resize(cols, rows);
                    a.set_cell_px(cw, ch);
                }
                None => {
                    let mut a = App::new(cfg.clone(), seed, path, cols, rows, true, true, true);
                    a.set_cell_px(cw, ch);
                    a.set_ui_layer_ok(true);
                    a.set_exact_keys();
                    a.start_audio();
                    if crate::save::on() {
                        a.keep_saves();
                    }
                    if let Some(f) = &fx {
                        if let Ok(mut g) = f.lock() {
                            *g = a.fx_cfg();
                        }
                        a.set_fx(f.clone());
                    }
                    app = Some(a);
                }
            }
            match &viewport {
                Some(v) => v.set_destination(st.size.0 as i32, st.size.1 as i32),
                None => st.layer.wl_surface().set_buffer_scale(st.int_scale),
            }
            let mut p = SlotPool::new((pw * ph * 4 * 2) as usize, &st.shm).map_err(|x| cant(&x))?;
            slots.clear();
            for _ in 0..2 {
                let (buf, bytes) = p.create_buffer(pw as i32, ph as i32, (pw * 4) as i32, wl_shm::Format::Xrgb8888).map_err(|x| cant(&x))?;
                bytes.fill(0);
                slots.push(Slot { buf, pending: vec![], full: true });
            }
            pool = Some(p);
            eprintln!("xmas-tui: {pw}x{ph} pixels (scale {s:.2}), {cols}x{rows} cells of {cw}x{ch}");
        }
        let (Some(a), Some(pool)) = (app.as_mut(), pool.as_mut()) else { continue };
        // input, in the order it came
        let (cw, ch) = (blit.raster.cw.max(1) as f64, blit.raster.ch.max(1) as f64);
        let to_cells = |(x, y): (f64, f64)| (((x - blit.offset.0 as f64) / cw) as f32, ((y - blit.offset.1 as f64) / ch) as f32);
        a.set_shift(st.shift);
        for ev in std::mem::take(&mut st.input) {
            let (fx, fy) = to_cells(cursor);
            match ev {
                Input::At(x, y) => {
                    cursor = (x, y);
                    let (fx, fy) = to_cells(cursor);
                    a.mouse_at(if left { MouseIn::Drag } else { MouseIn::Move }, fx, fy);
                }
                Input::Down(b) => {
                    left |= b == Btn::Left;
                    a.mouse_at(MouseIn::Down(b), fx, fy);
                }
                Input::Up(b) => {
                    if b == Btn::Left {
                        left = false;
                    }
                    a.mouse_at(MouseIn::Up(b), fx, fy);
                }
                Input::Wheel { steps, px, finger } => {
                    if finger && !st.ctrl {
                        // a trackpad: smooth, both ways at once
                        a.mouse_at(MouseIn::Pad(-(px.0 / cw) as f32, -(px.1 / ch) as f32), fx, fy);
                    } else if steps.0.abs() > steps.1.abs() || (steps == (0, 0) && px.0.abs() > px.1.abs()) {
                        a.mouse_at(MouseIn::ScrollSide(steps.0 > 0 || px.0 > 0.0), fx, fy);
                    } else if steps.1 != 0 || px.1 != 0.0 {
                        let up = steps.1 < 0 || (steps.1 == 0 && px.1 < 0.0);
                        let ev = if st.ctrl {
                            MouseIn::Zoom(if up { 1.12 } else { 1.0 / 1.12 })
                        } else if st.shift {
                            MouseIn::ScrollSky(up)
                        } else {
                            MouseIn::Scroll(up)
                        };
                        a.mouse_at(ev, fx, fy);
                    }
                }
                Input::Key { k, down } => {
                    if down {
                        a.key(k);
                    } else {
                        a.key_up(k);
                    }
                }
                Input::Ctrl(c) => a.ctrl_key(c, st.shift),
                Input::Help => a.toggle_help(),
                Input::Focus(on) => {
                    focused = on;
                    if !on {
                        a.release_all();
                    }
                }
            }
        }
        // a key held: again and again, at the keyboard's own rate
        if let Some((k, at)) = st.held {
            if Instant::now() >= at {
                a.key_repeat(k);
                st.held = Some((k, Instant::now() + Duration::from_millis((1000 / st.repeat.0.max(1)) as u64)));
            }
        }
        // the pointer's shape
        let want_shape = a.pointer();
        if want_shape != st.shape_now {
            st.shape_now = want_shape;
            if let Some(d) = &st.shape {
                use shape_dev::Shape;
                d.set_shape(
                    st.enter_serial,
                    match want_shape {
                        "pointer" => Shape::Pointer,
                        "grab" => Shape::Grab,
                        "grabbing" => Shape::Grabbing,
                        "text" => Shape::Text,
                        _ => Shape::Default,
                    },
                );
            }
        }
        if Instant::now() < next {
            continue;
        }
        // resting: the tray's "Pause effects", or a game (or the computer
        // busy) while the desktop isn't what has the keyboard
        let why = if crate::tray::paused() {
            Some("paused from the tray".to_string())
        } else if a.rest_for_games() && !focused {
            watch.why()
        } else {
            None
        };
        if why.is_some() != was_resting {
            was_resting = why.is_some();
            eprintln!("xmas-tui: {}", why.as_deref().map(|w| format!("resting ({w})")).unwrap_or_else(|| "carrying on".into()));
        }
        a.set_rest(why);
        let popped = a.popped_out();
        let quiet = popped || a.resting();
        if quiet != was_quiet {
            was_quiet = quiet;
            if let Some(f) = &fx {
                let mut c = a.fx_cfg();
                if quiet {
                    (c.snow, c.lights) = (false, false);
                }
                if let Ok(mut g) = f.lock() {
                    *g = c;
                }
            }
        }
        if popped {
            next = Instant::now() + Duration::from_millis(100);
            continue;
        }
        // the frame: drawn only once the compositor's shown the last (or
        // it's gone quiet, a monitor asleep), into a buffer it isn't reading
        let shown = st.waiting_frame.is_none_or(|at| at.elapsed() > Duration::from_millis(250));
        let free = [last_slot, 1 - last_slot.min(1)].into_iter().filter(|&i| i < slots.len()).find(|&i| slots[i].buf.canvas(pool).is_some());
        let (Some(i), true) = (free, shown) else {
            // (soon again: the next frame callback wakes the loop)
            next = Instant::now() + Duration::from_millis(4);
            continue;
        };
        let frame_dt = a.frame_dt();
        next = if Instant::now() - next > frame_dt { Instant::now() + frame_dt } else { next + frame_dt };
        a.screen_map = Some((pw as f32, ph as f32, blit.offset.0 as f32, blit.offset.1 as f32, blit.raster.cw as f32, blit.raster.ch as f32));
        a.frame.cell_aspect = blit.raster.ch as f32 / blit.raster.cw.max(1) as f32;
        let stepping = Instant::now();
        a.step();
        t_step += stepping.elapsed().as_secs_f64();
        let drawing = Instant::now();
        let drawn = blit.draw(a);
        t_draw += drawing.elapsed().as_secs_f64();
        let copying = Instant::now();
        // (what changed: copied cell by cell, a run of them at a time; told
        // the compositor in tiles, a handful of rects rather than one a cell)
        let changed = if drawn.full { vec![(0, 0, pw as i32, ph as i32)] } else { crate::winsnow::merge_damage(&drawn.rects, pw as i32, ph as i32) };
        for s in slots.iter_mut() {
            if drawn.full || s.pending.len() != drawn.cells.len() && !s.pending.is_empty() {
                s.full = true;
                s.pending.clear();
            } else if !s.full {
                if s.pending.is_empty() {
                    s.pending = vec![false; drawn.cells.len()];
                }
                for (p, &d) in s.pending.iter_mut().zip(&drawn.cells) {
                    *p |= d;
                }
            }
        }
        let first = slots[i].full;
        if first || !changed.is_empty() {
            let s = &mut slots[i];
            let Some(bytes) = s.buf.canvas(pool) else { continue };
            if s.full {
                copy_rects(&blit.fb, pw as usize, bytes, &[(0, 0, pw as i32, ph as i32)]);
            } else {
                // this frame's changes, and any older ones this buffer missed
                let cols = a.scene_size().0;
                copy_rects(&blit.fb, pw as usize, bytes, &blit.rects_of(&s.pending, cols));
            }
            let surf = st.layer.wl_surface();
            if first {
                surf.damage_buffer(0, 0, pw as i32, ph as i32);
            } else {
                for (x, y, w, h) in &changed {
                    surf.damage_buffer(*x, *y, *w, *h);
                }
            }
            s.full = false;
            s.pending.fill(false);
            last_slot = i;
            surf.frame(&qh, surf.clone());
            st.waiting_frame = Some(Instant::now());
            if s.buf.attach_to(surf).is_ok() {
                st.layer.commit();
                commits += 1;
            }
        }
        t_copy += copying.elapsed().as_secs_f64();
        a.drawn(drawing.elapsed().as_secs_f32());
        frames += 1;
        if let Some(d) = &debug {
            if dbg_at.elapsed() > Duration::from_secs(5) {
                dbg_at = Instant::now();
                let n = frames.max(1) as f64 * 0.001;
                let area: i64 = drawn.rects.iter().map(|r| (r.2 * r.3) as i64).sum();
                eprintln!("xmas-tui: (debug) {frames} frames, {commits} sent; this one {} rects, {} cell runs, {:.1}% of the screen; a frame: step {:.2} ms, draw {:.2} ms, copy {:.2} ms", changed.len(), drawn.rects.len(), area as f64 * 100.0 / (pw as f64 * ph as f64), t_step / n, t_draw / n, t_copy / n);
                if !dumped {
                    dumped = true;
                    let path = std::path::Path::new(d).join("wall.png");
                    if let Ok(file) = std::fs::File::create(&path) {
                        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), pw, ph);
                        enc.set_color(png::ColorType::Rgb);
                        enc.set_depth(png::BitDepth::Eight);
                        if let Ok(mut wr) = enc.write_header() {
                            let img: Vec<u8> = blit.fb.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
                            let _ = wr.write_image_data(&img);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Letters about kitty's 10-point size at the desktop's scale, never
    /// fewer than 75 rows: the 5120x1440 screen at 1.15 gets the same grid
    /// kitty gave it (568x75).
    #[test]
    fn cells_like_kittys() {
        let (cw, ch) = cell_size(1440, 1.15, 1.0);
        assert_eq!((cw, ch), (9, 19));
        assert_eq!((5120 / cw, 1440 / ch), (568, 75));
        let (_, ch) = cell_size(1080, 1.0, 1.0);
        assert!(1080 / ch >= 75);
        let (_, ch) = cell_size(1440, 2.0, 1.0);
        assert!(1440 / ch >= 75, "a big scale still keeps 75 rows");
    }

    #[test]
    fn keys_to_the_village() {
        use xkbcommon_dl::keysyms as k;
        assert_eq!(key_in(k::Return, None), Some(KeyIn::Enter));
        assert_eq!(key_in(k::a, Some('a')), Some(KeyIn::Char('a')));
        assert_eq!(key_in(k::Shift_L, None), None);
        assert_eq!(key_in(k::Tab, Some('\t')), None);
    }

    #[test]
    fn copying_rects_of_pixels() {
        let fb: Vec<u32> = (0..16).collect();
        let mut dst = vec![0u8; 64];
        copy_rects(&fb, 4, &mut dst, &[(1, 1, 2, 2)]);
        let px = |i: usize| u32::from_le_bytes(dst[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((px(5), px(6), px(9), px(10)), (5, 6, 9, 10));
        assert_eq!((px(0), px(4), px(7)), (0, 0, 0));
    }
}
