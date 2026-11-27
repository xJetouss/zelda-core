# Zelda Core – Mod-Vorlage (Rust)

Mods für Zelda Core sind Rust-Bibliotheken (DLL). Sie laufen im **Recompiled-Modus** (Knopf „RC“ oben rechts)
und dürfen dort fast alles:

- pro Spielbild Eingaben, RAM, VRAM, Paletten (CGRAM), Sprites (OAM) und sogar die ROM im Speicher ändern,
- sich mit **Hooks** vor Spielroutinen hängen: Register lesen/ändern oder die Routine überspringen,
- über dem Spielbild zeichnen (Rechtecke, Text),
- das Spieltempo ändern (Zeitlupe, Zeitraffer),
- Grafik-Kacheln (Sprites, Hintergründe) und Farben dauerhaft austauschen, auch aus PNG-Bildern,
- Bilder (PNG) über dem Spiel zeichnen und eigene Töne abspielen (WAV/PCM).

## Schnellstart

1. Rust installieren: <https://rustup.rs>
2. In diesem Ordner: `cargo build --release`
3. `target/release/mein_mod.dll` in den Ordner **`mods`** neben `zelda-core.exe` kopieren
   (Menü „Mods“ → „Mod-Ordner öffnen“).
4. In Zelda Core: „RC“ einschalten, Menü „Mods“ → „Mods neu laden“, dann die Mod anhaken.

## Aufbau

- `src/api.rs`: die Schnittstelle (nicht ändern; dieselbe Datei steckt in Zelda Core, Version 4).
- `src/lib.rs`: deine Mod. Sie exportiert `pastlink_mod()` und liefert eine `ModInfo` mit Name,
  Beschreibung, `pre_frame` / `post_frame` und optional Hook-Adressen + `on_hook`.

```rust
extern "C" fn post_frame(api: *const Api) {
    let g = Game(unsafe { &*api });
    if g.in_game() {
        g.text(4, 4, &format!("Rubine: {}", g.read16(0x7EF360)), 0xFFFFFFFF);
    }
}
```

### Funktionen von `Game`

| Funktion | Bedeutung |
|---|---|
| `frame()` | Spielbild-Zähler |
| `read8/read16/write8/write16(a)` | SNES-Adresse: WRAM `0x7E0000–0x7FFFFF`, SRAM `0x700000–0x70FFFF` |
| `mem_read/mem_write(space, a)` | `space::VRAM`, `CGRAM`, `OAM`, `ROM` (Dateioffset), `BUS` |
| `reg/set_reg(reg::A …)` | CPU-Register (A, X, Y, SP, DP, DB, PB, PC, P), vor allem in `on_hook` |
| `ret(long)` | in `on_hook`: Routine sofort verlassen (RTS bzw. RTL) |
| `pad()` / `set_pad(p)` | Controller 1 lesen / für dieses Bild ersetzen (in `pre_frame`) |
| `rect(x,y,w,h,rgba)` / `text(x,y,s,rgba)` | über dem Bild zeichnen (SNES-Pixel 256×224, Farbe `0xRRGGBBAA`) |
| `set_speed(f)` | Spieltempo für dieses Bild (0.5 = Zeitlupe, 2.0 = doppelt) |
| `in_game()` | Link steht im normalen Spiel (kein Menü, kein Übergang) |

### Ab Version 3: Grafik, Bilder, Töne

| Funktion | Bedeutung |
|---|---|
| `image_png(bytes)` → Nummer | PNG einmal anmelden (z. B. `include_bytes!("bild.png")`) |
| `image(id, x, y, w, h, tint)` | angemeldetes Bild zeichnen (`w`/`h` = 0: Originalgröße) |
| `play_sound(pcm, kanäle, rate, lautstärke)` / `play_wav(bytes, lautstärke)` | Ton abspielen |
| `vram_patch(wortadresse, daten)` | Grafikspeicher dauerhaft ersetzen (jedes Bild vor dem Zeichnen angewendet) |
| `cgram_patch(index, farben)` | Farben dauerhaft ersetzen (BGR555, z. B. `bgr555(0xFF8000)`) |
| `png_tiles(bytes, bpp)` → (Kacheln, Palette) | PNG in SNES-Kacheln umwandeln, dann mit `vram_patch` einsetzen |
| `patch_clear()` | eigene Grafik-/Farbänderungen entfernen |

### Ab Version 4: The Minish Cap und Mod-Icon

Eine Mod kann auch in **The Minish Cap** laufen. Dazu exportiert sie, für welche Spiele sie gedacht ist
(ohne diesen Export gilt sie nur für A Link to the Past):

```rust
#[no_mangle]
pub extern "C" fn pastlink_mod_games() -> u32 {
    api::GAMES_ALTTP | api::GAMES_MINISH
}
```

| Funktion | Bedeutung |
|---|---|
| `game()` / `is_minish()` | welches Spiel gerade läuft (`GAME_ALTTP`, `GAME_MINISH`) |
| `screen()` | Zeichenfläche in Spielpixeln (A Link to the Past 256×224, The Minish Cap 240×160, im Breitbild breiter) |
| `read8/…(a)` in The Minish Cap | GBA-Adressen: EWRAM `0x02000000…`, IWRAM `0x03000000…`, Paletten `0x05000000…`, VRAM `0x06000000…`, OAM `0x07000000…` |
| `api::minish::…` | wichtige Adressen (Leben, Rubine, Bomben, Pfeile …) und Zusatz-Infos der App (`INFO_IN_GAME`, `INFO_AREA`, `INFO_LINK_SX` …) |
| `mc_in_game()` / `mc_link_on_screen()` / `mc_area_room()` | bequemer Zugriff auf diese Infos |

In The Minish Cap gibt es keine Hooks, Register, ROM-Zugriffe und kein `vram_patch`/`cgram_patch`.

**Mod-Icon:** Mit `pastlink_mod_icon` zeigt die Mod-Liste ein eigenes Bild (PNG, am besten 32×32):

```rust
static ICON: &[u8] = include_bytes!("../icon.png");
#[no_mangle]
pub extern "C" fn pastlink_mod_icon(len: *mut u32) -> *const u8 {
    unsafe { *len = ICON.len() as u32 };
    ICON.as_ptr()
}
```
| `log(text)` | Meldung in Zelda Core anzeigen |
| `link_on_screen()` | Links Position auf dem Bildschirm |

Links Sprite-Palette ist Reihe 7 (Farben `0xF0..0xFF`), Gegner nutzen die Reihen 1–6 (`0x90..0xEF`). Die
Originalfarben stehen im Palettenpuffer des Spiels ab `0x7EC500` (je Farbe 2 Byte).

Tastenbits: B `0x8000`, Y `0x4000`, Select `0x2000`, Start `0x1000`, ↑ `0x0800`, ↓ `0x0400`,
← `0x0200`, → `0x0100`, A `0x0080`, X `0x0040`, L `0x0020`, R `0x0010`.

### Hooks

`hooks` enthält 24-Bit-Adressen von Spielroutinen (z. B. `0x0080C9` = NMI-Handler, steht im ROM-Vektor
`0x7FEA`). `on_hook` läuft jedes Mal, bevor das Spiel diese Adresse ausführt. Hooks greifen am Anfang von
Routinen und Sprungzielen (JSR/JSL), also da, wo eine Routine betreten wird.

### Nützliche RAM-Adressen (A Link to the Past)

| Adresse | Inhalt |
|---|---|
| `0x7E0010` / `0x7E0011` | Spielmodus (`0x07` Dungeon, `0x09` Oberwelt, `0x0E` Menü/Text, `0x12` Tod) / Untermodus |
| `0x7E001A` | Bildzähler des Spiels |
| `0x7E0022` / `0x7E0020` | Link X / Y (16 Bit) |
| `0x7E00E2` / `0x7E00E8` | Kamera X / Y (16 Bit) |
| `0x7E0373` | Schaden, den das Spiel als Nächstes abzieht (≥ Herzen = Tod) |
| `0x7E0DD0+i` | Sprite i (0–15) Zustand (`0x09` = aktiv) |
| `0x7E0E20+i` / `0x7E0E50+i` | Sprite-Typ / Leben |
| `0x7E0D10+i`,`0x7E0D30+i` / `0x7E0D00+i`,`0x7E0D20+i` | Sprite X (lo, hi) / Y (lo, hi) |
| `0x7EF360` | Rubinen (16 Bit) |
| `0x7EF36C` / `0x7EF36D` | Herzen max. / aktuell (8 = ein Herz) |
| `0x7EF36E` | Magie (0–0x80) |
| `0x7EF343` / `0x7EF377` | Bomben / Pfeile |
| `0x7EF359` | Schwert (0–4) |
| `0x7EF374` / `0x7EF37A` | Amulette / Kristalle (Bits) |

## Regeln

- Eine Mod muss schnell sein (sie läuft 60-mal pro Sekunde) und darf nicht abstürzen. Eine Panic
  über die C-Grenze beendet Zelda Core.
- Was eine Mod im RAM ändert, landet im Spielstand. Zustand in der Mod selbst (Variablen) nicht.
- Paletten/VRAM lädt das Spiel oft selbst neu. Wer sie dauerhaft ändern will, schreibt sie jedes Bild
  in `post_frame` (oder ändert die Quelle im WRAM/ROM).
