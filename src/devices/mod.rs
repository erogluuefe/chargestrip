//! Cihaz sürücüleri.
//!
//! Her sürücü bir [`Driver`] tanımıdır: hangi USB alıcısına (VID/PID + usage page) bağlanılacağı
//! ve açılan HID arayüzünden pilin nasıl okunacağı. Arayüz katmanları cihaz sayısından
//! bağımsızdır; yeni cihaz eklemek için:
//!
//! 1. `src/devices/<marka_model>.rs` dosyasını oluştur: `pub const DRIVER: Driver` ve
//!    `fn read(...)`. Protokolü kayıtlardan alınmış çerçevelerle birim testine bağla.
//! 2. Dosyayı aşağıda `mod` olarak ekle ve [`DRIVERS`] listesine bir satır yaz.
//!
//! Sürücüler cihaza yalnızca **salt-okuma** istekleri gönderir; ayar değiştirebilecek bir
//! komut asla gönderilmez. Protokol çözme yolu ve araçları için: README.md.

mod arctis_nova7;
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

/// Desteklenen cihazlar; arayüzde bu sırayla gösterilir.
pub const DRIVERS: &[Driver] = &[arctis_nova7::DRIVER, waizowl_ogm::DRIVER];

/// Bir cevabın beklenebileceği en uzun süre.
const TIMEOUT: Duration = Duration::from_millis(1000);

/// Takılı alıcıları sorgular. Dönen her öğe: (`DRIVERS` içindeki sıra, okuma).
/// Alıcısı takılı olmayan sürücüler listede yer almaz.
pub fn poll(api: &mut HidApi) -> Vec<(usize, Option<Battery>)> {
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
            Some((i, battery))
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
/// Üreticiye özel (usage page ≥ 0xFF00) tüm HID arayüzlerini ve sürücülerin okumalarını yazdırır;
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
        for (i, battery) in poll(&mut api) {
            println!("{:?}: {battery:?}", DRIVERS[i].kind);
        }
    }
}
