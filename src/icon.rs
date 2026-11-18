//! 32×32 RGBA tepsi simgesi: arkada soluk cihaz silüeti (kulaklık / fare),
//! önde büyük yüzde rakamları. Yazı tipi dosyası yok; 3×5 piksel gömülü glifler ve
//! basit geometriyle çizilen silüetler. Rakamlara 1 px koyu dış çizgi çekilir ki
//! açık ve koyu taskbar'da da okunsun.

use crate::model::Rgb;

pub const SIZE: u32 = 32;
const S: usize = SIZE as usize;

const MAX_DIGIT_SCALE: usize = 4;
const OUTLINE: [u8; 4] = [0, 0, 0, 200];
pub const SILHOUETTE: [u8; 4] = [160, 160, 160, 110];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Headset,
    Mouse,
}

/// 3×5 glif; her satır 3 bit, en soldaki piksel en yüksek bit.
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '–' | '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0b111, 0b001, 0b011, 0b000, 0b010], // '?'
    }
}

fn stamp(mask: &mut [bool], c: char, x0: usize, y0: usize, scale: usize) {
    for (row, bits) in glyph(c).iter().enumerate() {
        for col in 0..3 {
            if bits & (0b100 >> col) == 0 {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let (x, y) = (x0 + col * scale + dx, y0 + row * scale + dy);
                    if x < S && y < S {
                        mask[y * S + x] = true;
                    }
                }
            }
        }
    }
}

/// `n` karakter için (ölçek, aralık): dış çizgiyle birlikte sığan en büyük ölçek.
fn layout(n: usize) -> (usize, usize) {
    let fits = |s: usize, gap: usize| n * 3 * s + n.saturating_sub(1) * gap + 2 <= S && 5 * s + 2 <= S;
    for s in (1..=MAX_DIGIT_SCALE).rev() {
        let wide_gap = (s - 1).max(1);
        if fits(s, wide_gap) {
            return (s, wide_gap);
        }
        if fits(s, 1) {
            return (s, 1);
        }
    }
    (1, 1)
}

/// Piksel merkezinin silüet içinde olup olmadığı.
fn in_silhouette(kind: Kind, x: usize, y: usize) -> bool {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    match kind {
        Kind::Headset => {
            // Kafa bandı: üst yarım halka
            let (cx, cy) = (16.0, 17.0);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let band = py <= cy && (11.5..=15.0).contains(&d);
            // Kulaklıklar: iki yanda köşeleri kırpılmış dikdörtgen
            let cup = |x0: f32, x1: f32| {
                let inside = (x0..x1).contains(&px) && (15.0..30.0).contains(&py);
                let corner = (px < x0 + 1.0 || px > x1 - 1.0) && (py < 16.0 || py > 29.0);
                inside && !corner
            };
            band || cup(1.0, 8.0) || cup(24.0, 31.0)
        }
        Kind::Mouse => {
            // Gövde: elips; üstte iki tuşu ayıran boşluk
            let body = ((px - 16.0) / 10.5).powi(2) + ((py - 16.0) / 15.0).powi(2) <= 1.0;
            let split = (px - 16.0).abs() < 0.6 && py < 11.0;
            body && !split
        }
    }
}

/// `SIZE*SIZE*4` baytlık RGBA görüntü döndürür.
pub fn render_icon(kind: Kind, text: &str, color: Rgb) -> Vec<u8> {
    let mut mask = vec![false; S * S];
    let chars: Vec<char> = text.chars().take(3).collect();
    if !chars.is_empty() {
        let (scale, gap) = layout(chars.len());
        let width = chars.len() * 3 * scale + (chars.len() - 1) * gap;
        let x0 = (S - width) / 2;
        let y0 = (S - 5 * scale) / 2;
        for (i, &c) in chars.iter().enumerate() {
            stamp(&mut mask, c, x0 + i * (3 * scale + gap), y0, scale);
        }
    }

    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let px = &mut rgba[(y * S + x) * 4..][..4];
            if mask[y * S + x] {
                px.copy_from_slice(&[color.0, color.1, color.2, 255]);
                continue;
            }
            let near = (x > 0 && mask[y * S + x - 1])
                || (x + 1 < S && mask[y * S + x + 1])
                || (y > 0 && mask[(y - 1) * S + x])
                || (y + 1 < S && mask[(y + 1) * S + x]);
            if near {
                px.copy_from_slice(&OUTLINE);
            } else if in_silhouette(kind, x, y) {
                px.copy_from_slice(&SILHOUETTE);
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GRAY, RED, WHITE};

    fn px(buf: &[u8], x: usize, y: usize) -> [u8; 4] {
        buf[(y * S + x) * 4..][..4].try_into().unwrap()
    }

    /// Tam opak (rakam) piksel içeren sütunlar.
    fn digit_cols(buf: &[u8]) -> Vec<usize> {
        (0..S).filter(|&x| (0..S).any(|y| px(buf, x, y)[3] == 255)).collect()
    }

    fn digit_rows(buf: &[u8]) -> Vec<usize> {
        (0..S).filter(|&y| (0..S).any(|x| px(buf, x, y)[3] == 255)).collect()
    }

    fn runs(cols: &[usize]) -> usize {
        cols.windows(2).filter(|w| w[1] != w[0] + 1).count() + usize::from(!cols.is_empty())
    }

    #[test]
    fn render_size() {
        assert_eq!(render_icon(Kind::Mouse, "82", WHITE).len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn render_100_fits() {
        let cols = digit_cols(&render_icon(Kind::Mouse, "100", WHITE));
        assert!(cols[0] >= 1, "sol dış çizgiye yer kalmalı");
        assert!(*cols.last().unwrap() <= S - 2, "sağ dış çizgiye yer kalmalı");
        assert_eq!(runs(&cols), 3, "üç ayrı rakam görünmeli");
    }

    #[test]
    fn two_digits_are_big() {
        // 2 hane ölçek 4: 12 + 3 + 12 = 27 px genişlik, 20 px yükseklik
        let buf = render_icon(Kind::Headset, "88", WHITE);
        let cols = digit_cols(&buf);
        let rows = digit_rows(&buf);
        assert_eq!(cols.last().unwrap() - cols[0] + 1, 27);
        assert_eq!(rows.last().unwrap() - rows[0] + 1, 20);
    }

    #[test]
    fn digits_centered() {
        for text in ["5", "75", "100"] {
            let buf = render_icon(Kind::Mouse, text, WHITE);
            let (c, r) = (digit_cols(&buf), digit_rows(&buf));
            let h = (c[0] as i32 - (S - 1 - c.last().unwrap()) as i32).abs();
            let v = (r[0] as i32 - (S - 1 - r.last().unwrap()) as i32).abs();
            assert!(h <= 1 && v <= 1, "{text}: yatay fark {h}, dikey fark {v}");
        }
    }

    #[test]
    fn silhouettes_present_and_distinct() {
        let h = render_icon(Kind::Headset, "", WHITE);
        let m = render_icon(Kind::Mouse, "", WHITE);
        let count = |b: &[u8]| b.chunks(4).filter(|p| *p == SILHOUETTE).count();
        assert!(count(&h) > 100 && count(&m) > 100);
        assert_ne!(h, m);
        // kulaklığın ortası (bant ile kulaklıklar arası) boş, farenin ortası dolu
        assert_eq!(px(&h, 16, 22)[3], 0);
        assert_eq!(px(&m, 16, 22), SILHOUETTE);
    }

    #[test]
    fn digits_drawn_over_silhouette() {
        // ilk '8'in orta çizgisi (x=8, y=16) fare gövdesinin içinde; rakam silüetin önünde olmalı
        assert!(in_silhouette(Kind::Mouse, 8, 16));
        let buf = render_icon(Kind::Mouse, "88", WHITE);
        assert_eq!(px(&buf, 8, 16), [255, 255, 255, 255]);
    }

    #[test]
    fn glyph_has_dark_outline() {
        let buf = render_icon(Kind::Headset, "8", WHITE);
        let found = (1..S - 1).any(|y| {
            (1..S - 1).any(|x| {
                px(&buf, x, y) == [255, 255, 255, 255]
                    && [px(&buf, x - 1, y), px(&buf, x + 1, y), px(&buf, x, y - 1), px(&buf, x, y + 1)]
                        .contains(&OUTLINE)
            })
        });
        assert!(found);
    }

    #[test]
    fn color_applied() {
        let buf = render_icon(Kind::Mouse, "8", RED);
        assert!(buf.chunks(4).any(|p| p == [0xE5, 0x39, 0x35, 255]));
    }

    #[test]
    fn dash_renders() {
        assert!(!digit_cols(&render_icon(Kind::Mouse, "–", GRAY)).is_empty());
    }
}
