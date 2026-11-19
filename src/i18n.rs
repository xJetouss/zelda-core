//! Zweisprachige Oberfläche (Deutsch / Englisch).

/// Gibt je nach Sprache den deutschen oder englischen Text zurück.
#[inline]
pub fn tr(en: bool, de: &'static str, eng: &'static str) -> &'static str {
    if en {
        eng
    } else {
        de
    }
}

/// Menüsprache automatisch aus der Systemsprache: Deutsch, wenn Windows auf Deutsch läuft, sonst Englisch.
pub fn system_is_english() -> bool {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        let id = unsafe { GetUserDefaultUILanguage() };
        (id & 0x3FF) != 0x07
    }
    #[cfg(not(windows))]
    {
        !std::env::var("LANG").unwrap_or_default().starts_with("de")
    }
}
