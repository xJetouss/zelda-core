use crate::alttp::{CheatCode, Cheats};
use crate::filters::Filter;
use crate::hotkeys::{default_combo, Combo, ACTS};
use crate::gfx::Key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Reihenfolge entspricht den SNES-Joypad-Bits 15..4.
pub const BUTTONS: [&str; 12] = ["B", "Y", "Select", "Start", "↑", "↓", "←", "→", "A", "X", "L", "R"];

fn default_keys() -> Vec<String> {
    [Key::S, Key::A, Key::C, Key::Enter, Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight, Key::D, Key::W, Key::Q, Key::E]
        .iter()
        .map(|k| k.name().to_string())
        .collect()
}

/// Einstellungen, die zum Spiel gehören (werden je Spiel gespeichert und beim Laden des Spiels übernommen)
pub const GAME_KEYS: &[&str] = &[
    "cheats", "codes", "rando_mode", "rando_seed", "rando_reset", "rando_on", "color_rando", "color_seed",
    "x_slot", "pixel_inv", "minus_map", "more_bombs", "sword_pots", "mirror_direct", "auto_collect", "fast_text",
    "health_blink", "fairy_notice", "quick_dash", "wide_sprites", "edge_recomp", "transition_speed",
    "map_teleport", "map_own_window", "map_mode", "map_filter", "map_custom", "map_layers", "map_grid", "map_follow",
    "dpad_quickmenu", "dpad_no_move",
];

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// fehlt in alten Dateien -> 0, damit die Migration läuft
    #[serde(default)]
    pub version: u32,
    /// Menüsprache – wird beim Start aus der Windows-Sprache bestimmt
    #[serde(skip)]
    pub ui_en: bool,
    // Steuerung
    pub keys: Vec<String>,
    pub pad_label_layout: bool,
    pub deadzone: f32,
    pub block_opposite: bool,
    /// Aktions-ID -> Tastenkombination ("" = nicht belegt). Fehlende Einträge = Standard.
    pub hotkeys: BTreeMap<String, String>,
    // Audio
    pub volume: f32,
    pub mute: bool,
    pub interp: u8,
    pub echo: bool,
    pub voice_mute: u8,
    pub latency_ms: u32,
    pub mute_turbo: bool,
    // Grafik
    pub filter: Filter,
    pub last_hd: Filter,
    pub integer_scale: bool,
    pub aspect_43: bool,
    pub frame_blend: bool,
    pub show_fps: bool,
    pub layer_mask: u8,
    pub color_rando: bool,
    pub color_seed: u32,
    pub saturation: f32,
    pub gamma: f32,
    // Emulation
    pub speed: f32,
    pub turbo_speed: f32,
    pub runahead: u8,
    pub rewind: bool,
    pub rewind_seconds: u32,
    pub autoload: bool,
    pub pause_in_menus: bool,
    pub pal60: bool,
    // Karte
    pub map_grid: bool,
    pub map_follow: bool,
    // Randomizer
    pub rando_mode: u8,
    pub rando_seed: String,
    pub rando_reset: bool,
    /// Randomizer aktiv (wird beim Laden der ROM mit Seed/Modus angewendet)
    pub rando_on: bool,
    /// Spielinterne Spielstände (.srm) auf die Festplatte schreiben
    pub save_sram: bool,
    /// Steuerkreuz rechts (Controller) öffnet das Schnellmenü statt nach rechts zu laufen
    pub dpad_quickmenu: bool,
    /// Bildwiederholrate des Monitors übernehmen, wenn sie nah an der Spielrate liegt (ruckelfrei)
    pub vsync_lock: bool,
    /// Zwischenbilder (Scrollen + Sprites interpoliert) für hohe Bildraten
    pub hi_fps: bool,
    /// GBA: Bild glätten, ganzzahlig vergrößern, auf das Fenster strecken; zuletzt gespieltes GBA-Spiel
    pub gba_smooth: bool,
    pub gba_integer: bool,
    pub gba_stretch: bool,
    pub last_gba: String,
    /// The Minish Cap mit Picori (PC-Fassung, echtes Breitbild) statt mGBA, wenn tmc_core.dll da ist
    pub gba_rc: bool,
    /// GBA: Cheats und Y-Item je Spiel (Schlüssel: Dateiname der ROM ohne Endung)
    pub gba_games: std::collections::HashMap<String, crate::gba::GameCfg>,
    /// Recompiled-Modus: übersetzter ROM-Code statt CPU-Emulation (Spielstände bleiben austauschbar)
    pub recompiled: bool,
    /// eingeschaltete Mods (Kennungen)
    pub mods_on: Vec<String>,
    /// Bildrate: 0 = Monitor (VSync), sonst feste Rate ohne VSync (z. B. 200)
    pub fps_limit: u32,
    /// Breitbild: 0 = 4:3, 1 = 16:9, 2 = 21:9 (Ultrawide), 3 = Fenster füllen
    pub widescreen: u8,
    /// Karte: Ebenen (Herzteile, Höhlen-Truhen, Dungeons, Erledigte, Eingänge/Höhlen)
    pub map_layers: [bool; 5],
    /// Teleport-Auswahl in der Weltkarte des Spiels (L/R + Y)
    pub map_teleport: bool,
    /// Pause, wenn das Fenster nicht aktiv ist oder die Karte offen ist
    pub auto_pause: bool,
    /// Karte in einem eigenen Fenster
    pub map_own_window: bool,
    /// Steuerkreuz am Controller bewegt Link nicht (↑ Spielstände, ↓ Karte, → Schnellmenü)
    pub dpad_no_move: bool,
    /// Szenenwechsel schneller (1 = normal)
    pub transition_speed: f32,
    /// Texte sofort vollständig anzeigen
    pub fast_text: bool,
    /// Herzen und Rubine automatisch einsammeln
    pub auto_collect: bool,
    /// Spiegel wird direkt benutzt, wenn er im Inventar/Schnellmenü gewählt wird
    pub mirror_direct: bool,
    /// bis zu 4 Bomben gleichzeitig
    pub more_bombs: bool,
    /// kein Warnton bei wenig Energie, dafür blinkendes Herz am Bildrand
    pub health_blink: bool,
    /// Hinweis, wenn eine Fee aus der Flasche gerettet hat
    pub fairy_notice: bool,
    /// kürzerer Anlauf beim Pegasus-Sprint
    pub quick_dash: bool,
    /// echtes Breitbild: Gegner und Gegenstände auch im Rand (ROM-Patch)
    pub wide_sprites: bool,
    /// Breitbild-Rand wie zelda3 / zelda-alttp-recomp: nur bis zur Gebietsgrenze (dahinter schwarz)
    pub edge_recomp: bool,

    /// X = zweiter Item-Slot (statt Spielkarte)
    pub x_slot: bool,
    /// eigenes Inventar im Pixel-Look statt des Spielmenüs
    pub pixel_inv: bool,
    /// Controller-Minus/Select öffnet die Karte (statt Select im Spiel)
    pub minus_map: bool,
    /// Töpfe mit dem Schwert zerschlagen
    pub sword_pots: bool,
    /// Karte: 0 = alles (Spoiler), 1 = nur Gesammeltes und Teleport-Punkte
    pub map_mode: u8,
    /// Karte: was wird angezeigt (0 Übersicht, 1 Sammeln, 2 Teleport, 3 Quests, 4 Alles)
    pub map_filter: u8,
    /// Anzeige „Eigene“: abgehakte Arten (Bits: 0 Dungeons, 1 Teleport, 2 Herzen, 3 Truhen,
    /// 4 Flaschen, 5 Upgrades, 6 Quests, 7 eigene Markierungen)
    pub map_custom: u8,
    // Cheats
    pub cheats: Cheats,
    pub codes: Vec<CheatCode>,
    // Pfade / Sprache
    pub last_rom: String,
    /// Spiel-Einstellungen je Spiel (Schlüssel = Dateiname der ROM)
    pub games: BTreeMap<String, serde_json::Value>,
    pub base_rom: String,
}

impl Settings {
    /// gespeicherte Spiel-Einstellungen dieses Spiels übernehmen (falls vorhanden)
    pub fn apply_game(&mut self, key: &str) {
        let Some(src) = self.games.get(key).and_then(|v| v.as_object().cloned()) else { return };
        let Ok(mut all) = serde_json::to_value(&*self) else { return };
        if let Some(o) = all.as_object_mut() {
            for (k, v) in src {
                if GAME_KEYS.contains(&k.as_str()) {
                    o.insert(k, v);
                }
            }
        }
        if let Ok(ns) = serde_json::from_value::<Settings>(all) {
            let ui_en = self.ui_en;
            *self = ns;
            self.ui_en = ui_en;
        }
    }

    /// aktuelle Spiel-Einstellungen für dieses Spiel merken
    pub fn store_game(&mut self, key: &str) {
        let Ok(all) = serde_json::to_value(&*self) else { return };
        let m: serde_json::Map<String, serde_json::Value> = GAME_KEYS.iter().filter_map(|k| all.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
        self.games.insert(key.to_string(), serde_json::Value::Object(m));
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 8,
            ui_en: crate::i18n::system_is_english(),
            keys: default_keys(),
            pad_label_layout: false,
            deadzone: 0.35,
            block_opposite: true,
            hotkeys: BTreeMap::new(),
            volume: 0.8,
            mute: false,
            interp: 0,
            gba_smooth: false,
            gba_integer: true,
            gba_stretch: false,
            last_gba: String::new(),
            gba_rc: true,
            gba_games: std::collections::HashMap::new(),
            echo: true,
            voice_mute: 0,
            latency_ms: 30,
            mute_turbo: true,
            filter: Filter::Pixel,
            last_hd: Filter::HdSoft,
            integer_scale: false,
            aspect_43: false,
            frame_blend: false,
            show_fps: false,
            layer_mask: 0x1F,
            color_rando: false,
            color_seed: 1,
            saturation: 1.0,
            gamma: 1.0,
            speed: 1.0,
            turbo_speed: 4.0,
            runahead: 0,
            rewind: true,
            rewind_seconds: 20,
            autoload: true,
            pause_in_menus: false,
            pal60: true,
            map_grid: false,
            map_follow: false,
            rando_mode: 1,
            rando_seed: String::new(),
            rando_reset: true,
            rando_on: false,
            save_sram: false,
            dpad_quickmenu: true,
            vsync_lock: true,
            hi_fps: true,
            recompiled: false,
            mods_on: Vec::new(),
            fps_limit: 0,
            widescreen: 3,
            map_layers: [true, true, true, true, true],
            map_teleport: true,
            auto_pause: true,
            map_own_window: true,
            dpad_no_move: true,
            transition_speed: 2.0,
            fast_text: true,
            auto_collect: true,
            mirror_direct: true,
            more_bombs: true,
            health_blink: true,
            fairy_notice: true,
            quick_dash: true,
            wide_sprites: true,
            edge_recomp: false,

            x_slot: true,
            pixel_inv: true,
            minus_map: true,
            sword_pots: true,
            map_mode: 0,
            map_filter: 4,
            map_custom: 0xFF,
            cheats: Cheats { skip_title: true, ..Cheats::default() },
            codes: Vec::new(),
            last_rom: String::new(),
            games: BTreeMap::new(),
            base_rom: String::new(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("zelda-core.json")
    }

    /// frühere Einstellungsdatei (Programm hieß „pastlink“) – wird nur gelesen, nie geändert
    fn old_path() -> PathBuf {
        Self::path().with_file_name("pastlink.json")
    }

    pub fn load() -> Self {
        let src = if Self::path().is_file() { Self::path() } else { Self::old_path() };
        let mut s: Settings = std::fs::read_to_string(src).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
        if s.keys.len() != BUTTONS.len() {
            s.keys = default_keys();
        }
        // früher eingebaute Mods sind jetzt DLLs aus der Mod-Liste (Kennung „dll:Dateiname“)
        for id in s.mods_on.iter_mut() {
            if !id.starts_with("dll:") {
                *id = format!("dll:{id}.dll");
            }
        }
        if s.version < 4 {
            // flüssiger und schärfer: PAL mit 60 Hz, Clean HD als HD-Modus
            s.pal60 = true;
            s.last_hd = crate::filters::Filter::HdClean;
            if s.filter.is_hd() {
                s.filter = crate::filters::Filter::HdClean;
            }
            s.version = 4;
        }
        if s.version < 3 {
            // Leertaste ist jetzt "doppelte Geschwindigkeit", Intro-Skip ist Standard
            if s.keys[2] == Key::Space.name() {
                s.keys[2] = Key::C.name().to_string();
            }
            s.cheats.skip_title = true;
            s.autoload = true;
            s.rando_mode = 1;
            s.hotkeys.clear();
            s.speed = 1.0;
            s.version = 4;
        }
        if s.version < 5 {
            // weiches HD (xBRZ 6×), Zwischenbilder mit 200 FPS, weniger Eingabeverzögerung
            s.last_hd = crate::filters::Filter::HdSoft;
            if s.filter.is_hd() {
                s.filter = crate::filters::Filter::HdSoft;
            }
            s.hi_fps = true;
            s.fps_limit = 200;
            s.block_opposite = true;
            if s.runahead == 0 {
                s.runahead = 1;
            }
            s.version = 5;
        }
        if s.version < 6 {
            // flüssig: Bildrate an den Monitor binden (kein Zerreißen), kein Run-Ahead (halbe Rechenlast),
            // Bewegung wieder wie im Original
            s.fps_limit = 0;
            s.runahead = 0;
            s.block_opposite = true;
            s.version = 6;
        }
        if s.version < 8 {
            // HD-Taste wieder auf „HD weich“ (gefiel besser als HD Max)
            if s.last_hd == crate::filters::Filter::HdMax {
                s.last_hd = crate::filters::Filter::HdSoft;
            }
            if s.filter == crate::filters::Filter::HdMax && s.version == 7 {
                s.filter = crate::filters::Filter::HdSoft;
            }
            s.version = 8;
        }
        // nur noch fünf Bildmodi: ältere Auswahl auf den passenden abbilden (nach allen Umstellungen)
        s.filter = s.filter.menu_mode();
        s.last_hd = s.last_hd.menu_mode();
        // „Unbegrenzt“ läuft getaktet mit höchstens 1000 Bildern (sonst rechnet die CPU dauernd)
        if s.fps_limit > 1000 {
            s.fps_limit = 1000;
        }
        s.ui_en = crate::i18n::system_is_english();
        s
    }

    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), t);
        }
    }

    pub fn key(&self, i: usize) -> Option<Key> {
        self.keys.get(i).and_then(|n| Key::from_name(n))
    }

    pub fn reset_keys(&mut self) {
        self.keys = default_keys();
    }

    /// Aktuelle Belegung eines Hotkeys.
    pub fn hotkey(&self, id: &str) -> Option<Combo> {
        match self.hotkeys.get(id) {
            Some(t) => Combo::parse(t),
            None => ACTS.iter().find(|a| a.id == id).and_then(default_combo),
        }
    }
}
