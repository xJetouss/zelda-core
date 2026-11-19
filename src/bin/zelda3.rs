//! Reines Zelda 3 als eigenes Programm (`zelda3.exe`), ohne pastlink-Funktionen – zum Vergleich.
//!
//! Spiellogik und Grafik kommen komplett aus Zelda 3 (https://github.com/snesrev/zelda3, MIT-Lizenz, siehe
//! `vendor/zelda3`), mit den deutschen Dialogen. Oben links steht die Anzahl der angezeigten Bilder und der
//! Spielbilder pro Sekunde. Tab halten: Spiellogik ohne Bremse (zeigt, dass mehr Spielbilder = schnelleres Spiel).
//!
//! Steuerung (wie Zelda 3): Pfeiltasten, X = A, Z/Y = B, S = X, A = Y, C = L, V = R, Enter = Start,
//! Rechte Umschalttaste = Select; Controller werden ebenfalls erkannt.
//! Spielstände: `zelda3\saves\sram.dat` neben der .exe (getrennt von pastlink).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../audio.rs"]
#[allow(dead_code)]
mod audio;

#[cfg(pastlink_z3)]
mod data {
    include!(concat!(env!("OUT_DIR"), "/z3_data.rs"));
}

#[cfg(not(pastlink_z3))]
fn main() {
    message("Zelda 3 ist in diesem Build nicht enthalten (vendor/zelda3 bzw. zelda3_assets.dat fehlte beim Bauen).");
}

fn message(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let t = wide(text);
    let c = wide("Zelda 3");
    unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(pastlink_z3)]
fn main() {
    run::main()
}

#[cfg(pastlink_z3)]
mod run {
    use super::{data, wide};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    extern "C" {
        fn pl_z3_load_assets(data: *const u8, len: usize) -> i32;
        fn pl_z3_init(language: *const std::ffi::c_char);
        fn pl_z3_read_sram();
        fn pl_z3_run_frame(pad: u16);
        fn pl_z3_audio(buf: *mut i16, samples: i32);
        fn pl_z3_draw(pixels: *mut u8, pitch: usize);
    }

    const W: usize = 256;
    const H: usize = 224;
    /// Bildrate des SNES (NTSC) – so schnell läuft auch Zelda 3
    const GAME_HZ: f64 = 60.0988;

    unsafe extern "system" fn wndproc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(h, msg, w, l),
        }
    }

    fn key(vk: i32) -> bool {
        unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
    }

    /// Tastatur im Format von $4218 (B Y Sel St ↑ ↓ ← → A X L R)
    fn keyboard() -> u16 {
        let map: [(&[i32], u16); 12] = [
            (&[b'Z' as i32, b'Y' as i32], 0x8000),
            (&[b'A' as i32], 0x4000),
            (&[0xA1], 0x2000), // rechte Umschalttaste
            (&[0x0D], 0x1000), // Enter
            (&[0x26], 0x0800),
            (&[0x28], 0x0400),
            (&[0x25], 0x0200),
            (&[0x27], 0x0100),
            (&[b'X' as i32], 0x0080),
            (&[b'S' as i32], 0x0040),
            (&[b'C' as i32], 0x0020),
            (&[b'V' as i32], 0x0010),
        ];
        map.iter().filter(|(k, _)| k.iter().any(|&v| key(v))).fold(0, |a, (_, b)| a | b)
    }

    fn gamepad(g: &mut gilrs::Gilrs) -> u16 {
        use gilrs::{Axis, Button};
        while g.next_event().is_some() {}
        let mut p = 0u16;
        for (_, pad) in g.gamepads() {
            // Nintendo-Belegung: unten = B, rechts = A, links = Y, oben = X
            let b: [(Button, u16); 12] = [
                (Button::South, 0x8000),
                (Button::West, 0x4000),
                (Button::Select, 0x2000),
                (Button::Start, 0x1000),
                (Button::DPadUp, 0x0800),
                (Button::DPadDown, 0x0400),
                (Button::DPadLeft, 0x0200),
                (Button::DPadRight, 0x0100),
                (Button::East, 0x0080),
                (Button::North, 0x0040),
                (Button::LeftTrigger, 0x0020),
                (Button::RightTrigger, 0x0010),
            ];
            for (btn, bit) in b {
                if pad.is_pressed(btn) {
                    p |= bit;
                }
            }
            let (x, y) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
            if x < -0.5 {
                p |= 0x0200;
            }
            if x > 0.5 {
                p |= 0x0100;
            }
            if y > 0.5 {
                p |= 0x0800;
            }
            if y < -0.5 {
                p |= 0x0400;
            }
        }
        p
    }

    /// Spieldaten: `zelda3_assets.dat` neben der .exe, sonst die beim Bauen eingebauten
    fn assets() -> &'static [u8] {
        let f = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("zelda3_assets.dat")));
        match f.and_then(|f| std::fs::read(f).ok()).filter(|b| b.len() > 1024) {
            Some(b) => Box::leak(b.into_boxed_slice()),
            None => data::ASSETS,
        }
    }

    pub fn main() {
        // Spielstände getrennt von pastlink: zelda3\saves neben der .exe
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("zelda3"))) {
            let _ = std::fs::create_dir_all(dir.join("saves"));
            let _ = std::env::set_current_dir(&dir);
        }
        unsafe {
            let a = assets();
            if a.is_empty() || pl_z3_load_assets(a.as_ptr(), a.len()) != 0 {
                super::message("Keine Zelda-3-Spieldaten gefunden. Lege zelda3_assets.dat neben zelda3.exe (wie man sie erzeugt, steht in der README unter „RC-Modus mit Zelda 3“).");
                return;
            }
            pl_z3_init(c"de".as_ptr());
            pl_z3_read_sram();
        }
        let hwnd = unsafe { window() };
        let audio = crate::audio::AudioOut::new(60);
        let mut gil = gilrs::Gilrs::new().ok();
        let mut pixels = vec![0u32; W * H];
        let mut snd = vec![0i16; 2048];
        let mut audio_acc = 0.0f64;
        let frame_dt = Duration::from_secs_f64(1.0 / GAME_HZ);
        let mut next = Instant::now();
        let mut last_present = Instant::now();
        let (mut shown, mut played) = (0u32, 0u32);
        let (mut fps_shown, mut fps_game) = (0u32, 0u32);
        let mut t_count = Instant::now();
        loop {
            let mut m: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                if m.message == WM_QUIT {
                    return;
                }
                unsafe {
                    TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
            }
            let focus = unsafe { GetForegroundWindow() } == hwnd;
            let turbo = focus && key(0x09);
            if !turbo {
                // auf das nächste Spielbild warten (grob schlafen, das letzte Stück genau)
                let now = Instant::now();
                if next > now {
                    let rest = next - now;
                    if rest > Duration::from_millis(2) {
                        std::thread::sleep(rest - Duration::from_millis(1));
                    }
                    while Instant::now() < next {
                        std::hint::spin_loop();
                    }
                }
                next += frame_dt;
                if Instant::now() > next + frame_dt * 4 {
                    next = Instant::now();
                }
            } else {
                next = Instant::now();
            }
            let pad = if focus { keyboard() } else { 0 } | gil.as_mut().map_or(0, gamepad);
            unsafe { pl_z3_run_frame(pad) };
            played += 1;
            // Ton: 32 kHz
            audio_acc += 32040.5 / GAME_HZ;
            let k = audio_acc.floor() as usize;
            audio_acc -= k as f64;
            unsafe { pl_z3_audio(snd.as_mut_ptr(), k as i32) };
            if let (Some(a), false) = (&audio, turbo) {
                a.push(&snd[..k * 2]);
            }
            // im Turbo nur ~60-mal pro Sekunde zeichnen
            if !turbo || last_present.elapsed() >= Duration::from_millis(16) {
                last_present = Instant::now();
                unsafe { pl_z3_draw(pixels.as_mut_ptr() as *mut u8, W * 4) };
                unsafe { present(hwnd, &pixels, fps_shown, fps_game, turbo) };
                shown += 1;
            }
            if t_count.elapsed() >= Duration::from_secs(1) {
                let s = t_count.elapsed().as_secs_f64();
                fps_shown = (shown as f64 / s).round() as u32;
                fps_game = (played as f64 / s).round() as u32;
                (shown, played) = (0, 0);
                t_count = Instant::now();
                let t = wide(&format!("Zelda 3 – {fps_shown} Bilder/s, {fps_game} Spielbilder/s"));
                unsafe { SetWindowTextW(hwnd, t.as_ptr()) };
            }
        }
    }

    unsafe fn window() -> HWND {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide("pastlink_zelda3");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: LoadIconW(hinst, 1 as _),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let title = wide("Zelda 3");
        let mut r = RECT { left: 0, top: 0, right: (W * 3) as i32, bottom: (H * 3) as i32 };
        AdjustWindowRect(&mut r, WS_OVERLAPPEDWINDOW, 0);
        let h = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
        h
    }

    /// Bild seitenrichtig (8:7-Pixel als 4:3) ins Fenster, schwarze Ränder, FPS oben links
    unsafe fn present(hwnd: HWND, px: &[u32], fps_shown: u32, fps_game: u32, turbo: bool) {
        let mut cr: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut cr);
        let (cw, ch) = (cr.right.max(1), cr.bottom.max(1));
        let (mut dw, mut dh) = (cw, cw * 3 / 4);
        if dh > ch {
            dh = ch;
            dw = ch * 4 / 3;
        }
        let (dx, dy) = ((cw - dw) / 2, (ch - dh) / 2);
        let dc = GetDC(hwnd);
        // in ein Hilfsbild zeichnen und auf einmal ausgeben (kein Flackern der Schrift)
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, cw, ch);
        let old = SelectObject(mem, bmp);
        let black = GetStockObject(BLACK_BRUSH);
        FillRect(mem, &cr, black as _);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = W as i32;
        bi.bmiHeader.biHeight = -(H as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        SetStretchBltMode(mem, COLORONCOLOR);
        StretchDIBits(mem, dx, dy, dw, dh, 0, 0, W as i32, H as i32, px.as_ptr() as _, &bi, DIB_RGB_COLORS, SRCCOPY);
        let text = wide(&format!("FPS {fps_shown}  Spiel {fps_game}{}", if turbo { "  (Tab: ohne Bremse)" } else { "" }));
        let n = text.len() as i32 - 1;
        SetBkMode(mem, TRANSPARENT as _);
        SetTextColor(mem, 0x000000);
        TextOutW(mem, dx + 7, dy + 7, text.as_ptr(), n);
        SetTextColor(mem, 0x00FFFF);
        TextOutW(mem, dx + 6, dy + 6, text.as_ptr(), n);
        BitBlt(dc, 0, 0, cw, ch, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(hwnd, dc);
    }
}
