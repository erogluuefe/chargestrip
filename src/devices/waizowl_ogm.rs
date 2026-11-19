//! Waizowl OGM Pro V2 mouse ("OGM 8K" Nordic receiver).
//!
//! Protocol worked out from the Waizowl Driver Software traffic: docs/waizowl-ogm-protocol.md.
//! - 64-byte frame: `[command, 00, 81, 01, data…, checksum]`; the 64 bytes sum to 0xA1 (mod 256).
//! - `0x01` link state: reply `[4]` = 0x0A connected, 0x02 mouse off / asleep.
//! - `0x41` battery: reply `[5]` = 1 charging, `[6]` = percent.
//!
//! Both are read-only requests the Waizowl software sends on every connection.

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
    // The receiver may still return the last level while the mouse sleeps; check the link first.
    let state = exchange(dev, &request(CMD_STATE), |r| frame(r, CMD_STATE).map(|f| f[4]))?;
    if state != STATE_CONNECTED {
        return None;
    }
    exchange(dev, &request(CMD_BATTERY), |r| frame(r, CMD_BATTERY).map(battery))?.map(Found::from)
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, &b| a.wrapping_add(b))
}

/// Report ID (0) + 64-byte request frame.
fn request(cmd: u8) -> [u8; FRAME + 1] {
    let mut out = [0u8; FRAME + 1];
    let f = &mut out[1..];
    f[..4].copy_from_slice(&[cmd, 0x00, 0x81, 0x01]);
    f[FRAME - 1] = CHECKSUM.wrapping_sub(sum(&f[..FRAME - 1]));
    out
}

/// Returns the frame if the report is a reply to `cmd` with a valid checksum.
/// Strips the report ID if hidapi left it in (65 bytes).
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

    /// A captured frame: meaningful head + zeros + checksum (docs/waizowl-ogm-protocol.md).
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
        f[6] = 0x40; // checksum no longer matches
        assert!(frame(&f, CMD_BATTERY).is_none());
        assert!(frame(&state_on(), CMD_BATTERY).is_none(), "reply to another command");
        assert!(frame(&[], CMD_STATE).is_none());
    }
}
