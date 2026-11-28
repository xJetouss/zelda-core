//! FPS-Anzeige: Bilder pro Sekunde des Spiels (und auf Wunsch der Anzeige) in einem Feld; im HUD-Editor verschieb- und skalierbar.

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
    let fps = g.app_get(app::GAME_FPS_X100);
    if fps == app::NONE {
        return;
    }
    let mut lines = vec![(format!("{} FPS", (fps + 50) / 100), HUD_WHITE, 1.0)];
    if g.setting_bool("ui", false) {
        lines.push((format!("ANZEIGE {}", (g.app_get(app::UI_FPS_X100) + 50) / 100), HUD_BLUE, 1.0));
    }
    hud_lines(&g, "pos", "scale", hud_default(&g, 0), &lines);
}

crate::export_mod!(c"FPS-Anzeige", c"Bilder pro Sekunde des Spiels (und auf Wunsch der Anzeige) in einem Feld; im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
    crate::setting!(c"ui", c"Auch Bilder der Anzeige|Also display frames", setting::BOOL, 0, 0, 1),
]);
