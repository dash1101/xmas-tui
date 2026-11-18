//! Config file: ~/.config/xmas-tui/config.toml (all fields optional).

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub fps: f32,
    /// Cell size: smaller cells (0.5) mean more of them — finer detail at
    /// the same size (Settings shows it as resolution, 1 / scale).
    pub scale: f32,
    /// Magnify the view (1 = the whole scene), centred on the villager.
    pub zoom: f32,
    /// 0: the save file's village (a new one the first time).
    pub seed: u64,
    /// The desktop wallpaper rests (the village stands still, the widgets
    /// and online go on) while a game runs or the computer's busy, unless
    /// the desktop itself has the focus.
    pub rest_for_games: bool,
    /// The frame rate eased off (as far as 10 a second) while the
    /// wallpaper costs the computer a lot, and back as it can (see
    /// `App::govern`).
    pub ease_off: bool,
    /// A relay of the player's own choosing (a community's own server:
    /// `wss://…/mqtt` or `https://…/poll`); empty: the village's own.
    pub relay: String,
    pub scene: SceneCfg,
    pub clock: ClockCfg,
    pub widgets: WidgetCfg,
    pub weather: WeatherCfg,
    pub layout: LayoutCfg,
    pub windows: WindowsCfg,
    pub sound: SoundCfg,
    pub updates: UpdatesCfg,
    /// The villager's keys (each can be changed).
    pub keys: KeysCfg,
    /// How the app's own windows and panels look (the settings, menus,
    /// conversations): the same looks as the widgets.
    pub ui: UiCfg,
    /// Shelves: widgets that each show a folder's files and shortcuts.
    pub shelves: Vec<ShelfCfg>,
}

/// The villager's keys: a letter or digit, or "space", "enter", "tab".
/// Walking is always WASD and the arrows.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct KeysCfg {
    pub interact: String,
    pub jump: String,
    pub snowball: String,
    pub invite: String,
    pub pockets: String,
    pub held: String,
    pub centre: String,
    pub rest: String,
    pub todo: String,
    pub online: String,
    pub chat: String,
    pub pause: String,
}

impl Default for KeysCfg {
    fn default() -> Self {
        let k = |s: &str| s.to_string();
        KeysCfg { interact: k("e"), jump: k("space"), snowball: k("r"), invite: k("f"), pockets: k("i"), held: k("q"), centre: k("c"), rest: k("z"), todo: k("t"), online: k("o"), chat: k("/"), pause: k("p") }
    }
}

/// What each key does, in the order the settings list them: (name, what
/// it does).
pub const KEY_ACTIONS: [(&str, &str); 12] = [
    ("interact", "talk, use, pick up, help build"),
    ("jump", "jump (hold: higher), leap up onto things"),
    ("snowball", "scoop a snowball, then throw it"),
    ("invite", "ask someone along / say goodbye"),
    ("pockets", "pockets & Christmas list"),
    ("held", "what's in hand"),
    ("centre", "the view back on the villager"),
    ("rest", "lie down in the snow (Space: a snow angel)"),
    ("todo", "things to do here"),
    ("online", "friends & online"),
    ("chat", "talk (@name: to one person, @group: to a group)"),
    ("pause", "pause the village (and carry on)"),
];

impl KeysCfg {
    pub fn get(&self, i: usize) -> &str {
        [&self.interact, &self.jump, &self.snowball, &self.invite, &self.pockets, &self.held, &self.centre, &self.rest, &self.todo, &self.online, &self.chat, &self.pause][i.min(11)]
    }

    pub fn set(&mut self, i: usize, v: String) {
        let slot = match i {
            0 => &mut self.interact,
            1 => &mut self.jump,
            2 => &mut self.snowball,
            3 => &mut self.invite,
            4 => &mut self.pockets,
            5 => &mut self.held,
            6 => &mut self.centre,
            7 => &mut self.rest,
            8 => &mut self.todo,
            9 => &mut self.online,
            10 => &mut self.chat,
            _ => &mut self.pause,
        };
        *slot = v;
    }

    /// How a key's shown ("Space", "E").
    pub fn label(spec: &str) -> String {
        match spec.to_ascii_lowercase().as_str() {
            "space" | " " => "Space".into(),
            "enter" => "Enter".into(),
            "tab" => "Tab".into(),
            s => s.to_uppercase(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct UiCfg {
    /// "classic", "frosted", "wood" or "gift".
    pub look: String,
    /// The windows, menus, dialogs and text over the village drawn this
    /// much bigger (1: the village's own size), the village itself as it
    /// is. (Where the front-end draws its own letters: the window, a
    /// phone, the web.)
    pub scale: f32,
    /// The widgets' own size (unset: the same as `scale`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widgets: Option<f32>,
    /// The buttons up top (unset: the same as `scale`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<f32>,
    /// Windows pinned over the village (a menu tab, friends & chats, a
    /// friend's chat): kept open there, where they were put.
    pub pins: Vec<PinCfg>,
}

/// A window pinned over the village: what ("tab:3", "chats",
/// "friend:<code>"), where (its top-left, in the windows' cells), and
/// whether it's folded down to its title bar.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PinCfg {
    pub what: String,
    pub x: i32,
    pub y: i32,
    pub min: bool,
}

impl Default for UiCfg {
    fn default() -> Self {
        UiCfg { look: "classic".into(), scale: 1.0, widgets: None, buttons: None, pins: vec![] }
    }
}

impl UiCfg {
    /// The widgets' size (their own, or the windows').
    pub fn widgets_k(&self) -> f32 {
        self.widgets.unwrap_or(self.scale)
    }

    /// The buttons' size (their own, or the windows').
    pub fn buttons_k(&self) -> f32 {
        self.buttons.unwrap_or(self.scale)
    }
}

/// A shelf: a widget showing a folder's files and shortcuts (a tidier
/// desktop). Everything on it is an ordinary file in that folder.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ShelfCfg {
    pub name: String,
    /// The folder it shows. Empty: "Desktop Shelves/<name>" in Documents.
    pub folder: String,
    /// "frosted" (a frosted panel with snow on top), "wood" (wooden
    /// planks) or "gift" (a present with a ribbon and bow).
    pub style: String,
    /// "pixel" (icons as chunky pixel art) or "real" (the icons themselves).
    pub icons: String,
    /// Top-left as fractions of the screen (missing: a default spot).
    pub pos: Option<[f32; 2]>,
    /// Size in cells.
    pub size: [i32; 2],
    /// Rolled up to its title.
    pub rolled: bool,
    /// Items in the order they were arranged in (file names); others
    /// follow, by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

impl Default for ShelfCfg {
    fn default() -> Self {
        ShelfCfg { name: "Shelf".into(), folder: String::new(), style: "frosted".into(), icons: "pixel".into(), pos: None, size: [47, 15], rolled: false, order: vec![] }
    }
}

/// New versions from GitHub (release builds only).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct UpdatesCfg {
    /// Look for a new version now and then.
    pub check: bool,
    /// Install it without asking (otherwise a banner asks first).
    pub auto_install: bool,
}

impl Default for UpdatesCfg {
    fn default() -> Self {
        UpdatesCfg { check: true, auto_install: false }
    }
}

/// Synthesized sound: ambience, effects for scene events, generated music.
/// Volumes are 0..1 slider positions.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SoundCfg {
    pub enabled: bool,
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
    /// "shuffle", or one style: "music-box", "celesta", "piano", "sleigh-ride".
    pub track: String,
    /// A soft airy sparkle when the cursor moves fast.
    pub cursor: bool,
    /// Fade our sound out while another app plays media (a video, music…).
    pub pause_for_media: bool,
    /// Per-effect volumes (relative to `sfx`, 0..1.5).
    pub effects: EffectVolumes,
    /// Per-layer ambience volumes (relative to `ambience`, 0..1.5).
    pub ambience_mix: AmbienceMix,
}

/// How loud each kind of effect is, on top of the overall effects volume.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct EffectVolumes {
    pub presents: f32,
    pub snowman: f32,
    pub stars: f32,
    pub bells: f32,
    pub fireworks: f32,
    pub snow: f32,
    pub sparkles: f32,
    pub switches: f32,
    pub fanfare: f32,
    pub sleigh: f32,
    pub train: f32,
    pub animals: f32,
    pub people: f32,
    pub handling: f32,
    pub bushes: f32,
    pub cursor: f32,
}

impl Default for EffectVolumes {
    fn default() -> Self {
        EffectVolumes {
            presents: 1.0,
            snowman: 1.0,
            stars: 1.0,
            bells: 1.0,
            fireworks: 1.0,
            snow: 1.0,
            sparkles: 1.0,
            switches: 1.0,
            fanfare: 1.0,
            sleigh: 1.0,
            train: 1.0,
            animals: 1.0,
            people: 1.0,
            handling: 1.0,
            bushes: 1.0,
            cursor: 1.0,
        }
    }
}

impl EffectVolumes {
    /// (config key, label) for each slider, in display order.
    pub const ROWS: [(&'static str, &'static str); 16] = [
        ("presents", "Presents"),
        ("snowman", "Snowman"),
        ("animals", "Animals & munching"),
        ("people", "People & petting"),
        ("handling", "Pick up & drop"),
        ("bushes", "Bushes & birds"),
        ("sleigh", "Santa's sleigh bells"),
        ("stars", "Shooting stars"),
        ("bells", "Church bells"),
        ("fireworks", "Fireworks"),
        ("sparkles", "Sparkles & light shows"),
        ("snow", "Snow puffs & crunch"),
        ("switches", "Light switches & lamps"),
        ("train", "Train"),
        ("fanfare", "Easter-egg fanfare"),
        ("cursor", "Cursor sparkle"),
    ];

    pub fn get_mut(&mut self, key: &str) -> Option<&mut f32> {
        Some(match key {
            "presents" => &mut self.presents,
            "snowman" => &mut self.snowman,
            "stars" => &mut self.stars,
            "bells" => &mut self.bells,
            "fireworks" => &mut self.fireworks,
            "snow" => &mut self.snow,
            "sparkles" => &mut self.sparkles,
            "switches" => &mut self.switches,
            "fanfare" => &mut self.fanfare,
            "sleigh" => &mut self.sleigh,
            "train" => &mut self.train,
            "animals" => &mut self.animals,
            "people" => &mut self.people,
            "handling" => &mut self.handling,
            "bushes" => &mut self.bushes,
            "cursor" => &mut self.cursor,
            _ => return None,
        })
    }

    pub fn get(&self, key: &str) -> f32 {
        self.clone().get_mut(key).map(|v| *v).unwrap_or(1.0)
    }
}

/// Ambience layers.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AmbienceMix {
    pub wind: f32,
    pub snowfall: f32,
    pub fire: f32,
    pub chimes: f32,
    pub aurora: f32,
}

impl Default for AmbienceMix {
    fn default() -> Self {
        AmbienceMix { wind: 1.0, snowfall: 1.0, fire: 1.0, chimes: 1.0, aurora: 1.0 }
    }
}

impl Default for SoundCfg {
    fn default() -> Self {
        SoundCfg {
            enabled: true,
            master: 0.5,
            music: 0.6,
            sfx: 0.7,
            ambience: 0.5,
            track: "shuffle".into(),
            cursor: true,
            pause_for_media: true,
            effects: EffectVolumes::default(),
            ambience_mix: AmbienceMix::default(),
        }
    }
}

/// Snow that settles on top of open windows, and lights around them
/// (KDE Plasma Wayland and Windows).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WindowsCfg {
    pub snow: bool,
    pub lights: bool,
    /// Max pile height on a window, in text rows.
    pub pile_rows: f32,
    /// Falling-snow density in front of windows.
    pub density: f32,
    /// Draw the falling flakes in front of windows. Off: piles still grow
    /// (from the sky, unseen) but nothing drifts over the work underneath.
    pub front: bool,
    /// Let snow settle on the taskbar / panels too.
    pub docks: bool,
    /// How fast piles build up (multiplier).
    pub settle: f32,
    /// "multi", "warm", "icy" or "candy".
    pub light_style: String,
    /// How many windows get lights (topmost first).
    pub light_windows: u32,
    /// Bulb brightness multiplier.
    pub light_glow: f32,
    /// Max pile height on the taskbar / panels, in text rows.
    pub dock_rows: f32,
    /// How fast the flakes fall (multiplier).
    pub fall: f32,
    /// How much the wind blows them about (multiplier).
    pub wind: f32,
    /// How easily shaking a window throws its snow off (0: never).
    pub shake: f32,
    /// Sweeping the cursor through a pile brushes it away.
    pub brush: bool,
    /// Piles slowly melt away (0: never; higher: faster).
    pub melt: f32,
}

impl Default for WindowsCfg {
    fn default() -> Self {
        WindowsCfg {
            snow: true,
            lights: true,
            pile_rows: 1.5,
            density: 1.0,
            front: true,
            docks: true,
            settle: 1.0,
            light_style: "multi".into(),
            light_windows: 6,
            light_glow: 1.0,
            dock_rows: 1.5,
            fall: 1.0,
            wind: 1.0,
            shake: 1.0,
            brush: true,
            melt: 0.0,
        }
    }
}

/// Widget positions as fractions (0..1) of the screen, so they survive
/// font-size / resolution changes. Missing = default spot.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct LayoutCfg {
    /// When true, widgets can't be dragged.
    pub locked: bool,
    pub clock: Option<[f32; 2]>,
    pub system: Option<[f32; 2]>,
    pub music: Option<[f32; 2]>,
    pub weather: Option<[f32; 2]>,
    pub map: Option<[f32; 2]>,
    pub visualizer: Option<[f32; 2]>,
    pub friends: Option<[f32; 2]>,
    /// The map widget's size in cells (it can be resized).
    pub map_size: Option<[i32; 2]>,
    /// The other widgets' sizes (cells) once resized: "system", "music",
    /// "weather", "visualizer".
    pub sizes: std::collections::BTreeMap<String, [i32; 2]>,
}

impl LayoutCfg {
    pub fn get(&self, i: usize) -> Option<[f32; 2]> {
        [self.clock, self.system, self.music, self.weather, self.map, self.visualizer, self.friends][i.min(6)]
    }
    pub fn set(&mut self, i: usize, v: Option<[f32; 2]>) {
        match i {
            0 => self.clock = v,
            1 => self.system = v,
            2 => self.music = v,
            3 => self.weather = v,
            4 => self.map = v,
            5 => self.visualizer = v,
            _ => self.friends = v,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SceneCfg {
    pub aurora: bool,
    /// Snow density multiplier (0 disables snow).
    pub snow: f32,
    pub wind: f32,
    /// Minutes between sleigh fly-bys (0 disables).
    pub sleigh_minutes: f32,
    pub glow: f32,
    /// How often random surprises happen (0 disables).
    pub events: f32,
    /// How many animals and people come by (0 = none).
    pub critters: f32,
    /// The most animals and people out at once (the villager and their pet
    /// aren't counted).
    pub most_at_once: u32,
    /// Festive berry bushes the animals snack from.
    pub bushes: bool,
    /// A little villager to walk around with WASD / the arrow keys.
    pub player: bool,
    /// Left alone a while, the villager goes about their own day.
    pub autopilot: bool,
    /// How long they're left alone before autopilot takes over (seconds).
    pub autopilot_after: f32,
    /// Where autopilot keeps to: "anywhere" (the whole village), "home"
    /// (round the villager's house), "tree" (round the big tree) or "here"
    /// (round wherever it took over).
    pub autopilot_area: String,
    /// How big that stretch is, in screens wide.
    pub autopilot_reach: f32,
    /// The most in this villager's own village when it's open (2 to 48).
    pub village_limit: u32,
    /// Strong language in what others write starred out.
    pub clean_words: bool,
    /// Only friends & family (the names set) come by as people; the
    /// animals still do.
    pub only_friends: bool,
    /// How fast the villager walks (multiplier).
    pub walk_speed: f32,
    /// Time of day: "auto" (real sunrise / sunset for the weather city),
    /// or fixed: "night", "dawn", "morning", "noon", "afternoon", "dusk".
    pub time: String,
    /// Which side of the road the cars keep to: "auto" (from the weather
    /// city's country), "right" or "left".
    pub traffic: String,
    /// How long speech bubbles stay up (multiplier; longer lines already
    /// stay longer).
    pub talk_time: f32,
    /// How often people stop to chat and remark on things (0 = quiet).
    pub chattiness: f32,
    /// The villager's own pet.
    pub pet: PetCfg,
    /// More pets, of any kinds, coming along too (up to `MORE_PETS`).
    pub pets: Vec<PetCfg>,
    /// The villager's name (empty: people call them "neighbour").
    pub name: String,
    /// Friends and family: some of the folk who come to the village take
    /// these names (and know the villager).
    pub friends: Vec<FriendCfg>,
    /// The villager's own look (a part left empty: the classic one).
    pub wear: FriendCfg,
    /// A line about the villager, on their profile card (friends see it).
    pub motto: String,
}

/// The most pets beside the first.
pub const MORE_PETS: usize = 5;

impl SceneCfg {
    /// Pet `i`: 0 the first, then the others.
    pub fn pet_at(&self, i: u8) -> Option<&PetCfg> {
        if i == 0 { Some(&self.pet) } else { self.pets.get(i as usize - 1) }
    }

    pub fn pet_at_mut(&mut self, i: u8) -> Option<&mut PetCfg> {
        if i == 0 { Some(&mut self.pet) } else { self.pets.get_mut(i as usize - 1) }
    }


    /// What the village is built from: a change to any of this makes it
    /// anew (the rest applies as it is: names, looks, walking speed,
    /// talk, the time of day, traffic…).
    pub fn build_key(&self) -> SceneCfg {
        let mut c = self.clone();
        c.walk_speed = 0.0;
        c.most_at_once = 0;
        c.traffic.clear();
        c.time.clear();
        c.talk_time = 0.0;
        c.chattiness = 0.0;
        c.pet = Default::default();
        c.pets.clear();
        c.name.clear();
        c.autopilot = false;
        c.only_friends = false;
        c.friends.clear();
        c.wear = Default::default();
        c.motto.clear();
        c
    }
}

/// Someone in the friends & family list: a villager of their own.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct FriendCfg {
    pub name: String,
    /// Comes to the village as a kid.
    pub kid: bool,
    /// Their look: a colour name from FRIEND_COATS, _HATS ("none":
    /// bare-headed), _HAIRS and _SKINS. Left empty, one's picked from their
    /// name.
    pub coat: String,
    pub hat: String,
    pub hair: String,
    pub skin: String,
    pub scarf: bool,
}

pub const FRIEND_COATS: [(&str, u32); 8] = [("red", 0xb3202a), ("green", 0x1f6e3a), ("blue", 0x2a4c8a), ("purple", 0x6a3a8a), ("gold", 0xc8862a), ("pink", 0xc8588a), ("grey", 0x5a5e68), ("white", 0xd8d8e0)];
pub const FRIEND_HATS: [(&str, u32); 7] = [("red", 0xd8283a), ("green", 0x2fae5c), ("blue", 0x4a8dff), ("gold", 0xffc23a), ("pink", 0xff7ae0), ("white", 0xf0f0f4), ("none", 0)];
pub const FRIEND_HAIRS: [(&str, u32); 5] = [("brown", 0x6a4a2a), ("dark", 0x2a1c14), ("blonde", 0xc89a5a), ("red", 0xa8461e), ("grey", 0xb0b0b8)];
pub const FRIEND_SKINS: [(&str, u32); 4] = [("light", 0xf0d0b0), ("fair", 0xe8c39e), ("tan", 0xc89a72), ("deep", 0x8a5a3a)];

impl FriendCfg {
    fn lists(part: u8) -> &'static [(&'static str, u32)] {
        match part {
            0 => &FRIEND_COATS,
            1 => &FRIEND_HATS,
            2 => &FRIEND_HAIRS,
            _ => &FRIEND_SKINS,
        }
    }

    fn value(&self, part: u8) -> &str {
        match part {
            0 => &self.coat,
            1 => &self.hat,
            2 => &self.hair,
            _ => &self.skin,
        }
    }

    /// Which entry of a part's list they wear (0 coat, 1 hat, 2 hair, 3
    /// skin): the one named, or one picked from their name.
    pub fn pick(&self, part: u8) -> usize {
        let list = Self::lists(part);
        list.iter().position(|(n, _)| *n == self.value(part)).unwrap_or_else(|| {
            let h = self.name.to_lowercase().bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
            (h.rotate_left(part as u32 * 7) % list.len() as u32) as usize
        })
    }

    /// A part's name (for the settings).
    pub fn part_name(&self, part: u8) -> &'static str {
        Self::lists(part)[self.pick(part)].0
    }

    /// The next (or previous) colour for a part.
    pub fn step(&mut self, part: u8, dir: i32) {
        let list = Self::lists(part);
        let k = (self.pick(part) as i32 + dir).rem_euclid(list.len() as i32) as usize;
        let v = list[k].0.to_string();
        match part {
            0 => self.coat = v,
            1 => self.hat = v,
            2 => self.hair = v,
            _ => self.skin = v,
        }
    }

    /// The villager's own look as colours: coat, hat, hair, skin, scarf (a
    /// part left empty is the classic look's).
    pub fn wear_colours(&self) -> (u32, Option<u32>, u32, u32, Option<u32>) {
        let find = |list: &[(&str, u32)], v: &str, or: u32| list.iter().find(|(n, _)| *n == v).map(|c| c.1).unwrap_or(or);
        let hat = match self.hat.as_str() {
            "none" => None,
            v => Some(find(&FRIEND_HATS, v, 0x2fae5c)),
        };
        (find(&FRIEND_COATS, &self.coat, 0xe0a030), hat, find(&FRIEND_HAIRS, &self.hair, 0x3a2a1a), find(&FRIEND_SKINS, &self.skin, 0xe8c39e), self.scarf.then_some(0xd8283a))
    }

    /// A part of the villager's own look, by name ("classic" when unset).
    pub fn wear_name(&self, part: u8) -> &str {
        let v = self.value(part);
        if v.is_empty() {
            "classic"
        } else {
            v
        }
    }

    /// The next (or previous) colour for a part of the villager's own look:
    /// through the list, and round by the classic one.
    pub fn wear_step(&mut self, part: u8, dir: i32) {
        let list = Self::lists(part);
        let n = list.len() as i32 + 1;
        let at = list.iter().position(|(nm, _)| *nm == self.value(part)).map(|i| i as i32 + 1).unwrap_or(0);
        let k = (at + dir).rem_euclid(n);
        let v = if k == 0 { String::new() } else { list[(k - 1) as usize].0.to_string() };
        match part {
            0 => self.coat = v,
            1 => self.hat = v,
            2 => self.hair = v,
            _ => self.skin = v,
        }
    }

    /// Their look as colours: coat, hat (None: bare-headed), hair, skin.
    pub fn colours(&self) -> (u32, Option<u32>, u32, u32) {
        let hat = FRIEND_HATS[self.pick(1)];
        (FRIEND_COATS[self.pick(0)].1, (hat.0 != "none").then_some(hat.1), FRIEND_HAIRS[self.pick(2)].1, FRIEND_SKINS[self.pick(3)].1)
    }
}

/// The villager's pet: what it is, its colour and its name (empty: not
/// named yet — it asks).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PetCfg {
    /// "dog", "cat", "bunny", "reindeer", "fox", "bird" or "none".
    pub kind: String,
    pub coat: String,
    pub name: String,
}

impl Default for PetCfg {
    fn default() -> Self {
        PetCfg { kind: "dog".into(), coat: "golden".into(), name: String::new() }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ClockCfg {
    pub enabled: bool,
    pub hour24: bool,
    pub seconds: bool,
    pub countdown: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WidgetCfg {
    pub system: bool,
    pub music: bool,
    pub weather: bool,
    /// A strip map of the village (where the villager is, and where they started).
    pub map: bool,
    /// Bars dancing to whatever's playing (captured from the default output).
    pub visualizer: bool,
    /// Each visualizer bar's frequency (kHz) under it.
    pub visualizer_khz: bool,
    /// Friends & messages (while online): who's about, what's been said,
    /// a click for each friend's menu (ping, invite, write…).
    pub friends: bool,
    /// Mount point shown in the disk row.
    pub disk: String,
    /// Each widget's look: "classic", "frosted", "wood" or "gift".
    pub looks: WidgetLooks,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WidgetLooks {
    pub system: String,
    pub music: String,
    pub weather: String,
    pub map: String,
    pub visualizer: String,
    pub friends: String,
}

impl Default for WidgetLooks {
    fn default() -> Self {
        let c = || "classic".to_string();
        WidgetLooks { system: c(), music: c(), weather: c(), map: c(), visualizer: c(), friends: c() }
    }
}

impl WidgetLooks {
    /// Widget `i`'s look (the clock and the shelves have their own).
    pub fn get(&self, i: usize) -> Option<&str> {
        Some(match i {
            1 => &self.system,
            2 => &self.music,
            3 => &self.weather,
            4 => &self.map,
            5 => &self.visualizer,
            6 => &self.friends,
            _ => return None,
        })
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut String> {
        Some(match i {
            1 => &mut self.system,
            2 => &mut self.music,
            3 => &mut self.weather,
            4 => &mut self.map,
            5 => &mut self.visualizer,
            6 => &mut self.friends,
            _ => return None,
        })
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WeatherCfg {
    /// City name looked up via Open-Meteo geocoding (ignored if lat/lon set).
    pub city: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// "fahrenheit" or "celsius"
    pub units: String,
    pub refresh_minutes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fps: 30.0,
            scale: 1.0,
            zoom: 1.0,
            seed: 0,
            rest_for_games: true,
            ease_off: true,
            relay: String::new(),
            scene: SceneCfg::default(),
            clock: ClockCfg::default(),
            widgets: WidgetCfg::default(),
            weather: WeatherCfg::default(),
            layout: LayoutCfg::default(),
            windows: WindowsCfg::default(),
            sound: SoundCfg::default(),
            updates: UpdatesCfg::default(),
            keys: KeysCfg::default(),
            ui: UiCfg::default(),
            shelves: vec![],
        }
    }
}

impl Default for SceneCfg {
    fn default() -> Self {
        SceneCfg { aurora: true, snow: 1.0, wind: 1.0, sleigh_minutes: 4.0, glow: 1.0, events: 1.0, critters: 1.0, most_at_once: 24, bushes: true, player: true, autopilot: false, autopilot_after: 20.0, autopilot_area: "anywhere".into(), autopilot_reach: 1.0, village_limit: 8, clean_words: true, only_friends: false, walk_speed: 1.0, traffic: "auto".into(), time: "auto".into(), talk_time: 1.0, chattiness: 1.0, pet: PetCfg::default(), pets: vec![], name: String::new(), friends: vec![], wear: FriendCfg { scarf: true, ..Default::default() }, motto: String::new() }
    }
}

impl Default for ClockCfg {
    fn default() -> Self {
        ClockCfg { enabled: true, hour24: false, seconds: false, countdown: true }
    }
}

impl Default for WidgetCfg {
    fn default() -> Self {
        WidgetCfg { system: true, music: true, weather: true, map: true, visualizer: false, visualizer_khz: false, friends: true, disk: "/".into(), looks: WidgetLooks::default() }
    }
}

impl Default for WeatherCfg {
    fn default() -> Self {
        WeatherCfg { city: String::new(), latitude: None, longitude: None, units: "fahrenheit".into(), refresh_minutes: 15 }
    }
}

pub fn default_path() -> std::path::PathBuf {
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata).join("xmas-tui").join("config.toml");
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("xmas-tui").join("config.toml")
}

pub fn resolve(path: Option<&str>) -> std::path::PathBuf {
    path.map(std::path::PathBuf::from).unwrap_or_else(default_path)
}

/// Write the config back (atomically). Comments in a hand-edited file are
/// not preserved; config.example.toml documents every field.
/// f32 settings serialize via f64 as e.g. `0.550000011920929`; round long
/// float literals back to what the user actually picked.
fn tidy_floats(body: &str) -> String {
    body.lines()
        .map(|line| {
            let Some((k, v)) = line.split_once(" = ") else { return line.to_string() };
            let vt = v.trim();
            match vt.parse::<f64>() {
                Ok(x) if vt.contains('.') && vt.len() > 8 && x.is_finite() => {
                    let r = (x * 10_000.0).round() / 10_000.0;
                    let mut s = format!("{r}");
                    if !s.contains('.') && !s.contains('e') {
                        s.push_str(".0");
                    }
                    format!("{k} = {s}")
                }
                _ => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

pub fn save(cfg: &Config, path: &std::path::Path) -> std::io::Result<()> {
    let body = tidy_floats(&toml::to_string_pretty(cfg).map_err(std::io::Error::other)?);
    let text = format!(
        "# xmas-tui config — written by the in-wallpaper settings panel.\n\
         # See config.example.toml in the repo for what each field does.\n\n{body}"
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let res = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path));
    if res.is_err() {
        // don't leave a stray config.toml.tmp behind (e.g. read-only config.toml on Windows)
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

pub fn mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Parse config text and clamp it to sane values.
pub fn parse(text: &str) -> Result<Config, String> {
    let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
    c.sanitize();
    Ok(c)
}

/// Load at startup. A missing file gives the defaults; so does a broken one
/// (logged) — the running app's [`FileSync`] then refuses to save over it.
pub fn load(path: Option<&str>) -> Config {
    let p = resolve(path);
    match std::fs::read_to_string(&p) {
        Ok(s) => match parse(&s) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("xmas-tui: bad config {}: {e}", p.display());
                Config::default()
            }
        },
        Err(_) => Config::default(),
    }
}

/// Replace NaN/inf with `def` and clamp to `lo..=hi`.
/// Light-string colour schemes for the window lights.
pub const LIGHT_STYLES: [&str; 4] = ["multi", "warm", "icy", "candy"];

fn fix(v: &mut f32, lo: f32, hi: f32, def: f32) {
    *v = if v.is_finite() { v.clamp(lo, hi) } else { def };
}

/// A usable frames-per-second value (also used for the `--fps` override).
pub fn clean_fps(fps: f32) -> f32 {
    let mut f = fps;
    fix(&mut f, 1.0, 120.0, 30.0);
    f
}

impl Config {
    /// Settings back to how they start, keeping what's the villager's own:
    /// their village (seed), name, look, motto, pet and friends, the
    /// shelves' folders, and where the weather's for.
    pub fn reset_keeping_own(&self) -> Config {
        let mut c = Config::default();
        c.seed = self.seed;
        c.scene.name = self.scene.name.clone();
        c.scene.wear = self.scene.wear.clone();
        c.scene.motto = self.scene.motto.clone();
        c.scene.pet = self.scene.pet.clone();
        c.scene.pets = self.scene.pets.clone();
        c.scene.friends = self.scene.friends.clone();
        c.shelves = self.shelves.clone();
        c.weather.city = self.weather.city.clone();
        c.weather.latitude = self.weather.latitude;
        c.weather.longitude = self.weather.longitude;
        c.weather.units = self.weather.units.clone();
        c
    }

    /// Clamp every number to what the app can handle (the settings panel's
    /// ranges, a bit wider where hand-editing makes sense). TOML happily
    /// accepts `fps = nan` or `snow = 1e9`; those would panic in
    /// `Duration::from_secs_f32`, exhaust memory allocating snowflakes, or
    /// fire an event every frame.
    pub fn sanitize(&mut self) {
        let d = Config::default();
        self.fps = clean_fps(self.fps);
        fix(&mut self.scale, 0.5, 2.0, d.scale);
        fix(&mut self.zoom, 1.0, 4.0, d.zoom);
        let (s, ds) = (&mut self.scene, &d.scene);
        // more pets: real kinds only, and not too many
        s.pets.retain(|p| crate::scene::pets::PetKind::from_str(&p.kind).is_some());
        s.pets.truncate(MORE_PETS);
        fix(&mut s.snow, 0.0, 3.0, ds.snow);
        fix(&mut s.wind, 0.0, 3.0, ds.wind);
        fix(&mut s.glow, 0.0, 2.0, ds.glow);
        fix(&mut s.events, 0.0, 3.0, ds.events);
        fix(&mut s.walk_speed, 0.25, 6.0, ds.walk_speed);
        if !crate::scene::daylight::MODES.contains(&s.time.as_str()) {
            s.time = ds.time.clone();
        }
        if !["auto", "right", "left"].contains(&s.traffic.as_str()) {
            s.traffic = ds.traffic.clone();
        }
        fix(&mut s.sleigh_minutes, 0.0, 60.0, ds.sleigh_minutes);
        fix(&mut s.critters, 0.0, 3.0, ds.critters);
        let (w, dw) = (&mut self.windows, &d.windows);
        fix(&mut w.pile_rows, 0.0, 4.0, dw.pile_rows);
        fix(&mut w.density, 0.0, 3.0, dw.density);
        fix(&mut w.settle, 0.0, 4.0, dw.settle);
        fix(&mut w.dock_rows, 0.0, 4.0, dw.dock_rows);
        fix(&mut w.fall, 0.25, 3.0, dw.fall);
        fix(&mut w.wind, 0.0, 3.0, dw.wind);
        fix(&mut w.shake, 0.0, 3.0, dw.shake);
        fix(&mut w.melt, 0.0, 3.0, dw.melt);
        fix(&mut w.light_glow, 0.0, 2.0, dw.light_glow);
        w.light_windows = w.light_windows.clamp(1, 20);
        if !LIGHT_STYLES.contains(&w.light_style.as_str()) {
            w.light_style = dw.light_style.clone();
        }
        let (a, da) = (&mut self.sound, &d.sound);
        fix(&mut a.master, 0.0, 1.0, da.master);
        fix(&mut a.music, 0.0, 1.0, da.music);
        fix(&mut a.sfx, 0.0, 1.0, da.sfx);
        fix(&mut a.ambience, 0.0, 1.0, da.ambience);
        for (key, _) in EffectVolumes::ROWS {
            if let Some(v) = a.effects.get_mut(key) {
                fix(v, 0.0, 1.5, 1.0);
            }
        }
        let m = &mut a.ambience_mix;
        for v in [&mut m.wind, &mut m.snowfall, &mut m.fire, &mut m.chimes, &mut m.aurora] {
            fix(v, 0.0, 1.5, 1.0);
        }
        let wx = &mut self.weather;
        // the weather thread sleeps `minutes * 12` five-second steps
        wx.refresh_minutes = wx.refresh_minutes.clamp(5, 24 * 60);
        let coord = |v: Option<f64>, lim: f64| v.filter(|v| v.is_finite()).map(|v| v.clamp(-lim, lim));
        wx.latitude = coord(wx.latitude, 90.0);
        wx.longitude = coord(wx.longitude, 180.0);
        self.layout.map_size = self.layout.map_size.map(|[w, h]| [w.clamp(24, 600), h.clamp(3, 20)]);
        for i in 0..5 {
            let p = self.layout.get(i).filter(|p| p.iter().all(|v| v.is_finite())).map(|p| p.map(|v| v.clamp(0.0, 1.0)));
            self.layout.set(i, p);
        }
    }
}

fn to_value(c: &Config) -> toml::Value {
    toml::Value::try_from(c).unwrap_or_else(|_| toml::Value::Table(Default::default()))
}

/// Same settings (compared as TOML, so f32/Option/String fields all count).
pub fn same(a: &Config, b: &Config) -> bool {
    to_value(a) == to_value(b)
}

/// Three-way merge of TOML values: keys the local side changed (vs `base`)
/// win, everything else comes from `file`.
fn merge_value(base: &toml::Value, local: &toml::Value, file: &toml::Value) -> toml::Value {
    use toml::Value::Table;
    match (base, local, file) {
        (Table(b), Table(l), Table(f)) => {
            let mut out = f.clone();
            for (k, lv) in l {
                match (b.get(k), f.get(k)) {
                    (Some(bv), Some(fv)) => {
                        out.insert(k.clone(), merge_value(bv, lv, fv));
                    }
                    _ if b.get(k) != Some(lv) => {
                        out.insert(k.clone(), lv.clone());
                    }
                    _ => {}
                }
            }
            // an Option the local side cleared (e.g. "reset positions")
            for (k, bv) in b {
                if !l.contains_key(k) && f.get(k) == Some(bv) {
                    out.remove(k);
                }
            }
            Table(out)
        }
        _ if local != base => local.clone(),
        _ => file.clone(),
    }
}

/// Keeps an in-memory config and its file in step, for both the wallpaper
/// (`App`) and the settings window: picks up outside edits (by hand, the
/// tray, the other process), saves our own changes, and above all never
/// writes over a file it couldn't parse — a typo, or a file caught half
/// written by an editor, must not be replaced with defaults.
pub struct FileSync {
    path: std::path::PathBuf,
    /// What the file is known to hold (last good load or our last save):
    /// local edits are detected against it, and it's the merge base when
    /// the file changes under us.
    disk: Config,
    mtime: Option<std::time::SystemTime>,
    /// Some(message) while the file can't be read/parsed: the last good
    /// config stays in use, nothing is saved, and the file is re-checked
    /// every interval until it's fixed.
    pub error: Option<String>,
    /// The file was empty (likely mid-write): hold saves, re-check soon.
    empty_checks: u32,
    interval: std::time::Duration,
    last_check: std::time::Instant,
    /// Saving failed (read-only file, locked by another program…): retry
    /// on later checks; logged once.
    save_failed: bool,
    next_save: std::time::Instant,
    /// No reloads, no saves (headless renders).
    pub frozen: bool,
}

impl FileSync {
    /// `cfg` is what was loaded at startup (defaults if the file is broken).
    pub fn new(path: &std::path::Path, cfg: &Config, interval: std::time::Duration) -> Self {
        let error = match std::fs::read_to_string(path) {
            Ok(s) => parse(&s).err(), // already logged by `load`
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => Some(e.to_string()),
        };
        let now = std::time::Instant::now();
        FileSync {
            path: path.to_path_buf(),
            disk: cfg.clone(),
            mtime: mtime(path),
            error,
            empty_checks: 0,
            interval,
            last_check: now,
            save_failed: false,
            next_save: now,
            frozen: false,
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Call every frame. Reloads outside edits into `cfg` (keeping local
    /// changes that haven't been saved yet) and saves local changes when
    /// `can_save`. Returns true if the file was reloaded.
    pub fn poll(&mut self, cfg: &mut Config, can_save: bool) -> bool {
        if self.frozen {
            return false;
        }
        let mut reloaded = false;
        if self.last_check.elapsed() >= self.interval {
            self.last_check = std::time::Instant::now();
            reloaded = self.check(cfg);
        }
        let held = self.error.is_some() || self.empty_checks > 0;
        if can_save && !held && std::time::Instant::now() >= self.next_save && !same(cfg, &self.disk) {
            match save(cfg, &self.path) {
                Ok(()) => {
                    self.disk = cfg.clone();
                    self.mtime = mtime(&self.path);
                    self.save_failed = false;
                }
                Err(e) => {
                    if !self.save_failed {
                        eprintln!("xmas-tui: can't save {}: {e} (will keep retrying)", self.path.display());
                    }
                    self.save_failed = true;
                    self.next_save = std::time::Instant::now() + self.interval.max(std::time::Duration::from_secs(2));
                }
            }
        }
        reloaded
    }

    /// Save immediately if possible (e.g. before opening the file in an
    /// editor). Never writes over a broken file.
    pub fn save_now(&mut self, cfg: &Config) {
        if self.frozen || self.error.is_some() || self.empty_checks > 0 {
            return;
        }
        if save(cfg, &self.path).is_ok() {
            self.disk = cfg.clone();
            self.mtime = mtime(&self.path);
        }
    }

    fn check(&mut self, cfg: &mut Config) -> bool {
        let m = mtime(&self.path);
        let retry = self.error.is_some() || self.empty_checks > 0;
        if m.is_none() || (m == self.mtime && !retry) {
            return false; // unchanged, or gone (we'll recreate it on the next save)
        }
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
            Err(e) => {
                self.fail(e.to_string());
                return false;
            }
        };
        // editors that truncate and then write (nano, VS Code, vim with
        // backupcopy) can be caught in between: give an empty file a couple
        // of checks before believing it means "all defaults"
        if text.trim().is_empty() && self.empty_checks < 2 {
            self.empty_checks += 1;
            return false;
        }
        self.empty_checks = 0;
        match parse(&text) {
            Ok(new) => {
                if self.error.take().is_some() {
                    eprintln!("xmas-tui: config {} is readable again", self.path.display());
                }
                self.mtime = m;
                if same(&new, &self.disk) {
                    return false; // our own save, or a no-op touch
                }
                let merged = merge_value(&to_value(&self.disk), &to_value(cfg), &to_value(&new));
                let mut merged: Config = merged.try_into().unwrap_or_else(|_| new.clone());
                merged.sanitize();
                *cfg = merged;
                self.disk = new;
                true
            }
            Err(e) => {
                self.fail(e);
                false
            }
        }
    }

    fn fail(&mut self, e: String) {
        if self.error.as_deref() != Some(e.as_str()) {
            eprintln!("xmas-tui: can't use config {} (keeping the last good settings, not saving until it's fixed): {e}", self.path.display());
        }
        self.error = Some(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn sanitize_rejects_nan_and_huge_values() {
        let c = parse("fps = nan\n[scene]\nsnow = 1e9\nevents = inf\n[windows]\npile_rows = 99.0\n[layout]\nclock = [nan, 2.0]\n").unwrap();
        assert_eq!(c.fps, 30.0);
        assert_eq!(c.scene.snow, 3.0);
        assert_eq!(c.scene.events, 1.0);
        assert_eq!(c.windows.pile_rows, 4.0);
        assert_eq!(c.layout.clock, None);
    }

    #[test]
    fn broken_file_is_never_overwritten_and_edits_merge() {
        let dir = std::env::temp_dir().join(format!("xmas-cfg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "fps = 20.0\n[scene]\nglow = 1.7\n").unwrap();
        let mut cfg = load(Some(&path.to_string_lossy()));
        let mut sync = FileSync::new(&path, &cfg, Duration::ZERO);
        // a typo: the file stays as written, the last good config stays in use
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "fps = 20.0\n[scene]\nglow = 1.7 oops\n").unwrap();
        sync.poll(&mut cfg, true);
        assert!(sync.error.is_some());
        cfg.scene.snow = 2.0; // a panel change while it's broken: not saved
        sync.poll(&mut cfg, true);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fps = 20.0\n[scene]\nglow = 1.7 oops\n");
        assert_eq!((cfg.fps, cfg.scene.glow), (20.0, 1.7));
        // fixed by hand: the file wins, the unsaved panel change is kept on top, then saved
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "fps = 24.0\n[scene]\nglow = 1.5\n").unwrap();
        assert!(sync.poll(&mut cfg, true));
        assert!(sync.error.is_none());
        assert_eq!((cfg.fps, cfg.scene.glow, cfg.scene.snow), (24.0, 1.5, 2.0));
        let saved = load(Some(&path.to_string_lossy()));
        assert_eq!((saved.fps, saved.scene.glow, saved.scene.snow), (24.0, 1.5, 2.0));
        // an empty file (editor mid-write) holds saves for a couple of checks
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "").unwrap();
        cfg.scene.wind = 2.5;
        sync.poll(&mut cfg, true);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
