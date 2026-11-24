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
    Port_PPU_SetPresentIsFirstOfTick(false);
    Port_PPU_PresentFrame();
    port_hdma_vblank_reset();
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
