//! Arayüz metinleri. Dil, açılışta işletim sisteminin arayüz dilinden seçilir: Türkçe sistemde
//! Türkçe, diğer her dilde İngilizce.
//!
//! Yeni bir dil eklemek: aşağıya bir `Strings` tablosu yaz ve `detect`'te o dilin koduna bağla.
//! Metinlerde `{v}` yeni sürüm, `{cur}` çalışan sürüm numarasıyla doldurulur ([`fill`]).

use std::sync::OnceLock;

pub struct Strings {
    pub refresh: &'static str,
    pub autostart: &'static str,
    pub check_updates: &'static str,
    pub quit: &'static str,
    /// Menünün başındaki öğe: `{v}`
    pub update_to: &'static str,
    /// Elle denetimde yeni sürüm bulununca sorulan soru: `{v}`, `{cur}`
    pub ask_update: &'static str,
    /// `{cur}`
    pub up_to_date: &'static str,
    pub check_failed: &'static str,
    /// macOS: elle denetimin sonucu menü satırında yazar. `{v}` / `{cur}`
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

/// Bu sistemin dilindeki metinler (ilk çağrıda bir kez belirlenir).
pub fn t() -> &'static Strings {
    static CHOSEN: OnceLock<&'static Strings> = OnceLock::new();
    CHOSEN.get_or_init(|| for_language(&system_language()))
}

/// `{v}` ve `{cur}` yer tutucularını doldurur.
pub fn fill(template: &str, new_version: &str) -> String {
    template.replace("{v}", new_version).replace("{cur}", crate::update::CURRENT)
}

/// Dil kodundan ("tr", "tr-TR", "en-US"…) tablo; desteklenmeyen dilde İngilizce.
fn for_language(code: &str) -> &'static Strings {
    match code.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("tr") => &TR,
        _ => &EN,
    }
}

/// İşletim sisteminin arayüz dili, iki harfli kodla başlayan bir dizgi olarak.
#[cfg(windows)]
fn system_language() -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // LANGID'nin alt 10 biti ana dildir; 0x1F = Türkçe.
    match unsafe { GetUserDefaultUILanguage() } & 0x3FF {
        0x1F => "tr".into(),
        _ => "en".into(),
    }
}

/// macOS: tercih edilen dillerin ilki (`defaults read -g AppleLanguages` → `("tr-TR", "en-US")`).
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
        assert_eq!(for_language("de-DE").quit, "Quit"); // desteklenmeyen → İngilizce
        assert_eq!(for_language("").quit, "Quit");
    }

    #[test]
    fn placeholders() {
        let s = fill(EN.ask_update, "9.9.9");
        assert!(s.starts_with("Chargestrip 9.9.9 is available (installed: "));
        assert!(!s.contains('{'));
    }
}
