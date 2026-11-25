//! App-Zustand und Logik: ROM laden, Emulation, Eingabe, Hotkeys, Spielstände, Randomizer.
//! Die Fenster-Oberfläche (Menüleiste, Statusleiste, Fenster) liegt in `shell.rs`, das selbst
//! Gezeichnete (Spielbild, Karte, Inventar …) in `ui.rs`, `mapview.rs` und `inventory.rs`.

use crate::alttp::{self, CheatCode};
use crate::audio::AudioOut;
use crate::filters::{self, Filter};
use crate::hotkeys::{Act, Combo, ACTS};
use crate::i18n::tr;
use crate::randomizer::{self, ChestTable, Mode, RandoResult};
use crate::settings::{Settings, BUTTONS};
use crate::snes::dsp::DspCfg;
use crate::snes::ppu::{self, Snap, H, MAX_EXT, W};
use crate::snes::Snes;
use crate::switchpro::{self as pro, SwitchPro};
use crate::quickmenu::{self, QuickMenu};
use crate::tracker::{MarkerKind, Tracker};
use crate::world::{self, DungeonMap, Entrance, Location};
use std::collections::HashSet;
use crate::gfx::{self, Color32, Event, Key};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const NTSC_FPS: f64 = 21_477_272.0 / (1364.0 * 262.0);
pub const PAL_FPS: f64 = 21_281_370.0 / (1364.0 * 312.0);
pub const SLOTS: u8 = 6;

impl Game {
    /// Dungeon-Karte: alle unsichtbar geladenen Räume verwerfen (neuer Spielstand)
    pub fn reset_shots(&mut self) {
        self.room_shots.clear();
        self.shot_pending.clear();
        self.shot_gen = self.shot_gen.wrapping_add(1);
    }

    /// Eingang desselben Dungeons (richtige Grafiken), über den ein Raum unsichtbar geladen wird
    pub fn room_eid(&self, room: u16) -> u8 {
        let d = world::room_dungeon(&self.dmaps, room);
        self.ents
            .iter()
            .find(|e| d.is_some() && world::room_dungeon(&self.dmaps, e.room) == d)
            .map(|e| e.eid)
            .unwrap_or_else(|| alttp::ram8(&self.snes, 0x7E010E))
    }

    /// Räume für die Dungeon-Karte im Hintergrund laden (der erste zuerst); nur wenn das Spiel gerade steht
    /// (Teleport möglich) – ein Eingang desselben Dungeons sorgt für die richtigen Grafiken
    pub fn request_shots(&mut self, rooms: &[u16]) {
        while let Ok((gen, r, v)) = self.shot_rx.try_recv() {
            if gen != self.shot_gen {
                continue;
            }
            self.shot_pending.remove(&r);
            if let Some(v) = v {
                self.room_shots.insert(r, Arc::new(v));
            }
        }
        let todo: Vec<u16> = rooms.iter().copied().filter(|r| !self.room_shots.contains_key(r) && !self.shot_pending.contains(r)).collect();
        let Some(rt) = self.room_table else { return };
        if todo.is_empty() || !crate::teleport::can_teleport(&self.snes) {
            return;
        }
        let eid = self.room_eid(todo[0]);
        self.shot_pending.extend(todo.iter().copied());
        let base = self.snes.clone();
        let tx = self.shot_tx.clone();
        let gen = self.shot_gen;
        let _ = std::thread::Builder::new().name("room-shots".into()).spawn(move || {
            for r in todo {
                let v = crate::teleport::capture_room_shot(&base, rt, eid, r);
                if tx.send((gen, r, v)).is_err() {
                    break;
                }
            }
        });
    }
}

pub struct Game {
    pub snes: Snes,
    pub orig_rom: Arc<Vec<u8>>,
    pub path: PathBuf,
    pub info: alttp::RomInfo,
    pub rewind: VecDeque<Vec<u8>>,
    pub rewind_tick: u32,
    pub display: Vec<u16>,
    pub sram_timer: f64,
    pub map_timer: f64,
    pub tracker: Tracker,
    pub chests: Option<ChestTable>,
    pub rando: Option<RandoResult>,
    /// Alle Orte für 100 % (Herzteile, Herzcontainer, Truhen, Minispiele)
    pub locations: Vec<Location>,
    /// von Hand abgehakte Orte (Minispiele)
    pub manual: HashSet<usize>,
    /// Dungeon-Positionen (Welt, x, y) – am echten Eingang
    pub dungeons: Vec<(u8, f32, f32)>,
    /// alle Türen/Höhleneingänge der Oberwelt (für Teleport und Karte)
    pub ents: Vec<Entrance>,
    /// Grundrisse der 14 Dungeons aus der ROM
    pub dmaps: Vec<DungeonMap>,
    /// Breitbild-Ausgabe des letzten Bildes
    pub wide: Vec<u16>,
    /// Map16-Definitionen der Oberwelt (ROM $0F:8000) für die Breitbildränder
    pub m16tab: Option<Arc<Vec<u16>>>,
    /// Hintergrund-Aufnahme aller Gebiete (für echte Kacheln im Breitbildrand)
    pub areas_job: Option<std::sync::mpsc::Receiver<(u8, ppu::AreaData)>>,
    pub areas_done: bool,
    areas_new: Vec<(u8, Vec<u16>, Vec<u16>, Vec<u16>)>,
    /// Farben eines gespeicherten Gebiets wurden nachgezogen -> `.areas2` neu schreiben
    areas_dirty: bool,
    /// letzte gültige Gebietskarte (für stabile Ränder beim Gebietswechsel)
    last_ow: Option<std::sync::Arc<ppu::OwMap>>,
    /// wie lange die Farben schon unverändert sind (nur stabile Farben übernehmen)
    pal_stable: (u64, u32),
    /// Spielzeit in Bildern (nur im Spiel, nicht im Titel/Menü der Dateiauswahl)
    pub play_frames: u64,
    /// Bruchteil eines Bilds der Spielzeit (Echtzeit-Zählung)
    play_acc: f64,
    pub deaths: u32,
    last_module: u8,
    /// Link stirbt gerade (Todessequenz läuft) – gezählt wird erst, wenn sie zu Ende läuft
    /// (Zurückspulen, Laden oder Teleport davor zählt nicht)
    dying: bool,
    /// Bewegungssperre hängt: wie lange schon (Bilder) und Links letzte Position
    freeze_t: u32,
    freeze_pos: (u16, u16),
    /// Paletten-Tausch hängt: wie lange schon (Bilder)
    swap_t: u32,
    /// "Immer hell": (Raum, ursprüngliche Farbmathe-Einstellung) – beim Ausschalten wieder dunkel machen
    light_orig: Option<(u16, u8)>,
    /// in dieser Sitzung betretene Eingänge
    pub visited: HashSet<u8>,
    /// eigene Kartenmarkierungen (Welt, x, y, Symbol 0..5)
    pub pins: Vec<(u8, f32, f32, u8)>,
    /// Figuren aus der ROM (Sprite-ID, Welt, x, y) für genaue Kartenpositionen
    pub npcs: Vec<(u8, u8, f32, f32)>,
    /// zweiter Item-Slot (X)
    pub x_item: Option<quickmenu::Entry>,
    /// verfolgte Quest (Index in mapdata::QUESTS)
    pub track: Option<usize>,
    /// Bilder besuchter Räume (64×64, 15 Bit) und genommene Treppen (von, nach)
    pub room_imgs: HashMap<u16, Vec<u16>>,
    /// Grafikspeicher je besuchtem Raum (für den Breitbildrand beim Raumwechsel)
    pub room_vram: HashMap<u16, Arc<Vec<u16>>>,
    /// Nachbarräume, die gerade unsichtbar geladen werden (Hintergrund-Thread)
    room_pending: HashSet<u16>,
    room_tx: std::sync::mpsc::Sender<(u16, Vec<u16>)>,
    room_rx: std::sync::mpsc::Receiver<(u16, Vec<u16>)>,
    /// Dungeon-Karte: Räume in voller Größe, unsichtbar geladen; Stand (gen) gegen
    /// veraltete Ergebnisse nach dem Laden eines Spielstands
    pub room_shots: HashMap<u16, Arc<crate::teleport::RoomShot>>,
    pub shot_pending: HashSet<u16>,
    pub shot_gen: u32,
    pub shot_tx: std::sync::mpsc::Sender<(u32, u16, Option<crate::teleport::RoomShot>)>,
    pub shot_rx: std::sync::mpsc::Receiver<(u32, u16, Option<crate::teleport::RoomShot>)>,
    /// Raumtabelle der Eingänge in der ROM (zum unsichtbaren Laden)
    pub(crate) room_table: Option<usize>,
    pub stairs: Vec<(u16, u16)>,
    last_room: u16,
    room_frames: u32,
}

/// Gebiete (Karte + Grafik) für die Breitbildränder neben der ROM speichern/laden (`.areas`).
pub(crate) fn load_areas(rom: &Path) -> bool {
    let Some(raw) = std::fs::read(rom.with_extension("areas2")).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()) else { return false };
    let w = |i: usize| raw.get(i..i + 2).map(|b| b[0] as u16 | (b[1] as u16) << 8).unwrap_or(0);
    let rec = 1 + (4096 + 0x4000 + 256) * 2;
    let mut n = 0;
    for k in 0..raw.len() / rec {
        let o = k * rec;
        let id = raw[o];
        let words = |from: usize, cnt: usize| (0..cnt).map(|i| w(o + 1 + (from + i) * 2)).collect::<Vec<u16>>();
        let pa = (id & 0x3F) as i32;
        let size = if crate::teleport::area_parents().iter().filter(|&&p| p == id).count() > 1 { 1024 } else { 512 };
        ppu::set_area(id, ppu::AreaData { x0: (pa % 8) * 512, y0: (pa / 8) * 512, size, m16: words(0, 4096), chr: words(4096, 0x4000), cgram: words(4096 + 0x4000, 256) });
        n += 1;
    }
    n > 40
}

/// Nebel-Overlay des aktuellen Gebiets merken (mittlere Farbe) und neben der ROM speichern (`.fog`)
fn note_fog(g: &mut Game) {
    let a = g.snes.bus.ppu.fog_acc.iter().map(|v| v.swap(0, std::sync::atomic::Ordering::Relaxed)).collect::<Vec<u32>>();
    if a[3] < 2000 || alttp::ram8(&g.snes, 0x7E001B) != 0 || alttp::ram8(&g.snes, 0x7E0010) != 0x09 || alttp::ram8(&g.snes, 0x7E0011) != 0 {
        return;
    }
    if !g.snes.bus.ppu.ow.as_ref().map_or(false, |o| o.valid && !o.room) {
        return;
    }
    let n = a[3];
    let c = ((a[0] / n) | (a[1] / n) << 5 | (a[2] / n) << 10) as u16;
    let area = alttp::ram8(&g.snes, 0x7E008A) as usize;
    let parent = crate::teleport::area_parents().get(area).copied().unwrap_or(area as u8);
    if ppu::set_fog(parent, c) {
        let raw: Vec<u8> = ppu::fog_table().iter().flat_map(|v| v.to_le_bytes()).collect();
        let _ = std::fs::write(g.path.with_extension("fog"), raw);
    }
}

/// gespeicherte Nebelfarben laden
pub(crate) fn load_fog(rom: &Path) {
    let raw = std::fs::read(rom.with_extension("fog")).unwrap_or_default();
    ppu::load_fog(raw.chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
}

fn save_areas(rom: &Path, list: &[(u8, Vec<u16>, Vec<u16>, Vec<u16>)]) {
    let mut raw = Vec::new();
    for (id, m16, chr, cg) in list {
        raw.push(*id);
        for v in m16.iter().chain(chr.iter()).chain(cg.iter()) {
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    let _ = std::fs::write(rom.with_extension("areas2"), lz4_flex::compress_prepend_size(&raw));
}

/// Raumbilder und Treppen eines Savestates (`.ssN.rooms`) laden.
fn load_rooms(file: &Path) -> (HashMap<u16, Vec<u16>>, Vec<(u16, u16)>) {
    let mut imgs = HashMap::new();
    let mut stairs = Vec::new();
    let Some(raw) = std::fs::read(file).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok()) else { return (imgs, stairs) };
    let w = |i: usize| raw.get(i..i + 2).map(|b| b[0] as u16 | (b[1] as u16) << 8);
    let mut o = 0;
    let Some(n) = w(o) else { return (imgs, stairs) };
    o += 2;
    for _ in 0..n {
        let Some(room) = w(o) else { break };
        o += 2;
        let px: Vec<u16> = (0..4096).filter_map(|k| w(o + k * 2)).collect();
        o += 8192;
        if px.len() == 4096 {
            imgs.insert(room, px);
        }
    }
    if let Some(m) = w(o) {
        o += 2;
        for k in 0..m as usize {
            if let (Some(a), Some(b)) = (w(o + k * 4), w(o + k * 4 + 2)) {
                stairs.push((a, b));
            }
        }
    }
    (imgs, stairs)
}

fn save_rooms(file: &Path, imgs: &HashMap<u16, Vec<u16>>, stairs: &[(u16, u16)]) {
    let mut raw: Vec<u8> = Vec::new();
    raw.extend_from_slice(&(imgs.len() as u16).to_le_bytes());
    for (room, px) in imgs {
        raw.extend_from_slice(&room.to_le_bytes());
        for c in px {
            raw.extend_from_slice(&c.to_le_bytes());
        }
    }
    raw.extend_from_slice(&(stairs.len() as u16).to_le_bytes());
    for (a, b) in stairs {
        raw.extend_from_slice(&a.to_le_bytes());
        raw.extend_from_slice(&b.to_le_bytes());
    }
    let _ = std::fs::write(file, lz4_flex::compress_prepend_size(&raw));
}

/// Zusatzinfos eines Savestates (Spielzeit, Tode, Herzen, Ort)
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SlotMeta {
    pub play_frames: u64,
    pub fps: f64,
    pub deaths: u32,
    pub health: u8,
    pub max_health: u8,
    pub place_de: String,
    pub place_en: String,
    /// von Hand abgehakte Orte (Minispiele)
    pub manual: Vec<usize>,
    /// betretene Eingänge (für Teleport-Punkte)
    pub visited: Vec<u8>,
    /// eigene Kartenmarkierungen
    pub pins: Vec<(u8, f32, f32, u8)>,
    /// zweiter Item-Slot (Menü-Index, Flasche)
    pub x_item: Option<(u8, Option<u8>)>,
    /// verfolgte Quest
    pub track: Option<usize>,
    /// Rubine (fehlt in älteren Dateien -> wird einmal aus dem Savestate gelesen)
    pub rupees: Option<u16>,
}

impl SlotMeta {
    pub fn time_text(&self) -> String {
        fmt_time(self.play_frames, self.fps)
    }
}

/// Teleport-Animation: Ausblenden (Pixelquadrate wachsen von Link aus), Sprung, Einblenden
#[derive(Clone, Copy)]
pub struct TpAnim {
    pub t0: Instant,
    /// genau diese Tür (mehrere Türen können sich eine Eingangs-ID teilen, z. B. Feen-Höhlen und Läden)
    pub ent: Entrance,
    pub jumped: bool,
    /// Link auf dem Bildschirm (Spielpixel)
    pub from: (f32, f32),
}
pub const TP_OUT: f32 = 0.35;
pub const TP_IN: f32 = 0.45;

pub fn fmt_time(frames: u64, fps: f64) -> String {
    let s = (frames as f64 / fps.max(1.0)) as u64;
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

pub struct App {
    pub(crate) s: Settings,
    pub(crate) game: Option<Game>,
    pub(crate) audio: Option<AudioOut>,
    pub(crate) gilrs: Option<gilrs::Gilrs>,
    pub(crate) pro: SwitchPro,
    pub(crate) qm: QuickMenu,
    pub(crate) qm_held: bool,
    pub(crate) qm_dir: gfx::Vec2,
    disp_dt: f64,
    pro_prev: u32,
    pro_connected: bool,
    pub(crate) tex: Option<gfx::TextureHandle>,
    pub(crate) gpu: crate::gpu::GpuScreen,
    /// Mods (eingebaut + DLLs aus dem Ordner „mods“)
    pub(crate) mods: crate::mods::Mods,
    pub(crate) auto_paused: bool,
    pub(crate) map_tex: [Option<gfx::TextureHandle>; 2],
    pub(crate) full_tex: [Option<gfx::TextureHandle>; 2],
    #[allow(dead_code)]
    pub(crate) map_upload_tick: u32,
    pub(crate) thumbs: HashMap<u8, (Option<SystemTime>, Option<gfx::TextureHandle>)>,
    pub(crate) metas: HashMap<u8, (Option<SystemTime>, Option<SlotMeta>)>,
    /// angezeigtes Bild (Breite `view_w` × 224) – Zwischenbild, Breitbild oder normales Bild
    pub(crate) view: Vec<u16>,
    pub(crate) view_w: usize,
    /// Bild ohne das eingezeichnete X-Feld (zum Neuzeichnen, wenn sich nur das Feld ändert)
    view_base: Vec<u16>,
    hud_last: Option<(i32, bool, u8, usize)>,
    shown: Vec<u16>,
    shown_w: usize,
    pub(crate) snaps: [Option<Arc<Snap>>; 2],
    frame_no: u64,
    interp_key: (u64, u32, usize),
    interp_ok: bool,
    /// Breitbild-Rand je Seite in SNES-Pixeln
    pub(crate) ext: usize,
    /// Seitenverhältnis der Spielfläche im Fenster (für „Fenster füllen“)
    pub(crate) area_aspect: f32,
    pub(crate) map_dungeon: bool,
    pub(crate) map_dsel: u8,
    #[allow(dead_code)]
    pub(crate) map_auto_prev: Option<u8>,
    pub(crate) map_floor: Option<usize>,
    #[allow(dead_code)]
    pub(crate) map_floor_link: Option<usize>,
    pub(crate) map_floor_dungeon: Option<u8>,
    pub(crate) map_focused: bool,
    /// Bildschirmpixel je Spielpixel (für Einblendungen im Pixel-Stil)
    pub(crate) game_px: f32,
    pub(crate) tp_anim: Option<TpAnim>,
    pub(crate) mirror_pending: bool,
    mirror_press: u8,
    menu_close: u8,
    joy_prev: u16,
    x_using: bool,
    x_frames: u32,
    x_restore: (u8, u8, u8),
    pub(crate) x_notice: Option<quickmenu::Entry>,
    minus_now: bool,
    minus_prev: bool,
    pub(crate) inv_open: bool,
    pub(crate) inv_sel: usize,
    pub(crate) inv_prev: u16,
    pub(crate) inv_block: u16,
    inv_start_prev: u16,
    pub(crate) x_force: Option<quickmenu::Entry>,
    x_hold: u8,
    /// während der X-Benutzung lief Text o. Ä. -> Spiel-Anzeige beim Zurücksetzen neu aufbauen
    x_dirty: bool,
    pub(crate) fairy_msg: Option<u8>,
    /// Flaschen-Inhalte im letzten Bild (Feen-Hinweis)
    bottles_prev: [u8; 4],
    pub(crate) qm_for_x: bool,
    l3_prev: bool,
    /// Steuerkreuz des Controllers (nur für Menüs, nicht fürs Spiel)
    pub(crate) ctl_dpad: u16,
    /// Schnellmenü über Steuerkreuz → am Controller gehalten
    pub(crate) qm_pad: bool,
    ctl_dpad_prev: u16,
    pub(crate) game_fps_count: u32,
    pub(crate) game_fps: f64,
    pub(crate) room_tex: HashMap<u16, gfx::TextureHandle>,
    /// Räume in voller Größe (Dungeon-Karte, hereingezoomt) und Zoom der Dungeon-Karte (Ziel, aktuell, Blickmitte)
    pub(crate) room_hd_tex: HashMap<u16, gfx::TextureHandle>,
    pub(crate) map_dzoom: f32,
    pub(crate) map_dzoom_cur: f32,
    pub(crate) map_dview: gfx::Vec2,
    /// Raster der Dungeon-Karte (Ursprung, Zellgröße) für Mausklicks
    pub(crate) map_dgeo: (gfx::Pos2, f32),
    /// Stick/Steuerkreuz nach oben/unten (Stockwerk): letzte Richtung
    pub(crate) map_rflick: i32,
    /// Tab gedrückt (Turbo)
    tab_down: bool,
    /// Teleport-Auswahl in der Karte im Spiel
    pub(crate) igm_sel: usize,
    pub(crate) igm_view: (i32, i32, i32),
    igm_prev: u16,
    cur: Vec<Color32>,
    prev: Vec<Color32>,
    blend: Vec<Color32>,
    pub(crate) ftmp: Vec<Color32>,
    pub(crate) fout: Vec<Color32>,
    pub(crate) lut: Vec<Color32>,
    lut_key: (bool, u32, u32, u32),
    tex_key: (Filter, bool),
    new_frame: bool,
    last: Instant,
    pub(crate) acc: f64,
    pub(crate) paused: bool,
    pub(crate) advance: bool,
    pub(crate) slot: u8,
    pub(crate) status: Option<(String, Instant)>,
    // Popups
    pub(crate) win_states: bool,
    pub(crate) win_map: bool,
    pub(crate) win_rando: bool,
    pub(crate) win_cheats: bool,
    /// Fenster „Mod-Liste“
    pub(crate) win_mods: bool,
    /// GBA-Modus (z. B. The Minish Cap, Kern mGBA)
    pub(crate) gba: crate::gba::ui::GbaView,
    pub(crate) win_controls: bool,
    pub(crate) win_hotkeys: bool,
    pub(crate) show_bar: bool,
    pub(crate) fullscreen: bool,
    pub(crate) rebind: Option<usize>,
    pub(crate) rebind_hotkey: Option<&'static str>,
    pub(crate) delete_confirm: Option<u8>,
    /// Hauptmenü: gewählte Karte, vorige Tasten, Rückfrage „zum Hauptmenü“
    pub(crate) lib_sel: usize,
    /// Logo von A Link to the Past für die Spielkarte im Hauptmenü
    pub(crate) alttp_logo: Option<gfx::TextureHandle>,
    pub(crate) lib_prev: u16,
    pub(crate) main_confirm: bool,
    /// Spielstände: der Hauptmenü-Knopf unter den Karten ist angewählt
    pub(crate) states_menu: bool,
    /// untere Knopfreihe im Spielstand-Menü: 0 Hauptmenü, 1 Einstellungen
    pub(crate) states_menu_sel: u8,
    /// Einstellungen mit dem Controller (aus dem Spielstand-Menü): offen, gewählte Zeile, Bildlauf
    pub(crate) win_pad_settings: bool,
    pub(crate) pset_sel: usize,
    pub(crate) pset_scroll: f32,
    fps_count: u32,
    fps_time: Instant,
    pub(crate) fps: f64,
    pub(crate) code_input: String,
    pub(crate) code_desc: String,
    pub(crate) code_err: String,
    pub(crate) unknown_rom: Option<(Vec<u8>, PathBuf)>,
    abuf: Vec<i16>,
    pub(crate) sword: u8,
    pub(crate) shield: u8,
    pub(crate) armor: u8,
    rom_code_sig: String,
    title_dirty: bool,
    saved_settings: String,
    settings_timer: Instant,
    pub(crate) map_world: u8,
    pub(crate) map_zoom: f32,
    pub(crate) map_center: gfx::Vec2,
    pub(crate) game_area: gfx::Rect,
    /// Fläche zwischen Menü- und Statusleiste (setzt `shell::Root` vor jedem Bild)
    pub(crate) central: gfx::Rect,
    pub(crate) lstick: gfx::Vec2,
    /// Richtungen, die nur vom linken Stick kommen (Karte: Stick bewegt den Cursor, Steuerkreuz das Stockwerk)
    pub(crate) lstick_pad: u16,
    /// Ebenen, die gerade gezeichnet werden (Einstellung ohne Schatten-Überlagerung, siehe `shadow_overlay`)
    pub(crate) eff_mask: u8,
    pub(crate) rstick: gfx::Vec2,
    pub(crate) map_prev_pad: u16,
    pub(crate) map_room_cur: (i32, i32),
    pub(crate) map_sel: Option<crate::mapview::MapSel>,
    pub(crate) map_ov_tex: Option<gfx::TextureHandle>,
    pub(crate) map_ov_key: Option<(u8, bool)>,
    pub(crate) map_fit: f32,
    pub(crate) map_cur: gfx::Vec2,
    /// Anzeige „Eigene“: Zeilen der Auswahl (Fläche, Bit) und wohin das Fadenkreuz zeigt
    pub(crate) map_custom_rows: Vec<(gfx::Rect, u8)>,
    pub(crate) map_aim: gfx::Pos2,
    /// 3 = 100-%-Liste, 4 = Quests (sonst Karten-Reiter)
    pub(crate) map_page: u8,
    pub(crate) map_list_cat: usize,
    pub(crate) map_list_scroll: f32,
    pub(crate) map_quest_sel: usize,
    /// Symbol-Auswahl für eine eigene Markierung (Welt, x, y, Symbol)
    pub(crate) map_pick: Option<(u8, f32, f32, u8)>,
    pub(crate) map_rep: ((i32, i32), f32),
    pub(crate) map_half: gfx::Vec2,
    pub(crate) map_hd: [Option<gfx::TextureHandle>; 2],
    pub(crate) map_hd_job: Option<(u8, std::sync::mpsc::Receiver<Vec<Color32>>)>,
    /// maßstabsgetreue Weltkarte aus den gespeicherten Gebieten (15 Bit, None = kein Gebiet) – ersetzt dort das
    /// gezeichnete Kartenbild des Spiels, damit Links Punkt genau auf dem richtigen Gelände liegt
    pub(crate) map_real: [Option<Vec<Option<u16>>>; 2],
    pub(crate) map_real_job: Option<(u8, std::sync::mpsc::Receiver<Vec<Option<u16>>>)>,
    pub(crate) new_marker: Option<(u8, f32, f32)>,
    pub(crate) marker_label: String,
    pub(crate) marker_kind: MarkerKind,
    pub(crate) show_spoiler: bool,
    pub(crate) states_prev_pad: u16,
    mode_prev: bool,
    prev_noclip: bool,
    title_frame: u32,
    pub(crate) turbo_on: bool,
    pub(crate) double_on: bool,
    pub(crate) rewind_on: bool,
}

fn time_seed() -> u64 {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(12345);
    (n ^ (n >> 29)).wrapping_mul(0x2545_F491_4F6C_DD1D) % 1_000_000_000
}

/// Sucht neben der .exe (und im aktuellen Ordner) nach einer ALttP-ROM.
fn rom_next_to_exe() -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        dirs.push(d);
    }
    if let Ok(d) = std::env::current_dir() {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    let mut fallback = None;
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("sfc") || e.eq_ignore_ascii_case("smc")).unwrap_or(false))
            .collect();
        files.sort();
        for f in files {
            let small = std::fs::metadata(&f).map(|m| m.len() <= 8 << 20).unwrap_or(false);
            if !small {
                continue;
            }
            if let Ok(d) = std::fs::read(&f) {
                if let Ok((_, info)) = alttp::prepare_rom(d) {
                    if info.is_alttp {
                        return Some(f);
                    }
                }
            }
            fallback.get_or_insert(f);
        }
    }
    let _ = fallback;
    None
}

impl App {
    pub fn new(rom_arg: Option<PathBuf>) -> Self {
        let s = Settings::load();
        // umgestellte Einstellungen sofort sichern
        s.save();
        let audio = AudioOut::new(s.latency_ms);
        let saved_settings = serde_json::to_string(&s).unwrap_or_default();
        let mut app = App {
            s,
            metas: HashMap::new(),
            view: vec![0; W * H],
            view_w: W,
            view_base: Vec::new(),
            hud_last: None,
            shown: Vec::new(),
            shown_w: 0,
            snaps: [None, None],
            frame_no: 0,
            interp_key: (0, 0, 0),
            interp_ok: false,
            ext: 0,
            area_aspect: 4.0 / 3.0,
            map_dungeon: false,
            map_dsel: 2,
            map_auto_prev: None,
            map_floor: None,
            map_floor_link: None,
            map_floor_dungeon: None,
            map_focused: false,
            game_px: 3.0,
            tp_anim: None,
            mirror_pending: false,
            mirror_press: 0,
            menu_close: 0,
            joy_prev: 0,
            x_using: false,
            x_frames: 0,
            x_restore: (0, 0, 0),
            x_notice: None,
            minus_now: false,
            minus_prev: false,
            inv_open: false,
            inv_sel: 0,
            inv_prev: 0,
            inv_block: 0,
            inv_start_prev: 0,
            x_force: None,
            x_hold: 0,
            x_dirty: false,
            fairy_msg: None,
            bottles_prev: [0; 4],
            qm_for_x: false,
            l3_prev: false,
            ctl_dpad: 0,
            qm_pad: false,
            ctl_dpad_prev: 0,
            game_fps_count: 0,
            game_fps: 0.0,
            room_tex: HashMap::new(),
            room_hd_tex: HashMap::new(),
            map_dzoom: 1.0,
            map_dzoom_cur: 1.0,
            map_dview: gfx::vec2(2.5, 2.5),
            map_dgeo: (gfx::Pos2::ZERO, 0.0),
            map_rflick: 0,
            tab_down: false,
            igm_sel: 0,
            igm_view: (0, 0, 0),
            igm_prev: 0,
            game: None,
            audio,
            gilrs: gilrs::Gilrs::new().ok(),
            pro: SwitchPro::start(),
            qm: QuickMenu::new(),
            qm_held: false,
            qm_dir: gfx::Vec2::ZERO,
            disp_dt: 1.0 / 60.0,
            pro_prev: 0,
            pro_connected: false,
            tex: None,
            gpu: crate::gpu::GpuScreen::new(),
            mods: crate::mods::Mods::new(),
            auto_paused: false,
            map_tex: [None, None],
            full_tex: [None, None],
            map_upload_tick: 0,
            thumbs: HashMap::new(),
            cur: vec![Color32::BLACK; W * H],
            prev: vec![Color32::BLACK; W * H],
            blend: vec![Color32::BLACK; W * H],
            ftmp: Vec::new(),
            fout: Vec::new(),
            lut: Vec::new(),
            lut_key: (false, 0, 0, 0),
            tex_key: (Filter::Pixel, false),
            new_frame: true,
            last: Instant::now(),
            acc: 0.0,
            paused: false,
            advance: false,
            slot: 1,
            status: None,
            win_states: false,
            win_map: false,
            win_rando: false,
            win_cheats: false,
            // Entwicklung: Fenster gleich beim Start öffnen (PASTLINK_OPEN=mods)
            win_mods: std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "mods"),
            gba: Default::default(),
            win_controls: false,
            win_hotkeys: false,
            show_bar: true,
            fullscreen: false,
            rebind: None,
            rebind_hotkey: None,
            delete_confirm: None,
            lib_sel: 0,
            alttp_logo: None,
            lib_prev: 0xFFFF,
            main_confirm: false,
            states_menu: false,
            states_menu_sel: 0,
            win_pad_settings: false,
            pset_sel: 1,
            pset_scroll: 0.0,
            fps_count: 0,
            fps_time: Instant::now(),
            fps: 0.0,
            code_input: String::new(),
            code_desc: String::new(),
            code_err: String::new(),
            unknown_rom: None,
            abuf: Vec::with_capacity(4096),
            sword: 2,
            shield: 3,
            armor: 2,
            rom_code_sig: String::new(),
            title_dirty: true,
            saved_settings,
            settings_timer: Instant::now(),
            map_world: 0,
            map_zoom: 0.0,
            map_center: gfx::vec2(2048.0, 2048.0),
            game_area: gfx::Rect::NOTHING,
            central: gfx::Rect::from_min_size(gfx::Pos2::ZERO, gfx::vec2(1000.0, 700.0)),
            lstick: gfx::Vec2::ZERO,
            lstick_pad: 0,
            eff_mask: 0x1F,
            rstick: gfx::Vec2::ZERO,
            map_prev_pad: 0xFFFF,
            map_room_cur: (2, 2),
            map_sel: None,
            map_ov_tex: None,
            map_ov_key: None,
            map_fit: 0.1,
            map_cur: gfx::Vec2::ZERO,
            map_custom_rows: Vec::new(),
            map_aim: gfx::Pos2::ZERO,
            map_page: 0,
            map_list_cat: 0,
            map_list_scroll: 0.0,
            map_quest_sel: 0,
            map_pick: None,
            map_rep: ((0, 0), 0.0),
            map_half: gfx::vec2(300.0, 200.0),
            map_hd: [None, None],
            map_hd_job: None,
            map_real: [None, None],
            map_real_job: None,
            new_marker: None,
            marker_label: String::new(),
            marker_kind: MarkerKind::HeartPiece,
            show_spoiler: false,
            states_prev_pad: 0,
            mode_prev: false,
            prev_noclip: false,
            title_frame: 0,
            turbo_on: false,
            double_on: false,
            rewind_on: false,
        };
        app.ensure_lut();
        if app.audio.is_none() {
            app.msg(app.t("Kein Audiogerät gefunden – Ton deaktiviert.", "No audio device found – sound disabled."));
        }
        // Start immer im Hauptmenü; nur eine ausdrücklich übergebene Datei (Kommandozeile, „Öffnen mit“) startet sofort
        if let Some(p) = rom_arg {
            app.load_rom(p, false);
        }
        // Messlauf (nur mit PASTLINK_PERF): Spiel und Spielstand direkt laden
        if let Some(s) = crate::perf::start_slot() {
            let last = PathBuf::from(&app.s.last_rom);
            if app.game.is_none() {
                app.load_rom(last, false);
            }
            app.load_slot(s);
        }
        app
    }

    #[inline]
    pub(crate) fn t(&self, de: &'static str, en: &'static str) -> &'static str {
        tr(self.s.ui_en, de, en)
    }

    pub(crate) fn msg(&mut self, m: impl Into<String>) {
        self.status = Some((m.into(), Instant::now()));
    }

    pub(crate) fn frame_rate(&self) -> f64 {
        match &self.game {
            Some(g) if g.snes.bus.pal => PAL_FPS,
            _ => NTSC_FPS,
        }
    }

    // ------------------------------------------------------------ ROM

    pub(crate) fn open_dialog(&mut self) {
        let mut d = rfd::FileDialog::new()
            .set_title(self.t("A Link to the Past ROM öffnen", "Open A Link to the Past ROM"))
            .add_filter("SNES ROM / Patch", &["sfc", "smc", "bps", "ips"]);
        if let Some(dir) = Path::new(&self.s.last_rom).parent() {
            if dir.is_dir() {
                d = d.set_directory(dir);
            }
        }
        if let Some(p) = d.pick_file() {
            self.open_any(p);
        }
    }

    pub(crate) fn pick_rom_path(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(self.t("ROM wählen", "Choose ROM")).add_filter("SNES ROM", &["sfc", "smc"]).pick_file()
    }

    pub(crate) fn open_any(&mut self, p: PathBuf) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "bps" || ext == "ips" {
            self.patch_flow(p);
        } else {
            self.load_rom(p, false);
        }
    }

    pub(crate) fn load_rom(&mut self, path: PathBuf, force: bool) {
        if crate::gba::is_gba(&path) {
            self.gba_open(path);
            return;
        }
        // SNES-Spiel: ein laufendes GBA-Spiel beenden
        self.gba_close();
        // Recompiled-Modus: Spieldaten stecken in der .exe, die ROM-Datei wird nicht gelesen
        if self.s.recompiled {
            if let Some(d) = crate::recomp::embedded_rom(&path) {
                self.load_rom_data(d, path, force);
                return;
            }
        }
        match std::fs::read(&path) {
            Ok(d) => self.load_rom_data(d, path, force),
            Err(e) => {
                let m = format!("{}: {e}", self.t("Datei kann nicht gelesen werden", "Cannot read file"));
                self.msg(m)
            }
        }
    }

    pub(crate) fn load_rom_data(&mut self, data: Vec<u8>, path: PathBuf, force: bool) {
        let (rom, info) = match alttp::prepare_rom(data) {
            Ok(x) => x,
            Err(e) => {
                self.msg(e);
                return;
            }
        };
        if !info.is_alttp && !force {
            self.unknown_rom = Some((rom, path));
            return;
        }
        self.close_game();
        // Einstellungen dieses Spiels übernehmen (vor dem Aufbau der ROM: Patches, Randomizer)
        if let Some(k) = path.file_stem().map(|k| k.to_string_lossy().to_string()) {
            self.s.apply_game(&k);
        }
        ppu::clear_world_cache();
        let orig = Arc::new(rom);
        let chests = randomizer::find_chest_table(&orig);
        let rando = if self.s.rando_on {
            chests.as_ref().map(|c| randomizer::randomize(c, randomizer::seed_from_text(&self.s.rando_seed), mode_from(self.s.rando_mode)))
        } else {
            None
        };
        let rom = Arc::new(build_rom(&orig, rando.as_ref(), &self.s.codes, (self.s.more_bombs, self.s.sword_pots, self.s.mirror_direct, self.s.quick_dash, self.s.health_blink, self.s.wide_sprites)));
        let pal = info.pal && !self.s.pal60;
        let mut snes = Snes::new(rom, info.sram_size, pal);
        // Spielinterne Spielstände nur, wenn ausdrücklich eingeschaltet – sonst beginnt alles frisch
        if self.s.save_sram {
            if let Ok(d) = std::fs::read(path.with_extension("srm")) {
                if d.len() == snes.bus.sram.len() {
                    snes.bus.sram = d;
                }
            }
        }
        self.s.last_rom = path.to_string_lossy().into_owned();
        let title = info.title.clone();
        let has_rando = rando.is_some();
        // frischer Start: keine Raumbilder/Treppen (die gehören zu den Savestates)
        let (room_imgs, stairs): (HashMap<u16, Vec<u16>>, Vec<(u16, u16)>) = (HashMap::new(), Vec::new());
        ppu::clear_areas();
        ppu::set_area_parents(crate::teleport::area_parents());
        let areas_loaded = load_areas(&path);
        load_fog(&path);
        let m16tab = (orig.len() >= 0x80000).then(|| Arc::new(orig[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect()));
        let mut tracker = Tracker::for_rom(&orig);
        tracker.load_or_capture(&path, orig.clone(), snes.bus.sram.clone());
        let ents = world::find_entrances(&orig);
        let npcs = world::find_npcs(&orig, &ents);
        let dmaps = world::find_dungeon_maps(&orig);
        let room_table = world::entrance_room_table(&orig);
        let (room_tx, room_rx) = std::sync::mpsc::channel();
        let (shot_tx, shot_rx) = std::sync::mpsc::channel();
        let (locations, dungeons) = world::build_locations(&orig, chests.as_ref(), &tracker.statics, &ents, &dmaps);
        self.game = Some(Game {
            snes,
            orig_rom: orig,
            path,
            info,
            rewind: VecDeque::new(),
            rewind_tick: 0,
            display: vec![0; W * H],
            sram_timer: 0.0,
            map_timer: 0.0,
            tracker,
            chests,
            rando,
            locations,
            manual: HashSet::new(),
            dungeons,
            ents,
            dmaps,
            wide: Vec::new(),
            m16tab,
            areas_job: None,
            areas_done: areas_loaded,
            areas_new: Vec::new(),
            areas_dirty: false,
            last_ow: None,
            pal_stable: (0, 0),
            play_frames: 0,
            play_acc: 0.0,
            deaths: 0,
            last_module: 0,
            dying: false,
            freeze_t: 0,
            freeze_pos: (0, 0),
            swap_t: 0,
            light_orig: None,
            visited: HashSet::new(),
            pins: Vec::new(),
            npcs,
            x_item: None,
            track: None,
            room_imgs,
            room_vram: HashMap::new(),
            room_pending: HashSet::new(),
            room_tx,
            room_rx,
            room_shots: HashMap::new(),
            shot_pending: HashSet::new(),
            shot_gen: 0,
            shot_tx,
            shot_rx,
            room_table,
            stairs,
            last_room: 0xFFFF,
            room_frames: 0,
        });
        self.reset_interp();
        self.map_tex = [None, None];
        self.full_tex = [None, None];
        self.thumbs.clear();
        self.rom_code_sig = self.code_sig();
        self.paused = false;
        self.acc = 0.0;
        self.title_dirty = true;
        self.title_frame = 0;
        if let Some(a) = &self.audio {
            a.clear();
        }
        let r = if has_rando { self.t(" · Randomizer AN", " · randomizer ON") } else { "" };
        let m = format!("{}: {title}{r}", self.t("Geladen", "Loaded"));
        self.msg(m);
    }

    pub(crate) fn flush_sram(&mut self) {
        if !self.s.save_sram {
            return;
        }
        if let Some(g) = &mut self.game {
            if g.snes.bus.sram_dirty && !g.snes.bus.sram.is_empty() {
                g.snes.bus.sram_dirty = false;
                let _ = std::fs::write(g.path.with_extension("srm"), &g.snes.bus.sram);
            }
        }
    }

    /// Zurück zum Hauptmenü (Spiel schließen, Fenster zu)
    pub(crate) fn go_main_menu(&mut self) {
        self.gba_close();
        self.win_states = false;
        self.win_map = false;
        self.inv_open = false;
        self.qm.open = false;
        self.close_game();
        self.lib_prev = 0xFFFF;
    }

    /// Spiele für das Hauptmenü: zuletzt gespielt und ROMs neben der .exe
    pub(crate) fn library(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = Vec::new();
        let last = PathBuf::from(&self.s.last_rom);
        if last.is_file() {
            v.push(last);
        }
        if let Some(p) = rom_next_to_exe() {
            if !v.iter().any(|q| q.canonicalize().ok() == p.canonicalize().ok()) {
                v.push(p);
            }
        }
        // Recompiled-Modus: das eingebaute Spiel ist immer spielbar, auch ohne ROM-Datei
        if self.s.recompiled && crate::recomp::has_embedded() {
            let name = crate::recomp::embedded_name();
            if !v.iter().any(|p| p.file_stem().is_some_and(|s| s.to_string_lossy() == name)) {
                let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default();
                v.insert(0, dir.join(format!("{name}.sfc")));
            }
        }
        // GBA-Spiele (z. B. The Minish Cap): zuletzt gespieltes und alle neben der .exe
        if crate::gba::available() {
            let last = PathBuf::from(&self.s.last_gba);
            let mut g: Vec<PathBuf> = if last.is_file() { vec![last] } else { Vec::new() };
            for p in crate::gba::ui::roms_next_to_exe() {
                if !g.iter().any(|q| q.canonicalize().ok() == p.canonicalize().ok()) {
                    g.push(p);
                }
            }
            v.extend(g);
        }
        v
    }

    /// Hauptmenü mit Controller/Tastatur: ←/→ wählen, A/Start spielen
    fn library_input(&mut self, pad: u16) {
        let pressed = pad & !self.lib_prev;
        self.lib_prev = pad;
        let n = self.library().len() + 1; // letzte Karte: ROM hinzufügen
        if pressed & (1 << 9) != 0 && self.lib_sel > 0 {
            self.lib_sel -= 1;
        }
        if pressed & (1 << 8) != 0 && self.lib_sel + 1 < n {
            self.lib_sel += 1;
        }
        self.lib_sel = self.lib_sel.min(n - 1);
        if pressed & ((1 << 7) | (1 << 12)) != 0 {
            self.lib_start(self.lib_sel);
        }
    }

    pub(crate) fn lib_start(&mut self, k: usize) {
        let lib = self.library();
        match lib.get(k) {
            Some(p) => self.load_rom(p.clone(), false),
            None => self.open_dialog(),
        }
    }

    pub(crate) fn close_game(&mut self) {
        self.flush_sram();
        self.room_tex.clear();
        self.room_hd_tex.clear();
        self.game = None;
        self.title_dirty = true;
    }

    // ------------------------------------------------------------ Spielstände (mit Bild)

    pub(crate) fn state_path(&self, slot: u8) -> Option<PathBuf> {
        if let Some(g) = &self.gba.game {
            return Some(crate::gba::state_path(&g.path, slot, g.is_rc()));
        }
        self.game.as_ref().map(|g| g.path.with_extension(format!("ss{slot}")))
    }
    pub(crate) fn thumb_path(&self, slot: u8) -> Option<PathBuf> {
        if let Some(g) = &self.gba.game {
            return Some(g.path.with_extension(format!("{}{slot}.thumb", if g.is_rc() { "rs" } else { "gs" })));
        }
        self.game.as_ref().map(|g| g.path.with_extension(format!("ss{slot}.thumb")))
    }

    pub(crate) fn meta_path(&self, slot: u8) -> Option<PathBuf> {
        if let Some(g) = &self.gba.game {
            return Some(g.path.with_extension(format!("{}{slot}.meta", if g.is_rc() { "rs" } else { "gs" })));
        }
        self.game.as_ref().map(|g| g.path.with_extension(format!("ss{slot}.meta")))
    }

    /// Wo ist Link gerade? (Dungeon, Höhle/Haus bei Gebiet …, Gebiet)
    pub(crate) fn place_name(&self, en: bool) -> String {
        let Some(g) = &self.game else { return String::new() };
        let indoors = alttp::ram8(&g.snes, 0x7E001B) != 0;
        if indoors {
            let room = alttp::ram16(&g.snes, 0x7E00A0);
            if let Some(d) = world::room_dungeon(&g.dmaps, room) {
                let (de, e) = world::DMAP_NAMES[d as usize];
                return (if en { e } else { de }).to_string();
            }
            let eid = alttp::ram8(&g.snes, 0x7E010E);
            if let Some(e) = g.ents.iter().find(|e| e.eid == eid) {
                return format!("{} {}", if en { "Inside:" } else { "Drinnen:" }, world::area_name(e.area, en));
            }
            return (if en { "Inside" } else { "Drinnen" }).to_string();
        }
        world::area_name(alttp::ram8(&g.snes, 0x7E008A), en)
    }

    pub(crate) fn current_meta(&self) -> Option<SlotMeta> {
        let g = self.game.as_ref()?;
        Some(SlotMeta {
            play_frames: g.play_frames,
            fps: self.frame_rate(),
            deaths: g.deaths,
            health: alttp::ram8(&g.snes, 0x7EF36D),
            max_health: alttp::ram8(&g.snes, 0x7EF36C),
            place_de: self.place_name(false),
            place_en: self.place_name(true),
            manual: g.manual.iter().copied().collect(),
            visited: g.visited.iter().copied().collect(),
            pins: g.pins.clone(),
            x_item: g.x_item.map(|e| (e.idx, e.bottle)),
            track: g.track,
            rupees: Some(alttp::ram16(&g.snes, 0x7EF360)),
        })
    }

    pub(crate) fn save_slot(&mut self, slot: u8) {
        if self.gba.active() {
            self.gba_save_slot(slot);
            return;
        }
        let (Some(p), Some(tp), Some(mp)) = (self.state_path(slot), self.thumb_path(slot), self.meta_path(slot)) else { return };
        let meta = self.current_meta().unwrap_or_default();
        let g = self.game.as_ref().unwrap();
        let data = g.snes.save_state();
        let px: Vec<u8> = g.display.iter().flat_map(|c| c.to_le_bytes()).collect();
        let _ = std::fs::write(&tp, lz4_flex::compress_prepend_size(&px));
        let _ = std::fs::write(&mp, serde_json::to_string(&meta).unwrap_or_default());
        save_rooms(&g.path.with_extension(format!("ss{slot}.rooms")), &g.room_imgs, &g.stairs);
        self.slot = slot;
        let m = match std::fs::write(&p, data) {
            Ok(_) => format!(
                "{} {slot} · ⏱ {} · {} {} · ♥ {}/{}",
                self.t("Gespeichert in Slot", "Saved to slot"),
                meta.time_text(),
                self.t("Tode", "deaths"),
                meta.deaths,
                meta.health / 8,
                meta.max_health / 8
            ),
            Err(e) => format!("{}: {e}", self.t("Speichern fehlgeschlagen", "Saving failed")),
        };
        self.thumbs.remove(&slot);
        self.metas.remove(&slot);
        self.msg(m);
    }

    /// Todeszähler auf 0; `all`: auch in den Angaben aller Spielstände (nur die .meta-Dateien, der Spielstand selbst bleibt)
    pub(crate) fn reset_deaths(&mut self, all: bool) {
        if let Some(g) = &mut self.game {
            g.deaths = 0;
            g.dying = false;
        } else if self.gba.active() {
            self.gba.deaths = 0;
        } else {
            return;
        }
        let mut n = 0;
        if all {
            for slot in 0..=99u8 {
                let Some(p) = self.meta_path(slot) else { break };
                let Ok(txt) = std::fs::read_to_string(&p) else { continue };
                if let Ok(mut m) = serde_json::from_str::<SlotMeta>(&txt) {
                    if m.deaths != 0 {
                        m.deaths = 0;
                        if std::fs::write(&p, serde_json::to_string(&m).unwrap_or_default()).is_ok() {
                            n += 1;
                        }
                    }
                }
                self.metas.remove(&slot);
            }
        }
        let m = if all { format!("{} ({n} {})", self.t("Tode zurückgesetzt", "Deaths reset"), self.t("Spielstände", "save states")) } else { self.t("Tode zurückgesetzt", "Deaths reset").to_string() };
        self.msg(m);
    }

    /// Spielzeit/Tode/Herzen eines Slots (zwischengespeichert, neu gelesen bei Änderung).
    pub(crate) fn slot_meta(&mut self, slot: u8) -> Option<SlotMeta> {
        let p = self.meta_path(slot)?;
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        if let Some((t, m)) = self.metas.get(&slot) {
            if *t == mtime {
                return m.clone();
            }
        }
        let mut m = mtime.and_then(|_| std::fs::read_to_string(&p).ok()).and_then(|t| serde_json::from_str::<SlotMeta>(&t).ok());
        // ältere Spielstände: Rubine aus dem Savestate selbst lesen (nur lesen, einmal je Dateistand)
        if let (Some(meta), Some(g), Some(sp)) = (m.as_mut(), self.game.as_ref(), self.state_path(slot)) {
            if meta.rupees.is_none() {
                if let Ok(d) = std::fs::read(&sp) {
                    let mut tmp = g.snes.clone();
                    if tmp.load_state(&d).is_ok() {
                        meta.rupees = Some(alttp::ram16(&tmp, 0x7EF360));
                    }
                }
            }
        }
        self.metas.insert(slot, (mtime, m.clone()));
        m
    }

    /// Zwischenbilder verwerfen (nach Sprüngen: Laden, Teleport, Reset …)
    pub(crate) fn reset_interp(&mut self) {
        self.snaps = [None, None];
        self.new_frame = true;
    }

    pub(crate) fn load_slot(&mut self, slot: u8) {
        if self.gba.active() {
            self.gba_load_slot(slot);
            return;
        }
        let Some(p) = self.state_path(slot) else { return };
        self.slot = slot;
        let (ok, empty) = (self.t("geladen", "loaded"), self.t("ist leer", "is empty"));
        let m = match std::fs::read(&p) {
            Ok(d) => {
                let g = self.game.as_mut().unwrap();
                match g.snes.load_state(&d) {
                    Ok(_) => {
                        g.rewind.clear();
                        g.display.copy_from_slice(g.snes.framebuffer());
                        g.wide.clear();
                        g.last_module = alttp::ram8(&g.snes, 0x7E0010);
                        g.dying = false;
                        self.new_frame = true;
                        format!("Slot {slot} {ok}")
                    }
                    Err(e) => e,
                }
            }
            Err(_) => format!("Slot {slot} {empty}"),
        };
        if let Some(meta) = self.slot_meta(slot) {
            if let Some(g) = &mut self.game {
                g.play_frames = meta.play_frames;
                g.deaths = meta.deaths;
            }
        }
        if let Some(g) = &mut self.game {
            let (imgs, stairs) = load_rooms(&g.path.with_extension(format!("ss{slot}.rooms")));
            g.room_imgs = imgs;
            g.room_vram.clear();
            g.room_pending.clear();
            g.reset_shots();
            g.last_ow = None;
            ppu::clear_seen_cache();
            g.stairs = stairs;
            g.visited = self.metas.get(&slot).and_then(|m| m.1.as_ref()).map(|m| m.visited.iter().copied().collect()).unwrap_or_default();
            g.manual = self.metas.get(&slot).and_then(|m| m.1.as_ref()).map(|m| m.manual.iter().copied().collect()).unwrap_or_default();
            g.pins = self.metas.get(&slot).and_then(|m| m.1.as_ref()).map(|m| m.pins.clone()).unwrap_or_default();
            g.x_item = self.metas.get(&slot).and_then(|m| m.1.as_ref()).and_then(|m| m.x_item).map(|(idx, bottle)| quickmenu::Entry { idx, bottle });
            g.track = self.metas.get(&slot).and_then(|m| m.1.as_ref()).and_then(|m| m.track);
            self.room_tex.clear();
            self.room_hd_tex.clear();
        }
        self.reset_interp();
        self.msg(m);
    }

    pub(crate) fn delete_slot(&mut self, slot: u8) {
        if let (Some(p), Some(tp), Some(mp)) = (self.state_path(slot), self.thumb_path(slot), self.meta_path(slot)) {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(tp);
            let _ = std::fs::remove_file(mp);
            if let Some(g) = &self.game {
                let _ = std::fs::remove_file(g.path.with_extension(format!("ss{slot}.rooms")));
            }
        }
        self.thumbs.remove(&slot);
        self.metas.remove(&slot);
    }

    /// Cheat „Spiel durchgespielt“: alles für 100 % setzen.
    pub(crate) fn complete_100(&mut self) {
        let Some(g) = &mut self.game else { return };
        for i in world::apply_100(&mut g.snes, &g.locations, &g.ents) {
            g.manual.insert(i);
        }
        for e in &g.ents {
            g.visited.insert(e.eid);
        }
        self.msg(self.t("🏆 Alles auf 100 % gesetzt", "🏆 Everything set to 100 %"));
    }

    // ------------------------------------------------------------ Teleport

    /// Wurde dieser Eingang schon betreten? (Raum-Flags im Spielstand: besuchte Viertel, oder in dieser Sitzung)
    pub(crate) fn visited(&self, e: &Entrance) -> bool {
        let Some(g) = &self.game else { return false };
        if g.visited.contains(&e.eid) {
            return true;
        }
        // Räume mit Eingängen in beiden Welten (z. B. Läden): nur der wirklich benutzte Eingang zählt
        if g.ents.iter().any(|o| o.room == e.room && o.world != e.world) {
            return false;
        }
        let r = e.room as usize;
        r < 0x140 && g.snes.bus.wram[0xF000 + r * 2] & 0x0F != 0
    }

    /// Teleport möglich? (im normalen Spiel oder in der Weltkarte des Spiels)
    pub(crate) fn can_teleport(&self) -> bool {
        self.game.as_ref().map_or(false, |g| crate::teleport::can_teleport(&g.snes) || in_game_map(&g.snes))
    }

    /// Teleport mit Animation: erst ausblenden, dann springen, dann einblenden.
    pub(crate) fn teleport(&mut self, ent: Entrance) {
        if self.tp_anim.is_some() || !self.can_teleport() {
            if !self.can_teleport() {
                self.msg(self.t("Teleport gerade nicht möglich (nur im normalen Spiel)", "Teleport not possible right now (only during normal play)"));
            }
            return;
        }
        self.qm.open = false;
        self.win_map = self.win_map && self.s.map_own_window;
        self.tp_anim = Some(TpAnim { t0: Instant::now(), ent, jumped: false, from: self.link_screen_pos() });
    }

    /// Link auf dem Bildschirm (Spielpixel innerhalb von 256×224)
    pub(crate) fn link_screen_pos(&self) -> (f32, f32) {
        self.game.as_ref().map_or((128.0, 112.0), |g| {
            let lx = alttp::ram16(&g.snes, 0x7E0022) as f32 - alttp::ram16(&g.snes, 0x7E00E2) as f32 + 8.0;
            let ly = alttp::ram16(&g.snes, 0x7E0020) as f32 - alttp::ram16(&g.snes, 0x7E00E8) as f32 + 12.0;
            (lx.clamp(0.0, 256.0), ly.clamp(0.0, 224.0))
        })
    }

    /// Animation weiterführen: nach dem Ausblenden springen.
    fn tp_anim_step(&mut self) {
        let Some(a) = self.tp_anim else { return };
        let t = a.t0.elapsed().as_secs_f32();
        if !a.jumped && t >= TP_OUT {
            self.do_teleport(a.ent);
            let to = self.link_screen_pos();
            self.tp_anim = Some(TpAnim { t0: Instant::now(), ent: a.ent, jumped: true, from: to });
        } else if a.jumped && t >= TP_IN {
            self.tp_anim = None;
        }
    }

    fn do_teleport(&mut self, ent: Entrance) {
        let en = self.s.ui_en;
        let eid = ent.eid;
        let name = Some((ent.room, ent.area))
            .map(|(room, area)| {
                let d = self.game.as_ref().and_then(|g| world::room_dungeon(&g.dmaps, room));
                match d {
                    Some(d) => (if en { world::DMAP_NAMES[d as usize].1 } else { world::DMAP_NAMES[d as usize].0 }).to_string(),
                    None => world::area_name(area, en),
                }
            })
            .unwrap_or_default();
        let busy = self.t("Teleport gerade nicht möglich (nur im normalen Spiel)", "Teleport not possible right now (only during normal play)");
        let Some(g) = &mut self.game else { return };
        // Zurückspulen über den Teleport hinweg: den Stand direkt davor merken (der Spul-Speicher bleibt erhalten)
        let before = g.snes.save_state();
        if in_game_map(&g.snes) {
            // Karte des Spiels verlassen: der Ladevorgang baut den Bildschirm komplett neu auf
            g.snes.bus.wram[0x10] = 0x09;
            g.snes.bus.wram[0x11] = 0x00;
        }
        // Ziel ist genau die gewählte Tür (Welt, Gebiet, Position) – nicht die erste mit derselben ID
        let target = crate::teleport::Target { eid, area: ent.area, x: ent.x, y: ent.y };
        let rom = g.orig_rom.clone();
        match crate::teleport::to_target(&mut g.snes, &rom, target, true) {
            Ok(()) => {
                g.visited.insert(eid);
                g.dying = false;
                g.last_module = alttp::ram8(&g.snes, 0x7E0010);
                g.display.copy_from_slice(g.snes.framebuffer());
                g.wide = g.snes.bus.ppu.wide.clone();
                if self.s.rewind {
                    g.rewind.push_back(before);
                }
                self.reset_interp();
                self.acc = 0.0;
                self.last = Instant::now();
                self.msg(format!("✨ Teleport: {name}"));
            }
            Err(_) => self.msg(busy),
        }
    }

    /// Besuchte Eingänge einer Welt für das Teleport-Menü (eindeutige Positionen), sortiert nach Name.
    pub(crate) fn teleport_targets(&self, world: u8) -> Vec<(Entrance, String)> {
        let en = self.s.ui_en;
        let Some(g) = &self.game else { return Vec::new() };
        let mut out: Vec<(Entrance, String)> = Vec::new();
        for e in g.ents.iter().filter(|e| e.world == world) {
            if !self.visited(e) || out.iter().any(|(o, _)| (o.x - e.x).abs() < 1.0 && (o.y - e.y).abs() < 1.0) {
                continue;
            }
            let name = match world::room_dungeon(&g.dmaps, e.room) {
                Some(d) => (if en { world::DMAP_NAMES[d as usize].1 } else { world::DMAP_NAMES[d as usize].0 }).to_string(),
                None => world::area_name(e.area, en),
            };
            out.push((*e, name));
        }
        out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.eid.cmp(&b.0.eid)));
        out
    }

    /// Vorschaubild eines Slots (wird bei Änderung der Datei neu geladen).
    pub(crate) fn thumb(&mut self, ctx: &gfx::Context, slot: u8) -> Option<(gfx::TextureHandle, SystemTime)> {
        let p = self.state_path(slot)?;
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
        if let Some((t, Some(h))) = self.thumbs.get(&slot) {
            if *t == Some(mtime) {
                return Some((h.clone(), mtime));
            }
        }
        let tp = self.thumb_path(slot)?;
        // GBA: Vorschaubild als RGBA (240×160)
        if self.gba.active() {
            let tex = std::fs::read(tp)
                .ok()
                .and_then(|d| lz4_flex::decompress_size_prepended(&d).ok())
                .filter(|raw| raw.len() == crate::gba::W * crate::gba::H * 4)
                .map(|raw| ctx.load_texture(format!("gthumb{slot}"), gfx::ColorImage::from_rgba_unmultiplied([crate::gba::W, crate::gba::H], &raw), gfx::TextureOptions::LINEAR));
            self.thumbs.insert(slot, (Some(mtime), tex.clone()));
            return tex.map(|t| (t, mtime));
        }
        let tex = std::fs::read(tp)
            .ok()
            .and_then(|d| lz4_flex::decompress_size_prepended(&d).ok())
            .filter(|raw| raw.len() == W * H * 2)
            .map(|raw| {
                let pixels = raw.chunks_exact(2).map(|b| self.lut[(u16::from_le_bytes([b[0], b[1]]) & 0x7FFF) as usize]).collect();
                ctx.load_texture(format!("thumb{slot}"), gfx::ColorImage { size: [W, H], pixels }, gfx::TextureOptions::LINEAR)
            });
        self.thumbs.insert(slot, (Some(mtime), tex.clone()));
        tex.map(|t| (t, mtime))
    }

    pub(crate) fn set_slot(&mut self, n: u8) {
        self.slot = n.clamp(1, SLOTS);
        let exists = self.state_path(self.slot).map(|p| p.is_file()).unwrap_or(false);
        let m = format!("Slot {}{}", self.slot, if exists { " ●" } else { "" });
        self.msg(m);
    }

    pub(crate) fn reset(&mut self) {
        let keep_sram = self.s.save_sram;
        if let Some(g) = &mut self.game {
            g.snes.reset();
            if !keep_sram {
                // von vorne: kein alter Spielstand in der Dateiauswahl
                g.snes.bus.sram.iter_mut().for_each(|b| *b = 0);
            }
            g.rewind.clear();
            g.play_frames = 0;
            g.deaths = 0;
            g.visited.clear();
            g.manual.clear();
            g.pins.clear();
            g.x_item = None;
            g.track = None;
            g.room_imgs.clear();
            g.stairs.clear();
            g.reset_shots();
            self.title_frame = 0;
        }
        self.room_tex.clear();
        self.room_hd_tex.clear();
        self.reset_interp();
        self.msg("Reset");
    }

    pub(crate) fn patch_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().set_title(self.t("Patch wählen", "Choose patch")).add_filter("Patch", &["bps", "ips"]).pick_file() {
            self.patch_flow(p);
        }
    }

    pub(crate) fn patch_flow(&mut self, patch: PathBuf) {
        let saved = PathBuf::from(&self.s.base_rom);
        let base = if saved.is_file() { Some(saved) } else { self.pick_rom_path() };
        let Some(base) = base else { return };
        let not_alttp = self.t("Die gewählte Basis-ROM ist keine A-Link-to-the-Past-ROM.", "The chosen base ROM is not A Link to the Past.");
        let res = (|| -> Result<PathBuf, String> {
            let data = std::fs::read(&base).map_err(|e| e.to_string())?;
            let (rom, info) = alttp::prepare_rom(data)?;
            if !info.is_alttp {
                return Err(not_alttp.into());
            }
            let p = std::fs::read(&patch).map_err(|e| e.to_string())?;
            let is_bps = patch.extension().map(|e| e.eq_ignore_ascii_case("bps")).unwrap_or(false);
            let out = if is_bps { alttp::apply_bps(&rom, &p)? } else { alttp::apply_ips(&rom, &p)? };
            let out_path = patch.with_extension("sfc");
            std::fs::write(&out_path, &out).map_err(|e| e.to_string())?;
            Ok(out_path)
        })();
        match res {
            Ok(p) => {
                self.s.base_rom = base.to_string_lossy().into_owned();
                self.load_rom(p, true);
            }
            Err(e) => {
                self.s.base_rom.clear();
                self.msg(e);
            }
        }
    }

    // ------------------------------------------------------------ Randomizer / Codes

    fn code_sig(&self) -> String {
        self.s
            .codes
            .iter()
            .filter(|c| c.enabled && matches!(alttp::decode_code(&c.code), Ok(alttp::Code::Rom { .. })))
            .map(|c| c.code.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub(crate) fn rebuild_rom(&mut self) {
        if let Some(g) = &mut self.game {
            g.snes.bus.rom = Arc::new(build_rom(&g.orig_rom, g.rando.as_ref(), &self.s.codes, (self.s.more_bombs, self.s.sword_pots, self.s.mirror_direct, self.s.quick_dash, self.s.health_blink, self.s.wide_sprites)));
        }
    }

    pub(crate) fn reapply_rom_codes(&mut self) {
        let sig = self.code_sig();
        if sig != self.rom_code_sig {
            self.rom_code_sig = sig;
            self.rebuild_rom();
        }
    }

    pub(crate) fn rando_active(&self) -> bool {
        self.game.as_ref().map_or(false, |g| g.rando.is_some())
    }

    /// Schaltet den Randomizer an (mit dem eingegebenen bzw. einem neuen Seed) oder aus.
    pub(crate) fn set_rando(&mut self, on: bool) {
        if on {
            self.apply_rando();
        } else {
            self.remove_rando();
        }
    }

    pub(crate) fn apply_rando(&mut self) {
        let mode = mode_from(self.s.rando_mode);
        if self.s.rando_seed.trim().is_empty() {
            self.s.rando_seed = time_seed().to_string();
        }
        let seed = randomizer::seed_from_text(&self.s.rando_seed);
        let reset = self.s.rando_reset;
        let (none, nogame) = (
            self.t("Truhentabelle in dieser ROM nicht gefunden – Randomizer nicht möglich.", "Chest table not found in this ROM – randomizer unavailable."),
            self.t("Zuerst eine ROM laden.", "Load a ROM first."),
        );
        let Some(g) = &mut self.game else {
            self.msg(nogame);
            return;
        };
        let Some(t) = &g.chests else {
            self.msg(none);
            return;
        };
        let r = randomizer::randomize(t, seed, mode);
        let changed = r.spoiler.iter().filter(|(_, _, a, b)| a != b).count();
        g.rando = Some(r);
        self.s.rando_on = true;
        if reset {
            g.snes.reset();
            g.rewind.clear();
            self.title_frame = 0;
        }
        self.rebuild_rom();
        let m = format!("Randomizer AN · Seed {seed} · {changed} {}", self.t("Truhen geändert", "chests changed"));
        self.msg(m);
    }

    pub(crate) fn remove_rando(&mut self) {
        let reset = self.s.rando_reset;
        self.s.rando_on = false;
        if let Some(g) = &mut self.game {
            g.rando = None;
            if reset {
                g.snes.reset();
                g.rewind.clear();
                self.title_frame = 0;
            }
        }
        self.rebuild_rom();
        self.msg(self.t("Randomizer AUS – Original-Truheninhalte", "Randomizer OFF – original chest contents"));
    }

    pub(crate) fn new_rando_seed(&mut self) {
        self.s.rando_seed = time_seed().to_string();
    }

    // ------------------------------------------------------------ Diverses

    pub(crate) fn screenshot(&mut self) {
        let Some(g) = &self.game else { return };
        let dir = g.path.parent().map(|d| d.join("screenshots")).unwrap_or_else(|| PathBuf::from("screenshots"));
        let _ = std::fs::create_dir_all(&dir);
        let name = g.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let p = dir.join(format!("{name}_{ts}.bmp"));
        let m = match write_bmp(&p, self.view_w as u32, H as u32, &self.cur) {
            Ok(_) => format!("Screenshot: {}", p.display()),
            Err(e) => format!("Screenshot: {e}"),
        };
        self.msg(m);
    }

    pub(crate) fn toggle_fullscreen(&mut self, ctx: &gfx::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(gfx::ViewportCommand::Fullscreen(self.fullscreen));
    }

    pub(crate) fn toggle_hd(&mut self) {
        if self.s.filter.is_hd() {
            self.s.filter = Filter::Pixel;
        } else {
            self.s.filter = if self.s.last_hd.is_hd() { self.s.last_hd.menu_mode() } else { Filter::HdSoft };
        }
        let m = format!("{}: {}", self.t("Bild", "Video"), self.s.filter.name(self.s.ui_en));
        self.msg(m);
    }

    fn onoff(&self, b: bool) -> &'static str {
        if b {
            self.t("an", "on")
        } else {
            self.t("aus", "off")
        }
    }

    pub(crate) fn do_act(&mut self, a: Act, ctx: &gfx::Context) {
        match a {
            Act::OpenRom => self.open_dialog(),
            Act::Reset => self.reset(),
            Act::Quit => ctx.send_viewport_cmd(gfx::ViewportCommand::Close),
            Act::SaveState => self.save_slot(self.slot),
            Act::LoadState => self.load_slot(self.slot),
            Act::SlotPrev => self.set_slot(if self.slot <= 1 { SLOTS } else { self.slot - 1 }),
            Act::SlotNext => self.set_slot(if self.slot >= SLOTS { 1 } else { self.slot + 1 }),
            Act::Slot1 => self.set_slot(1),
            Act::Slot2 => self.set_slot(2),
            Act::Slot3 => self.set_slot(3),
            Act::Slot4 => self.set_slot(4),
            Act::Slot5 => self.set_slot(5),
            Act::Slot6 => self.set_slot(6),
            Act::Slot7 => self.set_slot(7),
            Act::Slot8 => self.set_slot(8),
            Act::Slot9 => self.set_slot(9),
            Act::Screenshot => self.screenshot(),
            Act::Pause => {
                self.paused = !self.paused;
                self.msg(if self.paused { self.t("Pause", "Paused") } else { self.t("Weiter", "Resumed") });
            }
            Act::FrameAdvance => {
                if self.paused {
                    self.advance = true;
                } else {
                    self.paused = true;
                }
            }
            Act::Turbo | Act::Rewind | Act::Double | Act::QuickMenu => {}
            Act::SpeedUp => {
                self.s.speed = (self.s.speed + 0.25).min(4.0);
                self.msg(format!("{} {:.0} %", self.t("Tempo", "Speed"), self.s.speed * 100.0));
            }
            Act::SpeedDown => {
                self.s.speed = (self.s.speed - 0.25).max(0.25);
                self.msg(format!("{} {:.0} %", self.t("Tempo", "Speed"), self.s.speed * 100.0));
            }
            Act::SpeedReset => {
                self.s.speed = 1.0;
                self.msg(format!("{} 100 %", self.t("Tempo", "Speed")));
            }
            Act::Runahead => {
                self.s.runahead = (self.s.runahead + 1) % 4;
                self.msg(format!("Run-Ahead: {}", self.s.runahead));
            }
            Act::WinStates => {
                // Spielstände haben Vorrang: Karte/Inventar schließen
                self.win_states = !self.win_states;
                if self.win_states {
                    self.inv_open = false;
                    if !self.s.map_own_window {
                        self.win_map = false;
                    }
                }
            }
            Act::WinMap if self.gba.active() => self.gba_map_toggle(),
            Act::WinMap => {
                if self.win_map {
                    self.win_map = false;
                } else {
                    self.open_map_overlay();
                }
            }
            Act::WinRando => self.win_rando = !self.win_rando,
            Act::WinCheats => self.win_cheats = !self.win_cheats,
            Act::WinControls => self.win_controls = !self.win_controls,
            Act::WinHotkeys => self.win_hotkeys = !self.win_hotkeys,
            Act::Fullscreen => self.toggle_fullscreen(ctx),
            Act::ToggleMenu => self.show_bar = !self.show_bar,
            Act::ToggleHd => self.toggle_hd(),
            Act::NextFilter => {
                let i = Filter::ALL.iter().position(|f| *f == self.s.filter).unwrap_or(0);
                self.s.filter = Filter::ALL[(i + 1) % Filter::ALL.len()];
                if self.s.filter.is_hd() {
                    self.s.last_hd = self.s.filter;
                }
                self.msg(self.s.filter.name(self.s.ui_en));
            }
            Act::ToggleFps => self.s.show_fps = !self.s.show_fps,
            Act::FrameBlend => {
                self.s.frame_blend = !self.s.frame_blend;
                self.msg(format!("{}: {}", self.t("Frame-Interpolation", "Frame interpolation"), self.onoff(self.s.frame_blend)));
            }
            Act::ColorRando => {
                self.s.color_rando = !self.s.color_rando;
                self.msg(format!("{}: {}", self.t("Farb-Randomizer", "Color randomizer"), self.onoff(self.s.color_rando)));
            }
            Act::Mute => {
                self.s.mute = !self.s.mute;
                self.msg(format!("{}: {}", self.t("Stumm", "Mute"), self.onoff(self.s.mute)));
            }
            Act::VolUp => {
                self.s.volume = (self.s.volume + 0.1).min(1.5);
                self.msg(format!("{} {:.0} %", self.t("Lautstärke", "Volume"), self.s.volume * 100.0));
            }
            Act::VolDown => {
                self.s.volume = (self.s.volume - 0.1).max(0.0);
                self.msg(format!("{} {:.0} %", self.t("Lautstärke", "Volume"), self.s.volume * 100.0));
            }
            Act::Echo => {
                self.s.echo = !self.s.echo;
                self.msg(format!("Echo: {}", self.onoff(self.s.echo)));
            }
            Act::Noclip => {
                self.s.cheats.noclip = !self.s.cheats.noclip;
                self.msg(format!("Noclip: {}", self.onoff(self.s.cheats.noclip)));
            }
            Act::InfHealth => {
                self.s.cheats.inf_health = !self.s.cheats.inf_health;
                self.msg(format!("{}: {}", self.t("Unendlich Herzen", "Infinite health"), self.onoff(self.s.cheats.inf_health)));
            }
            Act::InfMagic => {
                self.s.cheats.inf_magic = !self.s.cheats.inf_magic;
                self.msg(format!("{}: {}", self.t("Unendlich Magie", "Infinite magic"), self.onoff(self.s.cheats.inf_magic)));
            }
            Act::AllItems => {
                if let Some(g) = &mut self.game {
                    alttp::give_all_items(&mut g.snes);
                    self.msg(self.t("Alle Items erhalten", "All items given"));
                }
            }
            Act::RandoToggle => {
                let on = !self.rando_active();
                if on {
                    self.new_rando_seed();
                }
                self.set_rando(on);
            }
            Act::RandoNew => {
                self.new_rando_seed();
                self.apply_rando();
            }
        }
    }

    // ------------------------------------------------------------ Eingabe

    fn hotkeys(&mut self, ctx: &gfx::Context) {
        if self.rebind.is_some() || self.rebind_hotkey.is_some() {
            return;
        }
        let evs: Vec<(Key, gfx::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        if evs.is_empty() || ctx.wants_keyboard_input() {
            return;
        }
        for (k, m) in evs {
            let combo = Combo::from_event(k, m);
            let game_key = self.s.keys.iter().any(|n| n == k.name());
            let hits: Vec<Act> = ACTS
                .iter()
                .filter(|info| !info.hold)
                .filter(|info| self.s.hotkey(info.id).map_or(false, |c| c == combo && !(game_key && !c.has_mods())))
                .map(|info| info.act)
                .collect();
            for a in hits {
                self.do_act(a, ctx);
            }
        }
    }

    fn handle_rebind(&mut self, ctx: &gfx::Context) {
        if let Some(idx) = self.rebind {
            let k = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    Event::Key { key, pressed: true, .. } => Some(*key),
                    _ => None,
                })
            });
            if let Some(k) = k {
                if k != Key::Escape {
                    self.s.keys[idx] = k.name().to_string();
                }
                self.rebind = None;
            }
        }
        if let Some(id) = self.rebind_hotkey {
            let ev = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((k, m)) = ev {
                if !(k == Key::Escape && !m.any()) {
                    self.s.hotkeys.insert(id.to_string(), Combo::from_event(k, m).to_text());
                }
                self.rebind_hotkey = None;
            }
        }
    }

    /// (Joypad, Turbo, ×2, Zurückspulen)
    fn gather_input(&mut self, ctx: &gfx::Context) -> (u16, bool, bool, bool) {
        let mut pad = 0u16;
        let (mut turbo, mut double, mut rewind) = (false, false, false);
        let mut qm_key = false;
        if self.rebind.is_none() && self.rebind_hotkey.is_none() && !ctx.wants_keyboard_input() {
            let keys: Vec<Option<Key>> = (0..BUTTONS.len()).map(|i| self.s.key(i)).collect();
            let (tc, dc, rc, qc) = (self.s.hotkey("turbo"), self.s.hotkey("double"), self.s.hotkey("rewind"), self.s.hotkey("quickmenu"));
            ctx.input(|i| {
                for (idx, k) in keys.iter().enumerate() {
                    if let Some(k) = k {
                        if i.key_down(*k) || (*k == Key::Tab && self.tab_down) {
                            pad |= 1 << (15 - idx);
                        }
                    }
                }
                let tab = self.tab_down;
                let held = |c: Option<Combo>| c.map_or(false, |c| (i.key_down(c.key) || (c.key == Key::Tab && tab)) && (!c.ctrl || i.modifiers.ctrl) && (!c.shift || i.modifiers.shift) && (!c.alt || i.modifiers.alt));
                turbo = held(tc);
                double = held(dc);
                rewind = held(rc);
                qm_key = held(qc);
            });
        }
        let dq = self.s.dpad_quickmenu;
        let mut l3 = false;
        let mut minus = false;
        // Steuerkreuz am Controller: bewegt Link nicht, ↑ = Spiegel (Welt wechseln) (Menüs bedient es weiter)
        let dfree = self.s.dpad_no_move;
        let mut ctl_dpad = 0u16;
        let mut qm_pad = false;
        let mut dir = gfx::Vec2::ZERO;
        let (mut lst, mut rst) = (gfx::Vec2::ZERO, gfx::Vec2::ZERO);
        let mut lsp = 0u16;
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            use gilrs::{Axis, Button as Gb};
            let face: [(Gb, usize); 4] = if self.s.pad_label_layout {
                [(Gb::South, 8), (Gb::East, 0), (Gb::West, 9), (Gb::North, 1)]
            } else {
                [(Gb::South, 0), (Gb::East, 8), (Gb::West, 1), (Gb::North, 9)]
            };
            let rest: [(Gb, usize); 8] = [
                (Gb::Select, 2),
                (Gb::Start, 3),
                (Gb::DPadUp, 4),
                (Gb::DPadDown, 5),
                (Gb::DPadLeft, 6),
                (Gb::DPadRight, 7),
                (Gb::LeftTrigger, 10),
                (Gb::RightTrigger, 11),
            ];
            let dz = self.s.deadzone;
            let mut mode = false;
            for (_id, gp) in g.gamepads() {
                if !gp.is_connected() {
                    continue;
                }
                for (b, idx) in face.iter().chain(rest.iter()) {
                    if self.s.minus_map && *b == Gb::Select {
                        minus |= gp.is_pressed(*b);
                        continue;
                    }
                    if dq && *b == Gb::DPadRight {
                        qm_pad |= gp.is_pressed(*b);
                        continue;
                    }
                    if gp.is_pressed(*b) {
                        if dfree && (4..=7).contains(idx) {
                            ctl_dpad |= 1 << (15 - idx);
                        } else {
                            pad |= 1 << (15 - idx);
                        }
                    }
                }
                let (rx, ry) = (gp.value(Axis::RightStickX), gp.value(Axis::RightStickY));
                let (lx0, ly0) = (gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY));
                if lx0 * lx0 + ly0 * ly0 > lst.length_sq() {
                    lst = gfx::vec2(lx0, ly0);
                }
                if rx * rx + ry * ry > rst.length_sq() {
                    rst = gfx::vec2(rx, ry);
                }
                if rx * rx + ry * ry > 0.25 {
                    dir = gfx::vec2(rx, -ry);
                } else {
                    let (lx, ly) = (gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY));
                    if lx * lx + ly * ly > 0.25 && dir == gfx::Vec2::ZERO {
                        dir = gfx::vec2(lx, -ly);
                    }
                }
                let sd = stick_dirs(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY), dz);
                pad |= sd;
                lsp |= sd;
                turbo |= gp.is_pressed(Gb::RightTrigger2);
                // Trigger erst deutlich durchgedrückt (analog): ein leicht berührter Trigger spult nicht
                rewind |= gp.button_data(Gb::LeftTrigger2).map_or(false, |d| d.value() > 0.6);
                double |= gp.is_pressed(Gb::RightThumb);
                l3 |= gp.is_pressed(Gb::LeftThumb);
                mode |= gp.is_pressed(Gb::Mode);
            }
            if mode && !self.mode_prev {
                self.win_states = !self.win_states;
            }
            self.mode_prev = mode;
        }
        // Nintendo Switch Pro Controller (eigener USB/Bluetooth-Treiber)
        let ps = self.pro.get();
        if ps.connected != self.pro_connected {
            self.pro_connected = ps.connected;
            let m = if ps.connected {
                format!("🎮 {} {}", ps.name, self.t("verbunden", "connected"))
            } else {
                self.t("🎮 Controller getrennt", "🎮 Controller disconnected").to_string()
            };
            self.msg(m);
        }
        if ps.connected {
            // Nintendo-Belegung entspricht der SNES-Belegung (A rechts, B unten, X oben, Y links)
            let map: [(u32, usize); 12] = [
                (pro::B, 0),
                (pro::Y, 1),
                (pro::MINUS, 2),
                (pro::PLUS, 3),
                (pro::UP, 4),
                (pro::DOWN, 5),
                (pro::LEFT, 6),
                (pro::RIGHT, 7),
                (pro::A, 8),
                (pro::X, 9),
                (pro::L, 10),
                (pro::R, 11),
            ];
            for (bit, idx) in map {
                if self.s.minus_map && bit == pro::MINUS {
                    minus |= ps.buttons & bit != 0;
                    continue;
                }
                if dq && bit == pro::RIGHT {
                    qm_pad |= ps.buttons & bit != 0;
                    continue;
                }
                if ps.buttons & bit != 0 {
                    if dfree && (4..=7).contains(&idx) {
                        ctl_dpad |= 1 << (15 - idx);
                    } else {
                        pad |= 1 << (15 - idx);
                    }
                }
            }
            if ps.lx * ps.lx + ps.ly * ps.ly > lst.length_sq() {
                lst = gfx::vec2(ps.lx, ps.ly);
            }
            if ps.rx * ps.rx + ps.ry * ps.ry > rst.length_sq() {
                rst = gfx::vec2(ps.rx, ps.ry);
            }
            if ps.rx * ps.rx + ps.ry * ps.ry > 0.25 {
                dir = gfx::vec2(ps.rx, -ps.ry);
            } else if ps.lx * ps.lx + ps.ly * ps.ly > 0.25 && dir == gfx::Vec2::ZERO {
                dir = gfx::vec2(ps.lx, -ps.ly);
            }
            let sd = stick_dirs(ps.lx, ps.ly, self.s.deadzone);
            pad |= sd;
            lsp |= sd;
            turbo |= ps.buttons & pro::ZR != 0;
            rewind |= ps.buttons & pro::ZL != 0;
            double |= ps.buttons & pro::RSTICK != 0;
            l3 |= ps.buttons & pro::LSTICK != 0;
            let pressed = ps.buttons & !self.pro_prev;
            if pressed & pro::CAPTURE != 0 {
                self.screenshot();
            }
            if pressed & pro::HOME != 0 {
                self.win_states = !self.win_states;
            }
        }
        self.pro_prev = ps.buttons;
        let left = self.s.x_slot && ctl_dpad & (1 << 9) != 0;
        if !self.qm.open && (qm_key || qm_pad || left) && !self.qm.held_prev {
            self.qm_for_x = left && !(qm_key || qm_pad);
        }
        self.qm_held = qm_key || qm_pad || left;
        self.qm_pad = qm_pad;
        self.qm_dir = dir;
        let dzf = |v: gfx::Vec2| { let dz = self.s.deadzone.max(0.25); let l = v.length(); if l < dz { gfx::Vec2::ZERO } else { v / l * ((l - dz) / (1.0 - dz)).min(1.0) } };
        self.lstick = dzf(lst);
        self.lstick_pad = lsp;
        self.rstick = dzf(rst);
        if self.s.block_opposite {
            if pad & 0x0C00 == 0x0C00 {
                pad &= !0x0C00;
            }
            if pad & 0x0300 == 0x0300 {
                pad &= !0x0300;
            }
        }
        self.ctl_dpad = ctl_dpad;
        self.minus_now = minus;
        if l3 && !self.l3_prev && (self.game.is_some() || self.gba.active()) {
            self.paused = !self.paused;
            self.msg(if self.paused { self.t("Pause", "Paused") } else { self.t("Weiter", "Resumed") });
        }
        self.l3_prev = l3;
        (pad, turbo, double, rewind)
    }

    // ------------------------------------------------------------ Emulation

    /// Joypad-Bits: B=15 Y=14 Select=13 Start=12 Hoch=11 Runter=10 Links=9 Rechts=8 A=7 X=6
    pub(crate) fn states_nav(&mut self, pressed: u16) {
        if pressed == 0 {
            return;
        }
        // Knopfreihe unter der unteren Reihe (Hauptmenü, Einstellungen): ↑ zurück, ←/→ wechseln,
        // A/Start = auswählen, B = schließen
        if self.states_menu {
            if pressed & (1 << 11) != 0 {
                self.states_menu = false;
            } else if pressed & ((1 << 9) | (1 << 8)) != 0 {
                self.states_menu_sel ^= 1;
            } else if pressed & ((1 << 7) | (1 << 12)) != 0 {
                if self.states_menu_sel == 1 {
                    self.win_pad_settings = true;
                } else {
                    self.states_menu = false;
                    self.go_main_menu();
                }
            } else if pressed & (1 << 15) != 0 {
                self.states_menu = false;
                self.win_states = false;
            }
            return;
        }
        let s = self.slot;
        if pressed & (1 << 10) != 0 && s + 3 > SLOTS {
            self.states_menu = true;
            self.delete_confirm = None;
            return;
        }
        if pressed & (1 << 11) != 0 && s > 3 {
            self.slot -= 3;
        }
        if pressed & (1 << 10) != 0 && s + 3 <= SLOTS {
            self.slot += 3;
        }
        if pressed & (1 << 9) != 0 && (s - 1) % 3 != 0 {
            self.slot -= 1;
        }
        if pressed & (1 << 8) != 0 && s % 3 != 0 {
            self.slot += 1;
        }
        if self.slot != s {
            self.delete_confirm = None;
            self.main_confirm = false;
        }
        if pressed & (1 << 13) != 0 {
            if self.main_confirm {
                self.main_confirm = false;
                self.go_main_menu();
            } else {
                self.main_confirm = true;
                self.msg(self.t("Nochmal SELECT = zum Hauptmenü (nicht gespeicherter Fortschritt geht verloren)", "Press SELECT again = main menu (unsaved progress is lost)"));
            }
            return;
        }
        let exists = self.state_path(self.slot).map(|p| p.is_file()).unwrap_or(false);
        if pressed & ((1 << 7) | (1 << 12)) != 0 && exists {
            self.load_slot(self.slot);
            self.win_states = false;
        } else if pressed & (1 << 14) != 0 {
            self.save_slot(self.slot);
        } else if pressed & (1 << 6) != 0 && exists {
            if self.delete_confirm == Some(self.slot) {
                self.delete_slot(self.slot);
                self.delete_confirm = None;
            } else {
                self.delete_confirm = Some(self.slot);
                self.msg(self.t("Nochmal X = löschen", "Press X again to delete"));
            }
        } else if pressed & (1 << 15) != 0 {
            self.win_states = false;
        }
    }

    fn quickmenu_logic(&mut self, pad: u16) {
        let held = self.qm_held;
        let pressed = pad & !self.qm.prev_pad;
        self.qm.prev_pad = pad;
        let Some(g) = self.game.as_ref() else {
            self.qm.held_prev = held;
            return;
        };
        if held && !self.qm.held_prev && !self.qm.open {
            if quickmenu::can_use(&g.snes) {
                let entries = quickmenu::entries(&g.snes);
                if entries.is_empty() {
                    self.msg(self.t("Noch keine Items", "No items yet"));
                } else {
                    let cur = if self.qm_for_x { g.x_item.unwrap_or(quickmenu::current(&g.snes)) } else { quickmenu::current(&g.snes) };
                    self.qm.sel = Some(entries.iter().position(|e| *e == cur).unwrap_or(0));
                    self.qm.scroll = self.qm.sel.unwrap_or(0) as f32;
                    self.qm.nav_dir = 0;
                    self.qm.entries = entries;
                    self.qm.open = true;
                    self.qm.sticky = false;
                    self.qm.moved = false;
                    self.qm.t0 = Instant::now();
                }
            }
        } else if self.qm.open {
            let n = self.qm.entries.len().max(1);
            // Richtung links/rechts: Sticks, Pfeile/Steuerkreuz, L/R
            let mut dx = 0i8;
            let pad = if self.qm_for_x && self.qm_held { pad & !(1 << 9) } else { pad };
            if self.qm_dir.x < -0.5 || pad & (1 << 9) != 0 || pad & (1 << 5) != 0 {
                dx = -1;
            }
            if self.qm_dir.x > 0.5 || pad & (1 << 8) != 0 || pad & (1 << 4) != 0 {
                dx = 1;
            }
            let now = Instant::now();
            if dx != 0 && (dx != self.qm.nav_dir || now >= self.qm.nav_next) {
                let first = dx != self.qm.nav_dir;
                let cur = self.qm.sel.unwrap_or(0) as i32;
                self.qm.sel = Some((cur + dx as i32).rem_euclid(n as i32) as usize);
                self.qm.moved = true;
                self.qm.nav_next = now + std::time::Duration::from_millis(if first { 300 } else { 110 });
            }
            self.qm.nav_dir = dx;
            let mut confirm = false;
            let mut cancel = false;
            if !self.qm.sticky {
                if !held && self.qm.held_prev {
                    if self.qm.t0.elapsed().as_secs_f32() < 0.3 && !self.qm.moved {
                        self.qm.sticky = true;
                    } else {
                        confirm = true;
                    }
                }
            } else {
                if pressed & ((1 << 7) | (1 << 12)) != 0 || (held && !self.qm.held_prev) {
                    confirm = true;
                }
                if pressed & (1 << 15) != 0 {
                    cancel = true;
                }
            }
            if confirm {
                self.qm_confirm();
            } else if cancel {
                self.qm.open = false;
            }
        }
        self.qm.held_prev = held;
    }

    /// Ausgewähltes Item ausrüsten und Schnellmenü schließen.
    pub(crate) fn qm_confirm(&mut self) {
        self.qm.open = false;
        let en = self.s.ui_en;
        let Some(i) = self.qm.sel else { return };
        let Some(e) = self.qm.entries.get(i).copied() else { return };
        let Some(g) = self.game.as_mut() else { return };
        if self.qm_for_x {
            g.x_item = Some(e);
            let name = quickmenu::name(&g.snes, e, en);
            self.msg(format!("X: {name}"));
            return;
        }
        if quickmenu::current(&g.snes) == e {
            return;
        }
        let ok = quickmenu::equip(&mut g.snes, e);
        if ok && e.idx == 20 && self.s.mirror_direct {
            self.mirror_press = 6;
        }
        g.display.copy_from_slice(g.snes.framebuffer());
        let name = quickmenu::name(&g.snes, e, en);
        self.reset_interp();
        self.acc = 0.0;
        self.last = Instant::now();
        if ok {
            self.msg(format!("Y: {name}"));
        } else {
            self.msg(self.t("Item kann gerade nicht gewechselt werden", "Item cannot be changed right now"));
        }
    }

    /// Breitbild-Rand aus der Einstellung bzw. der Fenstergröße bestimmen.
    fn update_ext(&mut self) {
        let par = if self.s.aspect_43 { (4.0 / 3.0) / (256.0 / 224.0) } else { 1.0 };
        let target: f32 = match self.s.widescreen {
            1 => 16.0 / 9.0,
            2 => 21.0 / 9.0,
            3 => self.area_aspect.max(4.0 / 3.0),
            _ => 0.0,
        };
        let ext = if target <= 0.0 {
            0
        } else {
            let w = target * 224.0 / par;
            (((w - 256.0) / 2.0).max(0.0) as usize & !1).min(MAX_EXT)
        };
        if ext != self.ext {
            self.ext = ext;
            if let Some(g) = &mut self.game {
                g.snes.bus.ppu.ext = ext;
                g.wide.clear();
            }
            self.new_frame = true;
        }
    }

    /// Weltkarte im Spiel offen: L/R wählt einen besuchten Ort, Y teleportiert dorthin.
    /// Diese Tasten gehen dann nicht ans Spiel.
    fn in_game_map_logic(&mut self, pad: u16) -> u16 {
        let open = self.s.map_teleport && self.game.as_ref().map_or(false, |g| in_game_map(&g.snes));
        if !open {
            self.igm_prev = pad;
            return pad;
        }
        let world = self.game.as_ref().map_or(0, |g| (alttp::ram8(&g.snes, 0x7E008A) >= 0x40) as u8);
        let n = self.teleport_targets(world).len();
        let pressed = pad & !self.igm_prev;
        self.igm_prev = pad;
        const L: u16 = 1 << 5;
        const R: u16 = 1 << 4;
        const Y: u16 = 1 << 14;
        const A: u16 = 1 << 7;
        if n > 0 {
            if pressed & R != 0 {
                self.igm_sel = (self.igm_sel + 1) % n;
            }
            if pressed & L != 0 {
                self.igm_sel = (self.igm_sel + n - 1) % n;
            }
            self.igm_sel = self.igm_sel.min(n - 1);
            if pressed & (Y | A) != 0 {
                let ent = self.teleport_targets(world)[self.igm_sel].0;
                self.teleport(ent);
            }
        }
        pad & !(L | R | Y | A)
    }

    pub(crate) fn popup_open(&self) -> bool {
        self.win_states || self.win_rando || self.win_cheats || self.win_controls || self.win_hotkeys || self.win_mods || self.gba.win_cheats
    }

    fn run_emulation(&mut self, pad: u16, turbo: bool, double: bool, rewind: bool) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().min(0.25);
        self.last = now;
        self.turbo_on = turbo;
        self.double_on = double && !turbo;
        self.rewind_on = rewind && self.s.rewind;
        if let Some(a) = &self.audio {
            a.set_params(self.s.volume, self.s.latency_ms);
        }
        let fps = self.frame_rate();
        let menu_pause = (self.s.pause_in_menus && self.popup_open()) || self.win_states || self.qm.open || self.inv_open || (self.win_map && !self.s.map_own_window) || self.auto_paused || self.tp_anim.is_some();
        self.disp_dt = self.disp_dt * 0.95 + dt.clamp(0.002, 0.1) * 0.05;
        // ab ~90 Hz darf das HD-Bild im Hintergrund entstehen (ein Anzeige-Bild später), sonst reicht die Zeit nicht
        self.gpu.set_pipelined(self.disp_dt < 1.0 / 90.0);
        let display_hz = 1.0 / self.disp_dt;
        let lock = self.s.vsync_lock && !turbo && !double && (self.s.speed - 1.0).abs() < 0.01 && self.mods.speed() == 1.0 && ((display_hz - fps) / fps).abs() < 0.04;
        // Meldungen der Mods
        for m in self.mods.take_logs() {
            self.msg(m);
        }
        let Some(g) = self.game.as_mut() else { return };
        // Recompiled-Modus an/aus bzw. nach ROM-Wechsel neu prüfen
        // RC-Modus: Zelda 3 (wenn eingebaut), sonst der eigene Recompiler
        let z3 = crate::z3::available();
        crate::recomp::sync(&mut g.snes, self.s.recompiled && !z3);
        if let Err(m) = crate::z3::sync(&mut g.snes, self.s.recompiled && z3) {
            self.s.recompiled = true;
            self.status = Some((m.to_string(), Instant::now()));
        }
        g.sram_timer += dt;
        g.map_timer += dt;
        // Spielzeit: echte Zeit, solange wirklich gespielt wird (nicht pausiert, nicht im App-Menü, nicht beim
        // Zurückspulen). Unabhängig von Turbo, schnellen Szenenwechseln oder Rucklern – die verfälschten das
        // Zählen pro emuliertem Bild.
        if !(self.paused || menu_pause || self.rewind_on) {
            let m = alttp::ram8(&g.snes, 0x7E0010);
            if m >= 0x06 && m != 0x14 {
                g.play_acc += dt.clamp(0.0, 0.25) * fps;
                let whole = g.play_acc.floor();
                g.play_frames += whole as u64;
                g.play_acc -= whole;
            }
        }
        let mut frames = 0u32;
        if self.paused || menu_pause {
            if self.advance {
                frames = 1;
                self.advance = false;
            }
            self.acc = 0.0;
        } else {
            let mult = if turbo {
                self.s.turbo_speed as f64
            } else if double {
                2.0 * self.s.speed as f64
            } else {
                // Mods im Recompiled-Modus können das Tempo ändern (z. B. Zeitlupe)
                self.s.speed as f64 * self.mods.speed() as f64
            };
            if lock {
                // Monitor ~ Spielrate: genau ein Bild pro Bildschirm-Aktualisierung -> kein Ruckeln
                frames = 1;
                self.acc = 0.0;
            } else {
                self.acc += dt * fps * mult;
                frames = self.acc.floor() as u32;
                self.acc -= frames as f64;
                frames = frames.min(16);
            }
        }
        if frames == 0 {
            return;
        }
        g.snes.bus.apu.dsp.cfg = DspCfg { interp: self.s.interp, no_echo: !self.s.echo, mute_mask: self.s.voice_mute };
        // Schatten-Überlagerungen (Blätterdach, Wolkenschatten) ausblenden
        let mask = self.s.layer_mask & if shadow_overlay(&g.snes) { !1 } else { 0xFF };
        g.snes.bus.ppu.layer_mask = mask;
        self.eff_mask = mask;
        g.snes.bus.ppu.ext = self.ext;
        g.snes.bus.ppu.record = self.s.hi_fps;
        self.interp_ok = self.s.hi_fps && !lock;

        if self.prev_noclip && !self.s.cheats.noclip {
            alttp::clear_noclip(&mut g.snes);
        }
        self.prev_noclip = self.s.cheats.noclip;

        if rewind && self.s.rewind {
            for _ in 0..frames {
                if g.rewind.len() > 1 {
                    g.rewind.pop_back();
                }
            }
            if let Some(st) = g.rewind.back().cloned() {
                // Breitbild auch beim Zurückspulen: die nicht gespeicherten Rand-Angaben übernehmen bzw. neu berechnen
                let p = &g.snes.bus.ppu;
                let keep = (p.room_keep.clone(), p.room_next.clone(), p.room_side.clone(), p.ow_edge, p.win_stretch);
                let _ = g.snes.load_state(&st);
                let p = &mut g.snes.bus.ppu;
                (p.room_keep, p.room_next, p.room_side, p.ow_edge, p.win_stretch) = keep;
            }
            if self.ext > 0 {
                let full = alttp::ram8(&g.snes, 0x7E001B) != 0 && in_dungeon(&g.dmaps, shown_room(&g.snes));
                let fresh = g.m16tab.as_ref().and_then(|t| ow_map_full(&g.snes, t, full));
                // wie beim Spielen: gilt die Gebietskarte gerade als ungültig, die letzte gültige weiterverwenden –
                // sonst bleiben die Ränder beim Zurückspulen schwarz
                let stable = stable_ow(fresh, &g.last_ow);
                if let Some(o) = &stable {
                    if o.valid && !o.room {
                        g.last_ow = Some(o.clone());
                    }
                } else {
                    g.last_ow = None;
                }
                g.snes.bus.ppu.ow = stable;
            }
            g.snes.bus.joy[0] = 0;
            g.snes.run_frame();
            // zurückgespult: ein Tod, der dadurch ungeschehen ist, zählt nicht
            g.last_module = alttp::ram8(&g.snes, 0x7E0010);
            g.dying = g.last_module == 0x12;
            self.abuf.clear();
            g.snes.take_audio(&mut self.abuf);
            g.display.copy_from_slice(g.snes.framebuffer());
            g.wide.clone_from(&g.snes.bus.ppu.wide);
            self.snaps = [None, None];
            self.new_frame = true;
            return;
        }

        let audio_ok = !self.s.mute && !(turbo && self.s.mute_turbo);
        let max_rewind = ((self.s.rewind_seconds as f64 * fps / 2.0) as usize).max(1);
        let t_emu = Instant::now();
        for k in 0..frames {
            // Zeitbudget: lieber weniger Bilder pro Durchlauf als ein hängendes Fenster (Turbo)
            let final_frame = k + 1 == frames || t_emu.elapsed().as_secs_f64() > 0.009;
            // nicht angezeigte Bilder nicht zeichnen (bei Run-Ahead zeigt die Vorschau-Kopie das Bild)
            // Run-Ahead braucht eine Kopie des Spiels; mit Zelda 3 gibt es nur ein Spiel -> dort aus
            let runahead = if g.snes.z3.active() { 0 } else { self.s.runahead };
            g.snes.bus.ppu.skip = if runahead > 0 { true } else { !final_frame && k + 2 < frames };
            if self.ext > 0 {
                // Dungeon-Räume: den ganzen 512er-Block zeigen (auch was hinter Türen im anderen Viertel liegt)
                let full = alttp::ram8(&g.snes, 0x7E001B) != 0 && in_dungeon(&g.dmaps, shown_room(&g.snes));
                let fresh = g.m16tab.as_ref().and_then(|t| ow_map_full(&g.snes, t, full));
                // beim Gebietswechsel gilt die Gebietskarte kurz als ungültig -> die letzte gültige weiterverwenden,
                // sonst springt der Rand für einzelne Bilder auf eine andere Quelle (Flackern/Doppelbilder)
                let stable = stable_ow(fresh, &g.last_ow);
                if let Some(o) = &stable {
                    if o.valid && !o.room {
                        g.last_ow = Some(o.clone());
                    }
                } else {
                    g.last_ow = None;
                }
                g.snes.bus.ppu.ow = stable;
            } else {
                g.snes.bus.ppu.ow = None;
            }
            let mut p = pad;
            if self.s.cheats.skip_title && alttp::in_title(&g.snes) {
                self.title_frame = self.title_frame.wrapping_add(1);
                p = if self.title_frame % 24 < 4 { 0x1000 } else { 0 };
            }
            g.snes.bus.joy[0] = p;
            {
                let md = alttp::ram8(&g.snes, 0x7E0010);
                let sub = alttp::ram8(&g.snes, 0x7E0011);
                let joy = p;
                let a_new = joy & 0x0080 != 0 && self.joy_prev & 0x0080 == 0;
                let x_new = joy & 0x0040 != 0 && self.joy_prev & 0x0040 == 0;
                self.joy_prev = joy;
                let in_menu = md == 0x0E && sub == 0x01;
                // Item-Menü: A auf dem Spiegel -> Menü schließen und sofort benutzen
                if self.s.mirror_direct && in_menu && a_new && g.snes.bus.wram[0x0202] == 20 {
                    self.mirror_pending = true;
                    self.menu_close = 6;
                }
                if self.menu_close > 0 {
                    self.menu_close -= 1;
                    if self.menu_close > 2 && in_menu {
                        g.snes.bus.joy[0] |= 0x1000;
                    }
                }
                if self.s.x_slot || self.x_force.is_some() || self.x_using {
                    // X ist der zweite Item-Slot – nichts davon geht ans Spiel (keine Spielkarte)
                    if self.s.x_slot {
                        g.snes.bus.joy[0] &= !0x0040;
                    }
                    if in_menu && x_new {
                        let idx = g.snes.bus.wram[0x0202];
                        if (1..=20).contains(&idx) {
                            let e = quickmenu::Entry { idx, bottle: if idx == 16 { Some(g.snes.bus.wram[0xF34F].max(1)) } else { None } };
                            g.x_item = Some(e);
                            self.x_notice = Some(e);
                        }
                    }
                    let xheld = self.s.x_slot && joy & 0x0040 != 0;
                    let force = if !self.x_using && quickmenu::can_use(&g.snes) { self.x_force.take() } else { None };
                    if force.is_some() || (!self.x_using && x_new && !in_menu && self.s.x_slot && quickmenu::can_use(&g.snes)) {
                        if let Some(e) = force.or(g.x_item.filter(|e| quickmenu::entries(&g.snes).contains(e))) {
                            self.x_hold = if force.is_some() { 5 } else { 0 };
                            let w = &mut g.snes.bus.wram;
                            self.x_restore = (w[0x0202], w[0x0303], w[0xF34F]);
                            w[0x0202] = e.idx;
                            w[0x0303] = Y303[(e.idx as usize - 1).min(19)];
                            if let Some(b) = e.bottle {
                                w[0xF34F] = b;
                            }
                            self.x_using = true;
                            self.x_dirty = false;
                            self.x_frames = 0;
                        }
                    }
                    if self.x_using {
                        self.x_frames += 1;
                        let xheld = xheld || self.x_hold > 0;
                        self.x_hold = self.x_hold.saturating_sub(1);
                        if xheld {
                            g.snes.bus.joy[0] |= 0x4000;
                        }
                        if !quickmenu::can_use(&g.snes) {
                            // z. B. „Magie reicht nicht“: Text hat die Item-Anzeige mit dem X-Item neu gezeichnet
                            self.x_dirty = true;
                        }
                        let w = &g.snes.bus.wram;
                        // frei: normal stehen – als Hase (Schattenwelt ohne Mondperle) ist der Zustand 17/1C, auch das zählt
                        // (sonst wartet es nach dem Spiegel 20 Sekunden und ↑ geht so lange nicht)
                        let idle = matches!(w[0x5D], 0x00 | 0x17 | 0x1C) && w[0x0301] == 0 && w[0x037A] == 0 && w[0x0300] == 0;
                        if !xheld && self.x_frames > 6 && quickmenu::can_use(&g.snes) && (idle || self.x_frames > 1200) {
                            let (a, b, c) = self.x_restore;
                            let w = &mut g.snes.bus.wram;
                            w[0x0202] = a;
                            w[0x0303] = b;
                            w[0xF34F] = c;
                            self.x_using = false;
                            if self.x_dirty {
                                // Y-Item richtig ausrüsten, damit die Anzeige wieder stimmt
                                let e = quickmenu::Entry { idx: a, bottle: if a == 16 { Some(c.max(1)) } else { None } };
                                let skip = g.snes.bus.ppu.skip;
                                g.snes.bus.ppu.skip = true;
                                let _ = quickmenu::equip(&mut g.snes, e);
                                g.snes.bus.ppu.skip = skip;
                                self.x_dirty = false;
                            }
                        }
                    }
                }
                if self.mirror_pending && quickmenu::can_use(&g.snes) && g.snes.bus.wram[0x0202] == 20 {
                    // Y kurz drücken
                    self.mirror_pending = false;
                    self.mirror_press = 6;
                }
                if self.mirror_press > 0 {
                    self.mirror_press -= 1;
                    if self.mirror_press > 2 {
                        g.snes.bus.joy[0] |= 0x4000;
                    }
                }
            }
            let md = alttp::ram8(&g.snes, 0x7E0010);
            let sub = alttp::ram8(&g.snes, 0x7E0011);
            let indoors = alttp::ram8(&g.snes, 0x7E001B) != 0;
            let dialog = md == 0x0E && sub == 0x02;
            g.snes.bus.ppu.hud_split = self.ext > 0 && md >= 0x06 && md != 0x14 && (md != 0x0E || dialog);
            g.snes.bus.ppu.win_stretch = md == 0x0F || md == 0x10;
            // Breitbild in Räumen: Grafikspeicher vor einem Raumwechsel merken (der alte Raum für den Rand beim Scrollen)
            while let Ok((r, v)) = g.room_rx.try_recv() {
                g.room_pending.remove(&r);
                g.room_vram.entry(r).or_insert_with(|| Arc::new(v));
            }
            if self.ext > 0 && md == 0x07 && sub == 0 && indoors {
                let v = std::sync::Arc::new(g.snes.bus.ppu.vram.clone());
                // jeden besuchten Raum merken: beim nächsten Wechsel dorthin ist der Rand sofort vollständig
                g.room_vram.insert(alttp::ram16(&g.snes, 0x7E00A0), v.clone());
                g.snes.bus.ppu.room_keep = Some(v);
                g.snes.bus.ppu.room_next = None;
                // noch nie betretene Nachbarräume (gleiche Etage) unsichtbar im Hintergrund laden
                let room = alttp::ram16(&g.snes, 0x7E00A0);
                if let Some(rt) = g.room_table {
                    let todo: Vec<u16> = near_rooms(&g.dmaps, room).into_iter().filter(|n| !g.room_vram.contains_key(n) && !g.room_pending.contains(n)).collect();
                    if !todo.is_empty() {
                        // nacheinander in einem Hintergrund-Thread (nicht alle gleichzeitig – sonst ruckelt das Spiel)
                        g.room_pending.extend(todo.iter().copied());
                        let base = g.snes.clone();
                        let eid = alttp::ram8(&g.snes, 0x7E010E);
                        let tx = g.room_tx.clone();
                        let _ = std::thread::Builder::new().name("room-capture".into()).spawn(move || {
                            for n in todo {
                                let v = crate::teleport::capture_room(&base, rt, eid, n).unwrap_or_default();
                                if tx.send((n, v)).is_err() {
                                    break;
                                }
                            }
                        });
                    }
                }
            } else if md == 0x07 && matches!(sub, 1 | 2) && indoors {
                g.snes.bus.ppu.room_next = g.room_vram.get(&alttp::ram16(&g.snes, 0x7E00A0)).cloned();
            } else if !indoors || md < 0x06 {
                // draußen bzw. Titel: Raum-Kopien weg (Dialoge/Menüs im Raum behalten sie, sonst blinkt der Nachbarraum)
                g.snes.bus.ppu.room_keep = None;
                g.snes.bus.ppu.room_next = None;
                g.snes.bus.ppu.room_side.clear();
            }
            // Nachbarräume links/rechts derselben Etage – auch während des Raumwechsels (kein Aufblitzen).
            // Treppen/Überblendungen: der Raum steht schon auf dem Ziel, das Bild zeigt aber noch den alten Raum ->
            // die alten Nachbarn behalten, bis das Bild dunkel ist
            if md == 0x07 && indoors {
                let room = shown_room(&g.snes);
                g.snes.bus.ppu.room_side = near_rooms(&g.dmaps, room)
                    .into_iter()
                    .filter_map(|n| g.room_vram.get(&n).filter(|v| !v.is_empty()).map(|v| (n, v.clone())))
                    .collect();
            }
            // Rand hinter der Gebietsgrenze (beim Scrollen zwischen Gebieten immer die Nachbarn zeigen)
            g.snes.bus.ppu.ow_edge = if md == 0x09 && sub != 0 {
                ppu::EDGE_NEIGHBOR
            } else if self.s.edge_recomp {
                ppu::EDGE_RECOMP
            } else {
                ppu::EDGE_SEAMLESS
            };
            g.snes.bus.ppu.margin = if md == 0x14 {
                attract_margin(&g.snes)
            } else if md < 0x06 {
                ppu::MARGIN_BLANK
            } else if indoors {
                ppu::MARGIN_ROOM
            } else {
                ppu::MARGIN_WORLD
            };
            alttp::apply_cheats(&mut g.snes, &self.s.cheats, &self.s.codes);
            // Sicherheitsnetz: bleibt die Bewegungssperre des Spiels ($02E4) im normalen Spiel hängen (z. B. nach einer
            // abgebrochenen Szene), läuft Link nie wieder. Drückt man ~3 s eine Richtung und Link bleibt stehen -> lösen
            {
                let md = alttp::ram8(&g.snes, 0x7E0010);
                let pos = (alttp::ram16(&g.snes, 0x7E0022), alttp::ram16(&g.snes, 0x7E0020));
                let dir = g.snes.bus.joy[0] & 0x0F00 != 0;
                let locked = g.snes.bus.wram[0x02E4] != 0 && matches!(md, 0x07 | 0x09 | 0x0B) && alttp::ram8(&g.snes, 0x7E0011) == 0;
                if locked && dir && pos == g.freeze_pos {
                    g.freeze_t += 1;
                    if g.freeze_t > 180 {
                        g.snes.bus.wram[0x02E4] = 0;
                        g.freeze_t = 0;
                    }
                } else {
                    g.freeze_t = 0;
                }
                g.freeze_pos = pos;
            }
            // Sicherheitsnetz: Paletten-Tausch der oberen Ebene ($0ABD, z. B. auf Brücken) hängt geblieben, obwohl Link
            // unten ist ($EE = 0) – dann zeigen Schild und Schwert auf eine dunkle Palette (schwarz). Kurz warten, dann lösen
            {
                let md = alttp::ram8(&g.snes, 0x7E0010);
                let stuck = g.snes.bus.wram[0x0ABD] != 0 && g.snes.bus.wram[0xEE] == 0 && matches!(md, 0x09 | 0x0B) && alttp::ram8(&g.snes, 0x7E0011) == 0;
                g.swap_t = if stuck { g.swap_t + 1 } else { 0 };
                if g.swap_t > 20 {
                    g.snes.bus.wram[0x0ABD] = 0;
                    g.swap_t = 0;
                }
            }
            {
                let room = alttp::ram16(&g.snes, 0x7E00A0);
                if self.s.cheats.always_light && alttp::in_game(&g.snes) {
                    if let Some(v) = alttp::always_light(&mut g.snes) {
                        g.light_orig = Some((room, v));
                    }
                } else if let Some((r, v)) = g.light_orig.take() {
                    // ausgeschaltet: im selben Raum die Dunkelheit sofort zurück (neue Räume setzt das Spiel selbst)
                    if indoors && r == room && alttp::ram8(&g.snes, 0x7E009A) == 0x20 {
                        g.snes.bus.wram[0x9A] = v;
                    }
                }
            }
            g.tracker.pre_frame(&mut g.snes);
            self.mods.pre_frame(&mut g.snes, &self.s.mods_on);
            g.snes.run_frame();
            self.mods.post_frame(&mut g.snes, &self.s.mods_on);
            g.snes.bus.ppu.update_spot();
            note_fog(g);
            {
                // Fee aus der Flasche hat Link gerettet (Flasche wird beim Sterben leer)
                let now: [u8; 4] = [0, 1, 2, 3].map(|b| g.snes.bus.wram[0xF35C + b]);
                let dying = alttp::ram8(&g.snes, 0x7E0010) == 0x12 || alttp::ram8(&g.snes, 0x7EF36D) == 0;
                if self.s.fairy_notice && dying {
                    for b in 0..4 {
                        if self.bottles_prev[b] == 6 && now[b] == 2 {
                            self.fairy_msg = Some(b as u8 + 1);
                        }
                    }
                }
                self.bottles_prev = now;
            }
            g.tracker.post_frame(&mut g.snes);
            // Text sofort: solange die Schreibposition ($1CD9) weiterläuft, unsichtbar vorspulen (ohne Tasten)
            if self.s.fast_text && !turbo && alttp::ram8(&g.snes, 0x7E0010) == 0x0E && alttp::ram8(&g.snes, 0x7E0011) == 0x02 {
                let skip = g.snes.bus.ppu.skip;
                let joy = g.snes.bus.joy[0];
                g.snes.bus.joy[0] = 0;
                g.snes.bus.ppu.skip = true;
                let mut prev = g.snes.bus.wram[0x1CD9];
                let mut stable = 0;
                for _ in 0..60 {
                    g.snes.run_frame();
                    g.snes.bus.apu.dsp.out.clear();
                    if alttp::ram8(&g.snes, 0x7E0010) != 0x0E || alttp::ram8(&g.snes, 0x7E0011) != 0x02 {
                        break;
                    }
                    let now = g.snes.bus.wram[0x1CD9];
                    if now == prev {
                        stable += 1;
                        if stable >= 3 {
                            break;
                        }
                    } else {
                        stable = 0;
                        prev = now;
                    }
                }
                g.snes.bus.joy[0] = joy;
                g.snes.bus.ppu.skip = skip;
                if !skip {
                    g.snes.run_frame();
                    g.snes.bus.apu.dsp.out.clear();
                }
            }
            // Herzen und Rubine zu Link holen (das Spiel sammelt sie dann ganz normal ein)
            if self.s.auto_collect {
                auto_collect(&mut g.snes);
            }
            // Szenenwechsel (Gebiets-/Raumübergänge, Laden, Abblenden) schneller, sonst normales Tempo
            let ts = self.s.transition_speed.round().clamp(1.0, 8.0) as u32;
            if ts > 1 && !turbo && in_transition(&g.snes) {
                let skip = g.snes.bus.ppu.skip;
                g.snes.bus.ppu.skip = true;
                for _ in 1..ts {
                    g.snes.run_frame();
                    g.snes.bus.apu.dsp.out.clear();
                    if !in_transition(&g.snes) {
                        break;
                    }
                }
                g.snes.bus.ppu.skip = skip;
                if !skip {
                    // das angezeigte Bild nach dem Vorspulen neu zeichnen
                    g.snes.bus.ppu.skip = false;
                    g.snes.run_frame();
                    g.snes.bus.apu.dsp.out.clear();
                }
            }
            // Spielzeit und Tode zählen
            let m = alttp::ram8(&g.snes, 0x7E0010);
            if m == 0x12 && g.last_module != 0x12 {
                g.dying = true;
            }
            if m != 0x12 && g.last_module == 0x12 && g.dying {
                // Todessequenz zu Ende gelaufen (Fortsetzen-Menü oder Fee): jetzt zählt der Tod
                g.deaths += 1;
                g.dying = false;
            }
            if m == 0x07 && alttp::ram8(&g.snes, 0x7E0011) == 0 {
                g.visited.insert(alttp::ram8(&g.snes, 0x7E010E));
            }
            g.last_module = m;
            self.abuf.clear();
            g.snes.take_audio(&mut self.abuf);
            // Töne der Mods dazumischen
            self.mods.mix(&mut self.abuf);
            if audio_ok {
                if let Some(a) = &self.audio {
                    a.push(&self.abuf);
                }
            }
            if self.s.rewind {
                g.rewind_tick = g.rewind_tick.wrapping_add(1);
                if g.rewind_tick % (if turbo || double { 8 } else { 2 }) == 0 {
                    g.rewind.push_back(g.snes.save_state());
                    while g.rewind.len() > max_rewind {
                        g.rewind.pop_front();
                    }
                }
            } else if !g.rewind.is_empty() {
                g.rewind.clear();
            }
            if final_frame {
                if runahead > 0 {
                    let mut ahead = g.snes.clone();
                    ahead.bus.ppu.skip = false;
                    ahead.bus.ppu.capture = false;
                    for _ in 0..self.s.runahead {
                        ahead.run_frame();
                    }
                    g.display.copy_from_slice(ahead.framebuffer());
                    g.wide.clone_from(&ahead.bus.ppu.wide);
                    if let Some(sn) = ahead.bus.ppu.snap.take() {
                        self.snaps = [self.snaps[1].take(), Some(sn)];
                    }
                } else {
                    g.display.copy_from_slice(g.snes.framebuffer());
                    g.wide.clone_from(&g.snes.bus.ppu.wide);
                }
            }
            if runahead == 0 {
                if let Some(sn) = g.snes.bus.ppu.snap.take() {
                    self.snaps = [self.snaps[1].take(), Some(sn)];
                }
            }
            self.frame_no += 1;
            if final_frame {
                if k + 1 < frames && (turbo || double) {
                    self.acc = 0.0;
                }
                break;
            }
        }
        g.snes.bus.ppu.skip = false;
        self.game_fps_count += frames;
        // Farben des aktuellen Gebiets nachziehen, falls die Aufnahme sie nicht richtig hatte
        if g.areas_done && alttp::ram8(&g.snes, 0x7E0010) == 0x09 && alttp::ram8(&g.snes, 0x7E0011) == 0 && alttp::ram8(&g.snes, 0x7E001B) == 0 && g.snes.bus.ppu.brightness() == 15 {
            let h = g.snes.bus.ppu.cgram[..128].iter().fold(1469598103934665603u64, |h, &c| (h ^ c as u64).wrapping_mul(1099511628211));
            g.pal_stable = if g.pal_stable.0 == h { (h, g.pal_stable.1 + 1) } else { (h, 0) };
            if g.pal_stable.1 == 30 && ppu::refresh_area_cgram(alttp::ram8(&g.snes, 0x7E040A), &g.snes.bus.ppu.cgram) {
                g.areas_dirty = true;
            }
        }
        // Breitbildrand: Karte des aktuellen Gebiets mit dem Arbeitsspeicher abgleichen (Türen, Gras, Wände)
        if g.areas_done && self.fps_count % 10 == 0 && alttp::ram8(&g.snes, 0x7E0010) == 0x09 && alttp::ram8(&g.snes, 0x7E0011) == 0 && alttp::ram8(&g.snes, 0x7E001B) == 0 {
            let w = &g.snes.bus.wram;
            let live: Vec<u16> = (0..4096).map(|i| w[0x2000 + i * 2] as u16 | (w[0x2001 + i * 2] as u16) << 8).collect();
            if ppu::refresh_area_m16(alttp::ram8(&g.snes, 0x7E040A), &live) {
                g.areas_dirty = true;
            }
        }
        if g.areas_dirty && self.fps_count % 900 == 0 {
            save_areas(&g.path, &ppu::all_areas());
            g.areas_dirty = false;
        }
        // Gebiete einmal im Hintergrund aufnehmen (nur nötig, wenn Breitbild an ist)
        if self.ext > 0 && !g.areas_done && g.areas_job.is_none() && crate::teleport::can_teleport(&g.snes) && alttp::ram8(&g.snes, 0x7EF3C5) >= 1 {
            let base = g.snes.clone();
            let rom = g.orig_rom.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new().name("areas".into()).spawn(move || crate::teleport::capture_areas(&base, &rom, &[], tx));
            g.areas_job = Some(rx);
        }
        if let Some(rx) = &g.areas_job {
            loop {
                match rx.try_recv() {
                    Ok((id, a)) => {
                        g.areas_new.push((id, a.m16.clone(), a.chr.clone(), a.cgram.clone()));
                        ppu::set_area(id, a);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        g.areas_job = None;
                        g.areas_done = true;
                        if g.areas_new.len() > 40 {
                            save_areas(&g.path, &g.areas_new);
                        }
                        g.areas_new.clear();
                        break;
                    }
                }
            }
        }
        for w in 0..2 {
            if !ppu::has_world_map(w) {
                if let Some(img) = &g.tracker.full[w] {
                    let px = img.iter().map(|c| (c.r() as u16 >> 3) | ((c.g() as u16 >> 3) << 5) | ((c.b() as u16 >> 3) << 10)).collect();
                    ppu::set_world_map(w, px);
                }
            }
        }
        // Dungeon-Karte: Raumbilder aufnehmen und genommene Treppen merken
        if alttp::ram8(&g.snes, 0x7E0010) == 0x07 && alttp::ram8(&g.snes, 0x7E0011) == 0 && g.snes.bus.ppu.brightness() == 15 {
            let room = alttp::ram16(&g.snes, 0x7E00A0);
            if room != g.last_room {
                if let (Some(a), Some(b)) = (world::room_dungeon(&g.dmaps, g.last_room), world::room_dungeon(&g.dmaps, room)) {
                    let fl = |r: u16| g.dmaps[a as usize].floors.iter().position(|f| f.contains(&(r as u8)));
                    if a == b && fl(g.last_room) != fl(room) && !g.stairs.contains(&(g.last_room, room)) {
                        g.stairs.push((g.last_room, room));
                    }
                }
                g.last_room = room;
                g.room_frames = 0;
            }
            g.room_frames += frames;
            if (g.room_frames >= 20 && !g.room_imgs.contains_key(&room)) || (g.room_frames >= 240 && g.room_frames < 240 + frames) {
                // dunkler Raum (auch mit „Immer hell“): BG1 ist nur der Lichtkegel der Laterne
                let dark = alttp::ram8(&g.snes, 0x7E009A) & 0x80 != 0 || g.light_orig.map_or(false, |(r, _)| r == room);
                g.room_imgs.insert(room, g.snes.bus.ppu.room_thumb(dark));
                self.room_tex.remove(&room);
            }
        }
        let path = g.path.clone();
        g.tracker.maybe_capture_full(&g.snes, &path);
        if self.qm.cooldown > frames {
            self.qm.cooldown -= frames;
        } else if self.qm.job.is_none() && quickmenu::can_use(&g.snes) {
            let mut sig = quickmenu::signature(&g.snes);
            // im Dungeon einmal neu aufnehmen, solange die Dungeon-Symbole (Karte, Kompass …) noch fehlen
            sig.push((crate::ui::current_dungeon(&g.snes).is_some() && !self.qm.equip_raw.contains_key(&7)) as u8);
            if sig != self.qm.sig {
                // Symbole im Hintergrund aufnehmen (Kopie des Spiels) – kein Ruckeln
                self.qm.sig = sig;
                let copy = g.snes.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = std::thread::Builder::new().name("qm-icons".into()).spawn(move || {
                    let _ = tx.send(quickmenu::capture_all(&copy));
                });
                self.qm.job = Some(rx);
            }
        }
        self.new_frame = true;
    }

    pub(crate) fn ensure_lut(&mut self) {
        let key = (self.s.color_rando, self.s.color_seed, self.s.saturation.to_bits(), self.s.gamma.to_bits());
        if key == self.lut_key && !self.lut.is_empty() {
            return;
        }
        self.lut_key = key;
        self.new_frame = true;
        self.shown_w = 0;
        self.thumbs.clear();
        let mut rng = self.s.color_seed.wrapping_mul(2_654_435_761) ^ 0x9E37_79B9;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };
        let angle = ((next() % 300) as f32 + 30.0).to_radians();
        let perm = next() % 6;
        let (c, s) = (angle.cos(), angle.sin());
        let m = [
            [0.299 + 0.701 * c + 0.168 * s, 0.587 - 0.587 * c + 0.330 * s, 0.114 - 0.114 * c - 0.497 * s],
            [0.299 - 0.299 * c - 0.328 * s, 0.587 + 0.413 * c + 0.035 * s, 0.114 - 0.114 * c + 0.292 * s],
            [0.299 - 0.300 * c + 1.250 * s, 0.587 - 0.588 * c - 1.050 * s, 0.114 + 0.886 * c - 0.203 * s],
        ];
        let rando = self.s.color_rando;
        let sat = self.s.saturation;
        let inv_gamma = 1.0 / self.s.gamma.max(0.1);
        self.lut = (0..32768u32)
            .map(|v| {
                let mut rgb = [(v & 31) as f32 / 31.0, ((v >> 5) & 31) as f32 / 31.0, ((v >> 10) & 31) as f32 / 31.0];
                if rando {
                    rgb = match perm {
                        1 => [rgb[0], rgb[2], rgb[1]],
                        2 => [rgb[1], rgb[0], rgb[2]],
                        3 => [rgb[1], rgb[2], rgb[0]],
                        4 => [rgb[2], rgb[0], rgb[1]],
                        5 => [rgb[2], rgb[1], rgb[0]],
                        _ => rgb,
                    };
                    let o = rgb;
                    for (i, row) in m.iter().enumerate() {
                        rgb[i] = row[0] * o[0] + row[1] * o[1] + row[2] * o[2];
                    }
                }
                let l = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
                let ch = |x: f32| ((l + (x - l) * sat).clamp(0.0, 1.0).powf(inv_gamma) * 255.0 + 0.5) as u8;
                Color32::from_rgb(ch(rgb[0]), ch(rgb[1]), ch(rgb[2]))
            })
            .collect();
    }

    /// Stellt das anzuzeigende Bild zusammen: Zwischenbild (hohe FPS), Breitbild oder normales Bild.
    /// Rückgabe: ob sich das Bild geändert hat.
    fn compose_view(&mut self) -> bool {
        let Some(g) = &self.game else { return false };
        let w = W + 2 * self.ext;
        let frozen = self.paused || self.qm.open || self.win_states || self.rewind_on;
        if self.interp_ok && !frozen {
            if let [Some(a), Some(b)] = &self.snaps {
                let t = self.acc.clamp(0.0, 1.0) as f32;
                let key = (self.frame_no, (t * 8.0) as u32, w);
                if key == self.interp_key && !self.new_frame {
                    return false;
                }
                self.interp_key = key;
                self.new_frame = false;
                ppu::render_interp(a, b, t, self.ext, self.eff_mask, &mut self.view);
                self.view_w = w;
                return true;
            }
        }
        if !self.new_frame {
            return false;
        }
        self.new_frame = false;
        if self.ext > 0 && g.wide.len() == w * H {
            self.view.clone_from(&g.wide);
            self.view_w = w;
        } else if self.ext > 0 && self.snaps[1].is_some() {
            ppu::render_snap(self.snaps[1].as_ref().unwrap(), self.ext, self.eff_mask, &mut self.view);
            self.view_w = w;
        } else if self.ext > 0 {
            // kein Breitbild-Bild vorhanden (z. B. direkt nach Teleport/Laden): aus dem aktuellen Zustand zeichnen
            let snap = g.snes.bus.ppu.snapshot();
            ppu::render_snap(&snap, self.ext, self.eff_mask, &mut self.view);
            self.view_w = w;
        } else {
            self.view.clone_from(&g.display);
            self.view_w = W;
        }
        true
    }

    /// Spielbild zusammensetzen und das X-Feld hineinzeichnen (so wird es wie das Spiel hochskaliert).
    fn compose_view_hud(&mut self) -> bool {
        let composed = self.compose_view();
        let key = self.hud_key();
        if composed {
            self.view_base.clone_from(&self.view);
        } else if key == self.hud_last {
            return false;
        } else if self.view_base.len() == self.view.len() {
            self.view.clone_from(&self.view_base);
        }
        self.hud_last = key;
        if let Some(k) = key {
            self.draw_hud_into_view(k);
        }
        true
    }

    fn update_texture(&mut self, ctx: &gfx::Context) {
        self.ensure_lut();
        let mut changed = false;
        // unverändertes Zwischenbild (z. B. Stillstand) nicht erneut skalieren/hochladen
        if self.compose_view_hud() && !(self.view_w == self.shown_w && self.view == self.shown) {
            self.shown.clone_from(&self.view);
            self.shown_w = self.view_w;
            changed = true;
            let n = self.view_w * H;
            if self.cur.len() != n {
                self.cur = vec![Color32::BLACK; n];
                self.prev = vec![Color32::BLACK; n];
                self.blend = vec![Color32::BLACK; n];
            }
            std::mem::swap(&mut self.prev, &mut self.cur);
            for (o, &c) in self.cur.iter_mut().zip(self.view.iter()) {
                *o = self.lut[c as usize & 0x7FFF];
            }
        }
        let blending = self.s.frame_blend && !self.s.hi_fps && !self.paused && self.game.is_some() && self.prev.len() == self.cur.len();
        let key = (self.s.filter, blending);
        if !changed && !blending && key == self.tex_key && (self.tex.is_some() || self.gpu_mode().is_some()) {
            return;
        }
        self.tex_key = key;
        let src: &[Color32] = if blending {
            let a = (self.acc.clamp(0.0, 1.0) * 256.0) as u32;
            for ((o, p), c) in self.blend.iter_mut().zip(self.prev.iter()).zip(self.cur.iter()) {
                let mix = |x: u8, y: u8| ((x as u32 * (256 - a) + y as u32 * a) >> 8) as u8;
                *o = Color32::from_rgb(mix(p.r(), c.r()), mix(p.g(), c.g()), mix(p.b(), c.b()));
            }
            &self.blend
        } else {
            &self.cur
        };
        if self.gpu_mode().is_some() {
            // HD auf der Grafikkarte: nur das kleine Bild übergeben
            self.gpu.set_frame(src, self.view_w, H);
            return;
        }
        let size = filters::apply(self.s.filter, src, self.view_w, H, &mut self.ftmp, &mut self.fout);
        let img = gfx::ColorImage { size, pixels: std::mem::take(&mut self.fout) };
        let opts = self.s.filter.texture_options();
        match &mut self.tex {
            Some(t) => t.set(img, opts),
            None => self.tex = Some(ctx.load_texture("screen", img, opts)),
        }
    }

    /// Pixel-Modus: Einblendungen in Spielpixeln und Pixel-Schrift
    /// Karte, Inventar und Anzeigen immer im Stil der Spielmenüs. Gezeichnet wird je nach Bildmodus
    /// scharf (SD), mit feineren Pixeln (HD Max, `pixfont::fine`) oder geglättet (HD, `pixfont::smooth`).
    pub(crate) fn pixel_ui(&self) -> bool {
        true
    }

    /// passender Stil für Karte und Inventar: HD weich / HD Super geglättet,
    /// HD Max / HD Kanten in feinen Pixeln (`fine_ui`), Original scharf pixelig
    pub(crate) fn smooth_ui(&self) -> bool {
        matches!(self.s.filter.menu_mode(), Filter::HdSoft | Filter::HdSuper)
    }

    pub(crate) fn fine_ui(&self) -> bool {
        matches!(self.s.filter.menu_mode(), Filter::HdMax | Filter::HdSharp)
    }

    /// Bildfilter, die auf der Grafikkarte laufen (sonst None = CPU-Filter).
    pub(crate) fn gpu_mode(&self) -> Option<crate::gpu::Mode> {
        if self.gpu.failed() {
            return None;
        }
        use crate::gpu::Mode;
        match self.s.filter {
            Filter::Pixel => Some(Mode::Pixel),
            Filter::Smooth => Some(Mode::Smooth),
            Filter::HdSoft => Some(Mode::HdSoft),
            Filter::HdClean => Some(Mode::HdClean),
            Filter::HdUltra => Some(Mode::Ultra),
            Filter::HdMax => Some(Mode::Max),
            Filter::HdSuper => Some(Mode::Super),
            _ => None,
        }
    }

    /// Speichert die Einstellungen sofort, sobald sich etwas geändert hat.
    pub(crate) fn save_settings_if_changed(&mut self) {
        // Spiel-Einstellungen beim laufenden Spiel ablegen
        if let Some(k) = self.game.as_ref().and_then(|g| g.path.file_stem().map(|k| k.to_string_lossy().to_string())) {
            self.s.store_game(&k);
        }
        let now = serde_json::to_string(&self.s).unwrap_or_default();
        if now != self.saved_settings {
            self.s.save();
            self.saved_settings = now;
        }
    }

    fn periodic(&mut self, ctx: &gfx::Context) {
        if self.fps_time.elapsed().as_secs_f64() >= 1.0 {
            self.fps = self.fps_count as f64 / self.fps_time.elapsed().as_secs_f64();
            self.game_fps = self.game_fps_count as f64 / self.fps_time.elapsed().as_secs_f64();
            self.game_fps_count = 0;
            self.fps_count = 0;
            self.fps_time = Instant::now();
        }
        let mut flush = false;
        if let Some(g) = &mut self.game {
            if g.sram_timer > 2.0 {
                g.sram_timer = 0.0;
                flush = true;
            }
            if g.map_timer > 20.0 {
                g.map_timer = 0.0;
            }
        }
        if flush {
            self.flush_sram();
        }
        if self.settings_timer.elapsed().as_secs_f32() >= 0.5 {
            self.settings_timer = Instant::now();
            self.save_settings_if_changed();
        }
        if self.title_dirty {
            self.title_dirty = false;
            let t = match &self.game {
                Some(g) => format!("Zelda Core – {}", g.info.title),
                None => "Zelda Core".to_string(),
            };
            ctx.send_viewport_cmd(gfx::ViewportCommand::Title(t));
        }
    }
}

/// Raum, den das Bild gerade zeigt: bei Treppen steht $A0 schon auf dem Ziel, während noch der alte Raum zu sehen ist
pub(crate) fn shown_room(s: &Snes) -> u16 {
    let room = alttp::ram16(s, 0x7E00A0);
    if alttp::ram8(s, 0x7E0010) == 0x07 && !matches!(alttp::ram8(s, 0x7E0011), 0 | 1 | 2) {
        let (cx, cy) = (alttp::ram16(s, 0x7E00E2) as u32 + 128, alttp::ram16(s, 0x7E00E8) as u32 + 112);
        let (bx, by) = (cx >> 9, cy >> 9);
        if bx < 16 && by < 16 && (bx as u16 != room & 15 || by as u16 != (room >> 4) & 15) {
            return (room & 0x100) | (by as u16) << 4 | bx as u16;
        }
    }
    room
}

/// Räume links/rechts (auch schräg darüber/darunter), nur wenn sie auf derselben Dungeon-Etage liegen
pub(crate) fn near_rooms(dmaps: &[DungeonMap], room: u16) -> Vec<u16> {
    let (col, row) = ((room & 15) as i32, (room >> 4) as i32);
    let same = |n: u16| dmaps.iter().any(|d| d.floors.iter().any(|f| f.contains(&(room as u8)) && f.contains(&(n as u8))));
    let mut v = Vec::new();
    for dy in [0, -1, 1] {
        for dx in [-1, 1] {
            let (c, r) = (col + dx, row + dy);
            if (0..16).contains(&c) && (0..16).contains(&r) {
                let n = (r * 16 + c) as u16;
                if same(n) {
                    v.push(n);
                }
            }
        }
    }
    v
}

/// Breitbildrand in der Vorspann-Demo (Modul 0x14): Szenen in Räumen (Thronsaal, Kerker …) haben
/// vollständige Raumdaten -> wie im Spiel den Raum zeigen; Text- und Kartenszenen: Rand fortsetzen.
pub(crate) fn attract_margin(s: &Snes) -> u8 {
    if alttp::ram8(s, 0x7E001B) != 0 && s.bus.ppu.r.bgmode & 7 == 1 {
        ppu::MARGIN_ROOM
    } else {
        ppu::MARGIN_BLANK
    }
}

/// Gebietskarte der Oberwelt für die Breitbildränder. Wird nur genutzt, wenn sie in der Bildmitte
/// exakt zu dem passt, was das Spiel gerade zeigt (Selbstprüfung).
#[allow(dead_code)]
pub(crate) fn ow_map(s: &Snes, tab: &Arc<Vec<u16>>) -> Option<Arc<ppu::OwMap>> {
    ow_map_full(s, tab, false)
}

/// Raum liegt auf einer Dungeon-Etage (keine Häuser/Höhlen, die sich einen Block teilen)
pub(crate) fn in_dungeon(dmaps: &[DungeonMap], room: u16) -> bool {
    room < 0x100 && dmaps.iter().any(|d| d.floors.iter().any(|f| f.contains(&(room as u8))))
}

/// wie `ow_map`; `full`: in Räumen immer den ganzen 512er-Block als Raum nehmen (Dungeons: das andere Viertel
/// hinter einer Tür gehört dazu und steht vollständig in den Kacheln)
/// Beim Gebietswechsel gilt die Gebietskarte kurz als ungültig -> die letzte gültige weiterverwenden, sonst springt der
/// Rand für einzelne Bilder auf eine andere Quelle (Flackern/Doppelbilder). Ebenso beim Wechsel zwischen Licht- und
/// Schattenwelt (Spiegel, Teleport-Feld): das Spiel steht schon auf der neuen Welt, das Bild zeigt aber noch die alte
/// (sie wabert und blendet nach Weiß) – solange die alte Karte noch geladen ist, bleibt der Rand bei der alten Welt
pub(crate) fn stable_ow(fresh: Option<Arc<ppu::OwMap>>, last: &Option<Arc<ppu::OwMap>>) -> Option<Arc<ppu::OwMap>> {
    match (&fresh, last) {
        (Some(f), Some(l)) if !f.valid && !f.room && l.valid && (l.world == f.world || l.m16 == f.m16) && (f.camx - l.camx).abs() < 400 && (f.camy - l.camy).abs() < 400 => Some(Arc::new(ppu::OwMap {
            m16: l.m16.clone(),
            tab: l.tab.clone(),
            x0: l.x0,
            y0: l.y0,
            size: l.size,
            camx: f.camx,
            camy: f.camy,
            world: l.world,
            valid: true,
            room: false,
            bx0: l.bx0,
            bx1: l.bx1,
            by0: l.by0,
            by1: l.by1,
            rep0: l.rep0,
            rep1: l.rep1,
            keep0: 0,
            keep1: 0,
            next0: 0,
            next1: 0,
            blk0: 0,
            lx: 0,
        })),
        _ => fresh,
    }
}

pub(crate) fn ow_map_full(s: &Snes, tab: &Arc<Vec<u16>>, full: bool) -> Option<Arc<ppu::OwMap>> {
    let md = alttp::ram8(s, 0x7E0010);
    // Vorspann-Demo: nur ihre Raum-Szenen (siehe `attract_margin`)
    if md < 0x06 || (md == 0x14 && attract_margin(s) != ppu::MARGIN_ROOM) {
        return None;
    }
    if alttp::ram8(s, 0x7E001B) != 0 {
        // Raum: 512er-Block des Raums; $A6/$A7 = 0 heißt nur 256 breit/hoch (Viertel, in dem Link steht)
        let room = alttp::ram16(s, 0x7E00A0) as i32;
        let (mut rx, ry) = ((room & 15) * 512, (room >> 4) * 512);
        let (lx, ly) = (alttp::ram16(s, 0x7E0022) as i32 + 8, alttp::ram16(s, 0x7E0020) as i32 + 12);
        // Vorspann-Demo: Kamera wird vom Spiel gesteuert, die Viertel-Angaben stimmen dort nicht -> ganzer Raum
        let demo = md == 0x14;
        // Wechsel zwischen Räumen (Scrollen): vorne steht der neue Raum schon in den Kacheln, hinten noch der alte
        // bis zu seinem Rand -> beides echt zeigen, erst dahinter die Randkachel fortsetzen
        // nur echtes Scrollen (1 = im Raum, 2 = zum Nachbarraum); Treppen und Überblendungen nicht
        let scrolling = md == 0x07 && matches!(alttp::ram8(s, 0x7E0011), 1 | 2);
        static LAST_ROOM: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);
        static LAST_Y: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);
        let (xmin, xmax) = if alttp::ram8(s, 0x7E00A6) == 0 && !demo && !full {
            let q = rx + if lx - rx >= 256 { 256 } else { 0 };
            (q, q + 256)
        } else {
            (rx, rx + 512)
        };
        let (ymin, ymax) = if alttp::ram8(s, 0x7E00A7) == 0 && !demo {
            let q = ry + if ly - ry >= 256 { 256 } else { 0 };
            (q, q + 256)
        } else {
            (ry, ry + 512)
        };
        // Treppen/Überblendungen: der Raum steht schon auf dem Ziel, die Kamera zeigt aber noch den alten Raum ->
        // so lange die alten Grenzen nehmen (sonst wird alles außerhalb von 4:3 abgeschnitten)
        let (mut xmin, mut xmax, mut ymin, mut ymax) = (xmin, xmax, ymin, ymax);
        if md == 0x07 && !matches!(alttp::ram8(s, 0x7E0011), 0 | 1 | 2) && !demo {
            let (cx, cy) = (alttp::ram16(s, 0x7E00E2) as i32 + 128, alttp::ram16(s, 0x7E00E8) as i32 + 112);
            if !(rx..rx + 512).contains(&cx) || !(ry..ry + 512).contains(&cy) {
                if let (Some((a, b)), Some((c, d))) = (LAST_ROOM.lock().ok().and_then(|l| *l), LAST_Y.lock().ok().and_then(|l| *l)) {
                    (xmin, xmax, ymin, ymax) = (a, b, c, d);
                    rx = a & !511;
                }
            }
        }
        let (mut bx0, mut bx1) = if demo { (i32::MIN / 4, i32::MAX / 4) } else { (xmin, xmax) };
        let (mut rep0, mut rep1) = (bx0, bx1 - 16);
        let (mut keep0, mut keep1) = (0, 0);
        let (mut next0, mut next1) = (0, 0);
        if !scrolling && !demo {
            if let Ok(mut l) = LAST_ROOM.lock() {
                *l = Some((xmin, xmax));
            }
            if let Ok(mut l) = LAST_Y.lock() {
                *l = Some((ymin, ymax));
            }
        } else if scrolling {
            let (ox0, ox1) = LAST_ROOM.lock().ok().and_then(|l| *l).unwrap_or((xmin, xmax));
            // neuer Raum: seine Breite (Viertel oder ganzer Block), sobald das Spiel sie gesetzt hat
            let (nx0, nx1) = (xmin, xmax);
            // vom alten Raum stehen die letzten 256 Pixel vor der Grenze noch in den Kacheln (die andere Hälfte
            // hat das Spiel schon mit dem neuen Raum belegt): bis dahin echt zeigen, dahinter dessen Randkachel
            let cam = alttp::ram16(s, 0x7E00E2) as i32;
            // noch nicht losgescrollt: der neue Raum ist noch nicht geladen -> Rand wie vor dem Wechsel (alter Raum)
            let started = if nx0 >= ox1 - 1 { cam > ox1 - 256 } else if nx1 <= ox0 + 1 { cam < ox0 } else { true };
            if !started {
                (bx0, rep0, bx1, rep1) = (ox0, ox0, ox1, ox1 - 16);
                (keep0, keep1) = (ox0, ox1);
                // der neue Raum daneben gleich aus der Kopie (falls vorhanden), sonst Randkachel
                if nx0 >= ox1 - 1 {
                    (next0, next1) = (ox1, nx1);
                } else if nx1 <= ox0 + 1 {
                    (next0, next1) = (nx0, ox0);
                }
            } else if nx0 >= ox1 - 1 {
                // nach rechts: hinten (links) der ganze alte Raum aus der Kopie, vorne der neue Raum
                (bx0, rep0, bx1, rep1) = (ox0, ox0, nx1, nx1 - 16);
                (keep0, keep1) = (ox0, ox1);
                (next0, next1) = (ox1, nx1);
            } else if nx1 <= ox0 + 1 {
                // nach links
                (bx0, rep0, bx1, rep1) = (nx0, nx0, ox1, ox1 - 16);
                (keep0, keep1) = (ox0, ox1);
                (next0, next1) = (nx0, ox0);
            } else {
                // innerhalb desselben 512er-Blocks (oder senkrecht): ganzer Block steht in den Kacheln
                (bx0, rep0, bx1, rep1) = (rx, rx, rx + 512, rx + 512 - 16);
            }
        }
        return Some(Arc::new(ppu::OwMap {
            m16: Vec::new(),
            tab: tab.clone(),
            x0: 0,
            y0: 0,
            size: 0,
            camx: alttp::ram16(s, 0x7E00E2) as i32,
            camy: alttp::ram16(s, 0x7E00E8) as i32,
            world: 0,
            valid: false,
            room: true,
            // Vorspann-Demo: keine Raumgrenze (Kamera-Angaben stimmen dort nicht) – einfach die Raumkacheln zeigen
            bx0,
            bx1,
            by0: ymin,
            by1: ymax,
            rep0,
            rep1,
            keep0,
            keep1,
            next0,
            next1,
            blk0: rx,
            lx,
        }));
    }
    let camx = alttp::ram16(s, 0x7E00E2) as i32;
    let camy = alttp::ram16(s, 0x7E00E8) as i32;
    let area = (alttp::ram8(s, 0x7E040A) & 0x3F) as i32;
    let size = if alttp::ram16(s, 0x7E0712) != 0 { 1024 } else { 512 };
    let (x0, y0) = ((area % 8) * 512, (area / 8) * 512);
    let m16: Vec<u16> = (0..4096).map(|i| s.bus.wram[0x2000 + i * 2] as u16 | (s.bus.wram[0x2001 + i * 2] as u16) << 8).collect();
    // Prüfen: 16 Blöcke in der Bildmitte müssen mit dem Grafikspeicher übereinstimmen
    let r = &s.bus.ppu.r;
    let tm = |sx: i32, sy: i32| -> u16 {
        let xx = ((sx + r.hofs[1] as i32) & 0x3FF) as u32;
        let yy = ((sy + r.vofs[1] as i32) & 0x3FF) as u32;
        let (tx, ty) = ((xx / 8) & 63, (yy / 8) & 63);
        let mut a = (((r.bg_sc[1] & 0xFC) as u32) << 8) + ((ty & 31) << 5) + (tx & 31);
        if tx & 32 != 0 {
            a += 0x400;
        }
        if ty & 32 != 0 {
            a += 0x800;
        }
        s.bus.ppu.vram[(a & 0x7FFF) as usize]
    };
    if r.bg_sc[1] & 3 != 3 {
        return None;
    }
    let (mut ok, mut tot) = (0, 0);
    for by in 3..7 {
        for bx in 4..8 {
            let wx = (camx + bx * 32) & !15;
            let wy = (camy + by * 28) & !15;
            let (lx, ly) = (wx - x0, wy - y0);
            if lx < 0 || ly < 0 || lx >= size || ly >= size {
                continue;
            }
            let m = m16[((ly >> 4) * 64 + (lx >> 4)) as usize] as usize;
            let (sx, sy) = (wx - camx, wy - camy);
            tot += 1;
            if (0..4).all(|q| tab.get(m * 4 + q).copied() == Some(tm(sx + (q as i32 & 1) * 8, sy + (q as i32 >> 1) * 8))) {
                ok += 1;
            }
        }
    }
    let valid = tot >= 8 && ok * 10 >= tot * 9;
    let a8 = alttp::ram8(s, 0x7E008A);
    if a8 >= 0x80 {
        // Sondergebiet (Masterschwert-Hain, Zoras Wasserfall): eigene kleine Karte ohne Nachbarn – Welt 2.
        // Ursprung wie bei normalen Gebieten (Zoras Wasserfall liegt bei x = 512), Grenzen aus den Kameragrenzen
        let (left, top) = (alttp::ram16(s, 0x7E0604) as i32, alttp::ram16(s, 0x7E0600) as i32);
        let right = (alttp::ram16(s, 0x7E0606) as i32 + 256).max(left + 256);
        let bottom = (alttp::ram16(s, 0x7E0602) as i32 + 224).max(top + 224);
        let size = (right - x0).max(bottom - y0).clamp(256, 1024);
        let (bx0, bx1) = (left.max(x0), right.min(x0 + size));
        let (by0, by1) = (top.max(y0), bottom.min(y0 + size));
        return Some(Arc::new(ppu::OwMap { m16, tab: tab.clone(), x0, y0, size, camx, camy, world: 2, valid, room: false, bx0, bx1, by0, by1, rep0: bx0, rep1: bx1 - 16, keep0: 0, keep1: 0, next0: 0, next1: 0, blk0: 0, lx: 0 }));
    }
    let world = (a8 >= 0x40) as u8;
    Some(Arc::new(ppu::OwMap { m16, tab: tab.clone(), x0, y0, size, camx, camy, world, valid, room: false, bx0: x0, bx1: x0 + size, by0: y0, by1: y0 + size, rep0: x0, rep1: x0 + size - 16, keep0: 0, keep1: 0, next0: 0, next1: 0, blk0: 0, lx: 0 }))
}

/// $0303 (Item-Nummer für Y) je Menü-Index 1..20 – im Spiel ausgelesen
const Y303: [u8; 20] = [3, 2, 14, 1, 10, 5, 6, 15, 16, 17, 9, 4, 8, 7, 12, 11, 18, 13, 19, 20];

/// Herzen (0xD8) und Rubine (0xD9–0xDB) auf Link schieben, damit sie sofort eingesammelt werden.
fn auto_collect(s: &mut Snes) {
    let m = alttp::ram8(s, 0x7E0010);
    if !matches!(m, 0x07 | 0x09 | 0x0B) || alttp::ram8(s, 0x7E0011) != 0 {
        return;
    }
    let lx = alttp::ram16(s, 0x7E0022);
    let ly = alttp::ram16(s, 0x7E0020).wrapping_add(4);
    let w = &mut s.bus.wram;
    for k in 0..16 {
        if w[0x0DD0 + k] != 0 && (0xD8..=0xDB).contains(&w[0x0E20 + k]) {
            w[0x0D10 + k] = lx as u8;
            w[0x0D30 + k] = (lx >> 8) as u8;
            w[0x0D00 + k] = ly as u8;
            w[0x0D20 + k] = (ly >> 8) as u8;
            w[0x0F70 + k] = 0;
        }
    }
}

/// Szenenwechsel: Laden, Abblenden, Scrollen zwischen Gebieten/Räumen, Treppen
pub fn in_transition(s: &Snes) -> bool {
    let m = alttp::ram8(s, 0x7E0010);
    let sub = alttp::ram8(s, 0x7E0011);
    // 0x0B ist das Spielen in Sondergebieten (Masterschwert-Hain) – nur sein Untermodus ist ein Wechsel
    matches!(m, 0x06 | 0x08 | 0x0A | 0x0F | 0x10 | 0x11) || (matches!(m, 0x07 | 0x09 | 0x0B) && sub != 0)
}

/// Zeigt das Spiel gerade eine Schatten-Überlagerung? (BG1 nur auf dem Sub-Bildschirm in den Verlorenen Wäldern,
/// im Skelettwald und am Todesberg: Blätterdach bzw. Wolkenschatten) – die blendet die App aus.
/// Regen im Moor und der Hintergrund der Pyramide bleiben.
pub(crate) fn shadow_overlay(s: &Snes) -> bool {
    let m = alttp::ram8(s, 0x7E0010);
    // auch während Textboxen und Menüs (0x0E) über der Oberwelt – sonst kämen die Schatten dort zurück
    if !matches!(m, 0x09 | 0x0B | 0x0E) || alttp::ram8(s, 0x7E001B) != 0 {
        return false;
    }
    let a = alttp::ram8(s, 0x7E008A) & 0x7F;
    let parent = crate::teleport::LARGE.iter().copied().find(|&p| [p, p + 1, p + 8, p + 9].contains(&a)).unwrap_or(a);
    let r = &s.bus.ppu.r;
    matches!(parent, 0x00 | 0x40 | 0x03 | 0x05 | 0x07 | 0x43 | 0x45 | 0x47) && r.ts & 1 != 0 && r.tm & 1 == 0
}

/// Weltkarte des Spiels offen? (Modus 0x0E, Untermodus 0x07)
pub fn in_game_map(s: &Snes) -> bool {
    alttp::ram8(s, 0x7E0010) == 0x0E && alttp::ram8(s, 0x7E0011) == 0x07 && alttp::ram8(s, 0x7E001B) == 0
}

/// Linker Stick -> Steuerkreuz mit 8 Richtungen nach Winkel (Diagonalen etwas breiter = leichter zu treffen).
fn stick_dirs(x: f32, y: f32, dz: f32) -> u16 {
    let mut p = 0u16;
    if x > dz {
        p |= 1 << 8;
    }
    if x < -dz {
        p |= 1 << 9;
    }
    if y > dz {
        p |= 1 << 11;
    }
    if y < -dz {
        p |= 1 << 10;
    }
    p
}

pub fn mode_from(m: u8) -> Mode {
    match m {
        0 => Mode::Safe,
        1 => Mode::Mixed,
        _ => Mode::Chaos,
    }
}

pub(crate) fn build_rom(orig: &[u8], rando: Option<&RandoResult>, codes: &[CheatCode], tweaks: (bool, bool, bool, bool, bool, bool)) -> Vec<u8> {
    let mut rom = orig.to_vec();
    alttp::apply_tweaks(&mut rom, tweaks.0, tweaks.1, tweaks.2);
    alttp::apply_tweaks2(&mut rom, tweaks.3, tweaks.4);
    if tweaks.5 {
        alttp::apply_wide_sprites(&mut rom);
    }
    if let Some(r) = rando {
        for &(o, v) in &r.patches {
            if o < rom.len() {
                rom[o] = v;
            }
        }
    }
    alttp::patch_rom_codes(&rom, codes)
}

impl App {
    /// Tab (Turbo) wird nicht als Ereignis weitergereicht, nur sein Zustand gemerkt
    fn tab_hook(&mut self, ctx: &gfx::Context) {
        let typing = ctx.wants_keyboard_input() || self.rebind.is_some() || self.rebind_hotkey.is_some();
        if !typing {
            self.tab_down = ctx.input(|i| i.key_down(Key::Tab));
        }
        if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            self.tab_down = false;
        }
    }

    /// Ein Bild: Eingaben, Emulation, Zeichnen (wird von `shell::Root` aufgerufen)
    pub(crate) fn run_frame(&mut self, ctx: &gfx::Context) {
        self.tab_hook(ctx);
        self.update(ctx);
    }

    fn update(&mut self, ctx: &gfx::Context) {
        // feste Bildrate (z. B. 500 FPS, auch über der Monitorfrequenz): GPUI gibt den Takt vor (eigener
        // Timer statt Monitor, siehe vendor/gpui/PASTLINK.md); 0 = Monitor (VSync)
        gpui::pastlink_set_frame_rate(self.s.fps_limit);
        self.update_ext();
        self.tp_anim_step();
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true)) || self.map_focused;
        // Karte im eigenen Fenster: Spiel läuft weiter
        let map_blocks = self.win_map && !self.s.map_own_window;
        self.auto_paused = self.s.auto_pause && self.game.is_some() && (!focused || map_blocks);
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            self.open_any(p);
        }
        self.hotkeys(ctx);
        self.handle_rebind(ctx);
        let (mut pad, turbo, double, rewind) = self.gather_input(ctx);
        let menu_pad = pad | self.ctl_dpad;
        // GBA-Modus: eigener Ablauf (kein Inventar, nichts am Spiel geändert); Minus = eigene Karte (Minish Cap)
        if self.gba.active() {
            let minus_edge = self.minus_now && !self.minus_prev;
            self.minus_prev = self.minus_now;
            if minus_edge && !self.win_states {
                self.gba_map_toggle();
            }
            // Karte im eigenen Fenster: Eingaben nur an die Karte, wenn deren Fenster vorne ist
            if self.gba.map.open && !self.win_states && (!self.s.map_own_window || self.map_focused) {
                let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
                self.gba_map_input(menu_pad, dt);
                self.states_prev_pad = menu_pad;
                self.gba_frame(ctx, 0, false, false, focused, false, true);
            } else if self.win_states {
                // Spielstände per Controller/Tastatur – nichts geht ans Spiel
                let pressed = menu_pad & !self.states_prev_pad;
                self.states_prev_pad = menu_pad;
                if self.win_pad_settings {
                    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
                    self.pad_settings_input(pressed, menu_pad, dt);
                } else {
                    self.states_nav(pressed);
                }
                self.gba_frame(ctx, 0, false, false, focused, false, true);
            } else {
                self.states_prev_pad = pad;
                self.gba_frame(ctx, pad, turbo, double, focused, rewind, false);
            }
            self.draw_ui(ctx);
            self.periodic(ctx);
            self.fps_count += 1;
            ctx.request_repaint();
            return;
        }
        if self.game.is_none() {
            self.library_input(menu_pad);
        }
        // Steuerkreuz ↑/↓ als Schnellzugriff (nur wenn kein Menü offen ist)
        let dp = self.ctl_dpad & !self.ctl_dpad_prev;
        self.ctl_dpad_prev = self.ctl_dpad;
        if self.win_states {
            self.inv_open = false;
            if !self.s.map_own_window {
                self.win_map = false;
            }
        }
        let minus_edge = self.minus_now && !self.minus_prev;
        self.minus_prev = self.minus_now;
        if minus_edge && self.game.is_some() && !self.win_states && !self.qm.open && !self.inv_open {
            if self.win_map {
                self.win_map = false;
            } else {
                self.open_map_overlay();
            }
        }
        if let Some(b) = self.fairy_msg.take() {
            self.msg(&format!("{} {b}", self.t("✨ Eine Fee hat dich gerettet – Flasche", "✨ A fairy saved you – bottle")));
        }
        if let Some(e) = self.x_notice.take() {
            if let Some(g) = &self.game {
                let n = quickmenu::name(&g.snes, e, self.s.ui_en);
                self.msg(&format!("X: {n}"));
            }
        }
        // Karte über dem Spiel – oder im eigenen Fenster, wenn dieses vorne ist: Eingaben gehen an die Karte
        let overlay = self.win_map && self.game.is_some() && (!self.s.map_own_window || self.map_focused);
        if overlay {
            // Vollbild-Karte: alle Eingaben gehen an die Karte, nichts ans Spiel
            let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
            // Start, das die Karte schließt, darf danach nicht das Inventar öffnen
            self.inv_start_prev = menu_pad;
            self.inv_block |= menu_pad & 0x1000;
            self.map_input(pad | self.ctl_dpad, dt);
            self.states_prev_pad = menu_pad;
            self.update_texture(ctx);
            self.draw_ui(ctx);
            self.periodic(ctx);
            self.fps_count += 1;
            ctx.request_repaint();
            return;
        }
        // eigenes Inventar statt Spielmenü (Start)
        let start_new = menu_pad & 0x1000 != 0 && self.inv_start_prev & 0x1000 == 0;
        self.inv_start_prev = menu_pad;
        if self.inv_block != 0 {
            self.inv_block &= menu_pad;
            pad &= !self.inv_block;
        }
        if !self.inv_open && !self.win_map && self.s.pixel_inv && start_new && self.inv_block & 0x1000 == 0 && !self.win_states && !self.qm.open && self.game.as_ref().map_or(false, |g| quickmenu::can_use(&g.snes)) {
            self.inv_open();
            self.inv_prev = menu_pad;
        }
        if self.inv_open && self.game.is_some() {
            let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
            self.inv_input(menu_pad, dt);
            self.run_emulation(0, false, false, false);
            self.update_texture(ctx);
            self.draw_ui(ctx);
            self.periodic(ctx);
            self.fps_count += 1;
            ctx.request_repaint();
            return;
        }
        if !self.win_states && !self.qm.open && self.game.is_some() {
            // ↑: mit dem Spiegel sofort die Welt wechseln (danach ist wieder das alte Item auf Y)
            if dp & (1 << 11) != 0 {
                if let Some(g) = &self.game {
                    let has_mirror = quickmenu::entries(&g.snes).iter().any(|e| e.idx == 20);
                    if has_mirror && !self.x_using && self.x_force.is_none() && quickmenu::can_use(&g.snes) {
                        self.x_force = Some(quickmenu::Entry { idx: 20, bottle: None });
                    }
                }
            }
        }
        pad = self.in_game_map_logic(pad);
        if !self.win_states {
            self.quickmenu_logic(menu_pad);
        }
        if self.qm.open {
            self.run_emulation(0, false, false, false);
        } else if self.win_states && self.game.is_some() {
            // Spielstände per Controller/Tastatur bedienen – nichts davon geht ans Spiel
            let pressed = menu_pad & !self.states_prev_pad;
            self.states_prev_pad = menu_pad;
            // Plus/Start, das das Menü schließt, darf danach nicht das Inventar des Spiels öffnen
            self.inv_block |= menu_pad & 0x1000;
            if self.win_pad_settings {
                let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
                self.pad_settings_input(pressed, menu_pad, dt);
            } else {
                self.states_nav(pressed);
            }
            self.run_emulation(0, false, false, false);
        } else {
            self.states_prev_pad = pad;
            let c0 = self.game_fps_count;
            crate::perf::time("emu", || self.run_emulation(pad | crate::perf::walk_pad(), turbo, double, rewind));
            crate::perf::emu_frames(self.game_fps_count - c0);
        }
        crate::perf::time("textur", || self.update_texture(ctx));
        crate::perf::time("ui", || self.draw_ui(ctx));
        self.periodic(ctx);
        self.fps_count += 1;
        ctx.request_repaint();
    }

}

impl Drop for App {
    fn drop(&mut self) {
        self.gba_close();
        self.close_game();
        self.s.save();
    }
}

fn write_bmp(path: &Path, w: u32, h: u32, px: &[Color32]) -> std::io::Result<()> {
    let row = (w * 3 + 3) & !3;
    let size = 54 + row * h;
    let mut d: Vec<u8> = Vec::with_capacity(size as usize);
    d.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(w as i32).to_le_bytes());
    d.extend_from_slice(&(h as i32).to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&24u16.to_le_bytes());
    for v in [0, row * h, 2835, 2835, 0, 0] {
        d.extend_from_slice(&(v as u32).to_le_bytes());
    }
    for y in (0..h).rev() {
        for x in 0..w {
            let c = px[(y * w + x) as usize];
            d.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        d.resize(d.len() + (row - w * 3) as usize, 0);
    }
    std::fs::write(path, d)
}

#[cfg(test)]
mod input_tests {
    use super::stick_dirs;
    #[test]
    fn stick_eight_way() {
        assert_eq!(stick_dirs(1.0, 0.0, 0.3), 1 << 8);
        assert_eq!(stick_dirs(0.0, 1.0, 0.3), 1 << 11);
        assert_eq!(stick_dirs(0.7, 0.7, 0.3), (1 << 8) | (1 << 11));
        assert_eq!(stick_dirs(-0.7, -0.7, 0.3), (1 << 9) | (1 << 10));
        assert_eq!(stick_dirs(0.9, 0.2, 0.3), 1 << 8);
        assert_eq!(stick_dirs(0.1, 0.1, 0.3), 0);
    }
}
