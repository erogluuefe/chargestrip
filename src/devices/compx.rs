//! Wireless gaming mice built on Compx receivers: ATK / VXE (also the NearLink ones), Kysona,
//! Pulsar, Redragon, Hitscan and G-Wolves models. One protocol across these brands, so every
//! product of their vendor IDs is tried; a receiver that never answers is not shown.
//!
//! Protocol: 17-byte output and input report `08`: `[08, command, 0 × 14, checksum]`, where
//! checksum = 0x55 − sum of the first 16 bytes. Both requests only read:
//! - `03` link state: reply `[6]` = 1 when the mouse is connected to the receiver.
//! - `04` battery: reply `[6]` = percent, `[7]` = charging, `[8..10]` = millivolts.
//!
//! The battery channel is the vendor interface `FF02:0002` where it exists; other vendor
//! interfaces are tried after it. Some products of these vendors are headsets or keyboards that
//! speak something else; they never get these frames.

use super::{exchange_within, Driver, Found, Interface};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;
use std::time::Duration;

const fn driver(name: &'static str, vid: u16) -> Driver {
    Driver { name, kind: Kind::Mouse, vid, pids: &[], interface: Interface::Vendor(0xFF02, 0x0002), read }
}

/// ATK / VXE receivers name themselves generically ("Nearlink Mouse Dongle"), hence the fixed name.
pub const ATK: Driver = driver("ATK wireless mouse", 0x373B);
/// Compx's own vendor ID: Kysona, VXE Dragonfly R1, Redragon and others; named by the receiver.
pub const COMPX: Driver = driver("", 0x3554);
pub const PULSAR: Driver = driver("", 0x3710);
pub const HITSCAN: Driver = driver("Hitscan Hyperlight", 0x3770);
pub const G_WOLVES: Driver = driver("", 0x33E4);

/// Products of these vendors that are not mice on this protocol: ATK headsets (`11CA`, `11CB`,
/// `1203`, `1204`), the ATK 8K Nano receiver (`1178`, another protocol) and keyboard receivers on
/// Compx IDs (`FA09`, `FB40`; frames would keep them awake).
const NOT_MICE: &[u16] = &[0x11CA, 0x11CB, 0x1203, 0x1204, 0x1178, 0xFA09, 0xFB40];

const REPORT: u8 = 0x08;
const LINK: u8 = 0x03;
const BATTERY: u8 = 0x04;
const TIMEOUT: Duration = Duration::from_millis(500);

fn read(dev: &HidDevice, pid: u16) -> Option<Found> {
    if NOT_MICE.contains(&pid) {
        return None;
    }
    let linked = query(dev, LINK).map(|r| r[6] != 0);
    if linked == Some(false) {
        // The receiver answered: one of ours, but the mouse is off or out of range.
        return Some(Found { battery: None, kind: None, name: None });
    }
    parse(&query(dev, BATTERY)?).map(Found::from)
}

fn checksum(frame: &[u8]) -> u8 {
    frame[..16].iter().fold(0x55u8, |a, &b| a.wrapping_sub(b))
}

fn request(cmd: u8) -> [u8; 17] {
    let mut r = [0u8; 17];
    r[0] = REPORT;
    r[1] = cmd;
    r[16] = checksum(&r);
    r
}

/// Sends `cmd`; its checksummed reply.
fn query(dev: &HidDevice, cmd: u8) -> Option<Vec<u8>> {
    exchange_within(dev, &request(cmd), TIMEOUT, |r| {
        (r.len() >= 17 && r[0] == REPORT && r[1] == cmd && r[16] == checksum(r)).then(|| r.to_vec())
    })
}

fn parse(r: &[u8]) -> Option<Battery> {
    (r[6] <= 100).then_some(Battery { percent: r[6], charging: r[7] != 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(cmd: u8, b6: u8, b7: u8) -> Vec<u8> {
        let mut r = request(cmd).to_vec();
        r[6] = b6;
        r[7] = b7;
        r[16] = checksum(&r);
        r
    }

    #[test]
    fn request_checksum() {
        // 0x55 − (0x08 + 0x04) = 0x49
        assert_eq!(request(BATTERY), [0x08, 0x04, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x49]);
        assert_eq!(request(LINK)[16], 0x55 - 0x08 - 0x03);
    }

    #[test]
    fn battery_reply() {
        let r = reply(BATTERY, 76, 0);
        assert_eq!(r[16], checksum(&r));
        assert_eq!(parse(&r), Some(Battery { percent: 76, charging: false }));
        assert_eq!(parse(&reply(BATTERY, 40, 1)), Some(Battery { percent: 40, charging: true }));
        assert_eq!(parse(&reply(BATTERY, 200, 0)), None);
    }
}
