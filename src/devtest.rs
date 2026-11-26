//! Entwickler-Experimente mit einer echten ROM (`PASTLINK_ROM=… cargo test --release dev_ -- --ignored --nocapture`).
#![cfg(test)]

use crate::alttp::{ram16, ram8};
use crate::snes::Snes;
use crate::gfx::Color32;
use std::sync::Arc;

pub fn c15(c: u16) -> Color32 {
    let e = |v: u16| ((v & 31) * 255 / 31) as u8;
    Color32::from_rgb(e(c), e(c >> 5), e(c >> 10))
}

pub fn bmp(path: &str, w: usize, h: usize, px: &[Color32]) {
    let row = (w * 3 + 3) & !3;
    let mut d = b"BM".to_vec();
    for v in [(54 + row * h) as u32, 0, 54, 40, w as u32, h as u32] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&24u16.to_le_bytes());
    for v in [0u32, (row * h) as u32, 2835, 2835, 0, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for y in (0..h).rev() {
        for x in 0..w {
            let c = px[y * w + x];
            d.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        d.resize(d.len() + row - w * 3, 0);
    }
    std::fs::write(path, d).unwrap();
}

pub fn out_dir() -> String {
    std::env::var("PASTLINK_OUT").unwrap_or_else(|_| ".".into())
}

/// Startet ein Spiel (Prolog übersprungen) und läuft bis Link draußen steht.
pub fn boot() -> Option<(Snes, Vec<u8>)> {
    let path = std::env::var("PASTLINK_ROM").ok()?;
    let p = std::path::PathBuf::from(&path);
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).ok()?).ok()?;
    let mut snes = Snes::new(Arc::new(rom.clone()), info.sram_size, false);
    if let Ok(d) = std::fs::read(p.with_extension("srm")) {
        snes.bus.sram = d;
    }
    let mut patched = crate::alttp::skip_prologue_sram(&mut snes.bus.sram);
    for f in 0..12000 {
        if ram8(&snes, 0x7E0010) == 0x09 && ram8(&snes, 0x7E0011) == 0 && f > 60 {
            break;
        }
        let m = ram8(&snes, 0x7E0010);
        // neuer Spielstand: nach dem Namen den Prolog im SRAM überspringen
        if m == 0x01 && !patched {
            patched = crate::alttp::skip_prologue_sram(&mut snes.bus.sram);
        }
        snes.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            0x02 | 0x03 | 0x04 => if f % 24 < 2 { 0x1000 } else if (12..14).contains(&(f % 24)) { 0x0080 } else { 0 },
            _ => {
                let lx = ram16(&snes, 0x7E0022);
                if f % 16 < 2 {
                    0x0080
                } else if (f / 120) % 3 == 0 {
                    0x0400
                } else if lx < 2416 {
                    0x0100
                } else if lx > 2432 {
                    0x0200
                } else {
                    0x0400
                }
            }
        };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    snes.bus.joy[0] = 0;
    for _ in 0..20 {
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    Some((snes, rom))
}

pub fn shot(s: &Snes, name: &str) {
    let px: Vec<Color32> = s.framebuffer().iter().map(|&c| c15(c)).collect();
    bmp(&format!("{}/{name}.bmp", out_dir()), 256, 224, &px);
}

pub fn status(s: &Snes) -> String {
    format!(
        "modus={:02X} sub={:02X} innen={} raum={:04X} gebiet={:02X} link=({},{}) eingang={:02X} hell={}",
        ram8(s, 0x7E0010),
        ram8(s, 0x7E0011),
        ram8(s, 0x7E001B),
        ram16(s, 0x7E00A0),
        ram8(s, 0x7E008A),
        ram16(s, 0x7E0022),
        ram16(s, 0x7E0020),
        ram8(s, 0x7E010E),
        s.bus.ppu.brightness()
    )
}

#[test]
#[ignore]
fn dev_teleport() {
    let Some((mut snes, rom)) = boot() else { return };
    println!("Start: {}", status(&snes));
    let ents = crate::world::find_entrances(&rom);
    for e in ents.iter().take(12) {
        println!("  eid {:02X} gebiet {:02X} raum {:04X} ({:.0},{:.0})", e.eid, e.area, e.room, e.x, e.y);
    }
    let targets: Vec<u8> = std::env::var("PASTLINK_TP").ok().map(|v| v.split(',').map(|x| u8::from_str_radix(x, 16).unwrap()).collect()).unwrap_or(vec![0x08, 0x02]);
    for (k, eid) in targets.iter().enumerate() {
        let ok = crate::teleport::to_entrance(&mut snes, *eid, false);
        println!("Teleport {eid:02X}: {ok:?} -> {}", status(&snes));
        for _ in 0..30 {
            snes.run_frame();
            snes.bus.apu.dsp.out.clear();
        }
        shot(&snes, &format!("tp{k}"));
        if std::env::var("PASTLINK_TP_STAY").is_ok() {
            continue;
        }
        // ein paar Schritte nach unten (raus)
        for f in 0..240 {
            snes.bus.joy[0] = if f < 200 { 0x0400 } else { 0 };
            snes.run_frame();
            snes.bus.apu.dsp.out.clear();
        }
        println!("  nach Laufen: {}", status(&snes));
        shot(&snes, &format!("tp{k}_out"));
    }
}

#[test]
#[ignore]
fn dev_wide() {
    let Some((mut snes, _rom)) = boot() else { return };
    let ext = 96usize;
    let w = 256 + 2 * ext;
    snes.bus.ppu.ext = ext;
    snes.bus.ppu.record = true;
    snes.bus.ppu.hud_split = true;
    let save = |s: &Snes, name: &str| {
        let px: Vec<Color32> = s.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
        bmp(&format!("{}/{name}.bmp", out_dir()), w, 224, &px);
    };
    // nach rechts laufen, damit gescrollt wird
    for f in 0..200 {
        snes.bus.joy[0] = if f < 180 { 0x0100 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    save(&snes, "wide_ow");
    // Interpolation: Zwischenbild bei t=0.5
    let a = snes.bus.ppu.snap.clone().unwrap();
    snes.bus.joy[0] = 0x0100;
    snes.run_frame();
    let b = snes.bus.ppu.snap.clone().unwrap();
    let mut out = Vec::new();
    let t0 = std::time::Instant::now();
    let moved = crate::snes::ppu::render_interp(&a, &b, 0.5, ext, 0x1F, &mut out);
    println!("Zwischenbild: bewegt={moved} in {:?}", t0.elapsed());
    crate::teleport::to_entrance(&mut snes, 0x08, false).unwrap();
    for _ in 0..10 {
        snes.run_frame();
    }
    save(&snes, "wide_dungeon");
    crate::teleport::to_entrance(&mut snes, 0x02, false).unwrap();
    for f in 0..200 {
        snes.bus.joy[0] = if f < 150 { 0x0400 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    save(&snes, "wide_ow2");
    println!("{}", status(&snes));
}

#[test]
#[ignore]
fn dev_xbrz() {
    let Some((snes, _rom)) = boot() else { return };
    let px: Vec<u8> = snes.framebuffer().iter().flat_map(|&c| { let k = c15(c); [k.r(), k.g(), k.b(), 255] }).collect();
    for f in [4usize, 5, 6] {
        let t0 = std::time::Instant::now();
        let mut out = Vec::new();
        for _ in 0..10 {
            out = xbrz::scale_rgba(&px, 256, 224, f);
        }
        println!("xBRZ {f}x: {:?} pro Bild", t0.elapsed() / 10);
        let c: Vec<Color32> = out.chunks_exact(4).map(|p| Color32::from_rgb(p[0], p[1], p[2])).collect();
        bmp(&format!("{}/xbrz{f}.bmp", out_dir()), 256 * f, 224 * f, &c);
    }
}

#[test]
#[ignore]
fn dev_mapmode() {
    let Some((mut snes, _rom)) = boot() else { return };
    let rf = |s: &Snes, r: usize| s.bus.wram[0xF000 + r * 2] as u16 | (s.bus.wram[0xF001 + r * 2] as u16) << 8;
    println!("Raumflags 104={:04X} 12={:04X} C9={:04X}", rf(&snes, 0x104), rf(&snes, 0x12), rf(&snes, 0xC9));
    // Karte öffnen (X)
    for f in 0..90 {
        snes.bus.joy[0] = if f < 3 { 0x0040 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
        if f % 10 == 0 {
            println!("f{f} {}", status(&snes));
        }
    }
    shot(&snes, "igmap");
    // Teleport direkt aus der Karte
    crate::alttp::ram8(&snes, 0x7E0010);
    snes.bus.wram[0x10] = 0x09;
    snes.bus.wram[0x11] = 0x00;
    let r = crate::teleport::to_entrance(&mut snes, 0x08, false);
    println!("Teleport aus Karte: {r:?} {}", status(&snes));
    for _ in 0..20 {
        snes.run_frame();
    }
    shot(&snes, "igmap_tp");
    for f in 0..200 {
        snes.bus.joy[0] = if f < 160 { 0x0400 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    println!("draußen: {}", status(&snes));
    println!("Raumflags C9={:04X}", rf(&snes, 0xC9));
    shot(&snes, "igmap_out");
}

#[test]
#[ignore]
fn dev_dmaps() {
    let Some((mut snes, rom)) = boot() else { return };
    let maps = crate::world::find_dungeon_maps(&rom);
    println!("Dungeon-Grundrisse: {}", maps.len());
    for (i, m) in maps.iter().enumerate() {
        let rooms: usize = m.floors.iter().map(|f| f.iter().filter(|&&r| r != 0x0F).count()).sum();
        let names: Vec<String> = (0..m.floors.len()).map(|k| m.floor_name(k)).collect();
        println!("  {:2} {:22} Etagen {:?} Räume {rooms}", i, crate::world::DMAP_NAMES[i].0, names);
    }
    let t = crate::randomizer::find_chest_table(&rom);
    let st = crate::tracker::find_static_items(&rom);
    let ents = crate::world::find_entrances(&rom);
    let (locs, _) = crate::world::build_locations(&rom, t.as_ref(), &st, &ents, &maps);
    let dch: usize = locs.iter().filter(|l| l.dungeon.is_some() && l.kind == crate::world::LocKind::Chests).map(|l| l.total()).sum();
    let cch: usize = locs.iter().filter(|l| l.dungeon.is_none() && l.kind == crate::world::LocKind::Chests).map(|l| l.total()).sum();
    println!("Truhen in Dungeons {dch}, in Höhlen {cch}, gesamt Tabelle {}", t.as_ref().map_or(0, |t| t.chests.len()));
    for l in locs.iter().filter(|l| l.kind == crate::world::LocKind::Chests && l.dungeon.is_some()) {
        println!("  {} ({})", l.name_de, l.total());
    }
    // Projektion auf die Karte im Spiel prüfen
    for f in 0..90 {
        snes.bus.joy[0] = if f < 3 { 0x0040 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    let mut px: Vec<Color32> = snes.framebuffer().iter().map(|&v| c15(v)).collect();
    for e in ents.iter().filter(|e| e.world == 0) {
        let Some((sx, sy)) = snes.bus.ppu.m7_project(128.0 + e.x / 16.0, 128.0 + e.y / 16.0) else { continue };
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (xx, yy) = (sx as i32 + dx, sy as i32 + dy);
                if (0..256).contains(&xx) && (0..224).contains(&yy) {
                    px[(yy * 256 + xx) as usize] = Color32::from_rgb(255, 0, 255);
                }
            }
        }
    }
    bmp(&format!("{}/igmap_proj.bmp", out_dir()), 256, 224, &px);
    // Zwischenbild-Tempo
    snes.bus.ppu.record = true;
    snes.bus.ppu.ext = 46;
    for (eid, name) in [(0x01u8, "Links Haus"), (0x04, "Schloss"), (0x09, "Wüste")] {
        let mut h = snes.clone();
        let _ = crate::teleport::to_entrance(&mut h, eid, false);
        let v: Vec<u16> = (0..8).map(|i| ram16(&h, 0x7E0600 + i * 2)).collect();
        println!("{name}: raum {:04X} 600..60E={:?} cam=({},{})", ram16(&h, 0x7E00A0), v, ram16(&h, 0x7E00E2), ram16(&h, 0x7E00E8));
    }
    let _ = crate::teleport::to_entrance(&mut snes, 0x08, false);
    for _ in 0..3 {
        snes.run_frame();
    }
    let s1 = snes.bus.ppu.snap.clone().unwrap();
    snes.run_frame();
    let s2 = snes.bus.ppu.snap.clone().unwrap();
    let mut out = Vec::new();
    let t0 = std::time::Instant::now();
    for k in 0..50 {
        crate::snes::ppu::render_interp(&s1, &s2, k as f32 / 50.0, 46, 0x1F, &mut out);
    }
    println!("Zwischenbild 16:9: {:?}", t0.elapsed() / 50);
    let src: Vec<Color32> = out.iter().map(|&v| c15(v)).collect();
    let (mut tmp, mut o) = (Vec::new(), Vec::new());
    let t1 = std::time::Instant::now();
    for _ in 0..10 {
        crate::filters::apply(crate::filters::Filter::HdSoft, &src, 348, 224, &mut tmp, &mut o);
    }
    println!("HD weich 6x 16:9: {:?}", t1.elapsed() / 10);
    bmp(&format!("{}/soft169.bmp", out_dir()), 348 * 6, 224 * 6, &o);
}

/// Savestate für App-Tests: Ostpalast besucht, Link wieder draußen vor dem Ostpalast.
#[test]
#[ignore]
fn dev_make_state() {
    let Some((mut snes, _rom)) = boot() else { return };
    let Ok(target) = std::env::var("PASTLINK_STATE_OUT") else { return };
    crate::teleport::to_entrance(&mut snes, 0x08, false).unwrap();
    for f in 0..200 {
        snes.bus.joy[0] = if f < 160 { 0x0400 } else { 0 };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    println!("{}", status(&snes));
    std::fs::write(&target, snes.save_state()).unwrap();
    let px: Vec<u8> = snes.framebuffer().iter().flat_map(|c| c.to_le_bytes()).collect();
    std::fs::write(format!("{target}.thumb"), lz4_flex::compress_prepend_size(&px)).unwrap();
}

#[test]
#[ignore]
fn dev_death() {
    let Some((mut snes, _rom)) = boot() else { return };
    snes.bus.wram[0xF36D] = 4;
    snes.bus.wram[0x0373] = 16;
    let mut seen = Vec::new();
    for _ in 0..300 {
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
        let m = ram8(&snes, 0x7E0010);
        if seen.last() != Some(&m) {
            seen.push(m);
        }
    }
    println!("Module nach Tod: {:02X?}", seen);
}

#[test]
#[ignore]
fn dev_bootmap() {
    let path = std::env::var("PASTLINK_ROM").unwrap();
    let p = std::path::PathBuf::from(&path);
    let (rom, _) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let sram = std::fs::read(p.with_extension("srm")).unwrap();
    let t0 = std::time::Instant::now();
    let r = crate::tracker::boot_capture(std::sync::Arc::new(rom), sram);
    println!("Weltkarte beim Start: {} in {:?}", r.is_some(), t0.elapsed());
    if let Some([a, _]) = r {
        bmp(&format!("{}/bootmap.bmp", out_dir()), 1024, 1024, &a);
    }
}

#[test]
#[ignore]
fn dev_map16() {
    let Some((snes, rom)) = boot() else { return };
    let w16 = |a: usize| snes.bus.wram[a] as u16 | (snes.bus.wram[a + 1] as u16) << 8;
    println!("E0={} E2={} E4={} E6={} E8={} EA={} | 8A={:02X} 40A={:02X} 708={:04X} 70C={:04X} 70E={:04X} 712={:04X} 714={:04X} 716={:04X}",
        w16(0xE0), w16(0xE2), w16(0xE4), w16(0xE6), w16(0xE8), w16(0xEA), snes.bus.wram[0x8A], snes.bus.wram[0x40A], w16(0x708), w16(0x70C), w16(0x70E), w16(0x712), w16(0x714), w16(0x716));
    let r = &snes.bus.ppu.r;
    println!("hofs {:?} vofs {:?} sc {:02X?} nba {:04X?} tm {:02X}", r.hofs, r.vofs, r.bg_sc, r.bg_nba, r.tm);
    // Tilemap-Wort von BG1/BG2 an Bildschirmposition (sx, sy)
    let tm = |n: usize, sx: u32, sy: u32| -> u16 {
        let xx = (sx + r.hofs[n] as u32) & 0x3FF;
        let yy = (sy + r.vofs[n] as u32) & 0x3FF;
        let (tx, ty) = ((xx / 8) & 63, (yy / 8) & 63);
        let base = ((r.bg_sc[n] & 0xFC) as u32) << 8;
        let mut a = base + ((ty & 31) << 5) + (tx & 31);
        if tx & 32 != 0 { a += 0x400; }
        if ty & 32 != 0 { a += 0x800; }
        snes.bus.ppu.vram[(a & 0x7FFF) as usize]
    };
    // Kandidaten: Map16-Index aus $7E2000 an Weltposition; Muster in der ROM suchen
    for n in 0..2 {
        let sx = 128u32; let sy = 112u32;
        let words: Vec<u16> = [(0, 0), (8, 0), (0, 8), (8, 8)].iter().map(|(dx, dy)| tm(n, (sx & !15) + dx - (r.hofs[n] as u32 & 15) % 16 * 0, (sy & !15) + dy)).collect();
        println!("BG{} Tilemap bei Mitte: {:04X?}", n + 1, words);
    }
    // Map16-Tabelle suchen: für mehrere Bildschirmpositionen 4 Tilemap-Wörter (16x16-Block auf 16er-Raster) -> Folge in ROM
    for n in 0..2 {
        let mut hits = std::collections::HashMap::new();
        for k in 0..12u32 {
            let wx = (r.hofs[n] as u32 + 16 + k * 16) & !15; // Welt (mod 1024) auf 16er-Raster
            let wy = (r.vofs[n] as u32 + 96) & !15;
            let sx = wx - r.hofs[n] as u32; let sy = wy - r.vofs[n] as u32;
            let q = [tm(n, sx, sy), tm(n, sx + 8, sy), tm(n, sx, sy + 8), tm(n, sx + 8, sy + 8)];
            let pat: Vec<u8> = q.iter().flat_map(|w| w.to_le_bytes()).collect();
            let mut found = Vec::new();
            let mut i = 0;
            while i + 8 <= rom.len() {
                if rom[i..i + 8] == pat[..] { found.push(i); }
                i += 2;
            }
            for f in &found { *hits.entry(f % 8).or_insert(0) += 1; }
            println!("BG{} k{k} {:04X?} -> {} Treffer {:X?}", n + 1, q, found.len(), &found[..found.len().min(4)]);
        }
    }
    // Map16-Puffer
    let m: Vec<u16> = (0..16).map(|i| w16(0x2000 + i * 2)).collect();
    println!("$7E2000: {:04X?}", m);
}

fn map16_check(snes: &Snes, rom: &[u8], label: &str) {
    let w16 = |a: usize| snes.bus.wram[a] as u16 | (snes.bus.wram[a + 1] as u16) << 8;
    let r = &snes.bus.ppu.r;
    let n = 1usize;
    let tm = |sx: u32, sy: u32| -> u16 {
        let xx = (sx + r.hofs[n] as u32) & 0x3FF;
        let yy = (sy + r.vofs[n] as u32) & 0x3FF;
        let (tx, ty) = ((xx / 8) & 63, (yy / 8) & 63);
        let base = ((r.bg_sc[n] & 0xFC) as u32) << 8;
        let mut a = base + ((ty & 31) << 5) + (tx & 31);
        if tx & 32 != 0 { a += 0x400; }
        if ty & 32 != 0 { a += 0x800; }
        snes.bus.ppu.vram[(a & 0x7FFF) as usize]
    };
    let (camx, camy) = (w16(0xE2) as i32, w16(0xE8) as i32);
    println!("{label}: 712={:04X} 8A={:02X} 40A={:02X} cam=({camx},{camy}) grenzen 600..60E={:?}", w16(0x712), snes.bus.wram[0x8A], snes.bus.wram[0x40A], (0..8).map(|i| w16(0x600 + i * 2)).collect::<Vec<_>>());
    for (an, area) in [("8A", snes.bus.wram[0x8A]), ("40A", snes.bus.wram[0x40A])] {
        for stride in [32i32, 64] {
            let a = (area & 0x3F) as i32;
            let (x0, y0) = ((a % 8) * 512, (a / 8) * 512);
            let (mut ok, mut tot) = (0, 0);
            for by in 1..13 {
                for bx in 1..15 {
                    let wx = ((camx + bx * 16) & !15) as i32;
                    let wy = ((camy + by * 16) & !15) as i32;
                    let (sx, sy) = ((wx - camx) as u32, (wy - camy) as u32);
                    let q = [tm(sx, sy), tm(sx + 8, sy), tm(sx, sy + 8), tm(sx + 8, sy + 8)];
                    let idx = ((wy - y0) / 16) * stride + (wx - x0) / 16;
                    if idx < 0 || idx >= 4096 { continue; }
                    let m = w16(0x2000 + idx as usize * 2) as usize;
                    let o = 0x78000 + m * 8;
                    let rq: Vec<u16> = (0..4).map(|k| rom[o + k * 2] as u16 | (rom[o + k * 2 + 1] as u16) << 8).collect();
                    tot += 1;
                    if rq == q { ok += 1; }
                }
            }
            println!("{label}: Gebiet ${an}={area:02X} stride {stride}: {ok}/{tot}");
        }
    }
}

#[test]
#[ignore]
fn dev_map16b() {
    let Some((mut snes, rom)) = boot() else { return };
    map16_check(&snes, &rom, "Links Haus");
    for (eid, name) in [(0x08u8, "Ostpalast"), (0x02, "Heiligtum"), (0x04, "Schloss")] {
        let _ = crate::teleport::to_entrance(&mut snes, eid, true);
        for _ in 0..10 { snes.run_frame(); }
        map16_check(&snes, &rom, name);
    }
}

/// Prolog (Regen, Nacht) vor Links Haus im Breitbild – wie im Screenshot des Nutzers.
#[test]
#[ignore]
fn dev_rain_wide() {
    let Some((mut snes, rom)) = boot() else { return };
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let ext = 96usize;
    snes.bus.ppu.ext = ext;
    for f in 0..90 {
        snes.bus.joy[0] = if f < 60 { 0x0200 } else { 0 };
        snes.bus.ppu.ow = crate::app::ow_map(&snes, &tab);
        snes.bus.ppu.hud_split = true;
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
    }
    println!("{} ow={}", status(&snes), snes.bus.ppu.ow.is_some());
    let w = 256 + 2 * ext;
    let px: Vec<Color32> = snes.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
    bmp(&format!("{}/rain_wide.bmp", out_dir()), w, 224, &px);
}

/// Neues Spiel ohne Prolog-Skip bis zur ersten Textbox; Breitbild mit geteilter Statusleiste.
#[test]
#[ignore]
fn dev_dialog_wide() {
    let path = std::env::var("PASTLINK_ROM").unwrap();
    let p = std::path::PathBuf::from(&path);
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let mut snes = Snes::new(Arc::new(rom.clone()), info.sram_size, false);
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    if let Ok(d) = std::fs::read(p.with_extension("srm")) {
        snes.bus.sram = d;
    }
    let ext = 96usize;
    snes.bus.ppu.ext = ext;
    let mut seen = Vec::new();
    let mut shots = 0;
    for f in 0..9000u32 {
        let m = ram8(&snes, 0x7E0010);
        let sub = ram8(&snes, 0x7E0011);
        if seen.last() != Some(&(m, sub)) {
            seen.push((m, sub));
        }
        snes.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
        snes.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if ram8(&snes, 0x7E001B) != 0 { 2 } else { 1 };
        snes.bus.ppu.ow = crate::app::ow_map(&snes, &tab);
        snes.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            0x02 | 0x03 | 0x04 => if f % 24 < 2 { 0x1000 } else if (12..14).contains(&(f % 24)) { 0x0080 } else { 0 },
            0x0E => if f % 90 < 2 { 0x0080 } else { 0 },
            _ => 0,
        };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
        if m == 0x0E && sub == 0x02 && f % 200 == 0 && shots < 3 {
            let w = 256 + 2 * ext;
            let px: Vec<Color32> = snes.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
            bmp(&format!("{}/dialog{shots}.bmp", out_dir()), w, 224, &px);
            shots += 1;
        }
    }
    println!("Module: {:02X?}", &seen[..seen.len().min(40)]);
}

/// Welche Tasten schließen die Weltkarte im Spiel?
#[test]
#[ignore]
fn dev_map_keys() {
    let Some((snes0, _rom)) = boot() else { return };
    for (name, key) in [("A", 0x0080u16), ("B", 0x8000), ("Y", 0x4000), ("L", 0x0020), ("R", 0x0010), ("Select", 0x2000)] {
        let mut s = snes0.clone();
        for f in 0..90 {
            s.bus.joy[0] = if f < 3 { 0x0040 } else { 0 };
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        for f in 0..60 {
            s.bus.joy[0] = if f < 4 { key } else { 0 };
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        println!("{name}: {}", status(&s));
    }
}

/// Prolog: im Haus (Raum-Rand) und draußen im Regen (Oberwelt-Rand) – Breitbild wie in der App.
#[test]
#[ignore]
fn dev_prolog_wide() {
    let path = std::env::var("PASTLINK_ROM").unwrap();
    let p = std::path::PathBuf::from(&path);
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let mut snes = Snes::new(Arc::new(rom.clone()), info.sram_size, false);
    let ext = 96usize;
    snes.bus.ppu.ext = ext;
    let w = 256 + 2 * ext;
    let save = |s: &Snes, n: &str| {
        let px: Vec<Color32> = s.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
        bmp(&format!("{}/{n}.bmp", out_dir()), w, 224, &px);
    };
    let mut shot_in = false;
    let mut outside = 0;
    for f in 0..14000u32 {
        let m = ram8(&snes, 0x7E0010);
        let sub = ram8(&snes, 0x7E0011);
        let indoors = ram8(&snes, 0x7E001B) != 0;
        let dialog = m == 0x0E && sub == 0x02;
        snes.bus.ppu.hud_split = m >= 0x06 && m != 0x14 && (m != 0x0E || dialog);
        snes.bus.ppu.margin = if m < 0x06 || m == 0x14 || (m == 0x0E && !dialog && sub != 0x07) { 0 } else if indoors { 2 } else { 1 };
        snes.bus.ppu.ow = crate::app::ow_map(&snes, &tab);
        let lx = ram16(&snes, 0x7E0022);
        snes.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            0x02 | 0x03 | 0x04 => if f % 24 < 2 { 0x1000 } else if (12..14).contains(&(f % 24)) { 0x0080 } else { 0 },
            0x0E => if f % 40 < 2 { 0x0080 } else { 0 },
            0x07 => if f % 50 < 2 { 0x0080 } else if lx < 2424 && (f / 200) % 2 == 1 { 0x0100 } else { 0x0400 },
            0x09 => 0x0200,
            _ => 0,
        };
        snes.run_frame();
        snes.bus.apu.dsp.out.clear();
        if m == 0x07 && sub == 0 && !shot_in && f > 1500 && f % 97 == 0 {
            save(&snes, "pro_in");
            shot_in = true;
        }
        if m == 0x09 && sub == 0 && snes.bus.ppu.brightness() > 0 {
            outside += 1;
            if outside == 90 {
                save(&snes, "pro_rain");
                println!("draußen: {} ow={}", status(&snes), snes.bus.ppu.ow.is_some());
                break;
            }
        }
    }
    println!("Ende: {}", status(&snes));
}

/// Jeden Eingang anspringen (mit Rauslaufen) und prüfen, ob Link draußen im richtigen Gebiet steht.
#[test]
#[ignore]
fn dev_teleport_all() {
    let Some((mut snes0, rom)) = boot() else { return };
    if let Ok(n) = std::env::var("STATE") {
        let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
        snes0.load_state(&std::fs::read(path.with_extension(format!("ss{n}"))).unwrap()).unwrap();
    }
    let ents = crate::world::find_entrances(&rom);
    // Startpunkt draußen: erst vor einen Eingang teleportieren (wie im Spiel von der Oberwelt aus)
    if let Ok(v) = std::env::var("START_EID") {
        // "LW"/"DW": erster Eingang dieser Welt, vor dem Link draußen steht
        let want = if v == "DW" { Some(1u8) } else if v == "LW" { Some(0u8) } else { None };
        for e in ents.iter().filter(|e| want.map_or(e.eid == u8::from_str_radix(&v, 16).unwrap_or(0), |w| e.world == w)) {
            let mut t = snes0.clone();
            if crate::teleport::to_target(&mut t, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true).is_ok() && ram8(&t, 0x7E0010) == 0x09 {
                snes0 = t;
                println!("Start vor Eingang {:02X} (Gebiet {:02X})", e.eid, e.area);
                break;
            }
        }
        for _ in 0..60 {
            snes0.run_frame();
            snes0.bus.apu.dsp.out.clear();
        }
        println!("Start: modus {:02X} gebiet {:02X}", ram8(&snes0, 0x7E0010), ram8(&snes0, 0x7E008A));
    }
    let mut bad = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in &ents {
        if !seen.insert((e.eid, e.area, e.x as i32, e.y as i32)) {
            continue;
        }
        let mut s = snes0.clone();
        let r = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
        for _ in 0..30 {
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        let m = ram8(&s, 0x7E0010);
        let area = ram8(&s, 0x7E008A);
        let world_flag = s.bus.wram[0xF3CA];
        let lx = ram16(&s, 0x7E0022) as f32;
        let ly = ram16(&s, 0x7E0020) as f32;
        let dist = ((lx - e.x).powi(2) + (ly - e.y).powi(2)).sqrt();
        let tab: std::sync::Arc<Vec<u16>> = std::sync::Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
        let gfx_ok = crate::app::ow_map(&s, &tab).map_or(false, |o| o.valid);
        let ok = r.is_ok() && m == 0x09 && (area & 0x40 == e.area & 0x40) && dist < 80.0 && gfx_ok;
        if !gfx_ok && r.is_ok() && m == 0x09 && dist < 80.0 {
            bad.push(format!("eid {:02X}: Position ok, aber Grafik passt nicht", e.eid));
            continue;
        }
        if ok && std::env::var("SHARED").is_ok() && ents.iter().filter(|o| o.eid == e.eid).count() > 1 {
            println!("  geteilt eid {:02X}: Ziel Gebiet {:02X} -> angekommen Gebiet {:02X} Abstand {dist:.0}", e.eid, e.area, area);
        }
        if !ok {
            bad.push(format!("eid {:02X} gebiet {:02X} raum {:04X}: {:?} -> modus {:02X} gebiet {:02X} welt {:02X} abstand {:.0} hell {}", e.eid, e.area, e.room, r, m, area, world_flag, dist, s.bus.ppu.brightness()));
        }
    }
    println!("{} Eingänge, {} fehlerhaft", seen.len(), bad.len());
    for b in &bad {
        println!("  {b}");
    }
}

#[test]
#[ignore]
fn dev_cave_cache() {
    let Some((snes, _rom)) = boot() else { return };
    let r16 = |w: &[u8], a: usize| w[a] as u16 | (w[a + 1] as u16) << 8;
    let w0 = snes.bus.wram.clone();
    let vals = [r16(&w0, 0x20), r16(&w0, 0x22), r16(&w0, 0xE2), r16(&w0, 0xE8), r16(&w0, 0xE0), r16(&w0, 0xE6)];
    println!("live link=({},{}) E2={} E8={} E0={} E6={}", vals[1], vals[0], vals[2], vals[3], vals[4], vals[5]);
    let mut s = snes.clone();
    for f in 0..200 {
        s.bus.joy[0] = if f < 120 { 0x0800 } else { 0 };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    }
    println!("nach Betreten: {}", status(&s));
    let w1 = &s.bus.wram;
    for a in 0x100..0x1FFFE {
        let (o, n) = (r16(&w0, a), r16(w1, a));
        if o != n && vals.iter().any(|v| (*v as i32 - n as i32).abs() <= 2) {
            println!("  {:05X}: {} -> {}", a, o, n);
        }
    }
    let blk: Vec<String> = (0xC140..0xC180).step_by(2).map(|a| format!("{:04X}:{}", a, r16(w1, a))).collect();
    println!("Block: {}", blk.join(" "));
    let live: Vec<String> = [0x84usize, 0x88, 0x8A, 0x8C, 0xE0, 0xE2, 0xE4, 0xE6, 0xE8, 0xEA, 0x20, 0x22, 0x600, 0x602, 0x604, 0x606, 0x608, 0x60A, 0x60C, 0x60E, 0x610, 0x612, 0x614, 0x616, 0x618, 0x61A, 0x61C, 0x61E, 0x624, 0x626, 0x628, 0x62A, 0x708, 0x70A, 0x70C, 0x70E, 0x712, 0x714, 0x716, 0x0AA, 0x2F, 0x40A, 0x1B]
        .iter()
        .map(|&a| format!("{:03X}:{}", a, r16(&w0, a)))
        .collect();
    println!("Vorher: {}", live.join(" "));
    println!();
}

#[test]
#[ignore]
fn dev_cache_samples() {
    let Some((snes0, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    let r16 = |w: &[u8], a: usize| w[a] as u16 | (w[a + 1] as u16) << 8;
    let mut n = 0;
    for e in &ents {
        if n >= 14 { break; }
        let mut s = snes0.clone();
        if crate::teleport::to_entrance(&mut s, e.eid, true).is_err() { continue; }
        for _ in 0..20 { s.run_frame(); s.bus.apu.dsp.out.clear(); }
        let lx = ram16(&s, 0x7E0022) as f32; let ly = ram16(&s, 0x7E0020) as f32;
        if ram8(&s, 0x7E0010) != 0x09 || ((lx - e.x).powi(2) + (ly - e.y).powi(2)).sqrt() > 80.0 { continue; }
        // wieder hinein (Richtung: nach oben)
        let w0: Vec<u8> = s.bus.wram.clone();
        for f in 0..240 {
            s.bus.joy[0] = if f < 200 { 0x0800 } else { 0 };
            s.run_frame(); s.bus.apu.dsp.out.clear();
            if ram8(&s, 0x7E0010) == 0x07 { break; }
        }
        if ram8(&s, 0x7E0010) != 0x07 && ram8(&s, 0x7E0010) != 0x06 { continue; }
        n += 1;
        let w = &s.bus.wram;
        let blk: Vec<String> = (0xC140..0xC172).step_by(2).map(|a| format!("{}", r16(w, a) as i16)).collect();
        println!("eid {:02X} area {:02X} live84={} E2={} E8={} | {}", e.eid, e.area, r16(&w0, 0x84), r16(&w0, 0xE2), r16(&w0, 0xE8), blk.join(" "));
        let (v142, v166) = (r16(w, 0xC142), r16(w, 0xC166));
        let a142: Vec<String> = (0..0x2000).filter(|&a| r16(&w0, a) == v142).map(|a| format!("{a:04X}")).collect();
        let a166: Vec<String> = (0..0x2000).filter(|&a| r16(&w0, a) == v166).map(|a| format!("{a:04X}")).collect();
        println!("   C142={v142} bei {:?}", &a142[..a142.len().min(12)]);
        println!("   C166={v166} bei {:?}", &a166[..a166.len().min(12)]);
    }
}

#[test]
#[ignore]
fn dev_vram_walk() {
    let Some((snes0, _rom)) = boot() else { return };
    let mut cheats = crate::alttp::Cheats::default();
    cheats.noclip = true;
    for (eid, dirs) in [(0x0Du8, [0x0400u16, 0x0100, 0x0400, 0x0200]), (0x08, [0x0400, 0x0200, 0x0400, 0x0100]), (0x09, [0x0100, 0x0800, 0x0100, 0x0400])] {
        let mut s = snes0.clone();
        if crate::teleport::to_entrance(&mut s, eid, true).is_err() { continue; }
        let area0 = ram8(&s, 0x7E040A) & 0x3F;
        let (x0, y0) = ((area0 as i32 % 8) * 512, (area0 as i32 / 8) * 512);
        for (k, d) in dirs.iter().enumerate() {
            for f in 0..200 {
                s.bus.joy[0] = *d;
                crate::alttp::apply_cheats(&mut s, &cheats, &[]);
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                if f % 25 == 0 && ram8(&s, 0x7E0010) == 0x09 && ram8(&s, 0x7E0011) == 0 {
                    let (cx, cy, v) = (ram16(&s, 0x7E00E2) as i32, ram16(&s, 0x7E00E8) as i32, ram16(&s, 0x7E0084) as i32);
                    let pred = ((cy - y0 + 12) >> 4) * 128 + ((cx - x0) >> 4) * 2;
                    println!("eid {eid:02X} seg {k} area {:02X}/{:02X} rel ({},{}) 84={v} ({:#06X}) pred={pred} {}", ram8(&s, 0x7E008A), area0, cx - x0, cy - y0, v, if pred == v { "OK" } else { "XX" });
                }
            }
        }
    }
}

#[test]
#[ignore]
fn dev_walk_to_door() {
    let Some((snes0, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    let mut cheats = crate::alttp::Cheats::default();
    cheats.noclip = true;
    let r16 = |w: &[u8], a: usize| w[a] as u16 | (w[a + 1] as u16) << 8;
    for (start, target) in [(0x0Du8, 0x3Eu8), (0x0D, 0x42), (0x0D, 0x61), (0x09, 0x4D), (0x09, 0x72)] {
        let Some(t) = ents.iter().find(|e| e.eid == target) else { continue };
        let mut s = snes0.clone();
        if crate::teleport::to_entrance(&mut s, start, true).is_err() { continue; }
        let mut entered = false;
        for f in 0..6000 {
            let (lx, ly) = (ram16(&s, 0x7E0022) as f32 + 8.0, ram16(&s, 0x7E0020) as f32 + 12.0);
            let (dx, dy) = (t.x - lx, t.y + 20.0 - ly);
            let mut p = 0u16;
            if dx > 2.0 { p |= 0x0100 } else if dx < -2.0 { p |= 0x0200 }
            if dx.abs() <= 2.0 { if dy > 2.0 { p |= 0x0400 } else { p |= 0x0800 } }
            let near = dx.abs() <= 2.0 && dy.abs() < 6.0;
            let mut c = cheats.clone();
            c.noclip = !near;
            if near { p = 0x0800; }
            s.bus.joy[0] = p;
            crate::alttp::apply_cheats(&mut s, &c, &[]);
            if !c.noclip { crate::alttp::clear_noclip(&mut s); }
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            if ram8(&s, 0x7E0010) == 0x07 || ram8(&s, 0x7E0010) == 0x06 { entered = true; let _ = f; break; }
        }
        let w = &s.bus.wram;
        let blk: Vec<String> = (0xC140..0xC172).step_by(2).map(|a| format!("{}", r16(w, a) as i16)).collect();
        println!("Ziel {target:02X} (start {start:02X}) betreten={entered} eid jetzt {:02X} | {}", ram8(&s, 0x7E010E), blk.join(" "));
        // meine berechneten Werte zum Vergleich
        let mut m = snes0.clone();
        let _ = crate::teleport::to_target(&mut m, &rom, crate::teleport::Target { eid: t.eid, area: t.area, x: t.x, y: t.y }, false);
        let w = &m.bus.wram;
        let blk: Vec<String> = (0xC140..0xC172).step_by(2).map(|a| format!("{}", r16(w, a) as i16)).collect();
        println!("        berechnet                          | {}", blk.join(" "));
    }
}

#[test]
#[ignore]
fn dev_shift() {
    let Some((snes0, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    for target in [0x3Eu8, 0x42, 0x4D, 0x5D, 0x4F, 0x3C, 0x24, 0x66] {
        let Some(t) = ents.iter().find(|e| e.eid == target) else { continue };
        let mut s = snes0.clone();
        let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: t.eid, area: t.area, x: t.x, y: t.y }, true);
        for _ in 0..20 { s.run_frame(); s.bus.apu.dsp.out.clear(); }
        let w16 = |a: usize| s.bus.wram[a] as u16 | (s.bus.wram[a + 1] as u16) << 8;
        let r = &s.bus.ppu.r;
        let tm = |sx: i32, sy: i32| -> u16 {
            let xx = ((sx + r.hofs[1] as i32) & 0x3FF) as u32;
            let yy = ((sy + r.vofs[1] as i32) & 0x3FF) as u32;
            let (tx, ty) = ((xx / 8) & 63, (yy / 8) & 63);
            let mut a = (((r.bg_sc[1] & 0xFC) as u32) << 8) + ((ty & 31) << 5) + (tx & 31);
            if tx & 32 != 0 { a += 0x400; }
            if ty & 32 != 0 { a += 0x800; }
            s.bus.ppu.vram[(a & 0x7FFF) as usize]
        };
        let (camx, camy) = (w16(0xE2) as i32, w16(0xE8) as i32);
        let area = (s.bus.wram[0x40A] & 0x3F) as i32;
        let (x0, y0) = ((area % 8) * 512, (area / 8) * 512);
        let mut best = (0, 0, 0);
        for dy in (-64..=64).step_by(8) {
            for dx in (-64..=64).step_by(8) {
                let mut ok = 0;
                for by in 2..12 {
                    for bx in 2..14 {
                        let wx = (camx + bx * 16) & !15;
                        let wy = (camy + by * 16) & !15;
                        let (lx, ly) = (wx - x0, wy - y0);
                        if lx < 0 || ly < 0 || lx >= 1024 || ly >= 1024 { continue; }
                        let m = w16(0x2000 + (((ly >> 4) * 64 + (lx >> 4)) as usize) * 2) as usize;
                        let o = 0x78000 + m * 8;
                        let want = rom[o] as u16 | (rom[o + 1] as u16) << 8;
                        if tm(wx - camx + dx, wy - camy + dy) == want { ok += 1; }
                    }
                }
                if ok > best.0 { best = (ok, dx, dy); }
            }
        }
        println!("eid {target:02X} gebiet {:02X} 84={} cam=({},{}) rel=({},{}) hofs={} vofs={} -> beste Verschiebung {:?}", s.bus.wram[0x8A], w16(0x84), camx, camy, camx - x0, camy - y0, r.hofs[1], r.vofs[1], best);
    }
}

/// Breitbild ohne Schwarz: Start-UI, Oberwelt am Gebietsrand (Weltkarte/Zwischenspeicher), Haus, Dungeon, Menü.
#[test]
#[ignore]
fn dev_margins() {
    let path = std::env::var("PASTLINK_ROM").unwrap();
    let p = std::path::PathBuf::from(&path);
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    crate::snes::ppu::clear_world_cache();
    let ext = 96usize;
    let w = 256 + 2 * ext;
    let mut shots: Vec<Vec<Color32>> = Vec::new();
    let grab = |s: &Snes, v: &mut Vec<Vec<Color32>>| v.push(s.bus.ppu.wide.iter().map(|&c| c15(c)).collect());
    let set_mode = |s: &mut Snes| {
        let m = ram8(s, 0x7E0010);
        let sub = ram8(s, 0x7E0011);
        let indoors = ram8(s, 0x7E001B) != 0;
        let dialog = m == 0x0E && sub == 0x02;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14 && (m != 0x0E || dialog);
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        s.bus.ppu.ow = crate::app::ow_map(s, &tab);
    };
    // Start-UI (Dateiauswahl)
    let mut t = Snes::new(Arc::new(rom.clone()), info.sram_size, false);
    t.bus.ppu.ext = ext;
    for f in 0..900u32 {
        let m = ram8(&t, 0x7E0010);
        if m == 0x01 && f > 600 { break; }
        t.bus.joy[0] = if m == 0x00 || m == 0x14 { if f % 30 < 3 { 0x1000 } else { 0 } } else { 0 };
        set_mode(&mut t);
        t.run_frame();
        t.bus.apu.dsp.out.clear();
    }
    grab(&t, &mut shots);
    // Spiel
    let Some((mut snes, _)) = boot() else { return };
    if let Some([a, b]) = crate::tracker::boot_capture(Arc::new(rom.clone()), snes.bus.sram.clone()) {
        for (k, img) in [a, b].iter().enumerate() {
            crate::snes::ppu::set_world_map(k, img.iter().map(|c| (c.r() as u16 >> 3) | ((c.g() as u16 >> 3) << 5) | ((c.b() as u16 >> 3) << 10)).collect());
        }
    }
    snes.bus.ppu.ext = ext;
    for _ in 0..10 { set_mode(&mut snes); snes.run_frame(); snes.bus.apu.dsp.out.clear(); }
    grab(&snes, &mut shots); // Links Haus, linker Rand = Nachbargebiet aus der Weltkarte
    // Item-Menü öffnen (Rand bleibt stehen)
    for f in 0..70 { snes.bus.joy[0] = if f < 3 { 0x1000 } else { 0 }; set_mode(&mut snes); snes.run_frame(); snes.bus.apu.dsp.out.clear(); }
    grab(&snes, &mut shots);
    for f in 0..90 { snes.bus.joy[0] = if f < 3 { 0x1000 } else { 0 }; set_mode(&mut snes); snes.run_frame(); snes.bus.apu.dsp.out.clear(); }
    // in den Ostpalast (Raum mit Grenzen)
    let _ = crate::teleport::to_entrance(&mut snes, 0x08, false);
    for _ in 0..10 { set_mode(&mut snes); snes.run_frame(); snes.bus.apu.dsp.out.clear(); }
    grab(&snes, &mut shots);
    if let Some(o) = &snes.bus.ppu.ow {
        println!("Raum: room={} cam=({},{}) x {}..{} y {}..{} hofs={:?} vofs={:?} margin={} E0={} E6={}", o.room, o.camx, o.camy, o.bx0, o.bx1, o.by0, o.by1, snes.bus.ppu.r.hofs, snes.bus.ppu.r.vofs, snes.bus.ppu.margin, ram16(&snes, 0x7E00E0), ram16(&snes, 0x7E00E6));
    } else {
        let v: Vec<u16> = (0..8).map(|i| ram16(&snes, 0x7E0600 + i * 2)).collect();
        println!("Raum: keine Ansicht 600..60E={:?} E2={} E8={} E0={} E6={} A0={:04X}", v, ram16(&snes, 0x7E00E2), ram16(&snes, 0x7E00E8), ram16(&snes, 0x7E00E0), ram16(&snes, 0x7E00E6), ram16(&snes, 0x7E00A0));
    }
    let mut all = Vec::new();
    for sh in &shots { all.extend_from_slice(sh); }
    bmp(&format!("{}/margins.bmp", out_dir()), w, 224 * shots.len(), &all);
    println!("{} Bilder", shots.len());
}

#[test]
#[ignore]
fn dev_room_bounds() {
    let Some((snes, _rom)) = boot() else { return };
    for (eid, name) in [(0x01u8, "Links Haus"), (0x08, "Ostpalast"), (0x04, "Schloss"), (0x02, "Heiligtum"), (0x4B, "Kakariko-Haus")] {
        let mut h = snes.clone();
        let r = crate::teleport::to_entrance(&mut h, eid, false);
        let v: Vec<u16> = (0..8).map(|i| ram16(&h, 0x7E0600 + i * 2)).collect();
        println!("{name}: {r:?} raum {:04X} 600..60E={:?} cam=({},{}) link=({},{})", ram16(&h, 0x7E00A0), v, ram16(&h, 0x7E00E2), ram16(&h, 0x7E00E8), ram16(&h, 0x7E0022), ram16(&h, 0x7E0020));
        println!("   A0..AF={:02X?}  0400..040F={:02X?}", (0xA0..0xB0).map(|a| h.bus.wram[a]).collect::<Vec<_>>(), (0x400..0x410).map(|a| h.bus.wram[a]).collect::<Vec<_>>());
    }
}

#[test]
#[ignore]
fn dev_qm_lamp() {
    let Some((mut snes, _rom)) = boot() else { return };
    if let Ok(n) = std::env::var("STATE") {
        let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
        snes.load_state(&std::fs::read(path.with_extension(format!("ss{n}"))).unwrap()).unwrap();
    }
    println!("Einträge: {:?}", crate::quickmenu::entries(&snes).iter().map(|e| e.idx).collect::<Vec<_>>());
    println!("can_use={} aktuell $0202={:02X}", crate::quickmenu::can_use(&snes), ram8(&snes, 0x7E0202));
    match crate::quickmenu::capture_all(&snes) {
        Some(ic) => {
            for (i, it) in ic.items.iter().enumerate() {
                if let Some(px) = it {
                    println!("Symbol {i}: {} sichtbare Pixel", px.iter().filter(|p| p.a() > 0).count());
                }
            }
            println!("Symbole gesamt: {}", ic.items.iter().filter(|i| i.is_some()).count());
            // Ausrüstung/Dungeon-Symbole nebeneinander als Bild
            let (w, h) = (ic.equip.iter().map(|e| e.1 + 2).sum::<usize>(), 20usize);
            let mut px = vec![Color32::from_rgb(0, 0, 0); w * h];
            let mut x0 = 0;
            for (k, ew, eh, p) in &ic.equip {
                println!("Ausrüstung {k}: {ew}x{eh}");
                for i in 0..ew * eh {
                    if p[i].a() > 0 { px[(i / ew) * w + x0 + i % ew] = p[i]; }
                }
                x0 += ew + 2;
            }
            bmp(&format!("{}/equip.bmp", out_dir()), w, h, &px);
        }
        None => println!("Aufnahme fehlgeschlagen"),
    }
    let mut c = snes.clone();
    for f in 0..120 { c.bus.joy[0] = if f < 3 { 0x1000 } else { 0 }; c.run_frame(); c.bus.apu.dsp.out.clear(); }
    println!("Menü: modus {:02X} sub {:02X} sel {:02X}", ram8(&c, 0x7E0010), ram8(&c, 0x7E0011), ram8(&c, 0x7E0202));
    shot(&c, "qm_lamp_menu");
}

#[test]
#[ignore]
fn dev_areas() {
    let Some((mut snes, rom)) = boot() else { return };
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::clear_areas();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    let (tx, rx) = std::sync::mpsc::channel();
    let t0 = std::time::Instant::now();
    crate::teleport::capture_areas(&snes, &rom, &[], tx);
    let mut n = 0;
    let mut ids = Vec::new();
    while let Ok((id, a)) = rx.try_recv() { ids.push(id); crate::snes::ppu::set_area(id, a); n += 1; }
    println!("{n} Gebiete in {:?}: {:02X?}", t0.elapsed(), ids);
    let ext = 96usize;
    snes.bus.ppu.ext = ext;
    let mut all: Vec<Color32> = Vec::new();
    for eid in [0x01u8, 0x08, 0x02, 0x4D] {
        let mut s = snes.clone();
        let e = crate::world::find_entrances(&rom).into_iter().find(|e| e.eid == eid).unwrap();
        let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid, area: e.area, x: e.x, y: e.y }, true);
        for _ in 0..5 {
            s.bus.ppu.margin = 1;
            s.bus.ppu.hud_split = true;
            s.bus.ppu.ow = crate::app::ow_map(&s, &tab);
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        all.extend(s.bus.ppu.wide.iter().map(|&c| c15(c)));
    }
    bmp(&format!("{}/areas.bmp", out_dir()), 256 + 2 * ext, 224 * 4, &all);
}

#[test]
#[ignore]
fn dev_100() {
    let Some((mut snes, rom)) = boot() else { return };
    let t = crate::randomizer::find_chest_table(&rom);
    let st = crate::tracker::find_static_items(&rom);
    let ents = crate::world::find_entrances(&rom);
    let maps = crate::world::find_dungeon_maps(&rom);
    let (locs, _) = crate::world::build_locations(&rom, t.as_ref(), &st, &ents, &maps);
    let manual = crate::world::apply_100(&mut snes, &locs, &ents);
    for _ in 0..30 { snes.run_frame(); snes.bus.apu.dsp.out.clear(); }
    let mut by: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for (i, l) in locs.iter().enumerate() {
        let k = format!("{:?}", match l.kind { crate::world::LocKind::HeartPiece => 0, crate::world::LocKind::HeartContainer => 1, crate::world::LocKind::Chests => 2, crate::world::LocKind::Minigame => 3 });
        let e = by.entry(k).or_default();
        let d = l.done(&snes, manual.contains(&i));
        e.0 += d; e.1 += l.total();
        if d < l.total() { println!("  fehlt: {} {}/{}", l.name_de, d, l.total()); }
    }
    println!("{:?}", by);
    let p = crate::tracker::progress(&snes);
    println!("Items {}/{} Amulette {} Kristalle {}", p.items_owned.iter().filter(|b| **b).count(), p.items_owned.len(), p.pendants, p.crystals);
    for (i, o) in p.items_owned.iter().enumerate() { if !o { println!("  Item fehlt: {}", crate::alttp::INVENTORY[i].1); } }
}

#[test]
#[ignore]
fn dev_igmap_wide() {
    let Some((mut snes, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    snes.bus.ppu.ext = ext;
    let mut all: Vec<Color32> = Vec::new();
    for presses in [1usize, 2] {
        let mut s = snes.clone();
        let mut f = 0;
        for k in 0..presses {
            for g in 0..100 {
                s.bus.joy[0] = if g < 3 { 0x0040 } else { 0 };
                s.bus.ppu.margin = 1;
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                f += 1;
            }
            let _ = k;
        }
        let _ = f;
        let mut px: Vec<Color32> = s.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
        let lines = s.bus.ppu.line_regs().to_vec();
        for e in ents.iter().filter(|e| e.world == 0) {
            let Some((x, y)) = crate::snes::ppu::m7_project_lines(&lines, 128.0 + e.x / 16.0, 128.0 + e.y / 16.0) else { continue };
            let (xi, yi) = ((x + ext as f32) as i32, y as i32);
            for dy in -1..=1 { for dx in -1..=1 {
                let (xx, yy) = (xi + dx, yi + dy);
                if (0..w as i32).contains(&xx) && (0..224).contains(&yy) { px[yy as usize * w + xx as usize] = Color32::from_rgb(255, 0, 255); }
            }}
        }
        all.extend(px);
    }
    bmp(&format!("{}/igmap_wide.bmp", out_dir()), w, 448, &all);
}

/// Rubin neben Link erzeugen, auf Link schieben -> wird er eingesammelt?
#[test]
#[ignore]
fn dev_autocollect() {
    let Some((mut s, _rom)) = boot() else { return };
    let rupees0 = ram16(&s, 0x7EF360);
    let (lx, ly) = (ram16(&s, 0x7E0022), ram16(&s, 0x7E0020));
    // freien Sprite-Platz nehmen
    let k = (0..16usize).find(|&k| s.bus.wram[0x0DD0 + k] == 0).unwrap();
    s.bus.wram[0x0E20 + k] = 0xDB; // roter Rubin (20)
    s.bus.wram[0x0DD0 + k] = 0x09; // aktiv
    let (x, y) = (lx + 40, ly);
    s.bus.wram[0x0D10 + k] = x as u8; s.bus.wram[0x0D30 + k] = (x >> 8) as u8;
    s.bus.wram[0x0D00 + k] = y as u8; s.bus.wram[0x0D20 + k] = (y >> 8) as u8;
    for _ in 0..20 { s.run_frame(); s.bus.apu.dsp.out.clear(); }
    println!("Rubin aktiv? state={:02X} typ={:02X}", s.bus.wram[0x0DD0 + k], s.bus.wram[0x0E20 + k]);
    for f in 0..60 {
        let (lx, ly) = (ram16(&s, 0x7E0022), ram16(&s, 0x7E0020));
        if s.bus.wram[0x0DD0 + k] != 0 {
            s.bus.wram[0x0D10 + k] = lx as u8; s.bus.wram[0x0D30 + k] = (lx >> 8) as u8;
            s.bus.wram[0x0D00 + k] = (ly + 4) as u8; s.bus.wram[0x0D20 + k] = ((ly + 4) >> 8) as u8;
            s.bus.wram[0x0F70 + k] = 0;
        }
        s.run_frame(); s.bus.apu.dsp.out.clear();
        if f % 10 == 0 { println!("f{f} state={:02X} rubine={} ziel={}", s.bus.wram[0x0DD0 + k], ram16(&s, 0x7EF360), ram16(&s, 0x7EF362)); }
    }
    println!("Rubine vorher {rupees0}, nachher {} (Ziel {})", ram16(&s, 0x7EF360), ram16(&s, 0x7EF362));
}

/// Textbox: wie verändert sich BG3 während der Text erscheint?
#[test]
#[ignore]
fn dev_fasttext() {
    let path = std::env::var("PASTLINK_ROM").unwrap();
    let p = std::path::PathBuf::from(&path);
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let mut s = Snes::new(Arc::new(rom.clone()), info.sram_size, false);
    let mut last_hash = 0u64;
    let mut in_dialog = 0;
    for f in 0..9000u32 {
        let m = ram8(&s, 0x7E0010);
        let sub = ram8(&s, 0x7E0011);
        s.bus.joy[0] = match m {
            0x00 | 0x14 => if f % 30 < 3 { 0x1000 } else { 0 },
            0x01 | 0x05 => if f % 30 < 3 { 0x0080 } else { 0 },
            0x02 | 0x03 | 0x04 => if f % 24 < 2 { 0x1000 } else if (12..14).contains(&(f % 24)) { 0x0080 } else { 0 },
            _ => 0,
        };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        if m == 0x0E && sub == 0x02 {
            in_dialog += 1;
            let r = &s.bus.ppu.r;
            let base = ((r.bg_sc[2] & 0xFC) as usize) << 8;
            let h = (0..0x400).fold(0u64, |h, i| h.wrapping_mul(31).wrapping_add(s.bus.ppu.vram[(base + i) & 0x7FFF] as u64));
            if in_dialog < 400 && (h != last_hash || in_dialog % 20 == 0) {
                println!("dialog f{in_dialog}: bg3 {} | 1CD0..1CDF={:02X?} 1CE0..1CEF={:02X?}", if h != last_hash { "ÄNDERT" } else { "gleich" }, (0x1CD0..0x1CE0).map(|a| s.bus.wram[a]).collect::<Vec<_>>(), (0x1CE0..0x1CF0).map(|a| s.bus.wram[a]).collect::<Vec<_>>());
            }
            last_hash = h;
            if in_dialog > 400 { break; }
        }
    }
}

/// Mehr Bomben + Töpfe mit dem Schwert (ROM-Patches)
#[test]
#[ignore]
fn dev_tweaks() {
    let Some((mut s, rom)) = boot() else { return };
    let mut t = rom.clone();
    crate::alttp::apply_tweaks(&mut t, true, true, true);
    let diff: Vec<usize> = (0..t.len()).filter(|&i| t[i] != rom[i]).collect();
    println!("Bomben-Byte {:02X?}", &t[0x3A14F..0x3A154]);
    println!("geänderte Bytes: {} ab {:X?}", diff.len(), diff.iter().take(6).collect::<Vec<_>>());
    let run = |s: &mut Snes, n: usize, joy: u16| {
        for _ in 0..n {
            s.bus.joy[0] = joy;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        s.bus.joy[0] = 0;
    };
    for (name, r) in [("original", rom.clone()), ("gepatcht", t.clone())] {
        let mut m = s.clone();
        m.bus.rom = Arc::new(r);
        m.bus.wram[0xF343] = 10;
        let ok = crate::quickmenu::equip(&mut m, crate::quickmenu::Entry { idx: 4, bottle: None });
        run(&mut m, 10, 0);
        println!("ausgerüstet: {ok} 0202={} 0303={}", m.bus.wram[0x0202], m.bus.wram[0x0303]);
        let mut max = 0;
        for _ in 0..5 {
            run(&mut m, 3, 0x4000);
            run(&mut m, 14, 0);
            let n = (0..5).filter(|&k| m.bus.wram[0x0C4A + k] == 0x07).count();
            max = max.max(n);
            println!("  slots {:02X?} bomben {}", &m.bus.wram[0x0C4A..0x0C4F], m.bus.wram[0xF343]);
        }
        println!("{name}: höchstens {max} Bomben gleichzeitig, übrig {}", m.bus.wram[0xF343]);
    }
    // Töpfe: in Links Haus
    let ents = crate::world::find_entrances(&rom);
    let Some(e) = ents.iter().find(|e| e.room == 0x104) else { println!("kein Haus"); return };
    let orig_pots = std::env::var("POT_ORIG").is_ok();
    s.bus.rom = Arc::new(if orig_pots { rom.clone() } else { t.clone() });
    let r = crate::teleport::to_entrance(&mut s, e.eid, false);
    run(&mut s, 120, 0);
    println!("Teleport {:?}: {}", r, status(&s));
    s.bus.wram[0xF359] = 2;
    // Topf-Kachel suchen (Attribut 0x70.., Ersatz-Status 0x1010)
    let attr = |s: &Snes, i: usize| s.bus.wram[0x12000 + i];
    let pots: Vec<usize> = (0..0x1000).filter(|&i| (attr(&s, i) & 0xF0) == 0x70 && { let k = (attr(&s, i) & 0xF) as usize * 2; (s.bus.wram[0x500 + k] as u16 | (s.bus.wram[0x501 + k] as u16) << 8) & 0xF0F0 == 0x1010 }).collect();
    println!("Topf-Kacheln: {}", pots.len());
    let Some(&pot) = pots.first() else { return };
    let (px, py) = ((pot % 64) * 8, (pot / 64) * 8);
    let bx = ram16(&s, 0x7E0022) & 0xFE00;
    let by = ram16(&s, 0x7E0020) & 0xFE00;
    let (lx, ly) = (bx as usize + px - 4, by as usize + py + 16);
    for (a, v) in [(0x22usize, lx), (0x20usize, ly)] {
        s.bus.wram[a] = v as u8;
        s.bus.wram[a + 1] = (v >> 8) as u8;
    }
    s.bus.wram[0x2F] = 0; // nach oben schauen
    run(&mut s, 2, 0);
    let before = pots.iter().filter(|&&i| (attr(&s, i) & 0xF0) == 0x70).count();
    shot(&s, "pot0");
    run(&mut s, 4, 0x8000);
    run(&mut s, 30, 0);
    shot(&s, "pot1");
    let after = (0..0x1000).filter(|&i| pots.contains(&i) && (attr(&s, i) & 0xF0) == 0x70).count();
    println!("Topf-Kacheln vorher {before}, nachher {after}");
}

/// Zweiter Item-Slot: $0202/$0303 direkt setzen statt Menü
#[test]
#[ignore]
fn dev_xslot() {
    let Some((mut s, rom)) = boot() else { return };
    crate::alttp::give_all_items(&mut s);
    s.bus.wram[0xF343] = 10;
    s.bus.wram[0xF377] = 30;
    let run = |s: &mut Snes, n: usize, joy: u16| {
        for _ in 0..n {
            s.bus.joy[0] = joy;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
        s.bus.joy[0] = 0;
    };
    run(&mut s, 10, 0);
    let mut map = Vec::new();
    for idx in 1..=20u8 {
        let mut m = s.clone();
        let ok = crate::quickmenu::equip(&mut m, crate::quickmenu::Entry { idx, bottle: if idx == 16 { Some(1) } else { None } });
        map.push((idx, ok, m.bus.wram[0x0202], m.bus.wram[0x0303]));
    }
    println!("idx -> 0202/0303: {:?}", map);
    // Bomben ausgerüstet, dann direkt auf Bogen umstellen und Y drücken
    let mut m = s.clone();
    crate::quickmenu::equip(&mut m, crate::quickmenu::Entry { idx: 4, bottle: None });
    let bow = map.iter().find(|x| x.0 == 1).unwrap();
    m.bus.wram[0x0202] = bow.2;
    m.bus.wram[0x0303] = bow.3;
    let (b0, a0) = (m.bus.wram[0xF343], m.bus.wram[0xF377]);
    run(&mut m, 3, 0x4000);
    run(&mut m, 30, 0);
    println!("direkt: Bomben {b0}->{} Pfeile {a0}->{}", m.bus.wram[0xF343], m.bus.wram[0xF377]);
    // Spiegel in der Lichtwelt (mit Patch)
    let mut t = rom.clone();
    crate::alttp::apply_tweaks(&mut t, true, true, true);
    let mut m = s.clone();
    m.bus.rom = Arc::new(t);
    let mir = map.iter().find(|x| x.0 == 20).unwrap();
    m.bus.wram[0x0202] = mir.2;
    m.bus.wram[0x0303] = mir.3;
    // ein Stück vom Haus weg (in der Schattenwelt steht dort der Bombenladen)
    run(&mut m, 50, 0x0400);
        println!("vor Spiegel: {}", status(&m));
    run(&mut m, 3, 0x4000);
    for k in 0..8 {
        run(&mut m, 40, 0);
        println!("  +{} {}", (k + 1) * 40, status(&m));
    }
    shot(&m, "mirror_dw");
}

/// Bild des Original-Item-Menüs (für Ausrüstungs-Symbole)
#[test]
#[ignore]
fn dev_menu_shot() {
    let Some((mut s, _rom)) = boot() else { return };
    if let Ok(n) = std::env::var("STATE") {
        // Spielstand nur lesen (im Dungeon: Karte/Kompass/Großer Schlüssel sichtbar)
        let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
        s.load_state(&std::fs::read(path.with_extension(format!("ss{n}"))).unwrap()).unwrap();
        for a in 0xF364usize..0xF36A { s.bus.wram[a] = 0xFF; }
        s.bus.wram[0xF36F] = 3;
    }
    crate::alttp::give_all_items(&mut s);
    for (a, v) in [(0xF359usize, 4u8), (0xF35A, 3), (0xF35B, 2), (0xF354, 2), (0xF355, 1), (0xF356, 1), (0xF357, 1), (0xF374, 7), (0xF37A, 0x7F), (0xF36B, 2)] {
        s.bus.wram[a] = v;
    }
    for f in 0..240 {
        s.bus.joy[0] = if f < 4 { 0x1000 } else { 0 };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    }
    println!("{}", status(&s));
    shot(&s, "menu_full");
}

/// Breitbild-Prüfung: in welchen Situationen sind die Ränder schwarz, obwohl die Mitte ein Bild zeigt?
#[test]
#[ignore]
fn dev_wide_audit() {
    let Some((mut s, rom)) = boot() else { return };
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    if let Some([a, b]) = crate::tracker::boot_capture(Arc::new(rom.clone()), s.bus.sram.clone()) {
        for (k, img) in [a, b].iter().enumerate() {
            crate::snes::ppu::set_world_map(k, img.iter().map(|c| (c.r() as u16 >> 3) | ((c.g() as u16 >> 3) << 5) | ((c.b() as u16 >> 3) << 10)).collect());
        }
    }
    let mut t = rom.clone();
    crate::alttp::apply_tweaks(&mut t, true, true, true);
    crate::alttp::apply_wide_sprites(&mut t);
    s.bus.rom = Arc::new(t);
    crate::alttp::give_all_items(&mut s);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let mut bad: Vec<(String, String)> = Vec::new();
    let mut shots: Vec<Vec<Color32>> = Vec::new();
    let mut frame = |s: &mut Snes, joy: u16, label: &str, bad: &mut Vec<(String, String)>, shots: &mut Vec<Vec<Color32>>| {
        let m = ram8(s, 0x7E0010);
        let sub = ram8(s, 0x7E0011);
        let indoors = ram8(s, 0x7E001B) != 0;
        let dialog = m == 0x0E && sub == 0x02;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14 && (m != 0x0E || dialog);
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        s.bus.ppu.ow = crate::app::ow_map(s, &tab);
        s.bus.ppu.win_stretch = m == 0x0F || m == 0x10;
        s.bus.joy[0] = joy;
        s.run_frame();
        s.bus.ppu.update_spot();
        s.bus.apu.dsp.out.clear();
        if s.bus.ppu.brightness() < 15 {
            return;
        }
        let px = &s.bus.ppu.wide;
        if px.len() < w * 224 {
            return;
        }
        let (mut mb, mut mn, mut cb, mut cn) = (0, 0, 0, 0);
        for y in 40..200 {
            for x in 0..w {
                let c = px[y * w + x] & 0x7FFF;
                let black = c == 0;
                if x < ext - 8 || x >= ext + 256 + 8 {
                    mn += 1;
                    mb += black as usize;
                } else if x >= ext + 16 && x < ext + 240 {
                    cn += 1;
                    cb += black as usize;
                }
            }
        }
        let margin_black = mb as f32 / mn as f32;
        let center_black = cb as f32 / cn as f32;
        if margin_black > 0.6 && center_black < 0.5 {
            let key = format!("{label} modus={m:02X} sub={sub:02X} innen={}", indoors as u8);
            if !bad.iter().any(|b| b.0 == key) {
                bad.push((key, status(s)));
                if shots.len() < 12 {
                    shots.push(px.iter().map(|&c| c15(c)).collect());
                }
            }
        }
    };
    let walk = |s: &mut Snes, joy: u16, n: usize, label: &str, bad: &mut Vec<(String, String)>, shots: &mut Vec<Vec<Color32>>, frame: &mut dyn FnMut(&mut Snes, u16, &str, &mut Vec<(String, String)>, &mut Vec<Vec<Color32>>)| {
        for _ in 0..n {
            frame(s, joy, label, bad, shots);
        }
    };
    // 1) Oberwelt: in alle Richtungen über Gebietsgrenzen laufen
    for (joy, n, l) in [(0x0400u16, 600, "süd"), (0x0100, 700, "ost"), (0x0800, 900, "nord"), (0x0200, 900, "west")] {
        walk(&mut s, joy, n, l, &mut bad, &mut shots, &mut frame);
    }
    // 2) Teleport zu vielen Eingängen (draußen und drinnen)
    let ents = crate::world::find_entrances(&rom);
    for (k, e) in ents.iter().enumerate().step_by(7) {
        let outside = k % 2 == 0;
        let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, outside);
        walk(&mut s, 0, 90, if outside { "tp-draussen" } else { "tp-drinnen" }, &mut bad, &mut shots, &mut frame);
        walk(&mut s, 0x0400, 120, "nach-tp-laufen", &mut bad, &mut shots, &mut frame);
    }
    // 3) Spiegel (Licht -> Schatten)
    let _ = crate::teleport::to_entrance(&mut s, ents.iter().find(|e| e.room == 0x104).map_or(0, |e| e.eid), true);
    walk(&mut s, 0, 60, "haus", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0x0400, 50, "haus", &mut bad, &mut shots, &mut frame);
    s.bus.wram[0x0202] = 20;
    s.bus.wram[0x0303] = 20;
    walk(&mut s, 0x4000, 3, "spiegel", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0, 400, "spiegel", &mut bad, &mut shots, &mut frame);
    // 4) Spielkarte (X) und Menü
    walk(&mut s, 0x0040, 3, "karte", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0, 150, "karte", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0x0040, 3, "karte-zu", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0, 120, "karte-zu", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0x1000, 3, "menü", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0, 100, "menü", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0x1000, 3, "menü-zu", &mut bad, &mut shots, &mut frame);
    walk(&mut s, 0, 100, "menü-zu", &mut bad, &mut shots, &mut frame);
    // 5) Dungeon: Ostpalast, durch Räume laufen
    let _ = crate::teleport::to_entrance(&mut s, 0x08, false);
    for (joy, n) in [(0x0800u16, 300), (0x0100, 200), (0x0200, 400), (0x0800, 300), (0x0400, 600)] {
        walk(&mut s, joy, n, "dungeon", &mut bad, &mut shots, &mut frame);
    }
    println!("{} Fälle mit schwarzen Rändern:", bad.len());
    for (k, v) in &bad {
        println!("  {k}  | {v}");
    }
    let mut all = Vec::new();
    for sh in &shots {
        all.extend_from_slice(sh);
    }
    if !shots.is_empty() {
        bmp(&format!("{}/wide_audit.bmp", out_dir()), w, 224 * shots.len(), &all);
    }
}

/// Prüft, ob die neuen ROM-Stellen gefunden werden
#[test]
#[ignore]
fn dev_tweaks2() {
    let Ok(p) = std::env::var("PASTLINK_ROM") else { return };
    let (rom, _) = crate::alttp::prepare_rom(std::fs::read(p).unwrap()).unwrap();
    let mut t = rom.clone();
    crate::alttp::apply_tweaks2(&mut t, true, true);
    let diff: Vec<usize> = (0..t.len()).filter(|&i| t[i] != rom[i]).collect();
    println!("geänderte Bytes: {diff:X?}");
}

#[test]
#[ignore]
fn dev_beep() {
    let Some((s, rom)) = boot() else { return };
    let mut t = rom.clone();
    crate::alttp::apply_tweaks2(&mut t, false, true);
    for (name, r) in [("original", rom.clone()), ("gepatcht", t)] {
        let mut m = s.clone();
        m.bus.rom = Arc::new(r);
        m.bus.wram[0xF36D] = 4; // halbes Herz
        let mut beeps = 0;
        for _ in 0..240 {
            m.run_frame();
            m.bus.apu.dsp.out.clear();
            if m.bus.wram[0x012E] == 0x2B {
                beeps += 1;
            }
        }
        println!("{name}: Warnton-Bilder {beeps}");
    }
}

/// Echtes Breitbild: Gegner leben und erscheinen auch im Rand
#[test]
#[ignore]
fn dev_wide_sprites() {
    let Some((s, rom)) = boot() else { return };
    let mut t = rom.clone();
    let n = crate::alttp::apply_wide_sprites(&mut t);
    println!("gepatchte Stellen: {n}");
    let ents = crate::world::find_entrances(&rom);
    // Gebiet mit Gegnern: Hyrule-Feld südlich des Schlosses (Eingang mit Gebiet 0x2B/0x32 …)
    for (name, r) in [("original", rom.clone()), ("breitbild", t)] {
        let mut m = s.clone();
        m.bus.rom = Arc::new(r);
        m.bus.ppu.ext = 96;
        crate::alttp::give_all_items(&mut m);
        m.bus.wram[0xF36D] = 0xA0;
        let (mut minx, mut maxx, mut drawn_margin, mut tot) = (0i32, 0i32, 0usize, 0usize);
        for e in ents.iter().filter(|e| e.world == 0).step_by(9).take(10) {
            let _ = crate::teleport::to_target(&mut m, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
            for f in 0..900 {
                m.bus.joy[0] = match (f / 150) % 4 { 0 => 0x0100, 1 => 0x0400, 2 => 0x0200, _ => 0x0800 };
                m.bus.wram[0xF36D] = 0xA0;
                m.run_frame();
                m.bus.apu.dsp.out.clear();
                if ram8(&m, 0x7E0010) != 0x09 || ram8(&m, 0x7E0011) != 0 {
                    continue;
                }
                let hofs = ram16(&m, 0x7E00E2) as i32;
                for k in 0..16 {
                    if m.bus.wram[0x0DD0 + k] >= 0x09 {
                        let x = (m.bus.wram[0x0D10 + k] as i32 | (m.bus.wram[0x0D30 + k] as i32) << 8) - hofs;
                        minx = minx.min(x);
                        maxx = maxx.max(x);
                    }
                }
                // Gegner im rechten Rand: gibt es dazu einen sichtbaren Sprite-Eintrag?
                for k in 0..16 {
                    if m.bus.wram[0x0DD0 + k] < 0x09 {
                        continue;
                    }
                    let x = (m.bus.wram[0x0D10 + k] as i32 | (m.bus.wram[0x0D30 + k] as i32) << 8) - hofs;
                    if !(264..330).contains(&x) {
                        continue;
                    }
                    let found = (0..128).any(|i| {
                        let hi = (m.bus.ppu.oam[512 + (i >> 2)] >> ((i & 3) * 2)) & 1;
                        let x9 = m.bus.ppu.oam[i * 4] as i32 | (hi as i32) << 8;
                        m.bus.ppu.oam[i * 4 + 1] < 0xE0 && (x9 - x).abs() < 20
                    });
                    tot += 1;
                    drawn_margin += found as usize;
                    if found && name == "breitbild" && tot == 3 {
                        let w = 256 + 2 * 96;
                        if m.bus.ppu.wide.len() >= w * 224 {
                            let px: Vec<Color32> = m.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
                            bmp(&format!("{}/wide_sprite.bmp", out_dir()), w, 224, &px[..w * 224]);
                        }
                    }
                }
            }
        }
        println!("{name}: lebende Gegner von x={minx} bis x={maxx} (Bild 0..255); im rechten Rand sichtbar: {drawn_margin} von {tot} Bildern");
    }
}

/// Breitbildrand wie in der App (Gebietsdaten aus `.areas`, Weltkarte): viele Orte aufnehmen
#[test]
#[ignore]
fn dev_margin_scan() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    let ok = if std::env::var("RECAP").is_ok() {
        let (tx, rx) = std::sync::mpsc::channel();
        crate::teleport::capture_areas(&s, &rom, &[], tx);
        let mut n = 0;
        while let Ok((id, a)) = rx.try_recv() {
            crate::snes::ppu::set_area(id, a);
            n += 1;
        }
        println!("frisch aufgenommen: {n}");
        n > 40
    } else {
        crate::app::load_areas(&path)
    };
    println!("Gebiete geladen: {ok}");
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    if let Some([a, b]) = crate::tracker::boot_capture(Arc::new(rom.clone()), s.bus.sram.clone()) {
        for (k, img) in [a, b].iter().enumerate() {
            crate::snes::ppu::set_world_map(k, img.iter().map(|c| (c.r() as u16 >> 3) | ((c.g() as u16 >> 3) << 5) | ((c.b() as u16 >> 3) << 10)).collect());
        }
    }
    crate::alttp::give_all_items(&mut s);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let step = |s: &mut Snes, joy: u16| {
        let m = ram8(s, 0x7E0010);
        let sub = ram8(s, 0x7E0011);
        let indoors = ram8(s, 0x7E001B) != 0;
        let dialog = m == 0x0E && sub == 0x02;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14 && (m != 0x0E || dialog);
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        s.bus.ppu.ow = crate::app::ow_map(s, &tab);
        s.bus.ppu.win_stretch = m == 0x0F || m == 0x10;
        s.bus.joy[0] = joy;
        s.run_frame();
        s.bus.ppu.update_spot();
        s.bus.apu.dsp.out.clear();
    };
    let ents = crate::world::find_entrances(&rom);
    let mut shots: Vec<Vec<Color32>> = Vec::new();
    let mut names = Vec::new();
    // Nebel/Regen-Ebene: woran erkennt man sie?
    if std::env::var("OVERLAY").is_ok() {
        for area in [0x00u8, 0x02, 0x2C, 0x18, 0x40, 0x70, 0x1B] {
            let Some(e) = ents.iter().find(|e| crate::teleport::area_parents().get(e.area as usize) == Some(&area)) else { println!("{area:02X}: kein Eingang"); continue };
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
            for _ in 0..120 { step(&mut s, 0); }
            let r = &s.bus.ppu.r;
            println!("{area:02X}: 8C={:02X} 1D={:02X} TM={:02X} TS={:02X} cgadsub={:02X} bg3hofs={} bg3vofs={} sc3={:02X}", ram8(&s, 0x7E008C), ram8(&s, 0x7E001D), r.tm, r.ts, r.cgadsub, r.hofs[2], r.vofs[2], r.bg_sc[2]);
            if area == 0x00 {
                let px: Vec<Color32> = s.bus.ppu.wide[..w * 224].iter().map(|&v| c15(v)).collect();
                bmp(&format!("{}/fog_wide.bmp", out_dir()), w, 224, &px);
            }
        }
        return;
    }
    // Wahrheit: in ein Gebiet teleportieren, hinaus- und zu Fuß wieder hineinlaufen
    if std::env::var("WALKCHECK").is_ok() {
        let cmp = |s: &Snes, tag: &str| {
            let area = ram8(s, 0x7E040A);
            if let Some(cg) = crate::snes::ppu::area_cgram(area) {
                let d: Vec<usize> = (0..128).filter(|&i| i % 16 != 0 && cg[i] != s.bus.ppu.cgram[i]).collect();
                println!("{tag}: Gebiet {area:02X}: {} abweichend, Zeilen {:?}", d.len(), d.iter().map(|i| i / 16).collect::<std::collections::BTreeSet<_>>());
            }
        };
        // echte Spiel-Logik: Haus betreten und zu Fuß verlassen; davor aus einem fremden Gebiet kommen
        for (from, target) in [(0x45u8, 0x2Bu8), (0x43, 0x32), (0x6C, 0x15), (0x2C, 0x2B)] {
            if let Some(e) = ents.iter().find(|e| e.area == from) {
                let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
                for _ in 0..90 { step(&mut s, 0); }
            }
            let Some(e) = ents.iter().find(|e| e.area == target) else { continue };
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
            for _ in 0..90 { step(&mut s, 0); }
            cmp(&s, &format!("Teleport {from:02X}->{target:02X}"));
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, false);
            for _ in 0..60 { step(&mut s, 0); }
            for _ in 0..600 {
                step(&mut s, 0x0400);
                if ram8(&s, 0x7E001B) == 0 && ram8(&s, 0x7E0010) == 0x09 && ram8(&s, 0x7E0011) == 0 { break; }
            }
            for _ in 0..60 { step(&mut s, 0); }
            cmp(&s, &format!("  zu Fuß aus dem Haus ({:02X})", ram8(&s, 0x7E008A)));
            if from == 0x2C {
                let px: Vec<Color32> = s.framebuffer().iter().map(|&c| c15(c)).collect();
                bmp(&format!("{}/pal_live.bmp", out_dir()), 256, 224, &px);
                if let Some(cg) = crate::snes::ppu::area_cgram(ram8(&s, 0x7E040A)) {
                    let mut c2 = s.clone();
                    c2.bus.ppu.cgram[80..128].copy_from_slice(&cg[80..128]);
                    c2.bus.ppu.skip = false;
                    c2.run_frame();
                    let px: Vec<Color32> = c2.framebuffer().iter().map(|&c| c15(c)).collect();
                    bmp(&format!("{}/pal_cache.bmp", out_dir()), 256, 224, &px);
                    println!("Zeile 6 live {:04X?}", &s.bus.ppu.cgram[96..112]);
                    println!("Zeile 6 Spei {:04X?}", &cg[96..112]);
                }
            }
        }
        for target in [0x2Bu8, 0x32, 0x15] {
            let Some(e) = ents.iter().find(|e| e.area == target) else { continue };
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
            for _ in 0..90 { step(&mut s, 0); }
            cmp(&s, "nach Teleport");
            'dirs: for (out, back) in [(0x0100u16, 0x0200u16), (0x0200, 0x0100), (0x0400, 0x0800), (0x0800, 0x0400)] {
                let start = ram8(&s, 0x7E040A);
                let snap = s.clone();
                for _ in 0..1200 {
                    step(&mut s, out);
                    if ram8(&s, 0x7E040A) != start && ram8(&s, 0x7E0011) == 0 {
                        for _ in 0..30 { step(&mut s, 0); }
                        for _ in 0..1200 {
                            step(&mut s, back);
                            if ram8(&s, 0x7E040A) == start && ram8(&s, 0x7E0011) == 0 {
                                for _ in 0..60 { step(&mut s, 0); }
                                cmp(&s, "zu Fuß zurück");
                                break 'dirs;
                            }
                        }
                    }
                }
                s = snap;
            }
        }
        return;
    }
    // Paletten der gespeicherten Gebiete mit dem echten Spiel vergleichen
    if std::env::var("PALCHECK").is_ok() {
        let parents = crate::teleport::area_parents();
        for e in ents.iter() {
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
            for _ in 0..90 { step(&mut s, 0); }
            if ram8(&s, 0x7E001B) != 0 || ram8(&s, 0x7E0010) != 0x09 { continue; }
            let area = ram8(&s, 0x7E008A);
            let pa = parents.get(area as usize).copied().unwrap_or(area);
            if let Some(cg) = crate::snes::ppu::area_cgram(pa) {
                let live = &s.bus.ppu.cgram;
                let d: Vec<usize> = (0..128).filter(|&i| i % 16 != 0 && cg[i] != live[i]).collect();
                if !d.is_empty() {
                    println!("Gebiet {area:02X} (Haupt {pa:02X}): {} abweichende BG-Farben, Zeilen {:?}", d.len(), d.iter().map(|i| i / 16).collect::<std::collections::BTreeSet<_>>());
                }
            } else {
                println!("Gebiet {area:02X}: keine Daten");
            }
        }
        return;
    }
    for e in ents.iter().filter(|e| e.world == 0).step_by(4).take(16) {
        let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
        for _ in 0..120 {
            step(&mut s, 0);
        }
        for _ in 0..40 {
            step(&mut s, 0x0400);
        }
        if s.bus.ppu.wide.len() >= w * 224 {
            shots.push(s.bus.ppu.wide[..w * 224].iter().map(|&c| c15(c)).collect());
            names.push(format!("{:02X}", ram8(&s, 0x7E008A)));
            if std::env::var("LAYERS").is_ok() && shots.len() == 5 {
                // gleiche Stelle ohne BG1 bzw. ohne BG2
                for mask in [0x1Eu8, 0x1D] {
                    let mut c = s.clone();
                    c.bus.ppu.layer_mask = mask;
                    step(&mut c, 0);
                    let px: Vec<Color32> = c.bus.ppu.wide[..w * 224].iter().map(|&v| c15(v)).collect();
                    bmp(&format!("{}/layer_{mask:02X}.bmp", out_dir()), w, 224, &px);
                }
                let px: Vec<Color32> = s.bus.ppu.wide[..w * 224].iter().map(|&v| c15(v)).collect();
                bmp(&format!("{}/layer_1F.bmp", out_dir()), w, 224, &px);
                println!("Ebenen-Bilder gespeichert, Gebiet {:02X} Kamera {} {}", ram8(&s, 0x7E008A), ram16(&s, 0x7E00E2), ram16(&s, 0x7E00E8));
                // Rand-Kacheln: Map16-Version gegen echte Tilemap im VRAM vergleichen
                if let Some(o) = crate::app::ow_map(&s, &tab) {
                    let r = &s.bus.ppu.r;
                    let mut diff = std::collections::BTreeMap::new();
                    for sy in (0..224).step_by(8) {
                        for sx in (-96..0).step_by(8) {
                            let wx = o.camx + sx;
                            let wy = o.camy + sy;
                            let (lx, ly) = (wx - o.x0, wy - o.y0);
                            if lx < 0 || ly < 0 || lx >= o.size || ly >= o.size { continue; }
                            let m = o.m16[((ly >> 4) * 64 + (lx >> 4)) as usize] as usize;
                            let q = (((ly >> 3) & 1) * 2 + ((lx >> 3) & 1)) as usize;
                            let e = o.tab[m * 4 + q];
                            let xx = ((sx + r.hofs[1] as i32) & 0x3FF) as u32;
                            let yy = ((sy + r.vofs[1] as i32) & 0x3FF) as u32;
                            let (tx, ty) = ((xx / 8) & 63, (yy / 8) & 63);
                            let mut a = (((r.bg_sc[1] & 0xFC) as u32) << 8) + ((ty & 31) << 5) + (tx & 31);
                            if tx & 32 != 0 { a += 0x400; }
                            if ty & 32 != 0 { a += 0x800; }
                            let v = s.bus.ppu.vram[(a & 0x7FFF) as usize];
                            if v != e { *diff.entry((m, e, v)).or_insert(0) += 1; }
                        }
                    }
                    println!("abweichende Rand-Kacheln (map16, tab, vram): {:X?}", diff.iter().take(20).collect::<Vec<_>>());
                    for (wxp, wyp) in [(30, 115), (40, 120), (230, 122), (245, 125), (60, 130)] {
                        let (sx, sy) = (wxp as i32 - 96, wyp as i32);
                        let (lx, ly) = (o.camx + sx - o.x0, o.camy + sy - o.y0);
                        let m = o.m16[((ly >> 4) * 64 + (lx >> 4)) as usize];
                        let q = (((ly >> 3) & 1) * 2 + ((lx >> 3) & 1)) as usize;
                        let e = o.tab[m as usize * 4 + q];
                        let px = s.bus.ppu.wide[sy as usize * w + wxp];
                        println!("  bild ({wxp},{wyp}) map16={m:03X} kachel={e:04X} pal={} farbe={px:04X}", (e >> 10) & 7);
                    }
                }
            }
        }
    }
    println!("Gebiete: {}", names.join(" "));
    // Kontaktbogen 2 Spalten
    let cols = 2;
    let rows = (shots.len() + cols - 1) / cols;
    let mut all = vec![Color32::BLACK; w * cols * 224 * rows];
    for (k, sh) in shots.iter().enumerate() {
        let (cx, cy) = (k % cols, k / cols);
        for y in 0..224 {
            for x in 0..w {
                all[(cy * 224 + y) * w * cols + cx * w + x] = sh[y * w + x];
            }
        }
    }
    bmp(&format!("{}/margin_scan.bmp", out_dir()), w * cols, 224 * rows, &all);
}

/// Genaue Kartenpositionen: Figuren aus der ROM gegen die Schätzung
#[test]
#[ignore]
fn dev_npc_pos() {
    let Ok(p) = std::env::var("PASTLINK_ROM") else { return };
    let (rom, _) = crate::alttp::prepare_rom(std::fs::read(p).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let npcs = crate::world::find_npcs(&rom, &ents);
    for n in npcs.iter().filter(|n| n.1 == 1 && n.2 < 1024.0 && n.3 > 2560.0 && n.3 < 3072.0) {
        println!("  NPC {:02X} welt {} {:.0},{:.0}", n.0, n.1, n.2, n.3);
    }
    println!("{} Figuren gefunden", npcs.len());
    use crate::mapdata::*;
    let show = |name: &str, id: u8, est: (u8, f32, f32)| {
        let r = npc_pos(&npcs, id, est);
        let d = ((r.1 - est.1).powi(2) + (r.2 - est.2).powi(2)).sqrt();
        println!("{name:<45} id={id:02X} geschätzt=({:.0},{:.0}) genau=({:.0},{:.0}) Abstand {d:.0}{}", est.1, est.2, r.1, r.2, if id != 0 && d == 0.0 { "  (nicht gefunden)" } else { "" });
    };
    for (i, b) in BOTTLES.iter().enumerate() { show(b.de, BOTTLE_NPC[i], b.pos()); }
    for (i, u) in UPGRADES.iter().enumerate() { show(u.de, UPGRADE_NPC[i], u.pos()); }
    for (i, q) in QUESTS.iter().enumerate() { if let Some(e) = q.pos() { show(q.de, QUEST_NPC[i], e); } }
}

/// Gebietswechsel im Breitbild: Bilder während des Scrollens aufnehmen
#[test]
#[ignore]
fn dev_scroll_margin() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    let ok = crate::app::load_areas(&path);
    println!("Gebiete geladen: {ok}");
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    if let Some([a, b]) = crate::tracker::boot_capture(Arc::new(rom.clone()), s.bus.sram.clone()) {
        for (k, img) in [a, b].iter().enumerate() {
            crate::snes::ppu::set_world_map(k, img.iter().map(|c| (c.r() as u16 >> 3) | ((c.g() as u16 >> 3) << 5) | ((c.b() as u16 >> 3) << 10)).collect());
        }
    }
    crate::alttp::give_all_items(&mut s);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let mut shots: Vec<Vec<Color32>> = Vec::new();
    let mut prev_sub = 0;
    let mut fc = 0u32;
    let ents = crate::world::find_entrances(&rom);
    let mut plan: Vec<(Option<crate::world::Entrance>, u16, usize)> = Vec::new();
    for area in [0x2Bu8, 0x33, 0x18, 0x22, 0x1E, 0x35] {
        if let Some(e) = ents.iter().find(|e| e.area == area) {
            plan.push((Some(*e), 0x0400, 120));
            for joy in [0x0100u16, 0x0200, 0x0400, 0x0800] {
                plan.push((None, joy, 500));
            }
        }
    }
    for (tp, joy, n) in plan {
        if let Some(e) = tp {
            let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
        }
        for _ in 0..n {
            let m = ram8(&s, 0x7E0010);
            let sub = ram8(&s, 0x7E0011);
            let indoors = ram8(&s, 0x7E001B) != 0;
            s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
            s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
            s.bus.ppu.ow = crate::app::ow_map(&s, &tab);
            s.bus.joy[0] = joy;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            let scrolling = m == 0x09 && sub != 0;
            if scrolling && shots.len() < 24 && { fc += 1; fc % 6 == 0 } {
                shots.push(s.bus.ppu.wide[..w * 224].iter().map(|&c| c15(c)).collect());
            }
            prev_sub = sub;
        }
    }
    println!("{} Bilder beim Scrollen", shots.len());
    let cols = 2;
    let rows = (shots.len() + cols - 1) / cols;
    let mut all = vec![Color32::BLACK; w * cols * 224 * rows.max(1)];
    for (k, sh) in shots.iter().enumerate() {
        let (cx, cy) = (k % cols, k / cols);
        for y in 0..224 {
            for x in 0..w {
                all[(cy * 224 + y) * w * cols + cx * w + x] = sh[y * w + x];
            }
        }
    }
    bmp(&format!("{}/scroll_margin.bmp", out_dir()), w * cols, 224 * rows.max(1), &all);
}

/// Weltrand und Raumrand im Breitbild (Spiegeln + Abdunkeln)
#[test]
#[ignore]
fn dev_edge_shot() {
    let Some((mut s, rom)) = boot() else { return };
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let step = |s: &mut Snes, joy: u16| {
        let m = ram8(s, 0x7E0010);
        let indoors = ram8(s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        s.bus.ppu.ow = crate::app::ow_map(s, &tab);
        s.bus.joy[0] = joy;
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    };
    let ents = crate::world::find_entrances(&rom);
    let mut shots = Vec::new();
    // Ostrand der Welt (Ostpalast-Gebiet), dann ein Dungeon-Raum
    if let Some(e) = ents.iter().find(|e| e.area == 0x1E) {
        let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
        for _ in 0..90 { step(&mut s, 0); }
        for _ in 0..500 { step(&mut s, 0x0100); }
        shots.push(s.bus.ppu.wide[..w * 224].to_vec());
    }
    let _ = crate::teleport::to_entrance(&mut s, 0x08, false);
    for _ in 0..90 { step(&mut s, 0); }
    shots.push(s.bus.ppu.wide[..w * 224].to_vec());
    let px: Vec<Color32> = shots.iter().flatten().map(|&c| c15(c)).collect();
    bmp(&format!("{}/edge_shot.bmp", out_dir()), w, 224 * shots.len(), &px);
}

/// Breitbildrand im Spielstand (Slot 1, nur gelesen): laufen und Rand ansehen
#[test]
#[ignore]
fn dev_state_margin() {
    let Some((mut s, rom)) = boot() else { return };
    if std::env::var("IMMEDIATE").is_ok() {
        crate::snes::ppu::IMMEDIATE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if let Ok(v) = std::env::var("WARPTEST") {
        crate::snes::ppu::WARPTEST.store(v.parse().unwrap(), std::sync::atomic::Ordering::Relaxed);
    }
    if std::env::var("NOWARP").is_ok() {
        crate::snes::ppu::NOWARP.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    if std::env::var("RECAP").is_ok() {
        let (tx, rx) = std::sync::mpsc::channel();
        crate::teleport::capture_areas(&s, &rom, &[], tx);
        while let Ok((id, a)) = rx.try_recv() {
            crate::snes::ppu::set_area(id, a);
        }
    } else {
        println!("Gebiete geladen: {}", crate::app::load_areas(&path));
        crate::app::load_fog(&path);
        if let Ok(v) = std::env::var("FOG") {
            // Test: Nebelfarbe für ein Gebiet vorgeben (Gebiet:Farbe hex)
            let mut it = v.split(':');
            let id = u8::from_str_radix(it.next().unwrap(), 16).unwrap();
            let c = u16::from_str_radix(it.next().unwrap(), 16).unwrap();
            crate::snes::ppu::set_fog(id, c);
        }
    }
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    if let Ok(v) = std::env::var("ROWS") {
        // je 8 Zeilen: wie viele Pixel an einer senkrechten Grenze gleich sind (Welt 0)
        let bx: i32 = v.parse().unwrap();
        let (y0, y1): (i32, i32) = (std::env::var("Y0").unwrap().parse().unwrap(), std::env::var("Y1").unwrap().parse().unwrap());
        let mut y = y0;
        while y < y1 {
            let (mut eq, mut n) = (0, 0);
            for yy in y..y + 8 {
                let px = |x: i32| crate::snes::ppu::area_px_test(&tab, 0, x, yy);
                if let (Some(a), Some(b)) = (px(bx - 1), px(bx)) {
                    n += 1;
                    if a == b || px(bx - 2) == Some(b) || px(bx + 1) == Some(a) { eq += 1; }
                }
            }
            println!("y {y}: {eq}/{n}");
            y += 8;
        }
        return;
    }
    if std::env::var("WARPS").is_ok() {
        for world in 0..2u8 {
            for sy in 0..8 {
                for bxi in 1..8 {
                    let t0 = std::time::Instant::now();
                    let r = crate::snes::ppu::warp_runs_test(&tab, world, bxi * 512, sy * 512);
                    let ms = t0.elapsed().as_secs_f32() * 1000.0;
                    if ms > 5.0 { println!("ZEIT W{world} x={} y={} {ms:.1} ms", bxi * 512, sy * 512); }
                    for (r0, r1, d, l, rr) in r {
                        println!("W{world} x={} y {r0}..{r1} (Länge {}) Versatz {d} passt {l}..{rr}", bxi * 512, r1 - r0 + 1);
                    }
                }
            }
        }
        return;
    }
    if std::env::var("SEAMS").is_ok() {
        let par = crate::teleport::area_parents();
        for world in 0..2u8 {
            for sy in 0..8 {
                for bxi in 1..8 {
                    let bx = bxi * 512;
                    let (l, r) = (world as usize * 0x40 + sy * 8 + bxi - 1, world as usize * 0x40 + sy * 8 + bxi);
                    if par.get(l) == par.get(r) { continue; }
                    let (mut eq, mut n, mut eq2) = (0, 0, 0);
                    for y in sy as i32 * 512..sy as i32 * 512 + 512 {
                        let px = |x: i32| crate::snes::ppu::area_px_test(&tab, world, x, y);
                        if let (Some(a), Some(b), Some(a2), Some(b2)) = (px(bx as i32 - 1), px(bx as i32), px(bx as i32 - 2), px(bx as i32 + 1)) {
                            n += 1;
                            if a == b { eq += 1; }
                            if a2 == a { eq2 += 1; }
                            let _ = b2;
                        }
                    }
                    // deutliche Brüche (8-Zeilen-Bänder mit weniger als der Hälfte gleich)
                    let (mut bad, mut run, mut maxrun) = (0, 0, 0);
                    for yb in (sy as i32 * 512..sy as i32 * 512 + 512).step_by(8) {
                        let (mut e, mut m) = (0, 0);
                        for y in yb..yb + 8 {
                            let px = |x: i32| crate::snes::ppu::area_px_test(&tab, world, x, y);
                            if let (Some(a), Some(b)) = (px(bx as i32 - 1), px(bx as i32)) {
                                m += 1;
                                if a == b || px(bx as i32 - 2) == Some(b) || px(bx as i32 + 1) == Some(a) { e += 1; }
                            }
                        }
                        if m >= 4 && e * 2 < m { bad += 1; run += 1; maxrun = maxrun.max(run); } else { run = 0; }
                    }
                    let ratio = if n > 0 { eq as f32 / n as f32 } else { 1.0 };
                    let before = n < 64 || ratio >= 0.75;
                    if n > 0 && before && bad > 0 { println!("NEU gespiegelt: W{world} x={bx} y={} {:02X}|{:02X}: gleich {ratio:.2}, Brüche {bad}, längster {maxrun}", sy * 512, par[l], par[r]); }
                }
            }
        }
        return;
    }
    if std::env::var("CROP").is_ok() {
        let num = |k: &str, d: i32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let (x0, y0, cw, ch) = (num("CX", 512), num("CY", 3072), num("CW", 1024), num("CH", 1024));
        let px: Vec<Color32> = (0..ch).flat_map(|y| (0..cw).map(move |x| (x, y))).map(|(x, y)| crate::snes::ppu::area_px_test(&tab, std::env::var("WORLD").ok().and_then(|v| v.parse().ok()).unwrap_or(0), x0 + x, y0 + y).map_or(Color32::from_rgb(255, 0, 255), c15)).collect();
        bmp(&format!("{}/area_crop.bmp", out_dir()), cw as usize, ch as usize, &px);
        for id in [0x30u8, 0x32, 0x3A] { println!("{id:02X}: parent {:02X?}", crate::teleport::area_parents().get(id as usize)); }
        // Wahrheit: im Gebiet selbst stehen und den echten Bildschirm aufnehmen
        let ents = crate::world::find_entrances(&rom);
        let mut shots = Vec::new();
        let list: Vec<u8> = std::env::var("AREAS").map(|v| v.split(',').map(|x| u8::from_str_radix(x, 16).unwrap()).collect()).unwrap_or(vec![0x32, 0x3A]);
        for area in list {
            for e in ents.iter().filter(|e| e.area == area) {
                let _ = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
                for _ in 0..90 { s.bus.joy[0] = 0; s.run_frame(); s.bus.apu.dsp.out.clear(); }
                let (cx, cy) = (ram16(&s, 0x7E00E2) as i32, ram16(&s, 0x7E00E8) as i32);
                println!("{area:02X} eid {} Kamera {cx},{cy}", e.eid);
                let fb = s.framebuffer();
                let live: Vec<Color32> = fb.iter().map(|&c| c15(c)).collect();
                let cache: Vec<Color32> = (0..224).flat_map(|y| (0..256).map(move |x| (x, y))).map(|(x, y)| crate::snes::ppu::area_px_test(&tab, 0, cx + x, cy + y).map_or(Color32::from_rgb(255, 0, 255), c15)).collect();
                for y in 0..224 { shots.extend_from_slice(&live[y * 256..y * 256 + 256]); shots.extend_from_slice(&cache[y * 256..y * 256 + 256]); }
            }
        }
        bmp(&format!("{}/area_truth.bmp", out_dir()), 512, shots.len() / 512, &shots);
        return;
    }
    if std::env::var("NOSTATE").is_err() {
        let st = std::fs::read(path.with_extension(format!("ss{}", std::env::var("STATE").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(1)))).unwrap();
        s.load_state(&st).unwrap();
    }
    let ext: usize = std::env::var("EXT").ok().and_then(|v| v.parse().ok()).unwrap_or(96);
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let last_ow: std::cell::RefCell<Option<Arc<crate::snes::ppu::OwMap>>> = std::cell::RefCell::new(None);
    let step = |s: &mut Snes, joy: u16| {
        let m = ram8(s, 0x7E0010);
        let indoors = ram8(s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        let fresh = crate::app::ow_map(s, &tab);
        let st = crate::app::stable_ow(fresh, &last_ow.borrow());
        if let Some(o) = &st {
            if o.valid && !o.room {
                *last_ow.borrow_mut() = Some(o.clone());
            }
        } else {
            *last_ow.borrow_mut() = None;
        }
        s.bus.ppu.ow = st;
        if std::env::var("PALDBG").is_ok() {
            let o = s.bus.ppu.ow.clone();
            let c = &s.bus.ppu.cgram;
            println!("  pal {:?} welt {:?} gültig {:?} cg[33..36] {:04X} {:04X} {:04X} {:04X}", crate::snes::ppu::pal_fx(c, o.as_deref()).map(|t| t[0]), o.as_ref().map(|o| o.world), o.as_ref().map(|o| o.valid), c[33], c[34], c[50], c[70]);
        }
        s.bus.ppu.ow_edge = if m == 0x09 && ram8(s, 0x7E0011) != 0 { 2 } else if std::env::var("RECOMP").is_ok() { 1 } else { 0 };
        s.bus.joy[0] = joy;
        if std::env::var("GHOST").is_ok() {
            s.bus.wram[0x037B] = 1;
            s.bus.wram[0x037F] = 1;
        }
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        let a = s.bus.ppu.fog_acc.iter().map(|v| v.swap(0, std::sync::atomic::Ordering::Relaxed)).collect::<Vec<u32>>();
        if a[3] > 2000 && ram8(s, 0x7E0011) == 0 {
            let n = a[3];
            let c = ((a[0] / n) | (a[1] / n) << 5 | (a[2] / n) << 10) as u16;
            let area = ram8(s, 0x7E008A) as usize;
            let p = crate::teleport::area_parents().get(area).copied().unwrap_or(area as u8);
            if crate::snes::ppu::set_fog(p, c) { println!("  Nebel Gebiet {p:02X}: {c:04X}"); }
        }
    };
    if std::env::var("MIRROR").is_ok() {
        // Spiegel (auch Licht -> Schatten, wie in der App) ausrüsten; mit Y (4000) benutzen
        let mut t = rom.clone();
        crate::alttp::apply_tweaks(&mut t, true, true, true);
        crate::alttp::apply_tweaks2(&mut t, true, true);
        s.bus.rom = Arc::new(t);
        s.bus.wram[0xF353] = 2;
        s.bus.wram[0x0202] = 20;
        s.bus.wram[0x0303] = 20;
        if std::env::var("NOPEARL").is_err() {
            s.bus.wram[0xF357] = 1;
        }
    }
    if let Ok(v) = std::env::var("TP") {
        // Teleport vor einen Eingang (z. B. Schloss), optional Spielfortschritt setzen
        if let Ok(p) = std::env::var("PROG") {
            s.bus.wram[0xF3C5] = p.parse().unwrap();
        }
        let ents = crate::world::find_entrances(&rom);
        let eid = u8::from_str_radix(&v, 16).unwrap();
        let e = *ents.iter().find(|e| e.eid == eid).unwrap();
        crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true).unwrap();
    }
    let dirs: Vec<(u16, usize)> = std::env::var("WALK").unwrap_or("0:30".into()).split(',').map(|p| { let mut i = p.split(':'); (u16::from_str_radix(i.next().unwrap(), 16).unwrap(), i.next().unwrap().parse().unwrap()) }).collect();
    let mut shots = Vec::new();
    for (joy, n) in dirs {
        for _ in 0..n { step(&mut s, joy); }
        { let r = &s.bus.ppu.r; println!("  TM={:02X} TS={:02X} cgwsel={:02X} cgadsub={:02X} bgmode={:02X} hofs={:?}", r.tm, r.ts, r.cgwsel, r.cgadsub, r.bgmode, r.hofs); }
        println!("Grenzen 600={} 602={} 604={} 606={} 712={:04X} 40A={:02X} 8A={:02X}", ram16(&s, 0x7E0600), ram16(&s, 0x7E0602), ram16(&s, 0x7E0604), ram16(&s, 0x7E0606), ram16(&s, 0x7E0712), ram8(&s, 0x7E040A), ram8(&s, 0x7E008A));
        println!("Zustand 5D={:02X} 2E0={:02X} 301={:02X} 37A={:02X} 300={:02X}", ram8(&s, 0x7E005D), ram8(&s, 0x7E02E0), ram8(&s, 0x7E0301), ram8(&s, 0x7E037A), ram8(&s, 0x7E0300));
        println!("Modul {:02X}/{:02X} mosaic={:02X} bright={} Gebiet {:02X} Link x={} y={} ow={:?}", ram8(&s, 0x7E0010), ram8(&s, 0x7E0011), s.bus.ppu.r.mosaic, s.bus.ppu.r.inidisp & 15, ram8(&s, 0x7E008A), ram16(&s, 0x7E0022), ram16(&s, 0x7E0020), s.bus.ppu.ow.as_ref().map(|o| (o.valid, o.x0, o.y0, o.size, o.camx, o.camy)));
        shots.push(s.bus.ppu.wide[..w * 224].to_vec());
    }
    let px: Vec<Color32> = shots.iter().flatten().map(|&c| c15(c)).collect();
    bmp(&format!("{}/state_margin.bmp", out_dir()), w, 224 * shots.len(), &px);
}

/// Thronsaal (Hyrule-Schloss): Breitbildrand ansehen – vom Eingang nach Norden laufen
#[test]
#[ignore]
fn dev_throne() {
    let Some((mut s, rom)) = boot() else { return };
    if std::env::var("IMMEDIATE").is_ok() {
        crate::snes::ppu::IMMEDIATE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    println!("Modul {:02X}", ram8(&s, 0x7E0010));
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let mut rv: std::collections::HashMap<u16, Arc<Vec<u16>>> = std::collections::HashMap::new();
    let rt = crate::world::entrance_room_table(&rom);
    let dmaps = crate::world::find_dungeon_maps(&rom);
    let mut step = |s: &mut Snes, joy: u16| {
        let m = ram8(s, 0x7E0010);
        let indoors = ram8(s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        let full = std::env::var("NOFULL").is_err() && indoors && crate::app::in_dungeon(&dmaps, crate::app::shown_room(s));
        s.bus.ppu.ow = crate::app::ow_map_full(s, &tab, full);
        if m == 0x07 && ram8(s, 0x7E0011) == 0 && indoors {
            let v = Arc::new(s.bus.ppu.vram.clone());
            rv.insert(ram16(s, 0x7E00A0), v.clone());
            s.bus.ppu.room_keep = Some(v);
            s.bus.ppu.room_next = None;
            let room = ram16(s, 0x7E00A0);
            // Nachbarräume derselben Etage unsichtbar laden (wie in der App, hier sofort)
            if std::env::var("CAPTURE").is_ok() {
                for n in crate::app::near_rooms(&dmaps, room) {
                    if !rv.contains_key(&n) {
                        if let Some(v) = rt.and_then(|t| crate::teleport::capture_room(s, t, ram8(s, 0x7E010E), n)) {
                            println!("  Raum {n:02X} unsichtbar geladen");
                            rv.insert(n, Arc::new(v));
                        }
                    }
                }
            }
        } else if m == 0x07 && indoors && matches!(ram8(s, 0x7E0011), 1 | 2) {
            s.bus.ppu.room_next = rv.get(&ram16(s, 0x7E00A0)).cloned();
        }
        if m == 0x07 && indoors {
            s.bus.ppu.room_side = crate::app::near_rooms(&dmaps, crate::app::shown_room(s)).into_iter().filter_map(|n| rv.get(&n).map(|v| (n, v.clone()))).collect();
        }
        s.bus.joy[0] = joy;
        if std::env::var("LIGHT").is_ok() {
            let _ = crate::alttp::always_light(s);
        }
        if std::env::var("GHOST").is_ok() {
            s.bus.wram[0x037B] = 1;
            s.bus.wram[0x037F] = 1;
        }
        s.run_frame();
        s.bus.apu.dsp.out.clear();
    };
    let ents = crate::world::find_entrances(&rom);
    let show_area = std::env::var("AREA").ok().and_then(|v| u8::from_str_radix(&v, 16).ok()).unwrap_or(0x1B);
    for e in ents.iter().filter(|e| e.area == show_area) {
        println!("Eingang eid {:02X} bei {},{}", e.eid, e.x, e.y);
    }
    if std::env::var("STATE").is_ok() {
        // Spielstand nur lesen (wird nicht verändert)
        let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
        let st = std::fs::read(path.with_extension(format!("ss{}", std::env::var("STATE").unwrap().parse::<u8>().unwrap_or(1)))).unwrap();
        s.load_state(&st).unwrap();
    } else {
    let eid = std::env::var("EID").ok().and_then(|v| u8::from_str_radix(&v, 16).ok());
    let Some(eid) = eid else { return };
    let e = *ents.iter().find(|e| e.eid == eid).unwrap();
    // Spielfortschritt: nach Agahnim (Schloss hell, keine Wachen-Szene)
    s.bus.wram[0xF3C5] = std::env::var("PROG").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, false).unwrap();
    }
    let mut shots = Vec::new();
    let walk: Vec<(u16, usize)> = std::env::var("WALK").unwrap_or("0800:120".into()).split(',').map(|p| { let mut i = p.split(':'); (u16::from_str_radix(i.next().unwrap(), 16).unwrap(), i.next().unwrap().parse().unwrap()) }).collect();
    for (joy, n) in walk {
        for _ in 0..n { step(&mut s, joy); }
        let o = s.bus.ppu.ow.clone();
        { let r = &s.bus.ppu.r; println!("  TM={:02X} TS={:02X} TMW={:02X} TSW={:02X} w12={:02X} w34={:02X} wobj={:02X} cgwsel={:02X} cgadsub={:02X} bgmode={:02X} 1B={} 0458={:02X} lampe={:02X}", r.tm, r.ts, r.tmw, r.tsw, r.w12sel, r.w34sel, r.wobjsel, r.cgwsel, r.cgadsub, r.bgmode, ram8(&s, 0x7E001B), ram8(&s, 0x7E0458), ram8(&s, 0x7EF34A)); }
        println!("Raum {:04X} sub={:02X} hell={:02X} Kamera {},{} A6={} A7={} Grenzen {:?}", ram16(&s, 0x7E00A0), ram8(&s, 0x7E0011), s.bus.ppu.r.inidisp, ram16(&s, 0x7E00E2), ram16(&s, 0x7E00E8), ram8(&s, 0x7E00A6), ram8(&s, 0x7E00A7), o.as_ref().map(|o| (o.bx0, o.bx1, o.by0, o.by1)));
        shots.push(s.bus.ppu.wide[..w * 224].to_vec());
    }
    let px: Vec<Color32> = shots.iter().flatten().map(|&c| c15(c)).collect();
    bmp(&format!("{}/throne.bmp", out_dir()), w, 224 * shots.len(), &px);
}

/// Vorspann-Demo (Modul 0x14): Szenen mit Räumen aufnehmen, Breitbildrand ansehen
#[test]
#[ignore]
fn dev_attract() {
    let Ok(p) = std::env::var("PASTLINK_ROM") else { return };
    let (rom, info) = crate::alttp::prepare_rom(std::fs::read(&p).unwrap()).unwrap();
    let mut s = Snes::new(Arc::new(rom), info.sram_size, false);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let mode = std::env::var("MODE").unwrap_or_default();
    let tab: Arc<Vec<u16>> = Arc::new(s.bus.rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let mut shots = Vec::new();
    let mut last_room = 0xFFFF;
    let from: u32 = std::env::var("FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    for f in 0..60 * 200u32 {
        let m = ram8(&s, 0x7E0010);
        let indoors = ram8(&s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = false;
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        if mode == "fix" && m == 0x14 {
            s.bus.ppu.margin = crate::app::attract_margin(&s);
            s.bus.ppu.ow = crate::app::ow_map(&s, &tab);
        }
        s.bus.joy[0] = 0;
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        let room = ram16(&s, 0x7E00A0);
        if m == 0x14 && f >= from && f % 30 == 0 && s.bus.ppu.brightness() >= 15 && shots.len() < 12 && (room != last_room || f % 300 == 0) {
            last_room = room;
            println!("Bild {f}: Modul {m:02X} Untermodul {:02X} drinnen {indoors} Raum {room:04X} BG-Modus {}", ram8(&s, 0x7E0011), s.bus.ppu.r.bgmode & 7);
            shots.push(s.bus.ppu.wide[..w * 224].to_vec());
        }
    }
    let px: Vec<Color32> = shots.iter().flatten().map(|&c| c15(c)).collect();
    bmp(&format!("{}/attract.bmp", out_dir()), w, 224 * shots.len().max(1), &px);
}

/// Grundriss Hyrule-Schloss/Kanalisation ausgeben
#[test]
#[ignore]
fn dev_hc_floors() {
    let Some((_s, rom)) = boot() else { return };
    let dm = crate::world::find_dungeon_maps(&rom);
    for e in crate::world::find_entrances(&rom).iter().filter(|e| matches!(e.room, 0x70..=0x72 | 0x80..=0x82 | 0x50..=0x52 | 0x60..=0x62 | 0x01 | 0x11 | 0x21 | 0x22 | 0x32)) {
        println!("Eingang {:02X} -> Raum {:02X} (Gebiet {:02X})", e.eid, e.room, e.area);
    }
    for d in 0..2 {
        println!("{}", crate::world::DMAP_NAMES[d].0);
        for (k, f) in dm[d].floors.iter().enumerate() {
            println!(" {}: {:02X?}", dm[d].floor_name(k), f);
        }
    }
}

/// Leistung: Bildzeit wie in der App (Breitbild, Ränder, Zwischenbilder) – Slot nur lesen
#[test]
#[ignore]
fn dev_bench() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    crate::app::load_areas(&path);
    let n: u8 = std::env::var("STATE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    s.load_state(&std::fs::read(path.with_extension(format!("ss{n}"))).unwrap()).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let ext: usize = std::env::var("EXT").ok().and_then(|v| v.parse().ok()).unwrap_or(96);
    s.bus.ppu.ext = ext;
    let walk: Vec<(u16, usize)> = std::env::var("WALK").unwrap_or("0100:120,0200:120,0400:120,0800:120".into()).split(',').map(|p| { let mut i = p.split(':'); (u16::from_str_radix(i.next().unwrap(), 16).unwrap(), i.next().unwrap().parse().unwrap()) }).collect();
    let (mut t_ow, mut t_run, mut t_snap, mut t_interp) = (0f64, 0f64, 0f64, 0f64);
    let mut frames = 0;
    let mut out = Vec::new();
    let mut prev: Option<crate::snes::ppu::Snap> = None;
    let mut worst = 0f64;
    for (joy, k) in walk {
        for _ in 0..k {
            let t0 = std::time::Instant::now();
            let m = ram8(&s, 0x7E0010);
            let indoors = ram8(&s, 0x7E001B) != 0;
            s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
            s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
            s.bus.ppu.ow = crate::app::ow_map(&s, &tab);
            let t1 = std::time::Instant::now();
            s.bus.joy[0] = joy;
            s.bus.wram[0x037B] = 1;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            let t2 = std::time::Instant::now();
            let sn = s.bus.ppu.snapshot();
            let t3 = std::time::Instant::now();
            if let Some(p) = &prev {
                crate::snes::ppu::render_interp(p, &sn, 0.5, ext, 0x1F, &mut out);
            }
            let t4 = std::time::Instant::now();
            prev = Some(sn);
            let f = |a: std::time::Instant, b: std::time::Instant| (b - a).as_secs_f64() * 1000.0;
            if frames < 5 || f(t0, t4) > 6.0 {
                println!("Bild {frames}: ow {:.1} run {:.1} snap {:.1} interp {:.1} ms", f(t0, t1), f(t1, t2), f(t2, t3), f(t3, t4));
            }
            t_ow += f(t0, t1);
            t_run += f(t1, t2);
            t_snap += f(t2, t3);
            t_interp += f(t3, t4);
            worst = worst.max(f(t0, t4));
            frames += 1;
        }
    }
    let t0 = std::time::Instant::now();
    let mut sz = 0;
    for _ in 0..50 { sz = s.save_state().len(); }
    println!("save_state {:.3} ms ({sz} Bytes)", t0.elapsed().as_secs_f64() * 1000.0 / 50.0);
    let t0 = std::time::Instant::now();
    for _ in 0..50 { let _ = s.clone(); }
    println!("clone {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0 / 50.0);
    let nf = frames as f64;
    println!("Bilder {frames}  ow_map {:.3} ms  run_frame {:.3} ms  snapshot {:.3} ms  interp {:.3} ms  (schlechtestes {:.2} ms)", t_ow / nf, t_run / nf, t_snap / nf, t_interp / nf, worst);
}

/// Spiel-Lag: Bilder, in denen die Hauptschleife des Spiels nicht fertig wurde ($1A zählt nicht weiter)
#[test]
#[ignore]
fn dev_lag() {
    let Some((s0, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    let n: u8 = std::env::var("STATE").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let st = std::fs::read(path.with_extension(format!("ss{n}"))).unwrap();
    for variant in ["original", "tweaks", "tweaks+wide"] {
        let mut s = s0.clone();
        let mut t = rom.clone();
        if variant != "original" {
            crate::alttp::apply_tweaks(&mut t, true, true, true);
            crate::alttp::apply_tweaks2(&mut t, true, true);
        }
        if variant == "tweaks+wide" {
            crate::alttp::apply_wide_sprites(&mut t);
        }
        s.load_state(&st).unwrap();
        s.bus.rom = Arc::new(t);
        let walk: Vec<(u16, usize)> = std::env::var("WALK").unwrap_or("0100:200,0200:200,0400:100,0800:100".into()).split(',').map(|p| { let mut i = p.split(':'); (u16::from_str_radix(i.next().unwrap(), 16).unwrap(), i.next().unwrap().parse().unwrap()) }).collect();
        let (mut lag, mut tot) = (0, 0);
        let mut last = ram8(&s, 0x7E001A);
        let x0 = ram16(&s, 0x7E0022);
        for (joy, k) in walk {
            for _ in 0..k {
                s.bus.joy[0] = joy;
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                let now = ram8(&s, 0x7E001A);
                if now == last {
                    lag += 1;
                    if variant == "original" { println!("  Lag bei Bild {tot}: Modus {:02X}/{:02X} Gebiet {:02X} Sprites aktiv {}", ram8(&s, 0x7E0010), ram8(&s, 0x7E0011), ram8(&s, 0x7E008A), (0..16).filter(|&i| ram8(&s, 0x7E0DD0 + i) != 0).count()); }
                }
                last = now;
                tot += 1;
            }
        }
        println!("{variant}: {lag} Lag-Bilder von {tot}, Link x {} -> {}", x0, ram16(&s, 0x7E0022));
    }
}

/// Karte: welche Truhen/Herzteile fallen aus der Ortsliste (kein Dungeon und kein Höhleneingang)?
#[test]
#[ignore]
fn dev_locs() {
    let Some((mut s, rom)) = boot() else { return };
    if let Ok(n) = std::env::var("STATE") {
        let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
        s.load_state(&std::fs::read(path.with_extension(format!("ss{n}"))).unwrap()).unwrap();
    }
    let maps = crate::world::find_dungeon_maps(&rom);
    let t = crate::randomizer::find_chest_table(&rom);
    let st = crate::tracker::find_static_items(&rom);
    let ents = crate::world::find_entrances(&rom);
    let (locs, _) = crate::world::build_locations(&rom, t.as_ref(), &st, &ents, &maps);
    let count = |k: crate::world::LocKind| locs.iter().filter(|l| l.kind == k).map(|l| l.total()).sum::<usize>();
    println!("Herzteile {} Container {} Minispiele {} Truhen {}", count(crate::world::LocKind::HeartPiece), count(crate::world::LocKind::HeartContainer), count(crate::world::LocKind::Minigame), count(crate::world::LocKind::Chests));
    println!("Oberwelt-Herzteile (statisch) {}", st.len());
    for l in locs.iter().filter(|l| matches!(l.kind, crate::world::LocKind::HeartPiece | crate::world::LocKind::Minigame)) {
        println!("  HP {} welt {} {:.0},{:.0}", l.name_de, l.world, l.x, l.y);
    }
    let npcs = crate::world::find_npcs(&rom, &ents);
    println!("Figuren {} (davon in Räumen mit Ganon D6: {})", npcs.len(), npcs.iter().filter(|n| n.0 == 0xD6).count());
    for e in ents.iter().filter(|e| (0x50..0x60).contains(&e.area)) {
        println!("  EING eid {:02X} Gebiet {:02X} Raum {:03X} bei {:.0},{:.0}", e.eid, e.area, e.room, e.x, e.y);
    }
    let mut hc = 0;
    for l in locs.iter().filter(|l| l.kind != crate::world::LocKind::Chests) {
        let d = l.done(&s, false);
        if l.kind == crate::world::LocKind::HeartContainer { hc += d; }
        println!("  ORT W{} {:.0},{:.0} {} erledigt {}", l.world, l.x, l.y, l.name_de, d);
    }
    let maxhp = ram8(&s, 0x7EF36C) as i32;
    let pieces = (maxhp / 8 - 3 - hc as i32) * 4 + ram8(&s, 0x7EF36B) as i32;
    let room = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    println!("Max-Herzen {} Container {} -> Herzteile eingesammelt {pieces}", maxhp / 8, hc);
    for r in [0xE2u16, 0xE3, 0xEB, 0x11C, 0x127, 0x128] { println!("  Raum {r:03X} Bits {:04X}", room(r)); }
    let ow: Vec<String> = (0..0xC0usize).filter(|&a| s.bus.wram[0xF280 + a] & 0x40 != 0).map(|a| format!("{a:02X}")).collect();
    println!("Gebiete mit Item-Flag (40): {}", ow.join(" "));
    let rb: Vec<String> = (0..0x140u16).filter(|&r| room(r) & 0x0400 != 0).map(|r| format!("{r:03X}")).collect();
    println!("Räume mit Bit 0400: {}", rb.join(" "));
    let rc: Vec<String> = (0..0x140u16).filter(|&r| room(r) & 0x0800 != 0).map(|r| format!("{r:03X}")).collect();
    println!("Räume mit Bit 0800: {}", rc.join(" "));
    if let Some(t) = &t {
        println!("Truhen-Tabelle {}", t.chests.len());
        for c in &t.chests {
            if crate::world::room_dungeon(&maps, c.room).is_none() && crate::world::entrance_for_room_pub(&ents, c.room).is_none() {
                println!("  fehlt: Raum {:03X} Truhe {} Item {:02X}", c.room, c.index_in_room, c.item);
            }
        }
    }
    for r in crate::world::room_heart_pieces_pub(&rom) {
        let e = crate::world::entrance_for_room_pub(&ents, r);
        println!("  HERZRAUM {r:03X} Eingang {:?}", e.map(|i| (ents[i].eid, ents[i].room, crate::world::area_name(ents[i].area, false))));
    }
    if let Some(t) = &t {
        for c in t.chests.iter().filter(|c| c.item == 0x17) {
            let e = crate::world::entrance_for_room_pub(&ents, c.room);
            println!("  HERZTRUHE {:03X} Eingang {:?}", c.room, e.map(|i| (ents[i].eid, ents[i].room, crate::world::area_name(ents[i].area, false))));
        }
    }
    for r in crate::world::room_heart_pieces_pub(&rom) {
        if crate::world::entrance_for_room_pub(&ents, r).is_none() {
            println!("  Herzteil-Raum ohne Eingang: {r:03X}");
        }
    }
}

/// Alle Dungeon-Räume unsichtbar laden: Kontaktbogen je Etage (5×5, 128 px je Raum) und Raum-Auslöser
#[test]
#[ignore]
fn dev_rooms() {
    let Some((s, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    let maps = crate::world::find_dungeon_maps(&rom);
    let rt = crate::world::entrance_room_table(&rom).unwrap();
    let only: Option<usize> = std::env::var("DUNGEON").ok().and_then(|v| v.parse().ok());
    for (d, dm) in maps.iter().enumerate() {
        if only.map_or(false, |o| o != d) {
            continue;
        }
        let eid = ents.iter().find(|e| dm.contains(e.room)).map(|e| e.eid).unwrap_or(ents[0].eid);
        for (fl, f) in dm.floors.iter().enumerate() {
            let mut px = vec![Color32::from_rgb(20, 20, 30); 640 * 640];
            for (i, &rm) in f.iter().enumerate() {
                if rm == 0x0F {
                    continue;
                }
                let Some(shot) = crate::teleport::capture_room_shot(&s, rt, eid, rm as u16) else {
                    println!("D{d} {} Raum {rm:02X}: nicht geladen", dm.floor_name(fl));
                    continue;
                };
                println!("D{d} {} Raum {rm:02X} pos {}", dm.floor_name(fl), i);
                let (ox, oy) = ((i % 5) * 128, (i / 5) * 128);
                for y in 0..128 {
                    for x in 0..128 {
                        let c = shot.img[(y * 2) * 256 + x * 2];
                        let edge = x == 0 || y == 0;
                        px[(oy + y) * 640 + ox + x] = if edge { Color32::from_rgb(255, 0, 255) } else { c15(c) };
                    }
                }
                if std::env::var("FULL").is_ok() {
                    let p: Vec<Color32> = shot.img.iter().map(|&c| c15(c)).collect();
                    bmp(&format!("{}/room_{rm:02X}.bmp", out_dir()), 256, 256, &p);
                }
            }
            bmp(&format!("{}/rooms_d{d:02}_{}.bmp", out_dir(), dm.floor_name(fl)), 640, 640, &px);
        }
    }
}

/// Breitbild-Rundgang (Slot nur gelesen): vor jeden Eingang je Gebiet bzw. in jeden Dungeon-Eingang teleportieren
/// und das Breitbild aufnehmen – Kontaktbögen zum Durchsehen
#[test]
#[ignore]
fn dev_wide_tour() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    println!("Gebiete geladen: {}", crate::app::load_areas(&path));
    crate::app::load_fog(&path);
    let st = std::fs::read(path.with_extension(format!("ss{}", std::env::var("STATE").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(1)))).unwrap();
    s.load_state(&st).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let dmaps = crate::world::find_dungeon_maps(&rom);
    let rt = crate::world::entrance_room_table(&rom);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let inside = std::env::var("INSIDE").is_ok();
    let ents = crate::world::find_entrances(&rom);
    let parents = crate::teleport::area_parents();
    let mut seen = std::collections::HashSet::new();
    let list: Vec<crate::world::Entrance> = ents
        .iter()
        .filter(|e| if inside { crate::world::room_dungeon(&dmaps, e.room).is_some() } else { seen.insert(parents[e.area as usize & 0x7F]) })
        .copied()
        .collect();
    let base = s.clone();
    let mut shots: Vec<Vec<u16>> = Vec::new();
    let mut names = Vec::new();
    for e in &list {
        let mut c = base.clone();
        let mut last_ow: Option<Arc<crate::snes::ppu::OwMap>> = None;
        let mut rv: std::collections::HashMap<u16, Arc<Vec<u16>>> = Default::default();
        if crate::teleport::to_target(&mut c, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, !inside).is_err() {
            println!("eid {:02X}: Teleport fehlgeschlagen", e.eid);
            continue;
        }
        for f in 0..40 {
            let m = ram8(&c, 0x7E0010);
            let indoors = ram8(&c, 0x7E001B) != 0;
            c.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
            c.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
            let full = indoors && crate::app::in_dungeon(&dmaps, crate::app::shown_room(&c));
            let fresh = crate::app::ow_map_full(&c, &tab, full);
            let stv = crate::app::stable_ow(fresh, &last_ow);
            if let Some(o) = &stv {
                if o.valid && !o.room {
                    last_ow = Some(o.clone());
                }
            }
            c.bus.ppu.ow = stv;
            if m == 0x07 && ram8(&c, 0x7E0011) == 0 && indoors {
                let v = Arc::new(c.bus.ppu.vram.clone());
                let room = ram16(&c, 0x7E00A0);
                rv.insert(room, v.clone());
                c.bus.ppu.room_keep = Some(v);
                c.bus.ppu.room_next = None;
                if f == 0 {
                    for n in crate::app::near_rooms(&dmaps, room) {
                        if let Some(v) = rt.and_then(|t| crate::teleport::capture_room(&c, t, ram8(&c, 0x7E010E), n)) {
                            rv.insert(n, Arc::new(v));
                        }
                    }
                }
                c.bus.ppu.room_side = crate::app::near_rooms(&dmaps, crate::app::shown_room(&c)).into_iter().filter_map(|n| rv.get(&n).map(|v| (n, v.clone()))).collect();
            }
            c.bus.ppu.ow_edge = 0;
            c.bus.joy[0] = 0;
            c.run_frame();
            c.bus.apu.dsp.out.clear();
        }
        println!("  eid {:02X} Gebiet {:02X} Link {},{} Kamera {},{} Raum {:03X}", e.eid, ram8(&c, 0x7E008A), ram16(&c, 0x7E0022), ram16(&c, 0x7E0020), ram16(&c, 0x7E00E2), ram16(&c, 0x7E00E8), ram16(&c, 0x7E00A0));
        names.push(format!("{:02X}:{}", e.eid, crate::world::area_name(e.area, false)));
        shots.push(c.bus.ppu.wide[..w * 224].to_vec());
    }
    // Bögen mit je 12 Bildern (2 Spalten)
    for (k, chunk) in shots.chunks(12).enumerate() {
        let rows = (chunk.len() + 1) / 2;
        let mut px = vec![Color32::from_rgb(255, 0, 255); w * 2 * 224 * rows];
        for (i, img) in chunk.iter().enumerate() {
            let (ox, oy) = ((i % 2) * w, (i / 2) * 224);
            for y in 0..224 {
                for x in 0..w {
                    if y < 223 && x + 1 < w {
                        px[(oy + y) * w * 2 + ox + x] = c15(img[y * w + x]);
                    }
                }
            }
        }
        bmp(&format!("{}/tour_{}{k}.bmp", out_dir(), if inside { "in" } else { "out" }), w * 2, 224 * rows, &px);
        println!("Bogen {k}: {}", names[k * 12..(k * 12 + chunk.len())].join(" | "));
    }
}

/// Link-Position: wo sitzt Link im Bild relativ zu ($22,$20)? Markierungen in ein Bild (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_linkpos() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let mut shots = vec![("innen", s.clone())];
    for eid in [ents[3].eid, ents[20].eid] {
        let mut c = s.clone();
        c.bus.ppu.ext = 0;
        if crate::teleport::to_entrance(&mut c, eid, true).is_ok() {
            for _ in 0..30 {
                c.bus.joy[0] = 0;
                c.run_frame();
            }
            shots.push(("aussen", c));
        }
    }
    for (k, (n, c)) in shots.iter().enumerate() {
        let r = |a: u32| ram16(c, a) as i32;
        let (lx, ly, cx, cy) = (r(0x7E0022), r(0x7E0020), r(0x7E00E2), r(0x7E00E8));
        let mut p: Vec<Color32> = c.framebuffer().iter().map(|&v| c15(v)).collect();
        let mut put = |x: i32, y: i32, col: Color32| {
            if (0..256).contains(&x) && (0..224).contains(&y) {
                p[(y * 256 + x) as usize] = col;
            }
        };
        // rot: (x+8, y+12), blau: (x+8, y+16), gelb: Rahmen (x..x+16, y..y+24)
        for d in -3..=3 {
            put(lx - cx + 8 + d, ly - cy + 12, Color32::RED);
            put(lx - cx + 8, ly - cy + 12 + d, Color32::RED);
            put(lx - cx + 8 + d, ly - cy + 16, Color32::BLUE);
        }
        for i in 0..16 {
            put(lx - cx + i, ly - cy, Color32::YELLOW);
            put(lx - cx + i, ly - cy + 24, Color32::YELLOW);
        }
        println!("{n} link {lx},{ly} cam {cx},{cy} modul {:02X}", c.bus.wram[0x10]);
        let w16 = |a: usize| c.bus.wram[a] as u16 | (c.bus.wram[a + 1] as u16) << 8;
        println!("   oberwelt-merker gebiet {:02X} x {} y {}", c.bus.wram[0xC140], w16(0xC14A), w16(0xC148));
        // 3fach vergrößert
        let mut big = vec![Color32::BLACK; 768 * 672];
        for y in 0..672 {
            for x in 0..768 {
                big[y * 768 + x] = p[(y / 3) * 256 + x / 3];
            }
        }
        bmp(&format!("{}/lp_{k}.bmp", out_dir()), 768, 672, &big);
    }
}

/// Karte/Kompass je Dungeon im Spielstand (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_mapbits() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    for d in 0..14usize {
        let bit = if d <= 1 { 0xC000u16 } else { 0x8000u16 >> d };
        println!("{d} {} karte {} kompass {}", crate::world::DMAP_NAMES[d].0, ram16(&s, 0x7EF368) & bit != 0, ram16(&s, 0x7EF364) & bit != 0);
    }
}

/// Raumbild des aktuellen Raums mit/ohne BG1 (dunkle Räume: Lichtkegel) – Slot 1 nur gelesen
#[test]
#[ignore]
fn dev_darkroom() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    println!("raum {:03X} 9A {:02X} 1B {:02X} 458 {:02X}", ram16(&s, 0x7E00A0), s.bus.wram[0x9A], s.bus.wram[0x1B], s.bus.wram[0x458]);
    for (n, skip) in [("mit", false), ("ohne", true)] {
        let img = s.bus.ppu.room_full_ex(2, skip);
        let p: Vec<Color32> = img.iter().map(|&v| c15(v)).collect();
        bmp(&format!("{}/dark_{n}.bmp", out_dir()), 256, 256, &p);
    }
}

/// Eingänge, die sich eine Eingangs-ID teilen (Welt, Gebiet, Position)
#[test]
#[ignore]
fn dev_dup_eids() {
    let Some((_s, rom)) = boot() else { return };
    let ents = crate::world::find_entrances(&rom);
    let mut by: std::collections::BTreeMap<u8, Vec<String>> = Default::default();
    for e in &ents {
        by.entry(e.eid).or_default().push(format!("w{} a{:02X} {:.0},{:.0} r{:03X}", e.world, e.area, e.x, e.y, e.room));
    }
    for (k, v) in by.iter().filter(|(_, v)| v.len() > 1) {
        println!("eid {k:02X}: {}", v.join(" | "));
    }
}

/// Breitbild-Rand flackert? Slot 1 laden (nur gelesen), 90 Bilder wie in der App, linken Rand je Bild vergleichen
#[test]
#[ignore]
fn dev_blink() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    println!("Gebiete geladen: {}", crate::app::load_areas(&path));
    crate::app::load_fog(&path);
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let dmaps = crate::world::find_dungeon_maps(&rom);
    let ext = 96usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let mut last_ow: Option<Arc<crate::snes::ppu::OwMap>> = None;
    let mut prev: Option<Vec<u16>> = None;
    let mut frames: Vec<Vec<u16>> = Vec::new();
    for f in 0..90 {
        let m = ram8(&s, 0x7E0010);
        let indoors = ram8(&s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = m >= 0x06 && m != 0x14;
        s.bus.ppu.margin = if m < 0x06 || m == 0x14 { 0 } else if indoors { 2 } else { 1 };
        let full = indoors && crate::app::in_dungeon(&dmaps, crate::app::shown_room(&s));
        let fresh = crate::app::ow_map_full(&s, &tab, full);
        let stv = crate::app::stable_ow(fresh, &last_ow);
        if let Some(o) = &stv {
            if o.valid && !o.room {
                last_ow = Some(o.clone());
            }
        }
        s.bus.ppu.ow = stv;
        s.bus.ppu.ow_edge = std::env::var("EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        s.bus.ppu.layer_mask = 0x1F & if crate::app::shadow_overlay(&s) { !1 } else { 0xFF };
        s.bus.joy[0] = 0;
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        let left: Vec<u16> = (0..224).flat_map(|y| s.bus.ppu.wide[y * w..y * w + ext].to_vec()).collect();
        if let Some(p) = &prev {
            let d = left.iter().zip(p.iter()).filter(|(a, b)| a != b).count();
            if d > 0 {
                println!("Bild {f}: {d} Pixel links geändert | ow valid {:?}", s.bus.ppu.ow.as_ref().map(|o| (o.valid, o.x0, o.y0, o.size, o.camx, o.camy)));
            }
        }
        prev = Some(left);
        if f >= 60 && f < 68 {
            frames.push(s.bus.ppu.wide[..w * 224].to_vec());
        }
    }
    let mut px = vec![Color32::BLACK; w * 2 * 224 * 4];
    for (i, img) in frames.iter().enumerate() {
        let (ox, oy) = ((i % 2) * w, (i / 2) * 224);
        for y in 0..224 {
            for x in 0..w {
                px[(oy + y) * w * 2 + ox + x] = c15(img[y * w + x]);
            }
        }
    }
    bmp(&format!("{}/blink.bmp", out_dir()), w * 2, 224 * 4, &px);
    let fx = crate::snes::ppu::pal_fx(&s.bus.ppu.cgram, s.bus.ppu.ow.as_deref());
    println!("pal_fx aktiv: {}", fx.is_some());
    println!("8A {:02X} 40A {:02X} 10 {:02X} link {},{} cam {},{} 600..606 {} {} {} {} | 608..60E {} {} {} {} | 712 {}", ram8(&s, 0x7E008A), ram8(&s, 0x7E040A), ram8(&s, 0x7E0010), ram16(&s, 0x7E0022), ram16(&s, 0x7E0020), ram16(&s, 0x7E00E2), ram16(&s, 0x7E00E8), ram16(&s, 0x7E0600), ram16(&s, 0x7E0602), ram16(&s, 0x7E0604), ram16(&s, 0x7E0606), ram16(&s, 0x7E0608), ram16(&s, 0x7E060A), ram16(&s, 0x7E060C), ram16(&s, 0x7E060E), ram16(&s, 0x7E0712));
    if let Some(o) = s.bus.ppu.ow.as_ref() { println!("ow welt {} x0 {} y0 {} size {} bx {}..{} by {}..{} rep {}..{}", o.world, o.x0, o.y0, o.size, o.bx0, o.bx1, o.by0, o.by1, o.rep0, o.rep1); }
    let ft = crate::snes::ppu::fog_table();
    let fl: Vec<String> = ft.iter().enumerate().filter(|(_, v)| **v & 0x8000 != 0).map(|(i, v)| format!("{i:02X}:{:04X}", v & 0x7FFF)).collect();
    println!("Nebel: {}", fl.join(" "));
    let area = ram8(&s, 0x7E008A);
    for id in [area, area.wrapping_sub(1), area.wrapping_sub(8)] {
        if let Some(cg) = crate::snes::ppu::area_cgram(id) {
            let d: Vec<usize> = (32..128).filter(|i| i % 16 != 0 && cg[*i] != s.bus.ppu.cgram[*i]).collect();
            println!("Gebiet {id:02X} gespeichert: {} Farben abweichend vom Spiel, z. B. {:?}", d.len(), d.iter().take(6).map(|&i| (i, cg[i], s.bus.ppu.cgram[i])).collect::<Vec<_>>());
        } else {
            println!("Gebiet {id:02X}: nicht gespeichert");
        }
    }
}

/// Zurückspulen im Breitbild (Slot 1, nur gelesen): Rand nach dem Zurückspulen wie im Spiel?
/// WALK = Richtungen (hex Joypad:Bilder, Komma-getrennt); zählt schwarze Randpixel je zurückgespultem Bild
#[test]
#[ignore]
fn dev_rewind_wide() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::clear_world_cache();
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    crate::app::load_areas(&path);
    let st = std::fs::read(path.with_extension("ss1")).unwrap();
    s.load_state(&st).unwrap();
    let tab: Arc<Vec<u16>> = Arc::new(rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect());
    let ext = 88usize;
    let w = 256 + 2 * ext;
    s.bus.ppu.ext = ext;
    let last_ow: std::cell::RefCell<Option<Arc<crate::snes::ppu::OwMap>>> = std::cell::RefCell::new(None);
    let prep = |s: &mut Snes| {
        let md = ram8(s, 0x7E0010);
        let sub = ram8(s, 0x7E0011);
        let indoors = ram8(s, 0x7E001B) != 0;
        s.bus.ppu.hud_split = md >= 0x06 && md != 0x14;
        s.bus.ppu.ow_edge = if md == 0x09 && sub != 0 { crate::snes::ppu::EDGE_NEIGHBOR } else { crate::snes::ppu::EDGE_SEAMLESS };
        s.bus.ppu.margin = if md < 0x06 { crate::snes::ppu::MARGIN_BLANK } else if indoors { crate::snes::ppu::MARGIN_ROOM } else { crate::snes::ppu::MARGIN_WORLD };
        let st = crate::app::stable_ow(crate::app::ow_map(s, &tab), &last_ow.borrow());
        if let Some(o) = &st {
            if o.valid && !o.room {
                *last_ow.borrow_mut() = Some(o.clone());
            }
        }
        s.bus.ppu.ow = st;
    };
    let black = |wide: &[u16]| -> usize {
        (0..224).map(|y| (0..w).filter(|&x| (x < ext || x >= ext + 256) && wide[y * w + x] & 0x7FFF == 0).count()).sum()
    };
    let walk: Vec<(u16, usize)> = std::env::var("WALK").unwrap_or("0100:120".into()).split(',').map(|p| { let mut i = p.split(':'); (u16::from_str_radix(i.next().unwrap(), 16).unwrap(), i.next().unwrap().parse().unwrap()) }).collect();
    let mut hist = Vec::new();
    let mut f = 0;
    for (joy, n) in walk {
        for _ in 0..n {
            prep(&mut s);
            s.bus.joy[0] = joy;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            if f % 2 == 0 {
                hist.push(s.save_state());
            }
            f += 1;
        }
    }
    println!("vorwärts: schwarz {} Modul {:02X} Gebiet {:02X}", black(&s.bus.ppu.wide), ram8(&s, 0x7E0010), ram8(&s, 0x7E008A));
    let mut shots: Vec<Color32> = Vec::new();
    let mut worst = (0, 0);
    while let Some(st) = hist.pop() {
        // wie die App: zurückspulen
        let p = &s.bus.ppu;
        let keep = (p.room_keep.clone(), p.room_next.clone(), p.room_side.clone(), p.ow_edge, p.win_stretch);
        s.load_state(&st).unwrap();
        let p = &mut s.bus.ppu;
        (p.room_keep, p.room_next, p.room_side, p.ow_edge, p.win_stretch) = keep;
        // wie die App: letzte gültige Gebietskarte weiterverwenden (OLD = altes Verhalten ohne)
        if std::env::var("OLD").is_ok() {
            let full = ram8(&s, 0x7E001B) != 0;
            s.bus.ppu.ow = crate::app::ow_map_full(&s, &tab, full);
        } else {
            prep(&mut s);
        }
        s.bus.joy[0] = 0;
        s.run_frame();
        let b = black(&s.bus.ppu.wide);
        if b > worst.0 {
            worst = (b, hist.len());
            shots = s.bus.ppu.wide.iter().map(|&c| c15(c)).collect();
        }
        if b > 0 {
            println!("zurück {}: schwarz {b} Modul {:02X}/{:02X} Gebiet {:02X} ow {:?}", hist.len(), ram8(&s, 0x7E0010), ram8(&s, 0x7E0011), ram8(&s, 0x7E008A), s.bus.ppu.ow.as_ref().map(|o| (o.valid, o.world, o.room)));
        }
    }
    println!("schlimmstes Bild: {} schwarze Pixel (Zustand {})", worst.0, worst.1);
    if !shots.is_empty() {
        bmp(&format!("{}/rewind_wide.bmp", out_dir()), w, shots.len() / w, &shots);
    }
}

/// Herzteil-Räume (Sprite und Truhe) mit zugeordnetem Eingang und Raum-Bits im Spielstand (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_heartrooms() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let room = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    let show = |r: u16, what: &str| {
        let e = crate::world::entrance_for_room_pub(&ents, r).map(|i| ents[i]);
        println!("{what} Raum {r:03X} Bits {:04X} Eingang {:?}", room(r), e.map(|e| (e.eid, e.area, e.x, e.y, e.room)));
    };
    for r in crate::world::room_heart_pieces_pub(&rom) {
        show(r, "SPRITE");
    }
    if let Some(t) = crate::randomizer::find_chest_table(&rom) {
        for c in t.chests.iter().filter(|c| c.item == 0x17) {
            show(c.room, &format!("TRUHE#{}", c.index_in_room));
        }
    }
    for r in [0x123u16, 0x11B, 0x11C, 0x113, 0x10B] {
        show(r, "EXTRA");
    }
    for e in ents.iter().filter(|e| e.room == 0x11B || e.area == 0x14 || e.area == 0x32 || e.area == 0x30) {
        println!("EING eid {:02X} gebiet {:02X} raum {:03X} bei {:.0},{:.0}", e.eid, e.area, e.room, e.x, e.y);
    }
}

/// Wahrheit aus dem Spiel: in jede Herzteil-Höhle gehen (Kopie von Slot 1) und prüfen, ob das Herzteil (0xEB) noch da ist
#[test]
#[ignore]
fn dev_heart_truth() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let room = |s: &Snes, r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    for e in ents.iter().filter(|e| [0xE1u16, 0xE2, 0xEA, 0x11B, 0x126, 0x127].contains(&e.room)) {
        let mut c = s.clone();
        if crate::teleport::to_target(&mut c, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, false).is_err() {
            println!("eid {:02X}: Teleport fehlgeschlagen", e.eid);
            continue;
        }
        for _ in 0..20 {
            c.bus.joy[0] = 0;
            c.run_frame();
            c.bus.apu.dsp.out.clear();
        }
        let hearts: Vec<String> = (0..16).filter(|&i| c.bus.wram[0x0DD0 + i] != 0 && c.bus.wram[0x0E20 + i] == 0xEB).map(|i| {
            let x = c.bus.wram[0x0D10 + i] as u16 | (c.bus.wram[0x0D30 + i] as u16) << 8;
            let y = c.bus.wram[0x0D00 + i] as u16 | (c.bus.wram[0x0D20 + i] as u16) << 8;
            format!("({},{})", x & 0x1FF, y & 0x1FF)
        }).collect();
        println!("eid {:02X} gebiet {:02X} raum {:03X} Bits {:04X} Link {},{} -> Herzteil da: {}", e.eid, e.area, e.room, room(&s, e.room), ram16(&c, 0x7E0022) & 0x1FF, ram16(&c, 0x7E0020) & 0x1FF, if hearts.is_empty() { "nein (eingesammelt)".to_string() } else { hearts.join(" ") });
    }
}

/// Truhen/Treppen je Raum: Kachel-Gruppen, Truhentabelle und Raum-Bits (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_roomicons() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let maps = crate::world::find_dungeon_maps(&rom);
    let rt = crate::world::entrance_room_table(&rom).unwrap();
    let t = crate::randomizer::find_chest_table(&rom);
    let d: usize = std::env::var("DUNGEON").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let dm = &maps[d];
    let eid = ents.iter().find(|e| dm.contains(e.room)).map(|e| e.eid).unwrap();
    let flags = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    for f in &dm.floors {
        for &rm in f.iter().filter(|&&r| r != 0x0F) {
            let rm = rm as u16;
            let Some(shot) = crate::teleport::capture_room_shot(&s, rt, eid, rm) else { continue };
            let mut g: std::collections::BTreeMap<u8, (usize, usize, usize)> = Default::default();
            for (k, &a) in shot.attr.iter().enumerate() {
                if (0x58..=0x5D).contains(&a) || (0x1D..=0x1F).contains(&a) || (0x30..=0x3F).contains(&a) || (0x5E..=0x5F).contains(&a) || (0xC0..=0xCF).contains(&a) || (0x60..=0x67).contains(&a) {
                    let e = g.entry(a).or_insert((0, 0, 0));
                    e.0 += (k % 64) * 8 + 4;
                    e.1 += ((k / 64) % 64) * 8 + 4;
                    e.2 += 1;
                }
            }
            let gs: Vec<String> = g.iter().map(|(a, v)| format!("{a:02X}@{},{}x{}", v.0 / v.2, v.1 / v.2, v.2)).collect();
            let ch: Vec<String> = t.as_ref().map(|t| t.chests.iter().filter(|c| c.room == rm).map(|c| format!("#{}:{:02X}", c.index_in_room, c.item)).collect()).unwrap_or_default();
            println!("Raum {rm:03X} Bits {:04X} Truhen {:?} | {}", flags(rm), ch, gs.join(" "));
        }
    }
}

/// Türen und Stern-Fliesen je Raum: Tür-Tabellen im RAM ($1980…) und Kachel-Gruppen (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_doors() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let maps = crate::world::find_dungeon_maps(&rom);
    let rt = crate::world::entrance_room_table(&rom).unwrap();
    let d: usize = std::env::var("DUNGEON").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let dm = &maps[d];
    let eid = ents.iter().find(|e| dm.contains(e.room)).map(|e| e.eid).unwrap();
    let flags = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    for f in &dm.floors {
        for &rm in f.iter().filter(|&&r| r != 0x0F) {
            let rm = rm as u16;
            let mut c = s.clone();
            let mut r2 = (*c.bus.rom).clone();
            let o = rt + eid as usize * 2;
            r2[o] = rm as u8;
            r2[o + 1] = (rm >> 8) as u8;
            c.bus.rom = Arc::new(r2);
            c.bus.ppu.ext = 0;
            c.bus.ppu.skip = true;
            if crate::teleport::to_entrance(&mut c, eid, false).is_err() || ram16(&c, 0x7E00A0) != rm {
                continue;
            }
            let w = &c.bus.wram;
            let tab = |a: usize, n: usize| (0..n).map(|i| format!("{:02X}", w[a + i])).collect::<Vec<_>>().join(" ");
            let mut g: std::collections::BTreeMap<u8, usize> = Default::default();
            for &a in &w[0x12000..0x14000] {
                if (0x1D..=0x1F).contains(&a) || (0x30..=0x3F).contains(&a) || a >= 0x80 {
                    *g.entry(a).or_default() += 1;
                }
            }
            let gs: Vec<String> = g.iter().map(|(a, n)| format!("{a:02X}x{n}")).collect();
            println!("Raum {rm:03X} Bits {:04X} | 1980 {} | 19A0 {} | 19C0 {} | {}", flags(rm), tab(0x1980, 16), tab(0x19A0, 16), tab(0x19C0, 16), gs.join(" "));
        }
    }
}

/// Kann Link im Spielstand laufen? (Slot 1 nur gelesen) – je Richtung 40 Bilder drücken, Position vorher/nachher
#[test]
#[ignore]
fn dev_move() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    println!("Modus {:02X}/{:02X} Gebiet {:02X} Link {},{} Zustand 5D {:02X}", ram8(&s, 0x7E0010), ram8(&s, 0x7E0011), ram8(&s, 0x7E008A), ram16(&s, 0x7E0022), ram16(&s, 0x7E0020), ram8(&s, 0x7E005D));
    let w = &s.bus.wram;
    println!("Raum {:03X} 02E4 {:02X} 0046 {:02X} 004D {:02X} 037B {:02X} 0308 {:02X} 02D8 {:02X} 0112 {:02X} 0FFC {:02X} 00B0 {:02X} 0372 {:02X} 005B {:02X} 0024 {:02X} 0300 {:02X} 0301 {:02X} 02DA {:02X} 0345 {:02X} 004E {:02X} 03EF {:02X} 0C4A {:02X}",
        ram16(&s, 0x7E00A0), w[0x02E4], w[0x46], w[0x4D], w[0x37B], w[0x308], w[0x2D8], w[0x112], w[0xFFC], w[0xB0], w[0x372], w[0x5B], w[0x24], w[0x300], w[0x301], w[0x2DA], w[0x345], w[0x4E], w[0x3EF], w[0xC4A]);
    for (n, j) in [("hoch", 0x0800u16), ("runter", 0x0400), ("links", 0x0200), ("rechts", 0x0100)] {
        let mut c = s.clone();
        let (x0, y0) = (ram16(&c, 0x7E0022), ram16(&c, 0x7E0020));
        for _ in 0..40 {
            c.bus.joy[0] = j;
            c.run_frame();
            c.bus.apu.dsp.out.clear();
        }
        println!("{n}: {},{} -> {},{} Modus {:02X}", x0, y0, ram16(&c, 0x7E0022), ram16(&c, 0x7E0020), ram8(&c, 0x7E0010));
    }
    // länger warten und Tasten drücken: löst sich die Sperre?
    let mut c = s.clone();
    for f in 0..900 {
        c.bus.joy[0] = match f { 300..=310 => 0x0080, 400..=410 => 0x8000, 500..=510 => 0x1000, 520..=530 => 0x1000, _ => 0 };
        c.run_frame();
        c.bus.apu.dsp.out.clear();
        if f % 150 == 0 || c.bus.wram[0x02E4] == 0 {
            println!("Bild {f}: 02E4 {:02X} Modus {:02X}/{:02X}", c.bus.wram[0x02E4], ram8(&c, 0x7E0010), ram8(&c, 0x7E0011));
            if c.bus.wram[0x02E4] == 0 { break; }
        }
    }
    // Sperre von Hand lösen: kann Link dann laufen?
    let mut c = s.clone();
    c.bus.wram[0x02E4] = 0;
    for _ in 0..40 {
        c.bus.joy[0] = 0x0400;
        c.run_frame();
        c.bus.apu.dsp.out.clear();
    }
    println!("mit 02E4=0 runter: -> {},{}", ram16(&c, 0x7E0022), ram16(&c, 0x7E0020));
}

/// Schwarze Formen an Link? Slot 1 laden (nur gelesen), Schwert schwingen/laufen, Bilder speichern
#[test]
#[ignore]
fn dev_blackspr() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    s.bus.wram[0x02E4] = 0;
    println!("Modus {:02X} Gebiet {:02X} Raum {:03X} Link {},{}", ram8(&s, 0x7E0010), ram8(&s, 0x7E008A), ram16(&s, 0x7E00A0), ram16(&s, 0x7E0022), ram16(&s, 0x7E0020));
    println!("Schwert {:02X} Schild {:02X} Rüstung {:02X}", ram8(&s, 0x7EF359), ram8(&s, 0x7EF35A), ram8(&s, 0x7EF35B));
    for row in 8..16 {
        let c: Vec<String> = (0..16).map(|i| format!("{:04X}", s.bus.ppu.cgram[row * 16 + i])).collect();
        println!("CGRAM Zeile {row:2}: {}", c.join(" "));
    }
    if std::env::var("TPTEST").is_ok() {
        let rom = (*s.bus.rom).clone();
        let ents = crate::world::find_entrances(&rom);
        let e = ents.iter().find(|e| e.world == 0 && e.area == ram8(&s, 0x7E008A)).copied().unwrap_or(ents[0]);
        let r = crate::teleport::to_target(&mut s, &rom, crate::teleport::Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, true);
        for _ in 0..30 { s.run_frame(); s.bus.apu.dsp.out.clear(); }
        let c: Vec<String> = (0..16).map(|i| format!("{:04X}", s.bus.ppu.cgram[11 * 16 + i])).collect();
        println!("nach Teleport {:?} Modus {:02X}: Zeile 11: {}", r, ram8(&s, 0x7E0010), c.join(" "));
    }
    // im Spiel-RAM gepufferte Palette ($7EC500, 0x200 Bytes) – die überträgt das Spiel in die PPU
    for row in 8..16 {
        let c: Vec<String> = (0..16).map(|i| { let o = 0xC500 + (row * 16 + i) * 2; format!("{:04X}", s.bus.wram[o] as u16 | (s.bus.wram[o + 1] as u16) << 8) }).collect();
        println!("RAM-Pal  Zeile {row:2}: {}", c.join(" "));
    }
    let mut n = 0;
    for f in 0..120 {
        s.bus.joy[0] = match f { 0..=30 => 0x0400, 31..=36 => 0x8000, 60..=90 => 0x0100, 91..=96 => 0x8000, _ => 0 };
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        if f % 6 == 0 && n < 16 {
            let p: Vec<Color32> = s.framebuffer().iter().map(|&v| c15(v)).collect();
            bmp(&format!("{}/bs_{n:02}.bmp", out_dir()), 256, 224, &p);
            n += 1;
        }
    }
}

/// Richtige Schwert-/Schild-Farben: in einer Kopie das Laden des Spielstands (Modus 05) auslösen und die Palette lesen
#[test]
#[ignore]
fn dev_gearpal() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let row = |s: &Snes, r: usize| (0..16).map(|i| format!("{:04X}", s.bus.ppu.cgram[r * 16 + i])).collect::<Vec<_>>().join(" ");
    println!("vorher  Zeile 11: {}", row(&s, 11));
    let mut c = s.clone();
    if std::env::var("RAWROM").is_ok() {
        // unveränderte ROM (ohne Kopfzeile) statt der von der App vorbereiteten
        let mut raw = std::fs::read(&path).unwrap();
        if raw.len() % 0x8000 == 512 {
            raw.drain(..512);
        }
        println!("ROM gleich wie vorbereitete: {}", raw == **c.bus.rom);
        c.bus.rom = Arc::new(raw);
    }
    c.bus.wram[0x10] = 0x05;
    c.bus.wram[0x11] = 0x00;
    for f in 0..900 {
        c.bus.joy[0] = if f % 40 < 4 && c.bus.wram[0x10] != 0x07 && c.bus.wram[0x10] != 0x09 { 0x0080 } else { 0 };
        c.run_frame();
        c.bus.apu.dsp.out.clear();
        if f % 100 == 0 {
            println!("Bild {f} Modus {:02X}/{:02X} Zeile 11: {}", c.bus.wram[0x10], c.bus.wram[0x11], row(&c, 11));
        }
    }
}

/// Figuren-Tabelle (OAM) rund um Link: Kachel, Palette, Priorität (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_oam() {
    let Some((mut s, _rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    s.bus.wram[0x02E4] = 0;
    if std::env::var("RELOAD").is_ok() {
        s.bus.wram[0x10] = 0x05;
        s.bus.wram[0x11] = 0x00;
        for f in 0..900 {
            s.bus.joy[0] = if f % 40 < 4 && s.bus.wram[0x10] != 0x07 && s.bus.wram[0x10] != 0x09 { 0x0080 } else { 0 };
            s.run_frame();
            s.bus.apu.dsp.out.clear();
        }
    }
    println!("EE {:02X}", s.bus.wram[0xEE]);
    if std::env::var("FIXSWAP").is_ok() {
        s.bus.wram[0x0ABD] = 0;
    }
    println!("Link-Werte: 0ABD {:02X} 0B7B {:02X} 02E0 {:02X} 0FFF {:02X} 03CA {:02X} 0B79 {:02X} 004B {:02X} 0373 {:02X}", s.bus.wram[0x0ABD], s.bus.wram[0x0B7B], s.bus.wram[0x02E0], s.bus.wram[0x0FFF], s.bus.wram[0x03CA], s.bus.wram[0x0B79], s.bus.wram[0x4B], s.bus.wram[0x373]);
    for _ in 0..3 {
        s.bus.joy[0] = 0;
        s.run_frame();
    }
    let (lx, ly) = (ram16(&s, 0x7E0022) as i32 - ram16(&s, 0x7E00E2) as i32, ram16(&s, 0x7E0020) as i32 - ram16(&s, 0x7E00E8) as i32);
    println!("Link auf dem Bild {lx},{ly}  obsel {:02X}", s.bus.ppu.r.obsel);
    let o = &s.bus.ppu.oam;
    for i in 0..128 {
        let (x, y, t, a) = (o[i * 4] as i32, o[i * 4 + 1] as i32, o[i * 4 + 2], o[i * 4 + 3]);
        let hi = (o[512 + i / 4] >> ((i % 4) * 2)) & 3;
        let x = x | (((hi & 1) as i32) << 8);
        let x = if x >= 256 { x - 512 } else { x };
        if (x - lx).abs() < 40 && (y - ly).abs() < 40 {
            let pal = (a >> 1) & 7;
            let cols: Vec<String> = (0..16).map(|k| format!("{:04X}", s.bus.ppu.cgram[128 + pal as usize * 16 + k])).collect();
            println!("#{i:3} x {x:4} y {y:4} Kachel {:03X} Pal {pal} Prio {} groß {} | {}", t as u16 | ((a as u16 & 1) << 8), (a >> 4) & 3, hi >> 1, cols.join(" "));
        }
    }
}

/// Weltkarte aus den gespeicherten Gebieten als Bild (Lichtwelt 0 / Schattenwelt 1): fehlend = Magenta
#[test]
#[ignore]
fn dev_realmap() {
    let Some((_s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    crate::snes::ppu::set_area_parents(crate::teleport::area_parents());
    println!("Gebiete geladen: {}", crate::app::load_areas(&path));
    let tab: Vec<u16> = rom[0x78000..0x80000].chunks_exact(2).map(|b| b[0] as u16 | (b[1] as u16) << 8).collect();
    let w: u8 = std::env::var("WORLD").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let img = crate::snes::ppu::world_from_areas(&tab, w);
    let have: Vec<String> = crate::snes::ppu::all_areas().iter().map(|a| format!("{:02X}", a.0)).collect();
    println!("gespeicherte Gebiete: {}", have.join(" "));
    // Lücken mit der gezeichneten Karte des Spiels füllen (wie in der App)
    let game: Option<Vec<u8>> = std::fs::read(path.with_extension("worldmap")).ok().and_then(|d| lz4_flex::decompress_size_prepended(&d).ok());
    let px: Vec<Color32> = img
        .iter()
        .enumerate()
        .map(|(i, v)| match (v, &game) {
            (Some(c), _) => c15(*c),
            (None, Some(gm)) => {
                let o = (w as usize * 1024 * 1024 + i) * 3;
                Color32::from_rgb(gm[o], gm[o + 1], gm[o + 2])
            }
            _ => Color32::from_rgb(255, 0, 255),
        })
        .collect();
    bmp(&format!("{}/realmap{w}.bmp", out_dir()), 1024, 1024, &px);
}

/// Was fehlt in einem Dungeon? (Slot 1 nur gelesen) – Orte des Dungeons, Karte/Kompass/Großer Schlüssel
#[test]
#[ignore]
fn dev_missing() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let maps = crate::world::find_dungeon_maps(&rom);
    let t = crate::randomizer::find_chest_table(&rom);
    let st = crate::tracker::find_static_items(&rom);
    let ents = crate::world::find_entrances(&rom);
    let (locs, _) = crate::world::build_locations(&rom, t.as_ref(), &st, &ents, &maps);
    for d in [0u8, 1] {
        println!("== {}", crate::world::DMAP_NAMES[d as usize].0);
        for l in locs.iter().filter(|l| l.dungeon == Some(d)) {
            println!("  {} {}/{}", l.name_de, l.done(&s, false), l.total());
        }
        let bit = if d <= 1 { 0xC000u16 } else { 0x8000u16 >> d };
        for (n, a) in [("Karte", 0x7EF368u32), ("Kompass", 0x7EF364), ("Großer Schlüssel", 0x7EF366)] {
            println!("  {n}: {}", ram16(&s, a) & bit != 0);
        }
    }
    if let Some(t) = &t {
        for c in t.chests.iter() {
            if maps[0].contains(c.room) || maps[1].contains(c.room) {
                let f = s.bus.wram[0xF000 + c.room as usize * 2] as u16 | (s.bus.wram[0xF001 + c.room as usize * 2] as u16) << 8;
                println!("  Truhe Raum {:03X} #{} Inhalt {:02X} {} offen {}", c.room, c.index_in_room, c.item, crate::alttp::item_name(c.item, false), f & (0x10 << c.index_in_room) != 0);
            }
        }
    }
}

/// Schlüsseltüren, die nur auf einer Seite als aufgeschlossen vermerkt sind (Slot 1 nur gelesen)
#[test]
#[ignore]
fn dev_doorpairs() {
    let Some((mut s, rom)) = boot() else { return };
    let path = std::path::PathBuf::from(std::env::var("PASTLINK_ROM").unwrap());
    s.load_state(&std::fs::read(path.with_extension("ss1")).unwrap()).unwrap();
    let ents = crate::world::find_entrances(&rom);
    let maps = crate::world::find_dungeon_maps(&rom);
    let rt = crate::world::entrance_room_table(&rom).unwrap();
    let flags = |r: u16| s.bus.wram[0xF000 + r as usize * 2] as u16 | (s.bus.wram[0xF001 + r as usize * 2] as u16) << 8;
    let mut shots = std::collections::HashMap::new();
    for dm in &maps {
        let eid = ents.iter().find(|e| dm.contains(e.room)).map(|e| e.eid).unwrap();
        for f in &dm.floors {
            for &r in f.iter().filter(|&&r| r != 0x0F) {
                if let Some(sh) = crate::teleport::capture_room_shot(&s, rt, eid, r as u16) {
                    shots.insert(r as u16, sh);
                }
            }
        }
    }
    let (mut n, mut fixed) = (0, 0);
    for (&r, sh) in &shots {
        for &(i, ty, pos, dir) in &sh.doors {
            if !matches!(ty, 0x1C | 0x1E | 0x20 | 0x22 | 0x24 | 0x26) || dir > 3 {
                continue;
            }
            n += 1;
            let own = i < 4 && flags(r) & (0x8000 >> i) != 0;
            let nb = match dir { 0 => r.wrapping_sub(16), 1 => r + 16, 2 => r.wrapping_sub(1), _ => r + 1 };
            let other = shots.get(&nb).map_or(false, |ns| ns.doors.iter().any(|&(j, t2, p2, d2)| {
                let same = if dir < 2 { p2 % 64 == pos % 64 } else { p2 / 64 == pos / 64 };
                j < 4 && d2 == dir ^ 1 && t2 == ty && same && flags(nb) & (0x8000 >> j) != 0
            }));
            if !own && other {
                fixed += 1;
                println!("Raum {r:03X} Tür {i} Typ {ty:02X}: nur im Nachbarraum {nb:03X} aufgeschlossen -> jetzt beide Seiten");
            }
        }
    }
    println!("{n} Schlüsseltüren, {fixed} jetzt zusätzlich als aufgeschlossen erkannt");
}

/// Spielstand laden: Emulation gegen Zelda-3-Modus (Tür/Gras nach dem Laden), Bilder state_emu.bmp / state_z3.bmp
#[test]
#[ignore]
fn dev_state_z3() {
    let Some((snes, _rom)) = boot() else { return };
    let Ok(st) = std::env::var("STATE_FILE") else { return };
    let data = std::fs::read(st).unwrap();
    // Emulation
    let mut a = snes.clone();
    a.load_state(&data).unwrap();
    for _ in 0..30 {
        a.run_frame();
        a.bus.apu.dsp.out.clear();
    }
    shot(&a, "state_emu");
    println!("Emulation: {}", status(&a));
    // Zelda 3: einschalten, dann laden (wie in der App)
    let mut b = snes.clone();
    let _ = crate::z3::sync(&mut b, true);
    for _ in 0..5 {
        b.run_frame();
        b.bus.apu.dsp.out.clear();
    }
    b.load_state(&data).unwrap();
    for _ in 0..30 {
        b.run_frame();
        b.bus.apu.dsp.out.clear();
    }
    shot(&b, "state_z3");
    println!("Zelda 3 aktiv: {} – {}", b.z3.active(), status(&b));
}

/// Map16-Tabelle: Zelda-3-Portierung (US) gegen die geladene ROM (DE) – welche Einträge weichen ab?
#[test]
#[ignore]
#[cfg(pastlink_z3)]
fn dev_map16_diff() {
    let Ok(path) = std::env::var("PASTLINK_ROM") else { return };
    let (rom, _) = crate::alttp::prepare_rom(std::fs::read(path).unwrap()).unwrap();
    let us = crate::z3::map16_table();
    let de: Vec<u16> = rom[0x78000..0x78000 + us.len() * 2].chunks_exact(2).map(|c| c[0] as u16 | (c[1] as u16) << 8).collect();
    let diff: Vec<usize> = (0..us.len() / 4).filter(|&i| us[i * 4..i * 4 + 4] != de[i * 4..i * 4 + 4]).collect();
    println!("Map16: {} Einträge, {} weichen ab", us.len() / 4, diff.len());
    println!("abweichend: {:X?}", &diff[..diff.len().min(200)]);
    // Bereiche zusammenfassen
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for &d in &diff {
        match runs.last_mut() {
            Some(r) if r.1 + 1 == d => r.1 = d,
            _ => runs.push((d, d)),
        }
    }
    println!("Bereiche: {}", runs.iter().map(|r| format!("{:X}-{:X}", r.0, r.1)).collect::<Vec<_>>().join(" "));
    println!("Umrechnung: {:X?}", crate::z3::remap_check(&rom, &[0xda4, 0xda8, 0xdab, 0xdb4, 0xdc5, 0xdc7, 0xdc8, 0xdc9, 0xdcb, 0xe21, 0x918]));
    for id in [0xda8usize, 0xdab, 0xdb6, 0xdc5, 0xdc6, 0xdc7, 0xdc8, 0xdc9, 0xdca, 0xdcb, 0xdcd, 0xdd3, 0x918] {
        println!("  {id:X}: US {:04X?} DE {:04X?}", &us[id * 4..id * 4 + 4], &de[id * 4..id * 4 + 4]);
    }
}

/// Spielstand laden, nach unten laufen, nach links drehen, Schwert: Busch schneiden – Emulation gegen Zelda 3
#[test]
#[ignore]
fn dev_cut_bush() {
    let Some((snes, _rom)) = boot() else { return };
    let Ok(st) = std::env::var("STATE_FILE") else { return };
    let data = std::fs::read(st).unwrap();
    let script = |s: &mut Snes| {
        let mut run = |s: &mut Snes, pad: u16, n: u32| {
            for _ in 0..n {
                s.bus.joy[0] = pad;
                s.run_frame();
                s.bus.apu.dsp.out.clear();
            }
        };
        run(s, 0, 20);
        run(s, 0x0400, 27);
        run(s, 0x0200, 7);
        run(s, 0, 4);
        run(s, 0x8000, 4);
        run(s, 0, 60);
    };
    for z3 in [false, true] {
        let mut b = snes.clone();
        if z3 {
            let _ = crate::z3::sync(&mut b, true);
            for _ in 0..5 {
                b.run_frame();
                b.bus.apu.dsp.out.clear();
            }
        }
        b.load_state(&data).unwrap();
        script(&mut b);
        shot(&b, if z3 { "cut_z3" } else { "cut_emu" });
        println!("{}: Zelda 3 aktiv {} – {}", if z3 { "Z3" } else { "Emu" }, b.z3.active(), status(&b));
    }
}

/// Spielstand: Map16-Nummern der Oberwelt-Karte im Arbeitsspeicher rund um Link (Tür/Gras falsch gespeichert?)
#[test]
#[ignore]
fn dev_door_tiles() {
    let Some((snes, _rom)) = boot() else { return };
    let Ok(st) = std::env::var("STATE_FILE") else { return };
    let mut a = snes.clone();
    if std::env::var_os("Z3").is_some() {
        let _ = crate::z3::sync(&mut a, true);
        for _ in 0..5 {
            a.run_frame();
            a.bus.apu.dsp.out.clear();
        }
    }
    a.load_state(&std::fs::read(st).unwrap()).unwrap();
    for _ in 0..20 {
        a.run_frame();
        a.bus.apu.dsp.out.clear();
    }
    shot(&a, "door_now");
    let area = ram8(&a, 0x7E040A);
    let (lx, ly) = (ram16(&a, 0x7E0022) as i32, ram16(&a, 0x7E0020) as i32);
    let (ax, ay) = ((area as i32 & 7) * 512, ((area as i32 & 0x3F) / 8) * 512);
    let (tx, ty) = ((lx - ax) / 16, (ly - ay) / 16);
    println!("Gebiet {area:02X}, Link {lx},{ly} -> Kachel {tx},{ty}");
    for y in (ty - 6).max(0)..(ty + 3).min(64) {
        let mut row = String::new();
        for x in (tx - 6).max(0)..(tx + 6).min(64) {
            let i = (y * 64 + x) as usize * 2;
            let v = a.bus.wram[0x2000 + i] as u16 | (a.bus.wram[0x2001 + i] as u16) << 8;
            row += &format!("{}{v:03X} ", if v >= 0x9A4 { "*" } else { " " });
        }
        println!("{y:2}: {row}");
    }
}
