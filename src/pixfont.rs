//! Kleine Pixel-Schrift (5×7, Großbuchstaben, Ziffern, Umlaute) für Einblendungen im Pixel-Modus.

use crate::gfx::{self, Color32, Pos2};

/// 5×7-Raster eines Zeichens (Bit 4 = linke Spalte)
pub fn glyph_bits(c: char) -> [u8; 7] {
    glyph(c)
}

fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        'Ä' | 'ä' => [10, 0, 14, 17, 31, 17, 17],
        'Ö' | 'ö' => [10, 0, 14, 17, 17, 17, 14],
        'Ü' | 'ü' => [10, 0, 17, 17, 17, 17, 14],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '%' => [25, 26, 2, 4, 8, 11, 19],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '<' | '◀' => [2, 6, 14, 30, 14, 6, 2],
        '>' | '▶' => [8, 12, 14, 15, 14, 12, 8],
        '×' => [0, 17, 10, 4, 10, 17, 0],
        '·' => [0, 0, 0, 12, 12, 0, 0],
        '♥' => [0, 10, 31, 31, 14, 4, 0],
        '*' | '✨' => [4, 21, 14, 31, 14, 21, 4],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        _ => [0; 7],
    }
}

/// Sonderzeichen in darstellbare Zeichen umwandeln (⏩ -> ▶▶ usw.)
fn expand(text: &str) -> Vec<char> {
    text.chars()
        .flat_map(|c| match c {
            'ß' => vec!['S', 'S'],
            '–' | '—' => vec!['-'],
            '&' => vec!['+'],
            '…' => vec!['.', '.', '.'],
            '↑' | '▲' => vec!['◀'],
            '↓' | '▼' => vec!['▶'],
            'É' | 'é' => vec!['E'],
            '⏩' => vec!['▶', '▶'],
            '⏪' => vec!['◀', '◀'],
            '⏸' => vec!['|', '|'],
            c => vec![c],
        })
        .collect()
}

/// Feine Pixel (HD Max): Schrift und Symbole mit doppelt so vielen, halb so großen Pixeln (Scale2x)
static FINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_fine(on: bool) {
    FINE.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn fine() -> bool {
    FINE.load(std::sync::atomic::Ordering::Relaxed)
}

/// HD: dieselbe Schrift, aber mit geglätteten Kanten (wie die Spielgrafik im HD-Modus)
static SMOOTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_smooth(on: bool) {
    SMOOTH.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn smooth() -> bool {
    SMOOTH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Umlaut: voller Großbuchstabe, die Punkte werden darüber gesetzt (sonst sieht „Ü“ wie ein kleines „ü“ aus)
fn base(c: char) -> (char, bool) {
    match c {
        'Ä' | 'ä' => ('A', true),
        'Ö' | 'ö' => ('O', true),
        'Ü' | 'ü' => ('U', true),
        c => (c, false),
    }
}

/// Zeichen als 10×14-Raster (Scale2x der 5×7-Form)
fn glyph_fine(c: char) -> Vec<bool> {
    let g = glyph(c);
    let src: Vec<bool> = (0..7).flat_map(|r| (0..5).map(move |k| g[r] & (16 >> k) != 0)).collect();
    crate::filters::epx2x(&src, 5, 7, false)
}

/// Breite eines Texts in Bildschirmpixeln bei Pixelgröße `px`.
pub fn width(text: &str, px: f32) -> f32 {
    let n = expand(text).len();
    (n as f32 * 6.0 - 1.0).max(0.0) * px
}

/// Zeichnet Pixel-Text mit Schatten wie im Spiel. `align`: Ausrichtung relativ zu `pos`.
pub fn text(p: &gfx::Painter, pos: Pos2, align: gfx::Align2, text: &str, px: f32, col: Color32) -> gfx::Rect {
    // auf echte Bildschirmpixel runden (sonst ungleich breite, unscharfe Pixel bei 125 %/150 % Skalierung)
    let ppp = p.ctx().pixels_per_point();
    // geglättete Schrift (HD) braucht kein Pixelraster: stufenlose Größe
    let px = if smooth() { px.max(1.0) } else { ((px.max(1.0) * ppp).round() / ppp).max(1.0 / ppp) };
    // feine Pixel: Pixelgröße auf eine gerade Zahl Bildschirmpixel runden (vor dem Ausrichten),
    // sonst würde das halbe Pixel aufgerundet und die Buchstaben überlappen
    let px = if fine() && !smooth() && px * ppp >= 2.0 { (((px * ppp) / 2.0).floor() * 2.0).max(2.0) / ppp } else { px };
    let size = gfx::vec2(width(text, px), 7.0 * px);
    let rect = align.anchor_size(pos, size);
    let (x0, y0) = ((rect.left() * ppp).round() / ppp, (rect.top() * ppp).round() / ppp);
    let mut x;
    let chars: Vec<char> = expand(text);
    // HD: echte Schrift (Lilita One) mit dunklem Rand, eingepasst in den Platz der Pixel-Schrift
    if smooth() {
        let text_s: String = text.chars().map(|c| if c == '⏩' { '»' } else if c == '⏪' { '«' } else { c }).collect();
        // große Umlaute: Punkte ragen über die Großbuchstaben hinaus -> etwas kleiner und tiefer setzen,
        // damit sie nicht aus Kästen herausschauen
        let umlaut = text_s.chars().any(|c| matches!(c, 'Ä' | 'Ö' | 'Ü'));
        let mut size = 10.0 * px * if umlaut { 0.86 } else { 1.0 };
        let mut g = p.layout_no_wrap(text_s.clone(), gfx::FontId::new(size, gfx::FontFamily::Name("game".into())), col);
        let maxw = rect.width().max(px) * 1.08;
        if g.size().x > maxw {
            size *= maxw / g.size().x;
            g = p.layout_no_wrap(text_s.clone(), gfx::FontId::new(size, gfx::FontFamily::Name("game".into())), col);
        }
        let gs = g.size();
        let gx = match align.x() {
            gfx::Align::Min => rect.left(),
            gfx::Align::Max => rect.right() - gs.x,
            gfx::Align::Center => rect.center().x - gs.x * 0.5,
        };
        let at = gfx::pos2(gx, rect.center().y - gs.y * if umlaut { 0.46 } else { 0.52 });
        let dark = Color32::from_rgb(16, 16, 32);
        let gd = p.layout_no_wrap(text_s, gfx::FontId::new(size, gfx::FontFamily::Name("game".into())), dark);
        // Schatten, dann Umriss, dann Text – nur bei heller Schrift (dunkle Schrift auf hellen Tasten bleibt sauber)
        let light = col.r() as u32 * 3 + col.g() as u32 * 6 + col.b() as u32 > 900;
        if light {
            let o = (size * 0.06).max(1.0);
            p.galley(at + gfx::vec2(o * 1.6, o * 1.8), gd.clone(), dark);
            for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7)] {
                p.galley(at + gfx::vec2(dx * o, dy * o), gd.clone(), dark);
            }
        }
        p.galley(at, g, col);
        let _ = x0;
        return rect;
    }
    // feine Pixel nur, wenn ein halbes Pixel noch mindestens ein Bildschirmpixel ist.
    // Pixelgröße gerade halten: sonst würde das halbe Pixel aufgerundet und die Buchstaben überlappen.
    let pp = (px * ppp).round();
    if fine() && pp >= 2.0 {
        let hp = pp / 2.0 / ppp;
        for pass in 0..2 {
            x = x0;
            let (dx, c) = if pass == 0 { (hp, Color32::from_rgb(16, 16, 32)) } else { (0.0, col) };
            for ch in &chars {
                let (b, dots) = base(*ch);
                if dots {
                    for k in [1.0f32, 3.0] {
                        p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(x + k * px + dx, y0 - 1.0 * px + dx), gfx::vec2(px, px)), 0.0, c);
                    }
                }
                let g = glyph_fine(b);
                for row in 0..14 {
                    for colb in 0..10 {
                        if g[row * 10 + colb] {
                            let r = gfx::Rect::from_min_size(gfx::pos2(x + colb as f32 * hp + dx, y0 + row as f32 * hp + dx), gfx::vec2(hp, hp));
                            p.rect_filled(r, 0.0, c);
                        }
                    }
                }
                x += 6.0 * px;
            }
        }
        return rect;
    }
    for pass in 0..2 {
        x = x0;
        let (dx, c) = if pass == 0 { (px, Color32::from_rgb(16, 16, 32)) } else { (0.0, col) };
        for ch in &chars {
            let (b, dots) = base(*ch);
            if dots {
                for k in [1.0f32, 3.0] {
                    p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(x + k * px + dx, y0 - 1.0 * px + dx), gfx::vec2(px, px)), 0.0, c);
                }
            }
            let g = glyph(b);
            for (row, bits) in g.iter().enumerate() {
                for colb in 0..5 {
                    if bits & (16 >> colb) != 0 {
                        let r = gfx::Rect::from_min_size(gfx::pos2(x + colb as f32 * px + dx, y0 + row as f32 * px + dx), gfx::vec2(px, px));
                        p.rect_filled(r, 0.0, c);
                    }
                }
            }
            x += 6.0 * px;
        }
    }
    rect
}
