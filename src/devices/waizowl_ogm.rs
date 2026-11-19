//! Waizowl OGM Pro V2 fare ("OGM 8K" Nordic alıcısı).
//!
//! Protokol Waizowl Driver Software trafiğinden çıkarıldı: docs/waizowl-ogm-protocol.md.
//! - Çerçeve 64 bayt: `[komut, 00, 81, 01, veri…, sağlama]`; 64 baytın toplamı mod 256 = 0xA1.
//! - `0x01` bağlantı durumu: cevapta `[4]` = 0x0A bağlı, 0x02 fare kapalı/uykuda.
//! - `0x41` pil: cevapta `[5]` = 1 şarjda, `[6]` = yüzde.
//!
//! İkisi de Waizowl'un her bağlanmada ayar değiştirmeden gönderdiği salt-okuma istekleridir.

use super::{exchange, Driver, Found, ANY};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;

pub const DRIVER: Driver = Driver {
    kind: Kind::Mouse,
    vid: 0x1915,
    pids: &[0x0728],
    usage_page: 0xFF0A,
    usage: ANY,
    read,
};

const FRAME: usize = 64;
const CHECKSUM: u8 = 0xA1;
const CMD_STATE: u8 = 0x01;
const CMD_BATTERY: u8 = 0x41;
const STATE_CONNECTED: u8 = 0x0A;

fn read(dev: &HidDevice, _pid: u16) -> Option<Found> {
    // Fare uykudayken alıcı son pil değerini yine döndürebilir; önce bağlantıya bakılır.
    let state = exchange(dev, &request(CMD_STATE), |r| frame(r, CMD_STATE).map(|f| f[4]))?;
    if state != STATE_CONNECTED {
        return None;
    }
    exchange(dev, &request(CMD_BATTERY), |r| frame(r, CMD_BATTERY).map(battery))?.map(Found::from)
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, &b| a.wrapping_add(b))
}

/// Rapor no (0) + 64 baytlık istek çerçevesi.
fn request(cmd: u8) -> [u8; FRAME + 1] {
    let mut out = [0u8; FRAME + 1];
    let f = &mut out[1..];
    f[..4].copy_from_slice(&[cmd, 0x00, 0x81, 0x01]);
    f[FRAME - 1] = CHECKSUM.wrapping_sub(sum(&f[..FRAME - 1]));
    out
}

/// Rapor, `cmd` komutunun sağlaması tutan bir cevabıysa çerçeveyi döndürür.
/// Rapor numarası hidapi tarafından atılmamışsa (65 bayt) burada atılır.
fn frame(r: &[u8], cmd: u8) -> Option<&[u8]> {
    let f = if r.len() == FRAME + 1 { &r[1..] } else { r };
    (f.len() == FRAME && f[0] == cmd && sum(f) == CHECKSUM).then_some(f)
}

fn battery(f: &[u8]) -> Option<Battery> {
    (f[6] <= 100).then(|| Battery { percent: f[6], charging: f[5] == 0x01 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kayıttaki bir çerçeve: anlamlı baş kısım + sıfırlar + sağlama (docs/waizowl-ogm-protocol.md).
    fn captured(head: &[u8], checksum: u8) -> Vec<u8> {
        let mut f = vec![0u8; FRAME];
        f[..head.len()].copy_from_slice(head);
        f[FRAME - 1] = checksum;
        f
    }

    fn state_on() -> Vec<u8> {
        captured(&[0x01, 0, 0x8c, 0x01, 0x0a, 0x15, 0x19, 0x28, 0x07, 0x06, 0x01, 0xf5, 0x07], 0xa9)
    }
    fn state_off() -> Vec<u8> {
        captured(&[0x01, 0, 0x8c, 0x01, 0x02, 0x15, 0x19, 0x28, 0x07, 0x06, 0x01, 0xf5, 0x07], 0xb1)
    }
    fn battery_63() -> Vec<u8> {
        captured(&[0x41, 0, 0x8c, 0x01, 0, 0, 0x3f, 0, 0x01, 0x05, 0x06], 0x88)
    }
    fn battery_68_charging() -> Vec<u8> {
        captured(&[0x41, 0, 0x8c, 0x01, 0, 0x01, 0x44, 0, 0x01, 0x05, 0x06], 0x82)
    }

    #[test]
    fn requests_match_capture() {
        let r = request(CMD_STATE);
        assert_eq!(r[..5], [0x00, 0x01, 0x00, 0x81, 0x01]);
        assert_eq!(r[64], 0x1e);
        let r = request(CMD_BATTERY);
        assert_eq!(r[..5], [0x00, 0x41, 0x00, 0x81, 0x01]);
        assert_eq!(r[64], 0xde);
        assert!(r[5..64].iter().all(|&b| b == 0));
    }

    #[test]
    fn connection_state() {
        assert_eq!(frame(&state_on(), CMD_STATE).map(|f| f[4]), Some(STATE_CONNECTED));
        assert_eq!(frame(&state_off(), CMD_STATE).map(|f| f[4]), Some(0x02));
    }

    #[test]
    fn battery_levels() {
        let f = battery_63();
        assert_eq!(frame(&f, CMD_BATTERY).and_then(battery), Some(Battery { percent: 63, charging: false }));
        let f = battery_68_charging();
        assert_eq!(frame(&f, CMD_BATTERY).and_then(battery), Some(Battery { percent: 68, charging: true }));
    }

    #[test]
    fn report_id_is_stripped() {
        let mut with_id = vec![0u8];
        with_id.extend(battery_63());
        assert!(frame(&with_id, CMD_BATTERY).is_some());
    }

    #[test]
    fn rejects_corrupt_or_foreign_frames() {
        let mut f = battery_63();
        f[6] = 0x40; // sağlama artık tutmaz
        assert!(frame(&f, CMD_BATTERY).is_none());
        assert!(frame(&state_on(), CMD_BATTERY).is_none(), "başka komutun cevabı");
        assert!(frame(&[], CMD_STATE).is_none());
    }
}
