//! The village on the app's game screen: the scene stepped and drawn into
//! a pixel buffer the size of the screen, and touch and buttons turned into
//! what the keyboard and mouse do on the desktop.

use crate::audio::{self, Audio};
use crate::color::{pal, Rgb};
use crate::config::Config;
use crate::frame::{Bg, Cell, Frame, Glyph, Placed};
use crate::raster::Raster;
use crate::scene::{daylight, Do, Scene};

/// The buttons on the game screen (and what they are on the desktop).
pub const BTN_E: i32 = 0; // E: talk, use, pick up
pub const BTN_JUMP: i32 = 1; // Space
pub const BTN_SNOWBALL: i32 = 2; // R
pub const BTN_POCKETS: i32 = 3; // I
pub const BTN_INVITE: i32 = 4; // F
pub const BTN_HELD: i32 = 5; // Q: what's in hand
pub const BTN_BACK: i32 = 6; // Esc: close what's open
pub const BTN_CENTRE: i32 = 7; // C: the view back on the villager
pub const BTN_JUMP_UP: i32 = 8; // Space let go (a held jump stops rising)
pub const BTN_REST: i32 = 9; // Z: lie down in the snow
/// 1..5: an answer, or a pocket item (BTN_NUM + n - 1)
pub const BTN_NUM: i32 = 10;

pub struct Game {
    pub scene: Scene,
    cfg: Config,
    frame: Frame,
    spare: Frame,
    raster: Raster,
    /// The screen (px), the grid on it (cells) and where it starts.
    pw: usize,
    ph: usize,
    cols: usize,
    rows: usize,
    off: (usize, usize),
    /// What's drawn (0x00RRGGBB), last frame's cells and pictures.
    pub fb: Vec<u32>,
    prev: Vec<Cell>,
    pics: Vec<Placed>,
    t: f32,
    zoom_xf: (f32, f32, f32),
    focus: Option<(f32, f32)>,
    audio: Option<Audio>,
    /// No sound thread to be had (the web): the sound's made on demand.
    pull: Option<audio::Pull>,
    pcm: Vec<f32>,
    /// A finger down: where (cell), carrying someone, moved off.
    press: Option<(i32, i32)>,
    carrying: bool,
    moved: bool,
    /// The long-press menu's choices, waiting for a pick.
    choices: Vec<Do>,
    /// The save file (shared by the game screen and the wallpaper), and
    /// when it was last written.
    save: Option<std::path::PathBuf>,
    saved_at: f32,
    /// The joystick's last up/down (a push, not a hold, steps through a
    /// conversation's answers).
    stick_y: f32,
    /// A list to choose from, drawn in the village (as on the desktop).
    menu: Option<Menu>,
    /// A card of text (the controls), drawn in the village.
    card: Option<Card>,
    /// Held by its title bar: the menu or card, and the finger's offset.
    lift: Option<(i32, i32)>,
    /// Things for the app itself to do ("wallpaper", "settings",
    /// "text:<question>" for a line typed, "copy:<text>").
    asks: Vec<String>,
    /// What a line asked for is for (see `give_text`).
    text_for: Option<crate::scene::online::TextFor>,
    /// The villager's own village (the scene may be a friend's).
    own_seed: u32,
    /// Online at all here (not on the wallpaper).
    online_ok: bool,
    #[cfg(not(target_arch = "wasm32"))]
    link: Option<crate::net::tcp::Tcp>,
    /// The controls card shows the keyboard's (a computer's web page).
    help_keys: bool,
    /// Devices linked to share this villager (see `crate::sync`).
    linked: Option<crate::sync::Linked>,
    /// The table's being moved by its title bar.
    table_lift: bool,
}

/// What a choice in a menu does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Scene(Do),
    Button(i32),
    /// Lie down (or get up again).
    Rest,
    /// The controls card.
    Help,
    /// For the app itself (see `asks`).
    Ask(&'static str),
    /// Friends & online.
    Online,
}

/// Rows a choice takes (room for a finger).
const GAP: i32 = 2;

/// A list to choose from in the village: a title bar to move it by, the
/// choice picked out (the joystick steps, E picks), a ✕ to close it.
struct Menu {
    x: i32,
    y: i32,
    title: String,
    items: Vec<(String, Act)>,
    hover: usize,
    /// The choice a finger went down on.
    down: Option<usize>,
}

impl Menu {
    fn new(x: i32, y: i32, title: String, items: Vec<(String, Act)>, cols: i32, rows: i32) -> Menu {
        let mut m = Menu { x, y, title, items, hover: 0, down: None };
        m.move_to(x, y, cols, rows);
        m
    }

    fn size(&self) -> (i32, i32) {
        let w = self.items.iter().map(|i| i.0.chars().count()).chain(std::iter::once(self.title.chars().count() + 4)).max().unwrap_or(10) as i32 + 8;
        (w, self.items.len() as i32 * GAP + 2)
    }

    fn move_to(&mut self, x: i32, y: i32, cols: i32, rows: i32) {
        let (w, h) = self.size();
        self.x = x.clamp(0, (cols - w).max(0));
        self.y = y.clamp(0, (rows - h).max(0));
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.size();
        x >= self.x && x < self.x + w && y >= self.y - 1 && y < self.y + h
    }

    /// The title bar (a row of slack above it too: fingers are big).
    fn on_title(&self, x: i32, y: i32) -> bool {
        let (w, _) = self.size();
        (y == self.y || y == self.y - 1) && x >= self.x && x < self.x + w - 4
    }

    fn on_close(&self, x: i32, y: i32) -> bool {
        let (w, _) = self.size();
        (y == self.y || y == self.y - 1) && x >= self.x + w - 4 && x < self.x + w
    }

    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        let (w, h) = self.size();
        if x < self.x || x >= self.x + w || y <= self.y || y >= self.y + h - 1 {
            return None;
        }
        let i = ((y - self.y - 1) / GAP) as usize;
        (i < self.items.len()).then_some(i)
    }

    fn rect(&self) -> crate::widgets::Rect {
        let (w, h) = self.size();
        crate::widgets::Rect { x: self.x, y: self.y, w, h }
    }

    fn draw(&self, f: &mut Frame, look: &str) {
        let (w, h) = self.size();
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
        frame_box(f, self.x, self.y, w, h, bg);
        f.text(self.x + 2, self.y, &format!(" {} ", self.title), pal::UI_TITLE, bg, 0.0);
        f.text(self.x + w - 4, self.y, " ✕", pal::UI_DIM, bg, 0.0);
        for (i, (label, _)) in self.items.iter().enumerate() {
            let y = self.y + 1 + i as i32 * GAP;
            let hot = self.hover == i;
            let ibg = if hot { Bg::Mix(Rgb::hex(0x34406a), 0.95) } else { bg };
            for yy in y..y + GAP {
                for x in self.x + 1..self.x + w - 1 {
                    f.put_g(x, yy, Glyph::new(' ', pal::UI_TEXT).bg(ibg).lit(0.0));
                }
            }
            if hot {
                f.text(self.x + 2, y, "▸", Rgb::hex(0xffd36b), ibg, 0.0);
            }
            f.text(self.x + 4, y, label, if hot { Rgb::hex(0xffd36b) } else { pal::UI_TEXT }, ibg, 0.0);
        }
        crate::widgets::looks::apply(f, self.rect(), look, bg, 5);
    }
}

/// A card of text (the controls): moved by its title bar, closed by ✕, a
/// tap anywhere else, E or Back.
struct Card {
    x: i32,
    y: i32,
    title: String,
    lines: Vec<String>,
}

impl Card {
    fn size(&self) -> (i32, i32) {
        let w = self.lines.iter().map(|l| l.chars().count()).chain(std::iter::once(self.title.chars().count() + 4)).max().unwrap_or(10) as i32 + 4;
        (w, self.lines.len() as i32 + 3)
    }

    fn move_to(&mut self, x: i32, y: i32, cols: i32, rows: i32) {
        let (w, h) = self.size();
        self.x = x.clamp(0, (cols - w).max(0));
        self.y = y.clamp(0, (rows - h).max(0));
    }

    fn on_title(&self, x: i32, y: i32) -> bool {
        let (w, _) = self.size();
        (y == self.y || y == self.y - 1) && x >= self.x && x < self.x + w - 4
    }

    fn rect(&self) -> crate::widgets::Rect {
        let (w, h) = self.size();
        crate::widgets::Rect { x: self.x, y: self.y, w, h }
    }

    fn draw(&self, f: &mut Frame, look: &str) {
        let (w, h) = self.size();
        let bg = Bg::Mix(pal::UI_PANEL, 0.95);
        frame_box(f, self.x, self.y, w, h, bg);
        f.text(self.x + 2, self.y, &format!(" {} ", self.title), pal::UI_TITLE, bg, 0.0);
        f.text(self.x + w - 4, self.y, " ✕", pal::UI_DIM, bg, 0.0);
        for (i, l) in self.lines.iter().enumerate() {
            // "key: what it does": the key picked out
            let y = self.y + 1 + i as i32;
            match l.split_once(": ") {
                Some((k, rest)) if k.chars().count() < 14 => {
                    let x = f.text(self.x + 2, y, k, Rgb::hex(0xffd36b), bg, 0.0);
                    f.text(x + 1, y, rest, pal::UI_TEXT, bg, 0.0);
                }
                _ => {
                    f.text(self.x + 2, y, l, pal::UI_DIM, bg, 0.0);
                }
            }
        }
        crate::widgets::looks::apply(f, self.rect(), look, bg, 7);
    }
}

/// A panel's border, the panel colour inside.
fn frame_box(f: &mut Frame, x0: i32, y0: i32, w: i32, h: i32, bg: Bg) {
    let border = Rgb::hex(0xffd36b) * 0.8;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let ch = match (x == x0, x == x0 + w - 1, y == y0, y == y0 + h - 1) {
                (true, _, true, _) => '╭',
                (_, true, true, _) => '╮',
                (true, _, _, true) => '╰',
                (_, true, _, true) => '╯',
                (_, _, true, _) | (_, _, _, true) => '─',
                (true, _, _, _) | (_, true, _, _) => '│',
                _ => ' ',
            };
            f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
        }
    }
}

/// The keys, on the card (a computer's web page).
const HELP_KEYS: &[&str] = &[
    "WASD / arrows: walk (Shift: run); choose in menus",
    "Space: jump (hold: higher; in the air, again: the",
    "       Christmas spirit lifts the villager once more)",
    "E / Enter: talk, use, pick up, help build; pick",
    "R: a snowball · F: ask along · Z: lie in the snow",
    "T: things to do · I: pockets & scorecard · Q: in hand",
    "C: back to the villager · Esc: close · 1-9: choose",
    "click: people, animals, the sky, Santa",
    "right-click (or hold): what can be done there",
    "drag: carry people, animals and the pet",
    "the wheel: look along the village",
    "",
    "menus and cards move by their title bars",
];

/// The controls, on the card.
const HELP: &[&str] = &[
    "joystick: walk about (all the way: run); in a menu or a",
    "          conversation, up and down to choose",
    "E: talk, use, pick up, help build; picks in menus",
    "jump: jump (hold: higher; in the air, again: the",
    "      Christmas spirit lifts the villager once more)",
    "❄: scoop a snowball, then throw it",
    "✦: things to do right here",
    "☰: pockets, scorecard, invite along, lie down, settings",
    "◎: the view back on the villager",
    "tap: people, animals and things, as a click",
    "press and hold: what can be done there",
    "drag: carry people, animals and the pet",
    "two fingers: look along the village",
    "controller: stick walks, A jump, X use, Y snowball,",
    "            B back, Start ☰, Select ✦, L1 ◎, R1 lie down",
    "",
    "menus and cards move by their title bars",
];

/// Cells for a screen: rows from the short side (about 18 px a row at
/// "size" 100), the width to match.
fn grid(pw: usize, ph: usize, size: f32) -> (usize, usize, usize, usize) {
    let rows = ((ph.min(pw) as f32 / (18.0 * size.clamp(0.5, 2.0))).round() as usize).clamp(34, 110);
    let ch = (ph / rows).max(6);
    let cw = ((ch as f32 * 0.474).round() as usize).max(3);
    (cw, ch, pw / cw, ph / ch)
}

impl Game {
    pub fn new(pw: usize, ph: usize, size: f32, cfg: Config, save: Option<std::path::PathBuf>) -> Game {
        let saved = save.as_deref().and_then(crate::save::read);
        Game::new_saved(pw, ph, size, cfg, saved, save)
    }

    /// From a save already read (the web page keeps its own); `save`: where
    /// to write it (none: `save_text` hands it back instead).
    pub fn new_saved(pw: usize, ph: usize, size: f32, mut cfg: Config, saved: Option<crate::save::Save>, save: Option<std::path::PathBuf>) -> Game {
        crate::frame::set_pictures_ok(true);
        // the villager's name, the pet's, friends: from the save where unset
        if let Some(you) = saved.as_ref().and_then(|s| s.you.as_ref()) {
            you.fill(&mut cfg);
        }
        let (cw, ch, cols, rows) = grid(pw, ph, size);
        // the same village each time (and the same in the game and on the
        // wallpaper): the save's
        let seed = if cfg.seed != 0 { cfg.seed as u32 } else { saved.as_ref().map(|s| s.seed).unwrap_or_else(crate::app::random_seed) };
        let mut scene = Self::make_scene(cols, rows, seed, &cfg);
        // online: the friend code and friends (the villager's, whichever
        // village the save's for)
        let book = saved.as_ref().and_then(|s| s.online.clone()).unwrap_or_default();
        scene.put_online(crate::net::Online::new(book, &cfg.scene.name));
        let linked = saved.as_ref().and_then(|s| s.sync.clone());
        if let Some(l) = &linked {
            scene.online.sync_on(Some((&l.code, &l.dev)));
            scene.set_devices(Some((l.label.clone(), l.devices(crate::clock::now().timestamp()))));
        }
        if let Some(p) = &save {
            scene.set_villagers(crate::save::villagers(p).into_iter().map(|(n, _)| n).collect());
        }
        if let Some(s) = saved.filter(|s| s.seed == seed) {
            scene.set_pockets(s.pockets);
            scene.place_player(s.player);
        }
        let mut frame = Frame::new(cols, rows);
        frame.glow_near = cfg.scene.glow;
        frame.glow_far = cfg.scene.glow;
        frame.cell_aspect = ch as f32 / cw as f32;
        let audio = if cfg.sound.enabled && !cfg!(target_arch = "wasm32") { Audio::start(&cfg.sound) } else { None };
        let pull = cfg!(target_arch = "wasm32").then(|| audio::Pull::new(&cfg.sound));
        Game {
            scene,
            frame,
            spare: Frame::new(1, 1),
            raster: Raster::new(cw, ch),
            pw,
            ph,
            cols,
            rows,
            off: ((pw - cols * cw) / 2, (ph - rows * ch) / 2),
            fb: vec![0x0003_050f; pw * ph],
            prev: vec![],
            pics: vec![],
            t: 0.0,
            zoom_xf: (1.0, 0.0, 0.0),
            focus: None,
            audio,
            pull,
            pcm: vec![],
            press: None,
            carrying: false,
            moved: false,
            choices: vec![],
            save,
            saved_at: 0.0,
            stick_y: 0.0,
            menu: None,
            card: None,
            lift: None,
            asks: vec![],
            text_for: None,
            own_seed: seed,
            online_ok: true,
            #[cfg(not(target_arch = "wasm32"))]
            link: None,
            help_keys: false,
            linked,
            table_lift: false,
            cfg,
        }
    }

    /// The web page's own villagers (their names), kept in the browser.
    pub fn set_villagers(&mut self, names: Vec<String>) {
        self.scene.set_villagers(names);
    }

    /// Y or N: what someone online asked, answered.
    pub fn answer(&mut self, yes: bool) -> bool {
        self.scene.prompt_key(yes)
    }

    /// The villager as things stand (for the app's settings).
    fn you_json(&self) -> String {
        serde_json::to_string(&crate::save::You::of(&self.cfg)).unwrap_or_default()
    }

    /// Linked devices: this one's side, for the others.
    fn send_sync(&mut self) {
        if !self.online_ok {
            return;
        }
        let Some(l) = &mut self.linked else { return };
        let now = crate::clock::now().timestamp();
        let you = crate::save::You::of(&self.cfg);
        let st = l.state(&you, self.own_seed, self.scene.pockets(), &self.scene.online.book, now);
        self.scene.online.sync_send(now, st);
        let devs = l.devices(now);
        self.scene.set_devices(Some((l.label.clone(), devs)));
    }

    /// What the village asks of the app: the card's line, linking devices,
    /// another villager.
    fn do_front(&mut self) {
        use crate::scene::online::Front;
        for f in self.scene.take_front() {
            match f {
                Front::Motto(m) => {
                    self.cfg.scene.motto = m;
                    let j = self.you_json();
                    self.asks.push(format!("you:{j}"));
                }
                Front::LinkShow => {
                    let l = self.linked.get_or_insert_with(|| crate::sync::Linked::new(&crate::net::seal::new_code(), false));
                    let code = l.pretty_code();
                    let (c, d) = (l.code.clone(), l.dev.clone());
                    self.scene.online.sync_on(Some((&c, &d)));
                    self.asks.push(format!("copy:{code}"));
                    self.scene.note(&format!("⇄ link code {code} (copied) · on the other device: ✉ → my devices → link this device"));
                    self.saved_at = -1e9;
                }
                Front::Link(code) => {
                    let l = crate::sync::Linked::new(&code, true);
                    self.scene.online.sync_on(Some((&l.code, &l.dev)));
                    self.linked = Some(l);
                    self.scene.note("⇄ linking… (the other device needs to be online)");
                    self.saved_at = -1e9;
                }
                Front::Unlink => {
                    self.linked = None;
                    self.scene.online.sync_on(None);
                    self.scene.set_devices(None);
                    self.scene.note("⇄ this device isn't linked any more");
                    self.saved_at = -1e9;
                }
                Front::Unfriended(code) => {
                    if let Some(l) = &mut self.linked {
                        l.unfriended(&code);
                    }
                }
                Front::Villager(i) => match self.save.clone() {
                    Some(p) => {
                        let to = crate::save::villagers(&p).into_iter().nth(i).map(|(_, f)| f);
                        if let Some(to) = to {
                            self.save();
                            if crate::save::switch(&p, Some(&to)).is_ok() {
                                self.forget();
                                self.asks.push("reload".into());
                            }
                        }
                    }
                    None => self.asks.push(format!("villager:{i}")),
                },
                Front::NewVillager(name) => match self.save.clone() {
                    Some(p) => {
                        self.save();
                        if crate::save::switch(&p, None).is_ok() {
                            // the new villager's save: their name, a new village
                            let mut cfg = self.cfg.clone();
                            let fresh = crate::config::SceneCfg::default();
                            cfg.scene.name = name.clone();
                            cfg.scene.pet = fresh.pet;
                            cfg.scene.friends = vec![];
                            cfg.scene.wear = fresh.wear;
                            cfg.scene.motto = String::new();
                            let s = crate::save::Save { version: 1, seed: crate::app::random_seed(), name, you: Some(crate::save::You::of(&cfg)), ..Default::default() };
                            let _ = crate::save::write(&p, &s);
                            self.forget();
                            self.asks.push("reload".into());
                        }
                    }
                    None => self.asks.push(format!("newvillager:{name}")),
                },
            }
        }
    }

    /// Linked devices' sides, taken in.
    fn take_synced(&mut self) {
        let now = crate::clock::now().timestamp();
        for (dev, at, st) in self.scene.take_synced() {
            let Some(l) = &mut self.linked else { continue };
            let you = crate::save::You::of(&self.cfg);
            let a = l.take(&dev, at, &st, now, &you, self.own_seed, self.scene.pockets(), &self.scene.online.book);
            if let Some(p) = a.pockets {
                self.scene.set_pockets(p);
            }
            if let Some(y) = &a.you {
                y.apply(&mut self.cfg);
                self.scene.set_name(&self.cfg.scene.name);
                self.scene.set_pet(&self.cfg.scene.pet);
                self.scene.set_friends(&self.cfg.scene.friends);
                self.scene.set_only_friends(self.cfg.scene.only_friends);
                self.scene.set_wear(&self.cfg.scene.wear, &self.cfg.scene.motto);
                let j = self.you_json();
                self.asks.push(format!("you:{j}"));
                self.scene.note(&format!("⇄ {} from {}", if y.name.is_empty() { "the villager" } else { &y.name }, a.from));
            }
            if let Some(b) = a.book {
                if crate::net::seal::norm(&b.code) != crate::net::seal::norm(&self.scene.online.book.code) {
                    self.scene.online.rekey(b);
                } else {
                    self.scene.online.merge_book(b);
                }
            }
            if let Some(seed) = a.seed {
                self.own_seed = seed;
                self.rebuild(seed);
                self.scene.note(&format!("⇄ the village from {}", a.from));
            }
            self.saved_at = self.saved_at.min(self.t - 55.0);
        }
    }

    /// The village laid out anew from `seed`: the villager's things, the
    /// online side and their card carried over.
    fn rebuild(&mut self, seed: u32) {
        let pockets = self.scene.take_pockets();
        let online = self.scene.take_online();
        let social = self.scene.take_social();
        self.scene = Self::make_scene(self.cols, self.rows, seed, &self.cfg);
        self.scene.set_pockets(pockets);
        self.scene.put_online(online);
        self.scene.put_social(social);
        self.scene.set_own_seed(self.own_seed);
        self.menu = None;
        self.focus = None;
    }

    /// Open a menu in the village at (x, y) (cells).
    fn open(&mut self, x: i32, y: i32, title: String, items: Vec<(String, Act)>) {
        if items.is_empty() {
            return;
        }
        self.card = None;
        self.menu = Some(Menu::new(x, y, title, items, self.cols as i32, self.rows as i32));
    }

    fn scene_items(items: Vec<(String, Do)>) -> Vec<(String, Act)> {
        items.into_iter().map(|(l, d)| (l, Act::Scene(d))).collect()
    }

    /// A choice in the menu: done, or (a "▸") the next list in its place.
    fn choose(&mut self, i: usize) {
        let Some(m) = self.menu.take() else { return };
        let Some(&(_, act)) = m.items.get(i) else { return };
        self.scene.woke();
        match act {
            Act::Scene(Do::More(k)) => {
                let (title, items) = self.scene.more_menu(k);
                self.open(m.x, m.y, title, Self::scene_items(items));
            }
            Act::Scene(d) => self.scene.act(d),
            Act::Button(b) => self.button(b),
            Act::Rest => self.scene.rest(),
            Act::Help => self.help(self.help_keys),
            Act::Ask(what) => self.asks.push(what.to_string()),
            Act::Online => {
                let (title, items) = self.scene.more_menu(crate::scene::online::MORE_ONLINE);
                self.open(m.x, m.y, title, Self::scene_items(items));
            }
        }
    }

    /// The ☰ menu: the other things the keys do, and the app's own
    /// (`phone`: the Android app, which can be the wallpaper).
    pub fn more(&mut self, phone: bool) {
        let mut items = vec![
            ("pockets, scorecard & Christmas list".to_string(), Act::Button(BTN_POCKETS)),
            ("invite along / say goodbye".to_string(), Act::Button(BTN_INVITE)),
            ("what's in hand (berries, cocoa…)".to_string(), Act::Button(BTN_HELD)),
            ("lie down in the snow / get up".to_string(), Act::Rest),
        ];
        if phone {
            items.push(("set as the live wallpaper".to_string(), Act::Ask("wallpaper")));
        }
        items.push(("✉ friends & online".to_string(), Act::Online));
        items.push(("settings".to_string(), Act::Ask("settings")));
        items.push(("the controls".to_string(), Act::Help));
        let w = items.iter().map(|i| i.0.chars().count()).max().unwrap_or(10) as i32 + 8;
        self.open((self.cols as i32 - w) / 2, self.rows as i32 / 6, "☰ more".into(), items);
    }

    /// Friends & online (O on a keyboard).
    pub fn online_menu(&mut self) {
        let (title, items) = self.scene.more_menu(crate::scene::online::MORE_ONLINE);
        let w = items.iter().map(|i| i.0.chars().count()).max().unwrap_or(10) as i32 + 8;
        self.open((self.cols as i32 - w) / 2, self.rows as i32 / 8, title, Self::scene_items(items));
    }

    /// The controls card (`keys`: the keyboard's; else the touch controls').
    pub fn help(&mut self, keys: bool) {
        self.menu = None;
        self.help_keys = keys;
        let lines = if keys { HELP_KEYS } else { HELP };
        let mut c = Card { x: 0, y: 0, title: "⌨ playing in the village".into(), lines: lines.iter().map(|s| s.to_string()).collect() };
        let (w, h) = c.size();
        c.move_to((self.cols as i32 - w) / 2, (self.rows as i32 - h) / 3, self.cols as i32, self.rows as i32);
        self.card = Some(c);
    }

    /// The pointer over the village (not pressed): over a menu, the choice
    /// under it is picked out.
    pub fn hover(&mut self, x: f32, y: f32) {
        let c = self.cell(x, y);
        if let Some(m) = &mut self.menu {
            if let Some(i) = m.row_at(c.0, c.1) {
                m.hover = i;
            }
        }
    }

    /// Whether a menu or card is open.
    pub fn menu_open(&self) -> bool {
        self.menu.is_some() || self.card.is_some()
    }

    /// Something for the app itself to do, if anything's asked.
    pub fn take_ask(&mut self) -> Option<String> {
        (!self.asks.is_empty()).then(|| self.asks.remove(0))
    }

    /// The line typed for what was asked (empty: never mind).
    pub fn give_text(&mut self, text: &str) {
        if let Some(what) = self.text_for.take() {
            self.scene.woke();
            self.scene.give_text(what, text);
        }
    }

    /// Not online here (the wallpaper: the game's the one that is).
    pub fn set_online_ok(&mut self, ok: bool) {
        // (the book's left as it is: it's the game's, shared in the save)
        self.online_ok = ok;
    }

    /// Online: the link, what's asked of the app, into a friend's village
    /// (or home again).
    fn update_online(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.online_ok {
            crate::net::tcp::pump(&mut self.scene.online, &mut self.link);
        }
        if std::mem::take(&mut self.scene.online.dirty) {
            self.saved_at = self.saved_at.min(self.t - 55.0);
        }
        if let Some((what, ask)) = self.scene.take_text_ask() {
            self.text_for = Some(what);
            self.asks.push(format!("text:{ask}"));
        }
        if let Some(text) = self.scene.take_copy() {
            self.asks.push(format!("copy:{text}"));
        }
        if let Some(to) = self.scene.take_village() {
            let seed = to.unwrap_or(self.own_seed);
            self.rebuild(seed);
            self.scene.arrive_by_host();
        }
        if self.online_ok {
            self.do_front();
            self.take_synced();
        }
    }

    fn make_scene(cols: usize, rows: usize, seed: u32, cfg: &Config) -> Scene {
        let mut scene = Scene::new(cols, rows, seed, &cfg.scene);
        scene.set_autopilot(cfg.scene.autopilot);
        scene.set_live(cfg.scene.walk_speed, false, cfg.scene.most_at_once);
        scene.set_pet(&cfg.scene.pet);
        scene.set_name(&cfg.scene.name);
        scene.set_friends(&cfg.scene.friends);
        scene.set_only_friends(cfg.scene.only_friends);
        scene.set_ui_look(&cfg.ui.look);
        scene.set_wear(&cfg.scene.wear, &cfg.scene.motto);
        scene.set_post_top(true);
        scene
    }

    /// Back: close the menu, the card, or what's open in the village;
    /// whether anything was.
    pub fn back(&mut self) -> bool {
        if self.menu.take().is_some() || self.card.take().is_some() || self.scene.close_card() {
            return true;
        }
        if self.scene.at_table() {
            self.scene.table_key(None);
            return true;
        }
        self.scene.close_panels()
    }

    /// What's showing: 1 the pockets, 2 a menu or card.
    pub fn state(&self) -> i32 {
        (self.scene.pockets_shown() as i32) | (((self.menu.is_some() || self.card.is_some()) as i32) << 1)
    }

    /// Write the save file (every minute, when paused, and at the end).
    pub fn save(&mut self) {
        self.saved_at = self.t;
        self.send_sync();
        let Some(p) = self.save.clone() else { return };
        let _ = crate::save::write(&p, &self.saved());
    }

    /// No more writing the save file (another's taken its place).
    pub fn forget(&mut self) {
        self.save = None;
    }

    /// The save as text (the web page keeps it in the browser).
    pub fn save_text(&self) -> String {
        toml::to_string(&self.saved()).unwrap_or_default()
    }

    fn saved(&self) -> crate::save::Save {
        crate::save::Save {
            version: 1,
            seed: self.own_seed,
            name: self.cfg.scene.name.clone(),
            saved: crate::clock::now().format("%Y-%m-%d %H:%M").to_string(),
            player: self.scene.player_at(),
            pockets: self.scene.pockets().clone(),
            you: Some(crate::save::You::of(&self.cfg)),
            online: Some(self.scene.online.book.clone()),
            sync: self.linked.clone(),
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.pw, self.ph)
    }

    /// One frame: the village moves on `dt` seconds and is drawn into `fb`.
    pub fn frame(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.t += dt;
        let t = self.t;
        {
            use chrono::Timelike;
            let now = crate::clock::now();
            let mins = now.hour() as f32 * 60.0 + now.minute() as f32 + now.second() as f32 / 60.0;
            self.scene.set_daylight(daylight::compute(&self.cfg.scene.time, mins, 450.0, 1020.0));
        }
        self.scene.update(dt, t);
        // the pet named (or recoloured) in the village: kept, and saved
        if let Some(p) = self.scene.take_pet_change() {
            self.cfg.scene.pet = p;
            self.saved_at = -1e9;
        }
        self.update_online();
        if t - self.saved_at > 60.0 {
            self.save();
        }
        let sounds = self.scene.take_sounds();
        if let Some(a) = &self.audio {
            a.frame(sounds, self.scene.audio_state(t));
        } else if let Some(p) = &mut self.pull {
            p.frame(sounds, self.scene.audio_state(t));
        }
        self.frame.clear();
        let want = self.scene.zoom_focus();
        let f = match self.focus {
            Some((fx, fy)) if (fx - want.0).abs() < self.cols as f32 * 0.5 => {
                let k = 1.0 - (-dt * 5.0).exp();
                (fx + (want.0 - fx) * k, fy + (want.1 - fy) * k)
            }
            _ => want,
        };
        self.focus = Some(f);
        self.zoom_xf = self.scene.render(&mut self.frame, &mut self.spare, t, self.cfg.zoom, Some(f));
        self.scene.draw_top(&mut self.frame);
        let look = self.cfg.ui.look.clone();
        // (a menu or card hides the people and pets under it)
        self.frame.cover_pictures = true;
        if let Some(m) = &self.menu {
            m.draw(&mut self.frame, &look);
        }
        if let Some(c) = &self.card {
            c.draw(&mut self.frame, &look);
        }
        self.frame.compose();
        self.draw();
    }

    /// The cells (only those that changed, and those under pictures) and
    /// the pictures over them, into `fb`.
    fn draw(&mut self) {
        let (cols, rows) = (self.cols, self.rows);
        let cells = &self.frame.out;
        let pics = self.frame.pictures.clone();
        let (cw, ch) = (self.raster.cw, self.raster.ch);
        let stride = self.pw;
        let full = self.prev.len() != cells.len();
        let mut under = vec![false; cols * rows];
        for p in pics.iter().chain(self.pics.iter()) {
            let m = p.sprite.is_some() as i32;
            let (px, py, pc, pr) = (p.x - m, p.y - m, p.cols + 2 * m, p.rows + 2 * m);
            for y in py.max(0) as usize..((py + pr).max(0) as usize).min(rows) {
                for x in px.max(0) as usize..((px + pc).max(0) as usize).min(cols) {
                    under[y * cols + x] = true;
                }
            }
        }
        for cy in 0..rows {
            for cx in 0..cols {
                let i = cy * cols + cx;
                if full || cells[i] != self.prev[i] || under[i] {
                    self.raster.draw_cell(&mut self.fb, stride, self.off.0 + cx * cw, self.off.1 + cy * ch, &cells[i]);
                }
            }
        }
        self.prev = cells.to_vec();
        for p in &pics {
            match p.sprite_px(cw, ch) {
                Some((x, y, k)) => p.pic.blit_scaled(&mut self.fb, stride, self.off.0 as i32 + x, self.off.1 as i32 + y, k),
                None => p.pic.blit(&mut self.fb, stride, self.off.0 + p.x as usize * cw, self.off.1 + p.y as usize * ch, p.cols as usize * cw, p.rows as usize * ch),
            }
        }
        self.pics = pics;
    }

    /// The cell under screen pixel (x, y).
    fn cell(&self, x: f32, y: f32) -> (i32, i32) {
        let cx = ((x - self.off.0 as f32) / self.raster.cw as f32).floor() as i32;
        let cy = ((y - self.off.1 as f32) / self.raster.ch as f32).floor() as i32;
        (cx.clamp(0, self.cols as i32 - 1), cy.clamp(0, self.rows as i32 - 1))
    }

    /// A cell on the screen to one in the (possibly zoomed) village.
    fn world(&self, (x, y): (i32, i32)) -> (i32, i32) {
        let (z, x0, y0) = self.zoom_xf;
        if z <= 1.001 {
            return (x, y);
        }
        ((x0 + (x as f32 + 0.5) / z).floor() as i32, (y0 + (y as f32 + 0.5) / z).floor() as i32)
    }

    /// The joystick: -1..1 each way; pushed all the way, a run.
    pub fn steer(&mut self, mx: f32, my: f32, run: bool) {
        // a menu open: up and down choose (a push at a time)
        if let Some(m) = &mut self.menu {
            if my != 0.0 && my != self.stick_y {
                let n = m.items.len() as i32;
                m.hover = (m.hover as i32 + my.signum() as i32).rem_euclid(n.max(1)) as usize;
            }
            self.stick_y = my;
            self.scene.set_move(0.0, 0.0);
            return;
        }
        // a conversation's answers: the joystick pushed up or down steps
        // through them (the E button picks)
        if self.scene.convo_choosing() || self.scene.at_table() {
            if my != 0.0 && my != self.stick_y {
                self.scene.woke();
                if self.scene.at_table() {
                    self.scene.table_key(Some(if my < 0.0 { crate::scene::games::Key::Up } else { crate::scene::games::Key::Down }));
                } else {
                    self.scene.convo_step(my.signum() as i32);
                }
            }
            if self.scene.at_table() && mx != 0.0 && my == 0.0 && self.stick_y == 0.0 {
                self.scene.table_key(Some(if mx < 0.0 { crate::scene::games::Key::Left } else { crate::scene::games::Key::Right }));
            }
            self.stick_y = if my != 0.0 { my } else if mx != 0.0 { 1e-3 } else { 0.0 };
            self.scene.set_move(0.0, 0.0);
            return;
        }
        self.stick_y = my;
        self.scene.set_move(mx, my);
        self.scene.set_sprint(run);
    }

    pub fn button(&mut self, b: i32) {
        // the villager's buttons: steered by hand again (autopilot stands down)
        if !matches!(b, BTN_CENTRE | BTN_BACK) {
            self.scene.woke();
        }
        // a menu or card open: E picks (or closes the card), Back closes
        if self.menu.is_some() || self.card.is_some() {
            match b {
                // 1-9: that choice
                n if self.menu.is_some() && (BTN_NUM + 1..=BTN_NUM + 9).contains(&n) => self.choose((n - BTN_NUM - 1) as usize),
                BTN_E | BTN_JUMP => {
                    if let Some(i) = self.menu.as_ref().map(|m| m.hover) {
                        self.choose(i);
                    } else {
                        self.card = None;
                    }
                }
                BTN_BACK => {
                    self.back();
                }
                BTN_JUMP_UP => {}
                _ => {
                    self.menu = None;
                    self.card = None;
                    self.button(b);
                }
            }
            return;
        }
        match b {
            // (at the table: E plays; in a conversation, the answer the
            // joystick picked out)
            BTN_E if self.scene.at_table() => self.scene.table_key(Some(crate::scene::games::Key::Pick)),
            BTN_E if self.scene.convo_pick() => {}
            BTN_E => self.scene.interact(),
            BTN_JUMP if self.scene.at_table() => self.scene.table_key(Some(crate::scene::games::Key::Other)),
            BTN_JUMP => self.scene.jump(),
            BTN_JUMP_UP => self.scene.release_jump(),
            BTN_REST => self.scene.rest(),
            BTN_SNOWBALL => self.scene.snowball(),
            BTN_POCKETS => self.scene.toggle_pockets(),
            BTN_INVITE => self.scene.invite(),
            BTN_HELD => self.scene.cycle_held(),
            BTN_BACK if self.scene.at_table() => self.scene.table_key(None),
            BTN_BACK => {
                self.back();
            }
            BTN_CENTRE => self.scene.recenter(),
            n if (BTN_NUM + 1..=BTN_NUM + 5).contains(&n) => self.scene.number((n - BTN_NUM) as usize),
            _ => {}
        }
    }

    /// A finger on the village (not on a button): 0 down, 1 moving, 2 up,
    /// 3 cancelled (a long press turned into a menu, a second finger).
    pub fn touch(&mut self, action: i32, x: f32, y: f32) {
        let c = self.cell(x, y);
        let w = self.world(c);
        let t = self.t;
        // the menu or card takes its own touches
        if self.menu_touch(action, c) {
            return;
        }
        match action {
            0 => {
                self.moved = false;
                self.carrying = false;
                // a question asked by someone online (yes, no), a profile
                // card (put away), the post (friends & online)
                if self.scene.prompt_click(c.0, c.1) || self.scene.close_card() {
                    self.scene.woke();
                    self.press = None;
                    return;
                }
                if self.scene.post_click(c.0, c.1) {
                    self.press = None;
                    self.online_menu();
                    return;
                }
                // at the table: its buttons, the board, the cards (its
                // title bar moves it)
                if self.scene.at_table() {
                    self.press = None;
                    if self.scene.table_grab(c.0, c.1) {
                        self.table_lift = true;
                    } else {
                        self.scene.woke();
                        self.scene.table_click(c.0, c.1);
                    }
                    return;
                }
                // the dialogue box takes its own taps (answers, moving it)
                if self.scene.convo_click(c.0, c.1) {
                    self.scene.woke();
                    self.press = None;
                    return;
                }
                self.press = Some(c);
                self.carrying = self.scene.grab(w.0, w.1);
                if self.carrying {
                    self.scene.woke();
                }
            }
            1 if self.table_lift => self.scene.table_drag_to(c.0, c.1),
            2 | 3 if self.table_lift => {
                self.table_lift = false;
                self.scene.table_drop();
            }
            1 => {
                if self.press.is_some_and(|p| (p.0 - c.0).abs() > 1 || (p.1 - c.1).abs() > 1) {
                    self.moved = true;
                }
                if self.carrying {
                    self.scene.drag(w.0, w.1);
                }
            }
            2 => {
                if self.carrying {
                    self.scene.drag(w.0, w.1);
                    self.scene.release();
                } else if self.press.is_some() && !self.moved {
                    // a tap: the same as a click
                    self.scene.click(w.0, w.1, t);
                }
                self.press = None;
                self.carrying = false;
            }
            _ => {
                if self.carrying {
                    self.scene.let_go();
                }
                self.press = None;
                self.carrying = false;
            }
        }
    }

    /// A touch on the open menu or card (screen cell `c`): whether it took
    /// it. Its title bar moves it, ✕ closes it, a choice is picked when the
    /// finger lifts on it; a tap elsewhere closes it.
    fn menu_touch(&mut self, action: i32, c: (i32, i32)) -> bool {
        let (cols, rows) = (self.cols as i32, self.rows as i32);
        if let Some((dx, dy)) = self.lift {
            if let Some(m) = &mut self.menu {
                m.move_to(c.0 - dx, c.1 - dy, cols, rows);
            } else if let Some(k) = &mut self.card {
                k.move_to(c.0 - dx, c.1 - dy, cols, rows);
            }
            if action >= 2 {
                self.lift = None;
            }
            return true;
        }
        if let Some(m) = &mut self.menu {
            match action {
                0 if m.on_close(c.0, c.1) => self.menu = None,
                0 if m.on_title(c.0, c.1) => self.lift = Some((c.0 - m.x, c.1 - m.y)),
                0 => match m.row_at(c.0, c.1) {
                    Some(i) => {
                        m.hover = i;
                        m.down = Some(i);
                    }
                    None if m.contains(c.0, c.1) => {}
                    None => {
                        // a tap away from it: it closes (and the tap's spent)
                        self.menu = None;
                        self.press = None;
                    }
                },
                1 => {
                    if let (Some(i), true) = (m.row_at(c.0, c.1), m.down.is_some()) {
                        m.hover = i;
                    }
                }
                2 => {
                    let pick = m.down.take().filter(|&d| m.row_at(c.0, c.1) == Some(d));
                    if let Some(i) = pick {
                        self.choose(i);
                    }
                }
                _ => m.down = None,
            }
            return true;
        }
        if let Some(k) = &self.card {
            if action == 0 {
                if k.on_title(c.0, c.1) {
                    self.lift = Some((c.0 - k.x, c.1 - k.y));
                } else {
                    self.card = None;
                }
            }
            return true;
        }
        false
    }

    /// Two fingers sliding along: look along the village (pixels).
    pub fn pan(&mut self, dx: f32) {
        self.scene.pan(-dx / self.raster.cw as f32);
    }

    /// A long press at (x, y): what can be done there (its title, then the
    /// choices); `pick` does one.
    /// (It opens in the village; nothing's handed back.)
    pub fn menu(&mut self, x: f32, y: f32) -> Option<(String, Vec<String>)> {
        let c = self.cell(x, y);
        let w = self.world(c);
        let (title, items) = self.scene.menu_at(w.0, w.1);
        self.press = None;
        self.open(c.0 - 2, c.1 + 1, title, Self::scene_items(items));
        None
    }

    /// Things to do here (the ✦ button; T on the desktop), in the village.
    pub fn todo(&mut self) -> Option<(String, Vec<String>)> {
        let (title, items) = self.scene.todo_menu();
        let w = items.iter().map(|i| i.0.chars().count()).max().unwrap_or(10) as i32 + 8;
        self.open((self.cols as i32 - w) / 2, self.rows as i32 / 6, title, Self::scene_items(items));
        None
    }

    /// One of the choices: done, or (a "▸") a further list to choose from.
    pub fn pick(&mut self, i: usize) -> Option<(String, Vec<String>)> {
        let d = self.choices.get(i).copied();
        self.choices.clear();
        match d {
            Some(Do::More(k)) => {
                let (title, items) = self.scene.more_menu(k);
                self.choices = items.iter().map(|(_, d)| *d).collect();
                Some((title, items.into_iter().map(|(l, _)| l).collect()))
            }
            Some(d) => {
                self.scene.woke();
                self.scene.act(d);
                None
            }
            None => None,
        }
    }

    /// The next `n` frames of sound (48 kHz stereo, interleaved), made now
    /// (the web page's audio asking): where they are.
    pub fn audio_pull(&mut self, n: usize) -> &[f32] {
        match &mut self.pull {
            Some(p) => p.render(n.min(16384), &mut self.pcm),
            None => self.pcm.clear(),
        }
        &self.pcm
    }

    /// The sound's settings changed (the web page's volume, on or off).
    pub fn set_sound(&mut self, cfg: &crate::config::SoundCfg) {
        self.cfg.sound = cfg.clone();
        if let Some(p) = &mut self.pull {
            p.configure(cfg);
        }
    }

    /// Paused (the app's in the background): the sound goes quiet.
    pub fn pause(&mut self, on: bool) {
        if on {
            self.save();
            self.audio = None;
        } else if self.audio.is_none() && self.cfg.sound.enabled {
            self.audio = Audio::start(&self.cfg.sound);
        }
    }
}

/// The app's AudioTrack: as many samples (48 kHz stereo s16) as the sound
/// thread has made, into `out`; silence for the rest.
#[cfg(target_os = "android")]
pub fn audio_take(out: &mut [i16]) -> usize {
    audio::take_android(out)
}

impl Drop for Game {
    fn drop(&mut self) {
        self.save();
        // off the relay politely ("gone", for friends)
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(l) = self.link.take() {
            self.scene.online.goodbye();
            l.close(self.scene.online.take_out());
        }
    }
}
