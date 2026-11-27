//! Doppelte Rubine: Jede eingesammelte Rubine zählt doppelt (beide Spiele).

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

static LAST: Mutex<Option<u16>> = Mutex::new(None);

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut last = LAST.lock().unwrap();
    if !playing_any(&g) {
        *last = None;
        return;
    }
    let now = rupees(&g);
    if let Some(prev) = *last {
        if now > prev && now - prev <= 300 {
            let v = (now + (now - prev)).min(rupee_cap(&g));
            set_rupees(&g, v);
            *last = Some(v);
            return;
        }
    }
    *last = Some(now);
}

crate::export_mod!(c"Doppelte Rubine", c"Jede eingesammelte Rubine zählt doppelt.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
