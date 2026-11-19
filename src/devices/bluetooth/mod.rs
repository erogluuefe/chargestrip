//! Bluetooth cihazlar: işletim sisteminin bağlı cihazlardan okuyup sakladığı pil seviyesi
//! (Windows'ta Ayarlar → Bluetooth, macOS'ta Bluetooth menüsünde görünen değer). Model başına
//! sürücü gerekmez; cihaza hiçbir şey gönderilmez. Bluetooth şarj durumu bildirmez.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use super::Reading;

/// Bağlı ve pil seviyesi bilinen Bluetooth cihazlar, ada/anahtara göre sabit sırada.
pub fn poll() -> Vec<Reading> {
    #[cfg(windows)]
    return windows::poll();
    #[cfg(target_os = "macos")]
    return macos::poll();
}
