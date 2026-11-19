//! Zeitmessung für die Bildtaktung. Nur aktiv, wenn `PASTLINK_PERF=<datei>` gesetzt ist;
//! dann wird einmal pro Sekunde eine Zusammenfassung angehängt. Sonst kostet es nichts.

use std::cell::RefCell;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

fn target() -> Option<&'static str> {
    static T: OnceLock<Option<String>> = OnceLock::new();
    T.get_or_init(|| std::env::var("PASTLINK_PERF").ok().filter(|s| !s.is_empty())).as_deref()
}

pub fn on() -> bool {
    target().is_some()
}

#[derive(Default)]
struct Stat {
    n: u32,
    sum: f64,
    max: f64,
    last: f64,
}

struct State {
    t0: Instant,
    last_present: Option<Instant>,
    stats: Vec<(&'static str, Stat)>,
    /// Anzahl Anzeige-Bilder mit 0, 1, 2, 3+ emulierten Bildern
    emu_hist: [u32; 4],
    /// in diesem Anzeige-Bild wurde ein neues großes Bild an GPUI übergeben
    img: bool,
}

thread_local! {
    static S: RefCell<State> = RefCell::new(State { t0: Instant::now(), last_present: None, stats: Vec::new(), emu_hist: [0; 4], img: false });
}

/// Dauer (ms) eines Abschnitts verbuchen
pub fn add(name: &'static str, ms: f64) {
    if !on() {
        return;
    }
    S.with(|s| {
        let mut s = s.borrow_mut();
        let st = match s.stats.iter().position(|(n, _)| *n == name) {
            Some(i) => &mut s.stats[i].1,
            None => {
                s.stats.push((name, Stat::default()));
                &mut s.stats.last_mut().unwrap().1
            }
        };
        st.n += 1;
        st.last = ms;
        st.sum += ms;
        st.max = st.max.max(ms);
    });
}

/// Abschnitt messen
pub fn time<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    if !on() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    add(name, t.elapsed().as_secs_f64() * 1000.0);
    r
}

/// Ein neues Bild wurde an GPUI übergeben (muss hochgeladen werden)
pub fn new_image() {
    if on() {
        S.with(|s| s.borrow_mut().img = true);
    }
}

/// Wie viele Spielbilder in diesem Anzeige-Bild emuliert wurden
pub fn emu_frames(n: u32) {
    if !on() {
        return;
    }
    S.with(|s| s.borrow_mut().emu_hist[(n as usize).min(3)] += 1);
}

/// Ende eines Anzeige-Bildes: Abstand zum vorigen messen, ggf. Zusammenfassung schreiben
pub fn present() {
    let Some(path) = target() else { return };
    let now = Instant::now();
    let (prev, img) = S.with(|s| {
        let mut s = s.borrow_mut();
        (s.last_present.replace(now), std::mem::take(&mut s.img))
    });
    if let Some(p) = prev {
        let ms = (now - p).as_secs_f64() * 1000.0;
        add("abstand", ms);
        add(if img { "abst.bild" } else { "abst.ohne" }, ms);
        if ms > 8.0 {
            let mut l = format!("  hänger {ms:.1} ms:");
            S.with(|s| {
                for (n, st) in &s.borrow().stats {
                    l += &format!(" {n}={:.2}", st.last);
                }
            });
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{l}");
            }
        }
    }
    S.with(|s| {
        let mut s = s.borrow_mut();
        if s.t0.elapsed().as_secs_f64() < 1.0 {
            return;
        }
        let mut out = format!("emu/anzeige 0:{} 1:{} 2:{} 3+:{} |", s.emu_hist[0], s.emu_hist[1], s.emu_hist[2], s.emu_hist[3]);
        for (n, st) in &s.stats {
            out += &format!(" {n} n={} avg={:.2} max={:.2} |", st.n, st.sum / st.n.max(1) as f64, st.max);
        }
        s.stats.clear();
        s.emu_hist = [0; 4];
        s.t0 = now;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{out}");
        }
    });
}

/// Messlauf: Spielstand-Platz, der beim Start geladen wird (`PASTLINK_PERF_SLOT`)
pub fn start_slot() -> Option<u8> {
    on().then(|| std::env::var("PASTLINK_PERF_SLOT").ok()?.parse().ok()).flatten()
}

/// Messlauf: Link läuft automatisch hin und her (`PASTLINK_PERF_WALK=1`), damit gescrollt wird
pub fn walk_pad() -> u16 {
    static W: OnceLock<bool> = OnceLock::new();
    if !on() || !*W.get_or_init(|| std::env::var("PASTLINK_PERF_WALK").is_ok()) {
        return 0;
    }
    static T0: OnceLock<Instant> = OnceLock::new();
    let secs = T0.get_or_init(Instant::now).elapsed().as_secs_f64();
    match (secs / 2.5) as u32 % 4 {
        0 => 0x0100, // rechts
        1 => 0x0800, // hoch
        2 => 0x0200, // links
        _ => 0x0400, // runter
    }
}
