//! Device- and platform-independent model: the battery state a driver reads and how it should be
//! shown. Colors and symbols are chosen by the platform layer.

/// Device type; decides which icon is drawn.
/// A new type needs one line in `win::strip::glyph` and one in `mac::emoji`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Headset,
    Mouse,
    Keyboard,
    Gamepad,
    /// Device of unknown type (usually Bluetooth).
    Other,
}

/// Battery state reported by a driver after a successful read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Battery {
    /// 0–100
    pub percent: u8,
    /// Charging cable plugged in.
    pub charging: bool,
}

/// What the displayed value means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Normal,
    /// Battery at or below `LOW_PERCENT`.
    Low,
    Charging,
    /// Device off, asleep or unreadable; the last known value is shown if there is one.
    Inactive,
}

/// This level and below counts as low battery.
const LOW_PERCENT: u8 = 15;

/// How one device is shown.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Shown {
    pub kind: Kind,
    /// "75%", or "–" without a value.
    pub text: String,
    pub tone: Tone,
}

/// Without a `battery` reading (device off or asleep), `last`, the last level read in this session, is shown dimmed.
pub fn present(kind: Kind, battery: Option<Battery>, last: Option<u8>) -> Shown {
    let (percent, tone) = match battery {
        Some(b) if b.charging => (Some(b.percent), Tone::Charging),
        Some(b) if b.percent <= LOW_PERCENT => (Some(b.percent), Tone::Low),
        Some(b) => (Some(b.percent), Tone::Normal),
        None => (last, Tone::Inactive),
    };
    let text = percent.map_or_else(|| "–".to_string(), |p| format!("{p}%"));
    Shown { kind, text, tone }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(percent: u8, charging: bool) -> Option<Battery> {
        Some(Battery { percent, charging })
    }

    fn shown(text: &str, tone: Tone) -> Shown {
        Shown { kind: Kind::Mouse, text: text.into(), tone }
    }

    #[test]
    fn normal() {
        assert_eq!(present(Kind::Mouse, b(82, false), None), shown("82%", Tone::Normal));
    }

    #[test]
    fn low_threshold_is_inclusive() {
        assert_eq!(present(Kind::Mouse, b(15, false), None).tone, Tone::Low);
        assert_eq!(present(Kind::Mouse, b(0, false), None).tone, Tone::Low);
        assert_eq!(present(Kind::Mouse, b(16, false), None).tone, Tone::Normal);
    }

    #[test]
    fn charging_wins_over_low() {
        assert_eq!(present(Kind::Mouse, b(5, true), None), shown("5%", Tone::Charging));
    }

    #[test]
    fn inactive_shows_last_known() {
        assert_eq!(present(Kind::Mouse, None, Some(40)), shown("40%", Tone::Inactive));
    }

    #[test]
    fn inactive_without_history_is_dash() {
        assert_eq!(present(Kind::Mouse, None, None), shown("–", Tone::Inactive));
    }
}
