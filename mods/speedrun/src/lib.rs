//! Speedrun-Timer: Spielzeit oben links mit Zwischenzeiten (Amulette/Kristalle bzw. Elemente).

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
    frames: u64,
    running: bool,
    marks: (u8, u8),
    splits: Vec<(String, u64)>,
}

static RUN: Mutex<Run> = Mutex::new(Run { frames: 0, running: false, marks: (0, 0), splits: Vec::new() });

fn time(f: u64) -> String {
    let cs = f * 100 / 60;
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
    if g.is_minish() {
        // neue Datei (noch kein Herz): zurücksetzen; gezählt wird im Spiel und im Menü
        if hp_max(&g) == 0 {
            *r = Run { frames: 0, running: false, marks: (0, 0), splits: Vec::new() };
        } else if playing_any(&g) {
            let now = (mc_elements(&g), 0);
            if !r.running {
                r.running = true;
                r.marks = now;
            }
            r.frames += 1;
            let f = r.frames;
            if now.0 > r.marks.0 {
                r.splits.push((format!("Element {}", NAMES[(now.0 as usize).clamp(1, 4) - 1]), f));
            }
            r.marks = now;
        }
    } else {
        let md = g.read8(0x7E0010);
        // Spielerauswahl / neue Datei: Timer zurücksetzen
        if md <= 0x05 {
            *r = Run { frames: 0, running: false, marks: (0, 0), splits: Vec::new() };
        } else if md != 0x14 {
            let now = (g.read8(0x7EF374).count_ones() as u8, g.read8(0x7EF37A).count_ones() as u8);
            if !r.running {
                r.running = true;
                r.marks = now;
            }
            r.frames += 1;
            let f = r.frames;
            if now.0 > r.marks.0 {
                r.splits.push((format!("Amulett {}", now.0), f));
            }
            if now.1 > r.marks.1 {
                r.splits.push((format!("Kristall {}", now.1), f));
            }
            r.marks = now;
        }
    }
    if !r.running {
        return;
    }
    let n = r.splits.len().min(3);
    // The Minish Cap: unter den Herzen (die stehen oben links)
    let y0 = if g.is_minish() { 22 } else { 2 };
    g.rect(2, y0, 86, 11 + n as i32 * 9, 0x00000090);
    g.text(5, y0 + 1, &time(r.frames), 0xFFFFFFFF);
    for (k, (name, f)) in r.splits[r.splits.len() - n..].iter().enumerate() {
        g.text(5, y0 + 11 + k as i32 * 9, &format!("{name} {}", time(*f)), 0xA0E0FFFF);
    }
}

crate::export_mod!(c"Speedrun-Timer", c"Spielzeit oben links, mit Zwischenzeiten (Amulette und Kristalle bzw. die vier Elemente).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
