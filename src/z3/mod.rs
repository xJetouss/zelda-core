//! Zelda-3-Modus: Spiellogik aus dem Projekt Zelda 3 (https://github.com/snesrev/zelda3, MIT-Lizenz,
//! eingebunden in `vendor/zelda3`), Spieldaten aus der US-ROM (`zelda3_assets.dat`, mit deutschen Texten).

#[allow(dead_code)]
mod data {
    include!(concat!(env!("OUT_DIR"), "/z3_data.rs"));
}

#[cfg(pastlink_z3)]
#[allow(dead_code)]
pub mod ffi {
    extern "C" {
        pub fn pl_z3_load_assets(data: *const u8, len: usize) -> i32;
        pub fn pl_z3_init(language: *const std::ffi::c_char);
        pub fn pl_z3_run_frame(pad: u16);
        pub fn pl_z3_ram() -> *mut u8;
        pub fn pl_z3_sram() -> *mut u8;
        pub fn pl_z3_vram() -> *mut u16;
        pub fn pl_z3_cgram() -> *mut u16;
        pub fn pl_z3_oam() -> *mut u16;
        pub fn pl_z3_audio(buf: *mut i16, samples: i32);
        pub fn pl_z3_draw(pixels: *mut u8, pitch: usize);
        pub fn pl_z3_reset();
        pub fn pl_z3_set_ppu_hook(hook: extern "C" fn(reg: u8, val: u8));
        pub fn pl_z3_lines(line_cb: extern "C" fn(line: i32));
        pub fn pl_z3_import_apu(ram: *const u8, dsp_regs: *const u8);
        pub fn pl_z3_export_apu(ram: *mut u8, dsp_regs: *mut u8, ports_to_spc: *mut u8);
        pub fn pl_z3_transfer_font();
        pub fn pl_z3_set_map16(table: *const u16, count: usize);
        pub fn pl_z3_get_map16(out: *mut u16, count: usize) -> usize;
        pub fn PlM16(v: u16) -> u16;
        pub fn pl_z3_fix_saved_tiles();
    }
}

use crate::snes::Snes;

/// Zelda 3 am `Snes`: aktiv oder nicht. Eine Kopie (Run-Ahead, Hintergrund-Aufnahmen) läuft ohne Zelda 3 weiter,
/// weil es den Zelda-3-Kern nur einmal gibt.
#[derive(Default)]
pub struct Slot {
    active: bool,
    /// Ton-Zustand beim nächsten Bild aus der Emulation übernehmen (Umschalten, Spielstand geladen)
    import_apu: bool,
    audio_acc: f64,
    /// deutsche BG3-Belegung (HUD/Schrift, VRAM 0x6000..0x8000) beim Wechsel zu Zelda 3 – zum Zurücksetzen
    de_bg3: Option<Vec<u16>>,
    /// Die CPU der Emulation steht in der Hauptschleife (vorher lief das Spiel in der Emulation) – nur dann kann
    /// zurückgeschaltet werden
    cpu_ok: bool,
}

impl Clone for Slot {
    fn clone(&self) -> Self {
        Slot::default()
    }
}

impl Slot {
    pub fn active(&self) -> bool {
        self.active
    }
    /// nach dem Laden eines Spielstands: Grafik/Ton neu übernehmen; die CPU des Spielstands ist gültig
    pub fn reload(&mut self) {
        self.import_apu = self.active;
        if self.active {
            self.cpu_ok = true;
        }
    }
}

#[cfg(pastlink_z3)]
mod run {
    use super::ffi::*;
    use super::*;
    use crate::snes::ppu::Ppu;
    use std::cell::Cell;
    use std::sync::Mutex;

    /// Kern einmal anlegen (der Zelda-3-Kern ist global)
    static INIT: Mutex<Option<bool>> = Mutex::new(None);

    pub fn init() -> bool {
        let mut g = INIT.lock().unwrap();
        *g.get_or_insert_with(|| unsafe {
            if pl_z3_load_assets(super::assets().as_ptr(), super::assets().len()) != 0 {
                return false;
            }
            pl_z3_init(c"de".as_ptr());
            pl_z3_set_ppu_hook(hook);
            true
        })
    }

    thread_local! {
        /// pastlinks PPU, während Zelda 3 ein Bild ausführt
        static PPU: Cell<*mut Ppu> = const { Cell::new(std::ptr::null_mut()) };
    }

    extern "C" fn hook(reg: u8, val: u8) {
        let p = PPU.with(|c| c.get());
        if !p.is_null() {
            unsafe { (*p).write(reg, val) }
        }
    }

    extern "C" fn line(i: i32) {
        let p = PPU.with(|c| c.get());
        if !p.is_null() && (1..=224).contains(&i) {
            unsafe { (*p).render_line(i as u16) }
        }
    }

    /// Map16 -> 8x8-Tabelle aus der ROM (Datei-Offset 0x78000, 3752 Einträge à 4 Wörter) an Zelda 3 geben
    pub fn set_map16(rom: &[u8]) {
        const N: usize = 3752 * 4;
        let Some(b) = rom.get(0x78000..0x78000 + N * 2) else { return };
        let t: Vec<u16> = b.chunks_exact(2).map(|c| c[0] as u16 | (c[1] as u16) << 8).collect();
        unsafe { pl_z3_set_map16(t.as_ptr(), t.len()) };
    }

    /// Ein Bild mit Zelda 3: RAM rein, Spiellogik + NMI, RAM raus, mit pastlinks PPU zeichnen, Ton abholen
    pub fn run_frame(s: &mut Snes) {
        let mod_patch = s.mod_patch.clone();
        let b = &mut s.bus;
        unsafe {
            let ram = std::slice::from_raw_parts_mut(pl_z3_ram(), 0x20000);
            let sram = std::slice::from_raw_parts_mut(pl_z3_sram(), 0x2000);
            // Änderungen der App (Cheats, Mods, Teleport, Spielstand) gelten sofort
            ram.copy_from_slice(&b.wram);
            let n = sram.len().min(b.sram.len());
            sram[..n].copy_from_slice(&b.sram[..n]);
            if s.z3.import_apu {
                // Umschalten / Spielstand geladen: Grafikspeicher und Ton aus der Emulation übernehmen (das Spiel
                // lädt nur Geändertes neu, z. B. die HUD nie)
                s.z3.import_apu = false;
                if b.apu.ram.iter().all(|&v| v == 0) {
                    // Emulation lief noch nie (Start direkt im RC-Modus, Neustart): nichts übernehmen, Zelda 3 startet
                    // selbst von vorn. Sonst würde der leere Ton-Speicher übernommen (keine Musik, kein Ton).
                    pl_z3_reset();
                } else {
                    to_us_layout(&mut s.z3, &mut b.ppu);
                    std::slice::from_raw_parts_mut(pl_z3_vram(), 0x8000).copy_from_slice(&b.ppu.vram);
                    std::slice::from_raw_parts_mut(pl_z3_cgram(), 0x100).copy_from_slice(&b.ppu.cgram);
                    std::slice::from_raw_parts_mut(pl_z3_oam() as *mut u8, 544).copy_from_slice(&b.ppu.oam);
                    pl_z3_import_apu(b.apu.ram.as_ptr(), b.apu.dsp.regs.as_ptr());
                    // Spielstände einer früheren Fassung: falsch gespeicherte Tür-/Gras-Kacheln reparieren
                    pl_z3_fix_saved_tiles();
                }
            }
            // deutsche ROM: Ziel der HUD-Tilemap ($0219) in die US-Belegung umrechnen (0x7800.. -> 0x6000..) – sonst
            // schreibt Zelda 3 die Anzeige oben (Y-Item, Rubine …) an die alte Stelle und sie bleibt stehen
            if s.z3.de_bg3.is_some() {
                let w = ram[0x219] as u16 | (ram[0x21A] as u16) << 8;
                if (0x7800..0x8000).contains(&w) {
                    let u = w - 0x1800;
                    ram[0x219] = u as u8;
                    ram[0x21A] = (u >> 8) as u8;
                }
            }
            b.frame_done = false;
            PPU.with(|c| c.set(&mut b.ppu as *mut Ppu));
            pl_z3_run_frame(b.joy[0]);
            b.wram.copy_from_slice(ram);
            if b.sram[..n] != sram[..n] {
                b.sram[..n].copy_from_slice(&sram[..n]);
                b.sram_dirty = true;
            }
            // Grafikspeicher wie bei Zelda 3, dann Zeile für Zeile mit HDMA zeichnen
            b.ppu.vram.copy_from_slice(std::slice::from_raw_parts(pl_z3_vram(), 0x8000));
            b.ppu.cgram.copy_from_slice(std::slice::from_raw_parts(pl_z3_cgram(), 0x100));
            let oam = std::slice::from_raw_parts(pl_z3_oam() as *const u8, 544);
            b.ppu.oam.copy_from_slice(oam);
            // Register, die Zelda 3 nie schreibt (es verlässt sich auf die Startwerte seiner PPU): ohne das
            // stimmen sie nach einem Start direkt im RC-Modus nicht (Sprites aus falscher VRAM-Stelle = Streifen)
            for (reg, val) in [(0x01u8, 0x02u8), (0x1A, 0x00), (0x2A, 0x00), (0x2B, 0x00), (0x33, 0x00)] {
                b.ppu.write(reg, val);
            }
            // Grafik-/Farb-Änderungen der Mods (nach dem Grafikspeicher von Zelda 3, vor dem Zeichnen)
            if let Some(p) = &mod_patch {
                p.apply(&mut b.ppu);
            }
            pl_z3_lines(line);
            PPU.with(|c| c.set(std::ptr::null_mut()));
            b.ppu.vblank_start();
            b.frame_done = true;
            // Ton: 32 kHz, Bildrate wie die Emulation (NTSC)
            s.z3.audio_acc += 32040.5 / 60.0988;
            let k = s.z3.audio_acc.floor() as usize;
            s.z3.audio_acc -= k as f64;
            let at = b.apu.dsp.out.len();
            b.apu.dsp.out.resize(at + k * 2, 0);
            pl_z3_audio(b.apu.dsp.out[at..].as_mut_ptr(), k as i32);
        }
    }

    /// Deutsche ROM: HUD-Tilemap liegt bei 0x7800 (32×64), BG3-Zeichen ab 0x6000. Zelda 3 (US) erwartet die
    /// Tilemap bei 0x6000 (64×64) und die Zeichen ab 0x7000 – die HUD-Einträge selbst sind gleich.
    fn to_us_layout(z: &mut Slot, p: &mut Ppu) {
        if p.r.bg_nba[2] != 0x6000 || p.r.bg_sc[2] != 0x7A {
            z.de_bg3 = None;
            return;
        }
        let old: Vec<u16> = p.vram[0x6000..0x8000].to_vec();
        let v = &mut p.vram;
        let (tl, bl) = (old[0x1800..0x1C00].to_vec(), old[0x1C00..0x2000].to_vec());
        v[0x7000..0x8000].copy_from_slice(&old[0x0000..0x1000]);
        v[0x6000..0x6400].copy_from_slice(&tl);
        v[0x6400..0x6800].copy_from_slice(&tl);
        v[0x6800..0x6C00].copy_from_slice(&bl);
        v[0x6C00..0x7000].copy_from_slice(&bl);
        p.write(0x09, 0x63);
        p.write(0x0C, 0x07);
        z.de_bg3 = Some(old);
    }

    /// Zurück zur deutschen Belegung (Zeichen aus der Sicherung, aktuelle HUD-Tilemap übernehmen)
    fn to_de_layout(z: &mut Slot, p: &mut Ppu) {
        let Some(old) = z.de_bg3.take() else { return };
        let (tl, bl) = (p.vram[0x6000..0x6400].to_vec(), p.vram[0x6800..0x6C00].to_vec());
        p.vram[0x6000..0x7800].copy_from_slice(&old[0x0000..0x1800]);
        p.vram[0x7800..0x7C00].copy_from_slice(&tl);
        p.vram[0x7C00..0x8000].copy_from_slice(&bl);
        p.write(0x09, 0x7A);
        p.write(0x0C, 0x06);
    }

    /// Zurück zur Emulation: Ton-Zustand übergeben
    pub fn leave(s: &mut Snes) {
        // Ziel der HUD-Tilemap zurück in die deutsche Belegung
        if s.z3.de_bg3.is_some() {
            let w = s.bus.wram[0x219] as u16 | (s.bus.wram[0x21A] as u16) << 8;
            if (0x6000..0x6800).contains(&w) {
                let d = w + 0x1800;
                s.bus.wram[0x219] = d as u8;
                s.bus.wram[0x21A] = (d >> 8) as u8;
            }
        }
        to_de_layout(&mut s.z3, &mut s.bus.ppu);
        let mut regs = [0u8; 128];
        // Stimmen nicht neu anschlagen
        let _ = &mut regs;
        let mut ports = [0u8; 4];
        unsafe { pl_z3_export_apu(s.bus.apu.ram.as_mut_ptr(), regs.as_mut_ptr(), ports.as_mut_ptr()) };
        s.bus.apu.import_dsp_regs(&regs);
        s.bus.apu.port_in = ports;
    }
}

/// Zelda-3-Modus für dieses Spiel ein- oder ausschalten. Fehler: Zurückschalten geht (noch) nicht
pub fn sync(s: &mut Snes, want: bool) -> Result<(), &'static str> {
    #[cfg(pastlink_z3)]
    {
        let want = want && available() && run::init();
        if want && !s.z3.active {
            // im Spiel (nicht beim Einschalten): CPU der Emulation steht an der Bildgrenze in der Hauptschleife
            let cpu_ok = s.bus.wram[0x10] >= 0x06 && s.bus.clock > 0;
            s.z3 = Slot { active: true, import_apu: true, cpu_ok, ..Default::default() };
            // Map16-Tabelle der geladenen ROM (die deutsche weicht von der US-Fassung ab, auf der Zelda 3 aufbaut)
            run::set_map16(&s.bus.rom);
        } else if !want && s.z3.active {
            if !s.z3.cpu_ok {
                return Err("Zurück zur Emulation geht erst, nachdem das Spiel einmal in der Emulation lief (oder einen Spielstand laden).");
            }
            run::leave(s);
            s.z3 = Slot::default();
        }
    }
    #[cfg(not(pastlink_z3))]
    let _ = (s, want);
    Ok(())
}

/// Kopie im Format der Emulation (für Spielstände/Zurückspulen im Zelda-3-Modus); None ohne Zelda 3
pub fn emu_copy(s: &Snes) -> Option<Snes> {
    #[cfg(pastlink_z3)]
    if s.z3.active {
        let mut c = s.clone();
        c.z3.de_bg3 = s.z3.de_bg3.clone();
        c.z3.active = true;
        run::leave(&mut c);
        c.z3 = Slot::default();
        return Some(c);
    }
    let _ = s;
    None
}

/// Ein Bild im Zelda-3-Modus
pub fn run_frame(s: &mut Snes) {
    #[cfg(pastlink_z3)]
    run::run_frame(s);
    #[cfg(not(pastlink_z3))]
    let _ = s;
}

/// Ist Zelda 3 eingebaut (Quellcode und Spieldaten beim Bauen gefunden)?
pub fn available() -> bool {
    cfg!(pastlink_z3) && !assets().is_empty()
}

/// Zelda-3-Spieldaten: Datei aus `PASTLINK_Z3_ASSETS`, sonst `zelda3_assets.dat` neben der .exe, sonst die beim
/// Bauen eingebauten (nur bei eigenen Builds; veröffentlichte Builds enthalten keine Spieldaten)
pub fn assets() -> &'static [u8] {
    static A: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();
    A.get_or_init(|| {
        let file = std::env::var_os("PASTLINK_Z3_ASSETS").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("zelda3_assets.dat"))));
        if let Some(b) = file.and_then(|f| std::fs::read(f).ok()).filter(|b| b.len() > 1024) {
            return Box::leak(b.into_boxed_slice());
        }
        data::ASSETS
    })
}

#[cfg(all(test, pastlink_z3))]
mod tests {
    use super::ffi::*;

    /// Zelda 3 allein: Spieldaten laden, Vorspann + Spielerauswahl, ein Bild mit dem eigenen Renderer speichern
    /// Reines Zelda 3: Startbild (Triforce) Bild für Bild speichern
    #[test]
    #[ignore]
    fn dev_z3_title() {
        unsafe {
            assert_eq!(pl_z3_load_assets(super::assets().as_ptr(), super::assets().len()), 0);
            pl_z3_init(c"de".as_ptr());
            let (w, h) = (256usize, 224usize);
            let mut px = vec![0u8; w * h * 4];
            let mut wav: Vec<i16> = Vec::new();
            let mut acc = 0.0f64;
            for f in 0..700u32 {
                pl_z3_run_frame(0);
                acc += 32040.5 / 60.0988;
                let k = acc.floor() as usize;
                acc -= k as f64;
                let at = wav.len();
                wav.resize(at + k * 2, 0);
                pl_z3_audio(wav[at..].as_mut_ptr(), k as i32);
                if f % 50 == 0 {
                    let r = std::slice::from_raw_parts(pl_z3_ram(), 0x20000);
                    println!("f{f} mod {:02X} mus {:02X}/{:02X} wav {}", r[0x10], r[0x12C], r[0x133], wav.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0));
                    pl_z3_draw(px.as_mut_ptr(), w * 4);
                    let img: Vec<crate::gfx::Color32> = px.chunks(4).map(|c| crate::gfx::Color32::from_rgb(c[2], c[1], c[0])).collect();
                    crate::devtest::bmp(&format!("{}/z3_title_{f:03}.bmp", crate::devtest::out_dir()), w, h, &img);
                }
            }
            std::fs::write(format!("{}/z3_title.raw", crate::devtest::out_dir()), wav.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn dev_z3_smoke() {
        unsafe {
            assert_eq!(pl_z3_load_assets(super::assets().as_ptr(), super::assets().len()), 0);
            pl_z3_init(c"de".as_ptr());
            let mut audio = vec![0i16; 534 * 2];
            for f in 0..900u32 {
                let pad = if f > 300 && f % 40 < 3 { 0x1000 } else { 0 };
                pl_z3_run_frame(pad);
                pl_z3_audio(audio.as_mut_ptr(), 534);
            }
            let ram = std::slice::from_raw_parts(pl_z3_ram(), 0x20000);
            println!("Modus {:02X}/{:02X}, Ton-Spitze {}", ram[0x10], ram[0x11], audio.iter().map(|s| s.unsigned_abs()).max().unwrap());
            let (w, h) = (256usize, 224usize);
            let mut px = vec![0u8; w * h * 4];
            pl_z3_draw(px.as_mut_ptr(), w * 4);
            let img: Vec<crate::gfx::Color32> = px.chunks(4).map(|c| crate::gfx::Color32::from_rgb(c[2], c[1], c[0])).collect();
            crate::devtest::bmp(&format!("{}/z3_smoke.bmp", crate::devtest::out_dir()), w, h, &img);
        }
    }
}

#[cfg(all(test, pastlink_z3))]
mod bridge_tests {
    use super::*;

    /// Rundgang: jeder Eingang innen und außen, Emulation gegen Zelda 3 (abweichende Pixel; Bilder der schlimmsten)
    #[test]
    #[ignore]
    fn dev_z3_tour() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let mut base = Snes::new(std::sync::Arc::new(rom.clone()), sram, false);
        base.load_state(&st).unwrap();
        for _ in 0..10 {
            base.run_frame();
        }
        let ents = crate::world::find_entrances(&rom);
        let mut seen = std::collections::HashSet::new();
        let mut res: Vec<(usize, String, Vec<u16>, Vec<u16>)> = Vec::new();
        let outside_only = std::env::var("Z3_OUT").is_ok();
        let only: Option<Vec<u8>> = std::env::var("Z3_EIDS").ok().map(|v| v.split(',').filter_map(|x| u8::from_str_radix(x, 16).ok()).collect());
        for e in ents.iter().filter(|e| only.as_ref().map_or(true, |o| o.contains(&e.eid))) {
            for outside in [false, true] {
                if outside_only && !outside {
                    continue;
                }
                let key = if outside { (e.area as u16 | 0x100, e.x as i32 / 64, e.y as i32 / 64) } else { (e.room, 0, 0) };
                if !seen.insert(key) {
                    continue;
                }
                let mut shots = Vec::new();
                for rc in [false, true] {
                    let mut s = base.clone();
                    sync(&mut s, rc).unwrap();
                    s.run_frame();
                    let r = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, outside);
                    for _ in 0..40 {
                        s.run_frame();
                        s.bus.apu.dsp.out.clear();
                    }
                    shots.push((r.is_ok(), s.framebuffer().to_vec(), crate::alttp::ram8(&s, 0x7E0010)));
                    let _ = sync(&mut s, false);
                }
                if !shots[0].0 || !shots[1].0 {
                    continue;
                }
                // HUD (oben) ausgelassen; Sprites bewegen sich evtl. anders – nur grobe Abweichungen zählen
                let n = (32 * 256..224 * 256).filter(|&i| shots[0].1[i] != shots[1].1[i]).count();
                res.push((n, format!("eid {:02X} {} raum {:03X} gebiet {:02X} modus {:02X}/{:02X}", e.eid, if outside { "außen" } else { "innen" }, e.room, e.area, shots[0].2, shots[1].2), shots[0].1.clone(), shots[1].1.clone()));
            }
        }
        res.sort_by(|a, b| b.0.cmp(&a.0));
        let total = res.len();
        let good = res.iter().filter(|r| r.0 < 2000).count();
        println!("Orte {total}, fast gleich (<2000 Pixel) {good}");
        for (k, r) in res.iter().take(24).enumerate() {
            println!("{:6} {}", r.0, r.1);
            let mut px: Vec<crate::gfx::Color32> = r.2.iter().map(|&c| crate::devtest::c15(c)).collect();
            px.extend(r.3.iter().map(|&c| crate::devtest::c15(c)));
            crate::devtest::bmp(&format!("{}/tour_{k:02}.bmp", crate::devtest::out_dir()), 256, 448, &px);
        }
    }

    /// Ganzes aktuelles Oberwelt-Gebiet aus dem Spielstand als Bild (BG2-Kacheln, Map16 aus dem WRAM)
    #[test]
    #[ignore]
    fn dev_z3_area() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let mut s = Snes::new(std::sync::Arc::new(rom.clone()), sram, false);
        s.load_state(&st).unwrap();
        for _ in 0..10 {
            s.run_frame();
        }
        let tab: Vec<u16> = rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect();
        let nba = s.bus.ppu.r.bg_nba[1] as usize;
        let mut px = vec![crate::gfx::Color32::from_rgb(0, 0, 0); 1024 * 1024];
        for ly in 0..1024usize {
            for lx in 0..1024usize {
                let i = 0x2000 + ((ly >> 4) * 64 + (lx >> 4)) * 2;
                let m = (s.bus.wram[i] as usize) | (s.bus.wram[i + 1] as usize) << 8;
                let q = ((ly >> 3) & 1) * 2 + ((lx >> 3) & 1);
                let e = tab[(m * 4 + q) % tab.len()] as usize;
                let (mut x, mut y) = (lx & 7, ly & 7);
                if e & 0x4000 != 0 { x = 7 - x; }
                if e & 0x8000 != 0 { y = 7 - y; }
                let base = (e & 0x3FF) * 16 + y;
                let bit = 7 - x;
                let w0 = s.bus.ppu.vram[(nba + base) & 0x7FFF] as usize;
                let w1 = s.bus.ppu.vram[(nba + base + 8) & 0x7FFF] as usize;
                let c = ((w0 >> bit) & 1) | (((w0 >> (8 + bit)) & 1) << 1) | (((w1 >> bit) & 1) << 2) | (((w1 >> (8 + bit)) & 1) << 3);
                let col = s.bus.ppu.cgram[if c == 0 { 0 } else { (((e >> 10) & 7) * 16 + c) & 0xFF }];
                px[ly * 1024 + lx] = crate::devtest::c15(col);
            }
        }
        println!("{}", crate::devtest::status(&s));
        crate::devtest::bmp(&format!("{}/area.bmp", crate::devtest::out_dir()), 1024, 1024, &px);
    }

    /// Sprites um Link: OAM-Einträge mit Palette (für Mods)
    #[test]
    #[ignore]
    fn dev_link_oam() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let mut s = Snes::new(std::sync::Arc::new(rom), sram, false);
        s.load_state(&st).unwrap();
        for _ in 0..5 {
            s.run_frame();
        }
        let w = |a: usize| s.bus.wram[a] as i32 | (s.bus.wram[a + 1] as i32) << 8;
        let (lx, ly) = (w(0x22) - w(0xE2), w(0x20) - w(0xE8));
        println!("Link auf dem Schirm {lx},{ly}");
        for i in 0..128 {
            let o = &s.bus.ppu.oam;
            let (x, y, t, a) = (o[i * 4] as i32, o[i * 4 + 1] as i32, o[i * 4 + 2], o[i * 4 + 3]);
            if (x - lx).abs() < 24 && (y - ly).abs() < 24 {
                println!("OAM {i:3}: x {x} y {y} tile {t:02X} attr {a:02X} palette {}", (a >> 1) & 7);
            }
        }
        let c: Vec<String> = (0..16).map(|k| format!("{:04X}", s.bus.ppu.cgram[0xF0 + k])).collect();
        println!("CGRAM F0: {}", c.join(" "));
        let src: Vec<String> = (0..16).map(|k| format!("{:04X}", w(0xC500 + (0xF0 + k) * 2))).collect();
        println!("WRAM C500 (F0): {}", src.join(" "));
    }

    /// Wetterhahn: aus dem Spielstand laufen (Z3_PAD, Z3_N), Breitbilder in Emulation und Zelda 3
    #[test]
    #[ignore]
    fn dev_z3_vane() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let pad: u16 = std::env::var("Z3_PAD").ok().and_then(|v| u16::from_str_radix(&v, 16).ok()).unwrap_or(0x0100);
        let n: u32 = std::env::var("Z3_N").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        let mut vr: Vec<Vec<u16>> = Vec::new();
        for rc in [false, true] {
            let mut s = Snes::new(std::sync::Arc::new(rom.clone()), sram, false);
            s.load_state(&st).unwrap();
            s.bus.ppu.ext = 96;
            for _ in 0..10 {
                s.run_frame();
            }
            sync(&mut s, rc).unwrap();
            // Z3_SEQ: "pad:bilder,pad:bilder" (hex Tasten), sonst Z3_PAD für Z3_N Bilder
            let seq: Vec<(u16, u32)> = std::env::var("Z3_SEQ").ok().map(|v| v.split(',').filter_map(|p| { let (a, b) = p.split_once(':')?; Some((u16::from_str_radix(a, 16).ok()?, b.parse().ok()?)) }).collect()).unwrap_or(vec![(pad, n)]);
            let n: u32 = seq.iter().map(|x| x.1).sum();
            let pad_at = |f: u32| { let mut t = f; for &(p, k) in &seq { if t < k { return p; } t -= k; } 0 };
            for f in 0..n {
                s.bus.joy[0] = pad_at(f);
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                if f % 30 == 29 {
                    let w = 256 + 2 * 96;
                    let tag = format!("vane_{}_{f:03}", if rc { "rc" } else { "emu" });
                    if s.bus.ppu.wide.len() == w * 224 {
                        let px: Vec<crate::gfx::Color32> = s.bus.ppu.wide.iter().map(|&c| crate::devtest::c15(c)).collect();
                        crate::devtest::bmp(&format!("{}/{tag}.bmp", crate::devtest::out_dir()), w, 224, &px);
                    } else {
                        crate::devtest::shot(&s, &tag);
                    }
                }
            }
            println!("rc={rc} {}", crate::devtest::status(&s));
            let r = s.bus.ppu.r;
            println!("  tm {:02X} ts {:02X} mode {:02X} sc {:02X?} nba {:04X?} hofs {:?} vofs {:?} cgwsel {:02X} cgadsub {:02X}", r.tm, r.ts, r.bgmode, r.bg_sc, r.bg_nba, r.hofs, r.vofs, r.cgwsel, r.cgadsub);
            let w = &s.bus.wram;
            println!("  ow-overlay $8C={:02X} $1D={:02X} F34C={:02X} 7EF280+18={:02X} 7EF298={:02X}", w[0x8C], w[0x1D], w[0xF34C], w[0xF280 + 0x18], w[0xF298]);
            let m16 = |o: usize| w[0x2000 + o] as u16 | (w[0x2001 + o] as u16) << 8;
            println!("  map16 C3E..C46: {:03X} {:03X} {:03X} {:03X} {:03X} | CBE..CC6: {:03X} {:03X} {:03X} {:03X} {:03X}", m16(0xC3E), m16(0xC40), m16(0xC42), m16(0xC44), m16(0xC46), m16(0xCBE), m16(0xCC0), m16(0xCC2), m16(0xCC4), m16(0xCC6));
            vr.push(s.bus.ppu.vram.clone());
        }
        let dv: Vec<usize> = (0..0x8000).filter(|&i| vr[0][i] != vr[1][i]).collect();
        println!("VRAM verschieden: {} Wörter, Bereiche {:X?}", dv.len(), dv.iter().map(|i| i & !0x3FF).collect::<std::collections::BTreeSet<_>>());
    }

    /// Kakariko von außen: vor jedem Eingang im Gebiet 0x18 ein Breitbild (Wetterhahn suchen)
    #[test]
    #[ignore]
    fn dev_z3_kakariko() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let ents = crate::world::find_entrances(&rom);
        let only: Option<u8> = std::env::var("Z3_EID").ok().and_then(|v| u8::from_str_radix(&v, 16).ok());
        let modes: Vec<bool> = if only.is_some() { vec![false, true] } else { vec![false] };
        for e in ents.iter().filter(|e| e.area == 0x18 && only.map_or(true, |o| o == e.eid)) {
            for &rc in &modes {
                let mut s = Snes::new(std::sync::Arc::new(rom.clone()), sram, false);
                s.load_state(&st).unwrap();
                s.bus.ppu.ext = 96;
                for _ in 0..10 {
                    s.run_frame();
                }
                sync(&mut s, rc).unwrap();
                s.run_frame();
                let r = crate::teleport::to_entrance(&mut s, e.eid, true);
                for _ in 0..90 {
                    s.run_frame();
                    s.bus.apu.dsp.out.clear();
                }
                println!("eid {:02X} rc={rc} ({:.0},{:.0}) {r:?} {}", e.eid, e.x, e.y, crate::devtest::status(&s));
                let w = 256 + 2 * 96;
                if s.bus.ppu.wide.len() == w * 224 {
                    let px: Vec<crate::gfx::Color32> = s.bus.ppu.wide.iter().map(|&c| crate::devtest::c15(c)).collect();
                    crate::devtest::bmp(&format!("{}/kak_{:02x}_{}.bmp", crate::devtest::out_dir(), e.eid, if rc { "rc" } else { "emu" }), w, 224, &px);
                } else {
                    crate::devtest::shot(&s, &format!("kak_{:02x}_{}", e.eid, if rc { "rc" } else { "emu" }));
                }
            }
        }
    }

    /// Kakariko-Brunnen (Eingang 0x18) per Teleport: Emulation und Zelda 3 im Vergleich
    #[test]
    #[ignore]
    fn dev_z3_well() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        for rc in [false, true] {
            let mut s = Snes::new(std::sync::Arc::new(rom.clone()), sram, false);
            s.load_state(&st).unwrap();
            for _ in 0..10 {
                s.run_frame();
            }
            sync(&mut s, rc).unwrap();
            for _ in 0..5 {
                s.run_frame();
            }
            let ents = crate::world::find_entrances(&rom);
            let e = ents.iter().find(|e| e.room == 0x2F || e.room == 0x3F).map(|e| e.eid).unwrap_or(0x18);
            if !rc {
                for e in ents.iter().filter(|e| e.area == 0x18 || e.room == 0x2F) {
                    println!("eid {:02X} gebiet {:02X} raum {:04X}", e.eid, e.area, e.room);
                }
            }
            let r = crate::teleport::to_entrance(&mut s, e, false);
            for f in 0..120 {
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                if f % 40 == 39 {
                    crate::devtest::shot(&s, &format!("well_{}_{f}", if rc { "rc" } else { "emu" }));
                }
            }
            println!("rc={rc} teleport {r:?} {}", crate::devtest::status(&s));
        }
    }

    /// Zelda 3 über pastlinks PPU vom Einschalten an: Startbild Bild für Bild speichern
    #[test]
    #[ignore]
    fn dev_z3_title_pl() {
        let Some((rom, sram, _)) = crate::recomp::app_rom_for_tests() else { return };
        let mut s = Snes::new(std::sync::Arc::new(rom), sram, false);
        sync(&mut s, true).unwrap();
        let step: u32 = std::env::var("Z3_STEP").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
        let n: u32 = std::env::var("Z3_N").ok().and_then(|v| v.parse().ok()).unwrap_or(700);
        let mut wav: Vec<i16> = Vec::new();
        for f in 0..n {
            s.run_frame();
            s.take_audio(&mut wav);
            if f % step == 0 {
                println!("f{f} mod {:02X} mus {:02X}/{:02X} wav {}", s.bus.wram[0x10], s.bus.wram[0x12C], s.bus.wram[0x133], wav.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0));
                crate::devtest::shot(&s, &format!("z3pl_{f:04}"));
            }
        }
        std::fs::write(format!("{}/z3pl.raw", crate::devtest::out_dir()), wav.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    }

    /// Zelda 3 über pastlinks PPU: aus deinem Spielstand (Kopie) weiterspielen, Bild speichern, zurückschalten
    #[test]
    #[ignore]
    fn dev_z3_bridge() {
        let Some((rom, sram, path)) = crate::recomp::app_rom_for_tests() else { return };
        let st = std::fs::read(path.with_extension("ss1")).unwrap();
        let mut s = Snes::new(std::sync::Arc::new(rom), sram, false);
        s.load_state(&st).unwrap();
        for _ in 0..10 {
            s.run_frame();
        }
        crate::devtest::shot(&s, "z3_emu_before");
        let lines0 = s.bus.ppu.snapshot().lines.clone();
        let l0 = lines0[20];
        println!("Emu Zeile 20: hofs {:?} vofs {:?} tm {:02X} mode {:02X} inidisp {:02X}", l0.hofs, l0.vofs, l0.tm, l0.bgmode, l0.inidisp);
        let v0 = s.bus.ppu.vram.clone();
        let c0 = s.bus.ppu.cgram.clone();
        let w0 = s.bus.wram.clone();
        sync(&mut s, true);
        assert!(s.z3.active(), "Zelda 3 nicht verfügbar");
        s.run_frame();
        let dv: Vec<usize> = (0..0x8000).filter(|&i| v0[i] != s.bus.ppu.vram[i]).collect();
        let dc = (0..256).filter(|&i| c0[i] != s.bus.ppu.cgram[i]).count();
        let dw: Vec<usize> = (0..0x20000).filter(|&i| w0[i] != s.bus.wram[i]).collect();
        println!("nach 1 Bild: VRAM {} Wörter anders (erste {:X?}), CGRAM {dc}, WRAM {} Bytes (erste {:X?})", dv.len(), &dv[..dv.len().min(8)], dw.len(), &dw[..dw.len().min(16)]);
        println!("BG-SC={:X?}", s.bus.ppu.r.bg_sc);
        crate::devtest::shot(&s, "z3_first");
        let lines1 = s.bus.ppu.snapshot().lines.clone();
        for y in [0usize, 10, 20, 40, 100, 200] {
            let a = serde_json::to_value(lines0[y]).unwrap();
            let b2 = serde_json::to_value(lines1[y]).unwrap();
            let diff: Vec<String> = a.as_object().unwrap().iter().filter(|(k, v)| b2[k.as_str()] != **v).map(|(k, v)| format!("{k}: {v} -> {}", b2[k.as_str()])).collect();
            println!("Zeile {y}: {diff:?}");
        }
        let l1 = lines1[20];
        println!("Z3  Zeile 20: hofs {:?} vofs {:?} tm {:02X} mode {:02X} inidisp {:02X}", l1.hofs, l1.vofs, l1.tm, l1.bgmode, l1.inidisp);
        let x0 = crate::alttp::ram16(&s, 0x7E0022);
        for f in 0..300u32 {
            s.bus.joy[0] = if f < 150 { 0x0100 } else { 0x0400 };
            s.run_frame();
        }
        println!("Zelda 3: Modus {:02X}, Link X {} -> {}, Ton {} Werte", s.bus.wram[0x10], x0, crate::alttp::ram16(&s, 0x7E0022), s.bus.apu.dsp.out.len());
        crate::devtest::shot(&s, "z3_bridge");
        // Spielstand aus dem Zelda-3-Modus in der Emulation laden
        let st = s.save_state();
        let mut e = Snes::new(s.bus.rom.clone(), s.bus.sram.len(), false);
        e.load_state(&st).unwrap();
        for _ in 0..30 {
            e.run_frame();
        }
        println!("Spielstand aus Zelda 3 in der Emulation: Modus {:02X}, Link X {}", e.bus.wram[0x10], crate::alttp::ram16(&e, 0x7E0022));
        crate::devtest::shot(&e, "z3_state_in_emu");
        println!("TM ram={:02X} ppu={:02X} line100={:02X} bgmode={:02X}", s.bus.wram[0x1C], s.bus.ppu.r.tm, s.bus.ppu.snapshot().lines[100].tm, s.bus.ppu.r.bgmode);
        unsafe {
            let mut px = vec![0u8; 256 * 224 * 4];
            ffi::pl_z3_draw(px.as_mut_ptr(), 256 * 4);
            let img: Vec<crate::gfx::Color32> = px.chunks(4).map(|c| crate::gfx::Color32::from_rgb(c[2], c[1], c[0])).collect();
            crate::devtest::bmp(&format!("{}/z3_own.bmp", crate::devtest::out_dir()), 256, 224, &img);
        }
        sync(&mut s, false);
        s.bus.apu.dsp.out.clear();
        for _ in 0..120 {
            s.bus.joy[0] = 0x0200;
            s.run_frame();
        }
        println!("zurück in der Emulation: Modus {:02X}, Link X {}", s.bus.wram[0x10], crate::alttp::ram16(&s, 0x7E0022));
        crate::devtest::shot(&s, "z3_emu_after");
    }
}

#[cfg(all(test, pastlink_z3))]
mod layout_probe {
    /// Grafikspeicher-Belegung der ROM aus `PASTLINK_ROM` im Spiel (BG3 = HUD/Schrift)
    #[test]
    #[ignore]
    fn dev_z3_layout() {
        let Some((s, _)) = crate::devtest::boot() else { return };
        let r = &s.bus.ppu.r;
        println!("bg_sc {:X?} bg_nba {:X?} obsel {:02X}", r.bg_sc, r.bg_nba, r.obsel);
        // benutzte VRAM-Bereiche (je 0x400 Wörter: Anzahl Wörter != 0)
        let used: Vec<String> = (0..32).map(|k| format!("{:X}:{}", k * 0x400, s.bus.ppu.vram[k * 0x400..(k + 1) * 0x400].iter().filter(|&&w| w != 0).count())).collect();
        println!("{used:?}");
        let out = std::env::var("PASTLINK_DUMP").unwrap_or_default();
        if !out.is_empty() {
            let b: Vec<u8> = s.bus.ppu.vram.iter().flat_map(|w| w.to_le_bytes()).collect();
            std::fs::write(&out, b).unwrap();
        }
    }
}

/// (Entwicklung) Map16-Tabelle der Zelda-3-Portierung auslesen
#[cfg(pastlink_z3)]
pub fn map16_table() -> Vec<u16> {
    if !run::init() {
        return Vec::new();
    }
    let mut v = vec![0u16; 3752 * 4];
    let n = unsafe { ffi::pl_z3_get_map16(v.as_mut_ptr(), v.len()) };
    v.truncate(n);
    v
}

/// (Entwicklung) deutsche Tabelle setzen und die Umrechnung fester US-Nummern abfragen
#[cfg(pastlink_z3)]
pub fn remap_check(rom: &[u8], ids: &[u16]) -> Vec<(u16, u16)> {
    if !run::init() {
        return Vec::new();
    }
    run::set_map16(rom);
    ids.iter().map(|&i| (i, unsafe { ffi::PlM16(i) })).collect()
}
