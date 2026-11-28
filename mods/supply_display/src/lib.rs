//! Vorrats-Anzeige: Rubine, Bomben und Pfeile untereinander in einem Feld; im HUD-Editor verschieb- und skalierbar.

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
    if !playing_any(&g) && !g.hud_edit() {
        return;
    }
    let (b, a) = if g.is_minish() { (g.read8(minish::BOMBS), g.read8(minish::ARROWS)) } else { (g.read8(BOMBS), g.read8(ARROWS)) };
    hud_lines(&g, "pos", "scale", hud_default(&g, 6), &[
        (format!("RUBINE {}", rupees(&g)), HUD_GREEN, 1.0),
        (format!("BOMBEN {b}"), HUD_BLUE, 1.0),
        (format!("PFEILE {a}"), HUD_GOLD, 1.0),
    ]);
}

crate::export_mod!(c"Vorrats-Anzeige", c"Rubine, Bomben und Pfeile untereinander in einem Feld; im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
