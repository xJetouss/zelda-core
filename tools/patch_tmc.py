"""Zelda-Core-Anpassungen an Project Picori (vendor/tmc).

Alle Änderungen stehen hinter `#ifdef TMC_CORE_DLL`, das normale Picori bleibt unverändert. Das Skript ist
wiederholbar: Bereits vorhandene Änderungen werden übersprungen. Liste der Änderungen: vendor/tmc/ZELDA_CORE_CHANGES.md
"""
from pathlib import Path

TMC = Path(__file__).resolve().parent.parent / 'vendor' / 'tmc'
MARK = 'TMC_CORE_DLL'


def patch(rel, pairs):
    p = TMC / rel
    d = p.read_text(encoding='utf8')
    changed = False
    for a, b in pairs:
        if b in d:
            continue
        # schon geändert und von einem späteren Patch weiter angepasst: die Kommentarzeile des Patches steht schon da
        key = next((l for l in b.splitlines() if 'Zelda Core' in l), None)
        if key is not None and key in d:
            continue
        assert d.count(a) == 1, (rel, d.count(a), a[:70])
        d = d.replace(a, b)
        changed = True
    if changed:
        p.write_text(d, encoding='utf8', newline='\n')
        print('gepatcht', rel)


patch('port/port_main.c', [
    ('int main(int argc, char* argv[]) {',
     '#ifdef TMC_CORE_DLL\n/* Zelda Core: Einstieg wird von src/gba/tmc_core.c in einem eigenen Thread aufgerufen */\n#define main Port_CoreMain\n#endif\nint main(int argc, char* argv[]) {'),
    ('    { Port_TTS_Init(); }', '#ifndef TMC_CORE_DLL\n    { Port_TTS_Init(); }\n#endif'),
    ('    Port_CheckForUpdates(window);', '#ifndef TMC_CORE_DLL /* Zelda Core: keine Netzwerkzugriffe */\n    Port_CheckForUpdates(window);\n#endif'),
])

patch('port/port_audio.c', [
    ('    sAudioStream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, Port_Audio_Feed, NULL);',
     '''#ifdef TMC_CORE_DLL
    /* Zelda Core holt den Ton je Spielbild selbst ab (Port_Audio_HostRender) */
    {
        extern int gTmcCoreHosted;
        if (gTmcCoreHosted)
            return true;
    }
#endif
    sAudioStream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, Port_Audio_Feed, NULL);'''),
    ('bool Port_Audio_Init(void) {',
     '''#ifdef TMC_CORE_DLL
/* Zelda Core: dieselbe Mischung wie Port_Audio_Feed, aber in einen Puffer des Gastprogramms (48 kHz Stereo) */
int Port_Audio_HostRender(int16_t* out, int frames) {
    int done = 0;
    while (done < frames) {
        int n = frames - done;
        float mix[1024 * PORT_AUDIO_CHANNELS];
        if (n > 1024)
            n = 1024;
        Port_M4A_Backend_Render(mix, (uint32_t)n, gMain.muteAudio != 0);
        if (!sGbaAccurate) {
            Port_Audio_PostProcess(mix, out + done * PORT_AUDIO_CHANNELS, n);
        } else {
            for (int i = 0; i < n * PORT_AUDIO_CHANNELS; ++i) {
                const float s = mix[i] > 32767.0f ? 32767.0f : (mix[i] < -32767.0f ? -32767.0f : mix[i]);
                out[done * PORT_AUDIO_CHANNELS + i] = (int16_t)(((int)s / 128) * 128);
            }
        }
        done += n;
    }
    return done;
}
#endif

bool Port_Audio_Init(void) {'''),
])

patch('port/port_bios.c', [
    ('''void VBlankIntrWait(void) {
    u64 nowNs;
    bool decoupled;
''', '''#ifdef TMC_CORE_DLL
extern int gTmcCoreHosted;
extern u16 gTmcCoreKeys;
extern void TmcCore_FrameDone(void);
#endif

void VBlankIntrWait(void) {
    u64 nowNs;
    bool decoupled;

#ifdef TMC_CORE_DLL
    /* Zelda Core: ein Bild zeichnen, an das Gastprogramm abgeben und mit dessen Tasten weiterlaufen. Takt,
     * Turbo, Pause und Zurückspulen steuert das Gastprogramm. */
    if (gTmcCoreHosted) {
        Port_PresentOnce(true);
        TmcCore_FrameDone();
        Port_ApplyLanguage();
        *(vu16*)(gIoMem + REG_OFFSET_KEYINPUT) = (u16)(0x03FF & ~gTmcCoreKeys);
        Port_Config_ClearInputEdges();
        sFrameNum++;
        VBlankIntr();
        return;
    }
#endif
'''),
])

patch('port/port_ppu.cpp', [
    ('extern "C" uint8_t gMain[]; // fix for MSVC (UWP PORT)', '''extern "C" uint8_t gMain[]; // fix for MSVC (UWP PORT)
#ifdef TMC_CORE_DLL
extern "C" int gTmcCoreHosted, gTmcCoreViewW, gTmcCoreViewH, gTmcCoreLayers, gTmcCoreNoObj;
extern "C" void TmcCore_Present(const uint32_t* px, int w, int h, int pitch);
#endif'''),
    ('''    if (sWindow != nullptr) {
        int w = 0, h = 0;
        SDL_GetWindowSizeInPixels(sWindow, &w, &h);
        Port_Widescreen_SetWindowPixels(w, h);
    }''', '''#ifdef TMC_CORE_DLL
    if (gTmcCoreHosted) {
        Port_Widescreen_SetWindowPixels(gTmcCoreViewW, gTmcCoreViewH);
    } else
#endif
    if (sWindow != nullptr) {
        int w = 0, h = 0;
        SDL_GetWindowSizeInPixels(sWindow, &w, &h);
        Port_Widescreen_SetWindowPixels(w, h);
    }'''),
    ('''    virtuappu_registers.frame_width = Port_PPU_VisibleFrameWidth();
    virtuappu_registers.frame_pitch = MODE1_GBA_WIDTH;

    dispcnt = (uint16_t)(gIoMem[0x00] | (gIoMem[0x01] << 8));''', '''    virtuappu_registers.frame_width = Port_PPU_VisibleFrameWidth();
    virtuappu_registers.frame_pitch = MODE1_GBA_WIDTH;

#ifdef TMC_CORE_DLL
    /* Zelda Core: Hintergrundebenen ausblenden (Aufnahme der Item-Symbole) */
    const uint8_t coreDispHi = gIoMem[0x01];
    if (gTmcCoreHosted)
        gIoMem[0x01] = (uint8_t)(coreDispHi & ~(~gTmcCoreLayers & 0x0F));
    if (gTmcCoreHosted && gTmcCoreNoObj)
        gIoMem[0x01] = (uint8_t)(gIoMem[0x01] & ~0x10);
    const uint16_t coreBackdrop = gBgPltt[0];
    if (gTmcCoreHosted && gTmcCoreLayers != 0x0F)
        gBgPltt[0] = 0x7C1F;
#endif

    dispcnt = (uint16_t)(gIoMem[0x00] | (gIoMem[0x01] << 8));'''),
    ('''    Port_PPU_ApplyDisplayPostProcess(MODE1_GBA_WIDTH * MODE1_GBA_HEIGHT);
''', '''    Port_PPU_ApplyDisplayPostProcess(MODE1_GBA_WIDTH * MODE1_GBA_HEIGHT);

#ifdef TMC_CORE_DLL
    /* Zelda Core zeigt das Bild selbst an (eigene Filter/Upscaler) */
    if (gTmcCoreHosted) {
        gIoMem[0x01] = coreDispHi;
        gBgPltt[0] = coreBackdrop;
        TmcCore_Present(virtuappu_frame_buffer, virtuappu_registers.frame_width, MODE1_GBA_HEIGHT, MODE1_GBA_WIDTH);
        return;
    }
#endif
'''),
])

patch('port/port_quicksave.c', [
    ('void Port_RestorePlayerHitbox(void);\n', '''void Port_RestorePlayerHitbox(void);
#ifdef TMC_CORE_DLL
#define TMC_CORE_QS_API 1
#endif
'''),
    ('int Port_QuickSave_SaveSlot(int slot) {', '''#ifdef TMC_CORE_DLL
/* Zelda Core: Savestates im Speicher (Kopf: Magie, Version, Region, Länge, Basisadressen; dann die Bereiche) */
typedef struct {
    u32 magic, version, region, bytes;
    u64 bases[NUM_REGIONS];
} CoreStateHeader;

size_t Port_Core_StateSize(void) {
    return sizeof(CoreStateHeader) + TotalRegionBytes();
}

int Port_Core_StateSave(void* out, size_t cap) {
    CoreStateHeader h;
    u8* dst = (u8*)out;
    if (cap < Port_Core_StateSize())
        return 0;
    memset(&h, 0, sizeof(h));
    h.magic = MAGIC;
    h.version = VERSION;
    h.region = ActiveRegionTag();
    h.bytes = (u32)TotalRegionBytes();
    for (size_t i = 0; i < NUM_REGIONS; i++)
        h.bases[i] = (u64)(uintptr_t)sRegions[i].ptr;
    memcpy(dst, &h, sizeof(h));
    dst += sizeof(h);
    for (size_t i = 0; i < NUM_REGIONS; i++) {
        memcpy(dst, sRegions[i].ptr, sRegions[i].size);
        dst += sRegions[i].size;
    }
    return 1;
}

int Port_Core_StateLoad(const void* in, size_t n) {
    static Slot s;
    CoreStateHeader h;
    if (n != Port_Core_StateSize())
        return 0;
    memcpy(&h, in, sizeof(h));
    if (h.magic != MAGIC || h.version != VERSION || h.region != ActiveRegionTag() || h.bytes != TotalRegionBytes())
        return 0;
    if (s.snapshot == NULL || s.bytes != h.bytes) {
        free(s.snapshot);
        s.snapshot = (u8*)malloc(h.bytes);
        if (!s.snapshot)
            return 0;
        s.bytes = h.bytes;
    }
    memcpy(s.snapshot, (const u8*)in + sizeof(h), h.bytes);
    memcpy(s.saved_bases, h.bases, sizeof(s.saved_bases));
    s.valid = 1;
    return Snapshot_Restore(&s);
}
#endif

int Port_QuickSave_SaveSlot(int slot) {'''),
])

# Picori legt entpackte Grafiken neben seine .exe; als DLL in Zelda Core gehören sie in den Arbeitsordner
patch('port/port_exe_path.cpp', [
    ('''namespace port {

std::optional<std::filesystem::path> ExecutableDir() {
''',
     '''#ifdef TMC_CORE_DLL
extern "C" int gTmcCoreHosted;
#endif
namespace port {

std::optional<std::filesystem::path> ExecutableDir() {
#ifdef TMC_CORE_DLL
    if (gTmcCoreHosted) {
        std::error_code ec;
        auto cwd = std::filesystem::current_path(ec);
        if (!ec)
            return cwd;
    }
#endif
'''),
])

# Spielstand: dieselbe <rom>.sav wie mGBA (gleiches Format); geschrieben wird nur, wenn Zelda Core es erlaubt
patch('port/port_save.c', [
    ('''static void LoadEepromFile(void) {
    int legacyRamOrder = 0;
#ifdef MULTI_REGION
    ResolveRegionDefaultPath();
#endif''', '''#ifdef TMC_CORE_DLL
extern int gTmcCoreHosted;
extern int gTmcCoreSaveWrite;
extern char gTmcCoreSavePath[1024];
#endif

static void LoadEepromFile(void) {
    int legacyRamOrder = 0;
#ifdef MULTI_REGION
    ResolveRegionDefaultPath();
#endif
#ifdef TMC_CORE_DLL
    if (gTmcCoreHosted && gTmcCoreSavePath[0])
        snprintf(sActivePath, sizeof(sActivePath), "%s", gTmcCoreSavePath);
#endif'''),
    ('''static void FlushEepromFile(void) {
    if (!sEepromDirty)
        return;''', '''static void FlushEepromFile(void) {
    if (!sEepromDirty)
        return;
#ifdef TMC_CORE_DLL
    if (gTmcCoreHosted && !gTmcCoreSaveWrite)
        return; /* nur über Savestates speichern; bleibt „geändert“, bis Schreiben wieder erlaubt ist */
#endif'''),
])

# lange (absolute) Pfade für den Spielstand neben der ROM
patch('port/port_save.c', [
    ('#define SAVE_FILENAME_MAX 64\n', '''#define SAVE_FILENAME_MAX 64
#ifdef TMC_CORE_DLL
#define SAVE_PATH_MAX 1024
#else
#define SAVE_PATH_MAX SAVE_FILENAME_MAX
#endif
'''),
    ('static char sActivePath[SAVE_FILENAME_MAX] = DEFAULT_SAVE_FILENAME;', 'static char sActivePath[SAVE_PATH_MAX] = DEFAULT_SAVE_FILENAME;'),
    ('    char tmp[SAVE_FILENAME_MAX + 8];', '    char tmp[SAVE_PATH_MAX + 8];'),
    ('        char bak[SAVE_FILENAME_MAX + 4];', '        char bak[SAVE_PATH_MAX + 4];'),
])

# schneller Start: keine einzelnen ROM-Seiten unter rom_data/ lesen oder schreiben (die ganze ROM liegt im Speicher;
# 2791 kleine Dateien zu öffnen kostete bei jedem Start mehrere Sekunden)
patch('port/port_rom.c', [
    ('''    if (IsPageExtracted(page))
        return;

    u32 offset = page << ROM_PAGE_SHIFT;''', '''    if (IsPageExtracted(page))
        return;
#ifdef TMC_CORE_DLL
    MarkPageExtracted(page);
    return;
#endif

    u32 offset = page << ROM_PAGE_SHIFT;'''),
    ('''    int pagesLoaded = LoadExtractedPages();
    if (pagesLoaded > 0) {''', '''#ifdef TMC_CORE_DLL
    int pagesLoaded = 0;
#else
    int pagesLoaded = LoadExtractedPages();
#endif
    if (pagesLoaded > 0) {'''),
])

# „nur über Savestates speichern“: ein Speichern im Spiel gilt als gelungen, obwohl die .sav nicht geschrieben wird
# (der Spielstand liegt im Speicher und steckt in jedem Savestate) – sonst meldet das Spiel „Speichern fehlgeschlagen“
# und man kann keine neue Datei anlegen
patch('port/port_save.c', [
    ('''    if (sSaveTxnDepth == 0)
        FlushEepromFile();
    return !sEepromWriteBlocked && !sEepromDirty;''', '''    if (sSaveTxnDepth == 0)
        FlushEepromFile();
#ifdef TMC_CORE_DLL
    if (gTmcCoreHosted && !gTmcCoreSaveWrite)
        return !sEepromWriteBlocked;
#endif
    return !sEepromWriteBlocked && !sEepromDirty;'''),
])

# (Magenta-Hintergrund für Aufnahmen und Bild ohne Figuren für die Karte stehen oben bei port_ppu.cpp)

# eigene Karte (Zelda Core): Die Kartenseite des Pausenmenüs entfällt, L/R wechselt zwischen Items und Quest-Status
# (die eigene Karte öffnet mit Minus)
patch('src/ui.c', [
    ('''    dest = dest * 0x20 + 0x6010000;
    divRem = DivAndMod(param_2, 10);''', '''    dest = dest * 0x20 + 0x6010000;
#ifdef TMC_CORE_DLL
    /* Zelda Core: Anzahl auf B/A ausgeblendet – leere Kacheln statt Ziffern (im Pausenmenü liegen auf diesen
     * Kacheln sonst Teile der großen Item-Grafiken, deshalb hier und nicht erst beim Zeichnen) */
    extern int gTmcCoreCountsHidden;
    if (gTmcCoreCountsHidden) {
        DmaFill32(3, 0, dest, 0x40);
        return;
    }
#endif
    divRem = DivAndMod(param_2, 10);'''),
])
patch('src/menu/pauseMenu.c', [
    ('''    DmaCopy32(3, &tiles322[temp2[1] * 0x10], dest, 0x40 * 4);

    ammoCount = -1;''', '''    DmaCopy32(3, &tiles322[temp2[1] * 0x10], dest, 0x40 * 4);
#ifdef TMC_CORE_DLL
    /* Zelda Core: Aufnahme der Item-Grafiken – Bit 0: statt der Anzahl leere Kacheln */
    extern int gTmcCoreMenuCapture;
    if ((gTmcCoreMenuCapture & 1) && (item == 7 || item == 8)) {
        DmaFill32(3, 0, dest, 0x40);
        return;
    }
#endif

    ammoCount = -1;'''),
])
patch('src/menu/pauseMenu.c', [
    ('''    if (sub_080A51F4() && (gMenu.field_0xc != NULL)) {
        iVar1 = -1;''', '''#ifdef TMC_CORE_DLL
    extern int gTmcCoreOwnMap, gTmcCoreMapReq;
#endif
    if (sub_080A51F4() && (gMenu.field_0xc != NULL)) {
        iVar1 = -1;'''),
    ('''        if (iVar1 >= 0) {
            switch (bVar5 = gMenu.field_0xc[iVar1]) {''', '''#ifdef TMC_CORE_DLL
        /* Zelda Core: keine Kartenseite (die eigene Karte öffnet mit Minus) – L/R wechselt zwischen Items und
         * Quest-Status */
        if (iVar1 >= 0 && gTmcCoreOwnMap) {
            s32 t = gMenu.field_0xc[iVar1];
            if (t == 4 || t == 5 || t == 6 || t == 0xf) {
                gPauseMenuOptions.screen2 = gPauseMenuOptions.screen == 1 ? 2 : 1;
                SoundReq(SFX_TEXTBOX_OPEN);
                gMenu.field_0xc = NULL;
                iVar1 = -1;
            }
        }
#endif
        if (iVar1 >= 0) {
            switch (bVar5 = gMenu.field_0xc[iVar1]) {'''),
])

# Cheat „durch Wände gehen“ (Zelda Core): eigener Schalter, gilt auch im Konsolen-Treue-Modus
patch('src/movement.c', [
    ('''    if (this == &gPlayerEntity.base && Port_Debug_NoclipEnabled()) {
        colResult = COL_NONE;
    }''', '''#ifdef TMC_CORE_DLL
    /* Zelda Core: Cheat „durch Wände gehen“ */
    extern int gTmcCoreNoclip;
    if (this == &gPlayerEntity.base && gTmcCoreNoclip) {
        colResult = COL_NONE;
    }
#endif
    if (this == &gPlayerEntity.base && Port_Debug_NoclipEnabled()) {
        colResult = COL_NONE;
    }'''),
])

# Cheat „durch Wände gehen“: Links eigene Bewegung ohne Wand-Prüfung und ohne Herausschieben aus Wänden
patch('src/playerUtils.c', [
    ('''void sub_08079E58(s32 speed, u32 direction) {
    if ((gPlayerEntity.base.direction & 4) == 0) {''', '''void sub_08079E58(s32 speed, u32 direction) {
#ifdef TMC_CORE_DLL
    /* Zelda Core: Cheat „durch Wände gehen“ */
    extern int gTmcCoreNoclip;
    if (gTmcCoreNoclip) {
        gPlayerEntity.base.collisions = 0;
        LinearMoveDirectionOLD(&gPlayerEntity.base, (s16)speed, (u8)direction);
        return;
    }
#endif
    if ((gPlayerEntity.base.direction & 4) == 0) {'''),
])

# Cheat „durch Wände gehen“: auch beim normalen Gehen (UpdatePlayerMovement)
patch('src/playerUtils.c', [
    ('''        gPlayerEntity.base.speed = 0x20;
    }
    if ((gPlayerEntity.base.direction & 4) == 0) {
        sub_08079E90(gPlayerEntity.base.direction);
    }
    sub_0800857C(&gPlayerEntity.base);''', '''        gPlayerEntity.base.speed = 0x20;
    }
#ifdef TMC_CORE_DLL
    /* Zelda Core: Cheat „durch Wände gehen“ beim Gehen */
    extern int gTmcCoreNoclip;
    if (gTmcCoreNoclip) {
        gPlayerEntity.base.collisions = 0;
        LinearMoveDirectionOLD(&gPlayerEntity.base, gPlayerEntity.base.speed, gPlayerEntity.base.direction);
        return;
    }
#endif
    if ((gPlayerEntity.base.direction & 4) == 0) {
        sub_08079E90(gPlayerEntity.base.direction);
    }
    sub_0800857C(&gPlayerEntity.base);'''),
])

# Aufnahme der Item-Symbole (Zelda Core): die Anzahl (Bomben, Pfeile) nicht mitzeichnen – die Ziffern sind eigene
# Figuren-Kacheln (OBJ 0x11A/0x11B und 0x126/0x127) über dem Item; leer, solange aufgenommen wird
patch('port/port_ppu.cpp', [
    ('''    const uint16_t coreBackdrop = gBgPltt[0];
    if (gTmcCoreHosted && gTmcCoreLayers != 0x0F)
        gBgPltt[0] = 0x7C1F;
#endif''', '''    const uint16_t coreBackdrop = gBgPltt[0];
    if (gTmcCoreHosted && gTmcCoreLayers != 0x0F)
        gBgPltt[0] = 0x7C1F;
    /* Zelda Core: Ziffern der Anzahl bei Aufnahmen ausblenden */
    uint8_t coreDigits[0x80];
    extern int gTmcCoreHideCounts;
    const bool coreHideDigits = gTmcCoreHosted && gTmcCoreHideCounts;
    if (coreHideDigits) {
        memcpy(coreDigits, &gVram[0x10000 + 0x11a * 0x20], 0x40);
        memcpy(coreDigits + 0x40, &gVram[0x10000 + 0x126 * 0x20], 0x40);
        memset(&gVram[0x10000 + 0x11a * 0x20], 0, 0x40);
        memset(&gVram[0x10000 + 0x126 * 0x20], 0, 0x40);
    }
#endif'''),
    ('''    if (gTmcCoreHosted) {
        gIoMem[0x01] = coreDispHi;
        gBgPltt[0] = coreBackdrop;''', '''    if (gTmcCoreHosted) {
        gIoMem[0x01] = coreDispHi;
        gBgPltt[0] = coreBackdrop;
        if (coreHideDigits) {
            memcpy(&gVram[0x10000 + 0x11a * 0x20], coreDigits, 0x40);
            memcpy(&gVram[0x10000 + 0x126 * 0x20], coreDigits + 0x40, 0x40);
        }'''),
])
