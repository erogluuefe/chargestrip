//! Keyboards with a SONiX controller on their 2.4 GHz receiver: AJAZZ AK820 Pro / AK980 V2
//! (`0C45:FDFD`, `0C45:FEFE`) and AJAZZ AK820 MAX / AULA F108 Pro (`05AC:024F`). The receiver's
//! vendor interface is `FF60:0061` with 32-byte reports (report ID 0).
//!
//! Protocol (read-only): request `[20 01 00 …, sum]`, where the last byte is the sum of the 31
//! before it; reply `[20 01 00, level …]` with the level in percent, 0 when the keyboard is off,
//! out of range or in another mode. The keyboard only reports its level while it is in 2.4 GHz
//! mode: on its USB cable or over Bluetooth this receiver is not used.

use super::{exchange, Driver, Found, Interface};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;

const fn driver(vid: u16, pids: &'static [u16]) -> Driver {
    Driver { name: "", kind: Kind::Keyboard, vid, pids, interface: Interface::Usage(0xFF60, 0x0061), read }
}

pub const AJAZZ_PRO: Driver = driver(0x0C45, &[0xFDFD, 0xFEFE]);
/// The AK820 MAX receiver reuses Apple's vendor ID; the `FF60:0061` interface tells it apart.
pub const AJAZZ_MAX: Driver = driver(0x05AC, &[0x024F]);

const LEN: usize = 32;

fn read(dev: &HidDevice, _pid: u16) -> Option<Found> {
    // The first request can go unanswered while the 2.4 GHz link wakes up; try twice.
    let level = query(dev).or_else(|| query(dev))?;
    let name = dev.get_product_string().ok().flatten().map(|n| keyboard_name(&n));
    let battery = (1..=100).contains(&level).then_some(Battery { percent: level, charging: false });
    Some(Found { battery, kind: None, name })
}

fn request() -> [u8; LEN + 1] {
    let mut r = [0u8; LEN + 1]; // [0] report ID 0
    r[1..4].copy_from_slice(&[0x20, 0x01, 0x00]);
    r[LEN] = r[1..LEN].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    r
}

/// The level byte of the receiver's reply.
fn query(dev: &HidDevice) -> Option<u8> {
    exchange(dev, &request(), |r| (r.len() >= 4 && r[..3] == [0x20, 0x01, 0x00]).then_some(r[3]))
}

/// "AJAZZ AK820 MAX 2.4G" → "AJAZZ AK820 MAX"; generic receiver names → "AJAZZ keyboard".
fn keyboard_name(product: &str) -> String {
    let name = product.trim().trim_end_matches(" 2.4G").trim();
    if name.is_empty() || name.starts_with("2.4G") { "AJAZZ keyboard".into() } else { name.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout() {
        let r = request();
        assert_eq!(&r[..4], &[0, 0x20, 0x01, 0x00]);
        assert_eq!(r[LEN], 0x21); // 0x20 + 0x01
    }

    #[test]
    fn names() {
        assert_eq!(keyboard_name("AJAZZ AK820 MAX 2.4G"), "AJAZZ AK820 MAX");
        assert_eq!(keyboard_name("2.4G Dongle"), "AJAZZ keyboard");
        assert_eq!(keyboard_name(""), "AJAZZ keyboard");
    }
}
