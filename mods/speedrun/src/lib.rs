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
    let n = if g.setting_bool("splits", true) { r.splits.len().min(3) } else { 0 };
    // Position: vom Spieler gezogen, sonst oben links (The Minish Cap: unter den Herzen)
    let (x0, y0) = g.setting_pos("pos").unwrap_or((2, if g.is_minish() { 22 } else { 2 }));
    g.rect(x0, y0, 86, 11 + n as i32 * 9, 0x00000090);
    g.text(x0 + 3, y0 + 1, &time(t, fps), 0xFFFFFFFF);
    for (k, (name, f)) in r.splits[r.splits.len() - n..].iter().enumerate() {
        g.text(x0 + 3, y0 + 11 + k as i32 * 9, &format!("{name} {}", time(*f, fps)), 0xA0E0FFFF);
    }
}

crate::export_mod!(c"Speedrun-Timer", c"Spielzeit des Spielstands mit Zwischenzeiten (Amulette und Kristalle bzw. die vier Elemente).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::setting!(c"pos", c"Position des Timers|Timer position", setting::POS, setting::UNSET, 0, 0),
    crate::setting!(c"splits", c"Zwischenzeiten zeigen|Show splits", setting::BOOL, 1, 0, 1),
]);
