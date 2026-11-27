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
int gTmcCoreNoObj = 0;
static int sCapture;
static uint32_t sCap[FB_W * FB_H];
static int sCapW, sCapH;

void TmcCore_Present(const uint32_t* px, int w, int h, int pitch) {
    if (w > FB_W)
        w = FB_W;
    if (h > FB_H)
        h = FB_H;
    if (sCapture) {
        /* Karte: zusätzliches Bild ohne Figuren in einen eigenen Puffer */
        for (int y = 0; y < h; y++)
            memcpy(&sCap[y * w], &px[y * pitch], (size_t)w * 4);
        sCapW = w;
        sCapH = h;
        return;
    }
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
    {
        extern void TmcCore_UpdateHideCounts(void);
        TmcCore_UpdateHideCounts();
    }
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
        case 8:
            return gMain.task * 10000 + gMain.state * 100 + gMain.substate;
        case 9: {
            extern int gTmcCoreNoclip;
            return gTmcCoreNoclip;
        }
        default:
            return -2;
    }
}


/* ---------------------------------------------------------------------------------------------------------------
 * Karte (Zelda Core): Kamera/Raum, Bild ohne Figuren, Fundorte aus der Randomizer-Logik von Picori
 * ------------------------------------------------------------------------------------------------------------- */
#include "area.h"
#include "room.h"
#include "player.h"
#include "flags.h"
#include "rando/rando_logic.h"
#include "rando/rando_runtime.h"
#include "rando/rando_keymap.h"
#include "item_ids.h"

extern void Port_PPU_PresentFrame(void);
extern void Port_PPU_SetPresentIsFirstOfTick(bool first);
extern void port_hdma_vblank_reset(void);
extern u32 CheckLocalFlagByBank(u32 offset, u32 flag);
extern u32 GetFlagBankOffset(u32 idx);

/* Ansicht: [0] Gebiet, [1] Raum, [2] Oberwelt (1/0), [3] Raum-Ursprung x, [4] y, [5] Kamera x, [6] y,
 * [7] Raumbreite, [8] Raumhöhe, [9] Link x, [10] Link y, [11] Bildbreite. 0 = nicht im Spiel */
EXPORT int tmc_view(int* out) {
    if (gMain.task != 2 || gMain.substate == GAMEMAIN_SUBTASK)
        return 0;
    out[0] = gRoomControls.area;
    out[1] = gRoomControls.room;
    out[2] = AreaIsOverworld() ? 1 : 0;
    out[3] = gRoomControls.origin_x;
    out[4] = gRoomControls.origin_y;
    out[5] = gRoomControls.scroll_x;
    out[6] = gRoomControls.scroll_y;
    out[7] = gRoomControls.width;
    out[8] = gRoomControls.height;
    out[9] = gPlayerEntity.base.x.HALF.HI;
    out[10] = gPlayerEntity.base.y.HALF.HI;
    out[11] = sW;
    return 1;
}

/* das zuletzt gezeigte Spielbild noch einmal zeichnen – ohne Figuren (nur Hintergründe). Das Spiel läuft dabei
 * nicht weiter. Liefert RGBA (Breite × 160). */
EXPORT const uint32_t* tmc_render_bg(int* w, int* h) {
    if (sState != 1)
        return NULL;
    const int layers = gTmcCoreLayers;
    sCapture = 1;
    gTmcCoreNoObj = 1;
    gTmcCoreLayers = 0x0E; /* ohne BG0 (Anzeige, Texte); Hintergrundfarbe wird Magenta = nicht übernehmen */
    /* dunkle Räume (Königliches Tal, Höhlen): BG3 ist dort die Abdunkel-Schicht – für die Karte weglassen */
    if (gArea.lightType != 0)
        gTmcCoreLayers = 0x06;
    /* Aufhellen/Abdunkeln (Überblendungen, Intro) nicht mit aufnehmen; Transparenz bleibt */
    const u8 bld = gIoMem[0x50];
    if (((bld >> 6) & 3) >= 2)
        gIoMem[0x50] = (u8)(bld & 0x3F);
    /* Fenster aus (dunkle Gebiete wie das Königliche Tal zeigen sonst nur die Lichtkreise) */
    const u8 disp_hi = gIoMem[0x01];
    gIoMem[0x01] = (u8)(disp_hi & ~0xE0);
    Port_PPU_SetPresentIsFirstOfTick(false);
    Port_PPU_PresentFrame();
    port_hdma_vblank_reset();
    gIoMem[0x50] = bld;
    gIoMem[0x01] = disp_hi;
    gTmcCoreLayers = layers;
    gTmcCoreNoObj = 0;
    sCapture = 0;
    *w = sCapW;
    *h = sCapH;
    return sCap;
}

/* Raumkopf: Weltposition und Größe eines Raums (für Fundorte auf der Karte) */
EXPORT int tmc_room_rect(int area, int room, int* out) {
    if (area < 0 || area >= 0x90 || room < 0 || room >= 0x40)
        return 0;
    RoomHeader* t = gAreaRoomHeaders[area];
    if (t == NULL)
        return 0;
    RoomHeader* r = t + room;
    if (r->map_x == 0xFFFF || r->pixel_width == 0 || r->pixel_width > 0x1000)
        return 0;
    out[0] = r->map_x;
    out[1] = r->map_y;
    out[2] = r->pixel_width;
    out[3] = r->pixel_height;
    return 1;
}

/* Fundorte: Logik von Picori laden (Text der Datei default.logic) */
EXPORT int tmc_loc_load(const char* text, int len) {
    if (RandoLogic_IsLoaded())
        return 1;
    if (!RandoLogic_LoadText(text, (size_t)len))
        return 0;
    Rando_Keymap_Apply(); /* Adressen der Logik-Datei auf Gebiet/Raum/Flag abbilden */
    return 1;
}

EXPORT int tmc_loc_count(void) {
    return RandoLogic_IsLoaded() ? (int)RandoLogic_GetLocationCountRaw() : 0;
}

/* Fundort i: Name nach `name`, Schlüssel (Gebiet << 16 | Raum << 8 | Truhe/Flag, oder Sonderfall), Art */
EXPORT int tmc_loc_info(int i, char* name, int cap, unsigned* key, int* type) {
    const char* n = RandoLogic_GetLocationName((uint32_t)i);
    if (n == NULL)
        return 0;
    snprintf(name, (size_t)cap, "%s", n);
    *key = RandoLogic_GetLocationKeyAt((uint32_t)i);
    *type = (int)RandoLogic_GetLocationType((uint32_t)i);
    return 1;
}

/* schon eingesammelt? (wie der Tracker von Picori) */
EXPORT int tmc_loc_done(unsigned key) {
    if (key == 0xFFFFFFFFu)
        return 0;
    if (key & 0x80000000u) {
        uint32_t group = (key >> 24) & 0x7F;
        uint32_t subkey = (key >> 16) & 0xFF;
        if (group == RANDO_SCRIPTED_KEY_SPECIAL) {
            switch (subkey) {
                case RANDO_SPECIAL_KEY_BELL_HP:
                    return CheckLocalFlagByBank(GetFlagBankOffset(2), 0xd0) != 0;
                case RANDO_SPECIAL_KEY_TINGLE_TROPHY:
                    return GetInventoryValue(ITEM_QST_TINGLE_TROPHY) != 0;
                case RANDO_SPECIAL_KEY_FORTRESS_PRIZE:
                    return GetInventoryValue(ITEM_OCARINA) != 0;
            }
        }
        return -1; /* unbekannt */
    }
    uint32_t area = (key >> 16) & 0xFF;
    uint32_t room = (key >> 8) & 0xFF;
    uint32_t fc = key & 0xFF;
    unsigned flag = Rando_GetChestLocalFlag(area, room, fc);
    unsigned offset = GetFlagBankOffset(area);
    return CheckLocalFlagByBank(offset, flag != 0xFF ? flag : fc) != 0;
}

/* Gebiet: Bits 0 Oberwelt, 1 Dungeon (aus den Gebietsdaten des Spiels) */
extern bool32 CheckAreaOverworld(u32 area);
EXPORT int tmc_area_kind(int area) {
    if (area < 0 || area >= 0x90)
        return 0;
    return (CheckAreaOverworld((u32)area) ? 1 : 0) | ((gAreaMetadata[area].flags & AR_IS_DUNGEON) ? 2 : 0);
}

#include "sound.h"

/* eigene Karte statt der Kartenseite des Pausenmenüs (Patch in pauseMenu.c) */
int gTmcCoreOwnMap = 0, gTmcCoreMapReq = 0;
extern u8 gUnk_08128C00[];
extern s32 sub_080A50A0(s32 param_1);

EXPORT void tmc_set_own_map(int on) {
    gTmcCoreOwnMap = on;
}

/* 1 = im Pausenmenü wurde zur Kartenseite geblättert (einmalig), 2 = die Kartenseite des Spiels ist offen */
EXPORT int tmc_map_request(void) {
    if (gTmcCoreMapReq) {
        gTmcCoreMapReq = 0;
        return 1;
    }
    if (gMain.substate == GAMEMAIN_SUBTASK && gUI.lastState == 1 && gPauseMenuOptions.screen >= 4 &&
        gPauseMenuOptions.screen <= 6)
        return 2;
    return 0;
}

/* Pausenmenü von der (eigenen) Karte aus weiterblättern: 1 = L, 2 = R, 0 = Menü schließen */
EXPORT int tmc_menu_goto(int which) {
    if (!(gMain.substate == GAMEMAIN_SUBTASK && gUI.lastState == 1))
        return 0;
    s32 s = which == 0 ? 0xe : sub_080A50A0(gUnk_08128C00[which & 3]);
    if (which != 0 && s >= 4 && s <= 6)
        return 0;
    if (s != gPauseMenuOptions.screen) {
        gPauseMenuOptions.screen2 = s;
        gMenu.field_0xc = NULL;
    }
    SoundReq(SFX_TEXTBOX_OPEN);
    return 1;
}

/* ---------------------------------------------------------------------------------------------------------------
 * Ganze Karte vorab (Zelda Core): jeden Raum einmal besuchen und abfotografieren. Zelda Core sichert vorher einen
 * Savestate und lädt ihn danach wieder – am Spiel ändert sich nichts.
 * ------------------------------------------------------------------------------------------------------------- */
#include "fade.h"
#include "transitions.h"

static int sGenHold, sGenX, sGenY;
extern u8 gUpdateVisibleTiles;
extern void UpdateScrollVram(void);

/* Raum-Kopf i eines Gebiets, solange es Räume gibt (Ende = 0xFFFF) */
EXPORT int tmc_room_count(int area) {
    if (area < 0 || area >= 0x90 || gAreaRoomHeaders[area] == NULL)
        return 0;
    int n = 0;
    while (n < 0x40 && gAreaRoomHeaders[area][n].map_x != 0xFFFF)
        n++;
    return n;
}

/* in einen Raum springen (Position im Raum) */
EXPORT int tmc_warp(int area, int room, int x, int y) {
    if (gMain.task != 2 || gMain.substate != GAMEMAIN_UPDATE)
        return 0;
    /* nur während des Erstellens (der Spielstand wird danach zurückgeladen): Intro vorbei, damit Räume wie die
     * Stadt in ihrer normalen Form laden und nicht als Fest-Fassung */
    if (gSave.global_progress < 2)
        gSave.global_progress = 2;
    SetGlobalFlag(TABIDACHI);
    Transition t;
    memset(&t, 0, sizeof(t));
    t.warp_type = WARP_TYPE_AREA;
    t.endX = (u16)x;
    t.endY = (u16)y;
    t.area = (u8)area;
    t.room = (u8)room;
    t.layer = 1;
    gRoomTransition.stairs_idx = 0;
    DoExitTransition(&t);
    sGenHold = 0;
    return 1;
}

/* Raum fertig geladen und sichtbar? Gebiet << 8 | Raum, sonst -1. Hält Link dabei am Leben. */
EXPORT int tmc_gen_ready(void) {
    if (gSave.stats.maxHealth)
        gSave.stats.health = gSave.stats.maxHealth;
    if (sGenHold) {
        gRoomControls.camera_target = NULL;
        gRoomControls.scroll_x = (s16)sGenX;
        gRoomControls.scroll_y = (s16)sGenY;
    }
    if (gMain.task != 2 || gMain.substate != GAMEMAIN_UPDATE || gFadeControl.active || gRoomTransition.transitioningOut)
        return -1;
    return (gRoomControls.area << 8) | gRoomControls.room;
}

/* Kamera fest auf (x, y) (Weltpixel, linke obere Ecke); -1 = wieder Link folgen */
EXPORT void tmc_cam(int x, int y) {
    if (x < 0) {
        sGenHold = 0;
        gRoomControls.camera_target = &gPlayerEntity.base;
        return;
    }
    sGenHold = 1;
    sGenX = x;
    sGenY = y;
    gRoomControls.camera_target = NULL;
    gRoomControls.scroll_x = (s16)x;
    gRoomControls.scroll_y = (s16)y;
    /* sichtbare Kacheln komplett neu (wie beim Betreten eines Raums) */
    gUpdateVisibleTiles = 1;
    UpdateScrollVram();
    gUpdateVisibleTiles = 1;
}

/* Fortschritt für die Quest-Seite der Karte (Reihenfolge siehe mapui.rs: QUEST_VALUES) */
EXPORT int tmc_progress(int* out, int cap) {
    if (gMain.task != 2)
        return 0;
    int v[40];
    int n = 0;
#define HAS(i) (GetInventoryValue(i) != 0)
    v[n++] = HAS(ITEM_EARTH_ELEMENT);
    v[n++] = HAS(ITEM_FIRE_ELEMENT);
    v[n++] = HAS(ITEM_OCARINA);
    v[n++] = HAS(ITEM_WATER_ELEMENT);
    v[n++] = HAS(ITEM_WIND_ELEMENT);
    v[n++] = HAS(ITEM_FOURSWORD);
    v[n++] = HAS(ITEM_BOTTLE1) + HAS(ITEM_BOTTLE2) + HAS(ITEM_BOTTLE3) + HAS(ITEM_BOTTLE4);
    {
        int f = 0;
        for (int i = 0; i < 36; i++)
            for (int b = 0; b < 8; b++)
                f += (gSave.figurines[i] >> b) & 1;
        v[n++] = f;
    }
    v[n++] = gSave.kinstones.fusedCount;
    v[n++] = HAS(ITEM_SKILL_SPIN_ATTACK) + HAS(ITEM_SKILL_ROLL_ATTACK) + HAS(ITEM_SKILL_DASH_ATTACK) +
             HAS(ITEM_SKILL_ROCK_BREAKER) + HAS(ITEM_SKILL_SWORD_BEAM) + HAS(ITEM_SKILL_GREAT_SPIN) +
             HAS(ITEM_SKILL_DOWN_THRUST) + HAS(ITEM_SKILL_PERIL_BEAM);
    v[n++] = gSave.stats.maxHealth / 8;
    v[n++] = gSave.stats.walletType;
    v[n++] = gSave.stats.bombBagType;
    v[n++] = gSave.stats.quiverType;
    v[n++] = HAS(ITEM_KINSTONE_BAG);
    v[n++] = HAS(ITEM_GRIP_RING);
    v[n++] = HAS(ITEM_POWER_BRACELETS);
    v[n++] = HAS(ITEM_FLIPPERS);
    v[n++] = HAS(ITEM_QST_BOOK1) + HAS(ITEM_QST_BOOK2) + HAS(ITEM_QST_BOOK3);
    v[n++] = HAS(ITEM_QST_TINGLE_TROPHY);
    v[n++] = HAS(ITEM_QST_CARLOV_MEDAL);
    v[n++] = HAS(ITEM_QST_LONLON_KEY);
    v[n++] = HAS(ITEM_QST_GRAVEYARD_KEY);
    v[n++] = HAS(ITEM_LIGHT_ARROW);
    v[n++] = HAS(ITEM_MIRROR_SHIELD);
    v[n++] = HAS(ITEM_MAGIC_BOOMERANG);
#undef HAS
    if (n > cap)
        n = cap;
    memcpy(out, v, (size_t)n * sizeof(int));
    return n;
}

/* Gebietsnamen (Warp-Menü von Picori, englisch) und ob ein Sprung dorthin sicher ist */
extern const char* Port_DebugQuery_AreaName(unsigned char area);
extern int Port_DebugAction_AreaIsWarpable(unsigned char area);

EXPORT int tmc_area_name(int area, char* out, int cap) {
    if (area < 0 || area > 0xFF)
        return 0;
    const char* n = Port_DebugQuery_AreaName((unsigned char)area);
    if (n == NULL)
        return 0;
    snprintf(out, (size_t)cap, "%s", n);
    return Port_DebugAction_AreaIsWarpable((unsigned char)area) ? 1 : 2;
}

extern u32 GetCollisionDataAtWorldCoords(u32 worldX, u32 worldY, u32 layer);

/* Schnellreise (Karte): in einen Raum springen, ohne etwas am Spielstand zu ändern (anders als tmc_warp) */
EXPORT int tmc_travel(int area, int room, int x, int y) {
    if (gMain.task != 2 || gMain.substate != GAMEMAIN_UPDATE)
        return 0;
    Transition t;
    memset(&t, 0, sizeof(t));
    t.warp_type = WARP_TYPE_AREA;
    t.endX = (u16)x;
    t.endY = (u16)y;
    t.area = (u8)area;
    t.room = (u8)room;
    t.layer = 1;
    gRoomTransition.stairs_idx = 0;
    DoExitTransition(&t);
    return 1;
}

/* nach der Ankunft: Link auf die nächste begehbare Stelle stellen (Spirale um die aktuelle Position) */
EXPORT int tmc_place_safe(void) {
    if (gMain.task != 2 || gMain.substate != GAMEMAIN_UPDATE)
        return 0;
    Entity* p = &gPlayerEntity.base;
    s32 x0 = p->x.HALF.HI, y0 = p->y.HALF.HI;
    s32 rx0 = gRoomControls.origin_x + 8, ry0 = gRoomControls.origin_y + 8;
    s32 rx1 = gRoomControls.origin_x + gRoomControls.width - 8, ry1 = gRoomControls.origin_y + gRoomControls.height - 8;
    for (s32 r = 0; r < 160; r += 8) {
        for (s32 dy = -r; dy <= r; dy += 8) {
            for (s32 dx = -r; dx <= r; dx += 8) {
                if (dx != -r && dx != r && dy != -r && dy != r)
                    continue;
                s32 x = x0 + dx, y = y0 + dy;
                if (x < rx0 || y < ry0 || x > rx1 || y > ry1)
                    continue;
                /* Mitte und die vier Ecken eines 12×12-Felds frei */
                if (GetCollisionDataAtWorldCoords(x, y, p->collisionLayer) == 0 &&
                    GetCollisionDataAtWorldCoords(x - 6, y - 6, p->collisionLayer) == 0 &&
                    GetCollisionDataAtWorldCoords(x + 6, y - 6, p->collisionLayer) == 0 &&
                    GetCollisionDataAtWorldCoords(x - 6, y + 6, p->collisionLayer) == 0 &&
                    GetCollisionDataAtWorldCoords(x + 6, y + 6, p->collisionLayer) == 0) {
                    p->x.HALF.HI = (s16)x;
                    p->y.HALF.HI = (s16)y;
                    return 1;
                }
            }
        }
    }
    return 0;
}

/* besondere Punkte im aktuellen Raum für die Karte: je drei Werte (Art, x, y in Weltpixeln).
 * Art 1 = Schrumpfpunkt (Minish-Portal), 2 = Windwappen, 3 = Bohnenranke */
#include "manager.h"
#include "manager/minishPortalManager.h"
#include "object.h"
extern bool32 Port_IsValidEntityAddr(const void* p);

EXPORT int tmc_room_marks(int* out, int cap) {
    if (gMain.task != 2)
        return 0;
    int n = 0;
    for (int l = 0; l < 9; l++) {
        LinkedList* list = &gEntityLists[l];
        Entity* e = list->first;
        for (int w = 0; w < 256 && e != NULL && (intptr_t)e != (intptr_t)list && Port_IsValidEntityAddr(e); w++, e = e->next) {
            int kind = 0, x = 0, y = 0;
            if (e->kind == MANAGER && e->id == MINISH_PORTAL_MANAGER) {
                MinishPortalManager* m = (MinishPortalManager*)e;
                kind = 1;
                x = gRoomControls.origin_x + m->unk_38;
                y = gRoomControls.origin_y + m->unk_3a;
            } else if (e->kind == OBJECT && e->id == WINDCREST) {
                kind = 2;
            } else if (e->kind == OBJECT && e->id == BEANSTALK) {
                kind = 3;
            }
            if (kind == 0)
                continue;
            if (kind != 1) {
                x = e->x.HALF.HI;
                y = e->y.HALF.HI;
            }
            if (n + 3 <= cap) {
                out[n] = kind;
                out[n + 1] = x;
                out[n + 2] = y;
                n += 3;
            }
        }
    }
    return n / 3;
}

/* Cheats der Karte/Zelda Core: No-Clip (durch Wände gehen) und „100 %“ (alles, was in die 100 % zählt) */
extern void Port_DebugAction_SetNoclip(int on);
extern void Port_DebugAction_GiveAllItems(void);
extern void Port_DebugAction_MaxHearts(void);
extern void Port_DebugAction_AllKinstones(void);
extern void Port_DebugAction_AllFigurines130(void);

int gTmcCoreNoclip = 0;

EXPORT void tmc_noclip(int on) {
    gTmcCoreNoclip = on;
    Port_DebugAction_SetNoclip(on);
}

EXPORT int tmc_cheat_complete(void) {
    if (gMain.task != 2)
        return 0;
    Port_DebugAction_GiveAllItems();
    Port_DebugAction_MaxHearts();
    /* alle 20 Herzen, voll aufgefüllt (auch das letzte), keine angefangenen Herzteile */
    gSave.stats.maxHealth = 160;
    gSave.stats.health = 160;
    gSave.stats.heartPieces = 0;
    Port_DebugAction_AllKinstones();
    Port_DebugAction_AllFigurines130();
    for (int i = 0; i < 36; i++)
        gSave.figurines[i] = 0xFF;
    gSave.kinstones.fusedCount = 100;
    gSave.stats.walletType = 3;
    gSave.stats.bombBagType = 3;
    gSave.stats.quiverType = 3;
    static const int kQuest[] = { ITEM_EARTH_ELEMENT, ITEM_FIRE_ELEMENT, ITEM_WATER_ELEMENT, ITEM_WIND_ELEMENT,
                                  ITEM_OCARINA, ITEM_FOURSWORD, ITEM_BOTTLE1, ITEM_BOTTLE2, ITEM_BOTTLE3, ITEM_BOTTLE4,
                                  ITEM_SKILL_SPIN_ATTACK, ITEM_SKILL_ROLL_ATTACK, ITEM_SKILL_DASH_ATTACK,
                                  ITEM_SKILL_ROCK_BREAKER, ITEM_SKILL_SWORD_BEAM, ITEM_SKILL_GREAT_SPIN,
                                  ITEM_SKILL_DOWN_THRUST, ITEM_SKILL_PERIL_BEAM, ITEM_KINSTONE_BAG, ITEM_GRIP_RING,
                                  ITEM_POWER_BRACELETS, ITEM_FLIPPERS, ITEM_QST_BOOK1, ITEM_QST_BOOK2, ITEM_QST_BOOK3,
                                  ITEM_QST_TINGLE_TROPHY, ITEM_QST_CARLOV_MEDAL, ITEM_QST_LONLON_KEY,
                                  ITEM_QST_GRAVEYARD_KEY, ITEM_LIGHT_ARROW, ITEM_MIRROR_SHIELD, ITEM_MAGIC_BOOMERANG,
                                  ITEM_REMOTE_BOMBS, ITEM_BOW };
    for (unsigned i = 0; i < sizeof(kQuest) / sizeof(kQuest[0]); i++)
        if (GetInventoryValue(kQuest[i]) == 0)
            SetInventoryValue(kQuest[i], 1);
    /* Herzteil an der Glocke (per Skript vergeben, nicht in den Fundort-Schlüsseln) */
    SetLocalFlagByBank(GetFlagBankOffset(2), 0xd0);
    /* alle Fundorte der Logik als geholt markieren (Truhen, Herzteile, …) */
    int n = 0;
    if (RandoLogic_IsLoaded()) {
        for (uint32_t i = 0; i < RandoLogic_GetLocationCountRaw(); i++) {
            uint32_t key = RandoLogic_GetLocationKeyAt(i);
            if (key == 0xFFFFFFFFu || (key & 0x80000000u))
                continue;
            uint32_t area = (key >> 16) & 0xFF, room = (key >> 8) & 0xFF, fc = key & 0xFF;
            unsigned flag = Rando_GetChestLocalFlag(area, room, fc);
            SetLocalFlagByBank(GetFlagBankOffset(area), flag != 0xFF ? flag : fc);
            n++;
        }
    }
    return n;
}

/* Ausgänge (Türen) des aktuellen Raums: je 7 Werte – Art, x, y (Welt), Zielgebiet, Zielraum, Ziel-x, Ziel-y (im Raum) */
EXPORT int tmc_room_exits(int* out, int cap) {
    if (gMain.task != 2 || gArea.pCurrentRoomInfo == NULL)
        return 0;
    const Transition* t = gArea.pCurrentRoomInfo->exits;
    int n = 0;
    for (int k = 0; t != NULL && k < 64 && t->warp_type != WARP_TYPE_END_OF_LIST; k++, t++) {
        if (t->warp_type != WARP_TYPE_AREA && t->warp_type != WARP_TYPE_AREA2)
            continue;
        if (n + 7 > cap)
            break;
        out[n++] = t->warp_type;
        out[n++] = gRoomControls.origin_x + t->startX;
        out[n++] = gRoomControls.origin_y + t->startY;
        out[n++] = t->area;
        out[n++] = t->room;
        out[n++] = t->endX;
        out[n++] = t->endY;
    }
    return n / 7;
}

/* Anzahl (Bomben, Pfeile) auf den Tasten B/A nicht zeigen (Zelda Core: Einstellung, Standard an).
 * Die Ziffern-Kacheln werden nur geleert, solange auf A oder B wirklich ein Item mit Anzahl liegt – nur dann
 * gehören diese Kacheln sicher zu den Ziffern und nicht zu einer anderen Figur. */
static int sHideCountsOn = 1;
int gTmcCoreHideCounts = 0;
/* Aufnahme der Item-Grafiken aus dem Item-Menü: Bit 0 ohne Anzahl */
int gTmcCoreMenuCapture = 0;
EXPORT void tmc_menu_capture(int mode) {
    gTmcCoreMenuCapture = mode;
}
EXPORT void tmc_hide_counts(int on) {
    sHideCountsOn = on;
}

static int IsCountedItem(u8 i) {
    return i == 7 || i == 8 || i == 9 || i == 10;
}

void TmcCore_UpdateHideCounts(void) {
    const u8* eq = Resolve(0x02002AF4u, 2);
    gTmcCoreHideCounts = sHideCountsOn && eq != NULL && (IsCountedItem(eq[0]) || IsCountedItem(eq[1]));
}

