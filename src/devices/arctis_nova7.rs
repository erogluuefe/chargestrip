//! SteelSeries Arctis Nova 7P headset.
//!
//! Protocol:
//! - Request `[0x00, 0xB0]`: report ID 0 + "read status". Changes no settings.
//! - Reply: `[0]` = 0xB0, `[2]` = battery, `[3]` = status (0x00 headset off, 0x01/0x02 charging).
//! - `0x220A` (older firmware) reports the battery in steps 0–4 (0/25/50/75/100 %);
//!   `0x22A7` / `0x2298`, from the January 2026 firmware on, report a percentage.

use super::{exchange, Driver, Found, Interface};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;

/// Product ID that reports the battery in steps 0–4; the others report a percentage.
const PID_STEPPED: u16 = 0x220A;

pub const DRIVER: Driver = Driver {
    name: "SteelSeries Arctis Nova 7P",
    kind: Kind::Headset,
    vid: 0x1038,
    pids: &[PID_STEPPED, 0x22A7, 0x2298],
    interface: Interface::Page(0xFFC0),
    read,
};

const REQUEST: [u8; 2] = [0x00, 0xB0];
const STATUS_OFF: u8 = 0x00;

fn read(dev: &HidDevice, pid: u16) -> Option<Found> {
    exchange(dev, &REQUEST, |r| (r.len() >= 4 && r[0] == REQUEST[1]).then(|| parse(r, pid == PID_STEPPED)))?
        .map(Found::from)
}

/// Decodes the status reply; `None` when the headset is off.
fn parse(r: &[u8], stepped: bool) -> Option<Battery> {
    let (level, status) = (r[2], r[3]);
    if status == STATUS_OFF {
        return None;
    }
    let percent = if stepped { level.min(4) * 25 } else { level.min(100) };
    Some(Battery { percent, charging: matches!(status, 0x01 | 0x02) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(percent: u8, charging: bool) -> Option<Battery> {
        Some(Battery { percent, charging })
    }

    #[test]
    fn stepped_levels() {
        assert_eq!(parse(&[0xB0, 0, 4, 3], true), ok(100, false));
        assert_eq!(parse(&[0xB0, 0, 3, 3], true), ok(75, false));
        assert_eq!(parse(&[0xB0, 0, 0, 3], true), ok(0, false));
    }

    #[test]
    fn percent_firmware() {
        assert_eq!(parse(&[0xB0, 0, 67, 3], false), ok(67, false));
    }

    #[test]
    fn charging_states() {
        assert_eq!(parse(&[0xB0, 0, 1, 1], true), ok(25, true));
        assert_eq!(parse(&[0xB0, 0, 1, 2], true), ok(25, true));
    }

    #[test]
    fn headset_off() {
        assert_eq!(parse(&[0xB0, 0, 0, 0], true), None);
    }

    #[test]
    fn out_of_range_is_clamped() {
        assert_eq!(parse(&[0xB0, 0, 9, 3], true), ok(100, false));
        assert_eq!(parse(&[0xB0, 0, 140, 3], false), ok(100, false));
    }
}
