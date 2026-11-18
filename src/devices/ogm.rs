//! Waizowl OGM Pro V2 ("OGM 8K" alıcısı, Nordic 1915:0728) pil sorgusu.
//!
//! Protokol henüz bilinmiyor (plan Task 3: Waizowl yazılımının trafiği kaydedilip çözülecek,
//! `docs/fare-protokolu.md`). O zamana kadar fareye HİÇBİR paket gönderilmez; alıcı
//! takılıysa bile `Unavailable` döner ve simge "–" gösterir.

use crate::model::Reading;
use hidapi::HidApi;

pub const VID: u16 = 0x1915;
pub const PID: u16 = 0x0728;
pub const USAGE_PAGE: u16 = 0xFF0A;

pub fn query(_api: &HidApi) -> Reading {
    Reading::Unavailable
}
