// pastlink <-> Zelda 3 (https://github.com/snesrev/zelda3, MIT-Lizenz, siehe vendor/zelda3/LICENSE.txt).
// Ersetzt main.c/config.c von Zelda 3: kein Fenster, keine Dateien – pastlink liefert Spieldaten, Eingaben,
// Grafik- und Tonausgabe.
#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "types.h"
#include "variables.h"
#include "zelda_rtl.h"
#include "config.h"
#include "assets.h"
#include "audio.h"
#include "snes/ppu.h"
#include "snes/dma.h"

// --- was sonst main.c/config.c bereitstellen
Config g_config;
const uint8 *g_asset_ptrs[kNumberOfAssets];
uint32 g_asset_sizes[kNumberOfAssets];

MemBlk FindInAssetArray(int asset, int idx) {
  return FindIndexInMemblk((MemBlk) { g_asset_ptrs[asset], g_asset_sizes[asset] }, idx);
}

static uint8 *g_assets_copy;

// Spieldaten (zelda3_assets.dat) aus dem Speicher; 0 = ok
int pl_z3_load_assets(const uint8 *data, size_t length) {
  static const char kAssetsSig[] = { kAssets_Sig };
  if (length < 16 + 32 + 32 + 8 + kNumberOfAssets * 4 || memcmp(data, kAssetsSig, 48) != 0 ||
      *(uint32 *)(data + 80) != kNumberOfAssets)
    return 1;
  free(g_assets_copy);
  g_assets_copy = (uint8 *)malloc(length);  // Zelda 3 patcht einzelne Tabellen zur Laufzeit
  memcpy(g_assets_copy, data, length);
  uint32 offset = 88 + kNumberOfAssets * 4 + *(uint32 *)(g_assets_copy + 84);
  for (size_t i = 0; i < kNumberOfAssets; i++) {
    uint32 size = *(uint32 *)(g_assets_copy + 88 + i * 4);
    offset = (offset + 3) & ~3;
    if ((uint64)offset + size > length)
      return 2;
    g_asset_sizes[i] = size;
    g_asset_ptrs[i] = g_assets_copy + offset;
    offset += size;
  }
  return 0;
}

// Start: Kern anlegen, Sprache („de“, „us“ …); keine Zusatzfunktionen von Zelda 3 (Original-Verhalten)
void pl_z3_init(const char *language) {
  memset(&g_config, 0, sizeof(g_config));
  g_config.audio_freq = 32000;
  g_config.audio_channels = 2;
  ZeldaInitialize();
  g_zenv.ppu->extraLeftRight = 0;
  ZeldaEnableMsu(false);
  ZeldaSetLanguage(language);
}

// ein Spielbild; `snes_pad` im Format von $4218 (B Y Sel St ↑ ↓ ← → A X L R …)
void pl_z3_run_frame(uint16 snes_pad) {
  uint16 rev = 0, p = snes_pad;
  for (int i = 0; i < 16; i++, p >>= 1)
    rev = rev * 2 + (p & 1);
  ZeldaRunFrame(rev);
}

uint8 *pl_z3_ram(void) { return g_zenv.ram; }
uint8 *pl_z3_sram(void) { return g_zenv.sram; }
uint16 *pl_z3_vram(void) { return g_zenv.ppu->vram; }
uint16 *pl_z3_cgram(void) { return g_zenv.ppu->cgram; }
uint16 *pl_z3_oam(void) { return g_zenv.ppu->oam; }

// Ton für ein Bild (Stereo, 16 Bit, `samples` Abtastwerte je Kanal)
void pl_z3_audio(int16 *buf, int samples) { ZeldaRenderAudio(buf, samples, 2); }

// Bild mit dem eigenen Renderer von Zelda 3 (nur für Tests), 32 Bit XRGB
void pl_z3_draw(uint8 *pixels, size_t pitch) { ZeldaDrawPpuFrame(pixels, pitch, 0); }

// Neustart (Spielstand-RAM bleibt)
void pl_z3_reset(void) { ZeldaReset(true); }

// --- weitere Stücke aus main.c: pastlink ruft den Kern nur aus einem Thread auf, Sperren sind nicht nötig
void ZeldaApuLock(void) {}
void ZeldaApuUnlock(void) {}

void NORETURN Die(const char *error) {
  fprintf(stderr, "Zelda 3: %s\n", error);
  abort();
}

// --- Brücke zu pastlinks PPU (siehe pastlink-Patch in zelda_rtl.c)
#include "spc_player.h"
#include "snes/dsp.h"
extern void (*g_pastlink_ppu_hook)(uint8 reg, uint8 val);
void ZeldaPastlinkLines(void (*line_cb)(int line));
void ZeldaRestoreMusicAfterLoad_Locked(bool is_reset);

void pl_z3_set_ppu_hook(void (*hook)(uint8 reg, uint8 val)) { g_pastlink_ppu_hook = hook; }
void pl_z3_lines(void (*line_cb)(int line)) { ZeldaPastlinkLines(line_cb); }

// Ton-Zustand übernehmen (beim Umschalten aus der Emulation): SPC-RAM + DSP-Register
void pl_z3_import_apu(const uint8 *ram, const uint8 *dsp_regs) {
  SpcPlayer *p = g_zenv.player;
  memcpy(p->ram, ram, 65536);
  for (int i = 0; i < 128; i++)
    dsp_write(p->dsp, i, dsp_regs[i]);
  ZeldaRestoreMusicAfterLoad_Locked(false);
}

// Ton-Zustand ausgeben (beim Umschalten zurück in die Emulation)
void pl_z3_export_apu(uint8 *ram, uint8 *dsp_regs, uint8 *ports_to_spc) {
  SpcPlayer *p = g_zenv.player;
  SpcPlayer_CopyVariablesToRam(p);
  memcpy(ram, p->ram, 65536);
  for (int i = 0; i < 128; i++)
    dsp_regs[i] = dsp_read(p->dsp, i);
  memcpy(ports_to_spc, p->input_ports, 4);
}

// HUD/Schrift-Grafik so laden, wie Zelda 3 sie erwartet (beim Wechsel aus der deutschen Emulation)
#include "load_gfx.h"
void pl_z3_transfer_font(void) { TransferFontToVRAM(); }

// eigenständige zelda3.exe: Spielstände wie bei Zelda 3 aus saves/sram.dat (im aktuellen Ordner) lesen
void ZeldaReadSram(void);
void pl_z3_read_sram(void) { ZeldaReadSram(); }

// Map16 -> 8x8-Kacheln aus der geladenen ROM (deutsche Fassung: andere Einträge als die US-Fassung, z. B. Wetterhahn)
// (Entwicklung/Abgleich) die aktuell verwendete Map16-Tabelle auslesen
size_t pl_z3_get_map16(uint16 *out, size_t count) {
  size_t n = kMap16ToMap8_SIZE / 2;
  if (count < n)
    n = count;
  memcpy(out, kMap16ToMap8, n * 2);
  return n;
}

// Feste Map16-Nummern im Code von Zelda 3 stammen aus der US-Fassung. Die deutsche Tabelle ist in einem Bereich
// verschoben (z. B. deutsch 0xDC9 = US 0xDC5): ohne Umrechnung zeigten geöffnete Türen und geschnittenes Gras falsche
// Kacheln. Je US-Nummer die deutsche Nummer mit demselben Kachelinhalt.
static uint16 g_m16_remap[4096];
static uint16 *g_m16_us;
static size_t g_m16_n;

uint16 PlM16(uint16 v) {
  return (v < 4096 && g_m16_remap[v]) ? g_m16_remap[v] - 1 : v;
}

void pl_z3_set_map16(const uint16 *table, size_t count) {
  size_t n = kMap16ToMap8_SIZE / 2;
  if (count * 2 > kMap16ToMap8_SIZE)
    return;
  if (!g_m16_us) {
    // Original (US) einmal sichern
    g_m16_us = (uint16 *)malloc(n * 2);
    memcpy(g_m16_us, kMap16ToMap8, n * 2);
    g_m16_n = n;
  }
  memcpy((void *)kMap16ToMap8, table, count * 2);
  memset(g_m16_remap, 0, sizeof(g_m16_remap));
  size_t entries = count / 4;
  for (size_t i = 0; i < entries && i < 4096 && i * 4 + 3 < g_m16_n; i++) {
    const uint16 *u = g_m16_us + i * 4;
    if (memcmp(u, table + i * 4, 8) == 0)
      continue;
    // nächstgelegenen Eintrag mit gleichem Inhalt suchen (erst in der Nähe, dann überall)
    long best = -1;
    for (long d = 1; d < (long)entries && best < 0; d++) {
      for (int sgn = -1; sgn <= 1 && best < 0; sgn += 2) {
        long j = (long)i + sgn * d;
        if (j >= 0 && j < (long)entries && memcmp(u, table + j * 4, 8) == 0)
          best = j;
      }
    }
    if (best >= 0)
      g_m16_remap[i] = (uint16)(best + 1);
  }
}
