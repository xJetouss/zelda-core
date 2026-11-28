//! Mods: Erweiterungen in Rust, die nur im Recompiled-Modus (RC) laufen.
//!
//! Eine Mod kann pro Spielbild Eingaben, RAM, VRAM, Paletten, Sprites und sogar die ROM im Speicher ändern,
//! sich per Hook vor Spielroutinen hängen (Register ändern, Routine überspringen), über dem Bild zeichnen
//! (Rechtecke, Text, Bilder), Grafik und Farben dauerhaft austauschen, Töne abspielen und das Spieltempo ändern.
//! - Mods sind DLLs im Ordner `mods` neben der .exe (C-ABI aus `assets/mod_template/src/api.rs`, Vorlage über
//!   „Mods“ → „Mod-Vorlage exportieren…“). In der App selbst steckt keine Mod: Sie kommen aus der Mod-Liste
//!   (`shop.rs`, Download von GitHub) oder werden selbst gebaut.

#[path = "../../assets/mod_template/src/api.rs"]
pub mod api;
pub mod shop;

use crate::gfx::{self, Color32};
use crate::snes::bus::Bus;
use crate::snes::cpu::Cpu;
use crate::snes::Snes;
use api::space;
use std::collections::HashMap;
use std::ffi::{c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Zeichnung einer Mod über dem Spielbild (SNES-Pixel)
#[derive(Clone)]
pub enum Shape {
    Rect(i32, i32, i32, i32, u32),
    Text(i32, i32, u32, String),
    /// Bild-Nummer, x, y, w, h (0 = Originalgröße), Tönung
    Image(u32, i32, i32, i32, i32, u32),
}

/// Ausgaben der Mods in einem Bild
#[derive(Default)]
struct Out {
    shapes: Vec<Shape>,
    speed: f32,
}

/// Angemeldetes Bild einer Mod (RGBA)
struct Img {
    w: usize,
    h: usize,
    rgba: Arc<Vec<u8>>,
}

/// Laufender Ton (32 kHz Stereo, wie die Tonausgabe des Spiels)
struct Snd {
    data: Vec<i16>,
    pos: usize,
    vol: f32,
}

/// Dauerhafte Änderungen an Grafik und Farben, die vor dem Zeichnen jedes Bildes angewendet werden
#[derive(Default, Clone, PartialEq)]
pub struct Patch {
    /// (Wort-Adresse, Daten)
    pub vram: Vec<(u32, Vec<u16>)>,
    /// (Farb-Index, Farben)
    pub cgram: Vec<(u32, Vec<u16>)>,
}

impl Patch {
    pub fn is_empty(&self) -> bool {
        self.vram.is_empty() && self.cgram.is_empty()
    }
    /// auf eine PPU anwenden
    pub fn apply(&self, ppu: &mut crate::snes::ppu::Ppu) {
        for (a, d) in &self.vram {
            for (i, w) in d.iter().enumerate() {
                if let Some(x) = ppu.vram.get_mut((*a as usize + i) & 0x7FFF) {
                    *x = *w;
                }
            }
        }
        for (a, d) in &self.cgram {
            for (i, c) in d.iter().enumerate() {
                if let Some(x) = ppu.cgram.get_mut((*a as usize + i) & 0xFF) {
                    *x = *c & 0x7FFF;
                }
            }
        }
    }
}

/// Was Mods über mehrere Bilder hinweg anlegen: Bilder, Töne, Grafik-Änderungen (je Mod), Meldungen
#[derive(Default)]
struct Store {
    images: Vec<Img>,
    sounds: Vec<Snd>,
    /// (Mod-Index, Änderung)
    patches: Vec<(usize, Patch)>,
    patch_version: u64,
    logs: Vec<String>,
    /// Einstellungen der Mods (Kennung -> Schlüssel -> Wert), Standardwerte aus der Mod
    values: HashMap<String, HashMap<String, i32>>,
    defaults: HashMap<String, HashMap<String, i32>>,
    /// Werte der App für `app_get` (vor jedem Bild von der App gesetzt)
    app_vals: HashMap<String, i64>,
    /// Wünsche der Mods an die App: (Aktion statt Wert?, Schlüssel, Wert) – nach dem Bild ausgeführt
    app_ops: Vec<(bool, String, i64)>,
}

/// Eine Einstellung einer Mod (aus `pastlink_mod_settings`)
#[derive(Clone)]
pub struct SettingDef {
    pub key: String,
    pub label_de: String,
    pub label_en: String,
    pub kind: u32,
    pub def: i32,
    pub min: i32,
    pub max: i32,
}

/// Das laufende Spiel, auf das eine Mod zugreift
pub enum Sys<'a> {
    /// A Link to the Past (SNES, Recompiled-Modus)
    Snes { cpu: &'a mut Cpu, bus: &'a mut Bus },
    /// The Minish Cap (PC-Fassung): Tasten dieses Bilds, Ansicht (`tmc_view`), Item-Menü offen
    #[cfg(pastlink_gba)]
    Gba { g: &'a mut crate::gba::Gba, pad: &'a mut u16, view: Option<[i32; 12]>, menu: bool },
}

/// The Minish Cap: Zusatz-Infos für Mods ab dieser Adresse (siehe `api::minish`)
pub const MC_INFO: u32 = 0x0F00_0000;

/// The Minish Cap: Zusatz-Infos als Bytes (Aufbau wie `api::minish::INFO_…`)
#[cfg(pastlink_gba)]
fn mc_info(g: &crate::gba::Gba, view: Option<[i32; 12]>, menu: bool) -> [u8; 0x40] {
    let mut b = [0u8; 0x40];
    let put16 = |b: &mut [u8; 0x40], o: usize, v: i32| b[o..o + 2].copy_from_slice(&(v as i16).to_le_bytes());
    b[0x00] = (view.is_some() && !menu) as u8;
    b[0x01] = menu as u8;
    if let Some(v) = view {
        b[0x02] = v[0] as u8;
        b[0x03] = v[1] as u8;
        b[0x04] = (v[2] != 0) as u8;
        put16(&mut b, 0x06, v[9]);
        put16(&mut b, 0x08, v[10]);
        // auf dem Bild: Spielwelt minus Kamera (im Breitbild ist das Bild breiter)
        put16(&mut b, 0x0A, v[9] - v[5]);
        put16(&mut b, 0x0C, v[10] - v[6]);
        put16(&mut b, 0x0E, v[5]);
        put16(&mut b, 0x10, v[6]);
    }
    put16(&mut b, 0x12, g.w as i32);
    put16(&mut b, 0x14, g.h as i32);
    b
}

/// Zugriff einer Mod auf das laufende Spiel
pub struct Ctx<'a> {
    pub sys: Sys<'a>,
    /// Kennung der laufenden Mod (für ihre Einstellungen)
    pub id: String,
    pub frame: u64,
    out: &'a mut Out,
    store: &'a mut Store,
    /// Index der gerade laufenden Mod
    cur: usize,
}

impl Ctx<'_> {
    pub fn read8(&self, addr: u32) -> u8 {
        self.mem_read(space::BUS, addr)
    }
    pub fn read16(&self, addr: u32) -> u16 {
        self.read8(addr) as u16 | (self.read8(addr + 1) as u16) << 8
    }
    pub fn write8(&mut self, addr: u32, v: u8) {
        self.mem_write(space::BUS, addr, v)
    }
    pub fn write16(&mut self, addr: u32, v: u16) {
        self.write8(addr, v as u8);
        self.write8(addr + 1, (v >> 8) as u8);
    }
    /// laufendes Spiel (`api::GAME_…`)
    pub fn game(&self) -> u32 {
        match &self.sys {
            Sys::Snes { .. } => api::GAME_ALTTP,
            #[cfg(pastlink_gba)]
            Sys::Gba { .. } => api::GAME_MINISH,
        }
    }
    /// Zeichenfläche in Spielpixeln
    pub fn screen(&self) -> (u32, u32) {
        match &self.sys {
            Sys::Snes { .. } => (256, 224),
            #[cfg(pastlink_gba)]
            Sys::Gba { g, .. } => (g.w as u32, g.h as u32),
        }
    }
    pub fn mem_read(&self, sp: u32, addr: u32) -> u8 {
        let b = match &self.sys {
            Sys::Snes { bus, .. } => &**bus,
            #[cfg(pastlink_gba)]
            Sys::Gba { g, view, menu, .. } => {
                if sp != space::BUS {
                    return 0;
                }
                if (MC_INFO..MC_INFO + 0x40).contains(&addr) {
                    return mc_info(g, *view, *menu)[(addr - MC_INFO) as usize];
                }
                return g.peek(addr, 1).map_or(0, |b| b[0]);
            }
        };
        match sp {
            space::BUS => match addr >> 16 {
                0x7E | 0x7F => b.wram[(addr & 0x1FFFF) as usize],
                0x70 => b.sram.get((addr & 0xFFFF) as usize).copied().unwrap_or(0),
                _ => 0,
            },
            space::VRAM => b.ppu.vram.get((addr as usize & 0xFFFF) >> 1).map_or(0, |w| if addr & 1 == 0 { *w as u8 } else { (*w >> 8) as u8 }),
            space::CGRAM => b.ppu.cgram.get((addr as usize & 0x1FF) >> 1).map_or(0, |w| if addr & 1 == 0 { *w as u8 } else { (*w >> 8) as u8 }),
            space::OAM => b.ppu.oam.get(addr as usize).copied().unwrap_or(0),
            space::ROM => b.rom.get(addr as usize).copied().unwrap_or(0),
            _ => 0,
        }
    }
    pub fn mem_write(&mut self, sp: u32, addr: u32, v: u8) {
        let b = match &mut self.sys {
            Sys::Snes { bus, .. } => &mut **bus,
            #[cfg(pastlink_gba)]
            Sys::Gba { g, .. } => {
                if sp == space::BUS && !(MC_INFO..MC_INFO + 0x40).contains(&addr) {
                    g.poke(addr, &[v]);
                }
                return;
            }
        };
        let word = |w: &mut u16| *w = if addr & 1 == 0 { (*w & 0xFF00) | v as u16 } else { (*w & 0x00FF) | (v as u16) << 8 };
        match sp {
            space::BUS => match addr >> 16 {
                0x7E | 0x7F => b.wram[(addr & 0x1FFFF) as usize] = v,
                0x70 => {
                    if let Some(x) = b.sram.get_mut((addr & 0xFFFF) as usize) {
                        *x = v;
                        b.sram_dirty = true;
                    }
                }
                _ => {}
            },
            space::VRAM => {
                if let Some(w) = b.ppu.vram.get_mut((addr as usize & 0xFFFF) >> 1) {
                    word(w)
                }
            }
            space::CGRAM => {
                if let Some(w) = b.ppu.cgram.get_mut((addr as usize & 0x1FF) >> 1) {
                    word(w)
                }
            }
            space::OAM => {
                if let Some(x) = b.ppu.oam.get_mut(addr as usize) {
                    *x = v
                }
            }
            space::ROM => {
                // geänderter Code: der Recompiled-Modus prüft die Blöcke danach neu (dort dann Interpreter)
                if (addr as usize) < b.rom.len() && b.rom[addr as usize] != v {
                    Arc::make_mut(&mut b.rom)[addr as usize] = v;
                }
            }
            _ => {}
        }
    }
    pub fn reg(&self, r: u32) -> u32 {
        let Sys::Snes { cpu, .. } = &self.sys else { return 0 };
        let c = &**cpu;
        match r {
            api::reg::A => c.a as u32,
            api::reg::X => c.x as u32,
            api::reg::Y => c.y as u32,
            api::reg::SP => c.sp as u32,
            api::reg::DP => c.dp as u32,
            api::reg::DB => c.db as u32,
            api::reg::PB => c.pb as u32,
            api::reg::PC => c.pc as u32,
            api::reg::P => c.p_reg() as u32,
            _ => 0,
        }
    }
    pub fn set_reg(&mut self, r: u32, v: u32) {
        let Sys::Snes { cpu, .. } = &mut self.sys else { return };
        let c = &mut **cpu;
        match r {
            api::reg::A => c.a = v as u16,
            api::reg::X => c.x = v as u16,
            api::reg::Y => c.y = v as u16,
            api::reg::SP => c.sp = v as u16,
            api::reg::DP => c.dp = v as u16,
            api::reg::DB => c.db = v as u8,
            api::reg::PB => c.pb = v as u8,
            api::reg::PC => c.pc = v as u16,
            api::reg::P => c.set_p_reg(v as u8),
            _ => {}
        }
    }
    /// Routine sofort verlassen (wie RTS bzw. RTL des Spiels)
    pub fn ret(&mut self, long: bool) {
        if let Sys::Snes { cpu, bus } = &mut self.sys {
            cpu.exec_op(bus, if long { 0x6B } else { 0x60 });
        }
    }
    pub fn pad(&self) -> u16 {
        match &self.sys {
            Sys::Snes { bus, .. } => bus.joy[0],
            #[cfg(pastlink_gba)]
            Sys::Gba { pad, .. } => **pad,
        }
    }
    pub fn set_pad(&mut self, p: u16) {
        match &mut self.sys {
            Sys::Snes { bus, .. } => bus.joy[0] = p,
            #[cfg(pastlink_gba)]
            Sys::Gba { pad, .. } => **pad = p,
        }
    }
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: u32) {
        self.out.shapes.push(Shape::Rect(x, y, w, h, rgba));
    }
    pub fn text(&mut self, x: i32, y: i32, rgba: u32, t: impl Into<String>) {
        self.out.shapes.push(Shape::Text(x, y, rgba, t.into()));
    }
    pub fn set_speed(&mut self, s: f32) {
        if s.is_finite() && s > 0.0 {
            self.out.speed = s.clamp(0.1, 8.0);
        }
    }
    /// Link steht im normalen Spiel
    pub fn in_game(&self) -> bool {
        match &self.sys {
            Sys::Snes { .. } => matches!(self.read8(0x7E0010), 0x07 | 0x09 | 0x0B) && self.read8(0x7E0011) == 0,
            #[cfg(pastlink_gba)]
            Sys::Gba { view, menu, .. } => view.is_some() && !menu,
        }
    }

    // ---- Schnittstelle Version 3

    /// Ton in 32 kHz Stereo umrechnen und abspielen
    pub fn play_sound(&mut self, samples: &[i16], channels: u32, rate: u32, vol: f32) {
        let ch = channels.clamp(1, 2) as usize;
        let frames = samples.len() / ch;
        if frames == 0 || rate == 0 || !vol.is_finite() || self.store.sounds.len() >= 32 {
            return;
        }
        let n = ((frames as u64 * 32_000) / rate as u64).clamp(1, 32_000 * 30) as usize;
        let mut data = Vec::with_capacity(n * 2);
        let at = |k: usize, c: usize| samples[k.min(frames - 1) * ch + c.min(ch - 1)] as f32;
        for i in 0..n {
            let t = i as f64 * rate as f64 / 32_000.0;
            let (k, f) = (t as usize, t.fract() as f32);
            for c in 0..2 {
                data.push((at(k, c) * (1.0 - f) + at(k + 1, c) * f) as i16);
            }
        }
        self.store.sounds.push(Snd { data, pos: 0, vol: vol.clamp(0.0, 4.0) });
    }
    /// Bild anmelden (RGBA); Rückgabe Bild-Nummer (ab 1)
    pub fn image_rgba(&mut self, rgba: &[u8], w: usize, h: usize) -> u32 {
        if w == 0 || h == 0 || w > 4096 || h > 4096 || rgba.len() < w * h * 4 || self.store.images.len() >= 4096 {
            return 0;
        }
        self.store.images.push(Img { w, h, rgba: Arc::new(rgba[..w * h * 4].to_vec()) });
        self.store.images.len() as u32
    }
    pub fn image_png(&mut self, png: &[u8]) -> u32 {
        match decode_png(png) {
            Some((rgba, w, h)) => self.image_rgba(&rgba, w, h),
            None => 0,
        }
    }
    pub fn image(&mut self, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32) {
        if id >= 1 && (id as usize) <= self.store.images.len() {
            self.out.shapes.push(Shape::Image(id, x, y, w, h, tint));
        }
    }
    fn patch_mut(&mut self) -> &mut Patch {
        let cur = self.cur;
        self.store.patch_version += 1;
        if let Some(i) = self.store.patches.iter().position(|p| p.0 == cur) {
            return &mut self.store.patches[i].1;
        }
        self.store.patches.push((cur, Patch::default()));
        &mut self.store.patches.last_mut().unwrap().1
    }
    pub fn vram_patch(&mut self, word_addr: u32, data: &[u16]) {
        if data.is_empty() || word_addr >= 0x8000 {
            return;
        }
        let data = data[..data.len().min(0x8000)].to_vec();
        let p = self.patch_mut();
        // gleiche Stelle ersetzen statt anhängen (Mods setzen ihre Änderung oft jedes Bild neu)
        if let Some(e) = p.vram.iter_mut().find(|e| e.0 == word_addr) {
            e.1 = data;
        } else {
            p.vram.push((word_addr, data));
        }
    }
    pub fn cgram_patch(&mut self, index: u32, colors: &[u16]) {
        if colors.is_empty() || index >= 256 {
            return;
        }
        let colors = colors[..colors.len().min(256)].to_vec();
        let p = self.patch_mut();
        if let Some(e) = p.cgram.iter_mut().find(|e| e.0 == index) {
            e.1 = colors;
        } else {
            p.cgram.push((index, colors));
        }
    }
    pub fn patch_clear(&mut self) {
        let cur = self.cur;
        if self.store.patches.iter().any(|p| p.0 == cur) {
            self.store.patches.retain(|p| p.0 != cur);
            self.store.patch_version += 1;
        }
    }
    /// Wert der App (Launcher)
    pub fn app_get(&self, key: &str) -> i64 {
        self.store.app_vals.get(key).copied().unwrap_or(api::app::NONE)
    }
    /// Wert der App ändern bzw. Aktion auslösen (die App führt es nach dem Bild aus)
    pub fn app_op(&mut self, action: bool, key: &str, value: i64) {
        if self.store.app_ops.len() < 64 {
            self.store.app_ops.push((action, key.to_string(), value));
        }
    }
    /// Wert einer Einstellung der laufenden Mod
    pub fn setting(&self, key: &str) -> i32 {
        let get = |m: &HashMap<String, HashMap<String, i32>>| m.get(&self.id).and_then(|v| v.get(key)).copied();
        get(&self.store.values).or_else(|| get(&self.store.defaults)).unwrap_or(api::setting::UNSET)
    }
    pub fn log(&mut self, t: String) {
        if self.store.logs.len() < 16 {
            self.store.logs.push(t);
        }
    }
}

/// PNG -> RGBA
pub fn decode_png(png: &[u8]) -> Option<(Vec<u8>, usize, usize)> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some((img.into_raw(), w, h))
}

/// PNG -> SNES-Kacheln (2 oder 4 bpp) und Palette (BGR555; Farbe 0 = durchsichtig)
pub fn png_tiles(png: &[u8], bpp: u32) -> Option<(Vec<u16>, Vec<u16>)> {
    if bpp != 2 && bpp != 4 {
        return None;
    }
    let (rgba, w, h) = decode_png(png)?;
    if w % 8 != 0 || h % 8 != 0 {
        return None;
    }
    let max = 1usize << bpp;
    let mut pal: Vec<u16> = vec![0];
    let mut idx = vec![0u8; w * h];
    for (i, p) in rgba.chunks_exact(4).enumerate() {
        if p[3] < 128 {
            continue;
        }
        let c = (p[0] as u16 >> 3) | (p[1] as u16 >> 3) << 5 | (p[2] as u16 >> 3) << 10;
        let k = match pal.iter().skip(1).position(|&x| x == c) {
            Some(k) => k + 1,
            None if pal.len() < max => {
                pal.push(c);
                pal.len() - 1
            }
            // zu viele Farben: nächstliegende nehmen
            None => (1..pal.len()).min_by_key(|&k| {
                let d = |s: u16| ((pal[k] >> s) & 31) as i32 - ((c >> s) & 31) as i32;
                d(0).pow(2) + d(5).pow(2) + d(10).pow(2)
            })?,
        };
        idx[i] = k as u8;
    }
    let mut out = Vec::new();
    for ty in 0..h / 8 {
        for tx in 0..w / 8 {
            let px = |x: usize, y: usize| idx[(ty * 8 + y) * w + tx * 8 + x];
            for plane in 0..(bpp as usize / 2) {
                for y in 0..8 {
                    let (mut lo, mut hi) = (0u16, 0u16);
                    for x in 0..8 {
                        let v = px(x, y) >> (plane * 2);
                        lo |= ((v & 1) as u16) << (7 - x);
                        hi |= (((v >> 1) & 1) as u16) << (7 - x);
                    }
                    out.push(lo | hi << 8);
                }
            }
        }
    }
    pal.resize(max, 0);
    Some((out, pal))
}

/// Eine Mod (eingebaut oder DLL)
pub trait Mod: Send {
    fn pre_frame(&mut self, _c: &mut Ctx) {}
    fn post_frame(&mut self, _c: &mut Ctx) {}
    /// Adressen von Spielroutinen, vor denen `on_hook` laufen soll
    fn hooks(&self) -> Vec<u32> {
        Vec::new()
    }
    fn on_hook(&mut self, _addr: u32, _c: &mut Ctx) {}
}

pub struct Entry {
    /// Schlüssel für die Einstellungen (eingebaut: Name, DLL: „dll:Dateiname“)
    pub id: String,
    pub name_de: String,
    pub name_en: String,
    pub desc_de: String,
    pub desc_en: String,
    /// Spiele (`api::GAMES_…`)
    pub games: u32,
    /// eigenes Bild der Mod (RGBA, Breite, Höhe)
    pub icon: Option<Arc<(Vec<u8>, usize, usize)>>,
    /// Einstellungen, die die Mod anbietet
    pub settings: Vec<SettingDef>,
    pub m: Box<dyn Mod>,
}

struct Inner {
    list: Vec<Entry>,
    frame: u64,
    out: Out,
    store: Store,
    /// eingeschaltete Mods (Index in `list`) und ihre Hooks
    on: Vec<usize>,
    hook_map: HashMap<u32, Vec<usize>>,
}

pub struct Mods {
    inner: Arc<Mutex<Inner>>,
    loaded: Vec<PathBuf>,
    /// Meldungen beim Laden (z. B. falsche Version)
    pub errors: Vec<String>,
    /// zuletzt fertige Zeichnungen / Tempo
    shown: Vec<Shape>,
    speed: f32,
    /// eingeschaltete Kennungen beim letzten Abgleich und Version des Hook-Satzes
    on_ids: Vec<String>,
    hook_version: u64,
    hook_set: Option<Arc<crate::recomp::HookSet>>,
    /// Grafik-Änderungen der eingeschalteten Mods (fürs Spiel), Stand der Änderungen
    patch: Option<Arc<Patch>>,
    patch_version: u64,
    /// Texturen der Mod-Bilder (Bild-Nummer -> Textur)
    textures: Mutex<HashMap<u32, gfx::TextureHandle>>,
    /// Mod-Liste (Download von GitHub)
    pub shop: shop::Shop,
}

impl Mods {
    pub fn new() -> Mods {
        let inner = Inner { list: Vec::new(), frame: 0, out: Out { shapes: Vec::new(), speed: 1.0 }, store: Store::default(), on: Vec::new(), hook_map: HashMap::new() };
        let mut m = Mods {
            inner: Arc::new(Mutex::new(inner)),
            loaded: Vec::new(),
            errors: Vec::new(),
            shown: Vec::new(),
            speed: 1.0,
            on_ids: Vec::new(),
            hook_version: 0,
            hook_set: None,
            patch: None,
            patch_version: u64::MAX,
            textures: Mutex::new(HashMap::new()),
            shop: shop::Shop::default(),
        };
        m.load_dlls();
        m
    }

    /// Ordner für DLL-Mods (neben der .exe)
    pub fn dir() -> PathBuf {
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("mods"))).unwrap_or_else(|| PathBuf::from("mods"))
    }

    /// Ist diese DLL (Dateiname) schon geladen?
    pub fn is_loaded(&self, file: &str) -> bool {
        self.loaded.iter().any(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(file)))
    }

    /// Liste für das Menü: (Kennung, Name, Beschreibung)
    pub fn entries(&self, en: bool) -> Vec<(String, String, String)> {
        let i = self.inner.lock().unwrap();
        i.list.iter().map(|e| (e.id.clone(), if en { e.name_en.clone() } else { e.name_de.clone() }, if en { e.desc_en.clone() } else { e.desc_de.clone() })).collect()
    }

    /// Einstellungen einer geladenen Mod
    pub fn settings_of(&self, id: &str) -> Vec<SettingDef> {
        self.inner.lock().unwrap().list.iter().find(|e| e.id == id).map(|e| e.settings.clone()).unwrap_or_default()
    }

    /// Werte der App für die Mods (`app_get`)
    pub fn set_app_state(&self, v: HashMap<String, i64>) {
        self.inner.lock().unwrap().store.app_vals = v;
    }

    /// Wünsche der Mods an die App abholen: (Aktion?, Schlüssel, Wert)
    pub fn take_app_ops(&self) -> Vec<(bool, String, i64)> {
        std::mem::take(&mut self.inner.lock().unwrap().store.app_ops)
    }

    /// Werte der Einstellungen aller Mods übernehmen (aus den App-Einstellungen)
    pub fn set_values(&self, v: &HashMap<String, HashMap<String, i32>>) {
        self.inner.lock().unwrap().store.values = v.clone();
    }

    /// Spiele und Bild einer geladenen Mod (Kennung wie in `entries`)
    pub fn meta(&self, id: &str) -> Option<(u32, Option<Arc<(Vec<u8>, usize, usize)>>)> {
        let i = self.inner.lock().unwrap();
        i.list.iter().find(|e| e.id == id).map(|e| (e.games, e.icon.clone()))
    }

    /// Neue DLLs aus dem Mod-Ordner laden (schon geladene bleiben)
    pub fn load_dlls(&mut self) {
        // Updates aus dem Shop (`x.dll.new`): alte DLL entladen, ersetzen – gilt sofort, ohne Neustart
        if let Ok(rd) = std::fs::read_dir(Self::dir()) {
            for p in rd.filter_map(|e| e.ok().map(|e| e.path())) {
                if p.extension().is_some_and(|e| e == "new") {
                    let dll = p.with_extension("");
                    if let Some(f) = dll.file_name().map(|f| f.to_string_lossy().to_string()) {
                        self.unload(&f);
                    }
                    if std::fs::rename(&p, &dll).is_err() {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }
        }
        let Ok(rd) = std::fs::read_dir(Self::dir()) else { return };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll"))).collect();
        files.sort();
        for p in files {
            if self.loaded.contains(&p) {
                continue;
            }
            self.loaded.push(p.clone());
            match dll::load(&p) {
                Ok(e) => {
                    let mut i = self.inner.lock().unwrap();
                    let d: HashMap<String, i32> = e.settings.iter().map(|s| (s.key.clone(), s.def)).collect();
                    i.store.defaults.insert(e.id.clone(), d);
                    i.list.push(e);
                }
                Err(e) => self.errors.push(format!("{}: {e}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())),
            }
        }
        // neuer Abgleich beim nächsten Bild
        self.on_ids = vec!["\u{0}".into()];
    }

    /// Mod-DLL entladen (Dateiname, z. B. `crown.dll`); Rückgabe: war geladen
    pub fn unload(&mut self, file: &str) -> bool {
        let id = format!("dll:{file}");
        let mut i = self.inner.lock().unwrap();
        let before = i.list.len();
        i.list.retain(|e| !e.id.eq_ignore_ascii_case(&id));
        let removed = i.list.len() != before;
        if removed {
            // Nummern in den Listen stimmen nicht mehr: beim nächsten Bild neu abgleichen
            i.on.clear();
            i.hook_map.clear();
            i.store.patches.clear();
            i.store.patch_version += 1;
        }
        drop(i);
        self.loaded.retain(|p| !p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(file)));
        if removed {
            self.on_ids = vec!["\u{0}".into()];
            self.hook_set = None;
            self.hook_version += 1;
            self.shown.clear();
        }
        removed
    }

    /// Mod deinstallieren: entladen, DLL und Bild aus dem Mod-Ordner löschen
    pub fn uninstall(&mut self, file: &str) -> Result<(), String> {
        if file.contains(['/', '\\']) || !file.to_ascii_lowercase().ends_with(".dll") {
            return Err("ungültiger Dateiname".into());
        }
        self.unload(file);
        let d = Self::dir();
        let dll = d.join(file);
        for p in [dll.with_extension("dll.new"), dll.with_extension("png")] {
            let _ = std::fs::remove_file(p);
        }
        if dll.exists() {
            std::fs::remove_file(&dll).map_err(|e| e.to_string())?;
        }
        self.shop.forget(file);
        Ok(())
    }

    /// Eingeschaltete Mods übernehmen und die Hooks am Spiel anbringen (nur im Recompiled-Modus)
    pub fn attach(&mut self, snes: &mut Snes, on: &[String]) {
        if !snes.rc_active() {
            snes.hooks.0 = None;
            self.shown.clear();
            self.speed = 1.0;
            return;
        }
        if self.on_ids != on {
            self.on_ids = on.to_vec();
            let mut i = self.inner.lock().unwrap();
            let idx: Vec<usize> = (0..i.list.len()).filter(|&k| on.contains(&i.list[k].id) && i.list[k].games & api::GAMES_ALTTP != 0).collect();
            // Grafik-Änderungen ausgeschalteter Mods entfernen
            let before = i.store.patches.len();
            i.store.patches.retain(|p| idx.contains(&p.0));
            if i.store.patches.len() != before {
                i.store.patch_version += 1;
            }
            let mut map: HashMap<u32, Vec<usize>> = HashMap::new();
            for &k in &idx {
                for a in i.list[k].m.hooks() {
                    map.entry(a & 0xFFFFFF).or_default().push(k);
                }
            }
            i.on = idx;
            let addrs: Vec<u32> = map.keys().copied().collect();
            i.hook_map = map;
            drop(i);
            self.hook_version += 1;
            self.hook_set = (!addrs.is_empty()).then(|| {
                let inner = self.inner.clone();
                let sink = Box::new(move |a: u32, c: &mut Cpu, b: &mut Bus| {
                    let mut g = inner.lock().unwrap();
                    let g = &mut *g;
                    let Some(ks) = g.hook_map.get(&a).cloned() else { return };
                    for k in ks {
                        let mut ctx = Ctx { sys: Sys::Snes { cpu: &mut *c, bus: &mut *b }, id: g.list[k].id.clone(), frame: g.frame, out: &mut g.out, store: &mut g.store, cur: k };
                        g.list[k].m.on_hook(a, &mut ctx);
                    }
                });
                Arc::new(crate::recomp::HookSet::new(&addrs, self.hook_version, sink))
            });
        }
        let cur = snes.hooks.0.as_ref().map(|h| h.version);
        if cur != self.hook_set.as_ref().map(|h| h.version) {
            snes.hooks.0 = self.hook_set.clone();
        }
    }

    fn run(&self, snes: &mut Snes, post: bool) {
        let mut g = self.inner.lock().unwrap();
        let g = &mut *g;
        if !post {
            g.out.shapes.clear();
            g.out.speed = 1.0;
        }
        for &k in &g.on {
            let mut ctx = Ctx { sys: Sys::Snes { cpu: &mut snes.cpu, bus: &mut snes.bus }, id: g.list[k].id.clone(), frame: g.frame, out: &mut g.out, store: &mut g.store, cur: k };
            if post {
                g.list[k].m.post_frame(&mut ctx);
            } else {
                g.list[k].m.pre_frame(&mut ctx);
            }
        }
        if post {
            g.frame += 1;
        }
    }

    /// vor einem Spielbild (Mods laufen nur im Recompiled-Modus)
    pub fn pre_frame(&mut self, snes: &mut Snes, on: &[String]) {
        if self.shop.take_new() {
            self.load_dlls();
        }
        self.attach(snes, on);
        let rc = snes.rc_active() && !on.is_empty();
        if rc {
            self.run(snes, false);
        }
        // Grafik-Änderungen ans Spiel geben (angewendet vor dem Zeichnen jedes Bildes)
        {
            let g = self.inner.lock().unwrap();
            let v = if rc { g.store.patch_version } else { u64::MAX - 1 };
            if v != self.patch_version {
                self.patch_version = v;
                let mut p = Patch::default();
                if rc {
                    for (_, x) in &g.store.patches {
                        p.vram.extend(x.vram.iter().cloned());
                        p.cgram.extend(x.cgram.iter().cloned());
                    }
                }
                self.patch = (!p.is_empty()).then(|| Arc::new(p));
            }
        }
        if snes.mod_patch.as_ref().map(Arc::as_ptr) != self.patch.as_ref().map(Arc::as_ptr) {
            snes.mod_patch = self.patch.clone();
        }
    }

    /// The Minish Cap: Mods vor (`post` = false) bzw. nach einem Spielbild laufen lassen; `pad` kann die Mod ändern
    #[cfg(pastlink_gba)]
    pub fn gba_frame(&mut self, g: &mut crate::gba::Gba, pad: &mut u16, on: &[String], post: bool) {
        if !post && self.shop.take_new() {
            self.load_dlls();
        }
        if on.is_empty() || !g.is_rc() || g.loading() {
            self.shown.clear();
            self.speed = 1.0;
            return;
        }
        let view = g.map_view();
        let menu = g.menu_item().is_some();
        let mut inner = self.inner.lock().unwrap();
        let x = &mut *inner;
        if !post {
            x.out.shapes.clear();
            x.out.speed = 1.0;
        }
        let idx: Vec<usize> = (0..x.list.len()).filter(|&k| on.contains(&x.list[k].id) && x.list[k].games & api::GAMES_MINISH != 0).collect();
        for k in idx {
            let mut ctx = Ctx { sys: Sys::Gba { g: &mut *g, pad: &mut *pad, view, menu }, id: x.list[k].id.clone(), frame: x.frame, out: &mut x.out, store: &mut x.store, cur: k };
            if post {
                x.list[k].m.post_frame(&mut ctx);
            } else {
                x.list[k].m.pre_frame(&mut ctx);
            }
        }
        if post {
            x.frame += 1;
            self.shown.clone_from(&x.out.shapes);
            self.speed = x.out.speed;
        }
    }

    /// Töne der Mods in die Tonausgabe mischen (32 kHz Stereo, verschachtelt)
    pub fn mix(&self, buf: &mut [i16]) {
        let mut g = self.inner.lock().unwrap();
        if g.store.sounds.is_empty() {
            return;
        }
        for s in g.store.sounds.iter_mut() {
            for (o, v) in buf.iter_mut().zip(s.data[s.pos..].iter()) {
                *o = (*o as f32 + *v as f32 * s.vol).clamp(-32768.0, 32767.0) as i16;
            }
            s.pos = (s.pos + buf.len()).min(s.data.len());
        }
        g.store.sounds.retain(|s| s.pos < s.data.len());
    }

    /// Meldungen der Mods abholen
    pub fn take_logs(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.lock().unwrap().store.logs)
    }

    /// nach einem Spielbild
    pub fn post_frame(&mut self, snes: &mut Snes, on: &[String]) {
        if !snes.rc_active() || on.is_empty() {
            self.shown.clear();
            self.speed = 1.0;
            return;
        }
        self.run(snes, true);
        let g = self.inner.lock().unwrap();
        self.shown.clone_from(&g.out.shapes);
        self.speed = g.out.speed;
    }

    /// Spieltempo, das die Mods wünschen (1.0 = normal)
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Zeichnungen der Mods über das Spielbild legen. `r` = Spielbild auf dem Schirm, `view_w` = Breite in
    /// SNES-Pixeln (mit Breitbild-Rand `ext` links und rechts)
    pub fn draw(&self, p: &gfx::Painter, r: gfx::Rect, view_w: usize, ext: usize) {
        self.draw_h(p, r, view_w, 224, ext)
    }

    /// wie `draw`, mit Bildhöhe in Spielpixeln (The Minish Cap: 160)
    pub fn draw_h(&self, p: &gfx::Painter, r: gfx::Rect, view_w: usize, view_h: usize, ext: usize) {
        if self.shown.is_empty() || view_w == 0 || view_h == 0 {
            return;
        }
        let kx = r.width() / view_w as f32;
        let ky = r.height() / view_h as f32;
        let at = |x: i32, y: i32| gfx::pos2(r.left() + (x as f32 + ext as f32) * kx, r.top() + y as f32 * ky);
        let col = |c: u32| Color32::from_rgba_unmultiplied((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8);
        let p = p.with_clip_rect(r);
        let fid = gfx::FontId::monospace((7.0 * ky).max(10.0));
        for s in &self.shown {
            match s {
                Shape::Image(id, x, y, w, h, tint) => {
                    let (t, iw, ih) = {
                        let g = self.inner.lock().unwrap();
                        let Some(img) = g.store.images.get(*id as usize - 1) else { continue };
                        let mut tex = self.textures.lock().unwrap();
                        let t = tex.entry(*id).or_insert_with(|| p.ctx().load_texture(format!("mod-img-{id}"), gfx::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba), gfx::TextureOptions::NEAREST)).clone();
                        (t, img.w as i32, img.h as i32)
                    };
                    let (w, h) = (if *w > 0 { *w } else { iw }, if *h > 0 { *h } else { ih });
                    let rect = gfx::Rect::from_two_pos(at(*x, *y), at(x + w, y + h));
                    p.image(t, rect, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), col(*tint));
                }
                Shape::Rect(x, y, w, h, c) => p.rect_filled(gfx::Rect::from_two_pos(at(*x, *y), at(x + w, y + h)), 0.0, col(*c)),
                Shape::Text(x, y, c, t) => {
                    let pos = at(*x, *y);
                    p.text(pos + gfx::vec2(1.0, 1.0), gfx::Align2::LEFT_TOP, t, fid.clone(), Color32::from_black_alpha(200));
                    p.text(pos, gfx::Align2::LEFT_TOP, t, fid.clone(), col(*c));
                }
            }
        }
    }
}

/// Mod-Vorlage (Beispielprojekt + Doku) in `dir/pastlink-mod-vorlage` schreiben
pub fn export_template(dir: &Path) -> std::io::Result<PathBuf> {
    let root = dir.join("zelda-core-mod-vorlage");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("Cargo.toml"), include_str!("../../assets/mod_template/Cargo.toml.txt"))?;
    std::fs::write(root.join("README.md"), include_str!("../../assets/mod_template/README.md"))?;
    std::fs::write(root.join("src").join("api.rs"), include_str!("../../assets/mod_template/src/api.rs"))?;
    std::fs::write(root.join("src").join("lib.rs"), include_str!("../../assets/mod_template/src/lib.rs"))?;
    std::fs::write(root.join(".gitignore"), "target/\n")?;
    Ok(root)
}

/// DLL-Mods über die C-Schnittstelle
mod dll {
    use super::api::{Api, ModInfo, PASTLINK_MOD_ABI};
    use super::*;
    use windows_sys::Win32::Foundation::FreeLibrary;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    struct DllMod {
        info: &'static ModInfo,
        /// geladene DLL (wird beim Entfernen der Mod wieder freigegeben)
        module: usize,
    }

    impl Drop for DllMod {
        fn drop(&mut self) {
            if self.module != 0 {
                unsafe { FreeLibrary(self.module as _) };
            }
        }
    }

    fn cx<'a>(ctx: *mut c_void) -> &'a mut Ctx<'a> {
        unsafe { &mut *(ctx as *mut Ctx) }
    }
    extern "C" fn read8(ctx: *mut c_void, addr: u32) -> u8 {
        cx(ctx).read8(addr)
    }
    extern "C" fn write8(ctx: *mut c_void, addr: u32, v: u8) {
        cx(ctx).write8(addr, v)
    }
    extern "C" fn pad(ctx: *mut c_void) -> u16 {
        cx(ctx).pad()
    }
    extern "C" fn set_pad(ctx: *mut c_void, p: u16) {
        cx(ctx).set_pad(p)
    }
    extern "C" fn mem_read(ctx: *mut c_void, sp: u32, addr: u32) -> u8 {
        cx(ctx).mem_read(sp, addr)
    }
    extern "C" fn mem_write(ctx: *mut c_void, sp: u32, addr: u32, v: u8) {
        cx(ctx).mem_write(sp, addr, v)
    }
    extern "C" fn reg(ctx: *mut c_void, r: u32) -> u32 {
        cx(ctx).reg(r)
    }
    extern "C" fn set_reg(ctx: *mut c_void, r: u32, v: u32) {
        cx(ctx).set_reg(r, v)
    }
    extern "C" fn ret(ctx: *mut c_void, long: u32) {
        cx(ctx).ret(long != 0)
    }
    extern "C" fn draw_rect(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, c: u32) {
        cx(ctx).rect(x, y, w, h, c)
    }
    extern "C" fn draw_text(ctx: *mut c_void, x: i32, y: i32, t: *const std::ffi::c_char, c: u32) {
        if !t.is_null() {
            let s = unsafe { CStr::from_ptr(t) }.to_string_lossy().into_owned();
            cx(ctx).text(x, y, c, s)
        }
    }
    extern "C" fn set_speed(ctx: *mut c_void, s: f32) {
        cx(ctx).set_speed(s)
    }
    fn slice<'a, T>(p: *const T, n: u32) -> &'a [T] {
        if p.is_null() || n == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(p, n as usize) }
        }
    }
    extern "C" fn play_sound(ctx: *mut c_void, s: *const i16, frames: u32, ch: u32, rate: u32, vol: f32) {
        let ch = ch.clamp(1, 2);
        cx(ctx).play_sound(slice(s, frames.saturating_mul(ch).min(32_000 * 60 * 2)), ch, rate, vol)
    }
    extern "C" fn image_png(ctx: *mut c_void, png: *const u8, len: u32) -> u32 {
        cx(ctx).image_png(slice(png, len))
    }
    extern "C" fn image_rgba(ctx: *mut c_void, rgba: *const u8, w: u32, h: u32) -> u32 {
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return 0;
        }
        cx(ctx).image_rgba(slice(rgba, w * h * 4), w as usize, h as usize)
    }
    extern "C" fn draw_image(ctx: *mut c_void, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32) {
        cx(ctx).image(id, x, y, w, h, tint)
    }
    extern "C" fn vram_patch(ctx: *mut c_void, a: u32, d: *const u16, n: u32) {
        cx(ctx).vram_patch(a, slice(d, n.min(0x8000)))
    }
    extern "C" fn cgram_patch(ctx: *mut c_void, i: u32, d: *const u16, n: u32) {
        cx(ctx).cgram_patch(i, slice(d, n.min(256)))
    }
    extern "C" fn patch_clear(ctx: *mut c_void) {
        cx(ctx).patch_clear()
    }
    extern "C" fn png_tiles(_ctx: *mut c_void, png: *const u8, len: u32, bpp: u32, out: *mut u16, max: u32, pal: *mut u16) -> u32 {
        let Some((t, p)) = super::png_tiles(slice(png, len), bpp) else { return 0 };
        if out.is_null() || t.len() > max as usize {
            return 0;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(t.as_ptr(), out, t.len());
            if !pal.is_null() {
                std::ptr::copy_nonoverlapping(p.as_ptr(), pal, p.len());
            }
        }
        t.len() as u32
    }
    extern "C" fn setting(ctx: *mut c_void, key: *const std::ffi::c_char) -> i32 {
        if key.is_null() {
            return super::api::setting::UNSET;
        }
        let k = unsafe { CStr::from_ptr(key) }.to_string_lossy();
        cx(ctx).setting(&k)
    }
    fn key_of(k: *const std::ffi::c_char) -> Option<String> {
        (!k.is_null()).then(|| unsafe { CStr::from_ptr(k) }.to_string_lossy().into_owned())
    }
    extern "C" fn app_get(ctx: *mut c_void, key: *const std::ffi::c_char) -> i64 {
        key_of(key).map_or(super::api::app::NONE, |k| cx(ctx).app_get(&k))
    }
    extern "C" fn app_set(ctx: *mut c_void, key: *const std::ffi::c_char, v: i64) {
        if let Some(k) = key_of(key) {
            cx(ctx).app_op(false, &k, v)
        }
    }
    extern "C" fn app_do(ctx: *mut c_void, key: *const std::ffi::c_char, v: i64) {
        if let Some(k) = key_of(key) {
            cx(ctx).app_op(true, &k, v)
        }
    }
    extern "C" fn log(ctx: *mut c_void, t: *const std::ffi::c_char) {
        if !t.is_null() {
            let s = unsafe { CStr::from_ptr(t) }.to_string_lossy().into_owned();
            cx(ctx).log(s)
        }
    }

    fn api(c: &mut Ctx) -> Api {
        Api {
            abi: PASTLINK_MOD_ABI,
            ctx: c as *mut Ctx as *mut c_void,
            frame: c.frame,
            read8,
            write8,
            pad,
            set_pad,
            mem_read,
            mem_write,
            reg,
            set_reg,
            ret,
            draw_rect,
            draw_text,
            set_speed,
            play_sound,
            image_png,
            image_rgba,
            draw_image,
            vram_patch,
            cgram_patch,
            patch_clear,
            png_tiles,
            log,
            game: c.game(),
            screen_w: c.screen().0,
            screen_h: c.screen().1,
            setting,
            app_get,
            app_set,
            app_do,
        }
    }

    impl Mod for DllMod {
        fn pre_frame(&mut self, c: &mut Ctx) {
            if let Some(f) = self.info.pre_frame {
                f(&api(c));
            }
        }
        fn post_frame(&mut self, c: &mut Ctx) {
            if let Some(f) = self.info.post_frame {
                f(&api(c));
            }
        }
        fn hooks(&self) -> Vec<u32> {
            if self.info.hooks.is_null() || self.info.on_hook.is_none() {
                return Vec::new();
            }
            unsafe { std::slice::from_raw_parts(self.info.hooks, self.info.hook_count as usize) }.to_vec()
        }
        fn on_hook(&mut self, addr: u32, c: &mut Ctx) {
            if let Some(f) = self.info.on_hook {
                f(&api(c), addr);
            }
        }
    }

    pub fn load(p: &Path) -> Result<Entry, String> {
        let wide: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            // bleibt geladen, bis die Mod entfernt wird (Update, Deinstallieren)
            let h = LoadLibraryW(wide.as_ptr());
            if h.is_null() {
                return Err("DLL lässt sich nicht laden".into());
            }
            let f = GetProcAddress(h, c"pastlink_mod".as_ptr() as *const u8).ok_or("keine Funktion pastlink_mod")?;
            let f: extern "C" fn() -> *const ModInfo = std::mem::transmute(f);
            let info = f();
            if info.is_null() {
                return Err("pastlink_mod liefert nichts".into());
            }
            let info: &'static ModInfo = &*info;
            // Version 3 läuft weiter (nur A Link to the Past), neuere Mods brauchen eine neuere App
            if !(3..=PASTLINK_MOD_ABI).contains(&info.abi) {
                return Err(format!("Schnittstelle {} statt {} – bitte mit der aktuellen Vorlage neu bauen bzw. Zelda Core aktualisieren", info.abi, PASTLINK_MOD_ABI));
            }
            // ab Version 4: für welche Spiele (sonst nur A Link to the Past) und eigenes Bild (PNG)
            let games = match GetProcAddress(h, c"pastlink_mod_games".as_ptr() as *const u8) {
                Some(g) => {
                    let g: extern "C" fn() -> u32 = std::mem::transmute(g);
                    g() & (super::api::GAMES_ALTTP | super::api::GAMES_MINISH)
                }
                None => super::api::GAMES_ALTTP,
            };
            let mut icon = None;
            if let Some(ic) = GetProcAddress(h, c"pastlink_mod_icon".as_ptr() as *const u8) {
                let ic: extern "C" fn(*mut u32) -> *const u8 = std::mem::transmute(ic);
                let mut n = 0u32;
                let d = ic(&mut n);
                if !d.is_null() && n > 0 && n < 1 << 20 {
                    icon = decode_png(std::slice::from_raw_parts(d, n as usize)).map(Arc::new);
                }
            }
            let text = |c: *const std::ffi::c_char| if c.is_null() { String::new() } else { CStr::from_ptr(c).to_string_lossy().into_owned() };
            // ab Version 5: Einstellungen
            let mut settings = Vec::new();
            if let Some(st) = GetProcAddress(h, c"pastlink_mod_settings".as_ptr() as *const u8) {
                let st: extern "C" fn(*mut u32) -> *const super::api::Setting = std::mem::transmute(st);
                let mut n = 0u32;
                let d = st(&mut n);
                if !d.is_null() && n < 64 {
                    for x in std::slice::from_raw_parts(d, n as usize) {
                        let key = text(x.key);
                        if key.is_empty() {
                            continue;
                        }
                        let label = text(x.label);
                        let (de, en) = label.split_once('|').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((label.clone(), label.clone()));
                        settings.push(SettingDef { key, label_de: de, label_en: en, kind: x.kind, def: x.def, min: x.min, max: x.max });
                    }
                }
            }
            // sonst: gleichnamiges PNG neben der DLL (z. B. aus der Mod-Liste geholt)
            if icon.is_none() {
                icon = std::fs::read(p.with_extension("png")).ok().and_then(|b| decode_png(&b)).map(Arc::new);
            }
            let name = text(info.name);
            let desc = text(info.description);
            let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            Ok(Entry { id: format!("dll:{file}"), name_de: name.clone(), name_en: name, desc_de: desc.clone(), desc_en: desc, games, icon, settings, m: Box::new(DllMod { info, module: h as usize }) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vorlage exportieren (`PASTLINK_OUT`), danach dort `cargo build --release`
    #[test]
    #[ignore]
    fn dev_mod_export() {
        let out = PathBuf::from(std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into()));
        println!("{}", export_template(&out).unwrap().display());
    }

    /// Spielstand (Kopie) im RC-Modus: `z3` = Zelda 3, sonst der eigene Recompiler
    fn game(z3: bool) -> Option<Snes> {
        let (rom, sram, path) = crate::recomp::app_rom_for_tests()?;
        let st = std::fs::read(path.with_extension("ss1")).ok()?;
        let mut s = Snes::new(Arc::new(rom), sram, false);
        s.load_state(&st).ok()?;
        for _ in 0..3 {
            s.run_frame();
        }
        if z3 {
            crate::z3::sync(&mut s, true).ok()?;
        } else {
            crate::recomp::sync(&mut s, true);
        }
        Some(s)
    }

    fn frames(m: &mut Mods, s: &mut Snes, on: &[String], n: u32, pad: u16) {
        for _ in 0..n {
            s.bus.joy[0] = pad;
            m.pre_frame(s, on);
            s.run_frame();
            m.post_frame(s, on);
            s.bus.apu.dsp.out.clear();
        }
    }

    /// Alle Mod-DLLs aus `PASTLINK_MODS_DIR` (z. B. mods/target/release) laden
    fn load_all() -> Mods {
        let dir = PathBuf::from(std::env::var("PASTLINK_MODS_DIR").expect("PASTLINK_MODS_DIR fehlt"));
        let mut m = Mods::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "dll")).collect();
        files.sort();
        for p in files {
            let e = dll::load(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            println!("geladen: {:20} {}", e.id, e.name_de);
            m.inner.lock().unwrap().list.push(e);
        }
        m
    }

    fn id(n: &str) -> Vec<String> {
        vec![format!("dll:{n}.dll")]
    }

    /// The Minish Cap: Mods auf einem Spielstand (`GBA_RC_STATE`, lz4) prüfen
    #[cfg(pastlink_gba)]
    #[test]
    #[ignore]
    fn dev_mods_minish() {
        use crate::gba::Gba;
        let (Ok(rom), Ok(st)) = (std::env::var("PASTLINK_GBA_ROM"), std::env::var("GBA_RC_STATE")) else { return };
        let mut g = Gba::load_with(Path::new(&rom), true, false).unwrap();
        while g.loading() {
            g.run_frame(0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let raw = lz4_flex::decompress_size_prepended(&std::fs::read(st).unwrap()).unwrap();
        assert!(g.load_state(&raw));
        let mut m = load_all();
        let run = |m: &mut Mods, g: &mut Gba, on: &[String], n: u32, pad: u16| {
            for _ in 0..n {
                let mut p = pad;
                m.gba_frame(g, &mut p, on, false);
                g.run_frame(p);
                m.gba_frame(g, &mut p, on, true);
                let mut a = Vec::new();
                g.take_audio(&mut a);
            }
        };
        run(&mut m, &mut g, &[], 30, 0);
        let info = |g: &Gba| g.map_view();
        println!("Ansicht {:?}", info(&g));
        const HP: u32 = 0x0200_2AEA;
        const RUP: u32 = 0x0200_2B00;
        let rd16 = |g: &Gba, a: u32| g.peek(a, 2).map_or(0, |b| b[0] as u16 | (b[1] as u16) << 8);
        // Zeichnende Mods
        for n in ["speedrun", "info_hud", "day_night", "crown"] {
            run(&mut m, &mut g, &id(n), 20, 0);
            println!("{n}: {} Zeichnungen", m.shown.len());
            assert!(!m.shown.is_empty() || n == "day_night", "{n} zeichnet nichts");
        }
        if let Some(Shape::Image(_, x, y, ..)) = m.shown.iter().find(|s| matches!(s, Shape::Image(..))) {
            println!("Krone bei {x},{y}");
        }
        // Herz-Regeneration: Leben runter, nach 8 s wieder mehr
        let max = g.peek(HP + 1, 1).unwrap()[0];
        g.poke(HP, &[max - 12]);
        run(&mut m, &mut g, &id("heart_regen"), 500, 0);
        let hp = g.peek(HP, 1).unwrap()[0];
        println!("heart_regen: {} -> {hp} (max {max})", max - 12);
        assert!(hp > max - 12);
        // Doppelte Rubine
        let on = id("double_rupees");
        run(&mut m, &mut g, &on, 2, 0);
        let r0 = rd16(&g, RUP);
        g.poke(RUP, &((r0 + 5) as u16).to_le_bytes());
        run(&mut m, &mut g, &on, 2, 0);
        println!("double_rupees: {r0} -> {}", rd16(&g, RUP));
        assert!(rd16(&g, RUP) == r0 + 10 || rd16(&g, RUP) == [100, 300, 500, 999][g.peek(0x0200_2AE8, 1).unwrap()[0].min(3) as usize]);
        // Münz-Klang
        let on = id("coin_sound");
        run(&mut m, &mut g, &on, 2, 0);
        let r1 = rd16(&g, RUP).saturating_sub(5);
        g.poke(RUP, &r1.to_le_bytes());
        run(&mut m, &mut g, &on, 1, 0);
        g.poke(RUP, &(r1 + 1).to_le_bytes());
        run(&mut m, &mut g, &on, 1, 0);
        let mut buf = vec![0i16; 1066];
        m.mix(&mut buf);
        let peak = buf.iter().map(|v| v.unsigned_abs()).max().unwrap();
        println!("coin_sound: Spitze {peak}");
        assert!(peak > 1000);
        // Voller Vorrat
        g.poke(0x0200_2AEC, &[3, 4]);
        run(&mut m, &mut g, &id("full_supply"), 2, 0);
        println!("full_supply: Bomben/Pfeile {:?}", g.peek(0x0200_2AEC, 2).unwrap());
        assert!(g.peek(0x0200_2AEC, 1).unwrap()[0] >= 10);
        // Zeitlupe mit L
        run(&mut m, &mut g, &id("slowmo"), 2, 0x0020);
        println!("slowmo: Tempo {}", m.speed());
        assert!(m.speed() < 1.0);
        // Doppelter Schaden
        let on = id("hard_damage");
        g.poke(HP, &[max]);
        run(&mut m, &mut g, &on, 2, 0);
        g.poke(HP, &[max - 8]);
        run(&mut m, &mut g, &on, 2, 0);
        let hp = g.peek(HP, 1).unwrap()[0];
        println!("hard_damage: {max} -> {hp}");
        assert_eq!(hp, max - 16);
        // Ein-Treffer-Modus
        let on = id("one_hit");
        run(&mut m, &mut g, &on, 2, 0);
        g.poke(HP, &[hp - 4]);
        run(&mut m, &mut g, &on, 2, 0);
        println!("one_hit: Leben {}", g.peek(HP, 1).unwrap()[0]);
        assert_eq!(g.peek(HP, 1).unwrap()[0], 0);
        // nur-ALttP-Mods laufen in Minish Cap nicht
        run(&mut m, &mut g, &id("enemy_hp"), 5, 0);
        assert!(m.shown.is_empty());
        let ic = m.inner.lock().unwrap().list.iter().filter(|e| e.icon.is_some()).count();
        println!("Mods mit Bild: {ic}");
        // Einstellungen: Speedrun-Timer an gezogener Position, Zeitlupe mit 30 %
        println!("Einstellungen speedrun: {:?}", m.settings_of("dll:speedrun.dll").iter().map(|s| s.key.clone()).collect::<Vec<_>>());
        let mut vals: HashMap<String, HashMap<String, i32>> = HashMap::new();
        vals.entry("dll:speedrun.dll".into()).or_default().insert("pos".into(), api::setting::pos(120, 90));
        vals.entry("dll:slowmo.dll".into()).or_default().insert("percent".into(), 30);
        m.set_values(&vals);
        run(&mut m, &mut g, &id("speedrun"), 3, 0);
        let at = m.shown.iter().find_map(|s| if let Shape::Rect(x, y, ..) = s { Some((*x, *y)) } else { None });
        println!("speedrun an {at:?}");
        assert_eq!(at, Some((120, 90)));
        run(&mut m, &mut g, &id("slowmo"), 2, 0x0020);
        println!("slowmo 30 %: Tempo {}", m.speed());
        assert!((m.speed() - 0.3).abs() < 0.01);
        // Speedrun-Timer zeigt die Spielzeit des Spielstands aus der App (1 Minute)
        m.set_app_state(HashMap::from([("play_frames".to_string(), 3600i64), ("fps_x100".to_string(), 6000)]));
        run(&mut m, &mut g, &id("speedrun"), 2, 0);
        let txt = m.shown.iter().find_map(|s| if let Shape::Text(_, _, _, t) = s { Some(t.clone()) } else { None });
        println!("speedrun Text {txt:?}");
        assert_eq!(txt.as_deref(), Some("01:00.00"));
        // Entladen und wieder laden (Update ohne Neustart)
        let n0 = m.entries(false).len();
        assert!(m.unload("speedrun.dll"));
        assert_eq!(m.entries(false).len(), n0 - 1);
        let e = dll::load(&PathBuf::from(std::env::var("PASTLINK_MODS_DIR").unwrap()).join("speedrun.dll")).unwrap();
        m.inner.lock().unwrap().list.push(e);
        run(&mut m, &mut g, &id("speedrun"), 3, 0);
        println!("nach Neuladen: {} Zeichnungen", m.shown.len());
        assert!(!m.shown.is_empty());
    }

    /// Update einer geladenen Mod: entladen, Datei ersetzen, neu laden (wie die Mod-Liste)
    #[test]
    #[ignore]
    fn dev_mod_update() {
        let src = PathBuf::from(std::env::var("PASTLINK_MODS_DIR").expect("PASTLINK_MODS_DIR fehlt"));
        let dir = Mods::dir();
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("speedrun.dll");
        let _ = std::fs::remove_file(&dll);
        std::fs::copy(src.join("heart_regen.dll"), &dll).unwrap();
        let mut m = Mods::new();
        println!("vorher: {:?}", m.entries(false).iter().find(|e| e.0 == "dll:speedrun.dll").map(|e| e.1.clone()));
        // neue Fassung als .new ablegen, dann wie nach dem Download neu laden
        std::fs::copy(src.join("speedrun.dll"), dir.join("speedrun.dll.new")).unwrap();
        m.load_dlls();
        let now = m.entries(false).iter().find(|e| e.0 == "dll:speedrun.dll").map(|e| e.1.clone());
        println!("nachher: {now:?}, .new noch da: {}, Einstellungen: {}", dir.join("speedrun.dll.new").exists(), m.settings_of("dll:speedrun.dll").len());
        assert_eq!(now.as_deref(), Some("Speedrun-Timer"));
        assert!(!m.settings_of("dll:speedrun.dll").is_empty());
        // Deinstallieren
        m.uninstall("speedrun.dll").unwrap();
        println!("deinstalliert: Datei da {}", dll.exists());
        assert!(!dll.exists());
    }

    /// Alle Mods im RC-Modus prüfen (`Z3=1`: im Zelda-3-Modus)
    #[test]
    #[ignore]
    fn dev_mods() {
        let z3 = std::env::var("Z3").is_ok();
        let Some(mut s) = game(z3) else { return };
        assert!(s.rc_active(), "RC-Modus nicht verfügbar");
        let mut m = load_all();
        // Overlay-Mods liefern Zeichnungen
        for n in ["speedrun", "info_hud", "day_night"] {
            frames(&mut m, &mut s, &id(n), 120, 0x0100);
            println!("{n}: {} Zeichnungen", m.shown.len());
            assert!(!m.shown.is_empty() || n == "day_night", "{n} zeichnet nichts");
        }
        // Krone: Bild über Link
        frames(&mut m, &mut s, &id("crown"), 3, 0);
        let img = m.shown.iter().any(|x| matches!(x, Shape::Image(..)));
        println!("crown: Bild gezeichnet {img}");
        assert!(img);
        // Münz-Klang: Rubine dazu -> Ton
        frames(&mut m, &mut s, &id("coin_sound"), 2, 0);
        let r = s.bus.wram[0xF360] as u16 | (s.bus.wram[0xF361] as u16) << 8;
        s.bus.wram[0xF360] = (r + 5) as u8;
        s.bus.wram[0xF361] = ((r + 5) >> 8) as u8;
        frames(&mut m, &mut s, &id("coin_sound"), 1, 0);
        let mut buf = vec![0i16; 1066];
        m.mix(&mut buf);
        let peak = buf.iter().map(|v| v.unsigned_abs()).max().unwrap();
        println!("coin_sound: Spitze {peak}");
        assert!(peak > 1000);
        // Regenbogen-Tunika: Palette von Link geändert und im Bild angewendet
        frames(&mut m, &mut s, &[], 1, 0);
        let src: Vec<u16> = s.bus.ppu.cgram[0xF0..0x100].to_vec();
        frames(&mut m, &mut s, &id("tunic_colors"), 60, 0);
        let now: Vec<u16> = s.bus.ppu.cgram[0xF0..0x100].to_vec();
        let changed = (0..16).filter(|&k| src[k] != now[k]).count();
        println!("tunic_colors: {changed} Farben geändert, Patch aktiv {}", s.mod_patch.is_some());
        println!("  Quelle {:04X?}", src);
        println!("  jetzt  {:04X?}", now);
        crate::devtest::shot(&s, if z3 { "tunic_z3" } else { "tunic_rc" });
        assert!(changed > 0 && s.mod_patch.is_some());
        // ausschalten: Patch weg
        frames(&mut m, &mut s, &[], 2, 0);
        assert!(s.mod_patch.is_none());
        frames(&mut m, &mut s, &id("rainbow_enemies"), 30, 0);
        assert!(s.mod_patch.is_some());
        // Voller Vorrat
        s.bus.wram[0xF36E] = 0x10;
        frames(&mut m, &mut s, &id("full_supply"), 2, 0);
        println!("full_supply: Magie {:02X}", s.bus.wram[0xF36E]);
        assert_eq!(s.bus.wram[0xF36E], 0x80);
        // Zeitlupe mit L
        frames(&mut m, &mut s, &id("slowmo"), 2, 0x0020);
        println!("slowmo: Tempo {}", m.speed());
        assert!(m.speed() < 1.0);
        // Doppelter Schaden: 1 Herz Schaden -> 2 Herzen
        let on = id("hard_damage");
        frames(&mut m, &mut s, &on, 1, 0);
        let hp0 = s.bus.wram[0xF36D];
        s.bus.wram[0x0373] = 8;
        frames(&mut m, &mut s, &on, 3, 0);
        println!("hard_damage: {} -> {}", hp0, s.bus.wram[0xF36D]);
        assert_eq!(hp0 - s.bus.wram[0xF36D], 16);
        // Ein-Treffer-Modus
        let on = id("one_hit");
        frames(&mut m, &mut s, &on, 1, 0);
        let hp = s.bus.wram[0xF36D];
        s.bus.wram[0xF36D] = hp - 8;
        frames(&mut m, &mut s, &on, 200, 0);
        println!("one_hit: Modus {:02X}", s.bus.wram[0x10]);
        assert_eq!(s.bus.wram[0x10], 0x12);
        // Gegner-Lebensbalken: herumlaufen, bis ein Gegner im Bild ist
        let Some(mut g2) = game(z3) else { return };
        let mut best = 0usize;
        for f in 0..3000u32 {
            let pad = [0x0100, 0x0400, 0x0200, 0x0800][(f / 200 % 4) as usize];
            frames(&mut m, &mut g2, &id("enemy_hp"), 1, pad);
            best = best.max(m.shown.len());
        }
        println!("enemy_hp beim Herumlaufen: bis zu {best} Zeichnungen");
        assert!(best >= 2);
        // ohne RC-Modus laufen keine Mods
        let Some(mut e) = game(false) else { return };
        e.recomp = None;
        frames(&mut m, &mut e, &id("info_hud"), 5, 0);
        assert!(m.shown.is_empty());
    }
}
