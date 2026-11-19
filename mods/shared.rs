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

/// Mod-Beschreibung und Einstiegspunkt erzeugen
#[macro_export]
macro_rules! export_mod {
    ($name:expr, $desc:expr, pre: $pre:expr, post: $post:expr) => {
        static INFO: api::ModInfo = api::ModInfo {
            abi: api::PASTLINK_MOD_ABI,
            name: $name.as_ptr(),
            description: $desc.as_ptr(),
            pre_frame: $pre,
            post_frame: $post,
            hooks: std::ptr::null(),
            hook_count: 0,
            on_hook: None,
        };
        /// Einstiegspunkt, den pastlink sucht
        #[no_mangle]
        pub extern "C" fn pastlink_mod() -> *const api::ModInfo {
            &INFO
        }
    };
}
