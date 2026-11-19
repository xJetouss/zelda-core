//! Doppelter Schaden: Schwerer: Jeder Treffer kostet doppelt so viele Herzen.

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
    if !playing(&g) {
        *last = None;
        return;
    }
    let hp = g.read8(HP);
    if let Some(prev) = *last {
        if hp < prev && hp > 0 {
            let extra = prev - hp;
            if extra >= hp {
                // tödlich: auf dem normalen Weg sterben lassen
                g.write8(DAMAGE, 0xA0);
            } else {
                g.write8(HP, hp - extra);
            }
        }
    }
    *last = Some(g.read8(HP));
}

crate::export_mod!(c"Doppelter Schaden", c"Schwerer: Jeder Treffer kostet doppelt so viele Herzen.", pre: None, post: Some(post));
