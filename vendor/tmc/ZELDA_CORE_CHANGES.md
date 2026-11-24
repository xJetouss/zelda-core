# Änderungen für Zelda Core

Diese Kopie von Project Picori (https://github.com/999sian/tmc, GPL-3.0) wurde für Zelda Core angepasst
(November 2026). Alle Änderungen macht `tools/patch_tmc.py` im Zelda-Core-Projekt. Sie stehen hinter
`#ifdef TMC_CORE_DLL` und gelten nur im Build als `tmc_core.dll` (`tools/build_tmc.py`). Das normale Picori bleibt
unverändert.

- `port/port_main.c`: `main` heißt `Port_CoreMain` und wird von Zelda Core in einem eigenen Thread gestartet.
  Ohne Sprachausgabe beim Start und ohne Update-Prüfung übers Netz.
- `port/port_bios.c`: `VBlankIntrWait` gibt jedes fertige Bild an Zelda Core ab und übernimmt dessen Tasten. Takt,
  Turbo, Pause und Zurückspulen steuert Zelda Core.
- `port/port_ppu.cpp`: Das fertige Bild geht an Zelda Core (eigene Filter und Anzeige). Die Breite der Spielansicht
  folgt dem Seitenverhältnis, das Zelda Core vorgibt. Hintergrundebenen lassen sich ausblenden (Item-Symbole).
- `port/port_audio.c`: Kein eigenes Audiogerät. Zelda Core holt den gemischten Ton je Spielbild ab.
- `port/port_save.c`: Der Spielstand liegt als `<rom>.sav` neben der ROM (mGBA-Format) und wird nur geschrieben,
  wenn Zelda Core es erlaubt. Pfade dürfen bis 1024 Zeichen lang sein.
- `port/port_rom.c`: Keine einzelnen ROM-Seiten unter `rom_data/` lesen oder schreiben (die ganze ROM liegt im
  Speicher). Das spart bei jedem Start mehrere Sekunden.
- `port/port_exe_path.cpp`: Entpackte Grafiken und Töne liegen im Arbeitsordner (`minish` neben `zelda-core.exe`).
- `src/menu/pauseMenu.c`: Blättert man im Pausenmenü zur Kartenseite, meldet das Spiel das an Zelda Core und
  bleibt auf der aktuellen Seite; Zelda Core zeigt dann seine eigene Karte (abschaltbar).
- `port/port_ppu.cpp` (Karte): Das letzte Bild lässt sich ohne Figuren und Anzeige noch einmal zeichnen.
- `port/port_quicksave.c`: Funktionen für Savestates im Speicher (in Zelda Core derzeit nicht benutzt; Savestates
  sichern dort alle Spielvariablen über die Abschnitte `.tmcd`/`.tmcb`).

Hinzugefügt: `libs/VirtuaAPU` (Submodul-Inhalt) und `libs/imgui` (Dear ImGui v1.92.7), weil das Git-Repository
nicht mitkopiert wurde.
