//! Zelda Core Mod-Schnittstelle (C-ABI, Version 3). Mods laufen nur im Recompiled-Modus (RC).
//!
//! Diese Datei ist in der App und in jeder Mod dieselbe. Nicht ändern: Die App prüft `PASTLINK_MOD_ABI`.
//!
//! Eine Mod ist eine DLL (Rust: `crate-type = ["cdylib"]`), die `pastlink_mod() -> *const ModInfo`
//! exportiert. Was eine Mod kann:
//! - pro Spielbild `pre_frame` (vorher, z. B. Eingaben ändern) und `post_frame` (danach),
//! - **Hooks**: `on_hook` läuft, bevor das Spiel eine Routine an einer der Adressen in `hooks` betritt
//!   (nur im eigenen Recompiler, nicht im Zelda-3-Modus). Die Mod kann dort Register und Speicher ändern
//!   oder die Routine ganz überspringen (`ret`).
//! - jeden Speicher lesen/schreiben: WRAM, SRAM, VRAM, Paletten (CGRAM), Sprites (OAM), die ROM im Speicher,
//! - **Grafik ändern**: Kacheln (Sprites, Hintergründe) und Farben dauerhaft ersetzen (`vram_patch`,
//!   `cgram_patch`), auch aus PNG-Bildern (`png_tiles`),
//! - **Bilder** über dem Spielbild zeichnen (PNG oder RGBA), dazu Rechtecke und Text,
//! - **Töne** abspielen (PCM, z. B. aus einer WAV-Datei, siehe `wav_pcm`),
//! - das Spieltempo ändern.

#![allow(dead_code)]

use std::ffi::{c_char, c_void};

/// Versionsnummer der Schnittstelle
pub const PASTLINK_MOD_ABI: u32 = 3;

/// Speicherbereiche für `mem_read` / `mem_write`
pub mod space {
    /// SNES-Adressen: WRAM `0x7E0000..=0x7FFFFF`, SRAM `0x700000..=0x70FFFF`
    pub const BUS: u32 = 0;
    /// Grafikspeicher (Byte-Adresse 0..0x10000)
    pub const VRAM: u32 = 1;
    /// Paletten (Byte-Adresse 0..0x200, je Farbe 2 Byte BGR555)
    pub const CGRAM: u32 = 2;
    /// Sprite-Tabelle (0..0x220)
    pub const OAM: u32 = 3;
    /// ROM-Abbild im Speicher (Dateioffset ohne Header)
    pub const ROM: u32 = 4;
}

/// CPU-Register für `reg` / `set_reg`
pub mod reg {
    pub const A: u32 = 0;
    pub const X: u32 = 1;
    pub const Y: u32 = 2;
    pub const SP: u32 = 3;
    pub const DP: u32 = 4;
    pub const DB: u32 = 5;
    pub const PB: u32 = 6;
    pub const PC: u32 = 7;
    /// Statusregister (N V M X D I Z C)
    pub const P: u32 = 8;
}

/// Was die App einer Mod anbietet
#[repr(C)]
pub struct Api {
    /// = `PASTLINK_MOD_ABI`
    pub abi: u32,
    /// intern (an alle Funktionen durchreichen)
    pub ctx: *mut c_void,
    /// Spielbild-Zähler
    pub frame: u64,
    /// Byte lesen (SNES-Adresse: WRAM/SRAM)
    pub read8: extern "C" fn(ctx: *mut c_void, addr: u32) -> u8,
    /// Byte schreiben (SNES-Adresse: WRAM/SRAM)
    pub write8: extern "C" fn(ctx: *mut c_void, addr: u32, value: u8),
    /// Controller 1 (B=0x8000 Y=0x4000 Select=0x2000 Start=0x1000 ↑=0x0800 ↓=0x0400 ←=0x0200 →=0x0100
    /// A=0x0080 X=0x0040 L=0x0020 R=0x0010)
    pub pad: extern "C" fn(ctx: *mut c_void) -> u16,
    /// Controller 1 für dieses Bild ersetzen (in `pre_frame`)
    pub set_pad: extern "C" fn(ctx: *mut c_void, pad: u16),
    /// Speicher lesen (`space::…`)
    pub mem_read: extern "C" fn(ctx: *mut c_void, space: u32, addr: u32) -> u8,
    /// Speicher schreiben (`space::…`)
    pub mem_write: extern "C" fn(ctx: *mut c_void, space: u32, addr: u32, value: u8),
    /// CPU-Register lesen (`reg::…`)
    pub reg: extern "C" fn(ctx: *mut c_void, r: u32) -> u32,
    /// CPU-Register setzen (`reg::…`) – vor allem in `on_hook`
    pub set_reg: extern "C" fn(ctx: *mut c_void, r: u32, value: u32),
    /// in `on_hook`: Routine sofort verlassen (0 = RTS, 1 = RTL)
    pub ret: extern "C" fn(ctx: *mut c_void, long: u32),
    /// Rechteck über dem Spielbild (SNES-Pixel 256×224, Farbe 0xRRGGBBAA), gilt für dieses Bild
    pub draw_rect: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, rgba: u32),
    /// Text über dem Spielbild (SNES-Pixel, UTF-8 mit 0 abgeschlossen)
    pub draw_text: extern "C" fn(ctx: *mut c_void, x: i32, y: i32, text: *const c_char, rgba: u32),
    /// Spieltempo für dieses Bild (1.0 = normal, 0.5 = Zeitlupe, 2.0 = doppelt)
    pub set_speed: extern "C" fn(ctx: *mut c_void, speed: f32),

    // ---- ab Version 3
    /// Ton abspielen: `frames` Abtastwerte je Kanal (16 Bit, `channels` = 1 oder 2, verschachtelt), beliebige
    /// Abtastrate; `volume` 1.0 = normal. Mehrere Töne laufen gemischt nebeneinander.
    pub play_sound: extern "C" fn(ctx: *mut c_void, samples: *const i16, frames: u32, channels: u32, rate: u32, volume: f32),
    /// Bild aus PNG-Daten anmelden; Rückgabe: Bild-Nummer (0 = Fehler). Einmal anmelden, dann jedes Bild zeichnen.
    pub image_png: extern "C" fn(ctx: *mut c_void, png: *const u8, len: u32) -> u32,
    /// Bild aus RGBA-Pixeln (w×h×4 Byte) anmelden; Rückgabe: Bild-Nummer (0 = Fehler)
    pub image_rgba: extern "C" fn(ctx: *mut c_void, rgba: *const u8, w: u32, h: u32) -> u32,
    /// angemeldetes Bild zeichnen (SNES-Pixel; `w`/`h` = 0: Originalgröße), `tint` 0xFFFFFFFF = unverändert
    pub draw_image: extern "C" fn(ctx: *mut c_void, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32),
    /// Grafikspeicher dauerhaft überschreiben (Wort-Adresse 0..0x8000), bis `patch_clear` oder Mod aus.
    /// Wird jedes Bild vor dem Zeichnen angewendet – so lassen sich Sprites und Kacheln austauschen.
    pub vram_patch: extern "C" fn(ctx: *mut c_void, word_addr: u32, data: *const u16, words: u32),
    /// Farben dauerhaft überschreiben (Index 0..256, BGR555), bis `patch_clear` oder Mod aus
    pub cgram_patch: extern "C" fn(ctx: *mut c_void, index: u32, colors: *const u16, count: u32),
    /// alle eigenen Grafik-/Farb-Änderungen dieser Mod entfernen
    pub patch_clear: extern "C" fn(ctx: *mut c_void),
    /// PNG in SNES-Kacheln umwandeln (`bpp` = 2 oder 4; Bildgröße Vielfache von 8; durchsichtig = Farbe 0).
    /// Schreibt die Kacheln (Zeile für Zeile, je Kachel 8·bpp Wörter) nach `out` und die Palette (bis zu
    /// 2^bpp Farben BGR555) nach `pal`. Rückgabe: geschriebene Wörter (0 = Fehler).
    pub png_tiles: extern "C" fn(ctx: *mut c_void, png: *const u8, len: u32, bpp: u32, out: *mut u16, max_words: u32, pal: *mut u16) -> u32,
    /// Meldung in der App anzeigen (UTF-8, mit 0 abgeschlossen)
    pub log: extern "C" fn(ctx: *mut c_void, text: *const c_char),
}

/// Beschreibung einer Mod (statisch, lebt so lange wie die DLL)
#[repr(C)]
pub struct ModInfo {
    /// = `PASTLINK_MOD_ABI`
    pub abi: u32,
    /// Name (UTF-8, mit 0 abgeschlossen)
    pub name: *const c_char,
    /// kurze Beschreibung (UTF-8, mit 0 abgeschlossen)
    pub description: *const c_char,
    /// vor jedem Spielbild
    pub pre_frame: Option<extern "C" fn(api: *const Api)>,
    /// nach jedem Spielbild
    pub post_frame: Option<extern "C" fn(api: *const Api)>,
    /// Adressen (24 Bit, z. B. 0x0DDE5A) für `on_hook`
    pub hooks: *const u32,
    pub hook_count: u32,
    /// das Spiel betritt gleich die Routine an `addr`
    pub on_hook: Option<extern "C" fn(api: *const Api, addr: u32)>,
}

// Zeiger auf statische Daten dürfen zwischen Threads geteilt werden
unsafe impl Sync for ModInfo {}

/// Bequemer Zugriff auf die `Api` für Rust-Mods
pub struct Game<'a>(pub &'a Api);

impl Game<'_> {
    pub fn frame(&self) -> u64 {
        self.0.frame
    }
    pub fn read8(&self, addr: u32) -> u8 {
        (self.0.read8)(self.0.ctx, addr)
    }
    pub fn read16(&self, addr: u32) -> u16 {
        self.read8(addr) as u16 | (self.read8(addr + 1) as u16) << 8
    }
    pub fn write8(&self, addr: u32, v: u8) {
        (self.0.write8)(self.0.ctx, addr, v)
    }
    pub fn write16(&self, addr: u32, v: u16) {
        self.write8(addr, v as u8);
        self.write8(addr + 1, (v >> 8) as u8);
    }
    pub fn mem_read(&self, space: u32, addr: u32) -> u8 {
        (self.0.mem_read)(self.0.ctx, space, addr)
    }
    pub fn mem_write(&self, space: u32, addr: u32, v: u8) {
        (self.0.mem_write)(self.0.ctx, space, addr, v)
    }
    pub fn reg(&self, r: u32) -> u32 {
        (self.0.reg)(self.0.ctx, r)
    }
    pub fn set_reg(&self, r: u32, v: u32) {
        (self.0.set_reg)(self.0.ctx, r, v)
    }
    /// in `on_hook`: Routine überspringen
    pub fn ret(&self, long: bool) {
        (self.0.ret)(self.0.ctx, long as u32)
    }
    pub fn pad(&self) -> u16 {
        (self.0.pad)(self.0.ctx)
    }
    pub fn set_pad(&self, pad: u16) {
        (self.0.set_pad)(self.0.ctx, pad)
    }
    pub fn rect(&self, x: i32, y: i32, w: i32, h: i32, rgba: u32) {
        (self.0.draw_rect)(self.0.ctx, x, y, w, h, rgba)
    }
    pub fn text(&self, x: i32, y: i32, text: &str, rgba: u32) {
        let b = cstr(text);
        (self.0.draw_text)(self.0.ctx, x, y, b.as_ptr() as *const c_char, rgba)
    }
    pub fn set_speed(&self, s: f32) {
        (self.0.set_speed)(self.0.ctx, s)
    }
    /// Link steht im normalen Spiel (Oberwelt oder Dungeon, kein Menü, kein Übergang)
    pub fn in_game(&self) -> bool {
        matches!(self.read8(0x7E0010), 0x07 | 0x09 | 0x0B) && self.read8(0x7E0011) == 0
    }
    /// Links Position auf dem Bildschirm (SNES-Pixel, linke obere Ecke seines 16×16-Körpers)
    pub fn link_on_screen(&self) -> (i32, i32) {
        let x = self.read16(0x7E0022).wrapping_sub(self.read16(0x7E00E2)) as i16 as i32;
        let y = self.read16(0x7E0020).wrapping_sub(self.read16(0x7E00E8)) as i16 as i32;
        (x, y)
    }

    // ---- Version 3

    /// Ton abspielen (16-Bit-PCM, verschachtelt bei Stereo)
    pub fn play_sound(&self, samples: &[i16], channels: u32, rate: u32, volume: f32) {
        let ch = channels.clamp(1, 2);
        (self.0.play_sound)(self.0.ctx, samples.as_ptr(), samples.len() as u32 / ch, ch, rate, volume)
    }
    /// WAV-Datei (16-Bit-PCM) abspielen
    pub fn play_wav(&self, wav: &[u8], volume: f32) -> bool {
        match wav_pcm(wav) {
            Some((s, ch, rate)) => {
                self.play_sound(&s, ch, rate, volume);
                true
            }
            None => false,
        }
    }
    /// PNG anmelden (einmal), Rückgabe Bild-Nummer (0 = Fehler)
    pub fn image_png(&self, png: &[u8]) -> u32 {
        (self.0.image_png)(self.0.ctx, png.as_ptr(), png.len() as u32)
    }
    /// RGBA-Bild anmelden (einmal), Rückgabe Bild-Nummer (0 = Fehler)
    pub fn image_rgba(&self, rgba: &[u8], w: u32, h: u32) -> u32 {
        if rgba.len() < (w * h * 4) as usize {
            return 0;
        }
        (self.0.image_rgba)(self.0.ctx, rgba.as_ptr(), w, h)
    }
    /// Bild zeichnen (`w`/`h` = 0: Originalgröße)
    pub fn image(&self, id: u32, x: i32, y: i32, w: i32, h: i32, tint: u32) {
        (self.0.draw_image)(self.0.ctx, id, x, y, w, h, tint)
    }
    /// Grafikspeicher dauerhaft ersetzen (Wort-Adresse)
    pub fn vram_patch(&self, word_addr: u32, data: &[u16]) {
        (self.0.vram_patch)(self.0.ctx, word_addr, data.as_ptr(), data.len() as u32)
    }
    /// Farben dauerhaft ersetzen (Index 0..256, BGR555)
    pub fn cgram_patch(&self, index: u32, colors: &[u16]) {
        (self.0.cgram_patch)(self.0.ctx, index, colors.as_ptr(), colors.len() as u32)
    }
    /// eigene Grafik-/Farb-Änderungen entfernen
    pub fn patch_clear(&self) {
        (self.0.patch_clear)(self.0.ctx)
    }
    /// PNG -> SNES-Kacheln (bpp 2 oder 4) und Palette
    pub fn png_tiles(&self, png: &[u8], bpp: u32) -> Option<(Vec<u16>, Vec<u16>)> {
        let mut out = vec![0u16; 0x8000];
        let mut pal = vec![0u16; 16];
        let n = (self.0.png_tiles)(self.0.ctx, png.as_ptr(), png.len() as u32, bpp, out.as_mut_ptr(), out.len() as u32, pal.as_mut_ptr());
        if n == 0 {
            return None;
        }
        out.truncate(n as usize);
        pal.truncate(1 << bpp.min(4));
        Some((out, pal))
    }
    /// Meldung in der App anzeigen
    pub fn log(&self, text: &str) {
        let b = cstr(text);
        (self.0.log)(self.0.ctx, b.as_ptr() as *const c_char)
    }
}

/// Farbe 0xRRGGBB -> SNES BGR555
pub const fn bgr555(rgb: u32) -> u16 {
    let r = (rgb >> 19) & 31;
    let g = (rgb >> 11) & 31;
    let b = (rgb >> 3) & 31;
    (r | g << 5 | b << 10) as u16
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.retain(|&c| c != 0);
    b.push(0);
    b
}

/// WAV (RIFF, 16-Bit-PCM) lesen: (Abtastwerte, Kanäle, Abtastrate)
pub fn wav_pcm(wav: &[u8]) -> Option<(Vec<i16>, u32, u32)> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let (mut p, mut ch, mut rate, mut bits) = (12usize, 0u32, 0u32, 0u32);
    while p + 8 <= wav.len() {
        let id = &wav[p..p + 4];
        let len = u32::from_le_bytes(wav[p + 4..p + 8].try_into().ok()?) as usize;
        let body = wav.get(p + 8..(p + 8 + len).min(wav.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            ch = u16::from_le_bytes([body[2], body[3]]) as u32;
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]) as u32;
        } else if id == b"data" && bits == 16 && ch >= 1 {
            let s = body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            return Some((s, ch, rate));
        }
        p += 8 + len + (len & 1);
    }
    None
}
