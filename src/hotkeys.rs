//! Frei belegbare Hotkeys für alle Funktionen (mit Strg/Umschalt/Alt-Kombinationen).

use crate::gfx::{Key, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    OpenRom,
    Reset,
    Quit,
    SaveState,
    LoadState,
    SlotPrev,
    SlotNext,
    Slot1,
    Slot2,
    Slot3,
    Slot4,
    Slot5,
    Slot6,
    Slot7,
    Slot8,
    Slot9,
    Screenshot,
    Pause,
    FrameAdvance,
    Turbo,
    Rewind,
    SpeedUp,
    SpeedDown,
    SpeedReset,
    Runahead,
    Double,
    QuickMenu,
    WinStates,
    WinMap,
    WinRando,
    WinCheats,
    WinControls,
    WinHotkeys,
    Fullscreen,
    ToggleMenu,
    ToggleHd,
    NextFilter,
    ToggleFps,
    FrameBlend,
    ColorRando,
    Mute,
    VolUp,
    VolDown,
    Echo,
    Noclip,
    InfHealth,
    InfMagic,
    AllItems,
    RandoToggle,
    RandoNew,
}

pub struct ActInfo {
    pub act: Act,
    pub id: &'static str,
    pub de: &'static str,
    pub en: &'static str,
    pub key: Option<Key>,
    /// Bit 0 = Strg, Bit 1 = Umschalt, Bit 2 = Alt
    pub mods: u8,
    pub hold: bool,
    pub group: usize,
}

pub const GROUPS: [(&str, &str); 6] = [
    ("Allgemein", "General"),
    ("Spielstände", "Save states"),
    ("Emulation", "Emulation"),
    ("Ansicht & Fenster", "View & windows"),
    ("Audio", "Audio"),
    ("Cheats & Randomizer", "Cheats & randomizer"),
];

macro_rules! a {
    ($act:ident, $id:literal, $de:literal, $en:literal, $key:expr, $mods:literal, $hold:literal, $g:literal) => {
        ActInfo { act: Act::$act, id: $id, de: $de, en: $en, key: $key, mods: $mods, hold: $hold, group: $g }
    };
}

pub const ACTS: &[ActInfo] = &[
    a!(OpenRom, "open", "ROM öffnen", "Open ROM", Some(Key::O), 1, false, 0),
    a!(Reset, "reset", "Reset", "Reset", Some(Key::R), 1, false, 0),
    a!(Quit, "quit", "Beenden", "Quit", Some(Key::Q), 1, false, 0),
    a!(SaveState, "save", "Savestate speichern", "Save state", Some(Key::F5), 0, false, 1),
    a!(LoadState, "load", "Savestate laden", "Load state", Some(Key::F8), 0, false, 1),
    a!(SlotPrev, "slot_prev", "Vorheriger Slot", "Previous slot", Some(Key::F6), 0, false, 1),
    a!(SlotNext, "slot_next", "Nächster Slot", "Next slot", Some(Key::F7), 0, false, 1),
    a!(Slot1, "slot1", "Slot 1", "Slot 1", Some(Key::Num1), 1, false, 1),
    a!(Slot2, "slot2", "Slot 2", "Slot 2", Some(Key::Num2), 1, false, 1),
    a!(Slot3, "slot3", "Slot 3", "Slot 3", Some(Key::Num3), 1, false, 1),
    a!(Slot4, "slot4", "Slot 4", "Slot 4", Some(Key::Num4), 1, false, 1),
    a!(Slot5, "slot5", "Slot 5", "Slot 5", Some(Key::Num5), 1, false, 1),
    a!(Slot6, "slot6", "Slot 6", "Slot 6", Some(Key::Num6), 1, false, 1),
    a!(Slot7, "slot7", "Slot 7", "Slot 7", Some(Key::Num7), 1, false, 1),
    a!(Slot8, "slot8", "Slot 8", "Slot 8", Some(Key::Num8), 1, false, 1),
    a!(Slot9, "slot9", "Slot 9", "Slot 9", Some(Key::Num9), 1, false, 1),
    a!(Screenshot, "screenshot", "Screenshot", "Screenshot", Some(Key::F12), 0, false, 1),
    a!(Pause, "pause", "Pause", "Pause", Some(Key::P), 0, false, 2),
    a!(FrameAdvance, "frame", "Einzelbild vor", "Frame advance", Some(Key::N), 0, false, 2),
    a!(Double, "double", "Doppelte Geschwindigkeit ×2 (halten)", "Double speed ×2 (hold)", Some(Key::Space), 0, true, 2),
    a!(QuickMenu, "quickmenu", "Schnellmenü / Item-Rad (halten)", "Quick menu / item wheel (hold)", Some(Key::V), 0, true, 2),
    a!(Turbo, "turbo", "Turbo (halten)", "Fast-forward (hold)", Some(Key::Tab), 0, true, 2),
    a!(Rewind, "rewind", "Zurückspulen (halten)", "Rewind (hold)", Some(Key::Backspace), 0, true, 2),
    a!(SpeedUp, "speed_up", "Schneller", "Speed up", Some(Key::Plus), 0, false, 2),
    a!(SpeedDown, "speed_down", "Langsamer", "Slow down", Some(Key::Minus), 0, false, 2),
    a!(SpeedReset, "speed_reset", "Normale Geschwindigkeit", "Normal speed", Some(Key::Num0), 1, false, 2),
    a!(Runahead, "runahead", "Run-Ahead umschalten", "Cycle run-ahead", Some(Key::U), 1, false, 2),
    a!(WinStates, "win_states", "Spielstände (mit Bild)", "Save states (with picture)", Some(Key::F2), 0, false, 3),
    a!(WinCheats, "win_cheats", "Cheats", "Cheats", Some(Key::F3), 0, false, 3),
    a!(WinRando, "win_rando", "Randomizer", "Randomizer", Some(Key::F4), 0, false, 3),
    a!(WinMap, "win_map", "Karte", "Map", Some(Key::M), 0, false, 3),
    a!(WinControls, "win_controls", "Tastenbelegung", "Controls", Some(Key::S), 4, false, 3),
    a!(WinHotkeys, "win_hotkeys", "Hotkeys", "Hotkeys", Some(Key::H), 4, false, 3),
    a!(Fullscreen, "fullscreen", "Vollbild", "Fullscreen", Some(Key::F11), 0, false, 3),
    a!(ToggleMenu, "menu", "Menüleiste ein/aus", "Toggle menu bar", Some(Key::Escape), 0, false, 3),
    a!(ToggleHd, "hd", "HD an/aus", "HD on/off", Some(Key::F9), 0, false, 3),
    a!(NextFilter, "filter", "Nächster Bildfilter", "Next video filter", Some(Key::F9), 2, false, 3),
    a!(ToggleFps, "fps", "FPS-Anzeige", "Show FPS", Some(Key::F), 1, false, 3),
    a!(FrameBlend, "blend", "Frame-Interpolation", "Frame interpolation", Some(Key::B), 1, false, 3),
    a!(ColorRando, "color_rando", "Farb-Randomizer", "Color randomizer", Some(Key::K), 1, false, 3),
    a!(Mute, "mute", "Stumm", "Mute", Some(Key::M), 1, false, 4),
    a!(VolUp, "vol_up", "Lauter", "Volume up", Some(Key::ArrowUp), 1, false, 4),
    a!(VolDown, "vol_down", "Leiser", "Volume down", Some(Key::ArrowDown), 1, false, 4),
    a!(Echo, "echo", "Echo an/aus", "Echo on/off", Some(Key::E), 1, false, 4),
    a!(Noclip, "noclip", "Noclip (durch Wände)", "Noclip (walk through walls)", Some(Key::N), 1, false, 5),
    a!(InfHealth, "inf_health", "Unendlich Herzen", "Infinite health", Some(Key::H), 1, false, 5),
    a!(InfMagic, "inf_magic", "Unendlich Magie", "Infinite magic", Some(Key::J), 1, false, 5),
    a!(AllItems, "all_items", "Alle Items geben", "Give all items", Some(Key::I), 1, false, 5),
    a!(RandoToggle, "rando_toggle", "Randomizer an/aus", "Randomizer on/off", Some(Key::R), 3, false, 5),
    a!(RandoNew, "rando_new", "Randomizer neu würfeln", "Re-roll randomizer", Some(Key::R), 5, false, 5),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Combo {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Combo {
    pub fn new(key: Key, mods: u8) -> Self {
        Combo { key, ctrl: mods & 1 != 0, shift: mods & 2 != 0, alt: mods & 4 != 0 }
    }
    pub fn from_event(key: Key, m: Modifiers) -> Self {
        Combo { key, ctrl: m.ctrl || m.command, shift: m.shift, alt: m.alt }
    }
    pub fn has_mods(&self) -> bool {
        self.ctrl || self.shift || self.alt
    }
    pub fn to_text(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(self.key.name());
        s
    }
    pub fn parse(t: &str) -> Option<Combo> {
        let t = t.trim();
        if t.is_empty() {
            return None;
        }
        let (mods, key) = if t == "+" {
            ("", "+")
        } else if let Some(m) = t.strip_suffix("++") {
            (m, "+")
        } else {
            match t.rfind('+') {
                Some(i) => (&t[..i], &t[i + 1..]),
                None => ("", t),
            }
        };
        let key = Key::from_name(key)?;
        let mut c = Combo { key, ctrl: false, shift: false, alt: false };
        for m in mods.split('+').filter(|m| !m.is_empty()) {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "strg" => c.ctrl = true,
                "shift" => c.shift = true,
                "alt" => c.alt = true,
                _ => return None,
            }
        }
        Some(c)
    }
}

pub fn default_combo(info: &ActInfo) -> Option<Combo> {
    info.key.map(|k| Combo::new(k, info.mods))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combo_roundtrip() {
        for a in ACTS {
            if let Some(c) = default_combo(a) {
                assert_eq!(Combo::parse(&c.to_text()), Some(c), "{}", a.id);
            }
        }
        assert_eq!(Combo::parse("Ctrl++").map(|c| (c.key, c.ctrl)), Some((Key::Plus, true)));
    }
}
