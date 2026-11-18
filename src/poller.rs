//! Arka plan iş parçacığı: 60 sn'de bir (ya da "Yenile" gelince hemen) cihazları sorgular.
//! UI iş parçacığı hiçbir zaman HID beklemez.

use crate::devices;
use crate::model::{present, Reading, Shown};
use hidapi::HidApi;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

pub const INTERVAL: Duration = Duration::from_secs(60);
pub const HEADSET_NAME: &str = "Kulaklık";
pub const MOUSE_NAME: &str = "Fare";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub headset: Shown,
    pub mouse: Shown,
}

/// Son başarılı okumalar; cihaz uyurken gri olarak gösterilir.
#[derive(Default, Debug)]
pub struct Memory {
    pub headset: Option<u8>,
    pub mouse: Option<u8>,
}

pub fn remember(mem: &mut Memory, headset: &Reading, mouse: &Reading) {
    if let Reading::Ok { percent, .. } = *headset {
        mem.headset = Some(percent);
    }
    if let Reading::Ok { percent, .. } = *mouse {
        mem.mouse = Some(percent);
    }
}

pub fn make_update(mem: &Memory, headset: &Reading, mouse: &Reading) -> Update {
    Update {
        headset: present(HEADSET_NAME, headset, mem.headset),
        mouse: present(MOUSE_NAME, mouse, mem.mouse),
    }
}

/// Döngüyü başlatır. Dönen göndericiye `()` yollamak hemen yeniden sorgular;
/// gönderici düşürülürse döngü biter.
pub fn spawn(notify: impl Fn(Update) + Send + 'static) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("poller".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut api: Option<HidApi> = None;
            let mut mem = Memory::default();
            loop {
                if api.is_none() {
                    api = HidApi::new().ok();
                }
                let (h, m) = match api.as_mut() {
                    Some(api) => devices::poll_all(api),
                    None => (Reading::Unavailable, Reading::Unavailable),
                };
                remember(&mut mem, &h, &m);
                notify(make_update(&mem, &h, &m));
                match rx.recv_timeout(INTERVAL) {
                    Ok(()) => while rx.try_recv().is_ok() {}, // art arda tıklamaları birleştir
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("poller iş parçacığı başlatılamadı");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GRAY;

    fn ok(p: u8) -> Reading {
        Reading::Ok { percent: p, charging: false }
    }

    #[test]
    fn remember_keeps_last_on_disconnect() {
        let mut m = Memory::default();
        remember(&mut m, &ok(70), &ok(40));
        remember(&mut m, &Reading::Disconnected, &Reading::Unavailable);
        assert_eq!((m.headset, m.mouse), (Some(70), Some(40)));
    }

    #[test]
    fn remember_updates_on_ok() {
        let mut m = Memory::default();
        remember(&mut m, &ok(70), &ok(40));
        remember(&mut m, &ok(69), &ok(39));
        assert_eq!((m.headset, m.mouse), (Some(69), Some(39)));
    }

    #[test]
    fn sleeping_mouse_shows_last_gray() {
        let mut m = Memory::default();
        remember(&mut m, &ok(70), &ok(40));
        let u = make_update(&m, &ok(70), &Reading::Disconnected);
        assert_eq!((u.mouse.text.as_str(), u.mouse.color), ("40", GRAY));
        assert_eq!(u.headset.text, "70");
    }
}
