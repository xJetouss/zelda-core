//! GBA-Modus in der App: Spiel starten/beenden, Bilder takten, Bild zeigen, Spielstände, Karte im Hauptmenü.
//! Die Menüs dazu stehen in `menus.rs` (Teil der Menüleiste in `shell.rs`).

use super::Gba;
use crate::app::App;
use crate::gfx::{self, Color32, Stroke};
use crate::i18n::tr;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Zustand des GBA-Modus in der App
pub struct GbaView {
    pub game: Option<Gba>,
    tex: Option<gfx::TextureHandle>,
    acc: f64,
    last: Instant,
    /// Fenster „Cheat-Liste“
    pub win_cheats: bool,
    /// Eingabefelder im Cheat-Fenster
    pub cheat_name: String,
    pub cheat_code: String,
    /// Titelbilder für die Karten im Hauptmenü
    cards: HashMap<PathBuf, Option<gfx::TextureHandle>>,
    abuf: Vec<i16>,
    /// Bild nach Farbtabelle (Sättigung, Gamma …), vorheriges Bild (Überblenden), Mischbild
    cur: Vec<Color32>,
    prev: Vec<Color32>,
    blend: Vec<Color32>,
    /// unscharfes Hintergrundbild für Breitbild (Rand links/rechts)
    back: Option<gfx::TextureHandle>,
    /// Zurückspulen: Spielstände (lz4), Zähler
    hist: VecDeque<Vec<u8>>,
    tick: u32,
    /// neues Bild seit dem letzten Hochladen
    dirty: bool,
    /// Spielanzeige sichtbar (Y-Feld nur dann zeigen und Symbole nur dann aufnehmen)
    hud: bool,
    /// The Minish Cap: Symbole der Items (wie im B-Feld), leeres Y-Feld, Item-Leiste für die Y-Taste
    icons: HashMap<u8, gfx::TextureHandle>,
    y_empty: Option<gfx::TextureHandle>,
    pub bar_open: bool,
    bar_items: Vec<u8>,
    bar_sel: usize,
    bar_prev: u16,
    bar_stick: bool,
}

impl Default for GbaView {
    fn default() -> Self {
        GbaView {
            game: None,
            tex: None,
            acc: 0.0,
            last: Instant::now(),
            win_cheats: false,
            cheat_name: String::new(),
            cheat_code: String::new(),
            cards: HashMap::new(),
            abuf: Vec::with_capacity(4096),
            cur: Vec::new(),
            prev: Vec::new(),
            blend: Vec::new(),
            back: None,
            hist: VecDeque::new(),
            tick: 0,
            dirty: false,
            hud: false,
            icons: HashMap::new(),
            y_empty: None,
            bar_open: false,
            bar_items: Vec::new(),
            bar_sel: 0,
            bar_prev: 0,
            bar_stick: false,
        }
    }
}

impl GbaView {
    pub fn active(&self) -> bool {
        self.game.is_some()
    }
}

/// Alle GBA-ROMs neben der .exe
pub fn roms_next_to_exe() -> Vec<PathBuf> {
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) else { return Vec::new() };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| super::is_gba(p)).collect()).unwrap_or_default();
    v.sort();
    v
}

impl App {
    /// GBA-Spiel starten (ein laufendes SNES-Spiel wird vorher geschlossen)
    pub(crate) fn gba_open(&mut self, path: PathBuf) {
        if !super::available() {
            let m = self.t("GBA ist in diesem Build nicht enthalten (mGBA ließ sich beim Bauen nicht übersetzen).", "GBA is not included in this build.");
            self.msg(m);
            return;
        }
        self.close_game();
        self.gba.game = None;
        match Gba::load_with(&path, self.s.gba_rc, self.s.save_sram) {
            Ok(mut g) => {
                // Cheats/Y-Item aus den Einstellungen; ältere Fassungen legten sie neben die ROM (einmal übernehmen,
                // danach entfernen – gespeichert wird nur noch über Savestates und die Einstellungen der App)
                let key = gba_key(&path);
                match self.s.gba_games.get(&key) {
                    Some(c) => g.cfg = c.clone(),
                    None => {
                        self.s.gba_games.insert(key, g.cfg.clone());
                    }
                }
                let _ = std::fs::remove_file(super::cfg_path(&path));
                let _ = std::fs::remove_file(path.with_extension("title.rgba"));
                g.save_sav = self.s.save_sram;
                g.apply_cheats();
                self.gba.game = Some(g);
                self.gba.acc = 0.0;
                self.gba.last = Instant::now();
                self.gba.tex = None;
                self.paused = false;
                // Entwicklung: Cheat-Liste gleich öffnen (PASTLINK_OPEN=gbacheats)
                self.gba.win_cheats = std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "gbacheats");
                if std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "states") {
                    self.win_states = true;
                }
                // Entwicklung: Spielstand gleich laden (PASTLINK_GBA_LOAD=Datei)
                if let Some(b) = std::env::var_os("PASTLINK_GBA_LOAD").and_then(|f| std::fs::read(f).ok()) {
                    if let Some(g) = self.gba.game.as_mut() {
                        g.load_state(&b);
                    }
                }
                self.s.last_gba = path.to_string_lossy().to_string();
                let m = format!("{}: {}", self.t("Gestartet", "Started"), super::display_name(&path));
                self.msg(m);
            }
            Err(e) => self.msg(e),
        }
    }

    /// GBA-Spiel beenden (Spielstand des Spiels wird geschrieben)
    pub(crate) fn gba_close(&mut self) {
        self.gba.game = None;
        self.gba.win_cheats = false;
        self.gba.tex = None;
        self.gba.back = None;
        self.gba.hist.clear();
        self.gba.icons.clear();
        self.gba.y_empty = None;
        self.gba.bar_open = false;
        self.thumbs.clear();
    }

    /// ein Anzeige-Bild im GBA-Modus: Spielbilder nach Zeit (oder zurückspulen), Ton, Bild hochladen.
    /// `blocked`: ein Menü ist offen (Spiel steht)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn gba_frame(&mut self, ctx: &gfx::Context, pad: u16, turbo: bool, double: bool, focused: bool, rewind: bool, blocked: bool) {
        let now = Instant::now();
        let dt = (now - self.gba.last).as_secs_f64().min(0.1);
        self.gba.last = now;
        self.auto_paused = self.s.auto_pause && !focused;
        let blocked = blocked || self.paused || self.auto_paused || self.gba.win_cheats || (self.s.pause_in_menus && self.popup_open());
        let speed = if turbo { self.s.turbo_speed as f64 } else if double { 2.0 * self.s.speed as f64 } else { self.s.speed as f64 };
        let (pad, blocked) = self.gba_item_bar(ctx, pad, blocked);
        // wie bei A Link to the Past: Anzeige „TURBO“ / „×2“ / „Zurückspulen“ solange gehalten
        self.turbo_on = turbo;
        self.double_on = double && !turbo;
        self.rewind_on = rewind && self.s.rewind;
        let rewinding = self.rewind_on;
        let max_hist = (self.s.rewind_seconds as usize * 30).max(30);
        let save_sram = self.s.save_sram;
        let Some(g) = self.gba.game.as_mut() else { return };
        g.save_sav = save_sram;
        if g.cfg_dirty {
            g.cfg_dirty = false;
            let key = gba_key(&g.path);
            self.s.gba_games.insert(key, g.cfg.clone());
        }
        let mut n = 0;
        if blocked {
            self.gba.acc = 0.0;
        } else {
            self.gba.acc += dt * g.fps * if rewinding { 1.0 } else { speed };
            n = (self.gba.acc.floor() as u32).min(16);
            self.gba.acc -= n as f64;
            if n == 16 {
                self.gba.acc = 0.0;
            }
        }
        // Pause: „Einzelbild vor“
        if self.paused && self.advance {
            self.advance = false;
            n = 1;
        }
        if rewinding {
            // je Spielbild einen Stand zurück (gesichert alle 2 Bilder), dann diesen Stand zeigen
            for _ in 0..n {
                if self.gba.hist.len() > 1 {
                    self.gba.hist.pop_back();
                }
            }
            if n > 0 {
                if let Some(raw) = self.gba.hist.back().and_then(|b| lz4_flex::decompress_size_prepended(b).ok()) {
                    g.load_state(&raw);
                    g.run_frame(0);
                }
                let mut skip = Vec::new();
                g.take_audio(&mut skip);
            }
        } else {
            for _ in 0..n {
                g.run_frame(pad);
                self.gba.tick = self.gba.tick.wrapping_add(1);
                // wie bei ALttP: alle 2 Bilder sichern, im Turbo/×2 alle 8
                if self.s.rewind && self.gba.tick % (if turbo || double { 8 } else { 2 }) == 0 {
                    if let Some(st) = g.save_state() {
                        self.gba.hist.push_back(lz4_flex::compress_prepend_size(&st));
                        while self.gba.hist.len() > max_hist {
                            self.gba.hist.pop_front();
                        }
                    }
                }
            }
        }
        if !self.s.rewind {
            self.gba.hist.clear();
        }
        self.gba.hud = g.hud_visible();
        self.game_fps_count += n;
        // Ton: nur bei normalem Tempo (sonst würde er sich stauen)
        self.gba.abuf.clear();
        g.take_audio(&mut self.gba.abuf);
        let audio_ok = !self.s.mute && !(turbo && self.s.mute_turbo) && (speed - 1.0).abs() < 0.01 && !rewinding;
        if let Some(a) = &self.audio {
            a.set_params(self.s.volume, self.s.latency_ms);
            if audio_ok && !blocked {
                a.push(&self.gba.abuf);
            }
        }
        if g.new_frame {
            g.new_frame = false;
            self.gba.dirty = true;
        }
        self.gba_upload(ctx);
    }

    /// The Minish Cap: Item-Leiste für die Y-Taste (Schnellmenü-Taste halten, ←/→ oder L/R wählen, loslassen =
    /// auf Y legen). Liefert die Tasten fürs Spiel und ob das Spiel steht.
    fn gba_item_bar(&mut self, ctx: &gfx::Context, pad: u16, blocked: bool) -> (u16, bool) {
        // Entwicklung: Item-Leiste offen zeigen (PASTLINK_OPEN=gbabar)
        let held = (self.qm_held || std::env::var("PASTLINK_OPEN").is_ok_and(|v| v == "gbabar")) && !blocked;
        let dir_x = self.qm_dir.x;
        let Some(g) = self.gba.game.as_mut() else { return (pad, blocked) };
        if !g.is_minish() {
            return (pad, blocked);
        }
        // Symbole aus dem Spiel aufnehmen (einmal je Item; Flaschen jedes Mal neu, ihr Inhalt wechselt)
        let mut new_icons: Vec<(u8, Vec<u8>)> = Vec::new();
        let hud = self.gba.hud;
        if self.gba.y_empty.is_none() && hud {
            if let Some(mut img) = g.capture_icon(0) {
                letter_y(&mut img);
                new_icons.push((0, img));
            }
        }
        if held && !self.gba.bar_open && hud {
            self.gba.bar_items = g.owned_items();
            for &i in &self.gba.bar_items {
                if !self.gba.icons.contains_key(&i) || (28..=31).contains(&i) {
                    if let Some(img) = g.capture_icon(i) {
                        new_icons.push((i, img));
                    }
                }
            }
            self.gba.bar_sel = self.gba.bar_items.iter().position(|&i| i == g.cfg.y_item).unwrap_or(0);
            self.gba.bar_open = true;
            self.gba.bar_prev = pad;
        }
        let y = g.cfg.y_item;
        if y != 0 && !self.gba.icons.contains_key(&y) && hud {
            if let Some(img) = g.capture_icon(y) {
                new_icons.push((y, img));
            }
        }
        let mut out = (pad, blocked);
        if self.gba.bar_open {
            let n = self.gba.bar_items.len();
            let pressed = pad & !self.gba.bar_prev;
            self.gba.bar_prev = pad;
            let stick = if dir_x > 0.6 { 1 } else if dir_x < -0.6 { -1 } else { 0 };
            let step = if pressed & (0x0200 | 0x0020) != 0 || (stick < 0 && !self.gba.bar_stick) {
                -1
            } else if pressed & (0x0100 | 0x0010) != 0 || (stick > 0 && !self.gba.bar_stick) {
                1
            } else {
                0
            };
            self.gba.bar_stick = stick != 0;
            if n > 0 && step != 0 {
                self.gba.bar_sel = (self.gba.bar_sel as i32 + step).rem_euclid(n as i32) as usize;
            }
            if !held {
                self.gba.bar_open = false;
                if let Some(&i) = self.gba.bar_items.get(self.gba.bar_sel) {
                    g.cfg.y_item = i;
                    g.save_cfg();
                }
            }
            out = (0, true);
        }
        for (i, img) in new_icons {
            let t = ctx.load_texture(format!("gba-icon-{i}"), gfx::ColorImage::from_rgba_unmultiplied([super::ICON_W, super::ICON_H], &img), gfx::TextureOptions::NEAREST);
            if i == 0 {
                self.gba.y_empty = Some(t);
            } else {
                self.gba.icons.insert(i, t);
            }
        }
        out
    }

    /// Y-Feld neben B und offene Item-Leiste zeichnen (The Minish Cap)
    fn gba_draw_y(&self, painter: &gfx::Painter, r: gfx::Rect, gp: f32) {
        let Some(g) = &self.gba.game else { return };
        if !g.is_minish() || (!self.gba.hud && !self.gba.bar_open) {
            return;
        }
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        let at = |x: f32, y: f32| gfx::pos2(r.left() + x * gp, r.top() + y * gp);
        let size = gfx::vec2(super::ICON_W as f32 * gp, super::ICON_H as f32 * gp);
        let tex = if g.cfg.y_item != 0 { self.gba.icons.get(&g.cfg.y_item).or(self.gba.y_empty.as_ref()) } else { self.gba.y_empty.as_ref() };
        if let Some(t) = tex {
            painter.image(t.id(), gfx::Rect::from_min_size(at((super::ICON_X + g.hud_dx() - 24) as f32, super::ICON_Y as f32), size), uv, Color32::WHITE);
        }
        if !self.gba.bar_open {
            return;
        }
        // Item-Leiste unten in der Bildmitte, im Stil der Spielmenüs
        let n = self.gba.bar_items.len().max(1);
        let cell = 28.0 * gp;
        let w = n as f32 * cell + 12.0 * gp;
        let h = cell + 26.0 * gp;
        let panel = gfx::Rect::from_min_size(gfx::pos2(r.center().x - w / 2.0, r.bottom() - h - 8.0 * gp), gfx::vec2(w, h));
        painter.rect_filled(panel, 4.0 * gp, Color32::from_rgba_unmultiplied(8, 10, 40, 230));
        painter.rect_stroke(panel, 4.0 * gp, Stroke::new(gp, Color32::from_rgb(248, 208, 64)));
        if self.gba.bar_items.is_empty() {
            crate::pixfont::text(painter, panel.center(), gfx::Align2::CENTER_CENTER, if self.s.ui_en { "NO ITEMS YET" } else { "NOCH KEINE ITEMS" }, gp.round().max(1.0), Color32::WHITE);
            return;
        }
        for (k, &i) in self.gba.bar_items.iter().enumerate() {
            let c = gfx::Rect::from_min_size(gfx::pos2(panel.left() + 6.0 * gp + k as f32 * cell, panel.top() + 4.0 * gp), gfx::vec2(cell, cell));
            if k == self.gba.bar_sel {
                painter.rect_filled(c.shrink(gp), 3.0 * gp, Color32::from_rgba_unmultiplied(248, 208, 64, 70));
                painter.rect_stroke(c.shrink(gp), 3.0 * gp, Stroke::new(gp, Color32::from_rgb(248, 208, 64)));
            }
            if let Some(t) = self.gba.icons.get(&i) {
                let ic = gfx::Rect::from_center_size(c.center(), gfx::vec2(super::ICON_W as f32 * gp, super::ICON_H as f32 * gp));
                painter.image(t.id(), ic, uv, Color32::WHITE);
            }
        }
        if let Some(&i) = self.gba.bar_items.get(self.gba.bar_sel) {
            let txt = super::minish::name(i, self.s.ui_en).to_uppercase();
            crate::pixfont::text(painter, gfx::pos2(panel.center().x, panel.bottom() - 11.0 * gp), gfx::Align2::CENTER_CENTER, &txt, gp.round().max(1.0), Color32::WHITE);
        }
    }

    /// Bild aufbereiten wie bei A Link to the Past: Farbtabelle (Sättigung, Gamma, Farb-Randomizer), Überblenden,
    /// Bildfilter (HD auf der Grafikkarte oder auf der CPU)
    fn gba_upload(&mut self, ctx: &gfx::Context) {
        self.ensure_lut();
        let Some(g) = &self.gba.game else { return };
        let (w, h) = (g.w, g.h);
        let blending = self.s.frame_blend && !self.paused;
        if !self.gba.dirty && !blending && (self.gba.tex.is_some() || self.gpu_mode().is_some()) {
            return;
        }
        if self.gba.dirty {
            self.gba.dirty = false;
            std::mem::swap(&mut self.gba.prev, &mut self.gba.cur);
            let lut = &self.lut;
            self.gba.cur.clear();
            self.gba.cur.extend(g.frame.chunks_exact(4).map(|p| lut[((p[0] >> 3) as usize) | ((p[1] >> 3) as usize) << 5 | ((p[2] >> 3) as usize) << 10]));
            if self.gba.prev.len() != self.gba.cur.len() {
                self.gba.prev.clone_from(&self.gba.cur);
            }
            // Hintergrund für Breitbild: das rohe Bild, weich gezeichnet
            if self.s.widescreen != 0 && !self.s.gba_stretch {
                let img = gfx::ColorImage { size: [w, h], pixels: self.gba.cur.clone() };
                match &mut self.gba.back {
                    Some(t) => t.set(img, gfx::TextureOptions::LINEAR),
                    None => self.gba.back = Some(ctx.load_texture("gba-back", img, gfx::TextureOptions::LINEAR)),
                }
            }
        }
        // noch kein Bild (Recomp startet noch) oder Größe passt nicht: nichts hochladen
        if self.gba.cur.len() != w * h || self.gba.prev.len() != w * h {
            return;
        }
        let src: Vec<Color32> = if blending {
            let a = (self.gba.acc.clamp(0.0, 1.0) * 256.0) as u32;
            let mix = |x: u8, y: u8| ((x as u32 * (256 - a) + y as u32 * a) >> 8) as u8;
            self.gba.prev.iter().zip(self.gba.cur.iter()).map(|(p, c)| Color32::from_rgb(mix(p.r(), c.r()), mix(p.g(), c.g()), mix(p.b(), c.b()))).collect()
        } else {
            self.gba.cur.clone()
        };
        if self.gpu_mode().is_some() {
            self.gpu.set_frame(&src, w, h);
            return;
        }
        let size = crate::filters::apply(self.s.filter, &src, w, h, &mut self.ftmp, &mut self.fout);
        let img = gfx::ColorImage { size, pixels: std::mem::take(&mut self.fout) };
        let opts = self.s.filter.texture_options();
        match &mut self.gba.tex {
            Some(t) => t.set(img, opts),
            None => self.gba.tex = Some(ctx.load_texture("gba", img, opts)),
        }
    }

    /// Spielbild im GBA-Modus (3:2, auf Wunsch ganzzahlig vergrößert oder gestreckt)
    pub(crate) fn gba_view(&mut self, painter: &gfx::Painter, area: gfx::Rect, ctx: &gfx::Context) {
        let en = self.s.ui_en;
        // Recomp (Picori): die Breite der Spielansicht folgt dem gewählten Seitenverhältnis (echtes Breitbild)
        if let Some(g) = self.gba.game.as_mut() {
            let (vw, vh) = match self.s.widescreen {
                1 => (16, 9),
                2 => (21, 9),
                3 => (area.width().max(1.0) as i32, area.height().max(1.0) as i32),
                _ => (3, 2),
            };
            g.set_view(vw, vh, self.s.widescreen != 0);
            if g.loading() {
                let gp = (area.height() / 160.0).round().max(1.0);
                let txt = tr(en, "RECOMP STARTET ... (BEIM ERSTEN MAL ETWA 20 S)", "RECOMP STARTING ... (ABOUT 20 S THE FIRST TIME)");
                crate::pixfont::text(painter, area.center(), gfx::Align2::CENTER_CENTER, txt, gp, Color32::WHITE);
                return;
            }
        }
        let Some(g) = &self.gba.game else { return };
        let (gw, gh) = (g.w as f32, g.h as f32);
        let uv = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(1.0, 1.0));
        // Bildfläche: 16:9 / 21:9 / Fenster füllen wie bei A Link to the Past (das GBA-Bild selbst bleibt 3:2,
        // der Rand zeigt das Spielbild weich und dunkel – oder „strecken“ zieht das Bild auf die ganze Fläche)
        let frame_aspect = match self.s.widescreen {
            1 => Some(16.0 / 9.0),
            2 => Some(21.0 / 9.0),
            3 => Some(area.width() / area.height().max(1.0)),
            _ => None,
        };
        let fit = |aspect: f32| {
            let (w, h) = if area.width() / area.height() > aspect { (area.height() * aspect, area.height()) } else { (area.width(), area.width() / aspect) };
            gfx::Rect::from_center_size(area.center(), gfx::vec2(w, h))
        };
        let frame = frame_aspect.map(fit);
        let r = if self.s.gba_stretch {
            frame.unwrap_or(area)
        } else {
            let outer = frame.unwrap_or(area);
            let aspect = gw / gh;
            let (mut w, mut h) = if outer.width() / outer.height() > aspect { (outer.height() * aspect, outer.height()) } else { (outer.width(), outer.width() / aspect) };
            if self.s.integer_scale {
                let ppp = ctx.pixels_per_point();
                let k = ((h * ppp) / gh).floor().max(1.0);
                h = k * gh / ppp;
                w = h * aspect;
            }
            gfx::Rect::from_center_size(outer.center(), gfx::vec2(w, h))
        };
        if let (Some(f), Some(b), false) = (frame, &self.gba.back, self.s.gba_stretch) {
            // Rand: Spielbild vergrößert, abgedunkelt
            let zoom = gfx::Rect::from_center_size(f.center(), gfx::vec2(f.width().max(f.height() * gw / gh), f.width().max(f.height() * gw / gh) * gh / gw));
            painter.with_clip_rect(f).image(b.id(), zoom, uv, Color32::from_gray(70));
        }
        if let Some(m) = self.gpu_mode() {
            // HD-Filter auf der Grafikkarte: genau in Bildschirmpixeln berechnen und 1:1 zeigen
            let ppp = ctx.pixels_per_point();
            let (x0, y0) = ((r.left() * ppp).round(), (r.top() * ppp).round());
            let (x1, y1) = ((r.right() * ppp).round(), (r.bottom() * ppp).round());
            let snapped = gfx::Rect::from_min_max(gfx::pos2(x0 / ppp, y0 / ppp), gfx::pos2(x1 / ppp, y1 / ppp));
            if let Some(img) = self.gpu.render(m, (x1 - x0) as i32, (y1 - y0) as i32) {
                painter.render_image(img, snapped);
            }
        } else if let Some(t) = &self.gba.tex {
            painter.image(t.id(), r, uv, Color32::WHITE);
        }
        let gp_game = r.width() / gw;
        self.gba_draw_y(painter, r, gp_game);
        let gold = Color32::from_rgb(235, 200, 90);
        let ind = if self.turbo_on {
            Some(format!("⏩ TURBO ×{:.0}", self.s.turbo_speed))
        } else if self.double_on {
            Some("⏩ ×2".to_string())
        } else if self.rewind_on {
            Some(tr(en, "⏪ Zurückspulen", "⏪ Rewind").to_string())
        } else {
            None
        };
        // wie bei A Link to the Past: Pixel-Schrift in einem Kasten unten rechts, „PAUSE“ groß in der Mitte
        let gp = gp_game.max(1.0).round();
        if let Some(txt) = ind {
            let tr_ = gfx::pos2(r.right() - gp * 8.0, r.bottom() - gp * 18.0);
            let w = crate::pixfont::width(&txt, gp);
            let br = gfx::Rect::from_min_size(gfx::pos2(tr_.x - w - gp * 4.0, tr_.y - gp * 3.0), gfx::vec2(w + gp * 8.0, gp * 13.0));
            painter.rect_filled(br, 0.0, Color32::from_rgb(8, 10, 40));
            painter.rect_stroke(br, 0.0, Stroke::new(gp, Color32::WHITE));
            crate::pixfont::text(painter, tr_, gfx::Align2::RIGHT_TOP, &txt, gp, gold);
        }
        if self.paused || self.auto_paused {
            crate::pixfont::text(painter, area.center(), gfx::Align2::CENTER_CENTER, tr(en, "PAUSE", "PAUSED"), (gp_game * 2.0).round().max(2.0), Color32::WHITE);
        }
        if self.s.show_fps {
            painter.text(gfx::pos2(r.left() + 10.0, r.top() + 8.0), gfx::Align2::LEFT_TOP, format!("{:.0} FPS · {} {:.0}", self.fps, tr(en, "Spiel", "game"), self.game_fps), gfx::FontId::monospace(14.0), Color32::YELLOW);
        }
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
    }

    /// Spielstand speichern (Platz 1..9), mit Vorschaubild für das Spielstand-Menü
    pub(crate) fn gba_save_slot(&mut self, slot: u8) {
        let Some(g) = &self.gba.game else { return };
        let rc = g.is_rc();
        // Picori-Stände (alle Spielvariablen, gut 1 MB) gepackt speichern
        let r = g
            .save_state()
            .ok_or_else(|| "Kern liefert keinen Spielstand".to_string())
            .and_then(|b| std::fs::write(super::state_path(&g.path, slot, rc), if rc { lz4_flex::compress_prepend_size(&b) } else { b }).map_err(|e| e.to_string()));
        if let (true, Some(t)) = (r.is_ok(), g.thumb()) {
            let _ = std::fs::write(g.path.with_extension(format!("{}{slot}.thumb", if rc { "rs" } else { "gs" })), lz4_flex::compress_prepend_size(&t));
        }
        self.slot = slot;
        self.thumbs.remove(&slot);
        let m = match r {
            Ok(()) => format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("gespeichert", "saved")),
            Err(e) => format!("{}: {e}", self.t("Speichern fehlgeschlagen", "Saving failed")),
        };
        self.msg(m);
    }

    /// Spielstand laden (Platz 1..9)
    pub(crate) fn gba_load_slot(&mut self, slot: u8) {
        self.slot = slot;
        let Some(g) = &mut self.gba.game else { return };
        let rc = g.is_rc();
        let ok = std::fs::read(super::state_path(&g.path, slot, rc))
            .ok()
            .and_then(|b| if rc { lz4_flex::decompress_size_prepended(&b).ok() } else { Some(b) })
            .map(|b| g.load_state(&b))
            .unwrap_or(false);
        if ok {
            self.gba.hist.clear();
            g.run_frame(0);
            let mut skip = Vec::new();
            g.take_audio(&mut skip);
        }
        let m = if ok { format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("geladen", "loaded")) } else { format!("{} {slot} {}", self.t("Spielstand", "Save state"), self.t("ist leer", "is empty")) };
        self.msg(m);
    }

    /// Zeit des Spielstands (für das Menü), None = leer
    pub(crate) fn gba_slot_time(&self, slot: u8) -> Option<String> {
        let g = self.gba.game.as_ref()?;
        let t = std::fs::metadata(super::state_path(&g.path, slot, g.is_rc())).ok()?.modified().ok()?;
        let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
        Some(crate::gba::ui::local_time(secs))
    }

    /// Titelbild eines GBA-Spiels für die Karte im Hauptmenü (beim ersten Mal kurz das Spiel laufen lassen)
    pub(crate) fn gba_card(&mut self, path: &Path, ctx: &gfx::Context) -> Option<gfx::TextureHandle> {
        if let Some(t) = self.gba.cards.get(path) {
            return t.clone();
        }
        #[cfg(pastlink_gba)]
        let t = super::title_image(path).map(|(px, w, h)| ctx.load_texture(format!("gba-card-{}", path.display()), gfx::ColorImage::from_rgba_unmultiplied([w, h], &px), gfx::TextureOptions::NEAREST));
        #[cfg(not(pastlink_gba))]
        let t = {
            let _ = ctx;
            None
        };
        self.gba.cards.insert(path.to_path_buf(), t.clone());
        t
    }
}

/// Ortszeit als „TT.MM. HH:MM“ (Windows: Zeitzone aus dem System)
pub fn local_time(secs: i64) -> String {
    let off = utc_offset_secs();
    let t = secs + off;
    let days = t.div_euclid(86400);
    let rem = t.rem_euclid(86400);
    // Datum aus Tagen seit 1970 (Algorithmus von H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{d:02}.{m:02}. {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

fn utc_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Tzi {
            bias: i32,
            _rest: [u8; 168],
        }
        extern "system" {
            fn GetTimeZoneInformation(tz: *mut Tzi) -> u32;
        }
        let mut tz = Tzi { bias: 0, _rest: [0; 168] };
        let r = unsafe { GetTimeZoneInformation(&mut tz) };
        // r == 2: Sommerzeit aktiv (DaylightBias steht bei Offset 168 innerhalb von _rest: Bias(4)+StdName(64)+StdDate(16)+StdBias(4)+DltName(64)+DltDate(16) = 168)
        let dlt_bias = i32::from_le_bytes([tz._rest[164], tz._rest[165], tz._rest[166], tz._rest[167]]);
        let std_bias = i32::from_le_bytes([tz._rest[80], tz._rest[81], tz._rest[82], tz._rest[83]]);
        let b = tz.bias + if r == 2 { dlt_bias } else { std_bias };
        -(b as i64) * 60
    }
    #[cfg(not(windows))]
    0
}

/// Leeres B-Feld in ein Y-Feld verwandeln: den Buchstaben im Kreis durch ein „Y“ ersetzen (gleiche Farben)
pub(crate) fn letter_y(img: &mut [u8]) {
    let (w, h) = (super::ICON_W, super::ICON_H);
    let px = |img: &[u8], x: usize, y: usize| -> [u8; 4] { let i = (y * w + x) * 4; [img[i], img[i + 1], img[i + 2], img[i + 3]] };
    let lum = |p: [u8; 4]| p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32;
    // Kreis: sichtbare Pixel in der unteren Hälfte
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in h / 2..h {
        for x in 0..w {
            if px(img, x, y)[3] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 <= x0 + 4 || y1 <= y0 + 4 {
        return;
    }
    let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    let rad = ((x1 - x0) as f32 / 2.0).min((y1 - y0) as f32 / 2.0) - 2.0;
    // Füllfarbe = hellste häufige Farbe innen, Buchstabe = dunkle Pixel innen
    let inner: Vec<(usize, usize)> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() < rad).collect();
    let fill = inner.iter().map(|&(x, y)| px(img, x, y)).filter(|p| p[3] != 0).max_by_key(|&p| lum(p));
    let Some(fill) = fill else { return };
    let letter: Vec<(usize, usize)> = inner.iter().copied().filter(|&(x, y)| { let p = px(img, x, y); p[3] != 0 && lum(p) + 200 < lum(fill) }).collect();
    let Some(lcol) = letter.iter().map(|&(x, y)| px(img, x, y)).min_by_key(|&p| lum(p)) else { return };
    let (lx0, ly0) = (letter.iter().map(|p| p.0).min().unwrap(), letter.iter().map(|p| p.1).min().unwrap());
    let (lx1, ly1) = (letter.iter().map(|p| p.0).max().unwrap(), letter.iter().map(|p| p.1).max().unwrap());
    for &(x, y) in &letter {
        let i = (y * w + x) * 4;
        img[i..i + 4].copy_from_slice(&fill);
    }
    // „Y“ in der Größe des alten Buchstabens
    let (lw, lh) = (lx1 - lx0 + 1, ly1 - ly0 + 1);
    let mid = lx0 + lw / 2;
    let fork = (lh / 2).max(1);
    for y in 0..lh {
        let xs: Vec<usize> = if y < fork {
            let off = ((fork - y) as f32 * (lw as f32 / 2.0) / fork as f32).round() as usize;
            vec![mid.saturating_sub(off), (mid + off).min(lx1)]
        } else {
            vec![mid]
        };
        for x in xs {
            let i = ((ly0 + y) * w + x) * 4;
            img[i..i + 4].copy_from_slice(&lcol);
        }
    }
}

/// Schlüssel eines GBA-Spiels in den Einstellungen
fn gba_key(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}
