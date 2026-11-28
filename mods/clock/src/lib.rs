//! Uhr: Die echte Uhrzeit in einem Feld (auf Wunsch mit Sekunden); im HUD-Editor verschieb- und skalierbar.

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
    let t = g.app_get(app::LOCAL_TIME);
    if t == app::NONE {
        return;
    }
    let (h, m, s) = (t / 3600, t / 60 % 60, t % 60);
    let txt = if g.setting_bool("seconds", false) { format!("{h:02}:{m:02}:{s:02}") } else { format!("{h:02}:{m:02}") };
    hud_lines(&g, "pos", "scale", hud_default(&g, 1), &[(txt, HUD_GOLD, 2.0)]);
}

crate::export_mod!(c"Uhr", c"Die echte Uhrzeit in einem Feld (auf Wunsch mit Sekunden); im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
    crate::setting!(c"seconds", c"Mit Sekunden|With seconds", setting::BOOL, 0, 0, 1),
]);
