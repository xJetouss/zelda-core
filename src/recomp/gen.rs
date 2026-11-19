//! Recompiler: übersetzt 65C816-Code der ROM in Rust-Quelltext (läuft im Build-Schritt, siehe `build.rs`).
//!
//! Eingabe:
//! - die ROM, wie sie im Spiel läuft (`recomp/rom.bin`),
//! - eine Aufzeichnung der tatsächlich ausgeführten Befehle (`recomp/trace.txt`: Adresse, M, X, E, Anzahl).
//!   Nur so lassen sich Sprungtabellen und die Registerbreiten (M/X) zuverlässig bestimmen.
//!
//! Ausgabe: je Block (gerade Befehlsfolge bis zum nächsten Sprung) eine Rust-Funktion, die die Befehle mit
//! festen Opcodes hintereinander ausführt, dazu eine Tabelle (Adresse, Länge, Prüfsumme der Bytes).
//! Zur Laufzeit wird jeder Block gegen die geladene ROM geprüft. Passt er nicht (andere Version, Patches),
//! übernimmt dort der Interpreter. Nach jedem Befehl prüft der übersetzte Code, ob die CPU dort angekommen
//! ist, wo der Recompiler es erwartet hat (z. B. andere Registerbreite). Wenn nicht, geht es mit dem
//! Interpreter weiter. Das Ergebnis ist deshalb immer genau dasselbe wie bei der Emulation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Operandenbytes: feste Anzahl, oder abhängig von M bzw. X (Direktwert)
#[derive(Clone, Copy, PartialEq)]
enum Len {
    Fix(u8),
    M,
    X,
}

fn operand(op: u8) -> Len {
    use Len::*;
    match op {
        0x09 | 0x29 | 0x49 | 0x69 | 0x89 | 0xA9 | 0xC9 | 0xE9 => M,
        0xA0 | 0xA2 | 0xC0 | 0xE0 => X,
        0x20 | 0x4C | 0x6C | 0x7C | 0xDC | 0xFC | 0xF4 | 0x62 | 0x82 | 0x44 | 0x54 => Fix(2),
        0x22 | 0x5C => Fix(3),
        0x40 | 0x60 => Fix(0), // RTI, RTS
        _ => match op & 0x1F {
            0x08 | 0x0A | 0x0B | 0x18 | 0x1A | 0x1B => Fix(0),
            0x0C | 0x0D | 0x0E | 0x19 | 0x1C | 0x1D | 0x1E => Fix(2),
            0x0F | 0x1F => Fix(3),
            _ => Fix(1),
        },
    }
}

/// Befehle, nach denen der Block endet (Sprünge, Rücksprünge, Unterbrechungen, Flag-Wechsel mit
/// unbekanntem Ergebnis, Blockkopie mit Schleife)
fn ends_block(op: u8) -> bool {
    matches!(
        op,
        0x00 | 0x02 | 0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 | 0x80 | 0x82 | 0x20 | 0x22 | 0x4C | 0x5C | 0x6C | 0x7C | 0xDC | 0xFC | 0x40 | 0x60 | 0x6B | 0xCB | 0xDB | 0x44 | 0x54 | 0x28 | 0xFB
    )
}

fn inst_len(op: u8, m: bool, x: bool) -> u16 {
    1 + match operand(op) {
        Len::Fix(n) => n as u16,
        Len::M => if m { 1 } else { 2 },
        Len::X => if x { 1 } else { 2 },
    }
}

/// ROM-Index einer LoROM-Adresse (wie `Bus::rom_index`); None außerhalb des ROM-Bereichs
fn rom_index(addr: u32, len: usize) -> Option<usize> {
    let bank = (addr >> 16) as u8;
    let off = addr as u16;
    if (bank & 0x7F) >= 0x40 || off < 0x8000 || len == 0 {
        return None;
    }
    Some((((bank & 0x7F) as usize) * 0x8000 + (off as usize & 0x7FFF)) % len)
}

/// FNV-1a über die Bytes eines Blocks (zur Prüfung gegen die geladene ROM)
pub fn hash(bytes: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for &b in bytes {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01B3);
    }
    h
}

pub struct Stats {
    pub blocks: usize,
    pub insts: usize,
}

/// Erzeugt den Rust-Quelltext. `trace`: Zeilen „AAAAAA m x e anzahl“ (hex Adresse, 0/1-Flags).
pub fn generate(rom: &[u8], trace: &str, min_count: u64) -> (String, Stats) {
    // ausgeführte Befehle mit ihren häufigsten Flags
    let mut seen: BTreeMap<u32, ((bool, bool), u64)> = BTreeMap::new();
    for line in trace.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 5 {
            continue;
        }
        let (Ok(a), Ok(n)) = (u32::from_str_radix(f[0], 16), f[4].parse::<u64>()) else { continue };
        let (m, x, e) = (f[1] == "1", f[2] == "1", f[3] == "1");
        if e || n < min_count || rom_index(a, rom.len()).is_none() {
            continue;
        }
        let ent = seen.entry(a).or_insert(((m, x), 0));
        if n > ent.1 {
            *ent = ((m, x), n);
        }
    }
    // vollständige Analyse der ROM (Phase 1): auch Code, der nie aufgezeichnet wurde, mit seinen M/X-Flags.
    // Aufgezeichnete Flags haben Vorrang; Routinen-Einsprünge und Tabellenziele werden Blockanfänge.
    let an = super::analyze::analyze(rom, &super::analyze::parse_trace(trace));
    for (&a, fl) in &an.insts {
        if let Some(f) = fl.iter().next() {
            seen.entry(a).or_insert(((f.m, f.x), 0));
        }
    }
    let op_at = |a: u32| rom[rom_index(a, rom.len()).unwrap()];
    let next = |a: u32, l: u16| (a & 0xFF0000) | ((a as u16).wrapping_add(l) as u32);
    // Sprungziele und Folgebefehle
    let mut targets: BTreeSet<u32> = an.entries.clone();
    let mut fall: BTreeSet<u32> = BTreeSet::new();
    for (&a, &((m, x), _)) in &seen {
        let op = op_at(a);
        let l = inst_len(op, m, x);
        let rel = |k: u16| rom_index(next(a, k), rom.len()).map(|i| rom[i]).unwrap_or(0);
        match op {
            0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 | 0x80 => {
                let d = rel(1) as i8 as i16 as u16;
                targets.insert((a & 0xFF0000) | (a as u16).wrapping_add(2).wrapping_add(d) as u32);
            }
            0x20 | 0x4C => {
                targets.insert((a & 0xFF0000) | (rel(1) as u32 | (rel(2) as u32) << 8));
            }
            0x22 | 0x5C => {
                targets.insert(rel(1) as u32 | (rel(2) as u32) << 8 | (rel(3) as u32) << 16);
            }
            _ => {}
        }
        if !ends_block(op) {
            fall.insert(next(a, l));
        }
    }
    let mut out = String::new();
    let mut table = String::new();
    let mut stats = Stats { blocks: 0, insts: 0 };
    for (&start, &((m0, x0), _)) in &seen {
        // Blockanfang: Sprungziel oder nicht durch einen anderen Befehl erreichbar
        if fall.contains(&start) && !targets.contains(&start) {
            continue;
        }
        let (mut m, mut x) = (m0, x0);
        let mut a = start;
        let mut insts: Vec<(u32, u8, u32)> = Vec::new();
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let op = op_at(a);
            let l = inst_len(op, m, x);
            let nx = next(a, l);
            // ganzer Befehl im selben ROM-Bereich?
            if (a as u16).checked_add(l).is_none() || (0..l).any(|k| rom_index(next(a, k), rom.len()).is_none()) {
                break;
            }
            for k in 0..l {
                bytes.push(op_at(next(a, k)));
            }
            insts.push((a, op, nx));
            if op == 0xC2 || op == 0xE2 {
                let v = op_at(next(a, 1));
                let set = op == 0xE2;
                if v & 0x20 != 0 {
                    m = set;
                }
                if v & 0x10 != 0 {
                    x = set;
                }
            }
            if ends_block(op) || insts.len() >= 48 {
                break;
            }
            // weiter nur, wenn der nächste Befehl auch ausgeführt wurde, mit passenden Flags, und kein eigener Block ist
            match seen.get(&nx) {
                Some(&((nm, nxf), _)) if nm == m && nxf == x && !targets.contains(&nx) => a = nx,
                _ => break,
            }
        }
        if insts.is_empty() {
            continue;
        }
        stats.blocks += 1;
        stats.insts += insts.len();
        let _ = writeln!(out, "fn b{start:06x}(c: &mut Cpu, b: &mut Bus) {{");
        for (k, &(ia, op, nx)) in insts.iter().enumerate() {
            let _ = writeln!(out, "    i!(c, b, 0x{:02x}, 0x{:04x}, 0x{op:02x}, 0x{:04x}, {});", ia >> 16, ia & 0xFFFF, nx & 0xFFFF, if k == 0 { "first" } else { "next" });
        }
        let _ = writeln!(out, "}}");
        let _ = writeln!(table, "    (0x{start:06x}, {}, 0x{:016x}, b{start:06x}),", bytes.len(), hash(&bytes));
    }
    let mut src = String::new();
    let _ = writeln!(src, "// Automatisch erzeugt vom Zelda-Core-Recompiler (src/recomp/gen.rs) – nicht von Hand ändern.");
    let _ = writeln!(src, "// {} Blöcke, {} Befehle", stats.blocks, stats.insts);
    src.push_str(&out);
    let _ = writeln!(src, "pub static BLOCKS: &[(u32, u16, u64, BlockFn)] = &[");
    src.push_str(&table);
    let _ = writeln!(src, "];");
    (src, stats)
}

/// Leere Ausgabe (keine ROM/Aufzeichnung vorhanden): Recompiled-Modus nicht verfügbar
pub fn empty() -> String {
    "// Keine Aufzeichnung/ROM beim Bauen gefunden – Recompiled-Modus nicht verfügbar.\npub static BLOCKS: &[(u32, u16, u64, BlockFn)] = &[];\n".to_string()
}
