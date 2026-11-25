//! Eigene Karte für The Minish Cap (PC-Fassung) im Stil der Karte von A Link to the Past: Reiter OBERWELT,
//! GEBIET, 100 % und FUNDORTE, Link-Punkt, Fundorte (Herzteile, Truhen, Kinsteine …) mit „schon geholt“,
//! komplett per Controller (L/R Reiter, Stick bewegen, rechter Stick Zoom, Y Link, X Spoiler, B zurück).
//!
//! Öffnet mit Minus (Karte-Taste) oder wenn man im Item-Menü zur Kartenseite blättert – die Karte des Spiels
//! wird dann nicht mehr gezeigt. Im Item-Menü führen L/R am Rand zurück zu den Seiten des Spiels.

use super::tmcmap::{pretty, Loc, LocKind, MapData, MapGen, Space, CH, LOGIC};
use crate::app::App;
use crate::gfx::{self, Color32, Stroke};
use crate::i18n::tr;
use crate::ui::{paint_icon, Icon, CHEST, DUNGEON, GAME, GREYC, HEART};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

const GOLD: Color32 = Color32::from_rgb(232, 194, 88);
const GREEN: Color32 = Color32::from_rgb(120, 230, 120);
const KINSTONE: Color32 = Color32::from_rgb(120, 210, 110);
const WATER: Color32 = Color32::from_rgb(90, 160, 240);
const FIGURE: Color32 = Color32::from_rgb(250, 160, 60);
const RUPEE: Color32 = Color32::from_rgb(80, 220, 120);

const B: u16 = 1 << 15;
const Y: u16 = 1 << 14;
const START: u16 = 1 << 12;
const UP: u16 = 1 << 11;
const DOWN: u16 = 1 << 10;
const LEFT: u16 = 1 << 9;
const RIGHT: u16 = 1 << 8;
const A: u16 = 1 << 7;
const X: u16 = 1 << 6;
const L: u16 = 1 << 5;
const R: u16 = 1 << 4;

/// Reiter: 0 Oberwelt, 1 Dungeons/Räume, 2 100 %, 3 Quests, 4 Fundorte
const TABS: u8 = 5;

/// Quest-Seite: (deutsch, englisch, Hauptquest, Wert aus tmc_progress, Ziel)
const QUESTS: &[(&str, &str, bool, usize, i32)] = &[
    ("Waldschrein: Erd-Element", "Deepwood Shrine: Earth Element", true, 0, 1),
    ("Flammenhöhle: Feuer-Element", "Cave of Flames: Fire Element", true, 1, 1),
    ("Festung der Winde: Okarina", "Fortress of Winds: Ocarina", true, 2, 1),
    ("Tempel der Tropfen: Wasser-Element", "Temple of Droplets: Water Element", true, 3, 1),
    ("Palast der Winde: Wind-Element", "Palace of Winds: Wind Element", true, 4, 1),
    ("Vierschwert", "Four Sword", true, 5, 1),
    ("Flaschen", "Bottles", false, 6, 4),
    ("Figuren", "Figurines", false, 7, 136),
    ("Kinstein-Fusionen", "Kinstone fusions", false, 8, 100),
    ("Schwerttechniken", "Sword techniques", false, 9, 8),
    ("Herzen", "Hearts", false, 10, 20),
    ("Geldbörse", "Wallet", false, 11, 3),
    ("Bombentasche", "Bomb bag", false, 12, 3),
    ("Köcher", "Quiver", false, 13, 3),
    ("Kinstein-Beutel", "Kinstone bag", false, 14, 1),
    ("Greifring", "Grip ring", false, 15, 1),
    ("Kraftarmbänder", "Power bracelets", false, 16, 1),
    ("Schwimmflossen", "Flippers", false, 17, 1),
    ("Bücher der Bibliothek", "Library books", false, 18, 3),
    ("Tingles Trophäe", "Tingle trophy", false, 19, 1),
    ("Carlovs Medaille", "Carlov medal", false, 20, 1),
    ("Lon-Lon-Schlüssel", "Lon Lon key", false, 21, 1),
    ("Friedhofsschlüssel", "Graveyard key", false, 22, 1),
    ("Lichtpfeile", "Light arrows", false, 23, 1),
    ("Spiegelschild", "Mirror shield", false, 24, 1),
    ("Magischer Bumerang", "Magic boomerang", false, 25, 1),
];
/// alle so viele Spielbilder ein Bild ohne Figuren in die Karte eintragen
const CAPTURE_EVERY: u64 = 12;
/// Spalten der Liste „Fundorte“: Arten, dann Items und Gebiete
const LIST_ITEMS: usize = LocKind::ALL.len();
const LIST_CATS: usize = LocKind::ALL.len() + 2;

pub struct MapUi {
    pub open: bool,
    /// aus dem Item-Menü des Spiels geöffnet (L/R am Rand blättern dort weiter)
    from_menu: bool,
    pub data: Option<MapData>,
    rom: Option<PathBuf>,
    tex: HashMap<(Space, i32, i32), gfx::TextureHandle>,
    tab: u8,
    /// Reiter „Gebiet“: gezeigtes Gebiet
    space: Space,
    /// Mitte des Ausschnitts (Weltpixel) und Zoom (Bildschirmpunkte je Weltpixel) je Raum
    views: HashMap<Space, (gfx::Vec2, f32)>,
    spoiler: bool,
    list_cat: usize,
    list_sel: usize,
    prev_pad: u16,
    rep: ((i32, i32), f32),
    /// Link: Raum und Weltposition, Gebiet, Raum im Gebiet
    link: Option<(Space, f32, f32)>,
    last_cap: u64,
    saved: Instant,
    names: HashMap<u8, String>,
    done_at: Instant,
    items: (usize, usize),
    owned: Vec<u8>,
    /// Auswahl auf der Karte (Index in `locs`) – für die Fußzeile
    pick: Option<usize>,
    /// Ausschnitt für Mausbedienung
    map_rect: gfx::Rect,
    /// Entwicklung: zuletzt ausgelöste Karte-Taste (PASTLINK_GBA_KEYS „m:bild“), Anzeige-Bilder seit dem Öffnen
    pub dev_m: u32,
    dev_ui: u32,
    /// ganze Karte wird gerade erstellt (einmalig je ROM)
    gen: Option<MapGen>,
    /// Werte für die Quest-Seite, gewählte Quest
    progress: Vec<i32>,
    quest_sel: usize,
    /// Gebietsnamen aus Picori
    area_names: HashMap<u8, String>,
    /// Fadenkreuz (Abstand zur Kartenmitte, Bildschirmpunkte) und halbe Kartengröße – wie bei A Link to the Past
    cur: gfx::Vec2,
    half: gfx::Vec2,
    /// Anzeige (Y, wie bei A Link to the Past): 0 Übersicht, 1 Sammeln, 2 Schnellreise, 3 Alles
    filter: u8,
    /// eigene Markierungen (Raum, x, y, Symbol) und besuchte Räume (Schnellreise), gespeichert in `<rom>.tmcpins`
    pins: Vec<(Space, f32, f32, u8)>,
    visited: std::collections::BTreeSet<(u8, u8)>,
    pins_dirty: bool,
    /// Markierung wird gesetzt: Raum, Position, gewähltes Symbol
    pin_pick: Option<(Space, f32, f32, u8)>,
    /// Auswahl unter dem Fadenkreuz
    sel: Option<Sel>,
    /// Schnellreise läuft: Gebiet, Raum, Ziel im Raum, Schritt, Zähler
    travel: Option<(u8, u8, i32, i32, u8, u32)>,
    /// Raum-Rechtecke je (Gebiet, Raum) für Schnellreise-Punkte, Namen der Oberwelt-Gebiete (Mitte)
    rects: HashMap<(u8, u8), ([i32; 4], Space)>,
    labels: Vec<(String, f32, f32)>,
}

/// Auswahl auf der Karte
#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Loc(usize),
    Travel(u8, u8),
    Pin(usize),
    /// besonderer Punkt (Index in `MapData::marks`)
    Mark(usize),
}

/// Symbole für eigene Markierungen (wie bei A Link to the Past)
const PIN_ICONS: [Icon; 5] = [Icon::Star, Icon::Quest, Icon::Question, Icon::Cross, Icon::Heart];
const DOOR: Color32 = Color32::from_rgb(80, 216, 224);

impl Default for MapUi {
    fn default() -> Self {
        MapUi {
            open: false,
            from_menu: false,
            data: None,
            rom: None,
            tex: HashMap::new(),
            tab: 0,
            space: 0,
            views: HashMap::new(),
            spoiler: true,
            list_cat: 0,
            list_sel: 0,
            prev_pad: 0,
            rep: ((0, 0), 0.0),
            link: None,
            last_cap: 0,
            saved: Instant::now(),
            names: HashMap::new(),
            done_at: Instant::now(),
            items: (0, 18),
            owned: Vec::new(),
            pick: None,
            map_rect: gfx::Rect::NOTHING,
            dev_m: u32::MAX,
            dev_ui: 0,
            gen: None,
            progress: Vec::new(),
            quest_sel: 0,
            area_names: HashMap::new(),
            cur: gfx::Vec2::ZERO,
            half: gfx::vec2(200.0, 150.0),
            filter: 0,
            pins: Vec::new(),
            visited: Default::default(),
            pins_dirty: false,
            pin_pick: None,
            sel: None,
            travel: None,
            rects: HashMap::new(),
            labels: Vec::new(),
        }
    }
}

/// Raum auf der Karte für ein Gebiet: Oberwelt = gemeinsame Koordinaten
fn space_of(area: u8, kind: i32) -> Space {
    if kind & 1 != 0 {
        0
    } else {
        area as Space + 1
    }
}

fn kind_look(k: LocKind) -> (Icon, Color32) {
    match k {
        LocKind::HeartPiece => (Icon::Heart, HEART),
        LocKind::HeartContainer => (Icon::Container, HEART),
        LocKind::Chest => (Icon::Chest, CHEST),
        LocKind::Kinstone => (Icon::Crystal, KINSTONE),
        LocKind::Figurine => (Icon::Star, FIGURE),
        LocKind::Npc => (Icon::Quest, Color32::WHITE),
        LocKind::Dig => (Icon::Question, Color32::from_rgb(200, 160, 110)),
        LocKind::Pot => (Icon::Bottle, Color32::from_rgb(200, 150, 100)),
        LocKind::Underwater => (Icon::Dot, WATER),
        LocKind::Rupees => (Icon::Rupee, RUPEE),
        LocKind::Other => (Icon::Game, GAME),
    }
}

impl App {
    /// nach jedem Spielbild: Karte laden, Seite des Item-Menüs abfangen, Bild ohne Figuren eintragen
    pub(crate) fn gba_map_update(&mut self) {
        let own = self.s.gba_ownmap;
        let Some(g) = self.gba.game.as_mut() else { return };
        if !g.is_minish() || !g.is_recomp() || g.loading() {
            return;
        }
        let m = &mut self.gba.map;
        // Karte zu dieser ROM laden (gesehene Gebiete) und Fundorte aus der Logik von Picori
        if m.rom.as_deref() != Some(g.path.as_path()) {
            if let Some(d) = &mut m.data {
                d.save();
            }
            let mut d = MapData::load(&g.path);
            let raw = g.locations(LOGIC);
            let mut by_area: HashMap<u8, HashMap<String, usize>> = HashMap::new();
            for (name, key, ty) in raw {
                // echte Fundorte (keine Hilfsregeln, Musik, Platzhalter)
                // 3 = fest vergeben, 8–12 = zufällig belegbar, 13 = je nach Einstellung leer (z. B. Kinstein-Truhen)
                if !(ty == 3 || (8..=13).contains(&ty)) || name.contains("Dummy") || name.starts_with("Fake") || key == u32::MAX {
                    continue;
                }
                let scripted = key & 0x8000_0000 != 0;
                let (area, room) = if scripted { (0xFF, 0xFF) } else { ((key >> 16) as u8, (key >> 8) as u8) };
                let at = if scripted {
                    // per Skript vergeben, aber in Hyrule-Stadt (Herzteil an der Glocke, Truhe der Simulation)
                    name.starts_with("Town_").then(|| g.room_rect(2, 0)).flatten().map(|r| (0, r[0] as f32 + r[2] as f32 * 0.5, r[1] as f32 + r[3] as f32 * if name.contains("Bell") { 0.3 } else { 0.5 }))
                } else {
                    g.room_rect(area, room).map(|r| (space_of(area, g.area_kind(area)), r[0] as f32 + r[2] as f32 * 0.5, r[1] as f32 + r[3] as f32 * 0.5))
                };
                if !scripted {
                    let pre = name.split('_').next().unwrap_or("").to_string();
                    *by_area.entry(area).or_default().entry(pre).or_insert(0) += 1;
                }
                d.locs.push(Loc { kind: LocKind::from_name(&name), name, key, area, room, at, done: None });
            }
            // Gebietsnamen: häufigster Namensanfang der Fundorte darin („Deepwood_…“ → Deepwood)
            m.names = by_area.into_iter().filter_map(|(a, c)| c.into_iter().max_by_key(|x| x.1).map(|(n, _)| (a, pretty(&n)))).collect();
            m.area_names = (0u8..0x90).filter_map(|a| g.area_name(a).map(|n| (a, n.0))).collect();
            // Räume aller Gebiete (Schnellreise-Punkte) und Namen der Oberwelt-Gebiete in deren Mitte
            m.rects.clear();
            m.labels.clear();
            for a in 0u8..0x90 {
                let kind = g.area_kind(a);
                // Name in die Mitte des größten Raums des Gebiets
                let mut bb: Option<[i32; 4]> = None;
                for r in 0..g.room_count(a).clamp(0, 0x40) as u8 {
                    if let Some(rr) = g.room_rect(a, r) {
                        let sp = space_of(a, kind);
                        m.rects.insert((a, r), (rr, sp));
                        if sp == 0 && bb.is_none_or(|b| b[2] * b[3] < rr[2] * rr[3]) {
                            bb = Some(rr);
                        }
                    }
                }
                if let (Some(b), Some(n)) = (bb, m.area_names.get(&a)) {
                    // Sonderfassungen (Stadt beim Fest) liegen mit falschen Koordinaten auf anderen Gebieten
                    if b[2] >= 200 && !n.contains("Festival") {
                        m.labels.push((n.clone(), b[0] as f32 + b[2] as f32 * 0.5, b[1] as f32 + b[3] as f32 * 0.5));
                    }
                }
            }
            let (pins, visited) = load_pins(&g.path);
            m.pins = pins;
            m.visited = visited;
            d.locs_loaded = true;
            m.data = Some(d);
            m.rom = Some(g.path.clone());
            m.tex.clear();
            m.views.clear();
        }
        // eigene Karte an: das Item-Menü hat keine Kartenseite mehr (L/R wechselt Items und Quest-Status)
        g.set_own_map(own);
        // ganze Karte einmalig erstellen, sobald die Karte offen ist (das Spiel steht dabei; danach genau wie vorher)
        if m.open && m.gen.is_none() && m.data.as_ref().is_some_and(|d| !d.complete()) {
            m.gen = MapGen::new(g);
            if m.gen.is_some() {
                if let Some(d) = m.data.as_mut() {
                    d.marks.clear();
                }
            }
        }
        if let (Some(gen), Some(d)) = (m.gen.as_mut(), m.data.as_mut()) {
            gen.step(g, d, std::time::Duration::from_millis(14));
            if gen.done {
                m.gen = None;
                d.save();
                // Ausschnitte neu berechnen (die Karte ist jetzt viel größer als während des Erstellens)
                m.views.clear();
                m.cur = gfx::Vec2::ZERO;
            }
            return;
        }
        // Link und Ausschnitt; besuchte Räume (Schnellreise)
        let view = g.map_view();
        if let Some(v) = view {
            let sp = space_of(v[0] as u8, g.area_kind(v[0] as u8));
            m.link = Some((sp, v[9] as f32, v[10] as f32));
            if self.gba.hud && m.visited.insert((v[0] as u8, v[1] as u8)) {
                m.pins_dirty = true;
            }
        }
        // Schnellreise: springen, auf das Ende des Übergangs warten, Link auf eine freie Stelle stellen
        if let Some((a, r, x, y, step, n)) = m.travel {
            let n = n + 1;
            m.travel = match step {
                0 if g.travel(a, r, x, y) => Some((a, r, x, y, 1, 0)),
                0 if n < 300 => Some((a, r, x, y, 0, n)),
                1 if view.is_some_and(|v| v[0] == a as i32 && v[1] == r as i32) && n > 20 => {
                    g.place_safe();
                    None
                }
                1 if n < 900 => Some((a, r, x, y, 1, n)),
                _ => None,
            };
        }
        if m.pins_dirty {
            m.pins_dirty = false;
            save_pins(&g.path, &m.pins, &m.visited);
        }
        // gesehene Gebiete eintragen: Bild ohne Figuren an seiner Weltposition (nur im Raum, nur im Spiel)
        let tick = self.gba.play_frames;
        if !m.open && self.gba.hud && tick >= m.last_cap + CAPTURE_EVERY {
            m.last_cap = tick;
            if let (Some(v), Some(d)) = (view, m.data.as_mut()) {
                if let Some((px, w, h)) = g.map_render_bg() {
                    let sp = space_of(v[0] as u8, g.area_kind(v[0] as u8));
                    let (ox, oy, sx, sy, rw, rh) = (v[3], v[4], v[5], v[6], v[7], v[8]);
                    // nur der Teil, der im Raum liegt (außerhalb zeichnet das Spiel Reste)
                    let x0 = sx.max(ox);
                    let y0 = sy.max(oy);
                    let x1 = (sx + w as i32).min(ox + rw);
                    let y1 = (sy + h as i32).min(oy + rh);
                    if x1 > x0 && y1 > y0 {
                        d.paste(sp, x0, y0, &px, w, (x0 - sx) as usize, (y0 - sy) as usize, (x1 - x0) as usize, (y1 - y0) as usize);
                    }
                }
            }
        }
        // „schon geholt“ regelmäßig neu lesen
        if m.done_at.elapsed().as_secs_f32() > if m.open { 0.5 } else { 3.0 } {
            m.done_at = Instant::now();
            if let Some(d) = m.data.as_mut() {
                for l in &mut d.locs {
                    l.done = g.loc_done(l.key);
                }
            }
            m.owned = g.owned_items();
            m.items = (m.owned.len(), 18);
            let p = g.progress();
            if !p.is_empty() {
                m.progress = p;
            }
        }
        if m.saved.elapsed().as_secs() > 60 {
            m.saved = Instant::now();
            if let Some(d) = m.data.as_mut() {
                d.save();
            }
        }
    }

    /// Karte auf/zu (Minus, Karte-Taste)
    pub(crate) fn gba_map_toggle(&mut self) {
        let ok = self.s.gba_ownmap && self.gba.game.as_ref().is_some_and(|g| g.is_minish() && g.is_recomp() && !g.loading());
        if !ok {
            return;
        }
        if self.gba.map.open {
            self.gba_map_close(false);
            return;
        }
        if !self.gba.hud {
            let m = self.t("Karte erst im Spiel verfügbar", "Map is only available in game");
            self.msg(m);
            return;
        }
        let m = &mut self.gba.map;
        m.open = true;
        m.from_menu = false;
        m.prev_pad = 0xFFFF;
        m.dev_ui = 0;
        // im Haus/Dungeon gleich das Gebiet zeigen
        m.tab = if m.link.is_some_and(|l| l.0 != 0) { 1 } else { 0 };
        if let Some(l) = m.link.filter(|l| l.0 != 0) {
            m.space = l.0;
        }
        self.gba_map_center_link();
    }

    /// schließen; `close_menu` = auch das Item-Menü des Spiels schließen
    fn gba_map_close(&mut self, close_menu: bool) {
        let _ = close_menu;
        let m = &mut self.gba.map;
        // Erstellen abgebrochen: Spiel zurück auf den Stand vor dem Erstellen (das Bisherige bleibt)
        if let (Some(gen), Some(g)) = (m.gen.take(), self.gba.game.as_mut()) {
            gen.abort(g);
        }
        m.open = false;
        if let Some(d) = m.data.as_mut() {
            d.save();
        }
        m.from_menu = false;
    }

    fn gba_map_space(&self) -> Space {
        if self.gba.map.tab == 1 {
            self.gba.map.space
        } else {
            0
        }
    }

    fn gba_map_center_link(&mut self) {
        let sp = self.gba_map_space();
        let m = &mut self.gba.map;
        if let Some((ls, x, y)) = m.link {
            if ls == sp {
                let z = m.views.get(&sp).map_or(0.0, |v| v.1);
                m.views.insert(sp, (gfx::vec2(x, y), z));
            }
        }
    }

    /// Gebiete mit Bild oder Fundorten (Reiter „Gebiet“), sortiert
    fn gba_map_spaces(&self) -> Vec<Space> {
        let Some(d) = &self.gba.map.data else { return Vec::new() };
        let mut v: Vec<Space> = d.chunks.keys().map(|k| k.0).chain(d.locs.iter().filter_map(|l| l.at.map(|a| a.0))).filter(|&s| s != 0 && s != 0xFFFF).collect();
        v.sort();
        v.dedup();
        v
    }

    fn gba_space_name(&self, sp: Space) -> String {
        let en = self.s.ui_en;
        if sp == 0 {
            return tr(en, "Oberwelt", "Overworld").to_string();
        }
        let a = (sp - 1) as u8;
        self.gba.map.area_names.get(&a).or_else(|| self.gba.map.names.get(&a)).cloned().unwrap_or_else(|| format!("{} {a:02X}", tr(en, "Gebiet", "Area")))
    }

    fn gba_map_rep(&mut self, pad: u16, dt: f32) -> (i32, i32) {
        let ls = self.lstick;
        let mut d = (0i32, 0i32);
        if pad & LEFT != 0 || ls.x < -0.5 {
            d.0 = -1;
        } else if pad & RIGHT != 0 || ls.x > 0.5 {
            d.0 = 1;
        }
        if pad & UP != 0 || ls.y > 0.5 {
            d.1 = -1;
        } else if pad & DOWN != 0 || ls.y < -0.5 {
            d.1 = 1;
        }
        let (prev, t) = self.gba.map.rep;
        if d == (0, 0) {
            self.gba.map.rep = ((0, 0), 0.0);
            return (0, 0);
        }
        if d != prev {
            self.gba.map.rep = (d, -0.28);
            return d;
        }
        let t = t + dt;
        if t >= 0.11 {
            self.gba.map.rep = (d, 0.0);
            d
        } else {
            self.gba.map.rep = (d, t);
            (0, 0)
        }
    }

    /// Eingaben, solange die Karte offen ist (nichts geht ans Spiel)
    pub(crate) fn gba_map_input(&mut self, pad: u16, dt: f32) {
        // Entwicklung: Tasten auf der Karte (PASTLINK_GBA_MAPKEYS = „taste:bild,…“, Anzeige-Bilder seit dem Öffnen;
        // Q/E = L/R, a/b/x/y, u/d/l/r, z = rechter Stick hinein)
        let mut pad = pad;
        if let Ok(spec) = std::env::var("PASTLINK_GBA_MAPKEYS") {
            let t = self.gba.map.dev_ui;
            self.gba.map.dev_ui += 1;
            for e in spec.split(',') {
                let Some((k, at)) = e.split_once(':') else { continue };
                let Ok(at) = at.parse::<u32>() else { continue };
                if t < at || t >= at + 3 {
                    continue;
                }
                pad |= match k {
                    "a" => A,
                    "b" => B,
                    "x" => X,
                    "y" => Y,
                    "u" => UP,
                    "d" => DOWN,
                    "l" => LEFT,
                    "r" => RIGHT,
                    "Q" => L,
                    "E" => R,
                    "s" => START,
                    _ => 0,
                };
            }
        }
        let m = &mut self.gba.map;
        // Tasten, die beim Öffnen gehalten waren, zählen erst nach dem Loslassen
        if m.prev_pad == 0xFFFF {
            m.prev_pad = pad;
            return;
        }
        let pressed = pad & !m.prev_pad;
        m.prev_pad = pad;
        // Markierung setzen: ←/→ Symbol, A setzen, B abbrechen
        if let Some((sp, x, y, sym)) = m.pin_pick {
            let d = self.gba_map_rep(pad, dt);
            let m = &mut self.gba.map;
            let sym = (sym as i32 + d.0).rem_euclid(PIN_ICONS.len() as i32) as u8;
            m.pin_pick = Some((sp, x, y, sym));
            if pressed & A != 0 {
                m.pins.push((sp, x, y, sym));
                m.pin_pick = None;
                m.pins_dirty = true;
            } else if pressed & (B | START) != 0 {
                m.pin_pick = None;
            }
            return;
        }
        if pressed & B != 0 {
            self.gba_map_close(false);
            return;
        }
        if pressed & START != 0 {
            self.gba_map_close(true);
            return;
        }
        if pressed & (L | R) != 0 {
            let m = &mut self.gba.map;
            let right = pressed & R != 0;
            // (früher: aus dem Item-Menü geöffnet – gibt es nicht mehr, die Karte öffnet nur mit Minus)
            if m.from_menu && ((!right && m.tab == 0) || (right && m.tab == TABS - 1)) {
                let from_map_page = self.gba.game.as_ref().is_some_and(|g| g.map_request() == 2);
                m.open = false;
                m.from_menu = false;
                if let Some(d) = m.data.as_mut() {
                    d.save();
                }
                if let Some(g) = &self.gba.game {
                    // L = Seite links der Karte; R = Items (dort steht das Spiel ohnehin, außer auf der Kartenseite)
                    if !right || from_map_page {
                        g.menu_goto(if right { 2 } else { 1 });
                    }
                }
                return;
            }
            m.tab = if right { (m.tab + 1) % TABS } else { (m.tab + TABS - 1) % TABS };
            if m.tab == 1 && m.space == 0 {
                m.space = m.link.filter(|l| l.0 != 0).map(|l| l.0).unwrap_or(0);
            }
            if self.gba.map.tab == 1 && self.gba.map.space == 0 {
                self.gba.map.space = self.gba_map_spaces().first().copied().unwrap_or(0);
            }
            self.gba.map.pick = None;
            return;
        }
        let tab = self.gba.map.tab;
        if tab <= 1 {
            // wie bei A Link to the Past: X = alles / nur gesammelt, Y = Anzeige (Oberwelt) bzw. nächster Raum
            if pressed & X != 0 {
                self.gba.map.spoiler = !self.gba.map.spoiler;
            }
            if tab == 0 && pressed & Y != 0 {
                self.gba.map.filter = (self.gba.map.filter + 1) % 4;
            }
            // Y: nächster Dungeon/Raum
            if tab == 1 && pressed & Y != 0 {
                let v = self.gba_map_spaces();
                if !v.is_empty() {
                    let i = v.iter().position(|&s| s == self.gba.map.space).map_or(0, |i| (i + 1) % v.len());
                    self.gba.map.space = v[i];
                    self.gba.map.cur = gfx::Vec2::ZERO;
                }
            }
            if pressed & A != 0 {
                self.gba_map_activate();
                return;
            }
            let sp = self.gba_map_space();
            let ls = self.lstick;
            let rs = self.rstick;
            let mut v = gfx::vec2(ls.x, -ls.y);
            if pad & LEFT != 0 {
                v.x -= 1.0;
            }
            if pad & RIGHT != 0 {
                v.x += 1.0;
            }
            if pad & UP != 0 {
                v.y -= 1.0;
            }
            if pad & DOWN != 0 {
                v.y += 1.0;
            }
            if v.length() > 1.0 {
                v = v.normalized();
            }
            let m = &mut self.gba.map;
            let half = m.half;
            if let Some((c, z)) = m.views.get_mut(&sp) {
                let zoom = z.max(0.05);
                // Fadenkreuz frei bewegen; am Rand (65 %) schiebt es die Karte mit – wie bei A Link to the Past
                if v != gfx::Vec2::ZERO {
                    m.cur += v * half.y.max(80.0) * 1.3 * dt;
                    let lim = half * 0.65;
                    let over = m.cur - gfx::vec2(m.cur.x.clamp(-lim.x, lim.x), m.cur.y.clamp(-lim.y, lim.y));
                    *c += over / zoom;
                    m.cur -= over;
                }
                if rs.y.abs() > 0.2 {
                    // um das Fadenkreuz herum zoomen
                    let at = *c + m.cur / zoom;
                    *z = (*z * (rs.y * dt * 2.2).exp()).clamp(0.05, 16.0);
                    *c = at - m.cur / z.max(0.05);
                }
            }
        } else if tab == 2 {
            let d = self.gba_map_rep(pad, dt);
            let m = &mut self.gba.map;
            if d.1 != 0 {
                m.list_cat = (m.list_cat as i32 + d.1).rem_euclid(LIST_CATS as i32) as usize;
            }
            if pressed & A != 0 {
                m.tab = 4;
                m.list_sel = 0;
            }
        } else if tab == 3 {
            let d = self.gba_map_rep(pad, dt);
            let m = &mut self.gba.map;
            if d.1 != 0 {
                m.quest_sel = (m.quest_sel as i32 + d.1).clamp(0, QUESTS.len() as i32 - 1) as usize;
            }
        } else {
            let d = self.gba_map_rep(pad, dt);
            let n = self.gba_list_entries(self.gba.map.list_cat).len();
            let m = &mut self.gba.map;
            if d.0 != 0 {
                m.list_cat = (m.list_cat as i32 + d.0).rem_euclid(LIST_CATS as i32) as usize;
                m.list_sel = 0;
            }
            if d.1 != 0 && n > 0 {
                m.list_sel = (m.list_sel as i32 + d.1).clamp(0, n as i32 - 1) as usize;
            }
            if self.rstick.y.abs() > 0.5 && n > 0 {
                let step = if self.rstick.y > 0.0 { -1 } else { 1 };
                if self.gba_map_rep(if step < 0 { UP } else { DOWN }, dt).1 != 0 {
                    let m = &mut self.gba.map;
                    m.list_sel = (m.list_sel as i32 + step * 4).clamp(0, n as i32 - 1) as usize;
                }
            }
            if pressed & A != 0 {
                // A: Fundort auf der Karte zeigen
                let e = self.gba_list_entries(self.gba.map.list_cat);
                if let Some(at) = e.get(self.gba.map.list_sel).and_then(|e| e.at) {
                    let m = &mut self.gba.map;
                    m.tab = if at.0 == 0 { 0 } else { 1 };
                    if at.0 != 0 {
                        m.space = at.0;
                    }
                    let z = m.views.get(&at.0).map_or(0.0, |v| v.1.max(1.0));
                    m.views.insert(at.0, (gfx::vec2(at.1, at.2), z));
                }
            }
        }
    }

    /// Fortschritt je Art: (geholt, gesamt); nur Fundorte, deren Stand das Spiel kennt
    fn gba_map_counts(&self) -> Vec<(LocKind, usize, usize)> {
        let Some(d) = &self.gba.map.data else { return Vec::new() };
        LocKind::ALL
            .iter()
            .map(|&k| {
                let it = d.locs.iter().filter(|l| l.kind == k && l.done.is_some());
                let (mut a, mut n) = (0, 0);
                for l in it {
                    n += 1;
                    if l.done == Some(true) {
                        a += 1;
                    }
                }
                (k, a, n)
            })
            .filter(|x| x.2 > 0)
            .collect()
    }
}

/// Eintrag der Liste „Fundorte“
pub(crate) struct ListEntry {
    name: String,
    done: Option<bool>,
    at: Option<(Space, f32, f32)>,
    icon: Icon,
    col: Color32,
}

/// Items wie in der Item-Leiste (bei Schwert, Bumerang, Schild zählt jede Stufe als dasselbe Item)
const ITEM_GROUPS: &[&[u8]] = &[&[6, 5, 4, 3, 2, 1], &[12, 11], &[14, 13], &[7], &[8], &[9], &[15], &[17], &[18], &[19], &[20], &[21], &[22], &[23], &[28], &[29], &[30], &[31]];

impl App {
    fn gba_list_cat_name(&self, c: usize) -> String {
        let en = self.s.ui_en;
        if c < LIST_ITEMS {
            LocKind::ALL[c].label(en).to_string()
        } else if c == LIST_ITEMS {
            "Items".to_string()
        } else {
            tr(en, "Gebiete", "Areas").to_string()
        }
    }

    pub(crate) fn gba_list_entries(&self, c: usize) -> Vec<ListEntry> {
        let en = self.s.ui_en;
        let m = &self.gba.map;
        let Some(d) = &m.data else { return Vec::new() };
        if c < LIST_ITEMS {
            let k = LocKind::ALL[c];
            let (icon, col) = kind_look(k);
            let mut v: Vec<ListEntry> = d
                .locs
                .iter()
                .filter(|l| l.kind == k)
                .map(|l| ListEntry { name: pretty(&l.name), done: l.done, at: l.at, icon, col })
                .collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        } else if c == LIST_ITEMS {
            ITEM_GROUPS
                .iter()
                .map(|gr| {
                    let have = gr.iter().find(|i| m.owned.contains(i)).copied();
                    let id = have.unwrap_or(*gr.last().unwrap());
                    ListEntry { name: super::minish::name(id, en).to_string(), done: Some(have.is_some()), at: None, icon: Icon::Star, col: GOLD }
                })
                .collect()
        } else {
            let mut by: HashMap<u8, (usize, usize, Option<(Space, f32, f32)>)> = HashMap::new();
            for l in d.locs.iter().filter(|l| l.area != 0xFF && l.done.is_some()) {
                let e = by.entry(l.area).or_insert((0, 0, None));
                e.1 += 1;
                e.0 += (l.done == Some(true)) as usize;
                if e.2.is_none() {
                    e.2 = l.at;
                }
            }
            let mut v: Vec<ListEntry> = by
                .into_iter()
                .map(|(a, (dn, n, at))| ListEntry { name: format!("{} {dn}/{n}", self.gba_space_name(a as Space + 1)), done: Some(dn >= n), at, icon: Icon::Dungeon, col: DUNGEON })
                .collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        }
    }

    /// Fortschritt in Prozent: alle Fundorte, deren Stand das Spiel kennt, und die Items
    fn gba_map_percent(&self) -> f32 {
        let c = self.gba_map_counts();
        let (a, n) = c.iter().fold((0, 0), |s, x| (s.0 + x.1, s.1 + x.2));
        let (ia, inn) = self.gba.map.items;
        let (qa, qn) = self.gba_quest_score();
        (a + ia) as f32 / (n + inn).max(1) as f32 * 85.0 + qa / qn.max(1.0) * 15.0
    }

    /// Karte zeichnen (über dem Spielbild)
    pub(crate) fn gba_map_draw(&mut self, area: gfx::Rect, ctx: &gfx::Context) {
        if !self.gba.map.open {
            return;
        }
        ctx.request_repaint();
        let old_px = self.game_px;
        let gp = (area.height() / 224.0).round().max(1.0);
        self.game_px = gp;
        let en = self.s.ui_en;
        let painter = ctx.layer_painter(gfx::LayerId::background()).with_clip_rect(area);
        painter.rect_filled(area, 0.0, Color32::from_black_alpha(170));
        let panel = area.shrink(area.width().min(area.height()) * 0.008);
        self.frame(&painter, panel, Color32::from_rgb(10, 12, 32));
        let fs = gp * 8.0;
        let header = fs * 2.0;
        let footer = fs * 3.1;
        let inner = panel.shrink(fs * 0.5);
        let map_rect = gfx::Rect::from_min_max(inner.min + gfx::vec2(0.0, header), inner.max - gfx::vec2(0.0, footer));
        self.gba.map.map_rect = map_rect;
        // ---- Reiter wie bei A Link to the Past
        let tab = self.gba.map.tab;
        let names = [tr(en, "OBERWELT", "OVERWORLD"), tr(en, "DUNGEON/RÄUME", "DUNGEONS/ROOMS"), "100 %", "QUESTS", tr(en, "FUNDORTE", "LOCATIONS")];
        let tabs_l = inner.left() + fs * 2.4;
        let tw = (inner.width() - fs * 4.8) / names.len() as f32;
        let mut ts = fs;
        while names.iter().any(|n| self.text_w(&painter, n, ts) > tw - fs * 0.4) && ts > 8.0 {
            ts -= 8.0;
        }
        for (i, n) in names.iter().enumerate() {
            let c = gfx::pos2(tabs_l + tw * (i as f32 + 0.5), inner.top() + header * 0.45);
            let sel = i as u8 == tab;
            if sel {
                let w = self.text_w(&painter, n, ts) + fs;
                let tb = gfx::Rect::from_center_size(c, gfx::vec2(w, fs * 1.4));
                painter.rect_filled(tb.expand(gp), 0.0, Color32::BLACK);
                painter.rect_filled(tb, 0.0, crate::inventory::MENU_LINE);
            }
            self.mtext(&painter, c, gfx::Align2::CENTER_CENTER, n, ts, if sel { crate::inventory::MENU_BG } else { Color32::from_gray(170) });
        }
        for (txt, x, al) in [("L", inner.left() + fs * 0.6, gfx::Align2::LEFT_CENTER), ("R", inner.right() - fs * 0.6, gfx::Align2::RIGHT_CENTER)] {
            let br = al.anchor_size(gfx::pos2(x, inner.top() + header * 0.45), gfx::vec2(fs * 1.5, fs * 1.2));
            painter.rect_filled(br.expand(gp), 0.0, Color32::BLACK);
            painter.rect_filled(br, 0.0, Color32::from_gray(200));
            self.mtext(&painter, br.center(), gfx::Align2::CENTER_CENTER, txt, fs * 0.8, crate::inventory::MENU_BG);
        }
        // ---- Inhalt
        self.frame(&painter, map_rect.expand(gp), Color32::from_rgb(20, 24, 52));
        let mp = painter.with_clip_rect(map_rect);
        let (info, info2) = match tab {
            2 => self.gba_draw_percent(&mp, map_rect, fs),
            3 => self.gba_draw_quests(&mp, map_rect, fs),
            4 => self.gba_draw_list(&mp, map_rect, fs),
            _ => self.gba_draw_world(ctx, &mp, map_rect, fs),
        };
        // ganze Karte wird erstellt: Fortschritt über der Karte
        if let Some(gen) = &self.gba.map.gen {
            let pr = gen.progress();
            let br = gfx::Rect::from_center_size(map_rect.center(), gfx::vec2((fs * 26.0).min(map_rect.width() * 0.9), fs * 4.2));
            painter.rect_filled(br.expand(gp), 0.0, Color32::BLACK);
            painter.rect_filled(br, 0.0, Color32::from_rgb(10, 12, 32));
            painter.rect_stroke(br, 0.0, Stroke::new(gp, GOLD));
            let t1 = tr(en, "KARTE WIRD ERSTELLT (EINMALIG)", "BUILDING THE MAP (ONCE)");
            self.mtext(&painter, gfx::pos2(br.center().x, br.top() + fs * 1.1), gfx::Align2::CENTER_CENTER, t1, fs, Color32::WHITE);
            let bar = gfx::Rect::from_min_size(gfx::pos2(br.left() + fs, br.top() + fs * 2.3), gfx::vec2(br.width() - fs * 2.0, fs * 0.8));
            painter.rect_filled(bar.expand(gp), 0.0, Color32::BLACK);
            painter.rect_filled(bar, 0.0, Color32::from_rgb(40, 44, 80));
            painter.rect_filled(gfx::Rect::from_min_size(bar.min, gfx::vec2(bar.width() * pr, bar.height())), 0.0, GOLD);
        }
        // ---- Fußzeile: Auswahl links, Fortschritt rechts
        let fy = map_rect.bottom() + fs * 0.6;
        let pct = self.gba_map_percent();
        let pr = self.mtext(&painter, gfx::pos2(inner.right() - fs * 0.4, fy - fs * 0.1), gfx::Align2::RIGHT_TOP, &format!("{pct:.1} % / 100 %"), fs * 1.25, if pct >= 99.95 { GREEN } else { GOLD });
        let maxw = pr.left() - inner.left() - fs * 1.5;
        let info = self.fit_text(&painter, &info, fs, maxw);
        self.mtext(&painter, gfx::pos2(inner.left() + fs * 0.4, fy), gfx::Align2::LEFT_TOP, &info, fs, Color32::WHITE);
        if !info2.is_empty() {
            let info2 = self.fit_text(&painter, &info2, fs * 0.75, maxw);
            self.mtext(&painter, gfx::pos2(inner.left() + fs * 0.4, fy + fs * 1.35), gfx::Align2::LEFT_TOP, &info2, fs * 0.75, Color32::from_gray(185));
        }

        // ---- Maus: Rad = Zoom, Ziehen = verschieben
        if tab <= 1 {
            let (scroll, pos, drag) = ctx.input(|i| (i.smooth_scroll_delta.y, i.pointer.hover_pos(), if i.pointer.primary_down() { i.pointer.delta() } else { gfx::Vec2::ZERO }));
            if pos.is_some_and(|p| map_rect.contains(p)) {
                let sp = self.gba_map_space();
                if let Some((c, z)) = self.gba.map.views.get_mut(&sp) {
                    if scroll.abs() > 0.1 {
                        *z = (*z * (scroll * 0.003).exp()).clamp(0.05, 8.0 * gp);
                    }
                    if drag.length() > 0.0 {
                        *c -= drag / z.max(0.05);
                    }
                }
                if ctx.input(|i| i.pointer.primary_clicked()) {
                    self.gba_map_activate();
                }
            }
        }
        self.game_px = old_px;
    }

    /// Reiter Oberwelt / Dungeon-Räume: Kartenbild, Fundorte, Schnellreise-Punkte, Markierungen, Link, Fadenkreuz
    fn gba_draw_world(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let gp = self.game_px;
        let sp = self.gba_map_space();
        let space_name = self.gba_space_name(sp);
        let t = ctx.input(|i| i.time) as f32;
        // Maus: Zeiger = Fadenkreuz
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if r.contains(pos) && ctx.input(|i| i.pointer.delta().length() > 0.0) {
                self.gba.map.cur = pos - r.center();
            }
        }
        let m = &mut self.gba.map;
        m.half = r.size() * 0.5;
        let Some(d) = m.data.as_mut() else { return (String::new(), String::new()) };
        // Texturen der geänderten Kacheln (einige je Bild, damit nichts ruckelt)
        let mut n = 0;
        while let Some(k) = d.dirty.pop() {
            if k.0 == 0xFFFF {
                continue;
            }
            if let Some(px) = d.chunks.get(&k) {
                let img = gfx::ColorImage::from_rgba_unmultiplied([CH, CH], px);
                match m.tex.get_mut(&k) {
                    Some(t) => t.set(img, gfx::TextureOptions::NEAREST),
                    None => {
                        let t = ctx.load_texture(format!("tmcmap-{}-{}-{}", k.0, k.1, k.2), img, gfx::TextureOptions::NEAREST);
                        m.tex.insert(k, t);
                    }
                }
            }
            n += 1;
            if n >= 24 {
                break;
            }
        }
        // Ausschnitt: beim ersten Mal ganze Karte bzw. um Link
        let mut b: Option<(f32, f32, f32, f32)> = d.bounds(sp).map(|(a, b_, c, e)| (a as f32, b_ as f32, c as f32, e as f32));
        for l in d.locs.iter().filter_map(|l| l.at).filter(|a| a.0 == sp) {
            b = Some(match b {
                None => (l.1 - 64.0, l.2 - 64.0, l.1 + 64.0, l.2 + 64.0),
                Some((a, b_, c, e)) => (a.min(l.1 - 64.0), b_.min(l.2 - 64.0), c.max(l.1 + 64.0), e.max(l.2 + 64.0)),
            });
        }
        let link = m.link.filter(|l| l.0 == sp);
        let (bx0, by0, bx1, by1) = b.unwrap_or_else(|| link.map_or((0.0, 0.0, 256.0, 256.0), |l| (l.1 - 128.0, l.2 - 128.0, l.1 + 128.0, l.2 + 128.0)));
        let fit = (r.width() / (bx1 - bx0).max(64.0)).min(r.height() / (by1 - by0).max(64.0)) * 0.95;
        let v = m.views.entry(sp).or_insert((gfx::vec2(0.0, 0.0), 0.0));
        if v.1 <= 0.0 {
            v.1 = if sp == 0 { fit } else { fit.min(gp * 2.0) };
            v.0 = match link {
                Some(l) if sp != 0 => gfx::vec2(l.1, l.2),
                _ => gfx::vec2((bx0 + bx1) * 0.5, (by0 + by1) * 0.5),
            };
        }
        v.1 = v.1.clamp((fit * 0.6).min(gp), 8.0 * gp);
        v.0.x = v.0.x.clamp(bx0, bx1);
        v.0.y = v.0.y.clamp(by0, by1);
        let (c, z) = *v;
        let overview = z < fit * 1.8;
        let to_screen = |x: f32, y: f32| r.center() + (gfx::vec2(x, y) - c) * z;
        // Hintergrund: Raster
        p.rect_filled(r, 0.0, Color32::from_rgb(14, 18, 40));
        let step = (16.0 * z).max(gp * 6.0);
        let mut gx = r.left() - (c.x * z - r.width() * 0.5).rem_euclid(step);
        while gx < r.right() {
            p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(gx, r.top()), gfx::vec2(gp, r.height())), 0.0, Color32::from_rgb(20, 26, 54));
            gx += step;
        }
        let mut gy = r.top() - (c.y * z - r.height() * 0.5).rem_euclid(step);
        while gy < r.bottom() {
            p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(r.left(), gy), gfx::vec2(r.width(), gp)), 0.0, Color32::from_rgb(20, 26, 54));
            gy += step;
        }
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        for (&(s, cx, cy), tx) in &m.tex {
            if s != sp {
                continue;
            }
            let a = to_screen((cx * CH as i32) as f32, (cy * CH as i32) as f32);
            let rr = gfx::Rect::from_min_size(a, gfx::vec2(CH as f32 * z, CH as f32 * z));
            if rr.intersects(r) {
                p.image(tx.id(), rr, uv, Color32::WHITE);
            }
        }
        // Symbole sammeln: (Position, Auswahl, Symbol, Farbe, Punkt in der Übersicht)
        let rad = fs * 0.55;
        let filt = m.filter.min(3);
        let spoiler = m.spoiler;
        // Anzeige (Y): 0 Übersicht, 1 Sammeln, 2 Schnellreise, 3 Alles
        let show_travel = filt != 1;
        let show_special = filt != 1;
        let mut marks: Vec<(gfx::Pos2, Sel, Icon, Color32, bool)> = Vec::new();
        if filt != 2 {
            for (i, l) in d.locs.iter().enumerate() {
                let Some(at) = l.at.filter(|a| a.0 == sp) else { continue };
                let done = l.done == Some(true);
                // Übersicht: nur Herzteile und Truhen; X: ohne „alles“ nur Gesammeltes
                if (filt == 0 && !matches!(l.kind, LocKind::HeartPiece | LocKind::Chest)) || (!spoiler && !done) {
                    continue;
                }
                let (icon, col) = kind_look(l.kind);
                // wie bei A Link to the Past: gesammelt = Farbe, fehlt noch = grau
                marks.push((to_screen(at.1, at.2), Sel::Loc(i), icon, if done { col } else { GREYC }, true));
            }
        }
        if show_special {
            for (i, mk) in d.marks.iter().enumerate() {
                if mk.0 != sp || (filt == 2 && mk.1 != 2) {
                    continue;
                }
                let (icon, col) = match mk.1 {
                    1 => (Icon::Dot, Color32::from_rgb(190, 120, 255)),
                    2 => (Icon::Note, Color32::from_rgb(140, 230, 255)),
                    _ => (Icon::Upgrade, Color32::from_rgb(110, 220, 90)),
                };
                marks.push((to_screen(mk.2, mk.3), Sel::Mark(i), icon, col, false));
            }
        }
        if show_travel {
            for &(a, rm) in &m.visited {
                if let Some((rr, s)) = m.rects.get(&(a, rm)) {
                    if *s == sp {
                        marks.push((to_screen(rr[0] as f32 + rr[2] as f32 * 0.5, rr[1] as f32 + rr[3] as f32 * 0.5), Sel::Travel(a, rm), Icon::Door, DOOR, false));
                    }
                }
            }
        }
        for (i, pn) in m.pins.iter().enumerate() {
            if pn.0 == sp {
                marks.push((to_screen(pn.1, pn.2), Sel::Pin(i), PIN_ICONS[pn.3 as usize % PIN_ICONS.len()], Color32::WHITE, false));
            }
        }
        // Symbole am selben Ort im Kreis verteilen (wie bei A Link to the Past), nicht in der Übersicht
        if !overview {
            let mut groups: Vec<Vec<usize>> = Vec::new();
            for k in 0..marks.len() {
                match groups.iter_mut().find(|gr| marks[gr[0]].0.distance(marks[k].0) < rad * 1.4) {
                    Some(gr) => gr.push(k),
                    None => groups.push(vec![k]),
                }
            }
            for gr in groups.iter().filter(|gr| gr.len() > 1) {
                let cc = gr.iter().fold(gfx::Vec2::ZERO, |a, &k| a + marks[k].0.to_vec2()) / gr.len() as f32;
                let nn = gr.len() as f32;
                let dd = rad * (0.75 + 0.18 * nn);
                for (j, &k) in gr.iter().enumerate() {
                    let a = std::f32::consts::TAU * j as f32 / nn - std::f32::consts::FRAC_PI_2;
                    marks[k].0 = (cc + gfx::vec2(a.cos(), a.sin()) * dd).to_pos2();
                }
            }
        }
        // Fadenkreuz: Auswahl = nächstes Symbol
        m.cur = gfx::vec2(m.cur.x.clamp(-r.width() * 0.5, r.width() * 0.5), m.cur.y.clamp(-r.height() * 0.5, r.height() * 0.5));
        let aim = r.center() + m.cur;
        let sel = marks.iter().filter(|mk| mk.0.distance(aim) < rad * 1.6).min_by(|a, b| a.0.distance(aim).total_cmp(&b.0.distance(aim))).map(|mk| mk.1);
        m.sel = if m.pin_pick.is_some() { None } else { sel };
        let pin_pick = m.pin_pick;
        let labels: Vec<(String, gfx::Pos2)> = Vec::new();
        let info = match sel {
            Some(Sel::Loc(i)) => d.locs.get(i).map(|l| {
                let st = match l.done {
                    Some(true) => tr(en, "geholt", "collected"),
                    Some(false) => tr(en, "fehlt noch", "still missing"),
                    None => tr(en, "Stand unbekannt", "state unknown"),
                };
                (pretty(&l.name), format!("{} – {st}", l.kind.label(en)))
            }),
            Some(Sel::Travel(a, _)) => Some((
                format!("{} {}", tr(en, "SCHNELLREISE:", "FAST TRAVEL:"), m.area_names.get(&a).cloned().unwrap_or_default()),
                tr(en, "A: hinreisen", "A: travel there").to_string(),
            )),
            Some(Sel::Pin(_)) => Some((tr(en, "EIGENE MARKIERUNG", "OWN MARKER").to_string(), tr(en, "A: entfernen", "A: remove").to_string())),
            Some(Sel::Mark(i)) => d.marks.get(i).map(|mk| {
                let (de, e) = match mk.1 {
                    1 => ("SCHRUMPFPUNKT", "MINISH PORTAL"),
                    2 => ("WINDWAPPEN", "WIND CREST"),
                    _ => ("BOHNENRANKE", "BEANSTALK"),
                };
                (tr(en, de, e).to_string(), String::new())
            }),
            _ => None,
        }
        .unwrap_or_else(|| {
            let here = d.locs.iter().filter(|l| l.at.is_some_and(|a| a.0 == sp) && l.done.is_some());
            let (a, n) = here.fold((0, 0), |s, l| (s.0 + (l.done == Some(true)) as usize, s.1 + 1));
            // Gebiet unter dem Fadenkreuz (nur dort Text, nicht überall auf der Karte)
            let at = c + (aim - r.center()) / z.max(0.01);
            let under = m.rects.iter().find(|(_, (rr, s))| *s == sp && at.x >= rr[0] as f32 && at.y >= rr[1] as f32 && at.x < (rr[0] + rr[2]) as f32 && at.y < (rr[1] + rr[3]) as f32).and_then(|((ar, _), _)| m.area_names.get(ar).cloned());
            (format!("{}  {a}/{n}", under.unwrap_or(space_name.clone())), String::new())
        });
        // ---- zeichnen
        for (name, pos) in &labels {
            if r.contains(*pos) {
                let w = crate::pixfont::width(name, gp);
                let br = gfx::Rect::from_center_size(*pos, gfx::vec2(w + gp * 6.0, gp * 11.0));
                p.rect_filled(br, 0.0, Color32::from_black_alpha(150));
                crate::pixfont::text(p, *pos, gfx::Align2::CENTER_CENTER, name, gp, Color32::from_rgb(255, 240, 200));
            }
        }
        for (pos, s_, icon, col, dot) in &marks {
            if !r.expand(rad * 2.0).contains(*pos) {
                continue;
            }
            let s = Some(*s_) == sel;
            if matches!(s_, Sel::Travel(..)) {
                let n = if s { 6.0 } else { 5.0 };
                p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2((n + 2.0) * gp, (n + 2.0) * gp)), 0.0, Color32::from_rgb(16, 16, 24));
                p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2(n * gp, n * gp)), 0.0, if s && (t * 4.0) as i64 % 2 == 0 { Color32::from_rgb(248, 216, 64) } else { DOOR });
                p.rect_filled(gfx::Rect::from_min_size(*pos - gfx::vec2(n * gp * 0.5, n * gp * 0.5), gfx::vec2(gp, gp)), 0.0, Color32::from_rgb(220, 255, 255));
                continue;
            }
            if overview && *dot && !s {
                // Übersicht: kleiner Punkt in der Farbe der Art
                let dd = gp * 4.0;
                p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2(dd + gp * 2.0, dd + gp * 2.0)), 0.0, Color32::from_rgb(16, 16, 24));
                p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2(dd, dd)), 0.0, *col);
                continue;
            }
            paint_icon(p, *pos, if s { rad * 1.15 } else { rad * 0.85 }, *icon, *col);
        }
        // Link (blinkt leicht)
        if let Some(l) = link {
            let lp = to_screen(l.1, l.2);
            if r.contains(lp) && (t * 3.0) as i64 % 4 != 0 {
                self.link_dot(p, lp, gp, rad * 0.6);
            }
        }
        // Fadenkreuz wie bei A Link to the Past: goldene Ecken, rastet auf dem gewählten Symbol ein
        {
            let lock = sel.and_then(|k| marks.iter().find(|mk| mk.1 == k)).map(|mk| mk.0);
            let cc = lock.unwrap_or(aim);
            let pulse = (t * 4.0).sin();
            let h = if lock.is_some() { 7.0 } else { 5.0 } + if pulse > 0.0 { 1.0 } else { 0.0 };
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let corner = cc + gfx::vec2(sx * h * gp, sy * h * gp);
                let arm = 3.0 * gp;
                let hx = gfx::Rect::from_two_pos(corner, corner + gfx::vec2(-sx * arm, -sy * gp));
                let vy = gfx::Rect::from_two_pos(corner, corner + gfx::vec2(-sx * gp, -sy * arm));
                for rr in [hx, vy] {
                    p.rect_filled(rr.expand(gp), 0.0, Color32::from_rgb(16, 16, 24));
                }
                for rr in [hx, vy] {
                    p.rect_filled(rr, 0.0, GOLD);
                }
            }
            if lock.is_none() {
                p.rect_filled(gfx::Rect::from_center_size(cc, gfx::vec2(gp * 3.0, gp * 3.0)), 0.0, Color32::from_rgb(16, 16, 24));
                p.rect_filled(gfx::Rect::from_center_size(cc, gfx::vec2(gp, gp)), 0.0, Color32::WHITE);
            }
        }
        // Markierung setzen: Symbolauswahl am Fadenkreuz
        if let Some((_, _, _, sym)) = pin_pick {
            let cell = fs * 1.4;
            let br = gfx::Rect::from_center_size(aim + gfx::vec2(0.0, -fs * 1.8), gfx::vec2(cell * PIN_ICONS.len() as f32 + fs * 0.6, cell + fs * 0.4));
            p.rect_filled(br.expand(gp), 0.0, Color32::BLACK);
            p.rect_filled(br, 0.0, Color32::from_rgb(10, 12, 32));
            p.rect_stroke(br, 0.0, Stroke::new(gp, GOLD));
            for (k, ic) in PIN_ICONS.iter().enumerate() {
                let cc = gfx::pos2(br.left() + fs * 0.3 + cell * (k as f32 + 0.5), br.center().y);
                if k as u8 == sym {
                    p.rect_stroke(gfx::Rect::from_center_size(cc, gfx::vec2(cell * 0.9, cell * 0.9)), 0.0, Stroke::new(gp, Color32::WHITE));
                }
                paint_icon(p, cc, fs * 0.45, *ic, Color32::WHITE);
            }
        }
        // Anzeige oben rechts (X wechselt)
        {
            let names_de = ["ÜBERSICHT", "SAMMELN", "SCHNELLREISE", "ALLES"];
            let names_en = ["OVERVIEW", "COLLECT", "FAST TRAVEL", "ALL"];
            let view = if spoiler { tr(en, "ALLES", "ALL") } else { tr(en, "GESAMMELT", "COLLECTED") };
            let txt = if sp == 0 {
                format!("Y  {}   X  {view}", if en { names_en[filt as usize] } else { names_de[filt as usize] })
            } else {
                format!("X  {view}")
            };
            let small = fs * 0.75;
            let tw = self.text_w(p, &txt, small);
            let br = gfx::Rect::from_min_size(gfx::pos2(r.right() - tw - fs * 1.6, r.top() + fs * 0.4), gfx::vec2(tw + fs * 1.2, fs * 1.3));
            p.rect_filled(br, 0.0, Color32::from_black_alpha(200));
            p.rect_stroke(br, 0.0, Stroke::new(gp, crate::inventory::MENU_LINE));
            self.mtext(p, br.center(), gfx::Align2::CENTER_CENTER, &txt, small, crate::inventory::MENU_LINE);
            if sp != 0 {
                // Name des Raums oben links (Y wechselt)
                let txt = format!("Y  {}", space_name.to_uppercase());
                let tw = self.text_w(p, &txt, small);
                let bl = gfx::Rect::from_min_size(gfx::pos2(r.left() + fs * 0.4, r.top() + fs * 0.4), gfx::vec2(tw + fs * 1.2, fs * 1.3));
                p.rect_filled(bl, 0.0, Color32::from_black_alpha(200));
                p.rect_stroke(bl, 0.0, Stroke::new(gp, crate::inventory::MENU_LINE));
                self.mtext(p, bl.center(), gfx::Align2::CENTER_CENTER, &txt, small, crate::inventory::MENU_LINE);
            }
        }
        info
    }

    /// Reiter 100 %: Fortschritt je Art mit Balken
    fn gba_draw_percent(&mut self, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let gp = self.game_px;
        let counts = self.gba_map_counts();
        let mut rows: Vec<(usize, Icon, Color32, String, usize, usize)> = counts
            .iter()
            .map(|&(k, a, n)| {
                let (ic, col) = kind_look(k);
                (LocKind::ALL.iter().position(|x| *x == k).unwrap_or(0), ic, col, k.label(en).to_string(), a, n)
            })
            .collect();
        let (ia, inn) = self.gba.map.items;
        rows.push((LIST_ITEMS, Icon::Star, GOLD, "Items".to_string(), ia, inn));
        let qd = QUESTS.iter().filter(|q| self.gba.map.progress.get(q.3).is_some_and(|&v| v >= q.4)).count();
        rows.push((usize::MAX, Icon::Quest, Color32::WHITE, "Quests".to_string(), qd, QUESTS.len()));
        let pct = self.gba_map_percent();
        self.mtext(p, r.center_top() + gfx::vec2(0.0, fs * 0.8), gfx::Align2::CENTER_TOP, &format!("{pct:.1} %"), fs * 2.0, if pct >= 99.95 { GREEN } else { GOLD });
        let top = r.top() + fs * 3.6;
        let lh = ((r.bottom() - top - fs * 0.5) / rows.len().max(1) as f32).min(fs * 1.8);
        let x0 = r.left() + fs;
        let bar_x = r.left() + r.width() * 0.42;
        let bar_w = r.width() * 0.4;
        let sel_cat = self.gba.map.list_cat;
        for (j, (cat, ic, col, label, a, n)) in rows.iter().enumerate() {
            let y = top + lh * (j as f32 + 0.5);
            if *cat == sel_cat {
                let hr = gfx::Rect::from_min_max(gfx::pos2(r.left() + fs * 0.3, y - lh * 0.48), gfx::pos2(r.right() - fs * 0.3, y + lh * 0.48));
                p.rect_stroke(hr, 0.0, Stroke::new(gp, GOLD));
            }
            let full = a >= n;
            paint_icon(p, gfx::pos2(x0 + fs * 0.5, y), fs * 0.45, *ic, if full { *col } else { GREYC });
            self.mtext(p, gfx::pos2(x0 + fs * 1.4, y), gfx::Align2::LEFT_CENTER, label, fs * 0.8, Color32::WHITE);
            let br = gfx::Rect::from_min_size(gfx::pos2(bar_x, y - fs * 0.3), gfx::vec2(bar_w, fs * 0.6));
            p.rect_filled(br.expand(gp), 0.0, Color32::BLACK);
            p.rect_filled(br, 0.0, Color32::from_rgb(40, 44, 80));
            let f = *a as f32 / (*n).max(1) as f32;
            p.rect_filled(gfx::Rect::from_min_size(br.min, gfx::vec2(bar_w * f, br.height())), 0.0, if full { GREEN } else { GOLD });
            self.mtext(p, gfx::pos2(r.right() - fs * 0.8, y), gfx::Align2::RIGHT_CENTER, &format!("{a}/{n}"), fs * 0.8, if full { GREEN } else { GOLD });
        }
        let total: usize = counts.iter().map(|c| c.2).sum::<usize>();
        (format!("{} {}", total + inn, tr(en, "Dinge zu finden", "things to find")), String::new())
    }

    /// Reiter Fundorte: links Kategorien, rechts Einträge mit Häkchen
    fn gba_draw_list(&mut self, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let gp = self.game_px;
        let cat = self.gba.map.list_cat;
        let left_w = r.width() * 0.3;
        let lh = ((r.height() - fs) / LIST_CATS as f32).min(fs * 1.5);
        for c in 0..LIST_CATS {
            let e = self.gba_list_entries(c);
            let (a, n) = e.iter().fold((0, 0), |s, x| (s.0 + (x.done == Some(true)) as usize, s.1 + x.done.is_some() as usize));
            let y = r.top() + fs * 0.5 + lh * (c as f32 + 0.5);
            let row = gfx::Rect::from_min_max(gfx::pos2(r.left() + fs * 0.3, y - lh * 0.45), gfx::pos2(r.left() + left_w, y + lh * 0.45));
            if c == cat {
                p.rect_filled(row, 0.0, crate::inventory::MENU_LINE);
            }
            let col = if c == cat { crate::inventory::MENU_BG } else { Color32::WHITE };
            let name = self.gba_list_cat_name(c);
            let cnt = format!("{a}/{n}");
            let cw = self.text_w(p, &cnt, fs * 0.75);
            let name = self.fit_text(p, &name, fs * 0.75, row.width() - cw - fs * 1.2);
            self.mtext(p, gfx::pos2(row.left() + fs * 0.4, y), gfx::Align2::LEFT_CENTER, &name, fs * 0.75, col);
            self.mtext(p, gfx::pos2(row.right() - fs * 0.3, y), gfx::Align2::RIGHT_CENTER, &cnt, fs * 0.75, if c == cat { col } else if n > 0 && a >= n { GREEN } else { GOLD });
        }
        let entries = self.gba_list_entries(cat);
        let lr = gfx::Rect::from_min_max(gfx::pos2(r.left() + left_w + fs * 0.8, r.top() + fs * 0.4), r.max - gfx::vec2(fs * 0.4, fs * 0.4));
        p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(lr.left() - fs * 0.5, r.top() + fs * 0.4), gfx::vec2(gp, r.height() - fs * 0.8)), 0.0, GOLD);
        let elh = fs * 1.5;
        let rows = ((lr.height() / elh).floor() as usize).max(1);
        let sel = self.gba.map.list_sel.min(entries.len().saturating_sub(1));
        let first = sel.saturating_sub(rows / 2).min(entries.len().saturating_sub(rows));
        for (j, e) in entries.iter().enumerate().skip(first).take(rows) {
            let y = lr.top() + elh * ((j - first) as f32 + 0.5);
            if j == sel {
                p.rect_stroke(gfx::Rect::from_min_max(gfx::pos2(lr.left() - fs * 0.2, y - elh * 0.45), gfx::pos2(lr.right(), y + elh * 0.45)), 0.0, Stroke::new(gp, GOLD));
            }
            paint_icon(p, gfx::pos2(lr.left() + fs * 0.5, y), fs * 0.42, e.icon, if e.done == Some(true) { e.col } else { GREYC });
            match e.done {
                Some(d) => self.tick(p, gfx::pos2(lr.right() - fs * 0.7, y), fs * 0.4, d),
                None => {
                    self.mtext(p, gfx::pos2(lr.right() - fs * 0.7, y), gfx::Align2::CENTER_CENTER, "?", fs * 0.75, GREYC);
                }
            }
            let name = self.fit_text(p, &e.name, fs * 0.75, lr.width() - fs * 2.8);
            self.mtext(p, gfx::pos2(lr.left() + fs * 1.3, y), gfx::Align2::LEFT_CENTER, &name, fs * 0.75, if e.done == Some(true) { Color32::from_gray(170) } else { Color32::WHITE });
        }
        let _ = en;
        let info = entries.get(sel).map(|e| e.name.clone()).unwrap_or_default();
        (info, String::new())
    }
}

impl App {
    /// Quests: (erreicht als Bruchteile, Anzahl)
    fn gba_quest_score(&self) -> (f32, f32) {
        let p = &self.gba.map.progress;
        let a: f32 = QUESTS.iter().map(|q| (p.get(q.3).copied().unwrap_or(0) as f32 / q.4 as f32).min(1.0)).sum();
        (a, QUESTS.len() as f32)
    }

    /// Reiter Quests: Hauptquest und Nebenquests mit Stand
    fn gba_draw_quests(&mut self, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let gp = self.game_px;
        let prog = self.gba.map.progress.clone();
        let sel = self.gba.map.quest_sel.min(QUESTS.len() - 1);
        let lh = fs * 1.5;
        // Zeilen mit Überschriften
        let mut rows: Vec<Option<usize>> = Vec::new();
        let mut last_main = None;
        for (i, q) in QUESTS.iter().enumerate() {
            if last_main != Some(q.2) {
                rows.push(None);
                last_main = Some(q.2);
            }
            rows.push(Some(i));
        }
        let vis = (((r.height() - fs * 0.6) / lh).floor() as usize).max(1);
        let sel_row = rows.iter().position(|x| *x == Some(sel)).unwrap_or(0);
        let first = sel_row.saturating_sub(vis / 2).min(rows.len().saturating_sub(vis));
        let x0 = r.left() + fs * 0.6;
        let x1 = r.right() - fs * 0.6;
        for (k, row) in rows.iter().enumerate().skip(first).take(vis) {
            let y = r.top() + fs * 0.3 + lh * ((k - first) as f32 + 0.5);
            match row {
                None => {
                    let main = rows.get(k + 1).and_then(|x| *x).is_some_and(|i| QUESTS[i].2);
                    let t = if main { tr(en, "HAUPTQUEST", "MAIN QUEST") } else { tr(en, "NEBENQUESTS", "SIDE QUESTS") };
                    self.mtext(p, gfx::pos2(x0, y), gfx::Align2::LEFT_CENTER, t, fs * 0.8, GOLD);
                    p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(x0 + self.text_w(p, t, fs * 0.8) + fs * 0.6, y), gfx::vec2(x1 - x0 - self.text_w(p, t, fs * 0.8) - fs * 0.6, gp)), 0.0, GOLD);
                }
                Some(i) => {
                    let q = QUESTS[*i];
                    let v = prog.get(q.3).copied().unwrap_or(0);
                    let done = v >= q.4;
                    if *i == sel {
                        p.rect_stroke(gfx::Rect::from_min_max(gfx::pos2(x0 - fs * 0.3, y - lh * 0.45), gfx::pos2(x1 + fs * 0.3, y + lh * 0.45)), 0.0, Stroke::new(gp, GOLD));
                    }
                    paint_icon(p, gfx::pos2(x0 + fs * 0.5, y), fs * 0.42, if q.2 { Icon::Crystal } else { Icon::Quest }, if done { if q.2 { KINSTONE } else { Color32::WHITE } } else { GREYC });
                    let name = if en { q.1 } else { q.0 };
                    self.mtext(p, gfx::pos2(x0 + fs * 1.4, y), gfx::Align2::LEFT_CENTER, name, fs * 0.8, if done { Color32::from_gray(170) } else { Color32::WHITE });
                    if q.4 > 1 {
                        let txt = format!("{}/{}", v.min(q.4), q.4);
                        self.mtext(p, gfx::pos2(x1 - fs * 1.4, y), gfx::Align2::RIGHT_CENTER, &txt, fs * 0.8, if done { GREEN } else { GOLD });
                    }
                    self.tick(p, gfx::pos2(x1 - fs * 0.5, y), fs * 0.4, done);
                }
            }
        }
        let (a, n) = self.gba_quest_score();
        (format!("Quests  {:.0} %", a / n.max(1.0) * 100.0), String::new())
    }
}

/// eigene Markierungen und besuchte Räume laden (`<rom>.tmcpins`: „p Raum x y Symbol“, „v Gebiet Raum“)
fn load_pins(rom: &std::path::Path) -> (Vec<(Space, f32, f32, u8)>, std::collections::BTreeSet<(u8, u8)>) {
    let mut pins = Vec::new();
    let mut vis = std::collections::BTreeSet::new();
    let Ok(t) = std::fs::read_to_string(rom.with_extension("tmcpins")) else { return (pins, vis) };
    for l in t.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        match f.as_slice() {
            ["p", sp, x, y, sym] => {
                if let (Ok(sp), Ok(x), Ok(y), Ok(sym)) = (sp.parse(), x.parse(), y.parse(), sym.parse()) {
                    pins.push((sp, x, y, sym));
                }
            }
            ["v", a, r] => {
                if let (Ok(a), Ok(r)) = (a.parse(), r.parse()) {
                    vis.insert((a, r));
                }
            }
            _ => {}
        }
    }
    (pins, vis)
}

fn save_pins(rom: &std::path::Path, pins: &[(Space, f32, f32, u8)], vis: &std::collections::BTreeSet<(u8, u8)>) {
    let mut t = String::new();
    for (sp, x, y, sym) in pins {
        t += &format!("p {sp} {x:.0} {y:.0} {sym}\n");
    }
    for (a, r) in vis {
        t += &format!("v {a} {r}\n");
    }
    let _ = std::fs::write(rom.with_extension("tmcpins"), t);
}

impl App {
    /// A auf der Karte: Schnellreise, Markierung entfernen oder neue Markierung setzen
    fn gba_map_activate(&mut self) {
        let sp = self.gba_map_space();
        let m = &mut self.gba.map;
        match m.sel {
            Some(Sel::Travel(a, r)) => {
                if let Some((rr, _)) = m.rects.get(&(a, r)) {
                    m.travel = Some((a, r, rr[2] / 2, rr[3] / 2, 0, 0));
                    m.open = false;
                    if let Some(d) = m.data.as_mut() {
                        d.save();
                    }
                    let name = m.area_names.get(&a).cloned().unwrap_or_default();
                    let t = format!("{} {name}", self.t("Schnellreise:", "Fast travel:"));
                    self.msg(t);
                }
            }
            Some(Sel::Pin(i)) => {
                if i < m.pins.len() {
                    m.pins.remove(i);
                    m.pins_dirty = true;
                }
            }
            _ => {
                let (c, z) = m.views.get(&sp).copied().unwrap_or((gfx::Vec2::ZERO, 1.0));
                let at = c + m.cur / z.max(0.05);
                m.pin_pick = Some((sp, at.x, at.y, 0));
            }
        }
    }
}

impl App {
    /// Karte im eigenen Fenster: dieselbe Darstellung, füllt das ganze Fenster
    pub(crate) fn gba_map_window(&mut self, ctx: &gfx::Context) {
        let area = ctx.screen_rect();
        ctx.layer_painter(gfx::LayerId::background()).rect_filled(area, 0.0, Color32::BLACK);
        if self.gba.map.open {
            self.gba_map_draw(area, ctx);
        }
    }
}
