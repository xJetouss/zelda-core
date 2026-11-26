//! Game Boy Advance (z. B. „The Minish Cap“): Emulation mit mGBA, ohne Änderungen am Spiel.
//!
//! Hier: ein laufendes GBA-Spiel (`Gba`), Spielstände, Speicherdaten (`.sav` neben der ROM), Cheats und das
//! Titelbild für die Karte im Hauptmenü. Die Oberfläche steht in `ui.rs` (Bild, Ablauf) und `menus.rs`.

#[cfg(pastlink_gba)]
pub mod core;
pub mod picori;
pub mod tmcmap;
pub mod mapui;
pub mod ui;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const W: usize = 240;
pub const H: usize = 160;

/// Bereich des B-Felds in der Anzeige von The Minish Cap (inkl. Item darüber); das Y-Feld liegt 24 Pixel links
pub const ICON_X: usize = 174;
pub const ICON_Y: usize = 8;
pub const ICON_W: usize = 24;
/// 30 Reihen: der Schatten unter dem Tastenkreis reicht bis in die letzte Reihe
pub const ICON_H: usize = 30;
/// Item-Grafik aus dem Item-Menü (Seitenlänge in Pixeln)
/// 20×20: lange Items (Feuerstab) und Bomben ganz
pub const INV_ICON: usize = 20;
/// Kästchen der Item-Leiste (Pixel), wie die Kästchen im Item-Menü
pub const TILE_W: usize = 26;
pub const TILE_H: usize = 22;
/// Picori: Hintergrundebenen beim Aufnehmen der Item-Symbole (0 = nur Figuren)
pub const ICON_LAYERS: i32 = 0;
/// PC-Fassung: die Aufnahmen des B-Felds enthalten keine Anzahl (Ziffern-Kacheln werden dabei ausgeblendet)
pub const NO_DIGITS_IN_CAPTURE: bool = true;

/// Ist der GBA-Kern eingebaut?
pub fn available() -> bool {
    cfg!(pastlink_gba)
}

/// Ist das eine GBA-ROM (Dateiendung)?
pub fn is_gba(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gba"))
}

/// Spieltitel aus dem ROM-Kopf (z. B. „GBAZELDA MC“) und Spielkennung („BZMP“)
pub fn header(rom: &[u8]) -> Option<(String, String)> {
    let t = rom.get(0xA0..0xAC)?;
    let c = rom.get(0xAC..0xB0)?;
    let s = |b: &[u8]| String::from_utf8_lossy(b).trim_end_matches('\0').trim().to_string();
    Some((s(t), s(c)))
}

/// Schöner Name für bekannte Spiele (Kennung aus dem ROM-Kopf), sonst der Dateiname
pub fn display_name(path: &Path) -> String {
    let code = std::fs::File::open(path).ok().and_then(|mut f| {
        use std::io::{Read, Seek, SeekFrom};
        let mut b = [0u8; 0x10];
        f.seek(SeekFrom::Start(0xA0)).ok()?;
        f.read_exact(&mut b).ok()?;
        Some(String::from_utf8_lossy(&b[12..16]).to_string())
    });
    match code.as_deref() {
        Some(c) if c.starts_with("BZM") => "The Minish Cap".into(),
        _ => path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
    }
}

/// Ein Cheat (GameShark, Action Replay, CodeBreaker – mGBA erkennt das Format selbst)
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Cheat {
    pub name: String,
    pub code: String,
    pub on: bool,
}

/// Einstellungen und Cheats je GBA-Spiel (`<rom>.gba.json` neben der ROM)
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct GameCfg {
    pub cheats: Vec<Cheat>,
    /// The Minish Cap: Item auf der zusätzlichen Y-Taste (Item-Nummer, 0 = keins)
    #[serde(default)]
    pub y_item: u8,
}

/// The Minish Cap (Europa/USA): Spielstand im RAM ab 0x02002A40 (zeldaret/tmc), Werte ab 0x02002AE8
pub mod minish {
    /// ausgerüstete Items: A, B
    pub const EQUIP_A: u32 = 0x0200_2AF4;
    pub const EQUIP_B: u32 = 0x0200_2AF5;
    /// Weltkarte (Item 71 = Bits 6/7 von Inventar-Byte 17) als CodeBreaker-ODER-Zeile
    pub const MAP_LINE: &str = "22002B42 4000";
    /// Items mit Anzahl neben der Grafik (Bomben, Fernzünder-Bomben, Bogen)
    pub const COUNTED: &[u8] = &[7, 8, 9, 10];
    /// Inventar: je Item 2 Bit (Wert != 0 = vorhanden)
    pub const INVENTORY: u32 = 0x0200_2B32;
    /// Items, die man auf A/B legen kann (Nummern wie im Spiel); bei Schwert/Bumerang/Schild zählt die beste Stufe
    pub const EQUIPPABLE: &[u8] = &[6, 5, 4, 3, 2, 1, 7, 8, 9, 12, 11, 14, 13, 15, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30, 31];

    /// vorhandene Items aus dem Inventar (34 Bytes)
    pub fn owned(inv: &[u8]) -> Vec<u8> {
        let has = |i: u8| inv.get(i as usize >> 2).is_some_and(|b| (b >> ((i & 3) * 2)) & 3 != 0);
        let mut v = Vec::new();
        // nur die beste Stufe von Schwert, Bumerang, Schild und Bomben (Fernzünder ersetzen im Spiel die Bomben)
        for group in [&[6u8, 5, 4, 3, 2, 1][..], &[12, 11], &[14, 13], &[8, 7]] {
            if let Some(&i) = group.iter().find(|&&i| has(i)) {
                v.push(i);
            }
        }
        for &i in &[9u8, 15, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30, 31] {
            if has(i) {
                v.push(i);
            }
        }
        v.sort_by_key(|i| EQUIPPABLE.iter().position(|x| x == i));
        v
    }

    pub fn name(i: u8, en: bool) -> &'static str {
        let (de, e) = match i {
            1 => ("Schmiede-Schwert", "Smith's Sword"),
            2 => ("Weißes Schwert", "White Sword"),
            3 => ("Weißes Schwert (2)", "White Sword (2)"),
            4 => ("Weißes Schwert (3)", "White Sword (3)"),
            5 | 6 => ("Vierschwert", "Four Sword"),
            7 => ("Bomben", "Bombs"),
            8 => ("Fernzünder-Bomben", "Remote Bombs"),
            9 => ("Bogen", "Bow"),
            11 => ("Bumerang", "Boomerang"),
            12 => ("Magischer Bumerang", "Magical Boomerang"),
            13 => ("Schild", "Shield"),
            14 => ("Spiegelschild", "Mirror Shield"),
            15 => ("Laterne", "Lantern"),
            17 => ("Wirbelkrug", "Gust Jar"),
            18 => ("Wendestab", "Cane of Pacci"),
            19 => ("Maulwurfshandschuhe", "Mole Mitts"),
            20 => ("Roc-Umhang", "Roc's Cape"),
            21 => ("Pegasus-Stiefel", "Pegasus Boots"),
            22 => ("Feuerstab", "Flame Lantern"),
            23 => ("Okarina", "Ocarina"),
            28..=31 => ("Flasche", "Bottle"),
            _ => ("Item", "Item"),
        };
        if en { e } else { de }
    }
}

pub fn cfg_path(rom: &Path) -> PathBuf {
    let mut s = rom.as_os_str().to_owned();
    s.push(".json");
    PathBuf::from(s)
}

pub fn load_cfg(rom: &Path) -> GameCfg {
    std::fs::read(cfg_path(rom)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_cfg(rom: &Path, c: &GameCfg) {
    if let Ok(b) = serde_json::to_vec_pretty(c) {
        let _ = std::fs::write(cfg_path(rom), b);
    }
}

/// Spielstand-Datei für Platz `slot` (`<rom>.gs1` …; mit Picori `<rom>.rs1` …, anderes Format)
pub fn state_path(rom: &Path, slot: u8, rc: bool) -> PathBuf {
    rom.with_extension(format!("{}{slot}", if rc { "rs" } else { "gs" }))
}

/// Cheat-Zeilen selbst ausführen (für Picori, das keinen Cheat-Kern hat): CodeBreaker (3 = Byte schreiben,
/// 8 = Halbwort schreiben, 2 = Halbwort ODER, 6 = Halbwort UND, 7/A = nächste Zeile nur bei gleich/ungleich) und
/// VBA-Rohcodes `AAAAAAAA:WW` (Byte), `AAAAAAAA:WWWW` (Halbwort). Andere Formate werden übersprungen.
pub fn run_cheat_lines(lines: &[&str], mut read: impl FnMut(u32, usize) -> Option<Vec<u8>>, mut write: impl FnMut(u32, &[u8])) {
    let mut skip = false;
    for l in lines {
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        if skip {
            skip = false;
            continue;
        }
        if let Some((a, v)) = l.split_once(':') {
            let (Ok(a), Ok(val)) = (u32::from_str_radix(a.trim(), 16), u32::from_str_radix(v.trim(), 16)) else { continue };
            if v.trim().len() <= 2 {
                write(a, &[val as u8]);
            } else {
                write(a, &(val as u16).to_le_bytes());
            }
            continue;
        }
        let mut it = l.split_whitespace();
        let (Some(a), Some(v)) = (it.next(), it.next()) else { continue };
        let (Ok(a), Ok(val)) = (u32::from_str_radix(a, 16), u32::from_str_radix(v, 16)) else { continue };
        let (t, addr, val) = (a >> 28, a & 0x0FFF_FFFF, val as u16);
        let mut half = || read(addr, 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        match t {
            3 => write(addr, &[val as u8]),
            8 => write(addr, &val.to_le_bytes()),
            2 => {
                if let Some(h) = half() {
                    write(addr, &(h | val).to_le_bytes());
                }
            }
            6 => {
                if let Some(h) = half() {
                    write(addr, &(h & val).to_le_bytes());
                }
            }
            7 => skip = half() != Some(val),
            0xA => skip = half() == Some(val),
            _ => {}
        }
    }
}

/// Ordner für den Kern (BIOS `gba_bios.bin` darf hier liegen, sonst eingebauter Ersatz)
pub fn sys_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("gba"))).unwrap_or_else(|| PathBuf::from("gba"))
}

#[cfg(pastlink_gba)]
pub use running::Gba;

#[cfg(pastlink_gba)]
mod running {
    use super::core;
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Es gibt nur einen Kern (libretro ist global)
    static BUSY: AtomicBool = AtomicBool::new(false);

    pub struct Gba {
        pub path: PathBuf,
        pub fps: f64,
        rate: f64,
        /// Bild (RGBA, `w`×`h`)
        pub frame: Vec<u8>,
        pub w: usize,
        pub h: usize,
        /// seit dem letzten Abholen neues Bild
        pub new_frame: bool,
        sram_saved: Vec<u8>,
        /// Umrechnung des Tons auf 32 kHz
        res_pos: f64,
        res_last: (i16, i16),
        pub cfg: GameCfg,
        /// Spielkennung aus dem ROM-Kopf (z. B. „BZMP“)
        pub code: String,
        pub frames: u64,
        /// zusätzliche Cheat-Zeilen, die die App vorübergehend setzt (Y-Taste)
        temp_cheats: Vec<String>,
        /// Y-Taste: 0 = frei, 1 = Item gerade auf B gelegt, 2 = B wird gedrückt, 3.. = B-Item zurückstellen
        y_phase: u8,
        y_b_orig: u8,
        /// Probelauf (Titelbild): nie den Batterie-Speicher schreiben
        pub(super) no_save: bool,
        /// Speicherstand des Spiels als .sav schreiben (Einstellung „Spielinterne Spielstände speichern“, wie bei
        /// A Link to the Past; aus = es wird nur über Savestates gespeichert)
        pub save_sav: bool,
        /// Cheats/Y-Item geändert: die App übernimmt sie in ihre Einstellungen
        pub cfg_dirty: bool,
        /// The Minish Cap als PC-Fassung (Picori) statt mGBA
        rc: Option<super::picori::Core>,
        /// Schreiberlaubnis, die Picori zuletzt bekam
        rc_write: bool,
        /// The Minish Cap: Weltkarte von Anfang an (Einstellung „Spiel“)
        pub force_map: bool,
    }

    impl Gba {
        pub fn load(path: &Path) -> Result<Gba, String> {
            Self::load_with(path, false, false)
        }

        /// `rc`: The Minish Cap mit Picori (PC-Fassung, echtes Breitbild) statt mGBA, wenn möglich.
        /// `write_sav`: das Spiel darf seinen Spielstand (.sav) schreiben.
        pub fn load_with(path: &Path, rc: bool, write_sav: bool) -> Result<Gba, String> {
            if rc {
                let mut b = [0u8; 0xC0];
                use std::io::Read;
                std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("ROM nicht lesbar: {e}"))?;
                let code = header(&b).map(|h| h.1).unwrap_or_default();
                if super::picori::available() && super::picori::supports(&code) {
                    let core = super::picori::Core::open(path, &path.with_extension("sav"), write_sav)?;
                    return Ok(Gba {
                        path: path.to_path_buf(),
                        fps: 59.7275,
                        rate: 48000.0,
                        frame: vec![0; W * H * 4],
                        w: W,
                        h: H,
                        new_frame: false,
                        sram_saved: Vec::new(),
                        res_pos: 0.0,
                        res_last: (0, 0),
                        cfg: GameCfg::default(),
                        code,
                        frames: 0,
                        no_save: false,
                        save_sav: write_sav,
                        cfg_dirty: false,
                        temp_cheats: Vec::new(),
                        y_phase: 0,
                        y_b_orig: 0,
                        rc: Some(core),
                        rc_write: write_sav,
                        force_map: false,
                    });
                }
            }
            if BUSY.swap(true, Ordering::SeqCst) {
                return Err("Es läuft schon ein GBA-Spiel".into());
            }
            let r = Self::load_inner(path);
            if r.is_err() {
                BUSY.store(false, Ordering::SeqCst);
            }
            r
        }

        fn load_inner(path: &Path) -> Result<Gba, String> {
            let rom = std::fs::read(path).map_err(|e| format!("ROM nicht lesbar: {e}"))?;
            if rom.len() < 0xC0 || rom.len() > 32 << 20 {
                return Err("Keine GBA-ROM".into());
            }
            // Leerlauf-Optimierung des Kerns (spart Rechenzeit, ändert nichts am Spiel)
            core::start(&sys_dir(), &[("mgba_idle_optimization", "Remove Known"), ("mgba_skip_bios", "ON"), ("mgba_use_bios", "ON")]);
            let cpath = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default();
            let info = core::GameInfo { path: cpath.as_ptr(), data: rom.as_ptr() as *const _, size: rom.len(), meta: std::ptr::null() };
            if !unsafe { core::retro_load_game(&info) } {
                core::stop();
                return Err("Der GBA-Kern konnte die ROM nicht laden".into());
            }
            let mut av = core::AvInfo::default();
            unsafe { core::retro_get_system_av_info(&mut av) };
            let mut g = Gba {
                path: path.to_path_buf(),
                fps: if av.timing.fps > 1.0 { av.timing.fps } else { 59.7275 },
                rate: if av.timing.sample_rate > 1000.0 { av.timing.sample_rate } else { 32768.0 },
                frame: vec![0; W * H * 4],
                w: W,
                h: H,
                new_frame: false,
                sram_saved: Vec::new(),
                res_pos: 0.0,
                res_last: (0, 0),
                cfg: load_cfg(path),
                code: header(&rom).map(|h| h.1).unwrap_or_default(),
                frames: 0,
                no_save: false,
                save_sav: false,
                cfg_dirty: false,
                temp_cheats: Vec::new(),
                y_phase: 0,
                y_b_orig: 0,
                rc: None,
                rc_write: false,
                force_map: false,
            };
            // Spielstand des Spiels (Batterie-Speicher)
            if let Ok(b) = std::fs::read(path.with_extension("sav")) {
                let m = g.sram_mut();
                let n = m.len().min(b.len());
                m[..n].copy_from_slice(&b[..n]);
            }
            g.sram_saved = g.sram().to_vec();
            g.apply_cheats();
            Ok(g)
        }

        fn sram(&self) -> &[u8] {
            unsafe {
                let p = core::retro_get_memory_data(core::MEMORY_SAVE_RAM) as *const u8;
                let n = core::retro_get_memory_size(core::MEMORY_SAVE_RAM);
                if p.is_null() || n == 0 {
                    &[]
                } else {
                    std::slice::from_raw_parts(p, n)
                }
            }
        }
        fn sram_mut(&mut self) -> &mut [u8] {
            unsafe {
                let p = core::retro_get_memory_data(core::MEMORY_SAVE_RAM) as *mut u8;
                let n = core::retro_get_memory_size(core::MEMORY_SAVE_RAM);
                if p.is_null() || n == 0 {
                    &mut []
                } else {
                    std::slice::from_raw_parts_mut(p, n)
                }
            }
        }

        /// Batterie-Speicher schreiben, wenn sich etwas geändert hat
        pub fn flush_sram(&mut self) {
            if self.rc.is_some() {
                // Picori schreibt den Spielstand selbst (nur mit Erlaubnis, siehe rc_frame)
                return;
            }
            if self.no_save || !self.save_sav {
                return;
            }
            let now = self.sram().to_vec();
            if !now.is_empty() && now != self.sram_saved && now.iter().any(|&b| b != 0xFF) {
                if std::fs::write(self.path.with_extension("sav"), &now).is_ok() {
                    self.sram_saved = now;
                }
            }
        }

        /// ein Bild mit diesen Tasten (SNES-Bitbelegung)
        pub fn run_frame(&mut self, pad: u16) {
            let pad = self.y_step(pad);
            // X = Select (am Controller liegt Select/Minus bei Zelda Core auf „Karte“; der GBA hat kein X)
            let pad = if pad & 0x0040 != 0 { pad | 0x2000 } else { pad };
            if self.rc.is_some() {
                self.rc_frame(pad, None);
                return;
            }
            if let Ok(mut s) = core::ST.lock() {
                if let Some(s) = s.as_mut() {
                    s.pad = pad;
                }
            }
            unsafe { core::retro_run() };
            self.frames += 1;
            if let Ok(mut s) = core::ST.lock() {
                if let Some(s) = s.as_mut() {
                    if s.new_frame {
                        s.new_frame = false;
                        self.w = s.w;
                        self.h = s.h;
                        self.frame.clone_from(&s.frame);
                        self.new_frame = true;
                    }
                }
            }
            if self.frames % 300 == 0 {
                self.flush_sram();
            }
        }

        /// Picori: ein Spielbild (Cheats vorher in den Speicher schreiben, wie es mGBA je Bild tut)
        pub(super) fn rc_frame(&mut self, pad: u16, layers: Option<i32>) {
            let write = self.save_sav && !self.no_save;
            let lines: Vec<String> = self.cheat_lines();
            let sav = self.path.with_extension("sav");
            let Some(c) = self.rc.as_mut() else { return };
            if !c.ready() && !matches!(c.poll_ready(), Ok(true)) {
                return;
            }
            if write != self.rc_write {
                self.rc_write = write;
                c.set_save(&sav, write);
            }
            if !lines.is_empty() {
                let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let cell = std::cell::RefCell::new(&mut *c);
                super::run_cheat_lines(&refs, |a, n| cell.borrow().read(a, n), |a, b| {
                    cell.borrow_mut().write(a, b);
                });
            }
            let r = match layers {
                Some(m) => c.render_layers(m, pad, &mut self.frame),
                None => c.run_frame(pad, &mut self.frame),
            };
            if let Some((w, h)) = r {
                self.w = w;
                self.h = h;
                self.new_frame = true;
                self.frames += 1;
            }
        }

        /// alle eingeschalteten Cheat-Zeilen (vorübergehende der Y-Taste zuerst)
        fn cheat_lines(&self) -> Vec<String> {
            let mut v: Vec<String> = self.temp_cheats.clone();
            if self.force_map && self.is_minish() {
                v.push(minish::MAP_LINE.to_string());
            }
            for c in self.cfg.cheats.iter().filter(|c| c.on) {
                v.extend(c.code.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()));
            }
            v
        }

        /// Läuft The Minish Cap mit Picori (PC-Fassung)?
        pub fn is_rc(&self) -> bool {
            self.rc.is_some()
        }

        /// Picori startet noch (entpackt beim ersten Mal Grafiken und Töne aus der ROM)
        pub fn loading(&self) -> bool {
            self.rc.as_ref().is_some_and(|c| !c.ready())
        }

        /// Picori: Anzeigefläche für echtes Breitbild (Seitenverhältnis), `wide` = Breitbild an
        pub fn set_view(&mut self, w: i32, h: i32, wide: bool) {
            if let Some(c) = self.rc.as_mut() {
                c.set_view(w, h, wide);
            }
        }

        /// Versatz der Spielanzeige nach rechts (Picori rückt sie im Breitbild an den rechten Rand)
        pub fn hud_dx(&self) -> usize {
            if self.rc.is_some() { self.w.saturating_sub(W) } else { 0 }
        }

        /// Vorschaubild 240×160 (Mitte des Bildes)
        pub fn thumb(&self) -> Option<Vec<u8>> {
            if self.h != H || self.w < W || self.frame.len() < self.w * H * 4 {
                return None;
            }
            let x0 = (self.w - W) / 2;
            let mut v = Vec::with_capacity(W * H * 4);
            for y in 0..H {
                let i = (y * self.w + x0) * 4;
                v.extend_from_slice(&self.frame[i..i + W * 4]);
            }
            Some(v)
        }

        /// Ton seit dem letzten Abholen, umgerechnet auf 32 kHz Stereo (wie die Tonausgabe der App)
        pub fn take_audio(&mut self, out: &mut Vec<i16>) {
            let src: Vec<i16> = if let Some(c) = &self.rc {
                let mut v = Vec::new();
                c.take_audio(&mut v);
                v
            } else {
                core::ST.lock().ok().and_then(|mut s| s.as_mut().map(|s| std::mem::take(&mut s.audio))).unwrap_or_default()
            };
            let step = self.rate / 32_000.0;
            let n = src.len() / 2;
            let at = |i: usize, last: (i16, i16)| if i == 0 { last } else { (src[(i - 1) * 2], src[(i - 1) * 2 + 1]) };
            // Position läuft über die Quelle (Index 0 = letzter Wert des vorigen Blocks)
            while self.res_pos < n as f64 {
                let i = self.res_pos as usize;
                let f = (self.res_pos - i as f64) as f32;
                let (a, b) = (at(i, self.res_last), at(i + 1, self.res_last));
                out.push((a.0 as f32 * (1.0 - f) + b.0 as f32 * f) as i16);
                out.push((a.1 as f32 * (1.0 - f) + b.1 as f32 * f) as i16);
                self.res_pos += step;
            }
            if n > 0 {
                self.res_last = (src[(n - 1) * 2], src[(n - 1) * 2 + 1]);
                self.res_pos -= n as f64;
            }
        }

        pub fn reset(&mut self) {
            if let Some(c) = self.rc.as_mut() {
                c.reset();
                return;
            }
            unsafe { core::retro_reset() };
        }

        pub fn save_state(&self) -> Option<Vec<u8>> {
            if let Some(c) = &self.rc {
                return if c.ready() { c.save_state() } else { None };
            }
            let n = unsafe { core::retro_serialize_size() };
            let mut b = vec![0u8; n];
            (n > 0 && unsafe { core::retro_serialize(b.as_mut_ptr() as *mut _, n) }).then_some(b)
        }

        pub fn load_state(&mut self, b: &[u8]) -> bool {
            if let Some(c) = self.rc.as_mut() {
                return c.load_state(b);
            }
            !b.is_empty() && unsafe { core::retro_unserialize(b.as_ptr() as *const _, b.len()) }
        }

        /// Cheats neu setzen (mGBA kann einzelne nicht abschalten: alle löschen, eingeschaltete neu eintragen)
        /// Ist das The Minish Cap (Europa/USA)? Nur dort gibt es Y-Taste und fertige Cheats.
        pub fn is_minish(&self) -> bool {
            matches!(self.code.as_str(), "BZMP" | "BZME")
        }

        /// Y-Taste (The Minish Cap): solange Y gehalten wird, liegt das Y-Item auf B und B ist gedrückt; beim
        /// Loslassen kommt das alte B-Item zurück. Läuft je Spielbild.
        fn y_step(&mut self, pad: u16) -> u16 {
            const Y: u16 = 0x4000;
            const B: u16 = 0x8000;
            let y = pad & Y != 0;
            let mut pad = pad & !Y;
            let item = self.cfg.y_item;
            match self.y_phase {
                0 if y && item != 0 && self.is_minish() && self.owned_items().contains(&item) => {
                    self.y_b_orig = self.peek(minish::EQUIP_B, 1).map_or(0, |b| b[0]);
                    self.temp_cheats = vec![format!("3{:07X} 00{item:02X}", minish::EQUIP_B)];
                    self.apply_cheats();
                    self.y_phase = 1;
                }
                1 | 2 if y => {
                    self.y_phase = 2;
                    pad |= B;
                }
                1 | 2 => {
                    // losgelassen: altes B-Item für zwei Bilder zurückschreiben
                    self.temp_cheats = vec![format!("3{:07X} 00{:02X}", minish::EQUIP_B, self.y_b_orig)];
                    self.apply_cheats();
                    self.y_phase = 3;
                }
                3 | 4 => self.y_phase += 1,
                5 => {
                    self.temp_cheats.clear();
                    self.apply_cheats();
                    self.y_phase = 0;
                }
                _ => {}
            }
            pad
        }

        /// Y-Taste gerade in Benutzung (Y-Item liegt vorübergehend auf B); Item, das sonst auf B liegt
        pub fn y_active(&self) -> Option<u8> {
            (self.y_phase != 0).then_some(self.y_b_orig)
        }

        /// vorhandene Items (The Minish Cap), die man auf eine Taste legen kann
        pub fn owned_items(&self) -> Vec<u8> {
            self.peek(minish::INVENTORY, 34).map(|inv| minish::owned(&inv)).unwrap_or_default()
        }

        /// Symbol eines Items, wie es das Spiel im B-Feld zeichnet (24×28 RGBA, Hintergrund durchsichtig).
        /// Dazu wird das Item kurz auf B gelegt, mit ausgeblendeten Hintergründen gezeichnet und alles zurückgestellt –
        /// im Spiel sieht man davon nichts.
        pub fn capture_icon(&mut self, item: u8) -> Option<Vec<u8>> {
            let bbox = if self.rc.is_some() { Some(self.b_box()?) } else { None };
            let st = self.save_state()?;
            let frame = self.frame.clone();
            let (nf, fr) = (self.new_frame, self.frames);
            let (fw, fh) = (self.w, self.h);
            let mut img = None;
            if self.rc.is_some() {
                // Picori: Item auf B, zwei Bilder ohne Hintergründe, B-Feld ausschneiden
                if self.poke(minish::EQUIP_B, &[item]) {
                    // leeres Feld: Kreis und Buchstabe stehen dann auf der Anzeige-Ebene (BG0), nicht als Figur
                    let layers = if item == 0 { 1 } else { ICON_LAYERS };
                    for _ in 0..2 {
                        self.rc_frame(0, Some(layers));
                    }
                    let (ox, oy) = bbox.unwrap_or((ICON_X as isize, ICON_Y as isize));
                    if self.frame.len() >= self.w * self.h * 4 && self.h == H && ox >= 0 && ox as usize + ICON_W <= self.w {
                        let mut out = vec![0u8; ICON_W * ICON_H * 4];
                        for y in 0..ICON_H {
                            let fy = oy + y as isize;
                            if fy < 0 || fy as usize >= self.h {
                                continue;
                            }
                            for x in 0..ICON_W {
                                let i = ((fy as usize) * self.w + ox as usize + x) * 4;
                                let p = &self.frame[i..i + 4];
                                let o = (y * ICON_W + x) * 4;
                                // Hintergrund ist bei der Aufnahme Magenta (tmc_core.dll), Schwarz gehört zur Figur
                                let backdrop = p[0] >= 200 && p[1] <= 60 && p[2] >= 200;
                                if !backdrop {
                                    out[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
                                }
                            }
                        }
                        img = Some(out);
                    }
                    let mut skip = Vec::new();
                    self.take_audio(&mut skip);
                }
            }
            if self.rc.is_none() {
                for l in 0..4 {
                    unsafe { core::pl_mgba_enable_layer(l, false) };
                }
            }
            let ok = self.rc.is_none() && self.poke(minish::EQUIP_B, &[item]);
            if ok {
                for _ in 0..2 {
                    unsafe { core::retro_run() };
                }
                if let Ok(mut s) = core::ST.lock() {
                    if let Some(s) = s.as_mut() {
                        let mut out = vec![0u8; ICON_W * ICON_H * 4];
                        for y in 0..ICON_H {
                            for x in 0..ICON_W {
                                let (sx, sy) = (ICON_X + x, ICON_Y + y);
                                let i = (sy * s.w + sx) * 4;
                                let p = &s.frame[i..i + 4];
                                let o = (y * ICON_W + x) * 4;
                                if p[0] > 8 || p[1] > 8 || p[2] > 8 {
                                    out[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
                                }
                            }
                        }
                        s.new_frame = false;
                        s.audio.clear();
                        img = Some(out);
                    }
                }
            }
            if self.rc.is_none() {
                for l in 0..4 {
                    unsafe { core::pl_mgba_enable_layer(l, true) };
                }
            }
            self.load_state(&st);
            self.w = fw;
            self.h = fh;
            self.frame = frame;
            self.new_frame = nf;
            self.frames = fr;
            // gültig nur mit durchsichtigen Ecken und nicht fast ganz deckend (sonst lief gerade kein Spiel, z. B.
            // weißer Logo-Bildschirm)
            if img.is_none() && std::env::var_os("PASTLINK_ICON_LOG").is_some() {
                eprintln!("Symbol {item}: keine Aufnahme (Bild {}x{})", self.w, self.h);
            }
            img.filter(|im| {
                let a = |x: usize, y: usize| im[(y * ICON_W + x) * 4 + 3];
                let opaque = im.chunks_exact(4).filter(|p| p[3] != 0).count();
                // der Tastenkreis muss zu sehen sein (weiß mit dunklem Rand, untere Hälfte) – sonst war die Anzeige gerade
                // nicht da (Zwischensequenz, Übergang) und es wurde etwas anderes aufgenommen
                let (mut white, mut dark) = (0, 0);
                for p in im[14 * ICON_W * 4..].chunks_exact(4).filter(|p| p[3] != 0) {
                    if p[0].min(p[1]).min(p[2]) > 200 {
                        white += 1;
                    } else if p[0].max(p[1]).max(p[2]) < 50 {
                        dark += 1;
                    }
                }
                let ring = white >= 8 && dark >= 15;
                let ok = a(0, 0) == 0 && a(ICON_W - 1, 0) == 0 && a(0, ICON_H - 1) == 0 && a(ICON_W - 1, ICON_H - 1) == 0 && opaque < ICON_W * ICON_H * 6 / 10 && opaque > 20 && ring;
                if !ok && std::env::var_os("PASTLINK_ICON_LOG").is_some() {
                    eprintln!("Symbol {item} verworfen: Ecken {} {} {} {}, deckend {opaque}, weiß {white}, dunkel {dark}", a(0, 0), a(ICON_W - 1, 0), a(0, ICON_H - 1), a(ICON_W - 1, ICON_H - 1));
                }
                ok
            })
        }

        pub fn apply_cheats(&mut self) {
            if self.rc.is_some() {
                // Picori: die Cheats werden je Bild in rc_frame geschrieben
                return;
            }
            unsafe { core::retro_cheat_reset() };
            if self.force_map && self.is_minish() {
                if let Ok(cs) = std::ffi::CString::new(minish::MAP_LINE) {
                    unsafe { core::retro_cheat_set(999, true, cs.as_ptr()) };
                }
            }
            for (i, code) in self.temp_cheats.iter().enumerate() {
                if let Ok(cs) = std::ffi::CString::new(code.as_str()) {
                    unsafe { core::retro_cheat_set(1000 + i as u32, true, cs.as_ptr()) };
                }
            }
            for (i, c) in self.cfg.cheats.iter().enumerate().filter(|(_, c)| c.on && !c.code.trim().is_empty()) {
                // libretro-Format: mehrere Zeilen mit „+“ getrennt
                let code: String = c.code.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("+");
                if let Ok(cs) = std::ffi::CString::new(code) {
                    unsafe { core::retro_cheat_set(i as u32, true, cs.as_ptr()) };
                }
            }
        }

        /// Bytes in den Arbeitsspeicher (EWRAM 0x02000000..0x0203FFFF) schreiben – über den Spielstand (enthält das
        /// RAM ab Versatz 0x21000), weil der Kern keinen direkten Speicherzugriff anbietet
        /// Picori: linke obere Ecke des B-Feld-Ausschnitts (wie ICON_X/ICON_Y), nur wenn das B-Feld ganz zu sehen ist
        pub fn b_box(&self) -> Option<(isize, isize)> {
            self.rc.as_ref().and_then(|c| c.button_b()).map(|(x, y)| (x as isize - 10, y as isize - 20))
        }

        /// Karte (nur PC-Fassung): Ansicht, Bild ohne Figuren, Räume, Gebiete, Fundorte
        pub fn map_view(&self) -> Option<[i32; 12]> {
            self.rc.as_ref().and_then(|c| c.map_view())
        }
        pub fn map_render_bg(&self) -> Option<(Vec<u8>, usize, usize)> {
            self.rc.as_ref().and_then(|c| c.render_bg())
        }
        pub fn room_rect(&self, area: u8, room: u8) -> Option<[i32; 4]> {
            self.rc.as_ref().and_then(|c| c.room_rect(area, room))
        }
        pub fn area_kind(&self, area: u8) -> i32 {
            self.rc.as_ref().map_or(0, |c| c.area_kind(area))
        }
        pub fn locations(&self, logic: &str) -> Vec<(String, u32, i32)> {
            self.rc.as_ref().map_or(Vec::new(), |c| c.locations(logic))
        }
        pub fn set_own_map(&self, on: bool) {
            if let Some(c) = &self.rc {
                c.set_own_map(on);
            }
        }
        pub fn map_request(&self) -> i32 {
            self.rc.as_ref().map_or(0, |c| c.map_request())
        }
        pub fn room_count(&self, area: u8) -> i32 {
            self.rc.as_ref().map_or(0, |c| c.room_count(area))
        }
        pub fn warp(&self, area: u8, room: u8, x: i32, y: i32) -> bool {
            self.rc.as_ref().is_some_and(|c| c.warp(area, room, x, y))
        }
        pub fn area_name(&self, area: u8) -> Option<(String, bool)> {
            self.rc.as_ref().and_then(|c| c.area_name(area))
        }
        pub fn travel(&self, area: u8, room: u8, x: i32, y: i32) -> bool {
            self.rc.as_ref().is_some_and(|c| c.travel(area, room, x, y))
        }
        pub fn room_marks(&self) -> Vec<(u8, i32, i32)> {
            self.rc.as_ref().map_or(Vec::new(), |c| c.room_marks())
        }
        pub fn room_exits(&self) -> Vec<(i32, i32, u8, u8, i32, i32)> {
            self.rc.as_ref().map_or(Vec::new(), |c| c.room_exits())
        }
        /// Item-Grafiken aus dem Item-Menü des Spiels (ohne Tastenkreis, Flaschen mit Inhalt): Spielstand merken, Menü
        /// öffnen, alle Items aufnehmen, Spielstand zurück – unsichtbar für den Spieler (nur PC-Fassung)
        pub fn capture_menu_icons(&mut self) -> std::collections::HashMap<u8, Vec<u8>> {
            let mut out = std::collections::HashMap::new();
            if self.rc.is_none() || self.menu_item().is_some() {
                return out;
            }
            let Some(st) = self.save_state() else { return out };
            let (frame, nf, fr, fw, fh) = (self.frame.clone(), self.new_frame, self.frames, self.w, self.h);
            // Start drücken, bis das Item-Menü offen und fertig eingeblendet ist
            let mut open_at = None;
            for f in 0..240u32 {
                self.rc_frame(if f % 30 < 3 && open_at.is_none() { 0x1000 } else { 0 }, None);
                if self.menu_item().is_some() && open_at.is_none() {
                    open_at = Some(f);
                }
                if open_at.is_some_and(|o| f >= o + 45) {
                    break;
                }
            }
            if open_at.is_some() {
                let cursor = self.menu_item();
                if let Some((spr, w, h)) = self.capture_sprites() {
                    let dx = w.saturating_sub(W) / 2;
                    for i in self.owned_items() {
                        // Bomben/Pfeile: daneben steht die Anzahl – die kommen sauberer aus dem B-Feld
                        if minish::COUNTED.contains(&i) {
                            continue;
                        }
                        let pos = self.menu_pos(i).or_else(|| self.menu_pos(match i { 15 => 16, 16 => 15, x => x }));
                        if let Some((cx, cy)) = pos {
                            let img = crop_inv_icon(&spr, w, h, cx + dx, cy, cursor == Some(i));
                            if img.chunks_exact(4).filter(|p| p[3] != 0).count() > 12 {
                                out.insert(i, img);
                            }
                        }
                    }
                }
            }
            let mut skip = Vec::new();
            self.take_audio(&mut skip);
            self.load_state(&st);
            self.frame = frame;
            self.new_frame = nf;
            self.frames = fr;
            self.w = fw;
            self.h = fh;
            out
        }

        pub fn set_hide_counts(&self, on: bool) {
            if let Some(c) = &self.rc {
                c.set_hide_counts(on);
            }
        }
        pub fn set_noclip(&self, on: bool) {
            if let Some(c) = &self.rc {
                c.set_noclip(on);
            }
        }
        pub fn cheat_complete(&self) -> i32 {
            self.rc.as_ref().map_or(0, |c| c.cheat_complete())
        }
        pub fn place_safe(&self) -> bool {
            self.rc.as_ref().is_some_and(|c| c.place_safe())
        }
        pub fn progress(&self) -> Vec<i32> {
            self.rc.as_ref().map_or(Vec::new(), |c| c.progress())
        }
        pub fn gen_ready(&self) -> i32 {
            self.rc.as_ref().map_or(-1, |c| c.gen_ready())
        }
        pub fn cam(&self, x: i32, y: i32) {
            if let Some(c) = &self.rc {
                c.cam(x, y);
            }
        }
        pub fn menu_goto(&self, which: i32) -> bool {
            self.rc.as_ref().is_some_and(|c| c.menu_goto(which))
        }
        /// läuft die PC-Fassung (Picori)?
        pub fn is_recomp(&self) -> bool {
            self.rc.is_some()
        }
        pub fn loc_done(&self, key: u32) -> Option<bool> {
            self.rc.as_ref().and_then(|c| c.loc_done(key))
        }

        /// Entwicklung: Werte aus Picori zur Fehlersuche
        pub fn rc_debug(&self, what: i32) -> i32 {
            self.rc.as_ref().map_or(-9, |c| c.debug(what))
        }

        /// Item-Menü der PC-Fassung offen: Item unter dem Cursor (0 = leer); None = nicht offen oder mGBA
        pub fn menu_item(&self) -> Option<u8> {
            self.rc.as_ref().and_then(|c| c.menu_item())
        }

        /// Item-Menü: Mitte des Kästchens mit diesem Item
        pub fn menu_pos(&self, item: u8) -> Option<(usize, usize)> {
            self.rc.as_ref().and_then(|c| c.menu_pos(item))
        }

        /// Picori: das nächste Spielbild nur mit Figuren zeichnen (Hintergrund Magenta) – z. B. die Item-Grafiken des
        /// Item-Menüs. Ohne Zurückspulen (das brächte die Menü-Eingabe durcheinander): das Spiel läuft ein Bild weiter,
        /// angezeigt wird weiter das vorige. Liefert RGBA, Breite, Höhe.
        pub fn capture_sprites(&mut self) -> Option<(Vec<u8>, usize, usize)> {
            self.rc.as_ref()?;
            let (frame, nf, fw, fh) = (self.frame.clone(), self.new_frame, self.w, self.h);
            // das B-Item für dieses eine Bild herunternehmen: sonst zeichnet das Item-Menü sein hellblaues Kästchen als
            // Figur mit; gleich danach wieder zurück
            let b = self.peek(minish::EQUIP_B, 1).map_or(0, |b| b[0]);
            if b != 0 {
                self.poke(minish::EQUIP_B, &[0]);
            }
            self.rc_frame(0, Some(0));
            if b != 0 {
                self.poke(minish::EQUIP_B, &[b]);
            }
            let out = (self.frame.len() >= self.w * self.h * 4).then(|| (self.frame.clone(), self.w, self.h));
            self.frame = frame;
            self.new_frame = nf;
            self.w = fw;
            self.h = fh;
            out
        }

        /// Cheat-Zeilen einmal ausführen (Knöpfe im Cheat-Menü, z. B. „Alle Items geben“)
        pub fn run_once(&mut self, code: &str) {
            let lines: Vec<&str> = code.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            let mut writes: Vec<(u32, Vec<u8>)> = Vec::new();
            {
                let this = &*self;
                let pending = std::cell::RefCell::new(&mut writes);
                super::run_cheat_lines(&lines, |a, n| this.peek(a, n), |a, b| pending.borrow_mut().push((a, b.to_vec())));
            }
            let refs: Vec<(u32, &[u8])> = writes.iter().map(|(a, b)| (*a, b.as_slice())).collect();
            self.poke_many(&refs);
        }

        /// The Minish Cap: Item auf A (`a`) oder B legen; liegt es schon auf der anderen Taste, tauschen beide (wie
        /// im Item-Menü des Spiels)
        pub fn equip(&mut self, a: bool, item: u8) {
            let (this, other) = if a { (minish::EQUIP_A, minish::EQUIP_B) } else { (minish::EQUIP_B, minish::EQUIP_A) };
            let Some(cur) = self.peek(minish::EQUIP_A, 2) else { return };
            let (old_this, old_other) = if a { (cur[0], cur[1]) } else { (cur[1], cur[0]) };
            if old_other == item {
                self.poke_many(&[(this, &[item][..]), (other, &[old_this][..])]);
            } else {
                self.poke(this, &[item]);
            }
        }

        pub fn poke(&mut self, addr: u32, bytes: &[u8]) -> bool {
            self.poke_many(&[(addr, bytes)])
        }

        pub fn poke_many(&mut self, writes: &[(u32, &[u8])]) -> bool {
            if let Some(c) = self.rc.as_mut() {
                return c.ready() && writes.iter().all(|(a, b)| c.write(*a, b));
            }
            let Some(mut st) = self.save_state() else { return false };
            for (addr, bytes) in writes {
                let Some(off) = addr.checked_sub(0x0200_0000).filter(|o| (*o as usize) + bytes.len() <= 0x40000) else { return false };
                let at = 0x21000 + off as usize;
                let Some(dst) = st.get_mut(at..at + bytes.len()) else { return false };
                dst.copy_from_slice(bytes);
            }
            self.load_state(&st)
        }

        /// Bytes aus dem Arbeitsspeicher lesen (über den Spielstand)
        pub fn peek(&self, addr: u32, n: usize) -> Option<Vec<u8>> {
            if let Some(c) = &self.rc {
                return if c.ready() { c.read(addr, n) } else { None };
            }
            let st = self.save_state()?;
            let off = addr.checked_sub(0x0200_0000)? as usize;
            st.get(0x21000 + off..0x21000 + off + n).map(|s| s.to_vec())
        }

        /// Cheats/Y-Item wurden geändert (gespeichert werden sie in den Einstellungen der App, nicht neben der ROM)
        pub fn save_cfg(&mut self) {
            self.cfg_dirty = true;
        }

        /// Ist die Spielanzeige von The Minish Cap zu sehen? (Erkennung am R-Knopf oben rechts: hellgraue Fläche)
        pub fn hud_visible(&self) -> bool {
            if !self.is_minish() || self.w < W || self.h != H || self.frame.len() < self.w * H * 4 {
                return false;
            }
            if self.rc.is_some() {
                // Picori: das B-Feld steht ganz an seinem Platz (nicht bei Zwischensequenzen, Texten, Übergängen)
                return self.b_box().is_some();
            }
            let (dx, fw) = (self.hud_dx(), self.w);
            let at = |x: usize, y: usize| {
                let i = (y * fw + x + dx) * 4;
                (self.frame[i] as i32, self.frame[i + 1] as i32, self.frame[i + 2] as i32)
            };
            let lum = |x: usize, y: usize| {
                let (r, g, b) = at(x, y);
                (r * 3 + g * 6 + b) / 10
            };
            // R-Knopf: dunkler Rand oben, helle Fläche, dunkler Buchstabe „R“ (ein weißer oder heller Bildschirm hat
            // das nicht)
            let rim = [210usize, 214, 218].iter().all(|&x| lum(x, 10) < 130);
            let face = [210usize, 214, 218].iter().all(|&x| lum(x, 13) > 150);
            let letter = lum(222, 17) < 140;
            rim && face && letter
        }

        pub fn set_option(&self, key: &str, value: &str) {
            if self.rc.is_some() {
                return;
            }
            core::set_var(key, value);
        }
    }

    impl Drop for Gba {
        fn drop(&mut self) {
            if self.rc.is_some() {
                return;
            }
            self.flush_sram();
            unsafe { core::retro_unload_game() };
            core::stop();
            BUSY.store(false, Ordering::SeqCst);
        }
    }

    /// Titelbild für die Karte im Hauptmenü: Spiel kurz ohne Ton laufen lassen und das Bild nehmen, sobald der
    /// Titel steht (bleibt nur im Speicher – es wird nichts neben die ROM geschrieben)
    pub fn title_image(path: &Path) -> Option<(Vec<u8>, usize, usize)> {
        let mut g = Gba::load(path).ok()?;
        g.no_save = true;
        // Titelbild nach den Logos (ohne Tastendruck – Start ginge am Titel vorbei)
        // Minish Cap: ab etwa Bild 360 steht der Titel mit Logo
        for _ in 0..600u32 {
            g.run_frame(0);
        }
        let img = g.frame.clone();
        drop(g);
        Some((img, W, H))
    }
}

#[cfg(pastlink_gba)]
pub use running::title_image;

/// Ersatz ohne GBA-Kern (Build ohne CMake/mGBA): lädt nie
#[cfg(not(pastlink_gba))]
pub struct Gba {
    pub path: PathBuf,
    pub fps: f64,
    pub frame: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub new_frame: bool,
    pub cfg: GameCfg,
}

#[cfg(not(pastlink_gba))]
impl Gba {
    pub fn load(_: &Path) -> Result<Gba, String> {
        Err("GBA ist in diesem Build nicht enthalten".into())
    }
    pub fn load_with(_: &Path, _: bool, _: bool) -> Result<Gba, String> {
        Err("GBA ist in diesem Build nicht enthalten".into())
    }
    pub fn is_rc(&self) -> bool {
        false
    }
    pub fn loading(&self) -> bool {
        false
    }
    pub fn set_view(&mut self, _: i32, _: i32, _: bool) {}
    pub fn hud_dx(&self) -> usize {
        0
    }
    pub fn thumb(&self) -> Option<Vec<u8>> {
        None
    }
    pub fn run_frame(&mut self, _: u16) {}
    pub fn take_audio(&mut self, _: &mut Vec<i16>) {}
    pub fn reset(&mut self) {}
    pub fn capture_menu_icons(&mut self) -> std::collections::HashMap<u8, Vec<u8>> {
        Default::default()
    }
    pub fn save_state(&self) -> Option<Vec<u8>> {
        None
    }
    pub fn load_state(&mut self, _: &[u8]) -> bool {
        false
    }
    pub fn apply_cheats(&mut self) {}
    pub fn save_cfg(&mut self) {}
    pub fn flush_sram(&mut self) {}
}

/// Kästchen der Item-Leiste als Pixelbild: wie im Item-Menü (Rand, graue Fläche; gewählt hellblau mit derselben
/// weißen Fläche wie beim ausgerüsteten B-Item), darauf die Item-Grafik
/// Ziffern 3×5 für die Anzahl auf den Kästchen der Item-Leiste (Bit 2 = linke Spalte)
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

/// Kästchen der Item-Leiste mit Anzahl (Bomben, Pfeile) unten rechts – weiß mit schwarzem Rand wie im Spiel
pub fn bar_tile_count(item: &[u8], sel: bool, count: Option<u8>) -> Vec<crate::gfx::Color32> {
    use crate::gfx::Color32;
    let mut px = bar_tile(item, sel);
    let Some(n) = count else { return px };
    let txt = format!("{:02}", n.min(99));
    // zwei Ziffern (je 3 breit, 1 Abstand) mit Rand: Fläche 9×7, rechts unten innerhalb des Rahmens
    let (x0, y0) = (TILE_W as isize - 2 - 9, TILE_H as isize - 2 - 7);
    let on = |x: isize, y: isize| -> bool {
        let (lx, ly) = (x - 1, y - 1);
        if !(0..5).contains(&ly) || lx < 0 {
            return false;
        }
        let d = (lx / 4) as usize;
        let cx = lx % 4;
        if d >= 2 || cx == 3 {
            return false;
        }
        let g = txt.as_bytes()[d] - b'0';
        DIGITS[g as usize][ly as usize] & (4 >> cx) != 0
    };
    for y in 0..7isize {
        for x in 0..9isize {
            let (tx, ty) = ((x0 + x) as usize, (y0 + y) as usize);
            let c = if on(x, y) {
                Some(Color32::from_rgb(248, 248, 248))
            } else if (-1..=1).any(|dy| (-1..=1).any(|dx| on(x + dx, y + dy))) {
                Some(Color32::from_rgb(8, 8, 8))
            } else {
                None
            };
            if let Some(c) = c {
                px[ty * TILE_W + tx] = c;
            }
        }
    }
    px
}

pub fn bar_tile(item: &[u8], sel: bool) -> Vec<crate::gfx::Color32> {
    use crate::gfx::Color32;
    let (w, h) = (TILE_W, TILE_H);
    let border = Color32::from_rgb(142, 142, 142);
    let fill = if sel { Color32::from_rgb(120, 216, 243) } else { Color32::from_rgb(165, 165, 165) };
    let white = Color32::from_rgb(243, 243, 243);
    // Ankerpunkt wie im Item-Menü (cx, cy); weiße Fläche je Zeile von–bis
    let (cx, cy) = (13isize, 17isize);
    const MASK: [(isize, isize, isize); 17] = [
        (-14, -4, 3),
        (-13, -6, 5),
        (-12, -7, 6),
        (-11, -8, 7),
        (-10, -8, 7),
        (-9, -9, 8),
        (-8, -9, 8),
        (-7, -9, 8),
        (-6, -9, 8),
        (-5, -9, 8),
        (-4, -9, 8),
        (-3, -9, 8),
        (-2, -8, 7),
        (-1, -8, 7),
        (0, -7, 6),
        (1, -6, 5),
        (2, -4, 3),
    ];
    let mut px = vec![fill; w * h];
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                px[y * w + x] = border;
            } else if sel {
                let (dx, dy) = (x as isize - cx, y as isize - cy);
                if MASK.iter().any(|&(my, a, b)| my == dy && dx >= a && dx <= b) {
                    px[y * w + x] = white;
                }
            }
        }
    }
    // Item (INV_ICON²): links oben bei (cx - 10, cy - 16), wie im Item-Menü ausgeschnitten
    let (ox, oy) = (cx - 10, cy - 16);
    for y in 0..INV_ICON {
        for x in 0..INV_ICON {
            let o = (y * INV_ICON + x) * 4;
            let (tx, ty) = (ox + x as isize, oy + y as isize);
            if item[o + 3] != 0 && tx >= 0 && ty >= 0 && (tx as usize) < w && (ty as usize) < h {
                px[ty as usize * w + tx as usize] = Color32::from_rgb(item[o], item[o + 1], item[o + 2]);
            }
        }
    }
    px
}

/// Wo liegt die Inventar-Grafik (INV_ICON²) in einer Aufnahme des B-Felds (ICON_W×ICON_H)? Größte Übereinstimmung.
pub fn find_item_offset(hud: &[u8], inv: &[u8]) -> Option<(isize, isize)> {
    let mut best = (0usize, (0isize, 0isize));
    for oy in -6..10isize {
        for ox in -6..10isize {
            let mut n = 0;
            for y in 0..INV_ICON {
                for x in 0..INV_ICON {
                    let o = (y * INV_ICON + x) * 4;
                    if inv[o + 3] == 0 {
                        continue;
                    }
                    let (hx, hy) = (ox + x as isize, oy + y as isize);
                    if hx < 0 || hy < 0 || hx as usize >= ICON_W || hy as usize >= ICON_H {
                        continue;
                    }
                    let h = (hy as usize * ICON_W + hx as usize) * 4;
                    if hud[h + 3] != 0 && hud[h..h + 3] == inv[o..o + 3] {
                        n += 1;
                    }
                }
            }
            if n > best.0 {
                best = (n, (ox, oy));
            }
        }
    }
    (best.0 >= 20).then_some(best.1)
}

/// Y-Feld genau wie das B-Feld: alles, was sich in der B-Aufnahme des Items (`hud`) vom leeren B-Kreis (`ring`)
/// unterscheidet, auf den Y-Kreis (`base`) setzen – Item, Anzahl, Schatten wie im Spiel
pub fn slot_from_b(base: &[u8], hud: &[u8], ring: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    for p in 0..ICON_W * ICON_H {
        let o = p * 4;
        if hud[o + 3] != 0 && hud[o..o + 4] != ring[o..o + 4] {
            out[o..o + 4].copy_from_slice(&hud[o..o + 4]);
        }
    }
    out
}

/// Y-Feld wie `slot_from_b`, aber nie mit Anzahl: bei Bomben, Fernzünder-Bomben, Bogen den Bereich der Ziffern
/// (unten rechts) wieder auf den leeren Kreis setzen und das Item dort ergänzen (bei Bomben gespiegelt)
pub fn slot_from_b_nocount(base: &[u8], hud: &[u8], ring: &[u8], item: u8, off: (isize, isize)) -> Vec<u8> {
    let mut out = slot_from_b(base, hud, ring);
    if NO_DIGITS_IN_CAPTURE || !minish::COUNTED.contains(&item) {
        return out;
    }
    for y in 15..ICON_H {
        for x in 13..ICON_W {
            let o = (y * ICON_W + x) * 4;
            out[o..o + 4].copy_from_slice(&base[o..o + 4]);
        }
    }
    let mut it = item_from_b(hud, ring, off, true);
    clean_counted(&mut it, item);
    compose_slot(&out, &it, off)
}

/// Reine Item-Aufnahme (ohne Kreis) auf INV_ICON² zuschneiden: Fenster mittig um die sichtbaren Pixel
pub fn item_centered(clean: &[u8]) -> Vec<u8> {
    let none = vec![0u8; clean.len()];
    item_centered_from_b(clean, &none, None)
}

/// Item-Grafik (INV_ICON²) aus der B-Aufnahme lösen, ohne festen Versatz: alle Pixel, die zum Item gehören (anders
/// als der leere Kreis und, wenn vorhanden, auch nach dem Mehrheitsvergleich kein Kreis), dann ein Fenster zentriert
/// um ihre tatsächliche Ausdehnung – so wird kein Item oben oder seitlich abgeschnitten
pub fn item_centered_from_b(hud: &[u8], ring: &[u8], bare: Option<&[u8]>) -> Vec<u8> {
    let keep = |p: usize| {
        let o = p * 4;
        hud[o + 3] != 0 && hud[o..o + 4] != ring[o..o + 4] && bare.is_none_or(|b| b[o + 3] != 0)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (ICON_W, ICON_H, 0usize, 0usize);
    for y in 0..ICON_H {
        for x in 0..ICON_W {
            if keep(y * ICON_W + x) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let s = INV_ICON;
    let mut out = vec![0u8; s * s * 4];
    if x1 < x0 {
        return out;
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    // Fenster: mittig um das Item (zu groß: von oben links)
    let ox = x0 as isize - (s as isize - w as isize).max(0) / 2;
    let oy = y0 as isize - (s as isize - h as isize).max(0) / 2;
    for y in 0..s {
        for x in 0..s {
            let (hx, hy) = (ox + x as isize, oy + y as isize);
            if hx < 0 || hy < 0 || hx as usize >= ICON_W || hy as usize >= ICON_H {
                continue;
            }
            let p = hy as usize * ICON_W + hx as usize;
            if keep(p) {
                out[(y * s + x) * 4..(y * s + x) * 4 + 4].copy_from_slice(&hud[p * 4..p * 4 + 4]);
            }
        }
    }
    out
}

/// Item-Grafik (INV_ICON²) aus der B-Aufnahme lösen: was sich vom leeren B-Kreis unterscheidet, im Bereich, in dem
/// das Spiel Items auf B zeichnet (`off`, wie `find_item_offset`)
pub fn item_from_b(hud: &[u8], ring: &[u8], off: (isize, isize), counted: bool) -> Vec<u8> {
    let mut out = vec![0u8; INV_ICON * INV_ICON * 4];
    for y in 0..INV_ICON {
        for x in 0..INV_ICON {
            let (hx, hy) = (off.0 + x as isize, off.1 + y as isize);
            if hx < 0 || hy < 0 || hx as usize >= ICON_W || hy as usize >= ICON_H {
                continue;
            }
            // Anzahl (Bomben, Pfeile) unten rechts gehört nicht zur Grafik
            if counted && !NO_DIGITS_IN_CAPTURE && x >= INV_ICON - 6 && y >= 14 {
                continue;
            }
            let h = (hy as usize * ICON_W + hx as usize) * 4;
            if hud[h + 3] != 0 && hud[h..h + 4] != ring[h..h + 4] {
                let o = (y * INV_ICON + x) * 4;
                out[o..o + 4].copy_from_slice(&hud[h..h + 4]);
            }
        }
    }
    out
}

/// Items mit Anzahl (Bomben, Bogen): die Anzahl überdeckt im Spiel die untere rechte Ecke des Items – diesen Bereich
/// entfernen; die Bomben sind symmetrisch, dort wird die Ecke von links gespiegelt ergänzt
pub fn clean_counted(img: &mut [u8], item: u8) {
    // Aufnahmen enthalten die Anzahl nicht mehr (tmc_core.dll blendet die Ziffern-Kacheln aus) – nichts zu tun
    if NO_DIGITS_IN_CAPTURE || !minish::COUNTED.contains(&item) {
        return;
    }
    let s = INV_ICON;
    let bomb = matches!(item, 7 | 8);
    // Mitte der Bombe aus ihrer Breite in den Zeilen über den Ziffern (dort ist sie noch vollständig)
    let mut axis2 = 19isize;
    if bomb {
        let (mut lo, mut hi) = (s as isize, -1isize);
        for y in 7..12 {
            for x in 0..s {
                if img[(y * s + x) * 4 + 3] != 0 {
                    lo = lo.min(x as isize);
                    hi = hi.max(x as isize);
                }
            }
        }
        if hi >= lo {
            axis2 = lo + hi;
        }
    }
    for y in 12..s {
        for x in 13..s {
            let o = (y * s + x) * 4;
            img[o + 3] = 0;
            if bomb {
                let mx = axis2 - x as isize;
                if mx < 0 || mx >= 13 {
                    continue;
                }
                let mx = mx as usize;
                let m = (y * s + mx) * 4;
                if img[m + 3] != 0 {
                    let px = [img[m], img[m + 1], img[m + 2], 255];
                    img[o..o + 4].copy_from_slice(&px);
                }
            }
        }
    }
}

/// Tastenfeld zusammensetzen: Kreis (mit Buchstabe) und darauf die Item-Grafik an `off`
pub fn compose_slot(base: &[u8], item: &[u8], off: (isize, isize)) -> Vec<u8> {
    let mut out = base.to_vec();
    for y in 0..INV_ICON {
        for x in 0..INV_ICON {
            let o = (y * INV_ICON + x) * 4;
            let (tx, ty) = (off.0 + x as isize, off.1 + y as isize);
            if item[o + 3] != 0 && tx >= 0 && ty >= 0 && (tx as usize) < ICON_W && (ty as usize) < ICON_H {
                let t = (ty as usize * ICON_W + tx as usize) * 4;
                out[t..t + 4].copy_from_slice(&item[o..o + 4]);
            }
        }
    }
    out
}

/// Item-Grafik aus einem Figuren-Bild des Item-Menüs (`capture_sprites`) ausschneiden: INV_ICON² um das Kästchen
/// mit Ankerpunkt (cx, cy), ohne Zahlen daneben, ohne Ecken des Menü-Cursors und ohne Markierung des B-Items
pub fn crop_inv_icon(px: &[u8], w: usize, h: usize, cx: usize, cy: usize, under_cursor: bool) -> Vec<u8> {
    const S: usize = INV_ICON;
    let mut out = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            // Zeilen -15..+2 um den Ankerpunkt: manche Items (Spiegelschild, Handschuhe) reichen bis +2
            let (fx, fy) = ((cx + x) as isize - 10, (cy + y) as isize - 16);
            if fx < 0 || fy < 0 || fx as usize >= w || fy as usize >= h {
                continue;
            }
            let i = (fy as usize * w + fx as usize) * 4;
            let p = &px[i..i + 4];
            let magenta = p[0] >= 200 && p[1] <= 60 && p[2] >= 200;
            // Ecken des Menü-Cursors (rot) – nur in den Ecken und nur beim Item unter dem Cursor
            let corner = (x < 4 || x >= S - 4) && (y < 4 || y >= S - 4);
            let red = under_cursor && corner && p[0] >= 200 && p[1] <= 60 && p[2] <= 60;
            if !magenta && !red {
                let o = (y * S + x) * 4;
                out[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
    }
    peel_highlight(&mut out, S);
    out
}

/// Markierung des ausgerüsteten Items im Item-Menü entfernen (hellblaues Kästchen, weißer Kreis): von außen her alle
/// hellblauen und weißen Pixel durchsichtig machen, die an Durchsichtiges grenzen. Das Item selbst hat einen dunklen
/// Umriss und bleibt stehen.
pub fn peel_highlight(img: &mut [u8], s: usize) {
    let is = |img: &[u8], i: usize, c: [u8; 3]| img[i * 4 + 3] != 0 && img[i * 4..i * 4 + 3] == c;
    loop {
        let mut changed = false;
        for y in 0..s {
            for x in 0..s {
                let i = y * s + x;
                if !(is(img, i, [120, 216, 243]) || is(img, i, [243, 243, 243])) {
                    continue;
                }
                let edge = x == 0 || y == 0 || x + 1 == s || y + 1 == s;
                let open = edge
                    || [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                        .iter()
                        .any(|&(nx, ny)| img[(ny * s + nx) * 4 + 3] == 0);
                if open {
                    img[i * 4 + 3] = 0;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Reine Item-Grafiken (wie im Inventar) aus Aufnahmen des B-Felds: der Tastenkreis ist in allen Aufnahmen gleich –
/// Pixel, die in der Mehrheit der Aufnahmen übereinstimmen, gehören zum Kreis und werden durchsichtig. Bei weniger als
/// drei Aufnahmen bleiben sie unverändert.
pub fn strip_ring(icons: &[Vec<u8>]) -> Vec<Vec<u8>> {
    if icons.len() < 3 {
        return icons.to_vec();
    }
    let n = ICON_W * ICON_H;
    // gleiche Aufnahmen (z. B. vier gleiche Flaschen) nur einmal zählen – sonst gälte das Item selbst als Kreis
    let mut uniq: Vec<&Vec<u8>> = Vec::new();
    for im in icons {
        if !uniq.iter().any(|u| *u == im) {
            uniq.push(im);
        }
    }
    if uniq.len() < 3 {
        return icons.to_vec();
    }
    let mut ring: Vec<Option<[u8; 4]>> = vec![None; n];
    for (i, r) in ring.iter_mut().enumerate() {
        let mut counts: Vec<([u8; 4], usize)> = Vec::new();
        for im in uniq.iter() {
            let p = [im[i * 4], im[i * 4 + 1], im[i * 4 + 2], im[i * 4 + 3]];
            match counts.iter_mut().find(|c| c.0 == p) {
                Some(c) => c.1 += 1,
                None => counts.push((p, 1)),
            }
        }
        if let Some(best) = counts.iter().max_by_key(|c| c.1) {
            if best.0[3] != 0 && best.1 * 2 > uniq.len() {
                *r = Some(best.0);
            }
        }
    }
    icons
        .iter()
        .map(|im| {
            let mut out = im.clone();
            for i in 0..n {
                if ring[i].is_some_and(|p| p[..] == im[i * 4..i * 4 + 4]) {
                    out[i * 4 + 3] = 0;
                }
            }
            out
        })
        .collect()
}

#[cfg(all(test, pastlink_gba))]
mod tests {
    use super::*;

    /// Cheat-Ausführer für Picori: CodeBreaker 3/8/2/7 und VBA-Rohcodes auf einem Speicher-Ersatz
    #[test]
    fn cheat_lines() {
        let mut mem = std::collections::HashMap::<u32, u8>::new();
        let lines = ["32002AEA 0050", "82002B00 03E7", "22002B32 5554", "02002AEA:A0", "72002B00 1234", "32002AEB 0099", "32002AEC 0063"];
        mem.insert(0x2002B32, 0x01);
        let m = std::cell::RefCell::new(mem);
        run_cheat_lines(&lines, |a, n| Some((0..n as u32).map(|i| *m.borrow().get(&(a + i)).unwrap_or(&0)).collect()), |a, b| {
            for (i, v) in b.iter().enumerate() {
                m.borrow_mut().insert(a + i as u32, *v);
            }
        });
        let m = m.into_inner();
        assert_eq!(m[&0x2002AEA], 0xA0);
        assert_eq!((m[&0x2002B00], m[&0x2002B01]), (0xE7, 0x03));
        assert_eq!((m[&0x2002B32], m[&0x2002B33]), (0x55, 0x55));
        assert!(!m.contains_key(&0x2002AEB), "Bedingung 7 hätte die nächste Zeile überspringen müssen");
        assert_eq!(m[&0x2002AEC], 0x63);
    }

    /// GBA-ROM aus `PASTLINK_GBA_ROM` (eine Kopie! Der Test schreibt `.sav` daneben): Bilder alle 120 Spielbilder, Ton, Spielstand hin und zurück
    #[test]
    #[ignore]
    fn dev_gba_boot() {
        let Ok(p) = std::env::var("PASTLINK_GBA_ROM") else { return };
        let mut g = Gba::load(Path::new(&p)).unwrap();
        println!("{} fps {:.4}, Kopf {:?}", display_name(Path::new(&p)), g.fps, header(&std::fs::read(&p).unwrap()));
        let mut audio = Vec::new();
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        for f in 0..1800u32 {
            g.run_frame(0);
            g.take_audio(&mut audio);
            if f % 120 == 119 {
                let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                crate::devtest::bmp(&format!("{out}/gba_{f:04}.bmp"), g.w, g.h, &px);
            }
        }
        println!("Ton: {} Werte (32 kHz Stereo), Spitze {}", audio.len(), audio.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0));
        let st = g.save_state().unwrap();
        let before = g.frame.clone();
        for _ in 0..60 {
            g.run_frame(0x1000);
        }
        assert!(g.load_state(&st));
        g.run_frame(0);
        // Cheats: VBA-Format (32 Bit) und CodeBreaker „3aaaaaaa 00vv“ (8 Bit) – nachgeprüft im Spielstand (enthält das RAM)
        g.cfg.cheats = vec![Cheat { name: "Test".into(), code: "0203FF00:DEADBEEF".into(), on: true }, Cheat { name: "CB".into(), code: "3203FF04 005A".into(), on: true }];
        g.apply_cheats();
        for _ in 0..3 {
            g.run_frame(0);
        }
        let st2 = g.save_state().unwrap();
        let pos = st2.windows(5).position(|w| w == [0xEF, 0xBE, 0xAD, 0xDE, 0x5A]);
        println!("Cheats im RAM gefunden: {pos:?}");
        assert!(pos.is_some());
        g.cfg.cheats[0].on = false;
        g.apply_cheats();
        println!("Spielstand: {} Bytes, Bild nach Laden gleich wie vorher: {}", st.len(), g.frame.chunks(4).zip(before.chunks(4)).filter(|(a, b)| a != b).count() < 2000);
    }

    /// Neues Spiel automatisch beginnen (Tasten nach Plan `GBA_PLAN`, danach A im Wechsel), Bilder alle `GBA_SHOT`
    /// Spielbilder, am Ende Spielstand `gba_auto.gs`
    #[test]
    #[ignore]
    fn dev_gba_newgame() {
        let Ok(p) = std::env::var("PASTLINK_GBA_ROM") else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let n: u32 = std::env::var("GBA_N").ok().and_then(|v| v.parse().ok()).unwrap_or(12000);
        // Plan: „Taste:Bilder“ hintereinander, danach immer im Wechsel A / nichts
        let plan: Vec<(u16, u32)> = std::env::var("GBA_PLAN").ok().map(|v| v.split(',').filter_map(|x| { let (a, b) = x.split_once(':')?; Some((u16::from_str_radix(a, 16).ok()?, b.parse().ok()?)) }).collect()).unwrap_or_default();
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        // GBA_SEQ: Tasten (hex), je 8 Bilder gedrückt, alle 40 Bilder, ab Bild GBA_SEQ_AT; danach wieder der Rest
        let seq: Vec<u16> = std::env::var("GBA_SEQ").ok().map(|v| v.split(',').filter_map(|x| u16::from_str_radix(x, 16).ok()).collect()).unwrap_or_default();
        let seq_at: u32 = std::env::var("GBA_SEQ_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
        let gap: u32 = std::env::var("GBA_SEQ_GAP").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
        let pad_at = |f: u32| {
            if f >= seq_at && ((f - seq_at) / gap) < seq.len() as u32 {
                let i = ((f - seq_at) / gap) as usize;
                return if (f - seq_at) % gap < 8 { seq[i] } else { 0 };
            }
            let mut t = f;
            for &(k, len) in &plan {
                if t < len {
                    return k;
                }
                t -= len;
            }
            let hold: u16 = std::env::var("GBA_HOLD").ok().and_then(|v| u16::from_str_radix(&v, 16).ok()).unwrap_or(0);
            (if (t / 8) % 4 == 0 { 0x0080 } else { 0 }) | if f > 1000 && (t / 8) % 4 == 2 { hold } else { 0 }
        };
        for f in 0..n {
            g.run_frame(pad_at(f));
            let mut a = Vec::new();
            g.take_audio(&mut a);
            let every: u32 = std::env::var("GBA_SHOT").ok().and_then(|v| v.parse().ok()).unwrap_or(600);
            if f % every == every - 1 {
                let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                crate::devtest::bmp(&format!("{out}/ng_{f:05}.bmp"), g.w, g.h, &px);
            }
        }
        std::fs::write(format!("{out}/gba_auto.gs"), g.save_state().unwrap()).unwrap();
        let q: Vec<u32> = (0..16).map(|i| core::INPUT_DBG[i].load(std::sync::atomic::Ordering::Relaxed)).collect();
        let h: Vec<u32> = (16..32).map(|i| core::INPUT_DBG[i].load(std::sync::atomic::Ordering::Relaxed)).collect();
        println!("Abfragen je Taste {q:?}
gedrückt         {h:?}");
    }

    /// Spielstand `GBA_STATE` laden, Cheats aus `GBA_CHEATS` (mit „;“ getrennt) setzen, Bild speichern
    #[test]
    #[ignore]
    fn dev_gba_cheats() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        assert!(g.load_state(&std::fs::read(st).unwrap()));
        let codes = std::env::var("GBA_CHEATS").unwrap_or_default();
        g.cfg.cheats = codes.split(';').filter(|c| !c.trim().is_empty()).map(|c| Cheat { name: c.into(), code: c.replace('+', "
"), on: true }).collect();
        g.apply_cheats();
        let n: u32 = std::env::var("GBA_N").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
        for _ in 0..n {
            g.run_frame(0);
        }
        let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
        crate::devtest::bmp(&format!("{out}/cheat.bmp"), g.w, g.h, &px);
        let s = g.save_state().unwrap();
        let ew = &s[0x21000..0x61000];
        println!("2AEA..2AF0 {:02X?}  2B00..2B04 {:02X?}", &ew[0x2AEA..0x2AF0], &ew[0x2B00..0x2B04]);
        println!("Stats {:02X?}
Inventar {:02X?}
Items {:?}", &ew[0x2AE8..0x2B04], &ew[0x2B32..0x2B54], minish::owned(&ew[0x2B32..0x2B54]));
    }

    /// Spielstand `GBA_STATE` laden, Bytes aus `GBA_POKE` („adresse:hexbytes;…“) schreiben, Bilder speichern
    #[test]
    #[ignore]
    fn dev_gba_poke() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        assert!(g.load_state(&std::fs::read(st).unwrap()));
        let pokes: Vec<(u32, Vec<u8>)> = std::env::var("GBA_POKE").unwrap_or_default().split(';').filter_map(|x| {
            let (a, b) = x.split_once(':')?;
            let bytes = (0..b.len() / 2).map(|i| u8::from_str_radix(&b[i * 2..i * 2 + 2], 16).unwrap()).collect();
            Some((u32::from_str_radix(a, 16).ok()?, bytes))
        }).collect();
        let refs: Vec<(u32, &[u8])> = pokes.iter().map(|(a, b)| (*a, b.as_slice())).collect();
        assert!(g.poke_many(&refs));
        let plan: Vec<(u16, u32)> = std::env::var("GBA_PLAN").ok().map(|v| v.split(',').filter_map(|x| { let (a, b) = x.split_once(':')?; Some((u16::from_str_radix(a, 16).ok()?, b.parse().ok()?)) }).collect()).unwrap_or(vec![(0, 60)]);
        let mut k = 0;
        for (pad, n) in plan {
            for _ in 0..n {
                g.run_frame(pad);
            }
            let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/poke_{k}.bmp"), g.w, g.h, &px);
            k += 1;
        }
        println!("Stats {:02X?}", g.peek(0x2002AE8, 0x20).unwrap());
        println!("Inventar {:02X?}", g.peek(0x2002B32, 34).unwrap());
        if let Ok(o) = std::env::var("GBA_SAVE_TO") {
            std::fs::write(o, g.save_state().unwrap()).unwrap();
        }
    }

    /// Bereich des B- und A-Felds: Bild ohne Item mit Bild mit Item vergleichen
    #[test]
    #[ignore]
    fn dev_gba_hudbox() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        let base = std::fs::read(st).unwrap();
        let shot = |g: &mut Gba, slot_b: u8, slot_a: u8| {
            assert!(g.load_state(&base));
            assert!(g.poke_many(&[(0x2002B34, &[0x44]), (0x2002B33, &[0x40]), (0x2002AEE, &[1]), (0x2002AEC, &[10]), (0x2002AF4, &[slot_a]), (0x2002AF5, &[slot_b])]));
            for _ in 0..4 {
                g.run_frame(0);
            }
            g.frame.clone()
        };
        // nur Sprites: Hintergründe aus
        for l in 0..4 {
            unsafe { core::pl_mgba_enable_layer(l, false) };
        }
        let spr = shot(&mut g, 11, 7);
        let px: Vec<crate::gfx::Color32> = spr.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
        crate::devtest::bmp(&format!("{out}/hud_sprites.bmp"), W, H, &px);
        for l in 0..4 {
            unsafe { core::pl_mgba_enable_layer(l, true) };
        }
        let none = shot(&mut g, 0, 0);
        for (name, b, a) in [("B", 11u8, 0u8), ("A", 0, 11)] {
            let with = shot(&mut g, b, a);
            let (mut x0, mut y0, mut x1, mut y1) = (999, 999, 0, 0);
            for y in 0..H {
                for x in 0..W {
                    let i = (y * W + x) * 4;
                    if none[i..i + 3] != with[i..i + 3] {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            println!("{name}-Feld: x {x0}..{x1}, y {y0}..{y1}");
        }
        let px: Vec<crate::gfx::Color32> = none.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
        crate::devtest::bmp(&format!("{out}/hud_none.bmp"), W, H, &px);
    }

    /// Y-Taste: Bumerang auf Y, B leer – Y wirft den Bumerang, danach ist B wieder leer; Symbole speichern
    #[test]
    #[ignore]
    fn dev_gba_y() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        assert!(g.is_minish());
        assert!(g.load_state(&std::fs::read(st).unwrap()));
        assert!(g.poke(minish::EQUIP_B, &[0]));
        for _ in 0..10 {
            g.run_frame(0);
        }
        println!("vorhanden: {:?}", g.owned_items());
        let mut icons = Vec::new();
        for i in [0u8, 7, 9, 11] {
            let mut img = g.capture_icon(i).unwrap();
            if i == 0 {
                super::ui::letter_y(&mut img);
            }
            icons.push(img);
        }
        let mut px = Vec::new();
        for y in 0..ICON_H {
            for img in &icons {
                for x in 0..ICON_W {
                    let o = (y * ICON_W + x) * 4;
                    px.push(if img[o + 3] == 0 { crate::gfx::Color32::from_rgb(255, 0, 255) } else { crate::gfx::Color32::from_rgb(img[o], img[o + 1], img[o + 2]) });
                }
            }
        }
        crate::devtest::bmp(&format!("{out}/icons.bmp"), ICON_W * icons.len(), ICON_H, &px);
        g.cfg.y_item = 11;
        for f in 0..40u32 {
            g.run_frame(if f < 12 { 0x4000 } else { 0 });
            if f == 14 {
                let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                crate::devtest::bmp(&format!("{out}/y_throw.bmp"), W, H, &px);
            }
        }
        let b = g.peek(minish::EQUIP_B, 1).unwrap()[0];
        println!("B nach Y: {b}");
        assert_eq!(b, 0);
    }

    /// Picori: Spielstand `GBA_RC_STATE` (gepackter .rs-Stand) laden, Anzeige oben rechts je Hintergrundebene und
    /// Item-Symbole aufnehmen (`PASTLINK_OUT`)
    #[test]
    #[ignore]
    fn dev_rc_icons() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        if std::env::var_os("GBA_WIDE").is_some() {
            g.set_view(16, 9, true);
        }
        let t0 = std::time::Instant::now();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
            assert!(t0.elapsed().as_secs() < 60, "Picori startet nicht");
        }
        println!("bereit nach {:.1} s", t0.elapsed().as_secs_f32());
        let raw = std::fs::read(&st).ok().and_then(|b| lz4_flex::decompress_size_prepended(&b).ok());
        if !raw.is_some_and(|r| g.load_state(&r)) {
            // kein passender Stand: mit dem Spielstand der .sav ins Spiel (A/Start drücken, bis die Anzeige da ist)
            println!("Stand passt nicht – spiele ins Spiel");
            let mut f = 0u32;
            // Tastenfolge aus GBA_KEYS: „taste:bild,…“ (s=Start, a=A, u/d/l/r), danach A alle 40 Bilder
            let script: Vec<(u16, u32)> = std::env::var("GBA_KEYS").unwrap_or_default().split(',').filter_map(|e| {
                let (k, at) = e.split_once(':')?;
                let bit = match k { "s" => 0x1000, "a" => 0x0080, "b" => 0x8000, "u" => 0x0800, "d" => 0x0400, "l" => 0x0200, "r" => 0x0100, _ => return None };
                Some((bit, at.parse().ok()?))
            }).collect();
            let minf: u32 = std::env::var("GBA_MINF").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            let last = script.iter().map(|s| s.1).max().unwrap_or(0);
            while !(g.hud_visible() && f >= minf) {
                let k = if f <= last + 3 {
                    script.iter().filter(|(_, at)| f >= *at && f < at + 3).fold(0, |a, (b, _)| a | b)
                } else if f % 25 < 3 { 0x0080 } else { 0 };
                g.run_frame(k);
                f += 1;
                if f % std::env::var("GBA_DUMP").ok().and_then(|v| v.parse().ok()).unwrap_or(1000u32) == 0 {
                    let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                    crate::devtest::bmp(&format!("{out}/walk{f:05}.bmp"), g.w, g.h, &px);
                }
                assert!(f < std::env::var("GBA_MAXF").ok().and_then(|v| v.parse().ok()).unwrap_or(6000u32), "nicht ins Spiel gekommen");
            }
            println!("im Spiel nach {f} Bildern");
            for _ in 0..400 {
                g.run_frame(0);
            }
            if std::env::var_os("GBA_NOITEMS").is_none() {
            g.run_once("22002B32 5554
22002B34 5555
22002B36 5554
22002B38 5500");
            g.run_frame(0);
            let own = g.owned_items();
            println!("Items: {own:?}");
            g.equip(false, own[own.len() / 2]);
            g.equip(true, own[1]);
            }
            for _ in 0..30 {
                g.run_frame(0);
            }
            let b = g.save_state().unwrap();
            std::fs::write(&st, lz4_flex::compress_prepend_size(&b)).unwrap();
        }
        for _ in 0..20 {
            g.run_frame(0);
        }
        let dump = |g: &Gba, name: &str| {
            let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/{name}.bmp"), g.w, g.h, &px);
        };
        dump(&g, "rc_full");
        println!("Bild {}x{}, hud {}, B={:?} A={:?}", g.w, g.h, g.hud_visible(), g.peek(minish::EQUIP_B, 1), g.peek(minish::EQUIP_A, 1));
        let st2 = g.save_state().unwrap();
        // Vergleich: B-Feld im Spielbild gegen die Aufnahme des leeren Felds (GBA_RINGTEST)
        if std::env::var_os("GBA_RINGTEST").is_some() {
            let (bx, by) = g.b_box().unwrap();
            let frame = g.frame.clone();
            let fw = g.w;
            let cap = g.capture_icon(0).unwrap();
            let mut lines = String::new();
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    let i = ((by as usize + y) * fw + bx as usize + x) * 4;
                    let f = &frame[i..i + 3];
                    let o = (y * ICON_W + x) * 4;
                    let c = &cap[o..o + 4];
                    lines.push(if c[3] == 0 { if f[0] < 60 && f[1] < 60 && f[2] < 80 { '#' } else { '.' } } else if c[..3] == *f { '=' } else { 'x' });
                }
                lines.push('\n');
            }
            println!("Ring-Vergleich (. Hintergrund, # dunkel im Bild aber nicht aufgenommen, = gleich, x anders):\n{lines}");
            let mut yi = cap.clone();
            super::ui::letter_y(&mut yi);
            let mut l2 = String::new();
            for yy in 0..ICON_H {
                for x in 0..ICON_W {
                    let o = (yy * ICON_W + x) * 4;
                    l2.push(if cap[o + 3] == 0 {
                        '.'
                    } else if cap[o..o + 4] == yi[o..o + 4] {
                        '='
                    } else if yi[o..o + 3] == [0x19, 0x27, 0x2e] {
                        'Y'
                    } else {
                        'o'
                    });
                }
                l2.push('\n');
            }
            println!("B -> Y (= gleich, Y neuer Buchstabe, o alter Buchstabe überdeckt):\n{l2}");
        }
        // Item-Symbole (wie fürs Y-Feld): leeres Feld, einige Items
        let own = g.owned_items();
        let mut icons = Vec::new();
        for &i in [0u8].iter().chain(own.iter().take(7)) {
            if let Some(mut img) = g.capture_icon(i) {
                if i == 0 {
                    super::ui::letter_y(&mut img);
                }
                icons.push(img);
            }
        }
        let mut px = Vec::new();
        for y in 0..ICON_H {
            for img in &icons {
                for x in 0..ICON_W {
                    let o = (y * ICON_W + x) * 4;
                    px.push(if img[o + 3] == 0 { crate::gfx::Color32::from_rgb(255, 0, 255) } else { crate::gfx::Color32::from_rgb(img[o], img[o + 1], img[o + 2]) });
                }
            }
        }
        crate::devtest::bmp(&format!("{out}/rc_icons.bmp"), ICON_W * icons.len().max(1), ICON_H, &px);
        let bare = strip_ring(&icons[1..]);
        let mut px = Vec::new();
        for y in 0..ICON_H {
            for img in &bare {
                for x in 0..ICON_W {
                    let o = (y * ICON_W + x) * 4;
                    px.push(if img[o + 3] == 0 { crate::gfx::Color32::from_rgb(255, 0, 255) } else { crate::gfx::Color32::from_rgb(img[o], img[o + 1], img[o + 2]) });
                }
            }
        }
        if !bare.is_empty() {
            crate::devtest::bmp(&format!("{out}/rc_items.bmp"), ICON_W * bare.len(), ICON_H, &px);
        }
        for m in [0, 1, 2, 4, 8] {
            g.rc_frame(0, Some(m));
            g.rc_frame(0, Some(m));
            dump(&g, &format!("rc_layer{m}"));
            g.load_state(&st2);
        }
        // Karten-Cheat, dann Item-Menü (Start) und mit R/L zur Karte blättern (GBA_MAPTEST)
        if std::env::var_os("GBA_MAPTEST").is_some() {
            g.run_once("22002B42 4000");
            println!("Karte im Inventar: {:?}", g.peek(minish::INVENTORY + 18, 1));
            for f in 0..400 {
                let k = match f {
                    0..=2 => 0x1000,
                    120..=122 => 0x0020,
                    260..=262 => 0x0010,
                    _ => 0,
                };
                g.run_frame(k);
                if (118..135).contains(&f) {
                    println!("  f{f} Seite {} Tabelle {:?} ITEM_MAP {} Wert {}", g.rc_debug(5), (1..5).map(|i| g.rc_debug(i)).collect::<Vec<_>>(), g.rc_debug(6), g.rc_debug(7));
                }
                if f == 119 || f == 250 || f == 399 {
                    dump(&g, &format!("rc_page{f}"));
                    println!("Bild {f}: Karte {:?} Spiel-Karte {} Seiten {:?} Seite {}", g.peek(minish::INVENTORY + 18, 1), g.rc_debug(0), (1..5).map(|i| g.rc_debug(i)).collect::<Vec<_>>(), g.rc_debug(5));
                }
            }
            g.load_state(&st2);
        }
        // Item-Menü (Start)
        for f in 0..90 {
            g.run_frame(if f < 3 { 0x1000 } else { 0 });
        }
        dump(&g, "rc_menu");
        if let Some((px, w, h)) = g.capture_sprites() {
            let c: Vec<crate::gfx::Color32> = px.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/rc_menu_spr.bmp"), w, h, &c);
        }
        println!("Menü: hud {} Cursor {:?}", g.hud_visible(), g.menu_item());
        for &i in &g.owned_items() {
            println!("  Item {i}: {:?}", g.menu_pos(i));
        }
        g.load_state(&st2);
        // Spielstand speichern/laden: gleicher Stand, gleiche Bilder
        g.run_frame(0);
        let a = g.frame.clone();
        g.load_state(&st2);
        g.run_frame(0);
        assert_eq!(a, g.frame, "Laden ergibt ein anderes Bild");
    }

    /// Picori: jedes Item einzeln prüfen – Y-Feld (leerer Y-Kreis + Inventar-Grafik) gegen das echte B-Feld desselben
    /// Items. Abweichen darf nur der Buchstabe. Schreibt `rc_items_check.bmp` (je Item: B-Feld, Y-Feld, Kästchen der
    /// Item-Leiste) und gibt je Item die Zahl der abweichenden Pixel außerhalb des Buchstabens aus.
    #[test]
    #[ignore]
    fn dev_rc_items() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        for _ in 0..20 {
            g.run_frame(0);
        }
        if std::env::var_os("GBA_ALLITEMS").is_some() {
            // aus dem Intro heraus: aufs Feld springen (Intro gilt dann als vorbei)
            for _ in 0..400 {
                if g.warp(0x00, 0x00, 500, 500) {
                    break;
                }
                g.run_frame(0x0080);
                g.run_frame(0);
            }
            for _ in 0..300 {
                g.run_frame(0);
            }
            g.run_once("22002B32 5554
22002B34 5555
22002B36 5554
22002B38 5500
22002B3C 5555
32002AEC 0017
32002AED 0021");
            for _ in 0..30 {
                g.run_frame(0);
            }
        }
        // Item-Menü öffnen, alle Items als Figuren aufnehmen
        for f in 0..90 {
            g.run_frame(if f < 3 { 0x1000 } else { 0 });
        }
        if g.menu_item().is_none() {
            let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/items_fail.bmp"), g.w, g.h, &c);
            println!("Ansicht {:?} Task {}", g.map_view(), g.rc_debug(8));
        }
        let cursor = g.menu_item().expect("Item-Menü nicht offen");
        let (spr, w, h) = g.capture_sprites().unwrap();
        let own = g.owned_items();
        let mut inv = std::collections::HashMap::new();
        for &i in &own {
            let pos = g.menu_pos(i).or_else(|| g.menu_pos(match i { 7 => 8, 8 => 7, 15 => 16, x => x }));
            if let Some((cx, cy)) = pos {
                if !minish::COUNTED.contains(&i) {
                    inv.insert(i, crop_inv_icon(&spr, w, h, cx + w.saturating_sub(W) / 2, cy, cursor == i));
                }
            }
        }
        // Menü schließen
        for f in 0..90 {
            g.run_frame(if f < 3 { 0x1000 } else { 0 });
        }
        assert!(g.menu_item().is_none(), "Item-Menü noch offen");
        // so baut die Leiste: Item-Menü unsichtbar öffnen und aufnehmen (Spielstand danach zurück)
        let menu_icons = g.capture_menu_icons();
        println!("aus dem Item-Menü: {:?}", { let mut k: Vec<u8> = menu_icons.keys().copied().collect(); k.sort(); k });
        let ring = g.capture_icon(0).expect("leeres Feld");
        let mut base = ring.clone();
        super::ui::letter_y(&mut base);
        let mut cols: Vec<Vec<crate::gfx::Color32>> = Vec::new();
        let mut off_all = None;
        println!("Item  Abweichung(außer Buchstabe)  Lage");
        for &i in &own {
            let Some(truth) = g.capture_icon(i) else {
                println!("{i:>4}  keine B-Aufnahme");
                continue;
            };
            let empty = vec![0u8; INV_ICON * INV_ICON * 4];
            let item = inv.get(&i).unwrap_or(&empty);
            let off = find_item_offset(&truth, item);
            if off_all.is_none() {
                off_all = off;
            }
            let ours = if std::env::var_os("GBA_FROM_INV").is_some() { compose_slot(&base, item, off.or(off_all).unwrap_or((1, 4))) } else { slot_from_b_nocount(&base, &truth, &ring, i, off.or(off_all).unwrap_or((1, 4))) };
            // Abweichung außerhalb des Buchstabenbereichs (Mitte des Kreises)
            let mut bad = 0;
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    let o = (y * ICON_W + x) * 4;
                    let letter = (10..=15).contains(&x) && (18..=24).contains(&y);
                    let same = (truth[o + 3] == 0 && ours[o + 3] == 0) || truth[o..o + 4] == ours[o..o + 4];
                    if !same && !letter {
                        bad += 1;
                    }
                }
            }
            println!("{i:>4}  {bad:>4}  {off:?}  {}", minish::name(i, false));
            let to_c = |im: &[u8], o: usize| if im[o + 3] == 0 { crate::gfx::Color32::from_rgb(255, 0, 255) } else { crate::gfx::Color32::from_rgb(im[o], im[o + 1], im[o + 2]) };
            let mut derived = item_from_b(&truth, &ring, off_all.unwrap_or((1, 4)), minish::COUNTED.contains(&i));
            clean_counted(&mut derived, i);
            let tile = bar_tile_count(if inv.contains_key(&i) { item } else { &derived }, false, match i { 7 | 8 => Some(23), 9 | 10 => Some(7), _ => None });
            let mut col = Vec::new();
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    col.push(to_c(&truth, (y * ICON_W + x) * 4));
                }
            }
            for y in 0..ICON_H {
                for x in 0..ICON_W {
                    col.push(to_c(&ours, (y * ICON_W + x) * 4));
                }
            }
            for y in 0..TILE_H {
                for x in 0..ICON_W {
                    col.push(if x < TILE_W { tile[y * TILE_W + x] } else { crate::gfx::Color32::from_rgb(255, 0, 255) });
                }
            }
            // so baut die Leiste jetzt: reine Item-Aufnahme ohne Kreis, Fenster mittig
            let clean = menu_icons.get(&i).cloned().unwrap_or_else(|| derived.clone());
            let tile2 = bar_tile(&clean, false);
            for y in 0..TILE_H {
                for x in 0..ICON_W {
                    col.push(if x < TILE_W { tile2[y * TILE_W + x] } else { crate::gfx::Color32::from_rgb(255, 0, 255) });
                }
            }
            cols.push(col);
        }
        let hh = ICON_H * 2 + TILE_H * 2;
        let mut px = Vec::new();
        for y in 0..hh {
            for col in &cols {
                px.extend_from_slice(&col[y * ICON_W..(y + 1) * ICON_W]);
            }
        }
        crate::devtest::bmp(&format!("{out}/rc_items_check.bmp"), ICON_W * cols.len().max(1), hh, &px);
    }

    /// Picori: B-Feld im echten Bild neben der Aufnahme (Schatten), Item-Grafiken des Item-Menüs mit Rand (unten abgeschnitten?)
    #[test]
    #[ignore]
    fn dev_rc_shadow() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        for _ in 0..400 {
            if g.warp(0x00, 0x00, 500, 500) {
                break;
            }
            g.run_frame(0x0080);
            g.run_frame(0);
        }
        for _ in 0..300 {
            g.run_frame(0);
        }
        g.run_once("22002B32 5554
22002B34 5555
22002B36 5554
22002B38 5500");
        for _ in 0..20 {
            g.run_frame(0);
        }
        let to_c = |im: &[u8], o: usize| if im[o + 3] == 0 { crate::gfx::Color32::from_rgb(255, 0, 255) } else { crate::gfx::Color32::from_rgb(im[o], im[o + 1], im[o + 2]) };
        g.equip(false, 6);
        for _ in 0..10 {
            g.run_frame(0);
        }
        let (bx, by) = g.b_box().unwrap();
        println!("B-Feld bei {bx},{by}, Bild {}x{}", g.w, g.h);
        // echtes Bild: B-Feld mit 8 Pixel Rand
        let (rw, rh) = (ICON_W + 16, ICON_H + 32);
        let mut img = Vec::new();
        for y in 0..rh {
            for x in 0..rw {
                let (fx, fy) = (bx - 8 + x as isize, by - 24 + y as isize);
                img.push(if fx >= 0 && fy >= 0 && (fx as usize) < g.w && (fy as usize) < g.h { let i = (fy as usize * g.w + fx as usize) * 4; crate::gfx::Color32::from_rgb(g.frame[i], g.frame[i + 1], g.frame[i + 2]) } else { crate::gfx::Color32::BLACK });
            }
        }
        crate::devtest::bmp(&format!("{out}/sh_frame.bmp"), rw, rh, &img);
        for it in [0u8, 7, 17] {
            if let Some(c) = g.capture_icon(it) {
                let v: Vec<crate::gfx::Color32> = (0..ICON_W * ICON_H).map(|i| to_c(&c, i * 4)).collect();
                crate::devtest::bmp(&format!("{out}/sh_cap{it}.bmp"), ICON_W, ICON_H, &v);
            }
        }
        // Item-Menü: Grafiken mit großem Rand
        for f in 0..90 {
            g.run_frame(if f < 3 { 0x1000 } else { 0 });
        }
        let (spr, w, h) = g.capture_sprites().unwrap();
        let v: Vec<crate::gfx::Color32> = (0..w * h).map(|i| to_c(&[spr[i * 4], spr[i * 4 + 1], spr[i * 4 + 2], 255], 0)).collect();
        crate::devtest::bmp(&format!("{out}/sh_menu.bmp"), w, h, &v);
        for i in g.owned_items() {
            if let Some((cx, cy)) = g.menu_pos(i) {
                println!("Item {i:>2} Anker {cx},{cy}");
            }
        }
    }

    /// Picori: Fundorte zählen – je Art gesamt, mit Raum, mit bekanntem Stand; ohne Raum auflisten
    #[test]
    #[ignore]
    fn dev_rc_locstats() {
        let Ok(p) = std::env::var("PASTLINK_GBA_ROM") else { return };
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = g.locations(super::tmcmap::LOGIC);
        let mut st: std::collections::BTreeMap<String, (usize, usize, usize)> = Default::default();
        let mut missing = Vec::new();
        for (name, key, ty) in &raw {
            if name.contains("Dummy") || name.starts_with("Fake") || [1, 2, 5, 6, 7].contains(ty) {
                continue;
            }
            let k = super::tmcmap::LocKind::from_name(name);
            let e = st.entry(format!("{k:?}")).or_default();
            e.0 += 1;
            let placed = *key != u32::MAX && key & 0x8000_0000 == 0 && g.room_rect((key >> 16) as u8, (key >> 8) as u8).is_some();
            if placed {
                e.1 += 1;
            } else if matches!(k, super::tmcmap::LocKind::HeartPiece | super::tmcmap::LocKind::Chest) {
                missing.push(format!("{name} ({ty}, {key:08x})"));
            }
        }
        println!("{st:?}");
        println!("Herzteile/Truhen ohne Ort ({}):", missing.len());
        for m in &missing {
            println!("  {m}");
        }
    }

    /// Picori: Schnellreise in einen Raum, danach Link auf freie Stelle
    #[test]
    #[ignore]
    fn dev_rc_travel() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        let (a, r) = (0x03u8, 0x06u8);
        let rr = g.room_rect(a, r).unwrap();
        let mut f = 0;
        while !g.travel(a, r, rr[2] / 2, rr[3] / 2) && f < 600 {
            g.run_frame(if f % 20 < 2 { 0x0080 } else { 0 });
            f += 1;
        }
        println!("Sprung nach {f} Bildern");
        for _ in 0..200 {
            g.run_frame(0);
        }
        println!("vorher {:?}", g.map_view());
        println!("frei gestellt: {}", g.place_safe());
        for _ in 0..30 {
            g.run_frame(0);
        }
        println!("nachher {:?}", g.map_view());
        let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
        crate::devtest::bmp(&format!("{out}/travel.bmp"), g.w, g.h, &c);
    }

    /// Picori: No-Clip – gleiche Strecke nach oben laufen ohne/mit Cheat
    #[test]
    #[ignore]
    fn dev_rc_noclip() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        let rr = g.room_rect(0x03, 0x06).unwrap();
        let mut f = 0;
        while !g.travel(0x03, 0x06, rr[2] / 2, rr[3] / 2) && f < 600 {
            g.run_frame(if f % 20 < 2 { 0x0080 } else { 0 });
            f += 1;
        }
        for _ in 0..200 {
            g.run_frame(0);
        }
        let base = g.save_state().unwrap();
        for dir in [0x0800u16, 0x0400, 0x0200, 0x0100] {
            for on in [false, true] {
                assert!(g.load_state(&base));
                g.set_noclip(on);
                let a = g.map_view().unwrap();
                for _ in 0..240 {
                    g.run_frame(dir);
                }
                let b = g.map_view().unwrap();
                println!("Schalter in der DLL: {}", g.rc_debug(9));
                println!("Richtung {dir:04x} No-Clip {on}: Link von {},{} nach {},{} (Raum {:02X}/{:02X})", a[9], a[10], b[9], b[10], b[0], b[1]);
            }
        }
        g.set_noclip(false);
    }

    /// Picori: Bombe legen und explodieren lassen – mit/ohne Ausblendung der Ziffern (Figuren unverändert?)
    #[test]
    #[ignore]
    fn dev_rc_bomb() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        let rr = g.room_rect(0x03, 0x06).unwrap();
        let mut f = 0;
        while !g.travel(0x03, 0x06, rr[2] / 2, rr[3] / 2) && f < 600 {
            g.run_frame(if f % 20 < 2 { 0x0080 } else { 0 });
            f += 1;
        }
        for _ in 0..200 {
            g.run_frame(0);
        }
        let _ = g.locations(super::tmcmap::LOGIC);
        g.cheat_complete();
        for _ in 0..10 {
            g.run_frame(0);
        }
        g.equip(false, 7);
        g.equip(true, 7);
        g.poke(0x0200_2AEC, &[30]);
        for _ in 0..20 {
            g.run_frame(0);
        }
        let base = g.save_state().unwrap();
        let mut shots: Vec<Vec<Vec<u8>>> = Vec::new();
        for hide in [false, true] {
            assert!(g.load_state(&base));
            g.set_hide_counts(hide);
            let mut v = Vec::new();
            for f in 0..200 {
                g.run_frame(if (30..34).contains(&f) { 0x8000 } else if (40..44).contains(&f) { 0x0080 } else { 0 });
                if f % 10 == 0 {
                    v.push(g.frame.clone());
                    if hide && (f == 60 || f == 120 || f == 140 || f == 160) {
                        let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                        crate::devtest::bmp(&format!("{out}/bomb_{f}.bmp"), g.w, g.h, &c);
                    }
                }
            }
            shots.push(v);
        }
        let w = g.w;
        for (k, (a, b)) in shots[0].iter().zip(shots[1].iter()).enumerate() {
            // Unterschiede außerhalb der oberen Anzeige (Zeilen ab 32)
            let mut d = 0;
            for i in (w * 32 * 4..a.len().min(b.len())).step_by(4) {
                if a[i..i + 3] != b[i..i + 3] {
                    d += 1;
                }
            }
            println!("Bild {:3}: {d} Pixel verschieden (außerhalb der Anzeige)", k * 10);
        }
        g.set_hide_counts(true);
    }

    /// Picori: 100-%-Cheat – was danach noch fehlt
    #[test]
    #[ignore]
    fn dev_rc_complete() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        for _ in 0..30 {
            g.run_frame(0);
        }
        let locs = g.locations(super::tmcmap::LOGIC);
        println!("vorher Items {:?} Fortschritt {:?}", g.owned_items().len(), g.progress());
        println!("Cheat: {} Fundorte gesetzt", g.cheat_complete());
        for _ in 0..30 {
            g.run_frame(0);
        }
        println!("nachher Items {} {:?}", g.owned_items().len(), g.owned_items());
        println!("Fortschritt {:?}", g.progress());
        let (mut yes, mut no, mut unk) = (0, 0, 0);
        for (n, key, ty) in &locs {
            if !(*ty == 3 || (8..=13).contains(ty)) || n.contains("Dummy") || n.starts_with("Fake") || *key == u32::MAX {
                continue;
            }
            match g.loc_done(*key) {
                Some(true) => yes += 1,
                Some(false) => {
                    no += 1;
                    if no <= 15 {
                        println!("  fehlt: {n} {key:08x}");
                    }
                }
                None => unk += 1,
            }
        }
        println!("Fundorte: geholt {yes}, fehlt {no}, unbekannt {unk}");
        // Gebiet wechseln und zurück: bleiben die Flags?
        for (a, r) in [(0x03u8, 0x06u8), (0x02, 0x00), (0x03, 0x01)] {
            let rr = g.room_rect(a, r).unwrap();
            let mut f = 0;
            while !g.travel(a, r, rr[2] / 2, rr[3] / 2) && f < 600 {
                g.run_frame(if f % 20 < 2 { 0x0080 } else { 0 });
                f += 1;
            }
            for _ in 0..240 {
                g.run_frame(0);
            }
        }
        let (mut yes2, mut no2) = (0, 0);
        for (n, key, ty) in &locs {
            if !(*ty == 3 || (8..=13).contains(ty)) || n.contains("Dummy") || n.starts_with("Fake") || *key == u32::MAX {
                continue;
            }
            match g.loc_done(*key) {
                Some(true) => yes2 += 1,
                Some(false) => {
                    no2 += 1;
                    if no2 <= 10 {
                        println!("  danach fehlt: {n} {key:08x}");
                    }
                }
                None => {}
            }
        }
        println!("nach Gebietswechseln: geholt {yes2}, fehlt {no2}; Items {}; Fortschritt {:?}", g.owned_items().len(), g.progress());
    }

    /// Picori: ganze Karte vorab erstellen (alle Räume), Dauer messen, Übersichtsbilder je Raum speichern
    #[test]
    #[ignore]
    fn dev_rc_mapgen() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = std::fs::read(&st).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()).unwrap_or_default();
        if !g.load_state(&raw) {
            let mut f = 0u32;
            let mut in_files = 0u32;
            while !(g.hud_visible() && f > 500) && f < 20000 {
                let task = g.rc_debug(8) / 10000;
                in_files = if task == 1 { in_files + 1 } else { 0 };
                let k = match task {
                    0 => if f % 60 < 3 { 0x1000 } else { 0 },
                    1 if in_files < 60 => 0,
                    1 if in_files < 200 => if in_files % 40 < 3 { 0x0800 } else { 0 },
                    _ => if f % 40 < 3 { 0x0080 } else if f % 40 > 20 && f % 40 < 23 { 0x1000 } else { 0 },
                };
                g.run_frame(k);
                f += 1;
            }
            for _ in 0..200 {
                g.run_frame(0);
            }
            std::fs::write(&st, lz4_flex::compress_prepend_size(&g.save_state().unwrap())).unwrap();
        }
        if std::env::var_os("GBA_WIDE").is_some() {
            g.set_view(16, 9, true);
        }
        let mut d = super::tmcmap::MapData::default();
        let t0 = std::time::Instant::now();
        let mut gen = super::tmcmap::MapGen::new(&mut g).expect("nicht im Spiel");
        let mut last = 0.0;
        while !gen.done {
            gen.step(&mut g, &mut d, std::time::Duration::from_millis(500));
            if gen.progress() - last > 0.1 {
                last = gen.progress();
                println!("{:.0} % nach {:.0} s", last * 100.0, t0.elapsed().as_secs_f32());
            }
        }
        println!("fertig in {:.0} s, ausgelassen {}: {:02X?}", t0.elapsed().as_secs_f32(), gen.skipped.len(), &gen.skipped[..gen.skipped.len().min(40)]);
        println!("nach dem Laden wieder: {:?}", g.map_view());
        let mut spaces: Vec<u16> = d.chunks.keys().map(|k| k.0).filter(|&s| s != 0xFFFF).collect();
        spaces.sort();
        spaces.dedup();
        println!("Räume auf der Karte: {}", spaces.len());
        let cnt = |k: u8| d.marks.iter().filter(|m| m.1 == k).count();
        println!("Punkte: Schrumpfpunkte {}, Windwappen {}, Bohnenranken {}", cnt(1), cnt(2), cnt(3));
        println!("Türen: {} (Oberwelt {})", d.doors.len(), d.doors.iter().filter(|x| x.sp == 0).count());
        for area in 0u8..0x90 {
            if g.area_kind(area) & 1 != 0 {
                let rr: Vec<String> = (0..g.room_count(area) as u8).filter_map(|r| g.room_rect(area, r).map(|v| format!("{r:02X}@{},{} {}x{}", v[0], v[1], v[2], v[3]))).collect();
                println!("Oberwelt {area:02X}: {}", rr.join("  "));
            }
        }
        for sp in spaces {
            let Some((x0, y0, x1, y1)) = d.bounds(sp) else { continue };
            let sc = (((x1 - x0).max(y1 - y0) as f32) / 1024.0).ceil().max(1.0) as i32;
            let (w, h) = (((x1 - x0) / sc) as usize, ((y1 - y0) / sc) as usize);
            let mut img = vec![crate::gfx::Color32::from_rgb(20, 24, 52); w * h];
            for y in 0..h {
                for x in 0..w {
                    let (wx, wy) = (x0 + x as i32 * sc, y0 + y as i32 * sc);
                    let k = (sp, wx.div_euclid(256), wy.div_euclid(256));
                    if let Some(c) = d.chunks.get(&k) {
                        let o = ((wy.rem_euclid(256) * 256 + wx.rem_euclid(256)) * 4) as usize;
                        if c[o + 3] != 0 {
                            img[y * w + x] = crate::gfx::Color32::from_rgb(c[o], c[o + 1], c[o + 2]);
                        }
                    }
                }
            }
            crate::devtest::bmp(&format!("{out}/gen_{sp:03}.bmp"), w, h, &img);
        }
    }

    /// Picori: Grundlagen der Karte – Ansicht beim Laufen, Bild ohne Figuren, Fundorte mit Position und Stand
    #[test]
    #[ignore]
    fn dev_rc_mapbase() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let mut g = Gba::load_with(Path::new(&p), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = std::fs::read(st).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()).unwrap_or_default();
        if !g.load_state(&raw) {
            // Stand passt nicht zur DLL: vom Spielstand (.sav) aus ins Spiel drücken
            let mut f = 0;
            let mut in_files = 0u32;
            while !(g.hud_visible() && f > 500) && f < 20000 {
                let task = g.rc_debug(8) / 10000;
                in_files = if task == 1 { in_files + 1 } else { 0 };
                let k = match task {
                    0 => if f % 60 < 3 { 0x1000 } else { 0 },
                    1 if in_files < 60 => 0,
                    1 if in_files < 200 => if in_files % 40 < 3 { 0x0800 } else { 0 },
                    _ => if f % 40 < 3 { 0x0080 } else if f % 40 > 20 && f % 40 < 23 { 0x1000 } else { 0 },
                };
                g.run_frame(k);
                if f % 500 == 0 {
                    let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                    crate::devtest::bmp(&format!("{out}/boot{f:04}.bmp"), g.w, g.h, &c);
                }
                f += 1;
            }
            for _ in 0..200 {
                g.run_frame(0);
            }
            println!("im Spiel nach {f} Bildern");
            if let Ok(o) = std::env::var("GBA_SAVE_TO") {
                std::fs::write(o, lz4_flex::compress_prepend_size(&g.save_state().unwrap())).unwrap();
            }
        }
        for f in 0..120 {
            g.run_frame(if f > 20 { 0x0100 } else { 0 });
            if f % 20 == 0 {
                println!("f{f} Ansicht {:?}", g.map_view());
            }
        }
        if let Some((px, w, h)) = g.map_render_bg() {
            let c: Vec<crate::gfx::Color32> = px.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/rc_bg.bmp"), w, h, &c);
            let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/rc_fg.bmp"), g.w, g.h, &c);
        }
        g.set_view(16, 9, true);
        for _ in 0..30 {
            g.run_frame(0);
        }
        println!("breit: Ansicht {:?}", g.map_view());
        if let Some((px, w, h)) = g.map_render_bg() {
            let c: Vec<crate::gfx::Color32> = px.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
            crate::devtest::bmp(&format!("{out}/rc_bg_wide.bmp"), w, h, &c);
        }
        let locs = g.locations(super::tmcmap::LOGIC);
        println!("Fundorte: {}", locs.len());
        let mut kinds = std::collections::HashMap::new();
        let mut placed = 0;
        for (n, key, ty) in &locs {
            let k = super::tmcmap::LocKind::from_name(n);
            *kinds.entry(format!("{k:?}")).or_insert(0) += 1;
            if key & 0x8000_0000 == 0 && *key != u32::MAX {
                let (a, r) = ((key >> 16) as u8, (key >> 8) as u8);
                if g.room_rect(a, r).is_some() {
                    placed += 1;
                }
            }
            let _ = ty;
        }
        println!("Arten: {kinds:?}  mit Raum: {placed}");
        for (n, key, ty) in locs.iter().take(25) {
            println!("  {n} key {key:08x} typ {ty} fertig {:?} Gebiet-Art {}", g.loc_done(*key), g.area_kind((key >> 16) as u8));
        }
    }

    /// Erkennung der Spielanzeige: Logo (weiß), Titel, Spiel (`GBA_STATE`), Item-Menü
    #[test]
    #[ignore]
    fn dev_gba_hud() {
        let (Ok(p), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_STATE")) else { return };
        let mut g = Gba::load(Path::new(&p)).unwrap();
        g.no_save = true;
        for _ in 0..100 {
            g.run_frame(0);
        }
        let logo = g.hud_visible();
        for _ in 0..500 {
            g.run_frame(0);
        }
        let title = g.hud_visible();
        assert!(g.load_state(&std::fs::read(st).unwrap()));
        for _ in 0..10 {
            g.run_frame(0);
        }
        let game = g.hud_visible();
        let dump = |g: &Gba, name: &str| {
            println!("{name}:");
            for y in 8..26 {
                let row: String = (202..232).map(|x| { let i = (y * W + x) * 4; let l = (g.frame[i] as u32 * 3 + g.frame[i + 1] as u32 * 6 + g.frame[i + 2] as u32) / 10; [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'][(l as usize * 10 / 256).min(9)] }).collect();
                println!("  {y:2} {row}");
            }
        };
        dump(&g, "Spiel");
        let icon = g.capture_icon(0).is_some();
        for f in 0..220 {
            g.run_frame(if (30..40).contains(&f) { 0x1000 } else { 0 });
        }
        let menu = g.hud_visible();
        let px: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
        crate::devtest::bmp(&format!("{}/hud_menu.bmp", std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into())), W, H, &px);
        dump(&g, "Menü");
        println!("Anzeige: Logo {logo}, Titel {title}, Spiel {game}, Item-Menü {menu}; Symbol im Spiel gültig {icon}");
        assert!(!logo && !title && game && icon);
    }
}
