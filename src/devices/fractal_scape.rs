//! Fractal Design Scape kulaklık: 2.4 GHz alıcısı (`36BC:0001`) ya da USB-C kablosuyla kulaklığın
//! kendisi (`36BC:2001`). İkisi de usage page `FF00` üzerinde 64 baytlık bir arayüz sunar.
//!
//! Protokol (Fractal'ın kendi web yapılandırıcısının kullandığı istek; donanımla denenmiş):
//! - İstek: rapor `0x02` = `[0x02, cihaz türü, 0x21]` ("cihaz durumunu ver"). Ayar değiştirmez.
//!   Cihaz türü `0x11` alıcı, `0xF1` kulaklık; alıcıya giden kulaklık isteği havadan iletilir.
//! - Cevap: `[0x02, cihaz türü, 0x21, …]`.
//!   Alıcı: `[4]` = 1 ise kulaklık bağlı. Kulaklık: `[15]` = pil yüzdesi, `[23]` = 1 kabloda (şarjda).
//! - Kulaklık şarj standındayken alıcıdan kopar; o zaman kapalı görünür.

use super::{exchange, Driver, Found};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;

const PID_DONGLE: u16 = 0x0001;

pub const DRIVER: Driver = Driver {
    kind: Kind::Headset,
    vid: 0x36BC,
    pids: &[PID_DONGLE, 0x2001],
    usage_page: 0xFF00,
    usage: 0x0001,
    read,
};

const REPORT: u8 = 0x02;
const DONGLE: u8 = 0x11;
const HEADSET: u8 = 0xF1;
const GET_STATE: u8 = 0x21;

fn read(dev: &HidDevice, pid: u16) -> Option<Found> {
    if pid == PID_DONGLE && state(dev, DONGLE)?[4] != 1 {
        return None; // alıcı cevap verdi ama kulaklık kapalı, menzil dışında ya da standında
    }
    parse(&state(dev, HEADSET)?).map(Found::from)
}

/// "Cihaz durumunu ver" isteği; cevabın tamamı.
fn state(dev: &HidDevice, target: u8) -> Option<Vec<u8>> {
    exchange(dev, &[REPORT, target, GET_STATE], |r| {
        (r.len() >= 24 && r[..3] == [REPORT, target, GET_STATE]).then(|| r.to_vec())
    })
}

/// Kulaklığın durum cevabından pil.
fn parse(r: &[u8]) -> Option<Battery> {
    (r[15] <= 100).then_some(Battery { percent: r[15], charging: r[23] == 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(percent: u8, cable: u8) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[..3].copy_from_slice(&[REPORT, HEADSET, GET_STATE]);
        r[15] = percent;
        r[23] = cable;
        r
    }

    #[test]
    fn headset_state() {
        assert_eq!(parse(&reply(64, 0)), Some(Battery { percent: 64, charging: false }));
        assert_eq!(parse(&reply(90, 1)), Some(Battery { percent: 90, charging: true }));
        assert_eq!(parse(&reply(0xFF, 0)), None);
    }
}
