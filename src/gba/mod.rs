//! Game Boy Advance (z. B. „The Minish Cap“): Emulation mit mGBA, ohne Änderungen am Spiel.
//!
//! Hier: ein laufendes GBA-Spiel (`Gba`), Spielstände, Speicherdaten (`.sav` neben der ROM), Cheats und das
//! Titelbild für die Karte im Hauptmenü. Die Oberfläche steht in `ui.rs` (Bild, Ablauf) und `menus.rs`.

#[cfg(pastlink_gba)]
pub mod core;
pub mod ui;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const W: usize = 240;
pub const H: usize = 160;

/// Bereich des B-Felds in der Anzeige von The Minish Cap (inkl. Item darüber); das Y-Feld liegt 24 Pixel links
pub const ICON_X: usize = 174;
pub const ICON_Y: usize = 8;
pub const ICON_W: usize = 24;
pub const ICON_H: usize = 28;

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
    /// Inventar: je Item 2 Bit (Wert != 0 = vorhanden)
    pub const INVENTORY: u32 = 0x0200_2B32;
    /// Items, die man auf A/B legen kann (Nummern wie im Spiel); bei Schwert/Bumerang/Schild zählt die beste Stufe
    pub const EQUIPPABLE: &[u8] = &[6, 5, 4, 3, 2, 1, 7, 8, 9, 12, 11, 14, 13, 15, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30, 31];

    /// vorhandene Items aus dem Inventar (34 Bytes)
    pub fn owned(inv: &[u8]) -> Vec<u8> {
        let has = |i: u8| inv.get(i as usize >> 2).is_some_and(|b| (b >> ((i & 3) * 2)) & 3 != 0);
        let mut v = Vec::new();
        // nur die beste Stufe von Schwert, Bumerang, Schild
        for group in [&[6u8, 5, 4, 3, 2, 1][..], &[12, 11], &[14, 13]] {
            if let Some(&i) = group.iter().find(|&&i| has(i)) {
                v.push(i);
            }
        }
        for &i in &[7u8, 8, 9, 15, 17, 18, 19, 20, 21, 22, 23, 28, 29, 30, 31] {
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

/// Spielstand-Datei für Platz `slot` (`<rom>.gs1` …)
pub fn state_path(rom: &Path, slot: u8) -> PathBuf {
    rom.with_extension(format!("gs{slot}"))
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
    }

    impl Gba {
        pub fn load(path: &Path) -> Result<Gba, String> {
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
                temp_cheats: Vec::new(),
                y_phase: 0,
                y_b_orig: 0,
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
            if self.no_save {
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

        /// Ton seit dem letzten Abholen, umgerechnet auf 32 kHz Stereo (wie die Tonausgabe der App)
        pub fn take_audio(&mut self, out: &mut Vec<i16>) {
            let src: Vec<i16> = core::ST.lock().ok().and_then(|mut s| s.as_mut().map(|s| std::mem::take(&mut s.audio))).unwrap_or_default();
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
            unsafe { core::retro_reset() };
        }

        pub fn save_state(&self) -> Option<Vec<u8>> {
            let n = unsafe { core::retro_serialize_size() };
            let mut b = vec![0u8; n];
            (n > 0 && unsafe { core::retro_serialize(b.as_mut_ptr() as *mut _, n) }).then_some(b)
        }

        pub fn load_state(&mut self, b: &[u8]) -> bool {
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
                0 if y && item != 0 && self.is_minish() => {
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

        /// vorhandene Items (The Minish Cap), die man auf eine Taste legen kann
        pub fn owned_items(&self) -> Vec<u8> {
            self.peek(minish::INVENTORY, 34).map(|inv| minish::owned(&inv)).unwrap_or_default()
        }

        /// Symbol eines Items, wie es das Spiel im B-Feld zeichnet (24×28 RGBA, Hintergrund durchsichtig).
        /// Dazu wird das Item kurz auf B gelegt, mit ausgeblendeten Hintergründen gezeichnet und alles zurückgestellt –
        /// im Spiel sieht man davon nichts.
        pub fn capture_icon(&mut self, item: u8) -> Option<Vec<u8>> {
            let st = self.save_state()?;
            let frame = self.frame.clone();
            let (nf, fr) = (self.new_frame, self.frames);
            for l in 0..4 {
                unsafe { core::pl_mgba_enable_layer(l, false) };
            }
            let ok = self.poke(minish::EQUIP_B, &[item]);
            let mut img = None;
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
            for l in 0..4 {
                unsafe { core::pl_mgba_enable_layer(l, true) };
            }
            self.load_state(&st);
            self.frame = frame;
            self.new_frame = nf;
            self.frames = fr;
            img
        }

        pub fn apply_cheats(&mut self) {
            unsafe { core::retro_cheat_reset() };
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
        pub fn poke(&mut self, addr: u32, bytes: &[u8]) -> bool {
            self.poke_many(&[(addr, bytes)])
        }

        pub fn poke_many(&mut self, writes: &[(u32, &[u8])]) -> bool {
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
            let st = self.save_state()?;
            let off = addr.checked_sub(0x0200_0000)? as usize;
            st.get(0x21000 + off..0x21000 + off + n).map(|s| s.to_vec())
        }

        pub fn save_cfg(&self) {
            save_cfg(&self.path, &self.cfg);
        }

        pub fn set_option(&self, key: &str, value: &str) {
            core::set_var(key, value);
        }
    }

    impl Drop for Gba {
        fn drop(&mut self) {
            self.flush_sram();
            unsafe { core::retro_unload_game() };
            core::stop();
            BUSY.store(false, Ordering::SeqCst);
        }
    }

    /// Titelbild für die Karte im Hauptmenü: Spiel kurz ohne Ton laufen lassen und das Bild nehmen, sobald der
    /// Titel steht (zwischengespeichert als `<rom>.title.rgba`)
    pub fn title_image(path: &Path) -> Option<(Vec<u8>, usize, usize)> {
        let cache = path.with_extension("title.rgba");
        if let Ok(b) = std::fs::read(&cache) {
            if b.len() == W * H * 4 {
                return Some((b, W, H));
            }
        }
        let mut g = Gba::load(path).ok()?;
        g.no_save = true;
        // Titelbild nach den Logos (ohne Tastendruck – Start ginge am Titel vorbei)
        // Minish Cap: ab etwa Bild 360 steht der Titel mit Logo
        for _ in 0..600u32 {
            g.run_frame(0);
        }
        let img = g.frame.clone();
        drop(g);
        let _ = std::fs::write(&cache, &img);
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
    pub fn run_frame(&mut self, _: u16) {}
    pub fn take_audio(&mut self, _: &mut Vec<i16>) {}
    pub fn reset(&mut self) {}
    pub fn save_state(&self) -> Option<Vec<u8>> {
        None
    }
    pub fn load_state(&mut self, _: &[u8]) -> bool {
        false
    }
    pub fn apply_cheats(&mut self) {}
    pub fn save_cfg(&self) {}
    pub fn flush_sram(&mut self) {}
}

#[cfg(all(test, pastlink_gba))]
mod tests {
    use super::*;

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
}
