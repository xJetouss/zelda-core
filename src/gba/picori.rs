//! The Minish Cap als PC-Fassung („Recomp“): Project Picori (Dekompilierung, GPL-3.0) als `tmc_core.dll` neben der
//! .exe, gebaut von `tools/build_tmc.py`. Echtes Breitbild bis 384 Pixel, Savestates über alle Spielvariablen.
//!
//! Picori lässt sich je Programmlauf nur einmal starten (es läuft in einem eigenen Thread weiter). Ein neuer Start
//! lädt deshalb den beim ersten Start gesicherten Anfangszustand (wie ein Reset).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

type FStart = unsafe extern "C" fn(*const i8, *const i8) -> i32;
type FVoidI = unsafe extern "C" fn() -> i32;
type FRun = unsafe extern "C" fn(u16) -> i32;
type FFrame = unsafe extern "C" fn(*mut i32, *mut i32) -> *const u32;
type FAudio = unsafe extern "C" fn(*mut i16, i32) -> i32;
type FView = unsafe extern "C" fn(i32, i32, i32);
type FSave = unsafe extern "C" fn(*const i8, i32);
type FInt = unsafe extern "C" fn(i32);
type FIntR = unsafe extern "C" fn(i32) -> i32;
type FXY = unsafe extern "C" fn(*mut i32, *mut i32) -> i32;
type FSize = unsafe extern "C" fn() -> usize;
type FStSave = unsafe extern "C" fn(*mut u8, usize) -> i32;
type FStLoad = unsafe extern "C" fn(*const u8, usize) -> i32;
type FRead = unsafe extern "C" fn(u32, *mut u8, i32) -> i32;
type FWrite = unsafe extern "C" fn(u32, *const u8, i32) -> i32;

struct Api {
    start: FStart,
    ready: FVoidI,
    run: FRun,
    frame: FFrame,
    audio: FAudio,
    view: FView,
    set_save: FSave,
    mute: FInt,
    layers: FInt,
    state_size: FSize,
    state_save: FStSave,
    state_load: FStLoad,
    read: FRead,
    write: FWrite,
    task: FVoidI,
    hud: FVoidI,
    menu_item: FVoidI,
    menu_pos: FIntR,
    button_b: FXY,
    debug: FIntR,
}

/// geladene DLL, ROM des laufenden Picori, Anfangszustand
struct Global {
    api: Option<&'static Api>,
    rom: Option<PathBuf>,
    boot: Option<Vec<u8>>,
    busy: bool,
}

static G: Mutex<Global> = Mutex::new(Global { api: None, rom: None, boot: None, busy: false });

fn dll_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("tmc_core.dll")))
}

/// Liegt `tmc_core.dll` neben der .exe?
pub fn available() -> bool {
    dll_path().is_some_and(|p| p.is_file())
}

/// Kann Picori diese ROM spielen? (The Minish Cap USA/Europa/Japan)
pub fn supports(code: &str) -> bool {
    matches!(code, "BZME" | "BZMP" | "BZMJ")
}

/// Arbeitsordner für Picori (entpackte Grafiken und Töne aus der ROM, config.json)
pub fn data_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("minish"))).unwrap_or_else(|| PathBuf::from("minish"))
}

fn load_api() -> Result<&'static Api, String> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let p = dll_path().ok_or("kein Programmordner")?;
    let wide: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
    unsafe {
        let h = LoadLibraryW(wide.as_ptr());
        if h.is_null() {
            return Err("tmc_core.dll fehlt oder lässt sich nicht laden".into());
        }
        macro_rules! f {
            ($n:literal) => {
                std::mem::transmute(GetProcAddress(h, concat!($n, "\0").as_ptr()).ok_or(concat!("tmc_core.dll: ", $n, " fehlt"))?)
            };
        }
        let api = Api {
            start: f!("tmc_start"),
            ready: f!("tmc_ready"),
            run: f!("tmc_run_frame"),
            frame: f!("tmc_frame"),
            audio: f!("tmc_audio"),
            view: f!("tmc_set_view"),
            set_save: f!("tmc_set_save"),
            mute: f!("tmc_set_mute"),
            layers: f!("tmc_set_layers"),
            state_size: f!("tmc_state_size"),
            state_save: f!("tmc_state_save"),
            state_load: f!("tmc_state_load"),
            read: f!("tmc_read"),
            write: f!("tmc_write"),
            task: f!("tmc_task"),
            hud: f!("tmc_hud"),
            menu_item: f!("tmc_menu_item"),
            menu_pos: f!("tmc_menu_pos"),
            button_b: f!("tmc_button_b"),
            debug: f!("tmc_debug"),
        };
        Ok(Box::leak(Box::new(api)))
    }
}

/// laufendes Picori (höchstens eins)
pub struct Core {
    api: &'static Api,
    ready: bool,
    /// Savestate, der geladen wird, sobald der Start fertig ist
    pending: Option<Vec<u8>>,
    view: (i32, i32, bool),
}

/// SNES-Tastenbits der App → GBA-Tasten (gesetztes Bit = gedrückt)
fn gba_keys(pad: u16) -> u16 {
    const MAP: [(u16, u16); 10] = [
        (0x0080, 0x001), // A
        (0x8000, 0x002), // B
        (0x2000, 0x004), // Select
        (0x1000, 0x008), // Start
        (0x0100, 0x010), // →
        (0x0200, 0x020), // ←
        (0x0800, 0x040), // ↑
        (0x0400, 0x080), // ↓
        (0x0010, 0x100), // R
        (0x0020, 0x200), // L
    ];
    MAP.iter().filter(|(s, _)| pad & s != 0).fold(0, |k, (_, g)| k | g)
}

impl Core {
    /// Picori starten (oder das laufende auf den Anfangszustand zurücksetzen). `sav`: Spielstand des Spiels
    /// (mGBA-Format), `write`: darf das Spiel ihn schreiben.
    pub fn open(rom: &Path, sav: &Path, write: bool) -> Result<Core, String> {
        let mut g = G.lock().map_err(|_| "Picori gesperrt")?;
        if g.busy {
            return Err("Es läuft schon ein Spiel mit Picori".into());
        }
        let api = match g.api {
            Some(a) => a,
            None => {
                let a = load_api()?;
                g.api = Some(a);
                a
            }
        };
        let csav = std::ffi::CString::new(sav.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        match &g.rom {
            Some(r) if r != rom => {
                return Err("Picori läuft schon mit einer anderen ROM – bitte Zelda Core neu starten".into());
            }
            Some(_) => {
                unsafe { (api.set_save)(csav.as_ptr(), write as i32) };
                if let Some(b) = &g.boot {
                    unsafe { (api.state_load)(b.as_ptr(), b.len()) };
                }
            }
            None => {
                let crom = std::ffi::CString::new(rom.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
                let dir = data_dir();
                let cdir = std::ffi::CString::new(dir.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
                // Picori arbeitet relativ zum aktuellen Ordner; die App selbst nutzt nur absolute Pfade
                let here = std::env::current_dir().ok();
                unsafe { (api.set_save)(csav.as_ptr(), write as i32) };
                if unsafe { (api.start)(crom.as_ptr(), cdir.as_ptr()) } == 0 {
                    if let Some(h) = here {
                        let _ = std::env::set_current_dir(h);
                    }
                    return Err("Picori ließ sich nicht starten".into());
                }
                g.rom = Some(rom.to_path_buf());
            }
        }
        g.busy = true;
        let ready = g.boot.is_some();
        Ok(Core { api, ready, pending: None, view: (0, 0, false) })
    }

    /// Start abgeschlossen? (Beim allerersten Start entpackt Picori Grafiken und Töne aus der ROM, etwa 20 s.)
    pub fn poll_ready(&mut self) -> Result<bool, String> {
        if self.ready {
            return Ok(true);
        }
        match unsafe { (self.api.ready)() } {
            0 => Ok(false),
            1 => {
                self.ready = true;
                // Anfangszustand für spätere Starts in diesem Programmlauf
                if let Some(b) = self.save_state() {
                    if let Ok(mut g) = G.lock() {
                        g.boot = Some(b);
                    }
                }
                if let Some(b) = self.pending.take() {
                    self.load_state(&b);
                }
                Ok(true)
            }
            _ => Err("Picori wurde beendet (ROM oder Grafikdaten fehlerhaft?)".into()),
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Anzeigefläche (Seitenverhältnis bestimmt die Breite der Spielansicht), Breitbild an/aus
    pub fn set_view(&mut self, w: i32, h: i32, wide: bool) {
        if self.view != (w, h, wide) {
            self.view = (w, h, wide);
            unsafe { (self.api.view)(w, h, wide as i32) };
        }
    }

    pub fn set_save(&self, sav: &Path, write: bool) {
        if let Ok(c) = std::ffi::CString::new(sav.to_string_lossy().as_bytes()) {
            unsafe { (self.api.set_save)(c.as_ptr(), write as i32) };
        }
    }

    pub fn set_mute(&self, m: bool) {
        unsafe { (self.api.mute)(m as i32) };
    }

    /// ein Spielbild; liefert das Bild (RGBA, Breite, Höhe)
    pub fn run_frame(&mut self, pad: u16, frame: &mut Vec<u8>) -> Option<(usize, usize)> {
        if !self.ready || unsafe { (self.api.run)(gba_keys(pad)) } == 0 {
            return None;
        }
        Some(self.grab(frame))
    }

    fn grab(&self, frame: &mut Vec<u8>) -> (usize, usize) {
        let (mut w, mut h) = (0i32, 0i32);
        let p = unsafe { (self.api.frame)(&mut w, &mut h) };
        let (w, h) = (w.max(0) as usize, h.max(0) as usize);
        let px = unsafe { std::slice::from_raw_parts(p as *const u8, w * h * 4) };
        frame.clear();
        frame.extend_from_slice(px);
        for a in frame.iter_mut().skip(3).step_by(4) {
            *a = 255;
        }
        (w, h)
    }

    /// Ton seit dem letzten Abholen (48 kHz Stereo)
    pub fn take_audio(&self, out: &mut Vec<i16>) {
        let mut buf = vec![0i16; 48000 / 10 * 2];
        loop {
            let n = unsafe { (self.api.audio)(buf.as_mut_ptr(), (buf.len() / 2) as i32) } as usize;
            out.extend_from_slice(&buf[..n * 2]);
            if n < buf.len() / 2 {
                break;
            }
        }
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        let n = unsafe { (self.api.state_size)() };
        let mut b = vec![0u8; n];
        (n > 0 && unsafe { (self.api.state_save)(b.as_mut_ptr(), n) } != 0).then_some(b)
    }

    pub fn load_state(&mut self, b: &[u8]) -> bool {
        if !self.ready {
            self.pending = Some(b.to_vec());
            return true;
        }
        (unsafe { (self.api.state_load)(b.as_ptr(), b.len()) }) != 0
    }

    pub fn read(&self, addr: u32, n: usize) -> Option<Vec<u8>> {
        let mut b = vec![0u8; n];
        (unsafe { (self.api.read)(addr, b.as_mut_ptr(), n as i32) } != 0).then_some(b)
    }

    pub fn write(&mut self, addr: u32, b: &[u8]) -> bool {
        unsafe { (self.api.write)(addr, b.as_ptr(), b.len() as i32) != 0 }
    }

    /// Spielanzeige mit B-Feld sichtbar (im Spiel, kein Menü, B-Feld nicht ausgeblendet)
    pub fn hud(&self) -> bool {
        self.ready && unsafe { (self.api.hud)() } != 0
    }

    /// Item-Menü offen: Item unter dem Cursor (0 = leerer Platz); None = Menü nicht offen
    pub fn menu_item(&self) -> Option<u8> {
        if !self.ready {
            return None;
        }
        let v = unsafe { (self.api.menu_item)() };
        (v >= 0).then_some(v as u8)
    }

    /// Item-Menü: Mitte des Kästchens mit diesem Item (Bildpunkte im 240×160-Menü)
    pub fn menu_pos(&self, item: u8) -> Option<(usize, usize)> {
        if !self.ready {
            return None;
        }
        let v = unsafe { (self.api.menu_pos)(item as i32) };
        (v >= 0).then_some(((v & 0xFF) as usize, ((v >> 8) & 0xFF) as usize))
    }

    /// B-Tastenfeld ganz zu sehen: Mitte der Figur (Bildpunkte); None = gerade nicht (Zwischensequenz, Text …)
    pub fn button_b(&self) -> Option<(i32, i32)> {
        if !self.ready {
            return None;
        }
        let (mut x, mut y) = (0, 0);
        (unsafe { (self.api.button_b)(&mut x, &mut y) } != 0).then_some((x, y))
    }

    /// Entwicklung: Werte aus dem Spiel (siehe tmc_debug)
    pub fn debug(&self, what: i32) -> i32 {
        unsafe { (self.api.debug)(what) }
    }

    /// Spielaufgabe (2 = im Spiel)
    pub fn task(&self) -> i32 {
        unsafe { (self.api.task)() }
    }

    /// zurück auf den Zustand direkt nach dem Start (wie ein Reset)
    pub fn reset(&mut self) {
        let boot = G.lock().ok().and_then(|g| g.boot.clone());
        if let Some(b) = boot {
            self.load_state(&b);
        }
    }

    /// Item-Symbol aufnehmen: ein Bild nur mit den Hintergrundebenen aus `mask` zeichnen (Bit 0 = BG0 …)
    pub fn render_layers(&mut self, mask: i32, pad: u16, frame: &mut Vec<u8>) -> Option<(usize, usize)> {
        unsafe { (self.api.layers)(mask) };
        let r = self.run_frame(pad, frame);
        unsafe { (self.api.layers)(0x0F) };
        r
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if let Ok(mut g) = G.lock() {
            g.busy = false;
        }
    }
}
