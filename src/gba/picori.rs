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
type FWarp = unsafe extern "C" fn(i32, i32, i32, i32) -> i32;
type FXYv = unsafe extern "C" fn(i32, i32);
type FProg = unsafe extern "C" fn(*mut i32, i32) -> i32;
type FName = unsafe extern "C" fn(i32, *mut u8, i32) -> i32;
type FPtrI = unsafe extern "C" fn(*mut i32) -> i32;
type FRoom = unsafe extern "C" fn(i32, i32, *mut i32) -> i32;
type FLocLoad = unsafe extern "C" fn(*const u8, i32) -> i32;
type FLocInfo = unsafe extern "C" fn(i32, *mut u8, i32, *mut u32, *mut i32) -> i32;
type FLocDone = unsafe extern "C" fn(u32) -> i32;
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
    map_view: FPtrI,
    render_bg: FFrame,
    room_rect: FRoom,
    loc_load: FLocLoad,
    loc_count: FVoidI,
    loc_info: FLocInfo,
    loc_done: FLocDone,
    area_kind: FIntR,
    own_map: FInt,
    map_request: FVoidI,
    menu_goto: FIntR,
    room_count: FIntR,
    warp: FWarp,
    gen_ready: FVoidI,
    cam: FXYv,
    progress: FProg,
    area_name: FName,
    travel: FWarp,
    place_safe: FVoidI,
    room_marks: FProg,
    noclip: FInt,
    complete: FVoidI,
    room_exits: FProg,
    hide_counts: FInt,
    menu_capture: FInt,
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
            map_view: f!("tmc_view"),
            render_bg: f!("tmc_render_bg"),
            room_rect: f!("tmc_room_rect"),
            loc_load: f!("tmc_loc_load"),
            loc_count: f!("tmc_loc_count"),
            loc_info: f!("tmc_loc_info"),
            loc_done: f!("tmc_loc_done"),
            area_kind: f!("tmc_area_kind"),
            own_map: f!("tmc_set_own_map"),
            map_request: f!("tmc_map_request"),
            menu_goto: f!("tmc_menu_goto"),
            room_count: f!("tmc_room_count"),
            warp: f!("tmc_warp"),
            gen_ready: f!("tmc_gen_ready"),
            cam: f!("tmc_cam"),
            progress: f!("tmc_progress"),
            area_name: f!("tmc_area_name"),
            travel: f!("tmc_travel"),
            place_safe: f!("tmc_place_safe"),
            room_marks: f!("tmc_room_marks"),
            noclip: f!("tmc_noclip"),
            complete: f!("tmc_cheat_complete"),
            room_exits: f!("tmc_room_exits"),
            hide_counts: f!("tmc_hide_counts"),
            menu_capture: f!("tmc_menu_capture"),
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

    /// Karte: Gebiet, Raum, Oberwelt, Raum-Ursprung, Kamera, Raumgröße, Link, Bildbreite (None = nicht im Spiel)
    pub fn map_view(&self) -> Option<[i32; 12]> {
        if !self.ready {
            return None;
        }
        let mut v = [0i32; 12];
        (unsafe { (self.api.map_view)(v.as_mut_ptr()) } != 0).then_some(v)
    }

    /// Karte: das letzte Spielbild noch einmal ohne Figuren (RGBA, Breite, Höhe)
    pub fn render_bg(&self) -> Option<(Vec<u8>, usize, usize)> {
        if !self.ready {
            return None;
        }
        let (mut w, mut h) = (0i32, 0i32);
        let p = unsafe { (self.api.render_bg)(&mut w, &mut h) };
        if p.is_null() || w <= 0 || h <= 0 {
            return None;
        }
        let (w, h) = (w as usize, h as usize);
        let px = unsafe { std::slice::from_raw_parts(p as *const u8, w * h * 4) }.to_vec();
        Some((px, w, h))
    }

    /// Weltposition und Größe eines Raums
    pub fn room_rect(&self, area: u8, room: u8) -> Option<[i32; 4]> {
        let mut v = [0i32; 4];
        (unsafe { (self.api.room_rect)(area as i32, room as i32, v.as_mut_ptr()) } != 0).then_some(v)
    }

    /// Gebiet: Bit 0 Oberwelt, Bit 1 Dungeon
    pub fn area_kind(&self, area: u8) -> i32 {
        unsafe { (self.api.area_kind)(area as i32) }
    }

    /// Fundorte aus der Randomizer-Logik von Picori: (Name, Schlüssel, Art)
    pub fn locations(&self, logic: &str) -> Vec<(String, u32, i32)> {
        if unsafe { (self.api.loc_load)(logic.as_ptr(), logic.len() as i32) } == 0 {
            return Vec::new();
        }
        let n = unsafe { (self.api.loc_count)() }.max(0);
        let mut v = Vec::new();
        let mut buf = vec![0u8; 256];
        for i in 0..n {
            let (mut key, mut ty) = (0u32, 0i32);
            if unsafe { (self.api.loc_info)(i, buf.as_mut_ptr(), buf.len() as i32, &mut key, &mut ty) } == 0 {
                continue;
            }
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            v.push((String::from_utf8_lossy(&buf[..end]).into_owned(), key, ty));
        }
        v
    }

    /// Fundort schon eingesammelt? None = unbekannt
    pub fn loc_done(&self, key: u32) -> Option<bool> {
        match unsafe { (self.api.loc_done)(key) } {
            -1 => None,
            v => Some(v != 0),
        }
    }

    /// eigene Karte statt der Kartenseite des Pausenmenüs
    pub fn set_own_map(&self, on: bool) {
        if self.ready {
            unsafe { (self.api.own_map)(on as i32) };
        }
    }

    /// 1 = im Pausenmenü zur Kartenseite geblättert, 2 = Kartenseite des Spiels offen, 0 = nichts
    pub fn map_request(&self) -> i32 {
        if !self.ready {
            return 0;
        }
        unsafe { (self.api.map_request)() }
    }

    /// Pausenmenü von der eigenen Karte aus: 1 = L-Seite, 2 = R-Seite, 0 = schließen
    pub fn menu_goto(&self, which: i32) -> bool {
        self.ready && unsafe { (self.api.menu_goto)(which) } != 0
    }

    /// ganze Karte: Räume je Gebiet, Sprung in einen Raum, „Raum fertig“, Kamera festhalten (x < 0 = Link folgen)
    pub fn room_count(&self, area: u8) -> i32 {
        if !self.ready {
            return 0;
        }
        unsafe { (self.api.room_count)(area as i32) }
    }
    pub fn warp(&self, area: u8, room: u8, x: i32, y: i32) -> bool {
        self.ready && unsafe { (self.api.warp)(area as i32, room as i32, x, y) } != 0
    }
    pub fn gen_ready(&self) -> i32 {
        if !self.ready {
            return -1;
        }
        unsafe { (self.api.gen_ready)() }
    }
    /// Werte für die Quest-Seite (siehe tmc_progress)
    pub fn progress(&self) -> Vec<i32> {
        if !self.ready {
            return Vec::new();
        }
        let mut v = vec![0i32; 40];
        let n = unsafe { (self.api.progress)(v.as_mut_ptr(), 40) }.max(0) as usize;
        v.truncate(n);
        v
    }
    /// Name eines Gebiets und ob ein Sprung dorthin sicher ist
    pub fn area_name(&self, area: u8) -> Option<(String, bool)> {
        if !self.ready {
            return None;
        }
        let mut buf = vec![0u8; 96];
        let r = unsafe { (self.api.area_name)(area as i32, buf.as_mut_ptr(), buf.len() as i32) };
        if r == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some((String::from_utf8_lossy(&buf[..end]).into_owned(), r == 1))
    }
    /// Schnellreise: Sprung in einen Raum (ohne Änderung am Spielstand), danach Link auf freie Stelle
    pub fn travel(&self, area: u8, room: u8, x: i32, y: i32) -> bool {
        self.ready && unsafe { (self.api.travel)(area as i32, room as i32, x, y) } != 0
    }
    /// besondere Punkte im aktuellen Raum: (Art, x, y) – 1 Schrumpfpunkt, 2 Windwappen, 3 Bohnenranke
    pub fn room_marks(&self) -> Vec<(u8, i32, i32)> {
        if !self.ready {
            return Vec::new();
        }
        let mut v = vec![0i32; 3 * 64];
        let n = unsafe { (self.api.room_marks)(v.as_mut_ptr(), v.len() as i32) }.max(0) as usize;
        (0..n.min(64)).map(|i| (v[i * 3] as u8, v[i * 3 + 1], v[i * 3 + 2])).collect()
    }
    /// Cheats: durch Wände gehen; alles, was in die 100 % zählt (Rückgabe: Fundorte als geholt markiert)
    /// Türen des aktuellen Raums: (x, y Welt, Zielgebiet, Zielraum, Ziel-x, Ziel-y im Raum)
    pub fn room_exits(&self) -> Vec<(i32, i32, u8, u8, i32, i32)> {
        if !self.ready {
            return Vec::new();
        }
        let mut v = vec![0i32; 7 * 64];
        let n = unsafe { (self.api.room_exits)(v.as_mut_ptr(), v.len() as i32) }.max(0) as usize;
        (0..n.min(64)).map(|i| (v[i * 7 + 1], v[i * 7 + 2], v[i * 7 + 3] as u8, v[i * 7 + 4] as u8, v[i * 7 + 5], v[i * 7 + 6])).collect()
    }
    /// Anzahl (Bomben/Pfeile) auf den Tasten ausblenden
    /// Aufnahme aus dem Item-Menü: Bit 0 ohne Anzahl
    pub fn set_menu_capture(&self, mode: i32) {
        if self.ready {
            unsafe { (self.api.menu_capture)(mode) };
        }
    }
    pub fn set_hide_counts(&self, on: bool) {
        if self.ready {
            unsafe { (self.api.hide_counts)(on as i32) };
        }
    }
    pub fn set_noclip(&self, on: bool) {
        if self.ready {
            unsafe { (self.api.noclip)(on as i32) };
        }
    }
    pub fn cheat_complete(&self) -> i32 {
        if !self.ready {
            return 0;
        }
        unsafe { (self.api.complete)() }
    }
    pub fn place_safe(&self) -> bool {
        self.ready && unsafe { (self.api.place_safe)() } != 0
    }
    pub fn cam(&self, x: i32, y: i32) {
        if self.ready {
            unsafe { (self.api.cam)(x, y) };
        }
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
