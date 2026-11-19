//! Eingebaute Bildfilter (HD-Hochskalierung, CRT). Läuft komplett lokal, keine Downloads.

use crate::gfx::{self, Color32};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Filter {
    /// Original-Pixel (frühere Einstellung „Zelda3“ wird hierauf gelesen)
    #[serde(alias = "Zelda3")]
    Pixel,
    /// bilinear geglättet
    Smooth,
    /// HD: xBR 4x - erkennt Kanten in alle Richtungen und zeichnet sie glatt nach
    Hd,
    /// HD Kanten: Scale4x (einfacher, behaelt harte Pixelkanten)
    HdSharp,
    /// HD xBR ohne Weichzeichnen (knackig)
    HdCrisp,
    /// HD xBR mit leichten Scanlines (Retro-HD)
    HdScan,
    /// Röhrenfernseher-Look mit Scanlines
    Crt,
    /// Clean HD: xBR-Kantenglättung ohne Zwischenfarben, scharf dargestellt – modern und sauber
    HdClean,
    /// Weiches HD: xBRZ 6× – glatte, kantengeglättete Rundungen in sehr hoher Auflösung
    HdSoft,
    /// HD Ultra: MMPX 2× + 2× (Pixel-Art-Vergrößerung, die Formen und Linien erhält), dann weich
    HdUltra,
    /// HD Max: ScaleFX 3× + rAA (Sp00kyFox) – runde, saubere Kanten, danach weich geglättet
    HdMax,
    /// HD Super: Super-xBR 4× (Hyllian) – sehr weiche, saubere Kanten
    HdSuper,
}

impl Filter {
    /// Bildmodi im Menü (die übrigen Filter werden nur noch intern genutzt, z. B. für die Weltkarte)
    pub const ALL: [Filter; 5] = [Filter::Pixel, Filter::HdSoft, Filter::HdSuper, Filter::HdMax, Filter::HdSharp];
    /// GBA: alle Upscaler zur Auswahl (Pixel-Art-Vergrößerung MMPX, xBR, xBRZ, Super-xBR, ScaleFX …)
    pub const GBA_ALL: [Filter; 12] = [Filter::Pixel, Filter::Smooth, Filter::HdSoft, Filter::HdSuper, Filter::HdUltra, Filter::HdClean, Filter::Hd, Filter::HdCrisp, Filter::HdScan, Filter::HdMax, Filter::HdSharp, Filter::Crt];
    /// alte Einstellung auf einen der fünf Bildmodi abbilden
    pub fn menu_mode(self) -> Filter {
        match self {
            Filter::Smooth | Filter::Hd | Filter::HdScan => Filter::HdSoft,
            Filter::HdUltra => Filter::HdSuper,
            Filter::HdClean | Filter::HdCrisp => Filter::HdSharp,
            Filter::Crt => Filter::Pixel,
            f => f,
        }
    }
    pub fn is_hd(self) -> bool {
        matches!(self, Filter::Hd | Filter::HdSharp | Filter::HdCrisp | Filter::HdScan | Filter::HdClean | Filter::HdSoft | Filter::HdUltra | Filter::HdMax | Filter::HdSuper)
    }
    pub fn name(self, en: bool) -> &'static str {
        match (self, en) {
            (Filter::Pixel, false) => "Original",
            (Filter::Pixel, true) => "Original",
            (Filter::Smooth, false) => "Weich (bilinear)",
            (Filter::Smooth, true) => "Smooth (bilinear)",
            (Filter::HdSoft, false) => "HD Weich",
            (Filter::HdSoft, true) => "HD Soft",
            (Filter::HdClean, false) => "Clean HD (scharf & sauber)",
            (Filter::HdClean, true) => "Clean HD (sharp & clean)",
            (Filter::Hd, false) => "HD weich (xBR 4×)",
            (Filter::Hd, true) => "HD soft (xBR 4×)",
            (Filter::HdSharp, false) => "Pixel ×4 scharf (feinere Pixel)",
            (Filter::HdSharp, true) => "Pixel ×4 sharp (finer pixels)",
            (Filter::HdCrisp, false) => "HD knackig (xBR 4×, scharf)",
            (Filter::HdCrisp, true) => "HD crisp (xBR 4×, sharp)",
            (Filter::HdScan, false) => "HD Retro (xBR + Scanlines)",
            (Filter::HdScan, true) => "HD retro (xBR + scanlines)",
            (Filter::HdUltra, false) => "HD Ultra (MMPX)",
            (Filter::HdUltra, true) => "HD Ultra (MMPX)",
            (Filter::HdMax, false) => "Pixel ×3 rund (feinere Pixel)",
            (Filter::HdMax, true) => "Pixel ×3 round (finer pixels)",
            (Filter::HdSuper, false) => "HD Super",
            (Filter::HdSuper, true) => "HD Super",
            (Filter::Crt, false) => "Röhren-TV (Scanlines)",
            (Filter::Crt, true) => "CRT (scanlines)",
        }
    }
    pub fn texture_options(self) -> gfx::TextureOptions {
        match self {
            Filter::Smooth | Filter::Hd | Filter::HdScan | Filter::Crt | Filter::HdSoft | Filter::HdUltra | Filter::HdMax | Filter::HdSuper => gfx::TextureOptions::LINEAR,
            // HD-Bilder: beim Vergrößern scharf, beim Verkleinern geglättet
            Filter::HdClean | Filter::HdCrisp | Filter::HdSharp => gfx::TextureOptions { minification: gfx::TextureFilter::Linear, ..gfx::TextureOptions::NEAREST },
            Filter::Pixel => gfx::TextureOptions::NEAREST,
        }
    }
}

/// Scale2x (EPX): verdoppelt Pixel-Art, glättet nur Diagonalen – bleibt pixelig, aber feiner.
pub fn epx2x<T: Copy + PartialEq>(src: &[T], w: usize, h: usize, outside: T) -> Vec<T> {
    let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { outside } else { src[y as usize * w + x as usize] };
    let mut out = vec![outside; w * h * 4];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let p = at(x, y);
            let (a, b, c, d) = (at(x, y - 1), at(x + 1, y), at(x - 1, y), at(x, y + 1));
            let e0 = if c == a && c != d && a != b { a } else { p };
            let e1 = if a == b && a != c && b != d { b } else { p };
            let e2 = if d == c && d != b && c != a { c } else { p };
            let e3 = if b == d && b != a && d != c { d } else { p };
            let (ox, oy, ow) = (x as usize * 2, y as usize * 2, w * 2);
            out[oy * ow + ox] = e0;
            out[oy * ow + ox + 1] = e1;
            out[(oy + 1) * ow + ox] = e2;
            out[(oy + 1) * ow + ox + 1] = e3;
        }
    }
    out
}

/// Skalierung von „HD weich“
pub const XBRZ: usize = 6;

/// xBRZ (Zenju) in Streifen auf allen CPU-Kernen; jeder Streifen bekommt 3 Zeilen Rand dazu,
/// damit an den Nahtstellen exakt dasselbe herauskommt wie in einem Stück.
pub fn xbrz_par(src: &[Color32], w: usize, h: usize, f: usize, out: &mut Vec<Color32>) {
    const PAD: usize = 3;
    let ow = w * f;
    out.clear();
    out.resize(ow * h * f, Color32::BLACK);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let band = h.div_ceil(threads).max(8);
    std::thread::scope(|sc| {
        for (i, chunk) in out.chunks_mut(band * f * ow).enumerate() {
            sc.spawn(move || {
                let y0 = i * band;
                let y1 = (y0 + band).min(h);
                let a = y0.saturating_sub(PAD);
                let b = (y1 + PAD).min(h);
                let mut rgba = Vec::with_capacity((b - a) * w * 4);
                for c in &src[a * w..b * w] {
                    rgba.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                }
                let big = xbrz::scale_rgba(&rgba, w, b - a, f);
                let skip = (y0 - a) * f * ow;
                for (o, p) in chunk.iter_mut().zip(big[skip * 4..].chunks_exact(4)) {
                    *o = Color32::from_rgb(p[0], p[1], p[2]);
                }
            });
        }
    });
}

/// Scale2x (EPX): verdoppelt die Auflösung und rundet Treppenstufen ab.
pub fn scale2x(src: &[Color32], w: usize, h: usize, dst: &mut Vec<Color32>) {
    let w2 = w * 2;
    dst.clear();
    dst.resize(w2 * h * 2, Color32::BLACK);
    for y in 0..h {
        let up = if y > 0 { y - 1 } else { y };
        let dn = if y + 1 < h { y + 1 } else { y };
        for x in 0..w {
            let e = src[y * w + x];
            let b = src[up * w + x];
            let hh = src[dn * w + x];
            let d = src[y * w + if x > 0 { x - 1 } else { x }];
            let f = src[y * w + if x + 1 < w { x + 1 } else { x }];
            let (e0, e1, e2, e3) = if b != hh && d != f {
                (
                    if d == b { d } else { e },
                    if b == f { f } else { e },
                    if d == hh { d } else { e },
                    if hh == f { f } else { e },
                )
            } else {
                (e, e, e, e)
            };
            let o = y * 2 * w2 + x * 2;
            dst[o] = e0;
            dst[o + 1] = e1;
            dst[o + w2] = e2;
            dst[o + w2 + 1] = e3;
        }
    }
}

#[inline]
fn yuv(c: Color32) -> [i32; 3] {
    let (r, g, b) = (c.r() as i32, c.g() as i32, c.b() as i32);
    [(299 * r + 587 * g + 114 * b) / 1000, (-169 * r - 331 * g + 500 * b) / 1000, (500 * r - 419 * g - 81 * b) / 1000]
}

#[inline]
fn mix(dst: Color32, src: Color32, k: u32) -> Color32 {
    let m = |a: u8, b: u8| ((a as u32 * (256 - k) + b as u32 * k) >> 8) as u8;
    Color32::from_rgb(m(dst.r(), src.r()), m(dst.g(), src.g()), m(dst.b(), src.b()))
}

/// xBR 4x (nach dem xBR-Algorithmus von Hyllian): Fuer jede der vier Ecken eines Pixels wird anhand
/// der 5x5-Umgebung entschieden, ob dort eine Kante verlaeuft (flach, steil oder diagonal),
/// und die Kante wird mit Zwischentoenen in den 4x4 Ausgabepixeln nachgezeichnet.
pub fn xbr4x(src: &[Color32], w: usize, h: usize, dst: &mut Vec<Color32>) {
    xbr4x_mode(src, w, h, dst, true);
}

/// xBR 4x; `blend = false` = "Clean HD": Kanten werden geglättet, aber ohne Zwischenfarben
/// (keine Unschärfe, saubere harte Kanten). Läuft parallel auf allen CPU-Kernen.
pub fn xbr4x_mode(src: &[Color32], w: usize, h: usize, dst: &mut Vec<Color32>, blend: bool) {
    let yuvs: Vec<[i32; 3]> = src.iter().map(|c| yuv(*c)).collect();
    let ow = w * 4;
    dst.clear();
    dst.resize(ow * h * 4, Color32::BLACK);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let band = h.div_ceil(threads).max(1);
    let yuvs = &yuvs;
    std::thread::scope(|sc| {
        for (bi, chunk) in dst.chunks_mut(ow * 4 * band).enumerate() {
            let y0 = bi * band;
            sc.spawn(move || {
                let rows = chunk.len() / (ow * 4);
                for ry in 0..rows {
                    let y = y0 + ry;
                    for x in 0..w {
                        let blk = xbr_block(src, yuvs, w, h, x, y, blend);
                        for sy in 0..4 {
                            let o = (ry * 4 + sy) * ow + x * 4;
                            chunk[o..o + 4].copy_from_slice(&blk[sy * 4..sy * 4 + 4]);
                        }
                    }
                }
            });
        }
    });
}

#[inline]
fn xbr_block(src: &[Color32], yuvs: &[[i32; 3]], w: usize, h: usize, x: usize, y: usize, blend: bool) -> [Color32; 16] {
    let (wi, hi, xi, yi) = (w as i32, h as i32, x as i32, y as i32);
    let df = |a: usize, b: usize| -> i32 {
        let (p, q) = (yuvs[a], yuvs[b]);
        48 * (p[0] - q[0]).abs() + 7 * (p[1] - q[1]).abs() + 6 * (p[2] - q[2]).abs()
    };
    let eq = |a: usize, b: usize| df(a, b) < 155;
    let mixk = |d: Color32, s: Color32, k: u32| if blend { mix(d, s, k) } else if k >= 128 { s } else { d };
    // Ausgabepositionen (Spalte, Zeile) der Referenz-Ecke unten rechts im 4x4-Block
    const N15: (i32, i32) = (3, 3);
    const N14: (i32, i32) = (2, 3);
    const N11: (i32, i32) = (3, 2);
    const N13: (i32, i32) = (1, 3);
    const N12: (i32, i32) = (0, 3);
    const N10: (i32, i32) = (2, 2);
    const N7: (i32, i32) = (3, 1);
    const N3: (i32, i32) = (3, 0);
    let at = |dx: i32, dy: i32| ((yi + dy).clamp(0, hi - 1) * wi + (xi + dx).clamp(0, wi - 1)) as usize;
    let e = src[at(0, 0)];
    let mut blk = [e; 16];
    for rot in 0..4 {
        let r = |dx: i32, dy: i32| -> (i32, i32) {
            let (mut a, mut b) = (dx, dy);
            for _ in 0..rot {
                let t = a;
                a = -b;
                b = t;
            }
            (a, b)
        };
        let n = |dx: i32, dy: i32| {
            let (a, b) = r(dx, dy);
            at(a, b)
        };
        let sub = |p: (i32, i32)| -> usize {
            let (u, v) = r(2 * p.0 - 3, 2 * p.1 - 3);
            (((v + 3) / 2) * 4 + (u + 3) / 2) as usize
        };
        let (pe, pi, ph, pf, pg, pc, pd, pb) = (n(0, 0), n(1, 1), n(0, 1), n(1, 0), n(-1, 1), n(1, -1), n(-1, 0), n(0, -1));
        if src[pe] == src[ph] || src[pe] == src[pf] {
            continue;
        }
        let (h5, i5, f4, i4) = (n(0, 2), n(1, 2), n(2, 0), n(2, 1));
        let ev = df(pe, pc) + df(pe, pg) + df(pi, h5) + df(pi, f4) + (df(ph, pf) << 2);
        let iv = df(ph, pd) + df(ph, i5) + df(pf, i4) + df(pf, pb) + (df(pe, pi) << 2);
        let px = if df(pe, pf) <= df(pe, ph) { src[pf] } else { src[ph] };
        let cond = (!eq(pf, pb) && !eq(ph, pd)) || (eq(pe, pi) && !eq(pf, i4) && !eq(ph, i5)) || eq(pe, pg) || eq(pe, pc);
        if ev < iv && cond {
            let ke = df(pf, pg);
            let ki = df(ph, pc);
            let ex2 = src[pe] != src[pc] && src[pb] != src[pc];
            let ex3 = src[pe] != src[pg] && src[pd] != src[pg];
            let left = (ke << 1) <= ki && ex3;
            let up = ke >= (ki << 1) && ex2;
            if left && up {
                let b13 = mixk(blk[sub(N13)], px, 192);
                let b12 = mixk(blk[sub(N12)], px, 64);
                blk[sub(N13)] = b13;
                blk[sub(N12)] = b12;
                blk[sub(N15)] = px;
                blk[sub(N14)] = px;
                blk[sub(N11)] = px;
                blk[sub(N10)] = b12;
                blk[sub(N3)] = b12;
                blk[sub(N7)] = b13;
            } else if left {
                blk[sub(N15)] = px;
                blk[sub(N14)] = px;
                blk[sub(N11)] = mixk(blk[sub(N11)], px, 192);
                blk[sub(N13)] = mixk(blk[sub(N13)], px, 192);
                blk[sub(N10)] = mixk(blk[sub(N10)], px, 64);
                blk[sub(N12)] = mixk(blk[sub(N12)], px, 64);
            } else if up {
                blk[sub(N15)] = px;
                blk[sub(N11)] = px;
                blk[sub(N14)] = mixk(blk[sub(N14)], px, 192);
                blk[sub(N7)] = mixk(blk[sub(N7)], px, 192);
                blk[sub(N10)] = mixk(blk[sub(N10)], px, 64);
                blk[sub(N3)] = mixk(blk[sub(N3)], px, 64);
            } else {
                blk[sub(N11)] = mixk(blk[sub(N11)], px, 128);
                blk[sub(N14)] = mixk(blk[sub(N14)], px, 128);
                blk[sub(N15)] = px;
            }
        } else if ev <= iv && blend {
            blk[sub(N15)] = mix(blk[sub(N15)], px, 128);
        }
    }
    blk
}

/// Wendet den Filter an, gibt (Breite, Höhe) zurück.
pub fn apply(f: Filter, src: &[Color32], w: usize, h: usize, tmp: &mut Vec<Color32>, out: &mut Vec<Color32>) -> [usize; 2] {
    match f {
        Filter::Pixel | Filter::Smooth => {
            out.clear();
            out.extend_from_slice(src);
            [w, h]
        }
        Filter::Hd | Filter::HdCrisp => {
            xbr4x(src, w, h, out);
            [w * 4, h * 4]
        }
        Filter::HdClean => {
            xbr4x_mode(src, w, h, out, false);
            [w * 4, h * 4]
        }
        // ohne Grafikkarte: wie „HD weich“
        Filter::HdSoft | Filter::HdUltra | Filter::HdMax | Filter::HdSuper => {
            xbrz_par(src, w, h, XBRZ, out);
            [w * XBRZ, h * XBRZ]
        }
        Filter::HdScan => {
            xbr4x(src, w, h, out);
            let ow = w * 4;
            for y in (3..h * 4).step_by(4) {
                for p in &mut out[y * ow..(y + 1) * ow] {
                    *p = Color32::from_rgb((p.r() as u32 * 70 / 100) as u8, (p.g() as u32 * 70 / 100) as u8, (p.b() as u32 * 70 / 100) as u8);
                }
            }
            [w * 4, h * 4]
        }
        Filter::HdSharp => {
            scale2x(src, w, h, tmp);
            scale2x(tmp, w * 2, h * 2, out);
            [w * 4, h * 4]
        }
        Filter::Crt => {
            let w2 = w * 2;
            out.clear();
            out.resize(w2 * h * 3, Color32::BLACK);
            for y in 0..h {
                for x in 0..w {
                    let c = src[y * w + x];
                    let dim = Color32::from_rgb((c.r() as u32 * 45 / 100) as u8, (c.g() as u32 * 45 / 100) as u8, (c.b() as u32 * 45 / 100) as u8);
                    for k in 0..2 {
                        out[(y * 3) * w2 + x * 2 + k] = c;
                        out[(y * 3 + 1) * w2 + x * 2 + k] = c;
                        out[(y * 3 + 2) * w2 + x * 2 + k] = dim;
                    }
                }
            }
            [w2, h * 3]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hd_is_4x() {
        let src = vec![Color32::RED; 256 * 224];
        let (mut t, mut o) = (Vec::new(), Vec::new());
        assert_eq!(apply(Filter::Hd, &src, 256, 224, &mut t, &mut o), [1024, 896]);
        assert_eq!(o.len(), 1024 * 896);
        assert!(o.iter().all(|c| *c == Color32::RED));
    }
}
