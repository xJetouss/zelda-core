//! Bunte Gegner: Alle Gegner und Figuren schillern in wechselnden Farben (Sprite-Paletten 1–6).

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

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let base = (g.frame() % 360) as f32 * 2.0;
    let mut cols = [0u16; 96];
    for (k, c) in cols.iter_mut().enumerate() {
        let i = 0x90 + k as u32;
        let src = pal(&g, i);
        // Farbe 0 jeder Reihe ist durchsichtig
        *c = if k % 16 == 0 { src } else { hue(src, base + (k / 16) as f32 * 60.0) };
    }
    g.cgram_patch(0x90, &cols);
}

crate::export_mod!(c"Bunte Gegner", c"Alle Gegner und Figuren schillern in wechselnden Farben (Sprite-Paletten 1–6).", pre: None, post: Some(post));
