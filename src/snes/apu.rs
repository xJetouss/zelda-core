//! Sony SPC700 Audio-CPU mit Timern und Kommunikationsports.

use super::dsp::Dsp;
use serde::{Deserialize, Serialize};

/// Der 64-Byte-Boot-Code (IPL) des SPC700.
const IPL: [u8; 64] = [
    0xCD, 0xEF, 0xBD, 0xE8, 0x00, 0xC6, 0x1D, 0xD0, 0xFC, 0x8F, 0xAA, 0xF4, 0x8F, 0xBB, 0xF5, 0x78, //
    0xCC, 0xF4, 0xD0, 0xFB, 0x2F, 0x19, 0xEB, 0xF4, 0xD0, 0xFC, 0x7E, 0xF4, 0xD0, 0x0B, 0xE4, 0xF5, //
    0xCB, 0xF4, 0xD7, 0x00, 0xFC, 0xD0, 0xF3, 0xAB, 0x01, 0x10, 0xEF, 0x7E, 0xF4, 0x10, 0xEB, 0xBA, //
    0xF6, 0xDA, 0x00, 0xBA, 0xF4, 0xC4, 0xF4, 0xDD, 0x5D, 0xD0, 0xDB, 0x1F, 0x00, 0x00, 0xC0, 0xFF,
];

#[rustfmt::skip]
const CYC: [u8; 256] = [
    2,8,4,5,3,4,3,6,2,6,5,4,5,4,6,8,
    2,8,4,5,4,5,5,6,5,5,6,5,2,2,4,6,
    2,8,4,5,3,4,3,6,2,6,5,4,5,4,5,4,
    2,8,4,5,4,5,5,6,5,5,6,5,2,2,3,8,
    2,8,4,5,3,4,3,6,2,6,4,4,5,4,6,6,
    2,8,4,5,4,5,5,6,5,5,4,5,2,2,4,3,
    2,8,4,5,3,4,3,6,2,6,4,4,5,4,5,5,
    2,8,4,5,4,5,5,6,5,5,5,5,2,2,3,6,
    2,8,4,5,3,4,3,6,2,6,5,4,5,2,4,5,
    2,8,4,5,4,5,5,6,5,5,5,5,2,2,12,5,
    3,8,4,5,3,4,3,6,2,6,4,4,5,2,4,4,
    2,8,4,5,4,5,5,6,5,5,5,5,2,2,3,4,
    3,8,4,5,4,5,4,7,2,5,6,4,5,2,4,9,
    2,8,4,5,5,6,6,7,4,5,5,5,2,2,6,3,
    2,8,4,5,3,4,3,6,2,4,5,3,4,3,4,2,
    2,8,4,5,4,5,5,6,3,4,5,4,2,2,4,2,
];

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Timer {
    enabled: bool,
    target: u8,
    counter: u8,
    out: u8,
    stage: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Apu {
    pub ram: Vec<u8>,
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    pc: u16,
    n: bool,
    v: bool,
    p: bool,
    b: bool,
    h: bool,
    i: bool,
    z: bool,
    c: bool,
    pub port_in: [u8; 4],
    pub port_out: [u8; 4],
    dsp_addr: u8,
    ipl: bool,
    timers: [Timer; 3],
    pub dsp: Dsp,
    last_master: u64,
    debt: i64,
    dsp_cycles: u32,
    sleeping: bool,
    pub master_hz: i64,
}

impl Apu {
    /// DSP-Register übernehmen (beim Wechsel aus dem Zelda-3-Modus), ohne Stimmen neu anzuschlagen
    pub fn import_dsp_regs(&mut self, regs: &[u8; 128]) {
        self.dsp.regs.copy_from_slice(regs);
    }

    pub fn new() -> Self {
        let mut a = Apu {
            ram: vec![0; 0x10000],
            a: 0,
            x: 0,
            y: 0,
            sp: 0xEF,
            pc: 0,
            n: false,
            v: false,
            p: false,
            b: false,
            h: false,
            i: false,
            z: false,
            c: false,
            port_in: [0; 4],
            port_out: [0; 4],
            dsp_addr: 0,
            ipl: true,
            timers: [Timer::default(); 3],
            dsp: Dsp::new(),
            last_master: 0,
            debt: 0,
            dsp_cycles: 0,
            sleeping: false,
            master_hz: 21_477_272,
        };
        a.pc = a.rd16(0xFFFE);
        a
    }

    /// Lässt den SPC700 bis zur angegebenen SNES-Masterclock-Zeit laufen.
    pub fn sync(&mut self, master: u64) {
        let delta = master.saturating_sub(self.last_master);
        self.last_master = master;
        self.debt += delta as i64 * 1_024_000;
        while self.debt > 0 {
            let c = self.step();
            self.tick(c);
            self.debt -= c as i64 * self.master_hz;
        }
    }

    fn tick(&mut self, cycles: u32) {
        for (i, t) in self.timers.iter_mut().enumerate() {
            let div = if i == 2 { 16 } else { 128 };
            t.stage += cycles;
            while t.stage >= div {
                t.stage -= div;
                if t.enabled {
                    t.counter = t.counter.wrapping_add(1);
                    if t.counter == t.target {
                        t.counter = 0;
                        t.out = (t.out + 1) & 0xF;
                    }
                }
            }
        }
        self.dsp_cycles += cycles;
        while self.dsp_cycles >= 32 {
            self.dsp_cycles -= 32;
            self.dsp.tick(&mut self.ram);
        }
    }

    // ------------------------------------------------------------ Speicher
    fn rd(&mut self, a: u16) -> u8 {
        match a {
            0xF0..=0xFF => match a {
                0xF2 => self.dsp_addr,
                0xF3 => self.dsp.read(self.dsp_addr & 0x7F),
                0xF4..=0xF7 => self.port_in[(a - 0xF4) as usize],
                0xFD..=0xFF => {
                    let t = &mut self.timers[(a - 0xFD) as usize];
                    let v = t.out;
                    t.out = 0;
                    v
                }
                0xF8 | 0xF9 => self.ram[a as usize],
                _ => 0,
            },
            0xFFC0..=0xFFFF if self.ipl => IPL[(a - 0xFFC0) as usize],
            _ => self.ram[a as usize],
        }
    }

    fn wr(&mut self, a: u16, v: u8) {
        if (0xF0..=0xFF).contains(&a) {
            match a {
                0xF1 => {
                    for i in 0..3 {
                        let en = v & (1 << i) != 0;
                        if en && !self.timers[i].enabled {
                            self.timers[i].counter = 0;
                            self.timers[i].out = 0;
                        }
                        self.timers[i].enabled = en;
                    }
                    if v & 0x10 != 0 {
                        self.port_in[0] = 0;
                        self.port_in[1] = 0;
                    }
                    if v & 0x20 != 0 {
                        self.port_in[2] = 0;
                        self.port_in[3] = 0;
                    }
                    self.ipl = v & 0x80 != 0;
                }
                0xF2 => self.dsp_addr = v,
                0xF3 => {
                    if self.dsp_addr < 0x80 {
                        self.dsp.write(self.dsp_addr, v)
                    }
                }
                0xF4..=0xF7 => self.port_out[(a - 0xF4) as usize] = v,
                0xFA..=0xFC => self.timers[(a - 0xFA) as usize].target = v,
                _ => {}
            }
        }
        self.ram[a as usize] = v;
    }

    fn rd16(&mut self, a: u16) -> u16 {
        let lo = self.rd(a) as u16;
        lo | (self.rd(a.wrapping_add(1)) as u16) << 8
    }
    fn fetch(&mut self) -> u8 {
        let v = self.rd(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self) -> u16 {
        let lo = self.fetch() as u16;
        lo | (self.fetch() as u16) << 8
    }
    #[inline]
    fn dp(&self, o: u8) -> u16 {
        (if self.p { 0x100 } else { 0 }) | o as u16
    }
    fn rd_dp(&mut self, o: u8) -> u8 {
        let a = self.dp(o);
        self.rd(a)
    }
    fn wr_dp(&mut self, o: u8, v: u8) {
        let a = self.dp(o);
        self.wr(a, v)
    }
    fn rd_dp16(&mut self, o: u8) -> u16 {
        let lo = self.rd_dp(o) as u16;
        lo | (self.rd_dp(o.wrapping_add(1)) as u16) << 8
    }
    fn wr_dp16(&mut self, o: u8, v: u16) {
        self.wr_dp(o, v as u8);
        self.wr_dp(o.wrapping_add(1), (v >> 8) as u8);
    }
    fn push(&mut self, v: u8) {
        self.wr(0x100 | self.sp as u16, v);
        self.sp = self.sp.wrapping_sub(1);
    }
    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.rd(0x100 | self.sp as u16)
    }
    fn push16(&mut self, v: u16) {
        self.push((v >> 8) as u8);
        self.push(v as u8);
    }
    fn pop16(&mut self) -> u16 {
        let lo = self.pop() as u16;
        lo | (self.pop() as u16) << 8
    }

    fn psw(&self) -> u8 {
        (self.n as u8) << 7
            | (self.v as u8) << 6
            | (self.p as u8) << 5
            | (self.b as u8) << 4
            | (self.h as u8) << 3
            | (self.i as u8) << 2
            | (self.z as u8) << 1
            | self.c as u8
    }
    fn set_psw(&mut self, v: u8) {
        self.n = v & 0x80 != 0;
        self.v = v & 0x40 != 0;
        self.p = v & 0x20 != 0;
        self.b = v & 0x10 != 0;
        self.h = v & 0x08 != 0;
        self.i = v & 0x04 != 0;
        self.z = v & 0x02 != 0;
        self.c = v & 0x01 != 0;
    }

    // ------------------------------------------------------------ ALU
    #[inline]
    fn nz(&mut self, v: u8) {
        self.n = v & 0x80 != 0;
        self.z = v == 0;
    }
    fn nz16(&mut self, v: u16) {
        self.n = v & 0x8000 != 0;
        self.z = v == 0;
    }
    fn adc(&mut self, a: u8, b: u8) -> u8 {
        let r = a as u16 + b as u16 + self.c as u16;
        self.h = ((a ^ b ^ r as u8) & 0x10) != 0;
        self.v = (!(a ^ b) & (a ^ r as u8) & 0x80) != 0;
        self.c = r > 0xFF;
        let r = r as u8;
        self.nz(r);
        r
    }
    fn sbc(&mut self, a: u8, b: u8) -> u8 {
        self.adc(a, !b)
    }
    fn cmp(&mut self, a: u8, b: u8) {
        let r = a.wrapping_sub(b);
        self.c = a >= b;
        self.nz(r);
    }
    fn alu(&mut self, kind: u8, a: u8, b: u8) -> u8 {
        match kind {
            0 => {
                let r = a | b;
                self.nz(r);
                r
            }
            1 => {
                let r = a & b;
                self.nz(r);
                r
            }
            2 => {
                let r = a ^ b;
                self.nz(r);
                r
            }
            3 => {
                self.cmp(a, b);
                a
            }
            4 => self.adc(a, b),
            _ => self.sbc(a, b),
        }
    }
    fn shift(&mut self, kind: u8, v: u8) -> u8 {
        let r = match kind {
            0 => {
                self.c = v & 0x80 != 0;
                v << 1
            }
            1 => {
                let c = self.c as u8;
                self.c = v & 0x80 != 0;
                (v << 1) | c
            }
            2 => {
                self.c = v & 1 != 0;
                v >> 1
            }
            3 => {
                let c = (self.c as u8) << 7;
                self.c = v & 1 != 0;
                (v >> 1) | c
            }
            4 => v.wrapping_sub(1),
            _ => v.wrapping_add(1),
        };
        self.nz(r);
        r
    }
    fn branch(&mut self, cond: bool) -> u32 {
        let r = self.fetch() as i8;
        if cond {
            self.pc = self.pc.wrapping_add(r as i16 as u16);
            2
        } else {
            0
        }
    }
    fn bit_addr(&mut self) -> (u16, u8) {
        let w = self.fetch16();
        (w & 0x1FFF, (w >> 13) as u8)
    }
    fn ya(&self) -> u16 {
        ((self.y as u16) << 8) | self.a as u16
    }
    fn set_ya(&mut self, v: u16) {
        self.a = v as u8;
        self.y = (v >> 8) as u8;
    }

    // ------------------------------------------------------------ Ausführung
    fn step(&mut self) -> u32 {
        if self.sleeping {
            return 2;
        }
        let op = self.fetch();
        let mut cyc = CYC[op as usize] as u32;
        let col = op & 0x0F;
        let odd = op & 0x10 != 0;

        if op < 0xC0 && (4..=9).contains(&col) {
            let kind = op >> 5;
            match (col, odd) {
                (4, false) => {
                    let d = self.fetch();
                    let v = self.rd_dp(d);
                    self.a = self.alu(kind, self.a, v);
                }
                (4, true) => {
                    let d = self.fetch().wrapping_add(self.x);
                    let v = self.rd_dp(d);
                    self.a = self.alu(kind, self.a, v);
                }
                (5, false) => {
                    let ad = self.fetch16();
                    let v = self.rd(ad);
                    self.a = self.alu(kind, self.a, v);
                }
                (5, true) => {
                    let ad = self.fetch16().wrapping_add(self.x as u16);
                    let v = self.rd(ad);
                    self.a = self.alu(kind, self.a, v);
                }
                (6, false) => {
                    let v = self.rd_dp(self.x);
                    self.a = self.alu(kind, self.a, v);
                }
                (6, true) => {
                    let ad = self.fetch16().wrapping_add(self.y as u16);
                    let v = self.rd(ad);
                    self.a = self.alu(kind, self.a, v);
                }
                (7, false) => {
                    let d = self.fetch().wrapping_add(self.x);
                    let p = self.rd_dp16(d);
                    let v = self.rd(p);
                    self.a = self.alu(kind, self.a, v);
                }
                (7, true) => {
                    let d = self.fetch();
                    let p = self.rd_dp16(d).wrapping_add(self.y as u16);
                    let v = self.rd(p);
                    self.a = self.alu(kind, self.a, v);
                }
                (8, false) => {
                    let v = self.fetch();
                    self.a = self.alu(kind, self.a, v);
                }
                (8, true) => {
                    let imm = self.fetch();
                    let d = self.fetch();
                    let v = self.rd_dp(d);
                    let r = self.alu(kind, v, imm);
                    if kind != 3 {
                        self.wr_dp(d, r);
                    }
                }
                (9, false) => {
                    let s = self.fetch();
                    let sv = self.rd_dp(s);
                    let d = self.fetch();
                    let dv = self.rd_dp(d);
                    let r = self.alu(kind, dv, sv);
                    if kind != 3 {
                        self.wr_dp(d, r);
                    }
                }
                _ => {
                    let sv = self.rd_dp(self.y);
                    let dv = self.rd_dp(self.x);
                    let r = self.alu(kind, dv, sv);
                    if kind != 3 {
                        self.wr_dp(self.x, r);
                    }
                }
            }
            return cyc;
        }

        if op < 0xC0 && (col == 0xB || col == 0xC) {
            let kind = op >> 5;
            match (col, odd) {
                (0xB, false) => {
                    let d = self.fetch();
                    let v = self.rd_dp(d);
                    let r = self.shift(kind, v);
                    self.wr_dp(d, r);
                }
                (0xB, true) => {
                    let d = self.fetch().wrapping_add(self.x);
                    let v = self.rd_dp(d);
                    let r = self.shift(kind, v);
                    self.wr_dp(d, r);
                }
                (0xC, false) => {
                    let ad = self.fetch16();
                    let v = self.rd(ad);
                    let r = self.shift(kind, v);
                    self.wr(ad, r);
                }
                _ => self.a = self.shift(kind, self.a),
            }
            return cyc;
        }

        match col {
            1 => {
                // TCALL n
                let n = (op >> 4) as u16;
                self.push16(self.pc);
                self.pc = self.rd16(0xFFDE - n * 2);
                return cyc;
            }
            2 => {
                // SET1 / CLR1 d.bit
                let bit = op >> 5;
                let d = self.fetch();
                let v = self.rd_dp(d);
                let r = if !odd { v | (1 << bit) } else { v & !(1 << bit) };
                self.wr_dp(d, r);
                return cyc;
            }
            3 => {
                // BBS / BBC d.bit, rel
                let bit = op >> 5;
                let d = self.fetch();
                let v = self.rd_dp(d);
                let set = (v >> bit) & 1 != 0;
                cyc += self.branch(set != odd);
                return cyc;
            }
            _ => {}
        }

        match op {
            0x00 => {}
            0x10 => cyc += self.branch(!self.n),
            0x30 => cyc += self.branch(self.n),
            0x50 => cyc += self.branch(!self.v),
            0x70 => cyc += self.branch(self.v),
            0x90 => cyc += self.branch(!self.c),
            0xB0 => cyc += self.branch(self.c),
            0xD0 => cyc += self.branch(!self.z),
            0xF0 => cyc += self.branch(self.z),
            0x2F => cyc += self.branch(true),
            0x20 => self.p = false,
            0x40 => self.p = true,
            0x60 => self.c = false,
            0x80 => self.c = true,
            0xA0 => self.i = true,
            0xC0 => self.i = false,
            0xE0 => {
                self.v = false;
                self.h = false;
            }
            0xED => self.c = !self.c,
            // MOV Speicher <- Register
            0xC4 => {
                let d = self.fetch();
                self.wr_dp(d, self.a);
            }
            0xD4 => {
                let d = self.fetch().wrapping_add(self.x);
                self.wr_dp(d, self.a);
            }
            0xC5 => {
                let ad = self.fetch16();
                self.wr(ad, self.a);
            }
            0xD5 => {
                let ad = self.fetch16().wrapping_add(self.x as u16);
                self.wr(ad, self.a);
            }
            0xC6 => self.wr_dp(self.x, self.a),
            0xD6 => {
                let ad = self.fetch16().wrapping_add(self.y as u16);
                self.wr(ad, self.a);
            }
            0xC7 => {
                let d = self.fetch().wrapping_add(self.x);
                let p = self.rd_dp16(d);
                self.wr(p, self.a);
            }
            0xD7 => {
                let d = self.fetch();
                let p = self.rd_dp16(d).wrapping_add(self.y as u16);
                self.wr(p, self.a);
            }
            0xD8 => {
                let d = self.fetch();
                self.wr_dp(d, self.x);
            }
            0xD9 => {
                let d = self.fetch().wrapping_add(self.y);
                self.wr_dp(d, self.x);
            }
            0xC9 => {
                let ad = self.fetch16();
                self.wr(ad, self.x);
            }
            0xCB => {
                let d = self.fetch();
                self.wr_dp(d, self.y);
            }
            0xDB => {
                let d = self.fetch().wrapping_add(self.x);
                self.wr_dp(d, self.y);
            }
            0xCC => {
                let ad = self.fetch16();
                self.wr(ad, self.y);
            }
            0xAF => {
                self.wr_dp(self.x, self.a);
                self.x = self.x.wrapping_add(1);
            }
            0x8F => {
                let imm = self.fetch();
                let d = self.fetch();
                self.wr_dp(d, imm);
            }
            0xFA => {
                let s = self.fetch();
                let v = self.rd_dp(s);
                let d = self.fetch();
                self.wr_dp(d, v);
            }
            // MOV Register <- Speicher
            0xE4 => {
                let d = self.fetch();
                self.a = self.rd_dp(d);
                self.nz(self.a);
            }
            0xF4 => {
                let d = self.fetch().wrapping_add(self.x);
                self.a = self.rd_dp(d);
                self.nz(self.a);
            }
            0xE5 => {
                let ad = self.fetch16();
                self.a = self.rd(ad);
                self.nz(self.a);
            }
            0xF5 => {
                let ad = self.fetch16().wrapping_add(self.x as u16);
                self.a = self.rd(ad);
                self.nz(self.a);
            }
            0xE6 => {
                self.a = self.rd_dp(self.x);
                self.nz(self.a);
            }
            0xF6 => {
                let ad = self.fetch16().wrapping_add(self.y as u16);
                self.a = self.rd(ad);
                self.nz(self.a);
            }
            0xE7 => {
                let d = self.fetch().wrapping_add(self.x);
                let p = self.rd_dp16(d);
                self.a = self.rd(p);
                self.nz(self.a);
            }
            0xF7 => {
                let d = self.fetch();
                let p = self.rd_dp16(d).wrapping_add(self.y as u16);
                self.a = self.rd(p);
                self.nz(self.a);
            }
            0xE8 => {
                self.a = self.fetch();
                self.nz(self.a);
            }
            0xBF => {
                self.a = self.rd_dp(self.x);
                self.x = self.x.wrapping_add(1);
                self.nz(self.a);
            }
            0xF8 => {
                let d = self.fetch();
                self.x = self.rd_dp(d);
                self.nz(self.x);
            }
            0xF9 => {
                let d = self.fetch().wrapping_add(self.y);
                self.x = self.rd_dp(d);
                self.nz(self.x);
            }
            0xE9 => {
                let ad = self.fetch16();
                self.x = self.rd(ad);
                self.nz(self.x);
            }
            0xCD => {
                self.x = self.fetch();
                self.nz(self.x);
            }
            0xEB => {
                let d = self.fetch();
                self.y = self.rd_dp(d);
                self.nz(self.y);
            }
            0xFB => {
                let d = self.fetch().wrapping_add(self.x);
                self.y = self.rd_dp(d);
                self.nz(self.y);
            }
            0xEC => {
                let ad = self.fetch16();
                self.y = self.rd(ad);
                self.nz(self.y);
            }
            0x8D => {
                self.y = self.fetch();
                self.nz(self.y);
            }
            // Register-Transfers
            0x5D => {
                self.x = self.a;
                self.nz(self.x);
            }
            0x7D => {
                self.a = self.x;
                self.nz(self.a);
            }
            0xDD => {
                self.a = self.y;
                self.nz(self.a);
            }
            0xFD => {
                self.y = self.a;
                self.nz(self.y);
            }
            0x9D => {
                self.x = self.sp;
                self.nz(self.x);
            }
            0xBD => self.sp = self.x,
            0x1D => {
                self.x = self.x.wrapping_sub(1);
                self.nz(self.x);
            }
            0x3D => {
                self.x = self.x.wrapping_add(1);
                self.nz(self.x);
            }
            0xDC => {
                self.y = self.y.wrapping_sub(1);
                self.nz(self.y);
            }
            0xFC => {
                self.y = self.y.wrapping_add(1);
                self.nz(self.y);
            }
            // Vergleiche
            0xC8 => {
                let v = self.fetch();
                self.cmp(self.x, v);
            }
            0xAD => {
                let v = self.fetch();
                self.cmp(self.y, v);
            }
            0x1E => {
                let ad = self.fetch16();
                let v = self.rd(ad);
                self.cmp(self.x, v);
            }
            0x3E => {
                let d = self.fetch();
                let v = self.rd_dp(d);
                self.cmp(self.x, v);
            }
            0x5E => {
                let ad = self.fetch16();
                let v = self.rd(ad);
                self.cmp(self.y, v);
            }
            0x7E => {
                let d = self.fetch();
                let v = self.rd_dp(d);
                self.cmp(self.y, v);
            }
            // Stack
            0x0D => {
                let p = self.psw();
                self.push(p);
            }
            0x2D => self.push(self.a),
            0x4D => self.push(self.x),
            0x6D => self.push(self.y),
            0x8E => {
                let p = self.pop();
                self.set_psw(p);
            }
            0xAE => self.a = self.pop(),
            0xCE => self.x = self.pop(),
            0xEE => self.y = self.pop(),
            // Bit-Operationen
            0x0A | 0x2A | 0x4A | 0x6A | 0x8A | 0xAA => {
                let (ad, bit) = self.bit_addr();
                let m = (self.rd(ad) >> bit) & 1 != 0;
                match op {
                    0x0A => self.c |= m,
                    0x2A => self.c |= !m,
                    0x4A => self.c &= m,
                    0x6A => self.c &= !m,
                    0x8A => self.c ^= m,
                    _ => self.c = m,
                }
            }
            0xCA => {
                let (ad, bit) = self.bit_addr();
                let v = self.rd(ad);
                let r = if self.c { v | (1 << bit) } else { v & !(1 << bit) };
                self.wr(ad, r);
            }
            0xEA => {
                let (ad, bit) = self.bit_addr();
                let v = self.rd(ad);
                self.wr(ad, v ^ (1 << bit));
            }
            0x0E | 0x4E => {
                let ad = self.fetch16();
                let v = self.rd(ad);
                let t = self.a.wrapping_sub(v);
                self.nz(t);
                let r = if op == 0x0E { v | self.a } else { v & !self.a };
                self.wr(ad, r);
            }
            // 16-Bit
            0x1A | 0x3A => {
                let d = self.fetch();
                let v = self.rd_dp16(d);
                let r = if op == 0x1A { v.wrapping_sub(1) } else { v.wrapping_add(1) };
                self.wr_dp16(d, r);
                self.nz16(r);
            }
            0x5A => {
                let d = self.fetch();
                let w = self.rd_dp16(d);
                let ya = self.ya();
                self.c = ya >= w;
                self.nz16(ya.wrapping_sub(w));
            }
            0x7A => {
                let d = self.fetch();
                let w = self.rd_dp16(d);
                let ya = self.ya();
                let r = ya as u32 + w as u32;
                self.v = (!(ya ^ w) & (ya ^ r as u16) & 0x8000) != 0;
                self.h = (ya & 0xFFF) + (w & 0xFFF) > 0xFFF;
                self.c = r > 0xFFFF;
                self.set_ya(r as u16);
                self.nz16(r as u16);
            }
            0x9A => {
                let d = self.fetch();
                let w = self.rd_dp16(d);
                let ya = self.ya();
                let r = (ya as i32) - (w as i32);
                self.v = ((ya ^ w) & (ya ^ r as u16) & 0x8000) != 0;
                self.h = (ya & 0xFFF) >= (w & 0xFFF);
                self.c = r >= 0;
                self.set_ya(r as u16);
                self.nz16(r as u16);
            }
            0xBA => {
                let d = self.fetch();
                let w = self.rd_dp16(d);
                self.set_ya(w);
                self.nz16(w);
            }
            0xDA => {
                let d = self.fetch();
                let ya = self.ya();
                self.wr_dp16(d, ya);
            }
            // Arithmetik
            0xCF => {
                let r = self.y as u16 * self.a as u16;
                self.set_ya(r);
                self.nz(self.y);
            }
            0x9E => {
                let ya = self.ya() as u32;
                let x = self.x as u32;
                let y = self.y as u32;
                self.v = y >= x;
                self.h = (y & 15) >= (x & 15);
                if y < (x << 1) {
                    self.a = (ya / x) as u8;
                    self.y = (ya % x) as u8;
                } else {
                    self.a = (255 - (ya - (x << 9)) / (256 - x)) as u8;
                    self.y = (x + (ya - (x << 9)) % (256 - x)) as u8;
                }
                self.nz(self.a);
            }
            0xDF => {
                if self.c || self.a > 0x99 {
                    self.a = self.a.wrapping_add(0x60);
                    self.c = true;
                }
                if self.h || (self.a & 15) > 9 {
                    self.a = self.a.wrapping_add(6);
                }
                self.nz(self.a);
            }
            0xBE => {
                if !self.c || self.a > 0x99 {
                    self.a = self.a.wrapping_sub(0x60);
                    self.c = false;
                }
                if !self.h || (self.a & 15) > 9 {
                    self.a = self.a.wrapping_sub(6);
                }
                self.nz(self.a);
            }
            0x9F => {
                self.a = self.a.rotate_left(4);
                self.nz(self.a);
            }
            // Bedingte Schleifen
            0x2E => {
                let d = self.fetch();
                let v = self.rd_dp(d);
                cyc += self.branch(self.a != v);
            }
            0xDE => {
                let d = self.fetch().wrapping_add(self.x);
                let v = self.rd_dp(d);
                cyc += self.branch(self.a != v);
            }
            0x6E => {
                let d = self.fetch();
                let v = self.rd_dp(d).wrapping_sub(1);
                self.wr_dp(d, v);
                cyc += self.branch(v != 0);
            }
            0xFE => {
                self.y = self.y.wrapping_sub(1);
                cyc += self.branch(self.y != 0);
            }
            // Sprünge
            0x5F => self.pc = self.fetch16(),
            0x1F => {
                let p = self.fetch16().wrapping_add(self.x as u16);
                self.pc = self.rd16(p);
            }
            0x3F => {
                let t = self.fetch16();
                self.push16(self.pc);
                self.pc = t;
            }
            0x4F => {
                let u = self.fetch();
                self.push16(self.pc);
                self.pc = 0xFF00 | u as u16;
            }
            0x6F => self.pc = self.pop16(),
            0x7F => {
                let p = self.pop();
                self.set_psw(p);
                self.pc = self.pop16();
            }
            0x0F => {
                self.push16(self.pc);
                let p = self.psw();
                self.push(p);
                self.b = true;
                self.i = false;
                self.pc = self.rd16(0xFFDE);
            }
            0xEF | 0xFF => self.sleeping = true,
            _ => {}
        }
        cyc
    }
}
