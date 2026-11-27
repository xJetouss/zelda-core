//! Fenster-Oberfläche mit GPUI: Menüleiste, Statusleiste, Einstellungsfenster und Dialoge.
//!
//! Das Spielbild, die Karte, das Inventar und alle Einblendungen zeichnet die App selbst über
//! [`crate::gfx`]; hier entsteht alles drumherum aus GPUI-Elementen – in einem einheitlichen Stil:
//! dunkle Flächen, feine Linien, Gold als Akzent, Spielschrift für Überschriften.

use crate::alttp::{self, CheatCode};
use crate::app::{App as Core, SLOTS};
use crate::filters::Filter;
use crate::gfx;
use crate::hotkeys::{ACTS, GROUPS};
use crate::i18n::tr;
use crate::settings::BUTTONS;
use crate::tracker::{Marker, MarkerKind};
use gpui::prelude::*;
use gpui::{
    canvas, deferred, div, point, px, rgb, AnyElement, App, Bounds, ClickEvent, Context, Div, ElementId, Entity, ExternalPaths, FocusHandle, FontWeight, Hsla, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, Pixels, Point, Rgba, SharedString, Stateful, WeakEntity, Window, WindowHandle,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

// ---------------------------------------------------------------- Stil

/// Höhe der Menüleiste
pub const MENU_H: f32 = 34.0;

mod col {
    use gpui::{rgb, rgba, Rgba};
    pub fn bar() -> Rgba {
        rgb(0x14161a)
    }
    pub fn line() -> Rgba {
        rgb(0x262a31)
    }
    pub fn panel() -> Rgba {
        rgb(0x1a1d22)
    }
    pub fn panel_line() -> Rgba {
        rgb(0x30343c)
    }
    pub fn hover() -> Rgba {
        rgb(0x272b33)
    }
    pub fn press() -> Rgba {
        rgb(0x30353e)
    }
    pub fn field() -> Rgba {
        rgb(0x0f1114)
    }
    pub fn text() -> Rgba {
        rgb(0xe8eaed)
    }
    pub fn dim() -> Rgba {
        rgb(0x8d939d)
    }
    pub fn faint() -> Rgba {
        rgb(0x5d636d)
    }
    pub fn gold() -> Rgba {
        rgb(0xebc85a)
    }
    pub fn gold_line() -> Rgba {
        rgb(0x78591a)
    }
    pub fn green() -> Rgba {
        rgb(0x6ec878)
    }
    pub fn red() -> Rgba {
        rgb(0xff8a8a)
    }
    pub fn game_bg() -> Rgba {
        rgb(0x131518)
    }
    pub fn card() -> Rgba {
        rgb(0x1a1c20)
    }
    pub fn card_line() -> Rgba {
        rgb(0x24272d)
    }
    pub fn sel() -> Rgba {
        rgba(0xebc85a24)
    }
}

const UI_FONT: &str = gfx::UI_FONT;
const MONO_FONT: &str = gfx::MONO_FONT;
/// Symbolschrift von Windows (Pause, Lautsprecher, Controller …)
const ICON_FONT: &str = "Segoe MDL2 Assets";

fn icon(glyph: &'static str) -> Div {
    div().font_family(ICON_FONT).text_size(px(13.0)).child(glyph)
}

fn tint(c: Rgba, a: f32) -> Rgba {
    Rgba { a: c.a * a, ..c }
}

/// Symbole am Anfang (Emoji) weglassen – Menüs und Titel bleiben ruhig
fn clean(s: &str) -> String {
    s.chars().skip_while(|c| !c.is_alphanumeric()).collect()
}

// ---------------------------------------------------------------- Zustand der Oberfläche

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextField {
    RandoSeed,
    CodeInput,
    CodeDesc,
    MarkerLabel,
    GbaCheatName,
    GbaCheatCode,
}

// Menüs und Cheat-Fenster im GBA-Modus
#[path = "gba/menus.rs"]
mod gba_menus;

impl TextField {
    fn value(self, a: &mut Core) -> &mut String {
        match self {
            TextField::RandoSeed => &mut a.s.rando_seed,
            TextField::CodeInput => &mut a.code_input,
            TextField::CodeDesc => &mut a.code_desc,
            TextField::MarkerLabel => &mut a.marker_label,
            TextField::GbaCheatName => &mut a.gba.cheat_name,
            TextField::GbaCheatCode => &mut a.gba.cheat_code,
        }
    }
}

type Setter = Rc<dyn Fn(&mut Root, f64)>;

enum Drag {
    Window { id: &'static str, grab: Point<Pixels> },
    Slider { key: SharedString, min: f64, max: f64, step: f64, set: Setter },
    Value { start_x: f32, start: f64, min: f64, max: f64, set: Setter },
}

pub struct UiState {
    open_menu: Option<usize>,
    win_pos: HashMap<&'static str, Point<Pixels>>,
    drag: Option<Drag>,
    bounds: Rc<RefCell<HashMap<SharedString, Bounds<Pixels>>>>,
    pub text_focus: Option<TextField>,
    caret: usize,
    combo_open: Option<SharedString>,
    hk_open: Vec<bool>,
    /// Marker-Dialog: Fokus einmalig auf das Textfeld
    marker_focused: bool,
    marker_submit: bool,
    window_size: (f32, f32),
    /// Einblendung der Menüleiste (0 = weg, 1 = ganz da)
    menu_t: f32,
    menu_clock: Instant,
}

impl UiState {
    fn new() -> UiState {
        UiState {
            open_menu: None,
            win_pos: HashMap::new(),
            drag: None,
            bounds: Rc::new(RefCell::new(HashMap::new())),
            text_focus: None,
            caret: 0,
            combo_open: None,
            hk_open: (0..GROUPS.len()).map(|g| g < 3).collect(),
            marker_focused: false,
            marker_submit: false,
            window_size: (1000.0, 760.0),
            menu_t: 0.0,
            menu_clock: Instant::now(),
        }
    }
}

// ---------------------------------------------------------------- Hauptfenster

pub struct Root {
    pub app: Core,
    pub ui: UiState,
    pub gctx: gfx::Context,
    focus: FocusHandle,
    map_win: Option<WindowHandle<MapView>>,
    map_opening: bool,
    me: WeakEntity<Root>,
    quitting: bool,
}

impl Root {
    pub fn new(app: Core, window: &mut Window, cx: &mut Context<Root>) -> Root {
        let focus = cx.focus_handle();
        focus.focus(window);
        let me = cx.entity().downgrade();
        window.on_window_should_close(cx, {
            let me = me.clone();
            move |_, cx| {
                if let Some(r) = me.upgrade() {
                    r.update(cx, |r, cx| r.shutdown(cx));
                }
                true
            }
        });
        Root { app, ui: UiState::new(), gctx: gfx::Context::new(), focus, map_win: None, map_opening: false, me, quitting: false }
    }

    /// Beenden: Spiel schließen, Einstellungen sichern, alle Fenster zu
    fn shutdown(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        self.app.close_game();
        self.app.s.save();
        cx.quit();
    }

    fn key(&mut self, ks: &gpui::Keystroke, down: bool, held: bool, cx: &mut Context<Root>) {
        if down {
            if let Some(f) = self.ui.text_focus {
                self.edit_text(f, ks, cx);
                return;
            }
        }
        if let Some((k, m)) = gfx::key_from_gpui(ks) {
            self.gctx.collector().key(k, down, held, m);
        }
    }

    fn edit_text(&mut self, f: TextField, ks: &gpui::Keystroke, cx: &mut Context<Root>) {
        let m = &ks.modifiers;
        let paste = if m.control && ks.key == "v" { cx.read_from_clipboard().and_then(|c| c.text()) } else { None };
        let caret = self.ui.caret;
        let mut submit = false;
        let mut unfocus = false;
        {
            let s = f.value(&mut self.app);
            let mut c = caret.min(s.chars().count());
            let byte = |s: &String, c: usize| s.char_indices().nth(c).map(|(i, _)| i).unwrap_or(s.len());
            match ks.key.as_str() {
                "backspace" if c > 0 => {
                    let b = byte(s, c - 1);
                    s.remove(b);
                    c -= 1;
                }
                "delete" if c < s.chars().count() => {
                    let b = byte(s, c);
                    s.remove(b);
                }
                "left" => c = c.saturating_sub(1),
                "right" => c = (c + 1).min(s.chars().count()),
                "home" => c = 0,
                "end" => c = s.chars().count(),
                "enter" => {
                    submit = true;
                    unfocus = true;
                }
                "escape" | "tab" => unfocus = true,
                _ => {
                    let ins = if let Some(p) = paste {
                        Some(p.lines().next().unwrap_or("").to_string())
                    } else if !m.control && !m.alt {
                        ks.key_char.clone().filter(|t| !t.chars().any(|ch| ch.is_control()))
                    } else {
                        None
                    };
                    if let Some(t) = ins {
                        let b = byte(s, c);
                        s.insert_str(b, &t);
                        c += t.chars().count();
                    }
                }
            }
            self.ui.caret = c;
        }
        if unfocus {
            self.ui.text_focus = None;
        }
        if submit && f == TextField::MarkerLabel {
            self.ui.marker_submit = true;
        }
    }

    fn focus_text(&mut self, f: TextField) {
        self.ui.caret = f.value(&mut self.app).chars().count();
        self.ui.text_focus = Some(f);
    }

    /// laufende Zieh-Bewegungen (Fenster, Schieberegler) mit der aktuellen Mausposition fortsetzen
    fn continue_drag(&mut self) {
        let (mouse, down) = {
            let c = self.gctx.collector();
            (c.mouse, c.button_down)
        };
        let Some(mouse) = mouse else { return };
        if !down {
            self.ui.drag = None;
            return;
        }
        let mp = point(px(mouse.x), px(mouse.y));
        match self.ui.drag.take() {
            Some(Drag::Window { id, grab }) => {
                let (ww, wh) = self.ui.window_size;
                let p = mp - grab;
                let p = point(px(f32::from(p.x).clamp(-200.0, ww - 80.0)), px(f32::from(p.y).clamp(0.0, wh - 40.0)));
                self.ui.win_pos.insert(id, p);
                self.ui.drag = Some(Drag::Window { id, grab });
            }
            Some(Drag::Slider { key, min, max, step, set }) => {
                let b = self.ui.bounds.borrow().get(&key).copied();
                if let Some(b) = b {
                    let v = slider_value(b, mouse.x, min, max, step);
                    set(self, v);
                }
                self.ui.drag = Some(Drag::Slider { key, min, max, step, set });
            }
            Some(Drag::Value { start_x, start, min, max, set }) => {
                let v = (start + ((mouse.x - start_x) / 6.0).round() as f64).clamp(min, max);
                set(self, v);
                self.ui.drag = Some(Drag::Value { start_x, start, min, max, set });
            }
            None => {}
        }
    }

    /// Karte im eigenen Fenster öffnen/schließen, wie es die App gerade will
    fn sync_map_window(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        // A Link to the Past oder The Minish Cap (eigene Karte der PC-Fassung)
        let want = self.app.s.map_own_window && ((self.app.win_map && self.app.game.is_some()) || (self.app.gba.active() && self.app.gba.map.open));
        match (&self.map_win, want) {
            (None, true) if !self.map_opening => {
                self.map_opening = true;
                let me = self.me.clone();
                let title = tr(self.app.s.ui_en, "Zelda Core – Karte & 100 %", "Zelda Core – Map & 100 %").to_string();
                cx.defer(move |cx| {
                    let opts = gpui::WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::centered(None, gpui::size(px(1180.0), px(760.0)), cx))),
                        titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), ..Default::default() }),
                        window_min_size: Some(gpui::size(px(700.0), px(480.0))),
                        ..Default::default()
                    };
                    let me2 = me.clone();
                    let h = cx.open_window(opts, move |window, cx| cx.new(|cx| MapView::new(me2, window, cx)));
                    if let Some(r) = me.upgrade() {
                        r.update(cx, |r, _| {
                            r.map_opening = false;
                            r.map_win = h.ok();
                        });
                    }
                });
            }
            (Some(h), false) => {
                let h = *h;
                self.map_win = None;
                self.app.map_focused = false;
                cx.defer(move |cx| {
                    let _ = h.update(cx, |_, window, _| window.remove_window());
                    gfx::unregister_window(2);
                });
            }
            _ => {}
        }
        if self.map_win.is_none() {
            self.app.map_focused = false;
        }
        let _ = window;
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::perf::present();
        gfx::collect_garbage(window, 1);
        if window.focused(cx).is_none() {
            self.focus.focus(window);
        }
        let vs = window.viewport_size();
        let (ww, wh) = (f32::from(vs.width), f32::from(vs.height));
        self.ui.window_size = (ww, wh);
        self.continue_drag();
        // Menüleiste liegt über dem Spiel und erscheint nur, wenn die Maus oben ist (oder ein Menü offen ist)
        // Menüleiste nur im Spiel (nicht im Hauptmenü)
        let bar = self.app.show_bar && (self.app.game.is_some() || self.app.gba.active());
        let zone = MENU_H + if self.ui.menu_t > 0.5 { 10.0 } else { 0.0 };
        let mouse_top = {
            let c = self.gctx.collector();
            let recent = c.moved_at.map_or(false, |t| t.elapsed().as_secs_f32() < 1.2);
            (window.is_window_hovered() || recent) && c.mouse.map_or(false, |m| m.y <= zone)
        };
        // offenes Aufklappmenü hält die Leiste nur, solange die Maus darauf ist
        let over_panel = self.ui.open_menu.is_some() && {
            let m = self.gctx.collector().mouse;
            let b = self.ui.bounds.borrow().get("menu-panel").copied();
            match (m, b) {
                (Some(m), Some(b)) => {
                    let (x0, y0) = (f32::from(b.origin.x) - 10.0, f32::from(b.origin.y) - 10.0);
                    let (x1, y1) = (x0 + f32::from(b.size.width) + 20.0, y0 + f32::from(b.size.height) + 20.0);
                    m.x >= x0 && m.x <= x1 && m.y >= y0 && m.y <= y1
                }
                _ => true,
            }
        };
        let dragging = matches!(self.ui.drag, Some(Drag::Slider { .. })) && self.ui.open_menu.is_some();
        let show = bar && (mouse_top || over_panel || dragging);
        if !show {
            // Maus weg von Leiste und Menü: Menü schließen
            self.ui.open_menu = None;
            self.ui.bounds.borrow_mut().remove("menu-panel");
        }
        let target = if show { 1.0 } else { 0.0 };
        let dt = self.ui.menu_clock.elapsed().as_secs_f32().min(0.1);
        self.ui.menu_clock = Instant::now();
        self.ui.menu_t += (target - self.ui.menu_t) * (1.0 - (-dt * 16.0).exp());
        if (self.ui.menu_t - target).abs() < 0.01 {
            self.ui.menu_t = target;
        }
        self.app.central = gfx::Rect::from_min_max(gfx::pos2(0.0, 0.0), gfx::pos2(ww, wh.max(1.0)));
        let typing = self.ui.text_focus.is_some();
        self.gctx.begin_frame(window, typing);
        // Fenster/Menü offen und Spiel aus: Textfeld verliert den Fokus mit dem Fenster
        crate::perf::time("app", || self.app.run_frame(&self.gctx));
        for c in self.gctx.take_viewport_cmds() {
            match c {
                gfx::ViewportCommand::Title(t) => window.set_window_title(&t),
                gfx::ViewportCommand::Fullscreen(f) => {
                    if window.is_fullscreen() != f {
                        window.toggle_fullscreen();
                    }
                }
                gfx::ViewportCommand::Close => self.shutdown(cx),
            }
        }
        self.sync_map_window(window, cx);
        if self.ui.marker_submit {
            self.ui.marker_submit = false;
            self.submit_marker(true);
        }
        if self.app.new_marker.is_none() {
            self.ui.marker_focused = false;
        } else if !self.ui.marker_focused {
            self.ui.marker_focused = true;
            self.focus_text(TextField::MarkerLabel);
        }
        window.request_animation_frame();

        let ctx_bg = self.gctx.clone();
        let ctx_fg = self.gctx.clone();
        let mut root = div()
            .id("root")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(rgb(0x000000))
            .font_family(UI_FONT)
            .text_size(px(13.0))
            .text_color(col::text())
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.key(&e.keystroke, true, e.is_held, cx)))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, cx| this.key(&e.keystroke, false, false, cx)))
            .on_drop(cx.listener(|this, p: &ExternalPaths, _, _| {
                this.gctx.collector().dropped.extend(p.paths().iter().cloned());
            }))
            .child(
                canvas(
                    move |b, window, _| gfx::insert_hitbox(b, window),
                    move |_, hb, window, cx| {
                        gfx::install_mouse(&ctx_bg, &hb, window);
                        crate::perf::time("paint", || ctx_bg.paint_layer(0, window, cx));
                    },
                )
                .absolute()
                .size_full(),
            );
        root = root.children(self.windows(cx));
        if self.ui.menu_t > 0.0 {
            let t = self.ui.menu_t;
            root = root.child(div().absolute().left_0().top(px(-(1.0 - t) * MENU_H)).w_full().h(px(MENU_H)).opacity(t).child(self.menu_bar(window, cx)));
        }
        root = root.child(canvas(|_, _, _| {}, move |_, _, window, cx| ctx_fg.paint_layer(1, window, cx)).absolute().size_full());
        root
    }
}

// ---------------------------------------------------------------- Karte im eigenen Fenster

pub struct MapView {
    root: WeakEntity<Root>,
    ctx: gfx::Context,
    focus: FocusHandle,
}

impl MapView {
    fn new(root: WeakEntity<Root>, window: &mut Window, cx: &mut Context<MapView>) -> MapView {
        let focus = cx.focus_handle();
        focus.focus(window);
        let r2 = root.clone();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(r) = r2.upgrade() {
                r.update(cx, |r, _| {
                    r.app.win_map = false;
                    r.app.gba.map.open = false;
                    r.map_win = None;
                    r.app.map_focused = false;
                });
            }
            gfx::unregister_window(2);
            true
        });
        MapView { root, ctx: gfx::Context::new(), focus }
    }
}

impl Render for MapView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gfx::collect_garbage(window, 2);
        self.ctx.begin_frame(window, false);
        let focused = window.is_window_active();
        let ctx = self.ctx.clone();
        if let Some(r) = self.root.upgrade() {
            r.update(cx, |r, _| {
                r.app.map_focused = focused;
                if r.app.gba.active() {
                    r.app.gba_map_window(&ctx);
                } else if r.app.win_map && r.app.game.is_some() {
                    r.app.map_window(&ctx);
                }
            });
        }
        window.request_animation_frame();
        let c1 = self.ctx.clone();
        div()
            .id("map-root")
            .track_focus(&self.focus)
            .size_full()
            .bg(rgb(0x000000))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, _| {
                if let Some((k, m)) = gfx::key_from_gpui(&e.keystroke) {
                    this.ctx.collector().key(k, true, e.is_held, m);
                }
            }))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, _| {
                if let Some((k, m)) = gfx::key_from_gpui(&e.keystroke) {
                    this.ctx.collector().key(k, false, false, m);
                }
            }))
            .child(
                canvas(
                    move |b, window, _| gfx::insert_hitbox(b, window),
                    move |_, hb, window, cx| {
                        gfx::install_mouse(&c1, &hb, window);
                        c1.paint_layer(0, window, cx);
                        c1.paint_layer(1, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

// ---------------------------------------------------------------- Bausteine

fn eid(s: impl Into<SharedString>) -> ElementId {
    ElementId::Name(s.into())
}

/// Klick-Aktion, die den App-Zustand ändert
fn on<E: 'static>(cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let me = cx.entity().downgrade();
    move |_: &E, _: &mut Window, app: &mut App| {
        let _ = me.update(app, |this, _| f(this));
    }
}

fn label(t: impl Into<SharedString>) -> Div {
    div().child(t.into())
}

fn small(t: impl Into<SharedString>) -> Div {
    div().text_size(px(12.0)).text_color(col::dim()).child(t.into())
}

fn strong(t: impl Into<SharedString>) -> Div {
    div().font_weight(FontWeight::SEMIBOLD).child(t.into())
}

fn mono(t: impl Into<SharedString>) -> Div {
    div().font_family(MONO_FONT).child(t.into())
}

fn row() -> Div {
    div().flex().flex_row().items_center().gap(px(8.0))
}

fn wrap_row() -> Div {
    div().flex().flex_row().flex_wrap().items_center().gap(px(6.0))
}


fn sep() -> Div {
    div().my(px(4.0)).h(px(1.0)).w_full().bg(col::line())
}

/// Knopf
fn button(id: impl Into<SharedString>, text: impl Into<SharedString>, enabled: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    let b = div()
        .id(eid(id))
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(col::panel_line())
        .bg(rgb(0x22262d))
        .whitespace_nowrap()
        .child(text.into());
    if enabled {
        b.cursor_pointer().hover(|s| s.bg(col::hover()).border_color(rgb(0x3c424b))).active(|s| s.bg(col::press())).on_click(on::<ClickEvent>(cx, f))
    } else {
        b.text_color(col::faint())
    }
}

/// Knopf, der gedrückt bleibt (an/aus)
fn toggle_button(id: impl Into<SharedString>, text: impl Into<SharedString>, on_: bool, color: Rgba, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    div()
        .id(eid(id))
        .flex()
        .items_center()
        .h(px(26.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .whitespace_nowrap()
        .text_color(color)
        .when(on_, |d| d.bg(col::sel()))
        .cursor_pointer()
        .hover(|s| s.bg(col::hover()))
        .on_click(on::<ClickEvent>(cx, f))
        .child(text.into())
}

fn check_box(on_: bool) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if on_ { col::gold() } else { rgb(0x4a505a) })
        .bg(if on_ { col::gold() } else { col::field() })
        .text_color(rgb(0x14161a))
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .when(on_, |d| d.child("✓"))
}

/// Kästchen mit Beschriftung
fn check(id: impl Into<SharedString>, text: impl Into<SharedString>, value: bool, enabled: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    let d = div().id(eid(id)).flex().flex_row().items_start().gap(px(8.0)).py(px(3.0)).px(px(4.0)).rounded(px(5.0)).child(div().pt(px(1.0)).flex_none().child(check_box(value))).child(div().flex_1().min_w(px(0.0)).child(text.into()));
    if enabled {
        d.cursor_pointer().hover(|s| s.bg(col::hover())).on_click(on::<ClickEvent>(cx, f))
    } else {
        d.text_color(col::faint()).opacity(0.6)
    }
}

/// Auswahlpunkt
fn radio(id: impl Into<SharedString>, text: impl Into<SharedString>, selected: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    div()
        .id(eid(id))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .py(px(3.0))
        .px(px(4.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(|s| s.bg(col::hover()))
        .on_click(on::<ClickEvent>(cx, f))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(16.0))
                .rounded_full()
                .border_1()
                .border_color(if selected { col::gold() } else { rgb(0x4a505a) })
                .bg(col::field())
                .when(selected, |d| d.child(div().size(px(8.0)).rounded_full().bg(col::gold()))),
        )
        .child(div().whitespace_nowrap().child(text.into()))
}

/// Auswahl wie Reiter (Slot 1…6, Geschwindigkeit …)
fn chip(id: impl Into<SharedString>, text: impl Into<SharedString>, selected: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    div()
        .id(eid(id))
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .min_w(px(28.0))
        .px(px(8.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(if selected { col::gold() } else { col::panel_line() })
        .bg(if selected { col::sel() } else { col::field() })
        .text_color(if selected { col::gold() } else { col::text() })
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(col::hover()))
        .on_click(on::<ClickEvent>(cx, f))
        .child(text.into())
}

fn slider_value(b: Bounds<Pixels>, x: f32, min: f64, max: f64, step: f64) -> f64 {
    let x0 = f32::from(b.origin.x);
    let w = f32::from(b.size.width).max(1.0);
    let t = ((x - x0) / w).clamp(0.0, 1.0) as f64;
    let mut v = min + t * (max - min);
    if step > 0.0 {
        v = (v / step).round() * step;
    }
    v.clamp(min, max)
}

/// Schieberegler: Beschriftung, Spur, Wert
#[allow(clippy::too_many_arguments)]
fn slider(root: &Root, id: &str, text: impl Into<SharedString>, value: f64, min: f64, max: f64, step: f64, shown: String, enabled: bool, cx: &mut Context<Root>, set: impl Fn(&mut Root, f64) + 'static) -> Div {
    let key: SharedString = format!("slider-{id}").into();
    let t = ((value - min) / (max - min).max(1e-9)).clamp(0.0, 1.0) as f32;
    let set: Setter = Rc::new(set);
    let bounds = root.ui.bounds.clone();
    let k2 = key.clone();
    let probe = canvas(move |b, _, _| {
        bounds.borrow_mut().insert(k2.clone(), b);
    }, |_, _, _, _| {})
    .absolute()
    .size_full();
    let w = 170.0;
    let mut track = div()
        .id(eid(key.clone()))
        .relative()
        .flex_none()
        .w(px(w))
        .h(px(18.0))
        .child(probe)
        .child(div().absolute().top(px(7.0)).left_0().w_full().h(px(4.0)).rounded(px(2.0)).bg(rgb(0x2c3038)))
        .child(div().absolute().top(px(7.0)).left_0().w(px(w * t)).h(px(4.0)).rounded(px(2.0)).bg(if enabled { col::gold() } else { col::faint() }))
        .child(div().absolute().top(px(2.0)).left(px(w * t - 7.0)).size(px(14.0)).rounded_full().bg(if enabled { rgb(0xf4f5f7) } else { col::faint() }).border_2().border_color(rgb(0x14161a)));
    if enabled {
        let bounds = root.ui.bounds.clone();
        let k3 = key.clone();
        track = track.cursor_pointer().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, e: &MouseDownEvent, _, _| {
                if let Some(b) = bounds.borrow().get(&k3).copied() {
                    let v = slider_value(b, f32::from(e.position.x), min, max, step);
                    set(this, v);
                }
                this.ui.drag = Some(Drag::Slider { key: k3.clone(), min, max, step, set: set.clone() });
            }),
        );
    }
    let text: SharedString = text.into();
    row()
        .py(px(3.0))
        .px(px(4.0))
        .when(!enabled, |d| d.opacity(0.5))
        .child(track)
        .child(div().min_w(px(44.0)).font_family(MONO_FONT).text_color(col::gold()).child(shown))
        .when(!text.is_empty(), |d| d.child(div().text_color(col::dim()).child(text)))
}

/// Zahl, die man mit der Maus zieht (oder mit − / + verändert)
#[allow(clippy::too_many_arguments)]
fn drag_value(id: &str, value: f64, min: f64, max: f64, enabled: bool, cx: &mut Context<Root>, set: impl Fn(&mut Root, f64) + 'static) -> Div {
    let set: Setter = Rc::new(set);
    let s1 = set.clone();
    let s2 = set.clone();
    let s3 = set.clone();
    let step = |id: String, txt: &'static str, cx: &mut Context<Root>, f: Box<dyn Fn(&mut Root)>| {
        div()
            .id(eid(id))
            .flex()
            .items_center()
            .justify_center()
            .w(px(20.0))
            .h(px(24.0))
            .text_color(col::dim())
            .cursor_pointer()
            .hover(|s| s.bg(col::hover()).text_color(col::text()))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| f(this)))
            .child(txt)
    };
    let mut d = row()
        .gap(px(0.0))
        .h(px(26.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(col::panel_line())
        .bg(col::field())
        .overflow_hidden();
    if enabled {
        d = d
            .child(step(format!("{id}-dec"), "−", cx, Box::new(move |r| s1(r, (value - 1.0).max(min)))))
            .child(
                div()
                    .id(eid(format!("{id}-val")))
                    .min_w(px(46.0))
                    .px(px(4.0))
                    .flex()
                    .justify_center()
                    .font_family(MONO_FONT)
                    .cursor_ew_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, _| {
                            this.ui.drag = Some(Drag::Value { start_x: f32::from(e.position.x), start: value, min, max, set: s3.clone() });
                        }),
                    )
                    .child(format!("{}", value as i64)),
            )
            .child(step(format!("{id}-inc"), "+", cx, Box::new(move |r| s2(r, (value + 1.0).min(max)))));
    } else {
        d = d.opacity(0.5).child(div().min_w(px(86.0)).flex().justify_center().font_family(MONO_FONT).child(format!("{}", value as i64)));
    }
    d
}

/// Aufklappliste
fn combo(root: &Root, id: &str, selected: String, width: f32, options: Vec<(String, bool)>, cx: &mut Context<Root>, pick: impl Fn(&mut Root, usize) + 'static) -> Div {
    let key: SharedString = format!("combo-{id}").into();
    let open = root.ui.combo_open.as_ref() == Some(&key);
    let k2 = key.clone();
    let head = div()
        .id(eid(key.clone()))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .w(px(width))
        .h(px(28.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(if open { col::gold() } else { col::panel_line() })
        .bg(col::field())
        .cursor_pointer()
        .hover(|s| s.border_color(rgb(0x4a505a)))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
            this.ui.combo_open = if this.ui.combo_open.as_ref() == Some(&k2) { None } else { Some(k2.clone()) };
        }))
        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(selected))
        .child(div().text_color(col::dim()).text_size(px(10.0)).child("▼"));
    let mut d = div().relative().child(head);
    if open {
        let pick = Rc::new(pick);
        let mut list = div()
            .id(eid(format!("{key}-list")))
            .occlude()
            .absolute()
            .top(px(31.0))
            .left_0()
            .min_w(px(width))
            .max_h(px(320.0))
            .overflow_y_scroll()
            .py(px(4.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(col::panel_line())
            .bg(col::panel())
            .shadow_lg();
        for (i, (t, sel)) in options.into_iter().enumerate() {
            let p = pick.clone();
            list = list.child(
                div()
                    .id(eid(format!("{key}-{i}")))
                    .px(px(10.0))
                    .py(px(5.0))
                    .whitespace_nowrap()
                    .when(sel, |d| d.text_color(col::gold()).bg(col::sel()))
                    .cursor_pointer()
                    .hover(|s| s.bg(col::hover()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        p(this, i);
                        this.ui.combo_open = None;
                    }))
                    .child(t),
            );
        }
        d = d.child(deferred(list).with_priority(3));
    }
    d
}

/// einzeiliges Textfeld
fn text_field(root: &Root, f: TextField, value: &str, hint: &str, width: f32, cx: &mut Context<Root>) -> Stateful<Div> {
    let focused = root.ui.text_focus == Some(f);
    let caret = root.ui.caret.min(value.chars().count());
    let (a, b): (String, String) = (value.chars().take(caret).collect(), value.chars().skip(caret).collect());
    let blink = (root.gctx.input(|i| i.time) * 2.0) as i64 % 2 == 0;
    let mut inner = div().flex().flex_row().items_center().overflow_hidden().whitespace_nowrap();
    if value.is_empty() && !focused {
        inner = inner.text_color(col::faint()).child(hint.to_string());
    } else {
        inner = inner.child(a);
        if focused {
            inner = inner.child(div().w(px(1.5)).h(px(16.0)).bg(if blink { col::gold() } else { tint(col::gold(), 0.0) }));
        }
        inner = inner.child(b);
        if value.is_empty() {
            inner = inner.child(div().pl(px(4.0)).text_color(col::faint()).child(hint.to_string()));
        }
    }
    div()
        .id(eid(format!("text-{f:?}")))
        .flex()
        .items_center()
        .w(px(width))
        .h(px(28.0))
        .px(px(9.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(if focused { col::gold() } else { col::panel_line() })
        .bg(col::field())
        .cursor_text()
        .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.focus_text(f)))
        .child(inner)
}

/// Abschnitt: kleine Überschrift, darunter eine ruhige Fläche mit dem Inhalt
fn section(title: &str, body: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .pt(px(12.0))
        .child(div().pl(px(2.0)).text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(col::dim()).child(clean(title)))
        .child(div().flex().flex_col().gap(px(6.0)).p(px(12.0)).rounded(px(8.0)).bg(col::card()).border_1().border_color(col::card_line()).children(body))
}

/// Schalter (an/aus) als Zeile: Text links, Schalter rechts
fn switch(id: impl Into<SharedString>, text: impl IntoElement, value: bool, enabled: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    let knob = div()
        .flex()
        .flex_none()
        .items_center()
        .w(px(34.0))
        .h(px(20.0))
        .p(px(2.0))
        .rounded_full()
        .bg(if value { col::gold() } else { rgb(0x3a3f47) })
        .when(value, |d| d.justify_end())
        .child(div().size(px(16.0)).rounded_full().bg(if value { rgb(0x1a1608) } else { rgb(0xc9ccd1) }));
    let d = div()
        .id(eid(id))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .min_h(px(30.0))
        .px(px(6.0))
        .rounded(px(6.0))
        .child(div().flex_1().min_w(px(0.0)).child(text))
        .child(knob);
    if enabled {
        d.cursor_pointer().hover(|s| s.bg(col::hover())).on_click(on::<ClickEvent>(cx, f))
    } else {
        d.opacity(0.45)
    }
}

/// Auswahl aus wenigen Möglichkeiten als zusammenhängende Leiste
fn segmented(id: &str, options: &[&str], selected: usize, cx: &mut Context<Root>, pick: impl Fn(&mut Root, usize) + 'static) -> Div {
    let pick = Rc::new(pick);
    let mut d = div().flex().flex_row().p(px(3.0)).gap(px(3.0)).rounded(px(8.0)).bg(col::field()).border_1().border_color(col::card_line());
    for (i, o) in options.iter().enumerate() {
        let p = pick.clone();
        let sel = i == selected;
        d = d.child(
            div()
                .id(eid(format!("{id}-{i}")))
                .flex_1()
                .flex()
                .justify_center()
                .py(px(5.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .whitespace_nowrap()
                .when(sel, |d| d.bg(rgb(0x2c3038)).text_color(col::text()).font_weight(FontWeight::SEMIBOLD))
                .when(!sel, |d| d.text_color(col::dim()).cursor_pointer().hover(|s| s.text_color(col::text())))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| p(this, i)))
                .child(o.to_string()),
        );
    }
    d
}

/// Raster mit zwei (oder mehr) Spalten
fn grid(cols: usize, cells: Vec<AnyElement>, gap_x: f32) -> Div {
    let mut g = div().flex().flex_col().gap(px(2.0));
    let mut it = cells.into_iter().peekable();
    while it.peek().is_some() {
        let mut r = div().flex().flex_row().gap(px(gap_x)).items_center();
        for _ in 0..cols {
            if let Some(c) = it.next() {
                r = r.child(div().flex_1().min_w(px(0.0)).child(c));
            } else {
                r = r.child(div().flex_1());
            }
        }
        g = g.child(r);
    }
    g
}

fn badge(t: impl Into<SharedString>, c: Rgba) -> Div {
    div().flex().items_center().h(px(18.0)).px(px(7.0)).rounded(px(4.0)).bg(tint(c, 0.18)).text_color(c).text_size(px(11.0)).font_weight(FontWeight::BOLD).child(t.into())
}

// ---------------------------------------------------------------- Menüleiste

const MENUS: usize = 9;

impl Root {
    fn hk(&self, id: &str) -> String {
        self.app.s.hotkey(id).map(|c| c.to_text()).unwrap_or_default()
    }

    fn menu_bar(&mut self, window: &mut Window, cx: &mut Context<Root>) -> AnyElement {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let has = self.app.game.is_some();
        let gba = self.app.gba.active();
        let titles: [(&str, bool); MENUS] = [
            (t("Datei", "File"), true),
            (t("Spielstand", "Save"), true),
            ("Emulation", true),
            (t("Grafik", "Video"), true),
            ("Audio", true),
            (t("Steuerung", "Controls"), true),
            (t("Spiel", "Game"), has || gba),
            ("Cheats", has || gba),
            ("Mods", true),
        ];
        let mut bar = row().gap(px(1.0)).px(px(6.0)).h_full();
        for (i, (name, show)) in titles.iter().enumerate() {
            if !show {
                continue;
            }
            let open = self.ui.open_menu == Some(i);
            let title = div()
                .id(eid(format!("menu-{i}")))
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(9.0))
                .rounded(px(6.0))
                .whitespace_nowrap()
                .when(open, |d| d.bg(col::press()))
                .cursor_pointer()
                .hover(|s| s.bg(col::hover()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.ui.open_menu = if this.ui.open_menu == Some(i) { None } else { Some(i) };
                    this.ui.combo_open = None;
                }))
                .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                    if *hovered && this.ui.open_menu.is_some() {
                        this.ui.open_menu = Some(i);
                    }
                }))
                .child(name.to_string());
            let mut holder = div().relative().child(title);
            if open {
                let items = self.menu_items(i, cx);
                // feste Breite je Menü: ruhiger Umbruch, keine springenden Zeilen
                let width = [330.0, 320.0, 420.0, 440.0, 380.0, 420.0, 450.0, 320.0, 300.0][i];
                let bounds = self.ui.bounds.clone();
                let probe = canvas(move |b, _, _| {
                    bounds.borrow_mut().insert("menu-panel".into(), b);
                }, |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full();
                let inner = div()
                    .id(eid(format!("menu-panel-{i}")))
                    .w(px(width))
                    .max_h(px(self.ui.window_size.1 - 50.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .p(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(col::panel_line())
                    .bg(col::panel())
                    .shadow_lg()
                    .children(items);
                let panel = div().occlude().absolute().top(px(30.0)).left_0().child(probe).child(inner);
                holder = holder.child(deferred(panel).with_priority(2));
            }
            bar = bar.child(holder);
        }
        bar = bar.child(div().mx(px(6.0)).w(px(1.0)).h(px(18.0)).bg(col::line()));
        if has {
            let wm = self.app.win_map;
            bar = bar.child(
                toggle_button("map", t("Karte", "Map"), wm, if wm { col::gold() } else { col::text() }, cx, |r| {
                    if r.app.win_map {
                        r.app.win_map = false;
                    } else {
                        r.app.open_map_overlay();
                    }
                })
                .tooltip(tip(self.hk("win_map"))),
            );
            bar = bar.child(
                check("map-own", t("Eigenes Fenster", "Own window"), self.app.s.map_own_window, true, cx, |r| r.app.s.map_own_window = !r.app.s.map_own_window)
                    .tooltip(tip(t("Aus: Karte als Vollbild-Overlay über dem Spiel", "Off: map as a full-screen overlay over the game").to_string())),
            );
            let on_ = self.app.rando_active();
            let txt = format!("Randomizer: {}", if on_ { t("AN", "ON") } else { t("AUS", "OFF") });
            bar = bar.child(toggle_button("rando", txt, self.app.win_rando, if on_ { col::green() } else { col::dim() }, cx, |r| r.app.win_rando = !r.app.win_rando).tooltip(tip(self.hk("win_rando"))));
        }
        // rechts: Schnellschalter
        let hd = self.app.s.filter.is_hd();
        let mut right = row().gap(px(4.0)).ml_auto();
        if self.app.turbo_on {
            right = right.child(badge("TURBO", col::gold()));
        } else if self.app.double_on {
            right = right.child(badge("×2", col::gold()));
        }
        let paused = self.app.paused;
        let mute = self.app.s.mute;
        if !gba {
            // reines Zelda 3 als eigenes Programm (zelda3.exe neben pastlink.exe), mit FPS-Anzeige – zum Vergleich
            right = right.child(
                toggle_button("z3exe", "Z3", false, col::text(), cx, |r| {
                    let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("zelda3.exe")));
                    match exe.filter(|e| e.is_file()).map(|e| std::process::Command::new(&e).current_dir(e.parent().unwrap()).spawn()) {
                        Some(Ok(_)) => {
                            // pastlink anhalten, damit nicht zwei Spiele gleichzeitig laufen und Ton ausgeben
                            r.app.paused = true;
                        }
                        _ => {
                            let m = r.app.t("zelda3.exe nicht gefunden (wird mit Zelda Core gebaut, wenn Zelda 3 eingebaut ist).", "zelda3.exe not found (built with Zelda Core when Zelda 3 is included).");
                            r.app.msg(m);
                        }
                    }
                })
                .font_weight(FontWeight::BOLD)
                .tooltip(tip(t("Reines Zelda 3 als eigenes Programm starten (ohne Zelda-Core-Funktionen, mit FPS-Anzeige). Zelda Core wird dabei angehalten.", "Start plain Zelda 3 as its own program (without Zelda Core features, with FPS counter). Zelda Core is paused.").to_string())),
            );
            // Recompiled-Modus (übersetzter Code, Mods) / Emulation
            let rc = self.app.s.recompiled && crate::recomp::Table::available();
            right = right.child(
                toggle_button("rc", if rc { "RC" } else { "EMU" }, rc, if rc { col::green() } else { col::dim() }, cx, |r| {
                    if crate::recomp::Table::available() {
                        r.app.s.recompiled = !r.app.s.recompiled;
                    } else {
                        let m = r.app.t("Recompiled-Modus ist nicht eingebaut (siehe README).", "Recompiled mode is not built in (see README).");
                        r.app.msg(m);
                    }
                })
                .font_weight(FontWeight::BOLD)
                .tooltip(tip(t("Recompiled-Modus (RC): übersetzter Spielcode ohne ROM-Datei, Mods aktiv. EMU: normale Emulation. Spielstände gelten in beiden.", "Recompiled mode (RC): translated game code without ROM file, mods active. EMU: normal emulation. Save states work in both.").to_string())),
            );
        }
        {
            right = right.child(toggle_button("hd", if hd { "HD" } else { "SD" }, hd, if hd { col::gold() } else { col::dim() }, cx, |r| r.app.toggle_hd()).font_weight(FontWeight::BOLD).tooltip(tip(t("HD an/aus", "HD on/off").to_string())));
        }
        right = right
            .child(toggle_button("pause", "", paused, if paused { col::gold() } else { col::text() }, cx, |r| r.app.paused = !r.app.paused).child(icon(if paused { "\u{E768}" } else { "\u{E769}" })).tooltip(tip(format!("{}  [{}]", t("Pause", "Pause"), self.hk("pause")))))
            .child(toggle_button("mute", "", mute, if mute { col::red() } else { col::text() }, cx, |r| r.app.s.mute = !r.app.s.mute).child(icon(if mute { "\u{E74F}" } else { "\u{E767}" })).tooltip(tip(format!("{}  [{}]", t("Stumm", "Mute"), self.hk("mute")))));
        bar = bar.child(right);
        let mut outer = div().absolute().top_0().left_0().w_full().h(px(MENU_H)).occlude().bg(tint(col::bar(), 0.96)).border_b_1().border_color(col::line()).shadow_md().child(bar);
        // offenes Menü: Klick daneben schließt es
        if self.ui.open_menu.is_some() || self.ui.combo_open.is_some() {
            let _ = window;
            outer = outer.child(
                deferred(
                    div().id("menu-backdrop").absolute().top_0().left_0().w(px(self.ui.window_size.0)).h(px(self.ui.window_size.1)).occlude().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _, _| {
                            this.ui.open_menu = None;
                            this.ui.combo_open = None;
                        }),
                    ),
                )
                .with_priority(1),
            );
        }
        outer.into_any_element()
    }

    /// Eintrag, der eine Aktion auslöst und das Menü schließt
    fn item(&self, id: &str, text: impl Into<SharedString>, shortcut: String, enabled: bool, bold: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> AnyElement {
        let d = div()
            .id(eid(format!("item-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .child(div().whitespace_nowrap().when(bold, |d| d.font_weight(FontWeight::SEMIBOLD)).child(text.into()))
            .child(div().font_family(MONO_FONT).text_size(px(12.0)).text_color(col::faint()).child(shortcut));
        if enabled {
            d.cursor_pointer()
                .hover(|s| s.bg(col::hover()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.ui.open_menu = None;
                    f(this);
                }))
                .into_any_element()
        } else {
            d.text_color(col::faint()).into_any_element()
        }
    }

    fn heading(t: &str) -> AnyElement {
        div().pt(px(6.0)).pb(px(2.0)).px(px(4.0)).text_size(px(11.0)).font_weight(FontWeight::BOLD).text_color(col::gold()).child(t.to_uppercase()).into_any_element()
    }

    fn note(t: impl Into<SharedString>) -> AnyElement {
        div().px(px(4.0)).py(px(2.0)).text_size(px(12.0)).text_color(col::dim()).child(t.into()).into_any_element()
    }

    fn menu_items(&mut self, i: usize, cx: &mut Context<Root>) -> Vec<AnyElement> {
        if self.app.gba.active() {
            return self.gba_menu_items(i, cx);
        }
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let has = self.app.game.is_some();
        let s = &self.app.s;
        let mut v: Vec<AnyElement> = Vec::new();
        macro_rules! chk {
            ($id:expr, $txt:expr, $($f:tt)+) => {{
                let val = self.app.$($f)+;
                v.push(check($id, $txt, val, true, cx, |r| r.app.$($f)+ = !r.app.$($f)+).into_any_element());
            }};
        }
        macro_rules! chk_then {
            ($id:expr, $txt:expr, $after:expr, $($f:tt)+) => {{
                let val = self.app.$($f)+;
                v.push(check($id, $txt, val, true, cx, |r| { r.app.$($f)+ = !r.app.$($f)+; ($after)(r); }).into_any_element());
            }};
        }
        match i {
            0 => {
                v.push(self.item("open", t("ROM öffnen…", "Open ROM…"), self.hk("open"), true, false, cx, |r| r.app.open_dialog()));
                v.push(self.item("patch", t("Patch anwenden (BPS/IPS)…", "Apply patch (BPS/IPS)…"), String::new(), true, false, cx, |r| r.app.patch_dialog()));
                v.push(sep().into_any_element());
                v.push(self.item("reset", "Reset", self.hk("reset"), has, false, cx, |r| r.app.reset()));
                v.push(self.item("close", t("ROM schließen", "Close ROM"), String::new(), has, false, cx, |r| r.app.close_game()));
                let val = self.app.s.save_sram;
                v.push(
                    check("save_sram", t("Spielinterne Spielstände (.srm) speichern", "Save in-game saves (.srm)"), val, true, cx, |r| r.app.s.save_sram = !r.app.s.save_sram)
                        .tooltip(tip(t("Aus (Standard): Es wird nichts automatisch gespeichert – nur deine Savestates. An: Der Speicherpunkt im Spiel wird als .srm neben der ROM gesichert.", "Off (default): nothing is saved automatically – only your save states. On: the in-game save is written as .srm next to the ROM.").to_string()))
                        .into_any_element(),
                );
                v.push(Self::note(t("Tipp: Eine ROM im Ordner der .exe wird beim Start automatisch geladen.", "Tip: a ROM next to the .exe is loaded automatically on start.")));
                v.push(sep().into_any_element());
                v.push(self.item("quit", t("Beenden", "Quit"), self.hk("quit"), true, false, cx, |r| r.gctx.send_viewport_cmd(gfx::ViewportCommand::Close)));
                // Infos zu ROM und Tonausgabe
                let mut info = Vec::new();
                if let Some(g) = &self.app.game {
                    info.push(format!("{} · CRC32 {:08X} · {} KB", g.info.title, g.info.crc, g.orig_rom.len() / 1024));
                    info.push(format!("{} {} (0x{:02X}){}", t("Spieltext:", "Game text:"), g.info.lang.name(en), g.info.country, if g.info.verified { t(" · unveränderte US-Version", " · unmodified US version") } else { "" }));
                }
                if let Some(a) = &self.app.audio {
                    info.push(format!("Audio: {} · {} Hz", a.device_name, a.device_rate));
                }
                if !info.is_empty() {
                    v.push(sep().into_any_element());
                    for l in info {
                        v.push(div().px(px(8.0)).py(px(1.0)).text_size(px(11.0)).text_color(col::faint()).child(l).into_any_element());
                    }
                }
            }
            1 => {
                v.push(self.item("states", t("Spielstände mit Bild…", "Save states with picture…"), self.hk("win_states"), has, true, cx, |r| {
                    r.app.win_states = true;
                    r.app.inv_open = false;
                    if !r.app.s.map_own_window {
                        r.app.win_map = false;
                    }
                }));
                v.push(sep().into_any_element());
                let slot = self.app.slot;
                v.push(self.item("qsave", format!("{} (Slot {slot})", t("Schnell speichern", "Quick save")), self.hk("save"), has, false, cx, move |r| r.app.save_slot(r.app.slot)));
                v.push(self.item("qload", format!("{} (Slot {slot})", t("Schnell laden", "Quick load")), self.hk("load"), has, false, cx, move |r| r.app.load_slot(r.app.slot)));
                let mut slots = row().gap(px(4.0)).px(px(4.0)).py(px(4.0)).child(div().text_color(col::dim()).mr(px(4.0)).child("Slot"));
                for k in 1..=SLOTS {
                    slots = slots.child(chip(format!("slot-{k}"), k.to_string(), slot == k, cx, move |r| r.app.slot = k));
                }
                v.push(slots.into_any_element());
                v.push(sep().into_any_element());
                v.push(self.item("shot", "Screenshot", self.hk("screenshot"), has, false, cx, |r| r.app.screenshot()));
            }
            2 => {
                let k = self.hk("pause");
                let paused = self.app.paused;
                v.push(check("pause", format!("{}   [{k}]", t("Pause", "Pause")), paused, true, cx, |r| r.app.paused = !r.app.paused).into_any_element());
                let adv = self.app.paused;
                v.push(div().id("adv-wrap").child(self.item("advance", t("Einzelbild vor", "Frame advance"), self.hk("frame"), adv, false, cx, |r| {
                    r.app.advance = true;
                    r.ui.open_menu = Some(2);
                })).into_any_element());
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Geschwindigkeit", "Speed")));
                let mut sp = wrap_row().px(px(2.0));
                for (val, n) in [(0.5f32, "50%"), (1.0, "100%"), (1.5, "150%"), (2.0, "200%"), (3.0, "300%")] {
                    sp = sp.child(radio(format!("speed-{n}"), n, s.speed == val, cx, move |r| r.app.s.speed = val));
                }
                v.push(sp.into_any_element());
                v.push(Self::note(format!("{} {} · {} {}", self.hk("double"), t("halten = ×2", "hold = ×2"), self.hk("turbo"), t("halten = Turbo", "hold = turbo"))));
                let ts = s.turbo_speed as f64;
                v.push(slider(self, "turbo", "Turbo", ts, 2.0, 10.0, 0.0, format!("{ts:.1}×"), true, cx, |r, x| r.app.s.turbo_speed = x as f32).into_any_element());
                v.push(sep().into_any_element());
                let ra = s.runahead as f64;
                v.push(slider(self, "runahead", t("Run-Ahead (weniger Verzögerung)", "Run-ahead (less lag)"), ra, 0.0, 3.0, 1.0, format!("{}", ra as u32), true, cx, |r, x| r.app.s.runahead = x.round() as _).into_any_element());
                let k = self.hk("rewind");
                let rw = s.rewind;
                v.push(check("rewind", format!("{} [{k}]", t("Zurückspulen", "Rewind")), rw, true, cx, |r| r.app.s.rewind = !r.app.s.rewind).into_any_element());
                let rs = s.rewind_seconds as f64;
                v.push(slider(self, "rewind_s", t("Sekunden", "seconds"), rs, 5.0, 120.0, 1.0, format!("{}", rs as u32), rw, cx, |r, x| r.app.s.rewind_seconds = x.round() as _).into_any_element());
                chk!("pal60", t("PAL-ROMs mit 60 Hz (flüssiger, gilt ab nächstem Laden)", "PAL ROMs at 60 Hz (smoother, applies on next load)"), s.pal60);
                chk!("vsync_lock", t("An Monitor anpassen (ruckelfrei)", "Sync to monitor (no stutter)"), s.vsync_lock);
                chk!("pause_menus", t("Pause, solange ein Fenster offen ist", "Pause while a window is open"), s.pause_in_menus);
                chk!("auto_pause", t("Automatisch pausieren, wenn das Fenster nicht aktiv ist oder die Karte offen ist", "Pause automatically when the window is not active or the map is open"), s.auto_pause);
            }
            3 => {
                for f in Filter::ALL {
                    let sel = s.filter == f;
                    v.push(
                        radio(format!("filter-{}", f as u8), f.name(en), sel, cx, move |r| {
                            r.app.s.filter = f;
                            if f.is_hd() {
                                r.app.s.last_hd = f;
                            }
                        })
                        .into_any_element(),
                    );
                }
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Breitbild", "Widescreen")));
                let mut ws = wrap_row().px(px(2.0));
                for (val, n) in [(0u8, "4:3"), (1, "16:9"), (2, t("21:9 Ultrawide", "21:9 ultrawide")), (3, t("Fenster füllen", "Fill window"))] {
                    ws = ws.child(radio(format!("ws-{val}"), n, s.widescreen == val, cx, move |r| r.app.s.widescreen = val));
                }
                v.push(ws.into_any_element());
                if s.widescreen != 0 {
                    v.push(Self::note(t("Zeigt links und rechts mehr von der Welt. Am Rand der Oberwelt können kurz Kacheln nachladen.", "Shows more of the world left and right. Tiles may pop in briefly at the overworld edges.")));
                }
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Bildrate", "Frame rate")));
                let hf = s.hi_fps;
                v.push(
                    check("hi_fps", t("Zwischenbilder (flüssig über 60 FPS)", "In-between frames (smooth above 60 FPS)"), hf, true, cx, |r| r.app.s.hi_fps = !r.app.s.hi_fps)
                        .tooltip(tip(t("Scrollen und Figuren werden zwischen zwei Spielbildern interpoliert – wie bei Ship of Harkinian.", "Scrolling and sprites are interpolated between two game frames – like Ship of Harkinian.").to_string()))
                        .into_any_element(),
                );
                let mut fr = wrap_row().px(px(2.0));
                for (val, n) in [(0u32, t("Monitor (VSync)", "Monitor (VSync)")), (120, "120"), (144, "144"), (200, "200"), (240, "240"), (360, "360"), (500, "500"), (1000, t("Unbegrenzt (1000)", "Unlimited (1000)"))] {
                    fr = fr.child(radio(format!("fps-{val}"), n, s.fps_limit == val, cx, move |r| r.app.s.fps_limit = val));
                }
                v.push(fr.into_any_element());
                if s.fps_limit > 0 {
                    v.push(Self::note(t(
                        "Feste Bildrate, auch über der Monitorfrequenz: weniger Verzögerung, der Monitor zeigt aber nur so viele Bilder, wie er kann. Das Spiel läuft weiter mit 60 Spielbildern, Zwischenbilder machen die Bewegung flüssig.",
                        "Fixed frame rate, also above the monitor refresh: less latency, but the monitor only shows as many frames as it can. The game still runs at 60 game frames; in-between frames keep motion smooth.",
                    )));
                }
                let fb = s.frame_blend;
                v.push(check("blend", t("Überblenden (statt Zwischenbilder)", "Frame blending (instead of in-between frames)"), fb, !hf, cx, |r| r.app.s.frame_blend = !r.app.s.frame_blend).into_any_element());
                chk!("show_fps", t("FPS anzeigen", "Show FPS"), s.show_fps);
                {
                    // Recompiled-Modus: nur wählbar, wenn beim Bauen übersetzter Code eingebaut wurde
                    let avail = crate::recomp::Table::available();
                    let rc = s.recompiled && avail;
                    let info = self.app.game.as_ref().and_then(|g| g.snes.recomp.as_ref().map(|t| (t.active, t.total)));
                    let tip_txt = match (avail, info) {
                        (false, _) => t("Nicht eingebaut: erst den ROM-Code aufzeichnen und neu bauen (siehe README).", "Not built in: record the ROM code and rebuild first (see README).").to_string(),
                        (true, Some((a, n))) => format!("{} {a}/{n}", t("Übersetzter ROM-Code statt CPU-Emulation. Spielstände bleiben in beiden Modi gleich. Aktive Blöcke:", "Translated ROM code instead of CPU emulation. Save states stay the same in both modes. Active blocks:")),
                        (true, None) => t("Übersetzter ROM-Code statt CPU-Emulation. Spielstände bleiben in beiden Modi gleich.", "Translated ROM code instead of CPU emulation. Save states stay the same in both modes.").to_string(),
                    };
                    v.push(check("recompiled", t("Recompiled-Modus (übersetzter Code)", "Recompiled mode (translated code)"), rc, avail, cx, |r| r.app.s.recompiled = !r.app.s.recompiled).tooltip(tip(tip_txt)).into_any_element());
                }
                v.push(sep().into_any_element());
                chk_then!("wide_sprites", t("Echtes Breitbild: Gegner und Gegenstände auch im Rand (bis 64 Pixel)", "True widescreen: enemies and items in the margins too (up to 64 px)"), |r: &mut Root| r.app.rebuild_rom(), s.wide_sprites);
                chk!("edge_recomp", t("Rand-Aussehen wie zelda3-Recomp (nur bis zur Gebietsgrenze, dahinter schwarz)", "Edge look like the zelda3 recomp (only up to the area border, black beyond)"), s.edge_recomp);
                chk!("aspect", t("Seitenverhältnis 4:3", "Aspect ratio 4:3"), s.aspect_43);
                chk!("integer", t("Ganzzahlige Skalierung", "Integer scaling"), s.integer_scale);
                let sat = s.saturation as f64;
                v.push(slider(self, "sat", t("Sättigung", "Saturation"), sat, 0.0, 2.0, 0.0, format!("{sat:.2}"), true, cx, |r, x| r.app.s.saturation = x as f32).into_any_element());
                let gam = s.gamma as f64;
                v.push(slider(self, "gamma", "Gamma", gam, 0.5, 2.0, 0.0, format!("{gam:.2}"), true, cx, |r, x| r.app.s.gamma = x as f32).into_any_element());
                chk!("color_rando", t("Farb-Randomizer", "Color randomizer"), s.color_rando);
                v.push(sep().into_any_element());
                v.push(self.item("fullscreen", t("Vollbild", "Fullscreen"), self.hk("fullscreen"), true, false, cx, |r| {
                    let c = r.gctx.clone();
                    r.app.toggle_fullscreen(&c);
                }));
                v.push(self.item("hidebar", t("Menüleiste ausblenden", "Hide menu bar"), self.hk("menu"), true, false, cx, |r| r.app.show_bar = false));
            }
            4 => {
                let k = self.hk("mute");
                let m = s.mute;
                v.push(check("mute", format!("{} [{k}]", t("Stumm", "Mute")), m, true, cx, |r| r.app.s.mute = !r.app.s.mute).into_any_element());
                let vol = s.volume as f64;
                v.push(slider(self, "vol", t("Lautstärke", "Volume"), vol, 0.0, 1.5, 0.0, format!("{:.0}%", vol * 100.0), true, cx, |r, x| r.app.s.volume = x as f32).into_any_element());
                let lat = s.latency_ms as f64;
                v.push(slider(self, "lat", t("Latenz", "Latency"), lat, 30.0, 250.0, 1.0, format!("{} ms", lat as u32), true, cx, |r, x| r.app.s.latency_ms = x.round() as _).into_any_element());
                let names = [t("Gauß-ähnlich (original)", "Gaussian-like (original)"), t("Kubisch (klarer)", "Cubic (clearer)"), "Linear"];
                for (k, n) in names.iter().enumerate() {
                    let k = k as u8;
                    v.push(radio(format!("interp-{k}"), *n, s.interp == k, cx, move |r| r.app.s.interp = k).into_any_element());
                }
                chk!("echo", t("Hall/Echo", "Reverb/echo"), s.echo);
                chk!("mute_turbo", t("Im Turbo stumm", "Mute while fast-forwarding"), s.mute_turbo);
                v.push(Self::heading(t("Kanäle", "Channels")));
                let mut ch = wrap_row().px(px(2.0));
                for k in 0..8 {
                    let on_ = s.voice_mute & (1 << k) == 0;
                    ch = ch.child(check(format!("voice-{k}"), format!("{}", k + 1), on_, true, cx, move |r| r.app.s.voice_mute ^= 1 << k));
                }
                v.push(ch.into_any_element());
            }
            5 => {
                v.push(self.item("controls", t("Tasten belegen…", "Map buttons…"), self.hk("win_controls"), true, false, cx, |r| r.app.win_controls = true));
                v.push(self.item("hotkeys", t("Hotkeys belegen…", "Map hotkeys…"), self.hk("win_hotkeys"), true, false, cx, |r| r.app.win_hotkeys = true));
                v.push(sep().into_any_element());
                chk!("pad_label", t("Controller: A/B nach Beschriftung (Xbox)", "Controller: A/B by label (Xbox)"), s.pad_label_layout);
                let dz = s.deadzone as f64;
                v.push(slider(self, "deadzone", t("Stick-Totzone", "Stick dead zone"), dz, 0.1, 0.9, 0.0, format!("{dz:.2}"), true, cx, |r, x| r.app.s.deadzone = x as f32).into_any_element());
                chk!("block_opp", t("Gegenrichtungen: zuletzt gedrückte gewinnt (flüssige Richtungswechsel)", "Opposite directions: last pressed wins (smooth direction changes)"), s.block_opposite);
                chk!("dpad_qm", t("Controller: Steuerkreuz rechts = Schnellmenü", "Controller: d-pad right = quick menu"), s.dpad_quickmenu);
                chk!("dpad_nomove", t("Controller: Steuerkreuz bewegt Link nicht (↑ mit Spiegel: Welt wechseln)", "Controller: d-pad does not move Link (↑ with mirror: switch worlds)"), s.dpad_no_move);
                v.push(Self::note(format!("{} {}", t("Tastatur: Schnellmenü =", "Keyboard: quick menu ="), self.hk("quickmenu"))));
            }
            6 => {
                chk!("fast_text", t("Schnellere Dialoge: Text sofort anzeigen", "Faster dialogs: show text instantly"), s.fast_text);
                chk!("auto_collect", t("Herzen und Rubine automatisch einsammeln", "Collect hearts and rupees automatically"), s.auto_collect);
                chk!("mirror_direct", t("Spiegel direkt aus dem Inventar verwenden", "Use the mirror directly from the inventory"), s.mirror_direct);
                chk!("x_slot", t("X = zweiter Item-Slot (im Inventar mit X belegen, Steuerkreuz ↑ Auswahlleiste)", "X = second item slot (assign with X in the inventory, d-pad ↑ selector)"), s.x_slot);
                chk!("pixel_inv", t("Neues Inventar im Pixel-Look (Start)", "New pixel-style inventory (Start)"), s.pixel_inv);
                chk!("minus_map", t("Controller − / Select öffnet die Karte", "Controller − / Select opens the map"), s.minus_map);
                chk_then!("health_blink", t("Kein Warnton bei wenig Energie – Herz blinkt am Bildrand", "No low-health beep – heart blinks at the screen edge"), |r: &mut Root| r.app.rebuild_rom(), s.health_blink);
                chk!("fairy", t("Hinweis, wenn eine Fee aus der Flasche rettet", "Notice when a bottled fairy saves you"), s.fairy_notice);
                chk_then!("quick_dash", t("Kürzerer Anlauf beim Pegasus-Sprint", "Shorter Pegasus Boots charge"), |r: &mut Root| r.app.rebuild_rom(), s.quick_dash);
                chk_then!("more_bombs", t("Mehrere Bomben gleichzeitig (bis zu 4)", "More bombs at once (up to 4)"), |r: &mut Root| r.app.rebuild_rom(), s.more_bombs);
                chk_then!("sword_pots", t("Töpfe mit dem Schwert zerstören (ab Master-Schwert)", "Break pots with the sword (Master Sword and up)"), |r: &mut Root| r.app.rebuild_rom(), s.sword_pots);
                v.push(Self::heading(t("Szenenwechsel (Gebiete, Räume, Laden)", "Scene transitions (areas, rooms, loading)")));
                let mut tr_ = wrap_row().px(px(2.0));
                for (val, n) in [(1.0f32, t("normal", "normal")), (2.0, t("2× (Standard)", "2× (default)")), (3.0, "3×"), (4.0, "4×")] {
                    tr_ = tr_.child(radio(format!("trans-{val}"), n, s.transition_speed == val, cx, move |r| r.app.s.transition_speed = val));
                }
                v.push(tr_.into_any_element());
            }
            7 => {
                chk!("c_health", t("Unendlich Herzen", "Infinite health"), s.cheats.inf_health);
                chk!("c_magic", t("Unendlich Magie", "Infinite magic"), s.cheats.inf_magic);
                chk!("c_noclip", t("Noclip (durch Wände)", "Noclip (walk through walls)"), s.cheats.noclip);
                chk!("c_skip", t("Intro überspringen", "Skip intro"), s.cheats.skip_title);
                chk!("c_bombs", t("Bomben nie unter 10", "Bombs never below 10"), s.cheats.inf_bombs);
                chk!("c_arrows", t("Pfeile nie unter 30", "Arrows never below 30"), s.cheats.inf_arrows);
                chk!("c_rupees", t("999 Rubine", "999 rupees"), s.cheats.max_rupees);
                chk!("c_keys", t("Unendlich Schlüssel", "Infinite keys"), s.cheats.inf_keys);
                v.push(sep().into_any_element());
                v.push(self.item("cheats_win", t("Alle Cheats, Items & Codes…", "All cheats, items & codes…"), self.hk("win_cheats"), true, false, cx, |r| r.app.win_cheats = true));
            }
            8 => {
                // Mods: Liste im eigenen Fenster (installieren, an/aus), dazu Ordner und Vorlage
                v.push(Self::heading(t("Mods (in Rust geschrieben)", "Mods (written in Rust)")));
                let n_on = s.mods_on.len();
                v.push(self.item("mods_list", t("Mod-Liste…", "Mod list…"), if n_on > 0 { format!("{n_on} {}", t("an", "on")) } else { String::new() }, true, true, cx, |r| {
                    r.app.win_mods = true;
                    if r.app.mods.shop.status() == crate::mods::shop::Status::Idle {
                        r.app.mods.shop.refresh();
                    }
                }));
                v.push(sep().into_any_element());
                v.push(self.item("mods_reload", t("Mods neu laden", "Reload mods"), String::new(), true, false, cx, |r| r.app.mods.load_dlls()));
                v.push(self.item("mods_dir", t("Mod-Ordner öffnen", "Open mods folder"), String::new(), true, false, cx, |_| {
                    let d = crate::mods::Mods::dir();
                    let _ = std::fs::create_dir_all(&d);
                    let _ = std::process::Command::new("explorer").arg(&d).spawn();
                }));
                v.push(self.item("mods_tpl", t("Mod-Vorlage exportieren…", "Export mod template…"), String::new(), true, false, cx, |r| {
                    let en = r.app.s.ui_en;
                    if let Some(dir) = rfd::FileDialog::new().set_title(tr(en, "Ordner für die Mod-Vorlage wählen", "Choose a folder for the mod template")).pick_folder() {
                        let m = match crate::mods::export_template(&dir) {
                            Ok(p) => format!("{} {}", tr(en, "Mod-Vorlage gespeichert:", "Mod template saved:"), p.display()),
                            Err(e) => format!("{} {e}", tr(en, "Mod-Vorlage nicht gespeichert:", "Mod template not saved:")),
                        };
                        r.app.msg(m);
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    }
                }));
            }
            _ => {}
        }
        v
    }

    // ------------------------------------------------------------ Fenster

    /// Fenster im Stil der Spielmenüs: schwarz, goldener Rahmen, Titel in der Spielschrift
    fn game_window(&self, id: &'static str, title: &str, width: f32, max_h: f32, body: Vec<AnyElement>, cx: &mut Context<Root>, close: impl Fn(&mut Root) + 'static) -> AnyElement {
        let (ww, wh) = self.ui.window_size;
        let order = ["rando", "cheats", "controls", "hotkeys", "help", "unknown", "marker", "mods", "gbacheats"].iter().position(|x| *x == id).unwrap_or(0) as f32;
        let pos = self.ui.win_pos.get(id).copied().unwrap_or_else(|| point(px(((ww - width) / 2.0 + order * 24.0 - 48.0).max(8.0)), px((MENU_H + 24.0 + order * 18.0).min(wh * 0.3))));
        let max_body = (wh - f32::from(pos.y) - 90.0).clamp(160.0, max_h);
        let title_bar = div()
            .id(eid(format!("win-title-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(18.0))
            .h(px(48.0))
            .cursor_move()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                    this.ui.drag = Some(Drag::Window { id, grab: e.position - pos });
                }),
            )
            .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(col::text()).child(clean(title)))
            .child(
                div()
                    .id(eid(format!("win-close-{id}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(26.0))
                    .rounded(px(6.0))
                    .text_color(col::dim())
                    .text_size(px(16.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(col::hover()).text_color(col::text()))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(on::<ClickEvent>(cx, close))
                    .child("✕"),
            );
        div()
            .id(eid(format!("win-{id}")))
            .occlude()
            .absolute()
            .left(pos.x)
            .top(pos.y)
            .w(px(width))
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(0x2c3037))
            .bg(col::game_bg())
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(title_bar)
            .child(div().h(px(1.0)).bg(col::card_line()))
            .child(div().id(eid(format!("win-body-{id}"))).max_h(px(max_body)).overflow_y_scroll().px(px(18.0)).pt(px(4.0)).pb(px(18.0)).flex().flex_col().gap(px(4.0)).children(body))
            .into_any_element()
    }

    fn windows(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        if self.app.game.is_none() {
            self.app.win_rando = false;
            self.app.win_cheats = false;
        }
        let mut v = Vec::new();
        if self.app.win_rando {
            let body = self.rando_ui(cx);
            v.push(self.game_window("rando", "Randomizer", 580.0, 640.0, body, cx, |r| r.app.win_rando = false));
        }
        if self.app.win_cheats {
            let body = self.cheats_ui(cx);
            v.push(self.game_window("cheats", "Cheats", 620.0, 660.0, body, cx, |r| r.app.win_cheats = false));
        }
        if self.app.gba.win_cheats && self.app.gba.active() {
            let body = self.gba_cheats_ui(cx);
            v.push(self.game_window("gbacheats", t("Cheat-Liste", "Cheat list"), 560.0, 640.0, body, cx, |r| r.app.gba.win_cheats = false));
        } else {
            self.app.gba.win_cheats = false;
        }
        if self.app.win_mods {
            let body = self.mods_ui(cx);
            v.push(self.game_window("mods", t("Mod-Liste", "Mod list"), 600.0, 680.0, body, cx, |r| r.app.win_mods = false));
        }
        if let Some(id) = self.app.win_mod_settings.clone() {
            let name = self.app.mods.entries(en).into_iter().find(|e| e.0 == id).map(|e| e.1).unwrap_or_default();
            let body = self.mod_settings_ui(&id, cx);
            let title = format!("{}: {name}", t("Einstellungen", "Settings"));
            v.push(self.game_window("modsettings", &title, 460.0, 420.0, body, cx, |r| r.app.win_mod_settings = None));
        }
        if self.app.win_controls {
            let body = self.controls_ui(cx);
            v.push(self.game_window("controls", t("Tastenbelegung", "Controls"), 560.0, 640.0, body, cx, |r| r.app.win_controls = false));
        }
        if self.app.win_hotkeys {
            let body = self.hotkeys_ui(cx);
            v.push(self.game_window("hotkeys", "Hotkeys", 640.0, 660.0, body, cx, |r| r.app.win_hotkeys = false));
        }
        if self.app.unknown_rom.is_some() {
            let title = self.app.unknown_rom.as_ref().map(|(r, _)| alttp::prepare_rom(r.clone()).map(|x| x.1.title).unwrap_or_default()).unwrap_or_default();
            let body = vec![
                label(format!("»{title}« {}", t("sieht nicht nach A Link to the Past aus.", "does not look like A Link to the Past."))).into_any_element(),
                row()
                    .pt(px(8.0))
                    .child(button("unk-load", t("Trotzdem laden", "Load anyway"), true, cx, |r| {
                        if let Some((rom, path)) = r.app.unknown_rom.take() {
                            r.app.load_rom_data(rom, path, true);
                        }
                    }))
                    .child(button("unk-cancel", t("Abbrechen", "Cancel"), true, cx, |r| r.app.unknown_rom = None))
                    .into_any_element(),
            ];
            v.push(self.game_window("unknown", t("Unbekannte ROM", "Unknown ROM"), 460.0, 300.0, body, cx, |r| r.app.unknown_rom = None));
        }
        if self.app.new_marker.is_some() {
            let mut kinds = wrap_row();
            for k in MarkerKind::ALL {
                let c = k.color();
                let sel = self.app.marker_kind == k;
                kinds = kinds.child(chip(format!("mk-{k:?}"), format!("{} {}", k.glyph(), k.name(en)), sel, cx, move |r| r.app.marker_kind = k).text_color(rgb(((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32)));
            }
            let lbl = self.app.marker_label.clone();
            let body = vec![
                kinds.into_any_element(),
                div().pt(px(6.0)).child(text_field(self, TextField::MarkerLabel, &lbl, t("Beschriftung (optional)", "Label (optional)"), 300.0, cx)).into_any_element(),
                row().pt(px(8.0)).child(button("mk-ok", "OK", true, cx, |r| r.submit_marker(true))).child(button("mk-cancel", t("Abbrechen", "Cancel"), true, cx, |r| r.submit_marker(false))).into_any_element(),
            ];
            v.push(self.game_window("marker", t("Neuer Marker", "New marker"), 420.0, 400.0, body, cx, |r| r.submit_marker(false)));
        }
        v
    }

    fn submit_marker(&mut self, ok: bool) {
        if let Some((w, x, y)) = self.app.new_marker.take() {
            if ok {
                let kind = self.app.marker_kind;
                let label = self.app.marker_label.trim().to_string();
                if let Some(g) = &mut self.app.game {
                    g.tracker.markers.push(Marker { world: w, x, y, kind, label, done: false, auto: false });
                    g.tracker.save_dirty = true;
                }
            }
        }
        if self.ui.text_focus == Some(TextField::MarkerLabel) {
            self.ui.text_focus = None;
        }
    }

    // ------------------------------------------------------------ Randomizer

    fn rando_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let found = self.app.game.as_ref().map(|g| g.chests.as_ref().map(|c| c.chests.len()));
        let on_ = self.app.rando_active();
        let mut v = Vec::new();
        let enabled = matches!(found, Some(Some(_)));
        let status = match found {
            None => div().text_color(col::dim()).child(t("Lade zuerst deine ROM.", "Load your ROM first.")),
            Some(Some(n)) => div().text_color(col::dim()).child(format!("{n} {} · 0x{:05X}", t("Truhen gefunden", "chests found"), self.app.game.as_ref().and_then(|g| g.chests.as_ref()).map_or(0, |c| c.offset))),
            Some(None) => div().text_color(col::red()).child(t("Truhentabelle in dieser ROM nicht gefunden", "Chest table not found in this ROM")),
        };
        let text = div().flex().flex_col().gap(px(2.0)).py(px(4.0)).child(strong(t("Truhen-Randomizer", "Chest randomizer"))).child(status.text_size(px(12.0)));
        let head = switch("rando-on", text, on_, enabled, cx, move |r| {
            if !on_ && r.app.s.rando_seed.trim().is_empty() {
                r.app.new_rando_seed();
            }
            r.app.set_rando(!on_);
        });
        v.push(div().pt(px(8.0)).child(div().p(px(8.0)).rounded(px(8.0)).bg(col::card()).border_1().border_color(if on_ { col::gold_line() } else { col::card_line() }).child(head)).into_any_element());
        let mut set = Vec::new();
        let modes = [
            (t("Leicht", "Easy"), t("Nur Rubine, Munition, Herzteile & Herzcontainer werden gemischt – immer lösbar.", "Only rupees, ammo, heart pieces & containers are shuffled – always beatable.")),
            ("Normal", t("Auch Ausrüstung (Bogen, Enterhaken, Stiefel …) wird gemischt. Schlüssel/Karten bleiben. Nicht garantiert lösbar – Noclip hilft.", "Equipment (bow, hookshot, boots …) is shuffled too. Keys/maps stay. Not guaranteed beatable – noclip helps.")),
            ("Chaos", t("Alles wird gemischt, auch Schlüssel.", "Everything is shuffled, keys too.")),
        ];
        let cur = (self.app.s.rando_mode as usize).min(2);
        let names: Vec<&str> = modes.iter().map(|m| m.0).collect();
        set.push(segmented("rmode", &names, cur, cx, |r, i| r.app.s.rando_mode = i as u8).into_any_element());
        set.push(small(modes[cur].1).px(px(2.0)).into_any_element());
        let seed = self.app.s.rando_seed.clone();
        set.push(
            row()
                .pt(px(6.0))
                .child(div().w(px(44.0)).text_color(col::dim()).child("Seed"))
                .child(text_field(self, TextField::RandoSeed, &seed, t("leer = zufällig", "empty = random"), 190.0, cx))
                .child(button("reroll", t("Neu würfeln", "Re-roll"), true, cx, |r| {
                    r.app.new_rando_seed();
                    if r.app.rando_active() {
                        r.app.apply_rando();
                    }
                }))
                .into_any_element(),
        );
        let rr = self.app.s.rando_reset;
        set.push(switch("rando_reset", t("Beim An-/Ausschalten neu starten (empfohlen, dann neues Spiel beginnen)", "Restart when switching (recommended, then start a new game)"), rr, true, cx, |r| r.app.s.rando_reset = !r.app.s.rando_reset).into_any_element());
        if on_ {
            set.push(row().child(button("reapply", t("Mit diesen Einstellungen neu anwenden", "Re-apply with these settings"), true, cx, |r| r.app.apply_rando())).into_any_element());
        }
        v.push(section(t("Einstellungen", "Settings"), set).into_any_element());
        if let Some(r) = self.app.game.as_ref().and_then(|g| g.rando.as_ref()) {
            let changed = r.spoiler.iter().filter(|(_, _, a, b)| a != b).count();
            let mode = match r.mode {
                crate::randomizer::Mode::Safe => t("Leicht", "Easy"),
                crate::randomizer::Mode::Mixed => "Normal",
                crate::randomizer::Mode::Chaos => "Chaos",
            };
            let mut sp = vec![label(format!("Seed {} · {mode} · {changed} {}", r.seed, t("Truhen verändert", "chests changed"))).into_any_element()];
            let show = self.app.show_spoiler;
            let text = show.then(|| crate::randomizer::spoiler_text(r, en));
            let seed = r.seed;
            sp.push(switch("spoiler", t("Spoiler-Log anzeigen (verrät alles!)", "Show spoiler log (reveals everything!)"), show, true, cx, |r| r.app.show_spoiler = !r.app.show_spoiler).into_any_element());
            if let Some(text) = text {
                sp.push(
                    div()
                        .id("spoiler-log")
                        .max_h(px(220.0))
                        .overflow_y_scroll()
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(col::panel_line())
                        .bg(col::field())
                        .font_family(MONO_FONT)
                        .text_size(px(12.0))
                        .children(text.lines().map(|l| div().child(l.to_string())).collect::<Vec<_>>())
                        .into_any_element(),
                );
                let text2 = text.clone();
                sp.push(
                    row()
                        .child(button("spoiler-save", t("Als Datei speichern…", "Save to file…"), true, cx, move |_| {
                            if let Some(p) = rfd::FileDialog::new().set_file_name(format!("spoiler_{seed}.txt")).save_file() {
                                let _ = std::fs::write(p, &text2);
                            }
                        }))
                        .into_any_element(),
                );
            }
            v.push(section("Spoiler", sp).into_any_element());
        }
        let online = vec![
            small(t("Für einen Randomizer mit vollständiger Logik: Seed auf alttpr.com erzeugen und die .sfc hier öffnen.", "For a randomizer with full logic: create a seed on alttpr.com and open the .sfc here.")).into_any_element(),
            row()
                .pt(px(4.0))
                .child(
                    div()
                        .id("alttpr-link")
                        .text_color(col::gold())
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .on_click(|_, _, cx| cx.open_url("https://alttpr.com/de/randomizer"))
                        .child("alttpr.com"),
                )
                .child(button("r-open", t("ROM öffnen…", "Open ROM…"), true, cx, |r| r.app.open_dialog()))
                .child(button("r-patch", t("Patch anwenden…", "Apply patch…"), true, cx, |r| r.app.patch_dialog()))
                .into_any_element(),
        ];
        v.push(section(t("Online-Randomizer (optional)", "Online randomizer (optional)"), online).into_any_element());
        v
    }

    // ------------------------------------------------------------ Cheats

    /// Fenster „Mod-Liste“: alle Mods aus dem GitHub-Release „mods“ und eigene DLLs – holen, aktualisieren, an/aus
    /// (jede Mod mit Bild und den Spielen, in denen sie läuft)
    fn mods_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        use crate::mods::shop::{Have, Status};
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let mut v = Vec::new();
        if !self.app.s.recompiled {
            v.push(small(t("A Link to the Past: Mods laufen nur im RC-Modus (Knopf „RC“ oben rechts). In The Minish Cap laufen sie immer.", "A Link to the Past: mods only run in RC mode (\"RC\" button top right). In The Minish Cap they always run.")).into_any_element());
        }
        let shop = self.app.mods.shop.clone();
        if shop.take_icons_changed() {
            MOD_ICONS.with(|c| c.borrow_mut().clear());
        }
        // gerade geholt/aktualisiert: sofort laden (auch ohne laufendes Spiel)
        if shop.take_new() {
            self.app.mods.load_dlls();
        }
        let installed = self.app.mods.entries(en);
        let on_ids = self.app.s.mods_on.clone();
        let mods_ref = &self.app.mods;
        // Zeile: Bild, Name + Spiele + Beschreibung, rechts Schalter (installiert) und Knopf (holen/Update)
        let mod_row = |id: String, name: String, desc: String, have: Option<Have>, file: Option<crate::mods::shop::Item>, cx: &mut Context<Root>| {
            let key = format!("dll:{}", file.as_ref().map(|f| f.file.clone()).unwrap_or_default());
            let key = if file.is_some() { key } else { id.clone() };
            let is_inst = installed.iter().any(|e| e.0 == key);
            let on = on_ids.contains(&key);
            // Spiele: aus der Mod-Liste (neueste Version), sonst aus der geladenen Mod; Bild: aus der Mod, sonst Liste
            let meta = mods_ref.meta(&key);
            let (alttp, minish) = match (&file, &meta) {
                (Some(it), _) => it.for_games(),
                (None, Some((g, _))) => (g & 1 != 0, g & 2 != 0),
                _ => (true, false),
            };
            let has_settings = is_inst && !mods_ref.settings_of(&key).is_empty();
            let icon = match meta.as_ref().and_then(|m| m.1.clone()) {
                Some(ic) => mod_icon_from(&key, &ic),
                None => file.as_ref().and_then(|it| it.icon_path()).and_then(|p| mod_icon_file(&p)),
            };
            let games = match (alttp, minish) {
                (true, true) => "A Link to the Past · The Minish Cap",
                (false, true) => "The Minish Cap",
                _ => "A Link to the Past",
            };
            // Text darf schrumpfen und umbrechen (sonst läuft er unter die Knöpfe)
            let text = div().flex().flex_col().flex_1().min_w(px(0.0)).gap(px(2.0)).child(strong(name)).child(div().text_size(px(11.0)).text_color(col::faint()).child(games)).child(small(desc));
            let pic = match icon {
                Some(img) => gpui::img(gpui::ImageSource::Render(img)).w(px(32.0)).h(px(32.0)).flex_none().into_any_element(),
                None => div().w(px(32.0)).h(px(32.0)).flex_none().rounded(px(6.0)).bg(col::faint()).opacity(0.25).into_any_element(),
            };
            let text = row().gap(px(10.0)).items_center().child(pic).child(text);
            let k2 = key.clone();
            let sw = switch(format!("modsw-{id}"), text, on, is_inst, cx, move |r| {
                if let Some(i) = r.app.s.mods_on.iter().position(|x| *x == k2) {
                    r.app.s.mods_on.remove(i);
                } else {
                    r.app.s.mods_on.push(k2.clone());
                }
            });
            let mut r = row().gap(px(6.0)).items_center().child(div().flex_1().min_w(px(0.0)).child(sw));
            let dll_file = key.strip_prefix("dll:").unwrap_or("").to_string();
            // Knöpfe als Symbole: Holen/Update (Pfeil), Einstellungen (Zahnrad), Deinstallieren (Papierkorb)
            if let (Some(h), Some(it)) = (have, file) {
                match h {
                    Have::No | Have::Update => {
                        let upd = h == Have::Update;
                        r = r.child(icon_button(format!("modget-{id}"), if upd { "update" } else { "get" }, true, cx, move |r| {
                            let loaded = r.app.mods.is_loaded(&it.file);
                            r.app.mods.shop.install(&it, loaded);
                        }));
                    }
                    Have::Busy | Have::Restart => r = r.child(small("…")),
                    Have::Installed => {}
                }
            }
            if has_settings {
                let k3 = key.clone();
                r = r.child(icon_button(format!("modset-{id}"), "gear", true, cx, move |r| r.app.win_mod_settings = Some(k3.clone())));
            }
            if is_inst && !dll_file.is_empty() {
                let (f2, k4) = (dll_file.clone(), key.clone());
                r = r.child(icon_button(format!("moddel-{id}"), "trash", true, cx, move |r| {
                    r.app.s.mods_on.retain(|x| *x != k4);
                    if let Err(e) = r.app.mods.uninstall(&f2) {
                        r.app.msg(format!("{f2}: {e}"));
                    }
                }));
            }
            // Rechtsklick auf die Zeile: Einstellungen
            let r = r.id(eid(format!("modrow-{id}")));
            let r = if has_settings {
                let k6 = key.clone();
                r.on_mouse_down(MouseButton::Right, self::on::<MouseDownEvent>(cx, move |r| r.app.win_mod_settings = Some(k6.clone())))
            } else {
                r
            };
            r.into_any_element()
        };
        let mut rows = Vec::new();
        match shop.status() {
            Status::Idle => {
                shop.refresh();
                rows.push(small(t("Lade die Mod-Liste …", "Loading the mod list …")).into_any_element());
            }
            Status::Loading => rows.push(small(t("Lade die Mod-Liste …", "Loading the mod list …")).into_any_element()),
            Status::Error(e) => {
                rows.push(small(format!("{} {e}", t("Mod-Liste nicht erreichbar:", "Mod list not reachable:"))).into_any_element());
                rows.push(row().child(button("mods-retry", t("Nochmal versuchen", "Try again"), true, cx, |r| r.app.mods.shop.refresh())).into_any_element());
            }
            Status::Ready => {
                for it in shop.items() {
                    let have = shop.have(&it);
                    let name = if en && !it.name_en.is_empty() { it.name_en.clone() } else { it.name.clone() };
                    let desc = if en && !it.desc_en.is_empty() { it.desc_en.clone() } else { it.desc.clone() };
                    let id = it.id.clone();
                    let err = shop.error_of(&it.id);
                    rows.push(mod_row(id, format!("{name}  v{}", it.version), desc, Some(have), Some(it), cx));
                    if let Some(e) = err {
                        rows.push(small(e).into_any_element());
                    }
                }
            }
        }
        v.push(section(t("Mods", "Mods"), rows).into_any_element());
        // eigene DLLs, die nicht in der Liste stehen
        let listed: Vec<String> = shop.items().iter().map(|i| format!("dll:{}", i.file)).collect();
        let own: Vec<(String, String, String)> = installed.iter().filter(|e| !listed.contains(&e.0)).cloned().collect();
        if !own.is_empty() && shop.status() == Status::Ready {
            let mut rows = Vec::new();
            for (id, name, desc) in own {
                rows.push(mod_row(id.clone(), name, desc, None, None, cx));
            }
            v.push(section(t("Eigene Mods", "Own mods"), rows).into_any_element());
        }
        for e in self.app.mods.errors.clone() {
            v.push(small(e).into_any_element());
        }
        // Knöpfe unten
        let missing: Vec<crate::mods::shop::Item> = shop.items().into_iter().filter(|it| matches!(shop.have(it), Have::No | Have::Update)).collect();
        let mut bar = row().pt(px(10.0)).gap(px(8.0));
        if !missing.is_empty() {
            let n = missing.len();
            bar = bar.child(button("mods-all", format!("{} ({n})", t("Alle holen", "Get all")), true, cx, move |r| {
                for it in &missing {
                    let loaded = r.app.mods.is_loaded(&it.file);
                    r.app.mods.shop.install(it, loaded);
                }
            }));
        }
        bar = bar.child(button("mods-refresh", t("Liste aktualisieren", "Refresh list"), shop.status() != Status::Loading, cx, |r| r.app.mods.shop.refresh()));
        // alle Mods mit einem Klick deinstallieren
        let inst: Vec<String> = installed.iter().filter_map(|e| e.0.strip_prefix("dll:").map(|f| f.to_string())).collect();
        if !inst.is_empty() {
            bar = bar.child(button("mods-uninstall-all", format!("{} ({})", t("Alle deinstallieren", "Uninstall all"), inst.len()), true, cx, move |r| {
                for f in &inst {
                    r.app.s.mods_on.retain(|x| *x != format!("dll:{f}"));
                    if let Err(e) = r.app.mods.uninstall(f) {
                        r.app.msg(format!("{f}: {e}"));
                    }
                }
            }));
        }
        bar = bar.child(button("mods-reload", t("Mods neu laden", "Reload mods"), true, cx, |r| r.app.mods.load_dlls()));
        v.push(bar.into_any_element());
        v
    }

    /// Fenster „Einstellungen“ einer Mod: Schalter, Zahlen (−/+), Positionen (auf dem Spielbild ziehen)
    fn mod_settings_ui(&mut self, id: &str, cx: &mut Context<Root>) -> Vec<AnyElement> {
        use crate::mods::api::setting;
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let defs = self.app.mods.settings_of(id);
        let vals = self.app.s.mod_values.get(id).cloned().unwrap_or_default();
        let mut rows = Vec::new();
        // Wert setzen bzw. entfernen (None = Standard der Mod) und an die Mods geben
        fn set(r: &mut Root, id: &str, key: &str, v: Option<i32>) {
            let m = r.app.s.mod_values.entry(id.to_string()).or_default();
            match v {
                Some(v) => {
                    m.insert(key.to_string(), v);
                }
                None => {
                    m.remove(key);
                }
            }
            r.app.mods.set_values(&r.app.s.mod_values);
        }
        for d in defs {
            let label = if en { d.label_en.clone() } else { d.label_de.clone() };
            let cur = vals.get(&d.key).copied();
            let (idk, kk) = (id.to_string(), d.key.clone());
            match d.kind {
                setting::BOOL => {
                    let on = cur.unwrap_or(d.def) != 0;
                    rows.push(switch(format!("ms-{}", d.key), label, on, true, cx, move |r| set(r, &idk, &kk, Some((!on) as i32))).into_any_element());
                }
                setting::INT => {
                    let v = cur.unwrap_or(d.def).clamp(d.min, d.max);
                    let step = ((d.max - d.min) / 20).max(1);
                    let (min, max) = (d.min, d.max);
                    let (i2, k2) = (idk.clone(), kk.clone());
                    rows.push(
                        row()
                            .gap(px(8.0))
                            .items_center()
                            .px(px(6.0))
                            .child(div().flex_1().min_w(px(0.0)).child(label))
                            .child(button(format!("ms-{}-m", d.key), "−", v > min, cx, move |r| set(r, &idk, &kk, Some((v - step).max(min)))))
                            .child(div().w(px(56.0)).flex().justify_center().font_family(MONO_FONT).child(v.to_string()))
                            .child(button(format!("ms-{}-p", d.key), "+", v < max, cx, move |r| set(r, &i2, &k2, Some((v + step).min(max)))))
                            .into_any_element(),
                    );
                }
                _ => {
                    let txt = match cur {
                        Some(v) => format!("{}, {}", v as u32 as u16 as i16, (v as u32 >> 16) as u16 as i16),
                        None => t("Standard", "Default").to_string(),
                    };
                    let (i2, k2) = (idk.clone(), kk.clone());
                    rows.push(
                        row()
                            .gap(px(8.0))
                            .items_center()
                            .px(px(6.0))
                            .child(div().flex_1().min_w(px(0.0)).child(label))
                            .child(div().font_family(MONO_FONT).text_color(col::dim()).child(txt))
                            .child(button(format!("ms-{}-drag", d.key), t("Auf dem Bild ziehen", "Drag on screen"), true, cx, move |r| {
                                r.app.mod_place = Some((idk.clone(), kk.clone()));
                                r.app.win_mods = false;
                                r.app.win_mod_settings = None;
                            }))
                            .child(button(format!("ms-{}-def", d.key), t("Standard", "Default"), cur.is_some(), cx, move |r| set(r, &i2, &k2, None)))
                            .into_any_element(),
                    );
                }
            }
        }
        let mut v = vec![section(t("Einstellungen", "Settings"), rows).into_any_element()];
        let idr = id.to_string();
        v.push(
            row()
                .pt(px(10.0))
                .gap(px(8.0))
                .child(button("ms-reset", t("Alles zurücksetzen", "Reset all"), !vals.is_empty(), cx, move |r| {
                    r.app.s.mod_values.remove(&idr);
                    r.app.mods.set_values(&r.app.s.mod_values);
                }))
                .into_any_element(),
        );
        v
    }

    fn cheats_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let has = self.app.game.is_some();
        let hk = |a: &Core, id: &str| a.s.hotkey(id).map(|c| c.to_text()).unwrap_or_default();
        // Text mit Tastenkürzel (dezent daneben)
        let with_hk = |txt: &str, k: &str| row().gap(px(10.0)).child(txt.to_string()).child(div().font_family(MONO_FONT).text_size(px(11.0)).text_color(col::faint()).child(k.to_string()));
        let (hk_h, hk_m, hk_n) = (hk(&self.app, "inf_health"), hk(&self.app, "inf_magic"), hk(&self.app, "noclip"));
        let mut v = Vec::new();
        macro_rules! c {
            ($id:expr, $txt:expr, $f:ident) => {{
                let val = self.app.s.cheats.$f;
                switch($id, $txt, val, true, cx, |r| r.app.s.cheats.$f = !r.app.s.cheats.$f).into_any_element()
            }};
        }
        v.push(
            section(
                t("Spiel durchgespielt", "Game completed"),
                vec![
                    small(t("Setzt alles auf 100 %: alle Items, 20 Herzen, alle Herzteile, Truhen, Amulette und Kristalle.", "Sets everything to 100 %: all items, 20 hearts, all heart pieces, chests, pendants and crystals.")).into_any_element(),
                    row().pt(px(4.0)).child(button("c100", t("Alles auf 100 % setzen", "Set everything to 100 %"), has, cx, |r| r.app.complete_100())).into_any_element(),
                ],
            )
            .into_any_element(),
        );
        let n = self.app.game.as_ref().map_or(0, |g| g.deaths);
        v.push(
            section(
                t("Tode", "Deaths"),
                vec![
                    small(format!("{}: {n}", t("Tode im laufenden Spiel", "Deaths in the current game"))).into_any_element(),
                    row()
                        .pt(px(4.0))
                        .child(button("d-reset", t("Tode zurücksetzen", "Reset deaths"), has, cx, |r| r.app.reset_deaths(false)))
                        .child(button("d-reset-all", t("… auch in allen Spielständen", "… also in all save states"), has, cx, |r| r.app.reset_deaths(true)))
                        .into_any_element(),
                ],
            )
            .into_any_element(),
        );
        let player = vec![
            c!("c-h", with_hk(t("Unendlich Herzen", "Infinite health"), &hk_h), inf_health),
            c!("c-m", with_hk(t("Unendlich Magie", "Infinite magic"), &hk_m), inf_magic),
            c!("c-hm", t("Halber Magieverbrauch", "Half magic usage"), half_magic),
            c!("c-mp", t("Immer Mondperle", "Always Moon Pearl"), moon_pearl),
            c!("c-inv", t("Unverwundbar (kein Schaden, kein Rückstoß)", "Invincible (no damage, no knockback)"), invincible),
            c!("c-fb", t("Flaschen immer mit Fee", "Bottles always hold a fairy"), fairy_bottles),
            c!("c-light", t("Immer hell (dunkle Räume)", "Always bright (dark rooms)"), always_light),
        ];
        v.push(section(t("Spieler", "Player"), vec![grid(2, player, 12.0).into_any_element()]).into_any_element());
        let nc = self.app.s.cheats.noclip;
        v.push(
            section(
                t("Bewegung", "Movement"),
                vec![
                    switch("c-noclip", with_hk(t("Noclip – durch Wände gehen", "Noclip – walk through walls"), &hk_n), nc, true, cx, |r| r.app.s.cheats.noclip = !r.app.s.cheats.noclip).into_any_element(),
                    small(t("Zum Ausschalten auf freie Fläche gehen.", "Walk to open ground before turning it off.")).px(px(6.0)).into_any_element(),
                ],
            )
            .into_any_element(),
        );
        v.push(
            section(
                t("Spielstart", "Game start"),
                vec![
                    c!("c-skip", t("Intro & Titelbild überspringen (Standard)", "Skip intro & title screen (default)"), skip_title),
                    c!("c-prol", t("Prolog überspringen – neues Spiel beginnt nach Zeldas Rettung (experimentell)", "Skip prologue – new game starts after rescuing Zelda (experimental)"), skip_prologue),
                ],
            )
            .into_any_element(),
        );
        let inv = vec![
            c!("c-b", t("Bomben nie unter 10", "Bombs never below 10"), inf_bombs),
            c!("c-a", t("Pfeile nie unter 30", "Arrows never below 30"), inf_arrows),
            c!("c-r", t("999 Rubine", "999 rupees"), max_rupees),
            c!("c-k", t("Unendlich kleine Schlüssel", "Infinite small keys"), inf_keys),
            c!("c-cap", t("Größte Bomben- und Pfeiltasche", "Largest bomb and arrow capacity"), max_capacity),
            c!("c-di", t("Alle Karten, Kompasse, Großen Schlüssel", "All maps, compasses, big keys"), dungeon_items),
        ];
        let mut inv_b = vec![grid(2, inv, 12.0).into_any_element()];
        inv_b.push(
            row()
                .pt(px(4.0))
                .child(button("give-all", t("Alle Items geben", "Give all items"), has, cx, move |r| {
                    if let Some(g) = r.app.game.as_mut() {
                        alttp::give_all_items(&mut g.snes);
                    }
                    r.app.status = Some((tr(en, "Alle Items erhalten", "All items given").into(), Instant::now()));
                }))
                .child(button("give-20", t("20 Herzcontainer", "20 heart containers"), has, cx, move |r| {
                    if let Some(g) = r.app.game.as_mut() {
                        alttp::set_max_hearts(&mut g.snes);
                    }
                    r.app.status = Some((tr(en, "20 Herzen", "20 hearts").into(), Instant::now()));
                }))
                .into_any_element(),
        );
        if let Some(g) = self.app.game.as_ref() {
            let w = &g.snes.bus.wram;
            let cap = (w[0xF36C] / 8).clamp(1, 20) as f64;
            let cur = w[0xF36D] as f64 / 8.0;
            let pieces = (w[0xF36B] & 3) as f64;
            let line = |txt: &str, s: Div| row().child(div().w(px(230.0)).flex_none().child(txt.to_string())).child(s).into_any_element();
            let s1 = slider(self, "hearts-cap", "", cap, 1.0, 20.0, 1.0, format!("{}", cap as u32), true, cx, |r, x| {
                if let Some(g) = r.app.game.as_mut() {
                    let w = &mut g.snes.bus.wram;
                    let cap = x.round() as u32;
                    w[0xF36C] = (cap * 8) as u8;
                    w[0xF36D] = w[0xF36D].min((cap * 8) as u8);
                }
            });
            inv_b.push(line(t("Herzcontainer", "Heart containers"), s1));
            let s2 = slider(self, "hearts-cur", "", cur, 0.5, cap, 0.5, format!("{cur:.1}"), true, cx, |r, x| {
                if let Some(g) = r.app.game.as_mut() {
                    let w = &mut g.snes.bus.wram;
                    let cap = (w[0xF36C] / 8).clamp(1, 20) as u32;
                    w[0xF36D] = ((x * 8.0).round() as u32).clamp(4, cap * 8) as u8;
                }
            });
            inv_b.push(line(t("Aktuelle Herzen", "Current hearts"), s2));
            let s3 = slider(self, "hearts-pieces", "", pieces, 0.0, 3.0, 1.0, format!("{}", pieces as u32), true, cx, |r, x| {
                if let Some(g) = r.app.game.as_mut() {
                    g.snes.bus.wram[0xF36B] = x.round() as u8;
                }
            });
            inv_b.push(line(t("Herzteile (bis zum nächsten Herz)", "Heart pieces (toward next heart)"), s3));
        }
        v.push(section(t("Inventar", "Inventory"), inv_b).into_any_element());
        let swords = [t("Kein Schwert", "No sword"), t("Kämpferschwert", "Fighter Sword"), t("Master-Schwert", "Master Sword"), t("Gehärtetes Schwert", "Tempered Sword"), t("Goldenes Schwert", "Golden Sword")];
        let shields = [t("Kein Schild", "No shield"), t("Holzschild", "Fighter Shield"), t("Feuerschild", "Fire Shield"), t("Spiegelschild", "Mirror Shield")];
        let armors = [t("Grüne Tunika", "Green Mail"), t("Blaue Rüstung", "Blue Mail"), t("Rote Rüstung", "Red Mail")];
        let opts = |list: &[&str], cur: u8| list.iter().enumerate().map(|(i, n)| (n.to_string(), i as u8 == cur)).collect::<Vec<_>>();
        let (sw, sh, ar) = (self.app.sword, self.app.shield, self.app.armor);
        let eq = wrap_row()
            .child(combo(self, "sword", swords[sw as usize].to_string(), 170.0, opts(&swords, sw), cx, |r, i| r.app.sword = i as u8))
            .child(combo(self, "shield", shields[sh as usize].to_string(), 150.0, opts(&shields, sh), cx, |r, i| r.app.shield = i as u8))
            .child(combo(self, "armor", armors[ar as usize].to_string(), 150.0, opts(&armors, ar), cx, |r, i| r.app.armor = i as u8))
            .child(button("equip", t("Setzen", "Apply"), has, cx, move |r| {
                let (a, b, c) = (r.app.sword, r.app.shield, r.app.armor);
                if let Some(g) = r.app.game.as_mut() {
                    alttp::set_equipment(&mut g.snes, a, b, c);
                }
                r.app.status = Some((tr(en, "Ausrüstung gesetzt", "Equipment set").into(), Instant::now()));
            }));
        v.push(section(t("Ausrüstung", "Equipment"), vec![eq.into_any_element()]).into_any_element());
        v.push(section(t("Einzelne Items geben", "Give single items"), self.items_ui(cx)).into_any_element());
        v.push(section("Codes (Pro Action Replay / Game Genie)", self.codes_ui(cx)).into_any_element());
        v
    }

    fn items_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let Some(g) = self.app.game.as_ref() else {
            return vec![label(t("Lade zuerst eine ROM.", "Load a ROM first.")).text_color(col::dim()).into_any_element()];
        };
        let mut v = vec![small(t("Auswahl wird sofort im Spiel gesetzt (am besten im laufenden Spiel, nicht im Titelbild).", "Changes apply instantly in-game (best while playing, not on the title screen).")).into_any_element()];
        let wram = g.snes.bus.wram.clone();
        let rd = |a: u32| wram[(a & 0x1FFFF) as usize];
        let write = |r: &mut Root, a: u32, val: u8| {
            if let Some(g) = r.app.game.as_mut() {
                g.snes.bus.wram[(a & 0x1FFFF) as usize] = val;
            }
        };
        let mut cells = Vec::new();
        for (k, it) in crate::ui::ITEMS.iter().enumerate() {
            let cur = rd(it.addr);
            let lbl = it.opts.iter().find(|o| o.0 == cur).map(|o| if en { o.2 } else { o.1 }.to_string()).unwrap_or_else(|| format!("{cur}"));
            let opts: Vec<(String, bool)> = it.opts.iter().map(|o| ((if en { o.2 } else { o.1 }).to_string(), o.0 == cur)).collect();
            let addr = it.addr;
            let list: Vec<u8> = it.opts.iter().map(|o| o.0).collect();
            let cb = combo(self, &format!("item-{k}"), lbl, 150.0, opts, cx, move |r, i| {
                let val = list[i];
                write(r, addr, val);
                if addr == 0x7EF355 && val == 1 {
                    let old = r.app.game.as_ref().map_or(0, |g| g.snes.bus.wram[0xF379]);
                    write(r, 0x7EF379, old | 0x04); // Rennen erlauben
                }
            });
            cells.push(row().justify_between().child(div().text_color(col::dim()).child(if en { it.en } else { it.de })).child(cb).into_any_element());
        }
        v.push(grid(2, cells, 18.0).into_any_element());
        v.push(sep().into_any_element());
        let hearts = (rd(0x7EF36C) / 8).max(1) as f64;
        let rupees = (rd(0x7EF360) as u32 | (rd(0x7EF361) as u32) << 8) as f64;
        let bombs = rd(0x7EF343) as f64;
        let arrows = rd(0x7EF377) as f64;
        let keys = rd(0x7EF36F) as u32;
        let cell = |txt: &str, d: Div| row().justify_between().child(div().text_color(col::dim()).child(txt.to_string())).child(d).into_any_element();
        let mut counts = Vec::new();
        counts.push(cell(t("Herzcontainer", "Heart containers"), drag_value("dv-hearts", hearts, 1.0, 20.0, true, cx, move |r, x| {
            write(r, 0x7EF36C, (x as u32 * 8) as u8);
            write(r, 0x7EF36D, (x as u32 * 8) as u8);
        })));
        counts.push(cell(t("Rubine", "Rupees"), drag_value("dv-rupees", rupees, 0.0, 999.0, true, cx, move |r, x| {
            let x = x as u32;
            write(r, 0x7EF360, x as u8);
            write(r, 0x7EF361, (x >> 8) as u8);
        })));
        counts.push(cell(t("Bomben", "Bombs"), drag_value("dv-bombs", bombs, 0.0, 50.0, true, cx, move |r, x| write(r, 0x7EF343, x as u8))));
        counts.push(cell(t("Pfeile", "Arrows"), drag_value("dv-arrows", arrows, 0.0, 70.0, true, cx, move |r, x| write(r, 0x7EF377, x as u8))));
        counts.push(cell(t("Schlüssel (aktueller Dungeon)", "Keys (current dungeon)"), drag_value("dv-keys", keys.min(9) as f64, 0.0, 9.0, keys != 0xFF, cx, move |r, x| write(r, 0x7EF36F, x as u8))));
        let refill_h = rd(0x7EF36C);
        counts.push(
            row()
                .justify_end()
                .child(button("refill-m", t("Magie auffüllen", "Refill magic"), true, cx, move |r| write(r, 0x7EF36E, 0x80)))
                .child(button("refill-h", t("Herzen auffüllen", "Refill hearts"), true, cx, move |r| write(r, 0x7EF36D, refill_h)))
                .into_any_element(),
        );
        v.push(grid(2, counts, 18.0).into_any_element());
        v.push(sep().into_any_element());
        let pend = rd(0x7EF374);
        let mut pr = wrap_row().child(strong(t("Amulette:", "Pendants:")).mr(px(4.0)));
        for (bit, de, e) in [(0x04u8, "Mut (grün)", "Courage (green)"), (0x02, "Kraft (blau)", "Power (blue)"), (0x01, "Weisheit (rot)", "Wisdom (red)")] {
            pr = pr.child(check(format!("pend-{bit}"), if en { e } else { de }, pend & bit != 0, true, cx, move |r| write(r, 0x7EF374, pend ^ bit)));
        }
        v.push(pr.into_any_element());
        let cry = rd(0x7EF37A);
        let mut cr = wrap_row().child(strong(t("Kristalle:", "Crystals:")).mr(px(4.0)));
        for k in 0..7u8 {
            cr = cr.child(check(format!("cry-{k}"), format!("{}", k + 1), cry & (1 << k) != 0, true, cx, move |r| write(r, 0x7EF37A, cry ^ (1 << k))));
        }
        v.push(cr.into_any_element());
        v
    }

    fn codes_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let mut v = Vec::new();
        let mut list = div().flex().flex_col();
        for (i, cc) in self.app.s.codes.iter().enumerate() {
            list = list.child(
                row()
                    .py(px(2.0))
                    .when(i % 2 == 1, |d| d.bg(rgb(0x111318)))
                    .child(check(format!("code-{i}"), "", cc.enabled, true, cx, move |r| {
                        if let Some(c) = r.app.s.codes.get_mut(i) {
                            c.enabled = !c.enabled;
                        }
                        r.app.reapply_rom_codes();
                    }))
                    .child(mono(cc.code.clone()).w(px(140.0)).text_color(col::gold()))
                    .child(div().flex_1().child(cc.desc.clone()))
                    .child(
                        div()
                            .id(eid(format!("code-del-{i}")))
                            .px(px(8.0))
                            .rounded(px(4.0))
                            .text_color(col::dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(col::hover()).text_color(col::red()))
                            .on_click(on::<ClickEvent>(cx, move |r| {
                                if i < r.app.s.codes.len() {
                                    r.app.s.codes.remove(i);
                                    r.app.reapply_rom_codes();
                                }
                            }))
                            .child("✕"),
                    ),
            );
        }
        v.push(list.into_any_element());
        let (ci, cd) = (self.app.code_input.clone(), self.app.code_desc.clone());
        v.push(
            row()
                .pt(px(4.0))
                .child(text_field(self, TextField::CodeInput, &ci, "7EF36DA0 / XXXX-XXXX", 170.0, cx))
                .child(text_field(self, TextField::CodeDesc, &cd, t("Beschreibung", "Description"), 190.0, cx))
                .child(button("code-add", t("Hinzufügen", "Add"), true, cx, |r| {
                    let a = &mut r.app;
                    match alttp::decode_code(&a.code_input) {
                        Ok(_) => {
                            a.s.codes.push(CheatCode { code: a.code_input.trim().to_uppercase(), desc: a.code_desc.trim().to_string(), enabled: true });
                            a.code_input.clear();
                            a.code_desc.clear();
                            a.code_err.clear();
                            a.reapply_rom_codes();
                        }
                        Err(e) => a.code_err = e,
                    }
                    r.ui.text_focus = None;
                }))
                .into_any_element(),
        );
        if !self.app.code_err.is_empty() {
            v.push(div().text_color(col::red()).child(self.app.code_err.clone()).into_any_element());
        }
        v
    }

    // ------------------------------------------------------------ Steuerung & Hotkeys

    fn controls_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let mut v = vec![label(t("Klicke auf eine Taste und drücke die neue.", "Click a button, then press the new key.")).text_color(col::dim()).into_any_element()];
        let mut cells = Vec::new();
        for (i, name) in BUTTONS.iter().enumerate() {
            let waiting = self.app.rebind == Some(i);
            let txt = if waiting { t("… Taste drücken …", "… press a key …").to_string() } else { self.app.s.keys[i].clone() };
            let b = button(format!("bind-{i}"), txt, true, cx, move |r| r.app.rebind = Some(i)).w(px(140.0)).when(waiting, |d| d.border_color(col::gold()).text_color(col::gold()).bg(col::sel()));
            cells.push(row().justify_between().child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(15.0)).child(*name)).child(b).into_any_element());
        }
        v.push(div().pt(px(6.0)).child(grid(2, cells, 28.0)).into_any_element());
        v.push(row().pt(px(6.0)).child(button("keys-default", t("Standardbelegung", "Default layout"), true, cx, |r| r.app.s.reset_keys())).into_any_element());
        let mut ctl = Vec::new();
        let ps = self.app.pro.get();
        if ps.connected {
            ctl.push(row().text_color(col::green()).child(icon("\u{E7FC}")).child(format!("{} ({}) – {}", ps.name, if ps.usb { "USB" } else { "Bluetooth" }, t("verbunden", "connected"))).into_any_element());
            ctl.push(small(t("ZR = Turbo · ZL = Zurückspulen · rechten Stick drücken = ×2 · Home = Spielstände · Aufnahme-Taste = Screenshot", "ZR = turbo · ZL = rewind · right stick click = ×2 · Home = save states · Capture = screenshot")).into_any_element());
        } else {
            ctl.push(label(t("Switch Pro Controller: nicht gefunden – per USB-Datenkabel anschließen (wird automatisch erkannt).", "Switch Pro Controller: not found – connect with a USB data cable (detected automatically).")).text_color(col::dim()).into_any_element());
            if !ps.error.is_empty() {
                ctl.push(div().text_size(px(12.0)).text_color(col::red()).child(ps.error.clone()).into_any_element());
            }
        }
        if let Some(g) = &self.app.gilrs {
            let mut any = false;
            for (_id, gp) in g.gamepads() {
                if gp.is_connected() {
                    any = true;
                    ctl.push(row().text_color(col::green()).child(icon("\u{E7FC}")).child(gp.name().to_string()).into_any_element());
                }
            }
            if !any {
                ctl.push(label(t("Kein Controller verbunden.", "No controller connected.")).text_color(col::dim()).into_any_element());
            }
        }
        ctl.push(small(t("RT = Turbo · LT = Zurückspulen · rechten Stick drücken = ×2", "RT = turbo · LT = rewind · right stick click = ×2")).into_any_element());
        v.push(section("Controller", ctl).into_any_element());
        v
    }

    fn hotkeys_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let mut v = vec![label(t("Klicke auf eine Belegung und drücke die neue Kombination (Strg/Umschalt/Alt möglich). Esc bricht ab.", "Click a binding and press the new combination (Ctrl/Shift/Alt allowed). Esc cancels.")).text_color(col::dim()).into_any_element()];
        let combos: Vec<Option<String>> = ACTS.iter().map(|a| self.app.s.hotkey(a.id).map(|c| c.to_text())).collect();
        for (gi, (gde, gen)) in GROUPS.iter().enumerate() {
            let open = self.ui.hk_open.get(gi).copied().unwrap_or(false);
            v.push(
                div()
                    .id(eid(format!("hk-group-{gi}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .mt(px(6.0))
                    .py(px(4.0))
                    .px(px(4.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(col::hover()))
                    .on_click(on::<ClickEvent>(cx, move |r| {
                        if let Some(o) = r.ui.hk_open.get_mut(gi) {
                            *o = !*o;
                        }
                    }))
                    .child(div().w(px(12.0)).text_color(col::gold()).text_size(px(10.0)).child(if open { "▼" } else { "▶" }))
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_color(col::gold()).child(if en { *gen } else { *gde }))
                    .into_any_element(),
            );
            if !open {
                continue;
            }
            let mut list = div().flex().flex_col().ml(px(20.0));
            for (k, (ai, a)) in ACTS.iter().enumerate().filter(|(_, a)| a.group == gi).enumerate() {
                let cur = &combos[ai];
                let conflict = cur.is_some() && combos.iter().filter(|c| *c == cur).count() > 1;
                let waiting = self.app.rebind_hotkey == Some(a.id);
                let txt = if waiting { t("… drücken …", "… press …").to_string() } else { cur.clone().unwrap_or_else(|| "—".into()) };
                let id = a.id;
                let b = button(format!("hk-{id}"), txt, true, cx, move |r| r.app.rebind_hotkey = Some(id))
                    .w(px(160.0))
                    .font_family(MONO_FONT)
                    .when(conflict && !waiting, |d| d.text_color(col::red()))
                    .when(waiting, |d| d.border_color(col::gold()).text_color(col::gold()).bg(col::sel()));
                let icon = |idn: String, txt: &'static str, tipt: String, cx: &mut Context<Root>, f: Box<dyn Fn(&mut Root)>| {
                    div()
                        .id(eid(idn))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(24.0))
                        .rounded(px(5.0))
                        .text_color(col::dim())
                        .cursor_pointer()
                        .hover(|s| s.bg(col::hover()).text_color(col::text()))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, _| f(this)))
                        .tooltip(tip(tipt))
                        .child(txt)
                };
                list = list.child(
                    row()
                        .py(px(2.0))
                        .px(px(4.0))
                        .when(k % 2 == 1, |d| d.bg(rgb(0x111318)))
                        .child(div().flex_1().child(if en { a.en } else { a.de }))
                        .child(b)
                        .child(icon(format!("hk-x-{id}"), "✕", t("Entfernen", "Unbind").to_string(), cx, Box::new(move |r| {
                            r.app.s.hotkeys.insert(id.to_string(), String::new());
                        })))
                        .child(icon(format!("hk-d-{id}"), "↺", t("Standard", "Default").to_string(), cx, Box::new(move |r| {
                            r.app.s.hotkeys.remove(id);
                        }))),
                );
            }
            v.push(list.into_any_element());
        }
        v.push(row().pt(px(10.0)).child(button("hk-reset", t("Alle auf Standard zurücksetzen", "Reset all to default"), true, cx, |r| r.app.s.hotkeys.clear())).into_any_element());
        v
    }
}

// ---------------------------------------------------------------- Hinweise beim Überfahren

struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(360.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(col::panel_line())
            .bg(rgb(0x202329))
            .font_family(UI_FONT)
            .text_size(px(12.0))
            .text_color(col::text())
            .shadow_lg()
            .child(self.0.clone())
    }
}

fn tip(text: String) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let s: SharedString = text.trim().trim_end_matches("[]").trim().to_string().into();
    move |_, cx| cx.new(|_| Tip(s.clone())).into()
}

#[allow(dead_code)]
fn _unused(_: Hsla, _: Entity<Root>) {}


thread_local! {
    /// Bilder der Mods für die Mod-Liste (Schlüssel: Kennung oder Datei)
    static MOD_ICONS: RefCell<HashMap<String, Option<std::sync::Arc<gpui::RenderImage>>>> = RefCell::new(HashMap::new());
}

/// Mod-Bild (RGBA) als GPUI-Bild, zwischengespeichert
fn mod_icon_from(key: &str, ic: &std::sync::Arc<(Vec<u8>, usize, usize)>) -> Option<std::sync::Arc<gpui::RenderImage>> {
    MOD_ICONS.with(|c| {
        c.borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| {
                let (rgba, w, h) = &**ic;
                let mut bgra = rgba.clone();
                for p in bgra.chunks_exact_mut(4) {
                    p.swap(0, 2);
                }
                gfx::render_image(*w as u32, *h as u32, bgra).map(std::sync::Arc::new)
            })
            .clone()
    })
}

/// Mod-Bild aus einer PNG-Datei (Mod-Liste), zwischengespeichert
fn mod_icon_file(p: &std::path::Path) -> Option<std::sync::Arc<gpui::RenderImage>> {
    let key = p.to_string_lossy().to_string();
    if let Some(v) = MOD_ICONS.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let ic = std::fs::read(p).ok().and_then(|b| crate::mods::decode_png(&b)).map(std::sync::Arc::new);
    match ic {
        Some(ic) => mod_icon_from(&key, &ic),
        None => {
            MOD_ICONS.with(|c| c.borrow_mut().insert(key, None));
            None
        }
    }
}


/// Kleine Pixel-Symbole für Knöpfe (12×12, doppelt so groß gezeichnet)
fn ui_icon(kind: &str) -> Option<std::sync::Arc<gpui::RenderImage>> {
    const GET: [&str; 12] = [
        ".....##.....", ".....##.....", ".....##.....", ".....##.....", "..#..##..#..", "...#.##.#...",
        "....####....", ".....##.....", "............", "#..........#", "#..........#", "############",
    ];
    const UPDATE: [&str; 12] = [
        "....####....", "..##....##..", ".#........#.", ".#..........", "#...........", "#......#####",
        "#........##.", "#.......#.#.", ".#.....#..#.", ".#....#...#.", "..##........", "....####....",
    ];
    const GEAR: [&str; 12] = [
        ".....##.....", "..#.####.#..", ".##########.", "..###..###..", ".###....###.", "####....####",
        "####....####", ".###....###.", "..###..###..", ".##########.", "..#.####.#..", ".....##.....",
    ];
    const TRASH: [&str; 12] = [
        "....####....", "############", "............", ".##########.", ".#.#.##.#.#.", ".#.#.##.#.#.",
        ".#.#.##.#.#.", ".#.#.##.#.#.", ".#.#.##.#.#.", ".#.#.##.#.#.", ".##########.", "............",
    ];
    let (pat, c): (&[&str; 12], [u8; 3]) = match kind {
        "get" => (&GET, [120, 216, 120]),
        "update" => (&UPDATE, [248, 208, 64]),
        "gear" => (&GEAR, [208, 212, 218]),
        _ => (&TRASH, [224, 108, 108]),
    };
    MOD_ICONS.with(|cache| {
        cache
            .borrow_mut()
            .entry(format!("ui:{kind}"))
            .or_insert_with(|| {
                let k = 2;
                let n = 12 * k;
                let mut bgra = vec![0u8; n * n * 4];
                for (y, row) in pat.iter().enumerate() {
                    for (x, ch) in row.bytes().enumerate() {
                        if ch != b'#' {
                            continue;
                        }
                        for dy in 0..k {
                            for dx in 0..k {
                                let o = ((y * k + dy) * n + x * k + dx) * 4;
                                bgra[o..o + 4].copy_from_slice(&[c[2], c[1], c[0], 255]);
                            }
                        }
                    }
                }
                gfx::render_image(n as u32, n as u32, bgra).map(std::sync::Arc::new)
            })
            .clone()
    })
}

/// Knopf mit Pixel-Symbol (Holen, Update, Einstellungen, Deinstallieren)
fn icon_button(id: impl Into<SharedString>, kind: &str, enabled: bool, cx: &mut Context<Root>, f: impl Fn(&mut Root) + 'static) -> Stateful<Div> {
    let mut b = div()
        .id(eid(id))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(30.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(col::panel_line())
        .bg(rgb(0x22262d));
    if let Some(img) = ui_icon(kind) {
        b = b.child(gpui::img(gpui::ImageSource::Render(img)).size(px(18.0)));
    }
    if enabled {
        b.cursor_pointer().hover(|s| s.bg(col::hover()).border_color(rgb(0x3c424b))).active(|s| s.bg(col::press())).on_click(on::<ClickEvent>(cx, f))
    } else {
        b.opacity(0.4)
    }
}
