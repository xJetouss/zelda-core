//! Eigenes Inventar im Stil des Spielmenüs (schwarze Kästen mit farbigem Doppelrand):
//! Item markieren und mit Y oder X auf den jeweiligen Slot legen, A benutzt den Spiegel sofort.
//! Enthält alles aus dem Original-Menü: Items und Flaschen, Auswahl, Amulette/Kristalle mit
//! Herzteilen, Ausrüstung (im Dungeon dazu Karte/Kompass/Schlüssel) und die A-Fähigkeiten.

use crate::alttp::{ram16, ram8};
use crate::app::App;
use crate::i18n::tr;
use crate::quickmenu::{self, Entry};
use crate::ui::{current_dungeon, paint_icon, Icon, GREYC};
use crate::gfx::{self, Color32, Stroke};

/// Farben wie im Spielmenü
pub(crate) const MENU_BG: Color32 = Color32::from_rgb(0, 0, 0);
pub(crate) const MENU_LINE: Color32 = Color32::from_rgb(248, 248, 248);
pub(crate) const GREEN: Color32 = Color32::from_rgb(32, 197, 41);
pub(crate) const RED: Color32 = Color32::from_rgb(197, 0, 0);
pub(crate) const GOLDF: Color32 = Color32::from_rgb(230, 172, 0);
pub(crate) const YELLOW_X: Color32 = Color32::from_rgb(248, 200, 48);

/// Felder des Rasters: Items 1..20 (ohne 16) und danach die vier Flaschen
fn cells() -> Vec<Entry> {
    let mut v: Vec<Entry> = (1..=20u8).filter(|&i| i != 16).map(|idx| Entry { idx, bottle: None }).collect();
    v.extend((1..=4u8).map(|b| Entry { idx: 16, bottle: Some(b) }));
    v
}

const COLS: usize = 6;

impl App {
    /// Inventar öffnen (statt des Spielmenüs)
    pub(crate) fn inv_open(&mut self) {
        if self.win_map {
            return;
        }
        self.inv_open = true;
        if let Some(g) = &self.game {
            let cur = quickmenu::current(&g.snes);
            self.inv_sel = cells().iter().position(|e| *e == cur).unwrap_or(0);
        }
    }

    fn inv_close(&mut self, held: u16) {
        self.inv_open = false;
        // gedrückte Tasten erst nach dem Loslassen wieder ans Spiel geben
        self.inv_block = held;
    }

    /// Steuerung im Inventar (Joypad-Bits wie beim SNES)
    pub(crate) fn inv_input(&mut self, pad: u16, dt: f32) {
        let pressed = pad & !self.inv_prev;
        self.inv_prev = pad;
        let cs = cells();
        let d = self.rep_dir(pad, dt);
        if d != (0, 0) {
            let n = cs.len() as i32;
            let rows = (n + COLS as i32 - 1) / COLS as i32;
            let (mut x, mut y) = (self.inv_sel as i32 % COLS as i32, self.inv_sel as i32 / COLS as i32);
            x = (x + d.0).rem_euclid(COLS as i32);
            y = (y + d.1).rem_euclid(rows);
            let mut k = y * COLS as i32 + x;
            while k >= n {
                k -= if d.1 != 0 { COLS as i32 } else { 1 };
            }
            self.inv_sel = k.max(0) as usize;
        }
        if pressed & ((1 << 15) | (1 << 12)) != 0 {
            self.inv_close(pad);
            return;
        }
        let Some(e) = cs.get(self.inv_sel).copied() else { return };
        let Some(g) = &mut self.game else { return };
        if !quickmenu::entries(&g.snes).contains(&e) {
            return;
        }
        let name = quickmenu::name(&g.snes, e, self.s.ui_en);
        if pressed & (1 << 6) != 0 && self.s.x_slot {
            g.x_item = Some(e);
            self.msg(format!("X: {name}"));
        } else if pressed & (1 << 7) != 0 && e.idx == 20 && self.s.mirror_direct {
            // A auf dem Spiegel: sofort benutzen
            self.x_force = Some(e);
            self.inv_close(pad);
        } else if pressed & ((1 << 14) | (1 << 7)) != 0 && quickmenu::current(&g.snes) != e {
            let ok = quickmenu::equip(&mut g.snes, e);
            g.display.copy_from_slice(g.snes.framebuffer());
            self.reset_interp();
            if ok {
                self.msg(format!("Y: {name}"));
            }
        }
    }

    /// Kasten wie im Spielmenü (Gold), optional Titel oben in der Mitte
    pub(crate) fn gbox(&self, p: &gfx::Painter, r: gfx::Rect, title: Option<&str>, fs: f32) {
        self.gbox_c(p, r, GOLDF, title, fs);
    }

    /// Kasten mit deutlich runden Ecken (Spielstände): im HD-Stil weich, sonst als Pixel-Viertelkreis
    pub(crate) fn gbox_round(&self, p: &gfx::Painter, r: gfx::Rect, col: Color32) {
        if self.smooth_ui() {
            p.rect_filled(r, 22.0, Color32::BLACK);
            p.rect_filled(r.shrink(3.0), 19.0, col);
            p.rect_filled(r.shrink(8.0), 14.0, MENU_LINE);
            p.rect_filled(r.shrink(11.0), 11.0, MENU_BG);
            return;
        }
        let ppp = p.ctx().pixels_per_point();
        let u = (self.game_px * ppp).round().max(1.0) / ppp;
        let r = gfx::Rect::from_min_max(gfx::pos2((r.left() * ppp).round() / ppp, (r.top() * ppp).round() / ppp), gfx::pos2((r.right() * ppp).round() / ppp, (r.bottom() * ppp).round() / ppp));
        // Ecke als Pixel-Viertelkreis mit Radius `rad` (in Spielpixeln)
        let round = |p: &gfx::Painter, r: gfx::Rect, rad: f32, colr: Color32| {
            for k in 0..=rad as i32 {
                let k = k as f32;
                let dx = rad - (rad * rad - (rad - k) * (rad - k)).max(0.0).sqrt();
                p.rect_filled(r.shrink2(gfx::vec2(dx.round() * u, k * u)), 0.0, colr);
            }
        };
        round(p, r, 8.0, Color32::BLACK);
        round(p, r.shrink(u), 7.0, col);
        round(p, r.shrink(u * 3.0), 5.0, MENU_LINE);
        round(p, r.shrink(u * 4.0), 4.0, MENU_BG);
    }

    /// Kasten wie im Spielmenü: schwarzer Rand, Farblinie, weiße Linie, schwarz innen, runde Ecken
    pub(crate) fn gbox_c(&self, p: &gfx::Painter, r: gfx::Rect, col: Color32, title: Option<&str>, fs: f32) {
        if self.pixel_ui() {
            let ppp = p.ctx().pixels_per_point();
            let u = (self.game_px * ppp).round().max(1.0) / ppp;
            let r = gfx::Rect::from_min_max(gfx::pos2((r.left() * ppp).round() / ppp, (r.top() * ppp).round() / ppp), gfx::pos2((r.right() * ppp).round() / ppp, (r.bottom() * ppp).round() / ppp));
            // Treppen-Ecken wie im Spiel
            let stair = |p: &gfx::Painter, r: gfx::Rect, c: f32, colr: Color32| {
                for k in 0..=c as i32 {
                    let k = k as f32;
                    p.rect_filled(r.shrink2(gfx::vec2(k * u, (c - k) * u)), 0.0, colr);
                }
            };
            stair(p, r, 3.0, Color32::BLACK);
            stair(p, r.shrink(u), 2.0, col);
            stair(p, r.shrink(u * 3.0), 1.0, MENU_LINE);
            stair(p, r.shrink(u * 4.0), 1.0, MENU_BG);
        } else {
            p.rect_filled(r, 9.0, Color32::BLACK);
            p.rect_filled(r.shrink(2.0), 7.0, col);
            p.rect_filled(r.shrink(6.0), 4.0, MENU_LINE);
            p.rect_filled(r.shrink(8.0), 3.0, MENU_BG);
        }
        if let Some(t) = title {
            // Abstand zum Rand (im Pixel-Modus ist der Rand dicker)
            let dy = if self.pixel_ui() { fs * 1.25 } else { fs * 0.95 };
            self.mtext(p, gfx::pos2(r.center().x, r.top() + dy), gfx::Align2::CENTER_CENTER, t, fs * 0.85, MENU_LINE);
        }
    }

    /// Buchstabe auf schwarzem Grund (Pixel-Schrift bzw. weich im HD-Modus)
    fn letter(&self, p: &gfx::Painter, at: gfx::Pos2, txt: &str, size: f32, col: Color32) {
        if self.pixel_ui() {
            let gp = (size / 8.0).round().max(1.0);
            crate::pixfont::text(p, at, gfx::Align2::CENTER_CENTER, txt, gp, col);
        } else {
            p.text(at, gfx::Align2::CENTER_CENTER, txt, gfx::FontId::proportional(size * 1.2), col);
        }
    }

    /// Auswahl wie im Spielmenü (blinkende Ecken)
    fn cursor(&self, p: &gfx::Painter, r: gfx::Rect, t: f32) {
        let pix = self.pixel_ui();
        let u = if pix { self.game_px.round().max(1.0) } else { 2.5 };
        let r = r.expand(u * if (t * 4.0) as i64 % 2 == 0 { 0.5 } else { 1.5 });
        let a = r.width() * 0.3;
        for (c, sx, sy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
            for (dx, dy) in [(a, u * 2.0), (u * 2.0, a)] {
                let q = gfx::Rect::from_two_pos(c, c + gfx::vec2(sx * dx, sy * dy));
                p.rect_filled(q.expand(u * 0.5), if pix { 0.0 } else { 1.0 }, Color32::BLACK);
                p.rect_filled(q, if pix { 0.0 } else { 1.0 }, GREEN);
            }
        }
    }

    /// Item-Feld wie das Y-Feld im Spiel (goldener Rand, weiße Linie, schwarz innen)
    pub(crate) fn item_frame(&self, p: &gfx::Painter, r: gfx::Rect) {
        self.item_frame_s(p, r, true, true);
    }

    /// `snap`: auf ganze Bildschirmpixel runden (Menüs). Im Spielbild nicht runden, damit das Feld
    /// genau so groß ist wie das Y-Feld, das mit dem Spielbild skaliert wird.
    /// `fill`: innen schwarz (Feld mit Item) – sonst durchsichtig wie das leere Feld im Spiel
    pub(crate) fn item_frame_s(&self, p: &gfx::Painter, r: gfx::Rect, snap: bool, fill: bool) {
        if self.smooth_ui() {
            // HD: weich gezeichnet
            let u = r.width() / 22.0;
            if fill {
                // schwarz bis unter die weiße Linie (keine Lücke an Rand und Ecken)
                p.rect_filled(r.shrink(u * 1.2), u * 2.0, Color32::BLACK);
            }
            p.rect_stroke(r.shrink(u * 0.5), u * 2.5, Stroke::new(u, GOLDF));
            p.rect_stroke(r.shrink(u * 1.5), u * 1.8, Stroke::new(u, MENU_LINE));
            return;
        }
        // Muster 22×22 aus dem Spiel (Gold außen, Weiß innen, runde Ecken, innen durchsichtig)
        let ppp = p.ctx().pixels_per_point();
        let (u, v, r) = if snap {
            let u = ((r.width() / 22.0) * ppp).round().max(1.0) / ppp;
            let v = ((r.height() / 22.0) * ppp).round().max(1.0) / ppp;
            (u, v, gfx::Rect::from_min_size(gfx::pos2((r.left() * ppp).round() / ppp, (r.top() * ppp).round() / ppp), gfx::vec2(u * 22.0, v * 22.0)))
        } else {
            (r.width() / 22.0, r.height() / 22.0, r)
        };
        let col = |row: i32, c: i32| -> Option<Color32> {
            let a = row.min(21 - row);
            let b = c.min(21 - c);
            match (a.min(b), a.max(b)) {
                (0, 0) => None,
                (0, _) => Some(GOLDF),
                (1, 1) => Some(GOLDF),
                (1, _) => Some(MENU_LINE),
                (2, 2) => Some(MENU_LINE),
                _ => fill.then_some(Color32::BLACK),
            }
        };
        for row in 0..22 {
            // gleiche Farben in einer Zeile zusammenfassen
            let mut c = 0;
            while c < 22 {
                let k = col(row, c);
                let mut e = c + 1;
                while e < 22 && col(row, e) == k {
                    e += 1;
                }
                if let Some(k) = k {
                    let sn = |x: f32| (x * ppp).round() / ppp;
                    let rr = gfx::Rect::from_min_max(gfx::pos2(sn(r.left() + c as f32 * u), sn(r.top() + row as f32 * v)), gfx::pos2(sn(r.left() + e as f32 * u), sn(r.top() + (row + 1) as f32 * v)));
                    p.rect_filled(rr, 0.0, k);
                }
                c = e;
            }
        }
    }

    /// Herz mit Vierteln (Herzteile)
    fn heart_pieces(&self, p: &gfx::Painter, c: gfx::Pos2, r: f32, n: u8) {
        paint_icon(p, c, r, Icon::Heart, GREYC);
        let q = gfx::vec2(r * 2.0, r * 2.0);
        let quads = [gfx::Rect::from_min_size(c - q, q), gfx::Rect::from_min_size(c - gfx::vec2(q.x, 0.0), q), gfx::Rect::from_min_size(c - gfx::vec2(0.0, q.y), q), gfx::Rect::from_min_size(c, q)];
        for k in 0..(n as usize).min(4) {
            paint_icon(&p.with_clip_rect(quads[k].intersect(p.clip_rect())), c, r, Icon::Heart, Color32::WHITE);
        }
    }

    /// Ausrüstungs-Symbol aus dem Menü (oder grauer Platzhalter)
    fn equip_icon(&self, p: &gfx::Painter, c: gfx::Pos2, size: f32, k: u8, have: bool) {
        let pix = self.pixel_ui();
        let tex = if pix && !crate::pixfont::smooth() { if crate::pixfont::fine() { self.qm.equip_fine.get(&k).or(self.qm.equip_raw.get(&k)) } else { self.qm.equip_raw.get(&k) } } else { self.qm.equip_hd.get(&k) };
        match tex.filter(|_| have) {
            Some(t) => {
                let [w, h] = t.size();
                let s = size / w.max(h) as f32;
                let s = if pix { s.round().max(1.0) } else { s };
                let sz = gfx::vec2(w as f32 * s, h as f32 * s);
                p.image(t.id(), gfx::Rect::from_center_size(c, sz), gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                let d = if pix { self.game_px.round().max(1.0) } else { 2.0 };
                p.rect_filled(gfx::Rect::from_center_size(c, gfx::vec2(d * 2.0, d * 2.0)), 0.0, Color32::from_gray(70));
            }
        }
    }

    /// Dungeon-Gegenstand: Symbol aus dem Spielmenü (fehlt es noch: gezeichnetes Symbol); nicht vorhanden = abgedunkelt
    fn dungeon_icon(&self, p: &gfx::Painter, c: gfx::Pos2, size: f32, k: u8, have: bool, fallback: Icon) {
        let pix = self.pixel_ui();
        let tex = if pix && !crate::pixfont::smooth() { if crate::pixfont::fine() { self.qm.equip_fine.get(&k).or(self.qm.equip_raw.get(&k)) } else { self.qm.equip_raw.get(&k) } } else { self.qm.equip_hd.get(&k) };
        let tint = if have { Color32::WHITE } else { Color32::from_gray(55) };
        match tex {
            Some(t) => {
                let [w, h] = t.size();
                let s = size * 1.6 / w.max(h) as f32;
                let s = if pix { (s * w.max(h) as f32).round() / w.max(h) as f32 } else { s };
                let sz = gfx::vec2(w as f32 * s, h as f32 * s);
                let r = gfx::Rect::from_center_size(c, sz);
                let r = if pix { gfx::Rect::from_min_size(r.min.round(), sz) } else { r };
                p.image(t.id(), r, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), tint);
            }
            None => paint_icon(p, c, size, fallback, if have { Color32::WHITE } else { GREYC }),
        }
    }

    /// Inventar zeichnen (nur über dem Spielbild)
    pub(crate) fn inventory_overlay(&mut self, ctx: &gfx::Context, p: &gfx::Painter, area: gfx::Rect) {
        if !self.inv_open {
            return;
        }
        let Some(g) = &self.game else { return };
        ctx.request_repaint();
        let en = self.s.ui_en;
        let pix = self.pixel_ui();
        let gp = self.game_px.round().max(1.0);
        let s = &g.snes;
        let t = ctx.input(|i| i.time) as f32;
        p.rect_filled(area, 0.0, Color32::from_black_alpha(140));
        let fs = if pix { gp * 8.0 } else { (area.height() * 0.036).clamp(16.0, 30.0) };
        let small = fs * 0.75;
        let outer = area.shrink(area.width().min(area.height()) * 0.025);
        let gap = fs * 0.5;
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        let owned = quickmenu::entries(s);
        let cur = quickmenu::current(s);
        // Aufteilung wie im Spielmenü
        let lw = (outer.width() - gap) * 0.64;
        let top_h = (outer.height() - gap) * 0.68;
        let items_box = gfx::Rect::from_min_size(outer.min, gfx::vec2(lw, top_h));
        let a_box = gfx::Rect::from_min_max(gfx::pos2(outer.left(), items_box.bottom() + gap), gfx::pos2(items_box.right(), outer.bottom()));
        let rx = items_box.right() + gap;
        let sel_box = gfx::Rect::from_min_max(gfx::pos2(rx, outer.top()), gfx::pos2(outer.right(), outer.top() + top_h * 0.42));
        let am_box = gfx::Rect::from_min_max(gfx::pos2(rx, sel_box.bottom() + gap), gfx::pos2(outer.right(), items_box.bottom()));
        let eq_box = gfx::Rect::from_min_max(gfx::pos2(rx, a_box.top()), outer.max);
        self.gbox_c(p, items_box, GREEN, None, fs);
        self.gbox_c(p, a_box, RED, None, fs);
        self.gbox_c(p, sel_box, GREEN, None, fs);
        self.gbox(p, am_box, Some(tr(en, "AMULETTE", "PENDANTS")), fs);
        self.gbox(p, eq_box, Some(tr(en, "AUSRÜSTUNG", "EQUIPMENT")), fs);
        // große Buchstaben wie im Spiel: Y (grün) und X im Item-Kasten, A (rot)
        let big = fs * 1.5;
        self.letter(p, items_box.left_top() + gfx::vec2(fs * 1.5, fs * 1.7), "Y", big, GREEN);
        if self.s.x_slot {
            self.letter(p, items_box.left_top() + gfx::vec2(fs * 1.5, fs * 3.7), "X", big, YELLOW_X);
        }
        self.letter(p, gfx::pos2(a_box.left() + fs * 1.5, a_box.center().y), "A", big, RED);
        // ---- Items
        let cs = cells();
        let rows = (cs.len() + COLS - 1) / COLS;
        let gi = gfx::Rect::from_min_max(items_box.min + gfx::vec2(fs * 2.8, fs * 1.0), items_box.max - gfx::vec2(fs * 0.8, fs * 1.0));
        let cell = (gi.width() / COLS as f32).min(gi.height() / rows as f32).floor();
        let cell = if pix { (cell / gp).floor() * gp } else { cell };
        let gx0 = gi.center().x - cell * COLS as f32 * 0.5;
        let gy0 = gi.center().y - cell * rows as f32 * 0.5;
        let hover = ctx.input(|i| i.pointer.hover_pos());
        let mut hovered = None;
        for (k, e) in cs.iter().enumerate() {
            let r = gfx::Rect::from_min_size(gfx::pos2(gx0 + (k % COLS) as f32 * cell, gy0 + (k / COLS) as f32 * cell), gfx::vec2(cell, cell)).shrink(cell * 0.1);
            if owned.contains(e) {
                if let Some(tex) = if pix { self.qm.icon_raw(s, *e) } else { self.qm.icon(s, *e) } {
                    let isz = if pix { ((r.width() * 0.9) / (gp * 20.0)).floor().max(1.0) * gp * 20.0 } else { r.width() * 0.9 };
                    p.image(tex.id(), gfx::Rect::from_center_size(r.center(), gfx::vec2(isz, isz)), uv, Color32::WHITE);
                }
                // kleine Buchstaben: was liegt auf Y / X
                if *e == cur {
                    self.letter(p, r.left_bottom() + gfx::vec2(fs * 0.35, -fs * 0.35), "Y", fs * 0.8, GREEN);
                }
                if self.s.x_slot && g.x_item == Some(*e) {
                    self.letter(p, r.right_bottom() + gfx::vec2(-fs * 0.35, -fs * 0.35), "X", fs * 0.8, YELLOW_X);
                }
            }
            if k == self.inv_sel {
                self.cursor(p, r, t);
            }
            if hover.map_or(false, |h| r.contains(h)) {
                hovered = Some(k);
            }
        }
        if let Some(k) = hovered {
            if ctx.input(|i| i.pointer.delta().length() > 0.0) {
                self.inv_sel = k;
            }
            if ctx.input(|i| i.pointer.primary_clicked()) {
                self.inv_sel = k;
                self.inv_input(1 << 14, 0.0);
                self.inv_prev = 0;
            }
        }
        let Some(g) = &self.game else { return };
        let s = &g.snes;
        // ---- Auswahl: großes Symbol + Name, dazu Y- und X-Feld
        let sel = cs.get(self.inv_sel).copied().filter(|e| owned.contains(e));
        let si = sel_box.shrink(fs * 0.9);
        let slot = (si.height() * 0.62).min(si.width() * 0.26);
        let slots: Vec<(&str, Option<Entry>, Color32)> = if self.s.x_slot { vec![("Y", Some(cur), GREEN), ("X", g.x_item, YELLOW_X)] } else { vec![("Y", Some(cur), GREEN)] };
        let total = slots.len() as f32 * slot + (slots.len() as f32 - 1.0) * fs * 3.4 + fs * 1.6;
        let mut x = si.center().x - total * 0.5 + fs * 1.6;
        for (label, e, col) in slots {
            let br = gfx::Rect::from_min_size(gfx::pos2(x, si.top()), gfx::vec2(slot, slot));
            self.item_frame(p, br);
            if let Some(e) = e.filter(|e| owned.contains(e)) {
                if let Some(tex) = if pix { self.qm.icon_raw(s, e) } else { self.qm.icon(s, e) } {
                    p.image(tex.id(), br.shrink(slot * 0.13), uv, Color32::WHITE);
                }
            }
            self.letter(p, gfx::pos2(br.left() - fs * 0.9, br.center().y), label, fs * 1.1, col);
            x += slot + fs * 3.4;
        }
        let name = sel.map(|e| quickmenu::name(s, e, en)).unwrap_or_default().to_uppercase();
        let name = self.fit_text(p, &name, small, si.width());
        self.mtext(p, gfx::pos2(si.center().x, si.bottom()), gfx::Align2::CENTER_BOTTOM, &name, small, MENU_LINE);
        // ---- Amulette: Anhänger, Kristalle, Herzteile
        let ai = am_box.shrink(fs * 0.9);
        let ic = fs * 0.55;
        let pb = ram8(s, 0x7EF374);
        let cb = ram8(s, 0x7EF37A);
        let row1 = ai.top() + fs * if pix { 2.1 } else { 1.8 };
        let pend = [(0x04u8, Icon::PendG), (0x02, Icon::PendB), (0x01, Icon::PendR)];
        for (k, (bit, icon)) in pend.iter().enumerate() {
            let x = ai.center().x + (k as f32 - 1.0) * ic * 3.2;
            paint_icon(p, gfx::pos2(x, row1), ic, *icon, if pb & bit != 0 { Color32::WHITE } else { GREYC });
        }
        let row2 = row1 + ic * 2.8;
        for (k, (_, bit)) in crate::mapdata::CRYSTAL_BITS.iter().enumerate() {
            let x = ai.center().x + (k as f32 - 3.0) * ic * 2.4;
            paint_icon(p, gfx::pos2(x, row2), ic * 0.85, if k == 4 || k == 5 { Icon::CrystR } else { Icon::Cryst }, if cb & bit != 0 { Color32::WHITE } else { GREYC });
        }
        let row3 = (row2 + ic * 3.0).min(ai.bottom() - ic);
        self.heart_pieces(p, gfx::pos2(ai.center().x - fs * 1.2, row3), ic * 1.1, ram8(s, 0x7EF36B) & 3);
        self.mtext(p, gfx::pos2(ai.center().x, row3), gfx::Align2::LEFT_CENTER, &format!("{}/4", ram8(s, 0x7EF36B) & 3), small, MENU_LINE);
        // ---- Ausrüstung: Schwert, Schild, Gewand + Vorräte bzw. Dungeon-Gegenstände
        let ei = eq_box.shrink(fs * 0.9);
        let er = ei.top() + fs * 2.3;
        let esz = fs * 2.6;
        let sw = ram8(s, 0x7EF359);
        for (k, have) in [(0u8, sw > 0), (1, ram8(s, 0x7EF35A) > 0), (2, true)] {
            let x = ei.center().x + (k as f32 - 1.0) * esz * 1.25;
            self.equip_icon(p, gfx::pos2(x, er), esz, k, have);
        }
        let ly = er + esz * 0.85;
        if sw > 0 {
            // Schwertstufe klein rechts unten neben dem Schwert, sicher über der Linie
            let ny = (er + esz * 0.3).min(ly - small * 0.6);
            self.mtext(p, gfx::pos2(ei.center().x - esz * 1.25 + esz * 0.5, ny), gfx::Align2::LEFT_CENTER, &sw.to_string(), small, MENU_LINE);
        }
        // gestrichelte Linie wie im Spiel
        let dash = if pix { gp * 3.0 } else { 6.0 };
        let mut dx = ei.left();
        while dx < ei.right() {
            p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(dx, ly), gfx::vec2(dash * 0.6, if pix { gp } else { 2.0 })), 0.0, MENU_LINE);
            dx += dash;
        }
        let lower = (ly + fs * 1.6).min(ei.bottom() - fs * 1.4);
        if let Some(d) = current_dungeon(s) {
            // Dungeon-Gegenstände mittig zwischen Linie und unterem Rand, nie auf der Linie
            let room_h = (ei.bottom() - ly).max(1.0);
            let lower = ly + room_h * 0.55;
            let isz = (ic * 1.1).min(room_h * 0.7);
            let bit = if d <= 1 { 0xC000u16 } else { 0x8000u16 >> d };
            let has = |a: u32| ram16(s, a) & bit != 0;
            let keys = ram8(s, 0x7EF36F);
            let items = [(7u8, Icon::MapI, has(0x7EF368)), (8, Icon::Compass, has(0x7EF364)), (9, Icon::BigKey, has(0x7EF366)), (10, Icon::Key, keys > 0 && keys != 0xFF)];
            for (k, (key, icon, on)) in items.iter().enumerate() {
                let x = ei.left() + ei.width() * (k as f32 + 0.5) / 4.0;
                self.dungeon_icon(p, gfx::pos2(x, lower), isz, *key, *on, *icon);
            }
            if keys != 0xFF {
                self.mtext(p, gfx::pos2(ei.left() + ei.width() * 3.5 / 4.0 + isz * 0.65, lower + isz * 0.3), gfx::Align2::LEFT_CENTER, &keys.to_string(), small, MENU_LINE);
            }
        } else {
            // Vorräte in zwei Zeilen
            let l1 = format!("{} {}", tr(en, "RUBINE", "RUPEES"), ram16(s, 0x7EF362));
            let l2 = format!("{} {}   {} {}", tr(en, "BOMBEN", "BOMBS"), ram8(s, 0x7EF343), tr(en, "PFEILE", "ARROWS"), ram8(s, 0x7EF377));
            let sz = small * 0.9;
            let l2 = self.fit_text(p, &l2, sz, ei.width());
            self.mtext(p, gfx::pos2(ei.center().x, lower - fs * 0.3), gfx::Align2::CENTER_CENTER, &l1, sz, MENU_LINE);
            self.mtext(p, gfx::pos2(ei.center().x, lower + fs * 0.9), gfx::Align2::CENTER_CENTER, &l2, sz, MENU_LINE);
        }
        // ---- A: Fähigkeiten als ordentliches Raster, rechts daneben Stiefel, Handschuhe, Flossen, Mondperle
        let ai = gfx::Rect::from_min_max(a_box.min + gfx::vec2(fs * 3.0, fs * 0.9), a_box.max - gfx::vec2(fs * 1.0, fs * 0.9));
        let gl = ram8(s, 0x7EF354);
        let ab: Vec<(&str, bool)> = vec![
            (if gl >= 2 { tr(en, "HEBEN 3", "LIFT 3") } else if gl == 1 { tr(en, "HEBEN 2", "LIFT 2") } else { tr(en, "HEBEN 1", "LIFT 1") }, true),
            (tr(en, "LESEN", "READ"), ram8(s, 0x7EF34E) != 0),
            (tr(en, "SPRECHEN", "TALK"), true),
            (tr(en, "ZIEHEN", "PULL"), true),
            (tr(en, "LAUFEN", "RUN"), ram8(s, 0x7EF355) != 0),
            (tr(en, "SCHWIMMEN", "SWIM"), ram8(s, 0x7EF356) != 0),
        ];
        let split = ai.left() + ai.width() * 0.56;
        let tw = (split - ai.left()) / 2.0;
        let th = ai.height() / 3.0;
        for (k, (a, on)) in ab.iter().enumerate() {
            let c = gfx::pos2(ai.left() + tw * (k % 2) as f32, ai.top() + th * ((k / 2) as f32 + 0.5));
            self.mtext(p, c, gfx::Align2::LEFT_CENTER, a, small, if *on { MENU_LINE } else { Color32::from_gray(75) });
        }
        // Trennlinie
        let u = if pix { gp } else { 2.0 };
        p.rect_filled(gfx::Rect::from_min_size(gfx::pos2(split - fs * 0.5, ai.top() + fs * 0.3), gfx::vec2(u, ai.height() - fs * 0.6)), 0.0, Color32::from_gray(90));
        let ri = gfx::Rect::from_min_max(gfx::pos2(split + fs * 0.3, ai.top()), ai.max);
        let gear = [(3u8, ram8(s, 0x7EF355) != 0, tr(en, "STIEFEL", "BOOTS")), (4, gl > 0, tr(en, "HANDSCH.", "GLOVES")), (5, ram8(s, 0x7EF356) != 0, tr(en, "FLOSSEN", "FLIPPERS")), (6, ram8(s, 0x7EF357) != 0, tr(en, "PERLE", "PEARL"))];
        // 2×2 Symbole, groß und mittig
        let cw = ri.width() / 2.0;
        let rh = ri.height() / 2.0;
        let isz = (cw * 0.6).min(rh * 0.8).min(fs * 3.0);
        for (k, (key, have, _label)) in gear.iter().enumerate() {
            let c = gfx::pos2(ri.left() + cw * ((k % 2) as f32 + 0.5), ri.top() + rh * ((k / 2) as f32 + 0.5));
            self.equip_icon(p, c, isz, *key, *have);
        }
        let _ = Stroke::NONE;
    }

    /// Schriftgröße so wählen, dass der Text in die Breite passt
    #[allow(dead_code)]
    pub(crate) fn fit_size(&self, p: &gfx::Painter, text: &str, start: f32, maxw: f32) -> f32 {
        let pix = self.pixel_ui();
        let mut s = start;
        while self.text_w(p, text, s) > maxw && s > 8.0 {
            s -= if pix { 8.0 } else { 1.0 };
        }
        s
    }
}
