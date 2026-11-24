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
