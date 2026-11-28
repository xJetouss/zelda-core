//! Speedrun-Timer: Spielzeit des Spielstands (wie in der Spielstand-Liste) mit Zwischenzeiten.

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

struct Run {
    /// eigene Zählung (nur, wenn die App keine Spielzeit liefert)
    frames: u64,
    running: bool,
    marks: (u8, u8),
    /// Spielzeit beim letzten Bild (springt sie zurück, wurde ein anderer Spielstand geladen)
    last: u64,
    splits: Vec<(String, u64)>,
}

static RUN: Mutex<Run> = Mutex::new(Run { frames: 0, running: false, marks: (0, 0), last: 0, splits: Vec::new() });

fn time(f: u64, fps_x100: u64) -> String {
    let cs = f * 10_000 / fps_x100.max(1);
    let (h, m, s, c) = (cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{c:02}")
    } else {
        format!("{m:02}:{s:02}.{c:02}")
    }
}

/// The Minish Cap: Zahl der Elemente (Erde, Feuer, Wasser, Wind = Items 61..64)
fn mc_elements(g: &Game) -> u8 {
    (61u32..=64).filter(|&i| g.read8(minish::SAVE + 0xF2 + i / 4) >> ((i % 4) * 2) & 3 != 0).count() as u8
}

const NAMES: [&str; 4] = ["Erde", "Feuer", "Wasser", "Wind"];

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut r = RUN.lock().unwrap();
    // Spielzeit des Spielstands aus der App (wie in der Spielstand-Liste); ältere App: selbst zählen
    let play = g.app_get(app::PLAY_FRAMES);
    let fps = match g.app_get(app::FPS_X100) {
        app::NONE => 6000,
        v => v.max(1) as u64,
    };
    let (active, now, reset) = if g.is_minish() {
        (playing_any(&g), (mc_elements(&g), 0), hp_max(&g) == 0)
    } else {
        let md = g.read8(0x7E0010);
        (md > 0x05 && md != 0x14, (g.read8(0x7EF374).count_ones() as u8, g.read8(0x7EF37A).count_ones() as u8), md <= 0x05)
    };
    let t = if play != app::NONE { play as u64 } else { r.frames };
    // neuer/anderer Spielstand: Zwischenzeiten neu beginnen
    if reset || (play != app::NONE && t + 60 < r.last) {
        *r = Run { frames: 0, running: false, marks: (0, 0), last: t, splits: Vec::new() };
    }
    r.last = t;
    if active && !reset {
        if !r.running {
            r.running = true;
            r.marks = now;
        }
        if play == app::NONE {
            r.frames += 1;
        }
        if now.0 > r.marks.0 {
            let name = if g.is_minish() { format!("Element {}", NAMES[(now.0 as usize).clamp(1, 4) - 1]) } else { format!("Amulett {}", now.0) };
            r.splits.push((name, t));
        }
        if now.1 > r.marks.1 {
            r.splits.push((format!("Kristall {}", now.1), t));
        }
        r.marks = now;
    }
    if !r.running {
        return;
    }
    draw(&g, t, fps, &r.splits);
}

/// Anzeige im Stil der Spielmenüs: dunkelblaues Feld, weiße und goldene Linie, Zeit groß in der Pixelschrift
fn draw(g: &Game, t: u64, fps: u64, splits: &[(String, u64)]) {
    let k = g.setting_int("scale", 100).clamp(50, 400) as f32 / 100.0;
    let n = if g.setting_bool("splits", true) { splits.len().min(g.setting_int("rows", 3).clamp(1, 8) as usize) } else { 0 };
    let big = 2.0 * k;
    let small = k;
    let main = time(t, fps);
    let rows: Vec<(String, String)> = splits[splits.len() - n..].iter().map(|(name, f)| (name.to_uppercase(), time(*f, fps))).collect();
    // Maße in Spielpixeln (ein Zeichen der Pixelschrift ist 6 breit, 7 hoch)
    let pad = (4.0 * k).round() as i32;
    let main_w = Game::pixel_text_width(&main, big);
    let rows_w = rows.iter().map(|(a, b)| Game::pixel_text_width(a, small) + Game::pixel_text_width(b, small) + (8.0 * k) as i32).max().unwrap_or(0);
    let w = main_w.max(rows_w) + pad * 2;
    let line_h = (9.0 * k).round() as i32;
    // Höhe der großen Zeit: 7 Schriftpixel × Größe, darunter etwas Luft
    let main_h = (7.0 * big + 2.0 * k).round() as i32;
    let h = pad * 2 + main_h + n as i32 * line_h + if n > 0 { pad / 2 } else { 0 };
    let (x, y) = g.setting_pos("pos").unwrap_or((4, if g.is_minish() { 22 } else { 4 }));
    // Feld und Doppellinie (weiß außen, gold innen) wie im Item-Menü
    g.rect(x, y, w, h, 0x081028E0);
    let lw = k.max(1.0);
    g.frame_rect(x, y, w, h, lw, 0xF8F8F8FF);
    let i = (2.0 * k).round().max(2.0) as i32;
    g.frame_rect(x + i, y + i, w - 2 * i, h - 2 * i, lw * 0.6, 0xF8D040C0);
    // Zeit: weiß, mit dunklem Rand
    g.text_ex(x + w / 2, y + pad, &main, 0xFFFFFFFF, big, text::PIXEL | text::OUTLINE | text::CENTER);
    // Zwischenzeiten: Name links (gold), Zeit rechts (hellblau)
    for (j, (name, tm)) in rows.iter().enumerate() {
        let ry = y + pad + main_h + pad / 2 + j as i32 * line_h;
        g.text_ex(x + pad, ry, name, 0xF8D040FF, small, text::PIXEL | text::SHADOW);
        g.text_ex(x + w - pad, ry, tm, 0xA0E0FFFF, small, text::PIXEL | text::SHADOW | text::RIGHT);
    }
}

crate::export_mod!(c"Speedrun-Timer", c"Spielzeit des Spielstands mit Zwischenzeiten (Amulette und Kristalle bzw. die vier Elemente).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::setting!(c"pos", c"Position des Timers|Timer position", setting::POS, setting::UNSET, 0, 0),
    crate::setting!(c"scale", c"Größe (Prozent)|Size (percent)", setting::INT, 100, 50, 300),
    crate::setting!(c"splits", c"Zwischenzeiten zeigen|Show splits", setting::BOOL, 1, 0, 1),
    crate::setting!(c"rows", c"Zwischenzeiten (Anzahl)|Splits (count)", setting::INT, 3, 1, 8),
]);
