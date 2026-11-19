//! Zeichen- und Eingabeschicht über GPUI für alles, was selbst gezeichnet wird
//! (Spielbild, Karte, Inventar, Schnellmenü, Einblendungen).
//!
//! Die Spiel-Oberflächen werden Bild für Bild „immediate“ beschrieben: Ein [`Painter`] sammelt
//! Rechtecke, Linien, Texte und Bilder, ein GPUI-`canvas` zeichnet sie danach. Eingaben (Maus,
//! Tastatur, Mausrad, abgelegte Dateien) sammelt GPUI zwischen zwei Bildern; [`Context::input`]
//! liefert sie gebündelt für das aktuelle Bild.

use gpui::{
    point, px, size, App as GApp, Background, Bounds, ContentMask, Corners, Edges, Font, FontFeatures, FontStyle, FontWeight, Hitbox, HitboxBehavior, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Path, Pixels, RenderImage, Rgba, ScrollWheelEvent, ShapedLine, SharedString, TextRun, Window, WindowTextSystem, BorderStyle, DispatchPhase,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ---------------------------------------------------------------- Farben

/// RGBA-Farbe (8 Bit je Kanal, Alpha nicht vormultipliziert)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Color32(pub [u8; 4]);

impl Color32 {
    pub const TRANSPARENT: Color32 = Color32([0, 0, 0, 0]);
    pub const BLACK: Color32 = Color32([0, 0, 0, 255]);
    pub const WHITE: Color32 = Color32([255, 255, 255, 255]);
    pub const GRAY: Color32 = Color32([160, 160, 160, 255]);
    pub const LIGHT_GRAY: Color32 = Color32([220, 220, 220, 255]);
    pub const DARK_GRAY: Color32 = Color32([96, 96, 96, 255]);
    pub const RED: Color32 = Color32([255, 0, 0, 255]);
    pub const LIGHT_RED: Color32 = Color32([255, 128, 128, 255]);
    pub const YELLOW: Color32 = Color32([255, 255, 0, 255]);
    pub const LIGHT_BLUE: Color32 = Color32([173, 216, 230, 255]);
    pub const BLUE: Color32 = Color32([0, 0, 255, 255]);
    pub const GREEN: Color32 = Color32([0, 255, 0, 255]);

    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color32([r, g, b, 255])
    }
    #[inline]
    pub const fn from_gray(l: u8) -> Self {
        Color32([l, l, l, 255])
    }
    #[inline]
    pub const fn from_black_alpha(a: u8) -> Self {
        Color32([0, 0, 0, a])
    }
    #[inline]
    pub const fn from_white_alpha(a: u8) -> Self {
        Color32([255, 255, 255, a])
    }
    #[inline]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color32([r, g, b, a])
    }
    #[inline]
    pub const fn r(self) -> u8 {
        self.0[0]
    }
    #[inline]
    pub const fn g(self) -> u8 {
        self.0[1]
    }
    #[inline]
    pub const fn b(self) -> u8 {
        self.0[2]
    }
    #[inline]
    pub const fn a(self) -> u8 {
        self.0[3]
    }
    /// Deckkraft (und damit Helligkeit über dem Hintergrund) mit `f` multiplizieren
    pub fn gamma_multiply(self, f: f32) -> Self {
        Color32([self.0[0], self.0[1], self.0[2], (self.0[3] as f32 * f.clamp(0.0, 1.0)).round() as u8])
    }
    pub fn to_hsla(self) -> Hsla {
        Rgba { r: self.0[0] as f32 / 255.0, g: self.0[1] as f32 / 255.0, b: self.0[2] as f32 / 255.0, a: self.0[3] as f32 / 255.0 }.into()
    }
}

impl From<Color32> for Hsla {
    fn from(c: Color32) -> Hsla {
        c.to_hsla()
    }
}

// ---------------------------------------------------------------- Geometrie

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Pos2 {
    pub x: f32,
    pub y: f32,
}

#[inline]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}
#[inline]
pub const fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
    pub fn normalized(self) -> Vec2 {
        let l = self.length();
        if l == 0.0 {
            self
        } else {
            self / l
        }
    }
    pub fn to_pos2(self) -> Pos2 {
        pos2(self.x, self.y)
    }
    pub fn round(self) -> Vec2 {
        vec2(self.x.round(), self.y.round())
    }
    pub fn floor(self) -> Vec2 {
        vec2(self.x.floor(), self.y.floor())
    }
    pub fn abs(self) -> Vec2 {
        vec2(self.x.abs(), self.y.abs())
    }
    pub fn min(self, o: Vec2) -> Vec2 {
        vec2(self.x.min(o.x), self.y.min(o.y))
    }
    pub fn max(self, o: Vec2) -> Vec2 {
        vec2(self.x.max(o.x), self.y.max(o.y))
    }
    pub fn clamp(self, a: Vec2, b: Vec2) -> Vec2 {
        vec2(self.x.clamp(a.x, b.x), self.y.clamp(a.y, b.y))
    }
}

impl Pos2 {
    pub const ZERO: Pos2 = pos2(0.0, 0.0);
    pub fn to_vec2(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub fn distance(self, o: Pos2) -> f32 {
        (self - o).length()
    }
    pub fn distance_sq(self, o: Pos2) -> f32 {
        (self - o).length_sq()
    }
    pub fn round(self) -> Pos2 {
        pos2(self.x.round(), self.y.round())
    }
    pub fn floor(self) -> Pos2 {
        pos2(self.x.floor(), self.y.floor())
    }
    pub fn min(self, o: Pos2) -> Pos2 {
        pos2(self.x.min(o.x), self.y.min(o.y))
    }
    pub fn max(self, o: Pos2) -> Pos2 {
        pos2(self.x.max(o.x), self.y.max(o.y))
    }
    pub fn clamp(self, a: Pos2, b: Pos2) -> Pos2 {
        pos2(self.x.clamp(a.x, b.x), self.y.clamp(a.y, b.y))
    }
}

macro_rules! ops2 {
    ($t:ident) => {
        impl Mul<f32> for $t {
            type Output = $t;
            fn mul(self, k: f32) -> $t {
                $t { x: self.x * k, y: self.y * k }
            }
        }
        impl Div<f32> for $t {
            type Output = $t;
            fn div(self, k: f32) -> $t {
                $t { x: self.x / k, y: self.y / k }
            }
        }
        impl MulAssign<f32> for $t {
            fn mul_assign(&mut self, k: f32) {
                self.x *= k;
                self.y *= k;
            }
        }
        impl Add<Vec2> for $t {
            type Output = $t;
            fn add(self, o: Vec2) -> $t {
                $t { x: self.x + o.x, y: self.y + o.y }
            }
        }
        impl AddAssign<Vec2> for $t {
            fn add_assign(&mut self, o: Vec2) {
                self.x += o.x;
                self.y += o.y;
            }
        }
        impl SubAssign<Vec2> for $t {
            fn sub_assign(&mut self, o: Vec2) {
                self.x -= o.x;
                self.y -= o.y;
            }
        }
    };
}
ops2!(Vec2);
ops2!(Pos2);

impl Sub<Vec2> for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Sub<Vec2> for Pos2 {
    type Output = Pos2;
    fn sub(self, o: Vec2) -> Pos2 {
        pos2(self.x - o.x, self.y - o.y)
    }
}
impl Sub<Pos2> for Pos2 {
    type Output = Vec2;
    fn sub(self, o: Pos2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}
impl Mul<Vec2> for Vec2 {
    type Output = Vec2;
    fn mul(self, o: Vec2) -> Vec2 {
        vec2(self.x * o.x, self.y * o.y)
    }
}
impl Mul<Vec2> for f32 {
    type Output = Vec2;
    fn mul(self, o: Vec2) -> Vec2 {
        o * self
    }
}
impl Div<Vec2> for Vec2 {
    type Output = Vec2;
    fn div(self, o: Vec2) -> Vec2 {
        vec2(self.x / o.x, self.y / o.y)
    }
}
impl From<[f32; 2]> for Vec2 {
    fn from(a: [f32; 2]) -> Vec2 {
        vec2(a[0], a[1])
    }
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Rect {
    pub min: Pos2,
    pub max: Pos2,
}

impl Rect {
    pub const NOTHING: Rect = Rect { min: pos2(f32::INFINITY, f32::INFINITY), max: pos2(-f32::INFINITY, -f32::INFINITY) };
    pub const EVERYTHING: Rect = Rect { min: pos2(-f32::INFINITY, -f32::INFINITY), max: pos2(f32::INFINITY, f32::INFINITY) };
    pub fn from_min_max(min: Pos2, max: Pos2) -> Rect {
        Rect { min, max }
    }
    pub fn from_min_size(min: Pos2, s: Vec2) -> Rect {
        Rect { min, max: min + s }
    }
    pub fn from_center_size(c: Pos2, s: Vec2) -> Rect {
        Rect { min: c - s * 0.5, max: c + s * 0.5 }
    }
    pub fn from_two_pos(a: Pos2, b: Pos2) -> Rect {
        Rect { min: a.min(b), max: a.max(b) }
    }
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    pub fn center(&self) -> Pos2 {
        pos2((self.min.x + self.max.x) * 0.5, (self.min.y + self.max.y) * 0.5)
    }
    pub fn left(&self) -> f32 {
        self.min.x
    }
    pub fn right(&self) -> f32 {
        self.max.x
    }
    pub fn top(&self) -> f32 {
        self.min.y
    }
    pub fn bottom(&self) -> f32 {
        self.max.y
    }
    pub fn left_top(&self) -> Pos2 {
        self.min
    }
    pub fn right_top(&self) -> Pos2 {
        pos2(self.max.x, self.min.y)
    }
    pub fn left_bottom(&self) -> Pos2 {
        pos2(self.min.x, self.max.y)
    }
    pub fn right_bottom(&self) -> Pos2 {
        self.max
    }
    pub fn center_top(&self) -> Pos2 {
        pos2(self.center().x, self.min.y)
    }
    pub fn center_bottom(&self) -> Pos2 {
        pos2(self.center().x, self.max.y)
    }
    pub fn left_center(&self) -> Pos2 {
        pos2(self.min.x, self.center().y)
    }
    pub fn right_center(&self) -> Pos2 {
        pos2(self.max.x, self.center().y)
    }
    pub fn contains(&self, p: Pos2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn shrink(&self, a: f32) -> Rect {
        self.shrink2(vec2(a, a))
    }
    pub fn shrink2(&self, a: Vec2) -> Rect {
        Rect { min: self.min + a, max: self.max - a }
    }
    pub fn expand(&self, a: f32) -> Rect {
        self.shrink(-a)
    }
    pub fn translate(&self, v: Vec2) -> Rect {
        Rect { min: self.min + v, max: self.max + v }
    }
    pub fn intersect(&self, o: Rect) -> Rect {
        Rect { min: self.min.max(o.min), max: self.max.min(o.max) }
    }
    pub fn intersects(&self, o: Rect) -> bool {
        self.min.x <= o.max.x && o.min.x <= self.max.x && self.min.y <= o.max.y && o.min.y <= self.max.y
    }
    pub fn union(&self, o: Rect) -> Rect {
        Rect { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn is_positive(&self) -> bool {
        self.width() > 0.0 && self.height() > 0.0
    }
    fn to_bounds(self) -> Bounds<Pixels> {
        Bounds { origin: point(px(self.min.x), px(self.min.y)), size: size(px(self.width().max(0.0)), px(self.height().max(0.0))) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Min,
    Center,
    Max,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Align2(pub [Align; 2]);

impl Align2 {
    pub const LEFT_TOP: Align2 = Align2([Align::Min, Align::Min]);
    pub const LEFT_CENTER: Align2 = Align2([Align::Min, Align::Center]);
    pub const LEFT_BOTTOM: Align2 = Align2([Align::Min, Align::Max]);
    pub const CENTER_TOP: Align2 = Align2([Align::Center, Align::Min]);
    pub const CENTER_CENTER: Align2 = Align2([Align::Center, Align::Center]);
    pub const CENTER_BOTTOM: Align2 = Align2([Align::Center, Align::Max]);
    pub const RIGHT_TOP: Align2 = Align2([Align::Max, Align::Min]);
    pub const RIGHT_CENTER: Align2 = Align2([Align::Max, Align::Center]);
    pub const RIGHT_BOTTOM: Align2 = Align2([Align::Max, Align::Max]);

    pub fn x(self) -> Align {
        self.0[0]
    }

    /// Rechteck der Größe `s`, das mit diesem Ankerpunkt auf `p` liegt
    pub fn anchor_size(self, p: Pos2, s: Vec2) -> Rect {
        let x = match self.0[0] {
            Align::Min => p.x,
            Align::Center => p.x - s.x * 0.5,
            Align::Max => p.x - s.x,
        };
        let y = match self.0[1] {
            Align::Min => p.y,
            Align::Center => p.y - s.y * 0.5,
            Align::Max => p.y - s.y,
        };
        Rect::from_min_size(pos2(x, y), s)
    }
}

// ---------------------------------------------------------------- Formen, Schrift

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
}

impl Stroke {
    pub const NONE: Stroke = Stroke { width: 0.0, color: Color32::TRANSPARENT };
    pub fn new(width: impl Into<f32>, color: impl Into<Color32>) -> Stroke {
        Stroke { width: width.into(), color: color.into() }
    }
    fn visible(&self) -> bool {
        self.width > 0.0 && self.color.a() > 0
    }
}

impl From<(f32, Color32)> for Stroke {
    fn from((w, c): (f32, Color32)) -> Stroke {
        Stroke::new(w, c)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum FontFamily {
    Proportional,
    Monospace,
    Name(Arc<str>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct FontId {
    pub size: f32,
    pub family: FontFamily,
}

impl FontId {
    pub fn new(size: f32, family: FontFamily) -> FontId {
        FontId { size, family }
    }
    pub fn proportional(size: f32) -> FontId {
        FontId { size, family: FontFamily::Proportional }
    }
    pub fn monospace(size: f32) -> FontId {
        FontId { size, family: FontFamily::Monospace }
    }
}

/// Schriftart der Oberfläche
pub const UI_FONT: &str = "Segoe UI";
/// Schriftart für Zahlen und Codes
pub const MONO_FONT: &str = "Consolas";
/// Spielschrift (Lilita One, liegt in assets/fonts)
pub const GAME_FONT: &str = "Lilita One";

pub fn gpui_font(family: &FontFamily) -> Font {
    let name = match family {
        FontFamily::Proportional => UI_FONT,
        FontFamily::Monospace => MONO_FONT,
        FontFamily::Name(n) if &**n == "game" => GAME_FONT,
        FontFamily::Name(_) => UI_FONT,
    };
    Font { family: name.into(), features: FontFeatures::default(), fallbacks: None, weight: FontWeight::NORMAL, style: FontStyle::Normal }
}

/// Zeilenhöhe für eine Schriftgröße
fn row_height(f: &FontId) -> f32 {
    match f.family {
        FontFamily::Name(_) => f.size * 1.15,
        _ => f.size * 1.3,
    }
}

/// Gesetzter Text (eine Zeile oder mehrere)
pub struct Galley {
    lines: Vec<ShapedLine>,
    line_h: f32,
    size: Vec2,
}

impl Galley {
    pub fn size(&self) -> Vec2 {
        self.size
    }
}

#[derive(Clone, Copy, Default)]
pub struct Vertex {
    pub pos: Pos2,
    pub color: Color32,
}

#[derive(Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn colored_vertex(&mut self, pos: Pos2, color: Color32) {
        self.vertices.push(Vertex { pos, color });
    }
    pub fn add_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }
}

pub enum Shape {
    Polygon { points: Vec<Pos2>, fill: Color32, stroke: Stroke, closed: bool },
    Segment { points: [Pos2; 2], stroke: Stroke },
    Mesh(Mesh),
}

impl Shape {
    pub fn convex_polygon(points: Vec<Pos2>, fill: impl Into<Color32>, stroke: impl Into<Stroke>) -> Shape {
        Shape::Polygon { points, fill: fill.into(), stroke: stroke.into(), closed: true }
    }
    pub fn closed_line(points: Vec<Pos2>, stroke: impl Into<Stroke>) -> Shape {
        Shape::Polygon { points, fill: Color32::TRANSPARENT, stroke: stroke.into(), closed: true }
    }
    pub fn line_segment(points: [Pos2; 2], stroke: impl Into<Stroke>) -> Shape {
        Shape::Segment { points, stroke: stroke.into() }
    }
    pub fn mesh(m: Mesh) -> Shape {
        Shape::Mesh(m)
    }
}

// ---------------------------------------------------------------- Texturen

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextureOptions {
    pub magnification: TextureFilter,
    pub minification: TextureFilter,
}

impl TextureOptions {
    pub const LINEAR: TextureOptions = TextureOptions { magnification: TextureFilter::Linear, minification: TextureFilter::Linear };
    pub const NEAREST: TextureOptions = TextureOptions { magnification: TextureFilter::Nearest, minification: TextureFilter::Nearest };
}

/// Bild im Arbeitsspeicher (Zeilen von oben nach unten)
#[derive(Clone, Default)]
pub struct ColorImage {
    pub size: [usize; 2],
    pub pixels: Vec<Color32>,
}

impl ColorImage {
    pub fn from_rgba_unmultiplied(size: [usize; 2], rgba: &[u8]) -> ColorImage {
        ColorImage { size, pixels: rgba.chunks_exact(4).map(|p| Color32([p[0], p[1], p[2], p[3]])).collect() }
    }
}

/// GPUI-Bilder, die nicht mehr gebraucht werden: im nächsten Bild aus dem Grafikspeicher
/// jedes Fensters entfernen
static GARBAGE: Mutex<Vec<(u8, Arc<RenderImage>)>> = Mutex::new(Vec::new());
/// Fenster, die Bilder zeichnen (Bit je Fenster)
static WINDOWS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn retire(img: Arc<RenderImage>) {
    let mask = WINDOWS.load(std::sync::atomic::Ordering::Relaxed);
    if mask != 0 {
        if let Ok(mut g) = GARBAGE.lock() {
            g.push((mask, img));
        }
    }
}

struct TexData {
    w: usize,
    h: usize,
    pixels: Vec<Color32>,
    opts: TextureOptions,
    /// fertige GPUI-Bilder je (Vergrößerung, Tönung)
    cache: HashMap<(u8, Color32), Arc<RenderImage>>,
}

impl TexData {
    fn clear(&mut self) {
        for (_, img) in self.cache.drain() {
            retire(img);
        }
    }

    /// BGRA-Bild mit ganzzahliger Vergrößerung `k` (Nächster-Nachbar) und Tönung
    fn variant(&mut self, k: u8, tint: Color32) -> Option<Arc<RenderImage>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        if let Some(i) = self.cache.get(&(k, tint)) {
            return Some(i.clone());
        }
        let k = k.max(1) as usize;
        let (w, h) = (self.w * k, self.h * k);
        let mut buf = vec![0u8; w * h * 4];
        let t = tint.0;
        let white = tint == Color32::WHITE;
        for y in 0..self.h {
            let row = &self.pixels[y * self.w..(y + 1) * self.w];
            for (x, c) in row.iter().enumerate() {
                let c = c.0;
                let px = if white {
                    [c[2], c[1], c[0], c[3]]
                } else {
                    let m = |a: u8, b: u8| ((a as u32 * b as u32 + 127) / 255) as u8;
                    [m(c[2], t[2]), m(c[1], t[1]), m(c[0], t[0]), m(c[3], t[3])]
                };
                for dy in 0..k {
                    let o = ((y * k + dy) * w + x * k) * 4;
                    for dx in 0..k {
                        buf[o + dx * 4..o + dx * 4 + 4].copy_from_slice(&px);
                    }
                }
            }
        }
        let img = Arc::new(render_image(w as u32, h as u32, buf)?);
        self.cache.insert((k as u8, tint), img.clone());
        Some(img)
    }
}

impl Drop for TexData {
    fn drop(&mut self) {
        self.clear();
    }
}

/// GPUI-Bild aus BGRA-Daten
pub fn render_image(w: u32, h: u32, bgra: Vec<u8>) -> Option<RenderImage> {
    let buf = image::RgbaImage::from_raw(w, h, bgra)?;
    // smallvec! verschiebt das Bild; SmallVec::from_elem würde die ganzen Pixel einmal kopieren
    Some(RenderImage::new(smallvec::smallvec![image::Frame::new(buf)]))
}

/// Textur (geteilt, billig zu kopieren)
#[derive(Clone)]
pub struct TextureHandle(Arc<Mutex<TexData>>);

impl TextureHandle {
    pub fn id(&self) -> TextureHandle {
        self.clone()
    }
    pub fn set(&mut self, img: ColorImage, opts: TextureOptions) {
        let mut d = self.0.lock().unwrap();
        d.clear();
        d.w = img.size[0];
        d.h = img.size[1];
        d.pixels = img.pixels;
        d.opts = opts;
    }
    pub fn size(&self) -> [usize; 2] {
        let d = self.0.lock().unwrap();
        [d.w, d.h]
    }
}

pub fn new_texture(img: ColorImage, opts: TextureOptions) -> TextureHandle {
    TextureHandle(Arc::new(Mutex::new(TexData { w: img.size[0], h: img.size[1], pixels: img.pixels, opts, cache: HashMap::new() })))
}

// ---------------------------------------------------------------- Tasten

macro_rules! keys {
    ($($v:ident => $name:literal $(| $alt:literal)*),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
        pub enum Key { $($v),* }
        impl Key {
            /// Name wie in den gespeicherten Einstellungen
            pub fn name(self) -> &'static str {
                match self { $(Key::$v => $name),* }
            }
            pub fn from_name(n: &str) -> Option<Key> {
                match n { $($name $(| $alt)* => Some(Key::$v),)* _ => None }
            }
        }
    };
}

keys! {
    ArrowDown => "Down" | "ArrowDown", ArrowLeft => "Left" | "ArrowLeft", ArrowRight => "Right" | "ArrowRight", ArrowUp => "Up" | "ArrowUp",
    Escape => "Escape" | "Esc", Tab => "Tab", Backspace => "Backspace", Enter => "Enter" | "Return", Insert => "Insert" | "Help", Delete => "Delete",
    Home => "Home", End => "End", PageUp => "PageUp", PageDown => "PageDown", Copy => "Copy", Cut => "Cut", Paste => "Paste",
    Space => "Space" | " ", Colon => "Colon" | ":", Comma => "Comma" | ",", Minus => "Minus" | "-", Period => "Period" | ".", Plus => "Plus" | "+",
    Equals => "Equals" | "=", Semicolon => "Semicolon" | ";", Backslash => "Backslash" | "\\", Slash => "Slash" | "/", Pipe => "Pipe" | "|",
    Questionmark => "Questionmark" | "?", OpenBracket => "OpenBracket" | "[", CloseBracket => "CloseBracket" | "]", Backtick => "Backtick" | "`", Quote => "Quote" | "'",
    Num0 => "0", Num1 => "1", Num2 => "2", Num3 => "3", Num4 => "4", Num5 => "5", Num6 => "6", Num7 => "7", Num8 => "8", Num9 => "9",
    A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G", H => "H", I => "I", J => "J", K => "K", L => "L", M => "M",
    N => "N", O => "O", P => "P", Q => "Q", R => "R", S => "S", T => "T", U => "U", V => "V", W => "W", X => "X", Y => "Y", Z => "Z",
    F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6", F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
    F13 => "F13", F14 => "F14", F15 => "F15", F16 => "F16", F17 => "F17", F18 => "F18", F19 => "F19", F20 => "F20",
}

impl Key {
    /// GPUI-Tastenname (z. B. "a", "f5", "space", "-") -> Taste
    pub fn from_gpui(k: &str) -> Option<Key> {
        let k = k.strip_prefix("numpad").unwrap_or(k);
        Some(match k {
            "down" => Key::ArrowDown,
            "left" => Key::ArrowLeft,
            "right" => Key::ArrowRight,
            "up" => Key::ArrowUp,
            "escape" => Key::Escape,
            "tab" => Key::Tab,
            "backspace" => Key::Backspace,
            "enter" => Key::Enter,
            "insert" => Key::Insert,
            "delete" => Key::Delete,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "space" => Key::Space,
            "subtract" => Key::Minus,
            "add" => Key::Plus,
            "divide" => Key::Slash,
            "decimal" => Key::Period,
            _ => {
                let up = k.to_uppercase();
                return Key::from_name(&up).or_else(|| Key::from_name(k));
            }
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Modifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub command: bool,
}

impl Modifiers {
    pub fn any(&self) -> bool {
        self.alt || self.ctrl || self.shift || self.command
    }
    fn from_gpui(m: &gpui::Modifiers) -> Modifiers {
        Modifiers { alt: m.alt, ctrl: m.control, shift: m.shift, command: m.control }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Key { key: Key, pressed: bool, repeat: bool, modifiers: Modifiers },
}

// ---------------------------------------------------------------- Eingabe

#[derive(Clone, Default)]
pub struct DroppedFile {
    pub path: Option<PathBuf>,
}

#[derive(Clone, Default)]
pub struct RawInput {
    pub dropped_files: Vec<DroppedFile>,
}

#[derive(Clone, Default)]
pub struct PointerState {
    pos: Option<Pos2>,
    delta: Vec2,
    down: bool,
    clicked: bool,
}

impl PointerState {
    pub fn hover_pos(&self) -> Option<Pos2> {
        self.pos
    }
    pub fn primary_clicked(&self) -> bool {
        self.clicked
    }
    pub fn primary_down(&self) -> bool {
        self.down
    }
    pub fn delta(&self) -> Vec2 {
        self.delta
    }
}

#[derive(Clone, Default)]
pub struct ViewportInfo {
    pub focused: Option<bool>,
}

#[derive(Clone, Default)]
pub struct InputState {
    pub time: f64,
    pub stable_dt: f32,
    pub pointer: PointerState,
    pub smooth_scroll_delta: Vec2,
    pub events: Vec<Event>,
    pub modifiers: Modifiers,
    pub raw: RawInput,
    keys_down: HashSet<Key>,
    viewport: ViewportInfo,
}

impl InputState {
    pub fn key_down(&self, k: Key) -> bool {
        self.keys_down.contains(&k)
    }
    pub fn viewport(&self) -> &ViewportInfo {
        &self.viewport
    }
}

/// Von GPUI zwischen zwei Bildern gesammelte Eingaben eines Fensters
#[derive(Default)]
pub struct Collector {
    pub events: Vec<Event>,
    pub keys_down: HashSet<Key>,
    pub modifiers: Modifiers,
    pub mouse: Option<Pos2>,
    /// Maus über selbst Gezeichnetem (nicht über Menü/Fenster)
    pub over_canvas: bool,
    pub button_down: bool,
    /// Zeitpunkt der letzten Mausbewegung über dem Fenster
    pub moved_at: Option<Instant>,
    press_pos: Option<Pos2>,
    pub clicked: bool,
    pub scroll: Vec2,
    pub dropped: Vec<PathBuf>,
}

impl Collector {
    pub fn key(&mut self, key: Key, pressed: bool, repeat: bool, m: Modifiers) {
        self.modifiers = m;
        if pressed {
            self.keys_down.insert(key);
        } else {
            self.keys_down.remove(&key);
        }
        self.events.push(Event::Key { key, pressed, repeat, modifiers: m });
    }
    /// Tasten, die laut Windows gar nicht mehr gedrückt sind, loslassen. GPUI meldet das Loslassen nicht immer
    /// (z. B. wenn dazwischen ein Menü oder anderes Element den Fokus hatte) – dann blieb eine Taste „hängen“,
    /// etwa die Rücktaste fürs Zurückspulen, und das Spiel spulte ohne Tastendruck zurück.
    pub fn release_stuck(&mut self) {
        #[cfg(windows)]
        {
            // (für automatische Tests mit Fenster-Nachrichten abschaltbar)
            static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *OFF.get_or_init(|| std::env::var_os("PASTLINK_NO_KEYSYNC").is_some()) {
                return;
            }
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
            let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
            self.keys_down.retain(|k| match vk_codes(*k) {
                Some((a, b)) => down(a) || b.map_or(false, down),
                None => true,
            });
        }
    }
    /// Fenster verliert den Fokus: alle Tasten gelten als losgelassen
    pub fn release_all(&mut self) {
        self.keys_down.clear();
        self.button_down = false;
        self.modifiers = Modifiers::default();
    }
}

/// Windows-Tastencodes einer Taste (Haupttastatur, Ziffernblock); None = nicht prüfbar (bleibt wie gemeldet)
#[cfg(windows)]
fn vk_codes(k: Key) -> Option<(i32, Option<i32>)> {
    use Key::*;
    let n = |a: i32| Some((a, None));
    match k {
        ArrowLeft => n(0x25),
        ArrowUp => n(0x26),
        ArrowRight => n(0x27),
        ArrowDown => n(0x28),
        Escape => n(0x1B),
        Tab => n(0x09),
        Backspace => n(0x08),
        Enter => n(0x0D),
        Insert => Some((0x2D, Some(0x60))),
        Delete => Some((0x2E, Some(0x6E))),
        Home => Some((0x24, Some(0x67))),
        End => Some((0x23, Some(0x61))),
        PageUp => Some((0x21, Some(0x69))),
        PageDown => Some((0x22, Some(0x63))),
        Space => n(0x20),
        Minus => Some((0xBD, Some(0x6D))),
        Plus => Some((0xBB, Some(0x6B))),
        Slash => Some((0xBF, Some(0x6F))),
        Period => Some((0xBE, Some(0x6E))),
        Comma => n(0xBC),
        Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
            let d = Key::from_name(k.name()).map(|_| k.name().as_bytes()[0] as i32 - b'0' as i32).unwrap_or(0);
            Some((0x30 + d, Some(0x60 + d)))
        }
        _ => {
            let nm = k.name();
            if nm.len() == 1 && nm.as_bytes()[0].is_ascii_uppercase() {
                return n(nm.as_bytes()[0] as i32);
            }
            if let Some(f) = nm.strip_prefix('F').and_then(|v| v.parse::<i32>().ok()) {
                return n(0x70 + f - 1);
            }
            None
        }
    }
}

// ---------------------------------------------------------------- Zeichenbefehle

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    Background,
    Foreground,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Id(u64);

impl Id {
    pub fn new(h: impl std::hash::Hash) -> Id {
        use std::hash::Hasher;
        let mut s = std::collections::hash_map::DefaultHasher::new();
        h.hash(&mut s);
        Id(s.finish())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayerId {
    pub order: Order,
}

impl LayerId {
    pub fn new(order: Order, _id: Id) -> LayerId {
        LayerId { order }
    }
    pub fn background() -> LayerId {
        LayerId { order: Order::Background }
    }
}

enum Cmd {
    Quad { rect: Rect, radius: f32, fill: Background, border: f32, border_color: Color32 },
    Path { points: Vec<Pos2>, color: Background },
    Text { pos: Pos2, galley: Arc<Galley> },
    Image { rect: Rect, img: Arc<RenderImage> },
}

pub enum ViewportCommand {
    Title(String),
    Fullscreen(bool),
    Close,
}

struct Frame {
    input: InputState,
    screen: Rect,
    ppp: f32,
    text: Option<Arc<WindowTextSystem>>,
    /// Befehle je Ebene (unten / oben)
    layers: [Vec<(Rect, Cmd)>; 2],
    cmds: Vec<ViewportCommand>,
    wants_keyboard: bool,
}

/// Zeichen- und Eingabezustand eines Fensters
#[derive(Clone)]
pub struct Context(Rc<RefCell<Frame>>, Rc<RefCell<Collector>>, Rc<Instant>);

impl Context {
    pub fn new() -> Context {
        Context(
            Rc::new(RefCell::new(Frame { input: InputState::default(), screen: Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 700.0)), ppp: 1.0, text: None, layers: [Vec::new(), Vec::new()], cmds: Vec::new(), wants_keyboard: false })),
            Rc::new(RefCell::new(Collector::default())),
            Rc::new(Instant::now()),
        )
    }

    pub fn collector(&self) -> std::cell::RefMut<'_, Collector> {
        self.1.borrow_mut()
    }

    /// Neues Bild: Eingaben übernehmen, Zeichenbefehle leeren
    pub fn begin_frame(&self, window: &Window, wants_keyboard: bool) {
        let mut f = self.0.borrow_mut();
        let mut c = self.1.borrow_mut();
        let time = self.2.elapsed().as_secs_f64();
        let dt = (time - f.input.time).clamp(0.0, 0.25) as f32;
        let vs = window.viewport_size();
        f.screen = Rect::from_min_size(Pos2::ZERO, vec2(f32::from(vs.width), f32::from(vs.height)));
        f.ppp = window.scale_factor();
        f.text = Some(window.text_system().clone());
        f.layers[0].clear();
        f.layers[1].clear();
        f.wants_keyboard = wants_keyboard;
        let pos = if c.over_canvas { c.mouse } else { None };
        let delta = match (pos, f.input.pointer.pos) {
            (Some(a), Some(b)) => a - b,
            _ => Vec2::ZERO,
        };
        let focused = window.is_window_active();
        if !focused {
            c.release_all();
        } else {
            c.release_stuck();
        }
        f.input = InputState {
            time,
            stable_dt: if dt > 0.0 { dt } else { 1.0 / 60.0 },
            pointer: PointerState { pos, delta, down: c.button_down && c.over_canvas, clicked: std::mem::take(&mut c.clicked) },
            smooth_scroll_delta: std::mem::take(&mut c.scroll),
            events: std::mem::take(&mut c.events),
            modifiers: if focused { Modifiers::from_gpui(&window.modifiers()) } else { Modifiers::default() },
            raw: RawInput { dropped_files: c.dropped.drain(..).map(|p| DroppedFile { path: Some(p) }).collect() },
            keys_down: c.keys_down.clone(),
            viewport: ViewportInfo { focused: Some(focused) },
        };
    }

    pub fn input<R>(&self, f: impl FnOnce(&InputState) -> R) -> R {
        f(&self.0.borrow().input)
    }
    pub fn screen_rect(&self) -> Rect {
        self.0.borrow().screen
    }
    pub fn pixels_per_point(&self) -> f32 {
        self.0.borrow().ppp
    }
    pub fn request_repaint(&self) {}
    pub fn pointer_latest_pos(&self) -> Option<Pos2> {
        self.0.borrow().input.pointer.pos
    }
    pub fn wants_keyboard_input(&self) -> bool {
        self.0.borrow().wants_keyboard
    }
    pub fn send_viewport_cmd(&self, c: ViewportCommand) {
        self.0.borrow_mut().cmds.push(c);
    }
    pub fn take_viewport_cmds(&self) -> Vec<ViewportCommand> {
        std::mem::take(&mut self.0.borrow_mut().cmds)
    }
    pub fn load_texture(&self, _name: impl Into<String>, img: ColorImage, opts: TextureOptions) -> TextureHandle {
        new_texture(img, opts)
    }
    pub fn layer_painter(&self, layer: LayerId) -> Painter {
        let top = layer.order == Order::Foreground;
        Painter { ctx: self.clone(), layer: top as usize, clip: Rect::EVERYTHING }
    }
    /// Zeichenfläche für das Spielbild (unterste Ebene)
    pub fn painter(&self) -> Painter {
        self.layer_painter(LayerId::background())
    }

    fn push(&self, layer: usize, clip: Rect, c: Cmd) {
        self.0.borrow_mut().layers[layer].push((clip, c));
    }

    fn layout(&self, text: &str, font: &FontId, color: Color32) -> Arc<Galley> {
        let ts = self.0.borrow().text.clone();
        let line_h = row_height(font);
        let Some(ts) = ts else {
            let w = text.chars().count() as f32 * font.size * 0.55;
            return Arc::new(Galley { lines: Vec::new(), line_h, size: vec2(w, line_h) });
        };
        let gfont = gpui_font(&font.family);
        let mut lines = Vec::new();
        let mut w: f32 = 0.0;
        for l in text.split('\n') {
            let run = TextRun { len: l.len(), font: gfont.clone(), color: color.to_hsla(), background_color: None, underline: None, strikethrough: None };
            let shaped = ts.shape_line(SharedString::from(l.to_string()), px(font.size), &[run], None);
            w = w.max(f32::from(shaped.width));
            lines.push(shaped);
        }
        let h = line_h * lines.len().max(1) as f32;
        Arc::new(Galley { lines, line_h, size: vec2(w, h) })
    }

    /// Befehle einer Ebene mit GPUI zeichnen (in der Paint-Phase eines `canvas`)
    pub fn paint_layer(&self, layer: usize, window: &mut Window, cx: &mut GApp) {
        let cmds = std::mem::take(&mut self.0.borrow_mut().layers[layer]);
        for (clip, c) in &cmds {
            let mask = if *clip == Rect::EVERYTHING { None } else { Some(ContentMask { bounds: clip.to_bounds() }) };
            window.with_content_mask(mask, |window| match c {
                Cmd::Quad { rect, .. } | Cmd::Image { rect, .. } if !(rect.min.x.is_finite() && rect.min.y.is_finite() && rect.max.x.is_finite() && rect.max.y.is_finite()) => {
                    gfx_log(format!("Fläche/Bild mit ungültigem Rahmen verworfen: {rect:?}"));
                }
                Cmd::Quad { rect, radius, fill, border, border_color } => {
                    window.paint_quad(PaintQuad {
                        bounds: rect.to_bounds(),
                        corner_radii: Corners::all(px(*radius)),
                        background: *fill,
                        border_widths: Edges::all(px(*border)),
                        border_color: border_color.to_hsla(),
                        border_style: BorderStyle::Solid,
                    });
                }
                Cmd::Path { points, color } => {
                    // ungültige Koordinaten (z. B. im ersten Bild nach einem Tabwechsel, Zoom noch 0) würden als riesiger
                    // Keil über den ganzen Bildschirm gezeichnet -> solche Formen weglassen
                    let bad = points.iter().any(|q| !q.x.is_finite() || !q.y.is_finite() || q.x.abs() > 50_000.0 || q.y.abs() > 50_000.0);
                    if bad {
                        gfx_log(format!("Form mit ungültigen Punkten verworfen: {:?}", &points[..points.len().min(4)]));
                    }
                    if points.len() >= 3 && !bad {
                        let mut p = Path::new(point(px(points[0].x), px(points[0].y)));
                        for q in &points[1..] {
                            p.line_to(point(px(q.x), px(q.y)));
                        }
                        window.paint_path(p, *color);
                    }
                }
                Cmd::Text { pos, galley } => {
                    for (k, l) in galley.lines.iter().enumerate() {
                        let _ = l.paint(point(px(pos.x), px(pos.y + k as f32 * galley.line_h)), px(galley.line_h), window, cx);
                    }
                }
                Cmd::Image { rect, img } => {
                    let _ = window.paint_image(rect.to_bounds(), Corners::all(px(0.0)), img.clone(), 0, false);
                }
            });
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Context::new()
    }
}

/// (Fehlersuche) Meldung an die Datei in `PASTLINK_GFXLOG` anhängen
fn gfx_log(m: String) {
    if let Some(p) = std::env::var_os("PASTLINK_GFXLOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{m}");
        }
    }
}

/// Nicht mehr gebrauchte Bilder aus dem Grafikspeicher dieses Fensters entfernen
pub fn collect_garbage(window: &mut Window, bit: u8) {
    WINDOWS.fetch_or(bit, std::sync::atomic::Ordering::Relaxed);
    let mine: Vec<Arc<RenderImage>> = {
        let Ok(mut g) = GARBAGE.lock() else { return };
        let mut out = Vec::new();
        for (mask, img) in g.iter_mut() {
            if *mask & bit != 0 {
                *mask &= !bit;
                out.push(img.clone());
            }
        }
        g.retain(|(m, _)| *m != 0);
        out
    };
    for img in mine {
        let _ = window.drop_image(img);
    }
}

/// Fenster zeichnet keine Bilder mehr (geschlossen)
pub fn unregister_window(bit: u8) {
    WINDOWS.fetch_and(!bit, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut g) = GARBAGE.lock() {
        for (m, _) in g.iter_mut() {
            *m &= !bit;
        }
        g.retain(|(m, _)| *m != 0);
    }
}

/// Bild sofort zum Entfernen vormerken (z. B. jedes Spielbild)
pub fn retire_image(img: Arc<RenderImage>) {
    retire(img);
}

/// Maus-Ereignisse eines Fensters einsammeln. In der Prepaint-/Paint-Phase der untersten
/// Zeichenfläche aufrufen; `hitbox` deckt das ganze Fenster ab (Menüs und Fenster darüber verdecken sie).
pub fn install_mouse(ctx: &Context, hitbox: &Hitbox, window: &mut Window) {
    let c = ctx.1.clone();
    let hb = hitbox.clone();
    window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble {
            let mut c = c.borrow_mut();
            c.mouse = Some(pos2(f32::from(e.position.x), f32::from(e.position.y)));
            c.moved_at = Some(Instant::now());
            c.over_canvas = hb.is_hovered(window);
            c.button_down = e.pressed_button == Some(MouseButton::Left);
        }
    });
    let c = ctx.1.clone();
    let hb = hitbox.clone();
    window.on_mouse_event(move |e: &MouseDownEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble && hb.is_hovered(window) {
            let mut c = c.borrow_mut();
            let p = pos2(f32::from(e.position.x), f32::from(e.position.y));
            c.mouse = Some(p);
            c.over_canvas = true;
            if e.button == MouseButton::Left {
                c.button_down = true;
                c.press_pos = Some(p);
            }
        }
    });
    let c = ctx.1.clone();
    let hb = hitbox.clone();
    window.on_mouse_event(move |e: &MouseUpEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble {
            let mut c = c.borrow_mut();
            let p = pos2(f32::from(e.position.x), f32::from(e.position.y));
            let over = hb.is_hovered(window);
            if e.button == MouseButton::Left {
                c.button_down = false;
                if let Some(pp) = c.press_pos.take() {
                    if over && pp.distance(p) < 6.0 {
                        c.clicked = true;
                    }
                }
            }
        }
    });
    let c = ctx.1.clone();
    let hb = hitbox.clone();
    window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble && hb.is_hovered(window) {
            let d = e.delta.pixel_delta(px(40.0));
            c.borrow_mut().scroll += vec2(f32::from(d.x), f32::from(d.y));
        }
    });
}

pub fn insert_hitbox(bounds: Bounds<Pixels>, window: &mut Window) -> Hitbox {
    window.insert_hitbox(bounds, HitboxBehavior::Normal)
}

/// Tastenereignis von GPUI weiterreichen
pub fn key_from_gpui(ks: &gpui::Keystroke) -> Option<(Key, Modifiers)> {
    Some((Key::from_gpui(&ks.key)?, Modifiers::from_gpui(&ks.modifiers)))
}


// ---------------------------------------------------------------- Painter

#[derive(Clone)]
pub struct Painter {
    ctx: Context,
    layer: usize,
    clip: Rect,
}

fn bg(c: Color32) -> Background {
    c.to_hsla().into()
}

impl Painter {
    pub fn ctx(&self) -> &Context {
        &self.ctx
    }
    pub fn clip_rect(&self) -> Rect {
        if self.clip == Rect::EVERYTHING {
            self.ctx.screen_rect()
        } else {
            self.clip
        }
    }
    pub fn with_clip_rect(&self, r: Rect) -> Painter {
        Painter { ctx: self.ctx.clone(), layer: self.layer, clip: if self.clip == Rect::EVERYTHING { r } else { self.clip.intersect(r) } }
    }
    fn push(&self, c: Cmd) {
        self.ctx.push(self.layer, self.clip, c);
    }

    pub fn rect_filled(&self, r: Rect, rounding: f32, color: impl Into<Color32>) {
        let color = color.into();
        if color.a() == 0 || !r.is_positive() {
            return;
        }
        self.push(Cmd::Quad { rect: r, radius: rounding.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0), fill: bg(color), border: 0.0, border_color: Color32::TRANSPARENT });
    }

    /// Rahmen mittig auf der Rechteckkante
    pub fn rect_stroke(&self, r: Rect, rounding: f32, stroke: impl Into<Stroke>) {
        let s = stroke.into();
        if !s.visible() {
            return;
        }
        let o = r.expand(s.width * 0.5);
        let rad = if rounding > 0.0 { rounding + s.width * 0.5 } else { 0.0 };
        self.push(Cmd::Quad { rect: o, radius: rad, fill: bg(Color32::TRANSPARENT), border: s.width, border_color: s.color });
    }

    pub fn circle_filled(&self, c: Pos2, r: f32, color: impl Into<Color32>) {
        let color = color.into();
        if r <= 0.0 || color.a() == 0 {
            return;
        }
        self.push(Cmd::Quad { rect: Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)), radius: r, fill: bg(color), border: 0.0, border_color: Color32::TRANSPARENT });
    }

    pub fn circle_stroke(&self, c: Pos2, r: f32, stroke: impl Into<Stroke>) {
        let s = stroke.into();
        if !s.visible() {
            return;
        }
        let ro = r + s.width * 0.5;
        self.push(Cmd::Quad { rect: Rect::from_center_size(c, vec2(ro * 2.0, ro * 2.0)), radius: ro, fill: bg(Color32::TRANSPARENT), border: s.width, border_color: s.color });
    }

    pub fn line_segment(&self, pts: [Pos2; 2], stroke: impl Into<Stroke>) {
        let s = stroke.into();
        if !s.visible() {
            return;
        }
        let [a, b] = pts;
        let hw = s.width * 0.5;
        // waagrecht/senkrecht: scharfes Rechteck
        if (a.y - b.y).abs() < 0.01 {
            let r = Rect::from_min_max(pos2(a.x.min(b.x), a.y - hw), pos2(a.x.max(b.x), a.y + hw));
            self.push(Cmd::Quad { rect: r, radius: 0.0, fill: bg(s.color), border: 0.0, border_color: Color32::TRANSPARENT });
            return;
        }
        if (a.x - b.x).abs() < 0.01 {
            let r = Rect::from_min_max(pos2(a.x - hw, a.y.min(b.y)), pos2(a.x + hw, a.y.max(b.y)));
            self.push(Cmd::Quad { rect: r, radius: 0.0, fill: bg(s.color), border: 0.0, border_color: Color32::TRANSPARENT });
            return;
        }
        let d = (b - a).normalized();
        let n = vec2(-d.y, d.x) * hw;
        self.push(Cmd::Path { points: vec![a + n, b + n, b - n, a - n], color: bg(s.color) });
    }

    pub fn layout_no_wrap(&self, text: impl Into<String>, font: FontId, color: impl Into<Color32>) -> Arc<Galley> {
        self.ctx.layout(&text.into(), &font, color.into())
    }

    pub fn galley(&self, pos: Pos2, galley: Arc<Galley>, _fallback: impl Into<Color32>) {
        self.push(Cmd::Text { pos, galley });
    }

    pub fn text(&self, pos: Pos2, align: Align2, text: impl ToString, font: FontId, color: impl Into<Color32>) -> Rect {
        let color = color.into();
        let g = self.ctx.layout(&text.to_string(), &font, color);
        let r = align.anchor_size(pos, g.size());
        if color.a() > 0 {
            self.push(Cmd::Text { pos: r.min, galley: g });
        }
        r
    }

    /// Bild zeichnen; `uv` wählt den sichtbaren Ausschnitt (0..1), `tint` färbt (Weiß = unverändert)
    pub fn image(&self, tex: TextureHandle, rect: Rect, uv: Rect, tint: impl Into<Color32>) {
        let tint = tint.into();
        if !rect.is_positive() || tint.a() == 0 {
            return;
        }
        // ganzes Bild so groß, dass der Ausschnitt `uv` genau auf `rect` liegt
        let uw = uv.width().max(1e-6);
        let uh = uv.height().max(1e-6);
        let fw = rect.width() / uw;
        let fh = rect.height() / uh;
        let full = Rect::from_min_size(pos2(rect.left() - uv.left() * fw, rect.top() - uv.top() * fh), vec2(fw, fh));
        let ppp = self.ctx.pixels_per_point();
        let img = {
            let mut d = tex.0.lock().unwrap();
            // Nächster-Nachbar: in ganzen Vielfachen vergrößert hochladen, damit es scharf bleibt
            let k = if d.opts.magnification == TextureFilter::Nearest && d.w > 0 {
                let scale = (fw * ppp / d.w as f32).max(fh * ppp / d.h.max(1) as f32);
                let maxk = (4096 / d.w.max(d.h).max(1)).max(1) as u8;
                let mut k = 1u8;
                while (k as f32) < scale - 0.01 && k < 8 && k * 2 <= maxk {
                    k *= 2;
                }
                k
            } else {
                1
            };
            d.variant(k, tint)
        };
        let Some(img) = img else { return };
        let clip = if uv == Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)) { self.clip } else if self.clip == Rect::EVERYTHING { rect } else { self.clip.intersect(rect) };
        self.ctx.push(self.layer, clip, Cmd::Image { rect: full, img });
    }

    /// fertiges GPUI-Bild (z. B. Spielbild) zeichnen
    pub fn render_image(&self, img: Arc<RenderImage>, rect: Rect) {
        self.push(Cmd::Image { rect, img });
    }

    pub fn add(&self, shape: Shape) {
        match shape {
            Shape::Polygon { points, fill, stroke, closed } => {
                if fill.a() > 0 && points.len() >= 3 {
                    self.push(Cmd::Path { points: points.clone(), color: bg(fill) });
                }
                if stroke.visible() && points.len() >= 2 {
                    let n = points.len();
                    let m = if closed { n } else { n - 1 };
                    for k in 0..m {
                        self.line_segment([points[k], points[(k + 1) % n]], stroke);
                    }
                }
            }
            Shape::Segment { points, stroke } => self.line_segment(points, stroke),
            Shape::Mesh(m) => {
                // Vierecke aus je zwei Dreiecken: Verlauf von der einen Kante zur anderen
                for q in m.vertices.chunks_exact(4) {
                    let (a, b, c, d) = (q[0], q[1], q[2], q[3]);
                    let from = (a.pos + (b.pos - a.pos) * 0.5).to_vec2();
                    let to = (d.pos + (c.pos - d.pos) * 0.5).to_vec2();
                    let dir = to - from;
                    // CSS-Winkel: 0° = nach oben, 90° = nach rechts
                    let angle = dir.x.atan2(-dir.y).to_degrees();
                    let grad = gpui::linear_gradient(angle, gpui::linear_color_stop(a.color.to_hsla(), 0.0), gpui::linear_color_stop(d.color.to_hsla(), 1.0));
                    self.push(Cmd::Path { points: vec![a.pos, b.pos, c.pos, d.pos], color: grad });
                }
            }
        }
    }
}
