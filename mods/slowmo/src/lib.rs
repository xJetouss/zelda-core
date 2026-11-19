//! Zeitlupe (L halten): Solange L gedrückt ist, läuft das Spiel mit halber Geschwindigkeit.

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

extern "C" fn pre(api: *const Api) {
    let g = game(api);
    if g.pad() & 0x0020 != 0 {
        g.set_speed(0.5);
        // L geht nicht ans Spiel
        g.set_pad(g.pad() & !0x0020);
    }
}

crate::export_mod!(c"Zeitlupe (L halten)", c"Solange L gedrückt ist, läuft das Spiel mit halber Geschwindigkeit.", pre: Some(pre), post: None);
