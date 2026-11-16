//! The village on the app's game screen: the scene stepped and drawn into
//! a pixel buffer the size of the screen, and touch and buttons turned into
//! what the keyboard and mouse do on the desktop.

use crate::audio::{self, Audio};
use crate::config::Config;
use crate::frame::{Cell, Frame, Placed};
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
}

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
    pub fn new_saved(pw: usize, ph: usize, size: f32, cfg: Config, saved: Option<crate::save::Save>, save: Option<std::path::PathBuf>) -> Game {
        crate::frame::set_pictures_ok(true);
        let (cw, ch, cols, rows) = grid(pw, ph, size);
        // the same village each time (and the same in the game and on the
        // wallpaper): the save's
        let seed = if cfg.seed != 0 { cfg.seed as u32 } else { saved.as_ref().map(|s| s.seed).unwrap_or_else(crate::app::random_seed) };
        let mut scene = Scene::new(cols, rows, seed, &cfg.scene);
        if let Some(s) = saved.filter(|s| s.seed == seed) {
            scene.set_pockets(s.pockets);
            scene.place_player(s.player);
        }
        scene.set_autopilot(cfg.scene.autopilot);
        scene.set_live(cfg.scene.walk_speed, false, cfg.scene.most_at_once);
        scene.set_pet(&cfg.scene.pet);
        scene.set_name(&cfg.scene.name);
        scene.set_friends(&cfg.scene.friends);
        let mut frame = Frame::new(cols, rows);
        frame.glow_near = cfg.scene.glow;
        frame.glow_far = cfg.scene.glow;
        frame.cell_aspect = ch as f32 / cw as f32;
        let audio = if cfg.sound.enabled { Audio::start(&cfg.sound) } else { None };
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
            press: None,
            carrying: false,
            moved: false,
            choices: vec![],
            save,
            saved_at: 0.0,
            stick_y: 0.0,
            cfg,
        }
    }

    /// Write the save file (every minute, when paused, and at the end).
    pub fn save(&mut self) {
        self.saved_at = self.t;
        let Some(p) = self.save.clone() else { return };
        let _ = crate::save::write(&p, &self.saved());
    }

    /// The save as text (the web page keeps it in the browser).
    pub fn save_text(&self) -> String {
        toml::to_string(&self.saved()).unwrap_or_default()
    }

    fn saved(&self) -> crate::save::Save {
        crate::save::Save {
            version: 1,
            seed: self.scene_seed(),
            name: self.cfg.scene.name.clone(),
            saved: crate::clock::now().format("%Y-%m-%d %H:%M").to_string(),
            player: self.scene.player_at(),
            pockets: self.scene.pockets().clone(),
        }
    }

    fn scene_seed(&self) -> u32 {
        self.scene.seed()
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
        if t - self.saved_at > 60.0 {
            self.save();
        }
        let sounds = self.scene.take_sounds();
        if let Some(a) = &self.audio {
            a.frame(sounds, self.scene.audio_state(t));
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
                self.scene.close_panels();
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
        match action {
            0 => {
                self.moved = false;
                self.carrying = false;
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

    /// Two fingers sliding along: look along the village (pixels).
    pub fn pan(&mut self, dx: f32) {
        self.scene.pan(-dx / self.raster.cw as f32);
    }

    /// A long press at (x, y): what can be done there (its title, then the
    /// choices); `pick` does one.
    pub fn menu(&mut self, x: f32, y: f32) -> Option<(String, Vec<String>)> {
        let c = self.cell(x, y);
        let w = self.world(c);
        let (title, items) = self.scene.menu_at(w.0, w.1);
        if items.is_empty() {
            return None;
        }
        self.choices = items.iter().map(|(_, d)| *d).collect();
        Some((title, items.into_iter().map(|(l, _)| l).collect()))
    }

    /// Things to do here (the ✦ button; T on the desktop).
    pub fn todo(&mut self) -> Option<(String, Vec<String>)> {
        let (title, items) = self.scene.todo_menu();
        if items.is_empty() {
            return None;
        }
        self.choices = items.iter().map(|(_, d)| *d).collect();
        Some((title, items.into_iter().map(|(l, _)| l).collect()))
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
    }
}
