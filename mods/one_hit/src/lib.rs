//! Ein-Treffer-Modus: Jeder Treffer ist tödlich (beide Spiele).

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
    if !in_game_any(&g) {
        *last = None;
        return;
    }
    let now = hp(&g);
    if let Some(prev) = *last {
        if now < prev && now > 0 {
            kill(&g);
        }
    }
    *last = Some(hp(&g));
}

crate::export_mod!(c"Ein-Treffer-Modus", c"Hardcore: Jeder Treffer ist tödlich (Feen retten weiterhin).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
