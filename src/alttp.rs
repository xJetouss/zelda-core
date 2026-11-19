//! Alles, was speziell für A Link to the Past ist: ROM-Prüfung, Patches (Randomizer), Cheats.

use crate::snes::Snes;
use serde::{Deserialize, Serialize};

/// CRC32 der unveränderten US-ROM (ohne Header).
pub const US_CRC: u32 = 0x777A_AC2F;

pub struct RomInfo {
    pub title: String,
    pub crc: u32,
    pub is_alttp: bool,
    pub verified: bool,
    pub sram_size: usize,
    /// Ländercode aus dem ROM-Header ($FFD9)
    pub country: u8,
    pub pal: bool,
    pub lang: GameLang,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameLang {
    De,
    En,
    Fr,
    Jp,
    Other,
}

impl GameLang {
    pub fn name(self, en: bool) -> &'static str {
        match (self, en) {
            (GameLang::De, false) => "Deutsch",
            (GameLang::De, true) => "German",
            (GameLang::En, false) => "Englisch",
            (GameLang::En, true) => "English",
            (GameLang::Fr, false) => "Französisch",
            (GameLang::Fr, true) => "French",
            (GameLang::Jp, false) => "Japanisch",
            (GameLang::Jp, true) => "Japanese",
            (_, false) => "Unbekannt",
            (_, true) => "Unknown",
        }
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Entfernt einen eventuellen 512-Byte-Kopierer-Header und prüft die ROM.
pub fn prepare_rom(mut data: Vec<u8>) -> Result<(Vec<u8>, RomInfo), String> {
    if data.len() % 1024 == 512 {
        data.drain(0..512);
    }
    if data.len() < 0x10000 {
        return Err("Die Datei ist zu klein für eine SNES-ROM.".into());
    }
    let title: String = data[0x7FC0..0x7FD5]
        .iter()
        .map(|&c| if (0x20..0x7F).contains(&c) { c as char } else { ' ' })
        .collect::<String>()
        .trim()
        .to_string();
    let crc = crc32(&data);
    let lorom = data[0x7FD5] & 1 == 0;
    let verified = crc == US_CRC;
    let upper = title.to_uppercase();
    let is_alttp = verified
        || (lorom && (upper.contains("ZELDA") || upper.starts_with("VT") || upper.starts_with("ER") || upper.contains("LTTP")));
    let sram_code = data[0x7FD8];
    let mut sram_size = if sram_code == 0 || sram_code > 7 { 0 } else { 1024usize << sram_code };
    if is_alttp && sram_size < 0x2000 {
        sram_size = 0x2000;
    }
    let country = data[0x7FD9];
    let pal = (2..=12).contains(&country);
    let lang = match country {
        0x00 => GameLang::Jp,
        0x01 | 0x02 => GameLang::En,
        0x06 => GameLang::Fr,
        0x09 => GameLang::De,
        _ => GameLang::Other,
    };
    Ok((data, RomInfo { title, crc, is_alttp, verified, sram_size, country, pal, lang }))
}

// ---------------------------------------------------------------- Patches

pub fn apply_ips(rom: &[u8], patch: &[u8]) -> Result<Vec<u8>, String> {
    if patch.len() < 8 || &patch[..5] != b"PATCH" {
        return Err("Keine gültige IPS-Datei".into());
    }
    let mut out = rom.to_vec();
    let mut p = 5;
    loop {
        if p + 3 > patch.len() {
            return Err("IPS-Datei ist abgeschnitten".into());
        }
        if &patch[p..p + 3] == b"EOF" {
            break;
        }
        let off = (patch[p] as usize) << 16 | (patch[p + 1] as usize) << 8 | patch[p + 2] as usize;
        p += 3;
        let size = (*patch.get(p).ok_or("IPS defekt")? as usize) << 8 | *patch.get(p + 1).ok_or("IPS defekt")? as usize;
        p += 2;
        if size == 0 {
            let n = (*patch.get(p).ok_or("IPS defekt")? as usize) << 8 | *patch.get(p + 1).ok_or("IPS defekt")? as usize;
            let v = *patch.get(p + 2).ok_or("IPS defekt")?;
            p += 3;
            if out.len() < off + n {
                out.resize(off + n, 0);
            }
            out[off..off + n].fill(v);
        } else {
            let data = patch.get(p..p + size).ok_or("IPS defekt")?;
            p += size;
            if out.len() < off + size {
                out.resize(off + size, 0);
            }
            out[off..off + size].copy_from_slice(data);
        }
    }
    Ok(out)
}

pub fn apply_bps(src: &[u8], patch: &[u8]) -> Result<Vec<u8>, String> {
    if patch.len() < 16 || &patch[..4] != b"BPS1" {
        return Err("Keine gültige BPS-Datei".into());
    }
    let end = patch.len() - 12;
    let rd32 = |o: usize| u32::from_le_bytes([patch[o], patch[o + 1], patch[o + 2], patch[o + 3]]);
    if crc32(src) != rd32(end) {
        return Err("Dieser Patch gehört zu einer anderen Basis-ROM (CRC passt nicht). Bitte die unveränderte US-ROM ohne Header verwenden.".into());
    }
    let mut p = 4usize;
    let num = |p: &mut usize| -> Result<u64, String> {
        let mut data = 0u64;
        let mut shift = 1u64;
        loop {
            let x = *patch.get(*p).ok_or("BPS defekt")? as u64;
            *p += 1;
            data += (x & 0x7F) * shift;
            if x & 0x80 != 0 {
                break;
            }
            shift <<= 7;
            data += shift;
        }
        Ok(data)
    };
    let _src_size = num(&mut p)?;
    let tgt_size = num(&mut p)? as usize;
    let meta = num(&mut p)? as usize;
    p += meta;
    let mut out = vec![0u8; tgt_size];
    let (mut op, mut src_rel, mut tgt_rel) = (0usize, 0i64, 0i64);
    while p < end {
        let d = num(&mut p)?;
        let len = (d >> 2) as usize + 1;
        if op + len > tgt_size {
            return Err("BPS defekt".into());
        }
        match d & 3 {
            0 => {
                for i in 0..len {
                    out[op + i] = *src.get(op + i).unwrap_or(&0);
                }
                op += len;
            }
            1 => {
                let data = patch.get(p..p + len).ok_or("BPS defekt")?;
                out[op..op + len].copy_from_slice(data);
                p += len;
                op += len;
            }
            2 => {
                let o = num(&mut p)? as i64;
                src_rel += if o & 1 != 0 { -(o >> 1) } else { o >> 1 };
                for _ in 0..len {
                    out[op] = *src.get(src_rel as usize).unwrap_or(&0);
                    op += 1;
                    src_rel += 1;
                }
            }
            _ => {
                let o = num(&mut p)? as i64;
                tgt_rel += if o & 1 != 0 { -(o >> 1) } else { o >> 1 };
                for _ in 0..len {
                    out[op] = out[tgt_rel as usize];
                    op += 1;
                    tgt_rel += 1;
                }
            }
        }
    }
    if crc32(&out) != rd32(end + 4) {
        return Err("Patch-Ergebnis fehlerhaft (Ziel-CRC passt nicht)".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------- Cheats

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Cheats {
    pub inf_health: bool,
    pub inf_magic: bool,
    pub inf_bombs: bool,
    pub inf_arrows: bool,
    pub max_rupees: bool,
    pub inf_keys: bool,
    pub half_magic: bool,
    pub moon_pearl: bool,
    /// Durch Wände gehen (Debug-Flag des Spiels bei $7E037F)
    pub noclip: bool,
    /// Titel/Vorspann automatisch mit Start überspringen
    pub skip_title: bool,
    /// Neuer Spielstand beginnt nach der Flucht aus dem Schloss
    pub skip_prologue: bool,
    /// Unverwundbar: kein Schaden, kein Rückstoß (Spiel-Flag $7E037B)
    pub invincible: bool,
    /// größte Bomben- und Pfeiltasche (50 Bomben, 70 Pfeile)
    pub max_capacity: bool,
    /// in allen Dungeons Karte, Kompass und Großer Schlüssel
    pub dungeon_items: bool,
    /// vorhandene Flaschen enthalten immer eine Fee
    pub fairy_bottles: bool,
    /// dunkle Räume immer hell
    pub always_light: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CheatCode {
    pub code: String,
    pub desc: String,
    pub enabled: bool,
}

pub enum Code {
    Ram { addr: u32, val: u8 },
    Rom { addr: u32, val: u8 },
}

/// Pro Action Replay (`7EF36DA0`, auch `7EF36D:A0`) oder Game Genie (`XXXX-XXXX`).
pub fn decode_code(s: &str) -> Result<Code, String> {
    let t: String = s.chars().filter(|c| !c.is_whitespace() && *c != ':').collect::<String>().to_uppercase();
    if t.len() == 9 && t.as_bytes()[4] == b'-' {
        let mut n: u32 = 0;
        for ch in t.chars().filter(|&c| c != '-') {
            let i = "DF4709156BC8A23E".find(ch).ok_or("Ungültiges Game-Genie-Zeichen")?;
            n = (n << 4) | i as u32;
        }
        let val = (n >> 24) as u8;
        let d = n & 0xFF_FFFF;
        let addr = ((d & 0x003C00) << 10)
            | ((d & 0x00003C) << 14)
            | ((d & 0xF00000) >> 8)
            | ((d & 0x000003) << 10)
            | ((d & 0x00C000) >> 6)
            | ((d & 0x0F0000) >> 12)
            | ((d & 0x0003C0) >> 6);
        return Ok(Code::Rom { addr, val });
    }
    if t.len() == 8 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let n = u32::from_str_radix(&t, 16).map_err(|e| e.to_string())?;
        let addr = n >> 8;
        let val = n as u8;
        let bank = addr >> 16;
        if bank == 0x7E || bank == 0x7F || (bank & 0x7F < 0x40 && addr & 0xFFFF < 0x2000) {
            return Ok(Code::Ram { addr, val });
        }
        return Ok(Code::Rom { addr, val });
    }
    Err("Format: PAR 7EF36DA0 oder Game Genie XXXX-XXXX".into())
}

/// LoROM-Adresse -> Offset in der ROM-Datei.
pub fn rom_offset(addr: u32) -> Option<usize> {
    if addr & 0x8000 == 0 && (addr >> 16) & 0x7F < 0x40 {
        return None;
    }
    Some(((addr >> 16) as usize & 0x7F) * 0x8000 + (addr as usize & 0x7FFF))
}

fn find(rom: &[u8], pat: &[u8], from: usize, to: usize) -> Option<usize> {
    let to = to.min(rom.len().saturating_sub(pat.len()));
    (from..to).find(|&i| &rom[i..i + pat.len()] == pat)
}

/// LoROM-Dateiposition -> SNES-Langadresse (Bank, Adresse)
fn lorom(off: usize) -> [u8; 3] {
    let bank = (off / 0x8000) as u8;
    let a = 0x8000 + (off % 0x8000) as u16;
    [a as u8, (a >> 8) as u8, bank]
}

/// Freien Platz (0xFF) für kleine Zusatzroutinen suchen (in den unteren Banks, damit JSL sicher ist)
fn free_space(rom: &[u8], len: usize) -> Option<usize> {
    let mut run = 0;
    for i in 0x10000..rom.len().min(0x200000) {
        // nicht über eine Bankgrenze
        if i % 0x8000 == 0 {
            run = 0;
        }
        if rom[i] == 0xFF {
            run += 1;
            if run >= (len + 32).max(512) {
                return Some(i + 1 - run + 16);
            }
        } else {
            run = 0;
        }
    }
    None
}

/// Spiel-Verbesserungen direkt in der ROM (wie im zelda3-Projekt):
/// * `more_bombs`: bis zu 4 Bomben gleichzeitig statt 2
/// * `mirror_dark`: Spiegel auch in der Lichtwelt -> Schattenwelt (mit der normalen Spiegel-Animation)
/// * `sword_pots`: Töpfe in Dungeons mit dem Schwert zerschlagen (ab Master-Schwert, wie der Hammer)
pub fn apply_tweaks(rom: &mut [u8], more_bombs: bool, sword_pots: bool, mirror_dark: bool) {
    if mirror_dark {
        // LinkItem_Mirror: in der Lichtwelt nicht abbrechen (LDA $8A : AND #$40 : BNE ok -> BRA ok)
        if let Some(i) = find(rom, &[0xA5, 0x1B, 0xD0, 0x0D, 0xA5, 0x8A, 0x29, 0x40, 0xD0, 0x07, 0xA9, 0x3C], 0x38000, 0x40000) {
            rom[i + 8] = 0x80;
        }
    }
    if more_bombs {
        // LDY #$01 : LDA #$07 : JSL AncillaAdd_Bomb  ->  LDY #$03
        if let Some(i) = find(rom, &[0xA0, 0x01, 0xA9, 0x07, 0x22], 0x30000, 0x40000) {
            rom[i + 1] = 0x03;
        }
        // Ancilla_AllocInit: freie Plätze für Bomben bis zum Limit suchen (LDY #$01 -> LDY $0F)
        if let Some(i) = find(rom, &[0xA0, 0x01, 0xC9, 0x07, 0xF0, 0x06, 0xC9, 0x08], 0x78000, 0x80000) {
            rom[i] = 0xA4;
            rom[i + 1] = 0x0F;
        }
    }
    if sword_pots {
        // HandleItemTileAction_Dungeon: REP #$30 : LDA $0301 : AND #$0002 : BEQ return
        let gate = find(rom, &[0xC2, 0x30, 0xAD, 0x01, 0x03, 0x29, 0x02, 0x00, 0xF0, 0xF1], 0x8000, 0x10000).map(|i| i + 2);
        let peg = gate.and_then(|g| find(rom, &[0x29, 0xF0, 0xF0, 0xC9, 0x40, 0x40, 0xD0, 0x15], g, g + 0x60));
        let space = free_space(rom, 64);
        if let (Some(g), Some(pg), Some(sp)) = (gate, peg, space) {
            // Routine 1: Hammer ODER (Schwertschlag aktiv und mindestens Master-Schwert) -> A != 0
            let r1: [u8; 35] = [
                0xAD, 0x01, 0x03, 0x29, 0x02, 0x00, 0xD0, 0x13, // LDA $0301 : AND #2 : BNE ok
                0xA5, 0x3C, 0x29, 0xFF, 0x00, 0xF0, 0x10, // LDA $3C : AND #$FF : BEQ no
                0xAF, 0x59, 0xF3, 0x7E, 0x29, 0xFF, 0x00, 0xC9, 0x02, 0x00, 0x90, 0x04, // LDA $7EF359 : AND : CMP #2 : BCC no
                0xA9, 0x01, 0x00, 0x6B, // ok: LDA #1 : RTL
                0xA9, 0x00, 0x00, 0x6B, // no: LDA #0 : RTL
            ];
            // Routine 2: Hammerpflöcke weiterhin nur mit dem Hammer
            let r2: [u8; 28] = [
                0x29, 0xF0, 0xF0, 0xC9, 0x40, 0x40, 0xD0, 0x13, // AND #$F0F0 : CMP #$4040 : BNE done
                0x48, 0xAD, 0x01, 0x03, 0x29, 0x02, 0x00, 0xF0, 0x05, // PHA : LDA $0301 : AND #2 : BEQ nohammer
                0x68, 0xC9, 0x40, 0x40, 0x6B, // PLA : CMP #$4040 : RTL (Z=1 -> Pflock)
                0x68, 0xC9, 0x41, 0x40, 0x6B, // nohammer: PLA : CMP #$4041 : RTL (Z=0)
                0x6B, // done: RTL
            ];
            rom[sp..sp + r1.len()].copy_from_slice(&r1);
            let sp2 = sp + 36;
            rom[sp2..sp2 + r2.len()].copy_from_slice(&r2);
            let a1 = lorom(sp);
            let a2 = lorom(sp2);
            // Tor: JSL r1 : BEQ return : NOP : NOP
            rom[g..g + 8].copy_from_slice(&[0x22, a1[0], a1[1], a1[2], 0xF0, 0xF3, 0xEA, 0xEA]);
            // Pflock-Prüfung: JSL r2 : NOP : NOP (BNE bleibt)
            rom[pg..pg + 6].copy_from_slice(&[0x22, a2[0], a2[1], a2[2], 0xEA, 0xEA]);
        }
    }
}

/// Weitere Verbesserungen: kürzerer Pegasus-Anlauf, kein Warnton bei wenig Energie
pub fn apply_tweaks2(rom: &mut [u8], quick_dash: bool, no_beep: bool) {
    if quick_dash {
        // Anlauf-Zähler beim Start des Sprints: 29 Bilder -> 12 (LDA #$1D : STA $0374 : LDA #$40)
        if let Some(i) = find(rom, &[0xA9, 0x1D, 0x8D, 0x74, 0x03, 0xA9, 0x40], 0x38000, 0x40000) {
            rom[i + 1] = 0x0C;
        }
    }
    if no_beep {
        // Spiel-Anzeige: Warnton $2B bei wenig Energie nicht mehr abspielen (STA $012E -> NOP)
        if let Some(i) = find(rom, &[0xA9, 0x20, 0x8D, 0xCA, 0x04, 0xA9, 0x2B, 0x8D, 0x2E, 0x01], 0x60000, 0x70000) {
            rom[i + 7] = 0xEA;
            rom[i + 8] = 0xEA;
            rom[i + 9] = 0xEA;
        }
    }
}

/// Echtes Breitbild (wie `ExtendScreen64` im zelda3-Projekt): Gegner und Gegenstände leben und
/// erscheinen auch 64 Pixel links und rechts außerhalb des 4:3-Bilds. Gibt zurück, wie viele Stellen gepatcht wurden.
pub fn apply_wide_sprites(rom: &mut [u8]) -> usize {
    let mut n = 0;
    // 1) Sprites erst weiter draußen entfernen/abschalten: (x + $40) >= $170  ->  (x + $80) >= $1F0
    //    (nur die X-Prüfung; direkt danach folgt dieselbe Prüfung für Y)
    if let Some(i) = find(rom, &[0x38, 0xE5, 0xE2, 0x85, 0x00, 0x18, 0x69, 0x40, 0x00, 0xC9, 0x70, 0x01], 0x30000, 0x38000) {
        rom[i + 7] = 0x80;
        rom[i + 10] = 0xF0;
        n += 1;
    }
    // 2) Gegner in der Nähe früher nachladen: Spalte bei +$110 / -$10  ->  +$150 / -$50
    let mut from = 0x48000;
    while let Some(i) = find(rom, &[0xA5, 0xE2, 0x18, 0x79], from, 0x50000) {
        from = i + 4;
        let lo = u16::from_le_bytes([rom[i + 4], rom[i + 5]]) as usize;
        let tab = (i & !0x7FFF) + (lo & 0x7FFF);
        if rom.get(tab..tab + 4) == Some(&[0x10, 0xF0, 0x01, 0xFF]) {
            rom[tab..tab + 4].copy_from_slice(&[0x50, 0xB0, 0x01, 0xFF]);
            n += 1;
            break;
        }
    }
    // 3) beim Betreten eines Gebiets 29 statt 21 Spalten prüfen (LDY #$15 : PHY : JSR …)
    if let Some(i) = find(rom, &[0xA9, 0xFF, 0x8D, 0x9F, 0x06, 0xA0, 0x15, 0x5A, 0x20], 0x48000, 0x50000) {
        rom[i + 6] = 0x1D;
        n += 1;
    }
    // 4) waagerechte Nachlade-Zeile breiter: X ab Kamera -$70 statt -$30, 29 statt 21 Spalten
    if let Some(i) = find(rom, &[0xA5, 0xE2, 0x38, 0xE9, 0x30, 0x85, 0x0E, 0xA5, 0xE3, 0xE9, 0x00, 0x85, 0x0F, 0xA2, 0x15], 0x48000, 0x50000) {
        rom[i + 4] = 0x70;
        rom[i + 14] = 0x1D;
        n += 1;
    }
    // 5) Gegenstände/Geschosse (Ancilla, sichere Variante): (x + $80) < $180  ->  < $1C8
    let mut from = 0x40000;
    while let Some(i) = find(rom, &[0x18, 0x69, 0x80, 0x00, 0xC9, 0x80, 0x01, 0xB0], from, 0x50000) {
        rom[i + 5] = 0xC8;
        n += 1;
        from = i + 8;
    }
    // 6) und 7) brauchen ein paar Befehle mehr -> kleine Routinen im freien Platz
    let Some(sp) = free_space(rom, 64) else { return n };
    let mut at = sp;
    let mut put = |rom: &mut [u8], code: &[u8]| -> [u8; 3] {
        rom[at..at + code.len()].copy_from_slice(code);
        let a = lorom(at);
        at += code.len() + 4;
        a
    };
    // 6) Ancilla_SetOam: x < $100  ->  (x + $40) < $180 (A bleibt erhalten, Flags vom Vergleich)
    if let Some(i) = find(rom, &[0x38, 0xE5, 0xE2, 0x85, 0x02, 0xC9, 0x00, 0x01, 0xB0], 0x40000, 0x48000) {
        let code = [0x85, 0x02, 0x18, 0x69, 0x40, 0x00, 0xC9, 0x80, 0x01, 0x08, 0x38, 0xE9, 0x40, 0x00, 0x28, 0x6B];
        let a = put(rom, &code);
        rom[i + 3..i + 8].copy_from_slice(&[0x22, a[0], a[1], a[2], 0xEA]);
        n += 1;
    }
    // 7) Geschosse verschwinden erst weiter draußen: (x - Kamera) < $F4  ->  (x - Kamera + $40) < $174
    if let Some(i) = find(rom, &[0xA5, 0x0E, 0x38, 0xE5, 0xE2, 0xC9, 0xF4, 0x00, 0x90], 0x78000, 0x80000) {
        let code = [0x38, 0xE5, 0xE2, 0x18, 0x69, 0x40, 0x00, 0xC9, 0x74, 0x01, 0x6B];
        let a = put(rom, &code);
        rom[i + 2..i + 8].copy_from_slice(&[0x22, a[0], a[1], a[2], 0xEA, 0xEA]);
        n += 1;
    }
    n
}

/// Wendet die ROM-Codes (Game Genie / ROM-PAR) auf eine Kopie der Original-ROM an.
pub fn patch_rom_codes(orig: &[u8], codes: &[CheatCode]) -> Vec<u8> {
    let mut rom = orig.to_vec();
    for c in codes.iter().filter(|c| c.enabled) {
        if let Ok(Code::Rom { addr, val }) = decode_code(&c.code) {
            if let Some(o) = rom_offset(addr) {
                if o < rom.len() {
                    rom[o] = val;
                }
            }
        }
    }
    rom
}

#[inline]
fn ram_index(addr: u32) -> usize {
    if (addr >> 16) & 0xFE == 0x7E {
        (addr & 0x1FFFF) as usize
    } else {
        (addr & 0x1FFF) as usize
    }
}
#[inline]
fn r8(s: &Snes, a: u32) -> u8 {
    s.bus.wram[ram_index(a)]
}
#[inline]
fn w8(s: &mut Snes, a: u32, v: u8) {
    s.bus.wram[ram_index(a)] = v;
}

/// Hauptmodus des Spiels ($7E0010). Cheats nur im echten Spiel, nicht in Titel/Dateiauswahl/Demo.
pub fn in_game(s: &Snes) -> bool {
    let m = r8(s, 0x7E0010);
    (0x06..=0x13).contains(&m) || (0x15..=0x18).contains(&m)
}

pub fn apply_cheats(s: &mut Snes, c: &Cheats, codes: &[CheatCode]) {
    for cc in codes.iter().filter(|c| c.enabled) {
        if let Ok(Code::Ram { addr, val }) = decode_code(&cc.code) {
            w8(s, addr, val);
        }
    }
    if c.skip_prologue && r8(s, 0x7E0010) == 0x01 {
        // In der Spielerauswahl: neue Spielstände im SRAM (nur im Speicher) als "Zelda gerettet" markieren
        if skip_prologue_sram(&mut s.bus.sram) {
            s.bus.sram_dirty = true;
        }
    }
    if !in_game(s) {
        return;
    }
    if c.noclip {
        w8(s, 0x7E037F, 1);
    }
    if c.inf_health {
        let max = r8(s, 0x7EF36C);
        w8(s, 0x7EF36D, max);
    }
    if c.inf_magic {
        w8(s, 0x7EF36E, 0x80);
    }
    if c.inf_bombs && r8(s, 0x7EF343) < 10 {
        w8(s, 0x7EF343, 10);
    }
    if c.inf_arrows && r8(s, 0x7EF377) < 30 {
        w8(s, 0x7EF377, 30);
    }
    if c.max_rupees {
        w8(s, 0x7EF360, 0xE7);
        w8(s, 0x7EF361, 0x03);
    }
    if c.inf_keys {
        let k = r8(s, 0x7EF36F);
        if k != 0xFF && k < 9 {
            w8(s, 0x7EF36F, 9);
        }
    }
    if c.half_magic {
        w8(s, 0x7EF37B, 1);
    }
    if c.moon_pearl {
        w8(s, 0x7EF357, 1);
    }
    if c.invincible {
        w8(s, 0x7E037B, 1);
    }
    if c.max_capacity {
        // Aufrüstungen: +7 Stufen = 50 Bomben, 70 Pfeile
        w8(s, 0x7EF370, 7);
        w8(s, 0x7EF371, 7);
    }
    if c.dungeon_items {
        for a in [0x7EF364u32, 0x7EF365, 0x7EF366, 0x7EF367, 0x7EF368, 0x7EF369] {
            w8(s, a, 0xFF);
        }
    }
    if c.fairy_bottles {
        for b in 0..4u32 {
            // nur vorhandene Flaschen (2 = leer und mehr), nicht gerade in Benutzung
            if r8(s, 0x7EF35C + b) >= 2 {
                w8(s, 0x7EF35C + b, 6);
            }
        }
    }
}

/// Dunkle Räume hell machen: das Spiel zieht dort die Lichtmaske (BG1 auf dem Subscreen) per Farbsubtraktion ab
/// ($9A = Spiegel von CGADSUB) -> auf die normale Einstellung der hellen Räume setzen
/// Gibt den alten Wert zurück, wenn etwas geändert wurde (zum Wiederherstellen beim Ausschalten).
pub fn always_light(s: &mut Snes) -> Option<u8> {
    let v = r8(s, 0x7E009A);
    if r8(s, 0x7E001B) != 0 && v & 0x80 != 0 {
        w8(s, 0x7E009A, 0x20);
        return Some(v);
    }
    None
}

pub fn give_all_items(s: &mut Snes) {
    let items: &[(u32, u8)] = &[
        (0x7EF340, 4), // Silberpfeil-Bogen
        (0x7EF341, 2), // Magischer Bumerang
        (0x7EF342, 1), // Enterhaken
        (0x7EF343, 10),
        (0x7EF344, 2), // Zauberpuder
        (0x7EF345, 1), // Feuerstab
        (0x7EF346, 1), // Eisstab
        (0x7EF347, 1), // Bombos
        (0x7EF348, 1), // Ether
        (0x7EF349, 1), // Quake
        (0x7EF34A, 1), // Laterne
        (0x7EF34B, 1), // Hammer
        (0x7EF34C, 3), // Flöte (aktiviert)
        (0x7EF34D, 1), // Netz
        (0x7EF34E, 1), // Buch Mudora
        (0x7EF34F, 1), // Flasche ausgewählt
        (0x7EF350, 1), // Somaria
        (0x7EF351, 1), // Byrna
        (0x7EF352, 1), // Tarnumhang
        (0x7EF353, 2), // Spiegel
        (0x7EF354, 2), // Titanhandschuh
        (0x7EF355, 1), // Pegasusstiefel
        (0x7EF379, 0xFC), // Fähigkeiten (Rennen, Lesen, Heben, Schwimmen …)
        (0x7EF356, 1), // Zora-Flossen
        (0x7EF357, 1), // Mondperle
        (0x7EF35C, 3), // Flaschen: rot, grün, blau, Fee
        (0x7EF35D, 4),
        (0x7EF35E, 5),
        (0x7EF35F, 6),
        (0x7EF377, 30),
    ];
    for &(a, v) in items {
        w8(s, a, v);
    }
}

pub fn set_max_hearts(s: &mut Snes) {
    w8(s, 0x7EF36C, 0xA0);
    w8(s, 0x7EF36D, 0xA0);
}

pub fn set_equipment(s: &mut Snes, sword: u8, shield: u8, armor: u8) {
    w8(s, 0x7EF359, sword);
    w8(s, 0x7EF35A, shield);
    w8(s, 0x7EF35B, armor);
}

/// Markiert jeden gültigen Spielstand mit Fortschritt 0 als "Zelda gerettet" (Fortschritt 2),
/// gibt Schwert, Schild und Laterne und korrigiert die Prüfsumme (Summe aller Wörter = 0x5A5A).
pub fn skip_prologue_sram(sram: &mut [u8]) -> bool {
    let mut changed = false;
    for slot in 0..3 {
        let b = slot * 0x500;
        if sram.len() < b + 0x500 {
            break;
        }
        let sig = sram[b + 0x3E5] as u16 | (sram[b + 0x3E6] as u16) << 8;
        if sig != 0x55AA || sram[b + 0x3C5] != 0 {
            continue;
        }
        sram[b + 0x3C5] = 2; // Fortschritt: Zelda gerettet
        sram[b + 0x3CC] = 0; // kein Begleiter
        sram[b + 0x3C6] |= 0x14; // Story-Flags nach der Flucht aus dem Schloss
        sram[b + 0x3C8] = 0x01; // Startpunkt
        for (o, v) in [(0x359, 1u8), (0x35A, 1), (0x34A, 1)] {
            if sram[b + o] == 0 {
                sram[b + o] = v;
            }
        }
        let mut sum: u16 = 0;
        for i in (0..0x4FE).step_by(2) {
            sum = sum.wrapping_add(sram[b + i] as u16 | (sram[b + i + 1] as u16) << 8);
        }
        let ck = 0x5A5Au16.wrapping_sub(sum);
        sram[b + 0x4FE] = ck as u8;
        sram[b + 0x4FF] = (ck >> 8) as u8;
        changed = true;
    }
    changed
}

/// Wird beim Ausschalten von Noclip einmal aufgerufen.
pub fn clear_noclip(s: &mut Snes) {
    w8(s, 0x7E037F, 0);
}

/// Für "Titel überspringen": befindet sich das Spiel in Vorspann/Titel/Demo?
pub fn in_title(s: &Snes) -> bool {
    let m = r8(s, 0x7E0010);
    m == 0x00 || m == 0x14
}

pub fn ram16(s: &Snes, a: u32) -> u16 {
    r8(s, a) as u16 | (r8(s, a + 1) as u16) << 8
}
pub fn ram8(s: &Snes, a: u32) -> u8 {
    r8(s, a)
}

/// Namen der Item-IDs (wie in Truhen verwendet). (Deutsch, Englisch)
pub fn item_name(id: u8, en: bool) -> String {
    const N: [(&str, &str); 0x4C] = [
        ("Kämpferschwert+Schild", "Fighter Sword+Shield"), ("Master-Schwert", "Master Sword"), ("Gehärtetes Schwert", "Tempered Sword"), ("Goldenes Schwert", "Golden Sword"),
        ("Holzschild", "Fighter Shield"), ("Feuerschild", "Fire Shield"), ("Spiegelschild", "Mirror Shield"), ("Feuerstab", "Fire Rod"),
        ("Eisstab", "Ice Rod"), ("Hammer", "Hammer"), ("Enterhaken", "Hookshot"), ("Bogen", "Bow"),
        ("Bumerang", "Boomerang"), ("Zauberpuder", "Magic Powder"), ("Biene", "Bee"), ("Bombos", "Bombos"),
        ("Ether", "Ether"), ("Quake", "Quake"), ("Laterne", "Lamp"), ("Schaufel", "Shovel"),
        ("Flöte", "Flute"), ("Somaria-Stab", "Cane of Somaria"), ("Flasche", "Bottle"), ("Herzteil", "Piece of Heart"),
        ("Byrna-Stab", "Cane of Byrna"), ("Tarnumhang", "Magic Cape"), ("Zauberspiegel", "Magic Mirror"), ("Krafthandschuh", "Power Glove"),
        ("Titanhandschuh", "Titan's Mitt"), ("Buch Mudora", "Book of Mudora"), ("Zora-Flossen", "Zora's Flippers"), ("Mondperle", "Moon Pearl"),
        ("Kristall", "Crystal"), ("Käfernetz", "Bug Net"), ("Blaue Rüstung", "Blue Mail"), ("Rote Rüstung", "Red Mail"),
        ("Kleiner Schlüssel", "Small Key"), ("Kompass", "Compass"), ("Herzcontainer", "Heart Container"), ("1 Bombe", "Bomb"),
        ("3 Bomben", "3 Bombs"), ("Pilz", "Mushroom"), ("Magischer Bumerang", "Magic Boomerang"), ("Roter Trank", "Red Potion"),
        ("Grüner Trank", "Green Potion"), ("Blauer Trank", "Blue Potion"), ("Roter Trank", "Red Potion"), ("Grüner Trank", "Green Potion"),
        ("Blauer Trank", "Blue Potion"), ("10 Bomben", "10 Bombs"), ("Großer Schlüssel", "Big Key"), ("Karte", "Map"),
        ("1 Rubin", "1 Rupee"), ("5 Rubine", "5 Rupees"), ("20 Rubine", "20 Rupees"), ("Amulett des Mutes", "Pendant of Courage"),
        ("Amulett der Weisheit", "Pendant of Wisdom"), ("Amulett der Kraft", "Pendant of Power"), ("Bogen & Pfeile", "Bow & Arrows"), ("Silberpfeil-Bogen", "Silver Arrows Bow"),
        ("Bienenflasche", "Bee Bottle"), ("Feenflasche", "Fairy Bottle"), ("Herzcontainer (Boss)", "Boss Heart Container"), ("Herzcontainer", "Heart Container"),
        ("100 Rubine", "100 Rupees"), ("50 Rubine", "50 Rupees"), ("Herz", "Heart"), ("1 Pfeil", "Arrow"),
        ("10 Pfeile", "10 Arrows"), ("Kleine Magie", "Small Magic"), ("300 Rubine", "300 Rupees"), ("20 Rubine", "20 Rupees"),
        ("Goldbiene", "Good Bee"), ("Kämpferschwert", "Fighter Sword"), ("Flöte (aktiviert)", "Flute (active)"), ("Pegasusstiefel", "Pegasus Boots"),
    ];
    match N.get(id as usize) {
        Some((d, e)) => (if en { *e } else { *d }).to_string(),
        None => format!("Item {id:02X}"),
    }
}

/// Inventar-Einträge für den 100%-Tracker: (Adresse, Deutsch, Englisch)
pub const INVENTORY: [(u32, &str, &str); 27] = [
    (0x7EF359, "Schwert", "Sword"), (0x7EF35A, "Schild", "Shield"), (0x7EF35B, "Rüstung", "Mail"),
    (0x7EF340, "Bogen", "Bow"), (0x7EF341, "Bumerang", "Boomerang"), (0x7EF342, "Enterhaken", "Hookshot"),
    (0x7EF343, "Bomben", "Bombs"), (0x7EF344, "Pilz/Puder", "Mushroom/Powder"), (0x7EF345, "Feuerstab", "Fire Rod"),
    (0x7EF346, "Eisstab", "Ice Rod"), (0x7EF347, "Bombos", "Bombos"), (0x7EF348, "Ether", "Ether"),
    (0x7EF349, "Quake", "Quake"), (0x7EF34A, "Laterne", "Lamp"), (0x7EF34B, "Hammer", "Hammer"),
    (0x7EF34C, "Schaufel/Flöte", "Shovel/Flute"), (0x7EF34D, "Käfernetz", "Bug Net"), (0x7EF34E, "Buch Mudora", "Book"),
    (0x7EF350, "Somaria", "Somaria"), (0x7EF351, "Byrna", "Byrna"), (0x7EF352, "Tarnumhang", "Cape"),
    (0x7EF353, "Spiegel", "Mirror"), (0x7EF354, "Handschuhe", "Gloves"), (0x7EF355, "Pegasusstiefel", "Boots"),
    (0x7EF356, "Flossen", "Flippers"), (0x7EF357, "Mondperle", "Moon Pearl"), (0x7EF35C, "Flaschen", "Bottles"),
];
