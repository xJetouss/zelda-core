# Zelda Core 0.49 – A Link to the Past & The Minish Cap

Eigener SNES-Emulator in Rust, zugeschnitten auf *The Legend of Zelda: A Link to the Past*.
**Keine ROM enthalten** – du brauchst eine Kopie deines eigenen Moduls (`.sfc`/`.smc`).
Alles steckt in der einen `.exe`: HD-Filter, Randomizer, Karte – nichts wird nachgeladen.

```
cargo build --release
target\release\zelda-core.exe
```

## Start

* **Die App startet immer im Hauptmenü.** Dort stehen die zuletzt gespielte ROM und eine ALttP-ROM neben der `.exe`;
  sonst Strg+O / Datei ins Fenster ziehen. Nur eine beim Start übergebene Datei („Öffnen mit“) startet sofort.
* Das Intro wird standardmäßig übersprungen (direkt zur Spielerauswahl).
* Die Spielsprache ist die der ROM (deutsche ROM = deutscher Text, PAL mit 50 Hz). Die Menüs folgen der Windows-Sprache.
* Alle Einstellungen werden sofort gespeichert (`zelda-core.json` neben der `.exe`).
* **Es wird nichts automatisch gespeichert außer deinen Savestates.** Der Speicherpunkt im Spiel (.srm) wird nur geschrieben,
  wenn du unter Datei „Spielinterne Spielstände speichern“ einschaltest.

## Alles oben in der Menüleiste

| Menü | Inhalt |
|---|---|
| 📁 Datei | ROM öffnen, Patch anwenden, Reset, Auto-Laden |
| 💾 Spielstand | **Spielstände mit Bild** (Popup im Spielstil, 6 Plätze, nur Controller/Tastatur mit Screenshot, **Spielzeit, Tode, Herzen und Ort**), auch per Controller: Steuerkreuz wählen, A laden, Y speichern, X löschen, B schließen |
| ⏱ Emulation | Pause, Tempo, Run-Ahead, Zurückspulen |
| 🖥 Grafik | **HD an/aus**, Filter: **HD weich 6× (xBRZ)**, Clean HD, HD (xBR 4×), HD knackig, HD Retro, HD Kanten, Röhren-TV · **Breitbild 16:9 / 21:9 / Fenster füllen** · **Bildrate** (Monitor, 120–360, unbegrenzt) mit Zwischenbildern · Farben |
| 🔊 Audio | Lautstärke, Latenz, Interpolation, Echo, Kanäle |
| 🎮 Steuerung | Tasten und Hotkeys belegen, Controller |
| ✨ Cheats | Unendlich Herzen/Magie, Noclip, Intro- und Prolog-Skip, **jedes Item einzeln** (inkl. Stufen und Flascheninhalt), Herzen, Rubine, Bomben, Pfeile, Schlüssel, Amulette, Kristalle, Codes |
| 🗺 Karte | **Vollbild-Overlay im Stil eines Remakes** (Pixel-Look im SD-Modus, weich im HD-Modus) – komplett per Controller: **↓ öffnet und schließt**, freies Fadenkreuz rastet auf Orten ein (Name wird eingeblendet), überlappende Symbole werden leicht versetzt, im HD-Modus wird die Karte mit xBRZ hochgerechnet, **L/R** wechselt Lichtwelt → Schattenwelt → Dungeon, linker Stick/Steuerkreuz bewegt, **rechter Stick zoomt**, **A** reist zum Teleport-Punkt, **X** Ansicht, **Y** zu Link (Dungeon: nächster Dungeon), **B** zurück. Unten immer **x % / 100 %**. Reiter **100 %** (Liste aller Dinge mit Häkchen: pro Region, Herzteile, Herzcontainer, Gegenstände, Flaschen, Upgrades, Anhänger & Kristalle, Truhen) und **Quests** (nur Quests, die man jetzt machen kann, und erledigte, A zeigt das Ziel, **Y verfolgt** die Quest – das Ziel der verfolgten (sonst der aktuellen Haupt-) Quest steht als goldene Raute auf der Karte, außerhalb des Ausschnitts zeigt ein Pfeil die Richtung). In der Spoiler-Ansicht stehen auch Flaschen, Upgrades und Startpunkte der Nebenquests auf der Karte. **A auf eine freie Stelle** setzt eine eigene Markierung (5 Symbole, wird mit dem Savestate gespeichert). Im Dungeon-Reiter: eigene Dungeon-Prozentanzeige mit allen Gegenständen. In der Spoiler-Ansicht zeigt ein Dungeon unter dem Fadenkreuz, welche Gegenstände man ungefähr dafür braucht (mit Häkchen). Halbe Magie und mehr Bomben/Pfeile sind freiwillig und zählen nicht für 100 %. Optional weiterhin als **eigenes Fenster** |
| 🎲 Randomizer | **AN/AUS-Schalter** (Standard: AUS), Modi Leicht/Normal/Chaos, Seed, Spoiler |

## Wichtige Hotkeys (alle änderbar)

**Steuerkreuz rechts (Controller) bzw. V halten = Schnellmenü** · **Leertaste halten = ×2** · Tab halten = Turbo · Rücktaste halten = Zurückspulen · F2 Spielstände · F3 Cheats ·
F4 Randomizer · M Karte · F5/F8 Schnellspeichern/-laden · F6/F7 Slot · F9 HD · F11 Vollbild · F12 Screenshot ·
P Pause · Strg+N Noclip · Strg+Umschalt+R Randomizer an/aus · F1 alle Hotkeys

Die Spieltaste „Select“ liegt jetzt auf **C** (die Leertaste ist für ×2 reserviert).

## Hinweise

* Randomizer: Nur „Leicht“ ist garantiert lösbar. „Normal“ und „Chaos“ können die Lösung blockieren – dann hilft Noclip.
  Nach dem Einschalten startet das Spiel neu; am besten ein neues Spiel beginnen.
* Die Weltkarte wird beim ersten Start einmal im Hintergrund aus dem Kartenbildschirm des Spiels erstellt (dauert ~2 s)
  und als `.worldmap` neben der ROM gespeichert. Danach wird sie nur noch geladen.
* **Cheats → „Alles auf 100 % setzen“**: alle Items, 20 Herzen, alle Herzteile, Truhen, Amulette und Kristalle.
* Das Spiel pausiert automatisch, wenn das Fenster nicht aktiv ist oder die Karte offen ist (abschaltbar unter Emulation).
* Auf der Karte sind die 9 Herzteile eingezeichnet, die frei auf der Oberwelt liegen. Herzteile aus Höhlen, von
  Personen oder aus Minispielen sind nur im Zähler „Herzen“ enthalten.

## Switch Pro Controller

Wird automatisch erkannt (eigener Treiber, kein Steam/BetterJoy nötig):

* **USB:** mit einem USB-C-**Datenkabel** anschließen. Viele Ladekabel übertragen keine Daten – dann sieht Windows den
  Controller gar nicht. Wenn er erkannt wird, leuchtet die erste Spieler-LED, und unten erscheint „🎮 Pro“.
* **Bluetooth:** Windows-Einstellungen → Bluetooth → Gerät hinzufügen; am Controller die kleine Sync-Taste oben
  gedrückt halten, bis die LEDs laufen.
* Belegung wie beim SNES (A rechts, B unten, X oben, Y links), + = Start, − = Select, ZR = Turbo, ZL = Zurückspulen,
  rechten Stick drücken = ×2, Home = Spielstände, Aufnahme-Taste = Screenshot.
* Auch der SNES-Controller von Nintendo Switch Online wird unterstützt.

## Schnellmenü (Item-Leiste wie in BotW/TotK)

* Controller: **Steuerkreuz ← oder → halten** = Item für Y, **↑ halten** = Item für X; mit Stick (links/rechts) oder **L/R** wählen, loslassen = ausrüsten.
* Tastatur: **V halten**, Pfeiltasten links/rechts, loslassen. Kurz tippen = Leiste bleibt offen (dann A/Enter = ausrüsten, B = abbrechen). Maus: Symbol anklicken.
* Das Spiel pausiert, solange die Leiste offen ist. Die Symbole (auch jede Flasche mit ihrem Inhalt) werden im Hintergrund aus dem Item-Menü deiner ROM aufgenommen.
* Abschaltbar unter Steuerung (dann läuft Link mit Steuerkreuz rechts wieder nach rechts).

## Komfort (unter Emulation, standardmäßig an)

* **Kein Warnton bei wenig Energie** – stattdessen blinkt ein Herz unten rechts und der Rand pulsiert rot.
* **Feen-Hinweis:** rettet dich eine Fee aus der Flasche, steht kurz da, welche Flasche es war.
* **Kürzerer Anlauf** beim Pegasus-Sprint (12 statt 29 Bilder).
* **Remake-Effekte** (Grafik, einzeln abschaltbar, nur mit den Grafikkarten-Filtern): HD-Wasser, Licht von Fackeln/Feuer, weichere Schatten, Wetter (Regen, Nebel, Wolkenschatten), Partikel (Staub beim Rennen, Funkeln), Wasserfälle.

* **Neues Inventar (Start) im Stil des Spielmenüs:** Gegenstände und Flaschen, Ausgewählt (Y- und X-Feld), Ausrüstung, Fähigkeiten (Heben, Lesen, Ziehen, Reden, Schwimmen, Rennen), Fortschritt (Anhänger, Kristalle, Herzteile, Vorräte, im Dungeon Karte/Kompass/Schlüssel). Item markieren und **Y** oder **X** drücken – es liegt dann auf diesem Slot. **A** auf den Spiegel benutzt ihn sofort. Karte und Inventar gehen nie gleichzeitig auf.
* **X = zweiter Item-Slot:** das X-Feld sitzt direkt unter dem Y-Feld des Spiels. **←** am Steuerkreuz öffnet eine Auswahlleiste für X (wie → für Y).
* **Controller − / Select öffnet die Karte** (Overlay) und schließt sie wieder.
* **Spiegel:** im Inventar mit **A** auf den Spiegel – das Menü schließt sich und Link reist mit der normalen Spiegel-Animation, auch von der Licht- in die Schattenwelt.

* **Mehrere Bomben gleichzeitig:** bis zu 4 statt 2 (ROM-Patch wie im zelda3-Projekt).
* **Töpfe mit dem Schwert zerstören:** in Dungeons, ab dem Master-Schwert (wie mit dem Hammer).

* **Schnellere Dialoge:** Texte erscheinen sofort vollständig.
* **Herzen und Rubine automatisch einsammeln.**
* **Spiegel direkt aus dem Inventar verwenden:** Spiegel im Item-Menü oder Schnellmenü wählen – er wird beim Schließen sofort benutzt.

## Teleport

* Jeder Eingang (Höhle, Haus, Dungeon), den du schon betreten hast, ist ein Teleport-Ziel. Zelda Core liest das aus dem
  Spielstand (besuchte Räume), es funktioniert also auch mit alten Spielständen.
* **Karte im Emulator (M oder ↓ am Controller):** Fadenkreuz in der Mitte auf einen Eingang (türkis = besucht) und **A**
  (oder anklicken). Im Dungeon-Reiter den Eingangsraum mit dem Stick wählen, rechter Stick wechselt das Stockwerk.
* **Ansicht (X):** „Alles“ zeigt jeden Ort, jedes Herzteil und jede Truhe (voller Spoiler); „Gesammelt“ zeigt nur
  Teleport-Punkte, die du schon betreten hast, und Teile, die du schon hast. Der Fortschritt `x % / 100 %` steht immer da.
* **Karte im Spiel (X):** besuchte Orte werden als Punkte eingeblendet; der Ort in der Bildmitte ist ausgewählt (groß),
  **L/R** wechselt, **A** oder **Y** reist hin (oder anklicken).
* Link landet **vor dem Gebäude** (bei Eingängen ohne normale Tür bleibt er drinnen).

## Hohe FPS, Breitbild, weiches HD

* **Über der Monitorfrequenz:** Unter *Grafik → Bildrate* gehen 120 bis 500 und „Unbegrenzt (1000)“ auch dann,
  wenn der Monitor weniger kann; gemessen auf 200 Hz: 500 → ~490 FPS, 1000 → ~900 FPS (kleines Fenster). Das Spiel
  bleibt bei 60 Spielbildern, der Gewinn ist weniger Verzögerung.
* **Zwischenbilder:** Das Spiel selbst läuft mit 60 Bildern/s. Zelda Core zeichnet dazwischen weitere Bilder, in denen
  Scrollen und Figuren interpoliert sind (wie Ship of Harkinian), Standard **200 FPS**. Bildrate unter Grafik
  (Wechsel zu/von „Monitor (VSync)“ wirkt nach Neustart). Bei langsamer Bewegung (1 Pixel pro Spielbild) kann es nicht
  flüssiger werden als das Spiel selbst – schnelle Bewegungen wie Bildschirmwechsel profitieren am meisten.
* **Breitbild 16:9 / 21:9:** zeigt links und rechts mehr von der Welt (die Statusleiste bleibt in der Mitte). Auf der
  Oberwelt lädt das Spiel den Rand teils erst nach – dort können kurz falsche Kacheln auftauchen, Gegner erscheinen erst
  am alten Bildrand.
* **HD läuft auf der Grafikkarte** (HD weich, Clean HD, Original, Weich): Kantenglättung nach xBR-Regeln direkt in
* **HD Ultra (MMPX):** neuer Filter nach McGuire & Gagiu (2021) – vergrößert Pixel-Art in zwei Schritten 2×→4× und hält dabei Linien, Ecken und Formen scharf; Schrägen werden glatt, ohne zu verwischen.
  Bildschirmauflösung – kein Ruckeln beim Bewegen oder im Turbo. Die übrigen Filter laufen weiter auf der CPU.
* **Breitbild / Fenster füllen:** Auf der Oberwelt zeichnet Zelda Core die Ränder direkt aus der Gebietskarte des Spiels Der Kreis-Übergang beim Betreten/Verlassen von Häusern öffnet sich über die ganze Breite (kein kurzes 4:3 mehr). **Echtes Breitbild** (Grafik-Menü, standardmäßig aus): Gegner und Gegenstände leben und erscheinen bis 64 Pixel links und rechts außerhalb von 4:3 (ROM-Patches nach dem Vorbild von zelda3 `ExtendScreen64`).
  (RAM + Kacheltabelle der ROM, mit Selbstprüfung) – vollständig und ohne falsche Bildteile. Hinter dem Gebietsrand bleibt
  es schwarz, weil das Spiel das Nachbargebiet erst beim Übergang lädt. In Räumen ist der ganze Raum geladen.
  Die Statusleiste wird geteilt (Items links, Energie rechts).
* **Turbo:** Nicht angezeigte Bilder werden nicht gezeichnet und jedes Bild hat ein Zeitbudget – das Fenster bleibt flüssig.
* **Pixel-Modus:** Schnellmenü, Teleport-Hinweis, Turbo-Anzeige und Pause in Spielpixeln mit Pixel-Schrift.
  Texturpakete gibt es für SNES-Spiele nicht – alles wird live aus dem Originalbild berechnet.
* **Steuerung:** Drückst du zwei Gegenrichtungen, gewinnt die zuletzt gedrückte (kein kurzes Stehenbleiben beim
  Richtungswechsel); der Stick wird in 8 Richtungen mit etwas breiteren Diagonalen umgesetzt; Run-Ahead 1 ist an.

## Flüssiger & schärfer

* **60 Hz für PAL-ROMs** (Standard): Die deutsche Version läuft original mit 50 Bildern/s, auf 60-Hz-Monitoren ruckelt das.
  Mit 60 Hz läuft sie flüssig, aber etwa 20 % schneller. Abschaltbar unter Emulation.
* **An Monitor anpassen**: Liegt die Bildwiederholrate des Monitors nahe an der Spielrate, wird genau ein Bild pro Monitorbild gezeigt.
* **Clean HD** (Standard-HD): xBR-Kantenglättung ohne Zwischenfarben, scharf dargestellt – keine Unschärfe. Der Filter läuft auf allen CPU-Kernen (unter 1 ms pro Bild).

## Controller-Steuerkreuz

Standardmäßig bewegt das Steuerkreuz am Controller Link nicht (dafür ist der Stick da): **→ halten = Schnellmenü**,
**↑ = Spielstände**, **↓ = Karte**. In Menüs bedient es weiterhin die Auswahl. Abschaltbar unter Steuerung.

## Flüssig

Standard ist „Monitor (VSync)“: Jedes Bild passt genau zur Bildwiederholrate des Monitors (kein Zerreißen, keine
ungleichmäßigen Abstände). Die FPS-Anzeige zeigt beides: Bilder auf dem Monitor und Spielbilder pro Sekunde
(im Turbo z. B. 240). Run-Ahead ist aus (halbiert die Rechenlast).

## Dungeon-Karte

Besuchte Räume bekommen ein Bild des Raums, genommene Treppen werden mit ▲/▼ eingetragen
(gespeichert in `.rooms` neben der ROM).

## RC-Modus mit Zelda 3
Im RC-Modus läuft die Spiellogik aus **Zelda 3** (https://github.com/snesrev/zelda3, MIT-Lizenz, © snesrev und
Mitwirkende) statt der Emulation. Zelda Core zeichnet mit der eigenen PPU, deshalb funktionieren Breitbild, HD,
Zwischenbilder, Karte, Tracker, Cheats und Mods weiter. Die Dialoge sind deutsch.
- Zelda 3 braucht seine Spieldaten `zelda3_assets.dat`. Sie werden **einmal** mit den Werkzeugen von Zelda 3 aus der
  ROM erzeugt (siehe `vendor/zelda3/PASTLINK.md`) und danach nur noch gelesen – eine ROM wird dafür später nicht
  mehr gebraucht. Gesucht wird die Datei neben der .exe, sonst ist sie in eigenen Builds eingebaut
  (`vendor/zelda3/zelda3_assets.dat`). Die Datei enthält Spieldaten: nicht weitergeben.
- Die Kachel-Tabelle (Map16) kommt aus der geladenen deutschen ROM, damit Oberwelt-Details wie der Wetterhahn in
  Kakariko so aussehen wie in der Emulation.
- Umschalten RC ↔ EMU geht mitten im Spiel. HUD- und Schrift-Grafik werden zwischen deutscher und US-Belegung
  umgesetzt, der Ton wird übernommen.
- Spielstände aus dem RC-Modus werden im Format der Emulation gespeichert. Sie laden also in beiden Modi.
- Startest du direkt im RC-Modus, geht es erst nach dem Laden eines Spielstands zurück zur Emulation.
- Im RC-Modus ist Run-Ahead aus. Mod-Hooks an Spielroutinen gibt es dort nicht; alles andere der Mod-Schnittstelle
  (Speicher, Grafik, Farben, Bilder, Töne, Tempo) funktioniert.

## Game Boy Advance: The Minish Cap
Liegt eine GBA-ROM neben der .exe (oder wird über „ROM öffnen…“ geladen), erscheint sie als eigene Karte im
Hauptmenü – mit dem Titelbild samt Logo aus dem Spiel selbst. Emuliert wird mit **mGBA** (MPL-2.0, `vendor/mgba`).
Es gibt dieselben Emulator-Funktionen wie bei A Link to the Past, ohne Mods, Karte und Inventar (Recomp siehe unten):
- **Home / Switch-Home:** Spielstand-Menü mit Vorschaubildern, „Hauptmenü“ und „Einstellungen“ (wie bei ALttP).
  Spielstände liegen als `.gs1` … neben der ROM, der Speicherstand des Spiels automatisch als `.sav`.
- **Emulation:** Pause, Einzelbild vor, Reset, Tempo 50–300 %, ×2 und Turbo (halten), Zurückspulen, Auto-Pause.
- **Grafik:** alle Upscaler (MMPX, xBRZ, Super-xBR, xBR, ScaleFX, Clean HD, Röhren-TV …), HD-Knopf oben rechts,
  16:9 / 21:9 / Fenster füllen, Bildrate bis 1000, Überblenden, Sättigung, Gamma, ganzzahlige Skalierung.
- **Y-Taste (neu):** ein drittes Item auf Y, im Spiel als eigenes Feld links neben B – genau im Stil der B/A-Knöpfe.
  Item wählen wie bei ALttP: Schnellmenü-Taste halten (Tastatur V, Controller Steuerkreuz rechts), mit ←/→ oder
  L/R wählen, loslassen. Y benutzt das Item (es liegt dafür kurz auf B, danach ist B wieder wie vorher).
- **Cheats:** fertige Cheats (20/10 Herzen unendlich, 999 Rubine, 999 Muscheln, 99 Bomben, 99 Pfeile, alle Items
  und 4 Flaschen, alle Schwerttechniken – im Spiel nachgeprüft) und eigene Codes (GameShark, Action Replay,
  CodeBreaker, „Adresse:Wert“).

### Recomp: The Minish Cap als PC-Fassung (Project Picori) mit echtem 16:9
Mit `tmc_core.dll` neben der .exe läuft The Minish Cap (Europa, USA, Japan) nicht mehr emuliert, sondern als
PC-Fassung **Project Picori** (https://github.com/999sian/tmc, GPL-3.0). Die beruht auf der Dekompilierung zeldaret/tmc.
Schalter: *Bild → Recomp (Picori)* (Standard an; aus = mGBA).
- **Echtes Breitbild:** Bei 16:9, 21:9 oder „Fenster füllen“ zeigt das Spiel draußen wirklich mehr von der Welt
  (bis 384 Pixel breit statt 240). Enge Räume, Menüs und Zwischensequenzen bleiben 240 Pixel breit, mit Rand.
- Alles wie bei mGBA und bei A Link to the Past: Savestates mit Vorschaubild (`.rs1` …, gepackt, getrennt von den
  mGBA-Ständen `.gs1`), Pause, Einzelbild, ×2/Turbo, Zurückspulen, Cheats (CodeBreaker, `Adresse:Wert`; GameShark
  und Action Replay nur mit mGBA), Y-Taste mit Item-Leiste und alle Upscaler/HD-Filter.
- **Eigene Karte mit 100 %** (wie bei A Link to the Past), nur mit Minus/Karte-Taste. Reiter OBERWELT,
  DUNGEON/RÄUME, 100 %, QUESTS und FUNDORTE (Herzteile, Truhen, Kinsteine, Graben, Unter Wasser, Rubine, Items,
  Gebiete) mit „schon geholt“. Die Karte ist von Anfang an vollständig: Beim ersten Öffnen besucht Zelda Core
  einmalig jeden Raum und fotografiert ihn (etwa eine Minute, danach ist der Spielstand genau wie vorher). Gespeichert
  wird sie als `<rom>.tmcmap` neben der ROM – enthält Spielgrafik, nicht weitergeben. Das Item-Menü hat dann keine
  Kartenseite mehr, L/R wechselt zwischen Items und Quest-Status. Abschaltbar unter *Spiel*.
  Bedienung wie bei A Link to the Past: Fadenkreuz mit dem Stick (schiebt die Karte am Rand mit), rechter Stick
  zoomt, Y wechselt die Anzeige (Übersicht, Sammeln, Schnellreise, Alles), X zeigt alles oder nur Gesammeltes, unter
  DUNGEON/RÄUME wechselt Y den Raum. Eingezeichnet sind auch Schrumpfpunkte, Windwappen, Bohnenranken und jede Tür
  (Häuser, Cafés, Höhlen, Dungeons) als Reisepunkt; unbesuchte erscheinen grau bei X = alles. Anzeige „Eigene“:
  jede Art einzeln an/aus. Das Ziel der nächsten Hauptquest ist markiert. Im HD-Bildmodus wird die Karte geglättet.
- Cheats der PC-Fassung (Menü *Cheats*): durch Wände gehen und „100 %: alles holen“.
  **Schnellreise:** Jeder besuchte Raum ist ein Reisepunkt, A reist hin (Link landet auf freiem Boden).
  **Eigene Markierungen:** A auf eine freie Stelle, ←/→ Symbol, A setzen; A auf eine Markierung entfernt sie
  (gespeichert in `<rom>.tmcpins`). Die Karte kann auch im eigenen Fenster laufen (*Spiel → Karte im eigenen Fenster*).
- Der Spielstand des Spiels ist dieselbe `.sav` wie bei mGBA (gleiches Format). Sie wird nur geschrieben, wenn
  „Spielinterne Spielstände speichern“ an ist, sonst speichert man nur über Savestates.
- Beim **ersten Start** entpackt Picori Grafiken und Töne aus deiner ROM in den Ordner `minish` neben der .exe
  (etwa 20 Sekunden). Dieser Ordner enthält Spieldaten: nicht weitergeben.
- Picori lässt sich pro Programmlauf nur einmal starten. Ein erneuter Start oder ein Reset springt deshalb auf den
  Zustand direkt nach dem Start zurück. Für eine andere Minish-Cap-ROM muss Zelda Core neu gestartet werden.
- Bauen: `python tools/patch_tmc.py` (Anpassungen, einmal), dann `python tools/build_tmc.py target/release`
  (braucht msys2 mit ucrt64-GCC, SDL3, libpng und zlib unter `C:\msys64\ucrt64`).

## Reines Zelda 3 (zelda3.exe, Knopf „Z3“)

Mit Zelda Core wird zusätzlich `zelda3.exe` gebaut: Zelda 3 pur, ohne Zelda Core-Funktionen, in einem eigenen
Fenster. Oben links stehen die angezeigten Bilder und die Spielbilder pro Sekunde. Der Knopf **Z3** in der
Menüleiste startet es und hält Zelda Core an.

- Steuerung wie bei Zelda 3: Pfeiltasten, X = A, Z/Y = B, S = X, A = Y, C = L, V = R, Enter = Start,
  rechte Umschalttaste = Select. Controller werden erkannt.
- **Tab** gedrückt halten: Spiellogik ohne Bremse. Dann steigen die Spielbilder pro Sekunde, und das ganze
  Spiel läuft schneller. Deshalb läuft die Spiellogik auch in Zelda 3 fest mit 60 Bildern pro Sekunde.
- Spielstände des Spiels liegen in `zelda3\saves\sram.dat` neben der .exe, getrennt von Zelda Core.
- Ohne eingebaute Spieldaten (öffentlicher Build) sucht `zelda3.exe` die Datei `zelda3_assets.dat` neben sich.

## Recompiled-Modus (übersetzter Code statt Emulation)
Knopf **RC/EMU** oben rechts neben SD/HD (auch unter *Grafik → Bildrate*). Im RC-Modus kommen Code und Spieldaten
aus der `.exe` selbst – die ROM-Datei wird nicht gelesen und muss nicht mehr vorhanden sein. **Diese `.exe` enthält
dann die Spieldaten deiner ROM: nur selbst benutzen, nicht weitergeben.** Der 65C816-Code der ROM wird vom eigenen Recompiler
(`src/recomp/gen.rs`) beim Bauen nach Rust übersetzt. Code, der nicht übersetzt ist, führt automatisch der
Emulator aus. Ergebnis und Zeitverhalten sind bitgenau dieselben wie bei der Emulation (Test
`dev_recomp_lockstep`). **Spielstände sind in beiden Modi dieselben** und lassen sich jederzeit hin und her laden.
Der Gewinn ist klein (~7 % pro Bild), weil die meiste Zeit in Grafik und Ton steckt, nicht in der CPU.

Übersetzten Code einbauen (einmal, oder nach ROM-/Einstellungsänderungen):
1. Code aufzeichnen (spielt Vorspann, Spielstart und Kopien der Spielstände mit Zufallseingaben):
   `PASTLINK_ROM=<rom.sfc> PASTLINK_SETTINGS=<zelda-core.json> PASTLINK_STATES=<Ordner mit .ss1-Kopien> cargo test --release --target-dir target/test dev_recomp_trace -- --ignored`
   → schreibt `recomp/rom.bin` und `recomp/trace.txt` (nicht im Git, enthalten ROM-Daten).
2. `cargo build --release` übersetzt den aufgezeichneten Code (Meldung „Recompiler: … Blöcke“).
Ohne `recomp/` baut alles wie bisher, der Schalter ist dann ausgegraut.

## Mods (in Rust, nur im RC-Modus)
In der App steckt keine Mod. Menü *Mods* → „Mod-Liste…“ öffnet ein eigenes Fenster mit allen Mods (aus dem
GitHub-Release „mods“ dieses Repositorys): „Holen“ bzw. „Alle holen“ installiert sie in den Ordner `mods` neben der .exe,
der Schalter daneben schaltet sie an und aus. Neue Versionen erscheinen dort als „Update“.

In der Mod-Liste (Quelltext in `mods/`, jede Mod eine eigene DLL):

| Mod | Was sie macht |
|---|---|
| Gegner-Lebensbalken | Lebensanzeige über jedem Gegner |
| Speedrun-Timer | Spielzeit mit Zwischenzeiten für Amulette und Kristalle |
| Info-Anzeige | Position, Gebiet/Raum, Gegnerzahl, Lag-Bilder |
| Zeitlupe (L halten) | halbe Geschwindigkeit, solange L gedrückt ist |
| Doppelter Schaden / Ein-Treffer-Modus | schwerere Spielvarianten |
| Doppelte Rubine, Herz- und Magie-Regeneration, Voller Vorrat | Komfort |
| Tag & Nacht | Tageszeit mit Abend und Nacht auf der Oberwelt, Uhr oben rechts |
| Regenbogen-Tunika | Links Tunika wechselt die Farbe (Sprite-Palette) |
| Bunte Gegner | Gegner und Figuren schillern (Sprite-Paletten) |
| Königskrone | Bild aus einer PNG-Datei folgt Link (Bild-Schnittstelle) |
| Münz-Klang | eigener Ton bei jeder Rubine (Ton-Schnittstelle) |

**Eigene Mods** sind Rust-DLLs (Schnittstelle `assets/mod_template/src/api.rs`, Version 3). „Mod-Vorlage
exportieren…“ legt ein Beispielprojekt mit Doku an. Eine Mod darf im RC-Modus:
- RAM, SRAM, VRAM, Paletten, Sprites (OAM) und die ROM im Speicher lesen und schreiben,
- Grafik-Kacheln und Farben dauerhaft austauschen (`vram_patch`, `cgram_patch`, PNG → SNES-Kacheln mit `png_tiles`),
- Bilder (PNG/RGBA), Rechtecke und Text über dem Spielbild zeichnen,
- Töne abspielen (PCM, WAV-Hilfe `wav_pcm`),
- das Spieltempo ändern, Eingaben ersetzen und (im eigenen Recompiler) sich per Hook vor Spielroutinen hängen.

Die Mods der Liste baust du selbst mit `cargo build --release` im Ordner `mods/`.

## Öffentlicher Build und Release
Releases auf GitHub enthalten **keine Spieldaten**: Sie werden mit `PASTLINK_PUBLIC=1` gebaut, ohne eingebaute ROM,
ohne übersetzten ROM-Code und ohne Zelda-3-Daten. Du brauchst deine eigene ROM. Für den RC-Modus legst du deine
selbst erzeugte `zelda3_assets.dat` neben `zelda-core.exe`.

```powershell
$env:PASTLINK_PUBLIC = "1"; cargo build --release --target-dir target/public
```

## Lizenz
GPL-3.0 (siehe `LICENSE`), weil Project Picori (GPL-3.0) eingebunden ist. Fremde Bestandteile und ihre Lizenzen:
`THIRD-PARTY-LICENSES.md`. ROMs und daraus entpackte Spieldaten gehören nicht dazu.
