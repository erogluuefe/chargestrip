//! Cihazdan gelen okumayı ekranda gösterilecek hale çevirir. Donanımdan bağımsız, saf mantık.

/// Bir cihazın tek bir sorgu sonucu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reading {
    /// Pil okundu.
    Ok { percent: u8, charging: bool },
    /// Alıcı takılı ama cihaz kapalı / uykuda.
    Disconnected,
    /// Alıcı yok ya da okunamadı.
    Unavailable,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const WHITE: Rgb = Rgb(0xFF, 0xFF, 0xFF);
pub const RED: Rgb = Rgb(0xE5, 0x39, 0x35);
pub const GREEN: Rgb = Rgb(0x43, 0xA0, 0x47);
pub const GRAY: Rgb = Rgb(0x9E, 0x9E, 0x9E);

/// Bu değer ve altı kırmızı gösterilir.
pub const LOW_PERCENT: u8 = 15;

pub const DASH: &str = "–";

/// Simgede ve açıklamada gösterilecek şey.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Shown {
    pub text: String,
    pub color: Rgb,
    pub tooltip: String,
}

/// `last`: bu cihazdan en son başarıyla okunan yüzde (uykudayken gri gösterilir).
pub fn present(name: &str, r: &Reading, last: Option<u8>) -> Shown {
    match *r {
        Reading::Ok { percent, charging } => {
            let color = if charging {
                GREEN
            } else if percent <= LOW_PERCENT {
                RED
            } else {
                WHITE
            };
            let tooltip = if charging {
                format!("{name}: %{percent} · şarj oluyor")
            } else {
                format!("{name}: %{percent}")
            };
            Shown { text: percent.to_string(), color, tooltip }
        }
        Reading::Disconnected => match last {
            Some(p) => Shown {
                text: p.to_string(),
                color: GRAY,
                tooltip: format!("{name}: bağlı değil (son %{p})"),
            },
            None => Shown {
                text: DASH.into(),
                color: GRAY,
                tooltip: format!("{name}: bağlı değil"),
            },
        },
        Reading::Unavailable => Shown {
            text: DASH.into(),
            color: GRAY,
            tooltip: format!("{name}: alıcı bulunamadı"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(percent: u8, charging: bool) -> Reading {
        Reading::Ok { percent, charging }
    }

    #[test]
    fn present_ok_normal() {
        let s = present("Fare", &ok(82, false), None);
        assert_eq!((s.text.as_str(), s.color), ("82", WHITE));
        assert_eq!(s.tooltip, "Fare: %82");
    }

    #[test]
    fn present_low_is_red() {
        assert_eq!(present("Fare", &ok(15, false), None).color, RED);
        assert_eq!(present("Fare", &ok(0, false), None).color, RED);
        assert_eq!(present("Fare", &ok(16, false), None).color, WHITE);
    }

    #[test]
    fn present_charging_is_green_even_if_low() {
        let s = present("Kulaklık", &ok(5, true), None);
        assert_eq!(s.color, GREEN);
        assert_eq!(s.tooltip, "Kulaklık: %5 · şarj oluyor");
    }

    #[test]
    fn present_disconnected_with_last_shows_gray_last() {
        let s = present("Fare", &Reading::Disconnected, Some(40));
        assert_eq!((s.text.as_str(), s.color), ("40", GRAY));
        assert_eq!(s.tooltip, "Fare: bağlı değil (son %40)");
    }

    #[test]
    fn present_disconnected_without_last_is_dash() {
        let s = present("Kulaklık", &Reading::Disconnected, None);
        assert_eq!((s.text.as_str(), s.color), (DASH, GRAY));
        assert_eq!(s.tooltip, "Kulaklık: bağlı değil");
    }

    #[test]
    fn present_unavailable_is_gray_dash() {
        let s = present("Fare", &Reading::Unavailable, Some(40));
        assert_eq!((s.text.as_str(), s.color), (DASH, GRAY));
        assert_eq!(s.tooltip, "Fare: alıcı bulunamadı");
    }
}
