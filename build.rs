//! Windows: Triforce als Symbol der .exe (Taskleiste, Explorer).
//! Nutzt rc.exe aus dem Windows SDK; fehlt es, wird ohne Symbol gebaut (das Fenster setzt es trotzdem).

#[path = "src/recomp/gen.rs"]
#[allow(dead_code)]
mod recomp_gen;
#[path = "src/recomp/analyze.rs"]
#[allow(dead_code)]
mod analyze;

/// Recompiler: übersetzt den aufgezeichneten ROM-Code nach Rust (`recomp/rom.bin` + `recomp/trace.txt`,
/// erzeugt mit `cargo test --release dev_recomp_trace -- --ignored`). Fehlt beides, gibt es keinen
/// Recompiled-Modus – alles andere funktioniert unverändert.
/// Öffentlicher Build (`PASTLINK_PUBLIC=1`, z. B. für GitHub-Releases): ohne jede Spieldaten – kein übersetzter
/// ROM-Code, keine eingebaute ROM, keine Zelda-3-Daten, kein Spiel-Logo. Der Zelda-3-Kern selbst (MIT) ist dabei;
/// seine Daten (`zelda3_assets.dat`) können neben die .exe gelegt werden.
fn public() -> bool {
    println!("cargo:rerun-if-env-changed=PASTLINK_PUBLIC");
    std::env::var("PASTLINK_PUBLIC").is_ok_and(|v| v == "1")
}

/// Bild für die Spielkarte im Startmenü: das Logo aus `assets/alttp_logo.rgba` (nur eigene Builds) oder ein
/// schlichtes Triforce-Bild (öffentliche Builds)
fn logo() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("assets").join("alttp_logo.rgba");
    println!("cargo:rerun-if-changed={}", src.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("logo.rgba");
    let n = 384usize;
    let data = match std::fs::read(&src) {
        Ok(b) if !public() && b.len() == n * n * 4 => b,
        _ => {
            // goldenes Triforce auf dunkelblauem Grund
            let mut px = vec![0u8; n * n * 4];
            let inside = |x: f32, y: f32, cx: f32, top: f32, h: f32| y >= top && y < top + h && (x - cx).abs() <= (y - top) / h * h / 3f32.sqrt();
            for y in 0..n {
                for x in 0..n {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let h = 130.0;
                    let tri = inside(fx, fy, 192.0, 62.0, h) || inside(fx, fy, 192.0 - h / 3f32.sqrt(), 62.0 + h, h) || inside(fx, fy, 192.0 + h / 3f32.sqrt(), 62.0 + h, h);
                    let c: [u8; 4] = if tri { [0xF0, 0xC0, 0x30, 0xFF] } else { [0x18, 0x1C, 0x30, 0xFF] };
                    px[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
                }
            }
            px
        }
    };
    if std::fs::read(&out).ok().as_deref() != Some(data.as_slice()) {
        std::fs::write(&out, data).unwrap();
    }
}

fn recompile() {
    let dir = std::env::var("PASTLINK_RECOMP_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("recomp"));
    // öffentlicher Build: nichts aus der ROM übernehmen
    let dir = if public() { dir.join("__nicht_im_oeffentlichen_build__") } else { dir };
    let (rom_p, trace_p) = (dir.join("rom.bin"), dir.join("trace.txt"));
    println!("cargo:rerun-if-env-changed=PASTLINK_RECOMP_DIR");
    println!("cargo:rerun-if-changed=src/recomp/gen.rs");
    println!("cargo:rerun-if-changed={}", rom_p.display());
    println!("cargo:rerun-if-changed={}", trace_p.display());
    println!("cargo:rerun-if-changed={}", dir.join("orig.bin").display());
    println!("cargo:rerun-if-changed={}", dir.join("name.txt").display());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // Spieldaten für den Recompiled-Modus (die .exe braucht dann keine ROM-Datei mehr – nicht weitergeben!)
    let data = out_dir.join("recomp_data.rs");
    let data_src = match (std::fs::metadata(dir.join("orig.bin")), std::fs::read_to_string(dir.join("name.txt"))) {
        (Ok(_), Ok(name)) => {
            std::fs::copy(dir.join("orig.bin"), out_dir.join("orig.bin")).unwrap();
            format!("pub static ORIG_ROM: &[u8] = include_bytes!(\"orig.bin\");
pub static ROM_NAME: &str = {:?};
", name.trim())
        }
        _ => "pub static ORIG_ROM: &[u8] = &[];
pub static ROM_NAME: &str = \"\";
".to_string(),
    };
    if std::fs::read_to_string(&data).ok().as_deref() != Some(data_src.as_str()) {
        std::fs::write(&data, data_src).unwrap();
    }
    let out = out_dir.join("recomp_gen.rs");
    let src = match (std::fs::read(&rom_p), std::fs::read_to_string(&trace_p)) {
        (Ok(rom), Ok(trace)) => {
            let (src, st) = recomp_gen::generate(&rom, &trace, 1);
            println!("cargo:warning=Recompiler: {} Blöcke, {} Befehle übersetzt", st.blocks, st.insts);
            src
        }
        _ => recomp_gen::empty(),
    };
    // nur schreiben, wenn sich etwas geändert hat (spart Neu-Übersetzen)
    if std::fs::read_to_string(&out).ok().as_deref() != Some(src.as_str()) {
        std::fs::write(&out, src).unwrap();
    }
}

/// Zelda 3 (vendor/zelda3, MIT) als C-Bibliothek bauen und die Spieldaten (zelda3_assets.dat) einbetten.
/// Fehlt der Ordner oder die Spieldaten-Datei, gibt es den Zelda-3-Modus nicht – alles andere bleibt gleich.
fn zelda3() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let z = root.join("vendor").join("zelda3");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rustc-check-cfg=cfg(pastlink_z3)");
    println!("cargo:rerun-if-changed=src/z3/glue.c");
    println!("cargo:rerun-if-changed={}", z.join("zelda3_assets.dat").display());
    let data = out_dir.join("z3_data.rs");
    let assets = z.join("zelda3_assets.dat");
    if !z.join("src").join("zelda_rtl.c").exists() {
        std::fs::write(&data, "pub static ASSETS: &[u8] = &[];
").unwrap();
        return;
    }
    // Spieldaten nur in eigenen Builds einbauen (öffentlich: zur Laufzeit aus zelda3_assets.dat neben der .exe)
    if assets.exists() && !public() {
        std::fs::copy(&assets, out_dir.join("zelda3_assets.dat")).unwrap();
        std::fs::write(&data, "pub static ASSETS: &[u8] = include_bytes!(\"zelda3_assets.dat\");
").unwrap();
    } else {
        std::fs::write(&data, "pub static ASSETS: &[u8] = &[];
").unwrap();
    }
    let mut b = cc::Build::new();
    let skip = ["main.c", "config.c", "opengl.c", "glsl_shader.c"];
    for dir in ["src", "snes"] {
        for e in std::fs::read_dir(z.join(dir)).unwrap().flatten() {
            let p = e.path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if p.extension().is_some_and(|x| x == "c") && !skip.contains(&name.as_str()) {
                println!("cargo:rerun-if-changed={}", p.display());
                b.file(&p);
            }
        }
    }
    b.file(z.join("third_party").join("opus-1.3.1-stripped").join("opus_decoder_amalgam.c"));
    b.file(root.join("src").join("z3").join("glue.c"));
    b.include(&z).include(z.join("src")).include(root.join("src").join("z3").join("stub"));
    b.define("_CRT_SECURE_NO_WARNINGS", None).define("NDEBUG", None);
    b.warnings(false).opt_level(2);
    b.compile("zelda3");
    println!("cargo:rustc-cfg=pastlink_z3");
}

fn main() {
    zelda3();
    recompile();
    logo();
    println!("cargo:rerun-if-changed=assets/triforce.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe nicht gefunden – .exe ohne Triforce-Symbol");
        return;
    };
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("triforce.ico");
    let rc_file = out.join("icon.rc");
    let res = out.join("icon.res");
    std::fs::write(&rc_file, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "\\\\"))).unwrap();
    let ok = std::process::Command::new(&rc).arg("/nologo").arg("/fo").arg(&res).arg(&rc_file).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    } else {
        println!("cargo:warning=rc.exe fehlgeschlagen – .exe ohne Triforce-Symbol");
    }
}

/// rc.exe im PATH oder im neuesten Windows SDK (x64)
fn find_rc() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("PATH") {
        for d in std::env::split_paths(&p) {
            let f = d.join("rc.exe");
            if f.is_file() {
                return Some(f);
            }
        }
    }
    let base = std::path::Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut vers: Vec<std::path::PathBuf> = std::fs::read_dir(base).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("x64").join("rc.exe").is_file()).collect();
    vers.sort();
    vers.pop().map(|v| v.join("x64").join("rc.exe"))
}
