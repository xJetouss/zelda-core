//! Schnellmenü (Item-Leiste wie in BotW/TotK): Steuerkreuz rechts halten, mit Stick/Pfeilen nach
//! links/rechts wählen, loslassen = ausrüsten.
//!
//! * Die Item-Symbole stammen aus deiner ROM: Auf einer Kopie des Spiels wird kurz das Item-Menü
//!   geöffnet und jedes Symbol (16×16) ausgeschnitten, danach per Clean-HD (xBR 4×) hochskaliert.
//! * Ausrüsten: Das Spiel aktualisiert die Anzeige nur beim Schließen seines Menüs. Deshalb wird
//!   das Menü nach dem Setzen der Auswahl unsichtbar geöffnet und wieder geschlossen (~55 Bilder,
//!   ca. 75 ms), danach zeigt die Anzeige oben links das neue Item.

use crate::snes::Snes;
use crate::gfx::{self, Color32};
use std::time::Instant;

/// Menü-Index (1..20, wie im Item-Menü des Spiels) -> Inventar-Adresse
pub const MENU: [(u8, u32); 20] = [
    (1, 0x7EF340),
    (2, 0x7EF341),
    (3, 0x7EF342),
    (4, 0x7EF343),
    (5, 0x7EF344),
    (6, 0x7EF345),
    (7, 0x7EF346),
    (8, 0x7EF347),
    (9, 0x7EF348),
    (10, 0x7EF349),
    (11, 0x7EF34A),
    (12, 0x7EF34B),
    (13, 0x7EF34C),
    (14, 0x7EF34D),
    (15, 0x7EF34E),
    (16, 0x7EF35C),
    (17, 0x7EF350),
    (18, 0x7EF351),
    (19, 0x7EF352),
    (20, 0x7EF353),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub idx: u8,
    pub bottle: Option<u8>,
}

#[inline]
fn r8(s: &Snes, a: u32) -> u8 {
    s.bus.wram[(a & 0x1FFFF) as usize]
}

/// Darf jetzt ein Item gewechselt werden? (normales Spiel, kein Text, keine Überblendung)
pub fn can_use(s: &Snes) -> bool {
    let m = r8(s, 0x7E0010);
    // 0x0B: Sondergebiete der Oberwelt (z. B. Masterschwert-Hain) – dort spielt man ganz normal
    matches!(m, 0x07 | 0x09 | 0x0B) && r8(s, 0x7E0011) == 0 && s.bus.ppu.brightness() == 15
}

pub fn entries(s: &Snes) -> Vec<Entry> {
    let mut v = Vec::new();
    for (idx, addr) in MENU {
        if idx == 16 {
            for b in 0..4u8 {
                if r8(s, 0x7EF35C + b as u32) != 0 {
                    v.push(Entry { idx, bottle: Some(b + 1) });
                }
            }
        } else if r8(s, addr) != 0 {
            v.push(Entry { idx, bottle: None });
        }
    }
    v
}

/// Inhalt einer Flasche (1..4)
pub fn bottle_content(s: &Snes, b: u8) -> u8 {
    r8(s, 0x7EF35B + b.clamp(1, 4) as u32)
}

pub fn current(s: &Snes) -> Entry {
    let idx = r8(s, 0x7E0202);
    Entry { idx, bottle: if idx == 16 { Some(r8(s, 0x7EF34F).max(1)) } else { None } }
}

/// Inventar-Fingerabdruck – ändert er sich, werden die Symbole neu aufgenommen.
pub fn signature(s: &Snes) -> Vec<u8> {
    let mut v: Vec<u8> = (0x7EF340..=0x7EF35F).map(|a| if a == 0x7EF343 { (r8(s, a) > 0) as u8 } else { r8(s, a) }).collect();
    v.push(r8(s, 0x7EF34F));
    v
}

pub fn name(s: &Snes, e: Entry, en: bool) -> String {
    let v = r8(s, MENU[(e.idx as usize).saturating_sub(1).min(19)].1);
    let t = |d: &str, x: &str| if en { x.to_string() } else { d.to_string() };
    match e.idx {
        1 => if v >= 3 { t("Silberpfeil-Bogen", "Silver Arrows") } else { t("Bogen", "Bow") },
        2 => if v >= 2 { t("Magischer Bumerang", "Magic Boomerang") } else { t("Bumerang", "Boomerang") },
        3 => t("Enterhaken", "Hookshot"),
        4 => format!("{} ({})", t("Bomben", "Bombs"), v),
        5 => if v >= 2 { t("Zauberpuder", "Magic Powder") } else { t("Pilz", "Mushroom") },
        6 => t("Feuerstab", "Fire Rod"),
        7 => t("Eisstab", "Ice Rod"),
        8 => t("Bombos-Medaillon", "Bombos Medallion"),
        9 => t("Ether-Medaillon", "Ether Medallion"),
        10 => t("Quake-Medaillon", "Quake Medallion"),
        11 => t("Laterne", "Lamp"),
        12 => t("Hammer", "Hammer"),
        13 => if v >= 2 { t("Flöte", "Flute") } else { t("Schaufel", "Shovel") },
        14 => t("Käfernetz", "Bug Net"),
        15 => t("Buch Mudora", "Book of Mudora"),
        16 => {
            let b = e.bottle.unwrap_or(1);
            let c = r8(s, 0x7EF35B + b as u32);
            let what = match c {
                2 => t("leer", "empty"),
                3 => t("Roter Trank", "Red Potion"),
                4 => t("Grüner Trank", "Green Potion"),
                5 => t("Blauer Trank", "Blue Potion"),
                6 => t("Fee", "Fairy"),
                7 => t("Biene", "Bee"),
                8 => t("Goldbiene", "Good Bee"),
                _ => "?".into(),
            };
            format!("{} {b}: {what}", t("Flasche", "Bottle"))
        }
        17 => t("Somaria-Stab", "Cane of Somaria"),
        18 => t("Byrna-Stab", "Cane of Byrna"),
        19 => t("Tarnumhang", "Magic Cape"),
        20 => t("Zauberspiegel", "Magic Mirror"),
        _ => "?".into(),
    }
}

fn c15(c: u16) -> Color32 {
    let r = (c & 31) as u8;
    let g = ((c >> 5) & 31) as u8;
    let b = ((c >> 10) & 31) as u8;
    Color32::from_rgb(r << 3 | r >> 2, g << 3 | g >> 2, b << 3 | b >> 2)
}

/// Kantenlänge der ausgeschnittenen Symbole (16 + 2×2 Rand)
pub const ICON: usize = 20;

/// Aufgenommene Symbole: Items (Index 1..20) und Flaschen je Inhalt.
pub struct Icons {
    pub items: Vec<Option<Vec<Color32>>>,
    pub bottles: Vec<(u8, Vec<Color32>)>,
    /// Ausrüstung aus dem Menü: (Schlüssel, Breite, Höhe, Pixel) – 0 Schwert, 1 Schild, 2 Gewand,
    /// 3 Stiefel, 4 Handschuhe, 5 Flossen, 6 Mondperle
    pub equip: Vec<(u8, usize, usize, Vec<Color32>)>,
}

/// Stellen der Ausrüstungs-Symbole im Item-Menü (x, y, Breite, Höhe)
const EQUIP_POS: [(u8, usize, usize, usize, usize); 7] = [(0, 175, 157, 18, 16), (1, 198, 157, 20, 16), (2, 221, 157, 19, 15), (3, 30, 188, 17, 17), (4, 62, 188, 17, 17), (5, 95, 188, 17, 17), (6, 127, 188, 17, 17)];

/// Dungeon-Gegenstände im Item-Menü (nur im Dungeon sichtbar): 7 Karte, 8 Kompass, 9 Großer Schlüssel
const DUNGEON_POS: [(u8, usize, usize, usize, usize); 3] = [(7, 177, 191, 14, 14), (8, 201, 191, 14, 14), (9, 226, 190, 12, 14)];

/// kleiner Schlüssel (Schlüssel 10) als Pixelbild im Stil des Spiels
fn small_key_px() -> (usize, usize, Vec<Color32>) {
    const ART: [&str; 14] = [
        "..kkk..",
        ".kyyyk.",
        "kywkkyk",
        "kyk.kyk",
        "kyk.kyk",
        ".kyyyk.",
        "..kyk..",
        "..kyk..",
        "..kyyk.",
        "..kykk.",
        "..kyyk.",
        "..kyk..",
        "..kyk..",
        "...k...",
    ];
    let w = ART[0].len();
    let px = ART
        .iter()
        .flat_map(|r| r.chars())
        .map(|ch| match ch {
            'k' => Color32::from_rgb(40, 28, 8),
            'y' => Color32::from_rgb(232, 184, 48),
            'w' => Color32::from_rgb(255, 240, 160),
            _ => Color32::TRANSPARENT,
        })
        .collect();
    (w, ART.len(), px)
}

fn crop_at(c: &Snes, x0: usize, y0: usize, w: usize, h: usize) -> Vec<Color32> {
    let fb = c.framebuffer();
    (0..w * h)
        .map(|i| {
            let p = c15(fb[(y0 + i / w) * 256 + x0 + i % w]);
            // schwarzer Menü-Hintergrund -> durchsichtig
            if p.r() < 12 && p.g() < 12 && p.b() < 12 {
                Color32::TRANSPARENT
            } else {
                p
            }
        })
        .collect()
}

/// Öffnet auf einer Kopie das Item-Menü (Auswahl `sel`) und wartet, bis es ruhig steht.
fn open_menu(s: &Snes, sel: Option<(u8, u8)>) -> Option<Snes> {
    let mut c = s.clone();
    c.bus.ppu.capture = false;
    c.bus.ppu.ext = 0;
    c.bus.ppu.record = false;
    c.bus.ppu.skip = false;
    c.bus.ppu.layer_mask = 0x1F;
    if let Some((idx, bottle)) = sel {
        c.bus.wram[0x0202] = idx;
        if bottle > 0 {
            c.bus.wram[0xF34F] = bottle;
        }
    }
    let mut opened = false;
    let mut settle = 0;
    for f in 0..220 {
        c.bus.joy[0] = if !opened && f % 6 < 2 { 0x1000 } else { 0 };
        c.run_frame();
        c.bus.apu.dsp.out.clear();
        if r8(&c, 0x7E0010) == 0x0E {
            opened = true;
            settle += 1;
            if settle >= 60 && r8(&c, 0x7E0011) == 0x01 {
                return Some(c);
            }
        }
    }
    None
}

fn crop(c: &Snes, idx: u8) -> Vec<Color32> {
    let fb = c.framebuffer();
    let (col, row) = (((idx - 1) % 5) as usize, ((idx - 1) / 5) as usize);
    let (x0, y0) = (30 + col * 24, 26 + row * 24);
    let px: Vec<Color32> = (0..ICON * ICON).map(|i| c15(fb[(y0 + i / ICON) * 256 + x0 + i % ICON])).collect();
    // Menü-Hintergrund (Farbe der Ecken) durchsichtig machen -> Symbol schwebt auf der Kachel
    let bg = px[0];
    px.into_iter().map(|p| if p == bg { Color32::TRANSPARENT } else { p }).collect()
}

/// Bild des Symbols `sel` ohne den blinkenden grünen Menü-Cursor (40 Bilder lang das beste nehmen).
fn crop_selected(c: &mut Snes, sel: u8) -> Vec<Color32> {
    let green = |v: &[Color32]| v.iter().filter(|p| p.a() > 0 && p.g() as i32 > p.r() as i32 + 40 && p.g() as i32 > p.b() as i32 + 40).count();
    let mut best = crop(c, sel);
    let mut best_g = green(&best);
    for _ in 0..40 {
        c.run_frame();
        c.bus.apu.dsp.out.clear();
        let icon = crop(c, sel);
        let g = green(&icon);
        if g < best_g {
            best_g = g;
            best = icon;
        }
    }
    best
}

fn visible(icon: &[Color32]) -> bool {
    icon.iter().filter(|p| p.a() > 0 && p.r() as u32 + p.g() as u32 + p.b() as u32 > 60).count() > 6
}

/// Nimmt alle Symbole auf (läuft im Hintergrund-Thread auf einer Kopie des Spiels).
pub fn capture_all(s: &Snes) -> Option<Icons> {
    if !can_use(s) {
        return None;
    }
    let mut c = open_menu(s, None)?;
    let mut items: Vec<Option<Vec<Color32>>> = vec![None; 21];
    for (idx, _) in MENU {
        let icon = crop(&c, idx);
        if visible(&icon) {
            items[idx as usize] = Some(icon);
        }
    }
    let sel = r8(&c, 0x7E0202);
    if (1..=20).contains(&sel) && items[sel as usize].is_some() {
        items[sel as usize] = Some(crop_selected(&mut c, sel));
    }
    // jede Flasche einzeln auswählen und ihr Symbol aufnehmen
    let mut bottles: Vec<(u8, Vec<Color32>)> = Vec::new();
    for b in 1..=4u8 {
        let content = r8(s, 0x7EF35B + b as u32);
        if content == 0 || bottles.iter().any(|(k, _)| *k == content) {
            continue;
        }
        if let Some(mut m) = open_menu(s, Some((16, b))) {
            let mut icon = crop_selected(&mut m, 16);
            // Ecken des Auswahl-Cursors entfernen
            for (i, p) in icon.iter_mut().enumerate() {
                let (x, y) = (i % ICON, i / ICON);
                let ring = x < 2 || x >= ICON - 2 || y < 2 || y >= ICON - 2;
                let corner = (x < 5 || x >= ICON - 5) && (y < 5 || y >= ICON - 5);
                let cursor_green = p.g() > 140 && p.r() < 90 && p.b() < 90 && (x < 4 || x >= ICON - 4 || y < 4 || y >= ICON - 4);
                if ring || corner || cursor_green {
                    *p = Color32::TRANSPARENT;
                }
            }
            if visible(&icon) {
                bottles.push((content, icon));
            }
        }
    }
    let mut equip = Vec::new();
    for (k, x, y, w, h) in EQUIP_POS {
        let mut px = crop_at(&c, x, y, w, h);
        if k == 0 {
            // die Stufen-Ziffer neben dem Schwert entfernen
            for (i, p) in px.iter_mut().enumerate() {
                if i % w >= 11 && i / w >= 9 {
                    *p = Color32::TRANSPARENT;
                }
            }
        }
        if px.iter().filter(|p| p.a() > 0).count() > 6 {
            equip.push((k, w, h, px));
        }
    }
    // Dungeon-Gegenstände: auf einer Kopie alle besitzen und das Menü im Dungeon öffnen
    if r8(s, 0x7E001B) != 0 && r8(s, 0x7E040C) < 0x1C {
        let mut d = s.clone();
        for a in 0xF364usize..0xF36A {
            d.bus.wram[a] = 0xFF;
        }
        if let Some(m) = open_menu(&d, None) {
            for (k, x, y, w, h) in DUNGEON_POS {
                let px = crop_at(&m, x, y, w, h);
                if px.iter().filter(|p| p.a() > 0).count() > 6 {
                    equip.push((k, w, h, px));
                }
            }
        }
    }
    let (w, h, px) = small_key_px();
    equip.push((10, w, h, px));
    Some(Icons { items, bottles, equip })
}

/// Rüstet ein Item aus (Auswahl setzen + Spielmenü unsichtbar öffnen/schließen).
pub fn equip(s: &mut Snes, e: Entry) -> bool {
    if !can_use(s) {
        return false;
    }
    s.bus.wram[0x0202] = e.idx;
    if let Some(b) = e.bottle {
        s.bus.wram[0xF34F] = b;
    }
    let start = r8(s, 0x7E0010);
    let saved = s.bus.joy[0];
    let mut phase = 0;
    let mut ok = false;
    for f in 0..400 {
        let m = r8(s, 0x7E0010);
        let sub = r8(s, 0x7E0011);
        let press = match phase {
            0 => {
                if m == 0x0E {
                    phase = 1;
                }
                f % 4 < 2
            }
            1 => {
                if sub == 0x01 || f > 250 {
                    phase = 2;
                }
                false
            }
            2 => {
                if m != 0x0E {
                    phase = 3;
                }
                f % 4 < 2
            }
            _ => false,
        };
        s.bus.joy[0] = if press { 0x1000 } else { 0 };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        if phase == 3 && r8(s, 0x7E0010) == start && r8(s, 0x7E0011) == 0 {
            ok = true;
            break;
        }
    }
    s.bus.joy[0] = saved;
    ok && r8(s, 0x7E0202) == e.idx
}

/// Zustand des Schnellmenüs in der App.
pub struct QuickMenu {
    pub open: bool,
    pub sticky: bool,
    pub t0: Instant,
    pub sel: Option<usize>,
    pub entries: Vec<Entry>,
    pub icons: Vec<Option<gfx::TextureHandle>>,
    pub sig: Vec<u8>,
    pub cooldown: u32,
    pub held_prev: bool,
    pub moved: bool,
    pub prev_pad: u16,
    /// Hintergrund-Aufnahme der Symbole
    pub job: Option<std::sync::mpsc::Receiver<Option<Icons>>>,
    /// Flaschen-Symbole je Inhalt (leer, roter Trank, Fee …)
    pub bottle_icons: std::collections::HashMap<u8, gfx::TextureHandle>,
    /// dieselben Symbole in Originalpixeln (Pixel-Modus)
    pub icons_raw: Vec<Option<gfx::TextureHandle>>,
    pub bottle_raw: std::collections::HashMap<u8, gfx::TextureHandle>,
    /// Ausrüstungs-Symbole (Pixel, HD)
    pub equip_raw: std::collections::HashMap<u8, gfx::TextureHandle>,
    pub equip_hd: std::collections::HashMap<u8, gfx::TextureHandle>,
    /// feine Pixel (Scale2x) für HD Max
    pub icons_fine: Vec<Option<gfx::TextureHandle>>,
    /// Rohpixel (ICON×ICON) zum Einzeichnen ins Spielbild (X-Feld)
    pub icons_px: Vec<Option<Vec<gfx::Color32>>>,
    pub bottle_px: std::collections::HashMap<u8, Vec<gfx::Color32>>,
    pub bottle_fine: std::collections::HashMap<u8, gfx::TextureHandle>,
    pub equip_fine: std::collections::HashMap<u8, gfx::TextureHandle>,
    /// horizontale Auswahl: aktuelle Richtung (-1/0/1) und nächster Wiederholzeitpunkt
    pub nav_dir: i8,
    pub nav_next: Instant,
    /// weiches Mitlaufen der Leiste (Position der Auswahl)
    pub scroll: f32,
}

impl QuickMenu {
    pub fn new() -> Self {
        QuickMenu {
            open: false,
            sticky: false,
            t0: Instant::now(),
            sel: None,
            entries: Vec::new(),
            icons: vec![None; 21],
            sig: Vec::new(),
            cooldown: 0,
            held_prev: false,
            moved: false,
            prev_pad: 0,
            job: None,
            bottle_icons: Default::default(),
            icons_raw: vec![None; 21],
            bottle_raw: Default::default(),
            equip_raw: Default::default(),
            equip_hd: Default::default(),
            icons_fine: vec![None; 21],
            icons_px: vec![None; 21],
            bottle_px: Default::default(),
            bottle_fine: Default::default(),
            equip_fine: Default::default(),
            nav_dir: 0,
            nav_next: Instant::now(),
            scroll: 0.0,
        }
    }

    pub fn icon_raw(&self, s: &Snes, e: Entry) -> Option<&gfx::TextureHandle> {
        // HD: geglättete Version
        if crate::pixfont::smooth() {
            return self.icon(s, e);
        }
        if crate::pixfont::fine() {
            let f = match e.bottle {
                Some(b) => self.bottle_fine.get(&bottle_content(s, b)),
                None => self.icons_fine.get(e.idx as usize).and_then(|t| t.as_ref()),
            };
            if f.is_some() {
                return f;
            }
        }
        match e.bottle {
            Some(b) => self.bottle_raw.get(&bottle_content(s, b)),
            None => self.icons_raw.get(e.idx as usize).and_then(|t| t.as_ref()),
        }
    }

    /// Rohpixel eines Symbols
    pub fn icon_px(&self, s: &Snes, e: Entry) -> Option<&Vec<gfx::Color32>> {
        match e.bottle {
            Some(b) => self.bottle_px.get(&bottle_content(s, b)),
            None => self.icons_px.get(e.idx as usize).and_then(|t| t.as_ref()),
        }
    }

    pub fn icon(&self, s: &Snes, e: Entry) -> Option<&gfx::TextureHandle> {
        match e.bottle {
            Some(b) => self.bottle_icons.get(&bottle_content(s, b)),
            None => self.icons.get(e.idx as usize).and_then(|t| t.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn capture_from_state() {
        let Ok(p) = std::env::var("PASTLINK_ROM") else { return };
        let p = std::path::PathBuf::from(p);
        let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
        let mut s = Snes::new(Arc::new(rom), info.sram_size, false);
        s.load_state(&std::fs::read(p.with_extension("ss9")).unwrap()).unwrap();
        for k in 0..3 {
            let icons = capture_all(&s);
            let ok = icons.as_ref().map(|v| v.items[6].as_ref().map(|i| i.iter().filter(|p| p.r() > 150 && p.g() < 80).count()).unwrap_or(0));
            println!("direkt nach Laden +{k} Bilder: rote Pixel im Feuerstab-Symbol {:?}", ok);
            s.run_frame();
        }
        for f in 0..200 {
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            if f % 40 == 0 {
                println!("f{f} modus {:02X} sub {:02X} can_use {} hell {}", r8(&s, 0x7E0010), r8(&s, 0x7E0011), can_use(&s), s.bus.ppu.brightness());
            }
        }
        let mut c = s.clone();
        for f in 0..140 {
            c.bus.joy[0] = if f < 3 { 0x1000 } else { 0 };
            c.run_frame();
            if f % 10 == 0 {
                println!("menu f{f} modus {:02X} sub {:02X} hell {}", r8(&c, 0x7E0010), r8(&c, 0x7E0011), c.bus.ppu.brightness());
            }
        }
        let icons = capture_all(&s);
        println!("Symbole: {:?}", icons.map(|v| v.items.iter().filter(|i| i.is_some()).count()));
    }
}
