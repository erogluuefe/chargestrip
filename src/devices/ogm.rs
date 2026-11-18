//! Waizowl OGM Pro V2 ("OGM 8K" alıcısı, Nordic 1915:0728) pil sorgusu.
//! Protokol: `docs/fare-protokolu.md` (Waizowl Driver Software 1.0.5.1 trafiğinden, 2026-11-18).
//!
//! Sadece iki salt-okuma isteği gönderilir: 0x01 (bağlantı durumu) ve 0x41 (pil).
//! Bunlar Waizowl'un her bağlanmada ayar değiştirmeden gönderdiği isteklerin birebir aynısıdır.

use crate::model::Reading;
use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

pub const VID: u16 = 0x1915;
pub const PID: u16 = 0x0728;
pub const USAGE_PAGE: u16 = 0xFF0A;

pub const CMD_STATE: u8 = 0x01;
pub const CMD_BATTERY: u8 = 0x41;

const FRAME: usize = 64;
const CHECKSUM_TARGET: u8 = 0xA1;
const TIMEOUT: Duration = Duration::from_millis(1000);

const STATE_CONNECTED: u8 = 0x0A;
const STATE_DISCONNECTED: u8 = 0x02;

/// Çerçevedeki 64 baytın toplamı (mod 256) her zaman 0xA1 olur; son bayt bunu tamamlar.
pub fn checksum_ok(frame: &[u8]) -> bool {
    frame.len() == FRAME && frame.iter().fold(0u8, |a, &b| a.wrapping_add(b)) == CHECKSUM_TARGET
}

/// Rapor no (0) + 64 baytlık istek çerçevesi: `[cmd, 00, 81, 01, 0…, sağlama]`.
pub fn request(cmd: u8) -> [u8; FRAME + 1] {
    let mut out = [0u8; FRAME + 1];
    let f = &mut out[1..];
    f[0] = cmd;
    f[2] = 0x81;
    f[3] = 0x01;
    let sum = f[..FRAME - 1].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    f[FRAME - 1] = CHECKSUM_TARGET.wrapping_sub(sum);
    out
}

/// hidapi rapor no 0'ı genelde atar; atmadıysa (65 bayt) biz atarız.
fn strip(buf: &[u8]) -> &[u8] {
    if buf.len() == FRAME + 1 { &buf[1..] } else { buf }
}

/// 0x01 cevabı → fare bağlı mı? Tanınmayan değer → `None`.
pub fn parse_state(frame: &[u8]) -> Option<bool> {
    if !checksum_ok(frame) || frame[0] != CMD_STATE {
        return None;
    }
    match frame[4] {
        STATE_CONNECTED => Some(true),
        STATE_DISCONNECTED => Some(false),
        _ => None,
    }
}

/// 0x41 cevabı → `[5]` şarj (1 = kablo takılı), `[6]` yüzde.
pub fn parse_battery(frame: &[u8]) -> Reading {
    if !checksum_ok(frame) || frame[0] != CMD_BATTERY || frame[6] > 100 {
        return Reading::Unavailable;
    }
    Reading::Ok { percent: frame[6], charging: frame[5] == 0x01 }
}

/// İsteği yollar, aynı komutun geçerli cevabını bekler. Arada kendiliğinden gelen
/// bildirimler (fare pil/durum değişince yollar) atlanır.
fn transact(dev: &HidDevice, cmd: u8) -> Option<[u8; FRAME]> {
    dev.write(&request(cmd)).ok()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let mut buf = [0u8; FRAME + 1];
        let n = dev.read_timeout(&mut buf, left.as_millis().max(1) as i32).ok()?;
        if n == 0 {
            return None;
        }
        let f = strip(&buf[..n]);
        if f.len() == FRAME && f[0] == cmd && checksum_ok(f) {
            return f.try_into().ok();
        }
    }
}

pub fn query(api: &HidApi) -> Reading {
    let Some(info) = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == USAGE_PAGE)
    else {
        return Reading::Unavailable;
    };
    let Ok(dev) = info.open_device(api) else {
        return Reading::Unavailable;
    };
    match transact(&dev, CMD_STATE).and_then(|f| parse_state(&f)) {
        Some(false) => return Reading::Disconnected,
        Some(true) => {}
        None => return Reading::Unavailable,
    }
    transact(&dev, CMD_BATTERY).map_or(Reading::Unavailable, |f| parse_battery(&f))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kayıttaki çerçeveler: baştaki anlamlı baytlar + sıfırlar + sağlama.
    fn frame(head: &[u8], checksum: u8) -> Vec<u8> {
        let mut f = vec![0u8; FRAME];
        f[..head.len()].copy_from_slice(head);
        f[FRAME - 1] = checksum;
        f
    }

    // docs/fare-protokolu.md'deki kayıtlar
    fn state_on() -> Vec<u8> { frame(&[0x01, 0, 0x8c, 0x01, 0x0a, 0x15, 0x19, 0x28, 0x07, 0x06, 0x01, 0xf5, 0x07], 0xa9) }
    fn state_off() -> Vec<u8> { frame(&[0x01, 0, 0x8c, 0x01, 0x02, 0x15, 0x19, 0x28, 0x07, 0x06, 0x01, 0xf5, 0x07], 0xb1) }
    fn bat_63() -> Vec<u8> { frame(&[0x41, 0, 0x8c, 0x01, 0, 0, 0x3f, 0, 0x01, 0x05, 0x06], 0x88) }
    fn bat_charging_68() -> Vec<u8> { frame(&[0x41, 0, 0x8c, 0x01, 0, 0x01, 0x44, 0, 0x01, 0x05, 0x06], 0x82) }
    fn bat_unplugged_68() -> Vec<u8> { frame(&[0x41, 0, 0x8c, 0x01, 0, 0, 0x44, 0, 0x01, 0x05, 0x06], 0x83) }

    #[test]
    fn requests_match_capture() {
        let r = request(CMD_STATE);
        assert_eq!((r[0], r[1], r[2], r[3], r[4], r[64]), (0x00, 0x01, 0x00, 0x81, 0x01, 0x1e));
        let r = request(CMD_BATTERY);
        assert_eq!((r[1], r[3], r[4], r[64]), (0x41, 0x81, 0x01, 0xde));
        assert!(r[5..64].iter().all(|&b| b == 0));
    }

    #[test]
    fn captured_frames_pass_checksum() {
        for f in [state_on(), state_off(), bat_63(), bat_charging_68(), bat_unplugged_68()] {
            assert!(checksum_ok(&f));
        }
    }

    #[test]
    fn battery_63_not_charging() {
        assert_eq!(parse_battery(&bat_63()), Reading::Ok { percent: 63, charging: false });
    }

    #[test]
    fn battery_charging() {
        assert_eq!(parse_battery(&bat_charging_68()), Reading::Ok { percent: 68, charging: true });
        assert_eq!(parse_battery(&bat_unplugged_68()), Reading::Ok { percent: 68, charging: false });
    }

    #[test]
    fn state_on_off() {
        assert_eq!(parse_state(&state_on()), Some(true));
        assert_eq!(parse_state(&state_off()), Some(false));
    }

    #[test]
    fn corrupted_frame_rejected() {
        let mut f = bat_63();
        f[6] = 0x40; // sağlama artık tutmaz
        assert_eq!(parse_battery(&f), Reading::Unavailable);
        assert_eq!(parse_state(&[]), None);
    }

    #[test]
    fn wrong_command_rejected() {
        assert_eq!(parse_battery(&state_on()), Reading::Unavailable);
        assert_eq!(parse_state(&bat_63()), None);
    }

    #[test]
    fn strip_report_id() {
        let mut with_id = vec![0u8];
        with_id.extend(bat_63());
        assert_eq!(strip(&with_id), &bat_63()[..]);
        assert_eq!(strip(&bat_63()), &bat_63()[..]);
    }
}
