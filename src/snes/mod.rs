//! Minimaler SNES-Kern: 65C816-CPU, PPU, SPC700 + DSP, DMA/HDMA.
//! Nur LoROM ohne Zusatzchips – genau das, was A Link to the Past braucht.

pub mod apu;
pub mod bus;
pub mod cpu;
pub mod dsp;
pub mod ppu;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
pub struct Snes {
    pub cpu: cpu::Cpu,
    pub bus: bus::Bus,
    /// Recompiled-Modus: übersetzter ROM-Code (gehört nicht zum Spielstand – Savestates sind in beiden
    /// Modi dieselben)
    #[serde(skip)]
    pub recomp: Option<Arc<crate::recomp::Table>>,
    /// Mod-Hooks (nur im Recompiled-Modus; Kopien wie Run-Ahead lösen keine Hooks aus)
    #[serde(skip)]
    pub hooks: crate::recomp::HookSlot,
    /// Zelda-3-Modus (Spiellogik aus vendor/zelda3, Grafik/RAM über diesen `Snes`)
    #[serde(skip)]
    pub z3: crate::z3::Slot,
    /// Grafik-/Farb-Änderungen der Mods, vor dem Zeichnen jedes Bildes angewendet (nur im RC-Modus)
    #[serde(skip)]
    pub mod_patch: Option<Arc<crate::mods::Patch>>,
}

impl Snes {
    pub fn new(rom: Arc<Vec<u8>>, sram_size: usize, pal: bool) -> Self {
        let mut s = Snes {
            cpu: cpu::Cpu::default(),
            bus: bus::Bus::new(rom, sram_size, pal),
            recomp: None,
            hooks: Default::default(),
            z3: Default::default(),
            mod_patch: None,
        };
        s.cpu.reset(&mut s.bus);
        s
    }

    /// Reset (Spielstand im SRAM bleibt erhalten).
    pub fn reset(&mut self) {
        let rom = self.bus.rom.clone();
        let sram = self.bus.sram.clone();
        let cfg = self.bus.apu.dsp.cfg;
        let old = std::mem::take(&mut self.bus.ppu);
        let recomp = self.recomp.take();
        let hooks = std::mem::take(&mut self.hooks);
        let z3 = std::mem::take(&mut self.z3);
        *self = Snes::new(rom, sram.len(), self.bus.pal);
        self.recomp = recomp;
        self.hooks = hooks;
        self.z3 = z3;
        self.z3.reload();
        self.bus.ppu.copy_cfg(&old);
        self.bus.apu.dsp.cfg = cfg;
        self.bus.sram = sram;
    }

    /// Führt genau ein Bild (bis zum Beginn des VBlanks) aus.
    pub fn run_frame(&mut self) {
        if self.z3.active() {
            crate::z3::run_frame(self);
            return;
        }
        if let Some(t) = self.recomp.clone() {
            if let Some(p) = self.mod_patch.clone() {
                p.apply(&mut self.bus.ppu);
            }
            let h = self.hooks.0.clone();
            crate::recomp::run_frame(self, &t, h.as_deref());
            return;
        }
        self.bus.frame_done = false;
        while !self.bus.frame_done {
            self.cpu.step(&mut self.bus);
            self.bus.process_events();
        }
        self.bus.apu.sync(self.bus.clock);
    }

    pub fn framebuffer(&self) -> &[u16] {
        &self.bus.ppu.fb
    }

    pub fn take_audio(&mut self, out: &mut Vec<i16>) {
        out.extend_from_slice(&self.bus.apu.dsp.out);
        self.bus.apu.dsp.out.clear();
    }

    /// Läuft der RC-Modus (eigener Recompiler oder Zelda 3)?
    pub fn rc_active(&self) -> bool {
        self.recomp.is_some() || self.z3.active()
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut data = b"HEMU2".to_vec();
        // Zelda 3: Spielstand so speichern, wie ihn die Emulation erwartet (Grafik-Belegung, Ton)
        let raw = match crate::z3::emu_copy(self) {
            Some(c) => bincode::serialize(&c).expect("serialize"),
            None => bincode::serialize(self).expect("serialize"),
        };
        data.extend_from_slice(&lz4_flex::compress_prepend_size(&raw));
        data
    }

    pub fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() < 5 || &data[..5] != b"HEMU2" {
            return Err("Kein gültiger Savestate".into());
        }
        let raw = lz4_flex::decompress_size_prepended(&data[5..]).map_err(|e| e.to_string())?;
        let mut s: Snes = bincode::deserialize(&raw).map_err(|e| e.to_string())?;
        let ok = s.bus.wram.len() == 0x20000
            && s.bus.ppu.vram.len() == 0x8000
            && s.bus.ppu.cgram.len() == 256
            && s.bus.ppu.oam.len() == 544
            && s.bus.apu.ram.len() == 0x10000
            && s.bus.apu.dsp.regs.len() == 128
            && s.bus.sram.len() == self.bus.sram.len();
        if !ok {
            return Err("Savestate passt nicht zu diesem Spiel".into());
        }
        s.bus.rom = self.bus.rom.clone();
        s.bus.ppu.copy_cfg(&self.bus.ppu);
        s.bus.apu.dsp.cfg = self.bus.apu.dsp.cfg;
        s.recomp = self.recomp.take();
        s.hooks = std::mem::take(&mut self.hooks);
        s.z3 = std::mem::take(&mut self.z3);
        s.z3.reload();
        *self = s;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prüft den IPL-Handshake des SPC700 (Grundlage für jede Musik/SFX-Übertragung).
    #[test]
    fn spc_ipl_handshake() {
        let rom = Arc::new(vec![0u8; 0x8000]);
        let mut s = Snes::new(rom, 0x2000, false);
        s.bus.clock += 2_000_000;
        s.bus.apu.sync(s.bus.clock);
        assert_eq!(s.bus.apu.port_out[0], 0xAA);
        assert_eq!(s.bus.apu.port_out[1], 0xBB);
        // Upload starten: Zieladresse $0200, Kommando 1, Kick $CC
        s.bus.apu.port_in[2] = 0x00;
        s.bus.apu.port_in[3] = 0x02;
        s.bus.apu.port_in[1] = 0x01;
        s.bus.apu.port_in[0] = 0xCC;
        s.bus.clock += 200_000;
        s.bus.apu.sync(s.bus.clock);
        assert_eq!(s.bus.apu.port_out[0], 0xCC);
        // ein Byte übertragen
        s.bus.apu.port_in[1] = 0x5A;
        s.bus.apu.port_in[0] = 0x00;
        s.bus.clock += 200_000;
        s.bus.apu.sync(s.bus.clock);
        assert_eq!(s.bus.apu.port_out[0], 0x00);
        assert_eq!(s.bus.apu.ram[0x200], 0x5A);
    }

    /// Kleines CPU-Programm: Schleife, 16-Bit-Arithmetik, Dezimalmodus, Stack.
    #[test]
    fn cpu_basic_program() {
        let mut rom = vec![0xEAu8; 0x8000];
        let prog: &[u8] = &[
            0x18, 0xFB, // CLC; XCE -> native
            0xC2, 0x30, // REP #$30
            0xA9, 0x34, 0x12, // LDA #$1234
            0x18, 0x69, 0x11, 0x11, // CLC; ADC #$1111 -> 2345
            0x8D, 0x00, 0x00, // STA $0000
            0xF8, 0xE2, 0x20, // SED; SEP #$20
            0xA9, 0x19, 0x18, 0x69, 0x03, // LDA #$19; CLC; ADC #$03 -> $22 (BCD)
            0x8D, 0x02, 0x00, // STA $0002
            0xD8, // CLD
            0xA2, 0x05, 0x00, // LDX #$0005
            0xCA, 0xD0, 0xFD, // DEX; BNE -3
            0x8E, 0x03, 0x00, // STX $0003
            0x48, 0xA9, 0x00, 0x68, // PHA; LDA #0; PLA
            0x8D, 0x05, 0x00, // STA $0005
            0x80, 0xFE, // BRA *
        ];
        rom[..prog.len()].copy_from_slice(prog);
        rom[0x7FFC] = 0x00;
        rom[0x7FFD] = 0x80;
        let mut s = Snes::new(Arc::new(rom), 0x2000, false);
        for _ in 0..2000 {
            s.cpu.step(&mut s.bus);
        }
        assert_eq!(s.bus.wram[0], 0x45);
        assert_eq!(s.bus.wram[1], 0x23);
        assert_eq!(s.bus.wram[2], 0x22);
        assert_eq!(s.bus.wram[3], 0x00);
        assert_eq!(s.bus.wram[5], 0x22);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    #[test]
    #[ignore]
    fn speed() {
        // Endlosschleife mit NMI-Handler, PPU an, Mode 1 – misst rohe Emulationsgeschwindigkeit.
        let mut rom = vec![0xEAu8; 0x8000];
        let prog: &[u8] = &[0x18, 0xFB, 0xA9, 0x0F, 0x8D, 0x00, 0x21, 0xA9, 0x01, 0x8D, 0x05, 0x21, 0xA9, 0x1F, 0x8D, 0x2C, 0x21, 0xA9, 0x81, 0x8D, 0x00, 0x42, 0xCB, 0x80, 0xFD];
        rom[..prog.len()].copy_from_slice(prog);
        rom[0x100] = 0x40; // RTI
        rom[0x7FEA] = 0x00; rom[0x7FEB] = 0x81;
        rom[0x7FFC] = 0x00; rom[0x7FFD] = 0x80;
        let mut s = Snes::new(Arc::new(rom), 0x2000, false);
        let t = std::time::Instant::now();
        for _ in 0..600 { s.run_frame(); }
        let e = t.elapsed().as_secs_f64();
        let mut a = Vec::new(); s.take_audio(&mut a);
        println!("600 Frames in {:.3}s = {:.0} FPS, Audio-Samples: {}", e, 600.0 / e, a.len() / 2);
    }
}

#[cfg(test)]
mod pal_tests {
    use super::*;
    #[test]
    fn pal_frame_is_312_lines() {
        let mut rom = vec![0xEAu8; 0x8000];
        rom[0] = 0x80; rom[1] = 0xFE; // BRA *
        rom[0x7FFC] = 0x00; rom[0x7FFD] = 0x80;
        for (pal, lines) in [(false, 262u64), (true, 312)] {
            let mut s = Snes::new(Arc::new(rom.clone()), 0x2000, pal);
            s.run_frame();
            let c0 = s.bus.clock;
            s.run_frame();
            let d = s.bus.clock - c0;
            assert!((d as i64 - (lines * 1364) as i64).abs() < 64, "pal={pal} d={d}");
        }
    }
}
