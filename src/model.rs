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
    /// One-line description for the tooltip, e.g. "SteelSeries Arctis Nova 7P: 64% — charging".
    pub tip: String,
}

/// Without a `battery` reading (device off or asleep), `last`, the last level read in this
/// session, is shown dimmed.
pub fn present(kind: Kind, name: &str, battery: Option<Battery>, last: Option<u8>) -> Shown {
    let (percent, tone) = match battery {
        Some(b) if b.charging => (Some(b.percent), Tone::Charging),
        Some(b) if b.percent <= LOW_PERCENT => (Some(b.percent), Tone::Low),
        Some(b) => (Some(b.percent), Tone::Normal),
        None => (last, Tone::Inactive),
    };
    let text = percent.map_or_else(|| "–".to_string(), |p| format!("{p}%"));
    let t = crate::i18n::t();
    let tip = match (tone, percent) {
        (Tone::Charging, _) => format!("{name}: {text} — {}", t.tip_charging),
        (Tone::Low, _) => format!("{name}: {text} — {}", t.tip_low),
        (Tone::Inactive, Some(p)) => format!("{name}: {} ({} {p}%)", t.tip_asleep, t.tip_last),
        (Tone::Inactive, None) => format!("{name}: {}", t.tip_asleep),
        (Tone::Normal, _) => format!("{name}: {text}"),
    };
    Shown { kind, text, tone, tip }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::t;

    fn b(percent: u8, charging: bool) -> Option<Battery> {
        Some(Battery { percent, charging })
    }

    fn show(battery: Option<Battery>, last: Option<u8>) -> Shown {
        present(Kind::Mouse, "Mouse X", battery, last)
    }

    #[test]
    fn normal() {
        let s = show(b(82, false), None);
        assert_eq!((s.text.as_str(), s.tone, s.tip.as_str()), ("82%", Tone::Normal, "Mouse X: 82%"));
    }

    #[test]
    fn low_threshold_is_inclusive() {
        assert_eq!(show(b(15, false), None).tone, Tone::Low);
        assert_eq!(show(b(0, false), None).tone, Tone::Low);
        assert_eq!(show(b(16, false), None).tone, Tone::Normal);
        assert_eq!(show(b(15, false), None).tip, format!("Mouse X: 15% — {}", t().tip_low));
    }

    #[test]
    fn charging_wins_over_low() {
        let s = show(b(5, true), None);
        assert_eq!((s.text.as_str(), s.tone), ("5%", Tone::Charging));
        assert_eq!(s.tip, format!("Mouse X: 5% — {}", t().tip_charging));
    }

    #[test]
    fn inactive_shows_last_known() {
        let s = show(None, Some(40));
        assert_eq!((s.text.as_str(), s.tone), ("40%", Tone::Inactive));
        assert_eq!(s.tip, format!("Mouse X: {} ({} 40%)", t().tip_asleep, t().tip_last));
    }

    #[test]
    fn inactive_without_history_is_dash() {
        let s = show(None, None);
        assert_eq!((s.text.as_str(), s.tone), ("–", Tone::Inactive));
        assert_eq!(s.tip, format!("Mouse X: {}", t().tip_asleep));
    }
}
