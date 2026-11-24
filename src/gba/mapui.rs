//! Eigene Karte für The Minish Cap (PC-Fassung) im Stil der Karte von A Link to the Past: Reiter OBERWELT,
//! GEBIET, 100 % und FUNDORTE, Link-Punkt, Fundorte (Herzteile, Truhen, Kinsteine …) mit „schon geholt“,
//! komplett per Controller (L/R Reiter, Stick bewegen, rechter Stick Zoom, Y Link, X Spoiler, B zurück).
//!
//! Öffnet mit Minus (Karte-Taste) oder wenn man im Item-Menü zur Kartenseite blättert – die Karte des Spiels
//! wird dann nicht mehr gezeigt. Im Item-Menü führen L/R am Rand zurück zu den Seiten des Spiels.

use super::tmcmap::{pretty, Loc, LocKind, MapData, Space, CH, LOGIC};
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

/// Reiter: 0 Oberwelt, 1 Gebiet (Häuser, Höhlen, Dungeons), 2 100 %, 3 Fundorte
const TABS: u8 = 4;
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
}

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
                    None
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
            d.locs_loaded = true;
            m.data = Some(d);
            m.rom = Some(g.path.clone());
            m.tex.clear();
            m.views.clear();
        }
        g.set_own_map(own);
        // Kartenseite im Item-Menü: eigene Karte statt der des Spiels
        if own && !m.open {
            let req = g.map_request();
            if req != 0 {
                m.open = true;
                m.from_menu = true;
                m.prev_pad = 0xFFFF;
                m.dev_ui = 0;
                m.tab = 0;
            }
        }
        // Link und Ausschnitt
        let view = g.map_view();
        if let Some(v) = view {
            let sp = space_of(v[0] as u8, g.area_kind(v[0] as u8));
            m.link = Some((sp, v[9] as f32, v[10] as f32));
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
        let m = &mut self.gba.map;
        m.open = false;
        if let Some(d) = m.data.as_mut() {
            d.save();
        }
        if m.from_menu {
            if let Some(g) = &self.gba.game {
                if close_menu {
                    g.menu_goto(0);
                } else if g.map_request() == 2 {
                    // das Spiel steht auf seiner Kartenseite: zurück zu den Items
                    g.menu_goto(2);
                }
            }
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
        let mut v: Vec<Space> = d.chunks.keys().map(|k| k.0).chain(d.locs.iter().filter_map(|l| l.at.map(|a| a.0))).filter(|&s| s != 0).collect();
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
        self.gba.map.names.get(&a).cloned().unwrap_or_else(|| format!("{} {a:02X}", tr(en, "Gebiet", "Area")))
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
            // im Item-Menü: am Rand zurück zu den Seiten des Spiels
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
            if pressed & X != 0 {
                self.gba.map.spoiler = !self.gba.map.spoiler;
            }
            if pressed & Y != 0 {
                // Y: zu Link (im Gebiet-Reiter auch in Links Gebiet wechseln)
                if tab == 1 {
                    if let Some(l) = self.gba.map.link.filter(|l| l.0 != 0) {
                        self.gba.map.space = l.0;
                    }
                }
                self.gba_map_center_link();
            }
            if tab == 1 && pressed & A != 0 {
                // A: nächstes Gebiet
                let v = self.gba_map_spaces();
                if !v.is_empty() {
                    let i = v.iter().position(|&s| s == self.gba.map.space).map_or(0, |i| (i + 1) % v.len());
                    self.gba.map.space = v[i];
                }
            }
            let sp = self.gba_map_space();
            let ls = self.lstick;
            let rs = self.rstick;
            let mut mv = gfx::vec2(0.0, 0.0);
            if pad & LEFT != 0 {
                mv.x -= 1.0;
            }
            if pad & RIGHT != 0 {
                mv.x += 1.0;
            }
            if pad & UP != 0 {
                mv.y -= 1.0;
            }
            if pad & DOWN != 0 {
                mv.y += 1.0;
            }
            mv += gfx::vec2(ls.x, -ls.y);
            let m = &mut self.gba.map;
            if let Some((c, z)) = m.views.get_mut(&sp) {
                if mv.length() > 0.05 {
                    // gleich schnell auf dem Bildschirm, egal wie weit hereingezoomt
                    *c += mv * dt * 520.0 / z.max(0.05);
                }
                if rs.y.abs() > 0.2 {
                    *z = (*z * (rs.y * dt * 2.2).exp()).clamp(0.05, 8.0);
                }
            }
        } else if tab == 2 {
            let d = self.gba_map_rep(pad, dt);
            let m = &mut self.gba.map;
            if d.1 != 0 {
                m.list_cat = (m.list_cat as i32 + d.1).rem_euclid(LIST_CATS as i32) as usize;
            }
            if pressed & A != 0 {
                m.tab = 3;
                m.list_sel = 0;
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
        (a + ia) as f32 / (n + inn).max(1) as f32 * 100.0
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
        let names = [tr(en, "OBERWELT", "OVERWORLD"), tr(en, "GEBIET", "AREA"), "100 %", tr(en, "FUNDORTE", "LOCATIONS")];
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
            3 => self.gba_draw_list(&mp, map_rect, fs),
            _ => self.gba_draw_world(ctx, &mp, map_rect, fs),
        };
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
        let view = if self.gba.map.spoiler { tr(en, "ALLES", "ALL") } else { tr(en, "GEHOLT", "COLLECTED") };
        self.mtext(&painter, gfx::pos2(pr.right(), pr.bottom() + fs * 0.25), gfx::Align2::RIGHT_TOP, &format!("{}: {view}", tr(en, "ANSICHT", "VIEW")), fs * 0.7, Color32::from_gray(200));
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
            }
        }
        self.game_px = old_px;
    }

    /// Reiter Oberwelt/Gebiet: Bild aus den gesehenen Räumen, Fundorte, Link
    fn gba_draw_world(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let gp = self.game_px;
        let sp = self.gba_map_space();
        let space_name = self.gba_space_name(sp);
        let t = ctx.input(|i| i.time) as f32;
        let m = &mut self.gba.map;
        let Some(d) = m.data.as_mut() else { return (String::new(), String::new()) };
        // Texturen der geänderten Kacheln (wenige je Bild, damit nichts ruckelt)
        let mut n = 0;
        while let Some(k) = d.dirty.pop() {
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
            if n >= 12 {
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
            // Oberwelt: ganz zeigen (Y springt zu Link); Gebiet: ganz, höchstens doppelte Größe
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
        let to_screen = |x: f32, y: f32| r.center() + (gfx::vec2(x, y) - c) * z;
        // Hintergrund: Raster für noch nicht Gesehenes
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
        let empty = d.chunks.keys().all(|k| k.0 != sp);
        // Fundorte
        let rad = fs * 0.55;
        let spoiler = m.spoiler;
        let mut marks: Vec<(gfx::Pos2, usize)> = Vec::new();
        for (i, l) in d.locs.iter().enumerate() {
            let Some(at) = l.at.filter(|a| a.0 == sp) else { continue };
            if !spoiler && l.done != Some(true) {
                continue;
            }
            marks.push((to_screen(at.1, at.2), i));
        }
        // Symbole am selben Ort im Kreis verteilen (wie bei A Link to the Past)
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
        let aim = r.center();
        let sel = marks.iter().filter(|mk| mk.0.distance(aim) < rad * 3.0).min_by(|a, b| a.0.distance(aim).total_cmp(&b.0.distance(aim))).map(|mk| mk.1);
        m.pick = sel;
        let link_pos = link.map(|l| to_screen(l.1, l.2));
        let mut draw: Vec<(gfx::Pos2, Icon, Color32, bool, bool)> = Vec::new();
        for (pos, i) in &marks {
            if !r.expand(rad * 2.0).contains(*pos) {
                continue;
            }
            let l = &d.locs[*i];
            let (icon, col) = kind_look(l.kind);
            draw.push((*pos, icon, col, l.done == Some(true), Some(*i) == sel));
        }
        let info = match sel.and_then(|i| d.locs.get(i)) {
            Some(l) => {
                let st = match l.done {
                    Some(true) => tr(en, "geholt", "collected"),
                    Some(false) => tr(en, "fehlt noch", "still missing"),
                    None => tr(en, "Stand unbekannt", "state unknown"),
                };
                (pretty(&l.name), format!("{} – {st}", l.kind.label(en)))
            }
            None => {
                let here = d.locs.iter().filter(|l| l.at.is_some_and(|a| a.0 == sp) && l.done.is_some());
                let (a, n) = here.fold((0, 0), |s, l| (s.0 + (l.done == Some(true)) as usize, s.1 + 1));
                let hint = if sp == 0 {
                    tr(en, "STICK BEWEGEN  R-STICK ZOOM  Y LINK  X ANSICHT  B ZURÜCK", "STICK MOVE  R-STICK ZOOM  Y LINK  X VIEW  B BACK")
                } else {
                    tr(en, "STICK BEWEGEN  R-STICK ZOOM  A NÄCHSTES GEBIET  Y LINK  X ANSICHT", "STICK MOVE  R-STICK ZOOM  A NEXT AREA  Y LINK  X VIEW")
                };
                (format!("{space_name}  {a}/{n}"), hint.to_string())
            }
        };
        // ab hier nur noch zeichnen (braucht &self)
        if empty {
            let txt = tr(en, "NOCH NICHT ERKUNDET – DIE KARTE FÜLLT SICH BEIM SPIELEN", "NOT EXPLORED YET – THE MAP FILLS IN AS YOU PLAY");
            let mut px = (fs * 0.75 / 8.0).round().max(1.0);
            while px > 1.0 && crate::pixfont::width(txt, px) > r.width() * 0.9 {
                px -= 1.0;
            }
            crate::pixfont::text(p, r.center_bottom() - gfx::vec2(0.0, fs * 0.8), gfx::Align2::CENTER_BOTTOM, txt, px, Color32::from_gray(190));
        }
        for (pos, icon, col, done, s) in draw {
            paint_icon(p, pos, if s { rad * 1.15 } else { rad * 0.85 }, icon, if done { col } else { GREYC });
            if done {
                paint_icon(p, pos + gfx::vec2(rad * 0.7, rad * 0.6), rad * 0.45, Icon::Check, Color32::WHITE);
            }
        }
        // Fadenkreuz in der Mitte
        let ch = fs * 0.45;
        for (o, s) in [(gfx::vec2(-ch, 0.0), gfx::vec2(ch * 0.7, gp)), (gfx::vec2(ch * 0.3, 0.0), gfx::vec2(ch * 0.7, gp)), (gfx::vec2(0.0, -ch), gfx::vec2(gp, ch * 0.7)), (gfx::vec2(0.0, ch * 0.3), gfx::vec2(gp, ch * 0.7))] {
            let a = aim + o - if s.x > s.y { gfx::vec2(0.0, gp * 0.5) } else { gfx::vec2(gp * 0.5, 0.0) };
            p.rect_filled(gfx::Rect::from_min_size(a, s).expand(gp), 0.0, Color32::from_black_alpha(160));
            p.rect_filled(gfx::Rect::from_min_size(a, s), 0.0, Color32::WHITE);
        }
        // Link (blinkt leicht)
        if let Some(lp) = link_pos {
            if r.contains(lp) && (t * 3.0) as i64 % 4 != 0 {
                self.link_dot(p, lp, gp, rad * 0.6);
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
        (
            format!("{} {}", total + inn, tr(en, "Dinge zu finden", "things to find")),
            tr(en, "STICK KATEGORIE  A LISTE  L/R KARTE  B ZURÜCK", "STICK CATEGORY  A LIST  L/R MAP  B BACK").to_string(),
        )
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
        let info = entries.get(sel).map(|e| e.name.clone()).unwrap_or_default();
        let hint = if entries.get(sel).is_some_and(|e| e.at.is_some()) {
            tr(en, "◀ ▶ KATEGORIE  ▲ ▼ EINTRAG  A AUF KARTE ZEIGEN  B ZURÜCK", "◀ ▶ CATEGORY  ▲ ▼ ENTRY  A SHOW ON MAP  B BACK")
        } else {
            tr(en, "◀ ▶ KATEGORIE  ▲ ▼ EINTRAG  B ZURÜCK", "◀ ▶ CATEGORY  ▲ ▼ ENTRY  B BACK")
        };
        (info, hint.to_string())
    }
}
