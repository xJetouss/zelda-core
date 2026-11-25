//! Karte für The Minish Cap (PC-Fassung Picori): Weltbild aus dem, was man beim Spielen gesehen hat, und Fundorte
//! (Truhen, Herzteile, Kinsteine …) aus der Randomizer-Logik von Picori, mit „schon eingesammelt“.
//!
//! Das Bild entsteht beim Spielen: regelmäßig wird das aktuelle Bild ohne Figuren (nur Hintergründe) an seiner
//! Weltposition in Kacheln eingetragen – wie ein Nebel, der sich lichtet. Gespeichert wird es neben der ROM
//! (`.tmcmap`, enthält Spielgrafik – nur lokal, nie weitergeben).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Kantenlänge einer Kachel (Pixel)
pub const CH: usize = 256;

/// Raum der Karte: 0 = Oberwelt (gemeinsame Koordinaten), sonst Gebiet + 1 (Häuser, Höhlen, Dungeons)
pub type Space = u16;

/// Logik-Datei von Picori (GPL-3.0, Teil von vendor/tmc): Namen und Orte aller Fundorte
pub const LOGIC: &str = include_str!("../../vendor/tmc/assets/rando/default.logic");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocKind {
    HeartPiece,
    HeartContainer,
    Chest,
    Kinstone,
    Figurine,
    Dig,
    Pot,
    Underwater,
    Rupees,
    Npc,
    Other,
}

impl LocKind {
    /// aus dem Namen in der Logik-Datei (englische Kurznamen wie „HyruleTownBellHP“)
    pub fn from_name(n: &str) -> LocKind {
        let l = n.to_ascii_lowercase();
        if l.contains("heartcontainer") || l.ends_with("hc") || l.contains("_hc") {
            LocKind::HeartContainer
        } else if l.ends_with("hp") || l.contains("heartpiece") || l.contains("hp_") || l.contains("hpchest") {
            LocKind::HeartPiece
        } else if l.contains("dig") {
            LocKind::Dig
        } else if l.contains("pot") {
            LocKind::Pot
        } else if l.contains("underwater") || l.contains("water") && l.contains("dive") {
            LocKind::Underwater
        } else if l.contains("kinstone") || l.contains("fusion") {
            LocKind::Kinstone
        } else if l.contains("figurine") {
            LocKind::Figurine
        } else if l.contains("rupee") || l.contains("mania") {
            LocKind::Rupees
        } else if l.contains("chest") || l.contains("bigchest") {
            LocKind::Chest
        } else if l.contains("npc") || l.contains("reward") || l.contains("gift") || l.contains("prize") || l.contains("shop") || l.contains("scrub") {
            LocKind::Npc
        } else {
            LocKind::Other
        }
    }

    pub fn label(self, en: bool) -> &'static str {
        let (de, e) = match self {
            LocKind::HeartPiece => ("Herzteile", "Heart pieces"),
            LocKind::HeartContainer => ("Herzcontainer", "Heart containers"),
            LocKind::Chest => ("Truhen", "Chests"),
            LocKind::Kinstone => ("Kinsteine", "Kinstones"),
            LocKind::Figurine => ("Figuren", "Figurines"),
            LocKind::Dig => ("Graben", "Digging"),
            LocKind::Pot => ("Töpfe", "Pots"),
            LocKind::Underwater => ("Unter Wasser", "Underwater"),
            LocKind::Rupees => ("Rubine", "Rupees"),
            LocKind::Npc => ("Belohnungen", "Rewards"),
            LocKind::Other => ("Sonstiges", "Other"),
        };
        if en {
            e
        } else {
            de
        }
    }

    /// Arten, die in der Logik vorkommen (Herzcontainer und Figuren stehen dort nicht als eigene Fundorte)
    pub const ALL: [LocKind; 9] = [
        LocKind::HeartPiece,
        LocKind::Chest,
        LocKind::Kinstone,
        LocKind::Npc,
        LocKind::Dig,
        LocKind::Pot,
        LocKind::Underwater,
        LocKind::Rupees,
        LocKind::Other,
    ];
}

/// Fundort mit Position auf der Karte
#[derive(Clone)]
pub struct Loc {
    pub name: String,
    pub key: u32,
    pub kind: LocKind,
    pub area: u8,
    pub room: u8,
    /// Raum auf der Karte und Mitte des Raums (Weltpixel)
    pub at: Option<(Space, f32, f32)>,
    pub done: Option<bool>,
}

/// Name lesbar machen: „HyruleTownBellHP“ → „Hyrule Town Bell HP“
pub fn pretty(n: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = n.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            out.push(' ');
            continue;
        }
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || (prev.is_uppercase() && next_lower) {
                out.push(' ');
            }
        }
        out.push(c);
    }
    let mut s = out.split_whitespace().collect::<Vec<_>>().join(" ");
    // Abkürzungen der Dungeons zusammenhalten
    for (a, b) in [("Co F", "CoF"), ("Fo W", "FoW"), ("Po W", "PoW"), ("To D", "ToD"), ("DH C", "DHC")] {
        s = s.replace(a, b);
    }
    s
}

#[derive(Default)]
pub struct MapData {
    /// Kacheln: (Raum, Kachel x, Kachel y) → RGBA (Alpha 0 = noch nicht gesehen)
    pub chunks: HashMap<(Space, i32, i32), Vec<u8>>,
    /// geänderte Kacheln (Textur neu laden)
    pub dirty: Vec<(Space, i32, i32)>,
    pub locs: Vec<Loc>,
    pub locs_loaded: bool,
    /// besondere Punkte (beim Erstellen gefunden): Raum, Art (1 Schrumpfpunkt, 2 Windwappen, 3 Bohnenranke), x, y
    pub marks: Vec<(Space, u8, f32, f32)>,
    /// seit dem letzten Speichern geändert
    pub unsaved: bool,
    pub path: Option<PathBuf>,
}

impl MapData {
    pub fn file_for(rom: &Path) -> PathBuf {
        rom.with_extension("tmcmap")
    }

    /// gespeicherte Karte laden
    pub fn load(rom: &Path) -> MapData {
        let path = Self::file_for(rom);
        let mut m = MapData { path: Some(path.clone()), ..Default::default() };
        let Some(raw) = std::fs::read(&path).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()) else { return m };
        let mut i = 0;
        let rd = |b: &[u8], i: &mut usize, n: usize| -> Option<Vec<u8>> {
            let v = b.get(*i..*i + n)?.to_vec();
            *i += n;
            Some(v)
        };
        while i + 10 <= raw.len() {
            let Some(h) = rd(&raw, &mut i, 10) else { break };
            let space = u16::from_le_bytes([h[0], h[1]]);
            let cx = i32::from_le_bytes([h[2], h[3], h[4], h[5]]);
            let cy = i32::from_le_bytes([h[6], h[7], h[8], h[9]]);
            let Some(px) = rd(&raw, &mut i, CH * CH * 4) else { break };
            if space == 0xFFFE {
                // besondere Punkte: Anzahl, dann je 12 Byte (Raum u16, Art u8, frei, x i32, y i32)
                let n = u32::from_le_bytes([px[0], px[1], px[2], px[3]]) as usize;
                for k in 0..n.min((CH * CH * 4 - 4) / 12) {
                    let o = 4 + k * 12;
                    let sp = u16::from_le_bytes([px[o], px[o + 1]]);
                    let x = i32::from_le_bytes([px[o + 4], px[o + 5], px[o + 6], px[o + 7]]);
                    let y = i32::from_le_bytes([px[o + 8], px[o + 9], px[o + 10], px[o + 11]]);
                    m.marks.push((sp, px[o + 2], x as f32, y as f32));
                }
                continue;
            }
            m.chunks.insert((space, cx, cy), px);
            m.dirty.push((space, cx, cy));
        }
        m
    }

    pub fn save(&mut self) {
        let Some(path) = self.path.clone() else { return };
        if !self.unsaved {
            return;
        }
        let mut raw = Vec::with_capacity(self.chunks.len() * (CH * CH * 4 + 10));
        if !self.marks.is_empty() {
            let mut px = vec![0u8; CH * CH * 4];
            let n = self.marks.len().min((CH * CH * 4 - 4) / 12);
            px[..4].copy_from_slice(&(n as u32).to_le_bytes());
            for (k, &(sp, kind, x, y)) in self.marks.iter().take(n).enumerate() {
                let o = 4 + k * 12;
                px[o..o + 2].copy_from_slice(&sp.to_le_bytes());
                px[o + 2] = kind;
                px[o + 4..o + 8].copy_from_slice(&(x as i32).to_le_bytes());
                px[o + 8..o + 12].copy_from_slice(&(y as i32).to_le_bytes());
            }
            raw.extend_from_slice(&0xFFFEu16.to_le_bytes());
            raw.extend_from_slice(&0i32.to_le_bytes());
            raw.extend_from_slice(&0i32.to_le_bytes());
            raw.extend_from_slice(&px);
        }
        for (&(s, cx, cy), px) in &self.chunks {
            raw.extend_from_slice(&s.to_le_bytes());
            raw.extend_from_slice(&cx.to_le_bytes());
            raw.extend_from_slice(&cy.to_le_bytes());
            raw.extend_from_slice(px);
        }
        if std::fs::write(&path, lz4_flex::compress_prepend_size(&raw)).is_ok() {
            self.unsaved = false;
        }
    }

    /// Bild (RGBA, Zeilenlänge `stride` Pixel) ab Bildpunkt (sx, sy), Größe w×h, an Weltposition (x0, y0) eintragen.
    /// Magenta (Hintergrundfarbe der Aufnahme) bleibt frei.
    #[allow(clippy::too_many_arguments)]
    pub fn paste(&mut self, space: Space, x0: i32, y0: i32, px: &[u8], stride: usize, sx: usize, sy: usize, w: usize, h: usize) {
        for y in 0..h {
            let wy = y0 + y as i32;
            let cy = wy.div_euclid(CH as i32);
            let ly = wy.rem_euclid(CH as i32) as usize;
            for x in 0..w {
                let i = ((sy + y) * stride + sx + x) * 4;
                let Some(p) = px.get(i..i + 4) else { continue };
                if p[0] >= 200 && p[1] <= 60 && p[2] >= 200 {
                    continue;
                }
                let wx = x0 + x as i32;
                let key = (space, wx.div_euclid(CH as i32), cy);
                let lx = wx.rem_euclid(CH as i32) as usize;
                let chunk = self.chunks.entry(key).or_insert_with(|| vec![0u8; CH * CH * 4]);
                let o = (ly * CH + lx) * 4;
                let new = [p[0], p[1], p[2], 255];
                if chunk[o..o + 4] != new {
                    chunk[o..o + 4].copy_from_slice(&new);
                    if !self.dirty.contains(&key) {
                        self.dirty.push(key);
                    }
                    self.unsaved = true;
                }
            }
        }
    }

    /// Ausdehnung eines Raums (Kacheln) – für den Anfangsausschnitt
    pub fn bounds(&self, space: Space) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for &(s, cx, cy) in self.chunks.keys() {
            if s != space {
                continue;
            }
            let (x0, y0, x1, y1) = (cx * CH as i32, cy * CH as i32, (cx + 1) * CH as i32, (cy + 1) * CH as i32);
            b = Some(match b {
                None => (x0, y0, x1, y1),
                Some((a, b_, c, d)) => (a.min(x0), b_.min(y0), c.max(x1), d.max(y1)),
            });
        }
        b
    }
}

/// Fassung der vorab erstellten Karte (bei Änderungen am Verfahren erhöhen: dann wird neu erstellt)
pub const GEN_VERSION: i32 = 2;

impl MapData {
    /// ganze Karte schon erstellt?
    pub fn complete(&self) -> bool {
        self.chunks.contains_key(&(0xFFFF, GEN_VERSION, 0))
    }
    pub fn set_complete(&mut self) {
        // ältere Fassungen der fertigen Karte entfernen
        self.chunks.retain(|k, _| k.0 != 0xFFFF);
        self.chunks.insert((0xFFFF, GEN_VERSION, 0), vec![0u8; CH * CH * 4]);
        self.unsaved = true;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum GenPhase {
    Warp,
    Wait,
    Shoot,
}

/// Ganze Karte vorab: jeden Raum einmal besuchen und abfotografieren (Savestate vorher, danach zurück)
pub struct MapGen {
    /// Gebiet, Raum, Raum-Rechteck (Weltpixel), Raum auf der Karte
    rooms: Vec<(u8, u8, [i32; 4], Space)>,
    i: usize,
    phase: GenPhase,
    wait: u32,
    shots: Vec<(i32, i32)>,
    shot: usize,
    settle: u32,
    saved: Vec<u8>,
    pub done: bool,
    pub skipped: Vec<(u8, u8)>,
}

impl MapGen {
    /// Liste aller Räume; `None`, wenn das Spiel gerade nicht läuft
    pub fn new(g: &mut super::Gba) -> Option<MapGen> {
        g.map_view()?;
        let saved = g.save_state()?;
        let mut rooms: Vec<(u8, u8, [i32; 4], Space)> = Vec::new();
        for area in 0u8..0x90 {
            // nur Gebiete mit Namen, in die man sicher springen kann (Liste aus dem Warp-Menü von Picori)
            if !g.area_name(area).is_some_and(|n| n.1) {
                continue;
            }
            let n = g.room_count(area);
            let kind = g.area_kind(area);
            for room in 0..n.clamp(0, 0x40) as u8 {
                if let Some(r) = g.room_rect(area, room) {
                    if r[2] >= 16 && r[3] >= 16 {
                        let sp = if kind & 1 != 0 { 0 } else { area as Space + 1 };
                        rooms.push((area, room, r, sp));
                    }
                }
            }
        }
        // Entwicklung: nur bestimmte Räume (GBA_GEN_ONLY = „03:01,09:00“)
        if let Ok(only) = std::env::var("GBA_GEN_ONLY") {
            let want: Vec<(u8, u8)> = only.split(',').filter_map(|e| { let (a, b) = e.split_once(':')?; Some((u8::from_str_radix(a, 16).ok()?, u8::from_str_radix(b, 16).ok()?)) }).collect();
            rooms.retain(|r| want.contains(&(r.0, r.1)));
        }
        // Oberwelt: nur Räume innerhalb der Welt (Ausdehnung der großen Gebiete) und keine, die einen schon
        // vorhandenen Raum fast ganz überdecken (Sonderfassungen wie die Stadt beim Fest)
        let big: Vec<[i32; 4]> = rooms.iter().filter(|r| r.3 == 0 && r.2[2] >= 1000).map(|r| r.2).collect();
        if let (Some(bx1), Some(by1)) = (big.iter().map(|r| r[0] + r[2]).max(), big.iter().map(|r| r[1] + r[3]).max()) {
            let mut keep: Vec<(u8, [i32; 4])> = Vec::new();
            rooms.retain(|&(a, _, r, sp)| {
                if sp != 0 {
                    return true;
                }
                if r[0] < 0 || r[1] < 0 || r[0] + r[2] > bx1 || r[1] + r[3] > by1 {
                    return false;
                }
                let covered: i32 = keep
                    .iter()
                    .map(|(_, k)| ((r[0] + r[2]).min(k[0] + k[2]) - r[0].max(k[0])).max(0) * ((r[1] + r[3]).min(k[1] + k[3]) - r[1].max(k[1])).max(0))
                    .sum();
                if covered * 2 > r[2] * r[3] {
                    return false;
                }
                keep.push((a, r));
                true
            });
        }
        Some(MapGen { rooms, i: 0, phase: GenPhase::Warp, wait: 0, shots: Vec::new(), shot: 0, settle: 0, saved, done: false, skipped: Vec::new() })
    }

    /// abbrechen: Spiel zurück auf den Stand vor dem Erstellen
    pub fn abort(self, g: &mut super::Gba) {
        g.cam(-1, -1);
        g.load_state(&self.saved);
        g.run_frame(0);
        let mut sink = Vec::new();
        g.take_audio(&mut sink);
    }

    /// Fortschritt 0..1
    pub fn progress(&self) -> f32 {
        self.i as f32 / self.rooms.len().max(1) as f32
    }

    fn next_room(&mut self, g: &mut super::Gba) {
        g.cam(-1, -1);
        self.i += 1;
        self.phase = GenPhase::Warp;
        self.wait = 0;
    }

    /// ein paar Spielbilder weiterarbeiten (bis `budget` abgelaufen ist)
    pub fn step(&mut self, g: &mut super::Gba, d: &mut MapData, budget: std::time::Duration) {
        let t0 = std::time::Instant::now();
        let mut sink = Vec::new();
        while !self.done && t0.elapsed() < budget {
            let Some(&(area, room, r, sp)) = self.rooms.get(self.i) else {
                // fertig: Spiel genau wie vorher
                g.cam(-1, -1);
                g.load_state(&self.saved);
                g.run_frame(0);
                g.take_audio(&mut sink);
                d.set_complete();
                self.done = true;
                return;
            };
            match self.phase {
                GenPhase::Warp => {
                    if g.warp(area, room, r[2] / 2, r[3] / 2) {
                        self.phase = GenPhase::Wait;
                        self.wait = 0;
                        self.settle = 0;
                    } else {
                        self.wait += 1;
                        if self.wait > 400 {
                            // Spiel kommt nicht in einen Zustand zum Springen: Stand zurück und weiter
                            g.load_state(&self.saved);
                            self.skipped.push((area, room));
                            self.next_room(g);
                            continue;
                        }
                    }
                    g.run_frame(0);
                }
                GenPhase::Wait => {
                    g.run_frame(0);
                    self.wait += 1;
                    let ready = g.gen_ready();
                    if ready != ((area as i32) << 8 | room as i32) {
                        self.settle = 0;
                    } else {
                        self.settle += 1;
                    }
                    let need = std::env::var("GBA_GEN_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(8u32);
                    // Bild noch fast einfarbig (Überblendung, Intro): weiter warten, höchstens 5 Sekunden
                    let flat = || {
                        let f = &g.frame;
                        if f.len() < 16 {
                            return true;
                        }
                        let c0 = &f[f.len() / 2..f.len() / 2 + 3];
                        let n = f.len() / 4;
                        let same = (0..n).step_by(7).filter(|&i| &f[i * 4..i * 4 + 3] == c0).count();
                        same * 7 * 100 > n * 92
                    };
                    if ready == ((area as i32) << 8 | room as i32) && self.settle > need && (self.settle > 300 || !flat()) {
                        let fw = g.map_view().map_or(240, |v| v[11].max(240));
                        let (fw, fh) = (fw, 160);
                        let axis = |o: i32, len: i32, view: i32| -> Vec<i32> {
                            if len <= view {
                                return vec![o];
                            }
                            let mut v: Vec<i32> = (0..).map(|k| o + k * view).take_while(|&x| x + view < o + len).collect();
                            v.push(o + len - view);
                            v
                        };
                        let xs = axis(r[0], r[2], fw);
                        let ys = axis(r[1], r[3], fh);
                        self.shots = ys.iter().flat_map(|&y| xs.iter().map(move |&x| (x, y))).collect();
                        // Schrumpfpunkte, Windwappen, Bohnenranken dieses Raums (ohne doppelte)
                        for (k, x, y) in g.room_marks() {
                            let (x, y) = (x as f32, y as f32);
                            // eine Bohnenranke besteht aus mehreren Teilen: nahe Teile zählen als eine
                            let near = if k == 3 { 96.0 } else { 8.0 };
                            if !d.marks.iter().any(|m| m.0 == sp && m.1 == k && (m.2 - x).abs() < near && (m.3 - y).abs() < near) {
                                d.marks.push((sp, k, x, y));
                            }
                        }
                        self.shot = 0;
                        self.settle = 0;
                        self.phase = GenPhase::Shoot;
                    } else if ready >= 0 && ready != ((area as i32) << 8 | room as i32) && self.wait > 60 {
                        if std::env::var_os("GBA_GEN_LOG").is_some() {
                            eprintln!("Raum {area:02X}/{room:02X}: Spiel ist in {ready:04X}");
                        }
                        // das Spiel hat woandershin geschickt (Zwischensequenz): Raum auslassen
                        self.skipped.push((area, room));
                        self.next_room(g);
                    } else if self.wait > 900 {
                        if std::env::var_os("GBA_GEN_LOG").is_some() {
                            eprintln!("Raum {area:02X}/{room:02X}: Zeit um, bereit {ready}, Ansicht {:?}", g.map_view());
                        }
                        self.skipped.push((area, room));
                        g.load_state(&self.saved);
                        self.next_room(g);
                    }
                }
                GenPhase::Shoot => {
                    let Some(&(x, y)) = self.shots.get(self.shot) else {
                        self.next_room(g);
                        continue;
                    };
                    if self.settle == 0 {
                        g.cam(x, y);
                    }
                    g.gen_ready();
                    g.run_frame(0);
                    self.settle += 1;
                    if self.settle >= 3 {
                        if let (Some(v), Some((px, w, h))) = (g.map_view(), g.map_render_bg()) {
                            #[cfg(test)]
                            if let Ok(dir) = std::env::var("GBA_GEN_DUMP") {
                                let c: Vec<crate::gfx::Color32> = px.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                                crate::devtest::bmp(&format!("{dir}/shot_{area:02X}_{room:02X}_{}.bmp", self.shot), w, h, &c);
                                let c: Vec<crate::gfx::Color32> = g.frame.chunks_exact(4).map(|c| crate::gfx::Color32::from_rgb(c[0], c[1], c[2])).collect();
                                crate::devtest::bmp(&format!("{dir}/full_{area:02X}_{room:02X}_{}.bmp", self.shot), g.w, g.h, &c);
                            }
                            let (sx, sy) = (v[5], v[6]);
                            let x0 = sx.max(r[0]);
                            let y0 = sy.max(r[1]);
                            let x1 = (sx + w as i32).min(r[0] + r[2]);
                            let y1 = (sy + h as i32).min(r[1] + r[3]);
                            if v[0] == area as i32 && v[1] == room as i32 && x1 > x0 && y1 > y0 {
                                d.paste(sp, x0, y0, &px, w, (x0 - sx) as usize, (y0 - sy) as usize, (x1 - x0) as usize, (y1 - y0) as usize);
                            }
                        }
                        self.shot += 1;
                        self.settle = 0;
                    }
                }
            }
            sink.clear();
            g.take_audio(&mut sink);
        }
    }
}
