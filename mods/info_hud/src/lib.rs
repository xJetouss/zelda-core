//! Info-Anzeige: Position, Gebiet/Raum und mehr unten am Rand (beide Spiele).

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

static ST: Mutex<(u8, u32)> = Mutex::new((0, 0));

extern "C" fn post(api: *const Api) {
    let g = game(api);
    let (sw, sh) = g.screen();
    if g.is_minish() {
        if !g.mc_in_game() {
            return;
        }
        let (area, room) = g.mc_area_room();
        let x = g.read16(minish::INFO_LINK_X) as i16;
        let y = g.read16(minish::INFO_LINK_Y) as i16;
        let place = if g.read8(minish::INFO_OVERWORLD) != 0 { "Oberwelt" } else { "innen" };
        let txt = format!("X {x:5} Y {y:5}  Gebiet {area:02X} Raum {room:02X} {place}  besiegt {}", g.read32(minish::ENEMIES_KILLED));
        let y = if g.setting_bool("top", false) { 0 } else { sh - 10 };
        g.rect(0, y, sw, 10, 0x000000A0);
        g.text(3, y + 1, &txt, 0xE0E0E0FF);
        return;
    }
    let mut st = ST.lock().unwrap();
    let md = g.read8(0x7E0010);
    // Bildzähler des Spiels ($1A): steht er still, obwohl gespielt wird, war es ein Lag-Bild
    let tick = g.read8(0x7E001A);
    if matches!(md, 0x07 | 0x09) && tick == st.0 {
        st.1 += 1;
    }
    st.0 = tick;
    let indoors = g.read8(0x7E001B) != 0;
    let place = if indoors { format!("Raum {:03X}", g.read16(0x7E00A0)) } else { format!("Gebiet {:02X}", g.read8(0x7E008A)) };
    let enemies = (0..16u32).filter(|&i| g.read8(0x7E0DD0 + i) == 0x09 && !matches!(g.read8(0x7E0E50 + i), 0 | 0xFF)).count();
    let txt = format!("X {:4} Y {:4}  {place}  Gegner {enemies}  Lag {}", g.read16(0x7E0022), g.read16(0x7E0020), st.1);
    let y = if g.setting_bool("top", false) { 0 } else { 214 };
    g.rect(0, y, 256, 10, 0x000000A0);
    g.text(3, y + 1, &txt, 0xE0E0E0FF);
}

crate::export_mod!(c"Info-Anzeige", c"Position, Gebiet/Raum (A Link to the Past: Gegner und Lag-Bilder, The Minish Cap: besiegte Gegner) unten am Rand.", pre: None, post: Some(post), games: api::GAMES_ALTTP | api::GAMES_MINISH, icon: Some(include_bytes!("../icon.png")), settings: &[
    crate::setting!(c"top", c"Oben statt unten|At the top instead of the bottom", setting::BOOL, 0, 0, 1),
]);
