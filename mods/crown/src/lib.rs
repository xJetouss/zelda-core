//! Königskrone: Link trägt eine goldene Krone (Bild aus einer PNG-Datei über dem Spielbild).

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
    if !matches!(g.read8(0x7E0010), 0x07 | 0x09 | 0x0B) {
        return;
    }
    let (x, y) = g.link_on_screen();
    // sanft auf und ab
    let bob = ((g.frame() / 12) % 2) as i32;
    g.image(id, x, y - 10 - bob, 0, 0, 0xFFFFFFFF);
}

crate::export_mod!(c"Königskrone", c"Link trägt eine goldene Krone (Bild aus einer PNG-Datei über dem Spielbild).", pre: None, post: Some(post));
