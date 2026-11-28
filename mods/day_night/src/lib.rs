//! Tag & Nacht: Auf der Oberwelt wird es alle 6 Minuten Abend und Nacht (beide Spiele).

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


extern "C" fn post(api: *const Api) {
    let g = game(api);
    let outside = if g.is_minish() {
        playing_any(&g) && g.read8(minish::INFO_OVERWORLD) != 0 && g.read8(minish::INFO_MENU) == 0
    } else {
        matches!(g.read8(0x7E0010), 0x09 | 0x0B | 0x0E) && g.read8(0x7E001B) == 0
    };
    if !outside {
        return;
    }
    // Länge eines Tages in Spielbildern (Einstellung in Minuten)
    let day = g.setting_int("minutes", 6).clamp(1, 120) as u64 * 60 * 60;
    // Tageszeit 0..1 (0 = Mitternacht, 0.5 = Mittag)
    let t = (g.frame() % day) as f32 / day as f32;
    let dark = ((t * std::f32::consts::TAU).cos() * 0.5 + 0.5).powf(1.6);
    let a = (dark * 150.0) as u32;
    if a > 2 {
        // nachts bläulich, in der Dämmerung rötlich
        let dusk = (1.0 - (dark - 0.5).abs() * 2.0).max(0.0);
        let r = (0x10 as f32 + dusk * 0x40 as f32) as u32;
        // weit über den Rand hinaus: Breitbild und „Fenster füllen“ zeigen mehr als das Spielbild (wird beschnitten)
        g.rect(-2048, -64, 4352, 352, r << 24 | 0x10 << 16 | 0x40 << 8 | a);
    }
    if !g.setting_bool("clock", true) {
        return;
    }
    let hours = (t * 24.0) as u32;
    let mins = ((t * 24.0).fract() * 60.0) as u32;
    let (sw, _) = g.screen();
    // Uhr: vom Spieler gezogen (HUD-Editor), sonst oben rechts (The Minish Cap: unter den Knöpfen)
    let def = (sw - 44, if g.is_minish() { 40 } else { 2 });
    hud_lines(&g, "clock_pos", "clock_scale", def, &[(format!("{hours:02}:{mins:02}"), if dark > 0.5 { HUD_BLUE } else { HUD_GOLD }, 1.0)]);
}

crate::export_mod!(c"Tag & Nacht", c"Auf der Oberwelt wird es Abend und Nacht, dann wieder Tag (Tageslänge einstellbar).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::setting!(c"minutes", c"Länge eines Tages (Minuten)|Length of a day (minutes)", setting::INT, 6, 1, 60),
    crate::setting!(c"clock", c"Uhr zeigen|Show clock", setting::BOOL, 1, 0, 1),
    crate::setting!(c"clock_pos", c"Position der Uhr|Clock position", setting::POS, setting::UNSET, 0, 0),
    crate::setting!(c"clock_scale", c"Größe der Uhr (Prozent)|Clock size (percent)", setting::INT, 100, 50, 400),
]);
