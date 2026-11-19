//! Magie-Regeneration: Magie füllt sich langsam von selbst wieder auf.

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
    if *t >= 90 {
        *t = 0;
        let m = g.read8(MAGIC);
        if m < 0x80 {
            g.write8(MAGIC, m + 1);
        }
    }
}

crate::export_mod!(c"Magie-Regeneration", c"Magie füllt sich langsam von selbst wieder auf.", pre: None, post: Some(post));
