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

// Alte Spielstände: Kacheln, die eine frühere Fassung mit US-Nummern in die Karte geschrieben hat (offene Türen,
// Löcher …), stehen in der Liste der geänderten Kacheln. Beim Laden in die deutsche Nummer umrechnen und neu zeichnen.
static const uint16 kPlCodeTiles[] = { 0x918, 0x919, 0x91A, 0x91B, 0xDA4, 0xDA5, 0xDA6, 0xDA7, 0xDA8, 0xDA9, 0xDAA, 0xDAB, 0xDAC, 0xDAD, 0xDAE, 0xDAF, 0xDB0, 0xDB1, 0xDB2, 0xDB3, 0xDB4, 0xDB5, 0xDB6, 0xDB7, 0xDB8, 0xDB9, 0xDBA, 0xDBB, 0xDBC, 0xDBD, 0xDBE, 0xDBF, 0xDC0, 0xDC1, 0xDC2, 0xDC3, 0xDC4, 0xDC5, 0xDC6, 0xDC7, 0xDC8, 0xDC9, 0xDCA, 0xDCB, 0xDCC, 0xDCD, 0xDCE, 0xDCF, 0xDD0, 0xDD1, 0xDD2, 0xDD3, 0xDD4, 0xDD5, 0xDD6, 0xDD7, 0xDD8, 0xDD9, 0xDDA, 0xDDB, 0xDDC, 0xDDD, 0xDDE, 0xDDF, 0xDE0, 0xDE1, 0xDE2, 0xDE3, 0xDE4, 0xDE5, 0xDE6, 0xDE7, 0xDE8, 0xDE9, 0xDEA, 0xDEB, 0xDEC, 0xDED, 0xDEE, 0xDEF, 0xDF0, 0xDF1, 0xDF2, 0xDF3, 0xDF4, 0xDF5, 0xDF6, 0xDF7, 0xDF8, 0xDF9, 0xDFA, 0xDFB, 0xDFC, 0xDFD, 0xDFE, 0xDFF, 0xE00, 0xE01, 0xE02, 0xE03, 0xE04, 0xE05, 0xE06, 0xE07, 0xE08, 0xE09, 0xE0A, 0xE0B, 0xE0C, 0xE0D, 0xE0E, 0xE0F, 0xE10, 0xE11, 0xE12, 0xE13, 0xE14, 0xE15, 0xE16, 0xE17, 0xE18, 0xE19, 0xE1A, 0xE1B, 0xE1C, 0xE1D, 0xE1E, 0xE1F, 0xE20, 0xE21, 0xE22, 0xE23, 0xE24, 0xE25, 0xE26, 0xE27, 0xE28, 0xE29, 0xE2A, 0xE2B, 0xE2C, 0xE2D, 0xE2E, 0xE2F, 0xE30, 0xE31, 0xE32, 0xE33, 0xE34, 0xE35, 0xE36, 0xE37, 0xE38, 0xE39, 0xE3A, 0xE3B, 0xE3C, 0xE3D, 0xE3E, 0xE3F, 0xE40, 0xE41, 0xE42, 0xE43, 0xE44, 0xE45, 0xE46, 0xE47, 0xE48, 0xE54, 0xE64, 0xE65, 0xE66, 0xE67, 0xE68, 0xE69, 0xE6A, 0xE6B, 0xE6C, 0xE6D, 0xE6E, 0xE6F, 0xE70, 0xE71, 0xE72, 0xE73, 0xE74, 0xE75, 0xE76, 0xE77, 0xE78, 0xE79, 0xE7A, 0xE7B, 0xE7C, 0xE7D, 0xE7E, 0xE7F, 0xE80, 0xE81, 0xE82, 0xE83, 0xE84, 0xE85, 0xE86, 0xE87, 0xE88, 0xE89, 0xE8A, 0xE8B, 0xE8C, 0xE8D, 0xE8E, 0xE8F, 0xE90, 0xE91, 0xE92, 0xE93, 0xE94, 0xE95, 0xE96, 0xE97, 0xE98, 0xE99, 0xE9A, 0xE9B, 0xE9C, 0xE9D, 0xE9E, 0xE9F, 0xEA0, 0xEA1, 0xEA2, 0xEA3 };
void Overworld_DrawMap16(uint16 pos, uint16 value);

void pl_z3_fix_saved_tiles(void) {
  const int nc = sizeof(kPlCodeTiles) / sizeof(kPlCodeTiles[0]);
  int changed = 0;
  int n = num_memorized_tiles >> 1;
  if (n > 0x400)
    n = 0x400;
  for (int i = 0; i < n; i++) {
    uint16 v = memorized_tile_value[i];
    uint16 nv = PlM16(v);
    if (nv == v)
      continue;
    // nur echte Code-Nummern der US-Fassung, die nicht selbst schon eine umgerechnete Nummer sind
    int is_code = 0, is_target = 0;
    for (int k = 0; k < nc; k++) {
      if (kPlCodeTiles[k] == v)
        is_code = 1;
      if (PlM16(kPlCodeTiles[k]) == v)
        is_target = 1;
    }
    if (!is_code || is_target)
      continue;
    uint16 pos = memorized_tile_addr[i];
    memorized_tile_value[i] = nv;
    if ((pos >> 1) < 0x1000 && dung_bg2[pos >> 1] == v) {
      dung_bg2[pos >> 1] = nv;
      Overworld_DrawMap16(pos, nv);
      changed = 1;
    }
  }
  if (changed)
    nmi_load_bg_from_vram = 1;
}
