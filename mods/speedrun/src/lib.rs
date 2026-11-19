//! Speedrun-Timer: Spielzeit oben links, mit Zwischenzeiten für jedes Amulett und jeden Kristall.

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

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut r = RUN.lock().unwrap();
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
    if !r.running {
        return;
    }
    let n = r.splits.len().min(3);
    g.rect(2, 2, 82, 11 + n as i32 * 9, 0x00000090);
    g.text(5, 3, &time(r.frames), 0xFFFFFFFF);
    for (k, (name, f)) in r.splits[r.splits.len() - n..].iter().enumerate() {
        g.text(5, 13 + k as i32 * 9, &format!("{name} {}", time(*f)), 0xA0E0FFFF);
    }
}

crate::export_mod!(c"Speedrun-Timer", c"Spielzeit oben links, mit Zwischenzeiten für jedes Amulett und jeden Kristall.", pre: None, post: Some(post));
