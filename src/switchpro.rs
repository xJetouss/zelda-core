//! Eigener Treiber für den Nintendo Switch Pro Controller (USB und Bluetooth) sowie den
//! SNES-Controller von Nintendo Switch Online.
//!
//! Über USB sendet der Pro Controller erst Tasten, nachdem der PC einen Handshake geschickt hat
//! (deshalb erkennt Windows ihn per Kabel nicht als normalen Gamepad). Ablauf:
//! `80 02` (Handshake) · `80 03` (3 Mbit) · `80 02` · `80 04` (nur USB, kein Timeout),
//! danach Unterbefehl `03 30` = vollständige Eingabeberichte (0x30, alle 8 ms) und `30 01` = Spieler-LED 1.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const VID: u16 = 0x057E;
/// Pro Controller, SNES-Controller (NSO)
const PIDS: [u16; 2] = [0x2009, 0x2017];

// Bit-Layout von `buttons` = Byte3 | Byte4 << 8 | Byte5 << 16 des 0x30-Berichts
pub const Y: u32 = 0x01;
pub const X: u32 = 0x02;
pub const B: u32 = 0x04;
pub const A: u32 = 0x08;
pub const R: u32 = 0x40;
pub const ZR: u32 = 0x80;
pub const MINUS: u32 = 0x100;
pub const PLUS: u32 = 0x200;
pub const RSTICK: u32 = 0x400;
pub const LSTICK: u32 = 0x800;
pub const HOME: u32 = 0x1000;
pub const CAPTURE: u32 = 0x2000;
pub const DOWN: u32 = 0x1_0000;
pub const UP: u32 = 0x2_0000;
pub const RIGHT: u32 = 0x4_0000;
pub const LEFT: u32 = 0x8_0000;
pub const L: u32 = 0x40_0000;
pub const ZL: u32 = 0x80_0000;

#[derive(Clone, Default)]
pub struct ProState {
    pub connected: bool,
    pub name: String,
    pub usb: bool,
    pub buttons: u32,
    /// linker Stick, -1..1 (y positiv = oben)
    pub lx: f32,
    pub ly: f32,
    /// rechter Stick, -1..1
    pub rx: f32,
    pub ry: f32,
    pub error: String,
}

pub struct SwitchPro {
    state: Arc<Mutex<ProState>>,
}

impl SwitchPro {
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(ProState::default()));
        let st = state.clone();
        let _ = std::thread::Builder::new().name("switch-pro".into()).spawn(move || run(st));
        SwitchPro { state }
    }

    pub fn get(&self) -> ProState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn run(st: Arc<Mutex<ProState>>) {
    let mut api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            st.lock().unwrap().error = e.to_string();
            return;
        }
    };
    loop {
        let _ = api.refresh_devices();
        let found = api
            .device_list()
            .find(|d| d.vendor_id() == VID && PIDS.contains(&d.product_id()))
            .map(|d| {
                let name = d.product_string().map(|s| s.to_string()).unwrap_or_else(|| "Pro Controller".into());
                let usb = !matches!(d.bus_type(), hidapi::BusType::Bluetooth);
                (d.path().to_owned(), name, usb)
            });
        if let Some((path, name, usb)) = found {
            match api.open_path(&path) {
                Ok(dev) => session(&dev, &st, &name, usb),
                Err(e) => st.lock().unwrap().error = e.to_string(),
            }
        }
        {
            let mut s = st.lock().unwrap();
            s.connected = false;
            s.buttons = 0;
            s.lx = 0.0;
            s.ly = 0.0;
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
}

fn subcommand(dev: &hidapi::HidDevice, counter: &mut u8, id: u8, args: &[u8]) {
    let mut out = [0u8; 64];
    out[0] = 0x01;
    out[1] = *counter & 0x0F;
    *counter = counter.wrapping_add(1);
    out[2..10].copy_from_slice(&[0x00, 0x01, 0x40, 0x40, 0x00, 0x01, 0x40, 0x40]); // neutrales Rumble
    out[10] = id;
    out[11..11 + args.len()].copy_from_slice(args);
    let _ = dev.write(&out);
    let mut buf = [0u8; 64];
    let _ = dev.read_timeout(&mut buf, 60);
}

fn session(dev: &hidapi::HidDevice, st: &Mutex<ProState>, name: &str, usb: bool) {
    let mut buf = [0u8; 64];
    if usb {
        for cmd in [0x02u8, 0x03, 0x02, 0x04] {
            let mut out = [0u8; 64];
            out[0] = 0x80;
            out[1] = cmd;
            let _ = dev.write(&out);
            let _ = dev.read_timeout(&mut buf, 120);
        }
    }
    let mut counter = 0u8;
    subcommand(dev, &mut counter, 0x03, &[0x30]); // vollständige Eingabeberichte
    subcommand(dev, &mut counter, 0x30, &[0x01]); // Spieler-LED 1 an
    {
        let mut s = st.lock().unwrap();
        s.connected = true;
        s.name = name.to_string();
        s.usb = usb;
        s.error.clear();
    }
    let mut last = Instant::now();
    let mut retried = false;
    loop {
        match dev.read_timeout(&mut buf, 200) {
            Ok(n) if n >= 12 && (buf[0] == 0x30 || buf[0] == 0x21) => {
                let buttons = buf[3] as u32 | (buf[4] as u32) << 8 | (buf[5] as u32) << 16;
                let rx = buf[6] as u16 | ((buf[7] as u16 & 0x0F) << 8);
                let ry = (buf[7] as u16 >> 4) | ((buf[8] as u16) << 4);
                let norm = |v: u16| ((v as f32 - 2048.0) / 1500.0).clamp(-1.0, 1.0);
                let mut s = st.lock().unwrap();
                s.buttons = buttons;
                s.lx = norm(rx);
                s.ly = norm(ry);
                let r2x = buf[9] as u16 | ((buf[10] as u16 & 0x0F) << 8);
                let r2y = (buf[10] as u16 >> 4) | ((buf[11] as u16) << 4);
                s.rx = norm(r2x);
                s.ry = norm(r2y);
                last = Instant::now();
            }
            Ok(n) if n >= 4 && buf[0] == 0x3F => {
                // einfacher Bluetooth-Bericht (falls der Modus noch nicht umgeschaltet ist)
                let (b1, b2, hat) = (buf[1] as u32, buf[2] as u32, buf[3]);
                let mut bt = 0u32;
                for (bit, v) in [(0x01, B), (0x02, A), (0x04, Y), (0x08, X), (0x10, L), (0x20, R), (0x40, ZL), (0x80, ZR)] {
                    if b1 & bit != 0 {
                        bt |= v;
                    }
                }
                for (bit, v) in [(0x01, MINUS), (0x02, PLUS), (0x08, RSTICK), (0x10, HOME), (0x20, CAPTURE)] {
                    if b2 & bit != 0 {
                        bt |= v;
                    }
                }
                bt |= match hat {
                    0 => UP,
                    1 => UP | RIGHT,
                    2 => RIGHT,
                    3 => RIGHT | DOWN,
                    4 => DOWN,
                    5 => DOWN | LEFT,
                    6 => LEFT,
                    7 => LEFT | UP,
                    _ => 0,
                };
                st.lock().unwrap().buttons = bt;
                last = Instant::now();
                if !retried {
                    retried = true;
                    subcommand(dev, &mut counter, 0x03, &[0x30]);
                }
            }
            Ok(_) => {
                if last.elapsed() > Duration::from_secs(3) {
                    return; // keine Daten mehr -> neu verbinden
                }
            }
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release switch_pro_live -- --ignored --nocapture` (Controller angeschlossen)
    #[test]
    #[ignore]
    fn switch_pro_live() {
        let api = hidapi::HidApi::new().unwrap();
        for d in api.device_list().filter(|d| d.vendor_id() == VID) {
            println!("Nintendo-Gerät: PID {:04X} {:?} {:?} Bus {:?}", d.product_id(), d.product_string(), d.manufacturer_string(), d.bus_type());
        }
        drop(api);
        let p = SwitchPro::start();
        for i in 0..16 {
            std::thread::sleep(Duration::from_millis(250));
            let s = p.get();
            println!("{:>2}: verbunden={} name='{}' usb={} tasten={:06X} stick=({:+.2},{:+.2}) fehler='{}'", i, s.connected, s.name, s.usb, s.buttons, s.lx, s.ly, s.error);
        }
    }
}
