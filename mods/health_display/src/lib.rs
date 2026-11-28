//! Lebensanzeige: Leben als Zahl (Herzen von Herzen), bei wenig Leben rot; im HUD-Editor verschieb- und skalierbar.

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
    let (hp, max) = (hp(&g) as u32, hp_max(&g) as u32);
    if max == 0 {
        return;
    }
    let half = if hp % 8 >= 4 { ",5" } else { "" };
    let col = if hp * 4 <= max { HUD_RED } else { HUD_GREEN };
    hud_lines(&g, "pos", "scale", hud_default(&g, 5), &[("LEBEN".into(), HUD_GOLD, 1.0), (format!("{}{half}/{}", hp / 8, max / 8), col, 2.0)]);
}

crate::export_mod!(c"Lebensanzeige", c"Leben als Zahl (Herzen von Herzen), bei wenig Leben rot; im HUD-Editor verschieb- und skalierbar.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
