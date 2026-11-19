//! Teleport zu bereits besuchten Eingängen (Höhlen, Häuser, Dungeons).
//!
//! Funktioniert wie das Spiel selbst beim Betreten einer Tür: Eingangs-ID nach `$010E`, dann
//! lädt Modus 0x06 (Unterwelt laden) Raum, Position, Kamera, Musik und Palette aus den
//! Eingangstabellen der ROM. Danach läuft Link unsichtbar durch die Tür nach draußen und steht
//! vor dem Gebäude.

use crate::alttp::{ram16, ram8};
use crate::snes::Snes;

fn w8(s: &mut Snes, a: u32, v: u8) {
    s.bus.wram[(a & 0x1FFFF) as usize] = v;
}

/// Teleport möglich? (normales Spiel auf der Oberwelt oder in einem Raum, kein Menü/Übergang)
pub fn can_teleport(s: &Snes) -> bool {
    let m = ram8(s, 0x7E0010);
    let sub = ram8(s, 0x7E0011);
    // 0x0B: Sondergebiete der Oberwelt (Masterschwert-Hain usw.)
    matches!(m, 0x07 | 0x09 | 0x0B) && sub == 0
}

/// Teleportiert Link in den Eingang `eid`. Läuft so lange, bis der Raum geladen ist.
/// Ziel eines Teleports: Eingangs-ID, Gebiet und Türposition auf der Oberwelt
#[derive(Clone, Copy)]
pub struct Target {
    pub eid: u8,
    pub area: u8,
    pub x: f32,
    pub y: f32,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn to_entrance(s: &mut Snes, eid: u8, outside: bool) -> Result<(), &'static str> {
    to_target(s, &[], Target { eid, area: 0xFF, x: 0.0, y: 0.0 }, outside)
}

/// Teleport mit vollständigen Zieldaten: Schattenwelt-Markierung und Rückkehr-Daten der Höhle werden
/// passend gesetzt, damit Link beim Rausgehen vor genau dieser Tür steht.
pub fn to_target(s: &mut Snes, rom: &[u8], t: Target, outside: bool) -> Result<(), &'static str> {
    let eid = t.eid;
    if !can_teleport(s) {
        return Err("busy");
    }
    if t.area != 0xFF {
        // Licht-/Schattenwelt des Ziels (sonst lädt das Spiel beim Rausgehen die falsche Welt)
        s.bus.wram[0xF3CA] = if t.area >= 0x40 { 0x40 } else { 0x00 };
    }
    // Link in Ruhe versetzen (kein Schwimmen, Springen, Tragen …)
    w8(s, 0x7E005D, 0x00); // Link-Zustand: normal
    w8(s, 0x7E0308, 0x00); // trägt nichts
    w8(s, 0x7E0309, 0x00);
    w8(s, 0x7E037B, 0x00);
    w8(s, 0x7E0046, 0x00); // kein Rückstoß
    w8(s, 0x7E004D, 0x00);
    w8(s, 0x7E02E0, 0x00); // keine Hasenform
    w8(s, 0x7E0055, 0x00); // Umhang aus
    w8(s, 0x7E02E4, 0x00); // Bewegungssperre aus (sonst kann Link nach dem Teleport nicht laufen)
    // Paletten-Tausch der oberen Ebene (Brücken) aus – sonst bleiben Schild und Schwert schwarz
    w8(s, 0x7E0ABD, 0x00);
    w8(s, 0x7E00EE, 0x00);
    w8(s, 0x7E010E, eid);
    w8(s, 0x7E010F, 0x00);
    w8(s, 0x7E0013, 0x00); // Bildschirm dunkel
    w8(s, 0x7E0010, 0x06); // Unterwelt laden
    w8(s, 0x7E0011, 0x00);
    w8(s, 0x7E00B0, 0x00);
    let joy = s.bus.joy[0];
    s.bus.joy[0] = 0;
    let mut ok = false;
    for _ in 0..240 {
        s.run_frame();
        s.bus.apu.dsp.out.clear();
        if ram8(s, 0x7E0010) == 0x07 && ram8(s, 0x7E0011) == 0 && s.bus.ppu.brightness() == 15 {
            ok = true;
            break;
        }
    }
    if !ok {
        s.bus.joy[0] = joy;
        return Err("timeout");
    }
    // erst jetzt die Rückkehr-Daten setzen (das Laden des Raums überschreibt sie mit der alten Position)
    if t.area != 0xFF {
        prepare_exit(s, rom, t.area, t.x, t.y);
    }
    // Link steht innen an der Tür und schaut hinein: unsichtbar ein paar Schritte zurück nach draußen
    if outside {
        let back: u16 = match ram8(s, 0x7E002F) {
            0 => 0x0400, // schaut nach oben -> nach unten raus
            2 => 0x0800,
            4 => 0x0100,
            _ => 0x0200,
        };
        let saved = s.clone();
        let mut out = false;
        for _ in 0..360 {
            s.bus.joy[0] = back;
            s.run_frame();
            s.bus.apu.dsp.out.clear();
            if ram8(s, 0x7E0010) == 0x09 && ram8(s, 0x7E0011) == 0 && s.bus.ppu.brightness() == 15 {
                out = true;
                break;
            }
        }
        if out {
            // kurz stehen lassen und prüfen, dass Link draußen bleibt (nicht gleich wieder hineinläuft)
            for _ in 0..40 {
                s.bus.joy[0] = 0;
                s.run_frame();
                s.bus.apu.dsp.out.clear();
                if ram8(s, 0x7E0010) != 0x09 {
                    out = false;
                    break;
                }
            }
        }
        if !out {
            // kein normaler Ausgang (z. B. Loch/Treppe): drinnen bleiben
            *s = saved;
        }
    }
    s.bus.joy[0] = joy;
    Ok(())
}

/// Große Gebiete (2×2): obere linke Gebiets-ID; Licht- und Schattenwelt
pub(crate) const LARGE: [u8; 16] = [0x00, 0x03, 0x05, 0x18, 0x1B, 0x1E, 0x30, 0x35, 0x40, 0x43, 0x45, 0x58, 0x5B, 0x5E, 0x70, 0x75];

fn w16(s: &mut Snes, a: usize, v: i32) {
    s.bus.wram[a] = v as u8;
    s.bus.wram[a + 1] = (v >> 8) as u8;
}

/// Schreibt die Oberwelt-Daten, die das Spiel beim Betreten einer Höhle sichert ($7EC140…), so als wäre
/// Link wirklich durch diese Tür gegangen. Beim Verlassen stellt das Spiel genau diese Werte wieder her.
fn prepare_exit(s: &mut Snes, rom: &[u8], area: u8, door_x: f32, door_y: f32) {
    let parent = LARGE.iter().copied().find(|&p| [p, p + 1, p + 8, p + 9].contains(&area)).unwrap_or(area);
    let large = LARGE.contains(&parent);
    let size = if large { 1024 } else { 512 };
    let pa = (parent & 0x3F) as i32;
    let (x0, y0) = ((pa % 8) * 512, (pa / 8) * 512);
    let (lx, ly) = (door_x as i32, door_y as i32);
    // Gebiet (Viertel) in dem die Tür liegt
    let q = if large { ((lx - x0) >= 512) as u8 + 8 * ((ly - y0) >= 512) as u8 } else { 0 };
    let cur = parent + q;
    let cam_x = (lx - 126).clamp(x0, x0 + size - 256);
    let cam_y = (ly - 96).clamp(y0, y0 + size - 226);
    // Grafikspeicher-Position der Kamera (Map16-Raster, 64 je Zeile, relativ zum Gebiet)
    let vram = ((cam_y - y0 + 12) >> 4) * 128 + ((cam_x - x0) >> 4) * 2;
    let gfx_lo = rom.get(0x6FEF0 + cur as usize).copied().unwrap_or(0x20) as i32;
    let gfx_hi = rom.get(0x64370 + cur as usize).copied().unwrap_or(0) as i32;
    let tm = if [0x03, 0x05, 0x07, 0x43, 0x45, 0x47].contains(&(cur & 0x7F)) { 0x116 } else { 0x16 };
    let b = 0xC140;
    let _ = cur;
    let vals: [i32; 25] = [
        parent as i32,
        tm,
        cam_y,
        cam_x,
        ly,
        lx,
        parent as i32,
        vram,
        cam_y + 109,
        lx + 11,
        y0,
        y0 + size - 226,
        x0,
        x0 + size - 256,
        y0 - 224,
        y0 + size,
        x0 - 256,
        x0 + size,
        0x2000,
        gfx_lo | (gfx_hi << 8),
        0,
        0,
        0,
        0,
        0,
    ];
    for (i, v) in vals.iter().enumerate() {
        w16(s, b + i * 2, *v);
    }
}

/// Obere linke Gebiets-ID jedes Gebiets (0..0x7F)
pub fn area_parents() -> Vec<u8> {
    (0..0x80u8)
        .map(|a| LARGE.iter().copied().find(|&p| [p, p + 1, p + 8, p + 9].contains(&a)).unwrap_or(a))
        .collect()
}

/// Lädt einen Dungeon-Raum unsichtbar in einer Kopie des Spiels (über einen Eingang, dessen Raum in einer
/// Kopie der ROM auf den gewünschten Raum gesetzt wird) und gibt den Grafikspeicher zurück.
pub fn capture_room(base: &Snes, room_table: usize, eid: u8, room: u16) -> Option<Vec<u16>> {
    let mut c = base.clone();
    let mut rom = (*c.bus.rom).clone();
    let o = room_table + eid as usize * 2;
    if o + 1 >= rom.len() {
        return None;
    }
    rom[o] = room as u8;
    rom[o + 1] = (room >> 8) as u8;
    c.bus.rom = std::sync::Arc::new(rom);
    c.bus.ppu.ext = 0;
    c.bus.ppu.record = false;
    c.bus.ppu.capture = false;
    c.bus.ppu.skip = true;
    to_target(&mut c, &[], Target { eid, area: 0xFF, x: 0.0, y: 0.0 }, false).ok()?;
    if ram16(&c, 0x7E00A0) != room {
        return None;
    }
    Some(c.bus.ppu.vram.clone())
}

/// Ein unsichtbar geladener Raum für die Dungeon-Karte: Bild in voller Größe (256×256, 15 Bit) und die
/// Kachel-Eigenschaften (`$7F2000`, 64×64 je Ebene, 2 Ebenen) für genaue Symbole: Truhen, Treppen, Fackeln …
#[derive(Clone)]
pub struct RoomShot {
    pub img: Vec<u16>,
    pub attr: Vec<u8>,
    /// Türen des Raums aus den Tür-Tabellen des Spiels ($1980 Typ, $19A0 Position, $19C0 Richtung):
    /// (Nummer, Typ, Kachel-Position im 64er-Raster, Richtung 0 oben 1 unten 2 links 3 rechts)
    pub doors: Vec<(u8, u8, u16, u8)>,
}

pub fn capture_room_shot(base: &Snes, room_table: usize, eid: u8, room: u16) -> Option<RoomShot> {
    let mut c = base.clone();
    let mut rom = (*c.bus.rom).clone();
    let o = room_table + eid as usize * 2;
    if o + 1 >= rom.len() {
        return None;
    }
    rom[o] = room as u8;
    rom[o + 1] = (room >> 8) as u8;
    c.bus.rom = std::sync::Arc::new(rom);
    c.bus.ppu.ext = 0;
    c.bus.ppu.record = false;
    c.bus.ppu.capture = false;
    c.bus.ppu.skip = true;
    to_target(&mut c, &[], Target { eid, area: 0xFF, x: 0.0, y: 0.0 }, false).ok()?;
    if ram16(&c, 0x7E00A0) != room {
        return None;
    }
    // dunkle Räume: auf BG1 liegt nur der Lichtkegel der Laterne (sonst schwarze Flecken im Bild)
    let dark = ram8(&c, 0x7E009A) & 0x80 != 0;
    let img = c.bus.ppu.room_full_ex(2, dark);
    // für die Symbole noch einmal mit geschlossenen Truhen (geöffnete Truhen stehen nicht mehr in den Kachel-Daten):
    // in einer weiteren Kopie die Truhen-Bits des Raums löschen
    let mut z = base.clone();
    z.bus.rom = c.bus.rom.clone();
    z.bus.ppu.ext = 0;
    z.bus.ppu.record = false;
    z.bus.ppu.capture = false;
    z.bus.ppu.skip = true;
    let fi = 0xF000 + room as usize * 2;
    z.bus.wram[fi] &= !0xF0;
    z.bus.wram[fi + 1] &= !0x03;
    let attr = match to_target(&mut z, &[], Target { eid, area: 0xFF, x: 0.0, y: 0.0 }, false) {
        Ok(()) if ram16(&z, 0x7E00A0) == room => z.bus.wram[0x12000..0x14000].to_vec(),
        _ => c.bus.wram[0x12000..0x14000].to_vec(),
    };
    let w = &c.bus.wram;
    let doors = (0..16usize)
        .map(|i| (i as u8, w[0x1980 + i * 2], (w[0x19A0 + i * 2] as u16 | (w[0x19A1 + i * 2] as u16) << 8) / 2, w[0x19C0 + i * 2]))
        .filter(|d| d.2 != 0)
        .collect();
    Some(RoomShot { img, attr, doors })
}

/// Lädt jedes Oberwelt-Gebiet einmal in einer unsichtbaren Kopie des Spiels (Höhle betreten, mit
/// passenden Rückkehr-Daten wieder hinaus) und schickt Gebietskarte + Grafik zurück.
pub fn capture_areas(base: &Snes, rom: &[u8], skip: &[u8], tx: std::sync::mpsc::Sender<(u8, crate::snes::ppu::AreaData)>) {
    let mut inside = base.clone();
    inside.bus.ppu.ext = 0;
    inside.bus.ppu.record = false;
    inside.bus.ppu.capture = false;
    inside.bus.ppu.skip = true;
    // ein Haus in Kakariko als Ausgangspunkt (sein Ausgang nutzt die Rückkehr-Daten)
    if to_target(&mut inside, rom, Target { eid: 0x4B, area: 0xFF, x: 0.0, y: 0.0 }, false).is_err() {
        return;
    }
    let all_parents = area_parents();
    let ents = crate::world::find_entrances(rom);
    let mut parents: Vec<u8> = all_parents.clone();
    parents.sort();
    parents.dedup();
    for p in parents {
        if skip.contains(&p) {
            continue;
        }
        // bevorzugt wie ein echter Teleport vor einen Eingang des Gebiets: dann lädt das Spiel
        // alle Farben des Gebiets richtig (der Weg über das Kakariko-Haus ließ Farben 9–15 der Zeilen 5–7 stehen)
        if let Some(e) = ents.iter().find(|e| all_parents.get(e.area as usize) == Some(&p)) {
            let mut c = inside.clone();
            // in den Raum des Eingangs und dann zu Fuß hinaus (wie im echten Spiel)
            if to_target(&mut c, rom, Target { eid: e.eid, area: e.area, x: e.x, y: e.y }, false).is_ok() {
                let mut ok = false;
                for _ in 0..90 {
                    c.bus.joy[0] = 0;
                    c.run_frame();
                    c.bus.apu.dsp.out.clear();
                }
                for _ in 0..600 {
                    c.bus.joy[0] = 0x0400;
                    c.run_frame();
                    c.bus.apu.dsp.out.clear();
                    if ram8(&c, 0x7E0010) == 0x09 && ram8(&c, 0x7E0011) == 0 && ram8(&c, 0x7E001B) == 0 {
                        ok = true;
                        break;
                    }
                }
                for _ in 0..64 {
                    c.run_frame();
                    c.bus.apu.dsp.out.clear();
                }
                if ok && ram8(&c, 0x7E040A) == p && ram8(&c, 0x7E0010) == 0x09 {
                    let pa = (p & 0x3F) as i32;
                    let size = if LARGE.contains(&p) { 1024 } else { 512 };
                    let m16: Vec<u16> = (0..4096).map(|i| c.bus.wram[0x2000 + i * 2] as u16 | (c.bus.wram[0x2001 + i * 2] as u16) << 8).collect();
                    let nba = c.bus.ppu.r.bg_nba[1] as usize;
                    let chr: Vec<u16> = (0..0x4000).map(|i| c.bus.ppu.vram[(nba + i) & 0x7FFF]).collect();
                    let data = crate::snes::ppu::AreaData { x0: (pa % 8) * 512, y0: (pa / 8) * 512, size, m16, chr, cgram: c.bus.ppu.cgram.clone() };
                    if tx.send((p, data)).is_err() {
                        return;
                    }
                    continue;
                }
            }
        }
        let mut c = inside.clone();
        c.bus.wram[0xF3CA] = if p >= 0x40 { 0x40 } else { 0x00 };
        let pa = (p & 0x3F) as i32;
        let large = LARGE.contains(&p);
        let size = if large { 1024 } else { 512 };
        let (x0, y0) = ((pa % 8) * 512, (pa / 8) * 512);
        prepare_exit(&mut c, rom, p, (x0 + size / 2) as f32, (y0 + size / 2) as f32);
        let mut ok = false;
        for _ in 0..400 {
            c.bus.joy[0] = 0x0400;
            c.run_frame();
            c.bus.apu.dsp.out.clear();
            if ram8(&c, 0x7E0010) == 0x09 && ram8(&c, 0x7E0011) == 0 {
                ok = true;
                break;
            }
        }
        if !ok || ram8(&c, 0x7E040A) != p {
            continue;
        }
        // noch kurz warten: einige Farbzeilen (5–7, u. a. Büsche) lädt das Spiel erst nach dem Betreten
        for _ in 0..64 {
            c.bus.joy[0] = 0;
            c.run_frame();
            c.bus.apu.dsp.out.clear();
        }
        if ram8(&c, 0x7E040A) != p || ram8(&c, 0x7E0010) != 0x09 {
            continue;
        }
        let m16: Vec<u16> = (0..4096).map(|i| c.bus.wram[0x2000 + i * 2] as u16 | (c.bus.wram[0x2001 + i * 2] as u16) << 8).collect();
        let nba = c.bus.ppu.r.bg_nba[1] as usize;
        let chr: Vec<u16> = (0..0x4000).map(|i| c.bus.ppu.vram[(nba + i) & 0x7FFF]).collect();
        let cgram = c.bus.ppu.cgram.clone();
        let data = crate::snes::ppu::AreaData { x0, y0, size, m16, chr, cgram };
        if tx.send((p, data)).is_err() {
            return;
        }
    }
}
