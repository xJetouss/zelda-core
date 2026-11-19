//! Eingebauter Truhen-Randomizer – komplett offline.
//!
//! Die Truhentabelle wird in der geladenen ROM gesucht (3-Byte-Einträge: Raum-ID 16 Bit mit
//! Bit 15 = große Truhe, danach Item-ID). Wird keine eindeutige, plausible Tabelle gefunden,
//! ist der Randomizer deaktiviert, statt die ROM zu beschädigen.

use crate::alttp::item_name;

#[derive(Clone)]
pub struct Chest {
    pub offset: usize,
    pub room: u16,
    pub big: bool,
    pub item: u8,
    /// n-te Truhe in diesem Raum (für das Geöffnet-Flag)
    pub index_in_room: u8,
}

#[derive(Clone)]
pub struct ChestTable {
    pub offset: usize,
    pub chests: Vec<Chest>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Nur Rubine/Munition/Herzen werden gemischt – immer lösbar
    Safe,
    /// Zusätzlich Ausrüstung, Schlüssel bleiben – evtl. nicht lösbar
    Mixed,
    /// Alles – nur für Spaß, oft nicht ohne Cheats lösbar
    Chaos,
}

pub struct RandoResult {
    pub seed: u64,
    pub mode: Mode,
    pub patches: Vec<(usize, u8)>,
    pub spoiler: Vec<(u16, bool, u8, u8)>,
}

fn valid(rom: &[u8], o: usize) -> bool {
    let hi = rom[o + 1];
    let room = (rom[o] as u16 | (hi as u16) << 8) & 0x7FFF;
    matches!(hi, 0x00 | 0x01 | 0x80 | 0x81) && room < 0x140 && rom[o + 2] < 0x4C
}

/// Bekannte Lage der Truhentabelle (LoROM $01:E96E, 168 Einträge à 3 Byte).
const KNOWN_OFFSET: usize = 0xE96E;
const KNOWN_COUNT: usize = 168;

fn stats_ok(rom: &[u8], o: usize, n: usize) -> bool {
    let mut rooms: Vec<u16> = (0..n).map(|i| (rom[o + i * 3] as u16 | (rom[o + i * 3 + 1] as u16) << 8) & 0x7FFF).collect();
    let big = (0..n).filter(|i| rom[o + i * 3 + 1] & 0x80 != 0).count();
    let items: std::collections::HashSet<u8> = (0..n).map(|i| rom[o + i * 3 + 2]).collect();
    rooms.sort_unstable();
    rooms.dedup();
    rooms.len() >= 60 && (6..=40).contains(&big) && items.len() >= 15
}

fn build(rom: &[u8], off: usize, n: usize) -> ChestTable {
    let mut seen: std::collections::HashMap<u16, u8> = Default::default();
    let chests = (0..n)
        .map(|i| {
            let p = off + i * 3;
            let raw = rom[p] as u16 | (rom[p + 1] as u16) << 8;
            let room = raw & 0x7FFF;
            let c = seen.entry(room).or_insert(0);
            let idx = *c;
            *c += 1;
            Chest { offset: p, room, big: raw & 0x8000 != 0, item: rom[p + 2], index_in_room: idx }
        })
        .collect();
    ChestTable { offset: off, chests }
}

pub fn find_chest_table(rom: &[u8]) -> Option<ChestTable> {
    let run_ok = |o: usize, n: usize| o + n * 3 <= rom.len() && (0..n).all(|i| valid(rom, o + i * 3));
    // 1) bekannte Position
    if run_ok(KNOWN_OFFSET, KNOWN_COUNT) && stats_ok(rom, KNOWN_OFFSET, KNOWN_COUNT) {
        return Some(build(rom, KNOWN_OFFSET, KNOWN_COUNT));
    }
    // 2) Suche: genau 168 gültige Einträge, bevorzugt mit ungültigem Eintrag davor und danach
    let mut best: Option<(u8, usize)> = None;
    let end = rom.len().saturating_sub(KNOWN_COUNT * 3 + 3);
    let mut o = 3;
    while o < end {
        if valid(rom, o) && run_ok(o, KNOWN_COUNT) && stats_ok(rom, o, KNOWN_COUNT) {
            let score = (!valid(rom, o - 3)) as u8 + (!valid(rom, o + KNOWN_COUNT * 3)) as u8;
            if best.map_or(true, |(s, _)| score > s) {
                best = Some((score, o));
            }
        }
        o += 1;
    }
    best.filter(|(s, _)| *s >= 1).map(|(_, o)| build(rom, o, KNOWN_COUNT))
}

fn is_junk(i: u8) -> bool {
    matches!(i, 0x17 | 0x26 | 0x27 | 0x28 | 0x31 | 0x34 | 0x35 | 0x36 | 0x3E | 0x3F | 0x40 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45 | 0x46 | 0x47)
}
fn is_dungeon_item(i: u8) -> bool {
    matches!(i, 0x24 | 0x25 | 0x32 | 0x33)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

pub fn seed_from_text(t: &str) -> u64 {
    let t = t.trim();
    if let Ok(n) = t.parse::<u64>() {
        return n.max(1);
    }
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in t.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    h.max(1)
}

pub fn randomize(table: &ChestTable, seed: u64, mode: Mode) -> RandoResult {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let pool_idx: Vec<usize> = table
        .chests
        .iter()
        .enumerate()
        .filter(|(_, c)| match mode {
            Mode::Safe => is_junk(c.item),
            Mode::Mixed => !is_dungeon_item(c.item),
            Mode::Chaos => true,
        })
        .map(|(i, _)| i)
        .collect();
    let mut items: Vec<u8> = pool_idx.iter().map(|&i| table.chests[i].item).collect();
    for i in (1..items.len()).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
    let mut patches = Vec::new();
    let mut spoiler = Vec::new();
    for (k, &ci) in pool_idx.iter().enumerate() {
        let c = &table.chests[ci];
        patches.push((c.offset + 2, items[k]));
        spoiler.push((c.room, c.big, c.item, items[k]));
    }
    RandoResult { seed, mode, patches, spoiler }
}

pub fn spoiler_text(r: &RandoResult, en: bool) -> String {
    let mut s = String::new();
    s.push_str(&format!("Seed: {}\n\n", r.seed));
    for (room, big, old, new) in &r.spoiler {
        if old == new {
            continue;
        }
        s.push_str(&format!(
            "{} {:04X}{}: {} -> {}\n",
            if en { "Room" } else { "Raum" },
            room,
            if *big { if en { " (big chest)" } else { " (große Truhe)" } } else { "" },
            item_name(*old, en),
            item_name(*new, en)
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_rom() -> Vec<u8> {
        let mut rom: Vec<u8> = (0..0x100000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        let off = 0xE96C;
        for i in 0..168usize {
            let room = (i * 7 % 0x120) as u16 | if i % 9 == 0 { 0x8000 } else { 0 };
            rom[off + i * 3] = room as u8;
            rom[off + i * 3 + 1] = (room >> 8) as u8;
            rom[off + i * 3 + 2] = (i % 0x4B) as u8;
        }
        rom[off + 168 * 3 + 1] = 0x55; // Tabellenende ungültig machen
        rom
    }

    #[test]
    fn finds_table_and_shuffles_deterministically() {
        let rom = fake_rom();
        let t = find_chest_table(&rom).expect("Tabelle");
        assert_eq!(t.offset, 0xE96C);
        assert_eq!(t.chests.len(), 168);
        let a = randomize(&t, 42, Mode::Chaos);
        let b = randomize(&t, 42, Mode::Chaos);
        assert_eq!(a.patches, b.patches);
        // Chaos mischt nur um: gleiche Item-Menge
        let mut x: Vec<u8> = a.patches.iter().map(|p| p.1).collect();
        let mut y: Vec<u8> = t.chests.iter().map(|c| c.item).collect();
        x.sort();
        y.sort();
        assert_eq!(x, y);
        // Sicher: nur Junk-Items werden verändert
        let s = randomize(&t, 7, Mode::Safe);
        assert!(s.spoiler.iter().all(|(_, _, old, new)| is_junk(*old) && is_junk(*new)));
    }

    #[test]
    fn rejects_random_data() {
        let rom: Vec<u8> = (0..0x100000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        assert!(find_chest_table(&rom).is_none());
    }
}
