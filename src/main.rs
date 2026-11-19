#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alttp;
mod app;
mod gfx;
mod offgl;
mod shell;
mod audio;
mod filters;
mod pixfont;
mod gpu;
mod gpu_sfx;
mod gpu_sxbr;
mod hotkeys;
mod i18n;
mod quickmenu;
mod randomizer;
mod settings;
mod snes;
mod switchpro;
mod teleport;
mod devtest;
mod perf;
mod recomp;
mod mods;
mod z3;
mod tracker;
mod world;
mod ui;
mod mapview;
mod mapdata;
mod inventory;
mod padsettings;

use gpui::{px, size, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use std::borrow::Cow;
use std::path::PathBuf;

/// RGBA-Bild des App-Symbols (Breite, Höhe, Pixel)
struct IconData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// App-Symbol: goldenes Triforce mit dunklem Rand (wird zur Laufzeit gezeichnet, keine Datei nötig)
fn triforce_icon() -> IconData {
    let n = 256usize;
    let mut rgba = vec![0u8; n * n * 4];
    // Dreieck-Test über Halbebenen
    let inside = |px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        let s = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1);
        let (d1, d2, d3) = (s((px, py), a, b), s((px, py), b, c), s((px, py), c, a));
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    };
    let side = 236.0f32;
    let h = side * 0.866;
    let (x0, y0) = (10.0f32, (n as f32 - h) / 2.0);
    let tris = [
        ((x0 + side / 2.0, y0), (x0 + side * 0.75, y0 + h / 2.0), (x0 + side * 0.25, y0 + h / 2.0)),
        ((x0 + side * 0.25, y0 + h / 2.0), (x0 + side / 2.0, y0 + h), (x0, y0 + h)),
        ((x0 + side * 0.75, y0 + h / 2.0), (x0 + side, y0 + h), (x0 + side / 2.0, y0 + h)),
    ];
    let shrink = |t: ((f32, f32), (f32, f32), (f32, f32)), k: f32| {
        let cx = (t.0 .0 + t.1 .0 + t.2 .0) / 3.0;
        let cy = (t.0 .1 + t.1 .1 + t.2 .1) / 3.0;
        let f = |p: (f32, f32)| (cx + (p.0 - cx) * k, cy + (p.1 - cy) * k);
        (f(t.0), f(t.1), f(t.2))
    };
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut col = None;
            for t in tris {
                if inside(px, py, t.0, t.1, t.2) {
                    let inner = shrink(t, 0.86);
                    col = Some(if inside(px, py, inner.0, inner.1, inner.2) {
                        // goldener Verlauf: oben heller
                        let g = (1.0 - (py - y0) / h * 0.35).clamp(0.6, 1.0);
                        [(250.0 * g) as u8, (204.0 * g) as u8, (52.0 * g) as u8, 255]
                    } else {
                        [70, 46, 8, 255]
                    });
                }
            }
            if let Some(c) = col {
                rgba[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
            }
        }
    }
    IconData { rgba, width: n as u32, height: n as u32 }
}

/// Triforce als Fenstersymbol setzen (auch wenn die .exe ohne Symbol gebaut wurde)
fn set_window_icon(window: &gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIcon, SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};
    let Ok(h) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else { return };
    let icon = triforce_icon();
    let bgra: Vec<u8> = icon.rgba.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let and = vec![0u8; (icon.width * icon.height / 8) as usize];
    unsafe {
        let hinst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        let hicon = CreateIcon(hinst, icon.width as i32, icon.height as i32, 1, 32, and.as_ptr(), bgra.as_ptr());
        if !hicon.is_null() {
            let hwnd = w.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, hicon as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, hicon as isize);
        }
    }
}

fn main() {
    let rom_arg = std::env::args().nth(1).map(PathBuf::from);
    gpui::Application::new().run(move |cx: &mut gpui::App| {
        // Spielschrift für Karte, Inventar und Überschriften: Lilita One (SIL Open Font License, siehe assets/fonts)
        let _ = cx.text_system().add_fonts(vec![Cow::Borrowed(include_bytes!("../assets/fonts/LilitaOne-Regular.ttf"))]);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1000.0), px(760.0)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Zelda Core".into()), ..Default::default() }),
            window_min_size: Some(size(px(480.0), px(360.0))),
            app_id: Some("pastlink".into()),
            ..Default::default()
        };
        let res = cx.open_window(opts, move |window, cx| {
            set_window_icon(window);
            cx.new(|cx| shell::Root::new(app::App::new(rom_arg), window, cx))
        });
        if let Err(e) = res {
            eprintln!("Fenster konnte nicht geöffnet werden: {e}");
            cx.quit();
        }
        cx.activate(true);
    });
}
