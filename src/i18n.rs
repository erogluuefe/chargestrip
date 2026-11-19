//! UI strings. The language is picked at start-up from the operating system's UI language:
//! Turkish on a Turkish system, English for every other language.
//!
//! To add a language: write a `Strings` table below and map its code in [`for_language`].
//! In the strings, `{v}` is replaced with the new version and `{cur}` with the running one ([`fill`]).

use std::sync::OnceLock;

pub struct Strings {
    pub refresh: &'static str,
    pub autostart: &'static str,
    pub check_updates: &'static str,
    pub quit: &'static str,
    /// Entry at the top of the menu: `{v}`
    pub update_to: &'static str,
    /// Windows: message boxes after a manual check. Question when a new version is found:
    /// `{v}`, `{cur}`
    #[cfg_attr(not(windows), allow(dead_code))]
    pub ask_update: &'static str,
    /// `{cur}`
    #[cfg_attr(not(windows), allow(dead_code))]
    pub up_to_date: &'static str,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub check_failed: &'static str,
    /// macOS: a manual check's result is written into the menu entry. `{v}` / `{cur}`
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub label_new_version: &'static str,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub label_up_to_date: &'static str,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub label_check_failed: &'static str,
}

const EN: Strings = Strings {
    refresh: "Refresh",
    autostart: "Start at login",
    check_updates: "Check for updates",
    quit: "Quit",
    update_to: "Update to {v}",
    ask_update: "Chargestrip {v} is available (installed: {cur}).\nUpdate now?",
    up_to_date: "Chargestrip is up to date ({cur}).",
    check_failed: "Couldn't check for updates. Check your internet connection and try again.",
    label_new_version: "Update available: {v} (at the top)",
    label_up_to_date: "Up to date ({cur}) ✓ — check again",
    label_check_failed: "Couldn't check — try again",
};

const TR: Strings = Strings {
    refresh: "Yenile",
    autostart: "Açılışta başlat",
    check_updates: "Güncellemeleri denetle",
    quit: "Çıkış",
    update_to: "Güncelle ({v})",
    ask_update: "Chargestrip {v} yayımlandı (kurulu: {cur}).\nŞimdi güncellensin mi?",
    up_to_date: "Chargestrip güncel ({cur}).",
    check_failed: "Güncelleme denetlenemedi. İnternet bağlantısını kontrol edip tekrar dene.",
    label_new_version: "Yeni sürüm var: {v} (en üstte)",
    label_up_to_date: "Güncel ({cur}) ✓ — tekrar denetle",
    label_check_failed: "Denetlenemedi — tekrar dene",
};

/// Strings in this system's language (decided once, on first use).
pub fn t() -> &'static Strings {
    static CHOSEN: OnceLock<&'static Strings> = OnceLock::new();
    CHOSEN.get_or_init(|| for_language(&system_language()))
}

/// Fills in the `{v}` and `{cur}` placeholders.
pub fn fill(template: &str, new_version: &str) -> String {
    template.replace("{v}", new_version).replace("{cur}", crate::update::CURRENT)
}

/// Table for a language code ("tr", "tr-TR", "en-US"…); English for unsupported languages.
fn for_language(code: &str) -> &'static Strings {
    match code.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("tr") => &TR,
        _ => &EN,
    }
}

/// The operating system's UI language, as a string starting with the two-letter code.
#[cfg(windows)]
fn system_language() -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // The low 10 bits of a LANGID are the primary language; 0x1F = Turkish.
    match unsafe { GetUserDefaultUILanguage() } & 0x3FF {
        0x1F => "tr".into(),
        _ => "en".into(),
    }
}

/// macOS: the first preferred language (`defaults read -g AppleLanguages` → `("tr-TR", "en-US")`).
#[cfg(target_os = "macos")]
fn system_language() -> String {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.split('"').nth(1).map(str::to_string)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_choice() {
        assert_eq!(for_language("tr-TR").quit, "Çıkış");
        assert_eq!(for_language("TR").quit, "Çıkış");
        assert_eq!(for_language("en-US").quit, "Quit");
        assert_eq!(for_language("de-DE").quit, "Quit"); // unsupported → English
        assert_eq!(for_language("").quit, "Quit");
    }

    #[test]
    fn placeholders() {
        let s = fill(EN.ask_update, "9.9.9");
        assert!(s.starts_with("Chargestrip 9.9.9 is available (installed: "));
        assert!(!s.contains('{'));
    }
}
