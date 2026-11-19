//! Speicher-Bus (LoROM), CPU-I/O-Register, DMA/HDMA, Zeilen-Timing und Interrupts.

use super::{apu::Apu, ppu::Ppu};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const LINE_CYCLES: u64 = 1364;
const HBLANK_START: u64 = 1096;
const VBLANK_LINE: u16 = 225;

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct Dma {
    dmap: u8,
    bbad: u8,
    a1t: u16,
    a1b: u8,
    das: u16,
    dasb: u8,
    a2a: u16,
    nltr: u8,
    unused: u8,
    term: bool,
    do_transfer: bool,
}

const PATTERN: [[u8; 4]; 8] = [
    [0, 0, 0, 0],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
    [0, 1, 2, 3],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
];
const HDMA_LEN: [usize; 8] = [1, 2, 2, 4, 4, 4, 2, 4];

#[derive(Clone, Serialize, Deserialize)]
pub struct Bus {
    #[serde(skip)]
    pub rom: Arc<Vec<u8>>,
    pub sram: Vec<u8>,
    pub wram: Vec<u8>,
    pub ppu: Ppu,
    pub apu: Apu,
    dma: [Dma; 8],
    pub clock: u64,
    line_start: u64,
    pub line: u16,
    hblank_done: bool,
    nmitimen: u8,
    nmi_flag: bool,
    pub nmi_pending: bool,
    pub timeup: bool,
    irq_fired: bool,
    htime: u16,
    vtime: u16,
    wrmpya: u8,
    wrdiv: u16,
    rddiv: u16,
    rdmpy: u16,
    memsel: u8,
    hdmaen: u8,
    wmaddr: u32,
    pub joy: [u16; 2],
    joy_auto: [u16; 2],
    joy_shift: [u16; 2],
    pub frame_done: bool,
    open_bus: u8,
    #[serde(skip)]
    pub sram_dirty: bool,
    pub pal: bool,
}

impl Bus {
    pub fn new(rom: Arc<Vec<u8>>, sram_size: usize, pal: bool) -> Self {
        let mut apu = Apu::new();
        apu.master_hz = if pal { 21_281_370 } else { 21_477_272 };
        let mut ppu = Ppu::new();
        ppu.pal = pal;
        Bus {
            rom,
            sram: vec![0; sram_size],
            wram: vec![0; 0x20000],
            ppu,
            apu,
            dma: [Dma::default(); 8],
            clock: 0,
            line_start: 0,
            line: 0,
            hblank_done: false,
            nmitimen: 0,
            nmi_flag: false,
            nmi_pending: false,
            timeup: false,
            irq_fired: false,
            htime: 0x1FF,
            vtime: 0x1FF,
            wrmpya: 0xFF,
            wrdiv: 0xFFFF,
            rddiv: 0,
            rdmpy: 0,
            memsel: 0,
            hdmaen: 0,
            wmaddr: 0,
            joy: [0; 2],
            joy_auto: [0; 2],
            joy_shift: [0; 2],
            frame_done: false,
            open_bus: 0,
            sram_dirty: false,
            pal,
        }
    }

    #[cfg(test)]
    pub fn debug_io(&self) -> String {
        format!("nmitimen={:02X} joy={:04X} auto={:04X} line={}", self.nmitimen, self.joy[0], self.joy_auto[0], self.line)
    }

    #[inline]
    pub fn idle(&mut self) {
        self.clock += 6;
    }

    #[inline]
    fn speed(&self, bank: u8, off: u16) -> u64 {
        let fast = bank & 0x80 != 0 && self.memsel & 1 != 0;
        if bank & 0x40 != 0 {
            if fast {
                6
            } else {
                8
            }
        } else {
            match off {
                0x0000..=0x1FFF => 8,
                0x2000..=0x3FFF => 6,
                0x4000..=0x41FF => 12,
                0x4200..=0x5FFF => 6,
                0x6000..=0x7FFF => 8,
                _ => {
                    if fast {
                        6
                    } else {
                        8
                    }
                }
            }
        }
    }

    #[inline]
    pub fn read(&mut self, addr: u32) -> u8 {
        let bank = (addr >> 16) as u8;
        let off = addr as u16;
        self.clock += self.speed(bank, off);
        let v = self.read_raw(bank, off);
        self.open_bus = v;
        v
    }

    #[inline]
    pub fn write(&mut self, addr: u32, v: u8) {
        let bank = (addr >> 16) as u8;
        let off = addr as u16;
        self.clock += self.speed(bank, off);
        self.open_bus = v;
        self.write_raw(bank, off, v);
    }

    #[inline]
    fn rom_index(&self, bank: u8, off: u16) -> usize {
        (((bank & 0x7F) as usize) * 0x8000 + (off as usize & 0x7FFF)) % self.rom.len()
    }

    /// Lesen ohne Timing (für DMA und Cheats).
    pub fn read_raw(&mut self, bank: u8, off: u16) -> u8 {
        if bank == 0x7E || bank == 0x7F {
            return self.wram[((bank as usize & 1) << 16) | off as usize];
        }
        let b = bank & 0x7F;
        if b < 0x40 {
            match off {
                0x0000..=0x1FFF => self.wram[off as usize],
                0x2100..=0x21FF => self.read_b(off as u8),
                0x4016 => {
                    let v = (self.joy_shift[0] >> 15) as u8 & 1;
                    self.joy_shift[0] = (self.joy_shift[0] << 1) | 1;
                    (self.open_bus & 0xFC) | v
                }
                0x4017 => {
                    let v = (self.joy_shift[1] >> 15) as u8 & 1;
                    self.joy_shift[1] = (self.joy_shift[1] << 1) | 1;
                    (self.open_bus & 0xE0) | 0x1C | v
                }
                0x4200..=0x421F => self.read_cpu_reg(off),
                0x4300..=0x437F => self.read_dma(off),
                0x8000..=0xFFFF => self.rom[self.rom_index(bank, off)],
                _ => self.open_bus,
            }
        } else if b >= 0x70 && off < 0x8000 {
            if self.sram.is_empty() {
                self.open_bus
            } else {
                let i = ((b - 0x70) as usize * 0x8000 + off as usize) % self.sram.len();
                self.sram[i]
            }
        } else {
            self.rom[self.rom_index(bank, off)]
        }
    }

    pub fn write_raw(&mut self, bank: u8, off: u16, v: u8) {
        if bank == 0x7E || bank == 0x7F {
            self.wram[((bank as usize & 1) << 16) | off as usize] = v;
            return;
        }
        let b = bank & 0x7F;
        if b < 0x40 {
            match off {
                0x0000..=0x1FFF => self.wram[off as usize] = v,
                0x2100..=0x21FF => self.write_b(off as u8, v),
                0x4016 => {
                    if v & 1 != 0 {
                        self.joy_shift = self.joy;
                    }
                }
                0x4200..=0x421F => self.write_cpu_reg(off, v),
                0x4300..=0x437F => self.write_dma(off, v),
                _ => {}
            }
        } else if b >= 0x70 && off < 0x8000 && !self.sram.is_empty() {
            let i = ((b - 0x70) as usize * 0x8000 + off as usize) % self.sram.len();
            if self.sram[i] != v {
                self.sram[i] = v;
                self.sram_dirty = true;
            }
        }
    }

    fn read_b(&mut self, a: u8) -> u8 {
        match a {
            0x34..=0x3F => {
                let h = ((self.clock - self.line_start) / 4) as u16;
                let line = self.line;
                let ob = self.open_bus;
                self.ppu.read(a, h, line, ob)
            }
            0x40..=0x7F => {
                self.apu.sync(self.clock);
                self.apu.port_out[(a & 3) as usize]
            }
            0x80 => {
                let v = self.wram[self.wmaddr as usize];
                self.wmaddr = (self.wmaddr + 1) & 0x1FFFF;
                v
            }
            _ => self.open_bus,
        }
    }

    fn write_b(&mut self, a: u8, v: u8) {
        match a {
            0x00..=0x33 => self.ppu.write(a, v),
            0x40..=0x7F => {
                self.apu.sync(self.clock);
                self.apu.port_in[(a & 3) as usize] = v;
            }
            0x80 => {
                self.wram[self.wmaddr as usize] = v;
                self.wmaddr = (self.wmaddr + 1) & 0x1FFFF;
            }
            0x81 => self.wmaddr = (self.wmaddr & 0x1FF00) | v as u32,
            0x82 => self.wmaddr = (self.wmaddr & 0x100FF) | (v as u32) << 8,
            0x83 => self.wmaddr = (self.wmaddr & 0x0FFFF) | ((v as u32 & 1) << 16),
            _ => {}
        }
    }

    fn read_cpu_reg(&mut self, off: u16) -> u8 {
        match off {
            0x4210 => {
                let v = ((self.nmi_flag as u8) << 7) | 0x02 | (self.open_bus & 0x70);
                self.nmi_flag = false;
                v
            }
            0x4211 => {
                let v = ((self.timeup as u8) << 7) | (self.open_bus & 0x7F);
                self.timeup = false;
                v
            }
            0x4212 => {
                let h = self.clock - self.line_start;
                let mut v = self.open_bus & 0x3E;
                if self.line >= VBLANK_LINE {
                    v |= 0x80;
                }
                if h >= HBLANK_START || h < 4 {
                    v |= 0x40;
                }
                if self.nmitimen & 1 != 0 && self.line >= VBLANK_LINE && self.line <= VBLANK_LINE + 2 {
                    v |= 0x01;
                }
                v
            }
            0x4213 => 0,
            0x4214 => self.rddiv as u8,
            0x4215 => (self.rddiv >> 8) as u8,
            0x4216 => self.rdmpy as u8,
            0x4217 => (self.rdmpy >> 8) as u8,
            0x4218 => self.joy_auto[0] as u8,
            0x4219 => (self.joy_auto[0] >> 8) as u8,
            0x421A => self.joy_auto[1] as u8,
            0x421B => (self.joy_auto[1] >> 8) as u8,
            0x421C..=0x421F => 0,
            _ => self.open_bus,
        }
    }

    fn write_cpu_reg(&mut self, off: u16, v: u8) {
        match off {
            0x4200 => {
                let old = self.nmitimen;
                self.nmitimen = v;
                if v & 0x30 == 0 {
                    self.timeup = false;
                }
                if old & 0x80 == 0 && v & 0x80 != 0 && self.nmi_flag {
                    self.nmi_pending = true;
                }
            }
            0x4202 => self.wrmpya = v,
            0x4203 => self.rdmpy = self.wrmpya as u16 * v as u16,
            0x4204 => self.wrdiv = (self.wrdiv & 0xFF00) | v as u16,
            0x4205 => self.wrdiv = (self.wrdiv & 0x00FF) | (v as u16) << 8,
            0x4206 => {
                if v == 0 {
                    self.rddiv = 0xFFFF;
                    self.rdmpy = self.wrdiv;
                } else {
                    self.rddiv = self.wrdiv / v as u16;
                    self.rdmpy = self.wrdiv % v as u16;
                }
            }
            0x4207 => self.htime = (self.htime & 0x100) | v as u16,
            0x4208 => self.htime = (self.htime & 0xFF) | ((v as u16 & 1) << 8),
            0x4209 => self.vtime = (self.vtime & 0x100) | v as u16,
            0x420A => self.vtime = (self.vtime & 0xFF) | ((v as u16 & 1) << 8),
            0x420B => self.do_dma(v),
            0x420C => self.hdmaen = v,
            0x420D => self.memsel = v,
            _ => {}
        }
    }

    fn read_dma(&self, off: u16) -> u8 {
        let c = &self.dma[((off >> 4) & 7) as usize];
        match off & 0xF {
            0x0 => c.dmap,
            0x1 => c.bbad,
            0x2 => c.a1t as u8,
            0x3 => (c.a1t >> 8) as u8,
            0x4 => c.a1b,
            0x5 => c.das as u8,
            0x6 => (c.das >> 8) as u8,
            0x7 => c.dasb,
            0x8 => c.a2a as u8,
            0x9 => (c.a2a >> 8) as u8,
            0xA => c.nltr,
            0xB | 0xF => c.unused,
            _ => self.open_bus,
        }
    }

    fn write_dma(&mut self, off: u16, v: u8) {
        let c = &mut self.dma[((off >> 4) & 7) as usize];
        match off & 0xF {
            0x0 => c.dmap = v,
            0x1 => c.bbad = v,
            0x2 => c.a1t = (c.a1t & 0xFF00) | v as u16,
            0x3 => c.a1t = (c.a1t & 0x00FF) | (v as u16) << 8,
            0x4 => c.a1b = v,
            0x5 => c.das = (c.das & 0xFF00) | v as u16,
            0x6 => c.das = (c.das & 0x00FF) | (v as u16) << 8,
            0x7 => c.dasb = v,
            0x8 => c.a2a = (c.a2a & 0xFF00) | v as u16,
            0x9 => c.a2a = (c.a2a & 0x00FF) | (v as u16) << 8,
            0xA => c.nltr = v,
            0xB | 0xF => c.unused = v,
            _ => {}
        }
    }

    fn do_dma(&mut self, mask: u8) {
        if mask == 0 {
            return;
        }
        self.clock += 8;
        for ch in 0..8 {
            if mask & (1 << ch) == 0 {
                continue;
            }
            self.clock += 8;
            let mut c = self.dma[ch];
            let mut count: u32 = if c.das == 0 { 0x10000 } else { c.das as u32 };
            let pat = PATTERN[(c.dmap & 7) as usize];
            let step: i32 = match (c.dmap >> 3) & 3 {
                0 => 1,
                2 => -1,
                _ => 0,
            };
            let mut i = 0usize;
            while count > 0 {
                let baddr = c.bbad.wrapping_add(pat[i & 3]);
                if c.dmap & 0x80 != 0 {
                    let v = self.read_b(baddr);
                    self.write_raw(c.a1b, c.a1t, v);
                } else {
                    let v = self.read_raw(c.a1b, c.a1t);
                    self.write_b(baddr, v);
                }
                self.clock += 8;
                c.a1t = (c.a1t as i32 + step) as u16;
                count -= 1;
                i += 1;
            }
            c.das = 0;
            self.dma[ch] = c;
        }
    }

    fn hdma_init(&mut self) {
        for ch in 0..8 {
            let mut c = self.dma[ch];
            c.term = true;
            c.do_transfer = false;
            if self.hdmaen & (1 << ch) != 0 {
                c.a2a = c.a1t;
                c.term = false;
                c.nltr = self.read_raw(c.a1b, c.a2a);
                c.a2a = c.a2a.wrapping_add(1);
                if c.dmap & 0x40 != 0 {
                    let lo = self.read_raw(c.a1b, c.a2a) as u16;
                    let hi = self.read_raw(c.a1b, c.a2a.wrapping_add(1)) as u16;
                    c.das = lo | hi << 8;
                    c.a2a = c.a2a.wrapping_add(2);
                }
                if c.nltr == 0 {
                    c.term = true;
                }
                c.do_transfer = true;
                self.clock += 8;
            }
            self.dma[ch] = c;
        }
    }

    fn hdma_run(&mut self) {
        if self.hdmaen == 0 {
            return;
        }
        for ch in 0..8 {
            if self.hdmaen & (1 << ch) == 0 {
                continue;
            }
            let mut c = self.dma[ch];
            if c.term {
                continue;
            }
            let indirect = c.dmap & 0x40 != 0;
            if c.do_transfer {
                let mode = (c.dmap & 7) as usize;
                for i in 0..HDMA_LEN[mode] {
                    let v = if indirect {
                        let v = self.read_raw(c.dasb, c.das);
                        c.das = c.das.wrapping_add(1);
                        v
                    } else {
                        let v = self.read_raw(c.a1b, c.a2a);
                        c.a2a = c.a2a.wrapping_add(1);
                        v
                    };
                    let baddr = c.bbad.wrapping_add(PATTERN[mode][i]);
                    if c.dmap & 0x80 != 0 {
                        // B->A bei HDMA praktisch ungenutzt
                    } else {
                        self.write_b(baddr, v);
                    }
                    self.clock += 8;
                }
            }
            let lc = c.nltr.wrapping_sub(1);
            c.nltr = lc;
            c.do_transfer = lc & 0x80 != 0;
            if lc & 0x7F == 0 {
                c.nltr = self.read_raw(c.a1b, c.a2a);
                c.a2a = c.a2a.wrapping_add(1);
                if indirect {
                    let lo = self.read_raw(c.a1b, c.a2a) as u16;
                    let hi = self.read_raw(c.a1b, c.a2a.wrapping_add(1)) as u16;
                    c.das = lo | hi << 8;
                    c.a2a = c.a2a.wrapping_add(2);
                }
                if c.nltr == 0 {
                    c.term = true;
                }
                c.do_transfer = true;
            }
            self.dma[ch] = c;
        }
    }

    /// Wert an `addr` ist schon bekannt (übersetzter ROM-Code): nur Takt und offenen Bus wie bei `read`
    #[inline(always)]
    pub(crate) fn fetch_known(&mut self, addr: u32, v: u8) {
        self.clock += self.speed((addr >> 16) as u8, addr as u16);
        self.open_bus = v;
    }

    /// Verarbeitet Zeilen-Ereignisse (Rendering, HDMA, VBlank, IRQ) bis zur aktuellen Zeit.
    #[inline]
    pub fn process_events(&mut self) {
        // schneller Weg: nichts fällig (gleiche Bedingungen wie unten, nur ohne Funktionsaufruf)
        let h = self.clock - self.line_start;
        if (self.hblank_done || h < HBLANK_START) && h < LINE_CYCLES && (self.nmitimen & 0x30 == 0 || self.irq_fired) {
            return;
        }
        self.process_events_slow();
    }

    #[inline(never)]
    fn process_events_slow(&mut self) {
        loop {
            let h = self.clock - self.line_start;
            if !self.hblank_done && h >= HBLANK_START {
                self.hblank_done = true;
                if self.line >= 1 && self.line <= 224 {
                    self.ppu.render_line(self.line);
                }
                if self.line < VBLANK_LINE {
                    self.hdma_run();
                }
                continue;
            }
            if h >= LINE_CYCLES {
                self.line_start += LINE_CYCLES;
                self.next_line();
                continue;
            }
            break;
        }
        let en = self.nmitimen & 0x30;
        if en != 0 && !self.irq_fired {
            let h = self.clock - self.line_start;
            let line_ok = en & 0x20 == 0 || self.line == self.vtime;
            let target = if en & 0x10 != 0 { self.htime as u64 * 4 } else { 0 };
            if line_ok && h >= target {
                self.timeup = true;
                self.irq_fired = true;
            }
        }
    }

    fn next_line(&mut self) {
        self.line += 1;
        self.hblank_done = false;
        self.irq_fired = false;
        if self.line == if self.pal { 312 } else { 262 } {
            self.line = 0;
            self.nmi_flag = false;
            self.hdma_init();
        }
        if self.line == VBLANK_LINE {
            self.nmi_flag = true;
            if self.nmitimen & 0x80 != 0 {
                self.nmi_pending = true;
            }
            if self.nmitimen & 1 != 0 {
                self.joy_auto = self.joy;
            }
            self.ppu.vblank_start();
            self.frame_done = true;
        }
    }
}
