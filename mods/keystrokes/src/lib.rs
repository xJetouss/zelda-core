//! Tasten-Anzeige: Zeigt, welche Tasten gerade gedrückt sind (Steuerkreuz, A, B, X, Y, L, R) – gut für Videos; im HUD-Editor verschieb- und skalierbar.

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

static PAD: Mutex<u16> = Mutex::new(0);

extern "C" fn pre(api: *const Api) {
    *PAD.lock().unwrap() = game(api).pad();
}

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let pad = *PAD.lock().unwrap();
    let k = hud_scale(&g, "scale");
    let c = (11.0 * k).round() as i32;
    let gap = (2.0 * k).round().max(1.0) as i32;
    let pad_px = (4.0 * k).round() as i32;
    // Raster 6×3: links Steuerkreuz, rechts L R / Y X / B A
    let w = 6 * c + 5 * gap + 2 * pad_px + c / 2;
    let h = 3 * c + 2 * gap + 2 * pad_px;
    // Standard: unten links
    let (x, y) = g.setting_pos("pos").unwrap_or((4, g.screen().1 - h - 4));
    hud_panel(&g, x, y, w, h, k);
    let cell = |col: f32, row: i32, label: &str, bit: u16| {
        let cx = x + pad_px + (col * (c + gap) as f32) as i32;
        let cy = y + pad_px + row * (c + gap);
        let on = pad & bit != 0;
        g.rect(cx, cy, c, c, if on { 0xF8D040FF } else { 0x18284880 });
        g.frame_rect(cx, cy, c, c, k.max(1.0) * 0.6, if on { 0xFFFFFFFF } else { 0x8090B0C0 });
        g.text_ex(cx + c / 2, cy + (c - (7.0 * k) as i32) / 2, label, if on { 0x101018FF } else { 0xE0E8F8FF }, k, text::PIXEL | text::CENTER);
    };
    cell(1.0, 0, "▲", 0x0800);
    cell(0.0, 1, "◀", 0x0200);
    cell(2.0, 1, "▶", 0x0100);
    cell(1.0, 2, "▼", 0x0400);
    cell(3.5, 0, "L", 0x0020);
    cell(4.5, 0, "R", 0x0010);
    cell(3.5, 1, "Y", 0x4000);
    cell(4.5, 1, "X", 0x0040);
    cell(3.5, 2, "B", 0x8000);
    cell(4.5, 2, "A", 0x0080);
    g.hud_box("pos", "scale", x, y, w, h);
}

crate::export_mod!(c"Tasten-Anzeige", c"Zeigt, welche Tasten gerade gedrückt sind (Steuerkreuz, A, B, X, Y, L, R) – gut für Videos; im HUD-Editor verschieb- und skalierbar.", pre: Some(pre), post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::hud_settings!(),
    crate::hud_settings!(scale),
]);
