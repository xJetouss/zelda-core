//! Interaktive Weltkarte + 100%-Tracker.
//!
//! Die Karte wird beim Spielen automatisch aus der eigenen ROM aufgebaut: Alle paar Bilder rendert
//! die PPU zusätzlich nur das Gelände (BG1+BG2, ohne HUD und Figuren), das an der aktuellen
//! Kameraposition in eine 1024×1024-Weltkarte (Maßstab 1:4) kopiert wird.
//! Herzteile und Herzcontainer werden automatisch markiert, sobald sie auf dem Bildschirm sind.

use crate::alttp::{ram16, ram8};
use crate::snes::ppu::{H, W};
use crate::snes::Snes;
use crate::gfx::Color32;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCALE: usize = 4;
pub const DIM: usize = 4096 / SCALE;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum MarkerKind {
    HeartPiece,
    HeartContainer,
    Chest,
    Item,
    Note,
}

impl MarkerKind {
    pub const ALL: [MarkerKind; 5] = [MarkerKind::HeartPiece, MarkerKind::HeartContainer, MarkerKind::Chest, MarkerKind::Item, MarkerKind::Note];
    pub fn glyph(self) -> &'static str {
        match self {
            MarkerKind::HeartPiece => "♥",
            MarkerKind::HeartContainer => "❤",
            MarkerKind::Chest => "■",
            MarkerKind::Item => "★",
            MarkerKind::Note => "✎",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            MarkerKind::HeartPiece => Color32::from_rgb(255, 90, 110),
            MarkerKind::HeartContainer => Color32::from_rgb(255, 40, 60),
            MarkerKind::Chest => Color32::from_rgb(240, 190, 70),
            MarkerKind::Item => Color32::from_rgb(110, 200, 255),
            MarkerKind::Note => Color32::from_rgb(220, 220, 220),
        }
    }
    pub fn name(self, en: bool) -> &'static str {
        match (self, en) {
            (MarkerKind::HeartPiece, false) => "Herzteil",
            (MarkerKind::HeartPiece, true) => "Piece of Heart",
            (MarkerKind::HeartContainer, false) => "Herzcontainer",
            (MarkerKind::HeartContainer, true) => "Heart Container",
            (MarkerKind::Chest, false) => "Truhe",
            (MarkerKind::Chest, true) => "Chest",
            (MarkerKind::Item, false) => "Item",
            (MarkerKind::Item, true) => "Item",
            (MarkerKind::Note, false) => "Notiz",
            (MarkerKind::Note, true) => "Note",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Marker {
    pub world: u8,
    pub x: f32,
    pub y: f32,
    pub kind: MarkerKind,
    pub label: String,
    pub done: bool,
    pub auto: bool,
}

pub struct Tracker {
    pub canvas: [Vec<Color32>; 2],
    pub markers: Vec<Marker>,
    pub tex_dirty: [bool; 2],
    pub save_dirty: bool,
    frame: u32,
    capturing: bool,
    last_hearts: Vec<(u8, f32, f32, MarkerKind)>,
    /// (Welt, x, y) im Weltkoordinatensystem (0..4096)
    pub link: Option<(u8, f32, f32)>,
    pub explored: [usize; 2],
    /// Komplette Weltkarte (aus dem Kartenbildschirm des Spiels), 1024×1024
    pub full: [Option<Vec<Color32>>; 2],
    pub full_dirty: [bool; 2],
    full_cooldown: u32,
    job: Option<std::sync::mpsc::Receiver<Option<[Vec<Color32>; 2]>>>,
    /// Fest in der ROM hinterlegte Herzteile/Herzcontainer auf der Oberwelt
    pub statics: Vec<StaticItem>,
}

#[derive(Clone, Copy)]
pub struct StaticItem {
    pub world: u8,
    pub area: u8,
    pub x: f32,
    pub y: f32,
    pub kind: MarkerKind,
}

/// Dungeons: (Welt, x, y, Deutsch, Englisch) – Positionen anhand der Weltkarte des Spiels.
pub const DUNGEONS: [(u8, f32, f32, &str, &str); 13] = [
    (0, 2048.0, 2048.0, "Hyrule-Schloss", "Hyrule Castle"),
    (0, 3584.0, 2048.0, "Ostpalast", "Eastern Palace"),
    (0, 320.0, 3413.0, "Wüstenpalast", "Desert Palace"),
    (0, 2290.0, 90.0, "Turm des Herakles", "Tower of Hera"),
    (1, 3584.0, 2048.0, "Palast der Finsternis", "Palace of Darkness"),
    (1, 1910.0, 3910.0, "Sumpfpalast", "Swamp Palace"),
    (1, 512.0, 512.0, "Skelettwald", "Skull Woods"),
    (1, 512.0, 2048.0, "Diebesversteck", "Thieves Town"),
    (1, 3227.0, 3584.0, "Eispalast", "Ice Palace"),
    (1, 512.0, 3584.0, "Moorpalast", "Misery Mire"),
    (1, 3890.0, 190.0, "Schildkrötenfels", "Turtle Rock"),
    (1, 2290.0, 90.0, "Ganons Turm", "Ganons Tower"),
    (1, 2048.0, 2048.0, "Pyramide", "Pyramid"),
];

fn c15(c: u16) -> Color32 {
    let r = (c & 31) as u8;
    let g = ((c >> 5) & 31) as u8;
    let b = ((c >> 10) & 31) as u8;
    Color32::from_rgb(r << 3 | r >> 2, g << 3 | g >> 2, b << 3 | b >> 2)
}

/// Sucht die Oberwelt-Spritelisten (je 3 Byte: Y, X, Sprite-ID; Ende 0xFF).
/// Aufbau in der ROM: 64 Zeiger (Spielbeginn) + 144 Zeiger (Hauptspiel) + 144 Zeiger (nach Agahnim).
/// Aus der Hauptspiel-Tabelle (Index 0x00-0x7F = Gebiete, 0x80+ = Sondergebiete) werden alle
/// Herzteile (0xEB) und Herzcontainer (0xEA) gelesen.
pub fn find_static_items(rom: &[u8]) -> Vec<StaticItem> {
    fn list(rom: &[u8], mut o: usize) -> Option<Vec<(u8, u8, u8)>> {
        let mut v = Vec::new();
        for _ in 0..80 {
            let y = *rom.get(o)?;
            if y == 0xFF {
                return Some(v);
            }
            let x = *rom.get(o + 1)?;
            let id = *rom.get(o + 2)?;
            if y >= 64 || x >= 64 {
                return None;
            }
            v.push((y, x, id));
            o += 3;
        }
        None
    }
    const BEGIN: usize = 64;
    const MAIN: usize = 144;
    for bank in 0..rom.len() / 0x8000 {
        let base = bank * 0x8000;
        let mut t = base;
        while t + (BEGIN + MAIN) * 2 < base + 0x8000 {
            let mut lists = Vec::new();
            loop {
                let o = t + lists.len() * 2;
                if o + 1 >= base + 0x8000 || lists.len() >= BEGIN + 2 * MAIN {
                    break;
                }
                let p = rom[o] as usize | (rom[o + 1] as usize) << 8;
                if p < 0x8000 {
                    break;
                }
                match list(rom, base + p - 0x8000) {
                    Some(l) => lists.push(l),
                    None => break,
                }
            }
            if lists.len() < BEGIN + MAIN {
                t += 1;
                continue;
            }
            let main = &lists[BEGIN..BEGIN + MAIN];
            let hearts = main.iter().flatten().filter(|s| s.2 == 0xEB).count();
            let total: usize = main.iter().map(|l| l.len()).sum();
            if hearts < 3 || total < 100 {
                t += 1;
                continue;
            }
            let mut out: Vec<StaticItem> = Vec::new();
            for (i, l) in main.iter().enumerate() {
                for &(y, x, id) in l {
                    let kind = match id {
                        0xEB => MarkerKind::HeartPiece,
                        0xEA => MarkerKind::HeartContainer,
                        _ => continue,
                    };
                    let (lx, ly) = (x as f32 * 16.0 + 8.0, y as f32 * 16.0 + 8.0);
                    let (world, wx, wy) = if i < 0x80 {
                        let a = (i & 0x3F) as u8;
                        ((i >= 0x40) as u8, (a & 7) as f32 * 512.0 + lx, (a >> 3) as f32 * 512.0 + ly)
                    } else if i == 0x81 {
                        // Zoras Wasserfall: Sondergebiet oben rechts in der Lichtwelt
                        (0, 3584.0 + lx / 2.0, ly / 2.0)
                    } else {
                        continue;
                    };
                    if !out.iter().any(|s| s.world == world && (s.x - wx).abs() < 20.0 && (s.y - wy).abs() < 20.0) {
                        out.push(StaticItem { world, area: i as u8, x: wx, y: wy, kind });
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// Liest die komplette Mode-7-Weltkarte aus dem VRAM (128×128 Kacheln à 8×8 = 1024×1024).
/// Die Welt (4096×4096) liegt im Maßstab 1:16 bei 128..384 – sie wird ausgeschnitten und auf
/// 1024×1024 (1:4) vergrößert. Beim Spielen erkundete Gebiete werden detailliert darübergelegt.
fn m7_image(s: &Snes) -> Vec<Color32> {
    let ppu = &s.bus.ppu;
    let px = |x: usize, y: usize| -> Color32 {
        let tile = (ppu.vram[(y / 8) * 128 + x / 8] & 0xFF) as usize;
        let ci = (ppu.vram[(tile * 64 + (y % 8) * 8 + x % 8) & 0x7FFF] >> 8) as usize;
        c15(ppu.cgram[ci])
    };
    let mut img = vec![Color32::BLACK; DIM * DIM];
    for y in 0..DIM {
        for x in 0..DIM {
            img[y * DIM + x] = px(128 + x / 4, 128 + y / 4);
        }
    }
    img
}

/// Öffnet auf einer Kopie des laufenden Spiels kurz den Kartenbildschirm (X) und liest ihn aus.
fn capture_world_map(s: &Snes, world: u8) -> Option<Vec<Color32>> {
    capture_clone(s, world).and_then(|c| {
        let img = m7_image(&c);
        let mut colors = std::collections::HashSet::new();
        for p in img.iter().step_by(97) {
            colors.insert(*p);
        }
        (colors.len() > 12).then_some(img)
    })
}

fn capture_clone(s: &Snes, world: u8) -> Option<Snes> {
    let mut c = s.clone();
    c.bus.ppu.capture = false;
    c.bus.ppu.ext = 0;
    c.bus.ppu.record = false;
    c.bus.ppu.skip = false;
    let area = c.bus.wram[0x8A];
    if world == 1 {
        c.bus.wram[0xF3CA] = 0x40;
        c.bus.wram[0x8A] = area | 0x40;
    } else {
        c.bus.wram[0xF3CA] = 0;
        c.bus.wram[0x8A] = area & 0x3F;
    }
    let mut stable = 0;
    let mut seen = false;
    for f in 0..240 {
        let press = f < 3 || (!seen && (90..93).contains(&f));
        c.bus.joy[0] = if press { 0x0040 } else { 0 };
        c.run_frame();
        c.bus.apu.dsp.out.clear();
        if c.bus.ppu.mode() == 7 {
            seen = true;
            if c.bus.ppu.brightness() == 15 {
                stable += 1;
                if stable >= 10 {
                    break;
                }
                continue;
            }
        }
        stable = 0;
    }
    if stable < 10 {
        return None;
    }
    Some(c)
}

/// Startet das Spiel unsichtbar (Spielstand aus dem SRAM, Prolog übersprungen), läuft bis auf die
/// Oberwelt und nimmt dort beide Weltkarten auf.
pub fn boot_capture(rom: std::sync::Arc<Vec<u8>>, mut sram: Vec<u8>) -> Option<[Vec<Color32>; 2]> {
    crate::alttp::skip_prologue_sram(&mut sram);
    let valid = (0..3).any(|k| sram.len() >= k * 0x500 + 0x3E7 && sram[k * 0x500 + 0x3E5] == 0xAA && sram[k * 0x500 + 0x3E6] == 0x55);
    if !valid {
        return None;
    }
    let mut s = Snes::new(rom, sram.len(), false);
    s.bus.sram = sram;
    let mut ok = false;
    for f in 0..6000u32 {
        let m = ram8(&s, 0x7E0010);
        if m == 0x09 && ram8(&s, 0x7E0011) == 0 && s.bus.ppu.brightness() == 15 && f > 60 {
            ok = true;
            break;
        }
        s.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            // im Haus: A für Texte, sonst nach unten zur Tür
            _ => if f % 16 < 2 { 0x0080 } else { 0x0400 },
        };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    }
    if !ok {
        return None;
    }
    for _ in 0..30 {
        s.bus.joy[0] = 0;
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    }
    let a = capture_world_map(&s, 0)?;
    let b = capture_world_map(&s, 1)?;
    Some([a, b])
}

fn overworld(s: &Snes) -> Option<u8> {
    let module = ram8(s, 0x7E0010);
    let area = ram8(s, 0x7E008A);
    if module == 0x09 && area < 0x80 {
        Some((area >= 0x40) as u8)
    } else {
        None
    }
}

impl Tracker {
    pub fn new() -> Self {
        Tracker {
            canvas: [vec![Color32::TRANSPARENT; DIM * DIM], vec![Color32::TRANSPARENT; DIM * DIM]],
            markers: Vec::new(),
            tex_dirty: [true; 2],
            save_dirty: false,
            frame: 0,
            capturing: false,
            last_hearts: Vec::new(),
            link: None,
            explored: [0; 2],
            full: [None, None],
            full_dirty: [true; 2],
            full_cooldown: 120,
            job: None,
            statics: Vec::new(),
        }
    }





    /// Versucht selten und auf einer Kopie im Hintergrund, die komplette Weltkarte zu erfassen.
    pub fn maybe_capture_full(&mut self, s: &Snes, rom: &Path) {
        if self.full[0].is_some() && self.full[1].is_some() {
            return;
        }
        // Hintergrund-Aufnahme beim Start fertig?
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(Some([a, b])) => {
                    self.job = None;
                    self.full = [Some(a), Some(b)];
                    self.full_dirty = [true; 2];
                    self.save_full(rom);
                    return;
                }
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(_) => return,
            }
        }
        if self.full_cooldown > 0 {
            self.full_cooldown -= 1;
            return;
        }
        if overworld(s).is_none() || ram8(s, 0x7E0011) != 0 || s.bus.ppu.brightness() != 15 {
            self.full_cooldown = 60;
            return;
        }
        self.full_cooldown = 900;
        let mut changed = false;
        for w in 0..2u8 {
            if self.full[w as usize].is_none() {
                if let Some(img) = capture_world_map(s, w) {
                    if w == 1 && self.full[0].as_ref() == Some(&img) {
                        continue;
                    }
                    self.full[w as usize] = Some(img);
                    self.full_dirty[w as usize] = true;
                    changed = true;
                }
            }
        }
        if changed && self.full[0].is_some() && self.full[1].is_some() {
            self.save_full(rom);
        }
    }

    fn map_file(rom: &Path) -> std::path::PathBuf {
        rom.with_extension("worldmap")
    }

    /// Fertige Weltkarte neben der ROM speichern (wird nur einmal erstellt).
    pub fn save_full(&self, rom: &Path) {
        let (Some(a), Some(b)) = (&self.full[0], &self.full[1]) else { return };
        let mut raw = Vec::with_capacity(DIM * DIM * 6);
        for img in [a, b] {
            for c in img {
                raw.extend_from_slice(&[c.r(), c.g(), c.b()]);
            }
        }
        let _ = std::fs::write(Self::map_file(rom), lz4_flex::compress_prepend_size(&raw));
    }

    /// Gespeicherte Weltkarte laden; fehlt sie, wird sie im Hintergrund einmal erstellt.
    pub fn load_or_capture(&mut self, rom_path: &Path, rom: std::sync::Arc<Vec<u8>>, sram: Vec<u8>) {
        if let Some(raw) = std::fs::read(Self::map_file(rom_path)).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()) {
            if raw.len() == DIM * DIM * 6 {
                for w in 0..2 {
                    let part = &raw[w * DIM * DIM * 3..(w + 1) * DIM * DIM * 3];
                    self.full[w] = Some(part.chunks_exact(3).map(|p| Color32::from_rgb(p[0], p[1], p[2])).collect());
                    self.full_dirty[w] = true;
                }
                return;
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("worldmap".into()).spawn(move || {
            let _ = tx.send(boot_capture(rom, sram));
        });
        self.job = Some(rx);
    }

    #[allow(dead_code)]
    pub fn recapture_full(&mut self) {
        self.job = None;
        self.full = [None, None];
        self.full_dirty = [true; 2];
        self.full_cooldown = 0;
    }

    /// Neuer Tracker für eine ROM – liest und schreibt keine Dateien.
    pub fn for_rom(rom_data: &[u8]) -> Self {
        let mut t = Tracker::new();
        t.statics = find_static_items(rom_data);
        t
    }



    /// Vor jedem Bild: entscheidet, ob die PPU das Gelände mitzeichnet.
    pub fn pre_frame(&mut self, s: &mut Snes) {
        self.frame = self.frame.wrapping_add(1);
        // erkundete Gebiete werden nicht mehr übermalt (Nacht/Regen im Prolog hat die Karte verdunkelt)
        self.capturing = false;
        s.bus.ppu.capture = self.capturing;
    }

    /// Nach jedem Bild: Karte, Link-Position und Herzteil-Marker aktualisieren.
    pub fn post_frame(&mut self, s: &mut Snes) {
        let world = overworld(s);
        let lx = ram16(s, 0x7E0022) as f32;
        let ly = ram16(s, 0x7E0020) as f32;
        // Mitte von Links Körper (in Häusern/Höhlen/Dungeons zeigt die Karte den benutzten Eingang)
        self.link = world.map(|w| (w, lx + 8.0, ly + 14.0));

        if self.capturing {
            s.bus.ppu.capture = false;
            self.capturing = false;
            if let Some(w) = world {
                let cx = ram16(s, 0x7E00E2) as i32;
                let cy = ram16(s, 0x7E00E8) as i32;
                let dx = lx as i32 - cx;
                let dy = ly as i32 - cy;
                let fade_ok = s.bus.ppu.brightness() == 15;
                if (0..256).contains(&dx) && (0..224).contains(&dy) && cx < 4096 && cy < 4096 && fade_ok {
                    self.blit(w as usize, cx, cy + 1, &s.bus.ppu.cap);
                }
            }
        }

        // automatische Herzteil-/Herzcontainer-Erkennung
        let mut now = Vec::new();
        if let Some(w) = world {
            for i in 0..16u32 {
                if ram8(s, 0x7E0DD0 + i) == 0 {
                    continue;
                }
                let kind = match ram8(s, 0x7E0E20 + i) {
                    0xEB => MarkerKind::HeartPiece,
                    0xEA => MarkerKind::HeartContainer,
                    _ => continue,
                };
                let x = (ram8(s, 0x7E0D10 + i) as u16 | (ram8(s, 0x7E0D30 + i) as u16) << 8) as f32 + 8.0;
                let y = (ram8(s, 0x7E0D00 + i) as u16 | (ram8(s, 0x7E0D20 + i) as u16) << 8) as f32 + 8.0;
                now.push((w, x, y, kind));
                let exists = self.markers.iter().any(|m| m.world == w && m.kind == kind && (m.x - x).abs() < 32.0 && (m.y - y).abs() < 32.0);
                if !exists {
                    self.markers.push(Marker { world: w, x, y, kind, label: String::new(), done: false, auto: true });
                    self.save_dirty = true;
                }
            }
            // verschwunden, während Link daneben stand -> eingesammelt
            for &(lw, x, y, kind) in &self.last_hearts {
                let still = now.iter().any(|&(nw, nx, ny, _)| nw == lw && (nx - x).abs() < 12.0 && (ny - y).abs() < 12.0);
                if !still && lw == w && (lx + 8.0 - x).abs() < 40.0 && (ly + 12.0 - y).abs() < 40.0 {
                    for m in self.markers.iter_mut() {
                        if m.world == lw && m.kind == kind && (m.x - x).abs() < 32.0 && (m.y - y).abs() < 32.0 && !m.done {
                            m.done = true;
                            self.save_dirty = true;
                        }
                    }
                }
            }
        }
        self.last_hearts = now;
    }

    fn blit(&mut self, w: usize, cx: i32, cy: i32, cap: &[u16]) {
        let canvas = &mut self.canvas[w];
        let mut added = 0;
        for by in 0..(H / SCALE) {
            for bx in 0..(W / SCALE) {
                let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
                for yy in 0..SCALE {
                    for xx in 0..SCALE {
                        let c = cap[(by * SCALE + yy) * W + bx * SCALE + xx] as u32;
                        r += c & 31;
                        g += (c >> 5) & 31;
                        b += (c >> 10) & 31;
                    }
                }
                let n = (SCALE * SCALE) as u32;
                let wx = (cx + (bx * SCALE) as i32) / SCALE as i32;
                let wy = (cy + (by * SCALE) as i32) / SCALE as i32;
                if wx < 0 || wy < 0 || wx >= DIM as i32 || wy >= DIM as i32 {
                    continue;
                }
                let idx = wy as usize * DIM + wx as usize;
                if canvas[idx].a() == 0 {
                    added += 1;
                }
                canvas[idx] = Color32::from_rgb((r * 8 / n) as u8, (g * 8 / n) as u8, (b * 8 / n) as u8);
            }
        }
        self.explored[w] += added;
        self.tex_dirty[w] = true;
        self.save_dirty = true;
    }
}

// ---------------------------------------------------------------- 100%-Fortschritt

#[allow(dead_code)]
pub struct Progress {
    pub max_hearts: u8,
    pub pieces: u8,
    pub pendants: u8,
    pub crystals: u8,
    pub items_owned: Vec<bool>,
}

pub fn progress(s: &Snes) -> Progress {
    let items_owned = crate::alttp::INVENTORY
        .iter()
        .map(|&(a, _, _)| {
            if a == 0x7EF35C {
                (0..4).any(|i| ram8(s, a + i) != 0)
            } else if a == 0x7EF35B {
                ram8(s, a) != 0
            } else {
                ram8(s, a) != 0
            }
        })
        .collect();
    Progress {
        max_hearts: ram8(s, 0x7EF36C) / 8,
        pieces: ram8(s, 0x7EF36B) & 3,
        pendants: (ram8(s, 0x7EF374) & 7).count_ones() as u8,
        crystals: (ram8(s, 0x7EF37A) & 0x7F).count_ones() as u8,
        items_owned,
    }
}

#[cfg(test)]
mod real_rom_tests {
    //! Entwickler-Test mit einer echten ROM: `PASTLINK_ROM=pfad cargo test --release real_rom -- --ignored --nocapture`
    use super::*;
    use std::sync::Arc;

    fn bmp(path: &str, w: usize, h: usize, px: &[Color32]) {
        let row = (w * 3 + 3) & !3;
        let mut d = b"BM".to_vec();
        for v in [(54 + row * h) as u32, 0, 54, 40, w as u32, h as u32] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, (row * h) as u32, 2835, 2835, 0, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for y in (0..h).rev() {
            for x in 0..w {
                let c = px[y * w + x];
                d.extend_from_slice(&[c.b(), c.g(), c.r()]);
            }
            d.resize(d.len() + row - w * 3, 0);
        }
        std::fs::write(path, d).unwrap();
    }

    #[test]
    #[ignore]
    fn real_rom() {
        let Ok(path) = std::env::var("PASTLINK_ROM") else { return };
        let out = std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into());
        let p = std::path::PathBuf::from(&path);
        let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
        println!("Titel '{}' alttp={} pal={} crc={:08X} sram={}", info.title, info.is_alttp, info.pal, info.crc, info.sram_size);
        let t = crate::randomizer::find_chest_table(&rom);
        match &t {
            Some(t) => println!("Truhentabelle @0x{:X}: {} Truhen, {} groß", t.offset, t.chests.len(), t.chests.iter().filter(|c| c.big).count()),
            None => println!("Truhentabelle NICHT gefunden"),
        }
        let st = find_static_items(&rom);
        let (locs, _) = crate::world::build_locations(&rom, t.as_ref(), &st, &crate::world::find_entrances(&rom), &crate::world::find_dungeon_maps(&rom));
        let mut f = String::new();
        for l in &locs {
            let k = match l.kind { crate::world::LocKind::HeartPiece => "H", crate::world::LocKind::HeartContainer => "C", crate::world::LocKind::Chests => "T", crate::world::LocKind::Minigame => "M" };
            f.push_str(&format!("{k};{};{:.0};{:.0};{};{}
", l.world, l.x, l.y, l.total(), l.name_de));
        }
        std::fs::write(format!("{out}/locs.txt"), f).unwrap();
        println!("Orte: {} (Herzteile {}, Container {}, Truhengruppen {}, Truhen gesamt {})", locs.len(),
            locs.iter().filter(|l| matches!(l.kind, crate::world::LocKind::HeartPiece | crate::world::LocKind::Minigame)).count(),
            locs.iter().filter(|l| l.kind == crate::world::LocKind::HeartContainer).count(),
            locs.iter().filter(|l| l.kind == crate::world::LocKind::Chests).count(),
            locs.iter().filter(|l| l.kind == crate::world::LocKind::Chests).map(|l| l.total()).sum::<usize>());
        println!("Statische Herzteile/-container: {}", st.len());
        for s in &st {
            println!("  welt {} gebiet {:02X} ({:.0},{:.0}) {:?}", s.world, s.area, s.x, s.y, s.kind);
        }
        let mut snes = Snes::new(Arc::new(rom.clone()), info.sram_size, info.pal);
        if let Ok(d) = std::fs::read(p.with_extension("srm")) {
            snes.bus.sram = d;
        }
        if std::env::var("PASTLINK_PROLOG").is_ok() {
            let mut sk = snes.bus.sram.clone();
            println!("Prolog-Skip im SRAM: {}", crate::alttp::skip_prologue_sram(&mut sk));
            if let Ok(v) = std::env::var("PASTLINK_POKE") {
                for kv in v.split(',') {
                    let (k, val) = kv.split_once('=').unwrap();
                    sk[usize::from_str_radix(k, 16).unwrap()] = u8::from_str_radix(val, 16).unwrap();
                }
                let sum = (0..0x4FE).step_by(2).fold(0u16, |a, i| a.wrapping_add(sk[i] as u16 | (sk[i + 1] as u16) << 8));
                let ck = 0x5A5Au16.wrapping_sub(sum);
                sk[0x4FE] = ck as u8;
                sk[0x4FF] = (ck >> 8) as u8;
            }
            let sum = (0..0x500).step_by(2).fold(0u16, |a, i| a.wrapping_add(sk[i] as u16 | (sk[i + 1] as u16) << 8));
            println!("Pruefsumme Slot 1 = {sum:04X}");
            snes.bus.sram = sk;
        }
        match std::fs::read(p.with_extension(if std::env::var("PASTLINK_NOSTATE").is_ok() { "none" } else { "ss1" })) {
            Ok(d) => println!("Savestate laden: {:?}", snes.load_state(&d)),
            Err(_) => println!("kein Savestate"),
        }
        for f in 0..4000 {
            if ram8(&snes, 0x7E0010) == 0x09 && ram8(&snes, 0x7E0011) == 0 && f > 60 {
                break;
            }
            // Titel/Demo: Start, Dateiauswahl: A, im Haus: nach unten laufen
            let m = ram8(&snes, 0x7E0010);
            snes.bus.joy[0] = match m {
                0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
                0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
                _ => {
                    let lx = ram16(&snes, 0x7E0022);
                    if f % 16 < 2 {
                        0x0080
                    } else if (f / 120) % 3 == 0 {
                        0x0400
                    } else if lx < 2416 {
                        0x0100
                    } else if lx > 2432 {
                        0x0200
                    } else {
                        0x0400
                    }
                }
            };
            snes.run_frame();
            if f % 250 == 0 {
                println!("f={f} modus={:02X} sub={:02X} state5D={:02X} link=({},{}) {} F0={:02X} F2={:02X} F4={:02X} F6={:02X}", ram8(&snes, 0x7E0010), ram8(&snes, 0x7E0011), ram8(&snes, 0x7E005D), ram16(&snes, 0x7E0022), ram16(&snes, 0x7E0020), snes.bus.debug_io(), ram8(&snes, 0x7E00F0), ram8(&snes, 0x7E00F2), ram8(&snes, 0x7E00F4), ram8(&snes, 0x7E00F6));
            }
        }
        snes.bus.joy[0] = 0;
        for _ in 0..20 {
            snes.run_frame();
        }
        println!("Fortschritt 3C5={:02X} 3C6={:02X} 3C8={:02X} Schwert={:02X} Modus={:02X} Raum={:04X} Link=({},{})", ram8(&snes, 0x7EF3C5), ram8(&snes, 0x7EF3C6), ram8(&snes, 0x7EF3C8), ram8(&snes, 0x7EF359), ram8(&snes, 0x7E0010), ram16(&snes, 0x7E00A0), ram16(&snes, 0x7E0022), ram16(&snes, 0x7E0020));
        let px: Vec<Color32> = snes.framebuffer().iter().map(|&c| c15(c)).collect();
        bmp(&format!("{out}/game.bmp"), 256, 224, &px);
        let (mut tmp, mut hd) = (Vec::new(), Vec::new());
        let t0 = std::time::Instant::now();
        let sz = crate::filters::apply(crate::filters::Filter::Hd, &px, 256, 224, &mut tmp, &mut hd);
        println!("xBR 4x: {:?}", t0.elapsed());
        bmp(&format!("{out}/hd_xbr.bmp"), sz[0], sz[1], &hd);
        let sz = crate::filters::apply(crate::filters::Filter::HdSharp, &px, 256, 224, &mut tmp, &mut hd);
        bmp(&format!("{out}/hd_scale4x.bmp"), sz[0], sz[1], &hd);
        let sz = crate::filters::apply(crate::filters::Filter::HdClean, &px, 256, 224, &mut tmp, &mut hd);
        bmp(&format!("{out}/hd_clean.bmp"), sz[0], sz[1], &hd);
        println!("Modus {:02X} Sub {:02X} Gebiet {:02X} Link ({},{}) Kamera ({},{})", ram8(&snes, 0x7E0010), ram8(&snes, 0x7E0011), ram8(&snes, 0x7E008A), ram16(&snes, 0x7E0022), ram16(&snes, 0x7E0020), ram16(&snes, 0x7E00E2), ram16(&snes, 0x7E00E8));
        // ---- Leistungsmessung im echten Spiel
        {
            let mut b = snes.clone();
            let t0 = std::time::Instant::now();
            for _ in 0..300 {
                b.run_frame();
                b.bus.apu.dsp.out.clear();
            }
            let emu = t0.elapsed().as_secs_f64() / 300.0;
            let t1 = std::time::Instant::now();
            for _ in 0..100 {
                let _ = b.save_state();
            }
            let st = t1.elapsed().as_secs_f64() / 100.0;
            let t2 = std::time::Instant::now();
            for _ in 0..100 {
                let _ = b.clone();
            }
            let cl = t2.elapsed().as_secs_f64() / 100.0;
            let fpx: Vec<Color32> = b.framebuffer().iter().map(|&c| c15(c)).collect();
            let (mut tmp, mut o) = (Vec::new(), Vec::new());
            let t3 = std::time::Instant::now();
            for _ in 0..50 {
                crate::filters::apply(crate::filters::Filter::Hd, &fpx, 256, 224, &mut tmp, &mut o);
            }
            let hd = t3.elapsed().as_secs_f64() / 50.0;
            println!("BENCH Emulation {:.2} ms/Bild ({:.0} FPS möglich) · Savestate {:.2} ms · Klon {:.2} ms · xBR {:.2} ms", emu * 1e3, 1.0 / emu, st * 1e3, cl * 1e3, hd * 1e3);
        }
        // ---- Schnellmenü-Funktionen
        {
            let mut q = snes.clone();
            crate::alttp::give_all_items(&mut q);
            for _ in 0..5 { q.run_frame(); }
            let t0 = std::time::Instant::now();
            let icons = crate::quickmenu::capture_all(&q).map(|i| i.items);
            println!("QM Symbole: {:?} in {:?}", icons.as_ref().map(|v| v.iter().filter(|i| i.is_some()).count()), t0.elapsed());
            if let Some(icons) = &icons {
                let mut sheet = vec![Color32::BLACK; 80 * 5 * 80 * 4];
                for (k, ic) in icons.iter().enumerate().skip(1) {
                    if let Some(px) = ic {
                        let mut hd = Vec::new();
                        crate::filters::xbr4x_mode(px, 20, 20, &mut hd, false);
                        let (cx, cy) = (((k - 1) % 5) * 80, ((k - 1) / 5) * 80);
                        for y in 0..80 { for x in 0..80 { sheet[(cy + y) * 400 + cx + x] = hd[y * 80 + x]; } }
                    }
                }
                bmp(&format!("{out}/icons.bmp"), 400, 320, &sheet);
            }
            println!("QM Einträge: {}", crate::quickmenu::entries(&q).len());
            let t1 = std::time::Instant::now();
            let ok = crate::quickmenu::equip(&mut q, crate::quickmenu::Entry { idx: 6, bottle: None });
            println!("QM Ausrüsten Feuerstab: {ok} in {:?}, $0202={:02X}", t1.elapsed(), ram8(&q, 0x7E0202));
            let px: Vec<Color32> = q.framebuffer().iter().map(|&v| c15(v)).collect();
            bmp(&format!("{out}/qm_equip.bmp"), 256, 224, &px);
            if let Ok(target) = std::env::var("PASTLINK_STATE_ITEMS") {
                std::fs::write(&target, q.save_state()).unwrap();
                let px: Vec<u8> = q.framebuffer().iter().flat_map(|c| c.to_le_bytes()).collect();
                std::fs::write(format!("{target}.thumb"), lz4_flex::compress_prepend_size(&px)).unwrap();
            }
        }
        // ---- Item-Menü mit allen Items aufnehmen
        if std::env::var("PASTLINK_MENU").is_ok() {
            let mut c = snes.clone();
            crate::alttp::give_all_items(&mut c);
            for f in 0..120 {
                c.bus.joy[0] = if f < 3 { 0x1000 } else { 0 };
                c.run_frame();
                c.bus.apu.dsp.out.clear();
            }
            println!("MENU Modus {:02X} Sub {:02X} Auswahl $0202={:02X}", ram8(&c, 0x7E0010), ram8(&c, 0x7E0011), ram8(&c, 0x7E0202));
            let px: Vec<Color32> = c.framebuffer().iter().map(|&v| c15(v)).collect();
            bmp(&format!("{out}/menu.bmp"), 256, 224, &px);
            // Ausrüsten testen: Bogen (Index 1) direkt setzen
            let mut d = snes.clone();
            crate::alttp::give_all_items(&mut d);
            d.bus.wram[0x0202] = 12; // Hammer
            for _ in 0..30 {
                d.run_frame();
                d.bus.apu.dsp.out.clear();
            }
            let px: Vec<Color32> = d.framebuffer().iter().map(|&v| c15(v)).collect();
            bmp(&format!("{out}/equip.bmp"), 256, 224, &px);
            // Ausrüsten über unsichtbares Menü öffnen/schließen
            let mut e = snes.clone();
            crate::alttp::give_all_items(&mut e);
            e.bus.wram[0x0202] = 12; // Hammer
            let start_mod = ram8(&e, 0x7E0010);
            let mut frames = 0;
            let mut phase = 0;
            for f in 0..400 {
                let m = ram8(&e, 0x7E0010);
                let sub = ram8(&e, 0x7E0011);
                let press = match phase {
                    0 => {
                        if m == 0x0E { phase = 1; }
                        f % 4 < 2
                    }
                    1 => {
                        // Menue offen und ruhig?
                        if sub == 0x01 || f > 200 { phase = 2; }
                        false
                    }
                    2 => {
                        if m != 0x0E { phase = 3; }
                        f % 4 < 2
                    }
                    _ => false,
                };
                e.bus.joy[0] = if press { 0x1000 } else { 0 };
                e.run_frame();
                e.bus.apu.dsp.out.clear();
                frames = f;
                if phase == 3 && m == start_mod && ram8(&e, 0x7E0011) == 0 { break; }
            }
            println!("EQUIP {} Bilder, Modus {:02X}, $0202={:02X}", frames, ram8(&e, 0x7E0010), ram8(&e, 0x7E0202));
            for _ in 0..5 { e.run_frame(); }
            let px: Vec<Color32> = e.framebuffer().iter().map(|&v| c15(v)).collect();
            bmp(&format!("{out}/equip2.bmp"), 256, 224, &px);
            // Hintergrundfarbe im Menue auf Magenta setzen -> Icons freistellen
            c.bus.ppu.cgram[0] = 0x7C1F;
            for (i, m) in [1u8, 2, 4, 16].iter().enumerate() {
                c.bus.ppu.layer_mask = *m;
                c.run_frame();
                let px: Vec<Color32> = c.framebuffer().iter().map(|&v| c15(v)).collect();
                bmp(&format!("{out}/menu_layer{i}.bmp"), 256, 224, &px);
            }
        }
        if let Ok(target) = std::env::var("PASTLINK_STATE_OUT") {
            std::fs::write(&target, snes.save_state()).unwrap();
            let px: Vec<u8> = snes.framebuffer().iter().flat_map(|c| c.to_le_bytes()).collect();
            std::fs::write(format!("{target}.thumb"), lz4_flex::compress_prepend_size(&px)).unwrap();
            println!("Savestate geschrieben: {target}");
        }
        for w in 0..2u8 {
            let t0 = std::time::Instant::now();
            if let Some(c) = capture_clone(&snes, w) {
                let ppu = &c.bus.ppu;
                let raw: Vec<Color32> = (0..1024 * 1024)
                    .map(|i| {
                        let (x, y) = (i % 1024, i / 1024);
                        let tile = (ppu.vram[(y / 8) * 128 + x / 8] & 0xFF) as usize;
                        c15(ppu.cgram[(ppu.vram[(tile * 64 + (y % 8) * 8 + x % 8) & 0x7FFF] >> 8) as usize])
                    })
                    .collect();
                bmp(&format!("{out}/raw{w}.bmp"), 1024, 1024, &raw);
            }
            match capture_world_map(&snes, w) {
                Some(img) => {
                    println!("Weltkarte {w}: OK ({:?})", t0.elapsed());
                    bmp(&format!("{out}/map{w}.bmp"), DIM, DIM, &img);
                }
                None => println!("Weltkarte {w}: fehlgeschlagen ({:?})", t0.elapsed()),
            }
        }
        if let Some(t) = &t {
            let r = crate::randomizer::randomize(t, 1234, crate::randomizer::Mode::Mixed);
            println!("Randomizer Normal: {} Truhen verändert", r.spoiler.iter().filter(|x| x.2 != x.3).count());
            for (room, big, old, new) in r.spoiler.iter().take(8) {
                println!("  Raum {room:04X}{} {} -> {}", if *big { " (groß)" } else { "" }, crate::alttp::item_name(*old, false), crate::alttp::item_name(*new, false));
            }
        }
    }
}
