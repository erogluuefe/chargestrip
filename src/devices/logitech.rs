//! Logitech kablosuz alıcıları (LIGHTSPEED, Unifying, Bolt) ve onlara bağlı fare/klavyeler,
//! HID++ 2.0 ile. Özellikler cihaza sorularak bulunduğundan model listesi gerekmez.
//!
//! Mesaj (uzun rapor, 20 bayt): `[0x11, cihaz no, özellik sırası, fonksiyon << 4 | yazılım no, …]`.
//! Cevap aynı ilk dört baytla gelir; hata: `[0x11, cihaz no, 0xFF, özellik sırası, fonksiyon…]`.
//! Kullanılan istekler yalnızca okur:
//! - Kök özellik (`0x0000`): fn 1 = protokol sürümü (cihaz bağlı mı?), fn 0 = bir özelliğin sırası.
//! - `0x1004` birleşik pil: fn 1 → `[yüzde, seviye bitleri, şarj durumu]`.
//! - `0x1000` pil durumu (eski cihazlar): fn 0 → `[yüzde, sonraki seviye, şarj durumu]`.
//! - `0x0005` cihaz adı/türü: fn 2 → tür (klavye, fare, kulaklık…).
//!
//! Alıcıya en çok altı cihaz eşlenebilir (cihaz no 1–6); pili okunabilen ilk cihaz gösterilir.
//! Yazılım numarası, Logitech G HUB'ın kendi cevaplarını bizimkilerden ayırır.

use super::{exchange_within, Driver, Found};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;
use std::time::Duration;

pub const RECEIVERS: Driver = Driver {
    kind: Kind::Keyboard, // tür cihazdan öğrenilemezse
    vid: 0x046D,
    pids: &[
        0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC545, 0xC547, 0xC54D, // LIGHTSPEED
        0xC52B, 0xC532, // Unifying
        0xC548, // Bolt
    ],
    usage_page: 0xFF00,
    usage: 0x0002, // HID++ uzun rapor arayüzü
    read,
};

const SOFTWARE_ID: u8 = 0x0D;
const ROOT: u8 = 0x00;
const UNIFIED_BATTERY: u16 = 0x1004;
const BATTERY_STATUS: u16 = 0x1000;
const DEVICE_NAME: u16 = 0x0005;
/// Eşli olmayan ya da uyuyan cihazda cevap gelmez; tüm sıraları denemek uzun sürmesin.
const TIMEOUT: Duration = Duration::from_millis(300);

fn read(dev: &HidDevice, _pid: u16) -> Option<Found> {
    (1..=6).find_map(|index| {
        // Protokol sürümü 2.0 ve üstü mü (cihaz bağlı ve HID++ 2.0 konuşuyor)?
        if *call(dev, index, ROOT, 1, &[0, 0, 0x5A])?.first()? < 2 {
            return None;
        }
        let battery = if let Some(f) = feature(dev, index, UNIFIED_BATTERY) {
            unified_battery(&call(dev, index, f, 1, &[])?)
        } else {
            battery_status(&call(dev, index, feature(dev, index, BATTERY_STATUS)?, 0, &[])?)
        }?;
        let kind = feature(dev, index, DEVICE_NAME)
            .and_then(|f| call(dev, index, f, 2, &[]))
            .and_then(|p| kind(*p.first()?));
        Some(Found { battery, kind })
    })
}

/// HID++ 2.0 çağrısı; cevabın parametreleri, hata ya da zaman aşımında `None`.
fn call(dev: &HidDevice, index: u8, feature: u8, function: u8, params: &[u8]) -> Option<Vec<u8>> {
    let mut msg = [0u8; 20];
    msg[..4].copy_from_slice(&[0x11, index, feature, function << 4 | SOFTWARE_ID]);
    msg[4..4 + params.len()].copy_from_slice(params);
    exchange_within(dev, &msg, TIMEOUT, |r| match r {
        [0x11, i, f, fs, rest @ ..] if *i == index && *f == feature && *fs == msg[3] => Some(Some(rest.to_vec())),
        [0x11, i, 0xFF, f, fs, ..] if *i == index && *f == feature && *fs == msg[3] => Some(None),
        _ => None,
    })?
}

/// Bir özelliğin cihazdaki sırası; cihazda yoksa `None`.
fn feature(dev: &HidDevice, index: u8, id: u16) -> Option<u8> {
    call(dev, index, ROOT, 0, &id.to_be_bytes())?.first().copied().filter(|&f| f != 0)
}

/// `0x1004`: yüzde biliniyorsa o, yoksa seviye bitlerinden (tam/iyi/düşük/kritik) yaklaşık değer.
fn unified_battery(p: &[u8]) -> Option<Battery> {
    let (percent, level, status) = (p[0], p[1], p[2]);
    let percent = match percent {
        1..=100 => percent,
        _ if level & 8 != 0 => 100,
        _ if level & 4 != 0 => 60,
        _ if level & 2 != 0 => 20,
        _ if level & 1 != 0 => 5,
        _ => return None,
    };
    Some(Battery { percent, charging: matches!(status, 1..=3) })
}

/// `0x1000`: durum 1–4 şarjla ilgili; 3 = doldu (yüzde 0 bildirilse de).
fn battery_status(p: &[u8]) -> Option<Battery> {
    let (percent, status) = (p[0], p[2]);
    let percent = if percent == 0 && status == 3 { 100 } else { percent };
    (percent <= 100).then_some(Battery { percent, charging: matches!(status, 1..=4) })
}

/// `0x0005` fn 2'nin döndürdüğü cihaz türü.
fn kind(device_type: u8) -> Option<Kind> {
    match device_type {
        0x00 | 0x02 => Some(Kind::Keyboard),
        0x03..=0x05 => Some(Kind::Mouse), // fare, trackpad, trackball
        0x08 => Some(Kind::Headset),
        0x0B | 0x0C => Some(Kind::Gamepad),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(percent: u8, charging: bool) -> Option<Battery> {
        Some(Battery { percent, charging })
    }

    #[test]
    fn unified() {
        assert_eq!(unified_battery(&[73, 4, 0]), b(73, false));
        assert_eq!(unified_battery(&[40, 4, 1]), b(40, true));
        assert_eq!(unified_battery(&[0, 2, 0]), b(20, false)); // yalnızca seviye: düşük
        assert_eq!(unified_battery(&[0, 0, 0]), None);
    }

    #[test]
    fn status() {
        assert_eq!(battery_status(&[55, 50, 0]), b(55, false));
        assert_eq!(battery_status(&[0, 0, 3]), b(100, true)); // doldu
        assert_eq!(battery_status(&[200, 0, 0]), None);
    }

    #[test]
    fn device_type() {
        assert_eq!(kind(0x00), Some(Kind::Keyboard));
        assert_eq!(kind(0x03), Some(Kind::Mouse));
        assert_eq!(kind(0x07), None); // uzaktan kumanda vb.
    }
}
