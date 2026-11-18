//! Arka plan sorgu döngüsü. HID okumaları (her biri en çok 1 sn) arayüzü bekletmesin diye
//! ayrı bir iş parçacığında, `INTERVAL` aralıkla ya da yenileme istenince hemen çalışır.

use crate::devices::{self, DRIVERS};
use crate::model::{present, Battery, Shown};
use hidapi::HidApi;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

pub const INTERVAL: Duration = Duration::from_secs(60);

/// Döngüyü başlatır; her turun sonucu `notify` ile iletilir (takılı alıcısı olan cihazlar,
/// `DRIVERS` sırasıyla). Dönen göndericiye `()` yollamak hemen yeniden sorgular; gönderici
/// düşürülürse döngü biter.
pub fn spawn(notify: impl Fn(Vec<Shown>) + Send + 'static) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("poller".into())
        .stack_size(256 * 1024) // varsayılan 1 MB gereksiz; HID tamponları küçük
        .spawn(move || {
            let mut api: Option<HidApi> = None;
            let mut last = vec![None; DRIVERS.len()];
            loop {
                if api.is_none() {
                    api = HidApi::new().ok();
                }
                let readings = api.as_mut().map(devices::poll).unwrap_or_default();
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

/// Okumaları gösterime çevirir; başarılı okumalar `last`'a (sürücü sırasıyla) kaydedilir ki
/// cihaz uyurken son değer gösterilebilsin.
fn snapshot(readings: &[(usize, Option<Battery>)], last: &mut [Option<u8>]) -> Vec<Shown> {
    readings
        .iter()
        .map(|&(i, battery)| {
            if let Some(b) = battery {
                last[i] = Some(b.percent);
            }
            present(DRIVERS[i].kind, battery, last[i])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tone;

    fn b(percent: u8) -> Option<Battery> {
        Some(Battery { percent, charging: false })
    }

    #[test]
    fn keeps_last_value_while_asleep() {
        let mut last = vec![None; DRIVERS.len()];
        snapshot(&[(0, b(70)), (1, b(40))], &mut last);
        let s = snapshot(&[(0, b(69)), (1, None)], &mut last);
        assert_eq!((s[0].text.as_str(), s[0].tone), ("69%", Tone::Normal));
        assert_eq!((s[1].text.as_str(), s[1].tone), ("40%", Tone::Inactive));
    }

    #[test]
    fn only_present_receivers_are_shown() {
        let mut last = vec![None; DRIVERS.len()];
        let s = snapshot(&[(1, b(50))], &mut last);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, DRIVERS[1].kind);
    }
}
