//! Todeszähler: Wie oft Link in diesem Spielstand gestorben ist; im HUD-Editor verschieb- und skalierbar.

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
    let d = g.app_get(app::DEATHS);
    if d == app::NONE {
        return;
    }
    hud_lines(&g, "pos", "scale", hud_default(&g, 3), &[("TODE".into(), HUD_GOLD, 1.0), (d.to_string(), HUD_RED, 2.0)]);
}

crate::export_mod!(c"Todeszähler", c"Wie oft Link in diesem Spielstand gestorben ist; im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
