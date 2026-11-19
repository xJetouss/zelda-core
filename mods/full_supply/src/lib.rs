//! Voller Vorrat: Bomben, Pfeile und Magie bleiben immer voll (sobald du sie hast).

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
    if !g.in_game() {
        return;
    }
    // Obergrenzen mit Upgrades (Feen-Teich): Bomben 10 + $F370, Pfeile 30 + $F371
    let bomb_cap = (10 + g.read8(0x7EF370) as u32).min(99) as u8;
    let arrow_cap = (30 + g.read8(0x7EF371) as u32).min(99) as u8;
    if g.read8(BOMBS) > 0 && g.read8(BOMBS) < bomb_cap {
        g.write8(BOMBS, bomb_cap);
    }
    if g.read8(0x7EF340) != 0 && g.read8(ARROWS) < arrow_cap {
        g.write8(ARROWS, arrow_cap);
    }
    if g.read8(MAGIC) < 0x80 {
        g.write8(MAGIC, 0x80);
    }
}

crate::export_mod!(c"Voller Vorrat", c"Bomben, Pfeile und Magie bleiben immer voll (sobald du sie hast).", pre: None, post: Some(post));
