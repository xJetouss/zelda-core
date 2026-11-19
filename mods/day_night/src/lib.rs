//! Tag & Nacht: Auf der Oberwelt wird es alle 6 Minuten Abend und Nacht, dann wieder Tag. Oben rechts zeigt eine Uhr die Tageszeit.

#[path = "../../../assets/mod_template/src/api.rs"]
mod api;
#[path = "../../shared.rs"]
mod shared;

#[allow(unused_imports)]
use api::*;
#[allow(unused_imports)]
use shared::*;
#[allow(unused_imports)]
use std::sync::Mutex;

/// Länge eines Tages in Spielbildern (6 Minuten)
const DAY: u64 = 6 * 60 * 60;

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let md = g.read8(0x7E0010);
    if !matches!(md, 0x09 | 0x0B | 0x0E) || g.read8(0x7E001B) != 0 {
        return;
    }
    // Tageszeit 0..1 (0 = Mitternacht, 0.5 = Mittag)
    let t = (g.frame() % DAY) as f32 / DAY as f32;
    let dark = ((t * std::f32::consts::TAU).cos() * 0.5 + 0.5).powf(1.6);
    let a = (dark * 150.0) as u32;
    if a > 2 {
        // nachts bläulich, in der Dämmerung rötlich
        let dusk = (1.0 - (dark - 0.5).abs() * 2.0).max(0.0);
        let r = (0x10 as f32 + dusk * 0x40 as f32) as u32;
        // weit über den Rand hinaus: Breitbild und „Fenster füllen“ zeigen mehr als 256 Pixel (das Bild wird beschnitten)
        g.rect(-2048, -64, 4352, 352, r << 24 | 0x10 << 16 | 0x40 << 8 | a);
    }
    let hours = (t * 24.0) as u32;
    let mins = ((t * 24.0).fract() * 60.0) as u32;
    g.rect(214, 2, 40, 10, 0x00000090);
    g.text(217, 3, &format!("{hours:02}:{mins:02}"), if dark > 0.5 { 0xA0C0FFFF } else { 0xFFE080FF });
}

crate::export_mod!(c"Tag & Nacht", c"Auf der Oberwelt wird es alle 6 Minuten Abend und Nacht, dann wieder Tag.", pre: None, post: Some(post));
