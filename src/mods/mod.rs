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
}

/// Zugriff einer Mod auf das laufende Spiel
pub struct Ctx<'a> {
    pub cpu: &'a mut Cpu,
    pub bus: &'a mut Bus,
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
    pub fn mem_read(&self, sp: u32, addr: u32) -> u8 {
        let b = &*self.bus;
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
        let b = &mut *self.bus;
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
        let c = &*self.cpu;
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
        let c = &mut *self.cpu;
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
        self.cpu.exec_op(self.bus, if long { 0x6B } else { 0x60 });
    }
    pub fn pad(&self) -> u16 {
        self.bus.joy[0]
    }
    pub fn set_pad(&mut self, p: u16) {
        self.bus.joy[0] = p;
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
        matches!(self.read8(0x7E0010), 0x07 | 0x09 | 0x0B) && self.read8(0x7E0011) == 0
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

    /// Neue DLLs aus dem Mod-Ordner laden (schon geladene bleiben)
    pub fn load_dlls(&mut self) {
        // Updates aus dem Shop (`x.dll.new`): ersetzen, solange die alte DLL noch nicht geladen ist
        if let Ok(rd) = std::fs::read_dir(Self::dir()) {
            for p in rd.filter_map(|e| e.ok().map(|e| e.path())) {
                if p.extension().is_some_and(|e| e == "new") {
                    let dll = p.with_extension("");
                    if !self.loaded.contains(&dll) && std::fs::rename(&p, &dll).is_err() {
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
                Ok(e) => self.inner.lock().unwrap().list.push(e),
                Err(e) => self.errors.push(format!("{}: {e}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())),
            }
        }
        // neuer Abgleich beim nächsten Bild
        self.on_ids = vec!["\u{0}".into()];
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
            let idx: Vec<usize> = (0..i.list.len()).filter(|&k| on.contains(&i.list[k].id)).collect();
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
                        let mut ctx = Ctx { cpu: &mut *c, bus: &mut *b, frame: g.frame, out: &mut g.out, store: &mut g.store, cur: k };
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
            let mut ctx = Ctx { cpu: &mut snes.cpu, bus: &mut snes.bus, frame: g.frame, out: &mut g.out, store: &mut g.store, cur: k };
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
        if self.shown.is_empty() || view_w == 0 {
            return;
        }
        let kx = r.width() / view_w as f32;
        let ky = r.height() / 224.0;
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
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    struct DllMod {
        info: &'static ModInfo,
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
            // bleibt bis zum Programmende geladen (die Mod-Daten sind statisch)
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
            if info.abi != PASTLINK_MOD_ABI {
                return Err(format!("Schnittstelle {} statt {} – bitte mit der aktuellen Vorlage neu bauen", info.abi, PASTLINK_MOD_ABI));
            }
            let text = |c: *const std::ffi::c_char| if c.is_null() { String::new() } else { CStr::from_ptr(c).to_string_lossy().into_owned() };
            let name = text(info.name);
            let desc = text(info.description);
            let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            Ok(Entry { id: format!("dll:{file}"), name_de: name.clone(), name_en: name, desc_de: desc.clone(), desc_en: desc, m: Box::new(DllMod { info }) })
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
