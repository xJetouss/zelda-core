//! Ein-Treffer-Modus: Hardcore: Jeder Treffer ist tödlich (Feen retten weiterhin).

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

static LAST: Mutex<Option<u8>> = Mutex::new(None);

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut last = LAST.lock().unwrap();
    if !g.in_game() {
        *last = None;
        return;
    }
    let hp = g.read8(HP);
    if let Some(prev) = *last {
        if hp < prev && hp > 0 {
            // 160 = 20 Herzen, mehr als Link haben kann (0xFF wirkt im Spiel nicht tödlich)
            g.write8(DAMAGE, 0xA0);
        }
    }
    *last = Some(hp);
}

crate::export_mod!(c"Ein-Treffer-Modus", c"Hardcore: Jeder Treffer ist tödlich (Feen retten weiterhin).", pre: None, post: Some(post));
