//! Koordinaten: Links Position in der Welt (X/Y) in einem Feld; im HUD-Editor verschieb- und skalierbar.

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
    let (x, y) = if g.is_minish() {
        if !g.mc_in_game() && !g.hud_edit() {
            return;
        }
        (g.read16(minish::INFO_LINK_X) as i16 as i32, g.read16(minish::INFO_LINK_Y) as i16 as i32)
    } else {
        if !playing(&g) && !g.hud_edit() {
            return;
        }
        (g.read16(0x7E0022) as i32, g.read16(0x7E0020) as i32)
    };
    hud_lines(&g, "pos", "scale", hud_default(&g, 2), &[(format!("X {x}"), HUD_WHITE, 1.0), (format!("Y {y}"), HUD_WHITE, 1.0)]);
}

crate::export_mod!(c"Koordinaten", c"Links Position in der Welt (X/Y) in einem Feld; im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
