//! Fractal Design Scape headset: its 2.4 GHz receiver (`36BC:0001`) or the headset itself on its
//! USB-C cable (`36BC:2001`). Both expose a 64-byte interface on usage page `FF00`.
//!
//! Protocol (the request Fractal's own web configurator uses; tested with hardware):
//! - Request: report `0x02` = `[0x02, device type, 0x21]` ("get device state"). Changes nothing.
//!   Device type `0x11` receiver, `0xF1` headset; a headset request sent to the receiver is
//!   relayed over the air.
//! - Reply: `[0x02, device type, 0x21, …]`.
//!   Receiver: `[4]` = 1 when the headset is linked. Headset: `[15]` = battery percent,
//!   `[23]` = 1 on the cable (charging).
//! - On its charging stand the headset unlinks from the receiver and shows as off.

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
        return None; // the receiver answered, but the headset is off, out of range or on its stand
    }
    parse(&state(dev, HEADSET)?).map(Found::from)
}

/// "Get device state" request; the whole reply.
fn state(dev: &HidDevice, target: u8) -> Option<Vec<u8>> {
    exchange(dev, &[REPORT, target, GET_STATE], |r| {
        (r.len() >= 24 && r[..3] == [REPORT, target, GET_STATE]).then(|| r.to_vec())
    })
}

/// Battery from the headset's state reply.
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
