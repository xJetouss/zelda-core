/* Zelda-Core-Anbindung an Project Picori (The Minish Cap als PC-Fassung, GPL-3.0).
 *
 * Picori läuft in einem eigenen Thread. An jeder Bildgrenze (VBlankIntrWait) gibt es das fertige Bild ab und wartet,
 * bis Zelda Core das nächste Bild anfordert. Solange es wartet, darf Zelda Core Speicher lesen/schreiben und
 * Savestates anlegen/laden – das Spiel steht dann an einer festen Stelle. Takt, Turbo, Pause, Zurückspulen, Ton und
 * Anzeige übernimmt Zelda Core.
 */
#include "global.h"
#include "main.h"
#include "save.h"
#include "port_gba_mem.h"
#include "port_rom.h"
#include "menu.h"
#include "game.h"
#include "structures.h"
#include "ui.h"
#include "message.h"
#include "item_ids.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <SDL3/SDL.h>
#include <direct.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define EXPORT __declspec(dllexport)

int gTmcCoreHosted = 0;
u16 gTmcCoreKeys = 0;
int gTmcCoreViewW = 1920, gTmcCoreViewH = 1080;
int gTmcCoreLayers = 0x0F;
int gTmcCoreSaveWrite = 0;
char gTmcCoreSavePath[1024];

extern int Port_CoreMain(int argc, char* argv[]);
extern int Port_Audio_HostRender(int16_t* out, int frames);
extern size_t Port_Core_StateSize(void);
extern int Port_Core_StateSave(void* out, size_t cap);
extern int Port_Core_StateLoad(const void* in, size_t n);
extern void Port_Config_SetWidescreenEnabled(bool enabled);

#define FB_W 384
#define FB_H 160
#define AUDIO_RATE 48000
#define AUDIO_CAP (AUDIO_RATE / 4)

static HANDLE sThread, sGo, sDone;
static volatile LONG sState; /* 0 = startet, 1 = bereit, 2 = beendet/Fehler */
static uint32_t sFrame[FB_W * FB_H];
static int sW = 240, sH = 160;
static int16_t sAudio[AUDIO_CAP * 2];
static int sAudioN;
static double sAudioAcc;
static int sMute;
static char sRom[1024], sDir[1024];

/* von port_ppu.cpp: fertiges Bild (ABGR8888 = RGBA-Bytes) */
void TmcCore_Present(const uint32_t* px, int w, int h, int pitch) {
    if (w > FB_W)
        w = FB_W;
    if (h > FB_H)
        h = FB_H;
    for (int y = 0; y < h; y++)
        memcpy(&sFrame[y * w], &px[y * pitch], (size_t)w * 4);
    sW = w;
    sH = h;
}

/* von port_bios.c: Spielbild fertig – Ton für dieses Bild mischen, dann auf das nächste warten */
void TmcCore_FrameDone(void) {
    sAudioAcc += AUDIO_RATE / 59.7275;
    int n = (int)sAudioAcc;
    sAudioAcc -= n;
    if (sAudioN + n > AUDIO_CAP)
        sAudioN = 0;
    if (n > 0) {
        Port_Audio_HostRender(&sAudio[sAudioN * 2], n);
        if (sMute)
            memset(&sAudio[sAudioN * 2], 0, (size_t)n * 4);
        sAudioN += n;
    }
    InterlockedExchange(&sState, 1);
    SignalObjectAndWait(sDone, sGo, INFINITE, FALSE);
}

static DWORD WINAPI GameThread(LPVOID p) {
    (void)p;
    char* argv[] = { "zelda-core", "--skip-launcher", NULL };
    Port_CoreMain(2, argv);
    InterlockedExchange(&sState, 2);
    SetEvent(sDone);
    return 0;
}

/* Spiel starten. `rom`: Pfad zur ROM; `dir`: Arbeitsordner für Picori (entpackte Grafiken/Töne, config.json).
 * Kehrt sofort zurück; fertig ist der Start, wenn tmc_ready() 1 liefert. */
EXPORT int tmc_start(const char* rom, const char* dir) {
    if (sThread)
        return 0;
    snprintf(sRom, sizeof(sRom), "%s", rom);
    snprintf(sDir, sizeof(sDir), "%s", dir);
    _mkdir(sDir);
    if (_chdir(sDir) != 0)
        return 0;
    _putenv_s("TMC_BASEROM", sRom);
    _putenv_s("TMC_AUTOPLAY", "1");
    /* kein sichtbares Fenster: Picori zeichnet für Zelda Core in den Speicher */
    _putenv_s("SDL_VIDEODRIVER", "offscreen");
    SDL_SetHint(SDL_HINT_VIDEO_DRIVER, "offscreen");
    gTmcCoreHosted = 1;
    sGo = CreateEventA(NULL, FALSE, FALSE, NULL);
    sDone = CreateEventA(NULL, FALSE, FALSE, NULL);
    sThread = CreateThread(NULL, 64u << 20, GameThread, NULL, 0, NULL);
    return sThread != NULL;
}

/* Spielstand des Spiels (.sav im mGBA-Format, neben der ROM) – vor tmc_start setzen; `write` = Schreiben erlaubt */
EXPORT void tmc_set_save(const char* path, int write) {
    if (path)
        snprintf(gTmcCoreSavePath, sizeof(gTmcCoreSavePath), "%s", path);
    gTmcCoreSaveWrite = write != 0;
}

/* 0 = startet noch, 1 = bereit, 2 = beendet (Fehler) */
EXPORT int tmc_ready(void) {
    static int consumed;
    if (sState == 1 && !consumed) {
        /* Meldung des ersten Bildes abholen, damit tmc_run_frame wirklich ein neues Bild abwartet */
        WaitForSingleObject(sDone, INFINITE);
        consumed = 1;
    }
    return (int)sState;
}

/* ein Spielbild mit diesen GBA-Tasten (gesetztes Bit = gedrückt) */
EXPORT int tmc_run_frame(uint16_t keys) {
    if (sState != 1)
        return 0;
    gTmcCoreKeys = keys;
    SignalObjectAndWait(sGo, sDone, INFINITE, FALSE);
    return sState == 1;
}

EXPORT const uint32_t* tmc_frame(int* w, int* h) {
    *w = sW;
    *h = sH;
    return sFrame;
}

/* Ton seit dem letzten Abholen (48 kHz Stereo); gibt die Zahl der Stereo-Werte zurück */
EXPORT int tmc_audio(int16_t* out, int max) {
    int n = sAudioN < max ? sAudioN : max;
    memcpy(out, sAudio, (size_t)n * 4);
    memmove(sAudio, &sAudio[n * 2], (size_t)(sAudioN - n) * 4);
    sAudioN -= n;
    return n;
}

EXPORT void tmc_set_mute(int m) {
    sMute = m;
}

/* Seitenverhältnis der Anzeige (Breite der Spielansicht folgt ihm, bis 384 Pixel) */
EXPORT void tmc_set_view(int w, int h, int widescreen) {
    gTmcCoreViewW = w > 0 ? w : 240;
    gTmcCoreViewH = h > 0 ? h : 160;
    Port_Config_SetWidescreenEnabled(widescreen != 0);
}

EXPORT void tmc_set_layers(int mask) {
    gTmcCoreLayers = mask & 0x0F;
}

/* Savestate = alle Spielvariablen. build_tmc.py legt die Variablen der Spieldateien (Dekompilierung, nachgebildeter
 * GBA-Speicher, Spiel-Hilfen der PC-Fassung) in die Abschnitte .tmcd/.tmcb dieser DLL; die werden komplett gesichert.
 * Alles andere (Anzeige, Ton-Synthese, Einstellungen) bleibt, wie es ist. */
/* Kopf eines Savestates. Die Spielvariablen enthalten Zeiger in diese DLL (Variablen, Tabellen, Funktionen) und in
 * die geladene ROM. Beide liegen in jedem Programmlauf woanders – beim Laden werden solche Zeiger umgerechnet. */
typedef struct {
    u32 magic, data, bss, pad;
    u64 mod, mod_size, rom, rom_size;
} StateHeader;

static struct {
    u8* p;
    size_t n;
} sSec[2];
static size_t sStateSize;
static u8* sModBase;
static u64 sModSize;

static void FindStateSections(void) {
    HMODULE self = NULL;
    if (sStateSize)
        return;
    GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       (LPCSTR)&FindStateSections, &self);
    u8* base = (u8*)self;
    IMAGE_NT_HEADERS* nt = (IMAGE_NT_HEADERS*)(base + ((IMAGE_DOS_HEADER*)base)->e_lfanew);
    IMAGE_SECTION_HEADER* s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++) {
        int k = !memcmp(s[i].Name, ".tmcd", 6) ? 0 : !memcmp(s[i].Name, ".tmcb", 6) ? 1 : -1;
        if (k >= 0) {
            sSec[k].p = base + s[i].VirtualAddress;
            sSec[k].n = s[i].Misc.VirtualSize;
        }
    }
    sModBase = base;
    sModSize = nt->OptionalHeader.SizeOfImage;
    sStateSize = sizeof(StateHeader) + sSec[0].n + sSec[1].n;
}

#define STATE_MAGIC 0x32534D5Au /* "ZMS2" */

EXPORT size_t tmc_state_size(void) {
    FindStateSections();
    return sStateSize;
}

EXPORT int tmc_state_save(void* out, size_t cap) {
    FindStateSections();
    if (sState != 1 || cap < sStateSize || !sSec[0].n || !sSec[1].n)
        return 0;
    StateHeader h = { STATE_MAGIC, (u32)sSec[0].n, (u32)sSec[1].n, 0,
                      (u64)(uintptr_t)sModBase, sModSize, (u64)(uintptr_t)gRomData, gRomSize };
    u8* d = (u8*)out;
    memcpy(d, &h, sizeof(h));
    memcpy(d + sizeof(h), sSec[0].p, sSec[0].n);
    memcpy(d + sizeof(h) + sSec[0].n, sSec[1].p, sSec[1].n);
    return 1;
}

static void Relocate(u8* p, size_t n, const StateHeader* h) {
    const u64 mod = (u64)(uintptr_t)sModBase, rom = (u64)(uintptr_t)gRomData;
    if (h->mod == mod && h->rom == rom)
        return;
    for (size_t i = 0; i + 8 <= n; i += 8) {
        u64 v;
        memcpy(&v, p + i, 8);
        if (v >= h->mod && v < h->mod + h->mod_size)
            v = v - h->mod + mod;
        else if (h->rom && v >= h->rom && v < h->rom + h->rom_size)
            v = v - h->rom + rom;
        else
            continue;
        memcpy(p + i, &v, 8);
    }
}

EXPORT int tmc_state_load(const void* in, size_t n) {
    FindStateSections();
    const u8* d = (const u8*)in;
    StateHeader h;
    if (sState != 1 || n != sStateSize)
        return 0;
    memcpy(&h, d, sizeof(h));
    if (h.magic != STATE_MAGIC || h.data != sSec[0].n || h.bss != sSec[1].n || h.rom_size != gRomSize ||
        h.mod_size != sModSize)
        return 0;
    memcpy(sSec[0].p, d + sizeof(h), sSec[0].n);
    memcpy(sSec[1].p, d + sizeof(h) + sSec[0].n, sSec[1].n);
    Relocate(sSec[0].p, sSec[0].n, &h);
    Relocate(sSec[1].p, sSec[1].n, &h);
    return 1;
}

/* GBA-Adresse auf den Speicher der PC-Fassung abbilden. Der Spielstand (gSave) liegt dort als eigene Variable mit
 * dem Aufbau des Originals, alles andere im nachgebildeten EWRAM/IWRAM. */
static u8* Resolve(uint32_t addr, int n) {
    const uint32_t save = 0x02002A40u;
    if (addr >= save && addr + (uint32_t)n <= save + sizeof(gSave))
        return (u8*)&gSave + (addr - save);
    if (addr >= 0x02000000u && addr + (uint32_t)n <= 0x02040000u)
        return &gEwram[addr - 0x02000000u];
    if (addr >= 0x03000000u && addr + (uint32_t)n <= 0x03008000u)
        return &gIwram[addr - 0x03000000u];
    return NULL;
}

EXPORT int tmc_read(uint32_t addr, void* out, int n) {
    u8* p = Resolve(addr, n);
    if (!p || n <= 0)
        return 0;
    memcpy(out, p, (size_t)n);
    return 1;
}

EXPORT int tmc_write(uint32_t addr, const void* in, int n) {
    u8* p = Resolve(addr, n);
    if (!p || n <= 0)
        return 0;
    memcpy(p, in, (size_t)n);
    return 1;
}

/* Spielaufgabe (0 Logo, 1 Titel/Dateiauswahl, 2 Spiel, ...) */
EXPORT int tmc_task(void) {
    return gMain.task;
}

/* Spielanzeige mit B-Feld sichtbar? (im Spiel, kein Menü/Übergang, B-Feld nicht ausgeblendet) */
EXPORT int tmc_hud(void) {
    return gMain.task == 2 && gMain.state == 2 && !(gHUD.hideFlags & 0x02);
}

/* Item-Menü (Start, Seite „Items“): Item unter dem Cursor (0 = leerer Platz), -1 = Menü nicht offen */
typedef struct {
    u8 up, down, left, right, type, unk0, x, y;
} TmcItemMenuEntry;
extern const TmcItemMenuEntry gItemMenuTable[];

static int ItemMenuOpen(void) {
    return gMain.substate == GAMEMAIN_SUBTASK && gUI.lastState == 1 /* SUBTASK_PAUSEMENU */ &&
           gPauseMenuOptions.screen == 1 /* Items */ && gMenu.menuType == 1;
}

EXPORT int tmc_menu_item(void) {
    if (!ItemMenuOpen())
        return -1;
    u8 s = gMenu.field_0x3;
    return s < 17 ? gPauseMenu.items[s] : 0;
}

/* Position des Kästchens mit diesem Item im Item-Menü (x | y << 8, Mitte des Kästchens), -1 = nicht da */
EXPORT int tmc_menu_pos(int item) {
    if (!ItemMenuOpen() || item <= 0)
        return -1;
    for (int i = 0; i < 17; i++) {
        if (gPauseMenu.items[i] == item)
            return gItemMenuTable[i].x | (gItemMenuTable[i].y << 8);
    }
    return -1;
}

/* Lage des B-Tastenfelds (Mitte der Figur), nur wenn es ganz an seinem Platz steht: bei Zwischensequenzen und Texten
 * fährt das Spiel die Felder nach oben aus dem Bild. 0 = nicht zu sehen. */
EXPORT int tmc_button_b(int* x, int* y) {
    if (gHUD.hideFlags & HUD_HIDE_2)
        return 0;
    if (gMessage.state & MESSAGE_ACTIVE)
        return 0;
    for (int i = 0; i < MAX_UI_ELEMENTS; i++) {
        UIElement* e = &gHUD.elements[i];
        if (e->used && e->type == UI_ELEMENT_BUTTON_B && e->type2 == 0) {
            if ((s16)e->y != (s16)gHUD.buttonY[UI_ELEMENT_BUTTON_B])
                return 0;
            *x = (s16)e->x;
            *y = (s16)e->y;
            return 1;
        }
    }
    return 0;
}


/* Entwicklung: Werte aus dem Spiel zur Fehlersuche (0 = Weltkarte im Inventar, 1..4 = Seitenwahl des Menüs für
 * Start/L/R/B, 5 = aktuelle Menüseite) */
extern u32 GetInventoryValue(u32 item);
EXPORT int tmc_debug(int what) {
    switch (what) {
        case 0:
            return (int)GetInventoryValue(72);
        case 1: case 2: case 3: case 4:
            return gMenu.field_0xc ? gMenu.field_0xc[what - 1] : -1;
        case 5:
            return gPauseMenuOptions.screen;
        case 6:
            return (int)ITEM_MAP;
        case 7:
            return (int)GetInventoryValue(ITEM_MAP);
        default:
            return -2;
    }
}
