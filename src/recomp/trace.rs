//! Aufzeichnung des ausgeführten ROM-Codes für den Recompiler.
//!
//! `PASTLINK_ROM=<rom> [PASTLINK_SETTINGS=<pastlink.json>] [PASTLINK_STATES=<ordner mit .ss?-Kopien>]
//!  cargo test --release --target-dir target/test dev_recomp_trace -- --ignored --nocapture`
//!
//! Baut die ROM wie die App (mit den Spiel-Anpassungen aus den Einstellungen), spielt Vorspann, Spielstart
//! und die Spielstände mit Zufallseingaben durch und schreibt `recomp/rom.bin` und `recomp/trace.txt`.
//! Danach übersetzt `cargo build` den aufgezeichneten Code.

use crate::snes::Snes;
use std::collections::HashMap;
use std::sync::Arc;

type Rec = HashMap<(u32, u8), u64>;

fn settings() -> crate::settings::Settings {
    std::env::var("PASTLINK_SETTINGS")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// ROM wie in der App (ohne Randomizer)
pub fn app_rom() -> Option<(Vec<u8>, usize, std::path::PathBuf)> {
    let (rom, sram, path, _) = app_rom_orig()?;
    Some((rom, sram, path))
}

/// wie `app_rom`, zusätzlich die unveränderte (vorbereitete) ROM
fn app_rom_orig() -> Option<(Vec<u8>, usize, std::path::PathBuf, Vec<u8>)> {
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").ok()?);
    let (orig, info) = crate::alttp::prepare_rom(std::fs::read(&path).ok()?).ok()?;
    let mut s = settings();
    if let Some(k) = path.file_stem() {
        s.apply_game(&k.to_string_lossy());
    }
    let rom = crate::app::build_rom(&orig, None, &s.codes, (s.more_bombs, s.sword_pots, s.mirror_direct, s.quick_dash, s.health_blink, s.wide_sprites));
    Some((rom, info.sram_size, path, orig))
}

/// ein Bild mit Aufzeichnung jedes ausgeführten Befehls
fn frame(s: &mut Snes, rec: &mut Rec) {
    s.bus.frame_done = false;
    while !s.bus.frame_done {
        let (m, x, e) = s.cpu.mxe();
        let key = (((s.cpu.pb as u32) << 16) | s.cpu.pc as u32, m as u8 | (x as u8) << 1 | (e as u8) << 2);
        *rec.entry(key).or_insert(0) += 1;
        s.cpu.step(&mut s.bus);
        s.bus.process_events();
    }
    s.bus.apu.sync(s.bus.clock);
    s.bus.apu.dsp.out.clear();
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
}

/// Zufällig spielen: Richtung halten, ab und zu A/B/Y/X, selten Start
fn play(s: &mut Snes, rec: &mut Rec, frames: u32, seed: u64) {
    let mut r = Rng(seed | 1);
    let (mut dir, mut hold) = (0u16, 0u32);
    for f in 0..frames {
        if hold == 0 {
            dir = [0x0100, 0x0200, 0x0400, 0x0800, 0x0500, 0x0900, 0][r.next() as usize % 7];
            hold = 20 + r.next() % 90;
        }
        hold -= 1;
        let mut pad = dir;
        let k = r.next() % 100;
        if k < 4 {
            pad |= 0x0080; // A
        } else if k < 8 {
            pad |= 0x8000; // B
        } else if k < 10 {
            pad |= 0x4000; // Y
        } else if k < 11 {
            pad |= 0x0040; // X (Karte)
        } else if k == 11 && f % 7 == 0 {
            pad |= 0x1000; // Start (Menü)
        }
        s.bus.joy[0] = pad;
        frame(s, rec);
    }
    s.bus.joy[0] = 0;
}

#[test]
#[ignore]
fn dev_recomp_trace() {
    let Some((rom, sram_size, path, orig)) = app_rom_orig() else {
        println!("PASTLINK_ROM fehlt");
        return;
    };
    let mut rec = Rec::new();
    // 1) Einschalten, Vorspann und Demo ohne Eingabe
    let mut s = Snes::new(Arc::new(rom.clone()), sram_size, false);
    for _ in 0..4000 {
        frame(&mut s, &mut rec);
    }
    // 2) Spielstart: Dateiauswahl, neuer Name, Prolog übersprungen, dann frei herumlaufen
    let mut s = Snes::new(Arc::new(rom.clone()), sram_size, false);
    let mut patched = false;
    for f in 0..6000u32 {
        let m = crate::alttp::ram8(&s, 0x7E0010);
        if m == 0x01 && !patched {
            patched = crate::alttp::skip_prologue_sram(&mut s.bus.sram);
        }
        if m >= 0x07 && f > 600 {
            break;
        }
        s.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            0x02 | 0x03 | 0x04 => if f % 24 < 2 { 0x1000 } else if (12..14).contains(&(f % 24)) { 0x0080 } else { 0 },
            _ => if f % 16 < 2 { 0x0080 } else { 0 },
        };
        frame(&mut s, &mut rec);
    }
    play(&mut s, &mut rec, 6000, 7);
    // 3) Spielstände (Kopien) mit Zufallseingaben
    let dir = std::env::var("PASTLINK_STATES").map(std::path::PathBuf::from).unwrap_or_else(|_| path.parent().unwrap().to_path_buf());
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut files: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.len() == 3 && e.starts_with("ss") && e.as_bytes()[2].is_ascii_digit())).collect();
        files.sort();
        for p in files {
            let Ok(d) = std::fs::read(&p) else { continue };
            let mut s = Snes::new(Arc::new(rom.clone()), sram_size, false);
            if s.load_state(&d).is_err() {
                continue;
            }
            for seed in 1..=4u64 {
                let mut t = s.clone();
                play(&mut t, &mut rec, 5000, seed * 7919 + n);
            }
            n += 1;
        }
    }
    // 4) Ergebnis: Befehle mit Flags und Anzahl
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("recomp");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("rom.bin"), &rom).unwrap();
    // für den Recompiled-Modus ohne ROM-Datei: Spieldaten werden beim Bauen in die .exe übernommen
    std::fs::write(out.join("orig.bin"), &orig).unwrap();
    std::fs::write(out.join("name.txt"), path.file_stem().unwrap().to_string_lossy().as_bytes()).unwrap();
    let mut lines: Vec<_> = rec.iter().collect();
    lines.sort();
    let mut txt = String::new();
    for ((a, f), c) in lines {
        txt.push_str(&format!("{a:06X} {} {} {} {c}\n", f & 1, (f >> 1) & 1, (f >> 2) & 1));
    }
    std::fs::write(out.join("trace.txt"), txt).unwrap();
    let total: u64 = rec.values().sum();
    println!("Aufgezeichnet: {} Adressen, {} Befehle ausgeführt, {} Spielstände", rec.len(), total, n);
}

/// Gleichlauf: Interpreter und Recompiled-Modus mit denselben Eingaben – nach jedem Bild muss der ganze
/// Zustand (also der Spielstand) gleich sein. Misst nebenbei die Geschwindigkeit.
#[test]
#[ignore]
fn dev_recomp_lockstep() {
    let Some((rom, sram_size, path)) = app_rom() else {
        println!("PASTLINK_ROM fehlt");
        return;
    };
    let table = Arc::new(super::Table::new(&Arc::new(rom.clone())));
    println!("Blöcke aktiv: {} von {}", table.active, table.total);
    assert!(table.active > 0, "kein übersetzter Code passt zur ROM");
    let dir = std::env::var("PASTLINK_STATES").map(std::path::PathBuf::from).unwrap_or_else(|_| path.parent().unwrap().to_path_buf());
    let mut starts: Vec<Snes> = vec![Snes::new(Arc::new(rom.clone()), sram_size, false)];
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.len() == 3 && e.starts_with("ss") && e.as_bytes()[2].is_ascii_digit()) {
            let mut s = Snes::new(Arc::new(rom.clone()), sram_size, false);
            if s.load_state(&std::fs::read(&p).unwrap()).is_ok() {
                starts.push(s);
            }
        }
    }
    let (mut t_int, mut t_rec, mut frames) = (0f64, 0f64, 0u32);
    for (k, start) in starts.iter().enumerate() {
        let mut a = start.clone();
        let mut b = start.clone();
        b.recomp = Some(table.clone());
        let mut r = Rng(1234 + k as u64);
        let (mut dir, mut hold) = (0u16, 0u32);
        for f in 0..3000u32 {
            if hold == 0 {
                dir = [0x0100, 0x0200, 0x0400, 0x0800, 0][r.next() as usize % 5];
                hold = 20 + r.next() % 60;
            }
            hold -= 1;
            let pad = dir | if r.next() % 20 == 0 { 0x0080 } else { 0 } | if k == 0 && f % 40 < 2 { 0x1000 } else { 0 };
            a.bus.joy[0] = pad;
            b.bus.joy[0] = pad;
            let t = std::time::Instant::now();
            a.run_frame();
            t_int += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            b.run_frame();
            t_rec += t.elapsed().as_secs_f64();
            a.bus.apu.dsp.out.clear();
            b.bus.apu.dsp.out.clear();
            frames += 1;
            assert!(a.bus.clock == b.bus.clock && a.save_state() == b.save_state(), "Abweichung in Start {k}, Bild {f}");
            assert!(a.framebuffer() == b.framebuffer(), "Bild weicht ab in Start {k}, Bild {f}");
        }
        // Spielstand aus dem Recompiled-Modus im Interpreter laden und umgekehrt
        let mut x = start.clone();
        x.load_state(&b.save_state()).unwrap();
        assert!(x.save_state() == a.save_state());
    }
    println!("{frames} Bilder identisch. Interpreter {:.3} ms/Bild, Recompiled {:.3} ms/Bild (×{:.2})", t_int * 1000.0 / frames as f64, t_rec * 1000.0 / frames as f64, t_int / t_rec);
}

/// Phase 1: vollständige Code-Analyse von `recomp/rom.bin` mit der Aufzeichnung als Beleg
#[test]
#[ignore]
fn dev_recomp_analyze() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("recomp");
    let (Ok(rom), Ok(tr)) = (std::fs::read(dir.join("rom.bin")), std::fs::read_to_string(dir.join("trace.txt"))) else {
        println!("recomp/rom.bin oder trace.txt fehlt");
        return;
    };
    let trace = super::analyze::parse_trace(&tr);
    let t = std::time::Instant::now();
    let an = super::analyze::analyze(&rom, &trace);
    println!("{} ({:.2} s)", an.report(&trace), t.elapsed().as_secs_f64());
    let unres: Vec<String> = an.unresolved.iter().take(20).map(|a| format!("{a:06X}")).collect();
    println!("ungeklärt (Auszug): {unres:?}");
}
