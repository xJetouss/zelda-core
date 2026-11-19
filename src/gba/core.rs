//! Anbindung an den GBA-Kern mGBA (vendor/mgba, MPL-2.0) über dessen libretro-Schnittstelle.
//!
//! libretro arbeitet mit globalen Rückruf-Funktionen; es gibt deshalb genau einen Kern zur Zeit. Bild (RGB565),
//! Ton (32 768 Hz Stereo) und Eingaben laufen über `ST`.

#![allow(dead_code)]

use std::ffi::{c_char, c_void, CString};
use std::sync::Mutex;

#[repr(C)]
pub struct GameInfo {
    pub path: *const c_char,
    pub data: *const c_void,
    pub size: usize,
    pub meta: *const c_char,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Geometry {
    pub base_width: u32,
    pub base_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub aspect_ratio: f32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Timing {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct AvInfo {
    pub geometry: Geometry,
    pub timing: Timing,
}

#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}

pub const MEMORY_SAVE_RAM: u32 = 0;

extern "C" {
    fn retro_set_environment(cb: extern "C" fn(u32, *mut c_void) -> bool);
    fn retro_set_video_refresh(cb: extern "C" fn(*const c_void, u32, u32, usize));
    fn retro_set_audio_sample(cb: extern "C" fn(i16, i16));
    fn retro_set_audio_sample_batch(cb: extern "C" fn(*const i16, usize) -> usize);
    fn retro_set_input_poll(cb: extern "C" fn());
    fn retro_set_input_state(cb: extern "C" fn(u32, u32, u32, u32) -> i16);
    pub fn retro_init();
    pub fn retro_deinit();
    pub fn retro_load_game(info: *const GameInfo) -> bool;
    pub fn retro_unload_game();
    pub fn retro_run();
    pub fn retro_reset();
    pub fn retro_get_system_av_info(info: *mut AvInfo);
    pub fn retro_serialize_size() -> usize;
    pub fn retro_serialize(data: *mut c_void, size: usize) -> bool;
    pub fn retro_unserialize(data: *const c_void, size: usize) -> bool;
    pub fn retro_cheat_reset();
    pub fn retro_cheat_set(index: u32, enabled: bool, code: *const c_char);
    pub fn retro_get_memory_data(id: u32) -> *mut c_void;
    pub fn retro_get_memory_size(id: u32) -> usize;
    /// Zusatz in vendor/mgba (siehe PASTLINK.md): Bildebene ein/aus (0–3 Hintergründe, 4 Sprites)
    pub fn pl_mgba_enable_layer(id: usize, enable: bool);
}

/// Was die Rückrufe des Kerns lesen/schreiben
pub struct State {
    /// letztes Bild als RGBA
    pub frame: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub new_frame: bool,
    /// Ton (Stereo, verschachtelt) in der Rate des Kerns
    pub audio: Vec<i16>,
    /// Tasten im SNES-Format (B Y Sel St ↑ ↓ ← → A X L R)
    pub pad: u16,
    sys_dir: CString,
    /// Kern-Optionen (Schlüssel -> Wert), z. B. „mgba_frameskip“
    vars: Vec<(CString, CString)>,
    vars_changed: bool,
}

pub static ST: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    ST.lock().ok()?.as_mut().map(f)
}

const ENV_SET_PIXEL_FORMAT: u32 = 10;
const ENV_GET_SYSTEM_DIRECTORY: u32 = 9;
const ENV_GET_SAVE_DIRECTORY: u32 = 31;
const ENV_GET_VARIABLE: u32 = 15;
const ENV_GET_VARIABLE_UPDATE: u32 = 17;

extern "C" fn environment(cmd: u32, data: *mut c_void) -> bool {
    // höhere Bits sind Kennzeichen (z. B. „experimentell“)
    match cmd & 0xFFFF {
        ENV_SET_PIXEL_FORMAT => !data.is_null() && unsafe { *(data as *const u32) } == 2, // RGB565
        ENV_GET_SYSTEM_DIRECTORY | ENV_GET_SAVE_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            with(|s| unsafe { *(data as *mut *const c_char) = s.sys_dir.as_ptr() }).is_some()
        }
        ENV_GET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let v = unsafe { &mut *(data as *mut Variable) };
            if v.key.is_null() {
                return false;
            }
            let key = unsafe { std::ffi::CStr::from_ptr(v.key) };
            with(|s| match s.vars.iter().find(|(k, _)| k.as_c_str() == key) {
                Some((_, val)) => {
                    v.value = val.as_ptr();
                    true
                }
                None => false,
            })
            .unwrap_or(false)
        }
        ENV_GET_VARIABLE_UPDATE => {
            if data.is_null() {
                return false;
            }
            let ch = with(|s| std::mem::take(&mut s.vars_changed)).unwrap_or(false);
            unsafe { *(data as *mut bool) = ch };
            true
        }
        _ => false,
    }
}

extern "C" fn video(data: *const c_void, w: u32, h: u32, pitch: usize) {
    if data.is_null() {
        return; // Bild unverändert
    }
    with(|s| {
        let (w, h) = (w as usize, h as usize);
        s.w = w;
        s.h = h;
        s.frame.resize(w * h * 4, 0);
        for y in 0..h {
            let row = unsafe { std::slice::from_raw_parts((data as *const u8).add(y * pitch) as *const u16, w) };
            for (x, &c) in row.iter().enumerate() {
                let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
                let o = (y * w + x) * 4;
                s.frame[o] = ((r << 3) | (r >> 2)) as u8;
                s.frame[o + 1] = ((g << 2) | (g >> 4)) as u8;
                s.frame[o + 2] = ((b << 3) | (b >> 2)) as u8;
                s.frame[o + 3] = 255;
            }
        }
        s.new_frame = true;
    });
}

extern "C" fn audio_sample(l: i16, r: i16) {
    with(|s| s.audio.extend_from_slice(&[l, r]));
}

extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    if !data.is_null() {
        let d = unsafe { std::slice::from_raw_parts(data, frames * 2) };
        with(|s| s.audio.extend_from_slice(d));
    }
    frames
}

extern "C" fn input_poll() {}

/// Zähler für Tests: Abfragen je Tasten-Nummer und davon „gedrückt“
pub static INPUT_DBG: [std::sync::atomic::AtomicU32; 32] = [const { std::sync::atomic::AtomicU32::new(0) }; 32];

extern "C" fn input_state(port: u32, device: u32, _index: u32, id: u32) -> i16 {
    if cfg!(test) {
        INPUT_DBG[(id as usize).min(15)].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if port != 0 || device != 1 {
        return 0;
    }
    // libretro-Joypad: B Y Select Start ↑ ↓ ← → A X L R – dieselbe Reihenfolge wie die SNES-Bits von oben
    let bit = match id {
        0..=11 => 0x8000u16 >> id,
        _ => return 0,
    };
    let r = with(|s| (s.pad & bit != 0) as i16).unwrap_or(0);
    if cfg!(test) && r != 0 {
        INPUT_DBG[16 + (id as usize).min(15)].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    r
}

/// Kern vorbereiten (Rückrufe setzen). `sys_dir`: Ordner für BIOS/Spielstände des Kerns.
pub fn start(sys_dir: &std::path::Path, vars: &[(&str, &str)]) {
    let _ = std::fs::create_dir_all(sys_dir);
    *ST.lock().unwrap() = Some(State {
        frame: vec![0; 240 * 160 * 4],
        w: 240,
        h: 160,
        new_frame: false,
        audio: Vec::new(),
        pad: 0,
        sys_dir: CString::new(sys_dir.to_string_lossy().as_bytes()).unwrap_or_default(),
        vars: vars.iter().map(|(k, v)| (CString::new(*k).unwrap(), CString::new(*v).unwrap())).collect(),
        vars_changed: false,
    });
    unsafe {
        retro_set_environment(environment);
        retro_set_video_refresh(video);
        retro_set_audio_sample(audio_sample);
        retro_set_audio_sample_batch(audio_batch);
        retro_set_input_poll(input_poll);
        retro_set_input_state(input_state);
        retro_init();
    }
}

/// Kern-Option setzen (gilt ab dem nächsten Bild)
pub fn set_var(key: &str, value: &str) {
    with(|s| {
        let k = CString::new(key).unwrap();
        let v = CString::new(value).unwrap();
        match s.vars.iter_mut().find(|(a, _)| *a == k) {
            Some(e) => e.1 = v,
            None => s.vars.push((k, v)),
        }
        s.vars_changed = true;
    });
}

pub fn stop() {
    unsafe { retro_deinit() };
    *ST.lock().unwrap() = None;
}
