//! Mod-Shop: alle Mods kostenlos zum Herunterladen. Die Mods stecken nicht in der App, sondern liegen als
//! DLLs im GitHub-Release „mods“ des Projekts (`index.json` + DLLs). Heruntergeladen wird mit dem in Windows
//! eingebauten `curl.exe` im Hintergrund; installiert wird in den Mod-Ordner neben der .exe.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// GitHub-Repository des Projekts
pub const REPO: &str = "xJetouss/zelda-core";

/// Liste aller Mods im Shop
pub fn index_url() -> String {
    std::env::var("PASTLINK_SHOP_INDEX").unwrap_or_else(|_| format!("https://github.com/{REPO}/releases/download/mods/index.json"))
}

#[derive(Clone, Deserialize)]
pub struct Item {
    /// Kennung (= Dateiname ohne .dll)
    pub id: String,
    pub file: String,
    pub name: String,
    #[serde(default)]
    pub name_en: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub desc_en: String,
    pub version: String,
    /// Download-Adresse der DLL (sonst neben index.json)
    #[serde(default)]
    pub url: String,
}

#[derive(Deserialize)]
struct Index {
    mods: Vec<Item>,
}

#[derive(Clone, PartialEq)]
pub enum Status {
    Idle,
    Loading,
    Ready,
    Error(String),
}

#[derive(Default)]
struct State {
    status: Option<Status>,
    items: Vec<Item>,
    busy: HashSet<String>,
    failed: HashMap<String, String>,
    /// neue Dateien im Mod-Ordner (Mods neu laden)
    new_files: bool,
}

#[derive(Clone, Default)]
pub struct Shop {
    st: Arc<Mutex<State>>,
}

/// Zustand eines Eintrags
#[derive(Clone, Copy, PartialEq)]
pub enum Have {
    No,
    Busy,
    Installed,
    /// installiert, aber ältere Version
    Update,
    /// neue Version liegt bereit (gilt nach Neustart)
    Restart,
}

fn dir() -> PathBuf {
    super::Mods::dir()
}

/// installierte Versionen (`mods/shop.json`: Dateiname -> Version)
fn versions() -> HashMap<String, String> {
    std::fs::read(dir().join("shop.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_version(file: &str, v: &str) {
    let mut m = versions();
    m.insert(file.to_string(), v.to_string());
    if let Ok(s) = serde_json::to_vec_pretty(&m) {
        let _ = std::fs::write(dir().join("shop.json"), s);
    }
}

/// Datei herunterladen (curl.exe, ohne Konsolenfenster)
fn download(url: &str) -> Result<Vec<u8>, String> {
    let mut c = std::process::Command::new("curl.exe");
    c.args(["-fsSL", "--max-time", "60", "--retry", "2", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().map_err(|e| format!("curl.exe nicht gefunden ({e})"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { format!("Download fehlgeschlagen ({})", out.status) } else { err });
    }
    Ok(out.stdout)
}

impl Shop {
    pub fn status(&self) -> Status {
        self.st.lock().unwrap().status.clone().unwrap_or(Status::Idle)
    }
    pub fn items(&self) -> Vec<Item> {
        self.st.lock().unwrap().items.clone()
    }
    pub fn error_of(&self, id: &str) -> Option<String> {
        self.st.lock().unwrap().failed.get(id).cloned()
    }
    /// neue Dateien seit dem letzten Aufruf?
    pub fn take_new(&self) -> bool {
        std::mem::take(&mut self.st.lock().unwrap().new_files)
    }

    /// Liste aus dem Netz laden (im Hintergrund)
    pub fn refresh(&self) {
        {
            let mut s = self.st.lock().unwrap();
            if s.status == Some(Status::Loading) {
                return;
            }
            s.status = Some(Status::Loading);
        }
        let st = self.st.clone();
        std::thread::spawn(move || {
            let r = download(&index_url()).and_then(|b| serde_json::from_slice::<Index>(&b).map_err(|e| format!("Liste unlesbar: {e}")));
            let mut s = st.lock().unwrap();
            match r {
                Ok(i) => {
                    s.items = i.mods.into_iter().filter(|m| !m.file.contains(['/', '\\']) && m.file.to_ascii_lowercase().ends_with(".dll")).collect();
                    s.status = Some(Status::Ready);
                }
                Err(e) => s.status = Some(Status::Error(e)),
            }
        });
    }

    pub fn have(&self, it: &Item) -> Have {
        if self.st.lock().unwrap().busy.contains(&it.id) {
            return Have::Busy;
        }
        let d = dir();
        if d.join(format!("{}.new", it.file)).exists() {
            return Have::Restart;
        }
        if !d.join(&it.file).exists() {
            return Have::No;
        }
        match versions().get(&it.file) {
            Some(v) if *v == it.version => Have::Installed,
            Some(_) => Have::Update,
            // selbst hineinkopiert: als installiert zählen
            None => Have::Installed,
        }
    }

    /// Mod herunterladen und installieren (im Hintergrund). `loaded`: diese DLL ist schon geladen
    /// (dann als `.new` ablegen, gilt nach dem nächsten Start)
    pub fn install(&self, it: &Item, loaded: bool) {
        {
            let mut s = self.st.lock().unwrap();
            if !s.busy.insert(it.id.clone()) {
                return;
            }
            s.failed.remove(&it.id);
        }
        let st = self.st.clone();
        let it = it.clone();
        std::thread::spawn(move || {
            let url = if it.url.is_empty() { index_url().rsplit_once('/').map(|(b, _)| format!("{b}/{}", it.file)).unwrap_or_default() } else { it.url.clone() };
            let r = download(&url).and_then(|b| {
                // eine Windows-DLL beginnt mit „MZ“
                if b.len() < 1024 || &b[..2] != b"MZ" {
                    return Err("keine gültige DLL".into());
                }
                let d = dir();
                std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
                let target = if loaded { d.join(format!("{}.new", it.file)) } else { d.join(&it.file) };
                std::fs::write(&target, b).map_err(|e| e.to_string())?;
                save_version(&it.file, &it.version);
                Ok(())
            });
            let mut s = st.lock().unwrap();
            s.busy.remove(&it.id);
            match r {
                Ok(()) => s.new_files = true,
                Err(e) => {
                    s.failed.insert(it.id.clone(), e);
                }
            }
        });
    }
}
