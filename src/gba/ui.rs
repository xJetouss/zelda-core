//! GBA-Modus in der App: Spiel starten/beenden, Bilder takten, Bild zeigen, Spielstände, Karte im Hauptmenü.
//! Die Menüs dazu stehen in `menus.rs` (Teil der Menüleiste in `shell.rs`).

use super::Gba;
use crate::app::App;
use crate::gfx::{self, Color32, Stroke};
use crate::i18n::tr;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Zustand des GBA-Modus in der App
pub struct GbaView {
    pub game: Option<Gba>,
    tex: Option<gfx::TextureHandle>,
    acc: f64,
    last: Instant,
    /// Fenster „Cheat-Liste“
    pub win_cheats: bool,
    /// Eingabefelder im Cheat-Fenster
    pub cheat_name: String,
    pub cheat_code: String,
    /// Titelbilder für die Karten im Hauptmenü
    cards: HashMap<PathBuf, Option<gfx::TextureHandle>>,
    abuf: Vec<i16>,
    /// Bild nach Farbtabelle (Sättigung, Gamma …), vorheriges Bild (Überblenden), Mischbild
    cur: Vec<Color32>,
    /// Entwicklung: nächstes fertiges Bild hierhin speichern
    dev_dump: Option<String>,
    prev: Vec<Color32>,
    blend: Vec<Color32>,
    /// unscharfes Hintergrundbild für Breitbild (Rand links/rechts)
    back: Option<gfx::TextureHandle>,
    /// Zurückspulen: Spielstände (lz4), Zähler
    hist: VecDeque<Vec<u8>>,
    tick: u32,
    /// neues Bild seit dem letzten Hochladen
    dirty: bool,
    /// Spielanzeige sichtbar (Y-Feld nur dann zeigen und Symbole nur dann aufnehmen)
    pub(crate) hud: bool,
    /// The Minish Cap: Symbole der Items (wie im B-Feld), leeres Y-Feld, Item-Leiste für die Y-Taste
    icons: HashMap<u8, gfx::TextureHandle>,
    y_empty: Option<gfx::TextureHandle>,
    /// dieselben Symbole als RGBA: das Y-Feld wird ins Spielbild gesetzt (gleiche Farben und HD-Filter wie B/A)
    icons_rgba: HashMap<u8, Vec<u8>>,
    /// im Item-Menü aufgenommene Symbole (oben abgeschnitten, nur für die Menü-Anzeige)
    icons_menu: HashMap<u8, Vec<u8>>,
    /// reine Item-Grafiken ohne Tastenkreis für die Item-Leiste (wie im Inventar), beim Öffnen neu berechnet
    icons_bare: HashMap<u8, gfx::TextureHandle>,
    bare_dirty: bool,
    /// Item-Grafiken aus dem Item-Menü (24×24, ohne Tastenkreis) – die schönste Quelle für die Item-Leiste
    icons_inv: HashMap<u8, Vec<u8>>,
    /// reine Item-Grafiken, wie das Spiel sie auf B zeichnet (ohne Tastenkreis) – die beste Quelle für die Leiste
    icons_clean: HashMap<u8, Vec<u8>>,
    inv_tex: HashMap<u8, gfx::TextureHandle>,
    inv_tex_hd: bool,
    /// Kästchen der Item-Leiste als fertige Bilder (Kästchen + Item, durch den Bildfilter des Spiels): normal / gewählt
    tile_tex: HashMap<(u8, bool), gfx::TextureHandle>,
    /// Anzahl Bomben und Pfeile, mit der die Kästchen gebaut sind (ändert sie sich, werden sie neu gebaut)
    tile_counts: (u8, u8),
    tile_filter: Option<crate::filters::Filter>,
    /// Lage der Inventar-Grafik im Ausschnitt des B-Felds (wie das Spiel Items auf B/A zeichnet)
    item_off: Option<(isize, isize)>,
    /// leerer B-Kreis, wie aufgenommen (vor dem Umwandeln in Y) – zum Herauslösen eines Items aus einer B-Aufnahme
    ring_raw: Option<Vec<u8>>,
    inv_grabbed: bool,
    /// Spielbild, ab dem das Item-Menü fertig eingeblendet ist
    inv_at: u64,
    pub bar_open: bool,
    bar_items: Vec<u8>,
    bar_sel: usize,
    bar_prev: u16,
    /// Taste, für die die Item-Leiste offen ist: 0 = Y (Steuerkreuz ←), 1 = B (↓), 2 = A (→)
    bar_slot: u8,
    /// wie die Item-Leiste von A Link to the Past: kurz antippen = Leiste bleibt offen (A wählt, B bricht ab),
    /// Wiederholen beim Halten, weiches Mitlaufen der Leiste
    bar_held_prev: bool,
    bar_sticky: bool,
    bar_moved: bool,
    bar_t0: Instant,
    bar_nav_dir: i8,
    bar_nav_next: Instant,
    bar_scroll: f32,
    /// Spielzeit (Spielbilder) und Tode wie bei A Link to the Past (stehen in den Savestate-Angaben `.meta`)
    pub play_frames: u64,
    pub deaths: u32,
    last_hp: u8,
    /// Y-Taste im letzten Bild gedrückt (Item-Menü: Y legt das Item unter dem Cursor auf Y)
    y_prev: bool,
    /// nächster Versuch, ein Item-Symbol aufzunehmen (Spielbild-Zähler), nach einem Fehlschlag erst später
    capture_at: u64,
    /// Y-Item, das Link im laufenden Spielstand wirklich hat (0 = keins; ein Item aus einem anderen Spielstand zählt nicht)
    y_shown: u8,
    y_check_at: u64,
    y_checked: u8,
    /// B-Feld so, wie es vor dem Drücken von Y aussah (RGBA, Ausschnitt wie die Item-Symbole)
    b_patch: Option<Vec<u8>>,
    /// Entwicklung: Slot-Test (PASTLINK_GBA_SLOTTEST)
    dev_tick: u32,
    /// The Minish Cap (PC-Fassung): eigene Karte
    pub map: super::mapui::MapUi,
    /// Cheat: durch Wände gehen (PC-Fassung)
    pub noclip: bool,
    noclip_set: bool,
}

impl Default for GbaView {
    fn default() -> Self {
        GbaView {
            game: None,
            tex: None,
            acc: 0.0,
            last: Instant::now(),
            win_cheats: false,
            cheat_name: String::new(),
            cheat_code: String::new(),
            cards: HashMap::new(),
            abuf: Vec::with_capacity(4096),
            cur: Vec::new(),
            dev_dump: None,
            prev: Vec::new(),
            blend: Vec::new(),
            back: None,
            hist: VecDeque::new(),
            tick: 0,
            dirty: false,
            hud: false,
            icons: HashMap::new(),
            y_empty: None,
            icons_rgba: HashMap::new(),
            icons_menu: HashMap::new(),
            icons_bare: HashMap::new(),
            bare_dirty: false,
            icons_inv: HashMap::new(),
            icons_clean: HashMap::new(),
            inv_tex: HashMap::new(),
            inv_tex_hd: false,
            tile_tex: HashMap::new(),
            tile_counts: (0, 0),
            tile_filter: None,
            item_off: None,
            ring_raw: None,
            inv_grabbed: false,
            inv_at: u64::MAX,
            bar_open: false,
            bar_items: Vec::new(),
            bar_sel: 0,
            bar_prev: 0,
            bar_slot: 0,
            bar_held_prev: false,
            bar_sticky: false,
            bar_moved: false,
            bar_t0: Instant::now(),
            bar_nav_dir: 0,
            bar_nav_next: Instant::now(),
            bar_scroll: 0.0,
            play_frames: 0,
            deaths: 0,
            last_hp: 0,
            y_prev: false,
            capture_at: 0,
            y_shown: 0,
            y_check_at: 0,
            y_checked: 0,
            b_patch: None,
            dev_tick: 0,
            map: Default::default(),
            noclip: false,
            noclip_set: false,
        }
    }
}

impl GbaView {
    pub fn active(&self) -> bool {
        self.game.is_some()
    }
}

/// Alle GBA-ROMs neben der .exe
pub fn roms_next_to_exe() -> Vec<PathBuf> {
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) else { return Vec::new() };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| super::is_gba(p)).collect()).unwrap_or_default();
    v.sort();
    v
}

impl App {
    /// GBA-Spiel starten (ein laufendes SNES-Spiel wird vorher geschlossen)
    pub(crate) fn gba_open(&mut self, path: PathBuf) {
        if !super::available() {
            let m = self.t("GBA ist in diesem Build nicht enthalten (mGBA ließ sich beim Bauen nicht übersetzen).", "GBA is not included in this build.");
            self.msg(m);
            return;
        }
        self.close_game();
        self.gba.game = None;
        match Gba::load_with(&path, self.s.gba_rc, self.s.save_sram) {
            Ok(mut g) => {
                // Cheats/Y-Item aus den Einstellungen; ältere Fassungen legten sie neben die ROM (einmal übernehmen,
                // danach entfernen – gespeichert wird nur noch über Savestates und die Einstellungen der App)
                let key = gba_key(&path);
                match self.s.gba_games.get(&key) {
                    Some(c) => g.cfg = c.clone(),
                    None => {
                        self.s.gba_games.insert(key, g.cfg.clone());
                    }
                }
                let _ = std::fs::remove_file(super::cfg_path(&path));
                let _ = std::fs::remove_file(path.with_extension("title.rgba"));
                g.save_sav = self.s.save_sram;
                g.apply_cheats();
                self.gba.game = Some(g);
                self.gba.play_frames = 0;
                self.gba.deaths = 0;
                self.gba.last_hp = 0;
                self.metas.clear();
                self.gba.acc = 0.0;
                self.gba.last = Instant::now();
                self.gba.tex = None;
                self.paused = false;
                // Entwicklung: Cheat-Liste gleich öffnen (PASTLINK_OPEN=gbacheats)
                self.gba.win_cheats = std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "gbacheats");
                if std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "states") {
                    self.win_states = true;
                }
                // Entwicklung: Spielstand gleich laden (PASTLINK_GBA_LOAD=Datei)
                if let Some(b) = std::env::var_os("PASTLINK_GBA_LOAD").and_then(|f| std::fs::read(f).ok()).map(|b| lz4_flex::decompress_size_prepended(&b).unwrap_or(b)) {
                    if let Some(g) = self.gba.game.as_mut() {
                        g.load_state(&b);
                    }
                }
                // Entwicklung: Y-Item vorgeben (PASTLINK_GBA_YITEM=Nummer)
                if let Some(y) = std::env::var("PASTLINK_GBA_YITEM").ok().and_then(|v| v.parse::<u8>().ok()) {
                    if let Some(g) = self.gba.game.as_mut() {
                        g.cfg.y_item = y;
                    }
                }
                self.s.last_gba = path.to_string_lossy().to_string();
                let m = format!("{}: {}", self.t("Gestartet", "Started"), super::display_name(&path));
                self.msg(m);
            }
            Err(e) => self.msg(e),
        }
    }

    /// GBA-Spiel beenden (Spielstand des Spiels wird geschrieben)
    pub(crate) fn gba_close(&mut self) {
        if let Some(d) = self.gba.map.data.as_mut() {
            d.save();
        }
        self.gba.map = Default::default();
        self.gba.game = None;
        self.gba.win_cheats = false;
        self.gba.tex = None;
        self.gba.back = None;
        self.gba.hist.clear();
        self.gba.icons.clear();
        self.gba.icons_rgba.clear();
        self.gba.icons_menu.clear();
        self.gba.icons_bare.clear();
        self.gba.icons_inv.clear();
        self.gba.icons_clean.clear();
        self.gba.inv_tex.clear();
        self.gba.y_empty = None;
        self.gba.bar_open = false;
        self.thumbs.clear();
    }

    /// ein Anzeige-Bild im GBA-Modus: Spielbilder nach Zeit (oder zurückspulen), Ton, Bild hochladen.
    /// `blocked`: ein Menü ist offen (Spiel steht)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn gba_frame(&mut self, ctx: &gfx::Context, pad: u16, turbo: bool, double: bool, focused: bool, rewind: bool, blocked: bool) {
        let now = Instant::now();
        let dt = (now - self.gba.last).as_secs_f64().min(0.1);
        self.gba.last = now;
        self.auto_paused = self.s.auto_pause && !focused;
        let blocked = blocked || self.paused || self.auto_paused || self.gba.win_cheats || (self.s.pause_in_menus && self.popup_open());
        let speed = if turbo { self.s.turbo_speed as f64 } else if double { 2.0 * self.s.speed as f64 } else { self.s.speed as f64 };
        // Entwicklung: Tastenfolge abspielen (PASTLINK_GBA_KEYS = „taste:bild,…“, Bilder ab Spielstart gezählt;
        // s=Start, a=A, b=B, y=Y, u/d/l/r, D=Steuerkreuz ↓ am Controller …)
        let pad = pad | self.gba_dev_keys();
        let (pad, blocked) = self.gba_item_bar(ctx, pad, blocked);
        // wie bei A Link to the Past: Anzeige „TURBO“ / „×2“ / „Zurückspulen“ solange gehalten
        self.turbo_on = turbo;
        self.double_on = double && !turbo;
        self.rewind_on = rewind && self.s.rewind;
        let rewinding = self.rewind_on;
        let max_hist = (self.s.rewind_seconds as usize * 30).max(30);
        let save_sram = self.s.save_sram;
        let Some(g) = self.gba.game.as_mut() else { return };
        g.save_sav = save_sram;
        // Cheat „durch Wände gehen“ an Picori weitergeben
        if self.gba.noclip != self.gba.noclip_set && !g.loading() {
            g.set_noclip(self.gba.noclip);
            self.gba.noclip_set = self.gba.noclip;
        }
        // Picori startet noch: weiterladen, auch wenn ein Menü offen ist (sonst bleibt „Recomp startet“ stehen)
        if g.loading() {
            g.run_frame(0);
        }
        // Einstellungen „Spiel“: Weltkarte von Anfang an (mGBA braucht dafür die Cheats neu)
        if g.force_map != self.s.gba_map {
            g.force_map = self.s.gba_map;
            g.apply_cheats();
        }
        // Y-Taste aus: Y kommt im Spiel nicht an
        let pad = if self.s.gba_y { pad } else { pad & !0x4000 };
        // Item-Menü (Start): Y legt das Item unter dem Cursor auf Y – wie A und B im Spiel
        let mut pad = pad;
        let y_now = pad & 0x4000 != 0;
        let y_edge = y_now && !self.gba.y_prev;
        self.gba.y_prev = y_now;
        // Y gedrückt: B-Feld vorher festhalten – solange das Y-Item kurz auf B liegt, zeigt B weiter sein Item
        if y_now && g.y_active().is_none() && self.gba.y_shown != 0 && self.gba.hud {
            self.gba.b_patch = g.b_box().and_then(|(bx, by)| {
                let (w, h) = (g.w, g.h);
                if bx < 0 || bx as usize + super::ICON_W > w || g.frame.len() < w * h * 4 {
                    return None;
                }
                let mut out = vec![0u8; super::ICON_W * super::ICON_H * 4];
                for y in 0..super::ICON_H {
                    let fy = by + y as isize;
                    if fy < 0 || fy as usize >= h {
                        continue;
                    }
                    for x in 0..super::ICON_W {
                        let i = (fy as usize * w + bx as usize + x) * 4;
                        let o = (y * super::ICON_W + x) * 4;
                        out[o..o + 4].copy_from_slice(&[g.frame[i], g.frame[i + 1], g.frame[i + 2], 255]);
                    }
                }
                Some(out)
            });
        }
        if let Some(mi) = g.menu_item() {
            pad &= !0x4000;
            let mi = if mi == 16 { 15 } else { mi };
            if y_edge && super::minish::EQUIPPABLE.contains(&mi) {
                g.cfg.y_item = mi;
                g.save_cfg();
                self.gba.dirty = true;
            }
        }
        if g.cfg_dirty {
            g.cfg_dirty = false;
            let key = gba_key(&g.path);
            self.s.gba_games.insert(key, g.cfg.clone());
        }
        let mut n = 0;
        if blocked {
            self.gba.acc = 0.0;
        } else {
            self.gba.acc += dt * g.fps * if rewinding { 1.0 } else { speed };
            n = (self.gba.acc.floor() as u32).min(16);
            self.gba.acc -= n as f64;
            if n == 16 {
                self.gba.acc = 0.0;
            }
        }
        // Pause: „Einzelbild vor“
        if self.paused && self.advance {
            self.advance = false;
            n = 1;
        }
        if rewinding {
            // je Spielbild einen Stand zurück (gesichert alle 2 Bilder), dann diesen Stand zeigen
            for _ in 0..n {
                if self.gba.hist.len() > 1 {
                    self.gba.hist.pop_back();
                }
            }
            if n > 0 {
                if let Some(raw) = self.gba.hist.back().and_then(|b| lz4_flex::decompress_size_prepended(b).ok()) {
                    g.load_state(&raw);
                    g.run_frame(0);
                }
                let mut skip = Vec::new();
                g.take_audio(&mut skip);
            }
        } else {
            for _ in 0..n {
                g.run_frame(pad);
                self.gba.tick = self.gba.tick.wrapping_add(1);
                // wie bei ALttP: alle 2 Bilder sichern, im Turbo/×2 alle 8
                if self.s.rewind && self.gba.tick % (if turbo || double { 8 } else { 2 }) == 0 {
                    if let Some(st) = g.save_state() {
                        self.gba.hist.push_back(lz4_flex::compress_prepend_size(&st));
                        while self.gba.hist.len() > max_hist {
                            self.gba.hist.pop_front();
                        }
                    }
                }
            }
        }
        if !self.s.rewind {
            self.gba.hist.clear();
        }
        self.gba.hud = g.hud_visible();
        self.game_fps_count += n;
        // Spielzeit und Tode (Herzen fallen auf 0) wie bei A Link to the Past
        if !rewinding && n > 0 {
            self.gba.play_frames += n as u64;
            if g.is_minish() && self.gba.tick % 8 < n {
                if let Some(hp) = g.peek(0x0200_2AEA, 1).map(|b| b[0]) {
                    if hp == 0 && self.gba.last_hp > 0 && self.gba.hud {
                        self.gba.deaths += 1;
                    }
                    self.gba.last_hp = hp;
                }
            }
        }
        // Ton: nur bei normalem Tempo (sonst würde er sich stauen)
        self.gba.abuf.clear();
        g.take_audio(&mut self.gba.abuf);
        let audio_ok = !self.s.mute && !(turbo && self.s.mute_turbo) && (speed - 1.0).abs() < 0.01 && !rewinding;
        if let Some(a) = &self.audio {
            a.set_params(self.s.volume, self.s.latency_ms);
            if audio_ok && !blocked {
                a.push(&self.gba.abuf);
            }
        }
        if g.new_frame {
            g.new_frame = false;
            self.gba.dirty = true;
        }
        if std::env::var_os("PASTLINK_GBA_SLOTTEST").is_some() {
            self.gba_slot_test();
        }
        self.gba_map_update();
        self.gba_upload(ctx);
    }

    /// The Minish Cap: Item-Leiste (Steuerkreuz ← = Y, ↓ = B, → = A halten, mit Stick oder L/R wählen, loslassen =
    /// auf Y legen). Liefert die Tasten fürs Spiel und ob das Spiel steht.
    fn gba_item_bar(&mut self, ctx: &gfx::Context, pad: u16, blocked: bool) -> (u16, bool) {
        // Entwicklung: Item-Leiste offen zeigen (PASTLINK_OPEN=gbabar)
        // Steuerkreuz am Controller halten: ← = Y-Item, ↓ = B-Item, → = A-Item (Schnellmenü-Taste der Tastatur = Y)
        let d = self.ctl_dpad;
        let want = if d & (1 << 10) != 0 {
            Some(1u8)
        } else if self.qm_pad || d & (1 << 8) != 0 {
            Some(2)
        } else if d & (1 << 9) != 0 || self.qm_held || std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "gbabar") {
            Some(0)
        } else {
            None
        };
        // Einstellungen „Spiel“: Item-Leiste aus, Y-Taste aus (dann keine Leiste für Y)
        let want = if !self.s.gba_bar || (want == Some(0) && !self.s.gba_y) { None } else { want };
        let held = want.is_some() && !blocked;
        let dir_x = self.qm_dir.x;
        let Some(g) = self.gba.game.as_mut() else { return (pad, blocked) };
        if !g.is_minish() {
            return (pad, blocked);
        }
        // Symbole aus dem Spiel aufnehmen (einmal je Item; Flaschen jedes Mal neu, ihr Inhalt wechselt)
        let mut new_icons: Vec<(u8, Vec<u8>)> = Vec::new();
        let hud = self.gba.hud;
        // Aufnahmen kosten zwei Spielbilder (vor und zurück): nach einem Fehlschlag erst nach einer Sekunde wieder
        let can_capture = hud && self.gba.play_frames >= self.gba.capture_at;
        if self.gba.y_empty.is_none() && can_capture && g.menu_item().is_none() {
            match g.capture_icon(0) {
                Some(mut img) => {
                    self.gba.ring_raw = Some(img.clone());
                    letter_y(&mut img);
                    new_icons.push((0, img));
                }
                None => self.gba.capture_at = self.gba.play_frames + 60,
            }
        }
        let dev_bar = std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "gbabar");
        if held && (!self.gba.bar_held_prev || dev_bar) && !self.gba.bar_open && hud && g.menu_item().is_none() {
            self.gba.bar_items = g.owned_items();
            for &i in &self.gba.bar_items {
                if !self.gba.icons.contains_key(&i) || (28..=31).contains(&i) {
                    if let Some(img) = g.capture_icon(i) {
                        new_icons.push((i, img));
                    }
                }
            }
            // Item-Grafiken aus dem Item-Menü des Spiels (die saubersten: ohne Tastenkreis, Flaschen mit Inhalt) –
            // fehlt eine oder hat man Flaschen (Inhalt wechselt), das Menü unsichtbar kurz öffnen und aufnehmen
            let need = self.gba.bar_items.iter().any(|i| !matches!(i, 9 | 10 | 22) && (!self.gba.icons_clean.contains_key(i) || (28..=31).contains(i)));
            if g.is_recomp() && need {
                for (i, img) in g.capture_menu_icons() {
                    self.gba.icons_clean.insert(i, img.clone());
                    self.gba.icons_inv.insert(i, img);
                }
                self.gba.inv_tex.clear();
            }
            self.gba.bar_slot = want.unwrap_or(0);
            let cur = match self.gba.bar_slot {
                1 => g.peek(super::minish::EQUIP_B, 1).map_or(0, |b| b[0]),
                2 => g.peek(super::minish::EQUIP_A, 1).map_or(0, |b| b[0]),
                _ => g.cfg.y_item,
            };
            self.gba.bar_sel = self.gba.bar_items.iter().position(|&i| i == cur).unwrap_or(0);
            self.gba.bar_scroll = self.gba.bar_sel as f32;
            self.gba.bar_open = true;
            self.gba.bare_dirty = true;
            self.gba.bar_prev = pad;
            self.gba.bar_sticky = false;
            self.gba.bar_moved = false;
            self.gba.bar_t0 = Instant::now();
            self.gba.bar_nav_dir = 0;
        }
        // Y nur mit einem Item, das Link in diesem Spielstand hat (zweimal pro Sekunde geprüft)
        if self.gba.play_frames >= self.gba.y_check_at || self.gba.y_checked != g.cfg.y_item {
            self.gba.y_check_at = self.gba.play_frames + 30;
            let y = g.cfg.y_item;
            let owned = y != 0 && g.owned_items().contains(&y);
            // im Spiel (Anzeige sichtbar) und Item fehlt: Belegung stammt aus einem anderen Spielstand – löschen,
            // damit sie nicht später (z. B. nach einem Cheat) plötzlich wieder auftaucht
            if y != 0 && !owned && hud {
                g.cfg.y_item = 0;
                g.save_cfg();
            }
            self.gba.y_checked = g.cfg.y_item;
            self.gba.y_shown = if owned { y } else { 0 };
        }
        let y = self.gba.y_shown;
        let in_menu = g.menu_item().is_some();
        if y != 0 && !self.gba.icons.contains_key(&y) && hud && self.gba.play_frames >= self.gba.capture_at {
            if in_menu {
                // im Item-Menü nicht aufnehmen: das Zurückspulen brächte die Menü-Eingabe durcheinander – das Symbol
                // kommt, sobald das Menü zu ist
            } else {
                match g.capture_icon(y) {
                    Some(img) => new_icons.push((y, img)),
                    None => self.gba.capture_at = self.gba.play_frames + 60,
                }
            }
        }

        let mut out = (pad, blocked);
        if self.gba.bar_open {
            let n = self.gba.bar_items.len().max(1);
            self.gba.bar_prev = pad;
            // wählen nur mit dem Stick (rechter Stick, sonst linker); halten wiederholt
            let dx: i8 = if dir_x < -0.5 { -1 } else if dir_x > 0.5 { 1 } else { 0 };
            let now = Instant::now();
            if dx != 0 && (dx != self.gba.bar_nav_dir || now >= self.gba.bar_nav_next) {
                let first = dx != self.gba.bar_nav_dir;
                self.gba.bar_sel = (self.gba.bar_sel as i32 + dx as i32).rem_euclid(n as i32) as usize;
                self.gba.bar_nav_next = now + std::time::Duration::from_millis(if first { 300 } else { 110 });
            }
            self.gba.bar_nav_dir = dx;
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            self.gba.bar_scroll += (self.gba.bar_sel as f32 - self.gba.bar_scroll) * (1.0 - (-dt * 18.0).exp());
            if (self.gba.bar_scroll - self.gba.bar_sel as f32).abs() > n as f32 / 2.0 {
                self.gba.bar_scroll = self.gba.bar_sel as f32;
            }
            // Pfeil losgelassen: Item ausrüsten, Leiste zu
            if !held {
                self.gba.bar_open = false;
                if let Some(&i) = self.gba.bar_items.get(self.gba.bar_sel) {
                    match self.gba.bar_slot {
                        1 => g.equip(false, i),
                        2 => g.equip(true, i),
                        _ => {
                            g.cfg.y_item = i;
                            g.save_cfg();
                        }
                    }
                }
            }
            out = (0, true);
        }
        self.gba.bar_held_prev = held;
        for (i, img) in new_icons {
            let t = ctx.load_texture(format!("gba-icon-{i}"), gfx::ColorImage::from_rgba_unmultiplied([super::ICON_W, super::ICON_H], &img), gfx::TextureOptions::NEAREST);
            self.gba.icons_rgba.insert(i, img);
            if i == 0 {
                self.gba.y_empty = Some(t);
            } else {
                self.gba.icons.insert(i, t);
            }
        }
        // Item-Menü offen: einmal je Öffnen alle Item-Grafiken aufnehmen (Figuren ohne Hintergrund, wie im Inventar)
        let hd = self.s.filter.is_hd();
        if let Some(g) = self.gba.game.as_mut() {
            let menu = g.menu_item();
            if menu.is_some() && self.gba.inv_at == u64::MAX {
                self.gba.inv_at = self.gba.play_frames + 30;
            }
            match menu {
                Some(cursor) if !self.gba.inv_grabbed && self.gba.play_frames >= self.gba.inv_at => {
                    self.gba.inv_grabbed = true;
                    let b_item = g.peek(super::minish::EQUIP_B, 1).map_or(0, |b| b[0]);
                    if let Some((px, w, h)) = g.capture_sprites() {
                        let dx = w.saturating_sub(super::W) / 2;
                        let mut ids = g.owned_items();
                        ids.extend([8u8, 16]);
                        let _ = b_item;
                        for item in ids {
                            let Some((cx, cy)) = g.menu_pos(item) else { continue };
                            let cx = cx + dx;
                            // Bomben, Fernzünder-Bomben, Bogen: im Item-Menü steht die Anzahl als eigene Figur direkt
                            // daneben – diese Grafiken kommen sauberer aus dem B-Feld (siehe unten)
                            if super::minish::COUNTED.contains(&item) {
                                continue;
                            }
                            let out = super::crop_inv_icon(&px, w, h, cx, cy, cursor == item);
                            if out.chunks_exact(4).filter(|p| p[3] != 0).count() > 12 {
                                // Laterne an/aus sieht gleich aus; Bomben und Fernzünder-Bomben nicht
                                let key = if item == 16 { 15 } else { item };
                                if !self.gba.icons_clean.contains_key(&key) {
                                    self.gba.icons_inv.insert(key, out.clone());
                                }
                                if !self.gba.icons_clean.contains_key(&item) {
                                    self.gba.icons_inv.insert(item, out);
                                }
                            }
                        }
                        self.gba.inv_tex.clear();
                        if std::env::var_os("PASTLINK_ICON_LOG").is_some() {
                            let mut keys: Vec<u8> = self.gba.icons_inv.keys().copied().collect();
                            keys.sort();
                            let mut px = Vec::new();
                            for y in 0..super::INV_ICON {
                                for k in &keys {
                                    let im = &self.gba.icons_inv[k];
                                    for x in 0..super::INV_ICON {
                                        let o = (y * super::INV_ICON + x) * 4;
                                        px.push(if im[o + 3] == 0 { Color32::from_rgb(255, 0, 255) } else { Color32::from_rgb(im[o], im[o + 1], im[o + 2]) });
                                    }
                                }
                            }
                            let raw: Vec<u8> = px.iter().flat_map(|c| [c.r(), c.g(), c.b()]).collect();
                            let _ = std::fs::write("inv_icons.rgb", raw);
                            eprintln!("Inventar-Grafiken: {keys:?}");
                        }
                    }
                }
                Some(_) => {}
                None => {
                    self.gba.inv_grabbed = false;
                    self.gba.inv_at = u64::MAX;
                }
            }
        }
        // Kästchen der Item-Leiste: als Pixelbild wie im Item-Menü (graues Kästchen, gewählt hellblau mit weißer Fläche)
        // mit dem Item darauf – dann durch denselben Bildfilter wie das Spielbild (HD weich, Original scharf)
        let filter = self.s.filter;
        // Anzahl Bomben/Pfeile (Spielstand ab 0x02002AE8: +4 Bomben, +5 Pfeile)
        let counts = self.gba.game.as_ref().and_then(|g| g.peek(0x0200_2AEC, 2)).map_or((0, 0), |b| (b[0], b[1]));
        if counts != self.gba.tile_counts {
            self.gba.tile_counts = counts;
            self.gba.tile_filter = None;
        }
        if self.gba.tile_filter != Some(filter) || (self.gba.inv_tex.is_empty() && !self.gba.icons_inv.is_empty()) {
            self.gba.tile_filter = Some(filter);
            self.gba.tile_tex.clear();
            self.gba.inv_tex.clear();
            self.gba.inv_tex_hd = hd;
            // Entwicklung: fertige Kästchen als Bild speichern (PASTLINK_DUMP_BAR = Datei)
            if let Ok(f) = std::env::var("PASTLINK_DUMP_BAR") {
                let mut ids: Vec<u8> = self.gba.icons_inv.keys().copied().collect();
                ids.sort();
                let (tw, th) = (super::TILE_W, super::TILE_H);
                let mut px = vec![Color32::from_rgb(40, 40, 40); tw * ids.len().max(1) * th];
                for (k, i) in ids.iter().enumerate() {
                    let t = super::bar_tile_count(&self.gba.icons_inv[i], false, None);
                    for y in 0..th {
                        for x in 0..tw {
                            px[y * tw * ids.len() + k * tw + x] = t[y * tw + x];
                        }
                    }
                }
                crate::devtest_bmp(&f, tw * ids.len().max(1), th, &px);
                eprintln!("Leiste gespeichert: {ids:?}");
            }
            for (&i, img) in &self.gba.icons_inv {
                for sel in [false, true] {
                    // keine Anzahl auf den Kästchen (sie verdeckt die Bomben)
                    let _ = counts;
                    let count: Option<u8> = None;
                    let px = super::bar_tile_count(img, sel, count);
                    let (mut tmp, mut out) = (Vec::new(), Vec::new());
                    let size = crate::filters::apply(filter, &px, super::TILE_W, super::TILE_H, &mut tmp, &mut out);
                    let t = ctx.load_texture(format!("gba-tile-{i}-{sel}"), gfx::ColorImage { size, pixels: out }, filter.texture_options());
                    self.gba.tile_tex.insert((i, sel), t);
                }
                // Merker: Texturen für dieses Item gebaut
                let t = ctx.load_texture(format!("gba-inv-{i}"), gfx::ColorImage { size: [1, 1], pixels: vec![Color32::TRANSPARENT] }, gfx::TextureOptions::NEAREST);
                self.gba.inv_tex.insert(i, t);
            }
        }
        // Lage der Items im B-Feld einmal bestimmen: eine echte B-Aufnahme mit der Inventar-Grafik vergleichen
        if self.gba.item_off.is_none() {
            let pair = self.gba.icons_inv.iter().find_map(|(i, inv)| self.gba.icons_rgba.get(i).filter(|_| *i != 0).map(|hud| (hud.clone(), inv.clone())));
            if let Some((hud, inv)) = pair {
                self.gba.item_off = super::find_item_offset(&hud, &inv);
            }
        }
        // Item-Leiste: Grafiken ohne Tastenkreis (der Kreis ist in allen Aufnahmen gleich und wird herausgerechnet)
        if self.gba.bare_dirty {
            self.gba.bare_dirty = false;
            let mut ids: Vec<u8> = self.gba.icons_rgba.keys().copied().filter(|&i| i != 0).collect();
            ids.sort();
            let imgs: Vec<Vec<u8>> = ids.iter().map(|i| self.gba.icons_rgba[i].clone()).collect();
            if std::env::var_os("PASTLINK_ICON_LOG").is_some() {
                eprintln!("B-Aufnahmen {ids:?}, Kreis {}", self.gba.ring_raw.is_some());
            }
            if imgs.len() >= 3 {
                for (i, img) in ids.iter().zip(super::strip_ring(&imgs)) {
                    // Ersatz, solange das Item nicht aus dem Inventar aufgenommen ist: das Item (oben links im B-Feld)
                    // auf 16×16 zuschneiden – ohne Zahlen und Kreisreste darunter
                    // genauer: aus der B-Aufnahme minus leerem B-Kreis (wie das Y-Feld)
                    if !self.gba.icons_inv.contains_key(i) {
                        if let (Some(hud), Some(ring)) = (self.gba.icons_rgba.get(i), self.gba.ring_raw.as_ref()) {
                            // Item ohne Kreis, Fenster mittig um seine wirkliche Ausdehnung (kein fester Versatz, der bei
                            // manchen Items oben etwas abschnitt); Kreis-Reste fallen durch den Mehrheitsvergleich weg
                            let out = super::item_from_b(hud, ring, self.gba.item_off.unwrap_or((1, 4)), false);
                            self.gba.icons_inv.insert(*i, out);
                            self.gba.inv_tex.clear();
                        }
                    }
                    if !self.gba.icons_inv.contains_key(i) {
                        const S: usize = super::INV_ICON;
                        let mut out = vec![0u8; S * S * 4];
                        for y in 0..S.min(super::ICON_H) {
                            for x in 0..S.min(super::ICON_W - 1) {
                                let o = (y * super::ICON_W + x + 1) * 4;
                                out[(y * S + x) * 4..(y * S + x) * 4 + 4].copy_from_slice(&img[o..o + 4]);
                            }
                        }
                        self.gba.icons_inv.insert(*i, out);
                        self.gba.inv_tex.clear();
                    }
                    let t = ctx.load_texture(format!("gba-bare-{i}"), gfx::ColorImage::from_rgba_unmultiplied([super::ICON_W, super::ICON_H], &img), gfx::TextureOptions::NEAREST);
                    self.gba.icons_bare.insert(*i, t);
                }
            }
        }
        out
    }

    /// Item-Leiste zeichnen (The Minish Cap), im Stil des Item-Menüs des Spiels: Steinleiste, graue Kästchen, die
    /// Auswahl hellblau mit weißem Kreis und roten Cursor-Ecken, der Name in der schwarzen Textleiste. Gezeichnet in
    /// Spielpixeln (`gp` = Bildschirmpunkte je Spielpixel), Item-Grafiken im HD-Modus weich, sonst scharf.
    fn gba_draw_y(&self, painter: &gfx::Painter, r: gfx::Rect, gp: f32) {
        let Some(g) = &self.gba.game else { return };
        if !g.is_minish() || !self.gba.bar_open {
            return;
        }
        let en = self.s.ui_en;
        let hd = self.gba.inv_tex_hd;
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        // nur innerhalb des Spielbilds zeichnen (nicht über die Ränder daneben)
        let painter = painter.with_clip_rect(r);
        let painter = &painter;
        let gp = gp.max(1.0);
        // Spielpixel → Bildschirm (im SD-Modus auf ganze Bildschirmpixel)
        let snap = |v: f32| if hd { v } else { v.round() };
        let rnd = if hd { gp * 2.0 } else { 0.0 };
        painter.rect_filled(r, 0.0, Color32::from_black_alpha(110));
        let items = &self.gba.bar_items;
        let n = items.len().max(1);
        let sel = self.gba.bar_sel.min(n - 1);
        // Farben des Item-Menüs
        let stone = Color32::from_rgb(203, 190, 190);
        let stone_dark = Color32::from_rgb(142, 142, 142);
        let tile_c = Color32::from_rgb(165, 165, 165);
        let blue = Color32::from_rgb(120, 216, 243);
        let white = Color32::from_rgb(243, 243, 243);
        let red = Color32::from_rgb(243, 25, 0);
        let black = Color32::from_rgb(8, 8, 8);
        // Maße in Spielpixeln
        let (tw, th, step) = (super::TILE_W as f32, super::TILE_H as f32, super::TILE_W as f32 + 5.0);
        let cy = snap(r.top() + 62.0 * gp);
        let band = gfx::Rect::from_min_max(gfx::pos2(r.left(), snap(cy - 18.0 * gp)), gfx::pos2(r.right(), snap(cy + 18.0 * gp)));
        painter.rect_filled(band, 0.0, stone);
        for (y, c) in [(band.top(), black), (band.top() + gp, stone_dark), (band.bottom() - 2.0 * gp, stone_dark), (band.bottom() - gp, black)] {
            painter.rect_filled(gfx::Rect::from_min_size(gfx::pos2(band.left(), y), gfx::vec2(band.width(), gp)), 0.0, c);
        }
        if items.is_empty() {
            crate::pixfont::text(painter, gfx::pos2(band.center().x, cy), gfx::Align2::CENTER_CENTER, if en { "NO ITEMS YET" } else { "NOCH KEINE ITEMS" }, gp, black);
            return;
        }
        // wenige Items: alle mittig; viele: Auswahl in der Mitte, die Leiste läuft im Kreis
        let fits = n as f32 * step * gp < r.width() * 0.92;
        let center = if fits { (n as f32 - 1.0) / 2.0 } else { self.gba.bar_scroll };
        for (i, &it) in items.iter().enumerate() {
            let mut k = i as f32 - center;
            if !fits {
                let nf = n as f32;
                k = (k + nf / 2.0).rem_euclid(nf) - nf / 2.0;
            }
            let x = r.center().x + k * step * gp;
            if x < r.left() - step * gp || x > r.right() + step * gp {
                continue;
            }
            let is_sel = i == sel;
            let c = gfx::Rect::from_center_size(gfx::pos2(snap(x), cy), gfx::vec2(tw * gp, th * gp));
            match self.gba.tile_tex.get(&(it, is_sel)) {
                Some(t) => painter.image(t.id(), c, uv, Color32::WHITE),
                None => {
                    painter.rect_filled(c.expand(gp), rnd, stone_dark);
                    painter.rect_filled(c, rnd, if is_sel { blue } else { tile_c });
                    let ic = c.center() - gfx::vec2(0.0, gp);
                    if is_sel {
                        painter.circle_filled(ic, 9.0 * gp, white);
                    }
                    if let Some(t) = self.gba.icons_bare.get(&it).or_else(|| self.gba.icons.get(&it)) {
                        painter.image(t.id(), gfx::Rect::from_center_size(ic + gfx::vec2(0.0, 6.0 * gp), gfx::vec2(super::ICON_W as f32 * gp, super::ICON_H as f32 * gp)), uv, Color32::WHITE);
                    }
                }
            }
            if is_sel {
                // rote Cursor-Ecken wie im Item-Menü
                let o = c.expand(2.0 * gp);
                let (l, t2) = (4.0 * gp, 2.0 * gp);
                for (px_, py_, sx, sy) in [(o.left(), o.top(), 1.0, 1.0), (o.right(), o.top(), -1.0, 1.0), (o.left(), o.bottom(), 1.0, -1.0), (o.right(), o.bottom(), -1.0, -1.0)] {
                    let hx = gfx::Rect::from_two_pos(gfx::pos2(px_, py_), gfx::pos2(px_ + sx * l, py_ + sy * t2));
                    let vx = gfx::Rect::from_two_pos(gfx::pos2(px_, py_), gfx::pos2(px_ + sx * t2, py_ + sy * l));
                    painter.rect_filled(hx, 0.0, red);
                    painter.rect_filled(vx, 0.0, red);
                }
            }
        }
        // Name in der schwarzen Textleiste wie im Item-Menü
        if let Some(&it) = items.get(sel) {
            let name = super::minish::name(it, en).to_uppercase();
            let name = name.as_str();
            let tw_ = crate::pixfont::width(name, gp) + 16.0 * gp;
            let tb = gfx::Rect::from_center_size(gfx::pos2(r.center().x, snap(band.bottom() + 12.0 * gp)), gfx::vec2(tw_, 14.0 * gp));
            painter.rect_filled(tb.expand(gp), rnd, white);
            painter.rect_filled(tb, rnd, black);
            crate::pixfont::text(painter, tb.center(), gfx::Align2::CENTER_CENTER, name, gp, white);
        }
    }

    /// Bild aufbereiten wie bei A Link to the Past: Farbtabelle (Sättigung, Gamma, Farb-Randomizer), Überblenden,
    /// Bildfilter (HD auf der Grafikkarte oder auf der CPU)
    fn gba_upload(&mut self, ctx: &gfx::Context) {
        self.ensure_lut();
        let Some(g) = &self.gba.game else { return };
        let (w, h) = (g.w, g.h);
        let blending = self.s.frame_blend && !self.paused;
        if !self.gba.dirty && !blending && (self.gba.tex.is_some() || self.gpu_mode().is_some()) {
            return;
        }
        if self.gba.dirty {
            self.gba.dirty = false;
            std::mem::swap(&mut self.gba.prev, &mut self.gba.cur);
            let lut = &self.lut;
            self.gba.cur.clear();
            let col = |p: &[u8]| lut[((p[0] >> 3) as usize) | ((p[1] >> 3) as usize) << 5 | ((p[2] >> 3) as usize) << 10];
            self.gba.cur.extend(g.frame.chunks_exact(4).map(col));
            // Y-Feld links neben B ins Bild setzen (geht dann wie B/A durch Farbtabelle und HD-Filter)
            if self.gba.hud && g.is_minish() && self.s.gba_y && self.gba.cur.len() == w * h {
                // Y-Feld = leerer Y-Kreis + Item-Grafik aus dem Inventar (an der Stelle, wo das Spiel Items auf B/A
                // zeichnet) – so trägt es nie den Buchstaben B und ist auch im Item-Menü sofort da
                let y_item = self.gba.y_shown;
                // genau wie B: das Item aus seiner echten B-Aufnahme lösen (was sich vom leeren B-Kreis unterscheidet,
                // samt Anzahl bei Bomben/Pfeilen) und auf den Y-Kreis setzen; sonst die Inventar-Grafik
                let composed = match (y_item, self.gba.icons_rgba.get(&0)) {
                    (0, _) | (_, None) => None,
                    // am saubersten: die Grafik aus dem Item-Menü (nichts abgeschnitten, ohne Kreis und Anzahl)
                    (i, Some(base)) if self.gba.icons_clean.contains_key(&i) && self.gba.icons_inv.contains_key(&i) => {
                        Some(super::compose_slot(base, &self.gba.icons_inv[&i], self.gba.item_off.unwrap_or((1, 4))))
                    }
                    (i, Some(base)) => match (self.gba.icons_rgba.get(&i), self.gba.ring_raw.as_ref(), self.gba.icons_inv.get(&i)) {
                        (Some(hud), Some(ring), _) => Some(super::slot_from_b_nocount(base, hud, ring, y_item, self.gba.item_off.unwrap_or((1, 4)))),
                        (_, _, Some(item)) => Some(super::compose_slot(base, item, self.gba.item_off.unwrap_or((1, 4)))),
                        _ => None,
                    },
                };
                // Ersatz: die B-Aufnahme selbst – dann ohne die Anzahl unten rechts (Bomben, Bogen)
                let fallback = (y_item != 0 && composed.is_none()).then(|| self.gba.icons_rgba.get(&y_item).cloned()).flatten().map(|mut v| {
                    if super::minish::COUNTED.contains(&y_item) && !super::NO_DIGITS_IN_CAPTURE {
                        for y in 15..super::ICON_H {
                            for x in 13..super::ICON_W {
                                v[(y * super::ICON_W + x) * 4 + 3] = 0;
                            }
                        }
                    }
                    v
                });
                let owned = if y_item != 0 { composed.or(fallback) } else { None };
                if let Some(img) = owned.as_ref().or_else(|| self.gba.icons_rgba.get(&0)) {
                    // links neben dem B-Feld, wo Picori es gerade zeichnet (Spiel, Item-Menü, Breitbild)
                    let (bx, y0) = g.b_box().unwrap_or(((super::ICON_X + g.hud_dx()) as isize, super::ICON_Y as isize));
                    let x0 = (bx - 24).max(0) as usize;
                    for y in 0..super::ICON_H {
                        for x in 0..super::ICON_W {
                            let o = (y * super::ICON_W + x) * 4;
                            let (fx, fy) = (x0 + x, y0 + y as isize);
                            if fy < 0 {
                                continue;
                            }
                            let fy = fy as usize;
                            if img[o + 3] != 0 && fx < w && fy < h {
                                self.gba.cur[fy * w + fx] = col(&img[o..o + 4]);
                            }
                        }
                    }
                }
            }
            // Y in Benutzung: B-Feld zeigt weiter sein eigenes Item (gespeicherte Grafik oder Ausschnitt von vorher)
            if let (Some(orig), Some((bx, by))) = (g.y_active(), g.b_box()) {
                // der Ausschnitt von vorher deckt das Y-Item ab – aber nur dort, wo B-Item oder Y-Item liegen, damit
                // Beschriftungen daneben (z. B. „Rollen“ am R-Feld) unberührt bleiben
                let img = self.gba.b_patch.as_ref().or(self.gba.icons_rgba.get(&orig).filter(|_| orig != 0));
                let m_orig = self.gba.icons_rgba.get(&orig).filter(|_| orig != 0);
                let m_y = self.gba.icons_rgba.get(&self.gba.y_shown);
                let covered = |o: usize| match (m_orig, m_y) {
                    (None, None) => true,
                    (a, b) => a.is_some_and(|m| m[o + 3] != 0) || b.is_some_and(|m| m[o + 3] != 0),
                };
                if let (Some(img), true) = (img, self.gba.cur.len() == w * h) {
                    for y in 0..super::ICON_H {
                        for x in 0..super::ICON_W {
                            let o = (y * super::ICON_W + x) * 4;
                            let (fx, fy) = (bx + x as isize, by + y as isize);
                            if img[o + 3] != 0 && covered(o) && fx >= 0 && fy >= 0 && (fx as usize) < w && (fy as usize) < h {
                                self.gba.cur[fy as usize * w + fx as usize] = col(&img[o..o + 4]);
                            }
                        }
                    }
                }
            }
            // Item-Menü: die Items auf A, B und Y gleich markieren – wie das Spiel das B-Item zeigt (hellblaues Kästchen,
            // weißer Kreis hinter dem Item). B zeichnet das Spiel selbst, A (nur hellblau) und Y ergänzt Zelda Core.
            if self.gba.cur.len() == w * h && g.menu_item().is_some() {
                let a_item = g.peek(super::minish::EQUIP_A, 1).map_or(0, |b| b[0]);
                let blue = col(&[120, 216, 243, 255]);
                let white = col(&[243, 243, 243, 255]);
                let dx = w.saturating_sub(super::W) / 2;
                for item in [a_item, if self.s.gba_y { self.gba.y_shown } else { 0 }] {
                    // Bomben/Fernzünder-Bomben und Laterne an/aus stehen im Menü auf demselben Platz
                    let alt = match item {
                        7 => 8,
                        8 => 7,
                        15 => 16,
                        16 => 15,
                        i => i,
                    };
                    let Some((cx, cy)) = (item != 0).then(|| g.menu_pos(item).or_else(|| g.menu_pos(alt))).flatten() else { continue };
                    let (cx, cy) = (cx + dx, cy);
                    for y in cy.saturating_sub(15)..(cy + 4).min(h) {
                        for x in cx.saturating_sub(13)..(cx + 13).min(w) {
                            let i = (y * w + x) * 4;
                            let p = [g.frame[i], g.frame[i + 1], g.frame[i + 2]];
                            // Kästchen-Hintergrund (grau oder schon hellblau) – das Item selbst bleibt unberührt
                            if p == [165, 165, 165] || p == [142, 142, 142] || p == [120, 216, 243] {
                                // weiße Fläche genau wie beim B-Item (je Zeile von–bis, relativ zum Kästchen)
                                const MASK: [(isize, isize, isize); 17] = [
                                    (-14, -4, 3),
                                    (-13, -6, 5),
                                    (-12, -7, 6),
                                    (-11, -8, 7),
                                    (-10, -8, 7),
                                    (-9, -9, 8),
                                    (-8, -9, 8),
                                    (-7, -9, 8),
                                    (-6, -9, 8),
                                    (-5, -9, 8),
                                    (-4, -9, 8),
                                    (-3, -9, 8),
                                    (-2, -8, 7),
                                    (-1, -8, 7),
                                    (0, -7, 6),
                                    (1, -6, 5),
                                    (2, -4, 3),
                                ];
                                let (dx, dy) = (x as isize - cx as isize, y as isize - cy as isize);
                                let inside = MASK.iter().any(|&(my, a, b)| my == dy && dx >= a && dx <= b);
                                self.gba.cur[y * w + x] = if inside { white } else { blue };
                            }
                        }
                    }
                }
            }
            if self.gba.prev.len() != self.gba.cur.len() {
                self.gba.prev.clone_from(&self.gba.cur);
            }
            // Hintergrund für Breitbild (Ränder neben 4:3-Räumen und Menüs): das Bild stark verkleinert, damit es
            // gestreckt richtig weich wirkt
            if self.s.widescreen != 0 && !self.s.gba_stretch && self.gba.cur.len() == w * h {
                const K: usize = 8;
                let (bw, bh) = ((w / K).max(1), (h / K).max(1));
                let mut px = Vec::with_capacity(bw * bh);
                for by in 0..bh {
                    for bx in 0..bw {
                        let (mut r, mut gg, mut b) = (0u32, 0u32, 0u32);
                        for y in by * K..(by * K + K).min(h) {
                            for x in bx * K..(bx * K + K).min(w) {
                                let c = self.gba.cur[y * w + x];
                                r += c.r() as u32;
                                gg += c.g() as u32;
                                b += c.b() as u32;
                            }
                        }
                        let n = (K * K) as u32;
                        px.push(Color32::from_rgb((r / n) as u8, (gg / n) as u8, (b / n) as u8));
                    }
                }
                let img = gfx::ColorImage { size: [bw, bh], pixels: px };
                match &mut self.gba.back {
                    Some(t) => t.set(img, gfx::TextureOptions::LINEAR),
                    None => self.gba.back = Some(ctx.load_texture("gba-back", img, gfx::TextureOptions::LINEAR)),
                }
            }
        }
        if let Some(f) = self.gba.dev_dump.take() {
            crate::devtest_bmp(&f, self.gba.cur.len() / h.max(1), h, &self.gba.cur);
        }
        // noch kein Bild (Recomp startet noch) oder Größe passt nicht: nichts hochladen
        if self.gba.cur.len() != w * h || self.gba.prev.len() != w * h {
            return;
        }
        let src: Vec<Color32> = if blending {
            let a = (self.gba.acc.clamp(0.0, 1.0) * 256.0) as u32;
            let mix = |x: u8, y: u8| ((x as u32 * (256 - a) + y as u32 * a) >> 8) as u8;
            self.gba.prev.iter().zip(self.gba.cur.iter()).map(|(p, c)| Color32::from_rgb(mix(p.r(), c.r()), mix(p.g(), c.g()), mix(p.b(), c.b()))).collect()
        } else {
            self.gba.cur.clone()
        };
        if self.gpu_mode().is_some() {
            self.gpu.set_frame(&src, w, h);
            return;
        }
        let size = crate::filters::apply(self.s.filter, &src, w, h, &mut self.ftmp, &mut self.fout);
        let img = gfx::ColorImage { size, pixels: std::mem::take(&mut self.fout) };
        let opts = self.s.filter.texture_options();
        match &mut self.gba.tex {
            Some(t) => t.set(img, opts),
            None => self.gba.tex = Some(ctx.load_texture("gba", img, opts)),
        }
    }

    /// Spielbild im GBA-Modus (3:2, auf Wunsch ganzzahlig vergrößert oder gestreckt)
    pub(crate) fn gba_view(&mut self, painter: &gfx::Painter, area: gfx::Rect, ctx: &gfx::Context) {
        let en = self.s.ui_en;
        // Spielfläche für Menüs über dem Spiel (Spielstände): sonst stünde dort die Fläche von A Link to the Past
        self.game_area = area;
        // Recomp (Picori): die Breite der Spielansicht folgt dem gewählten Seitenverhältnis (echtes Breitbild)
        if let Some(g) = self.gba.game.as_mut() {
            let (vw, vh) = match self.s.widescreen {
                1 => (16, 9),
                2 => (21, 9),
                3 => (area.width().max(1.0) as i32, area.height().max(1.0) as i32),
                _ => (3, 2),
            };
            g.set_view(vw, vh, self.s.widescreen != 0);
            if g.loading() {
                let txt = tr(en, "RECOMP STARTET ... (BEIM ERSTEN MAL ETWA 20 S)", "RECOMP STARTING ... (ABOUT 20 S THE FIRST TIME)");
                // so groß wie möglich, aber immer ganz im Fenster
                let fit = area.width() * 0.9 / crate::pixfont::width(txt, 1.0).max(1.0);
                let gp = (area.height() / 160.0).round().min(fit.floor()).max(1.0);
                crate::pixfont::text(painter, area.center(), gfx::Align2::CENTER_CENTER, txt, gp, Color32::WHITE);
                return;
            }
        }
        let Some(g) = &self.gba.game else { return };
        let (gw, gh) = (g.w as f32, g.h as f32);
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        // Bildfläche: 16:9 / 21:9 / Fenster füllen wie bei A Link to the Past (das GBA-Bild selbst bleibt 3:2,
        // der Rand zeigt das Spielbild weich und dunkel – oder „strecken“ zieht das Bild auf die ganze Fläche)
        let frame_aspect = match self.s.widescreen {
            1 => Some(16.0 / 9.0),
            2 => Some(21.0 / 9.0),
            3 => Some(area.width() / area.height().max(1.0)),
            _ => None,
        };
        let fit = |aspect: f32| {
            let (w, h) = if area.width() / area.height() > aspect { (area.height() * aspect, area.height()) } else { (area.width(), area.width() / aspect) };
            gfx::Rect::from_center_size(area.center(), gfx::vec2(w, h))
        };
        let frame = frame_aspect.map(fit);
        let r = if self.s.gba_stretch {
            frame.unwrap_or(area)
        } else {
            let outer = frame.unwrap_or(area);
            let aspect = gw / gh;
            let (mut w, mut h) = if outer.width() / outer.height() > aspect { (outer.height() * aspect, outer.height()) } else { (outer.width(), outer.width() / aspect) };
            if self.s.integer_scale {
                let ppp = ctx.pixels_per_point();
                let k = ((h * ppp) / gh).floor().max(1.0);
                h = k * gh / ppp;
                w = h * aspect;
            }
            gfx::Rect::from_center_size(outer.center(), gfx::vec2(w, h))
        };
        if let (Some(f), Some(b), false) = (frame, &self.gba.back, self.s.gba_stretch) {
            // Rand: Spielbild vergrößert, abgedunkelt
            let zoom = gfx::Rect::from_center_size(f.center(), gfx::vec2(f.width().max(f.height() * gw / gh), f.width().max(f.height() * gw / gh) * gh / gw));
            painter.with_clip_rect(f).image(b.id(), zoom, uv, Color32::from_gray(140));
        }
        if let Some(m) = self.gpu_mode() {
            // HD-Filter auf der Grafikkarte: genau in Bildschirmpixeln berechnen und 1:1 zeigen
            let ppp = ctx.pixels_per_point();
            let (x0, y0) = ((r.left() * ppp).round(), (r.top() * ppp).round());
            let (x1, y1) = ((r.right() * ppp).round(), (r.bottom() * ppp).round());
            let snapped = gfx::Rect::from_min_max(gfx::pos2(x0 / ppp, y0 / ppp), gfx::pos2(x1 / ppp, y1 / ppp));
            if let Some(img) = self.gpu.render(m, (x1 - x0) as i32, (y1 - y0) as i32) {
                painter.render_image(img, snapped);
            }
        } else if let Some(t) = &self.gba.tex {
            painter.image(t.id(), r, uv, Color32::WHITE);
        }
        let gp_game = r.width() / gw;
        self.gba_draw_y(painter, r, gp_game);
        // eigene Karte über dem Spielbild (ganze Fläche) – oder im eigenen Fenster
        if !self.s.map_own_window {
            self.gba_map_draw(area, ctx);
        }
        let gold = Color32::from_rgb(235, 200, 90);
        let ind = if self.turbo_on {
            Some(format!("⏩ TURBO ×{:.0}", self.s.turbo_speed))
        } else if self.double_on {
            Some("⏩ ×2".to_string())
        } else if self.rewind_on {
            Some(tr(en, "⏪ Zurückspulen", "⏪ Rewind").to_string())
        } else {
            None
        };
        // wie bei A Link to the Past: Pixel-Schrift in einem Kasten unten rechts, „PAUSE“ groß in der Mitte
        let gp = gp_game.max(1.0).round();
        if let Some(txt) = ind {
            let tr_ = gfx::pos2(r.right() - gp * 8.0, r.bottom() - gp * 18.0);
            let w = crate::pixfont::width(&txt, gp);
            let br = gfx::Rect::from_min_size(gfx::pos2(tr_.x - w - gp * 4.0, tr_.y - gp * 3.0), gfx::vec2(w + gp * 8.0, gp * 13.0));
            painter.rect_filled(br, 0.0, Color32::from_rgb(8, 10, 40));
            painter.rect_stroke(br, 0.0, Stroke::new(gp, Color32::WHITE));
            crate::pixfont::text(painter, tr_, gfx::Align2::RIGHT_TOP, &txt, gp, gold);
        }
        // „PAUSE“ nicht unter offenen Menüs (Spielstände, Karte) – dort halb verdeckt
        if (self.paused || self.auto_paused) && !self.win_states && !self.gba.map.open {
            crate::pixfont::text(painter, area.center(), gfx::Align2::CENTER_CENTER, tr(en, "PAUSE", "PAUSED"), (gp_game * 2.0).round().max(2.0), Color32::WHITE);
        }
        if self.s.show_fps {
            painter.text(gfx::pos2(r.left() + 10.0, r.top() + 8.0), gfx::Align2::LEFT_TOP, format!("{:.0} FPS · {} {:.0}", self.fps, tr(en, "Spiel", "game"), self.game_fps), gfx::FontId::monospace(14.0), Color32::YELLOW);
        }
        if let Some((m, t)) = &self.status {
            let age = t.elapsed().as_secs_f32();
            if age < 3.0 {
                let a = ((3.0 - age) / 0.4).clamp(0.0, 1.0);
                let g = painter.layout_no_wrap(m.clone(), gfx::FontId::proportional(14.0), Color32::from_white_alpha((235.0 * a) as u8));
                let pos = area.left_bottom() + gfx::vec2(16.0, -16.0 - g.size().y);
                let bg = gfx::Rect::from_min_size(pos - gfx::vec2(12.0, 6.0), g.size() + gfx::vec2(24.0, 12.0));
                painter.rect_filled(bg, 8.0, Color32::from_rgba_unmultiplied(16, 18, 22, (215.0 * a) as u8));
                painter.rect_stroke(bg, 8.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, (22.0 * a) as u8)));
                painter.galley(pos, g, Color32::WHITE);
            }
        }
    }

    /// Spielstand speichern (Platz 1..9), mit Vorschaubild für das Spielstand-Menü
    pub(crate) fn gba_save_slot(&mut self, slot: u8) {
        let Some(g) = &self.gba.game else { return };
        let rc = g.is_rc();
        // Picori-Stände (alle Spielvariablen, gut 1 MB) gepackt speichern
        let r = g
            .save_state()
            .ok_or_else(|| "Kern liefert keinen Spielstand".to_string())
            .and_then(|b| std::fs::write(super::state_path(&g.path, slot, rc), if rc { lz4_flex::compress_prepend_size(&b) } else { b }).map_err(|e| e.to_string()));
        if let (true, Some(t)) = (r.is_ok(), g.thumb()) {
            let _ = std::fs::write(g.path.with_extension(format!("{}{slot}.thumb", if rc { "rs" } else { "gs" })), lz4_flex::compress_prepend_size(&t));
        }
        // Angaben wie bei A Link to the Past: Spielzeit, Tode, Herzen, Rubine
        if r.is_ok() {
            let meta = self.gba_meta();
            if let Some(mp) = self.meta_path(slot) {
                let _ = std::fs::write(mp, serde_json::to_string(&meta).unwrap_or_default());
            }
            self.metas.remove(&slot);
        }
        self.slot = slot;
        self.thumbs.remove(&slot);
        let m = match r {
            Ok(()) => format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("gespeichert", "saved")),
            Err(e) => format!("{}: {e}", self.t("Speichern fehlgeschlagen", "Saving failed")),
        };
        self.msg(m);
    }

    /// Spielstand laden (Platz 1..9)
    pub(crate) fn gba_load_slot(&mut self, slot: u8) {
        self.slot = slot;
        let Some(g) = &mut self.gba.game else { return };
        let rc = g.is_rc();
        let file = std::fs::read(super::state_path(&g.path, slot, rc)).ok();
        let exists = file.is_some();
        let ok = file.and_then(|b| if rc { lz4_flex::decompress_size_prepended(&b).ok() } else { Some(b) }).map(|b| g.load_state(&b)).unwrap_or(false);
        if ok {
            self.gba.hist.clear();
            g.run_frame(0);
            let mut skip = Vec::new();
            g.take_audio(&mut skip);
            self.gba.last_hp = 0;
            // Item-Grafiken neu aus dem geladenen Stand aufnehmen (alte Aufnahmen können aus einem
            // ungünstigen Moment stammen – dann wäre das Quickmenü nach dem Laden falsch)
            self.gba.icons.clear();
            self.gba.icons_rgba.clear();
            self.gba.icons_menu.clear();
            self.gba.icons_bare.clear();
            self.gba.icons_inv.clear();
            self.gba.icons_clean.clear();
            self.gba.inv_tex.clear();
            self.gba.tile_tex.clear();
            self.gba.tile_filter = None;
            self.gba.y_empty = None;
            self.gba.ring_raw = None;
            self.gba.item_off = None;
            self.gba.inv_grabbed = false;
            self.gba.b_patch = None;
            self.gba.y_checked = 0;
            self.gba.capture_at = 0;
            self.gba.dirty = true;
            if let Some(m) = self.slot_meta(slot) {
                self.gba.play_frames = m.play_frames;
                self.gba.deaths = m.deaths;
            }
        }
        let m = if ok {
            format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("geladen", "loaded"))
        } else if exists {
            // Picori-Stände passen nur zu der tmc_core.dll, mit der sie gespeichert wurden
            format!("{} {slot}: {}", self.t("Spielstand", "Save state"), self.t("stammt von einer anderen Recomp-Fassung und lässt sich nicht laden", "was saved by a different Recomp version and can't be loaded"))
        } else {
            format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("ist leer", "is empty"))
        };
        self.msg(m);
    }

    fn gba_dev_keys(&mut self) -> u16 {
        let Ok(spec) = std::env::var("PASTLINK_GBA_KEYS") else { return 0 };
        let Some(g) = &self.gba.game else { return 0 };
        if g.loading() {
            return 0;
        }
        // gezählt in Spielbildern (die Anzeige kann viel schneller laufen)
        let t = self.gba.play_frames as u32;
        let mut pad = 0u16;
        let mut dpad = 0u16;
        let mut map_at = None;
        let mut cheat_at = None;
        let mut equip_at = None;
        for e in spec.split(',') {
            let Some((k, at)) = e.split_once(':') else { continue };
            let Ok(at) = at.parse::<u32>() else { continue };
            if t < at || t >= at + 4 {
                continue;
            }
            match k {
                "s" => pad |= 0x1000,
                "a" => pad |= 0x0080,
                "b" => pad |= 0x8000,
                "y" => pad |= 0x4000,
                "u" => pad |= 0x0800,
                "d" => pad |= 0x0400,
                "l" => pad |= 0x0200,
                "r" => pad |= 0x0100,
                "Q" => pad |= 0x0020,
                "E" => pad |= 0x0010,
                "D" => dpad |= 1 << 10,
                "L" => dpad |= 1 << 9,
                "R" => dpad |= 1 << 8,
                // Karte (Minus) – einmal je Eintrag
                "m" => map_at = Some(at),
                // 100-%-Cheat – einmal je Eintrag
                "c" => cheat_at = Some(at),
                // Item auf B legen wie die Schnellleiste (PASTLINK_GBA_EQUIPB = Item)
                "e" if t == at => equip_at = std::env::var("PASTLINK_GBA_EQUIPB").ok().and_then(|v| v.parse::<u8>().ok()),
                // fertiges Bild (mit Y-Feld) als BMP neben die .exe speichern – einmal je Eintrag
                "p" if !std::path::Path::new(&format!("frame_{at}.bmp")).exists() => {
                    self.gba.dev_dump = Some(format!("frame_{at}.bmp"));
                }
                _ => {}
            }
        }
        if let (Some(i), Some(g)) = (equip_at, self.gba.game.as_mut()) {
            g.equip(false, i);
        }
        if let Some(at) = cheat_at {
            if self.gba.map.dev_m != at + 1_000_000 {
                self.gba.map.dev_m = at + 1_000_000;
                if let Some(g) = &self.gba.game {
                    let _ = g.locations(super::tmcmap::LOGIC);
                    eprintln!("Cheat: {} Fundorte", g.cheat_complete());
                }
                self.gba.map.visit_all();
            }
        }
        if let Some(at) = map_at {
            if self.gba.map.dev_m != at {
                self.gba.map.dev_m = at;
                self.gba_map_toggle();
            }
        }
        self.ctl_dpad |= dpad;
        if pad != 0 && std::env::var_os("PASTLINK_ICON_LOG").is_some() {
            eprintln!("Taste {pad:04x} bei Bild {t}, Menü {:?}", self.gba.game.as_ref().and_then(|g| g.menu_item()));
        }
        pad
    }

    /// Entwicklung: Slot 3 speichern, weiterspielen, laden; Ergebnis in `slottest.txt` neben der .exe, danach das
    /// Spielstand-Menü öffnen
    fn gba_slot_test(&mut self) {
        let Some(g) = &self.gba.game else { return };
        if g.loading() {
            return;
        }
        self.gba.dev_tick += 1;
        let t = self.gba.dev_tick;
        let log = |m: String| {
            if let Some(p) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("slottest.txt"))) {
                let old = std::fs::read_to_string(&p).unwrap_or_default();
                let _ = std::fs::write(p, old + &m + "
");
            }
        };
        if t == 100 {
            let g = self.gba.game.as_ref().unwrap();
            log(format!("hud {} w {} h {} dx {} y_item {} icons {:?}", self.gba.hud, g.w, g.h, g.hud_dx(), g.cfg.y_item, self.gba.icons_rgba.keys().collect::<Vec<_>>()));
        }
        if t == 120 {
            self.gba.deaths = 2;
            self.gba_save_slot(3);
            let g = self.gba.game.as_ref().unwrap();
            let meta = self.meta_path(3).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            let thumb = std::fs::metadata(g.path.with_extension(format!("{}3.thumb", if g.is_rc() { "rs" } else { "gs" }))).map(|m| m.len()).unwrap_or(0);
            let st = std::fs::metadata(super::state_path(&g.path, 3, g.is_rc())).map(|m| m.len()).unwrap_or(0);
            log(format!("gespeichert: Stand {st} B, Bild {thumb} B, meta {meta}"));
            log(format!("Meldung: {:?}", self.status.as_ref().map(|s| s.0.clone())));
            let g = self.gba.game.as_ref().unwrap();
            log(format!("frame@save {}", g.frame.iter().map(|&b| b as u64).sum::<u64>()));
        }
        if t == 300 {
            let before = self.gba.play_frames;
            self.gba.deaths = 0;
            self.gba_load_slot(3);
            let g = self.gba.game.as_ref().unwrap();
            log(format!("geladen: Meldung {:?}, Spielzeit vorher {before} nachher {}, Tode {}", self.status.as_ref().map(|s| s.0.clone()), self.gba.play_frames, self.gba.deaths));
            log(format!("frame@load {}", g.frame.iter().map(|&b| b as u64).sum::<u64>()));
            let m = self.slot_meta(3);
            log(format!("slot_meta: {:?}", m.map(|m| (m.play_frames, m.deaths, m.health, m.max_health, m.rupees))));
            self.win_states = std::env::var("PASTLINK_GBA_SLOTTEST").is_ok_and(|v| v == "menu");
            self.slot = 3;
        }
    }

    /// Angaben eines Savestates (Spielzeit, Tode, Herzen, Rubine) wie bei A Link to the Past
    pub(crate) fn gba_meta(&self) -> crate::app::SlotMeta {
        let mut m = crate::app::SlotMeta { play_frames: self.gba.play_frames, deaths: self.gba.deaths, ..Default::default() };
        let Some(g) = &self.gba.game else { return m };
        m.fps = g.fps;
        if g.is_minish() {
            if let Some(b) = g.peek(0x0200_2AEA, 0x18) {
                m.health = b[0];
                m.max_health = b[1];
                m.rupees = Some(u16::from_le_bytes([b[0x16], b[0x17]]));
            }
        }
        m
    }

    /// Zeit des Spielstands (für das Menü), None = leer
    pub(crate) fn gba_slot_time(&self, slot: u8) -> Option<String> {
        let g = self.gba.game.as_ref()?;
        let t = std::fs::metadata(super::state_path(&g.path, slot, g.is_rc())).ok()?.modified().ok()?;
        let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
        Some(crate::gba::ui::local_time(secs))
    }

    /// Titelbild eines GBA-Spiels für die Karte im Hauptmenü (beim ersten Mal kurz das Spiel laufen lassen)
    pub(crate) fn gba_card(&mut self, path: &Path, ctx: &gfx::Context) -> Option<gfx::TextureHandle> {
        if let Some(t) = self.gba.cards.get(path) {
            return t.clone();
        }
        #[cfg(pastlink_gba)]
        let t = super::title_image(path).map(|(px, w, h)| ctx.load_texture(format!("gba-card-{}", path.display()), gfx::ColorImage::from_rgba_unmultiplied([w, h], &px), gfx::TextureOptions::NEAREST));
        #[cfg(not(pastlink_gba))]
        let t = {
            let _ = ctx;
            None
        };
        self.gba.cards.insert(path.to_path_buf(), t.clone());
        t
    }
}

/// Ortszeit als „TT.MM. HH:MM“ (Windows: Zeitzone aus dem System)
pub fn local_time(secs: i64) -> String {
    let off = utc_offset_secs();
    let t = secs + off;
    let days = t.div_euclid(86400);
    let rem = t.rem_euclid(86400);
    // Datum aus Tagen seit 1970 (Algorithmus von H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{d:02}.{m:02}. {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

fn utc_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Tzi {
            bias: i32,
            _rest: [u8; 168],
        }
        extern "system" {
            fn GetTimeZoneInformation(tz: *mut Tzi) -> u32;
        }
        let mut tz = Tzi { bias: 0, _rest: [0; 168] };
        let r = unsafe { GetTimeZoneInformation(&mut tz) };
        // r == 2: Sommerzeit aktiv (DaylightBias steht bei Offset 168 innerhalb von _rest: Bias(4)+StdName(64)+StdDate(16)+StdBias(4)+DltName(64)+DltDate(16) = 168)
        let dlt_bias = i32::from_le_bytes([tz._rest[164], tz._rest[165], tz._rest[166], tz._rest[167]]);
        let std_bias = i32::from_le_bytes([tz._rest[80], tz._rest[81], tz._rest[82], tz._rest[83]]);
        let b = tz.bias + if r == 2 { dlt_bias } else { std_bias };
        -(b as i64) * 60
    }
    #[cfg(not(windows))]
    0
}

/// Leeres B-Feld in ein Y-Feld verwandeln: Buchstabe „B“ (1 Pixel dünn, dunkelblau auf graublau) durch ein „Y“ in
/// derselben Schrift ersetzen
pub(crate) fn letter_y(img: &mut [u8]) {
    let (w, h) = (super::ICON_W, super::ICON_H);
    let px = |img: &[u8], x: usize, y: usize| -> [u8; 4] { let i = (y * w + x) * 4; [img[i], img[i + 1], img[i + 2], img[i + 3]] };
    let lum = |p: [u8; 4]| p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32;
    // Kreis: sichtbare Pixel in der unteren Hälfte
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in h / 2..h {
        for x in 0..w {
            if px(img, x, y)[3] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 <= x0 + 4 || y1 <= y0 + 4 {
        return;
    }
    let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    let rad = ((x1 - x0) as f32 / 2.0).min((y1 - y0) as f32 / 2.0) - 2.0;
    let inner: Vec<(usize, usize)> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() < rad).collect();
    // Buchstabe nur in der Mitte suchen: der Schatten des Kreises hat dieselbe dunkle Farbe und muss bleiben
    let core: Vec<(usize, usize)> = inner.iter().copied().filter(|&(x, y)| (x as f32 - cx).abs() <= 3.6 && (y as f32 - cy).abs() <= 3.6).collect();
    // Buchstabe = dunkelste Farbe innen; Füllung = häufigste übrige Farbe innen (graublau)
    let Some(lcol) = core.iter().map(|&(x, y)| px(img, x, y)).filter(|p| p[3] != 0).min_by_key(|&p| lum(p)) else { return };
    let mut counts: Vec<([u8; 4], usize)> = Vec::new();
    for &(x, y) in &core {
        let p = px(img, x, y);
        if p[3] == 0 || p == lcol {
            continue;
        }
        match counts.iter_mut().find(|c| c.0 == p) {
            Some(c) => c.1 += 1,
            None => counts.push((p, 1)),
        }
    }
    let Some(&(fill, _)) = counts.iter().max_by_key(|c| c.1) else { return };
    let letter: Vec<(usize, usize)> = core.iter().copied().filter(|&(x, y)| px(img, x, y) == lcol).collect();
    if letter.is_empty() {
        return;
    }
    let (lx0, ly0) = (letter.iter().map(|p| p.0).min().unwrap(), letter.iter().map(|p| p.1).min().unwrap());
    let ly1 = letter.iter().map(|p| p.1).max().unwrap();
    for &(x, y) in &letter {
        let i = (y * w + x) * 4;
        img[i..i + 4].copy_from_slice(&fill);
    }
    // „Y“, 5 breit, so hoch wie das „B“: zwei Schrägen, dann der Stiel in der Mitte
    let left = lx0.saturating_sub(1);
    let lh = ly1 - ly0 + 1;
    for y in 0..lh {
        let xs: Vec<usize> = match y {
            0 => vec![left, left + 4],
            1 => vec![left + 1, left + 3],
            _ => vec![left + 2],
        };
        for x in xs {
            if x < w {
                let i = ((ly0 + y) * w + x) * 4;
                img[i..i + 4].copy_from_slice(&lcol);
            }
        }
    }
}

/// Schlüssel eines GBA-Spiels in den Einstellungen
fn gba_key(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}
