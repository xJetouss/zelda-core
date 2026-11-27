//! Münz-Klang: Jede Rubine klingt zusätzlich wie eine Münze (beide Spiele).

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

use std::sync::OnceLock;

static LAST: Mutex<Option<u16>> = Mutex::new(None);
static SOUND: OnceLock<Vec<i16>> = OnceLock::new();

/// zwei kurze Töne (H5, E6) mit Ausklang, 32 kHz mono
fn chime() -> Vec<i16> {
    let rate = 32_000.0f32;
    let mut v = Vec::new();
    for (f, len) in [(987.77f32, 0.07f32), (1318.5, 0.35)] {
        let n = (len * rate) as usize;
        for i in 0..n {
            let t = i as f32 / rate;
            let env = (-t * 9.0).exp() * (1.0 - (-t * 400.0).exp());
            let s = (t * f * std::f32::consts::TAU).sin() * 0.6 + (t * f * 2.0 * std::f32::consts::TAU).sin() * 0.15;
            v.push((s * env * 9000.0) as i16);
        }
    }
    v
}

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let mut last = LAST.lock().unwrap();
    if !playing_any(&g) {
        *last = None;
        return;
    }
    let now = rupees(&g);
    if let Some(prev) = *last {
        if now > prev && now - prev <= 300 {
            g.play_sound(SOUND.get_or_init(chime), 1, 32_000, 0.8);
        }
    }
    *last = Some(now);
}

crate::export_mod!(c"Münz-Klang", c"Jede Rubine klingt zusätzlich wie eine Münze (eigener Ton über die Ton-Schnittstelle).", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")));
