//! Arka plan sorgu döngüsü. HID okumaları (her biri en çok 1 sn) arayüzü bekletmesin diye
//! ayrı bir iş parçacığında, `INTERVAL` aralıkla ya da yenileme istenince hemen çalışır.

use crate::devices::{self, Reading};
use crate::model::{present, Shown};
use hidapi::HidApi;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

pub const INTERVAL: Duration = Duration::from_secs(60);

/// Döngüyü başlatır; her turun sonucu `notify` ile iletilir (takılı alıcısı olan cihazlar
/// `DRIVERS` sırasıyla, ardından bağlı Bluetooth cihazlar). Dönen göndericiye `()` yollamak
/// hemen yeniden sorgular; gönderici düşürülürse döngü biter.
pub fn spawn(notify: impl Fn(Vec<Shown>) + Send + 'static) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("poller".into())
        .stack_size(256 * 1024) // varsayılan 1 MB gereksiz; HID tamponları küçük
        .spawn(move || {
            let mut api: Option<HidApi> = None;
            let mut last = HashMap::new();
            loop {
                if api.is_none() {
                    api = HidApi::new().ok();
                }
                let readings = devices::poll(api.as_mut());
                notify(snapshot(&readings, &mut last));
                match rx.recv_timeout(INTERVAL) {
                    Ok(()) => while rx.try_recv().is_ok() {}, // art arda istekleri birleştir
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("sorgu iş parçacığı başlatılamadı");
    tx
}

/// Okumaları gösterime çevirir; başarılı okumalar cihaz anahtarıyla `last`'a kaydedilir ki
/// cihaz uyurken son değer gösterilebilsin.
fn snapshot(readings: &[Reading], last: &mut HashMap<String, u8>) -> Vec<Shown> {
    readings
        .iter()
        .map(|r| {
            if let Some(b) = r.battery {
                last.insert(r.key.clone(), b.percent);
            }
            present(r.kind, r.battery, last.get(&r.key).copied())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Battery, Kind, Tone};

    fn r(key: &str, percent: Option<u8>) -> Reading {
        let battery = percent.map(|percent| Battery { percent, charging: false });
        Reading { key: key.into(), kind: Kind::Mouse, battery }
    }

    #[test]
    fn keeps_last_value_while_asleep() {
        let mut last = HashMap::new();
        snapshot(&[r("usb:0", Some(70)), r("usb:1", Some(40))], &mut last);
        let s = snapshot(&[r("usb:0", Some(69)), r("usb:1", None)], &mut last);
        assert_eq!((s[0].text.as_str(), s[0].tone), ("69%", Tone::Normal));
        assert_eq!((s[1].text.as_str(), s[1].tone), ("40%", Tone::Inactive));
    }

    #[test]
    fn last_value_follows_the_device_not_the_position() {
        let mut last = HashMap::new();
        snapshot(&[r("usb:0", Some(70)), r("bt:a", Some(30))], &mut last);
        // usb:0 çıkarıldı; bt:a artık ilk sırada ama kendi değerini korur.
        let s = snapshot(&[r("bt:a", None)], &mut last);
        assert_eq!(s[0].text, "30%");
    }
}
