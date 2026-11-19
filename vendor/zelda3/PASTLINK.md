# Zelda 3 – eingebunden in Zelda Core

Quelle: https://github.com/snesrev/zelda3 (Commit 45a149d8b4bb8098f53d1009f76e76e7d8b79bbf), MIT-Lizenz (siehe `LICENSE.txt`).
Zelda 3 ist eine Neuimplementierung der Spiellogik von „A Link to the Past“ in C, von snesrev und Mitwirkenden.
Zelda Core nutzt sie im RC-Modus (Knopf „RC“) als Spiellogik. Grafik, Breitbild, HD, Karte, Tracker, Cheats und Mods
kommen weiter von Zelda Core.

## Spieldaten (nicht im Git, nicht weitergeben)

`zelda3_assets.dat` wird aus der eigenen **US-ROM** erzeugt, mit deutschen Dialogen aus der eigenen deutschen ROM:

    python -m venv .venv && .venv\Scripts\python -m pip install Pillow PyYAML
    copy "<US-ROM>.sfc" zelda3.sfc
    copy "<DE-ROM>.sfc" zelda3_de.sfc
    .venv\Scripts\python assets/restool.py --extract-dialogue -r zelda3_de.sfc
    .venv\Scripts\python assets/restool.py --extract-from-rom --languages=de -r zelda3.sfc

`cargo build` baut danach den C-Kern (`build.rs`, cc-Crate) und bettet die Spieldaten in die .exe ein. Fehlt
`zelda3_assets.dat`, nutzt der RC-Modus stattdessen den eigenen Recompiler von Zelda Core.

## Änderungen an Zelda 3

- `src/zelda_rtl.c`: `g_Zelda Core_ppu_hook` (meldet jeden PPU-Schreibzugriff an Zelda Core) und
  `ZeldaZelda CoreLines` (wie `ZeldaDrawPpuFrame`, aber zeichnet nicht selbst, sondern ruft je Zeile Zelda Core auf).
- Nicht gebaut: `main.c`, `config.c`, `opengl.c`, `glsl_shader.c` (SDL/Fenster). Ersatz: `src/z3/glue.c` in Zelda Core.

## Ergänzungen (Zelda Core 0.3)

- `src/z3/glue.c`: `pl_z3_read_sram` (eigenständige `zelda3.exe`, Spielstände in `zelda3\saves`) und
  `pl_z3_set_map16` (Map16-Tabelle aus der geladenen ROM; die deutsche Fassung weicht in 853 Einträgen ab).
- Zelda Core setzt vor jedem Bild die PPU-Register, die Zelda 3 nie schreibt (OBSEL, M7SEL, WBGLOG, WOBJLOG, SETINI),
  auf die Startwerte von Zelda 3. Startet man direkt im RC-Modus, beginnt Zelda 3 von vorn, statt den leeren
  Ton-Speicher der Emulation zu übernehmen.
- Öffentliche Builds (`PASTLINK_PUBLIC=1`) bauen den Kern ohne Spieldaten ein; `zelda3_assets.dat` wird dann zur
  Laufzeit neben der .exe gesucht.
- Versuch, die Daten nur aus der deutschen ROM zu erzeugen (ohne US-ROM): 130 von 165 Assets ließen sich exakt
  gleich gewinnen. Die übrigen hängen an Stellen, die die deutsche Revision strukturell geändert hat (Map32-Tabellen,
  Dungeon-Räume, Nachrichten-Nummern, Abspann). Zelda 3 ist für die US-Daten geschrieben und stürzte damit in
  Dungeons ab, deshalb nicht übernommen.
