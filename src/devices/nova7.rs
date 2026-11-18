//! SteelSeries Arctis Nova 7P pil sorgusu.
//!
//! Protokol:
//! - arayüz 3, usage page 0xFFC0, usage 0x01
//! - istek `[0x00, 0xB0]` (rapor no 0 + 0xB0 "durum oku"); ayar değiştirmez
//! - cevap: `[2]` pil, `[3]` durum (0x00 kulaklık kapalı, 0x01/0x02 şarjda, diğer açık)
//! - 0x220A (eski yazılım): pil 0..=4 kademe → %0/25/50/75/100
//! - 0x22A7, 0x2298 (Ocak 2026 sonrası "V2"): pil doğrudan 0..=100

use crate::model::Reading;
use hidapi::HidApi;

pub const VID: u16 = 0x1038;
pub const PID_DISCRETE: u16 = 0x220A;
pub const PIDS_PERCENT: [u16; 2] = [0x22A7, 0x2298];
pub const USAGE_PAGE: u16 = 0xFFC0;

const REQUEST: [u8; 2] = [0x00, 0xB0];
const TIMEOUT_MS: i32 = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    /// 0..=4 kademe
    Discrete,
    /// 0..=100
    Percent,
}

pub fn scale_for(pid: u16) -> Option<Scale> {
    if pid == PID_DISCRETE {
        Some(Scale::Discrete)
    } else if PIDS_PERCENT.contains(&pid) {
        Some(Scale::Percent)
    } else {
        None
    }
}

pub fn parse(buf: &[u8], scale: Scale) -> Reading {
    if buf.len() < 4 {
        return Reading::Unavailable;
    }
    let status = buf[3];
    if status == 0x00 {
        return Reading::Disconnected;
    }
    let percent = match scale {
        Scale::Discrete => (u16::from(buf[2].min(4)) * 25) as u8,
        Scale::Percent => buf[2].min(100),
    };
    Reading::Ok { percent, charging: status == 0x01 || status == 0x02 }
}

pub fn query(api: &HidApi) -> Reading {
    let Some((info, scale)) = api.device_list().find_map(|d| {
        let scale = scale_for(d.product_id())?;
        (d.vendor_id() == VID && d.usage_page() == USAGE_PAGE).then_some((d, scale))
    }) else {
        return Reading::Unavailable;
    };
    let Ok(dev) = info.open_device(api) else {
        return Reading::Unavailable;
    };
    if dev.write(&REQUEST).is_err() {
        return Reading::Unavailable;
    }
    let mut buf = [0u8; 128];
    match dev.read_timeout(&mut buf, TIMEOUT_MS) {
        Ok(n) if n > 0 => parse(&buf[..n], scale),
        _ => Reading::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_online_full() {
        assert_eq!(
            parse(&[0xB0, 0, 4, 3, 0, 0, 0, 0], Scale::Discrete),
            Reading::Ok { percent: 100, charging: false }
        );
    }

    #[test]
    fn parse_online_half() {
        assert_eq!(
            parse(&[0xB0, 0, 2, 3, 0, 0, 0, 0], Scale::Discrete),
            Reading::Ok { percent: 50, charging: false }
        );
    }

    #[test]
    fn parse_charging() {
        assert_eq!(
            parse(&[0xB0, 0, 1, 1, 0, 0, 0, 0], Scale::Discrete),
            Reading::Ok { percent: 25, charging: true }
        );
        assert_eq!(
            parse(&[0xB0, 0, 1, 2, 0, 0, 0, 0], Scale::Discrete),
            Reading::Ok { percent: 25, charging: true }
        );
    }

    #[test]
    fn parse_offline_is_disconnected() {
        assert_eq!(parse(&[0xB0, 0, 0, 0, 0, 0, 0, 0], Scale::Discrete), Reading::Disconnected);
    }

    #[test]
    fn parse_out_of_range_is_clamped() {
        assert_eq!(
            parse(&[0xB0, 0, 9, 3], Scale::Discrete),
            Reading::Ok { percent: 100, charging: false }
        );
        assert_eq!(
            parse(&[0xB0, 0, 140, 3], Scale::Percent),
            Reading::Ok { percent: 100, charging: false }
        );
    }

    #[test]
    fn parse_percent_firmware() {
        assert_eq!(
            parse(&[0xB0, 0, 67, 3], Scale::Percent),
            Reading::Ok { percent: 67, charging: false }
        );
    }

    #[test]
    fn parse_short_or_garbage_is_unavailable() {
        assert_eq!(parse(&[0xB0], Scale::Discrete), Reading::Unavailable);
        assert_eq!(parse(&[], Scale::Discrete), Reading::Unavailable);
    }

    #[test]
    fn pid_scales() {
        assert_eq!(scale_for(0x220A), Some(Scale::Discrete));
        assert_eq!(scale_for(0x22A7), Some(Scale::Percent));
        assert_eq!(scale_for(0x2202), None);
    }
}
