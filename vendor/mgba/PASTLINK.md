# mGBA 0.10.5 – Kopie für Zelda Core (GBA-Modus)

Quelle: https://github.com/mgba-emu/mgba, Tag `0.10.5`, Mozilla Public License 2.0 (siehe `LICENSE`).
Zelda Core baut daraus mit CMake den libretro-Kern als **statische** Bibliothek (`build.rs`, Funktion `mgba`) und
spricht ihn über die libretro-Schnittstelle an (`src/gba/core.rs`).

## Änderungen

1. `CMakeLists.txt`: `add_library(${BINARY_NAME}_libretro SHARED …)` → `STATIC`, damit der Kern in `zelda-core.exe`
   steckt (keine zusätzliche DLL).
2. Ordner `cinema/` (Testdaten, 33 MB) entfernt – wird beim Bauen nicht gebraucht.

Gebaut wird nur der GBA-Kern (`M_CORE_GB=OFF`), ohne Qt, SDL, FFmpeg, PNG, zlib, Lua und Debugger. Ein BIOS ist nicht
nötig (eingebauter Ersatz); `gba/gba_bios.bin` neben der .exe wird verwendet, falls vorhanden.
