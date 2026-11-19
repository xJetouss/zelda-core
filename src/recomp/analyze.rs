//! Vollständige Code-Analyse der ROM (Phase 1 des Recompilers).
//!
//! Läuft im Build-Schritt und in Dev-Tests. Ergebnis: alle Befehle, die als Code erreichbar sind, mit
//! Registerbreiten (M/X), dazu Routinen-Einsprünge und Sprungtabellen.
//!
//! Vorgehen:
//! 1. Startpunkte: Reset-/NMI-/IRQ-Vektoren und alle aufgezeichneten Befehle (Aufzeichnung = Beleg).
//! 2. Rekursiv weiter: Folgebefehl, Verzweigungen, JMP/JSR/JSL-Ziele (mit M/X über REP/SEP verfolgt).
//! 3. Sprungtabellen-Helfer: Routinen, nach deren Aufruf das Spiel nie zur Folgeadresse zurückkehrt –
//!    hinter jedem Aufruf steht dann eine Tabelle mit Zieladressen (ALttP: lokal 2 Byte, lang 3 Byte).
//! 4. `JMP (abs,X)` / `JSR (abs,X)`: Tabelle an `abs` in derselben Bank, so lange die Einträge auf
//!    plausiblen Code zeigen.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Operandenbytes (M/X-abhängig bei Direktwerten)
pub fn inst_len(op: u8, m: bool, x: bool) -> u16 {
    1 + match op {
        0x09 | 0x29 | 0x49 | 0x69 | 0x89 | 0xA9 | 0xC9 | 0xE9 => if m { 1 } else { 2 },
        0xA0 | 0xA2 | 0xC0 | 0xE0 => if x { 1 } else { 2 },
        0x20 | 0x4C | 0x6C | 0x7C | 0xDC | 0xFC | 0xF4 | 0x62 | 0x82 | 0x44 | 0x54 => 2,
        0x22 | 0x5C => 3,
        0x40 | 0x60 => 0,
        _ => match op & 0x1F {
            0x08 | 0x0A | 0x0B | 0x18 | 0x1A | 0x1B => 0,
            0x0C | 0x0D | 0x0E | 0x19 | 0x1C | 0x1D | 0x1E => 2,
            0x0F | 0x1F => 3,
            _ => 1,
        },
    }
}

/// Befehle, nach denen es nicht einfach mit dem nächsten Byte weitergeht
pub fn no_fallthrough(op: u8) -> bool {
    matches!(op, 0x80 | 0x82 | 0x4C | 0x5C | 0x6C | 0x7C | 0xDC | 0x40 | 0x60 | 0x6B | 0xDB | 0x00 | 0x02)
}

/// LoROM-Adresse -> ROM-Index (nur ROM-Bereich)
pub fn rom_index(addr: u32, len: usize) -> Option<usize> {
    let bank = (addr >> 16) as u8;
    let off = addr as u16;
    if (bank & 0x7F) >= 0x40 || off < 0x8000 || len == 0 {
        return None;
    }
    Some((((bank & 0x7F) as usize) * 0x8000 + (off as usize & 0x7FFF)) % len)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Flags {
    pub m: bool,
    pub x: bool,
}

#[derive(Default)]
pub struct Analysis {
    /// Befehlsadresse -> alle Flag-Varianten, mit denen sie erreicht wird
    pub insts: BTreeMap<u32, BTreeSet<Flags>>,
    /// Einsprünge von Routinen (Vektoren, JSR/JSL-Ziele, Tabellenziele)
    pub entries: BTreeSet<u32>,
    /// Sprungtabellen-Helfer (Adresse -> Eintragsgröße 2/3)
    pub table_helpers: BTreeMap<u32, u8>,
    /// gefundene Tabellen: Adresse der Tabelle -> Ziele
    pub tables: BTreeMap<u32, Vec<u32>>,
    /// Stellen mit unbekanntem Ziel (indirekt, ohne Tabelle)
    pub unresolved: BTreeSet<u32>,
    /// Stellen, an denen die Flags unbekannt werden (PLP, RTI …) – danach nur aus der Aufzeichnung
    pub flag_breaks: usize,
}

pub struct Rom<'a> {
    pub data: &'a [u8],
}

impl Rom<'_> {
    pub fn byte(&self, a: u32) -> Option<u8> {
        rom_index(a, self.data.len()).map(|i| self.data[i])
    }
    pub fn word(&self, a: u32) -> Option<u16> {
        Some(self.byte(a)? as u16 | (self.byte(next(a, 1))? as u16) << 8)
    }
}

/// Adresse + k innerhalb derselben Bank (PC läuft in der Bank um)
pub fn next(a: u32, k: u16) -> u32 {
    (a & 0xFF0000) | (a as u16).wrapping_add(k) as u32
}

/// Aufgezeichneter Befehl: Adresse, Flags, Anzahl
pub fn parse_trace(trace: &str) -> Vec<(u32, Flags, u64)> {
    let mut v = Vec::new();
    for line in trace.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 5 || f[3] == "1" {
            continue; // Emulationsmodus (nur beim Einschalten) nicht übersetzen
        }
        if let (Ok(a), Ok(n)) = (u32::from_str_radix(f[0], 16), f[4].parse::<u64>()) {
            v.push((a, Flags { m: f[1] == "1", x: f[2] == "1" }, n));
        }
    }
    v
}

pub fn analyze(rom_data: &[u8], trace: &[(u32, Flags, u64)]) -> Analysis {
    let rom = Rom { data: rom_data };
    let mut an = Analysis::default();
    let traced: BTreeSet<u32> = trace.iter().map(|t| t.0).collect();

    // --- Sprungtabellen-Helfer aus der Aufzeichnung: Aufrufe ohne Rückkehr zur Folgeadresse
    let mut calls: BTreeMap<u32, (usize, usize)> = BTreeMap::new(); // Ziel -> (Aufrufe, davon mit Rückkehr)
    for &(a, _, _) in trace {
        let Some(op) = rom.byte(a) else { continue };
        let (t, l) = match op {
            0x20 => (rom.word(next(a, 1)).map(|w| (a & 0xFF0000) | w as u32), 3),
            0x22 => (rom.word(next(a, 1)).zip(rom.byte(next(a, 3))).map(|(w, b)| (b as u32) << 16 | w as u32), 4),
            _ => continue,
        };
        let Some(t) = t else { continue };
        let e = calls.entry(t).or_insert((0, 0));
        e.0 += 1;
        if traced.contains(&next(a, l)) {
            e.1 += 1;
        }
    }
    let code_like = |a: u32| rom.byte(a).is_some();
    for (&t, &(n, back)) in &calls {
        if n >= 3 && back == 0 {
            // Eintragsgröße: welche Deutung trifft mehr aufgezeichnete Ziele?
            let score = |size: u16| -> usize {
                let mut s = 0;
                for &(a, _, _) in trace {
                    let op = rom.byte(a).unwrap_or(0);
                    let l = match op {
                        0x20 => 3,
                        0x22 => 4,
                        _ => continue,
                    };
                    let tt = if op == 0x20 { rom.word(next(a, 1)).map(|w| (a & 0xFF0000) | w as u32) } else { rom.word(next(a, 1)).zip(rom.byte(next(a, 3))).map(|(w, b)| (b as u32) << 16 | w as u32) };
                    if tt != Some(t) {
                        continue;
                    }
                    let base = next(a, l);
                    for k in 0..4u16 {
                        let e = if size == 2 { rom.word(next(base, k * 2)).map(|w| (base & 0xFF0000) | w as u32) } else { rom.word(next(base, k * 3)).zip(rom.byte(next(base, k * 3 + 2))).map(|(w, b)| (b as u32) << 16 | w as u32) };
                        if e.is_some_and(|e| traced.contains(&e)) {
                            s += 1;
                        }
                    }
                }
                s
            };
            let (s2, s3) = (score(2), score(3));
            if s2.max(s3) > 0 {
                an.table_helpers.insert(t, if s3 > s2 { 3 } else { 2 });
            }
        }
    }

    // --- rekursive Analyse
    let mut queue: VecDeque<(u32, Flags)> = VecDeque::new();
    let push = |q: &mut VecDeque<(u32, Flags)>, a: u32, f: Flags| {
        if code_like(a) {
            q.push_back((a, f));
        }
    };
    for v in [0xFFEAu32, 0xFFEE, 0xFFFC, 0xFFE6, 0xFFE4] {
        if let Some(w) = rom.word(v) {
            an.entries.insert(w as u32);
            push(&mut queue, w as u32, Flags { m: true, x: true });
        }
    }
    for &(a, f, _) in trace {
        push(&mut queue, a, f);
    }
    let mut seen: BTreeSet<(u32, Flags)> = BTreeSet::new();
    while let Some((a, f)) = queue.pop_front() {
        if !seen.insert((a, f)) {
            continue;
        }
        let Some(op) = rom.byte(a) else { continue };
        let l = inst_len(op, f.m, f.x);
        if (1..l).any(|k| rom.byte(next(a, k)).is_none()) {
            continue;
        }
        an.insts.entry(a).or_default().insert(f);
        let b1 = rom.byte(next(a, 1)).unwrap_or(0);
        let w1 = rom.word(next(a, 1)).unwrap_or(0);
        let long1 = (rom.byte(next(a, 3)).unwrap_or(0) as u32) << 16 | w1 as u32;
        let local = |w: u16| (a & 0xFF0000) | w as u32;
        let mut nf = f;
        match op {
            0xC2 => {
                nf.m &= b1 & 0x20 == 0;
                nf.x &= b1 & 0x10 == 0;
            }
            0xE2 => {
                nf.m |= b1 & 0x20 != 0;
                nf.x |= b1 & 0x10 != 0;
            }
            0xFB => {
                // XCE: beim Start CLC;XCE -> nativ, M/X bleiben gesetzt
                nf = Flags { m: true, x: true };
            }
            _ => {}
        }
        // Verzweigungen
        match op {
            0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 | 0x80 => {
                push(&mut queue, local((a as u16).wrapping_add(2).wrapping_add(b1 as i8 as i16 as u16)), nf);
            }
            0x82 => push(&mut queue, local((a as u16).wrapping_add(3).wrapping_add(w1)), nf),
            0x4C => push(&mut queue, local(w1), nf),
            0x5C => push(&mut queue, long1, nf),
            0x20 | 0x22 => {
                let t = if op == 0x20 { local(w1) } else { long1 };
                an.entries.insert(t);
                push(&mut queue, t, nf);
                if let Some(&size) = an.table_helpers.get(&t) {
                    // Tabelle hinter dem Aufruf
                    let base = next(a, l);
                    let mut out = Vec::new();
                    for k in 0..256u16 {
                        let ea = next(base, k * size as u16);
                        if traced.contains(&ea) && k > 0 {
                            break; // hier beginnt Code
                        }
                        let e = if size == 2 { rom.word(ea).map(|w| (base & 0xFF0000) | w as u32) } else { rom.word(ea).zip(rom.byte(next(ea, 2))).map(|(w, b)| (b as u32) << 16 | w as u32) };
                        match e {
                            Some(e) if (e as u16) >= 0x8000 && rom.byte(e).is_some() && (size == 2 || traced.contains(&e) || k == 0 || (e >> 16) == (base >> 16)) => out.push(e),
                            _ => break,
                        }
                    }
                    for &e in &out {
                        an.entries.insert(e);
                        push(&mut queue, e, nf);
                    }
                    an.tables.insert(base, out);
                    continue; // kein Weiterlaufen hinter den Aufruf
                }
            }
            0x7C | 0xFC => {
                // indirekt über Tabelle in derselben Bank
                let mut out = Vec::new();
                for k in 0..128u16 {
                    let Some(w) = rom.word(local(w1.wrapping_add(k * 2))) else { break };
                    let e = local(w);
                    if w < 0x8000 || !(traced.contains(&e) || k < 2) {
                        break;
                    }
                    out.push(e);
                }
                if out.is_empty() {
                    an.unresolved.insert(a);
                }
                for &e in &out {
                    an.entries.insert(e);
                    push(&mut queue, e, nf);
                }
                an.tables.insert(local(w1), out);
            }
            0x6C | 0xDC => {
                an.unresolved.insert(a);
            }
            0x28 | 0x40 => an.flag_breaks += 1,
            _ => {}
        }
        if !no_fallthrough(op) && op != 0x28 {
            push(&mut queue, next(a, l), nf);
        }
    }
    an
}

impl Analysis {
    pub fn report(&self, trace: &[(u32, Flags, u64)]) -> String {
        let total: u64 = trace.iter().map(|t| t.2).sum();
        let covered: u64 = trace.iter().filter(|t| self.insts.get(&t.0).is_some_and(|s| s.contains(&t.1))).map(|t| t.2).sum();
        let multi = self.insts.values().filter(|s| s.len() > 1).count();
        format!(
            "Befehle: {} (davon {} mit mehreren M/X-Varianten), Routinen: {}, Tabellen-Helfer: {:?}, Tabellen: {} ({} Ziele), ungeklärte indirekte Sprünge: {}, Flag-Brüche: {}, Abdeckung der Aufzeichnung: {:.4} %",
            self.insts.len(),
            multi,
            self.entries.len(),
            self.table_helpers.iter().map(|(a, s)| format!("{a:06X}/{s}")).collect::<Vec<_>>(),
            self.tables.len(),
            self.tables.values().map(|v| v.len()).sum::<usize>(),
            self.unresolved.len(),
            self.flag_breaks,
            covered as f64 * 100.0 / total.max(1) as f64
        )
    }
}
