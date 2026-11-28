//! Zelda Core Mod-Schnittstelle (C-ABI, Version 8). Mods laufen in A Link to the Past im Recompiled-Modus (RC)
//! und – ab Version 4 – in The Minish Cap (PC-Fassung, `tmc_core.dll`).
//!
//! Diese Datei ist in der App und in jeder Mod dieselbe. Nicht ändern: Die App prüft `PASTLINK_MOD_ABI`.
//!
//! Eine Mod ist eine DLL (Rust: `crate-type = ["cdylib"]`), die `pastlink_mod() -> *const ModInfo`
//! exportiert. Was eine Mod kann:
//! - pro Spielbild `pre_frame` (vorher, z. B. Eingaben ändern) und `post_frame` (danach),
//! - **Hooks**: `on_hook` läuft, bevor das Spiel eine Routine an einer der Adressen in `hooks` betritt
//!   (nur im eigenen Recompiler, nicht im Zelda-3-Modus). Die Mod kann dort Register und Speicher ändern
//!   oder die Routine ganz überspringen (`ret`).
//! - jeden Speicher lesen/schreiben: WRAM, SRAM, VRAM, Paletten (CGRAM), Sprites (OAM), die ROM im Speicher,
//! - **Grafik ändern**: Kacheln (Sprites, Hintergründe) und Farben dauerhaft ersetzen (`vram_patch`,
//!   `cgram_patch`), auch aus PNG-Bildern (`png_tiles`),
//! - **Bilder** über dem Spielbild zeichnen (PNG oder RGBA), dazu Rechtecke und Text,
//! - **Töne** abspielen (PCM, z. B. aus einer WAV-Datei, siehe `wav_pcm`),
//! - das Spieltempo ändern.
//!
//! **Spiele (ab Version 4):** Eine Mod sagt mit dem Export `pastlink_mod_games() -> u32`, für welche Spiele sie
//! gedacht ist (`GAMES_ALTTP`, `GAMES_MINISH`, beide verodert). Ohne diesen Export gilt sie nur für A Link to the
//! Past. Welches Spiel gerade läuft, steht in `Api::game`. In The Minish Cap sind die Adressen GBA-Adressen
//! (EWRAM `0x02000000..`, IWRAM `0x03000000..`, Spielstand `gSave` ab `0x02002A40`), gezeichnet wird in
//! GBA-Pixeln (`Api::screen_w` × `Api::screen_h`, im Breitbild breiter als 240). Hooks, Register, ROM und
//! `vram_patch`/`cgram_patch` gibt es dort nicht.
//!
//! **Icon (ab Version 4):** Mit dem Export `pastlink_mod_icon(len: *mut u32) -> *const u8` (PNG-Daten, am
//! besten 32×32) zeigt die Mod-Liste ein eigenes Bild.
//!
//! **Einstellungen (ab Version 5):** Mit dem Export `pastlink_mod_settings(n: *mut u32) -> *const Setting` zeigt
//! die Mod-Liste bei der Mod ein Zahnrad. Dort stellt der Spieler Schalter, Zahlen und Positionen ein (eine
//! Position zieht er mit der Maus direkt auf dem Spielbild an ihren Platz). Die Mod liest die Werte mit
//! `Game::setting` / `Game::setting_pos`; die App speichert sie.
//!
//! **Launcher (ab Version 6):** Eine Mod liest und ändert Werte der App (`app_get` / `app_set`, Schlüssel in
//! `app::…`, z. B. Spielzeit des Spielstands, Lautstärke, Tempo, Breitbild, Pause) und löst Aktionen aus
//! (`app_do`: Spielstand speichern/laden, Screenshot, Karte …). Änderungen gelten nach dem laufenden Bild.
//!
//! **Zeichnen und Eingaben (ab Version 7):** Text in der Pixelschrift des Spiels mit Größe, Ausrichtung und Rand
//! (`text_ex`), Linien und Rahmen; Maus (Position im Spielbild, Tasten) und Tastatur.
//!
//! **HUD-Editor (ab Version 8):** Eine Mod mit eigener Anzeige meldet jedes Bild deren Fläche mit `hud_box(key,
//! scale_key, x, y, w, h)` – `key` ist ihre Positions-Einstellung (`setting::POS`), `scale_key` ihre Größe in Prozent
//! (`setting::INT`, leer = keine). Im HUD-Editor („Mods“ → „Anzeigen verschieben“) zieht der Spieler die Anzeige mit
//! der Maus an ihren Platz, ändert an der Ecke unten rechts die Größe, öffnet mit dem Stift die Einstellungen und
//! schaltet sie mit dem X aus. Die Mod zeichnet ihre Anzeige mit der linken oberen Ecke an `setting_pos(key)`.

#![allow(dead_code)]

use std::ffi::{c_char, c_void};

/// Versionsnummer der Schnittstelle
pub const PASTLINK_MOD_ABI: u32 = 8;

/// Schalter für `text_ex`
pub mod text {
    /// Pixelschrift wie im Spiel (sonst die Schrift der App)
    pub const PIXEL: u32 = 1;
    /// dunkler Rand um jeden Buchstaben (gut lesbar auf jedem Hintergrund)
    pub const OUTLINE: u32 = 2;
    /// Schatten unten rechts
    pub const SHADOW: u32 = 4;
    /// waagerecht mittig bzw. rechtsbündig zu `x`
    pub const CENTER: u32 = 8;
    pub const RIGHT: u32 = 16;
}

/// `Api::game`: A Link to the Past
pub const GAME_ALTTP: u32 = 0;
/// `Api::game`: The Minish Cap
pub const GAME_MINISH: u32 = 1;
/// `pastlink_mod_games()`: läuft in A Link to the Past
pub const GAMES_ALTTP: u32 = 1;
/// `pastlink_mod_games()`: läuft in The Minish Cap
pub const GAMES_MINISH: u32 = 2;

/// Speicherbereiche für `mem_read` / `mem_write`
pub mod space {
    /// SNES-Adressen: WRAM `0x7E0000..=0x7FFFFF`, SRAM `0x700000..=0x70FFFF`
    /// (The Minish Cap: GBA-Adressen EWRAM `0x02000000..`, IWRAM `0x03000000..`)
    pub const BUS: u32 = 0;
    /// Grafikspeicher (Byte-Adresse 0..0x10000)
    pub const VRAM: u32 = 1;
    /// Paletten (Byte-Adresse 0..0x200, je Farbe 2 Byte BGR555)
    pub const CGRAM: u32 = 2;
    /// Sprite-Tabelle (0..0x220)
    pub const OAM: u32 = 3;
    /// ROM-Abbild im Speicher (Dateioffset ohne Header)
    pub const ROM: u32 = 4;
}

/// Werte und Aktionen der App (Launcher) für `app_get` / `app_set` / `app_do`
pub mod app {
    /// unbekannter Schlüssel bzw. ältere App
    pub const NONE: i64 = i64::MIN;
    // ---- lesen
    /// Spielzeit des laufenden Spielstands in Bildern (wie in der Spielstand-Liste; zählt nur, solange gespielt wird)
    pub const PLAY_FRAMES: &str = "play_frames";
    /// Bilder pro Sekunde des Spiels × 100 (A Link to the Past ≈ 6010, The Minish Cap ≈ 5973)
    pub const FPS_X100: &str = "fps_x100";
    /// Tode im laufenden Spielstand
    pub const DEATHS: &str = "deaths";
    /// Bilder seit dem Start der App (Sitzung)
    pub const SESSION_FRAMES: &str = "session_frames";
    // ---- lesen und ändern
    /// Lautstärke 0..100
    pub const VOLUME: &str = "volume";
    /// Ton aus (0/1)
    pub const MUTE: &str = "mute";
    /// Spieltempo in Prozent (100 = normal)
    pub const SPEED: &str = "speed";
    /// Breitbild: 0 aus, 1 16:9, 2 21:9, 3 Fenster füllen
    pub const WIDESCREEN: &str = "widescreen";
    /// Pause (0/1)
    pub const PAUSED: &str = "paused";
    /// Tempo beim Vorspulen (Turbo) als Faktor
    pub const TURBO: &str = "turbo_speed";
    // ---- Aktionen (`app_do`)
    /// Spielstand speichern (Wert = Platz 1..10)
    pub const SAVE_STATE: &str = "save_state";
    /// Spielstand laden (Wert = Platz 1..10)
    pub const LOAD_STATE: &str = "load_state";
    /// Bildschirmfoto
    pub const SCREENSHOT: &str = "screenshot";
    /// Karte öffnen/schließen
    pub const MAP: &str = "map";
    /// Spielstand-Menü öffnen/schließen
    pub const STATES_MENU: &str = "states_menu";
    // ---- weitere Werte (lesen)
    /// Bilder pro Sekunde der Anzeige (App) × 100
    pub const UI_FPS_X100: &str = "ui_fps_x100";
    /// gemessene Spielbilder pro Sekunde × 100
    pub const GAME_FPS_X100: &str = "game_fps_x100";
    /// Ortszeit: Sekunden seit Mitternacht
    pub const LOCAL_TIME: &str = "local_time";
    /// HUD-Editor gerade offen (0/1) – Anzeigen sollten dann immer sichtbar sein
    pub const HUD_EDIT: &str = "hud_edit";
    /// Tode des Spielstands auf 0
    pub const RESET_DEATHS: &str = "reset_deaths";
}

/// Arten von Einstellungen (`Setting::kind`)
pub mod setting {
    /// Schalter (0/1)
    pub const BOOL: u32 = 0;
    /// Zahl von `min` bis `max`
    pub const INT: u32 = 1;
    /// Position auf dem Spielbild (zum Ziehen); Wert: x in den unteren, y in den oberen 16 Bit (je mit Vorzeichen).
    /// `def` = `UNSET`: die Mod nimmt ihren eigenen Platz, bis der Spieler einen wählt
    pub const POS: u32 = 2;
    /// noch nicht gesetzt (Position) bzw. unbekannter Schlüssel
    pub const UNSET: i32 = i32::MIN;

    /// Position in einen Wert packen
    pub const fn pos(x: i32, y: i32) -> i32 {
        ((x as i16 as u16 as u32) | ((y as i16 as u16 as u32) << 16)) as i32
    }
}

/// Eine Einstellung der Mod (statisch, lebt so lange wie die DLL)
#[repr(C)]
pub struct Setting {
    /// Schlüssel (ASCII, mit 0 abgeschlossen), z. B. `c"pos"`
    pub key: *const c_char,
    /// Beschriftung, Deutsch und Englisch mit `|` getrennt, z. B. `c"Position|Position"`
    pub label: *const c_char,
    /// `setting::BOOL`, `INT` oder `POS`
    pub kind: u32,
    /// Standardwert
    pub def: i32,
    /// kleinster / größter Wert (nur `INT`)
    pub min: i32,
    pub max: i32,
}

unsafe impl Sync for Setting {}

/// The Minish Cap: wichtige Adressen (GBA) und Zusatz-Infos der App
pub mod minish {
    /// Spielstand im Speicher (`gSave`)
    pub const SAVE: u32 = 0x0200_2A40;
    /// Spielerwerte (`gSave.stats`)
    pub const STATS: u32 = SAVE + 0xA8;
    /// Herzteile (0..3)
    pub const HEART_PIECES: u32 = STATS + 0x01;
    /// Leben (8 = 1 Herz, 0 = tot)
    pub const HEALTH: u32 = STATS + 0x02;
    /// höchstes Leben (8 je Herz)
    pub const MAX_HEALTH: u32 = STATS + 0x03;
    pub const BOMBS: u32 = STATS + 0x04;
    pub const ARROWS: u32 = STATS + 0x05;
    /// Bombentasche (0 = keine) / Köcher (0 = keiner)
    pub const BOMB_BAG: u32 = STATS + 0x06;
    pub const QUIVER: u32 = STATS + 0x07;
    /// ausgerüstet: A, dann B
    pub const EQUIPPED: u32 = STATS + 0x0C;
    /// Rubine (16 Bit)
    pub const RUPEES: u32 = STATS + 0x18;
    /// Muscheln (16 Bit)
    pub const SHELLS: u32 = STATS + 0x1A;
    /// Brieftasche (0..3: 100, 300, 500, 999 Rubine)
    pub const WALLET: u32 = STATS + 0x00;
    /// besiegte Gegner (32 Bit)
    pub const ENEMIES_KILLED: u32 = SAVE + 0x50;

    /// Zusatz-Infos der App (nur lesen): ab dieser Adresse
    pub const INFO: u32 = 0x0F00_0000;
    /// 1 = Link steht im normalen Spiel (kein Menü, keine Zwischensequenz mit eigener Anzeige)
    pub const INFO_IN_GAME: u32 = INFO;
    /// 1 = Item-/Pausenmenü offen
    pub const INFO_MENU: u32 = INFO + 0x01;
    /// Gebiet und Raum
    pub const INFO_AREA: u32 = INFO + 0x02;
    pub const INFO_ROOM: u32 = INFO + 0x03;
    /// 1 = Oberwelt
    pub const INFO_OVERWORLD: u32 = INFO + 0x04;
    /// Links Position in der Welt (16 Bit mit Vorzeichen)
    pub const INFO_LINK_X: u32 = INFO + 0x06;
    pub const INFO_LINK_Y: u32 = INFO + 0x08;
    /// Links Position auf dem Bild (Mitte seiner Füße, Zeichen-Pixel)
    pub const INFO_LINK_SX: u32 = INFO + 0x0A;
    pub const INFO_LINK_SY: u32 = INFO + 0x0C;
    /// Kamera (linke obere Ecke in der Welt)
    pub const INFO_CAM_X: u32 = INFO + 0x0E;
    pub const INFO_CAM_Y: u32 = INFO + 0x10;
}

/// CPU-Register für `reg` / `set_reg`
pub mod reg {
    pub const A: u32 = 0;
    pub const X: u32 = 1;
    pub const Y: u32 = 2;
    pub const SP: u32 = 3;
    pub const DP: u32 = 4;
    pub const DB: u32 = 5;
    pub const PB: u32 = 6;
    pub const PC: u32 = 7;
    /// Statusregister (N V M X D I Z C)
    pub const P: u32 = 8;
}

/// Was die App einer Mod anbietet
#[repr(C)]
pub struct Api {
    /// = `PASTLINK_MOD_ABI`
    pub abi: u32,
    /// intern (an alle Funktionen durchreichen)
    pub ctx: *mut c_void,
    /// Spielbild-Zähler
    pub frame: u64,
    /// Byte lesen (SNES-Adresse: WRAM/SRAM)
    pub read8: extern "C" fn(ctx: *mut c_void, addr: u32) -> u8,
    /// Byte schreiben (SNES-Adresse: WRAM/SRAM)
    pub write8: extern "C" fn(ctx: *mut c_void, addr: u32, value: u8),
    /// Controller 1 (B=0x8000 Y=0x4000 Select=0x2000 Start=0x1000 ↑=0x0800 ↓=0x0400 ←=0x0200 →=0x0100
    /// A=0x0080 X=0x0040 L=0x0020 R=0x0010)
    pub pad: extern "C" fn(ctx: *mut c_void) -> u16,
    /// Controller 1 für dieses Bild ersetzen (in `pre_frame`)
    pub set_pad: extern "C" fn(ctx: *mut c_void, pad: u16),
    /// Speicher lesen (`space::…`)
    pub mem_read: extern "C" fn(ctx: *mut c_void, space: u32, addr: u32) -> u8,
    /// Speicher schreiben (`space::…`)
    pub mem_write: extern "C" fn(ctx: *mut c_void, space: u32, addr: u32, value: u8),
    /// CPU-Register lesen (`reg::…`)
    pub reg: extern "C" fn(ctx: *mut c_void, r: u32) -> u32,
    /// CPU-Register setzen (`reg::…`) – vor allem in `on_hook`
    pub set_reg: extern "C" fn(ctx: *mut c_void, r: u32, value: u32),
    /// in `on_hook`: Routine sofort verlassen (0 = RTS, 1 = RTL)
    pub ret: extern "C" fn(ctx: *mut c_void, long: u32),
    /// Rechteck über dem Spielbild (SNES-Pixel 256×224, Farbe 0xRRGGBBAA), gilt für dieses Bild
    pub draw_rect: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, rgba: u32),
    /// Text über dem Spielbild (SNES-Pixel, UTF-8 mit 0 abgeschlossen)
    pub draw_text: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, text: *const c_char, rgba: u32),
    /// Spieltempo für dieses Bild (1.0 = normal, 0.5 = Zeitlupe, 2.0 = doppelt)
    pub set_speed: extern "C" fn(ctx: *mut c_void, speed: f32),

    // ---- ab Version 3
    /// Ton abspielen: `frames` Abtastwerte je Kanal (16 Bit, `channels` = 1 oder 2, verschachtelt), beliebige
    /// Abtastrate; `volume` 1.0 = normal. Mehrere Töne laufen gemischt nebeneinander.
    pub play_sound: extern "C" fn(ctx: *mut c_void, samples: *const i16, frames: u32, channels: u32, rate: u32, volume: f32),
    /// Bild aus PNG-Daten anmelden; Rückgabe: Bild-Nummer (0 = Fehler). Einmal anmelden, dann jedes Bild zeichnen.
    pub image_png: extern "C" fn(ctx: *mut c_void, png: *const u8, len: u32) -> u32,
    /// Bild aus RGBA-Pixeln (w×h×4 Byte) anmelden; Rückgabe: Bild-Nummer (0 = Fehler)
    pub image_rgba: extern "C" fn(ctx: *mut c_void, rgba: *const u8, w: u32, h: u32) -> u32,
    /// angemeldetes Bild zeichnen (SNES-Pixel; `w`/`h` = 0: Originalgröße), `tint` 0xFFFFFFFF = unverändert
    pub draw_image: extern "C" fn(ctx: *mut c_void, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32),
    /// Grafikspeicher dauerhaft überschreiben (Wort-Adresse 0..0x8000), bis `patch_clear` oder Mod aus.
    /// Wird jedes Bild vor dem Zeichnen angewendet – so lassen sich Sprites und Kacheln austauschen.
    pub vram_patch: extern "C" fn(ctx: *mut c_void, word_addr: u32, data: *const u16, words: u32),
    /// Farben dauerhaft überschreiben (Index 0..256, BGR555), bis `patch_clear` oder Mod aus
    pub cgram_patch: extern "C" fn(ctx: *mut c_void, index: u32, colors: *const u16, count: u32),
    /// alle eigenen Grafik-/Farb-Änderungen dieser Mod entfernen
    pub patch_clear: extern "C" fn(ctx: *mut c_void),
    /// PNG in SNES-Kacheln umwandeln (`bpp` = 2 oder 4; Bildgröße Vielfache von 8; durchsichtig = Farbe 0).
    /// Schreibt die Kacheln (Zeile für Zeile, je Kachel 8·bpp Wörter) nach `out` und die Palette (bis zu
    /// 2^bpp Farben BGR555) nach `pal`. Rückgabe: geschriebene Wörter (0 = Fehler).
    pub png_tiles: extern "C" fn(ctx: *mut c_void, png: *const u8, len: u32, bpp: u32, out: *mut u16, max_words: u32, pal: *mut u16) -> u32,
    /// Meldung in der App anzeigen (UTF-8, mit 0 abgeschlossen)
    pub log: extern "C" fn(ctx: *mut c_void, text: *const c_char),

    // ---- ab Version 4 (bei `abi` < 4 nicht vorhanden)
    /// laufendes Spiel: `GAME_ALTTP` oder `GAME_MINISH`
    pub game: u32,
    /// Größe des Spielbilds in Pixeln, in denen gezeichnet wird (A Link to the Past 256×224, The Minish Cap
    /// 240×160 bzw. breiter im Breitbild)
    pub screen_w: u32,
    pub screen_h: u32,

    // ---- ab Version 5
    /// Wert einer Einstellung (`setting::UNSET`, wenn unbekannt bzw. Position noch nicht gewählt)
    pub setting: extern "C" fn(ctx: *mut c_void, key: *const c_char) -> i32,

    // ---- ab Version 6
    /// Wert der App lesen (`app::…`; `app::NONE`, wenn unbekannt)
    pub app_get: extern "C" fn(ctx: *mut c_void, key: *const c_char) -> i64,
    /// Wert der App ändern (gilt nach diesem Bild)
    pub app_set: extern "C" fn(ctx: *mut c_void, key: *const c_char, value: i64),
    /// Aktion der App auslösen (`app::SAVE_STATE` …; gilt nach diesem Bild)
    pub app_do: extern "C" fn(ctx: *mut c_void, action: *const c_char, value: i64),

    // ---- ab Version 7
    /// Text mit Größe (`scale` 1.0 = normal) und Schaltern (`text::…`)
    pub draw_text_ex: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, text: *const c_char, rgba: u32, scale: f32, flags: u32),
    /// Linie (Breite in Spielpixeln)
    pub draw_line: extern "C" fn(ctx: *mut c_void, x0: i32, y0: i32, x1: i32, y1: i32, width: f32, rgba: u32),
    /// Rahmen um ein Rechteck (Breite in Spielpixeln)
    pub draw_frame: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, width: f32, rgba: u32),
    /// Maus: schreibt `[x, y, Tasten (1 = links, 2 = rechts), über dem Spielbild (0/1)]` nach `out`
    pub mouse: extern "C" fn(ctx: *mut c_void, out: *mut i32),
    /// Taste gedrückt? Name wie in den Tastenkürzeln, z. B. `c"F5"`, `c"Space"`, `c"A"`
    pub key_down: extern "C" fn(ctx: *mut c_void, name: *const c_char) -> u32,

    // ---- ab Version 8
    /// Fläche der eigenen Anzeige für den HUD-Editor (jedes Bild melden, in dem sie gezeichnet wird)
    pub hud_box: extern "C" fn(ctx: *mut c_void, key: *const c_char, scale_key: *const c_char, x: i32, y: i32, w: i32, h: i32),
}

/// Beschreibung einer Mod (statisch, lebt so lange wie die DLL)
#[repr(C)]
pub struct ModInfo {
    /// = `PASTLINK_MOD_ABI`
    pub abi: u32,
    /// Name (UTF-8, mit 0 abgeschlossen)
    pub name: *const c_char,
    /// kurze Beschreibung (UTF-8, mit 0 abgeschlossen)
    pub description: *const c_char,
    /// vor jedem Spielbild
    pub pre_frame: Option<extern "C" fn(api: *const Api)>,
    /// nach jedem Spielbild
    pub post_frame: Option<extern "C" fn(api: *const Api)>,
    /// Adressen (24 Bit, z. B. 0x0DDE5A) für `on_hook`
    pub hooks: *const u32,
    pub hook_count: u32,
    /// das Spiel betritt gleich die Routine an `addr`
    pub on_hook: Option<extern "C" fn(api: *const Api, addr: u32)>,
}

// Zeiger auf statische Daten dürfen zwischen Threads geteilt werden
unsafe impl Sync for ModInfo {}

/// Bequemer Zugriff auf die `Api` für Rust-Mods
pub struct Game<'a>(pub &'a Api);

impl Game<'_> {
    pub fn frame(&self) -> u64 {
        self.0.frame
    }
    /// laufendes Spiel (`GAME_ALTTP` oder `GAME_MINISH`; ältere App: immer A Link to the Past)
    pub fn game(&self) -> u32 {
        if self.0.abi >= 4 { self.0.game } else { GAME_ALTTP }
    }
    pub fn is_minish(&self) -> bool {
        self.game() == GAME_MINISH
    }
    /// Größe des Spielbilds in Zeichen-Pixeln
    pub fn screen(&self) -> (i32, i32) {
        if self.0.abi >= 4 { (self.0.screen_w as i32, self.0.screen_h as i32) } else { (256, 224) }
    }
    /// Wert einer Einstellung (`pastlink_mod_settings`); ältere App: `setting::UNSET`
    pub fn setting(&self, key: &str) -> i32 {
        if self.0.abi < 5 {
            return setting::UNSET;
        }
        let b = cstr(key);
        (self.0.setting)(self.0.ctx, b.as_ptr() as *const c_char)
    }
    /// Wert der App (Launcher) lesen, z. B. `app::PLAY_FRAMES`; ältere App: `app::NONE`
    pub fn app_get(&self, key: &str) -> i64 {
        if self.0.abi < 6 {
            return app::NONE;
        }
        let b = cstr(key);
        (self.0.app_get)(self.0.ctx, b.as_ptr() as *const c_char)
    }
    /// Wert der App ändern, z. B. `app_set(app::VOLUME, 50)`
    pub fn app_set(&self, key: &str, value: i64) {
        if self.0.abi >= 6 {
            let b = cstr(key);
            (self.0.app_set)(self.0.ctx, b.as_ptr() as *const c_char, value)
        }
    }
    /// Aktion der App, z. B. `app_do(app::SAVE_STATE, 3)`
    pub fn app_do(&self, action: &str, value: i64) {
        if self.0.abi >= 6 {
            let b = cstr(action);
            (self.0.app_do)(self.0.ctx, b.as_ptr() as *const c_char, value)
        }
    }
    /// Text mit Größe und Schaltern (`text::PIXEL | text::OUTLINE …`); ältere App: normaler Text
    pub fn text_ex(&self, x: i32, y: i32, s: &str, rgba: u32, scale: f32, flags: u32) {
        if self.0.abi < 7 {
            return self.text(x, y, s, rgba);
        }
        let b = cstr(s);
        (self.0.draw_text_ex)(self.0.ctx, x, y, b.as_ptr() as *const c_char, rgba, scale, flags)
    }
    /// Linie
    pub fn line(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: f32, rgba: u32) {
        if self.0.abi >= 7 {
            (self.0.draw_line)(self.0.ctx, x0, y0, x1, y1, width, rgba)
        }
    }
    /// Rahmen
    pub fn frame_rect(&self, x: i32, y: i32, w: i32, h: i32, width: f32, rgba: u32) {
        if self.0.abi >= 7 {
            (self.0.draw_frame)(self.0.ctx, x, y, w, h, width, rgba)
        }
    }
    /// Maus: (x, y in Spielpixeln, Tasten, über dem Spielbild)
    pub fn mouse(&self) -> (i32, i32, u32, bool) {
        if self.0.abi < 7 {
            return (0, 0, 0, false);
        }
        let mut o = [0i32; 4];
        (self.0.mouse)(self.0.ctx, o.as_mut_ptr());
        (o[0], o[1], o[2] as u32, o[3] != 0)
    }
    /// Taste der Tastatur gedrückt (Name wie in den Tastenkürzeln)
    pub fn key_down(&self, name: &str) -> bool {
        if self.0.abi < 7 {
            return false;
        }
        let b = cstr(name);
        (self.0.key_down)(self.0.ctx, b.as_ptr() as *const c_char) != 0
    }
    /// Fläche der eigenen Anzeige für den HUD-Editor melden (`key`: Positions-Einstellung, `scale_key`: Größe in
    /// Prozent oder `""`)
    pub fn hud_box(&self, key: &str, scale_key: &str, x: i32, y: i32, w: i32, h: i32) {
        if self.0.abi >= 8 {
            let (k, s) = (cstr(key), cstr(scale_key));
            (self.0.hud_box)(self.0.ctx, k.as_ptr() as *const c_char, s.as_ptr() as *const c_char, x, y, w, h)
        }
    }
    /// HUD-Editor offen?
    pub fn hud_edit(&self) -> bool {
        self.app_get(app::HUD_EDIT) == 1
    }
    /// Breite eines Texts in der Pixelschrift (Spielpixel)
    pub fn pixel_text_width(s: &str, scale: f32) -> i32 {
        (s.chars().count() as f32 * 6.0 * scale).round() as i32
    }
    /// Schalter-Einstellung (sonst `def`)
    pub fn setting_bool(&self, key: &str, def: bool) -> bool {
        match self.setting(key) {
            setting::UNSET => def,
            v => v != 0,
        }
    }
    /// Zahl-Einstellung (sonst `def`)
    pub fn setting_int(&self, key: &str, def: i32) -> i32 {
        match self.setting(key) {
            setting::UNSET => def,
            v => v,
        }
    }
    /// Position-Einstellung; `None`, solange der Spieler keine gewählt hat
    pub fn setting_pos(&self, key: &str) -> Option<(i32, i32)> {
        match self.setting(key) {
            setting::UNSET => None,
            v => Some((v as u32 as u16 as i16 as i32, (v as u32 >> 16) as u16 as i16 as i32)),
        }
    }
    pub fn read32(&self, addr: u32) -> u32 {
        self.read16(addr) as u32 | (self.read16(addr + 2) as u32) << 16
    }
    pub fn read8(&self, addr: u32) -> u8 {
        (self.0.read8)(self.0.ctx, addr)
    }
    pub fn read16(&self, addr: u32) -> u16 {
        self.read8(addr) as u16 | (self.read8(addr + 1) as u16) << 8
    }
    pub fn write8(&self, addr: u32, v: u8) {
        (self.0.write8)(self.0.ctx, addr, v)
    }
    pub fn write16(&self, addr: u32, v: u16) {
        self.write8(addr, v as u8);
        self.write8(addr + 1, (v >> 8) as u8);
    }
    pub fn mem_read(&self, space: u32, addr: u32) -> u8 {
        (self.0.mem_read)(self.0.ctx, space, addr)
    }
    pub fn mem_write(&self, space: u32, addr: u32, v: u8) {
        (self.0.mem_write)(self.0.ctx, space, addr, v)
    }
    pub fn reg(&self, r: u32) -> u32 {
        (self.0.reg)(self.0.ctx, r)
    }
    pub fn set_reg(&self, r: u32, v: u32) {
        (self.0.set_reg)(self.0.ctx, r, v)
    }
    /// in `on_hook`: Routine überspringen
    pub fn ret(&self, long: bool) {
        (self.0.ret)(self.0.ctx, long as u32)
    }
    pub fn pad(&self) -> u16 {
        (self.0.pad)(self.0.ctx)
    }
    pub fn set_pad(&self, pad: u16) {
        (self.0.set_pad)(self.0.ctx, pad)
    }
    pub fn rect(&self, x: i32, y: i32, w: i32, h: i32, rgba: u32) {
        (self.0.draw_rect)(self.0.ctx, x, y, w, h, rgba)
    }
    pub fn text(&self, x: i32, y: i32, text: &str, rgba: u32) {
        let b = cstr(text);
        (self.0.draw_text)(self.0.ctx, x, y, b.as_ptr() as *const c_char, rgba)
    }
    pub fn set_speed(&self, s: f32) {
        (self.0.set_speed)(self.0.ctx, s)
    }
    /// Link steht im normalen Spiel (Oberwelt oder Dungeon, kein Menü, kein Übergang) – nur A Link to the Past
    pub fn in_game(&self) -> bool {
        matches!(self.read8(0x7E0010), 0x07 | 0x09 | 0x0B) && self.read8(0x7E0011) == 0
    }
    /// The Minish Cap: Link steht im normalen Spiel (kein Menü)
    pub fn mc_in_game(&self) -> bool {
        self.is_minish() && self.read8(minish::INFO_IN_GAME) != 0
    }
    /// The Minish Cap: Links Position auf dem Bild (Mitte seiner Füße, Zeichen-Pixel)
    pub fn mc_link_on_screen(&self) -> (i32, i32) {
        (self.read16(minish::INFO_LINK_SX) as i16 as i32, self.read16(minish::INFO_LINK_SY) as i16 as i32)
    }
    /// The Minish Cap: (Gebiet, Raum)
    pub fn mc_area_room(&self) -> (u8, u8) {
        (self.read8(minish::INFO_AREA), self.read8(minish::INFO_ROOM))
    }
    /// Links Position auf dem Bildschirm (SNES-Pixel, linke obere Ecke seines 16×16-Körpers)
    pub fn link_on_screen(&self) -> (i32, i32) {
        let x = self.read16(0x7E0022).wrapping_sub(self.read16(0x7E00E2)) as i16 as i32;
        let y = self.read16(0x7E0020).wrapping_sub(self.read16(0x7E00E8)) as i16 as i32;
        (x, y)
    }

    // ---- Version 3

    /// Ton abspielen (16-Bit-PCM, verschachtelt bei Stereo)
    pub fn play_sound(&self, samples: &[i16], channels: u32, rate: u32, volume: f32) {
        let ch = channels.clamp(1, 2);
        (self.0.play_sound)(self.0.ctx, samples.as_ptr(), samples.len() as u32 / ch, ch, rate, volume)
    }
    /// WAV-Datei (16-Bit-PCM) abspielen
    pub fn play_wav(&self, wav: &[u8], volume: f32) -> bool {
        match wav_pcm(wav) {
            Some((s, ch, rate)) => {
                self.play_sound(&s, ch, rate, volume);
                true
            }
            None => false,
        }
    }
    /// PNG anmelden (einmal), Rückgabe Bild-Nummer (0 = Fehler)
    pub fn image_png(&self, png: &[u8]) -> u32 {
        (self.0.image_png)(self.0.ctx, png.as_ptr(), png.len() as u32)
    }
    /// RGBA-Bild anmelden (einmal), Rückgabe Bild-Nummer (0 = Fehler)
    pub fn image_rgba(&self, rgba: &[u8], w: u32, h: u32) -> u32 {
        if rgba.len() < (w * h * 4) as usize {
            return 0;
        }
        (self.0.image_rgba)(self.0.ctx, rgba.as_ptr(), w, h)
    }
    /// Bild zeichnen (`w`/`h` = 0: Originalgröße)
    pub fn image(&self, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32) {
        (self.0.draw_image)(self.0.ctx, id, x, y, w, h, tint)
    }
    /// Grafikspeicher dauerhaft ersetzen (Wort-Adresse)
    pub fn vram_patch(&self, word_addr: u32, data: &[u16]) {
        (self.0.vram_patch)(self.0.ctx, word_addr, data.as_ptr(), data.len() as u32)
    }
    /// Farben dauerhaft ersetzen (Index 0..256, BGR555)
    pub fn cgram_patch(&self, index: u32, colors: &[u16]) {
        (self.0.cgram_patch)(self.0.ctx, index, colors.as_ptr(), colors.len() as u32)
    }
    /// eigene Grafik-/Farb-Änderungen entfernen
    pub fn patch_clear(&self) {
        (self.0.patch_clear)(self.0.ctx)
    }
    /// PNG -> SNES-Kacheln (bpp 2 oder 4) und Palette
    pub fn png_tiles(&self, png: &[u8], bpp: u32) -> Option<(Vec<u16>, Vec<u16>)> {
        let mut out = vec![0u16; 0x8000];
        let mut pal = vec![0u16; 16];
        let n = (self.0.png_tiles)(self.0.ctx, png.as_ptr(), png.len() as u32, bpp, out.as_mut_ptr(), out.len() as u32, pal.as_mut_ptr());
        if n == 0 {
            return None;
        }
        out.truncate(n as usize);
        pal.truncate(1 << bpp.min(4));
        Some((out, pal))
    }
    /// Meldung in der App anzeigen
    pub fn log(&self, text: &str) {
        let b = cstr(text);
        (self.0.log)(self.0.ctx, b.as_ptr() as *const c_char)
    }
}

/// Farbe 0xRRGGBB -> SNES BGR555
pub const fn bgr555(rgb: u32) -> u16 {
    let r = (rgb >> 19) & 31;
    let g = (rgb >> 11) & 31;
    let b = (rgb >> 3) & 31;
    (r | g << 5 | b << 10) as u16
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.retain(|&c| c != 0);
    b.push(0);
    b
}

/// WAV (RIFF, 16-Bit-PCM) lesen: (Abtastwerte, Kanäle, Abtastrate)
pub fn wav_pcm(wav: &[u8]) -> Option<(Vec<i16>, u32, u32)> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let (mut p, mut ch, mut rate, mut bits) = (12usize, 0u32, 0u32, 0u32);
    while p + 8 <= wav.len() {
        let id = &wav[p..p + 4];
        let len = u32::from_le_bytes(wav[p + 4..p + 8].try_into().ok()?) as usize;
        let body = wav.get(p + 8..(p + 8 + len).min(wav.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            ch = u16::from_le_bytes([body[2], body[3]]) as u32;
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]) as u32;
        } else if id == b"data" && bits == 16 && ch >= 1 {
            let s = body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            return Some((s, ch, rate));
        }
        p += 8 + len + (len & 1);
    }
    None
}
