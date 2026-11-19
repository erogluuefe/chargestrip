//! Chargestrip: kablosuz kulaklık, fare vb. cihazların pil durumunu Windows'ta taskbar'da,
//! macOS'ta menü çubuğunda gösterir.
//!
//! Katmanlar:
//! - `devices`: cihaz sürücüleri (USB HID üzerinden salt-okuma pil sorguları)
//! - `poller`:  arka planda periyodik sorgu
//! - `model`:   platformdan bağımsız gösterim kararları
//! - `update`:  yeni sürüm denetimi, indirme ve kurma
//! - `i18n`:    arayüz metinleri (sistem diline göre Türkçe / İngilizce)
//! - `win` / `mac`: platforma özgü arayüz

#![windows_subsystem = "windows"]

mod devices;
mod i18n;
mod model;
mod poller;
mod update;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    win::run();
    #[cfg(target_os = "macos")]
    mac::run();
}
