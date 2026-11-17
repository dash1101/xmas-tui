//! Linux tray icon: a StatusNotifierItem (the protocol KDE Plasma's system
//! tray speaks; also GNOME's AppIndicator extension, waybar, …) plus a
//! com.canonical.dbusmenu right-click menu, served with zbus from one
//! connection on its own thread.
//!
//! Left click opens the settings window; right click shows the menu
//! (Menu…, Window snow, Sound, Pause effects, Quit wallpaper). Hosts
//! that can't show a dbusmenu call ContextMenu instead, which also opens
//! the settings window. We re-register whenever the watcher (plasmashell)
//! restarts, and release our bus name on shutdown so the icon disappears
//! at once (process exit would drop it too).

use super::{flag, open_settings, open_settings_activated, quit_wallpaper, toggle_flag, toggle_pause, SOUND, TITLE, WINDOW_SNOW};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, StructureBuilder, Value};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
/// A fixed name next to the per-pid SNI one, so other processes (the
/// settings window) can find the wallpaper's tray.
const FIXED_NAME: &str = "org.xmastui.Tray";

/// Our connection, kept so `shutdown()` can drop the bus name.
static CONN: Mutex<Option<(zbus::blocking::Connection, String)>> = Mutex::new(None);

/// ARGB32 (network byte order) pixmaps at the sizes trays ask for.
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

fn pixmaps() -> Pixmaps {
    [16usize, 22, 32, 64]
        .iter()
        .map(|&n| {
            let rgba = super::icon_rgba(n);
            let argb = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            (n as i32, n as i32, argb)
        })
        .collect()
}

struct Item {
    config: PathBuf,
    icon: Pixmaps,
    /// XDG activation token the host handed us for the next click.
    token: Mutex<Option<String>>,
}

impl Item {
    fn open(&self) {
        let token = self.token.lock().ok().and_then(|mut t| t.take());
        open_settings_activated(&self.config, token);
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }
    #[zbus(property)]
    fn id(&self) -> String {
        "xmas-tui".into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        TITLE.into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "Active".into()
    }
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        self.icon.clone()
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Pixmaps {
        vec![]
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        vec![]
    }
    #[zbus(property)]
    fn attention_movie_name(&self) -> String {
        String::new()
    }
    /// (icon name, icon pixmaps, title, description)
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Pixmaps, String, String) {
        (String::new(), vec![], TITLE.into(), "Click for settings · right-click for quick toggles".into())
    }
    /// false: left click is Activate, the menu is only on right click.
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("valid path")
    }

    fn activate(&self, _x: i32, _y: i32) {
        self.open();
    }
    fn secondary_activate(&self, _x: i32, _y: i32) {
        self.open();
    }
    /// Only called by hosts that don't render our dbusmenu themselves.
    fn context_menu(&self, _x: i32, _y: i32) {
        self.open();
    }
    /// Plasma sends this right before Activate (SNI extension), so the
    /// window we open is allowed to take focus on Wayland.
    fn provide_xdg_activation_token(&self, token: String) {
        if let Ok(mut t) = self.token.lock() {
            *t = Some(token);
        }
    }
    fn scroll(&self, _delta: i32, _orientation: String) {}
}

// ------------------------------------------------------------- the menu

const ID_SETTINGS: i32 = 1;
const ID_SNOW: i32 = 3;
const ID_SOUND: i32 = 4;
const ID_PAUSE: i32 = 5;
const ID_QUIT: i32 = 7;

/// (id, label, checkbox state); label "-" = separator.
fn entries(config: &std::path::Path) -> Vec<(i32, &'static str, Option<bool>)> {
    vec![
        (ID_SETTINGS, "Menu…", None),
        (2, "-", None),
        (ID_SNOW, "Window snow", Some(flag(config, WINDOW_SNOW))),
        (ID_SOUND, "Sound", Some(flag(config, SOUND))),
        (ID_PAUSE, "Pause effects", Some(super::paused())),
        (6, "-", None),
        (ID_QUIT, "Quit wallpaper", None),
    ]
}

fn ov<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
    v.into().try_to_owned().expect("plain value")
}

fn props(label: &str, check: Option<bool>) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    if label == "-" {
        m.insert("type".into(), ov("separator"));
        return m;
    }
    m.insert("label".into(), ov(label));
    m.insert("enabled".into(), ov(true));
    m.insert("visible".into(), ov(true));
    if let Some(on) = check {
        m.insert("toggle-type".into(), ov("checkmark"));
        m.insert("toggle-state".into(), ov(on as i32));
    }
    m
}

/// One menu entry as the `v` wrapping a `(ia{sv}av)` that dbusmenu wants.
fn node(id: i32, label: &str, check: Option<bool>) -> OwnedValue {
    let s = StructureBuilder::new()
        .add_field(id)
        .add_field(props(label, check))
        .add_field(Vec::<OwnedValue>::new())
        .build()
        .expect("menu node");
    ov(s)
}

struct Menu {
    config: PathBuf,
    /// (layout revision, the checkbox states it was built with)
    rev: Mutex<(u32, Vec<Option<bool>>)>,
}

impl Menu {
    /// Bump the revision if the checkbox states changed; true if they did.
    fn refresh(&self) -> bool {
        let now: Vec<Option<bool>> = entries(&self.config).iter().map(|e| e.2).collect();
        let Ok(mut r) = self.rev.lock() else { return false };
        if r.1 != now {
            r.0 += 1;
            r.1 = now;
            true
        } else {
            false
        }
    }
    fn revision(&self) -> u32 {
        self.rev.lock().map(|r| r.0).unwrap_or(1)
    }
}

type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(&self, parent_id: i32, _recursion_depth: i32, _property_names: Vec<String>) -> zbus::fdo::Result<(u32, Layout)> {
        self.refresh();
        let items = entries(&self.config);
        let layout = if parent_id == 0 {
            let mut root = HashMap::new();
            root.insert("children-display".to_string(), ov("submenu"));
            (0, root, items.iter().map(|&(id, l, c)| node(id, l, c)).collect())
        } else {
            let &(id, l, c) = items
                .iter()
                .find(|e| e.0 == parent_id)
                .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no menu item {parent_id}")))?;
            (id, props(l, c), vec![])
        };
        Ok((self.revision(), layout))
    }

    fn get_group_properties(&self, ids: Vec<i32>, _property_names: Vec<String>) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        entries(&self.config)
            .into_iter()
            .filter(|e| ids.is_empty() || ids.contains(&e.0))
            .map(|(id, l, c)| (id, props(l, c)))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let e = entries(&self.config).into_iter().find(|e| e.0 == id);
        let mut p = e.map(|(_, l, c)| props(l, c)).unwrap_or_default();
        p.remove(&name).ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name} on {id}")))
    }

    async fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        if event_id != "clicked" {
            return;
        }
        match id {
            ID_SETTINGS => open_settings(&self.config),
            ID_SNOW => {
                toggle_flag(&self.config, WINDOW_SNOW);
            }
            ID_SOUND => {
                toggle_flag(&self.config, SOUND);
            }
            ID_PAUSE => {
                toggle_pause();
            }
            ID_QUIT => quit_wallpaper(),
            _ => return,
        }
        if self.refresh() {
            let _ = Self::layout_updated(&em, self.revision(), 0).await;
        }
    }

    async fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> Vec<i32> {
        for (id, ev, data, ts) in events {
            self.event(id, ev, data, ts, em.clone()).await;
        }
        vec![]
    }

    /// Called right before the menu opens: report whether the layout is stale.
    async fn about_to_show(&self, _id: i32, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> bool {
        let stale = self.refresh();
        if stale {
            let _ = Self::layout_updated(&em, self.revision(), 0).await;
        }
        stale
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        if self.refresh() {
            (vec![0], vec![])
        } else {
            (vec![], vec![])
        }
    }

    #[zbus(signal)]
    async fn layout_updated(em: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }
}

// ---------------------------------------------------------- lifecycle

fn register(conn: &zbus::blocking::Connection, name: &str) -> zbus::Result<()> {
    conn.call_method(Some(WATCHER), "/StatusNotifierWatcher", Some(WATCHER), "RegisterStatusNotifierItem", &(name,)).map(|_| ())
}

fn run(config: PathBuf) -> zbus::Result<()> {
    let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let menu = Menu { config: config.clone(), rev: Mutex::new((1, vec![])) };
    menu.refresh();
    let conn = zbus::blocking::connection::Builder::session()?
        .name(name.as_str())?
        .serve_at(ITEM_PATH, Item { config, icon: pixmaps(), token: Mutex::new(None) })?
        .serve_at(MENU_PATH, menu)?
        .build()?;
    let _ = conn.request_name(FIXED_NAME);
    if let Ok(mut c) = CONN.lock() {
        *c = Some((conn.clone(), name.clone()));
    }
    // subscribe before registering so a watcher appearing in between isn't missed
    let dbus = zbus::blocking::fdo::DBusProxy::new(&conn)?;
    let owners = dbus.receive_name_owner_changed_with_args(&[(0, WATCHER)])?;
    // no tray running (yet)? then we register when one shows up
    let _ = register(&conn, &name);
    // plasmashell restarted (or started late): register again
    for sig in owners {
        let Ok(args) = sig.args() else { continue };
        if args.new_owner().is_some() {
            let _ = register(&conn, &name);
        }
    }
    Ok(())
}

/// Click "Quit wallpaper" in the running wallpaper's menu, from outside.
pub fn remote_quit() {
    if let Ok(conn) = zbus::blocking::Connection::session() {
        let body = (ID_QUIT, "clicked", Value::from(0i32), 0u32);
        let _ = conn.call_method(Some(FIXED_NAME), MENU_PATH, Some("com.canonical.dbusmenu"), "Event", &body);
    }
}

pub fn spawn(config: PathBuf) {
    let _ = std::thread::Builder::new().name("tray".into()).spawn(move || {
        if let Err(e) = run(config) {
            eprintln!("xmas-tui: tray icon disabled: {e}");
        }
    });
}

pub fn shutdown() {
    let taken = CONN.lock().ok().and_then(|mut c| c.take());
    if let Some((conn, name)) = taken {
        let _ = conn.release_name(name.as_str());
        let _ = conn.release_name(FIXED_NAME);
    }
}
