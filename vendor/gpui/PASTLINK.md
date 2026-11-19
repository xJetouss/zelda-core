# GPUI 0.2.2 – lokale Kopie für pastlink

Unveränderte Kopie von `gpui 0.2.2` (crates.io, Apache-2.0, siehe `LICENSE-APACHE`). Beispiele, Doku und
Tests wurden entfernt. Eingebunden über `[patch.crates-io]` in der `Cargo.toml` des Projekts.

## Änderungen

1. `src/platform/windows/directx_atlas.rs`: Große Farb-Texturen (größer als die Standard-Atlasgröße
   1024×1024), die gerade frei geworden sind, werden aufgehoben (höchstens 4) und für ein neues Bild
   derselben Größe wiederverwendet.
   Grund: pastlink übergibt das Spielbild bei hoher Bildrate in jedem Anzeige-Bild neu. Bei großem Fenster
   legte GPUI dafür jedes Mal eine neue Direct3D-Textur an. Dadurch dauerte jedes dieser Bilder ~12 ms statt
   5 ms, und 200 Hz waren nicht zu halten.
2. `src/platform/windows/platform.rs` und `src/gpui.rs`: `gpui::pastlink_set_frame_rate(fps)`. Bei 0
   (Standard) wartet der Bildtakt-Thread wie bisher auf den Monitor (DwmFlush). Sonst gibt ein eigener,
   genauer Timer den Takt vor (grob schlafen, das letzte Stück genau abwarten). So sind feste Bildraten auch
   über der Monitorfrequenz möglich, etwa 500 auf einem 200-Hz-Monitor; Present wartet ohnehin nicht.
3. `src/taffy.rs`: `0.0`/`1.0` als `f32` geschrieben. Das beseitigt nur eine Compiler-Warnung, das Verhalten
   bleibt gleich.

Mit einer neueren GPUI-Version, die Bilder ohne neue Textur austauschen kann, lässt sich der Patch entfernen:
`[patch.crates-io]` in der `Cargo.toml` und diesen Ordner löschen.
