# Fremde Bestandteile und Lizenzen

Zelda Core steht unter der **GPL-3.0** (siehe [`LICENSE`](LICENSE)), weil Project Picori (GPL-3.0) eingebunden ist.
Die folgenden Bestandteile behalten ihre eigenen Lizenzen.

| Bestandteil | Pfad | Herkunft | Lizenz | Einbindung |
|---|---|---|---|---|
| **Project Picori** (PC-Fassung von The Minish Cap, auf Grundlage der Dekompilierung zeldaret/tmc) | `vendor/tmc` | https://github.com/999sian/tmc | GPL-3.0 | als `tmc_core.dll` (gebaut von `tools/build_tmc.py`); Änderungen: `vendor/tmc/ZELDA_CORE_CHANGES.md`. Picoris eigene Fremdbestandteile (agbplay LGPL-3.0, Randomizer GPL-3.0, Minish Cap Reborn GPL-3.0, Dear ImGui MIT, VirtuaAPU, SDL3 zlib, libpng, zlib, {fmt} MIT, nlohmann/json MIT u. a.): `vendor/tmc/THIRD-PARTY-LICENSES.md` |
| **Dear ImGui** v1.92.7 | `vendor/tmc/libs/imgui` | https://github.com/ocornut/imgui | MIT | in `tmc_core.dll` (Picoris Menüs; in Zelda Core nicht sichtbar) |
| **VirtuaAPU** | `vendor/tmc/libs/VirtuaAPU` | https://github.com/MatheoVignaud/VirtuaAPU | keine Lizenz veröffentlicht (Teil von Picori, siehe dessen THIRD-PARTY-LICENSES.md) | in `tmc_core.dll` |
| **SDL3**, libpng, zlib, libgomp, libstdc++ (msys2/ucrt64) | – | https://libsdl.org u. a. | zlib, libpng, zlib, GPL mit GCC-Laufzeit-Ausnahme | statisch in `tmc_core.dll` |
| **mGBA** 0.10.5 | `vendor/mgba` | https://github.com/mgba-emu/mgba | MPL-2.0 | statisch in `zelda-core.exe` |
| **Zelda 3** | `vendor/zelda3` | https://github.com/snesrev/zelda3 | MIT | statisch in `zelda-core.exe` und `zelda3.exe` |
| **GPUI** (gepatcht) | `vendor/gpui` u. a. | https://github.com/zed-industries/zed | Apache-2.0 | statisch |

Spieldaten (ROMs und alles, was daraus entpackt wird, z. B. der Ordner `minish` neben der .exe) gehören nicht zu
Zelda Core und werden nicht weitergegeben.
