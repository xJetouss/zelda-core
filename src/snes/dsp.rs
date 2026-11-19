//! S-DSP: 8 Stimmen BRR-Samples, ADSR/GAIN-Hüllkurven, Rauschen, Pitch-Modulation, Echo mit FIR.

use serde::{Deserialize, Serialize};

const RATES: [u32; 32] = [
    0, 2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 64, 48, 40, 32, 24, 20, 16, 12, 10, 8, 6, 5,
    4, 3, 2, 1,
];
const OFFSETS: [u32; 32] = [
    0, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536,
    0, 1040, 0, 0,
];

const ATTACK: u8 = 0;
const DECAY: u8 = 1;
const SUSTAIN: u8 = 2;
const RELEASE: u8 = 3;

/// Einstellungen aus dem Frontend (nicht Teil des Savestates).
#[derive(Clone, Copy, Default)]
pub struct DspCfg {
    /// 0 = Gauß-ähnlich (B-Spline, original-nah), 1 = Kubisch (klarer), 2 = Linear
    pub interp: u8,
    pub no_echo: bool,
    pub mute_mask: u8,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Voice {
    brr_addr: u16,
    brr_off: u8,
    hist: [i32; 4],
    frac: u32,
    env: i32,
    mode: u8,
    out: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Dsp {
    pub regs: Vec<u8>,
    voices: [Voice; 8],
    counter: u32,
    noise: i32,
    echo_off: u32,
    echo_len: u32,
    hist_l: [i32; 8],
    hist_r: [i32; 8],
    hist_pos: usize,
    kon: u8,
    #[serde(skip)]
    pub out: Vec<i16>,
    #[serde(skip)]
    pub cfg: DspCfg,
}

#[inline]
fn clamp16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

impl Dsp {
    pub fn new() -> Self {
        let mut regs = vec![0u8; 128];
        regs[0x6C] = 0xE0;
        Dsp {
            regs,
            voices: [Voice::default(); 8],
            counter: 0,
            noise: 0x4000,
            echo_off: 0,
            echo_len: 0,
            hist_l: [0; 8],
            hist_r: [0; 8],
            hist_pos: 0,
            kon: 0,
            out: Vec::with_capacity(4096),
            cfg: DspCfg::default(),
        }
    }

    pub fn read(&self, a: u8) -> u8 {
        self.regs[a as usize & 0x7F]
    }

    pub fn write(&mut self, a: u8, v: u8) {
        let a = a as usize & 0x7F;
        self.regs[a] = v;
        match a {
            0x4C => self.kon |= v,
            0x7C => self.regs[0x7C] = 0,
            _ => {}
        }
    }

    #[inline]
    fn event(&self, rate: usize) -> bool {
        rate != 0 && (self.counter + OFFSETS[rate]) % RATES[rate] == 0
    }

    fn envelope(&mut self, vi: usize) {
        let base = vi * 16;
        let adsr1 = self.regs[base + 5];
        let adsr2 = self.regs[base + 6];
        let gain = self.regs[base + 7];
        let v = self.voices[vi];
        let mut env = v.env;
        let mut mode = v.mode;
        if mode == RELEASE {
            env -= 8;
            if env < 0 {
                env = 0;
            }
            self.voices[vi].env = env;
            return;
        }
        let rate: usize;
        if adsr1 & 0x80 != 0 {
            if mode >= DECAY {
                env -= 1;
                env -= env >> 8;
                rate = if mode == DECAY { (((adsr1 >> 4) & 7) as usize) * 2 + 16 } else { (adsr2 & 0x1F) as usize };
            } else {
                rate = ((adsr1 & 0x0F) as usize) * 2 + 1;
                env += if rate < 31 { 0x20 } else { 0x400 };
            }
        } else if gain & 0x80 == 0 {
            env = (gain as i32 & 0x7F) * 16;
            rate = 31;
        } else {
            rate = (gain & 0x1F) as usize;
            match (gain >> 5) & 3 {
                0 => env -= 0x20,
                1 => {
                    env -= 1;
                    env -= env >> 8;
                }
                2 => env += 0x20,
                _ => env += if v.env < 0x600 { 0x20 } else { 0x08 },
            }
        }
        if !self.event(rate) {
            return;
        }
        if mode == DECAY && (env >> 8) == (adsr2 >> 5) as i32 {
            mode = SUSTAIN;
        }
        if !(0..=0x7FF).contains(&env) {
            env = if env < 0 { 0 } else { 0x7FF };
            if mode == ATTACK {
                mode = DECAY;
            }
        }
        self.voices[vi].env = env;
        self.voices[vi].mode = mode;
    }

    fn decode_next(v: &mut Voice, ram: &[u8], dir: u16, srcn: u8, endx: &mut u8, bit: u8) {
        let hdr = ram[v.brr_addr as usize];
        let byte = ram[v.brr_addr.wrapping_add(1 + (v.brr_off as u16 >> 1)) as usize];
        let nib = if v.brr_off & 1 == 0 { byte >> 4 } else { byte & 0xF } as i32;
        let nib = (nib ^ 8) - 8;
        let shift = (hdr >> 4) as i32;
        let filter = (hdr >> 2) & 3;
        let mut s = if shift <= 12 { (nib << shift) >> 1 } else if nib < 0 { -2048 } else { 0 };
        let p1 = v.hist[3];
        let p2 = v.hist[2] >> 1;
        match filter {
            1 => {
                s += p1 >> 1;
                s += (-p1) >> 5;
            }
            2 => {
                s += p1;
                s -= p2;
                s += p2 >> 4;
                s += (p1 * -3) >> 6;
            }
            3 => {
                s += p1;
                s -= p2;
                s += (p1 * -13) >> 7;
                s += (p2 * 3) >> 4;
            }
            _ => {}
        }
        let s = ((clamp16(s) << 1) as i16) as i32;
        v.hist = [v.hist[1], v.hist[2], v.hist[3], s];
        v.brr_off += 1;
        if v.brr_off == 16 {
            v.brr_off = 0;
            if hdr & 1 != 0 {
                *endx |= bit;
                if hdr & 2 == 0 {
                    v.mode = RELEASE;
                    v.env = 0;
                }
                let e = dir.wrapping_add(srcn as u16 * 4).wrapping_add(2);
                v.brr_addr = ram[e as usize] as u16 | (ram[e.wrapping_add(1) as usize] as u16) << 8;
            } else {
                v.brr_addr = v.brr_addr.wrapping_add(9);
            }
        }
    }

    fn interpolate(&self, v: &Voice) -> i32 {
        let [p0, p1, p2, p3] = v.hist;
        let t = v.frac as f32 / 4096.0;
        let r = match self.cfg.interp {
            2 => p1 as f32 + (p2 - p1) as f32 * t,
            1 => {
                let (p0, p1, p2, p3) = (p0 as f32, p1 as f32, p2 as f32, p3 as f32);
                let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
                let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                let c = -0.5 * p0 + 0.5 * p2;
                ((a * t + b) * t + c) * t + p1
            }
            _ => {
                // kubischer B-Spline: weicher Tiefpass wie die originale Gauß-Interpolation
                let t2 = t * t;
                let t3 = t2 * t;
                let w0 = (1.0 - t) * (1.0 - t) * (1.0 - t) / 6.0;
                let w1 = (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0;
                let w2 = (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0;
                let w3 = t3 / 6.0;
                p0 as f32 * w0 + p1 as f32 * w1 + p2 as f32 * w2 + p3 as f32 * w3
            }
        };
        clamp16(r as i32)
    }

    /// Ein Ausgabe-Sample (32 kHz).
    pub fn tick(&mut self, ram: &mut [u8]) {
        if self.counter == 0 {
            self.counter = 0x7800;
        }
        self.counter -= 1;

        let flg = self.regs[0x6C];
        let kon = self.kon;
        self.kon = 0;
        let koff = self.regs[0x5C];
        if self.event((flg & 0x1F) as usize) {
            let fb = (self.noise << 13) ^ (self.noise << 14);
            self.noise = (fb & 0x4000) | (self.noise >> 1);
        }
        let dir = (self.regs[0x5D] as u16) << 8;
        let pmon = self.regs[0x2D] & !1;
        let non = self.regs[0x3D];
        let eon = self.regs[0x4D];
        let mut endx = self.regs[0x7C];
        let (mut ml, mut mr, mut el, mut er) = (0i32, 0i32, 0i32, 0i32);
        let mut prev_out = 0i32;

        for vi in 0..8 {
            let base = vi * 16;
            let bit = 1u8 << vi;
            if kon & bit != 0 {
                let e = dir.wrapping_add(self.regs[base + 4] as u16 * 4);
                let v = &mut self.voices[vi];
                v.brr_addr = ram[e as usize] as u16 | (ram[e.wrapping_add(1) as usize] as u16) << 8;
                v.brr_off = 0;
                v.hist = [0; 4];
                v.frac = 0;
                v.env = 0;
                v.mode = ATTACK;
                endx &= !bit;
            } else if koff & bit != 0 {
                self.voices[vi].mode = RELEASE;
            }
            if flg & 0x80 != 0 {
                self.voices[vi].mode = RELEASE;
                self.voices[vi].env = 0;
            }

            let mut pitch = self.regs[base + 2] as i32 | ((self.regs[base + 3] as i32 & 0x3F) << 8);
            if pmon & bit != 0 {
                pitch += ((prev_out >> 5) * pitch) >> 10;
                pitch = pitch.clamp(0, 0x3FFF);
            }

            let s = if non & bit != 0 {
                ((self.noise << 1) as i16) as i32
            } else {
                let v = self.voices[vi];
                self.interpolate(&v)
            };
            self.envelope(vi);
            let env = self.voices[vi].env;
            let out = ((s * env) >> 11) & !1;
            self.voices[vi].out = out;
            self.regs[base + 8] = (env >> 4) as u8;
            self.regs[base + 9] = (out >> 8) as u8;
            prev_out = out;

            if self.cfg.mute_mask & bit == 0 {
                let l = (out * self.regs[base] as i8 as i32) >> 7;
                let r = (out * self.regs[base + 1] as i8 as i32) >> 7;
                ml = clamp16(ml + l);
                mr = clamp16(mr + r);
                if eon & bit != 0 {
                    el = clamp16(el + l);
                    er = clamp16(er + r);
                }
            }

            let srcn = self.regs[base + 4];
            let v = &mut self.voices[vi];
            v.frac += pitch as u32;
            while v.frac >= 0x1000 {
                v.frac -= 0x1000;
                Self::decode_next(v, ram, dir, srcn, &mut endx, bit);
            }
        }
        self.regs[0x7C] = endx;

        // ---- Echo
        let esa = (self.regs[0x6D] as u32) << 8;
        let addr = (esa + self.echo_off) & 0xFFFF;
        let rd16 = |ram: &[u8], a: u32| -> i32 { (ram[(a & 0xFFFF) as usize] as u16 | (ram[((a + 1) & 0xFFFF) as usize] as u16) << 8) as i16 as i32 };
        self.hist_pos = (self.hist_pos + 1) & 7;
        self.hist_l[self.hist_pos] = rd16(ram, addr) >> 1;
        self.hist_r[self.hist_pos] = rd16(ram, addr + 2) >> 1;
        let (mut fl, mut fr) = (0i32, 0i32);
        for i in 0..8 {
            let idx = (self.hist_pos + 1 + i) & 7;
            let c = self.regs[i * 16 + 0x0F] as i8 as i32;
            fl += (self.hist_l[idx] * c) >> 6;
            fr += (self.hist_r[idx] * c) >> 6;
        }
        let fl = clamp16(fl) & !1;
        let fr = clamp16(fr) & !1;

        let mvl = self.regs[0x0C] as i8 as i32;
        let mvr = self.regs[0x1C] as i8 as i32;
        let evl = self.regs[0x2C] as i8 as i32;
        let evr = self.regs[0x3C] as i8 as i32;
        let (echo_l, echo_r) = if self.cfg.no_echo { (0, 0) } else { ((fl * evl) >> 7, (fr * evr) >> 7) };
        let mut out_l = clamp16(((ml * mvl) >> 7) + echo_l);
        let mut out_r = clamp16(((mr * mvr) >> 7) + echo_r);
        if flg & 0x40 != 0 {
            out_l = 0;
            out_r = 0;
        }

        let efb = self.regs[0x0D] as i8 as i32;
        let wl = clamp16(el + ((fl * efb) >> 7)) & !1;
        let wr = clamp16(er + ((fr * efb) >> 7)) & !1;
        if flg & 0x20 == 0 {
            let a = addr as usize;
            ram[a] = wl as u8;
            ram[(a + 1) & 0xFFFF] = (wl >> 8) as u8;
            ram[(a + 2) & 0xFFFF] = wr as u8;
            ram[(a + 3) & 0xFFFF] = (wr >> 8) as u8;
        }
        if self.echo_off == 0 {
            self.echo_len = (self.regs[0x7D] as u32 & 0x0F) * 0x800;
        }
        self.echo_off += 4;
        if self.echo_off >= self.echo_len {
            self.echo_off = 0;
        }

        if self.out.len() < 1 << 16 {
            self.out.push(out_l as i16);
            self.out.push(out_r as i16);
        }
    }
}
