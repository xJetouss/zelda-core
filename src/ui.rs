//! Selbst gezeichnete Oberfläche: Spielbild, Hauptmenü, Spielstände, Schnellmenü, Symbole.
//! Menüleiste, Statusleiste und Fenster liegen in `shell.rs` (GPUI-Elemente).

use crate::alttp;
use crate::app::App;
use crate::gfx::{self, Color32, Stroke};
use crate::i18n::tr;
use crate::quickmenu;

const GOLD: Color32 = Color32::from_rgb(235, 200, 90);

impl App {
    pub(crate) fn draw_ui(&mut self, ctx: &gfx::Context) {
        init_pixel_icons(ctx);
        HD_ICONS.store(self.smooth_ui(), std::sync::atomic::Ordering::Relaxed);
        crate::pixfont::set_fine(self.fine_ui());
        crate::pixfont::set_smooth(self.smooth_ui());
        self.game_view(ctx);
        self.popups(ctx);
        self.draw_quickmenu(ctx);
        self.states_overlay(ctx);
    }

    #[allow(dead_code)]
    pub(crate) fn hk(&self, id: &str) -> String {
        self.s.hotkey(id).map(|c| c.to_text()).unwrap_or_default()
    }

    // ------------------------------------------------------------ Spielbild

    fn game_rect(&self, area: gfx::Rect, ctx: &gfx::Context) -> gfx::Rect {
        let base = if self.s.aspect_43 { 4.0 / 3.0 } else { 256.0 / 224.0 };
        let aspect = base * self.view_w as f32 / 256.0;
        let (mut w, mut h) = if area.width() / area.height() > aspect { (area.height() * aspect, area.height()) } else { (area.width(), area.width() / aspect) };
        if self.s.integer_scale {
            let ppp = ctx.pixels_per_point();
            let k = ((h * ppp) / 224.0).floor().max(1.0);
            h = k * 224.0 / ppp;
            w = h * aspect;
        }
        gfx::Rect::from_center_size(area.center(), gfx::vec2(w, h))
    }

    fn game_view(&mut self, ctx: &gfx::Context) {
        let en = self.s.ui_en;
        {
            let area = self.central;
            self.area_aspect = area.width() / area.height().max(1.0);
            let painter = ctx.painter().with_clip_rect(area);
            painter.rect_filled(area, 0.0, Color32::BLACK);
            if self.gba.active() {
                self.gba_view(&painter, area, ctx);
            } else if self.game.is_some() {
                let r = self.game_rect(area, ctx);
                self.game_px = r.width() / self.view_w as f32;
                self.game_area = area;
                if let Some(m) = self.gpu_mode() {
                    // HD-Filter auf der Grafikkarte: genau in Bildschirmpixeln berechnen und 1:1 zeigen
                    let ppp = ctx.pixels_per_point();
                    let (x0, y0) = ((r.left() * ppp).round(), (r.top() * ppp).round());
                    let (x1, y1) = ((r.right() * ppp).round(), (r.bottom() * ppp).round());
                    let snapped = gfx::Rect::from_min_max(gfx::pos2(x0 / ppp, y0 / ppp), gfx::pos2(x1 / ppp, y1 / ppp));
                    if let Some(img) = crate::perf::time("hd", || self.gpu.render(m, (x1 - x0) as i32, (y1 - y0) as i32)) {
                        painter.render_image(img, snapped);
                    }
                } else if let Some(t) = &self.tex {
                    painter.image(t.id(), r, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), Color32::WHITE);
                }
                self.in_game_map_overlay(&painter, r, ctx);
                self.tp_overlay(&painter, r, ctx);
                self.low_health_hud(&painter, r, ctx);
                // Zeichnungen der Mods (nur im Recompiled-Modus)
                self.mods.draw(&painter, r, self.view_w, self.ext);
                self.mod_place_overlay(&painter, ctx, r, self.view_w, 224, self.ext);
                self.inventory_overlay(ctx, &painter, area);
                let ind = if self.turbo_on {
                    Some(format!("⏩ TURBO ×{:.0}", self.s.turbo_speed))
                } else if self.double_on {
                    Some("⏩ ×2".to_string())
                } else if self.rewind_on {
                    Some(tr(en, "⏪ Zurückspulen", "⏪ Rewind").to_string())
                } else {
                    None
                };
                if let (Some(txt), true) = (&ind, self.pixel_ui()) {
                    let gp = self.game_px.max(1.0).round();
                    // unten rechts, damit nichts von der Statusleiste verdeckt wird
                    let tr_ = gfx::pos2(r.right() - gp * 8.0, r.bottom() - gp * 18.0);
                    let w = crate::pixfont::width(txt, gp);
                    let br = gfx::Rect::from_min_size(gfx::pos2(tr_.x - w - gp * 4.0, tr_.y - gp * 3.0), gfx::vec2(w + gp * 8.0, gp * 13.0));
                    painter.rect_filled(br, 0.0, Color32::from_rgb(8, 10, 40));
                    painter.rect_stroke(br, 0.0, Stroke::new(gp, Color32::WHITE));
                    crate::pixfont::text(&painter, tr_, gfx::Align2::RIGHT_TOP, txt, gp, GOLD);
                } else if let Some(txt) = ind {
                    let galley = painter.layout_no_wrap(txt, gfx::FontId::proportional(22.0), GOLD);
                    let pos = gfx::pos2(r.right() - galley.size().x - 18.0, r.bottom() - galley.size().y - 18.0);
                    painter.rect_filled(gfx::Rect::from_min_size(pos - gfx::vec2(8.0, 4.0), galley.size() + gfx::vec2(16.0, 8.0)), 6.0, Color32::from_black_alpha(170));
                    painter.galley(pos, galley, GOLD);
                }
                if self.paused || self.auto_paused {
                    if self.pixel_ui() {
                        crate::pixfont::text(&painter, area.center(), gfx::Align2::CENTER_CENTER, tr(en, "PAUSE", "PAUSED"), (self.game_px * 2.0).round().max(2.0), Color32::WHITE);
                    } else if self.paused {
                        painter.text(area.center(), gfx::Align2::CENTER_CENTER, tr(en, "PAUSE", "PAUSED"), gfx::FontId::proportional(42.0), Color32::from_white_alpha(200));
                    } else {
                        painter.text(area.center(), gfx::Align2::CENTER_CENTER, tr(en, "⏸ Pause – Fenster anklicken", "⏸ Paused – click the window"), gfx::FontId::proportional(26.0), Color32::from_white_alpha(220));
                    }
                }
                if self.s.show_fps {
                    painter.text(gfx::pos2(r.left() + 10.0, r.top() + 8.0), gfx::Align2::LEFT_TOP, format!("{:.0} FPS · {} {:.0}", self.fps, tr(en, "Spiel", "game"), self.game_fps), gfx::FontId::monospace(14.0), Color32::YELLOW);
                }
                // Meldungen (es gibt keine Statusleiste mehr): kleine Plakette unten links, blendet aus
                if let Some((m, t)) = &self.status {
                    let age = t.elapsed().as_secs_f32();
                    if age < 3.0 {
                        let a = ((3.0 - age) / 0.4).clamp(0.0, 1.0);
                        let g = painter.layout_no_wrap(m.clone(), gfx::FontId::proportional(14.0), Color32::from_white_alpha((235.0 * a) as u8));
                        let pos = area.left_bottom() + gfx::vec2(16.0, -16.0 - g.size().y);
                        let bg = gfx::Rect::from_min_size(pos - gfx::vec2(12.0, 6.0), g.size() + gfx::vec2(24.0, 12.0));
                        painter.rect_filled(bg, 8.0, Color32::from_rgba_unmultiplied(16, 18, 22, (215.0 * a) as u8));
                        painter.rect_stroke(bg, 8.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, (22.0 * a) as u8)));
                        painter.galley(pos, g, Color32::WHITE);
                    }
                }
            } else {
                self.main_menu(&painter, area, ctx);
            }
        }
    }

    /// Hauptmenü: alle Spiele als Karten (A/Start oder Klick = spielen)
    fn main_menu(&mut self, painter: &gfx::Painter, area: gfx::Rect, ctx: &gfx::Context) {
        let en = self.s.ui_en;
        let gold = Color32::from_rgb(248, 208, 64);
        let game = |size: f32| gfx::FontId::new(size, gfx::FontFamily::Name("game".into()));
        painter.rect_filled(area, 0.0, Color32::from_rgb(6, 6, 10));
        // kleines Triforce über dem Titel (wie das App-Symbol)
        paint_triforce(painter, area.center_top() + gfx::vec2(0.0, area.height() * 0.06), (area.height() * 0.02).clamp(8.0, 16.0), gold);
        painter.text(area.center_top() + gfx::vec2(0.0, area.height() * 0.17), gfx::Align2::CENTER_CENTER, "ZELDA CORE", game((area.height() * 0.09).clamp(28.0, 64.0)), gold);
        painter.text(area.center_top() + gfx::vec2(0.0, area.height() * 0.17 + (area.height() * 0.07).clamp(24.0, 48.0)), gfx::Align2::CENTER_CENTER, tr(en, "SPIELE", "GAMES"), game(18.0), Color32::from_gray(170));
        let lib = self.library();
        let n = lib.len() + 1;
        let cw = (area.width() * 0.26).clamp(160.0, 300.0);
        let ch = cw * 1.25;
        let gap = cw * 0.12;
        let total = n as f32 * cw + (n - 1) as f32 * gap;
        let x0 = area.center().x - total * 0.5;
        let y0 = area.center().y - ch * 0.42;
        let (click, hover) = ctx.input(|i| (i.pointer.primary_clicked(), i.pointer.hover_pos()));
        let t = ctx.input(|i| i.time) as f32;
        for k in 0..n {
            let card = gfx::Rect::from_min_size(gfx::pos2(x0 + k as f32 * (cw + gap), y0), gfx::vec2(cw, ch));
            if hover.map_or(false, |h| card.contains(h)) {
                self.lib_sel = k;
                if click {
                    self.lib_start(k);
                    return;
                }
            }
            let sel = self.lib_sel == k;
            self.gbox_round(painter, card, if sel { crate::inventory::GOLDF } else { Color32::from_gray(90) });
            let art = gfx::Rect::from_min_size(card.min + gfx::vec2(cw * 0.12, ch * 0.1), gfx::vec2(cw * 0.76, cw * 0.66));
            if let Some(p) = lib.get(k).filter(|p| crate::gba::is_gba(p)) {
                // GBA-Spiel: Titelbild mit Logo (aus dem Spiel selbst, beim ersten Mal kurz gestartet)
                let side_w = art.width();
                let lr = gfx::Rect::from_center_size(art.center(), gfx::vec2(side_w, side_w * 2.0 / 3.0));
                if let Some(t) = self.gba_card(p, ctx) {
                    painter.image(t.id(), lr, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), if sel { Color32::WHITE } else { Color32::from_gray(150) });
                } else {
                    painter.rect_filled(lr, 6.0, Color32::from_rgb(20, 60, 30));
                }
                let title = crate::gba::display_name(p).to_uppercase();
                painter.text(gfx::pos2(card.center().x, art.bottom() + ch * 0.1), gfx::Align2::CENTER_CENTER, title, game((cw * 0.085).clamp(12.0, 24.0)), if sel { Color32::WHITE } else { Color32::from_gray(160) });
                painter.text(gfx::pos2(card.center().x, art.bottom() + ch * 0.2), gfx::Align2::CENTER_CENTER, "GAME BOY ADVANCE", gfx::FontId::proportional((cw * 0.045).clamp(10.0, 14.0)), Color32::from_gray(120));
            } else if let Some(p) = lib.get(k) {
                // Symbol des Spiels: Logo von A Link to the Past
                let logo = self.alttp_logo.get_or_insert_with(|| {
                    // Bild aus dem Build (eigene Builds: Logo aus assets/, öffentliche Builds: Triforce)
                    let px = include_bytes!(concat!(env!("OUT_DIR"), "/logo.rgba"));
                    ctx.load_texture("alttp_logo", gfx::ColorImage::from_rgba_unmultiplied([384, 384], px), gfx::TextureOptions::LINEAR)
                });
                let side = art.width().min(art.height() * 1.25);
                let lr = gfx::Rect::from_center_size(art.center(), gfx::vec2(side, side));
                painter.image(logo.id(), lr, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), if sel { Color32::WHITE } else { Color32::from_gray(150) });
                painter.text(gfx::pos2(card.center().x, art.bottom() + ch * 0.1), gfx::Align2::CENTER_CENTER, "A LINK TO THE PAST", game((cw * 0.085).clamp(12.0, 24.0)), if sel { Color32::WHITE } else { Color32::from_gray(160) });
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let name: String = if name.chars().count() > 34 { name.chars().take(33).collect::<String>() + "…" } else { name };
                painter.text(gfx::pos2(card.center().x, art.bottom() + ch * 0.2), gfx::Align2::CENTER_CENTER, name, gfx::FontId::proportional((cw * 0.045).clamp(10.0, 14.0)), Color32::from_gray(120));
            } else {
                painter.text(art.center(), gfx::Align2::CENTER_CENTER, "+", game(cw * 0.4), if sel { gold } else { Color32::from_gray(110) });
                painter.text(gfx::pos2(card.center().x, art.bottom() + ch * 0.1), gfx::Align2::CENTER_CENTER, tr(en, "ROM HINZUFÜGEN", "ADD ROM"), game((cw * 0.085).clamp(12.0, 24.0)), if sel { Color32::WHITE } else { Color32::from_gray(160) });
            }
            if sel && (t * 3.0) as i64 % 2 == 0 {
                let a = card.left_center() - gfx::vec2(8.0, 0.0);
                painter.add(gfx::Shape::convex_polygon(vec![a, a - gfx::vec2(14.0, 10.0), a - gfx::vec2(14.0, -10.0)], gold, Stroke::NONE));
            }
        }
        painter.text(area.center_bottom() - gfx::vec2(0.0, area.height() * 0.08), gfx::Align2::CENTER_CENTER, tr(en, "◀ ▶ WÄHLEN     A / START SPIELEN", "◀ ▶ SELECT     A / START PLAY"), game(16.0), Color32::from_gray(170));
        ctx.request_repaint();
    }

    // ------------------------------------------------------------ Popups

    fn popups(&mut self, ctx: &gfx::Context) {
        if self.win_map && self.s.map_own_window {
            // Karte als eigenes Windows-Fenster (z. B. auf dem zweiten Bildschirm): zeichnet `shell::MapView`
        } else {
            self.map_focused = false;
            // Karte als Vollbild-Overlay über dem Spiel (wie die Karte im Spiel selbst)
            self.map_overlay(ctx);
        }
        if self.game.is_none() {
            self.win_rando = false;
            self.win_cheats = false;
        }
    }


    // ------------------------------------------------------------ Spielstände

    /// Teleport-Animation: Quadrate im Spielpixel-Raster wachsen von Link aus (passt zu Pixel und HD).
    fn tp_overlay(&self, painter: &gfx::Painter, r: gfx::Rect, ctx: &gfx::Context) {
        let Some(a) = self.tp_anim else { return };
        ctx.request_repaint();
        let t = a.t0.elapsed().as_secs_f32();
        // Abdeckung 0..1: beim Ausblenden steigend, beim Einblenden fallend
        let p = if a.jumped { 1.0 - (t / crate::app::TP_IN).clamp(0.0, 1.0) } else { (t / crate::app::TP_OUT).clamp(0.0, 1.0) };
        let gp = r.width() / self.view_w as f32;
        let ext = (self.view_w as f32 - 256.0) / 2.0;
        let cell = 8.0; // Spielpixel je Quadrat
        let (fx, fy) = (a.from.0 + ext, a.from.1);
        let cols = (self.view_w as f32 / cell).ceil() as i32;
        let rows = (224.0 / cell).ceil() as i32;
        let maxd = ((self.view_w as f32).powi(2) + 224.0f32.powi(2)).sqrt();
        let col = Color32::from_rgb(236, 244, 255);
        for cy in 0..rows {
            for cx in 0..cols {
                let (mx, my) = ((cx as f32 + 0.5) * cell, (cy as f32 + 0.5) * cell);
                let d = ((mx - fx).powi(2) + (my - fy).powi(2)).sqrt() / maxd;
                let local = (p * 1.8 - d * 0.8).clamp(0.0, 1.0);
                if local <= 0.0 {
                    continue;
                }
                // Größe in ganzen Spielpixeln -> bleibt pixelig
                // volle Quadrate leicht überlappen lassen, damit keine Fugen entstehen
                let size = (local * cell).round().max(1.0) * gp + if local >= 0.99 { 1.0 } else { 0.0 };
                let c = gfx::pos2(r.left() + mx * gp, r.top() + my * gp);
                painter.rect_filled(gfx::Rect::from_center_size(c, gfx::vec2(size, size)), 0.0, col);
            }
        }
        // kleiner Funkenring um Link (nur am Anfang des Ausblendens / Ende des Einblendens)
        if p < 0.6 {
            let rad = (1.0 - p) * 18.0 * gp;
            for k in 0..8 {
                let ang = k as f32 * std::f32::consts::TAU / 8.0 + t * 6.0;
                let c = gfx::pos2(r.left() + fx * gp + ang.cos() * rad, r.top() + fy * gp + ang.sin() * rad);
                painter.rect_filled(gfx::Rect::from_center_size(c, gfx::vec2(2.0 * gp, 2.0 * gp)), 0.0, Color32::from_rgb(255, 240, 140));
            }
        }
    }

    /// Text im Spielstil: Pixel-Schrift im Pixel-Modus, sonst Monospace.
    fn game_text(&self, p: &gfx::Painter, pos: gfx::Pos2, align: gfx::Align2, text: &str, size: f32, col: Color32) {
        self.game_text_w(p, pos, align, text, size, col, f32::INFINITY);
    }

    /// wie `game_text`, aber nie breiter als `max_w`
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn game_text_w(&self, p: &gfx::Painter, pos: gfx::Pos2, align: gfx::Align2, text: &str, size: f32, col: Color32, max_w: f32) {
        if self.pixel_ui() {
            let mut gp = (size / 7.0).round().max(1.0);
            if crate::pixfont::smooth() {
                // geglättete Schrift: stufenlos passend statt in ganzen Pixel-Schritten
                gp = (size / 7.0).min(max_w / crate::pixfont::width(text, 1.0).max(1.0)).max(1.0);
            }
            while gp > 1.0 && crate::pixfont::width(text, gp) > max_w {
                gp -= 1.0;
            }
            crate::pixfont::text(p, pos, align, text, gp, col);
        } else {
            let n = text.chars().count().max(1) as f32;
            let size = size.min(max_w / (n * 0.62));
            p.text(pos, align, text, gfx::FontId::monospace(size), col);
        }
    }

    /// Spielstände: Popup in der Bildmitte im Stil der Spielmenüs, nur Controller/Tastatur.
    pub(crate) fn states_overlay(&mut self, ctx: &gfx::Context) {
        if !self.win_states || (self.game.is_none() && !self.gba.active()) {
            // Menü zu: auch die Einstellungsseite schließen (beim nächsten Öffnen wieder die Spielstände)
            self.win_pad_settings = false;
            return;
        }
        let en = self.s.ui_en;
        let screen = if self.game_area.width() > 10.0 { self.game_area } else { ctx.screen_rect() };
        let painter = ctx.layer_painter(gfx::LayerId::background()).with_clip_rect(screen);
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(150));
        let gold = Color32::from_rgb(248, 208, 64);
        let navy = Color32::from_rgb(8, 10, 40);
        // Größe passend zum Fenster: drei Spalten, zwei Reihen, alles innerhalb des Rahmens
        // im Pixel-Modus ist der Rahmen dicker: oben und unten mehr Platz für den Text
        let pixm = self.pixel_ui();
        // Info-Zeile: Nummer/Zeit, zwei Reihen Herzen (wie im Spiel), Ort
        // unten zusätzlich Platz für den Hauptmenü-Knopf
        let menu_h = 50.0;
        let (head, foot, gap, info) = if pixm { (88.0, 78.0 + menu_h + 14.0, 16.0, 80.0) } else { (68.0, 58.0 + menu_h + 14.0, 16.0, 80.0) };
        // Abstand zwischen Rahmen und Kästen
        let side = 38.0;
        let max_w = (screen.width() * 0.92).min(1180.0);
        let max_h = screen.height() * 0.92;
        let by_w = (max_w - 2.0 * side - 2.0 * 20.0) / 3.0;
        // Vorschaubild im Bildformat des Spiels (4:3), mit 6 Pixel Rand
        let by_h = ((max_h - head - foot - gap) / 2.0 - info - 12.0) * 4.0 / 3.0 + 12.0;
        let card_w = by_w.min(by_h).max(90.0);
        let thumb_h = (card_w - 12.0) * 3.0 / 4.0 + 12.0;
        let card_h = thumb_h + info;
        let pw = card_w * 3.0 + 2.0 * 20.0 + 2.0 * side;
        let ph = head + card_h * 2.0 + gap + foot;
        let panel = gfx::Rect::from_center_size(screen.center(), gfx::vec2(pw, ph));
        // Rahmen wie die Spielmenüs (gold), mit runden Ecken
        let _ = navy;
        self.gbox_round(&painter, panel, crate::inventory::GOLDF);
        self.game_text_w(&painter, panel.center_top() + gfx::vec2(0.0, if pixm { 28.0 } else { 18.0 }), gfx::Align2::CENTER_TOP, tr(en, "SPIELSTÄNDE", "SAVE STATES"), 28.0, gold, pw - 60.0);
        let t = ctx.input(|i| i.time) as f32;
        let blink = (t * 3.0) as i64 % 2 == 0;
        for slot in 1..=crate::app::SLOTS {
            let i = (slot - 1) as usize;
            let (cx, cy) = ((i % 3) as f32, (i / 3) as f32);
            let card = gfx::Rect::from_min_size(panel.min + gfx::vec2(side + cx * (card_w + 20.0), head + cy * (card_h + gap)), gfx::vec2(card_w, card_h));
            let sel = self.slot == slot && !self.states_menu;
            let thumb = self.thumb(ctx, slot);
            let meta = if thumb.is_some() { self.slot_meta(slot) } else { None };
            painter.rect_filled(card, 0.0, Color32::from_rgb(0, 0, 12));
            painter.rect_stroke(card, 0.0, Stroke::new(if sel { 3.0_f32 } else { 2.0 }, if sel { gold } else { Color32::WHITE }));
            let tr_ = gfx::Rect::from_min_size(card.min + gfx::vec2(6.0, 6.0), gfx::vec2(card_w - 12.0, thumb_h - 12.0));
            match &thumb {
                Some((tex, _)) => {
                    painter.image(tex.id(), tr_, gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0)), if sel { Color32::WHITE } else { Color32::from_gray(170) });
                }
                None => {
                    painter.rect_filled(tr_, 0.0, Color32::from_rgb(16, 18, 40));
                    self.game_text_w(&painter, tr_.center(), gfx::Align2::CENTER_CENTER, tr(en, "LEER", "EMPTY"), 18.0, Color32::from_gray(110), card_w - 20.0);
                }
            }
            let ty = card.top() + thumb_h - 2.0;
            self.game_text(&painter, gfx::pos2(card.left() + 10.0, ty), gfx::Align2::LEFT_TOP, &format!("{slot}"), 20.0, if sel { gold } else { Color32::WHITE });
            if let Some(m) = &meta {
                self.game_text_w(&painter, gfx::pos2(card.right() - 10.0, ty), gfx::Align2::RIGHT_TOP, &m.time_text(), 16.0, Color32::WHITE, card_w * 0.55);
                // Herzen wie im Spiel: alle, in Reihen zu zehn
                let hearts = (m.max_health / 8).min(20) as usize;
                let full = ((m.health as usize) + 7) / 8;
                // unten eine feste Textzeile (Ort, Tode), darüber die Herzen in zwei Reihen – alles innerhalb der Karte
                let text_size = 14.0;
                let text_y = card.bottom() - 8.0;
                let top = ty + 28.0;
                let avail_h = (text_y - text_size - 6.0 - top).max(10.0);
                // rechts Platz für die Rubine freihalten (Symbol + dreistellige Zahl)
                let rp_txt = m.rupees.map(|rp| format!("{rp:03}"));
                let rp_tw = rp_txt.as_ref().map_or(0.0, |t| if self.pixel_ui() { crate::pixfont::width(t, (text_size / 7.0).round().max(1.0)) } else { text_size * 2.0 });
                let rp_ir = text_size * 0.48;
                let reserve = if rp_txt.is_some() { rp_tw + rp_ir * 2.6 * 0.7 + 12.0 } else { 0.0 };
                let hw_max = ((avail_h / 2.0 - 2.0) * 9.0 / 8.0).min((card_w - 20.0 - reserve) / 10.0 - 2.0).clamp(5.0, 17.0);
                // im Pixel-Stil ganze Bildschirmpixel je Kunstpixel (scharf, kein Überlappen)
                let ppp = ctx.pixels_per_point();
                let hw = if self.smooth_ui() { hw_max } else { ((hw_max * ppp / 9.0).floor().max(1.0) * 9.0) / ppp };
                // wenig Platz (scharfe Pixel lassen sich nicht beliebig verkleinern): engerer Abstand
                let gapx = if 10.0 * (hw + 2.0) > card_w - 20.0 - reserve { 1.0 } else { 2.0 };
                let (stepx, stepy) = (hw + gapx, hw * 8.0 / 9.0 + 2.0);
                let hs = hw / 2.6;
                for k in 0..hearts {
                    let c = gfx::pos2(card.left() + 10.0 + hw * 0.5 + (k % 10) as f32 * stepx, top + stepy * 0.5 + (k / 10) as f32 * stepy);
                    paint_icon(&painter, c, hs, Icon::HeartS, if k < full { HEART } else { GREYC });
                }
                // auf der anderen Seite der Herzen: Rubine (Symbol + Zahl wie in der Spielanzeige)
                if let Some(txt) = &rp_txt {
                    let rx = card.right() - 10.0;
                    let ry = top + stepy;
                    self.game_text_w(&painter, gfx::pos2(rx, ry), gfx::Align2::RIGHT_CENTER, txt, text_size, Color32::WHITE, card_w * 0.25);
                    // Symbol so hoch wie die Zahl, mit etwas Abstand
                    paint_icon(&painter, gfx::pos2(rx - rp_tw - 3.0 - rp_ir * 2.6 * 0.35, ry), rp_ir, Icon::Rupee, Color32::WHITE);
                }
                self.game_text_w(&painter, gfx::pos2(card.right() - 10.0, text_y), gfx::Align2::RIGHT_BOTTOM, &format!("{} {}", tr(en, "TODE", "DEATHS"), m.deaths), text_size, Color32::LIGHT_RED, card_w * 0.32);
                let place = if en { &m.place_en } else { &m.place_de };
                if !place.is_empty() && sel {
                    self.game_text_w(&painter, gfx::pos2(card.left() + 10.0, text_y), gfx::Align2::LEFT_BOTTOM, &place.to_uppercase(), text_size, Color32::from_gray(200), card_w * 0.58);
                }
            }
            if sel && blink {
                let a = card.left_center() - gfx::vec2(4.0, 0.0);
                painter.add(gfx::Shape::convex_polygon(vec![a, a - gfx::vec2(12.0, 9.0), a - gfx::vec2(12.0, -9.0)], gold, Stroke::NONE));
            }
        }
        // zwei große Knöpfe unten: wie ein Spielstand anwählen (↓), ←/→ wechseln, A auswählen
        for k in 0..2u8 {
            let txt = if k == 0 { tr(en, "HAUPTMENÜ", "MAIN MENU") } else { tr(en, "EINSTELLUNGEN", "SETTINGS") };
            let bw = card_w * 1.3;
            let top = panel.top() + head + card_h * 2.0 + gap + 14.0;
            let cx = panel.center().x + if k == 0 { -(bw * 0.5 + 10.0) } else { bw * 0.5 + 10.0 };
            let br = gfx::Rect::from_center_size(gfx::pos2(cx, top + menu_h * 0.5), gfx::vec2(bw, menu_h));
            let (hover, click) = ctx.input(|i| (i.pointer.hover_pos().map_or(false, |h| br.contains(h)), i.pointer.primary_clicked()));
            let chosen = self.states_menu && self.states_menu_sel == k;
            let sel = chosen || hover;
            painter.rect_filled(br, 0.0, Color32::from_rgb(0, 0, 12));
            painter.rect_stroke(br, 0.0, Stroke::new(if sel { 3.0_f32 } else { 2.0 }, if sel { gold } else { Color32::WHITE }));
            self.game_text_w(&painter, br.center(), gfx::Align2::CENTER_CENTER, txt, 22.0, if sel { gold } else { Color32::WHITE }, bw - 24.0);
            if chosen && blink {
                let a = br.left_center() - gfx::vec2(4.0, 0.0);
                painter.add(gfx::Shape::convex_polygon(vec![a, a - gfx::vec2(12.0, 9.0), a - gfx::vec2(12.0, -9.0)], gold, Stroke::NONE));
            }
            if hover && click && !self.win_pad_settings {
                if k == 0 {
                    self.go_main_menu();
                    return;
                }
                self.states_menu = true;
                self.states_menu_sel = 1;
                self.win_pad_settings = true;
            }
        }
        let confirm = self.delete_confirm == Some(self.slot);
        let hint = if confirm {
            tr(en, "X NOCHMAL = LÖSCHEN", "X AGAIN = DELETE").to_string()
        } else {
            tr(en, "A LADEN   Y SPEICHERN   X LÖSCHEN   B ZURÜCK", "A LOAD   Y SAVE   X DELETE   B BACK").to_string()
        };
        self.game_text_w(&painter, panel.center_bottom() - gfx::vec2(0.0, if pixm { 28.0 } else { 18.0 }), gfx::Align2::CENTER_BOTTOM, &hint, 18.0, if confirm { Color32::LIGHT_RED } else { Color32::from_gray(210) }, pw - 60.0);
        // Einstellungen mit dem Controller darüber
        self.pad_settings_overlay(ctx);
        ctx.request_repaint();
    }

    // ------------------------------------------------------------ Karte

    /// Weltkarte im Spiel: besuchte Orte einblenden, L/R wählen, Y = Teleport.
    fn in_game_map_overlay(&mut self, painter: &gfx::Painter, r: gfx::Rect, ui: &gfx::Context) {
        let Some(g) = &self.game else { return };
        if !self.s.map_teleport || !crate::app::in_game_map(&g.snes) || g.snes.bus.ppu.brightness() < 8 {
            return;
        }
        let world = (alttp::ram8(&g.snes, 0x7E008A) >= 0x40) as u8;
        let targets = self.teleport_targets(world);
        let sx = r.width() / self.view_w as f32;
        let sy = r.height() / 224.0;
        let ext = (self.view_w as f32 - 256.0) / 2.0;
        // Welt (Pixel) -> Mode-7-Textur (1:16 ab 128) -> Bildschirm
        // genau das angezeigte Bild verwenden, sonst laufen die Punkte beim Scrollen hinterher
        let lines: &[crate::snes::ppu::Regs] = match &self.snaps[1] {
            Some(sn) if self.s.hi_fps => &sn.lines,
            _ => g.snes.bus.ppu.line_regs(),
        };
        let to_screen = |wx: f32, wy: f32| -> Option<gfx::Pos2> {
            let (px, py) = crate::snes::ppu::m7_project_lines(lines, 128.0 + wx / 16.0, 128.0 + wy / 16.0)?;
            Some(gfx::pos2(r.left() + (px + ext) * sx, r.top() + py * sy))
        };
        // Karte bewegt? -> der Ort am nächsten zur Bildmitte wird ausgewählt
        let view = lines.get(112).map(|l| (l.m7hofs as i32, l.m7vofs as i32, l.m7a as i32)).unwrap_or_default();
        if view != self.igm_view {
            self.igm_view = view;
            let c = r.center();
            if let Some((k, _)) = targets.iter().enumerate().filter_map(|(k, (e, _))| to_screen(e.x, e.y).map(|p| (k, p.distance(c)))).min_by(|a, b| a.1.total_cmp(&b.1)) {
                self.igm_sel = k;
            }
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let clicked = ui.input(|i| i.pointer.primary_clicked());
        let mut click_eid = None;
        let sel = self.igm_sel.min(targets.len().saturating_sub(1));
        let blink = (ui.input(|i| i.time) * 4.0) as i64 % 2 == 0;
        let mut label: Option<(gfx::Pos2, String)> = None;
        for (i, (e, name)) in targets.iter().enumerate() {
            let Some(p) = to_screen(e.x, e.y) else { continue };
            if !r.contains(p) {
                continue;
            }
            let active = i == sel;
            // Punkt in Spielpixeln: schwarzer Rand, innen gold (Auswahl groß und blinkend) bzw. türkis
            let gp = sx.max(1.0);
            let n = if active { 7.0 } else { 3.0 };
            let q = gfx::pos2((p.x / gp).round() * gp, (p.y / gp).round() * gp);
            painter.rect_filled(gfx::Rect::from_center_size(q, gfx::vec2((n + 2.0) * gp, (n + 2.0) * gp)), 0.0, Color32::from_rgb(16, 16, 24));
            let inner = if active { if blink { Color32::from_rgb(248, 216, 64) } else { Color32::from_rgb(248, 248, 248) } } else { Color32::from_rgb(80, 216, 224) };
            painter.rect_filled(gfx::Rect::from_center_size(q, gfx::vec2(n * gp, n * gp)), 0.0, inner);
            let rad = (n + 2.0) * gp * 0.5;
            if active {
                label = Some((q - gfx::vec2(0.0, rad + gp * 3.0), name.clone()));
            }
            if pointer.map_or(false, |pp| pp.distance(p) < rad + 3.0) {
                self.igm_sel = i;
                if clicked {
                    click_eid = Some(*e);
                }
            }
        }
        // Name des ausgewählten Orts im Stil der Spielmenüs
        if let Some((p, name)) = label {
            let gp = sx.max(1.0).round();
            let txt = name.to_uppercase();
            let w = crate::pixfont::width(&txt, gp);
            let br = gfx::Rect::from_center_size(p - gfx::vec2(0.0, gp * 5.5), gfx::vec2(w + gp * 6.0, gp * 11.0));
            let br = br.translate(gfx::vec2((r.left() + 2.0 - br.left()).max(0.0) + (r.right() - 2.0 - br.right()).min(0.0), (r.top() + 2.0 - br.top()).max(0.0)));
            painter.rect_filled(br, 0.0, Color32::from_rgba_unmultiplied(8, 10, 40, 235));
            painter.rect_stroke(br, 0.0, Stroke::new(gp, Color32::WHITE));
            crate::pixfont::text(painter, br.center(), gfx::Align2::CENTER_CENTER, &txt, gp, Color32::WHITE);
        }
        if let Some(ent) = click_eid {
            self.teleport(ent);
        }
    }

}

// ---------------------------------------------------------------- selbst gezeichnete Symbole
// (Emoji aus der Schrift werden nicht überall angezeigt – diese Symbole funktionieren immer)

pub(crate) const HEART: Color32 = Color32::from_rgb(240, 70, 90);
pub(crate) const CHEST: Color32 = Color32::from_rgb(230, 175, 60);
pub(crate) const DUNGEON: Color32 = Color32::from_rgb(185, 140, 255);
pub(crate) const GAME: Color32 = Color32::from_rgb(90, 200, 230);
pub(crate) const GREYC: Color32 = Color32::from_rgb(125, 128, 135);

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum Icon {
    Heart,
    /// grüner Rubin (Spielstände)
    Rupee,
    /// kleines Herz wie in der Spielanzeige (Spielstände)
    HeartS,
    Container,
    Chest,
    Dungeon,
    Star,
    Crystal,
    Pendant,
    Note,
    Link,
    Game,
    Check,
    Dot,
    Door,
    Stairs,
    Bottle,
    Upgrade,
    Quest,
    Question,
    Cross,
    PendG,
    PendB,
    PendR,
    Cryst,
    CrystR,
    MapI,
    Compass,
    BigKey,
    Key,
}

/// Pixel-Symbole im Stil des Spiels (Farbe + ausgegraute Version).
static PIXEL_ICONS: std::sync::OnceLock<Vec<(Icon, gfx::TextureHandle, gfx::TextureHandle, [usize; 2], gfx::TextureHandle, gfx::TextureHandle)>> = std::sync::OnceLock::new();
/// feine Pixel-Symbole (Scale2x) für HD Max: (Symbol, Farbe, grau)
static FINE_ICONS: std::sync::OnceLock<Vec<(Icon, gfx::TextureHandle, gfx::TextureHandle)>> = std::sync::OnceLock::new();
/// HD-Modus: geglättete Symbole statt Pixel
pub(crate) static HD_ICONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const ART: &[(Icon, &[&str])] = &[
    (Icon::Heart, &["..KKK...KKK..", ".KRRRK.KRRRK.", "KRWWRRKRRRRrK", "KRWRRRRRRRRrK", "KRRRRRRRRRRrK", ".KRRRRRRRRrK.", "..KRRRRRRrK..", "...KRRRRrK...", "....KRRrK....", ".....KrK.....", "......K......"]),
    (Icon::Rupee, &["...K...", "..KGK..", ".KGWGK.", "KGWGGgK", "KGWGGgK", "KGWGGgK", "KGGGGgK", ".KGGgK.", "..KgK..", "...K..."]),
    (Icon::HeartS, &[".KKK.KKK.", "KRWRKRRRK", "KWRRRRRrK", "KRRRRRRrK", ".KRRRRrK.", "..KRRrK..", "...KrK...", "....K...."]),
    (Icon::Container, &[".YYY...YYY.", "YRWRY.YRRRY", "YRRRRYRRRRY", "YRRRRRRRRRY", ".YRRRRRRRY.", "..YRRRRRY..", "...YRRRY...", "....YRY....", ".....Y....."]),
    (Icon::Chest, &[".KKKKKKKKKKK.", "KYBBBBBBBBBYK", "KYByyyyyyyBYK", "KYYYYYYYYYYYK", "KKKKKKKKKKKKK", "KYBByKWKyBBYK", "KYByyKYKyyBYK", "KYBBBBBBBBBYK", "KYYYYYYYYYYYK", "KYyyyyyyyyyYK", ".KKKKKKKKKKK."]),
    (Icon::Dungeon, &["KKK.KKK.KKK", "KSK.KSK.KSK", "KSKKKSKKKSK", "KSSSSSSSSSK", "KSsSSSSSsSK", "KSSSKKKSSSK", "KSSKkkkKSSK", "KSSKkkkKSSK", "KSSKkkkKSSK", "KKKKKKKKKKK"]),
    (Icon::Door, &["...KKKKK...", "..KBBBBBK..", ".KBBKKKBBK.", "KBBKkkkKBBK", "KBKkkkkkKBK", "KBKkkkkkKBK", "KBKkkkkkKBK", "KBKkkkkkKBK", "KKKKKKKKKKK"]),
    (Icon::Game, &["..KKK..", ".KGWGK.", "KGWGGgK", "KGGGGgK", "KGGGGgK", "KGGGggK", "KgGGggK", ".KgggK.", "..KKK.."]),
    (Icon::Stairs, &["KKKKKKKKK", "KWWWWWWWK", "KsssssssK", "KWWWWWWWK", "KsssssssK", "KWWWWWWWK", "KsssssssK", "KWWWWWWWK", "KKKKKKKKK"]),
    (Icon::Link, &[".....KKKKK.....", "...KKGGGGGKK...", "..KGWGGGGGGGK..", ".KGGGGGGGGGGGK.", ".KgGGGGGGGGGgK.", "KPgggggggggggPK", "KPPFFFFFFFFFPPK", "KPFFKWFFFWKFFPK", "KPFFKKFFFKKFFPK", ".KFFFFFFFFFFFK.", "..KFFFRRRFFFK..", "...KKFFFFFKK...", ".....KKKKK....."]),
    (Icon::Check, &[".......KK", "......KGK", "KK...KGK.", "KGK.KGK..", ".KGKGK...", "..KGK....", "...K....."]),
    (Icon::Star, &[".....K.....", "....KYK....", "....KWK....", "...KYWYK...", "KKKKYWYOKKK", "KYYYYYYYYOK", ".KYYYYYYOK.", "..KYYYYOK..", "..KYYYYOK..", ".KYYKKKOOK.", ".KYK...KOK.", "KKK.....KKK"]),
    (Icon::Quest, &[".KKKKKKK.", "KOOOWOOOK", "KOOOWOOOK", "KOOOWOOOK", "KOOOOOOOK", "KOOOWOOOK", ".KKKKKKK.", "...KOK...", "....K...."]),
    (Icon::Question, &[".KKKKKKK.", "KUUWWWUUK", "KUWUUUWUK", "KUUUUWUUK", "KUUUWUUUK", "KUUUUUUUK", "KUUUWUUUK", ".KKKKKKK.", "....K...."]),
    (Icon::Cross, &["KK.....KK", "KRK...KRK", ".KRK.KRK.", "..KRKRK..", "...KRK...", "..KRKRK..", ".KRK.KRK.", "KRK...KRK", "KK.....KK"]),
    (Icon::Bottle, &["..KKK..", "..KsK..", "..KWK..", ".KWWWK.", "KWWWWWK", "KUUUUUK", "KUUWUUK", "KUUUUUK", ".KKKKK."]),
    (Icon::Upgrade, &["......KKK", ".....KWWK", "....KWWK.", "...KWWK..", "K.KWWK...", "KKYWK....", ".KYK.....", "KYKKK....", "KK......."]),
    (Icon::PendG, &["...K...", "..KGK..", ".KGGGK.", "KGGWGGK", "KGGGGGK", ".KGGGK.", "..KGK..", "...K..."]),
    (Icon::PendB, &["...K...", "..KUK..", ".KUUUK.", "KUUWUUK", "KUUUUUK", ".KUUUK.", "..KUK..", "...K..."]),
    (Icon::PendR, &["...K...", "..KRK..", ".KRRRK.", "KRRWRRK", "KRRRRRK", ".KRRRK.", "..KRK..", "...K..."]),
    (Icon::Cryst, &["..KKK..", ".KUWUK.", "KUWUUUK", "KUUUUUK", "KUUUUUK", ".KUUUK.", "..KUK..", "...K..."]),
    (Icon::CrystR, &["..KKK..", ".KRWRK.", "KRWRRRK", "KRRRRRK", "KRRRRRK", ".KRRRK.", "..KRK..", "...K..."]),
    (Icon::MapI, &["KKKKKKKKK", "KYYYYYYYK", "KYGGYYUYK", "KYGGGYUYK", "KYYGYUUYK", "KYYYYYUYK", "KYRYYYYYK", "KYYYYYYYK", "KKKKKKKKK"]),
    (Icon::Compass, &["..KKKKK..", ".KWWRWWK.", "KWWWRWWWK", "KWWWRWWWK", "KWWWKWWWK", "KWWWUWWWK", "KWWWUWWWK", ".KWWUWWK.", "..KKKKK.."]),
    (Icon::BigKey, &["..KKK....", ".KYYYK...", "KYKKKYKKK", "KYK.KYYYK", "KYKKKYKYK", ".KYYYK.K.", "..KKK...."]),
    (Icon::Key, &[".KKK.", "KYYYK", "KYKYK", "KYYYK", ".KYK.", ".KYYK", ".KYK.", ".KYYK", "..K.."]),
    (Icon::Note, &["KKKKKKKKK", "KWWWWWWWK", "KWKKKKKWK", "KWWWWWWWK", "KWKKKKWWK", "KWWWWWWWK", "KKKKKKKKK", "....B....", "....B...."]),
];

fn art_color(ch: char) -> Color32 {
    match ch {
        'K' => Color32::from_rgb(16, 16, 24),
        'R' => Color32::from_rgb(232, 40, 56),
        'r' => Color32::from_rgb(160, 16, 40),
        'W' => Color32::from_rgb(248, 248, 248),
        'Y' => Color32::from_rgb(248, 200, 48),
        'y' => Color32::from_rgb(128, 72, 24),
        'S' => Color32::from_rgb(200, 200, 184),
        's' => Color32::from_rgb(120, 120, 112),
        'k' => Color32::from_rgb(40, 24, 24),
        'B' => Color32::from_rgb(168, 112, 64),
        'G' => Color32::from_rgb(72, 200, 72),
        'g' => Color32::from_rgb(24, 120, 40),
        'O' => Color32::from_rgb(240, 140, 40),
        'U' => Color32::from_rgb(72, 144, 240),
        'F' => Color32::from_rgb(248, 184, 136),
        'P' => Color32::from_rgb(232, 112, 160),
        _ => Color32::TRANSPARENT,
    }
}

/// Pixel-Symbole einmal als Texturen anlegen.
pub(crate) fn init_pixel_icons(ctx: &gfx::Context) {
    PIXEL_ICONS.get_or_init(|| {
        ART.iter()
            .map(|(icon, rows)| {
                let w = rows[0].len();
                let h = rows.len();
                let col: Vec<Color32> = rows.iter().flat_map(|r| r.chars().map(art_color)).collect();
                let grey: Vec<Color32> = col
                    .iter()
                    .map(|c| {
                        if c.a() == 0 {
                            *c
                        } else {
                            let l = ((c.r() as u32 * 3 + c.g() as u32 * 6 + c.b() as u32) / 10) as u8;
                            let l = (l as u32 * 7 / 10 + 60) as u8;
                            Color32::from_rgb(l, l, l)
                        }
                    })
                    .collect();
                let mk = |px: Vec<Color32>, n: &str| ctx.load_texture(format!("pix{n}{:?}", *icon as u8), gfx::ColorImage { size: [w, h], pixels: px.clone() }, gfx::TextureOptions::NEAREST);
                // HD-Version: xBRZ 6× – weiche, saubere Rundungen. Durchsichtiges wird erst Schlüsselfarbe,
                // danach wird alles, was (fast) die Schlüsselfarbe hat, wieder durchsichtig; Mischkanten
                // bekommen Teil-Transparenz, damit kein farbiger Saum entsteht.
                let hd = |px: &[Color32], n: &str| {
                    let key = Color32::from_rgb(255, 0, 255);
                    // 1 Pixel Rand, damit am Bildrand nichts abgeschnitten wird
                    let (bw, bh) = (w + 2, h + 2);
                    let src: Vec<Color32> = (0..bh)
                        .flat_map(|y| (0..bw).map(move |x| (x, y)))
                        .map(|(x, y)| if x == 0 || y == 0 || x == bw - 1 || y == bh - 1 { key } else { let c = px[(y - 1) * w + x - 1]; if c.a() == 0 { key } else { c } })
                        .collect();
                    let mut out = Vec::new();
                    crate::filters::xbrz_par(&src, bw, bh, 6, &mut out);
                    let out: Vec<Color32> = out
                        .into_iter()
                        .map(|c| {
                            // Anteil der Schlüsselfarbe schätzen: Magenta hat Rot UND Blau hoch, Grün niedrig
                            // (reines Rot oder Blau zählt so nicht als Schlüsselfarbe)
                            let k = (c.r().min(c.b()) as f32 - c.g() as f32).max(0.0) / 255.0;
                            if c == key || k > 0.92 {
                                Color32::TRANSPARENT
                            } else if k > 0.35 {
                                Color32::from_rgba_unmultiplied(16, 16, 24, ((1.0 - k) * 255.0) as u8)
                            } else {
                                c
                            }
                        })
                        .collect();
                    ctx.load_texture(format!("hd{n}{:?}", *icon as u8), gfx::ColorImage { size: [bw * 6, bh * 6], pixels: out }, gfx::TextureOptions::LINEAR)
                };
                let (hc, hg) = (hd(&col, "c"), hd(&grey, "g"));
                (*icon, mk(col, "c"), mk(grey, "g"), [w, h], hc, hg)
            })
            .collect()
    });
    FINE_ICONS.get_or_init(|| {
        ART.iter()
            .map(|(icon, rows)| {
                let w = rows[0].len();
                let h = rows.len();
                let col: Vec<Color32> = rows.iter().flat_map(|r| r.chars().map(art_color)).collect();
                let grey: Vec<Color32> = col
                    .iter()
                    .map(|c| {
                        if c.a() == 0 {
                            *c
                        } else {
                            let l = ((c.r() as u32 * 3 + c.g() as u32 * 6 + c.b() as u32) / 10) as u8;
                            let l = (l as u32 * 7 / 10 + 60) as u8;
                            Color32::from_rgb(l, l, l)
                        }
                    })
                    .collect();
                let mk = |px: &[Color32], n: &str| ctx.load_texture(format!("fine{n}{:?}", *icon as u8), gfx::ColorImage { size: [w * 2, h * 2], pixels: crate::filters::epx2x(px, w, h, Color32::TRANSPARENT) }, gfx::TextureOptions::NEAREST);
                (*icon, mk(&col, "c"), mk(&grey, "g"))
            })
            .collect()
    });
}

/// Triforce (drei Dreiecke) mit dunklem Rand – Hauptmenü und App-Symbol
pub(crate) fn paint_triforce(p: &gfx::Painter, c: gfx::Pos2, r: f32, col: Color32) {
    let h = r * 3f32.sqrt();
    let top = c + gfx::vec2(0.0, -h);
    let tri = |a: gfx::Pos2, s: f32| vec![a, a + gfx::vec2(s * 0.5, s * 0.866), a + gfx::vec2(-s * 0.5, s * 0.866)];
    let s = r * 2.0 * 0.98;
    let dark = Color32::from_rgb(40, 28, 6);
    for (a, _) in [(top, 0), (top + gfx::vec2(-r, h), 1), (top + gfx::vec2(r, h), 2)] {
        let pts = tri(a + gfx::vec2(0.0, r * 0.02), s);
        p.add(gfx::Shape::convex_polygon(pts.clone(), col, Stroke::new(r * 0.05, dark)));
        // Glanz auf der linken Kante
        p.add(gfx::Shape::line_segment([pts[0], pts[2]], Stroke::new(r * 0.04, Color32::from_rgba_unmultiplied(255, 250, 200, 140))));
    }
}

pub(crate) fn paint_icon(p: &gfx::Painter, c: gfx::Pos2, r: f32, icon: Icon, col: Color32) {
    use gfx::{pos2, vec2, Shape};
    if let Some((_, tc, tg, [w, h], hc, hg)) = PIXEL_ICONS.get().and_then(|v| v.iter().find(|e| e.0 as u8 == icon as u8)) {
        let uv = gfx::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        if HD_ICONS.load(std::sync::atomic::Ordering::Relaxed) {
            // Textur hat 1 Pixel Rand ringsum
            let k = (r * 2.6) / *w.max(h) as f32;
            let rect = gfx::Rect::from_center_size(c, vec2((*w + 2) as f32 * k, (*h + 2) as f32 * k));
            p.image(if col == GREYC { hg.id() } else { hc.id() }, rect, uv, Color32::WHITE);
            return;
        }
        // ganze Pixel-Vielfache, damit es scharf bleibt
        let ppp = p.ctx().pixels_per_point();
        if crate::pixfont::fine() {
            // feine Pixel: Scale2x-Version mit halb so großen Pixeln
            if let Some((_, fc, fg)) = FINE_ICONS.get().and_then(|v| v.iter().find(|e| e.0 as u8 == icon as u8)) {
                let k = (((r * 2.6) / (2 * *w.max(h)) as f32) * ppp).round().max(1.0) / ppp;
                let size = vec2(*w as f32 * 2.0 * k, *h as f32 * 2.0 * k);
                let rect = gfx::Rect::from_center_size(c, size);
                let rect = gfx::Rect::from_min_size(pos2((rect.left() * ppp).round() / ppp, (rect.top() * ppp).round() / ppp), size);
                p.image(if col == GREYC { fg.id() } else { fc.id() }, rect, uv, Color32::WHITE);
                return;
            }
        }
        let k = (((r * 2.6) / *w.max(h) as f32) * ppp).round().max(1.0) / ppp;
        let size = vec2(*w as f32 * k, *h as f32 * k);
        let rect = gfx::Rect::from_center_size(c, size);
        let rect = gfx::Rect::from_min_size(pos2((rect.left() * ppp).round() / ppp, (rect.top() * ppp).round() / ppp), size);
        p.image(if col == GREYC { tg.id() } else { tc.id() }, rect, uv, Color32::WHITE);
        return;
    }
    let no = Stroke::NONE;
    match icon {
        Icon::Heart | Icon::HeartS | Icon::Rupee | Icon::Container => {
            let rr = if matches!(icon, Icon::Container) { r * 1.1 } else { r * 0.9 };
            p.circle_filled(c + vec2(-rr * 0.45, -rr * 0.22), rr * 0.5, col);
            p.circle_filled(c + vec2(rr * 0.45, -rr * 0.22), rr * 0.5, col);
            p.add(Shape::convex_polygon(vec![c + vec2(-rr * 0.93, -rr * 0.05), c + vec2(rr * 0.93, -rr * 0.05), c + vec2(0.0, rr * 0.95)], col, no));
            if matches!(icon, Icon::Container) {
                p.circle_filled(c + vec2(-rr * 0.4, -rr * 0.35), rr * 0.15, Color32::from_white_alpha(200));
            }
        }
        Icon::Chest => {
            let body = gfx::Rect::from_center_size(c + vec2(0.0, r * 0.1), vec2(r * 1.8, r * 1.3));
            p.rect_filled(body, r * 0.2, col);
            p.line_segment([pos2(body.left(), c.y - r * 0.15), pos2(body.right(), c.y - r * 0.15)], Stroke::new(r * 0.18, Color32::from_black_alpha(160)));
            p.rect_filled(gfx::Rect::from_center_size(c + vec2(0.0, r * 0.05), vec2(r * 0.35, r * 0.45)), 1.0, Color32::from_black_alpha(170));
        }
        Icon::Dungeon => {
            let body = gfx::Rect::from_min_max(c + vec2(-r * 0.85, -r * 0.35), c + vec2(r * 0.85, r * 0.85));
            p.rect_filled(body, 0.0, col);
            for k in [-0.85f32, -0.18, 0.5] {
                p.rect_filled(gfx::Rect::from_min_size(c + vec2(r * k, -r * 0.8), vec2(r * 0.35, r * 0.45)), 0.0, col);
            }
            p.rect_filled(gfx::Rect::from_center_size(c + vec2(0.0, r * 0.45), vec2(r * 0.45, r * 0.8)), r * 0.2, Color32::from_black_alpha(180));
        }
        Icon::Star => {
            let pts: Vec<gfx::Pos2> = (0..10)
                .map(|k| {
                    let a = std::f32::consts::PI * (k as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
                    let rr = if k % 2 == 0 { r } else { r * 0.45 };
                    c + vec2(a.cos() * rr, a.sin() * rr)
                })
                .collect();
            for k in 0..5 {
                p.add(Shape::convex_polygon(vec![c, pts[(2 * k + 9) % 10], pts[2 * k], pts[2 * k + 1]], col, no));
            }
        }
        Icon::Crystal => {
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, -r), c + vec2(r * 0.65, 0.0), c + vec2(0.0, r), c + vec2(-r * 0.65, 0.0)], col, no));
        }
        Icon::Pendant => {
            p.circle_stroke(c + vec2(0.0, -r * 0.55), r * 0.35, Stroke::new(r * 0.18, col));
            p.circle_filled(c + vec2(0.0, r * 0.25), r * 0.6, col);
        }
        Icon::Note => {
            let b = gfx::Rect::from_center_size(c, vec2(r * 1.4, r * 1.7));
            p.rect_filled(b, r * 0.15, col);
            for k in 0..3 {
                let y = b.top() + r * (0.45 + k as f32 * 0.4);
                p.line_segment([pos2(b.left() + r * 0.25, y), pos2(b.right() - r * 0.25, y)], Stroke::new(r * 0.12, Color32::from_black_alpha(170)));
            }
        }
        Icon::Link => {
            p.circle_filled(c, r, col);
            p.circle_stroke(c, r, Stroke::new(2.0_f32, Color32::WHITE));
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, -r * 0.6), c + vec2(r * 0.5, r * 0.4), c + vec2(-r * 0.5, r * 0.4)], Color32::WHITE, no));
        }
        Icon::Game => {
            p.circle_filled(c, r * 0.9, col);
            p.text(c, gfx::Align2::CENTER_CENTER, "?", gfx::FontId::proportional(r * 1.4), Color32::BLACK);
        }
        Icon::Check => {
            let s = Stroke::new(r * 0.35, col);
            p.line_segment([c + vec2(-r * 0.7, 0.0), c + vec2(-r * 0.2, r * 0.55)], s);
            p.line_segment([c + vec2(-r * 0.2, r * 0.55), c + vec2(r * 0.8, -r * 0.6)], s);
        }
        Icon::Bottle | Icon::Upgrade | Icon::Quest | Icon::Question | Icon::Cross | Icon::PendG | Icon::PendB | Icon::PendR | Icon::Cryst | Icon::CrystR | Icon::MapI | Icon::Compass | Icon::BigKey | Icon::Key => {
            p.circle_filled(c, r * 0.8, col);
        }
        Icon::Dot => {
            p.circle_stroke(c, r * 0.5, Stroke::new(r * 0.2, col));
        }
        Icon::Stairs => {
            for k in 0..4 {
                let y = c.y - r * 0.8 + k as f32 * r * 0.45;
                p.line_segment([pos2(c.x - r * 0.8, y), pos2(c.x + r * 0.8, y)], Stroke::new(r * 0.2, col));
            }
        }
        Icon::Door => {
            // Torbogen
            let body = gfx::Rect::from_min_max(c + vec2(-r * 0.7, -r * 0.2), c + vec2(r * 0.7, r * 0.9));
            p.rect_filled(body, 0.0, col);
            p.circle_filled(c + vec2(0.0, -r * 0.2), r * 0.7, col);
            p.rect_filled(gfx::Rect::from_min_max(c + vec2(-r * 0.35, -r * 0.1), c + vec2(r * 0.35, r * 0.9)), 0.0, Color32::from_black_alpha(200));
            p.circle_filled(c + vec2(0.0, -r * 0.1), r * 0.35, Color32::from_black_alpha(200));
        }
    }
}



pub(crate) struct ItemOpt {
    pub(crate) addr: u32,
    pub(crate) de: &'static str,
    pub(crate) en: &'static str,
    pub(crate) opts: &'static [(u8, &'static str, &'static str)],
}

const ONOFF: &[(u8, &str, &str)] = &[(0, "–", "–"), (1, "Ja", "Yes")];
const BOTTLE: &[(u8, &str, &str)] = &[(0, "–", "–"), (2, "Leer", "Empty"), (3, "Roter Trank", "Red potion"), (4, "Grüner Trank", "Green potion"), (5, "Blauer Trank", "Blue potion"), (6, "Fee", "Fairy"), (7, "Biene", "Bee"), (8, "Goldbiene", "Good bee")];

pub(crate) const ITEMS: &[ItemOpt] = &[
    ItemOpt { addr: 0x7EF359, de: "Schwert", en: "Sword", opts: &[(0, "–", "–"), (1, "Kämpferschwert", "Fighter"), (2, "Master-Schwert", "Master"), (3, "Gehärtet", "Tempered"), (4, "Gold", "Gold")] },
    ItemOpt { addr: 0x7EF35A, de: "Schild", en: "Shield", opts: &[(0, "–", "–"), (1, "Holzschild", "Fighter"), (2, "Feuerschild", "Fire"), (3, "Spiegelschild", "Mirror")] },
    ItemOpt { addr: 0x7EF35B, de: "Rüstung", en: "Mail", opts: &[(0, "Grün", "Green"), (1, "Blau", "Blue"), (2, "Rot", "Red")] },
    ItemOpt { addr: 0x7EF340, de: "Bogen", en: "Bow", opts: &[(0, "–", "–"), (2, "Bogen", "Bow"), (4, "Silberpfeile", "Silver arrows")] },
    ItemOpt { addr: 0x7EF341, de: "Bumerang", en: "Boomerang", opts: &[(0, "–", "–"), (1, "Blau", "Blue"), (2, "Magisch (rot)", "Magic (red)")] },
    ItemOpt { addr: 0x7EF342, de: "Enterhaken", en: "Hookshot", opts: ONOFF },
    ItemOpt { addr: 0x7EF344, de: "Pilz / Puder", en: "Mushroom / Powder", opts: &[(0, "–", "–"), (1, "Pilz", "Mushroom"), (2, "Zauberpuder", "Magic powder")] },
    ItemOpt { addr: 0x7EF345, de: "Feuerstab", en: "Fire Rod", opts: ONOFF },
    ItemOpt { addr: 0x7EF346, de: "Eisstab", en: "Ice Rod", opts: ONOFF },
    ItemOpt { addr: 0x7EF347, de: "Bombos", en: "Bombos", opts: ONOFF },
    ItemOpt { addr: 0x7EF348, de: "Ether", en: "Ether", opts: ONOFF },
    ItemOpt { addr: 0x7EF349, de: "Quake", en: "Quake", opts: ONOFF },
    ItemOpt { addr: 0x7EF34A, de: "Laterne", en: "Lamp", opts: ONOFF },
    ItemOpt { addr: 0x7EF34B, de: "Hammer", en: "Hammer", opts: ONOFF },
    ItemOpt { addr: 0x7EF34C, de: "Schaufel / Flöte", en: "Shovel / Flute", opts: &[(0, "–", "–"), (1, "Schaufel", "Shovel"), (2, "Flöte", "Flute"), (3, "Flöte (aktiv)", "Flute (active)")] },
    ItemOpt { addr: 0x7EF34D, de: "Käfernetz", en: "Bug Net", opts: ONOFF },
    ItemOpt { addr: 0x7EF34E, de: "Buch Mudora", en: "Book of Mudora", opts: ONOFF },
    ItemOpt { addr: 0x7EF350, de: "Somaria-Stab", en: "Cane of Somaria", opts: ONOFF },
    ItemOpt { addr: 0x7EF351, de: "Byrna-Stab", en: "Cane of Byrna", opts: ONOFF },
    ItemOpt { addr: 0x7EF352, de: "Tarnumhang", en: "Magic Cape", opts: ONOFF },
    ItemOpt { addr: 0x7EF353, de: "Zauberspiegel", en: "Magic Mirror", opts: &[(0, "–", "–"), (2, "Ja", "Yes")] },
    ItemOpt { addr: 0x7EF354, de: "Handschuhe", en: "Gloves", opts: &[(0, "–", "–"), (1, "Krafthandschuh", "Power Glove"), (2, "Titanhandschuh", "Titan's Mitt")] },
    ItemOpt { addr: 0x7EF355, de: "Pegasusstiefel", en: "Pegasus Boots", opts: ONOFF },
    ItemOpt { addr: 0x7EF356, de: "Zora-Flossen", en: "Zora's Flippers", opts: ONOFF },
    ItemOpt { addr: 0x7EF357, de: "Mondperle", en: "Moon Pearl", opts: ONOFF },
    ItemOpt { addr: 0x7EF37B, de: "Magieverbrauch", en: "Magic usage", opts: &[(0, "Normal", "Normal"), (1, "Halb", "Half"), (2, "Viertel", "Quarter")] },
    ItemOpt { addr: 0x7EF35C, de: "Flasche 1", en: "Bottle 1", opts: BOTTLE },
    ItemOpt { addr: 0x7EF35D, de: "Flasche 2", en: "Bottle 2", opts: BOTTLE },
    ItemOpt { addr: 0x7EF35E, de: "Flasche 3", en: "Bottle 3", opts: BOTTLE },
    ItemOpt { addr: 0x7EF35F, de: "Flasche 4", en: "Bottle 4", opts: BOTTLE },
];

impl App {
    /// Schnellmenü (Item-Rad) über allem zeichnen.
    pub(crate) fn draw_quickmenu(&mut self, ctx: &gfx::Context) {
        // fertige Hintergrund-Aufnahme der Symbole übernehmen
        let done = self.qm.job.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(r) => Some(r),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
            Err(_) => None,
        });
        if let Some(res) = done {
            self.qm.job = None;
            match res {
                Some(icons) => {
                    let hd = |px: &[Color32], name: String| {
                        let mut out = Vec::new();
                        crate::filters::xbr4x_mode(px, quickmenu::ICON, quickmenu::ICON, &mut out, false);
                        ctx.load_texture(name, gfx::ColorImage { size: [quickmenu::ICON * 4; 2], pixels: out }, gfx::TextureOptions::LINEAR)
                    };
                    let raw = |px: &[Color32], name: String| ctx.load_texture(name, gfx::ColorImage { size: [quickmenu::ICON; 2], pixels: px.to_vec() }, gfx::TextureOptions::NEAREST);
                    // feine Pixel (HD Max): Scale2x
                    let fine = |px: &[Color32], w: usize, h: usize, name: String| ctx.load_texture(name, gfx::ColorImage { size: [w * 2, h * 2], pixels: crate::filters::epx2x(px, w, h, Color32::TRANSPARENT) }, gfx::TextureOptions::NEAREST);
                    for (idx, icon) in icons.items.iter().enumerate() {
                        self.qm.icons[idx] = icon.as_ref().map(|px| hd(px, format!("qm{idx}")));
                        self.qm.icons_raw[idx] = icon.as_ref().map(|px| raw(px, format!("qmr{idx}")));
                        self.qm.icons_fine[idx] = icon.as_ref().map(|px| fine(px, quickmenu::ICON, quickmenu::ICON, format!("qmf{idx}")));
                        self.qm.icons_px[idx] = icon.clone();
                    }
                    for (k, w, h, px) in &icons.equip {
                        self.qm.equip_raw.insert(*k, ctx.load_texture(format!("eqr{k}"), gfx::ColorImage { size: [*w, *h], pixels: px.clone() }, gfx::TextureOptions::NEAREST));
                        self.qm.equip_fine.insert(*k, fine(px, *w, *h, format!("eqf{k}")));
                        let mut out = Vec::new();
                        crate::filters::xbr4x_mode(px, *w, *h, &mut out, false);
                        self.qm.equip_hd.insert(*k, ctx.load_texture(format!("eqh{k}"), gfx::ColorImage { size: [*w * 4, *h * 4], pixels: out }, gfx::TextureOptions::LINEAR));
                    }
                    for (content, px) in &icons.bottles {
                        let t = hd(px, format!("qmb{content}"));
                        self.qm.bottle_icons.insert(*content, t);
                        self.qm.bottle_raw.insert(*content, raw(px, format!("qmbr{content}")));
                        self.qm.bottle_fine.insert(*content, fine(px, quickmenu::ICON, quickmenu::ICON, format!("qmbf{content}")));
                        self.qm.bottle_px.insert(*content, px.clone());
                    }
                }
                None => {
                    // fehlgeschlagen (z. B. Übergang): später nochmal versuchen
                    self.qm.cooldown = 120;
                    self.qm.sig.clear();
                }
            }
        }
        if !self.qm.open || self.game.is_none() {
            return;
        }
        let en = self.s.ui_en;
        let g = self.game.as_ref().unwrap();
        let painter = ctx.layer_painter(gfx::LayerId::new(gfx::Order::Foreground, gfx::Id::new("quickmenu")));
        let screen = ctx.screen_rect();
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(120));
        let n = self.qm.entries.len().max(1);
        let sel = self.qm.sel.unwrap_or(0).min(n - 1);
        // Leiste läuft weich zur Auswahl
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        self.qm.scroll += (sel as f32 - self.qm.scroll) * (1.0 - (-dt * 18.0).exp());
        if (self.qm.scroll - sel as f32).abs() > n as f32 / 2.0 {
            self.qm.scroll = sel as f32;
        }
        let pix = self.pixel_ui();
        let gp = self.game_px.max(1.0).round();
        let tile = if pix { gp * 24.0 } else { (screen.height() * 0.13).clamp(56.0, 104.0) };
        let gap = if pix { gp * 4.0 } else { tile * 0.22 };
        let step = tile + gap;
        let cy = screen.top() + screen.height() * 0.40;
        let band = gfx::Rect::from_min_max(gfx::pos2(screen.left(), cy - tile * 0.95), gfx::pos2(screen.right(), cy + tile * 1.15));
        let green = Color32::from_rgb(56, 176, 72);
        let gold = Color32::from_rgb(248, 208, 64);
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        // Leiste im Stil des Spielmenüs: dunkelblau mit weißer und grüner Doppellinie
        painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(8, 10, 40, 225));
        let lw = if pix { gp } else { 2.0 };
        for (y, c, w) in [(band.top() + lw * 1.5, Color32::WHITE, lw), (band.top() + lw * 4.0, green, lw), (band.bottom() - lw * 1.5, Color32::WHITE, lw), (band.bottom() - lw * 4.0, green, lw)] {
            painter.line_segment([gfx::pos2(band.left(), y), gfx::pos2(band.right(), y)], Stroke::new(w, c));
        }
        let pointer = ctx.pointer_latest_pos();
        let mut hovered = None;
        // wenige Items: alle mittig; viele: Auswahl in der Mitte, Leiste scrollt
        let fits = n as f32 * step < screen.width() * 0.9;
        let center = if fits { (n as f32 - 1.0) / 2.0 } else { self.qm.scroll };
        let font = |s: f32| gfx::FontId::monospace(s);
        for (i, e) in self.qm.entries.iter().enumerate() {
            let mut k = i as f32 - center;
            if !fits {
                // im Kreis herum, damit links und rechts immer Items stehen
                let nf = n as f32;
                k = (k + nf / 2.0).rem_euclid(nf) - nf / 2.0;
            }
            let x = screen.center().x + k * step;
            if x < screen.left() - tile || x > screen.right() + tile {
                continue;
            }
            let is_sel = i == sel;
            let size = if is_sel { if pix { tile + gp * 4.0 } else { tile * 1.18 } } else { tile };
            let mut r = gfx::Rect::from_center_size(gfx::pos2(x, cy), gfx::vec2(size, size));
            if pix {
                r = gfx::Rect::from_min_size(gfx::pos2((r.left() / gp).round() * gp, (r.top() / gp).round() * gp), r.size());
            }
            let (rd, sw) = if pix { (0.0, gp) } else { (4.0, 2.0) };
            painter.rect_filled(r, rd, Color32::from_rgb(0, 0, 8));
            painter.rect_stroke(r, rd, Stroke::new(sw, Color32::WHITE));
            painter.rect_stroke(r.shrink(if pix { gp * 2.0 } else { 4.0 }), rd / 2.0, Stroke::new(if is_sel && !pix { 3.0_f32 } else { sw }, if is_sel { gold } else { green }));
            let icon = if pix { self.qm.icon_raw(&g.snes, *e) } else { self.qm.icon(&g.snes, *e) };
            match icon {
                Some(t) => {
                    let ir = if pix { gfx::Rect::from_center_size(r.center(), gfx::vec2(gp * 20.0, gp * 20.0)) } else { r.shrink(size * 0.12) };
                    painter.image(t.id(), ir, uv, if is_sel { Color32::WHITE } else { Color32::from_gray(200) });
                }
                None => {
                    // Symbol wird noch aufgenommen: kleines Fragezeichen statt Text
                    painter.text(r.center(), gfx::Align2::CENTER_CENTER, "?", font(size * 0.4), Color32::from_gray(150));
                }
            }
            if let Some(b) = e.bottle {
                painter.text(r.right_bottom() - gfx::vec2(7.0, 5.0), gfx::Align2::RIGHT_BOTTOM, b.to_string(), font(13.0), gold);
            }
            if pointer.map_or(false, |p| r.contains(p)) {
                hovered = Some(i);
            }
        }
        // für welchen Slot wird gewählt (Y oder X)
        {
            let (lbl, col) = if self.qm_for_x { ("X", Color32::from_rgb(40, 90, 200)) } else { ("Y", Color32::from_rgb(40, 150, 60)) };
            let bs = if pix { gp * 12.0 } else { tile * 0.42 };
            let b = gfx::Rect::from_min_size(gfx::pos2(band.left() + bs * 0.5, band.top() + bs * 0.5), gfx::vec2(bs, bs));
            painter.rect_filled(b, if pix { 0.0 } else { 5.0 }, col);
            painter.rect_stroke(b, if pix { 0.0 } else { 5.0 }, Stroke::new(if pix { gp } else { 2.0 }, Color32::WHITE));
            crate::pixfont::text(&painter, b.center(), gfx::Align2::CENTER_CENTER, lbl, if pix { gp * 1.0 } else { (bs / 9.0).round().max(1.0) }, Color32::WHITE);
        }
        // Auswahl-Pfeile wie im Spiel
        let sr = gfx::Rect::from_center_size(gfx::pos2(screen.center().x + (if fits { sel as f32 - center } else { 0.0 }) * step, cy), gfx::vec2(tile * 1.18, tile * 1.18));
        let t = ctx.input(|i| i.time) as f32;
        let bob = (t * 6.0).sin() * 3.0;
        for (dir, xx) in [(-1.0f32, sr.left() - 12.0 + bob), (1.0, sr.right() + 12.0 - bob)] {
            let tip = gfx::pos2(xx + dir * 8.0, cy);
            painter.add(gfx::Shape::convex_polygon(vec![tip, gfx::pos2(xx, cy - 9.0), gfx::pos2(xx, cy + 9.0)], gold, Stroke::NONE));
        }
        if let Some(e) = self.qm.entries.get(sel) {
            let name = quickmenu::name(&g.snes, *e, en);
            if pix {
                crate::pixfont::text(&painter, gfx::pos2(screen.center().x, band.bottom() + gp * 6.0), gfx::Align2::CENTER_TOP, &name, gp, Color32::WHITE);
            } else {
                painter.text(gfx::pos2(screen.center().x, band.bottom() + 18.0), gfx::Align2::CENTER_TOP, name, font((tile * 0.27).clamp(16.0, 26.0)), Color32::WHITE);
            }
        }
        if let Some(i) = hovered {
            if ctx.input(|i| i.pointer.primary_clicked()) {
                self.qm.sel = Some(i);
                self.qm_confirm();
            }
        }
        ctx.request_repaint();
    }
}
pub(crate) const DOOR: Color32 = Color32::from_rgb(90, 200, 210);

/// Dungeon, in dem Link gerade ist (Index wie `world::DMAP_NAMES`), sonst None (Oberwelt, Höhle, Haus).
pub(crate) fn current_dungeon(s: &crate::snes::Snes) -> Option<u8> {
    let d = alttp::ram8(s, 0x7E040C);
    (alttp::ram8(s, 0x7E001B) != 0 && d < 0x1C).then_some(d / 2)
}

impl App {
    /// Position einer Mod-Einstellung auf dem Spielbild ziehen: Maus gedrückt halten setzt sie (die Mod zeichnet
    /// gleich an der neuen Stelle), Enter/Esc/Rechtsklick beendet, Entf nimmt den Standardplatz der Mod.
    /// `r` = Spielbild auf dem Schirm, `view_w`×`view_h` Spielpixel, `ext` = Breitbild-Rand links
    pub(crate) fn mod_place_overlay(&mut self, p: &gfx::Painter, ctx: &gfx::Context, r: gfx::Rect, view_w: usize, view_h: usize, ext: usize) {
        let Some((id, key)) = self.mod_place.clone() else { return };
        let en = self.s.ui_en;
        let (kx, ky) = (r.width() / view_w.max(1) as f32, r.height() / view_h.max(1) as f32);
        let (pos, down, done, reset) = ctx.input(|i| {
            (i.pointer.hover_pos(), i.pointer.primary_down(), i.key_down(gfx::Key::Enter) || i.key_down(gfx::Key::Escape), i.key_down(gfx::Key::Delete))
        });
        let mut changed = false;
        if let (Some(m), true) = (pos, down) {
            if r.contains(m) {
                let gx = ((m.x - r.left()) / kx).round() as i32 - ext as i32;
                let gy = ((m.y - r.top()) / ky).round() as i32;
                self.s.mod_values.entry(id.clone()).or_default().insert(key.clone(), crate::mods::api::setting::pos(gx, gy));
                changed = true;
            }
        }
        if reset {
            if let Some(v) = self.s.mod_values.get_mut(&id) {
                v.remove(&key);
            }
            changed = true;
        }
        if changed {
            self.mods.set_values(&self.s.mod_values);
        }
        // Markierung an der gewählten Stelle
        if let Some(&v) = self.s.mod_values.get(&id).and_then(|m| m.get(&key)) {
            let (x, y) = (v as u32 as u16 as i16 as f32, (v as u32 >> 16) as u16 as i16 as f32);
            let c = gfx::pos2(r.left() + (x + ext as f32) * kx, r.top() + y * ky);
            let gold = Color32::from_rgb(248, 208, 64);
            p.line_segment([c - gfx::vec2(10.0, 0.0), c + gfx::vec2(10.0, 0.0)], Stroke::new(2.0f32, gold));
            p.line_segment([c - gfx::vec2(0.0, 10.0), c + gfx::vec2(0.0, 10.0)], Stroke::new(2.0f32, gold));
        }
        let txt = tr(
            en,
            "Position ziehen: Maus gedrückt halten und an die Stelle ziehen · Enter/Esc: fertig · Entf: Standard",
            "Drag the position: hold the mouse and drag it into place · Enter/Esc: done · Del: default",
        );
        let font = gfx::FontId::proportional(15.0);
        let w = (txt.chars().count() as f32 * 7.6).min(r.width() - 20.0);
        let b = gfx::Rect::from_center_size(gfx::pos2(r.center().x, r.top() + 24.0), gfx::vec2(w + 24.0, 30.0));
        p.rect_filled(b, 6.0, Color32::from_rgba_unmultiplied(8, 10, 40, 230));
        p.rect_stroke(b, 6.0, Stroke::new(1.0f32, Color32::from_rgb(248, 208, 64)));
        p.text(b.center(), gfx::Align2::CENTER_CENTER, txt, font, Color32::WHITE);
        if done {
            self.mod_place = None;
            self.win_mods = true;
            self.win_mod_settings = Some(id);
        }
    }
}
