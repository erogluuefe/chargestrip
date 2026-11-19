//! Razer wireless mice and keyboards (VID `1532`): receiver or charging cable.
//!
//! Protocol: 90-byte feature report (report ID 0):
//! `[0]` status, `[1]` transaction ID, `[2..4]` remaining packets, `[4]` protocol, `[5]` data size,
//! `[6]` command class, `[7]` command, `[8..88]` arguments, `[88]` checksum (XOR of 2..88).
//! - Battery: class `07` command `80` → `args[1]` = 0–255. Charging: class `07` command `84` →
//!   `args[1]` = 1. Both only read.
//! - Reply status: `01` busy, `02` OK, `03` failed, `04` device unreachable (off, or wrong
//!   transaction ID), `05` not supported.
//! - The transaction ID depends on the model: known for keyboards; for mice the known values are
//!   tried in turn. Razer Synapse shares the same feature "mailbox", so a reply that belongs to
//!   another request means sending ours again.
//!
//! The battery is read from the mouse/keyboard interface; which interface answers depends on the
//! model, so every matching interface is tried (`usage_page: ANY`).

use super::{Driver, Found, ANY};
use crate::model::{Battery, Kind};
use hidapi::HidDevice;
use std::time::Duration;

pub const MICE: Driver = Driver { kind: Kind::Mouse, vid: 0x1532, pids: MOUSE_PIDS, usage_page: ANY, usage: ANY, read };
pub const KEYBOARDS: Driver =
    Driver { kind: Kind::Keyboard, vid: 0x1532, pids: KEYBOARD_PIDS, usage_page: ANY, usage: ANY, read };

const CLASS_POWER: u8 = 0x07;
const GET_BATTERY: u8 = 0x80;
const GET_CHARGING: u8 = 0x84;
const ARGS_SIZE: u8 = 0x02;
/// Feature report length, report ID included.
const LEN: usize = 91;

fn read(dev: &HidDevice, pid: u16) -> Option<Found> {
    let fixed = keyboard_transaction_id(pid);
    let ids: &[u8] = match &fixed {
        Some(id) => std::slice::from_ref(id),
        None => &[0x1F, 0x3F, 0xFF, 0x08, 0x9F],
    };
    let keyboard = fixed.is_some();
    ids.iter().find_map(|&id| {
        let level = command(dev, id, GET_BATTERY, keyboard)?;
        let charging = command(dev, id, GET_CHARGING, keyboard).is_some_and(|a| a[1] == 1);
        Some(Battery { percent: ((level[1] as u32 * 100 + 127) / 255) as u8, charging }.into())
    })
}

/// Sends a power command; the arguments of the "OK" reply, otherwise `None`.
fn command(dev: &HidDevice, id: u8, cmd: u8, keyboard: bool) -> Option<[u8; 80]> {
    let request = request(id, cmd);
    let mut send = true;
    for attempt in 0..8 {
        if send {
            dev.send_feature_report(&request).ok()?;
        }
        // Wait while the reply is prepared (wireless keyboards ~5 ms); waiting longer lets
        // Synapse get in between.
        let wait = if keyboard { 5 } else if attempt == 0 { 15 } else { 40 };
        std::thread::sleep(Duration::from_millis(wait));
        send = true;
        let mut reply = [0u8; LEN];
        if dev.get_feature_report(&mut reply).ok()? < LEN - 1 {
            continue;
        }
        match parse(&reply[1..], id, cmd) {
            Reply::Ok(args) => return Some(args),
            Reply::Busy => send = false, // wait for the same request to finish
            Reply::Other | Reply::Unreachable => {} // another request's reply or a brief dropout: send again
            Reply::Failed => return None,
        }
    }
    None
}

fn request(id: u8, cmd: u8) -> [u8; LEN] {
    let mut req = [0u8; LEN]; // [0] report ID 0
    let r = &mut req[1..];
    r[1] = id;
    r[5] = ARGS_SIZE;
    r[6] = CLASS_POWER;
    r[7] = cmd;
    r[88] = crc(r);
    req
}

fn crc(report: &[u8]) -> u8 {
    report[2..88].iter().fold(0, |a, b| a ^ b)
}

#[derive(Debug, PartialEq)]
enum Reply {
    Ok([u8; 80]),
    Busy,
    Unreachable,
    Failed,
    /// Not a reply to this request (Synapse's, or corrupt).
    Other,
}

fn parse(r: &[u8], id: u8, cmd: u8) -> Reply {
    let ours = r[1] == id && r[2..5] == [0, 0, 0] && r[5] == ARGS_SIZE && r[6] == CLASS_POWER && r[7] == cmd;
    if !ours || r[88] != crc(r) {
        return Reply::Other;
    }
    match r[0] {
        0x01 => Reply::Busy,
        0x02 => Reply::Ok(r[8..88].try_into().unwrap_or([0; 80])),
        0x04 => Reply::Unreachable,
        0x03 | 0x05 => Reply::Failed,
        _ => Reply::Other,
    }
}

/// Transaction ID keyboards use for the battery query; `None` if unknown.
fn keyboard_transaction_id(pid: u16) -> Option<u8> {
    match pid {
        0x025C | 0x0271 | 0x0290 | 0x0296 | 0x02BA | 0x02C9 | 0x02D2 | 0x02D5 => Some(0x9F), // wireless
        0x0258 | 0x0292 | 0x0298 | 0x02B9 | 0x02CD | 0x02D4 | 0x02D7 => Some(0x1F),          // wired / Joro
        0x025A => Some(0x3F),                                                                // BlackWidow V3 Pro wired
        _ => None,
    }
}

/// Mice and their receivers (DeathAdder V3 Pro: `00B6` cable, `00B7` HyperSpeed receiver; and others).
const MOUSE_PIDS: &[u16] = &[
    0x0013, 0x0015, 0x0016, 0x001F, 0x0020, 0x0024, 0x0025, 0x0029, 0x002E, 0x002F, 0x0032, 0x0034, 0x0036,
    0x0037, 0x0038, 0x0039, 0x003E, 0x003F, 0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0048,
    0x004C, 0x004F, 0x0050, 0x0053, 0x0054, 0x0059, 0x005A, 0x005B, 0x005C, 0x005E, 0x005F, 0x0060, 0x0062,
    0x0064, 0x0065, 0x0067, 0x006A, 0x006B, 0x006C, 0x006E, 0x006F, 0x0070, 0x0071, 0x0072, 0x0073, 0x0077,
    0x0078, 0x007A, 0x007B, 0x007C, 0x007D, 0x0080, 0x0083, 0x0084, 0x0085, 0x0086, 0x0088, 0x008A, 0x008C,
    0x008D, 0x008F, 0x0090, 0x0091, 0x0094, 0x0095, 0x0096, 0x0098, 0x0099, 0x009A, 0x009C, 0x009E, 0x009F,
    0x00A1, 0x00A3, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00AA, 0x00AB, 0x00AF, 0x00B0, 0x00B2, 0x00B3, 0x00B4,
    0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BE, 0x00BF, 0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C7,
    0x00C8, 0x00CB, 0x00CC, 0x00CD, 0x00D0, 0x00D1, 0x00D3, 0x00D4, 0x00D6, 0x00D7, 0x00DA, 0x00DB, 0x00DE,
    0x00DF, 0x00E0, 0x00E5, 0x00E6,
];

/// Keyboards, keypads and their receivers.
const KEYBOARD_PIDS: &[u16] = &[
    0x010D, 0x010E, 0x010F, 0x0111, 0x0113, 0x0118, 0x011A, 0x011B, 0x011C, 0x0201, 0x0202, 0x0203, 0x0204,
    0x0205, 0x0207, 0x0208, 0x0209, 0x020F, 0x0210, 0x0211, 0x0214, 0x0216, 0x0217, 0x021A, 0x021E, 0x021F,
    0x0220, 0x0221, 0x0224, 0x0225, 0x0226, 0x0227, 0x0228, 0x022A, 0x022B, 0x022C, 0x022D, 0x022F, 0x0232,
    0x0233, 0x0234, 0x0235, 0x0237, 0x0239, 0x023A, 0x023B, 0x023F, 0x0240, 0x0241, 0x0243, 0x0244, 0x0245,
    0x0246, 0x024A, 0x024B, 0x024C, 0x024D, 0x024E, 0x0252, 0x0253, 0x0255, 0x0256, 0x0257, 0x0258, 0x0259,
    0x025A, 0x025C, 0x025D, 0x025E, 0x0266, 0x0268, 0x0269, 0x026A, 0x026B, 0x026C, 0x026D, 0x026E, 0x026F,
    0x0270, 0x0271, 0x0276, 0x0279, 0x027A, 0x0282, 0x0287, 0x028A, 0x028B, 0x028C, 0x028D, 0x028F, 0x0290,
    0x0292, 0x0293, 0x0294, 0x0295, 0x0296, 0x0298, 0x029D, 0x029E, 0x029F, 0x02A0, 0x02A1, 0x02A2, 0x02A3,
    0x02A5, 0x02A6, 0x02A7, 0x02B0, 0x02B6, 0x02B8, 0x02B9, 0x02BA, 0x02C5, 0x02C6, 0x02C7, 0x02C9, 0x02CD,
    0x02CF, 0x02D2, 0x02D4, 0x02D5, 0x02D7, 0x02E0, 0x02E1, 0x02EA, 0x0A24,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The device's reply: a copy of the request with status and arguments set, checksum recomputed.
    fn reply(status: u8, id: u8, cmd: u8, arg1: u8) -> [u8; 90] {
        let req = request(id, cmd);
        let mut r: [u8; 90] = req[1..].try_into().unwrap();
        r[0] = status;
        r[9] = arg1;
        r[88] = crc(&r);
        r
    }

    #[test]
    fn request_layout() {
        let r = request(0x1F, GET_BATTERY);
        assert_eq!(r[0], 0); // report ID
        assert_eq!(&r[1..9], &[0, 0x1F, 0, 0, 0, 0x02, 0x07, 0x80]);
        assert_eq!(r[89], 0x02 ^ 0x07 ^ 0x80);
    }

    #[test]
    fn battery_reply() {
        let Reply::Ok(args) = parse(&reply(0x02, 0x1F, GET_BATTERY, 255), 0x1F, GET_BATTERY) else { panic!() };
        assert_eq!(args[1], 255);
        assert_eq!(parse(&reply(0x01, 0x1F, GET_BATTERY, 0), 0x1F, GET_BATTERY), Reply::Busy);
        assert_eq!(parse(&reply(0x04, 0x1F, GET_BATTERY, 0), 0x1F, GET_BATTERY), Reply::Unreachable);
        assert_eq!(parse(&reply(0x05, 0x1F, GET_BATTERY, 0), 0x1F, GET_BATTERY), Reply::Failed);
    }

    #[test]
    fn foreign_or_corrupt_reply_is_ignored() {
        assert_eq!(parse(&reply(0x02, 0x3F, GET_BATTERY, 9), 0x1F, GET_BATTERY), Reply::Other); // another transaction ID
        assert_eq!(parse(&reply(0x02, 0x1F, GET_CHARGING, 1), 0x1F, GET_BATTERY), Reply::Other); // another command
        let mut r = reply(0x02, 0x1F, GET_BATTERY, 9);
        r[88] ^= 1;
        assert_eq!(parse(&r, 0x1F, GET_BATTERY), Reply::Other);
    }

    #[test]
    fn deathadder_v3_pro_is_listed() {
        assert!(MOUSE_PIDS.contains(&0x00B6) && MOUSE_PIDS.contains(&0x00B7));
    }
}
