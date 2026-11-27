//! Gemeinsame Hilfen für die Mods in diesem Ordner (jede Mod bindet die Datei mit `#[path]` ein).

#![allow(dead_code)]

use crate::api::{Api, Game};

pub const RUPEES: u32 = 0x7EF360;
pub const HP_MAX: u32 = 0x7EF36C;
pub const HP: u32 = 0x7EF36D;
pub const MAGIC: u32 = 0x7EF36E;
pub const BOMBS: u32 = 0x7EF343;
pub const ARROWS: u32 = 0x7EF377;
/// Schaden, den das Spiel als Nächstes von den Herzen abzieht
pub const DAMAGE: u32 = 0x7E0373;
/// Palettenpuffer des Spiels (Quelle der Farben, 256 × BGR555)
pub const PALETTE: u32 = 0x7EC500;

pub fn game<'a>(api: *const Api) -> Game<'a> {
    Game(unsafe { &*api })
}

/// Spielmodus, in dem Werte „echt“ dazukommen (nicht beim Laden eines Spielstands)
pub fn playing(g: &Game) -> bool {
    matches!(g.read8(0x7E0010), 0x07 | 0x09 | 0x0B | 0x0E)
}

/// Farbe aus dem Palettenpuffer des Spiels
pub fn pal(g: &Game, i: u32) -> u16 {
    g.read16(PALETTE + i * 2)
}

/// Farbton einer BGR555-Farbe drehen (Grad), Helligkeit/Sättigung bleiben
pub fn hue(c: u16, deg: f32) -> u16 {
    let (r, g, b) = ((c & 31) as f32 / 31.0, ((c >> 5) & 31) as f32 / 31.0, ((c >> 10) & 31) as f32 / 31.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-4 {
        return c;
    }
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let sector = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let mut h = sector * 60.0;
    h = (h + deg).rem_euclid(360.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let hh = h / 360.0;
    let to = |v: f32| (v.clamp(0.0, 1.0) * 31.0).round() as u16;
    to(f(hh + 1.0 / 3.0)) | to(f(hh)) << 5 | to(f(hh - 1.0 / 3.0)) << 10
}

// ---- beide Spiele (A Link to the Past und The Minish Cap)

use crate::api::minish as mc;

/// Link steht im normalen Spiel (kein Menü, kein Übergang)
pub fn in_game_any(g: &Game) -> bool {
    if g.is_minish() { g.mc_in_game() } else { g.in_game() }
}

/// Werte zählen „echt“ (nicht beim Laden eines Spielstands): im Spiel oder im Menü
pub fn playing_any(g: &Game) -> bool {
    if g.is_minish() { g.mc_in_game() || g.read8(mc::INFO_MENU) != 0 } else { playing(g) }
}

/// Leben (8 = 1 Herz)
pub fn hp(g: &Game) -> u8 {
    g.read8(if g.is_minish() { mc::HEALTH } else { HP })
}
pub fn set_hp(g: &Game, v: u8) {
    g.write8(if g.is_minish() { mc::HEALTH } else { HP }, v)
}
pub fn hp_max(g: &Game) -> u8 {
    g.read8(if g.is_minish() { mc::MAX_HEALTH } else { HP_MAX })
}

/// Link auf dem normalen Weg sterben lassen
pub fn kill(g: &Game) {
    if g.is_minish() {
        g.write8(mc::HEALTH, 0);
    } else {
        // 160 = 20 Herzen, mehr als Link haben kann (0xFF wirkt im Spiel nicht tödlich)
        g.write8(DAMAGE, 0xA0);
    }
}

pub fn rupees(g: &Game) -> u16 {
    g.read16(if g.is_minish() { mc::RUPEES } else { RUPEES })
}
pub fn set_rupees(g: &Game, v: u16) {
    g.write16(if g.is_minish() { mc::RUPEES } else { RUPEES }, v)
}
/// höchstens so viele Rubine (The Minish Cap: je nach Brieftasche)
pub fn rupee_cap(g: &Game) -> u16 {
    if g.is_minish() {
        [100, 300, 500, 999][(g.read8(mc::WALLET) as usize).min(3)]
    } else {
        999
    }
}

/// Mod-Beschreibung und Einstiegspunkt erzeugen
/// (`games`: `api::GAMES_…`, `icon`: PNG-Daten für die Mod-Liste, `settings`: Einstellungen)

/// Einstellung für `settings: &[…]` (Schlüssel, „Deutsch|English“, Art, Standard, min, max)
#[macro_export]
macro_rules! setting {
    ($key:literal, $label:literal, $kind:expr, $def:expr, $min:expr, $max:expr) => {
        api::Setting { key: $key.as_ptr(), label: $label.as_ptr(), kind: $kind, def: $def, min: $min, max: $max }
    };
}
#[macro_export]
macro_rules! export_mod {
    ($name:expr, $desc:expr, pre: $pre:expr, post: $post:expr) => {
        crate::export_mod!($name, $desc, pre: $pre, post: $post, games: api::GAMES_ALTTP, icon: None);
    };
    ($name:expr, $desc:expr, pre: $pre:expr, post: $post:expr, games: $games:expr, icon: $icon:expr) => {
        crate::export_mod!($name, $desc, pre: $pre, post: $post, games: $games, icon: $icon, settings: &[]);
    };
    ($name:expr, $desc:expr, pre: $pre:expr, post: $post:expr, games: $games:expr, icon: $icon:expr, settings: $settings:expr) => {
        static SETTINGS: &[api::Setting] = $settings;
        /// Einstellungen der Mod (Zahnrad in der Mod-Liste)
        #[no_mangle]
        pub extern "C" fn pastlink_mod_settings(n: *mut u32) -> *const api::Setting {
            if !n.is_null() {
                unsafe { *n = SETTINGS.len() as u32 };
            }
            SETTINGS.as_ptr()
        }
        static INFO: api::ModInfo = api::ModInfo {
            // nur A Link to the Past ohne Einstellungen: Version 3 – läuft dann auch mit älteren Fassungen von Zelda
            // Core; mit The Minish Cap 4, mit Einstellungen 5
            abi: if !SETTINGS.is_empty() { api::PASTLINK_MOD_ABI } else if $games == api::GAMES_ALTTP { 3 } else { 4 },
            name: $name.as_ptr(),
            description: $desc.as_ptr(),
            pre_frame: $pre,
            post_frame: $post,
            hooks: std::ptr::null(),
            hook_count: 0,
            on_hook: None,
        };
        /// Einstiegspunkt, den Zelda Core sucht
        #[no_mangle]
        pub extern "C" fn pastlink_mod() -> *const api::ModInfo {
            &INFO
        }
        /// Spiele, in denen die Mod läuft
        #[no_mangle]
        pub extern "C" fn pastlink_mod_games() -> u32 {
            $games
        }
        /// Bild der Mod (PNG) für die Mod-Liste
        #[no_mangle]
        pub extern "C" fn pastlink_mod_icon(len: *mut u32) -> *const u8 {
            let icon: Option<&'static [u8]> = $icon;
            match icon {
                Some(b) => {
                    if !len.is_null() {
                        unsafe { *len = b.len() as u32 };
                    }
                    b.as_ptr()
                }
                None => std::ptr::null(),
            }
        }
    };
}
