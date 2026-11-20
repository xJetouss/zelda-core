"""Baut Project Picori (vendor/tmc, GPL-3.0) als tmc_core.dll für Zelda Core.

Picori ist eine PC-Fassung von The Minish Cap auf Grundlage der Dekompilierung. Die Quellliste stammt aus dem
Ziel `tmc_pc` in vendor/tmc/xmake.lua (ohne Launcher, RetroAchievements, GPU-Renderer und Test-Harnesse).
Gebaut wird mit msys2/ucrt64-GCC; SDL3, libpng und zlib werden statisch gelinkt.

Aufruf:  python tools/build_tmc.py [ausgabeordner]
"""
import concurrent.futures as cf
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TMC = ROOT / 'vendor' / 'tmc'
OUT = ROOT / 'target' / 'tmc'
OBJ = OUT / 'obj'
GEN = OUT / 'gen'
MSYS = Path(os.environ.get('TMC_MSYS', r'C:\msys64\ucrt64'))
GCC = str(MSYS / 'bin' / 'gcc.exe')
GXX = str(MSYS / 'bin' / 'g++.exe')
VERSION = re.search(r'TMC_PC_VERSION = "([^"]+)"', (TMC / 'xmake.lua').read_text(encoding='utf8')).group(1)
WIDE = 384

SKIP = re.compile(r'tmc-Modern-Launcher|port_launcher_bootstrap|rcheevos|port_ra(_net|_ui)?\.c|port_gba_shadow|'
                  r'port_repro_|\.spv$|\.glsl$|\.png$')


def sources():
    x = (TMC / 'xmake.lua').read_text(encoding='utf8')
    a = x.index('target("tmc_pc")')
    b = x.index('target_end()', a)
    files = []
    for m in re.finditer(r'add_files\(([^)]*)\)', x[a:b]):
        for s in re.findall(r'"([^"]+)"', m.group(1)):
            if SKIP.search(s):
                continue
            files += sorted(TMC.glob(s)) if '*' in s else [TMC / s]
    imgui = TMC / 'libs' / 'imgui'
    files += [imgui / f for f in ('imgui.cpp', 'imgui_draw.cpp', 'imgui_tables.cpp', 'imgui_widgets.cpp', 'imgui_demo.cpp')]
    files += [imgui / 'backends' / f for f in ('imgui_impl_sdl3.cpp', 'imgui_impl_sdlrenderer3.cpp', 'imgui_impl_sdlgpu3.cpp')]
    files.append(ROOT / 'src' / 'gba' / 'tmc_core.c')
    seen, res = set(), []
    for f in files:
        if f not in seen and f.is_file():
            seen.add(f)
            res.append(f)
    return res


def gen():
    GEN.mkdir(parents=True, exist_ok=True)
    logo = (TMC / 'docs' / 'picori-logo.png').read_bytes()
    h = GEN / 'picori-logo.png.h'
    txt = ','.join('0x%02x' % c for c in logo)
    if not h.is_file() or h.read_text() != txt:
        h.write_text(txt)


DEFS = ['PC_PORT', 'NON_MATCHING', 'USE_HDMA', 'USA', 'ENGLISH', 'REVISION=0', 'MULTI_REGION', 'FMT_HEADER_ONLY=1',
        f'TMC_PC_VERSION="{VERSION}"', f'TMC_PORT_VERSION="{VERSION}"', f'MODE1_GBA_WIDTH={WIDE}', 'TMC_CORE_DLL',
        'SDL_MAIN_HANDLED']
INC = ['include', 'libs', 'port', '.', 'build/USA', 'port/ppu/include', 'port/shaders/build', 'libs/VirtuaAPU/include',
       'libs/agbplay_core', 'tools/src/assets_extractor', 'libs/imgui']
COMMON = ['-O2', '-g0', '-fno-strict-aliasing', '-fwrapv', '-fno-strict-overflow', '-funsigned-char', '-fopenmp',
          '-mno-ms-bitfields', '-w', '-include', 'region.h']


# Spielzustand: Variablen dieser Dateien kommen in eigene Abschnitte (.tmcd/.tmcb), die tmc_core.c als Savestate
# komplett sichert und zurückschreibt (Picoris eigener Quicksave deckt nicht alles ab, z. B. Paletten und Bildschirm)
STATE = re.compile(r"vendor[\\/]tmc[\\/]src[\\/]|port_(linked_stubs|gba_mem|draw|gameplay_stubs|hdma|m4a_stubs|animation|"
                   r'softslots|roll_attack_macro|figurines|practice)\.c$')


def obj_of(src):
    rel = os.path.relpath(src, ROOT).replace('\\', '/').replace('/', '_').replace('..', '_')
    return OBJ / (rel + '.o')


def compile_one(src):
    o = obj_of(src)
    if o.is_file() and o.stat().st_mtime >= src.stat().st_mtime:
        return src, 0, ''
    cxx = src.suffix in ('.cpp', '.cc')
    cmd = [GXX if cxx else GCC, '-std=gnu++20' if cxx else '-std=gnu11', *COMMON]
    cmd += ['-D' + d for d in DEFS]
    cmd += ['-I' + str(TMC / i) for i in INC] + ['-I' + str(GEN), '-I' + str(MSYS / 'include')]
    cmd += ['-c', str(src), '-o', str(o)]
    env = dict(os.environ, PATH=str(MSYS / 'bin') + os.pathsep + os.environ.get('PATH', ''))
    r = subprocess.run(cmd, cwd=TMC, capture_output=True, text=True, env=env)
    if r.returncode == 0 and STATE.search(str(src)):
        r2 = subprocess.run([str(MSYS / 'bin' / 'objcopy.exe'), '--rename-section', '.data=.tmcd',
                             '--rename-section', '.bss=.tmcb,alloc', str(o)], capture_output=True, text=True, env=env)
        if r2.returncode:
            o.unlink(missing_ok=True)
            return src, r2.returncode, r2.stderr
    return src, r.returncode, r.stderr[-4000:]


def main():
    dest = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else OUT
    OBJ.mkdir(parents=True, exist_ok=True)
    gen()
    srcs = sources()
    bad = []
    with cf.ThreadPoolExecutor(os.cpu_count() or 4) as ex:
        for src, rc, err in ex.map(compile_one, srcs):
            if rc:
                bad.append(src)
                print('FEHLER', src, '\n', err, file=sys.stderr)
    if bad:
        print(f'{len(bad)} Dateien fehlgeschlagen', file=sys.stderr)
        sys.exit(1)
    objs = [str(obj_of(s)) for s in srcs]
    rsp = OUT / 'link.rsp'
    rsp.write_text('\n'.join('"' + o.replace('\\', '/') + '"' for o in objs))
    dll = dest / 'tmc_core.dll'
    dest.mkdir(parents=True, exist_ok=True)
    cmd = [GXX, '-shared', '-o', str(dll), '@' + str(rsp), '-static', '-static-libgcc', '-static-libstdc++', '-fopenmp',
           '-Wl,--exclude-all-symbols', '-Wl,--export-all-symbols' if os.environ.get('TMC_EXPORT_ALL') else '-s',
           '-L' + str(MSYS / 'lib'), '-lSDL3', '-lpng16', '-lz',
           '-lwinhttp', '-lwinpthread', '-ldbghelp', '-lsetupapi', '-lole32', '-loleaut32', '-limm32', '-lversion',
           '-lwinmm', '-lgdi32', '-luser32', '-lshell32', '-ladvapi32', '-luuid', '-lcfgmgr32', '-lws2_32']
    env = dict(os.environ, PATH=str(MSYS / 'bin') + os.pathsep + os.environ.get('PATH', ''))
    r = subprocess.run(cmd, cwd=TMC, capture_output=True, text=True, env=env)
    if r.returncode:
        print(r.stderr[-20000:], file=sys.stderr)
        sys.exit(1)
    print('ok', dll)


if __name__ == '__main__':
    main()
