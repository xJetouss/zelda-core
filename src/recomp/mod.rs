//! Recompiled-Modus: Der 65C816-Code der ROM wurde beim Bauen nach Rust übersetzt (siehe `gen.rs`) und
//! läuft hier ohne Befehls-Dekodierung. Grafik, Ton, DMA und Zeitverhalten bleiben exakt wie bei der
//! Emulation: Jeder übersetzte Befehl nutzt dieselben Bus-Zugriffe und denselben Takt. Spielstände sind
//! deshalb in beiden Modi dieselben und lassen sich hin und her laden.
//!
//! Code, der nicht übersetzt ist (nicht aufgezeichnet, im RAM, durch Patches verändert), führt
//! automatisch der Interpreter aus.

#[allow(dead_code)] // Generator läuft im Build-Schritt; hier wird nur die Prüfsumme gebraucht
pub mod gen;
#[allow(dead_code)]
pub mod analyze;
#[cfg(test)]
mod trace;
#[cfg(test)]
pub use trace::app_rom as app_rom_for_tests;

use crate::snes::bus::Bus;
use crate::snes::cpu::Cpu;
use crate::snes::Snes;

pub type BlockFn = fn(&mut Cpu, &mut Bus);

/// Ein übersetzter Befehl: Vorbedingungen prüfen, Opcode holen (Takt wie im Interpreter), ausführen,
/// Ereignisse (Zeilen, HDMA, IRQ) nachziehen. Landet die CPU nicht dort, wo der Recompiler es erwartet hat,
/// geht es im Verteiler weiter.
#[allow(unused_macros)]
macro_rules! i {
    ($c:ident, $b:ident, $pb:expr, $pc:expr, $op:expr, $nx:expr, first) => {
        $b.fetch_known((($pb as u32) << 16) | $pc as u32, $op);
        $c.pc = ($pc as u16).wrapping_add(1);
        op::<$op>($c, $b);
        $b.process_events();
        if $b.frame_done || $c.pc != $nx || $c.pb != $pb {
            return;
        }
    };
    ($c:ident, $b:ident, $pb:expr, $pc:expr, $op:expr, $nx:expr, next) => {
        if !$c.running() || $b.nmi_pending || $b.timeup {
            return;
        }
        i!($c, $b, $pb, $pc, $op, $nx, first);
    };
}

/// Ein Opcode als eigene Funktion (der Interpreter-Code mit festem Opcode, ohne Verteiler)
#[inline]
#[allow(dead_code)]
fn op<const OP: u8>(c: &mut Cpu, b: &mut Bus) {
    c.exec_op(b, OP);
}

#[allow(clippy::all, unused_imports)]
mod generated {
    use super::{op, BlockFn};
    use crate::snes::bus::Bus;
    use crate::snes::cpu::Cpu;
    include!(concat!(env!("OUT_DIR"), "/recomp_gen.rs"));
}

/// Spieldaten aus dem Build (Recompiled-Modus braucht keine ROM-Datei)
mod data {
    include!(concat!(env!("OUT_DIR"), "/recomp_data.rs"));
}

/// Sind die Spieldaten in die .exe eingebaut?
pub fn has_embedded() -> bool {
    !data::ORIG_ROM.is_empty() && Table::available()
}

/// Name (Dateistamm) der eingebauten ROM – Spielstände liegen unter demselben Namen wie bei der Emulation
pub fn embedded_name() -> &'static str {
    data::ROM_NAME
}

/// Eingebaute Spieldaten, wenn `path` dieses Spiel meint (gleicher Dateistamm)
pub fn embedded_rom(path: &std::path::Path) -> Option<Vec<u8>> {
    (has_embedded() && path.file_stem().is_some_and(|s| s.to_string_lossy() == data::ROM_NAME)).then(|| data::ORIG_ROM.to_vec())
}

/// Recompiled-Modus für dieses Spiel ein- oder ausschalten; nach einem ROM-Wechsel (Patches, Randomizer)
/// werden die Blöcke neu gegen die ROM geprüft
pub fn sync(s: &mut Snes, want: bool) {
    let want = want && Table::available();
    if !want {
        s.recomp = None;
    } else if s.recomp.as_ref().is_none_or(|t| !std::sync::Arc::ptr_eq(&t.rom, &s.bus.rom)) {
        s.recomp = Some(std::sync::Arc::new(Table::new(&s.bus.rom)));
    }
}

/// Übersetzte Blöcke, die zur geladenen ROM passen
pub struct Table {
    /// ROM, gegen die geprüft wurde (gehalten, damit ein Wechsel sicher erkannt wird)
    rom: std::sync::Arc<Vec<u8>>,
    /// je Bank: Block-Nummer + 1 für Offset 0x8000..0xFFFF (0 = keiner)
    banks: Vec<Option<Box<[u32]>>>,
    fns: Vec<BlockFn>,
    pub active: usize,
    pub total: usize,
}

impl Table {
    /// Ist überhaupt übersetzter Code eingebaut?
    pub fn available() -> bool {
        !generated::BLOCKS.is_empty()
    }

    /// Prüft jeden Block gegen die ROM und baut die Sprungtabelle
    pub fn new(rom_arc: &std::sync::Arc<Vec<u8>>) -> Table {
        let rom: &[u8] = rom_arc;
        let mut t = Table { rom: rom_arc.clone(), banks: (0..256).map(|_| None).collect(), fns: Vec::new(), active: 0, total: generated::BLOCKS.len() };
        let idx = |bank: u8, off: u16| (((bank & 0x7F) as usize) * 0x8000 + (off as usize & 0x7FFF)) % rom.len().max(1);
        for &(addr, len, h, f) in generated::BLOCKS {
            let (bank, off) = ((addr >> 16) as u8, addr as u16);
            if rom.is_empty() || (off as u32 + len as u32) > 0x10000 {
                continue;
            }
            let bytes: Vec<u8> = (0..len).map(|k| rom[idx(bank, off + k)]).collect();
            if gen::hash(&bytes) != h {
                continue;
            }
            t.fns.push(f);
            let slot = t.banks[bank as usize].get_or_insert_with(|| vec![0u32; 0x8000].into_boxed_slice());
            slot[(off - 0x8000) as usize] = t.fns.len() as u32;
            t.active += 1;
        }
        t
    }

    #[inline(always)]
    fn get(&self, pb: u8, pc: u16) -> Option<BlockFn> {
        if pc < 0x8000 {
            return None;
        }
        let k = self.banks[pb as usize].as_ref()?[(pc - 0x8000) as usize];
        (k != 0).then(|| self.fns[k as usize - 1])
    }
}

/// Hooks für Mods: Adressen (Bitfeld je Bank) und die Funktion, die sie bedient
pub struct HookSet {
    bits: Vec<Option<Box<[u64]>>>,
    pub sink: Box<dyn Fn(u32, &mut Cpu, &mut Bus) + Send + Sync>,
    /// Kennung des Adress-Satzes (zum Erkennen von Änderungen)
    pub version: u64,
}

impl HookSet {
    pub fn new(addrs: &[u32], version: u64, sink: Box<dyn Fn(u32, &mut Cpu, &mut Bus) + Send + Sync>) -> HookSet {
        let mut bits: Vec<Option<Box<[u64]>>> = (0..256).map(|_| None).collect();
        for &a in addrs {
            let w = bits[(a >> 16) as usize & 0xFF].get_or_insert_with(|| vec![0u64; 1024].into_boxed_slice());
            w[(a as usize & 0xFFFF) >> 6] |= 1 << (a & 63);
        }
        HookSet { bits, sink, version }
    }
    #[inline(always)]
    fn has(&self, pb: u8, pc: u16) -> bool {
        self.bits[pb as usize].as_ref().is_some_and(|w| w[pc as usize >> 6] & (1 << (pc & 63)) != 0)
    }
}

/// Platz für die Hooks im `Snes`; eine Kopie (Run-Ahead, Hintergrund-Aufnahmen) hat keine
#[derive(Default)]
pub struct HookSlot(pub Option<std::sync::Arc<HookSet>>);

impl Clone for HookSlot {
    fn clone(&self) -> Self {
        HookSlot(None)
    }
}

/// Ein Bild ausführen (Gegenstück zu `Snes::run_frame`)
pub fn run_frame(s: &mut Snes, t: &Table, hooks: Option<&HookSet>) {
    let (c, b) = (&mut s.cpu, &mut s.bus);
    b.frame_done = false;
    let mut last_hook = (u32::MAX, 0u64);
    while !b.frame_done {
        // Hook: nur wenn der Befehl an dieser Stelle jetzt wirklich ausgeführt wird (keine Unterbrechung dazwischen)
        if let Some(h) = hooks {
            if c.running() && !b.nmi_pending && !(b.timeup && !c.irq_masked()) && h.has(c.pb, c.pc) {
                let a = ((c.pb as u32) << 16) | c.pc as u32;
                if last_hook != (a, b.clock) {
                    last_hook = (a, b.clock);
                    (h.sink)(a, c, b);
                    continue;
                }
            }
        }
        if c.running() && !b.nmi_pending && !b.timeup {
            if let Some(f) = t.get(c.pb, c.pc) {
                f(c, b);
                continue;
            }
        }
        c.step(b);
        b.process_events();
    }
    b.apu.sync(b.clock);
}
