//! Logitech wireless receivers (LIGHTSPEED, Unifying, Bolt) and the mice/keyboards paired with
//! them, over HID++ 2.0. Features are discovered by asking the device, so no model list is needed.
//!
//! Message (long report, 20 bytes): `[0x11, device index, feature index, function << 4 | software ID, …]`.
//! The reply starts with the same four bytes; an error is `[0x11, device index, 0xFF, feature index, function…]`.
//! All requests used here only read:
//! - Root feature (`0x0000`): fn 1 = protocol version (is the device connected?), fn 0 = index of a feature.
//! - `0x1004` unified battery: fn 1 → `[percent, level bits, charging state]`.
//! - `0x1000` battery status (older devices): fn 0 → `[percent, next level, charging state]`.
//! - `0x0005` device name/type: fn 2 → type (keyboard, mouse, headset…).
//!
//! Up to six devices can pair with a receiver (index 1–6); the first one with a readable battery
//! is shown. The software ID tells Logitech G HUB's replies apart from ours.

use super::{exchange_within, Driver, Found};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;
use std::time::Duration;

pub const RECEIVERS: Driver = Driver {
    kind: Kind::Keyboard, // if the kind can't be learned from the device
    vid: 0x046D,
    pids: &[
        0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541, 0xC545, 0xC547, 0xC54D, // LIGHTSPEED
        0xC52B, 0xC532, // Unifying
        0xC548, // Bolt
    ],
    usage_page: 0xFF00,
    usage: 0x0002, // HID++ long report interface
    read,
};

const SOFTWARE_ID: u8 = 0x0D;
const ROOT: u8 = 0x00;
const UNIFIED_BATTERY: u16 = 0x1004;
const BATTERY_STATUS: u16 = 0x1000;
const DEVICE_NAME: u16 = 0x0005;
/// Unpaired or sleeping devices don't answer; keep trying every index short.
const TIMEOUT: Duration = Duration::from_millis(300);

fn read(dev: &HidDevice, _pid: u16) -> Option<Found> {
    (1..=6).find_map(|index| {
        // Protocol version 2.0 or later (device connected and speaking HID++ 2.0)?
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

/// HID++ 2.0 call; the reply's parameters, `None` on error or timeout.
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

/// A feature's index on the device; `None` if the device lacks it.
fn feature(dev: &HidDevice, index: u8, id: u16) -> Option<u8> {
    call(dev, index, ROOT, 0, &id.to_be_bytes())?.first().copied().filter(|&f| f != 0)
}

/// `0x1004`: the percentage if known, otherwise an estimate from the level bits (full/good/low/critical).
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

/// `0x1000`: states 1–4 relate to charging; 3 = full (even if 0 % is reported).
fn battery_status(p: &[u8]) -> Option<Battery> {
    let (percent, status) = (p[0], p[2]);
    let percent = if percent == 0 && status == 3 { 100 } else { percent };
    (percent <= 100).then_some(Battery { percent, charging: matches!(status, 1..=4) })
}

/// Device type returned by `0x0005` fn 2.
fn kind(device_type: u8) -> Option<Kind> {
    match device_type {
        0x00 | 0x02 => Some(Kind::Keyboard),
        0x03..=0x05 => Some(Kind::Mouse), // mouse, trackpad, trackball
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
        assert_eq!(unified_battery(&[0, 2, 0]), b(20, false)); // level only: low
        assert_eq!(unified_battery(&[0, 0, 0]), None);
    }

    #[test]
    fn status() {
        assert_eq!(battery_status(&[55, 50, 0]), b(55, false));
        assert_eq!(battery_status(&[0, 0, 3]), b(100, true)); // full
        assert_eq!(battery_status(&[200, 0, 0]), None);
    }

    #[test]
    fn device_type() {
        assert_eq!(kind(0x00), Some(Kind::Keyboard));
        assert_eq!(kind(0x03), Some(Kind::Mouse));
        assert_eq!(kind(0x07), None); // remote control etc.
    }
}
