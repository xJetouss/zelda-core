//! Einstellungen mit dem Controller: aus dem Spielstand-Menü („EINSTELLUNGEN“) erreichbar.
//! Hoch/Runter wählen, Links/Rechts ändern, A umschalten, B/Start zurück. Ändert dieselben Einstellungen wie die Menüs der App.

use crate::app::App;
use crate::filters::Filter;
use crate::gfx::{self, Color32};
use crate::settings::Settings;

type GetB = fn(&Settings) -> bool;
type SetB = fn(&mut Settings, bool);
type GetN = fn(&Settings) -> f64;
type SetN = fn(&mut Settings, f64);

enum Kind {
    /// Schalter (an/aus); `rom`: ROM danach neu aufbauen (Spiel-Verbesserungen)
    Bool(GetB, SetB, bool),
    /// Zahl (min, max, Schritt, Anzeige)
    Num(GetN, SetN, f64, f64, f64, fn(f64) -> String),
    /// Auswahl aus mehreren Werten (Index lesen/setzen, Namen deutsch/englisch)
    Choice(fn(&Settings) -> usize, fn(&mut Settings, usize), &'static [(&'static str, &'static str)]),
    /// Überschrift (nicht wählbar)
    Head,
}

struct Item {
    de: &'static str,
    en: &'static str,
    kind: Kind,
}

const FILTERS: &[(&str, &str)] = &[("Original", "Original"), ("HD weich", "HD soft"), ("HD Super", "HD super"), ("HD Max", "HD max"), ("HD Kanten", "HD sharp")];
const WIDE: &[(&str, &str)] = &[("4:3", "4:3"), ("16:9", "16:9"), ("21:9", "21:9"), ("Fenster füllen", "Fill window")];
const LANG: &[(&str, &str)] = &[("Deutsch", "German"), ("Englisch", "English")];

macro_rules! b {
    ($de:expr, $en:expr, $f:ident) => {
        Item { de: $de, en: $en, kind: Kind::Bool(|s| s.$f, |s, v| s.$f = v, false) }
    };
    ($de:expr, $en:expr, $f:ident, rom) => {
        Item { de: $de, en: $en, kind: Kind::Bool(|s| s.$f, |s, v| s.$f = v, true) }
    };
}

fn head(de: &'static str, en: &'static str) -> Item {
    Item { de, en, kind: Kind::Head }
}

fn items() -> Vec<Item> {
    vec![
        head("BILD", "PICTURE"),
        Item {
            de: "Bildmodus",
            en: "Picture mode",
            kind: Kind::Choice(|s| Filter::ALL.iter().position(|f| *f == s.filter).unwrap_or(0), |s, i| {
                s.filter = Filter::ALL[i];
                if s.filter.is_hd() {
                    s.last_hd = s.filter;
                }
            }, FILTERS),
        },
        Item { de: "Breitbild", en: "Widescreen", kind: Kind::Choice(|s| s.widescreen.min(3) as usize, |s, i| s.widescreen = i as u8, WIDE) },
        b!("Ganzzahlig skalieren", "Integer scaling", integer_scale),
        b!("4:3-Seitenverhältnis", "4:3 aspect", aspect_43),
        b!("Bilder überblenden", "Frame blending", frame_blend),
        b!("Zwischenbilder (über 60 FPS)", "In-between frames (above 60 FPS)", hi_fps),
        b!("FPS anzeigen", "Show FPS", show_fps),
        b!("Rand neu berechnen", "Recompute edges", edge_recomp),
        Item { de: "Sättigung", en: "Saturation", kind: Kind::Num(|s| s.saturation as f64, |s, v| s.saturation = v as f32, 0.0, 2.0, 0.05, |v| format!("{v:.2}")) },
        Item { de: "Gamma", en: "Gamma", kind: Kind::Num(|s| s.gamma as f64, |s, v| s.gamma = v as f32, 0.5, 2.0, 0.05, |v| format!("{v:.2}")) },
        b!("Zufällige Farben", "Random colors", color_rando),
        head("TON", "SOUND"),
        Item { de: "Lautstärke", en: "Volume", kind: Kind::Num(|s| s.volume as f64, |s, v| s.volume = v as f32, 0.0, 1.5, 0.05, |v| format!("{:.0} %", v * 100.0)) },
        b!("Stumm", "Mute", mute),
        b!("Hall", "Echo", echo),
        b!("Stumm bei Turbo", "Mute while turbo", mute_turbo),
        Item { de: "Latenz", en: "Latency", kind: Kind::Num(|s| s.latency_ms as f64, |s, v| s.latency_ms = v.round() as u32, 30.0, 250.0, 10.0, |v| format!("{} ms", v as u32)) },
        head("SPIEL", "GAME"),
        Item { de: "Turbo-Tempo", en: "Turbo speed", kind: Kind::Num(|s| s.turbo_speed as f64, |s, v| s.turbo_speed = v as f32, 2.0, 10.0, 0.5, |v| format!("{v:.1}×")) },
        Item { de: "Run-Ahead", en: "Run-ahead", kind: Kind::Num(|s| s.runahead as f64, |s, v| s.runahead = v.round() as u8, 0.0, 3.0, 1.0, |v| format!("{}", v as u32)) },
        b!("Zurückspulen", "Rewind", rewind),
        Item { de: "Spul-Dauer", en: "Rewind length", kind: Kind::Num(|s| s.rewind_seconds as f64, |s, v| s.rewind_seconds = v.round() as u32, 5.0, 120.0, 5.0, |v| format!("{} s", v as u32)) },
        b!("Spielstand automatisch laden", "Autoload save", autoload),
        b!("Pause in Fenstern", "Pause in windows", pause_in_menus),
        b!("Automatisch pausieren", "Auto pause", auto_pause),
        b!("Schnellere Dialoge", "Faster dialogs", fast_text),
        b!("Herzen/Rubine einsammeln", "Auto-collect hearts/rupees", auto_collect),
        b!("Spiegel direkt", "Mirror directly", mirror_direct),
        b!("Feen-Hinweis", "Fairy notice", fairy_notice),
        b!("Mehr Bomben gleichzeitig", "More bombs at once", more_bombs, rom),
        b!("Kürzerer Pegasus-Anlauf", "Shorter Pegasus charge", quick_dash, rom),
        b!("Kein Warnton, Herz blinkt", "No beep, heart blinks", health_blink, rom),
        b!("Töpfe mit dem Schwert", "Break pots with sword", sword_pots, rom),
        b!("Gegner auch im Breitbild-Rand", "Enemies in widescreen edges", wide_sprites, rom),
        head("STEUERUNG", "CONTROLS"),
        Item { de: "Stick-Totzone", en: "Stick dead zone", kind: Kind::Num(|s| s.deadzone as f64, |s, v| s.deadzone = v as f32, 0.1, 0.9, 0.05, |v| format!("{v:.2}")) },
        b!("Gegenrichtungen sperren", "Block opposite directions", block_opposite),
        b!("Tasten wie aufgedruckt (A/B)", "Buttons as labelled (A/B)", pad_label_layout),
        b!("Steuerkreuz bewegt Link nicht", "D-pad doesn't move Link", dpad_no_move),
        b!("Steuerkreuz rechts = Schnellmenü", "D-pad right = quick menu", dpad_quickmenu),
        b!("X-Taste als Gegenstand", "X button as item", x_slot),
        b!("Eigenes Inventar", "Own inventory", pixel_inv),
        b!("Minus öffnet die Karte", "Minus opens the map", minus_map),
        head("KARTE", "MAP"),
        b!("Teleport über die Karte", "Teleport from the map", map_teleport),
        b!("Karte im eigenen Fenster", "Map in its own window", map_own_window),
        head("ALLGEMEIN", "GENERAL"),
        Item { de: "Sprache", en: "Language", kind: Kind::Choice(|s| s.ui_en as usize, |s, i| s.ui_en = i == 1, LANG) },
    ]
}

impl App {
    /// Eingaben auf der Einstellungsseite (Tasten wie im Spielstand-Menü: Bits B=15 … R=4)
    pub(crate) fn pad_settings_input(&mut self, pressed: u16, held: u16, dt: f32) {
        let list = items();
        let selectable: Vec<usize> = (0..list.len()).filter(|&i| !matches!(list[i].kind, Kind::Head)).collect();
        if self.pset_sel >= list.len() || matches!(list[self.pset_sel].kind, Kind::Head) {
            self.pset_sel = selectable[0];
        }
        if pressed & ((1 << 15) | (1 << 12)) != 0 {
            self.win_pad_settings = false;
            self.s.save();
            return;
        }
        let d = self.rep_dir(held & 0x0F00, dt);
        if d.1 != 0 {
            let k = selectable.iter().position(|&i| i == self.pset_sel).unwrap_or(0) as i32;
            let k = (k + d.1).rem_euclid(selectable.len() as i32) as usize;
            self.pset_sel = selectable[k];
        }
        let step = d.0 + if pressed & (1 << 7) != 0 { 2 } else { 0 };
        if step == 0 {
            return;
        }
        let it = &list[self.pset_sel];
        match it.kind {
            Kind::Bool(get, set, rom) => {
                let v = !get(&self.s);
                set(&mut self.s, v);
                if rom {
                    self.rebuild_rom();
                }
            }
            Kind::Num(get, set, min, max, st, _) => {
                let dir = if step == 2 { 1.0 } else { step as f64 };
                let mut v = get(&self.s) + dir * st;
                if step == 2 && v > max + st * 0.5 {
                    v = min;
                }
                set(&mut self.s, v.clamp(min, max));
            }
            Kind::Choice(get, set, names) => {
                let dir = if step == 2 { 1 } else { step };
                let i = (get(&self.s) as i32 + dir).rem_euclid(names.len() as i32) as usize;
                set(&mut self.s, i);
            }
            Kind::Head => {}
        }
    }

    /// Einstellungsseite über dem Spielstand-Menü zeichnen
    pub(crate) fn pad_settings_overlay(&mut self, ctx: &gfx::Context) {
        if !self.win_pad_settings {
            return;
        }
        let en = self.s.ui_en;
        let screen = if self.game_area.width() > 10.0 { self.game_area } else { ctx.screen_rect() };
        let painter = ctx.layer_painter(gfx::LayerId::background()).with_clip_rect(screen);
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
        let gold = Color32::from_rgb(248, 208, 64);
        let pw = (screen.width() * 0.8).min(900.0);
        let ph = screen.height() * 0.9;
        let panel = gfx::Rect::from_center_size(screen.center(), gfx::vec2(pw, ph));
        self.gbox_round(&painter, panel, crate::inventory::GOLDF);
        let pixm = self.pixel_ui();
        let top = panel.top() + if pixm { 28.0 } else { 18.0 };
        self.game_text_w(&painter, gfx::pos2(panel.center().x, top), gfx::Align2::CENTER_TOP, if en { "SETTINGS" } else { "EINSTELLUNGEN" }, 28.0, gold, pw - 60.0);
        let list = items();
        let row_h = 30.0;
        let body = gfx::Rect::from_min_max(gfx::pos2(panel.left() + 40.0, top + 50.0), gfx::pos2(panel.right() - 40.0, panel.bottom() - 60.0));
        // ausgewählte Zeile im sichtbaren Bereich halten
        let sel_y = self.pset_sel as f32 * row_h;
        let vis = body.height();
        if sel_y - self.pset_scroll > vis - row_h * 2.0 {
            self.pset_scroll = sel_y - vis + row_h * 2.0;
        }
        if sel_y - self.pset_scroll < row_h {
            self.pset_scroll = (sel_y - row_h).max(0.0);
        }
        let cp = painter.with_clip_rect(body);
        let t = ctx.input(|i| i.time) as f32;
        let blink = (t * 3.0) as i64 % 2 == 0;
        for (i, it) in list.iter().enumerate() {
            let y = body.top() + i as f32 * row_h - self.pset_scroll;
            if y < body.top() - row_h || y > body.bottom() {
                continue;
            }
            let label = if en { it.en } else { it.de };
            if let Kind::Head = it.kind {
                self.game_text_w(&cp, gfx::pos2(body.left(), y + row_h * 0.5), gfx::Align2::LEFT_CENTER, label, 20.0, gold, body.width());
                continue;
            }
            let sel = i == self.pset_sel;
            if sel {
                cp.rect_filled(gfx::Rect::from_min_size(gfx::pos2(body.left() - 8.0, y + 2.0), gfx::vec2(body.width() + 16.0, row_h - 4.0)), 4.0, Color32::from_rgba_unmultiplied(248, 208, 64, 40));
                if blink {
                    let a = gfx::pos2(body.left() + 8.0, y + row_h * 0.5);
                    cp.add(gfx::Shape::convex_polygon(vec![a + gfx::vec2(8.0, 0.0), a - gfx::vec2(4.0, 7.0), a - gfx::vec2(4.0, -7.0)], gold, gfx::Stroke::NONE));
                }
            }
            let col = if sel { Color32::WHITE } else { Color32::from_gray(200) };
            self.game_text_w(&cp, gfx::pos2(body.left() + 24.0, y + row_h * 0.5), gfx::Align2::LEFT_CENTER, label, 18.0, col, body.width() * 0.6);
            let val = match it.kind {
                Kind::Bool(get, _, _) => {
                    if get(&self.s) {
                        (if en { "ON" } else { "AN" }).to_string()
                    } else {
                        (if en { "OFF" } else { "AUS" }).to_string()
                    }
                }
                Kind::Num(get, _, _, _, _, fmt) => fmt(get(&self.s)),
                Kind::Choice(get, _, names) => {
                    let (d, e) = names[get(&self.s).min(names.len() - 1)];
                    (if en { e } else { d }).to_string()
                }
                Kind::Head => String::new(),
            };
            let on = matches!(it.kind, Kind::Bool(get, _, _) if get(&self.s));
            let vcol = if on { Color32::from_rgb(120, 230, 120) } else if sel { gold } else { Color32::from_gray(220) };
            let txt = if sel && !matches!(it.kind, Kind::Bool(..)) { format!("< {val} >") } else { val };
            self.game_text_w(&cp, gfx::pos2(body.right(), y + row_h * 0.5), gfx::Align2::RIGHT_CENTER, &txt, 18.0, vcol, body.width() * 0.38);
        }
        let hint = if en { "LEFT/RIGHT OR A: CHANGE    B: BACK" } else { "LINKS/RECHTS ODER A: ÄNDERN    B: ZURÜCK" };
        self.game_text_w(&painter, panel.center_bottom() - gfx::vec2(0.0, if pixm { 28.0 } else { 18.0 }), gfx::Align2::CENTER_BOTTOM, hint, 18.0, Color32::from_gray(210), pw - 60.0);
        ctx.request_repaint();
    }
}
