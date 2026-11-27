//! Königskrone: Link trägt eine goldene Krone (beide Spiele).

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

use std::sync::atomic::{AtomicU32, Ordering};

static PNG: &[u8] = include_bytes!("../crown.png");
static ID: AtomicU32 = AtomicU32::new(0);

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut id = ID.load(Ordering::Relaxed);
    if id == 0 {
        id = g.image_png(PNG);
        if id == 0 {
            g.log("Krone: Bild konnte nicht geladen werden");
            return;
        }
        ID.store(id, Ordering::Relaxed);
    }
    // sanft auf und ab
    let bob = if g.setting_bool("bob", true) { ((g.frame() / 12) % 2) as i32 } else { 0 };
    let lift = g.setting_int("lift", 0);
    if g.is_minish() {
        if !g.mc_in_game() {
            return;
        }
        // Mitte der Füße -> über dem Kopf (Link ist etwa 20 Pixel groß)
        let (x, y) = g.mc_link_on_screen();
        g.image(id, x - 8, y - 30 - bob - lift, 0, 0, 0xFFFFFFFF);
        return;
    }
    if !matches!(g.read8(0x7E0010), 0x07 | 0x09 | 0x0B) {
        return;
    }
    let (x, y) = g.link_on_screen();
    g.image(id, x, y - 10 - bob - lift, 0, 0, 0xFFFFFFFF);
}

crate::export_mod!(c"Königskrone", c"Link trägt eine goldene Krone (Bild aus einer PNG-Datei über dem Spielbild).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::setting!(c"bob", c"Auf und ab wippen|Bob up and down", setting::BOOL, 1, 0, 1),
    crate::setting!(c"lift", c"Höher / tiefer (Pixel)|Higher / lower (pixels)", setting::INT, 0, -10, 20),
]);
