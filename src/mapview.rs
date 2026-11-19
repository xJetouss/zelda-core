//! Karte als Vollbild-Overlay im Stil eines Remakes: Lichtwelt, Schattenwelt und Dungeons,
//! komplett per Controller bedienbar (L/R Karte, linker Stick bewegen, rechter Stick zoomen,
//! A Teleport, X Ansicht, Y Link/nächster Dungeon, B zurück). Passt sich an Pixel- und HD-Modus an.

use crate::alttp;
use crate::app::App;
use crate::i18n::tr;
use crate::tracker::DUNGEONS;
use crate::ui::{current_dungeon, paint_icon, Icon, CHEST, DOOR, DUNGEON, GAME, GREYC, HEART};
use crate::mapdata::{Poi, QState, BOTTLES, CRYSTAL_BITS, DUNGEON_REQS, DUNGEON_UPGRADES, OPTIONAL_UPGRADES, QUESTS, UPGRADES};
use crate::world::{self, LocKind};
use crate::gfx::{self, Color32, Stroke};

const GOLD: Color32 = Color32::from_rgb(232, 194, 88);
const GOLD_DARK: Color32 = Color32::from_rgb(132, 96, 36);
const NAVY: Color32 = Color32::from_rgb(10, 12, 32);
const PANEL: Color32 = Color32::from_rgb(20, 24, 52);
/// Dungeon-Karte: so weit hereinzoomen, bis ein Raum die Karte fast füllt
const DZOOM_MAX: f32 = 4.6;

/// Was gerade auf der Karte ausgewählt ist
#[derive(Clone, Copy, PartialEq)]
pub enum MapSel {
    Loc(usize),
    /// Index in `Game::ents` (genau diese Tür)
    Door(usize),
    Dungeon(usize),
    /// 0 Flasche, 1 Upgrade, 2 Quest
    Poi(u8, usize),
    Pin(usize),
}

impl App {
    /// Fortschritt in Prozent (wie in der Fortschrittsliste)
    pub(crate) fn completion(&self) -> f32 {
        let Some(g) = &self.game else { return 0.0 };
        let p = crate::tracker::progress(&g.snes);
        let (mut hp, mut hp_t, mut hc, mut hc_t, mut ch, mut ch_t) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
        for (i, l) in g.locations.iter().enumerate() {
            let (d, n) = (l.done(&g.snes, g.manual.contains(&i)), l.total());
            match l.kind {
                LocKind::HeartPiece | LocKind::Minigame => {
                    hp += d;
                    hp_t += n;
                }
                LocKind::HeartContainer => {
                    hc += d;
                    hc_t += n;
                }
                LocKind::Chests => {
                    ch += d;
                    ch_t += n;
                }
            }
        }
        let items = p.items_owned.iter().filter(|b| **b).count();
        let ups = UPGRADES.iter().enumerate().filter(|(i, u)| !OPTIONAL_UPGRADES.contains(i) && u.done(&g.snes)).count() + DUNGEON_UPGRADES.iter().filter(|u| (u.3)(&g.snes)).count();
        let bot = crate::mapdata::bottles(&g.snes) as usize;
        let parts = [(hp, hp_t), (hc, hc_t), (items, alttp::INVENTORY.len()), (p.pendants as usize, 3), (p.crystals as usize, 7), (ch, ch_t), (ups, UPGRADES.len() - OPTIONAL_UPGRADES.len() + DUNGEON_UPGRADES.len()), (bot, 4)];
        let f: Vec<f32> = parts.iter().filter(|x| x.1 > 0).map(|x| (x.0 as f32 / x.1 as f32).min(1.0)).collect();
        f.iter().sum::<f32>() / f.len().max(1) as f32 * 100.0
    }

    /// Fortschritt eines Dungeons (Index wie `DMAP_NAMES`): Truhen/Herzcontainer darin + Karte, Kompass, Großer Schlüssel
    pub(crate) fn completion_dungeon(&self, d: u8) -> f32 {
        let Some(g) = &self.game else { return 0.0 };
        let (mut done, mut total) = (0usize, 0usize);
        for (i, l) in g.locations.iter().enumerate().filter(|(_, l)| l.dungeon == Some(d)) {
            done += l.done(&g.snes, g.manual.contains(&i)).min(l.total());
            total += l.total();
        }
        let bit = if d <= 1 { 0xC000u16 } else { 0x8000u16 >> d };
        for a in [0x7EF368u32, 0x7EF364, 0x7EF366] {
            // Kanalisation und Hyrule-Schloss haben im Spiel keinen Kompass – nicht mitzählen (sonst nie 100 %)
            if d <= 1 && a == 0x7EF364 {
                continue;
            }
            total += 1;
            done += (alttp::ram16(&g.snes, a) & bit != 0) as usize;
        }
        done as f32 / total.max(1) as f32 * 100.0
    }

    /// Echte 100 %: zusätzlich freiwillige Upgrades und alle Nebenquests
    pub(crate) fn completion_true(&self) -> f32 {
        let Some(g) = &self.game else { return 0.0 };
        let s = &g.snes;
        let base = self.completion() / 100.0;
        let opt = OPTIONAL_UPGRADES.iter().filter(|&&i| UPGRADES[i].done(s)).count() as f32 / OPTIONAL_UPGRADES.len() as f32;
        let side: Vec<&crate::mapdata::Quest> = QUESTS.iter().filter(|q| !q.main).collect();
        let sq = side.iter().filter(|q| (q.done)(s)).count() as f32 / side.len().max(1) as f32;
        (base * 0.8 + opt * 0.1 + sq * 0.1) * 100.0
    }

    /// aktueller Reiter: 0 Lichtwelt, 1 Schattenwelt, 2 Dungeon
    fn map_tab(&self) -> u8 {
        if self.map_page >= 3 {
            self.map_page
        } else if self.map_dungeon {
            2
        } else {
            self.map_world.min(1)
        }
    }

    fn set_map_tab(&mut self, t: u8) {
        match t % 5 {
            3 | 4 => self.map_page = t % 5,
            2 => {
                self.map_page = 0;
                self.map_dungeon = true;
            }
            w => {
                self.map_page = 0;
                self.map_dungeon = false;
                self.map_world = w;
            }
        }
        self.s.map_follow = false;
        self.map_pick = None;
    }

    /// Karte öffnen: Reiter und Ausschnitt passend zu Links Standort
    pub(crate) fn open_map_overlay(&mut self) {
        // Karte, Inventar und Spielstände nie gleichzeitig
        if self.inv_open || self.win_states {
            return;
        }
        // nur im laufenden Spiel (Oberwelt/Dungeon) – nicht im Titel, Spielerauswahl, Intro, Übergang oder Menü
        if !self.game.as_ref().map_or(false, |g| crate::quickmenu::can_use(&g.snes)) {
            let m = self.t("Karte erst im Spiel verfügbar", "Map is only available in game");
            self.msg(m);
            return;
        }
        self.win_map = true;
        self.map_page = 0;
        self.map_pick = None;
        if let Some(g) = &self.game {
            if let Some(d) = current_dungeon(&g.snes) {
                self.map_dungeon = true;
                self.map_dsel = d;
                // immer auf Links Stockwerk und Raum öffnen (nicht auf dem zuletzt angesehenen)
                self.map_floor_dungeon = None;
            } else if let Some((w, x, y)) = g.tracker.link {
                self.map_dungeon = false;
                self.map_world = w;
                self.map_center = gfx::vec2(x, y);
            }
        }
        self.map_cur = gfx::Vec2::ZERO;
        // leicht hereingezoomt, Link in der Mitte
        self.map_zoom = self.map_fit;
        self.map_prev_pad = 0xFFFF;
    }

    /// Controller/Tastatur in der Karte (Joypad-Bits wie beim SNES; Sticks extra)
    pub(crate) fn map_input(&mut self, pad: u16, dt: f32) {
        let pressed = pad & !self.map_prev_pad;
        self.map_prev_pad = pad;
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
        if let Some((w, x, y, sym)) = self.map_pick {
            let d = self.rep_dir(pad, dt);
            let sym = (sym as i32 + d.0).rem_euclid(5) as u8;
            self.map_pick = Some((w, x, y, sym));
            if pressed & A != 0 {
                self.map_activate();
            } else if pressed & (B | START) != 0 {
                self.map_pick = None;
            }
            return;
        }
        if pressed & (B | START) != 0 {
            self.win_map = false;
            return;
        }
        if pressed & L != 0 {
            let t = self.map_tab();
            self.set_map_tab(t + 4);
        }
        if pressed & R != 0 {
            let t = self.map_tab();
            self.set_map_tab(t + 1);
        }
        if pressed & X != 0 {
            self.s.map_mode = (self.s.map_mode + 1) % 2;
        }
        let ls = self.lstick;
        let rs = self.rstick;
        let tab = self.map_tab();
        if tab == 3 {
            // 100 %: ↑/↓ Kategorie, rechter Stick oder ←/→ blättert
            let d = self.rep_dir(pad, dt);
            if d.1 != 0 {
                self.map_list_cat = (self.map_list_cat as i32 + d.1).rem_euclid(8) as usize;
                self.map_list_scroll = 0.0;
            }
            if d.0 != 0 {
                self.map_list_scroll = (self.map_list_scroll + d.0 as f32 * 8.0).max(0.0);
            }
            if rs.y.abs() > 0.2 {
                self.map_list_scroll = (self.map_list_scroll - rs.y * 16.0 * dt).max(0.0);
            }
        } else if tab == 4 {
            let d = self.rep_dir(pad, dt);
            let vis = self.quest_visible();
            if d.1 != 0 {
                self.map_quest_sel = (self.map_quest_sel as i32 + d.1).clamp(0, vis.len() as i32 - 1).max(0) as usize;
            }
            if pressed & Y != 0 {
                if let Some(&k) = vis.get(self.map_quest_sel) {
                    if let Some(g) = &mut self.game {
                        if QUESTS[k].state(&g.snes) == QState::Active {
                            g.track = if g.track == Some(k) { None } else { Some(k) };
                        }
                    }
                }
            }
        } else if tab == 2 {
            // Dungeon: Raum-Cursor mit Stick und ←/→ (springt über leere Felder), ↑/↓ Stockwerk,
            // rechter Stick zoomt (bis ein Raum die Karte füllt), Y = nächster Dungeon
            // ←/→ (Steuerkreuz und linker Stick): nächster/voriger Raum des Stockwerks (zeilenweise, so ist
            // jeder Raum erreichbar); ↑/↓ (Steuerkreuz und linker Stick): Stockwerk; rechter Stick: Zoom
            // linker Stick: Cursor frei in alle Richtungen (springt über leere Felder);
            // Steuerkreuz ←/→: nächster/voriger Raum (zeilenweise), Steuerkreuz ↑/↓: Stockwerk
            let stick = self.lstick_pad & (UP | DOWN | LEFT | RIGHT);
            let dp = pad & !stick;
            let stick_on = self.lstick.length() > 0.5;
            let d = self.rep_dir(dp & (LEFT | RIGHT), dt);
            if stick_on && d != (0, 0) {
                let cells = self.dungeon_cells();
                let occ = |x: i32, y: i32| (0..5).contains(&x) && (0..5).contains(&y) && cells.get((y * 5 + x) as usize).map_or(false, |&r| r != 0x0F);
                let (cx, cy) = self.map_room_cur;
                let mut best = None;
                if d.0 != 0 && d.1 != 0 {
                    best = Some(((cx + d.0).clamp(0, 4), (cy + d.1).clamp(0, 4)));
                } else {
                    'outer: for k in 1..5 {
                        for o in [0, 1, -1, 2, -2] {
                            let (x, y) = if d.0 != 0 { (cx + d.0 * k, cy + o) } else { (cx + o, cy + d.1 * k) };
                            if occ(x, y) {
                                best = Some((x, y));
                                break 'outer;
                            }
                        }
                    }
                }
                if let Some(b) = best {
                    self.map_room_cur = b;
                }
            } else if d.0 != 0 {
                let cells = self.dungeon_cells();
                let rooms: Vec<usize> = (0..25usize).filter(|&i| cells.get(i).map_or(false, |&r| r != 0x0F)).collect();
                if !rooms.is_empty() {
                    let (cx, cy) = self.map_room_cur;
                    let cur = (cy * 5 + cx) as usize;
                    let next = if d.0 > 0 {
                        rooms.iter().copied().find(|&i| i > cur).unwrap_or(rooms[0])
                    } else {
                        rooms.iter().rev().copied().find(|&i| i < cur).unwrap_or(rooms[rooms.len() - 1])
                    };
                    self.map_room_cur = ((next % 5) as i32, (next / 5) as i32);
                }
            }
            let fy = if dp & UP != 0 { 1 } else if dp & DOWN != 0 { -1 } else { 0 };
            if fy != 0 && fy != self.map_rflick {
                let n = self.game.as_ref().and_then(|g| g.dmaps.get(self.map_dsel as usize)).map_or(1, |m| m.floors.len());
                let cur = self.map_floor.unwrap_or(0) as i32;
                self.map_floor = Some((cur + fy).clamp(0, n as i32 - 1) as usize);
                self.snap_room_cur();
            }
            self.map_rflick = fy;
            // rechter Stick: nach oben/rechts hinein-, nach unten/links herauszoomen
            let z = if rs.y.abs() > rs.x.abs() { rs.y } else { rs.x };
            if z.abs() > 0.2 {
                self.map_dzoom = (self.map_dzoom * (z * 2.2 * dt).exp()).clamp(1.0, DZOOM_MAX);
            }
            if pressed & Y != 0 {
                self.map_dsel = (self.map_dsel + 1) % world::DMAP_NAMES.len() as u8;
                self.map_floor_dungeon = None;
            }
        } else {
            // Welt: verschieben (Steuerkreuz + linker Stick), zoomen (rechter Stick)
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
            // Fadenkreuz frei bewegen; am Rand (65 %) schiebt es die Karte mit
            let zoom = self.map_zoom.max(0.05);
            let half = self.map_half;
            if v != gfx::Vec2::ZERO {
                self.map_cur += v * half.y.max(80.0) * 1.3 * dt;
                let m = half * 0.65;
                let over = self.map_cur - gfx::vec2(self.map_cur.x.clamp(-m.x, m.x), self.map_cur.y.clamp(-m.y, m.y));
                self.map_center += over / zoom;
                self.map_cur -= over;
            }
            if rs.y.abs() > 0.2 {
                // um das Fadenkreuz herum zoomen
                let at = self.map_center + self.map_cur / zoom;
                self.map_zoom = (self.map_zoom * (rs.y * 2.2 * dt).exp()).clamp(self.map_fit * 0.9, 6.0);
                self.map_center = at - self.map_cur / self.map_zoom;
            }
            self.map_center = gfx::vec2(self.map_center.x.clamp(0.0, 4096.0), self.map_center.y.clamp(0.0, 4096.0));
            if pressed & Y != 0 {
                self.s.map_filter = (self.s.map_filter + 1) % 6;
            }
        }
        let _ = (UP, DOWN, LEFT, RIGHT);
        if pressed & A != 0 {
            self.map_activate();
        }
    }

    /// Räume des gezeigten Stockwerks der Dungeon-Karte (25, 0x0F = leer)
    fn dungeon_cells(&self) -> Vec<u8> {
        self.game
            .as_ref()
            .and_then(|g| g.dmaps.get(self.map_dsel as usize))
            .map(|m| m.floors[self.map_floor.unwrap_or(0).min(m.floors.len() - 1)].to_vec())
            .unwrap_or_default()
    }

    /// Nach einem Stockwerkwechsel: Cursor auf den nächstgelegenen Raum des neuen Stockwerks
    fn snap_room_cur(&mut self) {
        let cells = self.dungeon_cells();
        let (cx, cy) = self.map_room_cur;
        if let Some(i) = (0..25usize).filter(|&i| cells.get(i).map_or(false, |&r| r != 0x0F)).min_by_key(|&i| {
            let (x, y) = ((i % 5) as i32, (i / 5) as i32);
            (x - cx).pow(2) + (y - cy).pow(2)
        }) {
            self.map_room_cur = ((i % 5) as i32, (i / 5) as i32);
        }
    }

    /// A / Klick: Teleport zum ausgewählten Eingang, Minispiel abhaken
    fn map_activate(&mut self) {
        // eigene Markierung setzen
        if let Some((w, x, y, sym)) = self.map_pick.take() {
            if let Some(g) = &mut self.game {
                g.pins.push((w, x, y, sym));
            }
            return;
        }
        let tab = self.map_tab();
        if tab < 2 && self.s.map_filter == 5 {
            if let Some(&(_, bit)) = self.map_custom_rows.iter().find(|(r, _)| r.contains(self.map_aim)) {
                self.s.map_custom ^= 1 << bit;
                return;
            }
        }
        if tab == 4 {
            // Quest auf der Karte zeigen
            if let Some((w, x, y)) = self.quest_visible().get(self.map_quest_sel).and_then(|&k| self.quest_target(k)) {
                self.set_map_tab(w);
                self.map_center = gfx::vec2(x, y);
                self.map_cur = gfx::Vec2::ZERO;
            }
            return;
        }
        if tab == 3 {
            return;
        }
        match self.map_sel {
            Some(MapSel::Pin(i)) => {
                if let Some(g) = &mut self.game {
                    if i < g.pins.len() {
                        g.pins.remove(i);
                    }
                }
            }
            None if tab < 2 => {
                let at = self.map_center + self.map_cur / self.map_zoom.max(0.01);
                self.map_pick = Some((tab, at.x, at.y, 0));
            }
            Some(MapSel::Door(i)) => {
                if let Some(e) = self.game.as_ref().and_then(|g| g.ents.get(i).copied()) {
                    if self.visited(&e) {
                        self.win_map = false;
                        self.teleport(e);
                    }
                }
            }
            Some(MapSel::Loc(i)) => {
                if let Some(g) = &mut self.game {
                    let _ = (i, &g.manual);
                }
            }
            _ => {}
        }
    }

    /// Text im Stil der Karte (Pixel-Schrift im Pixel-Modus)
    pub(crate) fn mtext(&self, p: &gfx::Painter, pos: gfx::Pos2, align: gfx::Align2, text: &str, size: f32, col: Color32) -> gfx::Rect {
        if self.pixel_ui() {
            let gp = (size / 8.0).round().max(1.0);
            crate::pixfont::text(p, pos, align, text, gp, col)
        } else {
            let size = size.max(15.0);
            let g = p.layout_no_wrap(text.to_string(), gfx::FontId::proportional(size), col);
            let sh = p.layout_no_wrap(text.to_string(), gfx::FontId::proportional(size), Color32::from_black_alpha(170));
            let r = align.anchor_size(pos, g.size());
            // dezenter Schatten für Lesbarkeit
            p.galley(r.min + gfx::vec2(1.0, 1.0), sh, Color32::BLACK);
            p.galley(r.min, g, col);
            r
        }
    }

    /// Text auf eine Breite kürzen (mit „…“)
    pub(crate) fn fit_text(&self, p: &gfx::Painter, text: &str, size: f32, maxw: f32) -> String {
        if self.text_w(p, text, size) <= maxw {
            return text.to_string();
        }
        let mut v: Vec<char> = text.chars().collect();
        while !v.is_empty() {
            v.pop();
            let t: String = v.iter().collect::<String>().trim_end().to_string() + "…";
            if self.text_w(p, &t, size) <= maxw {
                return t;
            }
        }
        String::new()
    }

    /// Link auf der Karte: grüner Punkt (Pixel-Stil mit Kante und Glanzpunkt, im HD-Stil weich)
    pub(crate) fn link_dot(&self, p: &gfx::Painter, lp: gfx::Pos2, u: f32, rr: f32) {
        if self.smooth_ui() {
            p.circle_filled(lp + gfx::vec2(0.0, 1.5), rr + 2.5, Color32::from_black_alpha(120));
            p.circle_filled(lp, rr + 2.0, Color32::WHITE);
            p.circle_filled(lp, rr, Color32::from_rgb(56, 200, 80));
            p.circle_filled(lp - gfx::vec2(rr * 0.3, rr * 0.3), rr * 0.3, Color32::from_white_alpha(140));
            return;
        }
        let dot = |p: &gfx::Painter, rows: &[f32], col: Color32| {
            let h = rows.len() as f32;
            for (k, wdt) in rows.iter().enumerate() {
                let y0 = lp.y + (k as f32 - h * 0.5) * u;
                p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(lp.x - wdt * 0.5 * u, y0), gfx::vec2(wdt * u, u)), 0.0, col);
            }
        };
        dot(p, &[3.0, 5.0, 7.0, 7.0, 7.0, 5.0, 3.0], Color32::from_rgb(16, 16, 24));
        dot(p, &[0.0, 3.0, 5.0, 5.0, 5.0, 3.0, 0.0], Color32::from_rgb(64, 216, 88));
        p.rect_filled(gfx::Rect::from_min_size(lp + gfx::vec2(-u * 1.5, -u * 2.5), gfx::vec2(u, u)), 0.0, Color32::from_rgb(200, 255, 200));
    }

    /// Textbreite im Stil der Karte
    pub(crate) fn text_w(&self, p: &gfx::Painter, text: &str, size: f32) -> f32 {
        if self.pixel_ui() {
            crate::pixfont::width(text, (size / 8.0).round().max(1.0))
        } else {
            p.layout_no_wrap(text.to_string(), gfx::FontId::proportional(size.max(15.0)), Color32::WHITE).size().x
        }
    }

    /// Rahmen im Remake-Stil: dunkel, goldene Doppellinie, Eck-Rauten
    pub(crate) fn frame(&self, p: &gfx::Painter, r: gfx::Rect, fill: Color32) {
        // wie die Kästen im Spielmenü
        let _ = fill;
        self.gbox(p, r, None, 0.0);
    }

    /// Vollbild-Karte zeichnen (über dem Spielbild)
    pub(crate) fn map_overlay(&mut self, ctx: &gfx::Context) {
        if !self.win_map || self.s.map_own_window || self.game.is_none() {
            return;
        }
        // nur über dem Spielbild – Fenster und Menüs der App bleiben darüber
        let area = if self.game_area.width() > 10.0 { self.game_area } else { ctx.screen_rect() };
        self.map_draw(ctx, area);
    }

    /// Karte im eigenen Fenster: genau dieselbe Darstellung, nur füllt sie das ganze Fenster
    pub(crate) fn map_window(&mut self, ctx: &gfx::Context) {
        if self.game.is_none() {
            return;
        }
        let area = ctx.screen_rect();
        ctx.layer_painter(gfx::LayerId::background()).rect_filled(area, 0.0, Color32::BLACK);
        // Pixelgröße passend zu diesem Fenster (wie ein Spielbild in Fenstergröße)
        let old = self.game_px;
        self.game_px = (area.height() / 224.0).max(1.0);
        self.map_draw(ctx, area);
        self.game_px = old;
    }

    fn map_draw(&mut self, ctx: &gfx::Context, area: gfx::Rect) {
        ctx.request_repaint();
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let gp = self.game_px.round().max(1.0);
        let painter = ctx.layer_painter(gfx::LayerId::background()).with_clip_rect(area);
        painter.rect_filled(area, 0.0, Color32::from_black_alpha(170));
        // möglichst viel Platz für die Karte: schmaler Rand
        let panel = area.shrink(area.width().min(area.height()) * 0.008);
        self.frame(&painter, panel, NAVY);
        let fs = if pix { gp * 8.0 } else { (panel.height() * 0.038).clamp(17.0, 32.0) };
        let header = fs * 2.0;
        let footer = fs * if pix { 3.1 } else { 4.0 };
        let inner = panel.shrink(fs * 0.5);
        let map_rect = gfx::Rect::from_min_max(inner.min + gfx::vec2(0.0, header), inner.max - gfx::vec2(0.0, footer));
        // ---- Reiter
        let tab = self.map_tab();
        let names = [tr(en, "LICHTWELT", "LIGHT WORLD"), tr(en, "SCHATTENWELT", "DARK WORLD"), "DUNGEON", "100 %", "QUESTS"];
        let tabs_l = inner.left() + fs * 2.4;
        let tw = (inner.width() - fs * 4.8) / 5.0;
        let mut ts = fs;
        while names.iter().any(|n| self.text_w(&painter, n, ts) > tw - fs * 0.4) && ts > 9.0 {
            ts -= if pix { 8.0 } else { 1.0 };
        }
        for (i, n) in names.iter().enumerate() {
            let c = gfx::pos2(tabs_l + tw * (i as f32 + 0.5), inner.top() + header * 0.45);
            let sel = i as u8 == tab;
            if sel {
                // gewählter Reiter: heller Kasten wie die Auswahl im Spielmenü
                let w = self.text_w(&painter, n, ts) + fs * 1.0;
                let tb = gfx::Rect::from_center_size(c, gfx::vec2(w, fs * 1.4));
                painter.rect_filled(tb.expand(if pix { gp } else { 1.0 }), 0.0, Color32::from_rgb(0, 0, 0));
                painter.rect_filled(tb, 0.0, crate::inventory::MENU_LINE);
            }
            self.mtext(&painter, c, gfx::Align2::CENTER_CENTER, n, ts, if sel { crate::inventory::MENU_BG } else { Color32::from_gray(170) });
        }
        for (txt, x, al) in [("L", inner.left() + fs * 0.6, gfx::Align2::LEFT_CENTER), ("R", inner.right() - fs * 0.6, gfx::Align2::RIGHT_CENTER)] {
            let br = al.anchor_size(gfx::pos2(x, inner.top() + header * 0.45), gfx::vec2(fs * 1.5, fs * 1.2));
            painter.rect_filled(br.expand(if pix { gp } else { 1.0 }), 0.0, Color32::from_rgb(0, 0, 0));
            painter.rect_filled(br, 0.0, Color32::from_gray(200));
            self.mtext(&painter, br.center(), gfx::Align2::CENTER_CENTER, txt, fs * 0.8, crate::inventory::MENU_BG);
        }
        // ---- Karte
        self.frame(&painter, map_rect.expand(if pix { gp } else { 3.0 }), PANEL);
        let mp = painter.with_clip_rect(map_rect);
        let (info, info2) = match tab {
            2 => self.draw_dungeon_overlay(ctx, &mp, map_rect, fs),
            3 => self.draw_list_page(ctx, &mp, map_rect, fs),
            4 => self.draw_quest_page(ctx, &mp, map_rect, fs),
            _ => self.draw_world_overlay(ctx, &mp, map_rect, fs),
        };
        // ---- Fußzeile: Auswahl, Fortschritt, Tasten
        let fy = map_rect.bottom() + fs * 0.6;
        self.mtext(&painter, gfx::pos2(inner.left() + fs * 0.4, fy), gfx::Align2::LEFT_TOP, &info, fs, Color32::WHITE);
        let mut info_right = inner.left();
        if !info2.is_empty() {
            let r2 = self.mtext(&painter, gfx::pos2(inner.left() + fs * 0.4, fy + fs * 1.35), gfx::Align2::LEFT_TOP, &info2, fs * 0.8, Color32::from_gray(185));
            info_right = r2.right();
        }
        // Dungeon-Karte: Fortschritt dieses Dungeons (das Gesamte steht darunter)
        let dpct = (tab == 2).then(|| self.completion_dungeon(self.map_dsel));
        let pct = dpct.unwrap_or_else(|| self.completion());
        let pr = self.mtext(&painter, gfx::pos2(inner.right() - fs * 0.4, fy - fs * 0.1), gfx::Align2::RIGHT_TOP, &format!("{pct:.1} % / 100 %"), fs * 1.25, if pct >= 99.95 { Color32::from_rgb(120, 230, 120) } else { GOLD });
        let tp = self.completion_true();
        let mode = if self.s.map_mode == 0 { tr(en, "ALLES", "ALL") } else { tr(en, "GESAMMELT", "COLLECTED") };
        // rechts neben der Ortsbeschreibung: echte 100 % (und die Ansicht, wenn Platz ist)
        let avail = pr.right() - info_right - fs * 1.5;
        let long = format!("{} {tp:.1} % / 100 %   {}: {mode}", tr(en, "ECHT:", "TRUE:"), tr(en, "ANSICHT", "VIEW"));
        let short = format!("{} {tp:.1} % / 100 %", tr(en, "ECHT:", "TRUE:"));
        let line2 = if let Some(_) = dpct {
            let (de, e) = crate::world::DMAP_NAMES.get(self.map_dsel as usize).copied().unwrap_or(("Dungeon", "Dungeon"));
            let long = format!("{}   {} {:.1} %", (if en { e } else { de }).to_uppercase(), tr(en, "GESAMT:", "TOTAL:"), self.completion());
            let short = format!("{} {:.1} %", tr(en, "GESAMT:", "TOTAL:"), self.completion());
            if self.text_w(&painter, &long, fs * 0.7) <= avail { long } else { short }
        } else if self.text_w(&painter, &long, fs * 0.7) <= avail {
            long
        } else {
            short
        };
        self.mtext(&painter, gfx::pos2(pr.right(), pr.bottom() + fs * 0.25), gfx::Align2::RIGHT_TOP, &line2, fs * 0.7, if tp >= 99.95 { Color32::from_rgb(120, 230, 120) } else { Color32::from_gray(200) });
        let hint = if self.map_pick.is_some() {
            tr(en, "◀ ▶ SYMBOL  A SETZEN  B ABBRECHEN", "◀ ▶ SYMBOL   A PLACE   B CANCEL")
        } else if tab == 3 {
            tr(en, "L/R KARTE  STICK KATEGORIE  R-STICK BLÄTTERN  B ZURÜCK", "L/R MAP   STICK CATEGORY   R-STICK SCROLL   B BACK")
        } else if tab == 4 {
            tr(en, "L/R KARTE  STICK QUEST  Y VERFOLGEN  A AUF KARTE ZEIGEN  B ZURÜCK", "L/R MAP   STICK QUEST   Y TRACK   A SHOW ON MAP   B BACK")
        } else if tab == 2 {
            tr(en, "L/R KARTE  STICK RAUM  HOCH/RUNTER STOCKWERK  R-STICK ZOOM/VORSCHAU  Y DUNGEON  A REISEN  X ANSICHT  B ZURÜCK", "L/R MAP   STICK ROOM   UP/DOWN FLOOR   R-STICK ZOOM/PREVIEW   Y DUNGEON   A TELEPORT   X VIEW   B BACK")
        } else {
            tr(en, "L/R KARTE  STICK BEWEGEN  R-STICK ZOOM  Y ANZEIGE  A REISEN/MARKIEREN  X SPOILER  B ZURÜCK", "L/R MAP   STICK MOVE   R-STICK ZOOM   Y SHOW   A TRAVEL/MARK   X SPOILER   B BACK")
        };
        let hint_hd = hint.replace("  ", "     ");
        let hint: &str = if pix { hint } else { &hint_hd };
        let mut hs = fs * if pix { 0.5 } else { 0.52 };
        let maxw = inner.width() - fs;
        loop {
            let w = if pix { crate::pixfont::width(hint, (hs / 8.0).round().max(1.0)) } else { painter.layout_no_wrap(hint.to_string(), gfx::FontId::proportional(hs), Color32::WHITE).size().x };
            if w <= maxw || hs <= 8.0 {
                break;
            }
            hs -= if pix { 8.0 } else { 1.0 };
        }
        // keine Tastenzeile mehr (stört); die Tasten stehen in der Fußzeile
        let _ = (hint, hs);
        // Maus: Rad = Zoom, Klick = auswählen/Teleport
        let (scroll, clicked, pos, drag) = ctx.input(|i| (i.smooth_scroll_delta.y, i.pointer.primary_clicked(), i.pointer.hover_pos(), if i.pointer.primary_down() { i.pointer.delta() } else { gfx::Vec2::ZERO }));
        if pos.map_or(false, |p| map_rect.contains(p)) {
            if tab < 2 {
                if scroll.abs() > 0.1 {
                    self.map_zoom = (self.map_zoom * (scroll * 0.003).exp()).clamp(self.map_fit * 0.9, 6.0);
                }
                if drag.length() > 0.0 {
                    self.map_center -= drag / self.map_zoom.max(0.05);
                }
                if let Some(p) = pos {
                    if ctx.input(|i| i.pointer.delta().length() > 0.0) {
                        self.map_cur = p - map_rect.center();
                    }
                }
            }
            if tab == 2 {
                if scroll.abs() > 0.1 {
                    self.map_dzoom = (self.map_dzoom * (scroll * 0.003).exp()).clamp(1.0, DZOOM_MAX);
                }
                // Klick auf einen Raum wählt ihn aus (ein zweiter Klick auf denselben: Teleport)
                if clicked {
                    if let Some(p) = pos {
                        let (o, c) = self.map_dgeo;
                        if c > 0.0 {
                            let (x, y) = (((p.x - o.x) / c).floor() as i32, ((p.y - o.y) / c).floor() as i32);
                            let cells = self.dungeon_cells();
                            if (0..5).contains(&x) && (0..5).contains(&y) && cells.get((y * 5 + x) as usize).map_or(false, |&r| r != 0x0F) && self.map_room_cur != (x, y) {
                                self.map_room_cur = (x, y);
                                return;
                            }
                        }
                    }
                }
            }
            if clicked {
                self.map_activate();
            }
        }
    }

    /// Welt-Karte; Rückgabe: (Auswahl-Text, Zusatz)
    fn draw_world_overlay(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let world = self.map_world.min(1) as usize;
        let spoiler = self.s.map_mode == 0;
        // Kartenbild (Pixel: scharf, HD: weich)
        let key = (world as u8, pix);
        // maßstabsgetreue Karte aus den Gebieten einmal im Hintergrund berechnen
        if let Some((w, rx)) = &self.map_real_job {
            if let Ok(v) = rx.try_recv() {
                self.map_real[*w as usize] = Some(v);
                self.map_real_job = None;
                self.map_hd = [None, None];
                self.map_ov_key = None;
            }
        }
        if self.map_real[world].is_none() && self.map_real_job.is_none() {
            if let Some(tab) = self.game.as_ref().and_then(|g| g.m16tab.clone()) {
                let (tx, rx) = std::sync::mpsc::channel();
                let w = world as u8;
                std::thread::spawn(move || {
                    let _ = tx.send(crate::snes::ppu::world_from_areas(&tab, w));
                });
                self.map_real_job = Some((w, rx));
            }
        }
        if self.map_ov_key != Some(key) || self.map_ov_tex.is_none() {
            self.ensure_lut();
            let real = self.map_real[world].as_ref();
            let lut = &self.lut;
            let composed = self.game.as_ref().and_then(|g| g.tracker.full[world].clone()).map(|mut img| {
                if let Some(rv) = real {
                    for (o, v) in img.iter_mut().zip(rv.iter()) {
                        if let Some(c) = v {
                            *o = lut.get(*c as usize & 0x7FFF).copied().unwrap_or(*o);
                        }
                    }
                }
                img
            });
            if let Some(img) = composed {
                let opts = if pix { gfx::TextureOptions::NEAREST } else { gfx::TextureOptions::LINEAR };
                if !pix && self.map_hd[world].is_none() && self.map_hd_job.is_none() {
                    // HD: Karte einmal mit xBRZ x4 hochrechnen (im Hintergrund, alle Kerne)
                    let (tx, rx) = std::sync::mpsc::channel();
                    let src = img.clone();
                    std::thread::spawn(move || {
                        let mut out = Vec::new();
                        crate::filters::xbrz_par(&src, crate::tracker::DIM, crate::tracker::DIM, 4, &mut out);
                        let _ = tx.send(out);
                    });
                    self.map_hd_job = Some((world as u8, rx));
                }
                self.map_ov_tex = Some(match (&self.map_hd[world], pix) {
                    (Some(t), false) => t.clone(),
                    _ => ctx.load_texture("map_overlay", gfx::ColorImage { size: [crate::tracker::DIM, crate::tracker::DIM], pixels: img }, opts),
                });
                self.map_ov_key = Some(key);
            }
        }
        if let Some((w, rx)) = &self.map_hd_job {
            if let Ok(px) = rx.try_recv() {
                let w = *w as usize;
                let n = crate::tracker::DIM * 4;
                self.map_hd[w] = Some(ctx.load_texture(format!("map_hd{w}"), gfx::ColorImage { size: [n, n], pixels: px }, gfx::TextureOptions::LINEAR));
                self.map_hd_job = None;
                self.map_ov_key = None;
            }
        }
        // ganz herausgezoomt ist die ganze Welt zu sehen (passt vollständig in den Ausschnitt)
        self.map_fit = (r.width().min(r.height()) / 4096.0).max(0.02);
        if self.map_zoom < self.map_fit * 0.9 {
            self.map_zoom = self.map_fit;
        }
        let zoom = self.map_zoom;
        self.map_half = r.size() * 0.5;
        let half = r.size() * 0.5 / zoom;
        let cl = |c: f32, h: f32| if h * 2.0 >= 4096.0 { 2048.0 } else { c.clamp(h, 4096.0 - h) };
        let want = self.map_center;
        self.map_center = gfx::vec2(cl(want.x, half.x), cl(want.y, half.y));
        // was die Karte am Rand nicht mehr schieben kann, läuft das Fadenkreuz weiter
        self.map_cur += (want - self.map_center) * zoom;
        let lim = r.size() * 0.5 - gfx::vec2(8.0, 8.0);
        self.map_cur = gfx::vec2(self.map_cur.x.clamp(-lim.x, lim.x), self.map_cur.y.clamp(-lim.y, lim.y));
        let center = self.map_center;
        let to_screen = |x: f32, y: f32| r.center() + (gfx::vec2(x, y) - center) * zoom;
        let img = gfx::Rect::from_min_max(to_screen(0.0, 0.0), to_screen(4096.0, 4096.0));
        p.rect_filled(img, 0.0, Color32::from_rgb(24, 40, 60));
        if let Some(t) = &self.map_ov_tex {
            p.image(t.id(), img, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            self.mtext(p, r.center(), gfx::Align2::CENTER_CENTER, tr(en, "Weltkarte wird erstellt …", "Creating world map …"), fs, Color32::WHITE);
        }
        let g = self.game.as_ref().unwrap();
        let rad = if pix { (self.game_px * 3.0).round().max(4.0) } else { (8.0 + zoom * 10.0).clamp(8.0, 15.0) };
        // herausgezoomt: Übersicht mit weniger Details
        let overview = zoom < self.map_fit * 1.8;
        let mut marks: Vec<(gfx::Pos2, Icon, Color32, MapSel, String, String)> = Vec::new();
        // Anzeige-Filter (Y): 0 Übersicht, 1 Sammeln, 2 Teleport, 3 Quests, 4 Alles
        let filt = self.s.map_filter.min(5);
        // Eigene (5): jede Art einzeln abhakbar
        let cm = self.s.map_custom;
        let has = |b: u8| cm & (1 << b) != 0;
        let custom = filt == 5;
        let show_dungeon = !custom || has(0);
        let show_door = if custom { has(1) } else { filt == 0 || filt == 2 || filt == 4 };
        let show_heart = if custom { has(2) } else { filt == 1 || filt == 4 };
        let show_chest = if custom { has(3) } else { filt == 1 || filt == 4 };
        let show_bottle = if custom { has(4) } else { filt == 1 || filt == 4 };
        let show_upgrade = if custom { has(5) } else { filt == 1 || filt == 4 };
        let show_quest = if custom { has(6) } else { filt == 0 || filt == 3 || filt == 4 };
        let show_pin = if custom { has(7) } else { filt != 2 && filt != 3 };
        let show_loc = show_heart || show_chest;
        let show_poi = show_bottle || show_upgrade;
        // Dungeons
        for (i, (_, _, _, de, e)) in DUNGEONS.iter().enumerate() {
            let (w, x, y) = g.dungeons[i];
            if !show_dungeon || w as usize != world {
                continue;
            }
            let boss = world::BOSS_ROOMS[i];
            let done = boss.map_or(false, |b| g.snes.bus.wram[0xF000 + b as usize * 2 + 1] & 0x08 != 0);
            let st = if done { tr(en, "Boss besiegt", "boss defeated") } else { tr(en, "Boss noch nicht besiegt", "boss not defeated yet") };
            marks.push((to_screen(x, y), Icon::Dungeon, if done { DUNGEON } else { GREYC }, MapSel::Dungeon(i), (if en { *e } else { *de }).to_string(), st.to_string()));
        }
        // Orte für 100 %
        for (i, l) in g.locations.iter().enumerate() {
            // Dungeon-Truhen sonst nur auf der Dungeon-Karte – bei „Alles“ (und „Eigene“ mit Truhen) auch hier am Eingang
            let all_chests = filt == 4 || (custom && show_chest);
            if !show_loc || l.world as usize != world || (l.dungeon.is_some() && l.kind == LocKind::Chests && !all_chests) {
                continue;
            }
            if (l.kind == LocKind::Chests && !show_chest) || (l.kind != LocKind::Chests && !show_heart) {
                continue;
            }
            let (d, n) = (l.done(&g.snes, g.manual.contains(&i)), l.total());
            let complete = d >= n;
            if !spoiler && !complete {
                continue;
            }
            let (icon, col) = match l.kind {
                LocKind::HeartPiece => (Icon::Heart, HEART),
                LocKind::HeartContainer => (Icon::Container, HEART),
                LocKind::Chests => (Icon::Chest, CHEST),
                LocKind::Minigame => (Icon::Game, GAME),
            };
            let st = if n > 1 {
                format!("{d}/{n} {}", tr(en, "geöffnet", "opened"))
            } else if complete {
                tr(en, "erledigt", "done").to_string()
            } else if l.kind == LocKind::Minigame {
                tr(en, "fehlt noch", "still missing").to_string()
            } else {
                tr(en, "fehlt noch", "still missing").to_string()
            };
            marks.push((to_screen(l.x, l.y), icon, if complete { col } else { GREYC }, MapSel::Loc(i), l.name(en).to_string(), st));
        }
        // Eingänge / Teleport-Punkte
        let mut seen: Vec<(f32, f32)> = Vec::new();
        for (ei, e) in g.ents.iter().enumerate().filter(|(_, e)| show_door && e.world as usize == world) {
            if seen.iter().any(|(x, y)| (x - e.x).abs() < 1.0 && (y - e.y).abs() < 1.0) {
                continue;
            }
            seen.push((e.x, e.y));
            let vis = self.visited(e);
            if !spoiler && !vis {
                continue;
            }
            let name = match world::room_dungeon(&g.dmaps, e.room) {
                Some(d) => (if en { world::DMAP_NAMES[d as usize].1 } else { world::DMAP_NAMES[d as usize].0 }).to_string(),
                None => world::area_name(e.area, en),
            };
            let st = if vis { tr(en, "Teleport-Punkt – A: hinreisen", "teleport point – A: travel") } else { tr(en, "noch nicht betreten", "not entered yet") };
            marks.push((to_screen(e.x, e.y), Icon::Door, if vis { DOOR } else { GREYC }, MapSel::Door(ei), name, st.to_string()));
        }
        // Flaschen und Upgrades
        let sn = &g.snes;
        for (k, i, poi) in BOTTLES.iter().enumerate().map(|(i, p)| (0u8, i, p)).chain(UPGRADES.iter().enumerate().map(|(i, p)| (1u8, i, p))) {
            let npc = if k == 0 { crate::mapdata::BOTTLE_NPC[i] } else { crate::mapdata::UPGRADE_NPC[i] };
            let (w, x, y) = crate::mapdata::npc_pos(&g.npcs, npc, poi.pos());
            let done = poi.done(sn);
            if !show_poi || (k == 0 && !show_bottle) || (k == 1 && !show_upgrade) || w as usize != world || (!spoiler && !done) {
                continue;
            }
            let (a, b) = (poi.prog)(sn);
            let st = if b > 1 {
                format!("{a}/{b}")
            } else if done {
                tr(en, "erledigt", "done").to_string()
            } else {
                tr(en, "fehlt noch", "still missing").to_string()
            };
            marks.push((to_screen(x, y), if k == 0 { Icon::Bottle } else { Icon::Upgrade }, if done { Color32::WHITE } else { GREYC }, MapSel::Poi(k, i), (if en { poi.en } else { poi.de }).to_string(), st));
        }
        // Startpunkte der Nebenquests (Spoiler: alle, sonst nur erledigte)
        {
            for (i, q) in QUESTS.iter().enumerate() {
                let Some(est) = q.pos() else { continue };
                let (w, x, y) = crate::mapdata::npc_pos(&g.npcs, crate::mapdata::QUEST_NPC.get(i).copied().unwrap_or(0), est);
                if w as usize != world {
                    continue;
                }
                let st = q.state(sn);
                // ohne Spoiler: nur Quests, die man schon bekommen kann (und erledigte)
                if !show_quest || (!spoiler && st == QState::Locked) {
                    continue;
                }
                // Übersicht: nur Quests, die man jetzt machen kann
                if filt == 0 && st != QState::Active {
                    continue;
                }
                let txt = match st {
                    QState::Done => tr(en, "erledigt", "done"),
                    QState::Active => tr(en, "jetzt möglich", "possible now"),
                    QState::Locked => tr(en, "noch nicht möglich", "not possible yet"),
                };
                marks.push((to_screen(x, y), Icon::Quest, if st == QState::Done { Color32::WHITE } else { GREYC }, MapSel::Poi(2, i), (if en { q.en } else { q.de }).to_string(), format!("{txt} – {}", if en { q.hint_en } else { q.hint_de })));
            }
        }
        // eigene Markierungen
        for (i, &(w, x, y, sym)) in g.pins.iter().enumerate() {
            if show_pin && w as usize == world {
                marks.push((to_screen(x, y), PIN_ICONS[sym as usize % 5], Color32::WHITE, MapSel::Pin(i), tr(en, "Eigene Markierung", "Own marker").to_string(), tr(en, "A: entfernen", "A: remove").to_string()));
            }
        }
        // Symbole am selben Ort leicht auseinanderziehen, damit keins ein anderes verdeckt
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for k in 0..marks.len() {
            match groups.iter_mut().find(|gr| marks[gr[0]].0.distance(marks[k].0) < rad * 1.4) {
                Some(gr) => gr.push(k),
                None => groups.push(vec![k]),
            }
        }
        for gr in groups.iter().filter(|gr| gr.len() > 1) {
            let c = gr.iter().fold(gfx::Vec2::ZERO, |a, &k| a + marks[k].0.to_vec2()) / gr.len() as f32;
            let n = gr.len() as f32;
            let d = rad * (0.75 + 0.18 * n);
            for (j, &k) in gr.iter().enumerate() {
                let a = std::f32::consts::TAU * j as f32 / n - std::f32::consts::FRAC_PI_2;
                let o = gfx::vec2(a.cos(), a.sin()) * d;
                marks[k].0 = (c + o).to_pos2();
            }
        }
        // Auswahl: Symbol am nächsten zum Fadenkreuz
        let aim = r.center() + self.map_cur;
        // über der Auswahlliste von „Eigene“ (Stand vom letzten Bild) nichts auf der Karte wählen
        let on_panel = custom
            && self.map_custom_rows.iter().map(|(rr, _)| *rr).reduce(|a, b| a.union(b)).map_or(false, |u| u.expand(fs * 0.3).contains(aim));
        let sel = marks
            .iter()
            .enumerate()
            .filter(|_| !on_panel)
            .filter(|(_, m)| m.0.distance(aim) < rad * 3.0)
            .min_by(|a, b| a.1 .0.distance(aim).total_cmp(&b.1 .0.distance(aim)))
            .map(|(k, _)| k);
        let t = ctx.input(|i| i.time) as f32;
        let gp = self.game_px.round().max(1.0);
        let blink = (t * 4.0) as i64 % 2 == 0;
        for (k, (pos, icon, col, ms, _, _)) in marks.iter().enumerate() {
            if !r.expand(rad * 2.0).contains(*pos) {
                continue;
            }
            let s = Some(k) == sel;
            if let (Icon::Door, MapSel::Door(_)) = (icon, ms) {
                // Höhle/Eingang als Symbol, Teleport-Punkt als Plakette wie in der Karte im Spiel
                let vis = *col == DOOR;
                if overview && !s {
                    // Übersicht: nur ein kleiner Punkt je Teleport-Punkt
                    if vis {
                        let d = if pix { gp * 3.0 } else { rad * 0.45 };
                        p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2(d + if pix { gp * 2.0 } else { 3.0 }, d + if pix { gp * 2.0 } else { 3.0 })), if pix { 0.0 } else { d }, Color32::from_rgb(16, 16, 24));
                        p.rect_filled(gfx::Rect::from_center_size(*pos, gfx::vec2(d, d)), if pix { 0.0 } else { d }, Color32::from_rgb(80, 216, 224));
                    }
                    continue;
                }
                paint_icon(p, *pos, if s { rad * 1.1 } else { rad * 0.9 }, Icon::Door, if vis { DOOR } else { GREYC });
                if vis || s {
                    let inner = if s {
                        if blink { Color32::from_rgb(248, 216, 64) } else { Color32::from_rgb(248, 248, 248) }
                    } else {
                        Color32::from_rgb(80, 216, 224)
                    };
                    let b = *pos + gfx::vec2(rad * 0.8, -rad * 0.8);
                    if pix {
                        let n = if s { 5.0 } else { 4.0 };
                        p.rect_filled(gfx::Rect::from_center_size(b, gfx::vec2((n + 2.0) * gp, (n + 2.0) * gp)), 0.0, Color32::from_rgb(16, 16, 24));
                        p.rect_filled(gfx::Rect::from_center_size(b, gfx::vec2(n * gp, n * gp)), 0.0, inner);
                        p.rect_filled(gfx::Rect::from_min_size(b - gfx::vec2(n * gp * 0.5, n * gp * 0.5), gfx::vec2(gp, gp)), 0.0, Color32::from_rgb(220, 255, 255));
                    } else {
                        let rr = rad * if s { 0.5 } else { 0.4 };
                        p.circle_filled(b, rr + 2.0, Color32::from_rgb(16, 16, 28));
                        p.circle_filled(b, rr, inner);
                        p.circle_filled(b - gfx::vec2(rr * 0.3, rr * 0.3), rr * 0.32, Color32::from_white_alpha(170));
                    }
                }
                continue;
            }
            let rr = if overview && !matches!(icon, Icon::Dungeon) { rad * 0.75 } else { rad };
            paint_icon(p, *pos, if s { rad * 1.15 } else { rr }, *icon, *col);
        }
        // Ziel der verfolgten Quest
        let target = self.quest_target_active();
        if let Some((qi, (w, x, y))) = target {
            if w as usize == world {
                self.draw_goal(p, r, to_screen(x, y), rad, gp, t);
            }
            let q = &QUESTS[qi];
            let other = w as usize != world;
            let txt = format!("{} {}{}", tr(en, "ZIEL:", "GOAL:"), if en { q.en } else { q.de }, if other { if w == 1 { tr(en, " (Schattenwelt)", " (Dark World)") } else { tr(en, " (Lichtwelt)", " (Light World)") } } else { "" });
            let small = fs * 0.6;
            let tw = self.text_w(p, &txt, small);
            let br = gfx::Rect::from_min_size(r.left_top() + gfx::vec2(fs * 0.4, fs * 0.4), gfx::vec2(tw + fs * 1.6, fs * 1.2));
            p.rect_filled(br, if pix { 0.0 } else { 6.0 }, Color32::from_rgba_unmultiplied(12, 14, 36, 225));
            p.rect_stroke(br, if pix { 0.0 } else { 6.0 }, Stroke::new(if pix { gp } else { 1.5 }, GOLD));
            paint_icon(p, br.left_center() + gfx::vec2(fs * 0.55, 0.0), fs * 0.32, Icon::Quest, Color32::WHITE);
            self.mtext(p, br.left_center() + gfx::vec2(fs * 1.1, 0.0), gfx::Align2::LEFT_CENTER, &txt, small, Color32::WHITE);
        }
        // welche Anzeige gerade aktiv ist (Y wechselt)
        {
            let names_de = ["ÜBERSICHT", "SAMMELN", "TELEPORT", "QUESTS", "ALLES", "EIGENE"];
            let names_en = ["OVERVIEW", "COLLECT", "TELEPORT", "QUESTS", "ALL", "CUSTOM"];
            let txt = format!("Y  {}", if en { names_en[filt as usize] } else { names_de[filt as usize] });
            let small = fs * 0.6;
            let tw = self.text_w(p, &txt, small);
            let br = gfx::Rect::from_min_size(gfx::pos2(r.right() - tw - fs * 1.6, r.top() + fs * 0.4), gfx::vec2(tw + fs * 1.2, fs * 1.2));
            p.rect_filled(br, if pix { 0.0 } else { 6.0 }, Color32::from_rgba_unmultiplied(0, 0, 0, 200));
            p.rect_stroke(br, if pix { 0.0 } else { 6.0 }, Stroke::new(if pix { gp } else { 1.5 }, crate::inventory::MENU_LINE));
            self.mtext(p, br.center(), gfx::Align2::CENTER_CENTER, &txt, small, crate::inventory::MENU_LINE);
            // Eigene: Liste zum An-/Abhaken (Fadenkreuz drauf + A, oder anklicken)
            self.map_custom_rows.clear();
            self.map_aim = r.center() + self.map_cur;
            if custom {
                let items_de = ["Dungeons", "Teleport-Punkte", "Herzen", "Truhen", "Flaschen", "Upgrades", "Quests", "Eigene Markierungen"];
                let items_en = ["Dungeons", "Teleport points", "Hearts", "Chests", "Bottles", "Upgrades", "Quests", "Own markers"];
                let icons = [Icon::Dungeon, Icon::Door, Icon::Heart, Icon::Chest, Icon::Bottle, Icon::Upgrade, Icon::Quest, Icon::Star];
                let lh = fs * 1.15;
                let pw = items_de.iter().chain(items_en.iter()).map(|t| self.text_w(p, t, small)).fold(0.0, f32::max) + fs * 3.4;
                let panel = gfx::Rect::from_min_size(gfx::pos2(r.right() - pw - fs * 0.4, br.bottom() + fs * 0.4), gfx::vec2(pw, lh * 8.0 + fs * 0.5));
                p.rect_filled(panel, if pix { 0.0 } else { 6.0 }, Color32::from_rgba_unmultiplied(0, 0, 0, 215));
                p.rect_stroke(panel, if pix { 0.0 } else { 6.0 }, Stroke::new(if pix { gp } else { 1.5 }, crate::inventory::MENU_LINE));
                for (k, ((de, e_), ic)) in items_de.iter().zip(items_en.iter()).zip(icons.iter()).enumerate() {
                    let row = gfx::Rect::from_min_size(gfx::pos2(panel.left() + fs * 0.25, panel.top() + fs * 0.25 + k as f32 * lh), gfx::vec2(pw - fs * 0.5, lh));
                    let on = has(k as u8);
                    if row.contains(self.map_aim) {
                        p.rect_filled(row, if pix { 0.0 } else { 4.0 }, Color32::from_rgba_unmultiplied(232, 194, 88, 60));
                    }
                    // Kästchen
                    let bx = gfx::Rect::from_center_size(row.left_center() + gfx::vec2(fs * 0.45, 0.0), gfx::vec2(fs * 0.55, fs * 0.55));
                    p.rect_stroke(bx, 0.0, Stroke::new(if pix { gp } else { 1.5 }, Color32::from_gray(200)));
                    if on {
                        paint_icon(p, bx.center(), fs * 0.3, Icon::Check, Color32::WHITE);
                    }
                    paint_icon(p, row.left_center() + gfx::vec2(fs * 1.25, 0.0), fs * 0.3, *ic, if on { Color32::WHITE } else { GREYC });
                    self.mtext(p, row.left_center() + gfx::vec2(fs * 1.95, 0.0), gfx::Align2::LEFT_CENTER, if en { e_ } else { de }, small, if on { Color32::WHITE } else { Color32::from_gray(110) });
                    self.map_custom_rows.push((row, k as u8));
                }
            }
        }
        // Link: grüner Punkt
        // drinnen (Haus, Höhle, Dungeon): vor dem Eingang, durch den Link hineinging ($010E)
        let indoor = if g.tracker.link.is_none() && alttp::ram8(&g.snes, 0x7E0010) == 0x07 {
            let eid = alttp::ram8(&g.snes, 0x7E010E);
            g.ents.iter().find(|e| e.eid == eid).map(|e| (e.world, e.x, e.y + 4.0))
        } else {
            None
        };
        if let Some((w, x, y)) = g.tracker.link.or(indoor) {
            if w as usize == world {
                self.link_dot(p, to_screen(x, y), gp, rad * 0.55);
            }
        }
        // Fadenkreuz: frei beweglich, rastet auf dem gewählten Symbol ein
        let lock = sel.map(|k| marks[k].0);
        let c = lock.unwrap_or(aim);
        let pulse = (t * 4.0).sin();
        if self.smooth_ui() {
            // HD: dieselben Ecken wie im Pixel-Stil, aber weich gezeichnet (runde Enden, dunkler Rand)
            let u = gp.max(2.0);
            let h = (if lock.is_some() { 7.0 } else { 5.0 } + pulse.max(0.0)) * u;
            let arm = 3.2 * u;
            let lw = u * 1.1;
            let dark = Color32::from_rgb(16, 16, 24);
            let seg = |a: gfx::Pos2, b: gfx::Pos2, w: f32, col: Color32| {
                p.line_segment([a, b], Stroke::new(w, col));
                p.circle_filled(a, w * 0.5, col);
                p.circle_filled(b, w * 0.5, col);
            };
            for pass in 0..2 {
                let (w, col) = if pass == 0 { (lw + u * 1.2, dark) } else { (lw, GOLD) };
                for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let corner = c + gfx::vec2(sx * h, sy * h);
                    seg(corner, corner + gfx::vec2(-sx * arm, 0.0), w, col);
                    seg(corner, corner + gfx::vec2(0.0, -sy * arm), w, col);
                }
            }
            if lock.is_none() {
                p.circle_filled(c, u * 1.1, dark);
                p.circle_filled(c, u * 0.55, Color32::WHITE);
            }
        } else if pix {
            // Pixel-Ecken (gold mit schwarzem Rand) im Stil der Spielmenüs
            let h = if lock.is_some() { 7.0 } else { 5.0 } + if pulse > 0.0 { 1.0 } else { 0.0 };
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let corner = c + gfx::vec2(sx * h * gp, sy * h * gp);
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
                p.rect_filled(gfx::Rect::from_center_size(c, gfx::vec2(gp * 3.0, gp * 3.0)), 0.0, Color32::from_rgb(16, 16, 24));
                p.rect_filled(gfx::Rect::from_center_size(c, gfx::vec2(gp, gp)), 0.0, Color32::WHITE);
            }
        } else {
            // HD: goldener Ring mit vier Strichen, leicht pulsierend
            let rr = if lock.is_some() { rad * 1.45 } else { rad * 1.1 } + pulse * 1.5;
            let sh = Stroke::new(5.0_f32, Color32::from_black_alpha(140));
            let st = Stroke::new(2.2_f32, GOLD);
            p.circle_stroke(c, rr, sh);
            p.circle_stroke(c, rr, st);
            let rot = if lock.is_some() { t * 1.5 } else { 0.0 };
            for k in 0..4 {
                let a = rot + std::f32::consts::FRAC_PI_2 * k as f32;
                let d = gfx::vec2(a.cos(), a.sin());
                let (a0, a1) = (c + d * (rr - 4.0), c + d * (rr + 7.0));
                p.line_segment([a0, a1], Stroke::new(5.0_f32, Color32::from_black_alpha(140)));
                p.line_segment([a0, a1], Stroke::new(2.4_f32, Color32::WHITE));
            }
            if lock.is_none() {
                p.circle_filled(c, 2.6, Color32::from_black_alpha(160));
                p.circle_filled(c, 1.6, Color32::WHITE);
            }
        }
        // Rahmen des Namensschilds über dem gewählten Ort (wird unten gezeichnet) – der Benötigt-Kasten weicht ihm aus
        let name_rect = sel.map(|k| {
            let top = marks[k].0 - gfx::vec2(0.0, rad * 2.2);
            let br = if pix {
                let w = crate::pixfont::width(&marks[k].4.to_uppercase(), gp);
                let br = gfx::Rect::from_center_size(top - gfx::vec2(0.0, gp * 5.5), gfx::vec2(w + gp * 6.0, gp * 11.0));
                br.translate(gfx::vec2((r.left() + 2.0 - br.left()).max(0.0) + (r.right() - 2.0 - br.right()).min(0.0), (r.top() + 2.0 - br.top()).max(0.0)))
            } else {
                let sz = gfx::vec2(self.text_w(p, &marks[k].4, fs * 0.75), fs * 0.75 * 1.25);
                let br = gfx::Rect::from_center_size(top - gfx::vec2(0.0, sz.y * 0.6), sz + gfx::vec2(18.0, 8.0));
                br.translate(gfx::vec2((r.left() + 4.0 - br.left()).max(0.0) + (r.right() - 4.0 - br.right()).min(0.0), (r.top() + 4.0 - br.top()).max(0.0)))
            };
            br
        });
        // Spoiler: benötigte Gegenstände für den gewählten Dungeon
        let mut req_info = None;
        if let Some(MapSel::Dungeon(di)) = sel.map(|k| marks[k].3) {
            if spoiler {
                let reqs = DUNGEON_REQS[di.min(12)];
                let have = reqs.iter().filter(|q| crate::mapdata::req_ok(&g.snes, q.0, q.1)).count();
                req_info = Some(format!("{} {have}/{}", tr(en, "Benötigt:", "Needs:"), reqs.len()));
                let k = sel.unwrap();
                let small = fs * 0.6;
                let lh = fs * 0.95;
                let title = tr(en, "BENÖTIGT", "NEEDED");
                let w = reqs.iter().map(|q| self.text_w(p, if en { q.3 } else { q.2 }, small)).fold(self.text_w(p, title, small), f32::max) + fs * 2.2;
                let h = lh * (reqs.len() as f32 + 1.0) + fs * 0.6;
                let anchor = marks[k].0 + gfx::vec2(rad * 1.6, rad * 1.2);
                let mut br = gfx::Rect::from_min_size(anchor, gfx::vec2(w, h));
                if br.right() > r.right() - 4.0 {
                    br = br.translate(gfx::vec2(-(w + rad * 4.4), 0.0));
                }
                let clamp_v = |b: gfx::Rect| b.translate(gfx::vec2(0.0, (r.top() + 4.0 - b.top()).max(0.0) + (r.bottom() - 4.0 - b.bottom()).min(0.0)));
                let mut br = clamp_v(br);
                // nicht unter dem Namensschild: darunter, sonst rechts daneben, sonst links daneben
                if let Some(nr) = name_rect.filter(|nr| nr.expand(4.0).intersects(br)) {
                    let gap = fs * 0.35;
                    let below = br.translate(gfx::vec2(0.0, nr.bottom() + gap - br.top()));
                    let right = br.translate(gfx::vec2(nr.right() + gap - br.left(), 0.0));
                    let left = br.translate(gfx::vec2(nr.left() - gap - br.right(), 0.0));
                    br = if below.bottom() <= r.bottom() - 4.0 {
                        below
                    } else if right.right() <= r.right() - 4.0 {
                        right
                    } else {
                        left
                    };
                }
                let lw = if pix { gp } else { 1.5 };
                p.rect_filled(br, if pix { 0.0 } else { 7.0 }, Color32::from_rgba_unmultiplied(12, 14, 36, 240));
                p.rect_stroke(br, if pix { 0.0 } else { 7.0 }, Stroke::new(lw, GOLD));
                self.mtext(p, gfx::pos2(br.left() + fs * 0.5, br.top() + fs * 0.3 + lh * 0.5), gfx::Align2::LEFT_CENTER, title, small, GOLD);
                for (j, q) in reqs.iter().enumerate() {
                    let y = br.top() + fs * 0.3 + lh * (j as f32 + 1.5);
                    let ok = crate::mapdata::req_ok(&g.snes, q.0, q.1);
                    self.tick(p, gfx::pos2(br.left() + fs * 0.75, y), fs * 0.3, ok);
                    self.mtext(p, gfx::pos2(br.left() + fs * 1.3, y), gfx::Align2::LEFT_CENTER, if en { q.3 } else { q.2 }, small, if ok { Color32::from_gray(160) } else { Color32::WHITE });
                }
            }
        }
        // Name über dem gewählten Ort (wie in der Karte im Spiel)
        if let Some(k) = sel {
            let txt = marks[k].4.clone();
            let top = marks[k].0 - gfx::vec2(0.0, rad * 2.2);
            if pix {
                let txt = txt.to_uppercase();
                let w = crate::pixfont::width(&txt, gp);
                let br = gfx::Rect::from_center_size(top - gfx::vec2(0.0, gp * 5.5), gfx::vec2(w + gp * 6.0, gp * 11.0));
                let br = br.translate(gfx::vec2((r.left() + 2.0 - br.left()).max(0.0) + (r.right() - 2.0 - br.right()).min(0.0), (r.top() + 2.0 - br.top()).max(0.0)));
                p.rect_filled(br, 0.0, Color32::from_rgba_unmultiplied(8, 10, 40, 235));
                p.rect_stroke(br, 0.0, Stroke::new(gp, Color32::WHITE));
                crate::pixfont::text(p, br.center(), gfx::Align2::CENTER_CENTER, &txt, gp, Color32::WHITE);
            } else {
                let gal = p.layout_no_wrap(txt, gfx::FontId::proportional(fs * 0.75), Color32::WHITE);
                let br = gfx::Rect::from_center_size(top - gfx::vec2(0.0, gal.size().y * 0.6), gal.size() + gfx::vec2(18.0, 8.0));
                let br = br.translate(gfx::vec2((r.left() + 4.0 - br.left()).max(0.0) + (r.right() - 4.0 - br.right()).min(0.0), (r.top() + 4.0 - br.top()).max(0.0)));
                p.rect_filled(br.translate(gfx::vec2(0.0, 2.0)), 7.0, Color32::from_black_alpha(110));
                p.rect_filled(br, 7.0, Color32::from_rgba_unmultiplied(12, 14, 36, 235));
                p.rect_stroke(br, 7.0, Stroke::new(1.5_f32, GOLD));
                p.galley(br.center() - gal.size() * 0.5, gal, Color32::WHITE);
            }
        }
        self.map_sel = sel.map(|k| marks[k].3);
        if let Some((w, x, y, sym)) = self.map_pick {
            if w as usize == world {
                self.draw_pin_picker(p, to_screen(x, y), r, fs, sym);
            }
            return (tr(en, "Eigene Markierung", "Own marker").to_string(), tr(en, "Symbol wählen", "Choose a symbol").to_string());
        }
        match sel {
            Some(k) => (marks[k].4.clone(), match req_info {
                Some(ri) => format!("{}  ·  {ri}", marks[k].5),
                None => marks[k].5.clone(),
            }),
            None => {
                // Gebiet unter dem Fadenkreuz
                let c = center + (aim - r.center()) / zoom;
                (region_name(world as u8, c.x, c.y, en), tr(en, "A: eigene Markierung setzen", "A: place own marker").to_string())
            }
        }
    }

    /// Dungeon-Karte; Rückgabe: (Auswahl-Text, Zusatz)
    fn draw_dungeon_overlay(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let d = self.map_dsel as usize;
        let Some(dm) = self.game.as_ref().and_then(|g| g.dmaps.get(d).cloned()) else {
            return (tr(en, "Keine Dungeon-Daten", "No dungeon data").into(), String::new());
        };
        let (here, cur_room) = {
            let g = self.game.as_ref().unwrap();
            (current_dungeon(&g.snes) == Some(d as u8), alttp::ram16(&g.snes, 0x7E00A0))
        };
        let link_floor = if here { dm.floors.iter().position(|f| f.contains(&(cur_room as u8))) } else { None };
        if self.map_floor_dungeon != Some(d as u8) {
            self.map_floor_dungeon = Some(d as u8);
            self.map_floor = link_floor.or_else(|| (0..dm.floors.len()).find(|&k| dm.floor_name(k) == "1F")).or(Some(0));
            let fl0 = self.map_floor.unwrap_or(0).min(dm.floors.len() - 1);
            let g = self.game.as_ref().unwrap();
            let want = if here { Some(cur_room as u8) } else { g.ents.iter().find(|e| dm.contains(e.room)).map(|e| e.room as u8) };
            let idx = want.and_then(|w| dm.floors[fl0].iter().position(|&r| r == w)).or_else(|| dm.floors[fl0].iter().position(|&r| r != 0x0F));
            if let Some(i) = idx {
                self.map_room_cur = ((i % 5) as i32, (i / 5) as i32);
            }
            self.map_dzoom = 1.0;
            self.map_dzoom_cur = 1.0;
            self.map_dview = gfx::vec2(2.5, 2.5);
        }
        let fl = self.map_floor.unwrap_or(0).min(dm.floors.len() - 1);
        let (cx, cy) = self.map_room_cur;
        let sel_room = dm.floors[fl].get((cy * 5 + cx) as usize).copied().filter(|&r| r != 0x0F).map(|r| r as u16);
        // Räume dieses Stockwerks in voller Größe im Hintergrund laden (der ausgewählte zuerst)
        {
            let mut want: Vec<u16> = sel_room.into_iter().collect();
            want.extend(dm.floors[fl].iter().filter(|&&rm| rm != 0x0F).map(|&rm| rm as u16).filter(|&rm| Some(rm) != sel_room));
            if let Some(g) = self.game.as_mut() {
                g.request_shots(&want);
            }
        }
        let g = self.game.as_ref().unwrap();
        let flags = |rm: u8| g.snes.bus.wram[0xF000 + rm as usize * 2] as u16 | (g.snes.bus.wram[0xF001 + rm as usize * 2] as u16) << 8;
        let chests: Vec<(u16, u8)> = g.chests.as_ref().map(|t| t.chests.iter().map(|c| (c.room, c.index_in_room)).collect()).unwrap_or_default();
        let boss = world::BOSS_ROOMS[world::DMAP_TO_DUNGEON[d]].filter(|b| dm.contains(*b));
        let spoiler = self.s.map_mode == 0;
        // wie im Spiel: mit der Karte des Dungeons alle Räume, mit dem Kompass Boss und Truhen
        let (has_map, has_compass) = self.game.as_ref().map_or((false, false), |g| {
            let bit = if d <= 1 { 0xC000u16 } else { 0x8000u16 >> d };
            (alttp::ram16(&g.snes, 0x7EF368) & bit != 0, alttp::ram16(&g.snes, 0x7EF364) & bit != 0)
        });
        // Raumbilder als Texturen (klein: beim Besuch aufgenommen; groß: unsichtbar geladen)
        let imgs: Vec<(u16, Vec<u16>)> = dm.floors[fl].iter().filter(|&&rm| rm != 0x0F).filter_map(|&rm| g.room_imgs.get(&(rm as u16)).map(|px| (rm as u16, px.clone()))).collect();
        let shots: Vec<(u16, std::sync::Arc<crate::teleport::RoomShot>)> = dm.floors[fl].iter().filter(|&&rm| rm != 0x0F).filter_map(|&rm| g.room_shots.get(&(rm as u16)).map(|s| (rm as u16, s.clone()))).collect();
        let ents: Vec<(u16, usize, bool)> = g.ents.iter().enumerate().filter(|(_, e)| dm.contains(e.room)).map(|(i, e)| (e.room, i, self.visited(e))).collect();
        let stairs = g.stairs.clone();
        let room_flags: Vec<u16> = (0..=255u8).map(flags).collect();
        let items: Vec<(u8, bool)> = g
            .chests
            .as_ref()
            .map(|t| t.chests.iter().filter(|c| dm.contains(c.room)).map(|c| (c.item, c.index_in_room < 6 && flags(c.room as u8) & (0x10 << c.index_in_room) != 0)).collect())
            .unwrap_or_default();
        let opts = if pix { gfx::TextureOptions::NEAREST } else { gfx::TextureOptions::LINEAR };
        for (rm, px) in imgs {
            if !self.room_tex.contains_key(&rm) {
                let pixels = px.iter().map(|&v| self.lut.get(v as usize & 0x7FFF).copied().unwrap_or(Color32::BLACK)).collect();
                self.room_tex.insert(rm, ctx.load_texture(format!("room{rm}"), gfx::ColorImage { size: [64, 64], pixels }, opts));
            }
        }
        for (rm, s) in &shots {
            if !self.room_hd_tex.contains_key(rm) && s.img.len() == 256 * 256 {
                let pixels = s.img.iter().map(|&v| self.lut.get(v as usize & 0x7FFF).copied().unwrap_or(Color32::BLACK)).collect();
                self.room_hd_tex.insert(*rm, ctx.load_texture(format!("roomhd{rm}"), gfx::ColorImage { size: [256, 256], pixels }, opts));
            }
        }
        let t = ctx.input(|i| i.time) as f32;
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);

        // ---- links: Name, Stockwerke, darunter die Gegenstände
        let col_w = (r.width() * 0.3).max(fs * 9.0);
        let (de, e) = world::DMAP_NAMES[d];
        self.mtext(p, gfx::pos2(r.left() + fs * 0.6, r.top() + fs * 0.5), gfx::Align2::LEFT_TOP, &(if en { e } else { de }).to_uppercase(), fs * 0.95, GOLD);
        let n = dm.floors.len();
        let fh = fs * 1.7;
        for (slot, k) in (0..n).rev().enumerate() {
            let y = r.top() + fs * 2.2 + slot as f32 * fh;
            let br = gfx::Rect::from_min_size(gfx::pos2(r.left() + fs * 0.6, y), gfx::vec2(col_w - fs * 1.2, fh - fs * 0.25));
            let sel = k == fl;
            p.rect_filled(br, if pix { 0.0 } else { 5.0 }, if sel { GOLD_DARK } else { Color32::from_rgb(28, 32, 64) });
            let mark = if link_floor == Some(k) { "▶ " } else { "  " };
            self.mtext(p, br.left_center() + gfx::vec2(fs * 0.4, 0.0), gfx::Align2::LEFT_CENTER, &format!("{mark}{}", dm.floor_name(k)), fs * 0.8, if sel { Color32::WHITE } else { Color32::from_gray(170) });
            if sel {
                // ↑/↓ wechselt das Stockwerk
                let s = fs * 0.28;
                let c = br.right_center() - gfx::vec2(fs * 0.6, 0.0);
                if k + 1 < n {
                    tri(p, c - gfx::vec2(s * 2.6, 0.0), s, true, Color32::WHITE);
                }
                if k > 0 {
                    tri(p, c, s, false, Color32::WHITE);
                }
            }
        }
        let top = r.top() + fs * 2.2 + n as f32 * fh + fs * 0.3;
        let bx = gfx::Rect::from_min_max(gfx::pos2(r.left() + fs * 0.6, top), gfx::pos2(r.left() + col_w - fs * 0.6, r.bottom() - fs * 0.5));
        if bx.height() > fs * 3.0 {
            p.rect_filled(bx, if pix { 0.0 } else { 6.0 }, Color32::from_rgb(16, 18, 42));
            let cp = p.with_clip_rect(bx.shrink(2.0));
            {
                // Dungeon-Fortschritt und alle Gegenstände darin
                let boss_done = boss.map(|b| room_flags[b as usize] & 0x0800 != 0);
                let total = items.len() + boss_done.is_some() as usize;
                let done = items.iter().filter(|x| x.1).count() + boss_done.unwrap_or(false) as usize;
                let pct = if total > 0 { done as f32 * 100.0 / total as f32 } else { 100.0 };
                let full = done >= total;
                self.mtext(p, bx.left_top() + gfx::vec2(fs * 0.4, fs * 0.35), gfx::Align2::LEFT_TOP, &format!("{done}/{total}"), fs * 0.75, Color32::from_gray(200));
                self.mtext(p, bx.right_top() + gfx::vec2(-fs * 0.4, fs * 0.25), gfx::Align2::RIGHT_TOP, &format!("{pct:.0} %"), fs * 0.95, if full { Color32::from_rgb(120, 230, 120) } else { GOLD });
                let small = fs * 0.55;
                let lh = fs * 0.9;
                let mut y = bx.top() + fs * 1.75;
                let mut rows: Vec<(String, bool)> = items.iter().map(|(it, d)| (if spoiler || *d { alttp::item_name(*it, en) } else { "???".into() }, *d)).collect();
                if let Some(bd) = boss_done {
                    rows.push((tr(en, "Herzcontainer (Boss)", "Heart Container (boss)").to_string(), bd));
                }
                // fehlende zuerst
                rows.sort_by_key(|x| x.1);
                for (name, d) in rows {
                    if y > bx.bottom() - lh * 0.4 {
                        break;
                    }
                    self.tick(&cp, gfx::pos2(bx.left() + fs * 0.7, y), fs * 0.3, d);
                    let name = self.fit_text(&cp, &name, small, bx.width() - fs * 1.6);
                    self.mtext(&cp, gfx::pos2(bx.left() + fs * 1.3, y), gfx::Align2::LEFT_CENTER, &name, small, if d { Color32::from_gray(140) } else { Color32::WHITE });
                    y += lh;
                }
            }
        }

        // ---- rechts: 5×5 Räume, zoombar (rechter Stick / Mausrad) um den gewählten Raum
        let grid = gfx::Rect::from_min_max(gfx::pos2(r.left() + col_w, r.top() + fs * 0.5), r.max - gfx::vec2(fs * 0.5, fs * 0.5));
        let k = 1.0 - (-dt * 12.0).exp();
        self.map_dzoom_cur += (self.map_dzoom - self.map_dzoom_cur) * k;
        if (self.map_dzoom_cur - self.map_dzoom).abs() < 0.002 {
            self.map_dzoom_cur = self.map_dzoom;
        }
        let z = self.map_dzoom_cur;
        let base = (grid.width() / 5.0).min(grid.height() / 5.0);
        let cell = base * z;
        let half = gfx::vec2(grid.width(), grid.height()) * 0.5 / cell;
        let want = gfx::vec2(cx as f32 + 0.5, cy as f32 + 0.5);
        let clampv = |v: f32, h: f32| if h >= 2.5 { 2.5 } else { v.clamp(h, 5.0 - h) };
        let target = gfx::vec2(clampv(want.x, half.x), clampv(want.y, half.y));
        self.map_dview += (target - self.map_dview) * k;
        let origin = grid.center() - self.map_dview * cell;
        self.map_dgeo = (origin, cell);
        let gp = p.with_clip_rect(grid.intersect(p.clip_rect()));
        let mut info = (format!("{} {}", tr(en, "Stockwerk", "Floor"), dm.floor_name(fl)), String::new());
        self.map_sel = None;
        let lw = if pix { self.game_px.round().max(1.0) } else { 2.0 };
        let round = if pix { 0.0 } else { 4.0 };
        // Symbole: Größe wächst mit dem Zoom, jedes auf dunklem Plättchen (auch auf hellen Räumen gut lesbar)
        let ic = (cell * 0.075).clamp(6.0, 20.0);
        let badge = |p: &gfx::Painter, c: gfx::Pos2, w: f32| {
            let br = gfx::Rect::from_center_size(c, gfx::vec2(w, ic * 2.5));
            p.rect_filled(br, if pix { 0.0 } else { ic * 0.6 }, Color32::from_black_alpha(185));
            p.rect_stroke(br, if pix { 0.0 } else { ic * 0.6 }, Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
        };
        for (i, &rm) in dm.floors[fl].iter().enumerate() {
            let (gx, gy) = ((i % 5) as i32, (i / 5) as i32);
            let cr = gfx::Rect::from_min_size(origin + gfx::vec2(gx as f32 * cell, gy as f32 * cell), gfx::vec2(cell, cell)).shrink(cell * 0.03);
            if rm == 0x0F || !cr.intersects(grid) {
                continue;
            }
            let is_cur = (gx, gy) == (cx, cy);
            let r16 = rm as u16;
            let f = room_flags[rm as usize];
            let visited = f & 0x0F != 0 || (here && cur_room == r16);
            let shown = spoiler || visited || has_map;
            // weit genug hereingezoomt, mit Kompass oder Spoiler: genaue Symbole im Raum statt der Eck-Symbole
            let shot_here = shots.iter().find(|(r, _)| *r == r16).map(|(_, s)| s.clone()).filter(|s| s.attr.len() >= 4096);
            let precise = shown && (spoiler || has_compass) && cell > 170.0 && shot_here.is_some();
            if !shown {
                gp.rect_filled(cr, round, Color32::from_rgb(16, 18, 36));
            } else {
                gp.rect_filled(cr, round, if visited { Color32::from_rgb(50, 78, 150) } else { Color32::from_rgb(30, 34, 56) });
                let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
                // groß: unsichtbar geladener Raum in voller Größe, sonst das beim Besuch aufgenommene Bild
                if let Some(tx) = self.room_hd_tex.get(&r16).or_else(|| self.room_tex.get(&r16)) {
                    gp.image(tx.id(), cr.shrink(lw), uv, if visited { Color32::WHITE } else { Color32::from_gray(150) });
                }
            }
            gp.rect_stroke(cr, round, Stroke::new(lw, if here && cur_room == r16 { Color32::from_rgb(120, 230, 120) } else if visited { Color32::from_rgb(190, 205, 240) } else { Color32::from_gray(70) }));
            let ent = ents.iter().find(|x| x.0 == r16).copied();
            // obere Reihe: Eingang links, Boss rechts
            let top_y = cr.top() + ic * 1.6;
            if let Some((_, _, vis)) = ent {
                if spoiler || vis {
                    let c = gfx::pos2(cr.left() + ic * 1.7, top_y);
                    badge(&gp, c, ic * 2.5);
                    paint_icon(&gp, c, ic, Icon::Door, if vis { DOOR } else { GREYC });
                }
            }
            let fd = f & 0x0800 != 0;
            if boss == Some(r16) && (spoiler || fd || has_compass) {
                let c = gfx::pos2(cr.right() - ic * 1.7, top_y);
                badge(&gp, c, ic * 2.5);
                paint_icon(&gp, c, ic, Icon::Container, if fd { HEART } else { GREYC });
            }
            // untere Reihe: Truhen (Anzahl geöffnet/gesamt) links, Treppen rechts
            let bot_y = cr.bottom() - ic * 1.6;
            let mine_idx: Vec<(u8, bool)> = chests.iter().filter(|(rr, _)| *rr == r16).map(|(_, x)| (*x, *x < 6 && f & (0x10 << x) != 0)).collect();
            let mine: Vec<bool> = mine_idx.iter().map(|(_, o)| *o).collect();
            let opened = mine.iter().filter(|o| **o).count();
            // Truhen immer zeigen (auch ohne Kompass und in noch nicht erledigten Räumen). In der genauen Ansicht
            // stehen sie an ihrer Stelle; Truhen, die dort fehlen (z. B. erscheinen erst nach den Gegnern), kommen
            // als Zähler unten links dazu
            let placed: u32 = match (precise, &shot_here) {
                (true, Some(sh)) => attr_parts(&sh.attr, |a| (0x58..=0x5D).contains(&a)).iter().fold(0, |m, p| m | 1 << (p.3 - 0x58)),
                _ => 0,
            };
            let rest: Vec<bool> = mine_idx.iter().filter(|(x, _)| !(*x < 6 && placed & (1 << x) != 0)).map(|(_, o)| *o).collect();
            let rest_open = rest.iter().filter(|o| **o).count();
            if !rest.is_empty() {
                let (opened, mine) = (rest_open, &rest);
                let txt = if mine.len() > 1 { format!("{opened}/{}", mine.len()) } else { String::new() };
                let tw = if txt.is_empty() { 0.0 } else { self.text_w(&gp, &txt, ic * 1.4) + ic * 0.4 };
                let c = gfx::pos2(cr.left() + ic * 1.7 + tw * 0.5, bot_y);
                badge(&gp, c, ic * 2.5 + tw);
                let all = opened == mine.len();
                // wie auf der Weltkarte: gefunden = farbig, fehlt noch = grau
                paint_icon(&gp, gfx::pos2(cr.left() + ic * 1.7, bot_y), ic, Icon::Chest, if all { CHEST } else { GREYC });
                if !txt.is_empty() {
                    self.mtext(&gp, gfx::pos2(cr.left() + ic * 3.0, bot_y), gfx::Align2::LEFT_CENTER, &txt, ic * 1.4, if all { Color32::from_gray(170) } else { Color32::WHITE });
                }
            }
            let mut sx = cr.right() - ic * 1.7;
            for (a, b) in stairs.iter().filter(|(a, b)| (*a == r16 || *b == r16) && !precise) {
                let other = if *a == r16 { *b } else { *a };
                let up = dm.floors.iter().position(|f| f.contains(&(other as u8))).map_or(false, |o| o > fl);
                let c = gfx::pos2(sx - ic * 0.7, bot_y);
                badge(&gp, c, ic * 3.9);
                paint_icon(&gp, gfx::pos2(sx, bot_y), ic, Icon::Stairs, Color32::WHITE);
                tri(&gp, gfx::pos2(sx - ic * 1.9, bot_y), ic * 0.55, up, if up { Color32::from_rgb(140, 230, 255) } else { Color32::from_rgb(255, 190, 120) });
                sx -= ic * 4.2;
                if sx < cr.center().x {
                    break;
                }
            }
            if let (true, Some(sh)) = (precise, &shot_here) {
                // genaue Positionen aus den Kachel-Daten des Raums: Fackeln, Treppen, Truhen (offen grau, große Truhe größer)
                let inner = cr.shrink(lw);
                let at = |x: f32, y: f32| inner.min + gfx::vec2(x / 512.0 * inner.width(), y / 512.0 * inner.height());
                let dot = |c: gfx::Pos2, r: f32| {
                    gp.circle_filled(c, r, Color32::from_black_alpha(190));
                    gp.circle_stroke(c, r, Stroke::new(1.0_f32, Color32::from_white_alpha(60)));
                };
                for (x, y, _, _) in attr_parts(&sh.attr, |a| (0xC0..=0xCF).contains(&a)) {
                    let c = at(x, y);
                    dot(c, ic * 0.75);
                    gp.circle_filled(c, ic * 0.42, Color32::from_rgb(255, 150, 40));
                    gp.circle_filled(c - gfx::vec2(0.0, ic * 0.12), ic * 0.2, Color32::from_rgb(255, 230, 120));
                }
                // Stern-Fliesen (öffnen/schließen Löcher): eigener Stern, nicht das Treppen-Symbol
                for (x, y, _, _) in attr_parts(&sh.attr, |a| a == 0x3A || a == 0x3B) {
                    let c = at(x, y);
                    dot(c, ic * 0.95);
                    paint_icon(&gp, c, ic * 0.75, Icon::Star, Color32::from_rgb(255, 170, 50));
                }
                // verschlossene Türen: Schlüsseltür / Treppe mit Schloss (Gold), Tür für den Großen Schlüssel (Rot, größer);
                // aufgeschlossen grau (Bit 0x8000 >> Nummer im Spielstand)
                for &(i, ty, pos, dir) in &sh.doors {
                    let big = ty == 0x1E;
                    if !(big || matches!(ty, 0x1C | 0x20 | 0x22 | 0x24 | 0x26)) {
                        continue;
                    }
                    let (tx, ty_) = ((pos % 64) as f32 * 8.0, (pos / 64) as f32 * 8.0);
                    let (x, y) = if dir >= 2 { (tx + 12.0, ty_ + 16.0) } else { (tx + 16.0, ty_ + 12.0) };
                    // eine Tür gehört zu zwei Räumen; das Spiel merkt sich „aufgeschlossen“ je Raum. Auch die passende Tür im
                    // Nachbarraum prüfen (gleiche Stelle, Gegenrichtung) – einmal durchgehen hakt so beide Seiten ab
                    let mut open = i < 4 && f & (0x8000 >> i) != 0;
                    if !open && dir < 4 {
                        let nb = match dir {
                            0 => r16.wrapping_sub(16),
                            1 => r16 + 16,
                            2 => r16.wrapping_sub(1),
                            _ => r16 + 1,
                        };
                        let opp = dir ^ 1;
                        if let (Some(nf), Some((_, ns))) = (room_flags.get(nb as usize), shots.iter().find(|(r, _)| *r == nb)) {
                            open = ns.doors.iter().any(|&(j, t2, p2, d2)| {
                                let same_line = if dir < 2 { p2 % 64 == pos % 64 } else { p2 / 64 == pos / 64 };
                                j < 4 && d2 == opp && t2 == ty && same_line && nf & (0x8000 >> j) != 0
                            });
                        }
                    }
                    let col = if open { GREYC } else if big { Color32::from_rgb(235, 70, 60) } else { Color32::from_rgb(250, 200, 60) };
                    let sz = if big { ic * 1.25 } else { ic };
                    let c = at(x, y);
                    dot(c, sz * 1.2);
                    lock_icon(&gp, c, sz, col);
                }
                for (x, y, n, _) in attr_parts(&sh.attr, |a| (0x1D..=0x1F).contains(&a) || ((0x30..=0x3F).contains(&a) && a != 0x3A && a != 0x3B)) {
                    if n < 2 {
                        continue;
                    }
                    let c = at(x, y);
                    dot(c, ic * 1.15);
                    paint_icon(&gp, c, ic * 0.9, Icon::Stairs, Color32::WHITE);
                }
                for (x, y, n, a) in attr_parts(&sh.attr, |a| (0x58..=0x5D).contains(&a)) {
                    let idx = (a - 0x58) as u16;
                    let open = f & (0x10 << idx) != 0;
                    let big = n > 4;
                    let c = at(x, y);
                    let sz = if big { ic * 1.35 } else { ic };
                    dot(c, sz * 1.25);
                    paint_icon(&gp, c, sz, Icon::Chest, if open { CHEST } else { GREYC });
                }
            }
            if here && cur_room == r16 {
                // genau dort, wo Link im Raum steht (Raum = 512×512, Mitte seines Körpers)
                let rr = (cell * 0.05).clamp(6.0, 14.0);
                let lp = self.game.as_ref().map_or(cr.center(), |g| {
                    let inner = cr.shrink(lw);
                    let fx = ((alttp::ram16(&g.snes, 0x7E0022) & 0x1FF) as f32 + 8.0) / 512.0;
                    let fy = ((alttp::ram16(&g.snes, 0x7E0020) & 0x1FF) as f32 + 14.0) / 512.0;
                    inner.min + gfx::vec2(fx.clamp(0.0, 1.0) * inner.width(), fy.clamp(0.0, 1.0) * inner.height())
                });
                self.link_dot(&gp, lp, (rr / 3.5).round().max(1.0), rr * 0.6);
            }
            if is_cur {
                let mut a = format!("{} {:02X}", tr(en, "Raum", "Room"), rm);
                if !mine.is_empty() {
                    a.push_str(&format!("  ·  {} {}/{}", tr(en, "Truhen", "chests"), opened, mine.len()));
                }
                if boss == Some(r16) {
                    a.push_str(tr(en, "  ·  Boss", "  ·  boss"));
                }
                let mut b = if visited { String::new() } else { tr(en, "noch nicht betreten", "not visited yet").to_string() };
                if let Some((_, ei, vis)) = ent {
                    if vis {
                        b = tr(en, "Eingang – A: Teleport", "entrance – A: teleport").to_string();
                        self.map_sel = Some(MapSel::Door(ei));
                    }
                }
                info = (a, b);
            }
        }
        let cr = gfx::Rect::from_min_size(origin + gfx::vec2(cx as f32 * cell, cy as f32 * cell), gfx::vec2(cell, cell)).shrink(cell * 0.03);
        // Auswahlrahmen zuletzt, damit ihn keine Nachbarzelle verdeckt
        let g2 = cr.expand(lw * (2.0 + (t * 5.0).sin()));
        gp.rect_stroke(g2, if pix { 0.0 } else { 6.0 }, Stroke::new(lw * 1.5, GOLD));
        // Zoomstufe (unten rechts)
        if self.map_dzoom > 1.05 {
            let zt = format!("{:.1}×", z);
            self.mtext(&gp, grid.right_bottom() - gfx::vec2(fs * 0.3, fs * 0.2), gfx::Align2::RIGHT_BOTTOM, &zt, fs * 0.6, Color32::from_gray(200));
        }
        info
    }

}

/// Zusammenhängende Kachelgruppen einer Art (beide Ebenen): (Mitte x, Mitte y in 0..512, Kacheln, erste Eigenschaft)
fn attr_parts(attr: &[u8], want: impl Fn(u8) -> bool) -> Vec<(f32, f32, usize, u8)> {
    let mut seen = vec![false; attr.len()];
    let mut out = Vec::new();
    for layer in 0..attr.len() / 4096 {
        let base = layer * 4096;
        for start in 0..4096 {
            if seen[base + start] || !want(attr[base + start]) {
                continue;
            }
            let first = attr[base + start];
            let (mut sx, mut sy, mut n) = (0usize, 0usize, 0usize);
            let mut stack = vec![start];
            seen[base + start] = true;
            while let Some(k) = stack.pop() {
                let (x, y) = (k % 64, k / 64);
                sx += x;
                sy += y;
                n += 1;
                let mut push = |q: usize| {
                    // Truhen nur mit gleicher Eigenschaft verbinden (jede Truhe hat ihre eigene Nummer)
                    let same_kind = !(0x58..=0x5D).contains(&first) || attr[base + q] == first;
                    if !seen[base + q] && want(attr[base + q]) && same_kind {
                        seen[base + q] = true;
                        stack.push(q);
                    }
                };
                if x > 0 {
                    push(k - 1);
                }
                if x < 63 {
                    push(k + 1);
                }
                if y > 0 {
                    push(k - 64);
                }
                if y < 63 {
                    push(k + 64);
                }
            }
            out.push((sx as f32 / n as f32 * 8.0 + 4.0, sy as f32 / n as f32 * 8.0 + 4.0, n, first));
        }
    }
    out
}

/// Vorhängeschloss (verschlossene Tür): Bügel oben, Körper unten mit Schlüsselloch
fn lock_icon(p: &gfx::Painter, c: gfx::Pos2, s: f32, col: Color32) {
    let body = gfx::Rect::from_center_size(c + gfx::vec2(0.0, s * 0.25), gfx::vec2(s * 1.3, s * 0.95));
    p.circle_stroke(c - gfx::vec2(0.0, s * 0.25), s * 0.42, Stroke::new((s * 0.22).max(1.5), col));
    p.rect_filled(body, s * 0.15, col);
    p.circle_filled(body.center() - gfx::vec2(0.0, s * 0.08), s * 0.14, Color32::from_black_alpha(220));
    p.rect_filled(gfx::Rect::from_center_size(body.center() + gfx::vec2(0.0, s * 0.12), gfx::vec2(s * 0.12, s * 0.3)), 0.0, Color32::from_black_alpha(220));
}

/// Dreieck nach oben/unten (die Schrift hat keine Pfeile)
fn tri(p: &gfx::Painter, c: gfx::Pos2, s: f32, up: bool, col: Color32) {
    let d = if up { -1.0 } else { 1.0 };
    let pts = vec![gfx::pos2(c.x, c.y + d * s), gfx::pos2(c.x - s, c.y - d * s * 0.8), gfx::pos2(c.x + s, c.y - d * s * 0.8)];
    p.add(gfx::Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// Symbole für eigene Markierungen
pub const PIN_ICONS: [Icon; 5] = [Icon::Star, Icon::Quest, Icon::Question, Icon::Cross, Icon::Heart];

const LIST_CATS: [(&str, &str); 8] = [
    ("PRO REGION", "BY REGION"),
    ("HERZTEILE", "HEART PIECES"),
    ("HERZCONTAINER", "HEART CONTAINERS"),
    ("GEGENSTÄNDE", "ITEMS"),
    ("FLASCHEN", "BOTTLES"),
    ("UPGRADES", "UPGRADES"),
    ("ANHÄNGER & KRISTALLE", "PENDANTS & CRYSTALS"),
    ("TRUHEN", "CHESTS"),
];

/// Name des Gebiets an einer Weltposition (große Gebiete -> Hauptgebiet, unbenannt -> Weltname)
pub(crate) fn region_name(world: u8, x: f32, y: f32, en: bool) -> String {
    let a = ((y.clamp(0.0, 4095.0) as i32 / 512) * 8 + x.clamp(0.0, 4095.0) as i32 / 512) as u8 + if world == 1 { 0x40 } else { 0 };
    let a = crate::teleport::LARGE.iter().copied().find(|&q| [q, q + 1, q + 8, q + 9].contains(&a)).unwrap_or(a);
    let n = world::area_name(a, en);
    if n.ends_with(&format!("{a:02X}")) {
        (if world == 1 { tr(en, "Schattenwelt", "Dark World") } else { tr(en, "Lichtwelt", "Light World") }).to_string()
    } else {
        n
    }
}

/// Eintrag der 100-%-Liste: (Name, Region/Zusatz, erledigt)
type Entry = (String, String, bool);

impl App {
    /// Richtungen mit Wiederholung beim Halten (Steuerkreuz und linker Stick)
    pub(crate) fn rep_dir(&mut self, pad: u16, dt: f32) -> (i32, i32) {
        let ls = self.lstick;
        let mut d = (0i32, 0i32);
        if pad & (1 << 9) != 0 || ls.x < -0.5 {
            d.0 = -1;
        } else if pad & (1 << 8) != 0 || ls.x > 0.5 {
            d.0 = 1;
        }
        if pad & (1 << 11) != 0 || ls.y > 0.5 {
            d.1 = -1;
        } else if pad & (1 << 10) != 0 || ls.y < -0.5 {
            d.1 = 1;
        }
        let (prev, t) = self.map_rep;
        if d == (0, 0) {
            self.map_rep = ((0, 0), 0.0);
            return (0, 0);
        }
        if d != prev {
            self.map_rep = (d, -0.28);
            return d;
        }
        let t = t + dt;
        if t >= 0.11 {
            self.map_rep = (d, 0.0);
            d
        } else {
            self.map_rep = (d, t);
            (0, 0)
        }
    }

    /// Alle Einträge einer Kategorie der 100-%-Liste
    fn list_entries(&self, cat: usize) -> Vec<Entry> {
        let en = self.s.ui_en;
        let Some(g) = &self.game else { return Vec::new() };
        let s = &g.snes;
        let loc_region = |l: &world::Location| match l.dungeon {
            Some(d) => (if en { world::DMAP_NAMES[d as usize].1 } else { world::DMAP_NAMES[d as usize].0 }).to_string(),
            None => region_name(l.world, l.x, l.y, en),
        };
        let locs = |f: &dyn Fn(LocKind) -> bool| -> Vec<Entry> {
            g.locations
                .iter()
                .enumerate()
                .filter(|(_, l)| f(l.kind))
                .map(|(i, l)| {
                    let (d, n) = (l.done(s, g.manual.contains(&i)), l.total());
                    let sub = if n > 1 { format!("{}  {d}/{n}", loc_region(l)) } else { loc_region(l) };
                    (l.name(en).to_string(), sub, d >= n)
                })
                .collect()
        };
        let poi = |p: &Poi| {
            let (w, x, y) = p.pos();
            let (a, b) = (p.prog)(s);
            let reg = region_name(w, x, y, en);
            (if en { p.en } else { p.de }.to_string(), if b > 1 { format!("{reg}  {a}/{b}") } else { reg }, a >= b)
        };
        match cat {
            0 => {
                // fehlende Dinge je Region
                let mut all: Vec<(String, String, bool)> = Vec::new();
                for (i, l) in g.locations.iter().enumerate() {
                    let (d, n) = (l.done(s, g.manual.contains(&i)), l.total());
                    all.push((loc_region(l), l.name(en).to_string(), d >= n));
                }
                for (k, p) in BOTTLES.iter().chain(UPGRADES.iter()).enumerate() {
                    if k >= BOTTLES.len() && OPTIONAL_UPGRADES.contains(&(k - BOTTLES.len())) {
                        continue;
                    }
                    let (n, reg, done) = poi(p);
                    let reg = reg.split("  ").next().unwrap_or("").to_string();
                    all.push((reg, n, done));
                }
                for (de, e, di, f) in DUNGEON_UPGRADES.iter() {
                    all.push(((if en { DUNGEONS[*di].4 } else { DUNGEONS[*di].3 }).to_string(), (if en { *e } else { *de }).to_string(), f(s)));
                }
                let mut regions: Vec<String> = Vec::new();
                for a in &all {
                    if !regions.contains(&a.0) {
                        regions.push(a.0.clone());
                    }
                }
                // Regionen mit fehlenden Dingen zuerst
                regions.sort_by_key(|r| (all.iter().filter(|a| &a.0 == r && !a.2).count() == 0, r.clone()));
                let mut out = Vec::new();
                for r in regions {
                    let mine: Vec<&(String, String, bool)> = all.iter().filter(|a| a.0 == r).collect();
                    let done = mine.iter().filter(|a| a.2).count();
                    out.push((r.to_uppercase(), format!("{done}/{}", mine.len()), done == mine.len()));
                    for m in mine.iter().filter(|a| !a.2) {
                        out.push((format!("   {}", m.1), String::new(), false));
                    }
                }
                out
            }
            1 => locs(&|k| k == LocKind::HeartPiece || k == LocKind::Minigame),
            2 => locs(&|k| k == LocKind::HeartContainer),
            3 => {
                let p = crate::tracker::progress(s);
                alttp::INVENTORY.iter().zip(p.items_owned.iter()).map(|((_, de, e), o)| ((if en { *e } else { *de }).to_string(), String::new(), *o)).collect()
            }
            4 => {
                let n = crate::mapdata::bottles(s);
                let mut v = vec![(tr(en, "Flaschen insgesamt", "Bottles in total").to_string(), format!("{n}/4"), n >= 4)];
                v.extend(BOTTLES.iter().map(poi));
                v
            }
            5 => {
                let opt = tr(en, "freiwillig", "optional");
                let mut v: Vec<Entry> = UPGRADES
                    .iter()
                    .enumerate()
                    .map(|(i, u)| {
                        let mut e = poi(u);
                        if OPTIONAL_UPGRADES.contains(&i) {
                            e.1 = format!("{opt} · {}", e.1);
                        }
                        e
                    })
                    .collect();
                for (de, e, di, f) in DUNGEON_UPGRADES.iter() {
                    v.push(((if en { *e } else { *de }).to_string(), (if en { DUNGEONS[*di].4 } else { DUNGEONS[*di].3 }).to_string(), f(s)));
                }
                v
            }
            6 => {
                let pb = alttp::ram8(s, 0x7EF374);
                let cb = alttp::ram8(s, 0x7EF37A);
                let mut v: Vec<Entry> = [(0x04u8, "Anhänger des Mutes", "Pendant of Courage", 1usize), (0x02, "Anhänger der Kraft", "Pendant of Power", 2), (0x01, "Anhänger der Weisheit", "Pendant of Wisdom", 3)]
                    .iter()
                    .map(|(b, de, e, di)| ((if en { *e } else { *de }).to_string(), (if en { DUNGEONS[*di].4 } else { DUNGEONS[*di].3 }).to_string(), pb & b != 0))
                    .collect();
                for (k, (di, b)) in CRYSTAL_BITS.iter().enumerate() {
                    v.push((format!("{} {}", tr(en, "Kristall", "Crystal"), k + 1), (if en { DUNGEONS[*di].4 } else { DUNGEONS[*di].3 }).to_string(), cb & b != 0));
                }
                v
            }
            _ => locs(&|k| k == LocKind::Chests),
        }
    }

    /// Kleine Häkchen-/Fehlt-Markierung
    pub(crate) fn tick(&self, p: &gfx::Painter, c: gfx::Pos2, r: f32, done: bool) {
        if done {
            paint_icon(p, c, r, Icon::Check, Color32::WHITE);
        } else if self.pixel_ui() {
            let gp = self.game_px.round().max(1.0);
            p.rect_stroke(gfx::Rect::from_center_size(c, gfx::vec2(r * 1.4, r * 1.4)), 0.0, Stroke::new(gp, Color32::from_gray(110)));
        } else {
            p.circle_stroke(c, r * 0.7, Stroke::new(1.5_f32, Color32::from_gray(120)));
        }
    }

    /// Reiter „100 %“: links Kategorien, rechts die Einträge mit Häkchen
    fn draw_list_page(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let cats: Vec<Vec<Entry>> = (0..LIST_CATS.len()).map(|c| self.list_entries(c)).collect();
        let cat = self.map_list_cat.min(LIST_CATS.len() - 1);
        let col_w = (r.width() * 0.34).max(fs * 9.0);
        let rh = fs * 1.45;
        let small = fs * 0.62;
        for (k, (de, e)) in LIST_CATS.iter().enumerate() {
            let br = gfx::Rect::from_min_size(gfx::pos2(r.left() + fs * 0.5, r.top() + fs * 0.5 + k as f32 * rh), gfx::vec2(col_w - fs, rh - fs * 0.2));
            let sel = k == cat;
            p.rect_filled(br, if pix { 0.0 } else { 5.0 }, if sel { GOLD_DARK } else { Color32::from_rgb(28, 32, 64) });
            // Kategorie „pro Region“ zählt Regionen, sonst Einträge
            let list: Vec<&Entry> = if k == 0 { cats[0].iter().filter(|x| !x.0.starts_with(' ')).collect() } else { cats[k].iter().filter(|x| !x.1.starts_with(tr(en, "freiwillig", "optional"))).collect() };
            let done = list.iter().filter(|x| x.2).count();
            let full = done == list.len();
            let cnt = format!("{done}/{}", list.len());
            let label = self.fit_text(p, if en { e } else { de }, small, br.width() - fs * 1.2 - self.text_w(p, &cnt, small));
            self.mtext(p, br.left_center() + gfx::vec2(fs * 0.4, 0.0), gfx::Align2::LEFT_CENTER, &label, small, if full { Color32::from_rgb(120, 230, 120) } else if sel { Color32::WHITE } else { Color32::from_gray(185) });
            self.mtext(p, br.right_center() - gfx::vec2(fs * 0.4, 0.0), gfx::Align2::RIGHT_CENTER, &format!("{done}/{}", list.len()), small, if full { Color32::from_rgb(120, 230, 120) } else { GOLD });
        }
        // rechte Seite
        let right = gfx::Rect::from_min_max(gfx::pos2(r.left() + col_w + fs * 0.2, r.top() + fs * 0.5), r.max - gfx::vec2(fs * 0.5, fs * 0.5));
        p.rect_filled(right, if pix { 0.0 } else { 6.0 }, Color32::from_rgb(16, 18, 42));
        let rows = &cats[cat];
        let lh = fs * 1.05;
        let vis = ((right.height() - fs * 0.4) / lh).floor().max(1.0) as usize;
        // Mausrad
        let (scroll, hover) = ctx.input(|i| (i.smooth_scroll_delta.y, i.pointer.hover_pos()));
        if hover.map_or(false, |h| right.contains(h)) && scroll.abs() > 0.1 {
            self.map_list_scroll -= scroll / lh;
        }
        let max = rows.len().saturating_sub(vis) as f32;
        self.map_list_scroll = self.map_list_scroll.clamp(0.0, max);
        let first = self.map_list_scroll.round() as usize;
        let cp = p.with_clip_rect(right);
        for (k, (name, sub, done)) in rows.iter().enumerate().skip(first).take(vis + 1) {
            let y = right.top() + fs * 0.2 + (k - first) as f32 * lh + lh * 0.5;
            let header = cat == 0 && !name.starts_with(' ');
            let x0 = right.left() + fs * 0.5;
            if header {
                cp.line_segment([gfx::pos2(x0, y + lh * 0.48), gfx::pos2(right.right() - fs * 0.5, y + lh * 0.48)], Stroke::new(1.0_f32, Color32::from_gray(60)));
            }
            if !(cat == 0 && !header) {
                self.tick(&cp, gfx::pos2(x0 + fs * 0.35, y), fs * 0.38, *done);
            }
            let col = if header {
                if *done { Color32::from_rgb(120, 230, 120) } else { GOLD }
            } else if *done {
                Color32::from_gray(150)
            } else {
                Color32::WHITE
            };
            let sub_w = if sub.is_empty() { 0.0 } else { self.text_w(&cp, sub, small * 0.85) + fs * 0.8 };
            let name = self.fit_text(&cp, name, small, right.right() - fs * 0.5 - sub_w - (x0 + fs * 1.0));
            self.mtext(&cp, gfx::pos2(x0 + fs * 1.0, y), gfx::Align2::LEFT_CENTER, &name, small, col);
            if !sub.is_empty() {
                self.mtext(&cp, gfx::pos2(right.right() - fs * 0.5, y), gfx::Align2::RIGHT_CENTER, sub, small * 0.85, Color32::from_gray(150));
            }
        }
        if rows.len() > vis {
            // Scrollbalken
            let h = right.height() * vis as f32 / rows.len() as f32;
            let t = (right.height() - h) * if max > 0.0 { self.map_list_scroll / max } else { 0.0 };
            p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(right.right() - fs * 0.25, right.top() + t), gfx::vec2(fs * 0.15, h)), 2.0, GOLD_DARK);
        }
        let list: Vec<&Entry> = if cat == 0 { rows.iter().filter(|x| !x.0.starts_with(' ')).collect() } else { rows.iter().filter(|x| !x.1.starts_with(tr(en, "freiwillig", "optional"))).collect() };
        let done = list.iter().filter(|x| x.2).count();
        ((if en { LIST_CATS[cat].1 } else { LIST_CATS[cat].0 }).to_string(), format!("{done}/{} {}", list.len(), tr(en, "erledigt", "done")))
    }

    /// Sichtbare Quests: erst die jetzt möglichen (Hauptquest zuerst), dann die erledigten
    pub(crate) fn quest_visible(&self) -> Vec<usize> {
        let Some(g) = &self.game else { return Vec::new() };
        let s = &g.snes;
        let mut act: Vec<usize> = (0..QUESTS.len()).filter(|&i| QUESTS[i].state(s) == QState::Active).collect();
        act.sort_by_key(|&i| !QUESTS[i].main);
        let done: Vec<usize> = (0..QUESTS.len()).filter(|&i| QUESTS[i].state(s) == QState::Done).collect();
        act.extend(done);
        act
    }

    /// Reiter „Quests“: nur Quests, die man jetzt machen kann, und erledigte
    fn draw_quest_page(&mut self, ctx: &gfx::Context, p: &gfx::Painter, r: gfx::Rect, fs: f32) -> (String, String) {
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let vis = self.quest_visible();
        let Some(g) = &self.game else { return (String::new(), String::new()) };
        let s = &g.snes;
        if vis.is_empty() {
            return (tr(en, "Keine Quests", "No quests").into(), String::new());
        }
        let sel = self.map_quest_sel.min(vis.len() - 1);
        // oben ein fester Streifen für den Zähler, die Liste läuft darunter
        let band = fs * 1.9;
        let r_all = r;
        let r = gfx::Rect::from_min_max(gfx::pos2(r.left(), r.top() + band), r.max);
        let first_done = vis.iter().position(|&i| QUESTS[i].state(s) == QState::Done).unwrap_or(vis.len());
        let lh = fs * 1.75;
        let head = fs * 1.5;
        let small = fs * 0.62;
        let ypos = |k: usize| head + k as f32 * lh + if k >= first_done { head } else { 0.0 };
        let total = ypos(vis.len() - 1) + lh;
        let view = r.height() - fs;
        let want = ypos(sel) + lh * 0.5 - view * 0.5;
        let off = want.clamp(0.0, (total - view).max(0.0));
        let cp = p.with_clip_rect(r.shrink(fs * 0.3));
        let x0 = r.left() + fs * 0.8;
        let t = ctx.input(|i| i.time) as f32;
        let mut heads = vec![(tr(en, "JETZT MÖGLICH", "AVAILABLE NOW"), 0.0)];
        if first_done < vis.len() {
            heads.push((tr(en, "ERLEDIGT", "DONE"), ypos(first_done) - head));
        }
        if first_done == 0 {
            heads.remove(0);
        }
        for (title, at) in heads {
            let y = r.top() + fs * 0.5 + at - off;
            self.mtext(&cp, gfx::pos2(x0, y + head * 0.45), gfx::Align2::LEFT_CENTER, title, fs * 0.8, GOLD);
        }
        // Zähler: erledigte Haupt- und Nebenquests
        {
            let cnt = |main: bool| {
                let all: Vec<&crate::mapdata::Quest> = QUESTS.iter().filter(|q| q.main == main).collect();
                (all.iter().filter(|q| (q.done)(s)).count(), all.len())
            };
            let (md, mt) = cnt(true);
            let (sd, st) = cnt(false);
            let txt = format!("{} {md}/{mt}    {} {sd}/{st}", tr(en, "HAUPTQUEST", "MAIN"), tr(en, "NEBENQUESTS", "SIDE"));
            let w = self.text_w(p, &txt, small) + fs * 1.2;
            let br = gfx::Rect::from_min_size(gfx::pos2(r_all.right() - w - fs * 0.5, r_all.top() + (band - fs * 1.3) * 0.5), gfx::vec2(w, fs * 1.3));
            p.rect_filled(br, 0.0, crate::inventory::MENU_BG);
            p.rect_stroke(br, 0.0, Stroke::new(if pix { self.game_px.round().max(1.0) } else { 1.5 }, crate::inventory::MENU_LINE));
            self.mtext(p, br.center(), gfx::Align2::CENTER_CENTER, &txt, small, crate::inventory::MENU_LINE);
        }
        let track = g.track;
        for (k, &qi) in vis.iter().enumerate() {
            let q = &QUESTS[qi];
            let y = r.top() + fs * 0.5 + ypos(k) - off;
            if y + lh < r.top() || y > r.bottom() {
                continue;
            }
            let row = gfx::Rect::from_min_size(gfx::pos2(x0 - fs * 0.3, y), gfx::vec2(r.width() - fs * 1.6, lh - fs * 0.15));
            if k == sel {
                cp.rect_filled(row, if pix { 0.0 } else { 6.0 }, Color32::from_rgb(40, 44, 84));
                cp.rect_stroke(row, if pix { 0.0 } else { 6.0 }, Stroke::new(if pix { self.game_px.round().max(1.0) } else { 1.5 }, GOLD));
            }
            let st = q.state(s);
            let ic = row.left_center() + gfx::vec2(fs * 0.7, 0.0);
            if st == QState::Done {
                paint_icon(&cp, ic, fs * 0.42, Icon::Check, Color32::WHITE);
            } else {
                let bob = if pix { 0.0 } else { (t * 4.0).sin() * 1.5 };
                paint_icon(&cp, ic + gfx::vec2(0.0, bob), fs * 0.42, Icon::Quest, Color32::WHITE);
            }
            let col = if st == QState::Done { Color32::from_gray(140) } else { Color32::WHITE };
            self.mtext(&cp, gfx::pos2(ic.x + fs * 0.9, row.top() + lh * 0.3), gfx::Align2::LEFT_CENTER, if en { q.en } else { q.de }, small, col);
            self.mtext(&cp, gfx::pos2(ic.x + fs * 0.9, row.top() + lh * 0.68), gfx::Align2::LEFT_CENTER, if en { q.hint_en } else { q.hint_de }, small * 0.8, Color32::from_gray(140));
            let (tag, tcol) = if st == QState::Done {
                (tr(en, "ERLEDIGT", "DONE"), Color32::from_rgb(120, 230, 120))
            } else if track == Some(qi) {
                (tr(en, "VERFOLGT", "TRACKED"), Color32::from_rgb(120, 200, 255))
            } else if q.main {
                (tr(en, "HAUPTQUEST", "MAIN QUEST"), GOLD)
            } else {
                (tr(en, "NEBENQUEST", "SIDE QUEST"), Color32::from_gray(190))
            };
            self.mtext(&cp, gfx::pos2(row.right() - fs * 0.5, row.center().y), gfx::Align2::RIGHT_CENTER, tag, small * 0.85, tcol);
        }
        let qi = vis[sel];
        let q = &QUESTS[qi];
        let st = if q.state(s) == QState::Done { tr(en, "erledigt", "done") } else { tr(en, "jetzt möglich", "possible now") };
        let mut extra = String::new();
        if self.quest_target(qi).is_some() {
            extra.push_str(tr(en, "  ·  A: auf der Karte", "  ·  A: show on map"));
        }
        if q.state(s) == QState::Active {
            extra.push_str(tr(en, "  ·  Y: verfolgen", "  ·  Y: track"));
        }
        ((if en { q.en } else { q.de }).to_string(), format!("{st}{extra}"))
    }

    /// Symbol-Auswahl für eine eigene Markierung über dem Fadenkreuz
    fn draw_pin_picker(&self, p: &gfx::Painter, at: gfx::Pos2, r: gfx::Rect, fs: f32, sel: u8) {
        let pix = self.pixel_ui();
        let cell = fs * 1.6;
        let w = cell * 5.0 + fs * 0.6;
        let br = gfx::Rect::from_center_size(at - gfx::vec2(0.0, cell * 1.2), gfx::vec2(w, cell + fs * 0.6));
        let br = br.translate(gfx::vec2((r.left() + 4.0 - br.left()).max(0.0) + (r.right() - 4.0 - br.right()).min(0.0), (r.top() + 4.0 - br.top()).max(0.0)));
        let lw = if pix { self.game_px.round().max(1.0) } else { 1.5 };
        p.rect_filled(br, if pix { 0.0 } else { 7.0 }, Color32::from_rgba_unmultiplied(12, 14, 36, 240));
        p.rect_stroke(br, if pix { 0.0 } else { 7.0 }, Stroke::new(lw, GOLD));
        for (k, ic) in PIN_ICONS.iter().enumerate() {
            let c = gfx::pos2(br.left() + fs * 0.3 + cell * (k as f32 + 0.5), br.center().y);
            if k as u8 == sel {
                p.rect_stroke(gfx::Rect::from_center_size(c, gfx::vec2(cell * 0.9, cell * 0.9)), if pix { 0.0 } else { 5.0 }, Stroke::new(lw * 1.5, Color32::WHITE));
            }
            paint_icon(p, c, fs * 0.45, *ic, Color32::WHITE);
        }
    }
}

/// Ziel der Hauptquests: Index in `tracker::DUNGEONS` (None = Sonderfall Master-Schwert)
const MAIN_TARGET: [Option<usize>; 14] = [Some(0), Some(1), Some(2), Some(3), None, Some(0), Some(4), Some(5), Some(6), Some(7), Some(8), Some(9), Some(10), Some(11)];

impl App {
    /// Wohin muss man für eine Quest? (Welt, x, y)
    pub(crate) fn quest_target(&self, i: usize) -> Option<(u8, f32, f32)> {
        let q = QUESTS.get(i)?;
        if let Some(p) = q.pos() {
            let p = crate::mapdata::npc_pos(&self.game.as_ref()?.npcs, crate::mapdata::QUEST_NPC.get(i).copied().unwrap_or(0), p);
            return Some(p);
        }
        if !q.main {
            return None;
        }
        match MAIN_TARGET.get(i).copied().flatten() {
            Some(d) => self.game.as_ref().and_then(|g| g.dungeons.get(d).copied()),
            // Master-Schwert: Sockel in den Verlorenen Wäldern
            None => Some((0, 190.0, 230.0)),
        }
    }

    /// Verfolgte Quest (oder sonst die aktuelle Hauptquest) mit Ziel
    pub(crate) fn quest_target_active(&self) -> Option<(usize, (u8, f32, f32))> {
        let g = self.game.as_ref()?;
        let s = &g.snes;
        let i = g.track.filter(|&i| QUESTS.get(i).map_or(false, |q| q.state(s) == QState::Active)).or_else(|| QUESTS.iter().position(|q| q.main && q.state(s) == QState::Active))?;
        Some((i, self.quest_target(i)?))
    }

    /// Ziel-Markierung: goldene Raute, die pulsiert; außerhalb des Ausschnitts ein Pfeil am Rand
    fn draw_goal(&self, p: &gfx::Painter, r: gfx::Rect, at: gfx::Pos2, rad: f32, gp: f32, t: f32) {
        let pix = self.pixel_ui();
        let inner = r.shrink(rad * 1.8);
        if inner.contains(at) {
            let s = rad * (1.3 + 0.18 * (t * 4.0).sin());
            let pts = |k: f32| vec![at + gfx::vec2(0.0, -k), at + gfx::vec2(k, 0.0), at + gfx::vec2(0.0, k), at + gfx::vec2(-k, 0.0)];
            p.add(gfx::Shape::convex_polygon(pts(s + if pix { gp } else { 2.0 }), Color32::from_rgb(16, 16, 24), Stroke::NONE));
            p.add(gfx::Shape::closed_line(pts(s), Stroke::new(if pix { gp * 1.5 } else { 3.0 }, GOLD)));
            let bob = if pix { ((t * 3.0) as i64 % 2) as f32 * gp } else { (t * 3.0).sin() * 2.0 };
            paint_icon(p, at - gfx::vec2(0.0, s + rad * 0.9 + bob), rad * 0.75, Icon::Quest, Color32::WHITE);
        } else {
            // Pfeil am Rand in Richtung Ziel
            let c = r.center();
            let d = (at - c).normalized();
            let kx = if d.x.abs() > 1e-3 { (inner.width() * 0.5) / d.x.abs() } else { f32::MAX };
            let ky = if d.y.abs() > 1e-3 { (inner.height() * 0.5) / d.y.abs() } else { f32::MAX };
            let e = c + d * kx.min(ky);
            let n = gfx::vec2(-d.y, d.x);
            let s = rad * (1.1 + 0.15 * (t * 4.0).sin());
            let tri = vec![e + d * s, e - d * s * 0.6 + n * s * 0.8, e - d * s * 0.6 - n * s * 0.8];
            p.add(gfx::Shape::convex_polygon(tri, GOLD, Stroke::new(if pix { gp } else { 2.0 }, Color32::from_rgb(16, 16, 24))));
        }
    }

    /// Was im X-Feld zu sehen ist (None = nichts zeichnen): (Symbol-Schlüssel, vorhanden, Helligkeit, x-Versatz)
    pub(crate) fn hud_key(&self) -> Option<(i32, bool, u8, usize)> {
        if !self.s.x_slot || self.inv_open || self.win_map || self.tp_anim.is_some() {
            return None;
        }
        let g = self.game.as_ref()?;
        let md = alttp::ram8(&g.snes, 0x7E0010);
        let sub = alttp::ram8(&g.snes, 0x7E0011);
        let dialog = md == 0x0E && sub == 0x02;
        // immer wenn die Spiel-Anzeige zu sehen ist – auch bei Szenenwechseln; nicht in Menüs
        let scene = matches!(md, 0x06..=0x0B | 0x0F..=0x11 | 0x13 | 0x15 | 0x16) || dialog;
        let bright = g.snes.bus.ppu.brightness();
        if !scene || g.snes.bus.ppu.r.tm & 0x04 == 0 || bright == 0 {
            return None;
        }
        let ox = if g.snes.bus.ppu.hud_split { 0 } else { self.view_w.saturating_sub(256) / 2 };
        let (key, have) = match g.x_item {
            Some(e) => (e.idx as i32 * 16 + e.bottle.map_or(0, |b| crate::quickmenu::bottle_content(&g.snes, b) as i32 + 1), quickmenu_has(&g.snes, e)),
            None => (-1, false),
        };
        Some((key, have, bright, ox))
    }

    /// Y-/X-Buchstaben und das X-Feld (Rahmen wie das Y-Feld, Item-Symbol) in `view` einzeichnen
    pub(crate) fn draw_hud_into_view(&mut self, key: (i32, bool, u8, usize)) {
        let (_, have, bright, ox) = key;
        let w = self.view_w;
        let h = self.view.len() / w.max(1);
        let to15 = |c: Color32| -> u16 {
            let f = |v: u8| ((v as u16 >> 3) * (bright as u16 + 1)) >> 4;
            f(c.r()) | (f(c.g()) << 5) | (f(c.b()) << 10)
        };
        let view = &mut self.view;
        let mut put = |x: i32, y: i32, c: u16| {
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                view[y as usize * w + x as usize] = c;
            }
        };
        let x0 = 37 + ox as i32;
        // X-Feld: 22×22-Muster wie im Spiel (Gold außen, Weiß innen, runde Ecken); mit Item schwarz hinterlegt
        let item = self.game.as_ref().and_then(|g| g.x_item.map(|e| (e, self.qm.icon_px(&g.snes, e).cloned())));
        let filled = have;
        for row in 0..22i32 {
            for c in 0..22i32 {
                let a = row.min(21 - row);
                let b = c.min(21 - c);
                let col = match (a.min(b), a.max(b)) {
                    (0, 0) => None,
                    (0, _) | (1, 1) => Some(crate::inventory::GOLDF),
                    (1, _) | (2, 2) => Some(crate::inventory::MENU_LINE),
                    _ => filled.then_some(Color32::BLACK),
                };
                if let Some(k) = col {
                    put(x0 + c, 46 + row, to15(k));
                }
            }
        }
        // Item-Symbol (20×20, mittig)
        if let Some((_, Some(px))) = item {
            let n = crate::quickmenu::ICON as i32;
            for y in 0..n {
                for x in 0..n {
                    let c = px[(y * n + x) as usize];
                    if c.a() < 128 {
                        continue;
                    }
                    let c = if have { c } else { let l = ((c.r() as u32 + c.g() as u32 + c.b() as u32) / 3 * 45 / 100) as u8; Color32::from_rgb(l, l, l) };
                    put(x0 + 1 + x, 47 + y, to15(c));
                }
            }
        }
        // Buchstaben Y (über dem X-Feld, am Y-Feld) und X: Pixel-Schrift mit dunklem Rand
        let dark = to15(Color32::from_rgb(16, 16, 24));
        for (ch, cx, cy, col) in [('Y', x0 + 21, 40, crate::inventory::GREEN), ('X', x0 + 21, 66, crate::inventory::YELLOW_X)] {
            let bits = crate::pixfont::glyph_bits(ch);
            let (gx, gy) = (cx - 2, cy - 3);
            let on = |x: i32, y: i32| (0..5).contains(&x) && (0..7).contains(&y) && bits[y as usize] & (16 >> x) != 0;
            for y in -1..8 {
                for x in -1..6 {
                    if !on(x, y) && (-1..=1).any(|dy| (-1..=1).any(|dx| on(x + dx, y + dy))) {
                        put(gx + x, gy + y, dark);
                    }
                }
            }
            for y in 0..7 {
                for x in 0..5 {
                    if on(x, y) {
                        put(gx + x, gy + y, to15(col));
                    }
                }
            }
        }
    }
}

impl App {
    /// Wenig Energie: blinkendes Herz unten rechts und leichtes rotes Pulsieren am Rand (statt Warnton)
    pub(crate) fn low_health_hud(&self, p: &gfx::Painter, r: gfx::Rect, ctx: &gfx::Context) {
        if !self.s.health_blink || self.inv_open || self.win_map {
            return;
        }
        let Some(g) = &self.game else { return };
        let md = alttp::ram8(&g.snes, 0x7E0010);
        if !(md == 0x07 || md == 0x09) || g.snes.bus.ppu.brightness() < 15 || self.tp_anim.is_some() {
            return;
        }
        let cur = alttp::ram8(&g.snes, 0x7EF36D) as u32;
        let cap = alttp::ram8(&g.snes, 0x7EF36C) as u32;
        // ein Viertel bzw. drei Herzen – aber höchstens die Hälfte der Herzen
        // (bei 3 Herzen also erst ab 1,5 Herzen, nicht schon nach dem ersten Treffer)
        let low = cur > 0 && cur <= (cap / 4).max(24).min(cap / 2);
        if !low {
            return;
        }
        ctx.request_repaint();
        let t = ctx.input(|i| i.time) as f32;
        // Herzschlag „ba-bumm“: bei sehr wenig Energie schneller
        let period = if cur <= 8 { 0.75 } else { 1.15 };
        let ph = (t % period) / period;
        let beat = (-((ph - 0.0) / 0.07).powi(2)).exp() + 0.65 * (-((ph - 0.24) / 0.07).powi(2)).exp() + (-((ph - 1.0) / 0.07).powi(2)).exp();
        let beat = beat.min(1.0);
        // weicher roter Verlauf vom Rand nach innen
        let edge = r.width().min(r.height()) * (0.09 + 0.03 * beat);
        let inner = r.shrink(edge);
        let a_out = (60.0 + 120.0 * beat) as u8;
        let out_c = Color32::from_rgba_unmultiplied(170, 0, 10, a_out);
        let in_c = Color32::from_rgba_unmultiplied(170, 0, 10, 0);
        let mut mesh = gfx::Mesh::default();
        let o = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        let i = [inner.left_top(), inner.right_top(), inner.right_bottom(), inner.left_bottom()];
        for k in 0..4 {
            let n = (k + 1) % 4;
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(o[k], out_c);
            mesh.colored_vertex(o[n], out_c);
            mesh.colored_vertex(i[n], in_c);
            mesh.colored_vertex(i[k], in_c);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        p.add(gfx::Shape::mesh(mesh));
        // Herz unten rechts schlägt mit
        let sz = (r.height() * 0.032).max(10.0) * (1.0 + 0.28 * beat);
        let c = gfx::pos2(r.right() - r.height() * 0.075, r.bottom() - r.height() * 0.075);
        paint_icon(p, c, sz, Icon::Heart, Color32::WHITE);
    }
}

fn quickmenu_has(s: &crate::snes::Snes, e: crate::quickmenu::Entry) -> bool {
    crate::quickmenu::entries(s).contains(&e)
}
