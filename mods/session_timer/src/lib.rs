//! Sitzungszeit: Wie lange die App schon läuft (diese Sitzung); im HUD-Editor verschieb- und skalierbar.

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
    let f = g.app_get(app::SESSION_FRAMES);
    let fps = g.app_get(app::UI_FPS_X100).max(100);
    static ACC: Mutex<(f64, i64)> = Mutex::new((0.0, 0));
    // Sekunden aus den Bildern der App (Bildrate schwankt: Schritt für Schritt aufsummieren)
    let mut a = ACC.lock().unwrap();
    if f != app::NONE && f > a.1 {
        a.0 += (f - a.1) as f64 * 100.0 / fps as f64;
        a.1 = f;
    }
    let s = a.0 as u64;
    hud_lines(&g, "pos", "scale", hud_default(&g, 4), &[("SITZUNG".into(), HUD_GOLD, 1.0), (format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), HUD_WHITE, 1.0)]);
}

crate::export_mod!(c"Sitzungszeit", c"Wie lange die App schon läuft (diese Sitzung); im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
