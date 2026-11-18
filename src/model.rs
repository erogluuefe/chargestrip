//! Cihazdan ve platformdan bağımsız veri modeli: sürücünün okuduğu pil bilgisi ve
//! bunun ekranda nasıl gösterileceği. Renk/sembol seçimi platform katmanındadır.

/// Cihaz türü; hangi ikonun çizileceğini belirler.
/// Yeni bir tür eklenirse `win::strip::glyph` ve `mac::emoji` eşlemelerine birer satır eklenir.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Headset,
    Mouse,
}

/// Bir sürücünün başarılı bir okumada bildirdiği pil durumu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Battery {
    /// 0–100
    pub percent: u8,
    /// Şarj kablosu takılı.
    pub charging: bool,
}

/// Gösterimin anlamı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Normal,
    /// Pil `LOW_PERCENT` veya altında.
    Low,
    Charging,
    /// Cihaz kapalı/uykuda ya da okunamadı; varsa son bilinen değer gösterilir.
    Inactive,
}

/// Bu değer ve altı düşük pil sayılır.
pub const LOW_PERCENT: u8 = 15;

/// Bir cihazın ekranda gösterilecek hali.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Shown {
    pub kind: Kind,
    /// "75%" ya da değer yoksa "–".
    pub text: String,
    pub tone: Tone,
}

/// `battery` yoksa (cihaz kapalı/uykuda) `last` — bu oturumda son okunan yüzde — soluk gösterilir.
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
