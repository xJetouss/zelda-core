//! Doppelter Schaden: Jeder Treffer kostet doppelt so viele Herzen (beide Spiele).

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
    if !playing_any(&g) {
        *last = None;
        return;
    }
    let now = hp(&g);
    if let Some(prev) = *last {
        if now < prev && now > 0 {
            let extra = prev - now;
            if extra >= now {
                // tödlich: auf dem normalen Weg sterben lassen
                kill(&g);
            } else {
                set_hp(&g, now - extra);
            }
        }
    }
    *last = Some(hp(&g));
}

crate::export_mod!(c"Doppelter Schaden", c"Schwerer: Jeder Treffer kostet doppelt so viele Herzen.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
