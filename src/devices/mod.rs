//! Cihaz sürücüleri. İki kaynak var, ikisi de aynı [`Reading`] listesini üretir:
//!
//! - **USB alıcılı cihazlar:** her model bir [`Driver`] tanımıdır: hangi alıcıya (VID/PID + usage
//!   page) bağlanılacağı ve açılan HID arayüzünden pilin nasıl okunacağı.
//! - **Bluetooth:** işletim sisteminin bağlı cihazlar için zaten tuttuğu pil seviyesi
//!   ([`bluetooth`]); model başına kod gerekmez.
//!
//! Arayüz katmanları cihaz sayısından bağımsızdır; yeni bir USB alıcılı cihaz eklemek için:
//!
//! 1. `src/devices/<marka_model>.rs` dosyasını oluştur: `pub const DRIVER: Driver` ve
//!    `fn read(...)`. Protokolü kayıtlardan alınmış çerçevelerle birim testine bağla.
//! 2. Dosyayı aşağıda `mod` olarak ekle ve [`DRIVERS`] listesine bir satır yaz.
//!
//! Sürücüler cihaza yalnızca **salt-okuma** istekleri gönderir; ayar değiştirebilecek bir
//! komut asla gönderilmez. Protokol çözme yolu ve araçları için: README.md.

mod arctis_nova7;
mod bluetooth;
mod waizowl_ogm;

use crate::model::{Battery, Kind};
use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

/// Bir cihaz modelinin tanımı.
pub struct Driver {
    /// Gösterilecek ikon.
    pub kind: Kind,
    /// USB alıcısının üretici kimliği.
    pub vid: u16,
    /// Desteklenen ürün kimlikleri (aynı protokolü konuşan alıcı/kablo varyantları).
    pub pids: &'static [u16],
    /// Pil bilgisinin okunduğu HID arayüzünün usage page değeri (genelde 0xFFxx, üreticiye özel).
    pub usage_page: u16,
    /// Açılmış arayüzden pili okur. `pid` aynı sürücüdeki varyantları ayırmak içindir.
    /// `None`: cihaz kapalı/uykuda ya da geçerli cevap gelmedi.
    pub read: fn(dev: &HidDevice, pid: u16) -> Option<Battery>,
}

/// Bir cihazın bir turdaki okuması.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Turlar arasında aynı cihazı tanımak için kalıcı anahtar (son değeri hatırlamak için).
    pub key: String,
    pub kind: Kind,
    /// `None`: alıcı takılı ama cihaz kapalı/uykuda ya da geçerli cevap gelmedi.
    pub battery: Option<Battery>,
}

/// Desteklenen USB alıcılı cihazlar; arayüzde bu sırayla, Bluetooth cihazlardan önce gösterilir.
pub const DRIVERS: &[Driver] = &[arctis_nova7::DRIVER, waizowl_ogm::DRIVER];

/// Bir cevabın beklenebileceği en uzun süre.
const TIMEOUT: Duration = Duration::from_millis(1000);

/// Takılı alıcıları ve bağlı Bluetooth cihazları sorgular. Alıcısı takılı olmayan sürücüler ve
/// bağlı olmayan Bluetooth cihazlar listede yer almaz. `api` yoksa (HID açılamadı) yalnızca Bluetooth.
pub fn poll(api: Option<&mut HidApi>) -> Vec<Reading> {
    let mut readings = api.map(poll_receivers).unwrap_or_default();
    readings.extend(bluetooth::poll());
    readings
}

fn poll_receivers(api: &mut HidApi) -> Vec<Reading> {
    // Alıcılar takılıp çıkarılabildiği için liste her turda yenilenir.
    if api.refresh_devices().is_err() {
        return Vec::new();
    }
    DRIVERS
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let info = api.device_list().find(|info| {
                info.vendor_id() == d.vid
                    && d.pids.contains(&info.product_id())
                    && info.usage_page() == d.usage_page
            })?;
            let battery = info.open_device(api).ok().and_then(|dev| (d.read)(&dev, info.product_id()));
            Some(Reading { key: format!("usb:{i}"), kind: d.kind, battery })
        })
        .collect()
}

/// İsteği gönderir ve `accept` bir değer döndürene kadar gelen raporları okur.
/// Arada cihazın kendiliğinden yolladığı bildirimler `accept`'in `None` döndürmesiyle atlanır.
/// Zaman aşımında veya HID hatasında `None`.
pub(crate) fn exchange<T>(
    dev: &HidDevice,
    request: &[u8],
    mut accept: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    dev.write(request).ok()?;
    let deadline = Instant::now() + TIMEOUT;
    let mut buf = [0u8; 128];
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let n = dev.read_timeout(&mut buf, left.as_millis().max(1) as i32).ok()?;
        if n == 0 {
            return None;
        }
        if let Some(value) = accept(&buf[..n]) {
            return Some(value);
        }
    }
}

/// Gerçek donanımla deneme: `cargo test probe -- --ignored --nocapture`
/// Üreticiye özel (usage page ≥ 0xFF00) tüm HID arayüzlerini ve tüm okumaları (Bluetooth dahil) yazdırır;
/// yeni bir cihazın VID/PID/usage page değerlerini bulmak için de kullanılır.
#[cfg(test)]
mod probe {
    use super::*;

    #[test]
    #[ignore]
    fn probe() {
        let mut api = HidApi::new().expect("hidapi");
        for d in api.device_list().filter(|d| d.usage_page() >= 0xFF00) {
            println!(
                "{:04x}:{:04x} arayüz={} usage_page={:04x} usage={:04x} {}",
                d.vendor_id(), d.product_id(), d.interface_number(), d.usage_page(), d.usage(),
                d.product_string().unwrap_or_default()
            );
        }
        for r in poll(Some(&mut api)) {
            println!("{} {:?}: {:?}", r.key, r.kind, r.battery);
        }
    }
}
