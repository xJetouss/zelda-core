//! Gegner-Lebensbalken: Zeigt über jedem Gegner, wie viel Leben er noch hat.

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

/// höchstes gesehenes Leben je Sprite-Platz (Typ, Maximum)
static MAX: Mutex<[(u8, u8); 16]> = Mutex::new([(0, 0); 16]);
/// Balken des vorigen Bildes (x, y, Breite, Farbe): das angezeigte Bild zeigt die Gegner so, wie sie ein Bild vorher
/// standen (die Figuren gehen erst am Bildende an die Grafik) – sonst laufen die Balken beim Bewegen hinterher
static LAST: Mutex<Vec<(i32, i32, i32, u32)>> = Mutex::new(Vec::new());

extern "C" fn post(api: *const Api) {
    let g = game(api);
    if !matches!(g.read8(0x7E0010), 0x07 | 0x09) {
        LAST.lock().unwrap().clear();
        return;
    }
    let mut now = Vec::new();
    let mut max = MAX.lock().unwrap();
    let cam = (g.read16(0x7E00E2), g.read16(0x7E00E8));
    for i in 0..16u32 {
        let state = g.read8(0x7E0DD0 + i);
        let typ = g.read8(0x7E0E20 + i);
        let hp = g.read8(0x7E0E50 + i);
        if state == 0 || max[i as usize].0 != typ {
            max[i as usize] = (typ, 0);
        }
        // nur aktive Gegner mit echtem Leben (0 = harmlos, 255 = unverwundbar/Huhn)
        if state != 0x09 || hp == 0 || hp == 0xFF {
            continue;
        }
        let m = &mut max[i as usize].1;
        *m = (*m).max(hp);
        let x = (g.read8(0x7E0D10 + i) as u16 | (g.read8(0x7E0D30 + i) as u16) << 8).wrapping_sub(cam.0) as i16 as i32;
        let y = (g.read8(0x7E0D00 + i) as u16 | (g.read8(0x7E0D20 + i) as u16) << 8).wrapping_sub(cam.1) as i16 as i32;
        if !(-80..336).contains(&x) || !(-16..240).contains(&y) {
            continue;
        }
        let frac = hp as f32 / (*m).max(1) as f32;
        let w = (14.0 * frac).round().max(1.0) as i32;
        let col = if frac > 0.6 { 0x40E040FF } else if frac > 0.3 { 0xF0D030FF } else { 0xF04030FF };
        now.push((x, y, w, col));
    }
    let mut last = LAST.lock().unwrap();
    for &(x, y, w, col) in last.iter() {
        g.rect(x, y - 5, 16, 4, 0x000000C0);
        g.rect(x + 1, y - 4, w, 2, col);
    }
    *last = now;
}

crate::export_mod!(c"Gegner-Lebensbalken", c"Zeigt über jedem Gegner, wie viel Leben er noch hat.", pre: None, post: Some(post));
