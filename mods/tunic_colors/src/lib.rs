//! Regenbogen-Tunika: Links Tunika wechselt fließend durch alle Farben (Sprite-Palette).

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
    // Links Sprite-Palette: Reihe 7 (Farben 0xF0..0xFF); grüne Töne sind die Tunika
    let deg = (g.frame() % 600) as f32 * 0.6;
    let mut cols = [0u16; 16];
    for (k, c) in cols.iter_mut().enumerate() {
        let src = pal(&g, 0xF0 + k as u32);
        let (r, gr, b) = (src & 31, (src >> 5) & 31, (src >> 10) & 31);
        *c = if k > 0 && gr > r && gr > b { hue(src, deg) } else { src };
    }
    g.cgram_patch(0xF0, &cols);
}

crate::export_mod!(c"Regenbogen-Tunika", c"Links Tunika wechselt fließend durch alle Farben (Sprite-Palette).", pre: None, post: Some(post));
