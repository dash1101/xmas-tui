// No console window when double-clicked on Windows (release builds).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod audio;
mod color;
mod clock;
mod config;
mod ctxmenu;
mod export;
mod frame;
mod icon;
mod platform;
mod raster;
mod rng;
mod scene;
mod setwin;
mod settings;
mod term;
mod tray;
mod update;
mod widgets;
mod window;
#[cfg(windows)]
mod winsys;
#[cfg(any(target_os = "linux", windows))]
mod winsnow;

use app::{App, Btn, KeyIn, MouseIn};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use frame::Frame;
use scene::Scene;
use settings::Settings;
use std::time::{Duration, Instant};
use term::Term;
use widgets::Widgets;

struct Args {
    config: Option<String>,
    export: Option<String>,
    frames: usize,
    size: Option<(usize, usize)>,
    at: f32,
    sleigh: bool,
    bench: usize,
    fps: Option<f32>,
    zoom: Option<f32>,
    /// Previews: the controls card.
    controls: bool,
    /// Previews: poke something in the room (0 stairs, 1 bed, 2 sofa,
    /// 3 oven) and let it play this many seconds.
    poke: Option<(u8, f32)>,
    wallpaper: bool,
    settings: bool,
    tab: usize,
    demo: bool,
    windowed: bool,
    /// A normal window, full screen (like a game).
    fullscreen: bool,
    terminal: bool,
    settings_window: bool,
    rows: u32,
    export_audio: Option<String>,
    audio_test: f32,
    pan: f32,
    after_update: Option<u32>,
    /// --export: raw RGB frames to stdout (for ffmpeg) instead of PNGs.
    raw: bool,
    /// --export: just the scene (no widgets, settings gear or prompts).
    clean: bool,
    /// --export: inside house N (or the church).
    inside: Option<usize>,
    /// --export: look this many rows up into the sky.
    sky: f32,
    /// --export: time of day (as the setting: night, dawn, noon…).
    time: Option<String>,
    /// --export: at the cocoa cabin with the menu and pockets open.
    shop: bool,
    /// --export: talking with someone (0 = a grown-up, 1 = a kid).
    talk: Option<u8>,
    /// --export: people out with their pets.
    pets: bool,
    /// --export: a room of the home (with `--inside 900`), and looking out
    /// of its window (0) or through the telescope (1).
    room: Option<usize>,
    look: Option<u8>,
    /// --export: "host" (talking with them, indoors) or "pet" (its menu).
    ui: Option<String>,
    /// --export: the stats card open / a right-click menu at (x, y).
    stats: bool,
    menu: Option<(i32, i32)>,
    /// --export: the visualizer on, listening for real (frames in real time).
    viz: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        config: None,
        export: None,
        frames: 1,
        size: None,
        at: 12.0,
        sleigh: false,
        bench: 0,
        fps: None,
        zoom: None,
        controls: false,
        poke: None,
        wallpaper: false,
        settings: false,
        tab: 0,
        demo: false,
        windowed: false,
        fullscreen: false,
        terminal: false,
        settings_window: false,
        rows: 68,
        export_audio: None,
        audio_test: 0.0,
        pan: 0.0,
        after_update: None,
        raw: false,
        clean: false,
        inside: None,
        sky: 0.0,
        time: None,
        shop: false,
        talk: None,
        pets: false,
        room: None,
        look: None,
        ui: None,
        stats: false,
        menu: None,
        viz: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = it.next(),
            "--export" => a.export = it.next(),
            "--export-audio" => a.export_audio = it.next(),
            "--audio-test" => a.audio_test = it.next().and_then(|v| v.parse().ok()).unwrap_or(8.0),
            "--frames" => a.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            "--at" => a.at = it.next().and_then(|v| v.parse().ok()).unwrap_or(12.0),
            "--sleigh" => a.sleigh = true,
            "--settings" => a.settings = true,
            "--tab" => a.tab = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--demo" => a.demo = true,
            "--update" => {}
            "--raw" => a.raw = true,
            "--clean" => a.clean = true,
            "--time" => a.time = it.next(),
            "--date" => {
                if let Some(d) = it.next() {
                    std::env::set_var("XMAS_NOW", d);
                }
            }
            "--shop" => a.shop = true,
            "--talk" => a.talk = it.next().and_then(|v| v.parse().ok()),
            "--pets" => a.pets = true,
            "--room" => a.room = it.next().and_then(|v| v.parse().ok()),
            "--look" => a.look = it.next().and_then(|v| v.parse().ok()),
            "--ui" => a.ui = it.next(),
            "--stats" => a.stats = true,
            "--viz" => a.viz = true,
            "--controls" => a.controls = true,
            "--menu" => {
                a.menu = it.next().and_then(|v| {
                    let (x, y) = v.split_once(',')?;
                    Some((x.parse().ok()?, y.parse().ok()?))
                })
            }
            "--sky" => a.sky = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--inside" => a.inside = it.next().and_then(|v| v.parse().ok()),
            "--after-update" => a.after_update = it.next().and_then(|v| v.parse().ok()),
            "--pan" => a.pan = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--wallpaper" => a.wallpaper = true,
            "--windowed" => a.windowed = true,
            "--fullscreen" => {
                a.windowed = true;
                a.fullscreen = true;
            }
            "--terminal" => a.terminal = true,
            "--settings-window" => a.settings_window = true,
            "--rows" => a.rows = it.next().and_then(|v| v.parse().ok()).unwrap_or(68),
            "--bench" => a.bench = it.next().and_then(|v| v.parse().ok()).unwrap_or(300),
            "--fps" => a.fps = it.next().and_then(|v| v.parse().ok()),
            "--zoom" => a.zoom = it.next().and_then(|v| v.parse().ok()),
            "--poke" => {
                a.poke = it.next().and_then(|v| {
                    let (k, secs) = v.split_once(',').unwrap_or((v.as_str(), "1"));
                    Some((k.parse().ok()?, secs.parse().ok()?))
                })
            }
            "--size" => {
                a.size = it.next().and_then(|v| {
                    let (w, h) = v.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                })
            }
            "-h" | "--help" => {
                println!(
                    "xmas-tui — cozy glowing Christmas scene for your terminal / desktop\n\n\
                     USAGE: xmas-tui [--config PATH] [--fps N]           (Linux: run inside a terminal; Windows: desktop wallpaper)\n\
                     \x20      xmas-tui --wallpaper                       (terminal mode, keys don't quit — used by the kitty launcher)\n\
                     \x20      xmas-tui --windowed [--rows N]             (a normal window, any OS)\n\
                     \x20      xmas-tui --fullscreen [--rows N]           (full screen, F11 toggles)\n\
                     \x20      xmas-tui --settings-window [--export DIR]  (just the settings panel; what the tray icon opens)\n\
                     \x20      xmas-tui --export DIR [--size 568x75] [--frames N] [--at SECONDS] [--sleigh] [--settings] [--demo] [--pan CELLS] [--clean] [--raw]\n\
                     \x20      xmas-tui --bench FRAMES [--size WxH]\n\
                     \x20      xmas-tui --update                         (install a newer release now; release builds)\n\
                     \x20      xmas-tui --export-audio DIR                (render all sounds/music to WAV; nothing is played)\n\
                     \x20      xmas-tui --audio-test SECONDS              (drive the real sound output; see XMAS_AUDIO_DEV_* in src/audio/out.rs)\n\n\
                     Mouse: drag widgets, click things in the scene, ⚙ or right-click for settings.\n\
                     Config: {}",
                    config::default_path().display()
                );
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    a
}

fn main() {
    let args = parse_args();
    if std::env::args().any(|a| a == "--update") {
        std::process::exit(update::cli());
    }
    if let Some(pid) = args.after_update {
        // just updated: let the previous version finish exiting first
        update::wait_for(pid);
    }
    let cfg = config::load(args.config.as_deref());
    if let Some(f) = args.fps {
        // interactive modes: an override for this run only, kept out of the
        // config the app saves (see `app::set_fps_override`)
        app::set_fps_override(f);
    }
    let seed = if cfg.seed == 0 { app::random_seed() } else { cfg.seed as u32 };
    let path = config::resolve(args.config.as_deref());
    // the icon and launchers the Linux desktop looks windows up by
    #[cfg(target_os = "linux")]
    if (args.wallpaper || args.windowed || args.settings_window) && args.export.is_none() {
        std::thread::spawn(icon::install_desktop_files);
    }
    if args.settings_window {
        let res = match &args.export {
            Some(dir) => setwin::export(cfg, &path, dir).map_err(|e| e.into()),
            None => setwin::run(cfg, path),
        };
        if let Err(e) = res {
            eprintln!("xmas-tui: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(dir) = &args.export_audio {
        if let Err(e) = audio::export::export_all(dir, &cfg.sound) {
            eprintln!("xmas-tui: audio export failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.audio_test > 0.0 {
        audio::run_test(args.audio_test, &cfg.sound);
        return;
    }
    if args.export.is_some() || args.bench > 0 {
        // the one-shot tools never save, so they can just use the override
        let mut cfg = cfg.clone();
        if let Some(f) = args.fps {
            cfg.fps = config::clean_fps(f);
        }
        if let Some(z) = args.zoom {
            cfg.zoom = z.clamp(1.0, 3.0);
        }
        match &args.export {
            Some(dir) => run_export(&cfg, seed, dir, &args),
            None => run_bench(&cfg, seed, &args),
        }
        return;
    }
    let native = args.windowed || (cfg!(windows) && !args.terminal);
    if native {
        let desktop = cfg!(windows) && !args.windowed;
        #[cfg(windows)]
        {
            redirect_stderr_to_log(if desktop { "wallpaper.log" } else { "window.log" });
            let (major, build) = winsys::windows_version();
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            eprintln!("--- xmas-tui {} starting ({}; Windows {major} build {build}; unix time {secs})", env!("CARGO_PKG_VERSION"), if desktop { "wallpaper" } else { "window" });
        }
        if desktop && !platform::single_instance() {
            return; // already running
        }
        if desktop {
            tray::spawn(path.clone());
        }
        let res = window::run(cfg, seed, path, window::Opts { desktop, rows: args.rows, fullscreen: args.fullscreen });
        tray::shutdown();
        if let Err(e) = res {
            eprintln!("xmas-tui: {e}");
            std::process::exit(1);
        }
        if update::restart_requested() {
            update::relaunch();
        }
        return;
    }
    if let Err(e) = run_terminal(cfg, seed, &path, args.wallpaper) {
        Term::leave();
        eprintln!("xmas-tui: {e}");
        std::process::exit(1);
    }
    if update::restart_requested() {
        update::relaunch();
    }
}

/// Terminal front-end (the kitty wallpaper on Linux): crossterm events in,
/// diffed escape sequences out.
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Set once this process started the window-snow overlay (whose bridge
/// loads the "xmas-snow" KWin script).
#[cfg(target_os = "linux")]
static FX_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Did this process load the KWin script? The overlay's bridge owns the
/// D-Bus name the script reports to for as long as the script is ours; a
/// second instance fails to get the name (and doesn't load the script).
#[cfg(target_os = "linux")]
fn owns_kwin_script() -> bool {
    if !FX_STARTED.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    let owner_pid = || -> zbus::Result<u32> {
        let conn = zbus::blocking::Connection::session()?;
        let dbus = zbus::blocking::fdo::DBusProxy::new(&conn)?;
        // same name as winsnow::kde::NAME
        let name = zbus::names::BusName::try_from("org.xmastui.Snow")?;
        Ok(dbus.get_connection_unix_process_id(name)?)
    };
    owner_pid().map(|pid| pid == std::process::id()).unwrap_or(false)
}

fn shutdown_extras() {
    tray::shutdown();
    // only the instance that loaded the KWin script may unload it: quitting
    // a preview in Konsole (or a second wallpaper instance) must not take
    // the running wallpaper's window snow down with it
    #[cfg(target_os = "linux")]
    if owns_kwin_script() {
        winsnow::kde::unload_script();
    }
}

/// As the wallpaper our stderr is kitty's terminal: anything printed there
/// would scribble over the scene. Send it to a log file instead.
#[cfg(unix)]
fn redirect_stderr_to_log() {
    use std::os::fd::AsRawFd;
    let dir = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state"))
        .join("xmas-tui");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("wallpaper.log");
    // keep the log small: start fresh once it passes 1 MB
    let big = std::fs::metadata(&path).map(|m| m.len() > 1 << 20).unwrap_or(false);
    let file = std::fs::OpenOptions::new().create(true).append(!big).write(true).truncate(big).open(&path);
    if let Ok(f) = file {
        unsafe {
            libc::dup2(f.as_raw_fd(), 2);
        }
    }
}

/// Windows (a GUI app: no console, so stderr goes nowhere): send it to
/// %LOCALAPPDATA%\xmas-tui\<name>, so what went wrong (the desktop attach,
/// a panic) can be read afterwards.
#[cfg(windows)]
fn redirect_stderr_to_log(name: &str) {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    let Some(base) = std::env::var_os("LOCALAPPDATA") else { return };
    let dir = std::path::PathBuf::from(base).join("xmas-tui");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(name);
    // keep the log small: start fresh once it passes 1 MB
    let big = std::fs::metadata(&path).map(|m| m.len() > 1 << 20).unwrap_or(false);
    let file = std::fs::OpenOptions::new().create(true).append(!big).write(true).truncate(big).open(&path);
    if let Ok(f) = file {
        unsafe {
            // (the handle stays open for the life of the process)
            SetStdHandle(STD_ERROR_HANDLE, f.into_raw_handle() as _);
        }
    }
}

fn run_terminal(cfg: config::Config, seed: u32, path: &std::path::Path, wallpaper: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    if wallpaper {
        redirect_stderr_to_log();
    }
    std::panic::set_hook(Box::new(|info| {
        Term::leave();
        shutdown_extras();
        eprintln!("{info}");
    }));
    // exit cleanly (and unload the KWin script) when systemd / kitty stop us
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as usize);
        libc::signal(libc::SIGHUP, on_signal as *const () as usize);
        libc::signal(libc::SIGINT, on_signal as *const () as usize);
        // if kitty (our parent) goes away, we're told to stop too
        #[cfg(target_os = "linux")]
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
    }
    Term::enter()?;
    let (w, h) = crossterm::terminal::size().map(|(w, h)| (w as usize, h as usize))?;
    let mut app = App::new(cfg, seed, path, w, h, wallpaper, true, false);
    // kitty's keyboard protocol reports releases: keys are held until let go
    if crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
        app.set_exact_keys();
    }
    app.start_audio();
    // window snow + lights overlay: only as the actual wallpaper on KDE Wayland
    #[cfg(target_os = "linux")]
    let mut fx_handle = None;
    #[cfg(target_os = "linux")]
    {
        let kde = std::env::var("XDG_CURRENT_DESKTOP").map(|d| d.contains("KDE")).unwrap_or(false);
        if wallpaper && kde && std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let fx = std::sync::Arc::new(std::sync::Mutex::new(app.fx_cfg()));
            winsnow::overlay::spawn(fx.clone());
            FX_STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
            app.set_fx(fx.clone());
            fx_handle = Some(fx);
        }
    }
    // tray / status icon (settings window, quick toggles) for the wallpaper
    if wallpaper {
        tray::spawn(path.to_path_buf());
    }
    let mut was_paused = false;
    let mut term = Term::new();
    let mut pointer = "";
    let mut next = Instant::now();
    loop {
        // bounded per frame, and bail out promptly once asked to stop (a
        // hung-up terminal can report "readable" forever)
        let mut budget = 256;
        while budget > 0 && !STOP.load(std::sync::atomic::Ordering::SeqCst) && event::poll(Duration::ZERO)? {
            budget -= 1;
            match event::read()? {
                Event::Mouse(m) => {
                    let (x, y) = (m.column as i32, m.row as i32);
                    let ev = match m.kind {
                        MouseEventKind::Moved => MouseIn::Move,
                        MouseEventKind::Down(MouseButton::Left) => MouseIn::Down(Btn::Left),
                        MouseEventKind::Down(MouseButton::Right) => MouseIn::Down(Btn::Right),
                        MouseEventKind::Drag(MouseButton::Left) => MouseIn::Drag,
                        MouseEventKind::Up(MouseButton::Left) => MouseIn::Up(Btn::Left),
                        MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::CONTROL) => MouseIn::ScrollSky(true),
                        MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::CONTROL) => MouseIn::ScrollSky(false),
                        MouseEventKind::ScrollUp => MouseIn::Scroll(true),
                        MouseEventKind::ScrollDown => MouseIn::Scroll(false),
                        _ => continue,
                    };
                    app.mouse(ev, x, y);
                }
                Event::Key(k) => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        if k.kind != KeyEventKind::Release {
                            match k.code {
                                KeyCode::Char('c') if !wallpaper => break,
                                KeyCode::Char(c) => app.ctrl_key(c, k.modifiers.contains(KeyModifiers::SHIFT)),
                                _ => {}
                            }
                        }
                        continue;
                    }
                    if k.code == KeyCode::F(1) {
                        if k.kind == KeyEventKind::Press {
                            app.toggle_help();
                        }
                        continue;
                    }
                    if k.code == KeyCode::F(11) {
                        if k.kind == KeyEventKind::Press {
                            app.toggle_wall_full();
                        }
                        continue;
                    }
                    if k.kind != KeyEventKind::Release {
                        app.set_shift(k.modifiers.contains(KeyModifiers::SHIFT));
                    }
                    let key = match k.code {
                        KeyCode::Char(c) => KeyIn::Char(c),
                        KeyCode::Backspace => KeyIn::Backspace,
                        KeyCode::Enter => KeyIn::Enter,
                        KeyCode::Esc => KeyIn::Esc,
                        KeyCode::Up => KeyIn::Up,
                        KeyCode::Down => KeyIn::Down,
                        KeyCode::Left => KeyIn::Left,
                        KeyCode::Right => KeyIn::Right,
                        _ => continue,
                    };
                    match k.kind {
                        KeyEventKind::Press => app.key(key),
                        KeyEventKind::Repeat => app.key_repeat(key),
                        KeyEventKind::Release => app.key_up(key),
                    }
                }
                Event::Resize(_, _) => term.invalidate(),
                // releases can go missing while unfocused: let go of everything
                Event::FocusLost => app.release_all(),
                _ => {}
            }
        }
        if app.quit || STOP.load(std::sync::atomic::Ordering::SeqCst) || tray::quit_requested() || update::restart_requested() || app::RESTART_FOR_SCALE.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        // tray "Pause effects": hold the last frame, hide window snow/lights
        // (the same while the village is popped out into a window)
        let paused = tray::paused() || app.popped_out();
        if paused != was_paused {
            was_paused = paused;
            #[cfg(target_os = "linux")]
            if let Some(fx) = &fx_handle {
                let mut c = app.fx_cfg();
                if paused {
                    (c.snow, c.lights) = (false, false);
                }
                if let Ok(mut f) = fx.lock() {
                    *f = c;
                }
            }
        }
        if paused {
            std::thread::sleep(Duration::from_millis(100));
            next = Instant::now();
            continue;
        }
        let (nw, nh) = crossterm::terminal::size().map(|(w, h)| (w as usize, h as usize))?;
        if (nw, nh) != (app.w, app.h) && nw > 0 && nh > 0 {
            app.resize(nw, nh);
            term.invalidate();
        }
        let want = app.pointer();
        if want != pointer {
            pointer = want;
            Term::pointer(want);
        }
        let (w, h) = (app.w, app.h);
        let cells = app.step();
        if let Err(e) = term.draw(cells, w, h) {
            Term::leave();
            shutdown_extras();
            return Err(e);
        }
        let frame_dt = app.frame_dt();
        next += frame_dt;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
    Term::leave();
    shutdown_extras();
    Ok(())
}

fn run_export(cfg: &config::Config, seed: u32, dir: &str, args: &Args) {
    let mut cfg = cfg.clone();
    cfg.widgets.visualizer |= args.viz;
    let cfg = &cfg;
    let (w, h) = args.size.unwrap_or((568, 75));
    std::fs::create_dir_all(dir).expect("create export dir");
    let mut frame = Frame::new(w, h);
    frame.glow_near = cfg.scene.glow;
    frame.glow_far = cfg.scene.glow;
    let mut scene = Scene::new(w, h, seed, &cfg.scene);
    let mut widgets = Widgets::new(cfg);
    // give the data threads a moment to report
    std::thread::sleep(Duration::from_millis(2600));
    let dt = 1.0 / cfg.fps.max(1.0);
    let mut t = 0.0;
    if args.sleigh {
        scene.sleigh.trigger((args.at - 9.0).max(0.0));
    }
    scene.jump(args.pan);
    scene.look_up(args.sky);
    if let Some(m) = &args.time {
        scene.set_daylight(scene::daylight::compute(m, 720.0, 450.0, 1020.0));
    }
    while t < args.at {
        scene.update(dt, t);
        t += dt;
    }
    let mut ex = export::Exporter::new();
    let mut settings = Settings::default();
    settings.open = args.settings;
    settings.help = args.controls;
    settings.tab = args.tab;
    settings.window_fx = true;
    // previews: plausible levels for the meters
    if args.settings {
        let mut m = [0.0f32; audio::METERS];
        m[audio::METER_MUSIC] = 0.18;
        m[audio::METER_AMB] = 0.05;
        m[2] = 0.3;
        m[5] = 0.08;
        settings.meters = Some(m);
    }
    if args.demo {
        scene.demo(t);
        for _ in 0..(3.0 / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
        widgets.hover = Some(1);
    }
    if let Some(i) = args.inside {
        scene.preview_inside(i);
        if args.room.is_some() || args.look.is_some() {
            scene.preview_room(args.room.unwrap_or(0), args.look);
        }
        for _ in 0..(4.0 / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
    }
    if let Some((k, secs)) = args.poke {
        scene.preview_poke(k);
        for _ in 0..(secs / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
    }
    if args.shop {
        scene.preview_shop();
    }
    if let Some(u) = &args.ui {
        scene.preview_ui(u);
        for _ in 0..(2.5 / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
    }
    if args.pets {
        scene.preview_pets();
        for _ in 0..(1.0 / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
    }
    if let Some(k) = args.talk {
        scene.preview_talk(k == 1);
        for _ in 0..(2.0 / dt) as usize {
            scene.update(dt, t);
            t += dt;
        }
    }
    if args.stats {
        scene.act(scene::Do::Stats);
    }
    let menu = args.menu.map(|(x, y)| {
        let (title, items) = scene.menu_at(x, y);
        ctxmenu::CtxMenu::new(x, y, title, items, w as i32, h as i32)
    });
    if args.clean {
        scene.quiet_player();
    }
    let mut out = std::io::BufWriter::with_capacity(1 << 22, std::io::stdout().lock());
    let mut spare = Frame::new(1, 1);
    for i in 0..args.frames {
        scene.update(dt, t);
        frame.clear();
        scene.render(&mut frame, &mut spare, t, cfg.zoom, None);
        if !args.clean {
            widgets.map = Some(scene.minimap());
            widgets.draw(&mut frame, t);
            settings.draw_gear(&mut frame, t);
            settings.draw(&mut frame, cfg, t);
            settings.draw_help(&mut frame);
        }
        if let Some(m) = &menu {
            m.draw(&mut frame);
        }
        frame.compose();
        if args.raw {
            use std::io::Write;
            let (img, iw, ih) = ex.rgb(&frame.out, w, h);
            if i == 0 {
                eprintln!("raw rgb24 {iw}x{ih}");
            }
            if out.write_all(&img).is_err() {
                return; // the reader went away
            }
        } else {
            let p = std::path::Path::new(dir).join(format!("frame_{i:04}.png"));
            ex.render(&frame.out, w, h, &p).expect("write png");
        }
        t += dt;
        if args.viz {
            std::thread::sleep(Duration::from_secs_f32(dt));
        }
    }
    eprintln!("exported {} frame(s) of {}x{} cells to {}", args.frames, w, h, dir);
}

fn run_bench(cfg: &config::Config, seed: u32, args: &Args) {
    let (w, h) = args.size.unwrap_or((568, 75));
    let mut frame = Frame::new(w, h);
    let mut scene = Scene::new(w, h, seed, &cfg.scene);
    let mut widgets = Widgets::new(cfg);
    let dt = 1.0 / cfg.fps.max(1.0);
    let mut t = 0.0;
    let mut prev: Vec<frame::Cell> = vec![];
    let (mut t_sim, mut t_comp, mut changed) = (0.0f64, 0.0f64, 0usize);
    let mut term = Term::new();
    let (mut bytes, mut t_term) = (0usize, 0.0f64);
    let mut spare = Frame::new(1, 1);
    for _ in 0..args.bench {
        let a = Instant::now();
        scene.update(dt, t);
        frame.clear();
        scene.render(&mut frame, &mut spare, t, cfg.zoom, None);
        widgets.draw(&mut frame, t);
        let b = Instant::now();
        frame.compose();
        let c = Instant::now();
        t_sim += (b - a).as_secs_f64();
        t_comp += (c - b).as_secs_f64();
        if prev.len() == frame.out.len() {
            changed += frame.out.iter().zip(&prev).filter(|(x, y)| x != y).count();
        }
        prev = frame.out.clone();
        let d = Instant::now();
        let n = term.render(&frame.out, w, h);
        t_term += d.elapsed().as_secs_f64();
        if t > 0.0 {
            bytes += n;
        }
        t += dt;
    }
    let n = args.bench as f64;
    println!(
        "{}x{} cells: scene+widgets {:.2} ms/frame, compose {:.2} ms/frame, ~{:.0} cells changed/frame ({:.1}%), encode {:.2} ms/frame, ~{:.0} KB/frame",
        w,
        h,
        t_sim / n * 1e3,
        t_comp / n * 1e3,
        changed as f64 / (n - 1.0),
        changed as f64 / (n - 1.0) / (w * h) as f64 * 100.0,
        t_term / n * 1e3,
        bytes as f64 / (n - 1.0) / 1024.0
    );
    #[cfg(target_os = "linux")]
    {
        let w = &cfg.windows;
        let _ = w;
        let fx = winsnow::FxCfg::from_cfg(&cfg.windows);
        print!("{}", winsnow::bench::run(args.bench, fx));
    }
}
