//! Welt-Daten aus der ROM: Höhlen-/Dungeon-Eingänge, Räume, Herzteile in Höhlen, Truhengruppen,
//! Boss-Herzcontainer. Daraus entstehen alle Orte, die für 100 % zählen und auf der Karte stehen.
//!
//! Tabellen (in der US- und deutschen ROM verifiziert):
//! * Eingänge: 129 × Gebiet (u16) | 129 × Map16-Position (u16) | 129 × Eingangs-ID (u8), Abstand 0x102
//! * Raum je Eingangs-ID: u16-Tabelle, wird über bekannte Paare (Heiligtum, Kakariko-Brunnen, …) gefunden
//! * Raum-Spritelisten: Zeigertabelle (0x140 Räume), Einträge 3 Byte, Herzteil = Sprite 0xEB

use crate::randomizer::ChestTable;
use crate::snes::Snes;
use crate::tracker::StaticItem;

#[derive(Clone, Copy)]
pub struct Entrance {
    pub world: u8,
    pub area: u8,
    pub x: f32,
    pub y: f32,
    pub room: u16,
    /// Eingangs-ID ($010E)
    pub eid: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LocKind {
    HeartPiece,
    HeartContainer,
    Chests,
    Minigame,
}

#[derive(Clone)]
pub enum Check {
    /// Oberwelt-Flag $7EF280+Gebiet, Bit 0x40
    OwFlag(u8),
    /// Raum-Flag $7EF000+Raum*2 mit Maske (0x0400 = Gegenstand, 0x0800 = Boss-Herz)
    RoomBit(u16, u16),
    /// Truhen (Raum, n-te Truhe im Raum)
    Chests(Vec<(u16, u8)>),
    /// ohne Speicher-Flag -> von Hand abhaken
    #[allow(dead_code)]
    Manual,
}

#[derive(Clone)]
pub struct Location {
    pub kind: LocKind,
    /// gehört zu einem Dungeon (Index in `DMAP_NAMES`) -> nur auf der Dungeon-Karte zeichnen
    pub dungeon: Option<u8>,
    pub world: u8,
    pub x: f32,
    pub y: f32,
    pub name_de: String,
    pub name_en: String,
    pub check: Check,
}

impl Location {
    pub fn name(&self, en: bool) -> &str {
        if en {
            &self.name_en
        } else {
            &self.name_de
        }
    }
    pub fn total(&self) -> usize {
        match &self.check {
            Check::Chests(v) => v.len(),
            _ => 1,
        }
    }
    /// Anzahl erledigt (bei Truhen: geöffnete Truhen).
    pub fn done(&self, s: &Snes, manual: bool) -> usize {
        let room = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
        match &self.check {
            Check::OwFlag(a) => (s.bus.wram[0xF280 + *a as usize] & 0x40 != 0) as usize,
            Check::RoomBit(r, m) => (room(*r) & m != 0) as usize,
            Check::Chests(v) => v.iter().filter(|(r, i)| *i < 6 && room(*r) & (0x10 << i) != 0).count(),
            Check::Manual => manual as usize,
        }
    }
}

fn u16at(r: &[u8], o: usize) -> u16 {
    r[o] as u16 | (r[o + 1] as u16) << 8
}

/// Bekannte Paare Gebiet -> Raum zur Prüfung der Raumtabelle.
const KNOWN: [(u16, u16); 5] = [(0x13, 0x12), (0x18, 0x2F), (0x1B, 0x61), (0x1E, 0xC9), (0x2C, 0x104)];

/// Adresse der Raumtabelle der Eingänge (Eingang -> Raum, je 2 Byte)
pub fn entrance_room_table(rom: &[u8]) -> Option<usize> {
    find_entrance_tables(rom).map(|t| t.1)
}

pub fn find_entrances(rom: &[u8]) -> Vec<Entrance> {
    let Some((x, rt)) = find_entrance_tables(rom) else { return Vec::new() };
    const N: usize = 129;
    let raw: Vec<(u16, u16, u8)> = (0..N).map(|i| (u16at(rom, x + i * 2), u16at(rom, x + 0x102 + i * 2), rom[x + 0x204 + i])).collect();
    raw.iter()
        .map(|&(m, p, e)| {
            let (tx, ty) = (((p >> 1) % 64) as f32, ((p >> 1) / 64) as f32);
            let a = (m & 0x3F) as f32;
            Entrance {
                world: (m >= 0x40) as u8,
                area: m as u8,
                x: (a as u32 % 8) as f32 * 512.0 + tx * 16.0 + 8.0,
                y: (a as u32 / 8) as f32 * 512.0 + ty * 16.0 + 8.0,
                room: u16at(rom, rt + e as usize * 2),
                eid: e,
            }
        })
        .collect()
}

/// (Tabelle der Oberwelt-Eingänge, Raumtabelle)
fn find_entrance_tables(rom: &[u8]) -> Option<(usize, usize)> {
    const N: usize = 129;
    let table_ok = |x: usize| -> bool {
        x + 0x204 + N <= rom.len()
            && (0..N).all(|i| u16at(rom, x + i * 2) < 0x80)
            && (0..N).all(|i| u16at(rom, x + 0x102 + i * 2) < 0x2000)
            && (0..N).all(|i| rom[x + 0x204 + i] < 0x86)
    };
    let x = if table_ok(0xDB96F) { Some(0xDB96F) } else { (0..rom.len().saturating_sub(0x300)).find(|&x| table_ok(x)) };
    let x = x?;
    let raw: Vec<(u16, u16, u8)> = (0..N).map(|i| (u16at(rom, x + i * 2), u16at(rom, x + 0x102 + i * 2), rom[x + 0x204 + i])).collect();
    // Raumtabelle suchen: die Position, an der die meisten bekannten Paare stimmen
    let used = raw.iter().map(|r| r.2 as usize).max().unwrap_or(0) + 1;
    let mut best = (0usize, 0usize);
    let mut o = 0usize;
    while o + used * 2 < rom.len() {
        if (0..used).all(|i| u16at(rom, o + i * 2) < 0x140) {
            let score = KNOWN.iter().filter(|(a, r)| raw.iter().any(|(m, _, e)| m == a && u16at(rom, o + *e as usize * 2) == *r)).count();
            if score > best.0 {
                best = (score, o);
            }
        }
        o += 1;
    }
    if best.0 < 4 {
        return None;
    }
    Some((x, best.1))
}

/// Räume, in deren Spriteliste ein Herzteil (0xEB) steht.
#[cfg_attr(not(test), allow(dead_code))]
pub fn room_heart_pieces(rom: &[u8]) -> Vec<u16> {
    let mut v: Vec<u16> = room_heart_spots(rom).into_iter().map(|h| h.0).collect();
    v.dedup();
    v
}

/// Jedes Herzteil (0xEB) in einem Raum: (Raum, x-Kachel 0..31 im 512er-Raum). Ein Raum kann zwei haben
/// (0x11B: Höhle 45 im Westteil, Friedhofs-Höhle im Ostteil).
pub fn room_heart_spots(rom: &[u8]) -> Vec<(u16, u8)> {
    fn slist(rom: &[u8], o: usize) -> Option<Vec<(u8, u8)>> {
        if *rom.get(o)? > 1 {
            return None;
        }
        let mut o = o + 1;
        let mut ids = Vec::new();
        for _ in 0..40 {
            if *rom.get(o)? == 0xFF {
                return Some(ids);
            }
            ids.push((*rom.get(o + 2)?, *rom.get(o + 1)? & 0x1F));
            o += 3;
        }
        None
    }
    for bank in 0..rom.len() / 0x8000 {
        let base = bank * 0x8000;
        let mut t = base;
        while t + 0x280 < base + 0x8000 {
            let mut lists = Vec::new();
            while lists.len() < 0x180 && t + lists.len() * 2 + 1 < base + 0x8000 {
                let o = t + lists.len() * 2;
                let p = u16at(rom, o) as usize;
                if p < 0x8000 {
                    break;
                }
                match slist(rom, base + p - 0x8000) {
                    Some(l) => lists.push(l),
                    None => break,
                }
            }
            if lists.len() >= 0x128 {
                // Die Tabelle hat 0x140 Räume. Davor liegende Bytes können zufällig auch wie Zeiger auf Listen aussehen –
                // dann wäre jeder Raum um so viele Einträge verschoben (z. B. Herzteil in Raum EB statt EA)
                // Anker: Raum 0 ist Ganons Raum (Figur D6), Raum 0x20 der von Agahnim (Figur 7A)
                let has = |l: &Vec<(u8, u8)>, id: u8| l.iter().any(|e| e.0 == id);
                let Some(skip) = (0..8).find(|&k| lists.len() > k + 0x13F && has(&lists[k], 0xD6) && has(&lists[k + 0x20], 0x7A)) else {
                    t += 1;
                    continue;
                };
                #[cfg(test)]
                if std::env::var("HEARTDBG").is_ok() {
                    println!("Sprite-Tabelle bei {t:X}: {} Einträge, übersprungen {skip}", lists.len());
                    let _ = &lists;
                }
                return lists[skip..skip + 0x140]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 0)
                    .flat_map(|(i, l)| l.iter().filter(|e| e.0 == 0xEB).map(move |e| (i as u16, e.1)))
                    .collect();
            }
            t += 1;
        }
    }
    Vec::new()
}

/// Alle Figuren aus den Sprite-Listen der ROM: (Sprite-ID, Welt, x, y).
/// Oberwelt: Position aus der Gebietsliste; Räume: Position des Eingangs zum Raum.
pub fn find_npcs(rom: &[u8], ents: &[Entrance]) -> Vec<(u8, u8, f32, f32)> {
    let mut out = Vec::new();
    // Oberwelt (gleiche Tabelle wie bei den Herzteilen)
    fn olist(rom: &[u8], mut o: usize) -> Option<Vec<(u8, u8, u8)>> {
        let mut v = Vec::new();
        for _ in 0..80 {
            let y = *rom.get(o)?;
            if y == 0xFF {
                return Some(v);
            }
            let (x, id) = (*rom.get(o + 1)?, *rom.get(o + 2)?);
            if y >= 64 || x >= 64 {
                return None;
            }
            v.push((y, x, id));
            o += 3;
        }
        None
    }
    'ow: for bank in 0..rom.len() / 0x8000 {
        let base = bank * 0x8000;
        let mut t = base;
        while t + 208 * 2 < base + 0x8000 {
            let mut lists = Vec::new();
            loop {
                let o = t + lists.len() * 2;
                if o + 1 >= base + 0x8000 || lists.len() >= 64 + 2 * 144 {
                    break;
                }
                let p = u16at(rom, o) as usize;
                if p < 0x8000 {
                    break;
                }
                match olist(rom, base + p - 0x8000) {
                    Some(l) => lists.push(l),
                    None => break,
                }
            }
            if lists.len() >= 64 + 144 {
                let main = &lists[64..64 + 144];
                if main.iter().flatten().filter(|s| s.2 == 0xEB).count() >= 3 {
                    for (i, l) in main.iter().enumerate() {
                        for &(y, x, id) in l {
                            let (lx, ly) = (x as f32 * 16.0 + 8.0, y as f32 * 16.0 + 8.0);
                            if i < 0x80 {
                                let a = (i & 0x3F) as u8;
                                out.push((id, (i >= 0x40) as u8, (a & 7) as f32 * 512.0 + lx, (a >> 3) as f32 * 512.0 + ly));
                            } else if i == 0x81 {
                                out.push((id, 0, 3584.0 + lx / 2.0, ly / 2.0));
                            }
                        }
                    }
                    break 'ow;
                }
            }
            t += 1;
        }
    }
    // Räume
    fn rlist(rom: &[u8], o: usize) -> Option<Vec<u8>> {
        if *rom.get(o)? > 1 {
            return None;
        }
        let mut o = o + 1;
        let mut ids = Vec::new();
        for _ in 0..40 {
            if *rom.get(o)? == 0xFF {
                return Some(ids);
            }
            ids.push(*rom.get(o + 2)?);
            o += 3;
        }
        None
    }
    'rooms: for bank in 0..rom.len() / 0x8000 {
        let base = bank * 0x8000;
        let mut t = base;
        while t + 0x280 < base + 0x8000 {
            let mut lists = Vec::new();
            while lists.len() < 0x150 && t + lists.len() * 2 + 1 < base + 0x8000 {
                let p = u16at(rom, t + lists.len() * 2) as usize;
                if p < 0x8000 {
                    break;
                }
                match rlist(rom, base + p - 0x8000) {
                    Some(l) => lists.push(l),
                    None => break,
                }
            }
            if lists.len() >= 0x128 {
                // gleicher Anker wie bei den Herzteilen (sonst sind alle Räume um einen Eintrag verschoben)
                let Some(skip) = (0..8).find(|&k| lists.len() > k + 0x13F && lists[k].contains(&0xD6) && lists[k + 0x20].contains(&0x7A)) else {
                    t += 1;
                    continue;
                };
                for (room, l) in lists[skip..skip + 0x140].iter().enumerate() {
                    if room == 0 {
                        continue;
                    }
                    if let Some(k) = entrance_for_room(ents, room as u16) {
                        let e = ents[k];
                        for &id in l {
                            out.push((id, e.world, e.x, e.y));
                        }
                    }
                }
                break 'rooms;
            }
            t += 1;
        }
    }
    out
}

/// Nächster Eingang zu einem Raum (Räume einer Höhle/eines Dungeons liegen im 16er-Raster nebeneinander).
fn entrance_for_room(ents: &[Entrance], room: u16) -> Option<usize> {
    let mut seen = vec![room];
    let mut front = vec![room];
    for _ in 0..10 {
        for r in &front {
            if let Some(i) = ents.iter().position(|e| e.room == *r) {
                return Some(i);
            }
        }
        let mut next = Vec::new();
        for &r in &front {
            let cand = [r.wrapping_sub(1), r + 1, r.wrapping_sub(16), r + 16];
            for (k, &q) in cand.iter().enumerate() {
                let same_row = k >= 2 || q / 16 == r / 16;
                if q < 0x140 && same_row && !seen.contains(&q) {
                    seen.push(q);
                    next.push(q);
                }
            }
        }
        front = next;
    }
    None
}

/// Boss-Räume (Herzcontainer-Flag 0x0800) in der Reihenfolge von `tracker::DUNGEONS`.
pub const BOSS_ROOMS: [Option<u16>; 13] = [None, Some(0xC8), Some(0x33), Some(0x07), Some(0x5A), Some(0x06), Some(0x29), Some(0xAC), Some(0xDE), Some(0x90), Some(0xA4), None, None];

pub fn area_name(area: u8, en: bool) -> String {
    let n: Option<(&str, &str)> = match area {
        0x00 => Some(("Verlorene Wälder", "Lost Woods")),
        0x02 => Some(("Holzfäller", "Lumberjacks")),
        0x03 => Some(("Todesberg West", "West Death Mountain")),
        0x05 => Some(("Todesberg Ost", "East Death Mountain")),
        0x07 => Some(("Schildkrötenfels", "Turtle Rock")),
        0x0A => Some(("Todesberg-Aufgang", "Death Mountain Ascent")),
        0x0F | 0x81 => Some(("Zoras Wasserfall", "Zora's Waterfall")),
        0x10 => Some(("Weg zu den Wäldern", "Lost Woods Path")),
        0x11 => Some(("Kakariko Nord", "North Kakariko")),
        0x13 => Some(("Heiligtum", "Sanctuary")),
        0x14 => Some(("Friedhof", "Graveyard")),
        0x15 => Some(("Fluss", "River")),
        0x16 => Some(("Hexenhütte", "Witch's Hut")),
        0x18 => Some(("Kakariko", "Kakariko")),
        0x1B => Some(("Hyrule-Schloss", "Hyrule Castle")),
        0x1E => Some(("Ostpalast", "Eastern Palace")),
        0x22 => Some(("Schmiede", "Smithy")),
        0x28 => Some(("Labyrinth-Rennen", "Maze Race")),
        0x29 => Some(("Bibliothek", "Library")),
        0x2A => Some(("Spukhain", "Haunted Grove")),
        0x2C => Some(("Links Haus", "Link's House")),
        0x30 => Some(("Wüste", "Desert")),
        0x32 => Some(("Wüstengrenze", "Desert Edge")),
        0x33 => Some(("Südwiese", "South Field")),
        0x35 => Some(("Hylia-See", "Lake Hylia")),
        0x37 => Some(("Eishöhle", "Ice Cave")),
        0x3A => Some(("Südhang", "South Hills")),
        0x3B => Some(("Damm", "Dam")),
        0x40 => Some(("Skelettwald", "Skull Woods")),
        0x42 => Some(("Dunkle Holzfäller", "Dark Lumberjacks")),
        0x43 => Some(("Dunkler Todesberg", "Dark Death Mountain")),
        0x45 => Some(("Dunkler Todesberg Ost", "East Dark Death Mountain")),
        0x47 => Some(("Schildkrötenfels", "Turtle Rock")),
        0x4A => Some(("Bumper-Höhle", "Bumper Cave")),
        0x58 => Some(("Dorf der Ausgestoßenen", "Village of Outcasts")),
        0x5B => Some(("Pyramide", "Pyramid")),
        0x5E => Some(("Palast der Finsternis", "Palace of Darkness")),
        0x62 => Some(("Hammerpflöcke", "Hammer Pegs")),
        0x68 => Some(("Grabungsspiel", "Digging Game")),
        0x6C => Some(("Bombenladen", "Bomb Shop")),
        0x70 => Some(("Moor", "Misery Mire")),
        0x75 => Some(("Eissee", "Ice Lake")),
        0x7B => Some(("Sumpf", "Swamp")),
        _ => None,
    };
    match n {
        Some((d, e)) => (if en { e } else { d }).to_string(),
        None if area >= 0x40 => format!("{} {:02X}", if en { "Dark World" } else { "Schattenwelt" }, area),
        None => format!("{} {:02X}", if en { "Area" } else { "Gebiet" }, area),
    }
}

/// Namen der Dungeons in der Reihenfolge des Spiels (`$040C / 2`).
pub const DMAP_NAMES: [(&str, &str); 14] = [
    ("Kanalisation", "Sewers"),
    ("Hyrule-Schloss", "Hyrule Castle"),
    ("Ostpalast", "Eastern Palace"),
    ("Wüstenpalast", "Desert Palace"),
    ("Schlossturm", "Castle Tower"),
    ("Sumpfpalast", "Swamp Palace"),
    ("Palast der Finsternis", "Palace of Darkness"),
    ("Moorpalast", "Misery Mire"),
    ("Skelettwald", "Skull Woods"),
    ("Eispalast", "Ice Palace"),
    ("Turm des Herakles", "Tower of Hera"),
    ("Diebesversteck", "Thieves Town"),
    ("Schildkrötenfels", "Turtle Rock"),
    ("Ganons Turm", "Ganons Tower"),
];

/// Dungeon des Spiels -> Index in `tracker::DUNGEONS` (Position auf der Oberwelt)
pub const DMAP_TO_DUNGEON: [usize; 14] = [0, 0, 1, 2, 0, 5, 4, 9, 6, 8, 3, 7, 10, 11];

/// Grundriss eines Dungeons aus der ROM (wie die Dungeonkarte im Spiel): Etagen von unten nach oben,
/// je 5 × 5 Räume (0x0F = kein Raum).
#[derive(Clone)]
pub struct DungeonMap {
    pub floors: Vec<[u8; 25]>,
    /// Anzahl Kellergeschosse (B1, B2 …) – die untersten Etagen
    pub basements: u8,
}

impl DungeonMap {
    pub fn floor_name(&self, k: usize) -> String {
        let b = self.basements as usize;
        if k < b {
            format!("B{}", b - k)
        } else {
            format!("{}F", k - b + 1)
        }
    }
    pub fn contains(&self, room: u16) -> bool {
        room < 0x100 && room != 0x0F && self.floors.iter().any(|f| f.contains(&(room as u8)))
    }
}

/// Sucht die Grundrisse aller 14 Dungeons: 14 Etagen-Infos (u16: Bit 4-7 = oberirdische, Bit 0-3 = Kellergeschosse)
/// direkt vor 14 aufsteigenden Zeigern auf die 25-Byte-Etagen.
pub fn find_dungeon_maps(rom: &[u8]) -> Vec<DungeonMap> {
    let floors_of = |info: u16| ((info >> 4) & 0x0F) + (info & 0x0F);
    let check = |i: usize| -> Option<Vec<DungeonMap>> {
        if i < 64 || i + 28 > rom.len() {
            return None;
        }
        let ptr: Vec<u16> = (0..14).map(|k| u16at(rom, i + k * 2)).collect();
        if ptr.iter().any(|&p| p < 0x8000) || ptr.windows(2).any(|w| w[1] <= w[0]) || ptr.windows(2).any(|w| (w[1] - w[0]) % 25 != 0) {
            return None;
        }
        // Etagen-Infos liegen kurz davor (in der deutschen/US-ROM 44 Byte)
        let info: Vec<u16> = (28..=64).step_by(2).find_map(|back| {
            let info: Vec<u16> = (0..14).map(|k| u16at(rom, i - back + k * 2)).collect();
            (0..13).all(|k| {
                let n = floors_of(info[k]);
                n > 0 && n <= 9 && (ptr[k + 1] - ptr[k]) as usize == n as usize * 25
            })
            .then_some(info)
        })?;
        let bank = i & !0x7FFF;
        let mut out = Vec::new();
        for k in 0..14 {
            let n = floors_of(info[k]) as usize;
            let o = bank + (ptr[k] as usize - 0x8000);
            if n == 0 || o + n * 25 > rom.len() {
                return None;
            }
            let floors = (0..n).map(|f| rom[o + f * 25..o + f * 25 + 25].try_into().unwrap()).collect();
            out.push(DungeonMap { floors, basements: (info[k] & 0x0F) as u8 });
        }
        Some(out)
    };
    if let Some(m) = check(0x57620) {
        return m;
    }
    (28..rom.len().saturating_sub(28)).find_map(check).unwrap_or_default()
}

/// Dungeon, zu dem ein Raum gehört.
pub fn room_dungeon(maps: &[DungeonMap], room: u16) -> Option<u8> {
    maps.iter().position(|m| m.contains(room)).map(|i| i as u8)
}

/// Eingangsräume der Dungeons (Reihenfolge wie `tracker::DUNGEONS`), um sie exakt zu platzieren.
const DUNGEON_ENTRY: [Option<u16>; 13] = [Some(0x61), Some(0xC9), Some(0x84), Some(0x77), Some(0x4A), Some(0x28), None, Some(0xDB), Some(0x0E), Some(0x98), Some(0xD6), Some(0x0C), None];

/// Cheat „Spiel durchgespielt“: setzt alles, was für 100 % zählt. Rückgabe: von Hand abzuhakende Orte.
pub fn apply_100(s: &mut Snes, locs: &[Location], ents: &[Entrance]) -> Vec<usize> {
    crate::alttp::give_all_items(s);
    let w = &mut s.bus.wram;
    let mut manual = Vec::new();
    let set16 = |w: &mut Vec<u8>, r: u16, m: u16| {
        let o = 0xF000 + r as usize * 2;
        w[o] |= m as u8;
        w[o + 1] |= (m >> 8) as u8;
    };
    for (i, l) in locs.iter().enumerate() {
        match &l.check {
            Check::OwFlag(a) => w[0xF280 + *a as usize] |= 0x40,
            Check::RoomBit(r, m) => set16(w, *r, *m),
            Check::Chests(v) => {
                for (r, idx) in v {
                    if *idx < 6 {
                        set16(w, *r, 0x10u16 << idx);
                    }
                }
            }
            Check::Manual => manual.push(i),
        }
    }
    // alle Eingänge als besucht -> alle Teleport-Punkte frei
    for e in ents {
        if (e.room as usize) < 0x140 {
            set16(w, e.room, 0x000F);
        }
    }
    w[0xF359] = 4; // Goldenes Schwert
    w[0xF35A] = 3; // Spiegelschild
    w[0xF35B] = 2; // Rote Rüstung
    w[0xF36C] = 0xA0; // 20 Herzen
    w[0xF36D] = 0xA0;
    w[0xF36B] = 0; // keine angefangenen Herzteile
    w[0xF374] = 0x07; // alle Amulette
    w[0xF37A] = 0x7F; // alle Kristalle
    w[0xF340] = w[0xF340].max(4); // Silberpfeile
    w[0xF341] = 2; // Magischer Bumerang
    w[0xF344] = 2; // Magisches Puder
    w[0xF34C] = 3; // Flöte aktiviert
    w[0xF354] = 2; // Titanhandschuh
    w[0xF370] = 7; // mehr Bomben
    w[0xF371] = 7; // mehr Pfeile
    w[0xF37B] = w[0xF37B].max(1); // halbe Magie
    for b in 0..4 {
        if w[0xF35C + b] == 0 {
            w[0xF35C + b] = 2; // leere Flasche
        }
    }
    manual
}

/// Liefert alle Orte für 100 % und die Positionen der Dungeons (Welt, x, y).
pub fn build_locations(rom: &[u8], chests: Option<&ChestTable>, statics: &[StaticItem], ents: &[Entrance], maps: &[DungeonMap]) -> (Vec<Location>, Vec<(u8, f32, f32)>) {
    use crate::tracker::DUNGEONS;
    let _ = rom;
    let dungeons: Vec<(u8, f32, f32)> = DUNGEONS
        .iter()
        .zip(DUNGEON_ENTRY.iter())
        .map(|(d, room)| room.and_then(|r| ents.iter().find(|e| e.room == r && e.world == d.0)).map(|e| (e.world, e.x, e.y - 10.0)).unwrap_or((d.0, d.1, d.2)))
        .collect();
    let mut out = Vec::new();
    // Herzteile auf der Oberwelt
    for s in statics {
        let (de, en) = (area_name(s.area, false), area_name(s.area, true));
        out.push(Location {
            kind: if s.kind == crate::tracker::MarkerKind::HeartContainer { LocKind::HeartContainer } else { LocKind::HeartPiece },
            world: s.world,
            x: s.x,
            y: s.y,
            name_de: format!("Herzteil: {de}"),
            name_en: format!("Piece of Heart: {en}"),
            check: Check::OwFlag(s.area),
            dungeon: None,
        });
    }
    // Herzteile in Höhlen (als Raum-Sprite oder in einer Truhe)
    // Herzteil als Figur: das Spiel merkt es sich im Raum-Bit 0x0400 (Westhälfte des Raums) bzw. 0x0200 (Osthälfte) –
    // im Spiel nachgeprüft (Höhle Verlorene Wälder 0x0200, Höhle am Todesberg 0x0400, Raum 0x11B beide)
    let mut sprite_hearts: Vec<(Option<usize>, Check)> = Vec::new();
    for (r, x) in room_heart_spots(rom) {
        let east = x >= 16;
        // mehrere Eingänge in denselben Raum: der Eingang, der in derselben Raumhälfte ankommt
        // (0x11B: Westteil Höhle 45 = Eingang 0x51, Ostteil Friedhofs-Höhle = 0x52; 0x126: Schachbrett-Höhle 0x72)
        let want: Option<u8> = match (r, east) {
            (0x11B, false) => Some(0x51),
            (0x11B, true) => Some(0x52),
            (0x126, _) => Some(0x72),
            _ => None,
        };
        let ent = want.and_then(|w| ents.iter().position(|e| e.eid == w && e.room == r)).or_else(|| entrance_for_room(ents, r));
        sprite_hearts.push((ent, Check::RoomBit(r, if east { 0x0200 } else { 0x0400 })));
    }
    let mut cave_hearts: Vec<(Option<usize>, Check)> = sprite_hearts;
    if let Some(t) = chests {
        for c in t.chests.iter().filter(|c| c.item == 0x17) {
            cave_hearts.push((entrance_for_room(ents, c.room), Check::Chests(vec![(c.room, c.index_in_room)])));
        }
    }
    let mut used_entrances = Vec::new();
    for (ent, check) in cave_hearts {
        if let Some(i) = ent {
            // pro Höhleneingang höchstens ein Herzteil (Sprite und Truhe derselben Höhle nicht doppelt zählen)
            if used_entrances.contains(&i) {
                continue;
            }
            used_entrances.push(i);
            let e = ents[i];
            out.push(Location {
                kind: LocKind::HeartPiece,
                world: e.world,
                x: e.x,
                y: e.y,
                name_de: format!("Herzteil: Höhle {}", area_name(e.area, false)),
                name_en: format!("Piece of Heart: cave {}", area_name(e.area, true)),
                check,
                dungeon: None,
            });
        }
    }
    // Grabungsspiel: das Herzteil ist ein normales Oberwelt-Herzteil (Flag von Gebiet 0x68) -> automatisch
    out.push(Location { kind: LocKind::Minigame, world: 1, x: 300.0, y: 2820.0, name_de: "Grabungsspiel: Herzteil mit der Schaufel ausgraben".into(), name_en: "Digging Game: dig up the Piece of Heart with the shovel".into(), check: Check::OwFlag(0x68), dungeon: None });
    // Truhenspiel im Dorf der Ausgestoßenen (Raum 0x106): beim Gewinn des Herzteils setzt das Spiel im Raum das
    // Gegenstands-Bit 0x0400 (wie bei den Herzteilen in Höhlen) -> automatisch abhaken
    if let Some(e) = ents.iter().filter(|e| e.room == 0x106).min_by(|a, b| a.y.total_cmp(&b.y)) {
        out.push(Location { kind: LocKind::Minigame, world: e.world, x: e.x, y: e.y + 18.0, name_de: "Herzteil: Truhenspiel".into(), name_en: "Piece of Heart: Treasure Chest Game".into(), check: Check::RoomBit(0x106, 0x0400), dungeon: None });
    }
    // Boss-Herzcontainer
    for (i, room) in BOSS_ROOMS.iter().enumerate() {
        if let Some(r) = room {
            let (_, _, _, de, en) = DUNGEONS[i];
            let (w, x, y) = dungeons[i];
            out.push(Location {
                kind: LocKind::HeartContainer,
                world: w,
                x: x + 28.0,
                y: y - 28.0,
                name_de: format!("Herzcontainer: {de}"),
                name_en: format!("Heart Container: {en}"),
                check: Check::RoomBit(*r, 0x0800),
                dungeon: room_dungeon(maps, *r),
            });
        }
    }
    // Truhen: in Dungeons je Dungeon, sonst je Höhleneingang
    if let Some(t) = chests {
        let mut dgroups: Vec<(u8, Vec<(u16, u8)>)> = Vec::new();
        let mut groups: Vec<(usize, Vec<(u16, u8)>)> = Vec::new();
        for c in &t.chests {
            if let Some(d) = room_dungeon(maps, c.room) {
                match dgroups.iter_mut().find(|g| g.0 == d) {
                    Some(g) => g.1.push((c.room, c.index_in_room)),
                    None => dgroups.push((d, vec![(c.room, c.index_in_room)])),
                }
            } else if let Some(i) = entrance_for_room(ents, c.room) {
                match groups.iter_mut().find(|g| g.0 == i) {
                    Some(g) => g.1.push((c.room, c.index_in_room)),
                    None => groups.push((i, vec![(c.room, c.index_in_room)])),
                }
            }
        }
        dgroups.sort_by_key(|g| g.0);
        for (d, list) in dgroups {
            let (w, x, y) = dungeons[DMAP_TO_DUNGEON[d as usize]];
            let (de, en) = DMAP_NAMES[d as usize];
            out.push(Location {
                kind: LocKind::Chests,
                world: w,
                x,
                y: y + 24.0,
                name_de: format!("Truhen: {de}"),
                name_en: format!("Chests: {en}"),
                check: Check::Chests(list),
                dungeon: Some(d),
            });
        }
        for (i, list) in groups {
            let e = ents[i];
            out.push(Location {
                kind: LocKind::Chests,
                world: e.world,
                x: e.x,
                y: e.y + 18.0,
                name_de: format!("Truhen: Höhle {}", area_name(e.area, false)),
                name_en: format!("Chests: cave {}", area_name(e.area, true)),
                check: Check::Chests(list),
                dungeon: None,
            });
        }
    }
    (out, dungeons)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn entrances_debug() {
        let Ok(p) = std::env::var("PASTLINK_ROM") else { return };
        let (rom, _) = crate::alttp::prepare_rom(std::fs::read(p).unwrap()).unwrap();
        let x = 0xDB96F;
        let bad_m = (0..129).find(|&i| super::u16at(&rom, x + i * 2) >= 0x80);
        let bad_p = (0..129).find(|&i| super::u16at(&rom, x + 0x102 + i * 2) >= 0x2000);
        let bad_e = (0..129).find(|&i| rom[x + 0x204 + i] >= 0x86);
        println!("bad map {:?} pos {:?} eid {:?}", bad_m, bad_p, bad_e);
        let e = super::find_entrances(&rom);
        println!("Eingänge: {}", e.len());
        println!("Herz-Räume: {:?}", super::room_heart_pieces(&rom));
    }
}

#[cfg(test)]
pub fn entrance_for_room_pub(ents: &[Entrance], room: u16) -> Option<usize> {
    entrance_for_room(ents, room)
}
#[cfg(test)]
pub fn room_heart_pieces_pub(rom: &[u8]) -> Vec<u16> {
    room_heart_pieces(rom)
}
