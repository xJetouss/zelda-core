//! Beispiel-Mod für Zelda Core (läuft im Recompiled-Modus „RC“):
//! - zeigt oben rechts, wie oft das Spiel seinen NMI-Handler betreten hat (Hook-Beispiel),
//! - im normalen Spiel alle 2 Sekunden eine Rubine,
//! - L+R gedrückt: Magie füllt sich und das Spiel läuft in Zeitlupe.
//!
//! Bauen: `cargo build --release`, dann `target/release/mein_mod.dll` in den Ordner `mods` neben
//! `zelda-core.exe` kopieren, im Menü „Mods“ → „Mods neu laden“ und die Mod einschalten.

mod api;

use api::{Api, Game, ModInfo, PASTLINK_MOD_ABI};
use std::sync::atomic::{AtomicU32, Ordering};

// Wichtige RAM-Adressen (siehe README)
const RUPEES: u32 = 0x7EF360; // Rubinen (16 Bit)
const MAGIC: u32 = 0x7EF36E; // Magie (0..=0x80)

/// Hook-Adressen: hier der NMI-Handler (Adresse aus der ROM des Spiels; siehe README „Hooks finden“)
static HOOKS: [u32; 1] = [0x0080C9];
static NMI: AtomicU32 = AtomicU32::new(0);

extern "C" fn on_hook(_api: *const Api, _addr: u32) {
    NMI.fetch_add(1, Ordering::Relaxed);
}

extern "C" fn pre_frame(api: *const Api) {
    let g = Game(unsafe { &*api });
    if g.pad() & 0x0030 == 0x0030 {
        g.set_speed(0.5);
    }
}

extern "C" fn post_frame(api: *const Api) {
    let g = Game(unsafe { &*api });
    g.text(178, 2, &format!("NMI {}", NMI.load(Ordering::Relaxed)), 0xFFFF80FF);
    if !g.in_game() {
        return;
    }
    if g.frame() % 120 == 0 {
        let r = g.read16(RUPEES);
        g.write16(RUPEES, (r + 1).min(999));
    }
    if g.pad() & 0x0030 == 0x0030 {
        let m = g.read8(MAGIC);
        g.write8(MAGIC, (m + 1).min(0x80));
    }
}

static INFO: ModInfo = ModInfo {
    abi: PASTLINK_MOD_ABI,
    name: c"Beispiel-Mod".as_ptr(),
    description: c"NMI-Zähler (Hook), Rubine alle 2 s, L+R: Magie + Zeitlupe.".as_ptr(),
    pre_frame: Some(pre_frame),
    post_frame: Some(post_frame),
    hooks: HOOKS.as_ptr(),
    hook_count: HOOKS.len() as u32,
    on_hook: Some(on_hook),
};

/// Einstiegspunkt, den Zelda Core sucht
#[no_mangle]
pub extern "C" fn pastlink_mod() -> *const ModInfo {
    &INFO
}
