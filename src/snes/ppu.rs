//! Bildprozessor: Hintergründe (Mode 0–7), Sprites, Fenster, Farbmathematik, Mosaik.
//! Scanline-Renderer, gibt 15-Bit-BGR-Farben aus.

use serde::{Deserialize, Serialize};

pub const W: usize = 256;
pub const H: usize = 224;

fn default_fb() -> Vec<u16> {
    vec![0; W * H]
}
fn default_mask() -> u8 {
    0x1F
}

const OPAQUE: u32 = 0x8000_0000;
/// Breitbildrand ohne gültige Daten (wird schwarz statt Hintergrundfarbe)
const VOID: u32 = 0x4000_0000;

/// Oberwelt-Rand: Nachbargebiet wo es passt, sonst eigenes Gebiet gespiegelt (keine schwarzen Ränder)
pub const EDGE_SEAMLESS: u8 = 0;
/// wie zelda3 / zelda-alttp-recomp: nur bis zur Gebietsgrenze, dahinter schwarz
pub const EDGE_RECOMP: u8 = 1;
/// Nachbargebiet immer zeigen (beim Scrollen zwischen zwei Gebieten)
pub const EDGE_NEIGHBOR: u8 = 2;

/// Alle PPU-Register (Kopie pro Bildzeile für Zwischenbilder).
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct Regs {
    pub inidisp: u8,
    pub obsel: u8,
    pub oam_addr: u16,
    pub oam_reload: u16,
    pub oam_prio: bool,
    pub oam_latch: u8,
    pub bgmode: u8,
    pub mosaic: u8,
    pub bg_sc: [u8; 4],
    pub bg_nba: [u16; 4],
    pub hofs: [u16; 4],
    pub vofs: [u16; 4],
    pub ofs_latch1: u8,
    pub ofs_latch2: u8,
    pub m7sel: u8,
    pub m7a: i16,
    pub m7b: i16,
    pub m7c: i16,
    pub m7d: i16,
    pub m7x: i16,
    pub m7y: i16,
    pub m7hofs: i16,
    pub m7vofs: i16,
    pub m7_latch: u8,
    pub vmain: u8,
    pub vaddr: u16,
    pub vread: u16,
    pub cg_addr: u16,
    pub cg_latch: u8,
    pub w12sel: u8,
    pub w34sel: u8,
    pub wobjsel: u8,
    pub wh: [u8; 4],
    pub wbglog: u8,
    pub wobjlog: u8,
    pub tm: u8,
    pub ts: u8,
    pub tmw: u8,
    pub tsw: u8,
    pub cgwsel: u8,
    pub cgadsub: u8,
    pub fixed: [u8; 3],
    pub setini: u8,
    pub hlatch: u16,
    pub vlatch: u16,
    pub hflip_r: bool,
    pub vflip_r: bool,
    pub latched: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Ppu {
    pub vram: Vec<u16>,
    pub cgram: Vec<u16>,
    pub oam: Vec<u8>,
    pub r: Regs,
    #[serde(skip, default = "default_fb")]
    pub fb: Vec<u16>,
    /// Debug: welche Ebenen gezeichnet werden (BG1-4, OBJ)
    #[serde(skip, default = "default_mask")]
    pub layer_mask: u8,
    pub pal: bool,
    /// Kartenaufnahme: zusätzlich nur BG1+BG2 (ohne HUD/Sprites) in `cap` zeichnen
    #[serde(skip)]
    pub capture: bool,
    #[serde(skip, default = "default_fb")]
    pub cap: Vec<u16>,
    /// Breitbild: zusätzliche Pixel links und rechts (0 = 4:3)
    #[serde(skip)]
    pub ext: usize,
    /// Breitbild-Ausgabe ((256 + 2·ext) × 224), nur wenn ext > 0
    #[serde(skip)]
    pub wide: Vec<u16>,
    /// Register je Zeile + Speicher am Bildende festhalten (für Zwischenbilder)
    #[serde(skip)]
    pub record: bool,
    #[serde(skip)]
    lines: Vec<Regs>,
    #[serde(skip)]
    pub snap: Option<std::sync::Arc<Snap>>,
    /// Breitbild: Statusleiste nach außen schieben (nur im normalen Spiel)
    #[serde(skip)]
    pub hud_split: bool,
    /// je VRAM-Wort: BG1-Scroll beim Schreiben (für gültige Breitbildränder)
    #[serde(skip)]
    tmcam: Vec<u32>,
    #[serde(skip)]
    pub ow: Option<std::sync::Arc<OwMap>>,
    /// Bild wird nicht angezeigt (Turbo-Zwischenbilder) -> nicht zeichnen
    #[serde(skip)]
    pub skip: bool,
    /// Art des Breitbildrands (siehe MARGIN_*)
    #[serde(skip)]
    pub margin: u8,
    /// Kreis-Übergang (Spotlight): Fenster über die ganze Breite ziehen, damit der Rand mit aufgeht
    #[serde(skip)]
    pub win_stretch: bool,
    /// Oberwelt-Rand hinter der Gebietsgrenze: EDGE_SEAMLESS, EDGE_RECOMP oder EDGE_NEIGHBOR (beim Gebietswechsel)
    #[serde(skip)]
    pub ow_edge: u8,
    /// Grafikspeicher kurz vor einem Raumwechsel (der alte Raum, für den Breitbildrand während des Scrollens)
    #[serde(skip)]
    pub room_keep: Option<std::sync::Arc<Vec<u16>>>,
    /// Grafikspeicher des Zielraums von einem früheren Besuch (Rand beim Scrollen sofort vollständig)
    #[serde(skip)]
    pub room_next: Option<std::sync::Arc<Vec<u16>>>,
    /// Grafikspeicher der Nachbarräume links/rechts von einem früheren Besuch (Breitbildrand zeigt den Nachbarraum)
    #[serde(skip)]
    pub room_side: Vec<(u16, std::sync::Arc<Vec<u16>>)>,
    /// Nebel-/Blätter-Overlay der Oberwelt (BG1 halb aufaddiert): Farbsumme im Bild (r, g, b, Anzahl)
    #[serde(skip)]
    pub fog_acc: std::sync::Arc<[std::sync::atomic::AtomicU32; 4]>,
    /// Zeilen, deren Register gemerkt, aber noch nicht gezeichnet sind (werden gesammelt parallel gezeichnet)
    #[serde(skip)]
    pend: Option<(usize, usize)>,
    #[serde(skip)]
    line_buf: Vec<u16>,
    /// geschätzter Spotlight-Kreis (Mitte x, Mitte y, Radius, Fenster 0/1) aus dem letzten Bild
    #[serde(skip)]
    pub spot: Option<(f32, f32, f32, u8)>,
}

/// Größte Breite einer Zeile (Ultrawide)
pub const MAXW: usize = 512;
/// (Test) Zeilen sofort zeichnen wie früher – zum Vergleich mit dem gesammelten Zeichnen
#[cfg(test)]
pub static IMMEDIATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
pub static NOWARP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
pub static WARPTEST: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub const MAX_EXT: usize = (MAXW - W) / 2;

/// Ein komplettes Bild zum erneuten Zeichnen (Register je Zeile, VRAM, Paletten, Sprites).
pub struct Snap {
    pub lines: Vec<Regs>,
    pub vram: Vec<u16>,
    pub cgram: Vec<u16>,
    pub oam: Vec<u8>,
    pub hud: bool,
    pub tmcam: Vec<u32>,
    pub ow: Option<std::sync::Arc<OwMap>>,
    pub margin: u8,
    pub stretch: bool,
    pub edge: u8,
    pub keep: Option<std::sync::Arc<Vec<u16>>>,
    pub next: Option<std::sync::Arc<Vec<u16>>>,
    pub side: Vec<(u16, std::sync::Arc<Vec<u16>>)>,
    pub spot: Option<(f32, f32, f32, u8)>,
}

/// Breitbildrand: 0 = schwarz (Menüs, Titel), 1 = Oberwelt (aus der Gebietskarte), 2 = Raum (Hintergrundfarbe)
pub const MARGIN_BLANK: u8 = 0;
pub const MARGIN_WORLD: u8 = 1;
pub const MARGIN_ROOM: u8 = 2;
/// Menü im Spiel: Rand vom letzten Spielbild stehen lassen
pub const MARGIN_KEEP: u8 = 3;

/// Oberwelt-Karte des aktuellen Gebiets (aus dem Spiel-RAM $7E2000 und der Map16-Tabelle der ROM).
/// Damit werden die Breitbildränder von BG2 vollständig und korrekt gezeichnet – unabhängig davon,
/// welche Kacheln das Spiel schon in den Grafikspeicher geladen hat.
pub struct OwMap {
    /// Map16-Indizes, 64 je Zeile (16×16-Pixel-Kacheln)
    pub m16: Vec<u16>,
    /// Map16-Definitionen: je 4 Tilemap-Wörter
    pub tab: std::sync::Arc<Vec<u16>>,
    pub x0: i32,
    pub y0: i32,
    pub size: i32,
    /// volle BG2-Kameraposition am Bildanfang
    pub camx: i32,
    pub camy: i32,
    /// 0 = Lichtwelt, 1 = Schattenwelt
    pub world: u8,
    /// Gebietskarte passt zum Grafikspeicher (sonst nur Zwischenspeicher/Weltkarte)
    pub valid: bool,
    /// Raum statt Oberwelt: sichtbarer Bereich = Kameragrenzen des Raums
    pub room: bool,
    pub bx0: i32,
    pub bx1: i32,
    #[allow(dead_code)]
    pub by0: i32,
    #[allow(dead_code)]
    pub by1: i32,
    /// Raum: welche 16-Pixel-Kachelspalte links von bx0 bzw. rechts ab bx1 wiederholt wird
    /// (normal die äußerste Spalte des Raums; beim Raumwechsel die des alten Raums)
    pub rep0: i32,
    pub rep1: i32,
    /// Raumwechsel: dieser Bereich (Welt-x) kommt aus der Kopie des Grafikspeichers vor dem Wechsel (alter Raum)
    pub keep0: i32,
    pub keep1: i32,
    /// Raumwechsel: dieser Bereich (der neue Raum) kommt aus der Kopie von einem früheren Besuch – sofort vollständig
    pub next0: i32,
    pub next1: i32,
    /// Raum: linke Kante des 512er-Blocks des aktuellen Raums (Nachbarräume liegen links/rechts davon)
    pub blk0: i32,
    /// Raum: Links Welt-x (dunkle Räume: Lichtmaske im Rand von der Link abgewandten Bildkante)
    pub lx: i32,
}

/// Zwischenspeicher aller schon gesehenen Oberwelt-Pixel (BG2, je Welt 4096×4096, Bit 15 = bekannt)
/// und die Weltkarte (1024×1024) als letzte Rückfallebene für die Breitbildränder.
static WCACHE: std::sync::RwLock<Vec<Vec<u16>>> = std::sync::RwLock::new(Vec::new());
static WMAP: std::sync::RwLock<Vec<Vec<u16>>> = std::sync::RwLock::new(Vec::new());
/// Nebel-Overlay je Gebiet (Elterngebiet, 0x8000 | mittlere Farbe; 0 = keiner) – damit ein Gebiet mit Nebel
/// (z. B. Verlorene Wälder) auch vom Nachbargebiet aus im Rand so aussieht wie im Spiel
static FOG: std::sync::RwLock<Vec<u16>> = std::sync::RwLock::new(Vec::new());

/// Gebiete, in denen das Spiel wirklich eine Überlagerung zeigt (Nebel der Verlorenen Wälder, Blätterdach des
/// Skelettwalds, Regen im Moor, Wolken am Todesberg, Hintergrund der Pyramide). Anderswo gelernte „Nebelfarben“
/// sind Fehlmessungen (z. B. rötlich bei 2B/6B) und färbten den Breitbildrand falsch.
pub fn fog_area(id: u8) -> bool {
    // Blätterdach/Wolkenschatten (Verlorene Wälder, Skelettwald, Todesberg) blendet die App aus -> nur Regen und Pyramide
    matches!(id & 0x7F, 0x70 | 0x5B)
}

/// Nebelfarbe eines Gebiets setzen; true, wenn sie neu ist oder sich deutlich geändert hat
pub fn set_fog(id: u8, c: u16) -> bool {
    if !fog_area(id) {
        return false;
    }
    let Ok(mut f) = FOG.write() else { return false };
    if f.len() < 128 {
        f.resize(128, 0);
    }
    let old = f[id as usize & 127];
    let d = |s: u16| ((old >> s) & 31).abs_diff((c >> s) & 31);
    let changed = old & 0x8000 == 0 || d(0) > 2 || d(5) > 2 || d(10) > 2;
    if changed {
        f[id as usize & 127] = 0x8000 | (c & 0x7FFF);
    }
    changed
}

pub fn fog_table() -> Vec<u16> {
    FOG.read().map(|f| f.clone()).unwrap_or_default()
}

pub fn load_fog(t: Vec<u16>) {
    if let Ok(mut f) = FOG.write() {
        // früher fälschlich gelernte Einträge verwerfen
        *f = t.iter().enumerate().map(|(i, &v)| if fog_area(i as u8) { v } else { 0 }).collect();
    }
}
const WDIM: usize = 4096;

/// Nur die gesehenen Oberwelt-Pixel vergessen (anderer Spielstand: Farben/Zustand können anders sein)
pub fn clear_seen_cache() {
    if let Ok(mut c) = WCACHE.write() {
        c.clear();
    }
}

/// Zwischenspeicher leeren (neue ROM).
pub fn clear_world_cache() {
    if let Ok(mut c) = WCACHE.write() {
        c.clear();
    }
    if let Ok(mut m) = WMAP.write() {
        m.clear();
    }
}

/// Weltkarte (1024×1024, 15-Bit-Farben) für eine Welt hinterlegen.
pub fn set_world_map(world: usize, px: Vec<u16>) {
    if let Ok(mut m) = WMAP.write() {
        while m.len() < 2 {
            m.push(Vec::new());
        }
        m[world] = px;
    }
}

pub fn has_world_map(world: usize) -> bool {
    WMAP.read().map(|m| m.get(world).map_or(false, |v| !v.is_empty())).unwrap_or(false)
}


/// Ein Oberwelt-Gebiet zum Zeichnen der Breitbildränder: Gebietskarte (Map16) und seine Grafik.
pub struct AreaData {
    pub x0: i32,
    pub y0: i32,
    pub size: i32,
    pub m16: Vec<u16>,
    /// BG2-Kachelgrafik ab dem Grafikbeginn (0x4000 Wörter)
    pub chr: Vec<u16>,
    pub cgram: Vec<u16>,
}

/// Alle Gebiete (Schlüssel = obere linke Gebiets-ID inkl. Welt-Bit 0x40)
static AREAS: std::sync::RwLock<Vec<Option<std::sync::Arc<AreaData>>>> = std::sync::RwLock::new(Vec::new());
/// Gebiets-ID -> obere linke Gebiets-ID (große Gebiete)
static PARENT: std::sync::RwLock<Vec<u8>> = std::sync::RwLock::new(Vec::new());

pub fn set_area_parents(p: Vec<u8>) {
    if let Ok(mut g) = PARENT.write() {
        *g = p;
    }
}

pub fn set_area(id: u8, a: AreaData) {
    if let Ok(mut g) = AREAS.write() {
        if g.len() < 128 {
            g.resize(128, None);
        }
        g[(id & 0x7F) as usize] = Some(std::sync::Arc::new(a));
    }
    clear_seams();
}


/// (Test) Paletten-Kopie eines gespeicherten Gebiets
#[cfg(test)]
pub fn area_cgram(id: u8) -> Option<Vec<u16>> {
    AREAS.read().ok()?.get((id & 0x7F) as usize)?.as_ref().map(|a| a.cgram.clone())
}

/// Farben eines gespeicherten Gebiets mit dem echten Spiel abgleichen (beim Besuch).
/// Gibt true zurück, wenn sich etwas geändert hat.
pub fn refresh_area_cgram(id: u8, live: &[u16]) -> bool {
    let Ok(mut g) = AREAS.write() else { return false };
    let Some(Some(a)) = g.get_mut((id & 0x7F) as usize) else { return false };
    if a.cgram.len() >= 128 && live.len() >= 128 && a.cgram[..128] != live[..128] {
        let mut b = (**a).clone_data();
        b.cgram[..128].copy_from_slice(&live[..128]);
        *a = std::sync::Arc::new(b);
        drop(g);
        clear_seams();
        return true;
    }
    false
}

/// Karte (Map16) eines gespeicherten Gebiets mit dem echten Spiel abgleichen: geöffnete Türen, geschnittenes Gras,
/// gesprengte Wände stehen nur im Arbeitsspeicher – ohne Abgleich zeigte der Breitbildrand den alten Zustand.
/// Gibt true zurück, wenn sich etwas geändert hat.
pub fn refresh_area_m16(id: u8, live: &[u16]) -> bool {
    let Ok(mut g) = AREAS.write() else { return false };
    let Some(Some(a)) = g.get_mut((id & 0x7F) as usize) else { return false };
    if a.m16.len() == live.len() && a.m16[..] != live[..] {
        let mut b = (**a).clone_data();
        b.m16.copy_from_slice(live);
        *a = std::sync::Arc::new(b);
        drop(g);
        clear_seams();
        return true;
    }
    false
}

impl AreaData {
    fn clone_data(&self) -> AreaData {
        AreaData { x0: self.x0, y0: self.y0, size: self.size, m16: self.m16.clone(), chr: self.chr.clone(), cgram: self.cgram.clone() }
    }
}

/// Alle gespeicherten Gebiete (zum Sichern)
pub fn all_areas() -> Vec<(u8, Vec<u16>, Vec<u16>, Vec<u16>)> {
    AREAS.read().map(|g| g.iter().enumerate().filter_map(|(i, a)| a.as_ref().map(|a| (i as u8, a.m16.clone(), a.chr.clone(), a.cgram.clone()))).collect()).unwrap_or_default()
}

pub fn clear_areas() {
    if let Ok(mut g) = AREAS.write() {
        g.clear();
    }
    clear_seams();
}

/// (Test) Versatz-Abschnitte an einer Gebietsgrenze
#[cfg(test)]
pub fn warp_runs_test(tab: &std::sync::Arc<Vec<u16>>, world: u8, bx: i32, wy: i32) -> Vec<(i32, i32, i32, i32, i32)> {
    let a = AREAS.read().unwrap();
    let p = PARENT.read().unwrap();
    warp_runs(&a, &p, tab, world, bx, wy).warp.iter().map(|w| (w.r0, w.r1, w.d, w.a, w.b)).collect()
}

/// (Test) Pixel aus den gespeicherten Gebieten
#[cfg(test)]
pub fn area_px_test(tab: &[u16], world: u8, wx: i32, wy: i32) -> Option<u16> {
    let a = AREAS.read().ok()?;
    let p = PARENT.read().ok()?;
    area_pixel(&a, &p, tab, world, wx, wy)
}

/// Passen zwei Gebiete an einer senkrechten Grenze zusammen? Je Gebietsabschnitt (Welt, Grenze x, y/512).
/// Ganzer Abschnitt statt Stücke, damit Nachbar und Spiegelung nicht übereinander springen.
/// Manche Gebiete grenzen an Klippen, die im Spiel nie nebeneinander zu sehen sind und nicht zueinander passen.
static SEAM: std::sync::RwLock<Option<std::collections::HashMap<(u8, i32, i32), bool>>> = std::sync::RwLock::new(None);

/// Versatz an einer Grenze: Manche Nachbargebiete passen fast, aber eine Kante (z. B. ein Ufer) liegt ein paar
/// Zeilen höher oder tiefer als im eigenen Gebiet. Je Stelle (Zeilen r0..=r1) der Versatz d, um den das
/// Nachbargebiet an der Grenze verschoben werden muss. Schlüssel: (Welt, Grenze x, y/512).
/// Ergebnis mit dem Stand (WARP_GEN), für den es berechnet wurde – ältere werden weiter benutzt, bis das neue fertig ist
static WARP: std::sync::RwLock<Option<std::collections::HashMap<(u8, i32, i32), (u32, std::sync::Arc<Warps>)>>> = std::sync::RwLock::new(None);
static WARP_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// gerade im Hintergrund berechnete Stellen
static WARP_PENDING: std::sync::Mutex<Vec<(u8, i32, i32)>> = std::sync::Mutex::new(Vec::new());

/// Abschnitt r0..=r1 mit Versatz d. Für die Zeilen a..=b ist Zeile für Zeile zugeordnet, welche Zeile des Nachbarn
/// an der Grenze passt (Wasser zu Wasser, Ufer zu Ufer, Klippe zu Klippe, Wiese zu Wiese): mr für den rechten Rand
/// (eigene Zeilen links -> Zeilen rechts), ml für den linken. Eine dünnere Klippe im Nachbarn wird so an der Grenze
/// passend breiter, nichts erscheint doppelt und es bleibt keine Lücke
#[cfg_attr(not(test), allow(dead_code))]
pub struct WarpRun {
    pub r0: i32,
    pub r1: i32,
    pub d: i32,
    pub a: i32,
    pub b: i32,
    mr: Vec<f32>,
    ml: Vec<f32>,
}
/// Wiederholung des eigenen Gebiets über die Grenze: Zeilen y0..=y1, Seite (true = rechter Rand), Muster-Breite p,
/// bis c Pixel hinter der Grenze. Oder (c = 0) für einen Absatz unter Bäumen, den es im Nachbargebiet nicht gibt: dort
/// steht an der Grenze eine Baumkrone, deren linke Hälfte die Gebietsgrenze abschneidet. Sie bekommt ihren runden Rand
/// (Ellipse: Mitte x/y, Halbachsen, Umriss-Farbe) und daneben läuft der Absatz weiter – er verschwindet hinter dem Baum
pub type RepRun = (i32, i32, bool, i32, i32, [f32; 4], u16);
pub struct Warps {
    pub warp: Vec<WarpRun>,
    pub rep: Vec<RepRun>,
}

fn clear_seams() {
    if let Ok(mut g) = SEAM.write() {
        *g = None;
    }
    // Versatz-Ergebnisse nicht wegwerfen: sie gelten als veraltet und werden im Hintergrund neu berechnet
    WARP_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Stellen an einer Grenze, an denen das Nachbargebiet versetzt anschließt, mit passendem Versatz
/// Die Berechnung dauert einige Millisekunden – sie läuft im Hintergrund, damit das Bild nie darauf wartet (sonst hakt es
/// kurz beim Laden eines Spielstands oder Betreten eines Gebiets). Bis sie fertig ist: das letzte Ergebnis bzw. keins
fn warp_runs(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &std::sync::Arc<Vec<u16>>, world: u8, bx: i32, wy: i32) -> std::sync::Arc<Warps> {
    let key = (world, bx, wy.div_euclid(512));
    let gen = WARP_GEN.load(std::sync::atomic::Ordering::Relaxed);
    let cached = WARP.read().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&key).cloned()));
    if let Some((g, v)) = &cached {
        if *g == gen {
            return v.clone();
        }
    }
    #[cfg(test)]
    {
        let v = std::sync::Arc::new(compute_warps(areas, parents, tab, world, bx, key.2));
        if let Ok(mut g) = WARP.write() {
            g.get_or_insert_with(Default::default).insert(key, (gen, v.clone()));
        }
        return v;
    }
    #[cfg(not(test))]
    {
        let _ = (areas, parents);
        let start = WARP_PENDING.lock().map(|mut p| {
            if p.contains(&key) {
                false
            } else {
                p.push(key);
                true
            }
        }).unwrap_or(false);
        if start {
            let tab = tab.clone();
            let spawned = std::thread::Builder::new().name("seams".into()).spawn(move || {
                let gen = WARP_GEN.load(std::sync::atomic::Ordering::Relaxed);
                let v = {
                    let a = AREAS.read().map(|g| g.clone()).unwrap_or_default();
                    let p = PARENT.read().map(|g| g.clone()).unwrap_or_default();
                    compute_warps(&a, &p, &tab, world, bx, key.2)
                };
                if let Ok(mut g) = WARP.write() {
                    g.get_or_insert_with(Default::default).insert(key, (gen, std::sync::Arc::new(v)));
                }
                if let Ok(mut p) = WARP_PENDING.lock() {
                    p.retain(|k| *k != key);
                }
            });
            if spawned.is_err() {
                if let Ok(mut p) = WARP_PENDING.lock() {
                    p.retain(|k| *k != key);
                }
            }
        }
        cached.map(|c| c.1).unwrap_or_else(|| std::sync::Arc::new(Warps { warp: Vec::new(), rep: Vec::new() }))
    }
}

fn compute_warps(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &[u16], world: u8, bx: i32, sy: i32) -> Warps {
    let key = (world, bx, sy);
    let px = |x: i32, y: i32| area_pixel(areas, parents, tab, world, x, y);
    // passt Zeile r des Nachbarn (links bei bx-1) zu Zeile q des eigenen Gebiets (bei bx)? (±1 Spalte Spielraum)
    let fits = |r: i32, q: i32| match (px(bx - 1, r), px(bx, q)) {
        (Some(a), Some(b)) => a == b || px(bx - 2, r) == Some(b) || px(bx + 1, q) == Some(a),
        _ => true,
    };
    let y0 = key.2 * 512;
    let mut runs = Vec::new();
    let mut reps = Vec::new();
    let mut y = y0;
    while y < y0 + 512 {
        if fits(y, y) {
            y += 1;
            continue;
        }
        // zusammenhängende Abweichung (kleine Lücken bis 2 Zeilen zählen mit)
        let r0 = y;
        let mut r1 = y;
        let mut gap = 0;
        y += 1;
        while y < y0 + 512 && gap <= 2 {
            if fits(y, y) {
                gap += 1;
            } else {
                gap = 0;
                r1 = y;
            }
            y += 1;
        }
        // nur längere Abweichungen ausgleichen (z. B. Schlossgraben); kurze sind kleine Unebenheiten wie
        // Wege oder Grasränder – dort würde das Verschieben Kerben und abgeschnittene Stellen erzeugen
        if r1 - r0 + 1 >= 24 {
            // Versatz suchen, bei dem die Grenzspalten am besten zusammenpassen
            let cost = |d: i32| (r0..=r1).filter(|&r| !fits(r - d, r)).count();
            let base = cost(0);
            let best = (-40..=40).map(|d| (cost(d), d.abs(), d)).min().unwrap();
            // nur echte versetzte Kanten: nach dem Verschieben müssen fast alle Zeilen passen (mind. 80 %).
            // Sich wiederholende Muster wie Baumkronen passen zufällig halb – verschoben würden sie nur verzerrt
            if best.2 != 0 && best.0 * 2 < base && (best.0 * 5 <= (r1 - r0 + 1) as usize || warp_loose()) {
                let d = best.2;
                // Zeilen beider Grenzspalten einander zuordnen (kleinste Abweichung, Reihenfolge bleibt; gerade
                // Zuordnung bevorzugt). Anfang und Ende passen ohne Versatz
                let a = r0.min(r0 - d) - 24;
                let b = r1.max(r1 - d) + 24;
                let n = (b - a + 1) as usize;
                let inf = f32::MAX / 4.0;
                // ähnliche Farben (z. B. zwei Brauntöne einer Klippe) kosten wenig, ganz andere viel
                let cdist = |x: u16, y: u16| (0..3).map(|k| (((x >> (k * 5)) & 31) as i32 - ((y >> (k * 5)) & 31) as i32).abs()).sum::<i32>() as f32 / 40.0;
                let near = |i: i32, j: i32| match (px(bx - 1, i), px(bx, j)) {
                    (Some(x), Some(y)) => {
                        let mut m = cdist(x, y);
                        for (x2, y2) in [(px(bx - 2, i), Some(y)), (Some(x), px(bx + 1, j))] {
                            if let (Some(x2), Some(y2)) = (x2, y2) {
                                m = m.min(cdist(x2, y2));
                            }
                        }
                        m.min(1.0)
                    }
                    _ => 0.0,
                };
                let mut cost = vec![inf; n * n];
                let mut from = vec![0u8; n * n];
                for i in 0..n {
                    for j in 0..n {
                        if (i as i32 - j as i32).abs() > 48 {
                            continue;
                        }
                        let c = if fits(a + i as i32, a + j as i32) { 0.0 } else { near(a + i as i32, a + j as i32) };
                        if i == 0 && j == 0 {
                            cost[0] = c;
                            continue;
                        }
                        let mut m = (inf, 0u8);
                        if i > 0 && j > 0 && cost[(i - 1) * n + j - 1] < m.0 {
                            m = (cost[(i - 1) * n + j - 1], 0);
                        }
                        if i > 0 && cost[(i - 1) * n + j] + 0.3 < m.0 {
                            m = (cost[(i - 1) * n + j] + 0.3, 1);
                        }
                        if j > 0 && cost[i * n + j - 1] + 0.3 < m.0 {
                            m = (cost[i * n + j - 1] + 0.3, 2);
                        }
                        cost[i * n + j] = m.0 + c;
                        from[i * n + j] = m.1;
                    }
                }
                // Weg zurückverfolgen; je Zeile Mittel der zugeordneten Zeilen
                let (mut sr, mut cr, mut sl, mut cl) = (vec![0.0f32; n], vec![0u32; n], vec![0.0f32; n], vec![0u32; n]);
                let (mut i, mut j) = (n - 1, n - 1);
                loop {
                    sr[i] += j as f32;
                    cr[i] += 1;
                    sl[j] += i as f32;
                    cl[j] += 1;
                    if i == 0 && j == 0 {
                        break;
                    }
                    match from[i * n + j] {
                        0 => {
                            i -= 1;
                            j -= 1;
                        }
                        1 => i -= 1,
                        _ => j -= 1,
                    }
                }
                let mr = (0..n).map(|k| a as f32 + sr[k] / cr[k].max(1) as f32).collect();
                let ml = (0..n).map(|k| a as f32 + sl[k] / cl[k].max(1) as f32).collect();
                runs.push(WarpRun { r0, r1, d, a, b, mr, ml });
                continue;
            }
            // kein passender Versatz: wiederholt sich das eigene Gebiet waagrecht (Wald), dort weiterlaufen lassen
            for right in [true, false] {
                // own(t): Pixel des eigenen Gebiets t Pixel vor der Grenze, nb(t): Nachbar t Pixel hinter der Grenze
                let own = |t: i32, r: i32| if right { px(bx - 1 - t, r) } else { px(bx + t, r) };
                let nb = |t: i32, r: i32| if right { px(bx + t, r) } else { px(bx - 1 - t, r) };
                // nur wo auch der Nachbar dort bloß ein sich wiederholendes Muster zeigt (Baumkronen) – echte Dinge
                // wie Häuser oder Wege im Nachbargebiet werden nie überdeckt
                let col_ok = |q: i32, t: i32| {
                    let n = (r0..=r1).count();
                    let eq = (r0..=r1).filter(|&r| nb(t, r) == nb(t + q, r)).count();
                    eq * 100 >= n * 80
                };
                let Some(q) = (16..=64).step_by(8).max_by_key(|&q| (0..64).filter(|&t| col_ok(q, t)).count()) else {
                    continue;
                };
                // so weit der Nachbar gleichförmig bleibt (8 abweichende Spalten am Stück beenden es)
                let mut plain = 0;
                let mut bad = 0;
                for t in 0..512 {
                    if col_ok(q, t) {
                        bad = 0;
                        plain = t + 1;
                    } else {
                        bad += 1;
                        if bad >= 8 {
                            break;
                        }
                    }
                }
                if plain < 16 {
                    continue;
                }
                // Breite des eigenen Musters (nahe der Grenze); ohne Wiederholung bleibt es beim Nachbarn
                let Some(p) = (16..=64).step_by(8).find(|&p| {
                    let (mut eq, mut n) = (0, 0);
                    for r in r0..=r1 {
                        for t in 0..p.min(16) {
                            n += 1;
                            if own(t, r) == own(t + p, r) {
                                eq += 1;
                            }
                        }
                    }
                    eq * 10 >= n * 9
                }) else {
                    continue;
                };
                // Übergang c: fortgesetztes Muster (Pixel bei c-1) neben dem Nachbarn (bei c) – am wenigsten Bruch
                let ext = |t: i32, r: i32| own(p - 1 - t, r);
                let (cost, c) = (0..=p).map(|c| ((r0..=r1).filter(|&r| c > 0 && ext(c - 1, r) != nb(c, r) || c == 0 && own(0, r) != nb(0, r)).count(), c)).min().unwrap();
                let rows = (r0..=r1).count();
                if cost * 4 <= rows {
                    // sauberer Übergang innerhalb einer Musterbreite
                    let c = c.min(plain);
                    if c > 0 {
                        reps.push((r0, r1, right, p, c, [0.0; 4], 0));
                    }
                    continue;
                }
                // Absatz, den es im Nachbarn nicht gibt: an der Grenze steht dort eine Baumkrone des Nachbarn, deren linke
                // Hälfte von der Gebietsgrenze abgeschnitten ist. Kronen sind symmetrisch – Mitte und Größe aus der Krone
                // selbst ablesen, ihr den runden Rand geben und daneben den Absatz weiterlaufen lassen
                let lum = |c: u16| (c & 31) as u32 * 2 + ((c >> 5) & 31) as u32 * 4 + ((c >> 10) & 31) as u32;
                let Some(col) = (r0 - 40..=r1 + 40).flat_map(|r| (0..48).filter_map(move |t| nb(t, r))).min_by_key(|&c| lum(c)) else {
                    continue;
                };
                // senkrechte Spiegelachse der Krone (doppelt gezählt, damit auch zwischen zwei Pixeln möglich)
                let mut best_axis: Option<(f32, i32)> = None;
                for a2 in 16..=96 {
                    let (mut eq, mut n) = (0, 0);
                    for r in (r0..=r1).step_by(2) {
                        for t in (a2 - 1 - 40).max(0)..a2 / 2 {
                            n += 1;
                            if nb(t, r) == nb(a2 - 1 - t, r) {
                                eq += 1;
                            }
                        }
                    }
                    if n > 0 {
                        let sc = eq as f32 / n as f32;
                        if best_axis.map_or(true, |b| sc > b.0 + 0.001) {
                            best_axis = Some((sc, a2));
                        }
                    }
                }
                let Some((sc, a2)) = best_axis else {
                    continue;
                };
                if sc < 0.8 {
                    continue;
                }
                let ax = a2 as f32 / 2.0;
                // oberer und unterer Umriss der Krone in der Mittelspalte
                let tc = (a2 / 2).max(0);
                let mid = (r0 + r1) / 2;
                // nur eine echte Krone: Umriss oben und unten gefunden, sinnvolle Größe, nicht einfarbig (Wasser oder
                // Wiese sind auch "symmetrisch")
                let (Some(top), Some(bot)) = ((1..60).map(|k| mid - k).find(|&r| nb(tc, r) == Some(col)), (1..60).map(|k| mid + k).find(|&r| nb(tc, r) == Some(col))) else {
                    continue;
                };
                let mut cols: Vec<u16> = (top..=bot).filter_map(|r| nb(tc, r)).collect();
                cols.sort_unstable();
                cols.dedup();
                // Baumkronen sind etwa rund
                if ax < 16.0 || bot - top < 32 || cols.len() < 3 || ((bot - top) as f32 / 2.0 - ax).abs() > 6.0 {
                    continue;
                }
                let ell = [ax, (top + bot) as f32 / 2.0, ax.max(8.0), ((bot - top) as f32 / 2.0).max(8.0)];
                #[cfg(test)]
                if NOWARP.load(std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("REP W{world} x={bx} y {r0}..{r1} rechts={right} p={p} Krone {ell:?} Symmetrie {sc:.2}");
                }
                reps.push((r0, r1, right, p, 0, ell, col));
            }
        }
    }
    Warps { warp: runs, rep: reps }
}

#[cfg(test)]
fn warp_loose() -> bool {
    WARPTEST.load(std::sync::atomic::Ordering::Relaxed) & 1 != 0
}
#[cfg(not(test))]
fn warp_loose() -> bool {
    false
}
#[cfg(test)]
fn warp_fade() -> i32 {
    match WARPTEST.load(std::sync::atomic::Ordering::Relaxed) >> 1 { 0 => 48, v => v }
}
#[cfg(not(test))]
fn warp_fade() -> i32 {
    48
}

/// Versatz für eine Bildzeile: nur im gewählten Streifen (kein Auslaufen in die Zeilen darüber oder darunter –
/// sonst würden dort z. B. Bäume verzogen)
fn warp_at(runs: &[WarpRun], wy: i32, right: bool) -> f32 {
    #[cfg(test)]
    if NOWARP.load(std::sync::atomic::Ordering::Relaxed) {
        return 0.0;
    }
    let mut best = 0.0f32;
    for w in runs {
        if wy < w.a || wy > w.b {
            continue;
        }
        let m = if right { &w.mr } else { &w.ml };
        let off = m[(wy - w.a) as usize] - wy as f32;
        if off.abs() > best.abs() {
            best = off;
        }
    }
    best
}

fn seam_ok(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &[u16], world: u8, bx: i32, wy: i32) -> bool {
    let key = (world, bx, wy.div_euclid(512));
    // Stellen, an denen trotz Abweichung immer das echte Nachbargebiet gezeigt wird statt der Spiegelung
    // (Kakariko, Ostrand beim Brunnen: Weg, Zaun und Bäume laufen dort sichtbar weiter)
    const ALWAYS_NEIGHBOR: [(u8, i32, i32); 1] = [(0, 1024, 4)];
    if ALWAYS_NEIGHBOR.contains(&key) {
        return true;
    }
    if let Some(v) = SEAM.read().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&key).copied())) {
        return v;
    }
    let (mut eq, mut n) = (0, 0);
    for y in key.2 * 512..key.2 * 512 + 512 {
        let px = |x: i32| area_pixel(areas, parents, tab, world, x, y);
        if let (Some(a), Some(b)) = (px(bx - 1), px(bx)) {
            n += 1;
            // gleiche Farbe direkt an der Grenze (oder einen Pixel versetzt)
            if a == b || px(bx - 2) == Some(b) || px(bx + 1) == Some(a) {
                eq += 1;
            }
        }
    }
    // ohne Daten: wie bisher Nachbarn zeigen
    let ok = n < 64 || eq as f32 / n as f32 >= 0.75;
    if let Ok(mut g) = SEAM.write() {
        g.get_or_insert_with(Default::default).insert(key, ok);
    }
    ok
}

/// Pixel aus einem geladenen Nachbargebiet (Weltposition), mit dessen eigener Grafik.
fn area_pixel(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &[u16], world: u8, wx: i32, wy: i32) -> Option<u16> {
    area_pixel_live(areas, parents, tab, world, wx, wy, None)
}

/// Haben zwei gespeicherte Gebiete dieselbe Landschafts-Palette? (letztes Ergebnis je Thread gemerkt)
fn same_pal(a: &AreaData, b: &AreaData) -> bool {
    thread_local!(static LAST: std::cell::Cell<(usize, usize, bool)> = const { std::cell::Cell::new((0, 0, false)) });
    let k = (a as *const AreaData as usize, b as *const AreaData as usize);
    LAST.with(|c| {
        let v = c.get();
        if (v.0, v.1) == k {
            return v.2;
        }
        let r = a.cgram.len() >= 128 && b.cgram.len() >= 128 && a.cgram[32..128] == b.cgram[32..128];
        c.set((k.0, k.1, r));
        r
    })
}

/// wie `area_pixel`; `live` = (gezeigtes Gebiet, echte Palette): nutzt das Nachbargebiet dieselbe Palette wie das
/// gezeigte, kommen die Farben aus dem echten Spiel (gespeicherte Farben können veraltet sein – sonst passt der Rand
/// farblich nicht, z. B. rotstichig)
fn area_pixel_live(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &[u16], world: u8, wx: i32, wy: i32, live: Option<(usize, &[u16])>) -> Option<u16> {
    area_pixel_ex(areas, parents, tab, world, wx, wy, live, false)
}

/// wie `area_pixel_live`; `solid`: durchsichtige Kachelpixel (dort liegt im Spiel z. B. Himmel/Wolken auf einer
/// anderen Ebene) als None statt der Hintergrundfarbe
fn area_pixel_ex(areas: &[Option<std::sync::Arc<AreaData>>], parents: &[u8], tab: &[u16], world: u8, wx: i32, wy: i32, live: Option<(usize, &[u16])>, solid: bool) -> Option<u16> {
    let id = (world as i32 * 0x40 + (wy / 512) * 8 + wx / 512) as usize;
    let p = *parents.get(id)? as usize;
    let a = areas.get(p)?.as_ref()?;
    let pal: &[u16] = match live {
        Some((cur, cg)) if cg.len() >= 128 && (cur == p || areas.get(cur).and_then(|c| c.as_ref()).map_or(false, |c| same_pal(c, a))) => cg,
        _ => &a.cgram,
    };
    let (lx, ly) = (wx - a.x0, wy - a.y0);
    if lx < 0 || ly < 0 || lx >= a.size || ly >= a.size {
        return None;
    }
    let m = *a.m16.get(((ly >> 4) * 64 + (lx >> 4)) as usize)? as usize;
    let q = (((ly >> 3) & 1) * 2 + ((lx >> 3) & 1)) as usize;
    let e = *tab.get(m * 4 + q)? as u32;
    let (mut px, mut py) = ((lx & 7) as u32, (ly & 7) as u32);
    if e & 0x4000 != 0 {
        px = 7 - px;
    }
    if e & 0x8000 != 0 {
        py = 7 - py;
    }
    let base = (e & 0x3FF) * 16 + py;
    let bit = 7 - px;
    let w0 = *a.chr.get((base & 0x3FFF) as usize)? as u32;
    let w1 = *a.chr.get(((base + 8) & 0x3FFF) as usize)? as u32;
    let c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1) | (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
    if c == 0 {
        return if solid { None } else { pal.first().copied() };
    }
    pal.get(((((e >> 10) & 7) * 16 + c) & 0xFF) as usize).copied()
}

/// Weltkarte aus den gespeicherten Gebieten (echte Kacheln, maßstabsgetreu): 1024×1024, je Kartenpixel der
/// Mittelwert von 4 Weltpixeln; None, wo kein Gebiet gespeichert ist (dort zeigt die Karte das gezeichnete Bild
/// des Spiels). Durchsichtige Kachelpixel (Himmel am Todesberg) werden helles Wolkenweiß statt Schwarz
pub fn world_from_areas(tab: &[u16], world: u8) -> Vec<Option<u16>> {
    let areas = AREAS.read().map(|a| a.clone()).unwrap_or_default();
    let parents = PARENT.read().map(|p| p.clone()).unwrap_or_default();
    let mut out = vec![None; 1024 * 1024];
    for my in 0..1024i32 {
        for mx in 0..1024i32 {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for (dx, dy) in [(1, 1), (3, 1), (1, 3), (3, 3)] {
                let (x, y) = (mx * 4 + dx, my * 4 + dy);
                // durchsichtig (Himmel/Wolken auf einer anderen Ebene, z. B. am Todesberg): in der Lichtwelt helles
                // Wolkenweiß; in der Schattenwelt (dunkler Himmel, Pyramide) das gezeichnete Kartenbild des Spiels
                let c = area_pixel_ex(&areas, &parents, tab, world, x, y, None, true).or_else(|| if world == 0 { area_pixel(&areas, &parents, tab, world, x, y).map(|_| 29 | 30 << 5 | 31 << 10) } else { None });
                if let Some(c) = c {
                    r += (c & 31) as u32;
                    g += ((c >> 5) & 31) as u32;
                    b += ((c >> 10) & 31) as u32;
                    n += 1;
                }
            }
            if n == 4 {
                out[(my * 1024 + mx) as usize] = Some(((r / 4) | (g / 4) << 5 | (b / 4) << 10) as u16);
            }
        }
    }
    out
}

/// Pixel für eine Weltposition außerhalb des geladenen Gebiets: gesehen > Weltkarte > nichts
fn world_pixel(cache: &[Vec<u16>], map: &[Vec<u16>], world: u8, wx: i32, wy: i32) -> Option<u16> {
    if wx < 0 || wy < 0 || wx >= WDIM as i32 || wy >= WDIM as i32 {
        return None;
    }
    if let Some(c) = cache.get(world as usize).filter(|c| !c.is_empty()) {
        let v = c[wy as usize * WDIM + wx as usize];
        if v & 0x8000 != 0 {
            return Some(v & 0x7FFF);
        }
    }
    map.get(world as usize).filter(|m| m.len() == 1024 * 1024).map(|m| m[(wy as usize >> 2) * 1024 + (wx as usize >> 2)])
}

/// Breitbild: Statusleiste an dieser Stelle teilen (links Items, rechts Energie)
const HUD_SPLIT: i32 = 157;
/// Zeilen der Statusleiste
const HUD_LINES: u16 = 62;
/// Kamera-Merker je VRAM-Wort: bekannt
const CAM_KNOWN: u32 = 1 << 31;

#[inline]
fn sext13(v: u16) -> i16 {
    ((v << 3) as i16) >> 3
}

#[inline]
fn cond(sel: u8, inside: bool) -> bool {
    match sel & 3 {
        0 => false,
        1 => !inside,
        2 => inside,
        _ => true,
    }
}

fn color_math(a: u16, b: u16, sub: bool, half: bool) -> u16 {
    let mut out = 0u16;
    for sh in [0u16, 5, 10] {
        let x = ((a >> sh) & 31) as i32;
        let y = ((b >> sh) & 31) as i32;
        let mut r = if sub { x - y } else { x + y };
        if half {
            r >>= 1;
        }
        out |= (r.clamp(0, 31) as u16) << sh;
    }
    out
}

#[inline]
fn direct_color(c: u32, pal: u32) -> u16 {
    let r = ((c & 0x07) << 2) | ((pal & 1) << 1);
    let g = ((c & 0x38) >> 1) | (pal & 2);
    let b = ((c & 0xC0) >> 3) | (pal & 4);
    (r | (g << 5) | (b << 10)) as u16
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            vram: vec![0; 0x8000],
            cgram: vec![0; 256],
            oam: vec![0; 544],
            r: Regs { inidisp: 0x80, ..Default::default() },
            fb: default_fb(),
            layer_mask: default_mask(),
            cap: default_fb(),
            ..Default::default()
        }
    }

    fn vram_remap(&self) -> usize {
        let a = self.r.vaddr;
        let r = match (self.r.vmain >> 2) & 3 {
            0 => a,
            1 => (a & 0xFF00) | ((a & 0xE0) >> 5) | ((a & 0x1F) << 3),
            2 => (a & 0xFE00) | ((a & 0x1C0) >> 6) | ((a & 0x3F) << 3),
            _ => (a & 0xFC00) | ((a & 0x380) >> 7) | ((a & 0x7F) << 3),
        };
        (r & 0x7FFF) as usize
    }
    fn vinc(&self) -> u16 {
        [1, 32, 128, 128][(self.r.vmain & 3) as usize]
    }
    fn prefetch(&mut self) {
        self.r.vread = self.vram[self.vram_remap()];
    }

    fn write_hofs(&mut self, n: usize, v: u8) {
        self.r.hofs[n] = (((v as u16) << 8) | (self.r.ofs_latch1 as u16 & !7) | (self.r.ofs_latch2 as u16 & 7)) & 0x3FF;
        self.r.ofs_latch1 = v;
        self.r.ofs_latch2 = v;
    }
    fn write_vofs(&mut self, n: usize, v: u8) {
        self.r.vofs[n] = (((v as u16) << 8) | self.r.ofs_latch1 as u16) & 0x3FF;
        self.r.ofs_latch1 = v;
    }
    fn m7_word(&mut self, v: u8) -> u16 {
        let w = ((v as u16) << 8) | self.r.m7_latch as u16;
        self.r.m7_latch = v;
        w
    }

    fn oam_write(&mut self, v: u8) {
        self.flush_lines();
        let a = self.r.oam_addr as usize;
        if a & 1 == 0 {
            self.r.oam_latch = v;
        }
        if a < 0x200 {
            if a & 1 == 1 {
                self.oam[a - 1] = self.r.oam_latch;
                self.oam[a] = v;
            }
        } else {
            self.oam[0x200 + (a & 0x1F)] = v;
        }
        self.r.oam_addr = (self.r.oam_addr + 1) & 0x3FF;
    }

    pub fn mode(&self) -> u8 {
        self.r.bgmode & 7
    }

    pub fn brightness(&self) -> u8 {
        if self.r.inidisp & 0x80 != 0 {
            0
        } else {
            self.r.inidisp & 0x0F
        }
    }

    pub fn write(&mut self, a: u8, v: u8) {
        match a {
            0x00 => self.r.inidisp = v,
            0x01 => self.r.obsel = v,
            0x02 => {
                self.r.oam_reload = (self.r.oam_reload & 0x200) | ((v as u16) << 1);
                self.r.oam_addr = self.r.oam_reload;
            }
            0x03 => {
                self.r.oam_reload = ((v as u16 & 1) << 9) | (self.r.oam_reload & 0x1FE);
                self.r.oam_prio = v & 0x80 != 0;
                self.r.oam_addr = self.r.oam_reload;
            }
            0x04 => self.oam_write(v),
            0x05 => self.r.bgmode = v,
            0x06 => self.r.mosaic = v,
            0x07..=0x0A => self.r.bg_sc[(a - 0x07) as usize] = v,
            0x0B => {
                self.r.bg_nba[0] = ((v & 0x0F) as u16) << 12;
                self.r.bg_nba[1] = ((v >> 4) as u16) << 12;
            }
            0x0C => {
                self.r.bg_nba[2] = ((v & 0x0F) as u16) << 12;
                self.r.bg_nba[3] = ((v >> 4) as u16) << 12;
            }
            0x0D => {
                let w = self.m7_word(v);
                self.r.m7hofs = sext13(w);
                self.write_hofs(0, v);
            }
            0x0E => {
                let w = self.m7_word(v);
                self.r.m7vofs = sext13(w);
                self.write_vofs(0, v);
            }
            0x0F | 0x11 | 0x13 => self.write_hofs(((a - 0x0D) / 2) as usize, v),
            0x10 | 0x12 | 0x14 => self.write_vofs(((a - 0x0E) / 2) as usize, v),
            0x15 => self.r.vmain = v,
            0x16 => {
                self.r.vaddr = (self.r.vaddr & 0xFF00) | v as u16;
                self.prefetch();
            }
            0x17 => {
                self.r.vaddr = (self.r.vaddr & 0x00FF) | (v as u16) << 8;
                self.prefetch();
            }
            0x18 => {
                self.flush_lines();
                let i = self.vram_remap();
                self.mark_cam(i);
                self.vram[i] = (self.vram[i] & 0xFF00) | v as u16;
                if self.r.vmain & 0x80 == 0 {
                    self.r.vaddr = self.r.vaddr.wrapping_add(self.vinc());
                }
            }
            0x19 => {
                self.flush_lines();
                let i = self.vram_remap();
                self.mark_cam(i);
                self.vram[i] = (self.vram[i] & 0x00FF) | (v as u16) << 8;
                if self.r.vmain & 0x80 != 0 {
                    self.r.vaddr = self.r.vaddr.wrapping_add(self.vinc());
                }
            }
            0x1A => self.r.m7sel = v,
            0x1B => self.r.m7a = self.m7_word(v) as i16,
            0x1C => self.r.m7b = self.m7_word(v) as i16,
            0x1D => self.r.m7c = self.m7_word(v) as i16,
            0x1E => self.r.m7d = self.m7_word(v) as i16,
            0x1F => {
                let w = self.m7_word(v);
                self.r.m7x = sext13(w)
            }
            0x20 => {
                let w = self.m7_word(v);
                self.r.m7y = sext13(w)
            }
            0x21 => self.r.cg_addr = (v as u16) << 1,
            0x22 => {
                if self.r.cg_addr & 1 == 0 {
                    self.r.cg_latch = v;
                } else {
                    self.flush_lines();
                    self.cgram[(self.r.cg_addr >> 1) as usize] = (((v & 0x7F) as u16) << 8) | self.r.cg_latch as u16;
                }
                self.r.cg_addr = (self.r.cg_addr + 1) & 0x1FF;
            }
            0x23 => self.r.w12sel = v,
            0x24 => self.r.w34sel = v,
            0x25 => self.r.wobjsel = v,
            0x26..=0x29 => self.r.wh[(a - 0x26) as usize] = v,
            0x2A => self.r.wbglog = v,
            0x2B => self.r.wobjlog = v,
            0x2C => self.r.tm = v,
            0x2D => self.r.ts = v,
            0x2E => self.r.tmw = v,
            0x2F => self.r.tsw = v,
            0x30 => self.r.cgwsel = v,
            0x31 => self.r.cgadsub = v,
            0x32 => {
                let val = v & 0x1F;
                if v & 0x20 != 0 {
                    self.r.fixed[0] = val
                }
                if v & 0x40 != 0 {
                    self.r.fixed[1] = val
                }
                if v & 0x80 != 0 {
                    self.r.fixed[2] = val
                }
            }
            0x33 => self.r.setini = v,
            _ => {}
        }
    }

    pub fn read(&mut self, a: u8, h: u16, line: u16, open: u8) -> u8 {
        let mpy = self.r.m7a as i32 * ((self.r.m7b >> 8) as i8 as i32);
        match a {
            0x34 => mpy as u8,
            0x35 => (mpy >> 8) as u8,
            0x36 => (mpy >> 16) as u8,
            0x37 => {
                self.r.hlatch = h;
                self.r.vlatch = line;
                self.r.latched = true;
                open
            }
            0x38 => {
                let a = self.r.oam_addr as usize;
                let v = if a < 0x200 { self.oam[a] } else { self.oam[0x200 + (a & 0x1F)] };
                self.r.oam_addr = (self.r.oam_addr + 1) & 0x3FF;
                v
            }
            0x39 => {
                let v = self.r.vread as u8;
                if self.r.vmain & 0x80 == 0 {
                    self.prefetch();
                    self.r.vaddr = self.r.vaddr.wrapping_add(self.vinc());
                }
                v
            }
            0x3A => {
                let v = (self.r.vread >> 8) as u8;
                if self.r.vmain & 0x80 != 0 {
                    self.prefetch();
                    self.r.vaddr = self.r.vaddr.wrapping_add(self.vinc());
                }
                v
            }
            0x3B => {
                let w = self.cgram[(self.r.cg_addr >> 1) as usize];
                let v = if self.r.cg_addr & 1 == 0 { w as u8 } else { (w >> 8) as u8 & 0x7F };
                self.r.cg_addr = (self.r.cg_addr + 1) & 0x1FF;
                v
            }
            0x3C => {
                let v = if self.r.hflip_r { (self.r.hlatch >> 8) as u8 & 1 } else { self.r.hlatch as u8 };
                self.r.hflip_r = !self.r.hflip_r;
                v
            }
            0x3D => {
                let v = if self.r.vflip_r { (self.r.vlatch >> 8) as u8 & 1 } else { self.r.vlatch as u8 };
                self.r.vflip_r = !self.r.vflip_r;
                v
            }
            0x3E => 0x01,
            0x3F => {
                self.r.hflip_r = false;
                self.r.vflip_r = false;
                let v = ((self.r.latched as u8) << 6) | ((self.pal as u8) << 4) | 0x03;
                self.r.latched = false;
                v
            }
            _ => open,
        }
    }

    #[inline]
    fn mark_cam(&mut self, i: usize) {
        if self.ext == 0 {
            return;
        }
        if self.tmcam.len() != 0x8000 {
            self.tmcam = vec![0; 0x8000];
        }
        self.tmcam[i] = CAM_KNOWN | (self.r.hofs[0] as u32 & 0x3FF) | ((self.r.vofs[0] as u32 & 0x3FF) << 10);
    }

    /// Einstellungen, die nicht zum Spielzustand gehören, von einer anderen PPU übernehmen.
    pub fn copy_cfg(&mut self, o: &Ppu) {
        self.hud_split = o.hud_split;
        self.margin = o.margin;
        self.layer_mask = o.layer_mask;
        self.capture = o.capture;
        self.ext = o.ext;
        self.record = o.record;
    }

    /// Mode-7-Projektion: Bildschirmposition (x, y) eines Texels (u, v) im letzten Bild.
    /// Nutzt die Register jeder Zeile (die Karte im Spiel ändert sie per HDMA).
    #[allow(dead_code)]
    pub fn m7_project(&self, u: f32, v: f32) -> Option<(f32, f32)> {
        m7_project_lines(&self.lines, u, v)
    }

    /// Momentaufnahme des aktuellen Zustands (zum Neuzeichnen in anderer Breite)
    pub fn snapshot(&self) -> Snap {
        let lines = if self.lines.len() == H { self.lines.clone() } else { vec![self.r; H] };
        Snap { lines, vram: self.vram.clone(), cgram: self.cgram.clone(), oam: self.oam.clone(), hud: self.hud_split, tmcam: self.tmcam.clone(), ow: self.ow.clone(), margin: self.margin, stretch: self.win_stretch, edge: self.ow_edge, keep: self.room_keep.clone(), next: self.room_next.clone(), side: self.room_side.clone(), spot: self.spot }
    }

    /// Register je Zeile des letzten Bildes
    /// Spotlight-Kreis aus den Fensterregistern des letzten Bilds schätzen (für den Breitbildrand)
    pub fn update_spot(&mut self) {
        self.spot = None;
        if !self.win_stretch {
            return;
        }
        for wi in 0..2usize {
            let pts: Vec<(f32, f32, f32)> = self
                .lines
                .iter()
                .enumerate()
                .filter_map(|(y, r)| {
                    let (l, rr) = (r.wh[wi * 2] as i32, r.wh[wi * 2 + 1] as i32);
                    (l <= rr && l > 0 && rr < 255 && y < 225).then(|| (y as f32, (l + rr) as f32 / 2.0, (rr - l) as f32 / 2.0))
                })
                .collect();
            if pts.len() < 4 {
                continue;
            }
            let cx = pts.iter().map(|p| p.1).sum::<f32>() / pts.len() as f32;
            let (y1, _, h1) = pts[0];
            let (y2, _, h2) = pts[pts.len() - 1];
            if (y2 - y1).abs() < 4.0 {
                continue;
            }
            let cy = (h1 * h1 - h2 * h2 + y1 * y1 - y2 * y2) / (2.0 * (y1 - y2));
            let r = (h1 * h1 + (y1 - cy).powi(2)).sqrt();
            if r.is_finite() && r > 0.0 {
                self.spot = Some((cx, cy, r, wi as u8));
                return;
            }
        }
    }

    pub fn line_regs(&self) -> &[Regs] {
        &self.lines
    }

    /// Ganzer Raum (512×512-Kachelkarten von BG1 und BG2 mit Priorität, ohne Figuren) in voller Größe,
    /// verkleinert um `div` (1, 2, 4 …). 0 = durchsichtig/Hintergrundfarbe
    /// wie `room_full`; `skip_bg1`: in dunklen Räumen liegt auf BG1 nur der Lichtkegel der Laterne –
    /// der gehört nicht ins Raumbild
    pub fn room_full_ex(&self, div: u32, skip_bg1: bool) -> Vec<u16> {
        let r = &self.r;
        let tile = |n: usize, x: u32, y: u32| -> Option<(u16, bool)> {
            let (tx, ty) = ((x / 8) & 63, (y / 8) & 63);
            let mut a = (((r.bg_sc[n] & 0xFC) as u32) << 8) + ((ty & 31) << 5) + (tx & 31);
            if tx & 32 != 0 && r.bg_sc[n] & 1 != 0 {
                a += 0x400;
            }
            if ty & 32 != 0 && r.bg_sc[n] & 2 != 0 {
                a += if r.bg_sc[n] & 1 != 0 { 0x800 } else { 0x400 };
            }
            let e = self.vram[(a & 0x7FFF) as usize] as u32;
            let (mut px, mut py) = (x & 7, y & 7);
            if e & 0x4000 != 0 {
                px = 7 - px;
            }
            if e & 0x8000 != 0 {
                py = 7 - py;
            }
            let base = r.bg_nba[n] as u32 + (e & 0x3FF) * 16 + py;
            let bit = 7 - px;
            let w0 = self.vram[(base & 0x7FFF) as usize] as u32;
            let w1 = self.vram[((base + 8) & 0x7FFF) as usize] as u32;
            let c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1) | (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
            (c != 0).then(|| (self.cgram[((((e >> 10) & 7) * 16 + c) & 0xFF) as usize], e & 0x2000 != 0))
        };
        let div = div.clamp(1, 8);
        let n = 512 / div;
        let mut out = vec![0u16; (n * n) as usize];
        for by in 0..n {
            for bx in 0..n {
                let (x, y) = (bx * div + div / 2, by * div + div / 2);
                let (a, b) = (if skip_bg1 { None } else { tile(0, x, y) }, tile(1, x, y));
                let c = match (a, b) {
                    (Some((c, true)), _) => c,
                    (_, Some((c, true))) => c,
                    (Some((c, _)), _) => c,
                    (_, Some((c, _))) => c,
                    _ => self.cgram[0],
                };
                out[(by * n + bx) as usize] = c & 0x7FFF;
            }
        }
        out
    }

    /// Vorschaubild des aktuellen Raums (ganze 512×512-Kachelkarte von BG1 über BG2), 64×64.
    pub fn room_thumb(&self, skip_bg1: bool) -> Vec<u16> {
        let r = &self.r;
        let tile = |n: usize, x: u32, y: u32| -> Option<u16> {
            let (tx, ty) = ((x / 8) & 63, (y / 8) & 63);
            let mut a = (((r.bg_sc[n] & 0xFC) as u32) << 8) + ((ty & 31) << 5) + (tx & 31);
            if tx & 32 != 0 && r.bg_sc[n] & 1 != 0 {
                a += 0x400;
            }
            if ty & 32 != 0 && r.bg_sc[n] & 2 != 0 {
                a += if r.bg_sc[n] & 1 != 0 { 0x800 } else { 0x400 };
            }
            let e = self.vram[(a & 0x7FFF) as usize] as u32;
            let (mut px, mut py) = (x & 7, y & 7);
            if e & 0x4000 != 0 {
                px = 7 - px;
            }
            if e & 0x8000 != 0 {
                py = 7 - py;
            }
            let base = r.bg_nba[n] as u32 + (e & 0x3FF) * 16 + py;
            let bit = 7 - px;
            let w0 = self.vram[(base & 0x7FFF) as usize] as u32;
            let w1 = self.vram[((base + 8) & 0x7FFF) as usize] as u32;
            let c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1) | (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
            (c != 0).then(|| self.cgram[((((e >> 10) & 7) * 16 + c) & 0xFF) as usize])
        };
        let mut out = vec![0u16; 64 * 64];
        for by in 0..64u32 {
            for bx in 0..64u32 {
                let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
                for k in 0..4u32 {
                    let (x, y) = (bx * 8 + 1 + (k & 1) * 4, by * 8 + 1 + (k >> 1) * 4);
                    let c = (if skip_bg1 { None } else { tile(0, x, y) }).or_else(|| tile(1, x, y)).unwrap_or(self.cgram[0]) as u32;
                    sr += c & 31;
                    sg += (c >> 5) & 31;
                    sb += (c >> 10) & 31;
                }
                out[(by * 64 + bx) as usize] = ((sr / 4) | ((sg / 4) << 5) | ((sb / 4) << 10)) as u16;
            }
        }
        out
    }

    pub fn render_line(&mut self, line: u16) {
        let y = line as usize - 1;
        if y >= H {
            return;
        }
        if self.lines.len() != H {
            self.lines = vec![Regs::default(); H];
        }
        self.lines[y] = self.r;
        if self.skip {
            return;
        }
        // erst merken; gezeichnet wird gesammelt (am Bildende oder vor einer Änderung an VRAM/CGRAM/OAM)
        self.pend = Some(match self.pend {
            Some((a, _)) => (a, y + 1),
            None => (y, y + 1),
        });
        #[cfg(test)]
        if IMMEDIATE.load(std::sync::atomic::Ordering::Relaxed) {
            self.flush_lines();
        }
    }

    /// Gemerkte Zeilen zeichnen – viele Zeilen auf alle Kerne verteilt (gleiches Ergebnis wie Zeile für Zeile,
    /// weil Grafikspeicher/Farben/Sprites seit dem Merken unverändert sind)
    pub fn flush_lines(&mut self) {
        let Some((a, b)) = self.pend.take() else { return };
        let ext = self.ext.min(MAX_EXT);
        let w = W + 2 * ext;
        // Oberwelt sicher geladen: gesehene Pixel für spätere Breitbildränder merken
        let learn = ext > 0 && !self.capture && self.margin == MARGIN_WORLD && self.ow.as_ref().map_or(false, |o| o.valid && !o.room && o.world < 2);
        let want_cap = self.capture || learn;
        let n = b - a;
        let mut bufs = std::mem::take(&mut self.line_buf);
        if bufs.len() < H * MAXW * 2 {
            bufs.resize(H * MAXW * 2, 0);
        }
        let pal = pal_fx(&self.cgram, self.ow.as_deref());
        {
            let this = &*self;
            let draw = |y: usize, chunk: &mut [u16]| {
                let (o, c) = chunk.split_at_mut(MAXW);
                let o: &mut [u16; MAXW] = o.try_into().unwrap();
                let c: &mut [u16; MAXW] = c.try_into().unwrap();
                let ctx = Ctx { r: &this.lines[y], vram: &this.vram, cgram: &this.cgram, oam: &this.oam, mask: this.layer_mask, ext: ext as i32, w, hud: this.hud_split, tmcam: &this.tmcam, ow: this.ow.as_deref(), margin: this.margin, stretch: this.win_stretch, edge: this.ow_edge, keep: this.room_keep.as_deref().map(|v| v.as_slice()), next: this.room_next.as_deref().map(|v| v.as_slice()), side: &this.room_side, fog_acc: Some(&this.fog_acc), spot: this.spot, pal, cap_mask: if learn { 0x02 } else { 0x03 } };
                ctx.draw_line((y + 1) as u16, o, if want_cap { Some(c) } else { None });
            };
            let work = &mut bufs[..n * MAXW * 2];
            let threads = if n >= 24 { std::thread::available_parallelism().map(|t| t.get()).unwrap_or(4).clamp(1, 8) } else { 1 };
            if threads <= 1 {
                for (k, chunk) in work.chunks_mut(MAXW * 2).enumerate() {
                    draw(a + k, chunk);
                }
            } else {
                let per = n.div_ceil(threads);
                std::thread::scope(|sc| {
                    for (bi, band) in work.chunks_mut(per * MAXW * 2).enumerate() {
                        let draw = &draw;
                        sc.spawn(move || {
                            for (k, chunk) in band.chunks_mut(MAXW * 2).enumerate() {
                                draw(a + bi * per + k, chunk);
                            }
                        });
                    }
                });
            }
        }
        for y in a..b {
            let o = (y - a) * MAXW * 2;
            self.store_line(y, &bufs[o..o + MAXW], &bufs[o + MAXW..o + 2 * MAXW], learn);
        }
        self.line_buf = bufs;
    }

    /// fertige Zeile ins Bild (und Aufnahme/gesehene Pixel)
    fn store_line(&mut self, y: usize, buf: &[u16], capbuf: &[u16], learn: bool) {
        let ext = self.ext.min(MAX_EXT);
        let w = W + 2 * ext;
        let line = y + 1;
        let r = self.lines[y];
        let row = y * W;
        self.fb[row..row + W].copy_from_slice(&buf[ext..ext + W]);
        if self.capture {
            self.cap[row..row + W].copy_from_slice(&capbuf[ext..ext + W]);
        }
        if learn {
            let o = self.ow.as_ref().unwrap();
            let wrap = |d: i32| {
                let d = d.rem_euclid(1024);
                if d > 512 {
                    d - 1024
                } else {
                    d
                }
            };
            let dh = wrap(r.hofs[1] as i32 - (o.camx & 0x3FF));
            let dv = wrap(r.vofs[1] as i32 - (o.camy & 0x3FF));
            let wy = o.camy + dv + line as i32;
            if (0..WDIM as i32).contains(&wy) && r.inidisp & 0x80 == 0 {
                if let Ok(mut c) = WCACHE.write() {
                    while c.len() < 2 {
                        c.push(Vec::new());
                    }
                    let wc = &mut c[o.world as usize & 1];
                    if wc.is_empty() {
                        *wc = vec![0; WDIM * WDIM];
                    }
                    let base = wy as usize * WDIM;
                    for x in 0..W {
                        let wx = o.camx + dh + x as i32;
                        if (0..WDIM as i32).contains(&wx) {
                            wc[base + wx as usize] = 0x8000 | capbuf[ext + x];
                        }
                    }
                }
            }
        }
        if ext > 0 {
            if self.wide.len() != w * H {
                self.wide = vec![0; w * H];
            }
            if self.margin == MARGIN_KEEP {
                // Menü: nur die Mitte erneuern, die Ränder vom letzten Spielbild bleiben
                self.wide[y * w + ext..y * w + ext + W].copy_from_slice(&buf[ext..ext + W]);
            } else {
                self.wide[y * w..(y + 1) * w].copy_from_slice(&buf[..w]);
            }
        }
    }

    pub fn vblank_start(&mut self) {
        self.flush_lines();
        if self.r.inidisp & 0x80 == 0 {
            self.r.oam_addr = self.r.oam_reload;
        }
        if self.record && !self.skip && self.lines.len() == H {
            let tmcam = if self.ext > 0 { self.tmcam.clone() } else { Vec::new() };
            self.snap = Some(std::sync::Arc::new(Snap { lines: self.lines.clone(), vram: self.vram.clone(), cgram: self.cgram.clone(), oam: self.oam.clone(), hud: self.hud_split, tmcam, ow: self.ow.clone(), margin: self.margin, stretch: self.win_stretch, edge: self.ow_edge, keep: self.room_keep.clone(), next: self.room_next.clone(), side: self.room_side.clone(), spot: self.spot }));
        }
    }
}

/// Mode-7-Projektion: Bildschirmposition eines Texels (u, v) anhand der Register jeder Zeile.
pub fn m7_project_lines(lines: &[Regs], u: f32, v: f32) -> Option<(f32, f32)> {
    if lines.len() != H {
        return None;
    }
    let mut best: Option<(f32, f32, f32)> = None;
    for (y, r) in lines.iter().enumerate() {
        if r.bgmode & 7 != 7 || r.inidisp & 0x80 != 0 {
            continue;
        }
        let (a, b, c, d) = (r.m7a as f32, r.m7b as f32, r.m7c as f32, r.m7d as f32);
        let (cx, cy) = (r.m7x as f32, r.m7y as f32);
        let (h, vv) = (r.m7hofs as f32, r.m7vofs as f32);
        let line = y as f32 + 1.0;
        let u0 = (a * (h - cx) + b * (vv - cy) + b * line) / 256.0 + cx;
        let v0 = (c * (h - cx) + d * (vv - cy) + d * line) / 256.0 + cy;
        let (du, dv) = (a / 256.0, c / 256.0);
        if du.abs() < 1e-3 {
            continue;
        }
        let x = (u - u0) / du;
        let dist = (v0 + dv * x - v).abs();
        if best.map_or(true, |b| dist < b.0) {
            best = Some((dist, x, y as f32));
        }
    }
    best.filter(|b| b.0 < 3.0).map(|b| (b.1, b.2))
}

/// Kürzeste Differenz zweier Werte auf einem Ring der Größe `m`.
#[inline]
fn wrap_diff(a: i32, b: i32, m: i32) -> i32 {
    let d = (a - b).rem_euclid(m);
    if d > m / 2 {
        d - m
    } else {
        d
    }
}

struct Spr {
    x: i32,
    y: i32,
    tile: u8,
    attr: u8,
    big: bool,
}

fn sprite(oam: &[u8], i: usize) -> Spr {
    let hi = (oam[512 + (i >> 2)] >> ((i & 3) * 2)) & 3;
    Spr { x: oam[i * 4] as i32 | (((hi & 1) as i32) << 8), y: oam[i * 4 + 1] as i32, tile: oam[i * 4 + 2], attr: oam[i * 4 + 3], big: hi & 2 != 0 }
}

#[inline]
fn hidden(y: i32) -> bool {
    (0xE0..0xF8).contains(&y)
}

/// Zeichnet ein Zwischenbild zwischen `prev` und `cur` (t = 0 → prev, 1 → cur):
/// Scroll-Register je Zeile und Sprite-Positionen werden interpoliert, alles andere stammt aus `cur`.
/// Rückgabe: ob sich gegenüber `cur` etwas verschoben hat.
pub fn render_interp(prev: &Snap, cur: &Snap, t: f32, ext: usize, mask: u8, out: &mut Vec<u16>) -> bool {
    let ext = ext.min(MAX_EXT);
    let w = W + 2 * ext;
    if out.len() != w * H {
        out.resize(w * H, 0);
    }
    let back = 1.0 - t.clamp(0.0, 1.0);
    let mut moved = false;
    // Sprites zuordnen (gleicher OAM-Platz, sonst gleiches Tile in der Nähe) und verschieben
    let mut oam = cur.oam.clone();
    for i in 0..128 {
        let c = sprite(&cur.oam, i);
        if hidden(c.y) {
            continue;
        }
        let close = |p: &Spr| -> Option<(i32, i32)> {
            if p.big != c.big || hidden(p.y) {
                return None;
            }
            let dx = wrap_diff(c.x, p.x, 512);
            let dy = wrap_diff(c.y, p.y, 256);
            (dx.abs() <= 16 && dy.abs() <= 16).then_some((dx, dy))
        };
        let p = sprite(&prev.oam, i);
        // gleicher OAM-Platz nur, wenn es offensichtlich dasselbe Objekt ist (gleiches Tile oder gleiche Attribute, nah)
        let same_slot = (p.tile == c.tile && p.attr == c.attr) || (p.attr == c.attr && wrap_diff(c.x, p.x, 512).abs() <= 6 && wrap_diff(c.y, p.y, 256).abs() <= 6);
        let d = same_slot.then(|| close(&p)).flatten().or_else(|| {
            (0..128)
                .map(|j| sprite(&prev.oam, j))
                .filter(|q| q.tile == c.tile && q.attr == c.attr)
                .filter_map(|q| close(&q))
                .min_by_key(|(dx, dy)| dx.abs() + dy.abs())
        });
        let Some((dx, dy)) = d else { continue };
        let ox = (dx as f32 * back).round() as i32;
        let oy = (dy as f32 * back).round() as i32;
        if ox == 0 && oy == 0 {
            continue;
        }
        moved = true;
        let x = (c.x - ox).rem_euclid(512);
        let y = (c.y - oy).rem_euclid(256);
        oam[i * 4] = x as u8;
        oam[i * 4 + 1] = y as u8;
        let sh = (i & 3) * 2;
        let hb = &mut oam[512 + (i >> 2)];
        *hb = (*hb & !(1 << sh)) | (((x >> 8) as u8 & 1) << sh);
    }
    // Zeilen parallel zeichnen (Register je Zeile interpolieren)
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let band = H.div_ceil(threads);
    let oam = &oam;
    let moved_bg = std::sync::atomic::AtomicBool::new(false);
    let keep = cur.margin == MARGIN_KEEP && ext > 0;
    std::thread::scope(|sc| {
        for (bi, chunk) in out.chunks_mut(band * w).enumerate() {
            let moved_bg = &moved_bg;
            sc.spawn(move || {
                let mut buf = [0u16; MAXW];
                for (k, row) in chunk.chunks_mut(w).enumerate() {
                    let y = bi * band + k;
                    if interp_line(prev, cur, y, back, oam, mask, ext, w, &mut buf) {
                        moved_bg.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if keep {
                        // Ränder vom letzten Bild stehen lassen (z. B. während eines Raumwechsels)
                        row[ext..ext + W].copy_from_slice(&buf[ext..ext + W]);
                    } else {
                        row.copy_from_slice(&buf[..w]);
                    }
                }
            });
        }
    });
    moved || moved_bg.into_inner()
}

#[allow(clippy::too_many_arguments)]
fn interp_line(prev: &Snap, cur: &Snap, y: usize, back: f32, oam: &[u8], mask: u8, ext: usize, w: usize, buf: &mut [u16; MAXW]) -> bool {
    let mut moved = false;
    {
        let mut r = cur.lines[y];
        let p = &prev.lines[y];
        if r.bgmode & 7 != 7 && p.bgmode & 7 == r.bgmode & 7 {
            for n in 0..4 {
                let dh = wrap_diff(r.hofs[n] as i32, p.hofs[n] as i32, 1024);
                let dv = wrap_diff(r.vofs[n] as i32, p.vofs[n] as i32, 1024);
                if dh.abs() <= 48 && dv.abs() <= 48 {
                    let oh = (dh as f32 * back).round() as i32;
                    let ov = (dv as f32 * back).round() as i32;
                    if oh != 0 || ov != 0 {
                        moved = true;
                        r.hofs[n] = ((r.hofs[n] as i32 - oh) & 0x3FF) as u16;
                        r.vofs[n] = ((r.vofs[n] as i32 - ov) & 0x3FF) as u16;
                    }
                }
            }
        }
        let ctx = Ctx { r: &r, vram: &cur.vram, cgram: &cur.cgram, oam, mask, ext: ext as i32, w, hud: cur.hud, tmcam: &cur.tmcam, ow: cur.ow.as_deref(), margin: cur.margin, stretch: cur.stretch, edge: cur.edge, keep: cur.keep.as_deref().map(|v| v.as_slice()), next: cur.next.as_deref().map(|v| v.as_slice()), side: &cur.side, fog_acc: None, spot: cur.spot, pal: pal_fx(&cur.cgram, cur.ow.as_deref()), cap_mask: 0x03 };
        ctx.draw_line(y as u16 + 1, buf, None);
    }
    moved
}

/// Zeichnet ein gespeichertes Bild ohne Interpolation (z. B. für Breitbild aus einer Momentaufnahme).
pub fn render_snap(s: &Snap, ext: usize, mask: u8, out: &mut Vec<u16>) {
    render_interp(s, s, 1.0, ext, mask, out);
}

/// Zeichen-Kontext einer Zeile: Register + Speicher, Breite `w` = 256 + 2·ext.
/// Wie die echte Palette gegenüber der gespeicherten Palette des gezeigten Gebiets verändert ist (Überblendung nach Weiß,
/// Grau oder Schwarz, z. B. bei der Teleportation in die andere Welt). Je Farbkanal eine Tabelle alter Wert -> neuer Wert
/// (so passen auch Überblendungen, die an Weiß anstoßen); None, wenn nichts verändert ist oder die Paletten nicht
/// zueinander passen (dann bleiben die Randfarben, wie sie sind)
pub(crate) fn pal_fx(cgram: &[u16], ow: Option<&OwMap>) -> Option<[[u8; 32]; 3]> {
    let o = ow?;
    if o.room || o.size == 0 || o.world > 1 || cgram.len() < 128 {
        return None;
    }
    let id = o.world as usize * 0x40 + (o.y0 / 512) as usize * 8 + (o.x0 / 512) as usize;
    let pid = PARENT.read().ok().and_then(|p| p.get(id).copied()).map(|v| v as usize).unwrap_or(id);
    let areas = AREAS.read().ok()?;
    let a = areas.get(pid & 0x7F)?.as_ref()?;
    if a.cgram.len() < 128 {
        return None;
    }
    // Eine Überblendung verändert fast alle Farben; ändern sich nur wenige, sind das animierte Farben des Gebiets
    // (z. B. Todesberg in der Schattenwelt alle 4 Bilder) – dann den Rand nicht umfärben (sonst blinkt er)
    let idx: Vec<usize> = (32..128).filter(|i| i % 16 != 0).collect();
    let changed = idx.iter().filter(|&&i| a.cgram[i] != cgram[i]).count();
    if changed * 2 < idx.len() {
        return None;
    }
    let mut out = [[0u8; 32]; 3];
    let mut same = true;
    for k in 0..3 {
        // Landschafts-Paletten (ohne die Leiste oben, die nicht mitblendet)
        let pts: Vec<(usize, f32)> = (32..128).filter(|i| i % 16 != 0).map(|i| (((a.cgram[i] >> (k * 5)) & 31) as usize, ((cgram[i] >> (k * 5)) & 31) as f32)).collect();
        let (mut sum, mut cnt) = ([0f32; 32], [0u32; 32]);
        for &(v, l) in &pts {
            sum[v] += l;
            cnt[v] += 1;
        }
        let mean: Vec<Option<f32>> = (0..32).map(|v| if cnt[v] > 0 { Some(sum[v] / cnt[v] as f32) } else { None }).collect();
        // gleiche alte Werte müssen (fast) gleich verändert sein – sonst ist es eine andere Palette, keine Überblendung
        let fit = pts.iter().filter(|&&(v, l)| (mean[v].unwrap() - l).abs() <= 2.0).count();
        if fit * 4 < pts.len() * 3 || mean.iter().flatten().count() < 4 {
            return None;
        }
        // Überblendungen behalten die Reihenfolge (heller bleibt heller): Werte, die das verletzen (unbenutzte Einträge
        // mit anderem Inhalt), weglassen – jeweils den mit weniger Belegen
        let mut known: Vec<(usize, f32)> = (0..32).filter_map(|v| mean[v].map(|m| (v, m))).collect();
        loop {
            let bad = (0..known.len().saturating_sub(1)).find(|&i| known[i].1 > known[i + 1].1 + 1.5);
            let Some(i) = bad else { break };
            let drop = if cnt[known[i].0] <= cnt[known[i + 1].0] { i } else { i + 1 };
            known.remove(drop);
        }
        if known.len() < 4 {
            return None;
        }
        // fehlende Werte dazwischen ergänzen (gerade Linie zu den Nachbarn, an den Enden mit gleichem Abstand)
        for v in 0..32 {
            let m = match (known.iter().rev().find(|p| p.0 <= v), known.iter().find(|p| p.0 >= v)) {
                (Some(&(a0, m0)), Some(&(a1, m1))) if a1 > a0 => m0 + (m1 - m0) * (v - a0) as f32 / (a1 - a0) as f32,
                (Some(&(_, m0)), Some(_)) => m0,
                (Some(&(a0, m0)), None) => m0 + (v - a0) as f32 * if m0 >= 30.5 { 0.0 } else { 1.0 },
                (None, Some(&(a1, m1))) => m1 - (a1 - v) as f32 * if m1 <= 0.5 { 0.0 } else { 1.0 },
                (None, None) => v as f32,
            };
            let n = m.round().clamp(0.0, 31.0) as u8;
            if (n as i32 - v as i32).abs() > 0 {
                same = false;
            }
            out[k][v] = n;
        }
    }
    if same {
        None
    } else {
        Some(out)
    }
}

fn apply_pal(c: u16, f: [[u8; 32]; 3]) -> u16 {
    let mut o = 0u16;
    for k in 0..3 {
        o |= (f[k][((c >> (k * 5)) & 31) as usize] as u16) << (k * 5);
    }
    o
}

struct Ctx<'a> {
    r: &'a Regs,
    vram: &'a [u16],
    cgram: &'a [u16],
    oam: &'a [u8],
    mask: u8,
    ext: i32,
    w: usize,
    hud: bool,
    tmcam: &'a [u32],
    ow: Option<&'a OwMap>,
    margin: u8,
    stretch: bool,
    edge: u8,
    keep: Option<&'a [u16]>,
    next: Option<&'a [u16]>,
    side: &'a [(u16, std::sync::Arc<Vec<u16>>)],
    fog_acc: Option<&'a [std::sync::atomic::AtomicU32; 4]>,
    spot: Option<(f32, f32, f32, u8)>,
    /// Überblendung der echten Palette gegenüber der gespeicherten (je Farbkanal neu = a·alt + b), für den Rand
    pal: Option<[[u8; 32]; 3]>,
    /// Ebenen für die Kartenaufnahme (`cap`)
    cap_mask: u8,
}

type Line = [u32; MAXW];

impl Ctx<'_> {
    fn window_mask(&self, sel: u8, log: u8, line: u16, m: &mut [bool; MAXW]) {
        let w1 = sel & 2 != 0;
        let w2 = sel & 8 != 0;
        if !w1 && !w2 {
            m[..self.w].fill(false);
            return;
        }
        let r = self.r;
        let (l1, r1, l2, r2) = (r.wh[0] as i32, r.wh[1] as i32, r.wh[2] as i32, r.wh[3] as i32);
        for (i, out) in m[..self.w].iter_mut().enumerate() {
            let x = (i as i32 - self.ext).clamp(0, 255);
            let mut ia = x >= l1 && x <= r1;
            let mut ib = x >= l2 && x <= r2;
            // Kreis-Übergang im Breitbild: derselbe Kreis, so vergrößert, dass er das ganze Bild in der gleichen Zeit öffnet
            if let (true, Some((cx, cy, rad, wi))) = (self.stretch && self.ext > 0, self.spot) {
                let e = self.ext as f32;
                let far = |xs: [f32; 2]| xs.iter().flat_map(|&xx| [0.0f32, 223.0].map(|yy| ((xx - cx).powi(2) + (yy - cy).powi(2)).sqrt())).fold(0.0f32, f32::max);
                let k = far([-e, 255.0 + e]) / far([0.0, 255.0]).max(1.0);
                let dx = i as f32 - e - cx;
                let dy = line as f32 - cy;
                let inside = dx * dx + dy * dy <= (rad * k) * (rad * k);
                if wi == 0 {
                    ia = inside;
                } else {
                    ib = inside;
                }
            }
            let a = ia ^ (sel & 1 != 0);
            let b = ib ^ (sel & 4 != 0);
            *out = if w1 && w2 {
                match log & 3 {
                    0 => a | b,
                    1 => a & b,
                    2 => a ^ b,
                    _ => !(a ^ b),
                }
            } else if w1 {
                a
            } else {
                b
            };
        }
    }

    fn render_bg(&self, n: usize, bpp: u32, mode: u8, line: u16, out: &mut Line) {
        let rg = self.r;
        let big = rg.bgmode & (0x10 << n) != 0;
        let tsz: u32 = if big { 16 } else { 8 };
        let sc = rg.bg_sc[n];
        let map_base = ((sc & 0xFC) as u32) << 8;
        let wide = sc & 1 != 0;
        let tall = sc & 2 != 0;
        let chr = rg.bg_nba[n] as u32;
        let mos = if rg.mosaic & (1 << n) != 0 { (rg.mosaic >> 4) as i32 + 1 } else { 1 };
        let mut ly = line as u32;
        if mos > 1 {
            ly -= (ly - 1) % mos as u32;
        }
        let yy = (ly + rg.vofs[n] as u32) & 0x3FF;
        let hofs = rg.hofs[n] as i32;
        let wpt = bpp * 4;
        let pal_base: u32 = if mode == 0 { n as u32 * 32 } else { 0 };
        let direct = bpp == 8 && rg.cgwsel & 1 != 0;
        let ty = (yy / tsz) & 63;
        let mut row_addr = map_base + ((ty & 31) << 5);
        if ty & 32 != 0 && tall {
            row_addr += if wide { 0x800 } else { 0x400 };
        }
        let vram = self.vram;
        let full_map = wide && tall;
        let ext = self.ext;
        // Pixel einer Kachel (Tilemap-Wort e, Position in der Kachel)
        let pix_in = |vram: &[u16], e: u32, mut px: u32, mut py: u32| -> (u32, bool) {
            let prio = e & 0x2000 != 0;
            if e & 0x4000 != 0 {
                px = tsz - 1 - px;
            }
            if e & 0x8000 != 0 {
                py = tsz - 1 - py;
            }
            let mut t = e & 0x3FF;
            if big {
                t = (t + (px >> 3) + ((py >> 3) << 4)) & 0x3FF;
            }
            let base = chr + t * wpt + (py & 7);
            let bit = 7 - (px & 7);
            let w0 = vram[(base & 0x7FFF) as usize] as u32;
            let mut c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1);
            if bpp >= 4 {
                let w1 = vram[((base + 8) & 0x7FFF) as usize] as u32;
                c |= (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
            }
            if bpp == 8 {
                let w2 = vram[((base + 16) & 0x7FFF) as usize] as u32;
                let w3 = vram[((base + 24) & 0x7FFF) as usize] as u32;
                c |= (((w2 >> bit) & 1) << 4) | (((w2 >> (8 + bit)) & 1) << 5);
                c |= (((w3 >> bit) & 1) << 6) | (((w3 >> (8 + bit)) & 1) << 7);
            }
            if c == 0 {
                return (0, prio);
            }
            let pal = (e >> 10) & 7;
            let color = if bpp == 8 {
                if direct {
                    direct_color(c, pal)
                } else {
                    self.cgram[c as usize]
                }
            } else {
                self.cgram[((pal_base + pal * (1 << bpp) + c) & 0xFF) as usize]
            };
            (OPAQUE | (((e >> 13) & 1) << 16) | color as u32, prio)
        };
        let pix = |e: u32, px: u32, py: u32| pix_in(vram, e, px, py);
        let keep_vram = self.keep;
        let next_vram = self.next;
        let side_vram = self.side;
        // Oberwelt (BG2): Breitbildränder direkt aus der Gebietskarte des Spiels
        let ow = if ext > 0 && n == 1 && !big { self.ow.filter(|o| !o.room) } else { None };
        let ow_edge = self.edge;
        // Sondergebiet (Welt 2): beide Ebenen im Rand aus dem sichtbaren Bild falten (siehe `sample`)
        let special_ow = if ext > 0 && n < 2 && !big { self.ow.filter(|o| !o.room && o.world >= 2) } else { None };
        let room = if ext > 0 && n < 2 { self.ow.filter(|o| o.room) } else { None };
        // dunkler Raum: diese Ebene ist die Lichtmaske (nur auf dem Subscreen, wird abgezogen). Im Rand nicht
        // wiederholen (sonst doppelter Lichtkegel), sondern die Bildkante nehmen, die von Link am weitesten weg ist
        let light_mask = room.is_some() && rg.cgadsub & 0x80 != 0 && rg.ts & (1 << n) != 0 && rg.tm & (1 << n) == 0;
        let link_sx = room.map_or(128, |o| o.lx - o.camx);
        let mask_col = if link_sx < 128 { 255 } else { 0 };
        let cache_guard = if ow.is_some() { WCACHE.read().ok() } else { None };
        let map_guard = if ow.is_some() { WMAP.read().ok() } else { None };
        let empty: Vec<Vec<u16>> = Vec::new();
        let cache: &[Vec<u16>] = cache_guard.as_deref().map(|v| v.as_slice()).unwrap_or(&empty);
        let areas_guard = if ow.is_some() { AREAS.read().ok() } else { None };
        let parents_guard = if ow.is_some() { PARENT.read().ok() } else { None };
        let no_areas: Vec<Option<std::sync::Arc<AreaData>>> = Vec::new();
        let no_parents: Vec<u8> = Vec::new();
        let areas: &[Option<std::sync::Arc<AreaData>>] = areas_guard.as_deref().map(|v| v.as_slice()).unwrap_or(&no_areas);
        let parents: &[u8] = parents_guard.as_deref().map(|v| v.as_slice()).unwrap_or(&no_parents);
        let wmap: &[Vec<u16>] = map_guard.as_deref().map(|v| v.as_slice()).unwrap_or(&empty);
        let wrap = |d: i32| {
            let d = d.rem_euclid(1024);
            if d > 512 {
                d - 1024
            } else {
                d
            }
        };
        // nahtlos: passt das Nachbargebiet an der linken/rechten Gebietsgrenze über die ganze Höhe an?
        let (seam_l, seam_r) = match ow {
            Some(o) if ow_edge == EDGE_SEAMLESS && o.size > 0 && o.world < 2 => {
                let all = |bx: i32| (0..o.size / 512).all(|k| seam_ok(areas, parents, &o.tab, o.world, bx, o.y0 + k * 512));
                (all(o.x0), all(o.x0 + o.size))
            }
            _ => (true, true),
        };
        let ow_dh = ow.or(room).map_or(0, |o| wrap(hofs - (o.camx & 0x3FF)));
        let fog_live = rg.ts & 1 != 0 && rg.cgadsub & 0x02 != 0 && rg.cgadsub & 0x80 == 0;
        let fog_guard = if ow.is_some() { FOG.read().ok() } else { None };
        let no_fog: Vec<u16> = Vec::new();
        let fogs: &[u16] = fog_guard.as_deref().map(|v| v.as_slice()).unwrap_or(&no_fog);
        let ow_dv = ow.or(room).map_or(0, |o| wrap(rg.vofs[n] as i32 - (o.camy & 0x3FF)));
        // diese Bildzeile: liegt das Nachbargebiet an der linken/rechten Grenze versetzt? (Zeilen)
        let ((warp_l, rep_l, wl), (warp_r, rep_r, wr)) = match ow {
            Some(o) if ow_edge == EDGE_SEAMLESS && o.size > 0 && o.world < 2 => {
                let wy = o.camy + ow_dv + ly as i32;
                if (o.y0..o.y0 + o.size).contains(&wy) {
                    let at = |bx: i32| {
                        let right = bx != o.x0;
                        let w = warp_runs(areas, parents, &o.tab, o.world, bx, wy);
                        let rp = w.rep.iter().find(|r| r.2 == right && wy >= r.0 && wy <= r.1).copied();
                        let off = warp_at(&w.warp, wy, right);
                        (off, rp, if off != 0.0 { Some(w) } else { None })
                    };
                    (at(o.x0), at(o.x0 + o.size))
                } else {
                    ((0.0, None, None), (0.0, None, None))
                }
            }
            _ => ((0.0, None, None), (0.0, None, None)),
        };

        // ein Pixel der Ebene an Bildschirm-x: (Pixel, Kachel mit Priorität)
        let sample = |sx: i32| -> (u32, bool) {
            let mx = if mos > 1 { sx - sx.rem_euclid(mos) } else { sx };
            let mut mx = mx;
            let mut from_keep = false;
            let mut from_next = false;
            let mut from_side: Option<&[u16]> = None;
            // nah an Link: der echte Lichtkegel läuft in den Rand weiter; weiter weg wäre es nur die Wiederholung
            if light_mask && !(0..256).contains(&sx) && (sx - link_sx).abs() > 128 {
                mx = mask_col;
            }
            if let (Some(o), false, false) = (room, (0..256).contains(&sx), light_mask) {
                // Raum: innerhalb der Raumgrenzen normal zeichnen; dahinter die äußerste Kachelspalte (16 Pixel) des
                // Raums wiederholen – wie die Wand, ohne den Raum ein zweites Mal zu zeigen und ohne Streifen
                // (eine einzelne Pixelspalte zu strecken gab waagrechte Streifen)
                let wx = o.camx + ow_dh + mx;
                // neben dem 512er-Block des Raums: den Raum zeigen, der an dieser Welt-Stelle liegt (Nachbarraum aus
                // der Kopie) – nach Welt-Position, damit der Rand beim Raumwechsel wie die Mitte mitscrollt
                let wy = o.camy + ow_dv + ly as i32;
                let in_keep = keep_vram.is_some() && wx >= o.keep0 && wx < o.keep1;
                let in_next = next_vram.is_some() && wx >= o.next0 && wx < o.next1;
                if (wx < o.blk0 || wx >= o.blk0 + 512) && wx >= 0 && wy >= 0 && !in_keep && !in_next {
                    let nr = (((wy >> 9) << 4) | (wx >> 9)) as u16;
                    from_side = side_vram.iter().find(|(r, _)| *r == nr).map(|(_, v)| v.as_slice());
                }
                let ex = if from_side.is_some() || (next_vram.is_some() && wx >= o.next0 && wx < o.next1) {
                    // Nachbarraum bzw. Zielraum aus der Kopie: echt zeigen statt Randkachel
                    wx
                } else if wx >= o.bx1 {
                    o.rep1 + (wx - o.bx1).rem_euclid(16)
                } else if wx < o.bx0 {
                    o.rep0 + (wx - o.bx0).rem_euclid(16)
                } else {
                    wx
                };
                // Raumwechsel: den alten Raum aus der Kopie zeichnen (in den Kacheln steht dort schon der neue)
                from_keep = keep_vram.is_some() && ex >= o.keep0 && ex < o.keep1;
                from_next = next_vram.is_some() && ex >= o.next0 && ex < o.next1;
                mx += ex - wx;
            }
            // Sondergebiet (unter der Brücke, Masterschwert-Hain …): die Kamera steht still und neben dem sichtbaren
            // Teil liegen in den Kacheln fremde Daten – den Rand deshalb aus dem echten Bild fortsetzen: die Landschaft
            // (BG2) einmal an der Bildkante gespiegelt (kein Hin-und-her-Muster); BG1 (z. B. der halbdurchsichtige
            // Schatten unter der Brücke) gehört nur in die Mitte und gab im Rand helle/dunkle Streifen -> weglassen
            let mut special = false;
            // (nur hinter der Grenze des Gebiets – innerhalb zeigt der Rand das echte Gelände, z. B. bei Zoras Wasserfall)
            if let (Some(o), false) = (special_ow, (0..256).contains(&sx)) {
                let wxr = o.camx + sx;
                // hinter der Grenze: BG1 (Schatten/Wasser-Überlagerung) weglassen; die Landschaft (BG2) spiegelt der
                // Gebietskarten-Teil unten einmal an der Grenze
                if (wxr < o.bx0 || wxr >= o.bx1) && n == 0 {
                    return (0, false);
                }
                let _ = &mut special;
            }
            if let (Some(o), false) = (ow, (0..256).contains(&sx) || special) {
                // hinter dem Rand der Welt: Landschaft spiegeln und zum Rand hin sanft abdunkeln
                // (statt den letzten Streifen zu wiederholen – das gab mehrfache Klippen)
                let wd = WDIM as i32;
                let rx = o.camx + ow_dh + mx;
                let ry = o.camy + ow_dv + ly as i32;
                let mirror = |v: i32| if v >= wd { (2 * wd - 1 - v).max(0) } else if v < 0 { (-1 - v).min(wd - 1) } else { v };
                let mut over = (-rx).max(rx - (wd - 1)).max(-ry).max(ry - (wd - 1)).max(0);
                let mut wx = mirror(rx);
                let mut wy = mirror(ry);
                let mut outside = o.size > 0 && (rx < o.x0 || rx >= o.x0 + o.size || ry < o.y0 || ry >= o.y0 + o.size);
                if o.world >= 2 {
                    // Sondergebiet ohne Nachbarn (Masterschwert-Hain): hinter seinem Rand den äußeren Streifen (Wald)
                    // hin und her fortsetzen – nicht das ganze Gebiet spiegeln (sonst stünde dort ein zweiter Sockel)
                    // einmal an der Grenze spiegeln (kein Hin-und-her-Muster)
                    let pp = |v: i32, lo: i32, hi: i32| {
                        if v < lo {
                            (2 * lo - 1 - v).min(hi - 1)
                        } else if v >= hi {
                            (2 * hi - 1 - v).max(lo)
                        } else {
                            v
                        }
                    };
                    wx = pp(rx, o.bx0, o.bx1);
                    wy = pp(ry, o.by0, o.by1);
                    over = 0;
                    outside = false;
                }
                if ow_edge == EDGE_RECOMP && outside {
                    return (OPAQUE, false);
                }
                // nahtlos: wo das Nachbargebiet nicht anschließt, das eigene Gebiet an der Grenze spiegeln
                if ow_edge == EDGE_SEAMLESS && outside && over == 0 && (o.y0..o.y0 + o.size).contains(&wy) {
                    let right = wx >= o.x0 + o.size;
                    let bx = if right { o.x0 + o.size } else { o.x0 };
                    if !(if right { seam_r } else { seam_l }) {
                        let m = (2 * bx - 1 - wx).clamp(o.x0, o.x0 + o.size - 1);
                        over = (wx - m).abs() / 2;
                        wx = m;
                    } else {
                        // Nachbargebiet: liegt eine Kante dort versetzt, an der Grenze sanft angleichen
                        // (an der Grenze voller Versatz, nach 48 Pixeln keiner mehr) – keine Spiegelung, keine harte Kante
                        let d = if right { warp_r } else { warp_l };
                        let t = if right { wx - bx } else { bx - 1 - wx };
                        let fade = warp_fade();
                        // Absatz hinter der Grenze: eigenes Muster weiterführen, dann sein gespiegeltes Ende
                        let cont = (if right { rep_r } else { rep_l }).and_then(|(_, _, _, p, c, ell, col)| {
                            let tr = p * (t / p + 1) - t - 1;
                            if t < c {
                                return Some(Ok(tr));
                            }
                            if ell[2] > 0.0 && (t as f32) < ell[0] {
                                // links der Kronenmitte: außerhalb der Krone der Absatz, auf ihrem Rand der Umriss
                                let ex = (t as f32 + 0.5 - ell[0]) / ell[2];
                                let ey = (wy as f32 - ell[1]) / ell[3];
                                let e = (ex * ex + ey * ey).sqrt();
                                if e > 1.0 {
                                    return Some(Ok(tr));
                                }
                                if e > 1.0 - 1.4 / ell[2].min(ell[3]) {
                                    return Some(Err(col));
                                }
                            }
                            None
                        });
                        if let Some(Err(col)) = cont {
                            return (OPAQUE | col as u32, false);
                        }
                        if let Some(Ok(u)) = cont {
                            // u: eigene Spalte, u Pixel vor der Grenze
                            wx = if right { bx - 1 - u } else { bx + u };
                        } else if d != 0.0 && t < fade {
                            // Übergang über fade Pixel: jede Zeile u des eigenen Gebiets wandert von ihrer Höhe an der Grenze
                            // gleichmäßig zur Höhe der zugeordneten Nachbarzeile m(u). Gezeigt wird der Nachbar dort –
                            // außer wo es ihn so nicht gibt (z. B. die helle Fläche einer breiten Klippe, wo der Nachbar nur
                            // einen dunklen Streifen hat): dort läuft die eigene Grenzspalte weiter und spitz in seine Kante aus
                            let run = (if right { wr.as_ref() } else { wl.as_ref() }).and_then(|w| w.warp.iter().find(|r| wy >= r.a && wy <= r.b));
                            if let Some(run) = run {
                                let f = t as f32 / fade as f32;
                                let m = if right { &run.mr } else { &run.ml };
                                let pos = |u: i32| u as f32 + (m[(u - run.a) as usize] - u as f32) * f;
                                let (mut lo, mut hi) = (run.a, run.b);
                                while lo < hi {
                                    let mid = (lo + hi + 1) / 2;
                                    if pos(mid) <= wy as f32 + 0.5 {
                                        lo = mid;
                                    } else {
                                        hi = mid - 1;
                                    }
                                }
                                let u = lo;
                                let yn = m[(u - run.a) as usize].round() as i32;
                                let own_x = if right { bx - 1 } else { bx };
                                let own = area_pixel(areas, parents, &o.tab, o.world, own_x, u);
                                let nb = area_pixel(areas, parents, &o.tab, o.world, wx, yn);
                                if let (Some(a), Some(b)) = (own, nb) {
                                    let dist: i32 = (0..3).map(|k| (((a >> (k * 5)) & 31) as i32 - ((b >> (k * 5)) & 31) as i32).abs()).sum();
                                    if dist > 14 {
                                        return (OPAQUE | a as u32, false);
                                    }
                                }
                                wy = yn;
                            } else {
                                wy += (d * (1.0 - t as f32 / fade as f32)).round() as i32;
                            }
                        }
                    }
                }
                let lx = wx - o.x0;
                let lyy = wy - o.y0;
                let res = if !o.valid || lx < 0 || lyy < 0 || lx >= o.size || lyy >= o.size {
                    // außerhalb des geladenen Gebiets: Nachbargebiet (echte Kacheln), sonst gesehen, sonst Weltkarte
                    // (während einer Überblendung die gespeicherten Farben, die werden unten mitgeblendet)
                    let cur = (o.world as i32 * 0x40 + (o.y0 / 512) * 8 + o.x0 / 512) as usize;
                    let cur = parents.get(cur).map_or(cur, |&p| p as usize);
                    let mut c = area_pixel_live(areas, parents, &o.tab, o.world, wx, wy, self.pal.is_none().then_some((cur, self.cgram))).or_else(|| world_pixel(cache, wmap, o.world, wx, wy));
                    // Palette wird gerade überblendet (z. B. Teleportation): die gespeicherten Farben genauso mitblenden
                    if let (Some(v), Some(f)) = (c, self.pal) {
                        c = Some(apply_pal(v, f));
                    }
                    // Nachbargebiet mit Nebel-Overlay, hier aber keiner aktiv: Nebel wie im Spiel halb dazumischen
                    if let (Some(v), false) = (c, fog_live) {
                        let id = (o.world as i32 * 0x40 + (wy / 512) * 8 + wx / 512) as usize;
                        if let Some(&f) = parents.get(id).and_then(|&p| fogs.get(p as usize)) {
                            if f & 0x8000 != 0 {
                                let h = |s: u16| ((((v >> s) & 31) + ((f >> s) & 31)) >> 1) << s;
                                c = Some(h(0) | h(5) | h(10));
                            }
                        }
                    }
                    match c {
                        Some(c) => (OPAQUE | c as u32, false),
                        None => (VOID, false),
                    }
                } else {
                    let m = o.m16.get(((lyy >> 4) * 64 + (lx >> 4)) as usize).copied().unwrap_or(0) as usize;
                    let q = (((lyy >> 3) & 1) * 2 + ((lx >> 3) & 1)) as usize;
                    let e = o.tab.get(m * 4 + q).copied().unwrap_or(0) as u32;
                    pix(e, (lx & 7) as u32, (lyy & 7) as u32)
                };
                // (Abdunkeln hinter dem Weltrand aus: mit 5-Bit-Farben gab es sichtbare senkrechte Stufen)
                if false && over > 0 && res.0 & OPAQUE != 0 && res.0 & VOID == 0 {
                    // sanft abdunkeln, nie schwarz
                    let f = 1.0 - (over as f32 / 200.0).min(0.35);
                    let c = res.0 & 0x7FFF;
                    let sc = |v: u32| ((v as f32 * f) as u32).min(31);
                    let d = sc(c & 31) | sc((c >> 5) & 31) << 5 | sc((c >> 10) & 31) << 10;
                    return ((res.0 & !0x7FFF) | d, res.1);
                }
                return res;
            }
            let xx = ((mx + hofs) & 0x3FF) as u32;
            let tx = (xx / tsz) & 63;
            let mut addr = row_addr + (tx & 31);
            if tx & 32 != 0 && wide {
                addr += 0x400;
            }
            let ai = (addr & 0x7FFF) as usize;
            // sonst am Rand: nur Kacheln zeigen, die das Spiel für die aktuelle Kameraposition geschrieben hat
            // (Oberwelt-BG1 = Wetter-/Nebel-Overlay: einfach fortsetzen)
            if ext > 0 && full_map && !(0..256).contains(&sx) && !(n == 0 && self.margin == MARGIN_WORLD) && room.is_none() {
                if let Some(&m) = self.tmcam.get(ai) {
                    if m & CAM_KNOWN != 0 {
                        let h0 = (m & 0x3FF) as i32;
                        let v0 = ((m >> 10) & 0x3FF) as i32;
                        if wrap(xx as i32 - (h0 + 128)).abs() > 256 || wrap(yy as i32 - (v0 + 112)).abs() > 256 {
                            return (VOID, false);
                        }
                    }
                }
            }
            let res = match (from_keep, keep_vram, from_next, next_vram) {
                _ if from_side.is_some() => {
                    let sv = from_side.unwrap();
                    pix_in(sv, sv[ai] as u32, xx % tsz, yy % tsz)
                }
                (true, Some(kv), _, _) => pix_in(kv, kv[ai] as u32, xx % tsz, yy % tsz),
                (_, _, true, Some(nv)) => pix_in(nv, nv[ai] as u32, xx % tsz, yy % tsz),
                _ => pix(vram[ai] as u32, xx % tsz, yy % tsz),
            };
            res
        };
        // Statusleiste (BG3, Kacheln mit Priorität): Items nach links, Energie nach rechts schieben
        let hud_line = ext > 0 && self.hud && n == 2 && mode == 1 && line <= HUD_LINES;
        // andere 32 Kacheln breite Ebenen nicht in den Rand wiederholen (BG3 darf: Regen, Nebel)
        // Nebel/Regen der Oberwelt: BG1 nur auf dem Sub-Bildschirm (Verlorene Wälder, Moor …) -> Muster fortsetzen
        let fog = n == 0 && self.margin == MARGIN_WORLD && rg.ts & 1 != 0 && rg.tm & 1 == 0;
        let wrap_ok = wide || ext == 0 || n == 2 || fog;
        for (i, o) in out[..self.w].iter_mut().enumerate() {
            let sx = i as i32 - ext;
            let margin = !(0..256).contains(&sx);
            if margin && !hud_line && rg.bgmode & 7 != 7 {
                match self.margin {
                    MARGIN_BLANK => {
                        // Titel/Dateiauswahl: den Hintergrund am Bildrand seitlich fortsetzen
                        let src = if sx < 0 { sx.rem_euclid(16) } else { 240 + (sx - 256).rem_euclid(16) };
                        *o = sample(src).0;
                        continue;
                    }
                    MARGIN_KEEP => {
                        *o = 0;
                        continue;
                    }
                    MARGIN_ROOM if room.is_none() && n != 2 => {
                        *o = 0;
                        continue;
                    }
                    _ => {}
                }
            }
            if hud_line {
                let src = if sx >= HUD_SPLIT + ext {
                    Some(sx - ext)
                } else if sx < HUD_SPLIT - ext {
                    Some(sx + ext)
                } else {
                    None
                };
                if let Some(x2) = src.filter(|x| (0..256).contains(x)) {
                    let (p, prio) = sample(x2);
                    if prio && p != 0 {
                        *o = p;
                        continue;
                    }
                }
                let (p, prio) = sample(sx);
                *o = if prio || margin { 0 } else { p };
                continue;
            }
            if !wrap_ok && margin {
                *o = 0;
                continue;
            }
            // BG3 (Statusleiste, Textboxen, Menüs) nie in den Rand wiederholen
            if margin && n == 2 {
                *o = 0;
                continue;
            }
            *o = sample(sx).0;
        }
    }

    fn render_m7(&self, line: u16, bg: &mut [Line; 4]) {
        let rg = self.r;
        let (a, b, c, d) = (rg.m7a as i32, rg.m7b as i32, rg.m7c as i32, rg.m7d as i32);
        let (cx, cy) = (rg.m7x as i32, rg.m7y as i32);
        let (hofs, vofs) = (rg.m7hofs as i32, rg.m7vofs as i32);
        let clip = |v: i32| if v & 0x2000 != 0 { v | !0x3FF } else { v & 0x3FF };
        let yy = if rg.m7sel & 2 != 0 { 255 - line as i32 } else { line as i32 };
        let psx = ((a * clip(hofs - cx)) & !63) + ((b * clip(vofs - cy)) & !63) + ((b * yy) & !63) + (cx << 8);
        let psy = ((c * clip(hofs - cx)) & !63) + ((d * clip(vofs - cy)) & !63) + ((d * yy) & !63) + (cy << 8);
        let extbg = rg.setini & 0x40 != 0;
        let direct = rg.cgwsel & 1 != 0;
        let repeat = rg.m7sel >> 6;
        for i in 0..self.w {
            let sx = i as i32 - self.ext;
            let xx = if rg.m7sel & 1 != 0 { 255 - sx } else { sx };
            let mut px = (psx + a * xx) >> 8;
            let mut py = (psy + c * xx) >> 8;
            if !(0..256).contains(&sx) {
                // Breitbildrand: außerhalb der Karte spiegeln (Wolken laufen weiter)
                // gültige Karte: 0..512 (Welt bei 128..384, ringsum Wolken)
                let m = |v: i32| {
                    let v = v.rem_euclid(1024);
                    if v >= 512 {
                        1023 - v
                    } else {
                        v
                    }
                };
                px = m(px);
                py = m(py);
            }
            let outside = !(0..1024).contains(&px) || !(0..1024).contains(&py);
            let tile: u32 = if outside {
                match repeat {
                    2 => continue,
                    3 => 0,
                    _ => (self.vram[((((py & 1023) >> 3) << 7) | ((px & 1023) >> 3)) as usize] & 0xFF) as u32,
                }
            } else {
                (self.vram[(((py >> 3) << 7) | (px >> 3)) as usize] & 0xFF) as u32
            };
            let idx = (tile * 64 + ((py & 7) as u32) * 8 + (px & 7) as u32) & 0x7FFF;
            let col = (self.vram[idx as usize] >> 8) as u32;
            if col != 0 && self.mask & 1 != 0 {
                let color = if direct { direct_color(col, 0) } else { self.cgram[col as usize] };
                bg[0][i] = OPAQUE | color as u32;
            }
            if extbg && col & 0x7F != 0 && self.mask & 2 != 0 {
                bg[1][i] = OPAQUE | ((col >> 7) << 16) | self.cgram[(col & 0x7F) as usize] as u32;
            }
        }
    }

    fn render_obj(&self, line: u16, out: &mut Line) {
        const SIZES: [(u32, u32, u32, u32); 8] = [
            (8, 8, 16, 16),
            (8, 8, 32, 32),
            (8, 8, 64, 64),
            (16, 16, 32, 32),
            (16, 16, 64, 64),
            (32, 32, 64, 64),
            (16, 32, 32, 64),
            (16, 32, 32, 32),
        ];
        let rg = self.r;
        let (sw, sh, lw, lh) = SIZES[(rg.obsel >> 5) as usize];
        let base = ((rg.obsel & 7) as u32) << 13;
        let gap = ((((rg.obsel >> 3) & 3) as u32) + 1) << 12;
        let first = if rg.oam_prio { ((rg.oam_reload >> 2) & 0x7F) as usize } else { 0 };
        let sy = line as i32 - 1;
        let (lo, hi_x) = (-self.ext, 256 + self.ext);
        for k in 0..128 {
            let i = (first + k) & 127;
            let o = i * 4;
            let hi = (self.oam[512 + (i >> 2)] >> ((i & 3) * 2)) & 3;
            let x9 = self.oam[o] as i32 | (((hi & 1) as i32) << 8);
            let y = self.oam[o + 1] as i32;
            let tile = self.oam[o + 2] as u32;
            let attr = self.oam[o + 3];
            let (w, h) = if hi & 2 != 0 { (lw, lh) } else { (sw, sh) };
            let mut r = ((sy - y) & 0xFF) as u32;
            if r >= h {
                continue;
            }
            // 9-Bit-X: 256..511 = links außerhalb; im Breitbild ggf. rechts daneben
            let mut x = if x9 >= 256 { x9 - 512 } else { x9 };
            if x + (w as i32) <= lo && x9 >= 256 && x9 < hi_x {
                x = x9;
            }
            if x + w as i32 <= lo || x >= hi_x {
                continue;
            }
            if attr & 0x80 != 0 {
                r = h - 1 - r;
            }
            let pal = ((attr >> 1) & 7) as u32;
            let prio = ((attr >> 4) & 3) as u32;
            let tbl = if attr & 1 != 0 { gap } else { 0 };
            let math = ((pal >= 4) as u32) << 20;
            for col in 0..w {
                let px = x + col as i32;
                if px < lo || px >= hi_x {
                    continue;
                }
                let pi = (px - lo) as usize;
                if out[pi] != 0 {
                    continue;
                }
                let c2 = if attr & 0x40 != 0 { w - 1 - col } else { col };
                let tlo = ((tile & 0x0F) + (c2 >> 3)) & 0x0F;
                let thi = ((tile >> 4) + (r >> 3)) & 0x0F;
                let t = (thi << 4) | tlo;
                let addr = (base + tbl + t * 16 + (r & 7)) & 0x7FFF;
                let bit = 7 - (c2 & 7);
                let w0 = self.vram[addr as usize] as u32;
                let w1 = self.vram[((addr + 8) & 0x7FFF) as usize] as u32;
                let c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1) | (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
                if c == 0 {
                    continue;
                }
                out[pi] = OPAQUE | (prio << 16) | math | self.cgram[(128 + pal * 16 + c) as usize] as u32;
            }
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn pick(order: &[(u8, u8)], bg: &[Line; 4], obj: &Line, wins: &[[bool; MAXW]; 6], x: usize, en: u8, wen: u8, use_win: bool) -> (u16, u8, bool) {
        for &(l, p) in order {
            let li = l as usize;
            if en & (1 << l) == 0 {
                continue;
            }
            if use_win && wen & (1 << l) != 0 && wins[li][x] {
                continue;
            }
            let px = if li == 4 { obj[x] } else { bg[li][x] };
            if px & OPAQUE != 0 && ((px >> 16) & 0xF) as u8 == p {
                return ((px & 0x7FFF) as u16, l, px & (1 << 20) != 0);
            }
        }
        (0, 5, false)
    }

    /// Zeichnet eine Zeile (1..=224) in `out[..w]`, optional die Kartenaufnahme (nur BG1+BG2) in `cap`.
    fn draw_line(&self, line: u16, out: &mut [u16; MAXW], cap: Option<&mut [u16; MAXW]>) {
        let rg = self.r;
        let w = self.w;
        if rg.inidisp & 0x80 != 0 {
            out[..w].fill(0);
            if let Some(c) = cap {
                c[..w].fill(0);
            }
            return;
        }
        let mode = rg.bgmode & 7;
        let main_en = rg.tm & self.mask;
        let sub_en = rg.ts & self.mask;
        let need = main_en | sub_en | if cap.is_some() { 0x03 } else { 0 };
        let mut bg = [[0u32; MAXW]; 4];
        let mut obj = [0u32; MAXW];
        if mode == 7 {
            if need & 3 != 0 {
                self.render_m7(line, &mut bg);
            }
        } else {
            let bpps: [u32; 4] = match mode {
                0 => [2, 2, 2, 2],
                1 => [4, 4, 2, 0],
                2 => [4, 4, 0, 0],
                3 => [8, 4, 0, 0],
                4 => [8, 2, 0, 0],
                5 => [4, 2, 0, 0],
                _ => [4, 0, 0, 0],
            };
            for n in 0..4 {
                if bpps[n] != 0 && need & (1 << n) != 0 {
                    self.render_bg(n, bpps[n], mode, line, &mut bg[n]);
                }
            }
        }
        if need & 0x10 != 0 {
            self.render_obj(line, &mut obj);
        }
        // Oberwelt: zeigt die Mitte in dieser Zeile gar keine Landschaft (z. B. kurz bei der Teleportation in die andere
        // Welt, wenn nur die Hintergrundfarbe zu sehen ist), dann auch im Rand keine – sonst stünde dort schon die neue Welt
        if self.margin == MARGIN_WORLD && self.ext > 0 && mode != 7 {
            let e = self.ext as usize;
            if (e..(e + 256).min(w)).all(|x| (bg[0][x] | bg[1][x]) & OPAQUE == 0) {
                for n in 0..2 {
                    bg[n][..e].fill(0);
                    if e + 256 < w {
                        bg[n][e + 256..w].fill(0);
                    }
                }
            }
        }
        let mut wins = [[false; MAXW]; 6];
        self.window_mask(rg.w12sel & 0xF, rg.wbglog, line, &mut wins[0]);
        self.window_mask(rg.w12sel >> 4, rg.wbglog >> 2, line, &mut wins[1]);
        self.window_mask(rg.w34sel & 0xF, rg.wbglog >> 4, line, &mut wins[2]);
        self.window_mask(rg.w34sel >> 4, rg.wbglog >> 6, line, &mut wins[3]);
        self.window_mask(rg.wobjsel & 0xF, rg.wobjlog, line, &mut wins[4]);
        self.window_mask(rg.wobjsel >> 4, rg.wobjlog >> 2, line, &mut wins[5]);
        let order: &[(u8, u8)] = match mode {
            0 => &[(4, 3), (0, 1), (1, 1), (4, 2), (0, 0), (1, 0), (4, 1), (2, 1), (3, 1), (4, 0), (2, 0), (3, 0)],
            1 => {
                if rg.bgmode & 8 != 0 {
                    &[(2, 1), (4, 3), (0, 1), (1, 1), (4, 2), (0, 0), (1, 0), (4, 1), (2, 0), (4, 0)]
                } else {
                    &[(4, 3), (0, 1), (1, 1), (4, 2), (0, 0), (1, 0), (4, 1), (2, 1), (4, 0), (2, 0)]
                }
            }
            7 => &[(4, 3), (4, 2), (1, 1), (4, 1), (0, 0), (4, 0), (1, 0)],
            _ => &[(4, 3), (0, 1), (4, 2), (1, 1), (4, 1), (0, 0), (4, 0), (1, 0)],
        };
        let fixed = (rg.fixed[0] as u16) | ((rg.fixed[1] as u16) << 5) | ((rg.fixed[2] as u16) << 10);
        let bright = (rg.inidisp & 0xF) as u16;
        let use_sub = rg.cgwsel & 2 != 0;
        for x in 0..w {
            let (mc, ms, mobj) = Self::pick(order, &bg, &obj, &wins, x, main_en, rg.tmw, true);
            if ms == 5 && (bg[0][x] | bg[1][x]) & VOID != 0 {
                // nichts geladen (hinter dem Gebietsrand): schwarz lassen
                out[x] = 0;
                continue;
            }
            let main_color = if ms == 5 { self.cgram[0] } else { mc };
            let cw = wins[5][x];
            let black = cond(rg.cgwsel >> 6, cw);
            let prevent = cond(rg.cgwsel >> 4, cw);
            let c = if black { 0 } else { main_color };
            let math_on = match ms {
                0..=3 => rg.cgadsub & (1 << ms) != 0,
                4 => rg.cgadsub & 0x10 != 0 && mobj,
                _ => rg.cgadsub & 0x20 != 0,
            };
            let mut sub_c: Option<(u16, bool)> = None;
            if math_on && !prevent {
                let (sc, nohalf) = if use_sub {
                    let (sc, ss, _) = Self::pick(order, &bg, &obj, &wins, x, sub_en, rg.tsw, true);
                    if ss == 5 {
                        (fixed, true)
                    } else {
                        (sc, false)
                    }
                } else {
                    (fixed, false)
                };
                let half = rg.cgadsub & 0x40 != 0 && !black && !nohalf;
                sub_c = Some((sc, half));
            }
            // Farbmischung und Helligkeit (auch für die 4fach-Unterpixel)
            let finish = |c: u16| -> u16 {
                let mut c = c;
                if let Some((sc, half)) = sub_c {
                    c = color_math(c, sc, rg.cgadsub & 0x80 != 0, half);
                }
                if bright != 15 {
                    let r = ((c & 31) * (bright + 1)) >> 4;
                    let g = (((c >> 5) & 31) * (bright + 1)) >> 4;
                    let b = (((c >> 10) & 31) * (bright + 1)) >> 4;
                    c = r | (g << 5) | (b << 10);
                }
                c
            };
            let c = finish(c);
            out[x] = c;
        }
        // Nebel-Overlay (BG1 auf dem Subscreen, halb auf BG2 addiert): mittlere Farbe sammeln
        if let Some(acc) = self.fog_acc {
            if self.margin == MARGIN_WORLD && rg.ts & 1 != 0 && rg.cgadsub & 0x02 != 0 && rg.cgadsub & 0x80 == 0 && need & 1 != 0 {
                let mut a = [0u32; 4];
                let e = self.ext.max(0) as usize;
                for x in (e..(e + 256).min(w)).step_by(2) {
                    let v = bg[0][x];
                    if v & OPAQUE != 0 {
                        a[0] += v & 31;
                        a[1] += (v >> 5) & 31;
                        a[2] += (v >> 10) & 31;
                        a[3] += 1;
                    }
                }
                for k in 0..4 {
                    acc[k].fetch_add(a[k], std::sync::atomic::Ordering::Relaxed);
                }
            }
        }
        if let Some(capb) = cap {
            for x in 0..w {
                let (c, src, _) = Self::pick(order, &bg, &obj, &wins, x, self.cap_mask, 0, false);
                capb[x] = if src == 5 { self.cgram[0] } else { c };
            }
        }
    }
}
