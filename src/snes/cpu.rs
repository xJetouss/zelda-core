//! WDC 65C816 – die Haupt-CPU des SNES.

use super::bus::Bus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
enum Am {
    Dp,
    DpX,
    DpY,
    DpInd,
    DpIndX,
    DpIndY,
    DpIndL,
    DpIndLY,
    Abs,
    AbsX,
    AbsY,
    Long,
    LongX,
    Sr,
    SrIndY,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Cpu {
    pub a: u16,
    pub x: u16,
    pub y: u16,
    pub sp: u16,
    pub dp: u16,
    pub db: u8,
    pub pb: u8,
    pub pc: u16,
    n: bool,
    v: bool,
    m: bool,
    xf: bool,
    d: bool,
    i: bool,
    z: bool,
    c: bool,
    e: bool,
    waiting: bool,
    stopped: bool,
}

type Op = fn(&mut Cpu, u16) -> u16;

impl Cpu {
    pub fn reset(&mut self, b: &mut Bus) {
        self.e = true;
        self.m = true;
        self.xf = true;
        self.i = true;
        self.d = false;
        self.sp = 0x01FF;
        self.dp = 0;
        self.db = 0;
        self.pb = 0;
        self.x &= 0xFF;
        self.y &= 0xFF;
        self.waiting = false;
        self.stopped = false;
        self.pc = self.rd16(b, 0xFFFC, false);
    }

    pub fn step(&mut self, b: &mut Bus) {
        if self.stopped {
            b.clock += 16;
            return;
        }
        if b.nmi_pending {
            b.nmi_pending = false;
            self.waiting = false;
            self.interrupt(b, 0xFFEA, 0xFFFA, false);
            return;
        }
        if b.timeup {
            self.waiting = false;
            if !self.i {
                self.interrupt(b, 0xFFEE, 0xFFFE, false);
                return;
            }
        }
        if self.waiting {
            b.clock += 16;
            return;
        }
        let op = self.fetch8(b);
        self.exec(b, op);
    }

    // ---------------------------------------------------------------- Flags
    fn p(&self, brk: bool) -> u8 {
        let mut p = 0u8;
        if self.n {
            p |= 0x80
        }
        if self.v {
            p |= 0x40
        }
        if self.e {
            p |= 0x20;
            if brk {
                p |= 0x10
            }
        } else {
            if self.m {
                p |= 0x20
            }
            if self.xf {
                p |= 0x10
            }
        }
        if self.d {
            p |= 0x08
        }
        if self.i {
            p |= 0x04
        }
        if self.z {
            p |= 0x02
        }
        if self.c {
            p |= 0x01
        }
        p
    }

    fn set_p(&mut self, p: u8) {
        self.n = p & 0x80 != 0;
        self.v = p & 0x40 != 0;
        self.d = p & 0x08 != 0;
        self.i = p & 0x04 != 0;
        self.z = p & 0x02 != 0;
        self.c = p & 0x01 != 0;
        if self.e {
            self.m = true;
            self.xf = true;
        } else {
            self.m = p & 0x20 != 0;
            self.xf = p & 0x10 != 0;
        }
        if self.xf {
            self.x &= 0xFF;
            self.y &= 0xFF;
        }
    }

    #[inline]
    fn nz8(&mut self, v: u16) {
        self.z = v & 0xFF == 0;
        self.n = v & 0x80 != 0;
    }
    #[inline]
    fn nz16(&mut self, v: u16) {
        self.z = v == 0;
        self.n = v & 0x8000 != 0;
    }

    fn set_a(&mut self, v: u16) {
        if self.m {
            self.a = (self.a & 0xFF00) | (v & 0xFF);
            self.nz8(v);
        } else {
            self.a = v;
            self.nz16(v);
        }
    }
    fn set_x(&mut self, v: u16) {
        if self.xf {
            self.x = v & 0xFF;
            self.nz8(v);
        } else {
            self.x = v;
            self.nz16(v);
        }
    }
    fn set_y(&mut self, v: u16) {
        if self.xf {
            self.y = v & 0xFF;
            self.nz8(v);
        } else {
            self.y = v;
            self.nz16(v);
        }
    }

    // ---------------------------------------------------------------- Speicher
    #[inline]
    fn fetch8(&mut self, b: &mut Bus) -> u8 {
        let v = b.read(((self.pb as u32) << 16) | self.pc as u32);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self, b: &mut Bus) -> u16 {
        let lo = self.fetch8(b) as u16;
        lo | (self.fetch8(b) as u16) << 8
    }
    fn fetch24(&mut self, b: &mut Bus) -> u32 {
        let lo = self.fetch16(b) as u32;
        lo | (self.fetch8(b) as u32) << 16
    }

    #[inline]
    fn next_addr(a: u32, wrap: bool) -> u32 {
        if wrap {
            (a & 0xFF0000) | (a.wrapping_add(1) & 0xFFFF)
        } else {
            a.wrapping_add(1) & 0xFFFFFF
        }
    }
    fn rd16(&mut self, b: &mut Bus, a: u32, wrap: bool) -> u16 {
        let lo = b.read(a) as u16;
        lo | (b.read(Self::next_addr(a, wrap)) as u16) << 8
    }
    fn wr16(&mut self, b: &mut Bus, a: u32, wrap: bool, v: u16) {
        b.write(a, v as u8);
        b.write(Self::next_addr(a, wrap), (v >> 8) as u8);
    }

    fn push8(&mut self, b: &mut Bus, v: u8) {
        b.write(self.sp as u32, v);
        self.sp = self.sp.wrapping_sub(1);
        if self.e {
            self.sp = 0x100 | (self.sp & 0xFF);
        }
    }
    fn pull8(&mut self, b: &mut Bus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        if self.e {
            self.sp = 0x100 | (self.sp & 0xFF);
        }
        b.read(self.sp as u32)
    }
    fn push16(&mut self, b: &mut Bus, v: u16) {
        self.push8(b, (v >> 8) as u8);
        self.push8(b, v as u8);
    }
    fn pull16(&mut self, b: &mut Bus) -> u16 {
        let lo = self.pull8(b) as u16;
        lo | (self.pull8(b) as u16) << 8
    }

    fn dp_off(&mut self, b: &mut Bus) -> u8 {
        let o = self.fetch8(b);
        if self.dp & 0xFF != 0 {
            b.idle();
        }
        o
    }

    /// Effektive Adresse; bool = Wrap innerhalb von Bank 0 (Direct Page / Stack).
    fn ea(&mut self, b: &mut Bus, am: Am) -> (u32, bool) {
        let db = (self.db as u32) << 16;
        match am {
            Am::Dp => {
                let o = self.dp_off(b);
                (self.dp.wrapping_add(o as u16) as u32, true)
            }
            Am::DpX => {
                let o = self.dp_off(b);
                b.idle();
                (self.dp.wrapping_add(o as u16).wrapping_add(self.x) as u32, true)
            }
            Am::DpY => {
                let o = self.dp_off(b);
                b.idle();
                (self.dp.wrapping_add(o as u16).wrapping_add(self.y) as u32, true)
            }
            Am::DpInd => {
                let o = self.dp_off(b);
                let p = self.dp.wrapping_add(o as u16) as u32;
                let ptr = self.rd16(b, p, true) as u32;
                (db | ptr, false)
            }
            Am::DpIndX => {
                let o = self.dp_off(b);
                b.idle();
                let p = self.dp.wrapping_add(o as u16).wrapping_add(self.x) as u32;
                let ptr = self.rd16(b, p, true) as u32;
                (db | ptr, false)
            }
            Am::DpIndY => {
                let o = self.dp_off(b);
                let p = self.dp.wrapping_add(o as u16) as u32;
                let ptr = self.rd16(b, p, true) as u32;
                b.idle();
                (((db | ptr) + self.y as u32) & 0xFFFFFF, false)
            }
            Am::DpIndL | Am::DpIndLY => {
                let o = self.dp_off(b);
                let p = self.dp.wrapping_add(o as u16) as u32;
                let lo = self.rd16(b, p, true) as u32;
                let bank = b.read((p + 2) & 0xFFFF) as u32;
                let mut a = (bank << 16) | lo;
                if let Am::DpIndLY = am {
                    a = (a + self.y as u32) & 0xFFFFFF;
                }
                (a, false)
            }
            Am::Abs => {
                let a = self.fetch16(b) as u32;
                (db | a, false)
            }
            Am::AbsX => {
                let a = self.fetch16(b) as u32;
                b.idle();
                (((db | a) + self.x as u32) & 0xFFFFFF, false)
            }
            Am::AbsY => {
                let a = self.fetch16(b) as u32;
                b.idle();
                (((db | a) + self.y as u32) & 0xFFFFFF, false)
            }
            Am::Long => (self.fetch24(b), false),
            Am::LongX => {
                let a = self.fetch24(b);
                ((a + self.x as u32) & 0xFFFFFF, false)
            }
            Am::Sr => {
                let o = self.fetch8(b);
                b.idle();
                (self.sp.wrapping_add(o as u16) as u32, true)
            }
            Am::SrIndY => {
                let o = self.fetch8(b);
                b.idle();
                let p = self.sp.wrapping_add(o as u16) as u32;
                let ptr = self.rd16(b, p, true) as u32;
                b.idle();
                (((db | ptr) + self.y as u32) & 0xFFFFFF, false)
            }
        }
    }

    fn imm(&mut self, b: &mut Bus, wide: bool) -> u16 {
        if wide {
            self.fetch16(b)
        } else {
            self.fetch8(b) as u16
        }
    }
    fn load(&mut self, b: &mut Bus, am: Am, wide: bool) -> u16 {
        let (a, w) = self.ea(b, am);
        if wide {
            self.rd16(b, a, w)
        } else {
            b.read(a) as u16
        }
    }
    fn store(&mut self, b: &mut Bus, am: Am, v: u16, wide: bool) {
        let (a, w) = self.ea(b, am);
        if wide {
            self.wr16(b, a, w, v)
        } else {
            b.write(a, v as u8)
        }
    }

    // ---------------------------------------------------------------- ALU
    fn add_sub(&mut self, v: u16, sub: bool) {
        let bits: u32 = if self.m { 8 } else { 16 };
        let mask: u32 = (1 << bits) - 1;
        let a = self.a as u32 & mask;
        let v = v as u32 & mask;
        let top = 1u32 << (bits - 1);
        let r: u32;
        if !self.d {
            let s = a + v + self.c as u32;
            self.v = (!(a ^ v) & (a ^ s) & top) != 0;
            self.c = s > mask;
            r = s & mask;
        } else {
            let mut res = 0u32;
            let mut carry = self.c as u32;
            let nib = bits / 4;
            for i in 0..nib {
                let sh = i * 4;
                let mut ds = ((a >> sh) & 0xF) + ((v >> sh) & 0xF) + carry;
                if i == nib - 1 {
                    let pre = (ds << sh) + res;
                    self.v = (!(a ^ v) & (a ^ pre) & top) != 0;
                }
                if sub {
                    carry = (ds > 0xF) as u32;
                    if ds <= 0xF {
                        ds = ds.wrapping_sub(6);
                    }
                } else {
                    if ds > 9 {
                        ds += 6;
                    }
                    carry = (ds > 0xF) as u32;
                }
                res |= (ds & 0xF) << sh;
            }
            self.c = carry != 0;
            r = res;
        }
        if self.m {
            self.a = (self.a & 0xFF00) | r as u16;
            self.nz8(r as u16);
        } else {
            self.a = r as u16;
            self.nz16(r as u16);
        }
    }

    fn cmp(&mut self, r: u16, v: u16, wide: bool) {
        if wide {
            self.c = r >= v;
            self.nz16(r.wrapping_sub(v));
        } else {
            let (r, v) = (r & 0xFF, v & 0xFF);
            self.c = r >= v;
            self.nz8(r.wrapping_sub(v));
        }
    }

    fn bit(&mut self, v: u16, imm: bool) {
        if self.m {
            self.z = v & self.a & 0xFF == 0;
            if !imm {
                self.n = v & 0x80 != 0;
                self.v = v & 0x40 != 0;
            }
        } else {
            self.z = v & self.a == 0;
            if !imm {
                self.n = v & 0x8000 != 0;
                self.v = v & 0x4000 != 0;
            }
        }
    }

    fn asl(&mut self, v: u16) -> u16 {
        if self.m {
            self.c = v & 0x80 != 0;
            let r = (v << 1) & 0xFF;
            self.nz8(r);
            r
        } else {
            self.c = v & 0x8000 != 0;
            let r = v << 1;
            self.nz16(r);
            r
        }
    }
    fn lsr(&mut self, v: u16) -> u16 {
        let v = if self.m { v & 0xFF } else { v };
        self.c = v & 1 != 0;
        let r = v >> 1;
        if self.m {
            self.nz8(r)
        } else {
            self.nz16(r)
        }
        r
    }
    fn rol(&mut self, v: u16) -> u16 {
        let c = self.c as u16;
        if self.m {
            self.c = v & 0x80 != 0;
            let r = ((v << 1) | c) & 0xFF;
            self.nz8(r);
            r
        } else {
            self.c = v & 0x8000 != 0;
            let r = (v << 1) | c;
            self.nz16(r);
            r
        }
    }
    fn ror(&mut self, v: u16) -> u16 {
        let c = self.c;
        if self.m {
            let v = v & 0xFF;
            self.c = v & 1 != 0;
            let r = (v >> 1) | if c { 0x80 } else { 0 };
            self.nz8(r);
            r
        } else {
            self.c = v & 1 != 0;
            let r = (v >> 1) | if c { 0x8000 } else { 0 };
            self.nz16(r);
            r
        }
    }
    fn inc(&mut self, v: u16) -> u16 {
        if self.m {
            let r = v.wrapping_add(1) & 0xFF;
            self.nz8(r);
            r
        } else {
            let r = v.wrapping_add(1);
            self.nz16(r);
            r
        }
    }
    fn dec(&mut self, v: u16) -> u16 {
        if self.m {
            let r = v.wrapping_sub(1) & 0xFF;
            self.nz8(r);
            r
        } else {
            let r = v.wrapping_sub(1);
            self.nz16(r);
            r
        }
    }
    fn tsb(&mut self, v: u16) -> u16 {
        if self.m {
            self.z = v & self.a & 0xFF == 0;
            (v | self.a) & 0xFF
        } else {
            self.z = v & self.a == 0;
            v | self.a
        }
    }
    fn trb(&mut self, v: u16) -> u16 {
        if self.m {
            self.z = v & self.a & 0xFF == 0;
            v & !self.a & 0xFF
        } else {
            self.z = v & self.a == 0;
            v & !self.a
        }
    }

    fn rmw(&mut self, b: &mut Bus, am: Am, f: Op) {
        let wide = !self.m;
        let (a, w) = self.ea(b, am);
        let v = if wide { self.rd16(b, a, w) } else { b.read(a) as u16 };
        b.idle();
        let r = f(self, v);
        if wide {
            self.wr16(b, a, w, r)
        } else {
            b.write(a, r as u8)
        }
    }

    fn acc_op(&mut self, b: &mut Bus, f: Op) {
        b.idle();
        let a = self.a;
        let r = f(self, a);
        if self.m {
            self.a = (self.a & 0xFF00) | (r & 0xFF);
        } else {
            self.a = r;
        }
    }

    fn branch(&mut self, b: &mut Bus, cond: bool) {
        let off = self.fetch8(b) as i8;
        if cond {
            b.idle();
            self.pc = self.pc.wrapping_add(off as i16 as u16);
        }
    }

    fn interrupt(&mut self, b: &mut Bus, native: u16, emu: u16, brk: bool) {
        b.idle();
        b.idle();
        if !self.e {
            self.push8(b, self.pb);
        }
        self.push16(b, self.pc);
        let p = self.p(brk);
        self.push8(b, p);
        self.i = true;
        self.d = false;
        self.pb = 0;
        let v = if self.e { emu } else { native };
        self.pc = self.rd16(b, v as u32, false);
    }

    fn group(&mut self, b: &mut Bus, kind: u8, am: Option<Am>) {
        let wide = !self.m;
        if kind == 4 {
            let a = self.a;
            self.store(b, am.unwrap(), a, wide);
            return;
        }
        let v = match am {
            None => self.imm(b, wide),
            Some(am) => self.load(b, am, wide),
        };
        match kind {
            0 => self.set_a(self.a | v),
            1 => self.set_a(self.a & v),
            2 => self.set_a(self.a ^ v),
            3 => self.add_sub(v, false),
            5 => self.set_a(v),
            6 => self.cmp(self.a, v, wide),
            _ => self.add_sub(!v, true),
        }
    }

    // ---------------------------------------------------------------- Recompiler-Schnittstelle
    /// Läuft die CPU normal (kein WAI/STP)? Sonst übernimmt der Interpreter.
    #[inline(always)]
    pub(crate) fn running(&self) -> bool {
        !self.waiting && !self.stopped
    }

    /// IRQ gesperrt (I-Flag)?
    #[inline(always)]
    pub(crate) fn irq_masked(&self) -> bool {
        self.i
    }

    /// Statusregister lesen/setzen (für Mods im Recompiled-Modus)
    pub(crate) fn p_reg(&self) -> u8 {
        self.p(false)
    }
    pub(crate) fn set_p_reg(&mut self, p: u8) {
        self.set_p(p)
    }

    /// Flags M, X, E (für die Aufzeichnung des ausgeführten Codes)
    #[cfg(test)]
    pub(crate) fn mxe(&self) -> (bool, bool, bool) {
        (self.m, self.xf, self.e)
    }

    /// Einen Befehl ausführen, dessen Opcode schon geholt ist (genau wie `step`)
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) fn exec_op(&mut self, b: &mut Bus, op: u8) {
        self.exec(b, op)
    }

    // ---------------------------------------------------------------- Opcodes
    #[inline(always)]
    fn exec(&mut self, b: &mut Bus, op: u8) {
        if op != 0x89 {
            let am = match op & 0x1F {
                0x01 => Some(Some(Am::DpIndX)),
                0x03 => Some(Some(Am::Sr)),
                0x05 => Some(Some(Am::Dp)),
                0x07 => Some(Some(Am::DpIndL)),
                0x09 => Some(None),
                0x0D => Some(Some(Am::Abs)),
                0x0F => Some(Some(Am::Long)),
                0x11 => Some(Some(Am::DpIndY)),
                0x12 => Some(Some(Am::DpInd)),
                0x13 => Some(Some(Am::SrIndY)),
                0x15 => Some(Some(Am::DpX)),
                0x17 => Some(Some(Am::DpIndLY)),
                0x19 => Some(Some(Am::AbsY)),
                0x1D => Some(Some(Am::AbsX)),
                0x1F => Some(Some(Am::LongX)),
                _ => None,
            };
            if let Some(am) = am {
                self.group(b, op >> 5, am);
                return;
            }
        }
        let xw = !self.xf;
        let mw = !self.m;
        match op {
            0x00 => {
                self.fetch8(b);
                self.interrupt(b, 0xFFE6, 0xFFFE, true);
            }
            0x02 => {
                self.fetch8(b);
                self.interrupt(b, 0xFFE4, 0xFFF4, false);
            }
            // TSB / TRB
            0x04 => self.rmw(b, Am::Dp, Cpu::tsb),
            0x0C => self.rmw(b, Am::Abs, Cpu::tsb),
            0x14 => self.rmw(b, Am::Dp, Cpu::trb),
            0x1C => self.rmw(b, Am::Abs, Cpu::trb),
            // Shift / Rotate / INC / DEC Speicher
            0x06 => self.rmw(b, Am::Dp, Cpu::asl),
            0x0E => self.rmw(b, Am::Abs, Cpu::asl),
            0x16 => self.rmw(b, Am::DpX, Cpu::asl),
            0x1E => self.rmw(b, Am::AbsX, Cpu::asl),
            0x26 => self.rmw(b, Am::Dp, Cpu::rol),
            0x2E => self.rmw(b, Am::Abs, Cpu::rol),
            0x36 => self.rmw(b, Am::DpX, Cpu::rol),
            0x3E => self.rmw(b, Am::AbsX, Cpu::rol),
            0x46 => self.rmw(b, Am::Dp, Cpu::lsr),
            0x4E => self.rmw(b, Am::Abs, Cpu::lsr),
            0x56 => self.rmw(b, Am::DpX, Cpu::lsr),
            0x5E => self.rmw(b, Am::AbsX, Cpu::lsr),
            0x66 => self.rmw(b, Am::Dp, Cpu::ror),
            0x6E => self.rmw(b, Am::Abs, Cpu::ror),
            0x76 => self.rmw(b, Am::DpX, Cpu::ror),
            0x7E => self.rmw(b, Am::AbsX, Cpu::ror),
            0xC6 => self.rmw(b, Am::Dp, Cpu::dec),
            0xCE => self.rmw(b, Am::Abs, Cpu::dec),
            0xD6 => self.rmw(b, Am::DpX, Cpu::dec),
            0xDE => self.rmw(b, Am::AbsX, Cpu::dec),
            0xE6 => self.rmw(b, Am::Dp, Cpu::inc),
            0xEE => self.rmw(b, Am::Abs, Cpu::inc),
            0xF6 => self.rmw(b, Am::DpX, Cpu::inc),
            0xFE => self.rmw(b, Am::AbsX, Cpu::inc),
            // Akkumulator
            0x0A => self.acc_op(b, Cpu::asl),
            0x2A => self.acc_op(b, Cpu::rol),
            0x4A => self.acc_op(b, Cpu::lsr),
            0x6A => self.acc_op(b, Cpu::ror),
            0x1A => self.acc_op(b, Cpu::inc),
            0x3A => self.acc_op(b, Cpu::dec),
            // Stack
            0x08 => {
                b.idle();
                let p = self.p(true);
                self.push8(b, p);
            }
            0x28 => {
                b.idle();
                b.idle();
                let p = self.pull8(b);
                self.set_p(p);
            }
            0x0B => {
                b.idle();
                self.push16(b, self.dp);
            }
            0x2B => {
                b.idle();
                b.idle();
                self.dp = self.pull16(b);
                self.nz16(self.dp);
            }
            0x4B => {
                b.idle();
                self.push8(b, self.pb);
            }
            0x8B => {
                b.idle();
                self.push8(b, self.db);
            }
            0xAB => {
                b.idle();
                b.idle();
                self.db = self.pull8(b);
                self.nz8(self.db as u16);
            }
            0x48 => {
                b.idle();
                if self.m {
                    self.push8(b, self.a as u8)
                } else {
                    self.push16(b, self.a)
                }
            }
            0x68 => {
                b.idle();
                b.idle();
                let v = if self.m { self.pull8(b) as u16 } else { self.pull16(b) };
                self.set_a(v);
            }
            0xDA => {
                b.idle();
                if self.xf {
                    self.push8(b, self.x as u8)
                } else {
                    self.push16(b, self.x)
                }
            }
            0xFA => {
                b.idle();
                b.idle();
                let v = if self.xf { self.pull8(b) as u16 } else { self.pull16(b) };
                self.set_x(v);
            }
            0x5A => {
                b.idle();
                if self.xf {
                    self.push8(b, self.y as u8)
                } else {
                    self.push16(b, self.y)
                }
            }
            0x7A => {
                b.idle();
                b.idle();
                let v = if self.xf { self.pull8(b) as u16 } else { self.pull16(b) };
                self.set_y(v);
            }
            0xF4 => {
                let v = self.fetch16(b);
                self.push16(b, v);
            }
            0xD4 => {
                let o = self.dp_off(b);
                let a = self.dp.wrapping_add(o as u16) as u32;
                let v = self.rd16(b, a, true);
                self.push16(b, v);
            }
            0x62 => {
                let o = self.fetch16(b);
                b.idle();
                let v = self.pc.wrapping_add(o);
                self.push16(b, v);
            }
            // Branches
            0x10 => self.branch(b, !self.n),
            0x30 => self.branch(b, self.n),
            0x50 => self.branch(b, !self.v),
            0x70 => self.branch(b, self.v),
            0x90 => self.branch(b, !self.c),
            0xB0 => self.branch(b, self.c),
            0xD0 => self.branch(b, !self.z),
            0xF0 => self.branch(b, self.z),
            0x80 => self.branch(b, true),
            0x82 => {
                let o = self.fetch16(b);
                b.idle();
                self.pc = self.pc.wrapping_add(o);
            }
            // Flags
            0x18 => {
                b.idle();
                self.c = false
            }
            0x38 => {
                b.idle();
                self.c = true
            }
            0x58 => {
                b.idle();
                self.i = false
            }
            0x78 => {
                b.idle();
                self.i = true
            }
            0xD8 => {
                b.idle();
                self.d = false
            }
            0xF8 => {
                b.idle();
                self.d = true
            }
            0xB8 => {
                b.idle();
                self.v = false
            }
            0xC2 => {
                let v = self.fetch8(b);
                b.idle();
                let p = self.p(false) & !v;
                self.set_p(p);
            }
            0xE2 => {
                let v = self.fetch8(b);
                b.idle();
                let p = self.p(false) | v;
                self.set_p(p);
            }
            0xFB => {
                b.idle();
                let c = self.c;
                self.c = self.e;
                self.e = c;
                if self.e {
                    self.m = true;
                    self.xf = true;
                    self.x &= 0xFF;
                    self.y &= 0xFF;
                    self.sp = 0x100 | (self.sp & 0xFF);
                }
            }
            // Transfers
            0x1B => {
                b.idle();
                self.sp = if self.e { 0x100 | (self.a & 0xFF) } else { self.a };
            }
            0x3B => {
                b.idle();
                self.a = self.sp;
                self.nz16(self.a);
            }
            0x5B => {
                b.idle();
                self.dp = self.a;
                self.nz16(self.dp);
            }
            0x7B => {
                b.idle();
                self.a = self.dp;
                self.nz16(self.a);
            }
            0xAA => {
                b.idle();
                self.set_x(self.a)
            }
            0xA8 => {
                b.idle();
                self.set_y(self.a)
            }
            0x8A => {
                b.idle();
                self.set_a(self.x)
            }
            0x98 => {
                b.idle();
                self.set_a(self.y)
            }
            0x9B => {
                b.idle();
                self.set_y(self.x)
            }
            0xBB => {
                b.idle();
                self.set_x(self.y)
            }
            0xBA => {
                b.idle();
                self.set_x(self.sp)
            }
            0x9A => {
                b.idle();
                self.sp = if self.e { 0x100 | (self.x & 0xFF) } else { self.x };
            }
            0xEB => {
                b.idle();
                b.idle();
                self.a = self.a.swap_bytes();
                self.nz8(self.a);
            }
            // Index-Register
            0xE8 => {
                b.idle();
                self.set_x(self.x.wrapping_add(1))
            }
            0xC8 => {
                b.idle();
                self.set_y(self.y.wrapping_add(1))
            }
            0xCA => {
                b.idle();
                self.set_x(self.x.wrapping_sub(1))
            }
            0x88 => {
                b.idle();
                self.set_y(self.y.wrapping_sub(1))
            }
            0xA2 => {
                let v = self.imm(b, xw);
                self.set_x(v)
            }
            0xA6 => {
                let v = self.load(b, Am::Dp, xw);
                self.set_x(v)
            }
            0xAE => {
                let v = self.load(b, Am::Abs, xw);
                self.set_x(v)
            }
            0xB6 => {
                let v = self.load(b, Am::DpY, xw);
                self.set_x(v)
            }
            0xBE => {
                let v = self.load(b, Am::AbsY, xw);
                self.set_x(v)
            }
            0xA0 => {
                let v = self.imm(b, xw);
                self.set_y(v)
            }
            0xA4 => {
                let v = self.load(b, Am::Dp, xw);
                self.set_y(v)
            }
            0xAC => {
                let v = self.load(b, Am::Abs, xw);
                self.set_y(v)
            }
            0xB4 => {
                let v = self.load(b, Am::DpX, xw);
                self.set_y(v)
            }
            0xBC => {
                let v = self.load(b, Am::AbsX, xw);
                self.set_y(v)
            }
            0x86 => self.store(b, Am::Dp, self.x, xw),
            0x8E => self.store(b, Am::Abs, self.x, xw),
            0x96 => self.store(b, Am::DpY, self.x, xw),
            0x84 => self.store(b, Am::Dp, self.y, xw),
            0x8C => self.store(b, Am::Abs, self.y, xw),
            0x94 => self.store(b, Am::DpX, self.y, xw),
            0x64 => self.store(b, Am::Dp, 0, mw),
            0x74 => self.store(b, Am::DpX, 0, mw),
            0x9C => self.store(b, Am::Abs, 0, mw),
            0x9E => self.store(b, Am::AbsX, 0, mw),
            0xE0 => {
                let v = self.imm(b, xw);
                self.cmp(self.x, v, xw)
            }
            0xE4 => {
                let v = self.load(b, Am::Dp, xw);
                self.cmp(self.x, v, xw)
            }
            0xEC => {
                let v = self.load(b, Am::Abs, xw);
                self.cmp(self.x, v, xw)
            }
            0xC0 => {
                let v = self.imm(b, xw);
                self.cmp(self.y, v, xw)
            }
            0xC4 => {
                let v = self.load(b, Am::Dp, xw);
                self.cmp(self.y, v, xw)
            }
            0xCC => {
                let v = self.load(b, Am::Abs, xw);
                self.cmp(self.y, v, xw)
            }
            // BIT
            0x89 => {
                let v = self.imm(b, mw);
                self.bit(v, true)
            }
            0x24 => {
                let v = self.load(b, Am::Dp, mw);
                self.bit(v, false)
            }
            0x2C => {
                let v = self.load(b, Am::Abs, mw);
                self.bit(v, false)
            }
            0x34 => {
                let v = self.load(b, Am::DpX, mw);
                self.bit(v, false)
            }
            0x3C => {
                let v = self.load(b, Am::AbsX, mw);
                self.bit(v, false)
            }
            // Sprünge
            0x4C => self.pc = self.fetch16(b),
            0x5C => {
                let a = self.fetch24(b);
                self.pb = (a >> 16) as u8;
                self.pc = a as u16;
            }
            0x6C => {
                let p = self.fetch16(b) as u32;
                self.pc = self.rd16(b, p, true);
            }
            0x7C => {
                let p = self.fetch16(b).wrapping_add(self.x) as u32;
                b.idle();
                self.pc = self.rd16(b, ((self.pb as u32) << 16) | p, true);
            }
            0xDC => {
                let p = self.fetch16(b) as u32;
                let lo = self.rd16(b, p, true);
                let bank = b.read((p + 2) & 0xFFFF);
                self.pc = lo;
                self.pb = bank;
            }
            0x20 => {
                let t = self.fetch16(b);
                b.idle();
                let r = self.pc.wrapping_sub(1);
                self.push16(b, r);
                self.pc = t;
            }
            0xFC => {
                let p = self.fetch16(b);
                let r = self.pc.wrapping_sub(1);
                self.push16(b, r);
                b.idle();
                let a = ((self.pb as u32) << 16) | p.wrapping_add(self.x) as u32;
                self.pc = self.rd16(b, a, true);
            }
            0x22 => {
                let t = self.fetch24(b);
                self.push8(b, self.pb);
                b.idle();
                let r = self.pc.wrapping_sub(1);
                self.push16(b, r);
                self.pb = (t >> 16) as u8;
                self.pc = t as u16;
            }
            0x60 => {
                b.idle();
                b.idle();
                self.pc = self.pull16(b).wrapping_add(1);
                b.idle();
            }
            0x6B => {
                b.idle();
                b.idle();
                self.pc = self.pull16(b).wrapping_add(1);
                self.pb = self.pull8(b);
            }
            0x40 => {
                b.idle();
                b.idle();
                let p = self.pull8(b);
                self.set_p(p);
                self.pc = self.pull16(b);
                if !self.e {
                    self.pb = self.pull8(b);
                }
            }
            // Block-Moves
            0x44 | 0x54 => {
                let dst = self.fetch8(b);
                let src = self.fetch8(b);
                self.db = dst;
                let v = b.read(((src as u32) << 16) | self.x as u32);
                b.write(((dst as u32) << 16) | self.y as u32, v);
                b.idle();
                b.idle();
                if op == 0x54 {
                    self.x = self.x.wrapping_add(1);
                    self.y = self.y.wrapping_add(1);
                } else {
                    self.x = self.x.wrapping_sub(1);
                    self.y = self.y.wrapping_sub(1);
                }
                if self.xf {
                    self.x &= 0xFF;
                    self.y &= 0xFF;
                }
                self.a = self.a.wrapping_sub(1);
                if self.a != 0xFFFF {
                    self.pc = self.pc.wrapping_sub(3);
                }
            }
            0x42 => {
                self.fetch8(b);
            }
            0xEA => b.idle(),
            0xCB => {
                b.idle();
                b.idle();
                self.waiting = true;
            }
            0xDB => {
                b.idle();
                self.stopped = true;
            }
            _ => b.idle(),
        }
    }
}
