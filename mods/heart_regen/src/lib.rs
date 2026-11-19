//! Herz-Regeneration: Im Spiel alle 8 Sekunden ein halbes Herz zurück.

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

static T: Mutex<u32> = Mutex::new(0);

extern "C" fn post(api: *const Api) {
    let g = game(api);
    if !g.in_game() {
        return;
    }
    let mut t = T.lock().unwrap();
    *t += 1;
    if *t >= 480 {
        *t = 0;
        let (hp, max) = (g.read8(HP), g.read8(HP_MAX));
        if hp > 0 && hp < max {
            g.write8(HP, (hp + 4).min(max));
        }
    }
}

crate::export_mod!(c"Herz-Regeneration", c"Im Spiel alle 8 Sekunden ein halbes Herz zurück.", pre: None, post: Some(post));
