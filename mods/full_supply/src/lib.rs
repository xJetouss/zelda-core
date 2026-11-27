//! Voller Vorrat: Bomben, Pfeile (und in A Link to the Past Magie) bleiben voll.

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
    if !in_game_any(&g) {
        return;
    }
    if g.is_minish() {
        // Obergrenzen je nach Bombentasche / Köcher
        let bomb_cap = [10u8, 30, 50, 99][(g.read8(minish::BOMB_BAG) as usize).min(3)];
        let arrow_cap = [30u8, 50, 70, 99][(g.read8(minish::QUIVER) as usize).min(3)];
        if g.read8(minish::BOMBS) > 0 && g.read8(minish::BOMBS) < bomb_cap {
            g.write8(minish::BOMBS, bomb_cap);
        }
        if g.read8(minish::ARROWS) > 0 && g.read8(minish::ARROWS) < arrow_cap {
            g.write8(minish::ARROWS, arrow_cap);
        }
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

crate::export_mod!(c"Voller Vorrat", c"Bomben und Pfeile (in A Link to the Past auch Magie) bleiben immer voll, sobald du sie hast.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
