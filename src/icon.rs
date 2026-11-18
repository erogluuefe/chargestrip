//! 32×32 RGBA tepsi simgesi: sol üstte cihaz harfi (K/F), altta yüzde rakamları.
//! Yazı tipi dosyası yok; 3×5 piksel gömülü glifler. Her glife 1 px koyu dış çizgi
//! çekilir ki açık renkli taskbar'da da okunsun.

use crate::model::Rgb;

pub const SIZE: u32 = 32;
const S: usize = SIZE as usize;

/// Harfin ölçeği ve konumu (dış çizgi dahil 0..=11. satırlar).
const LETTER_SCALE: usize = 2;
const LETTER_X: usize = 1;
const LETTER_Y: usize = 1;
/// Rakamlar bu satırdan aşağıya yerleşir.
pub const DIGIT_TOP: usize = LETTER_Y + 5 * LETTER_SCALE + 1;
const MAX_DIGIT_SCALE: usize = 3;

const OUTLINE: [u8; 4] = [0, 0, 0, 200];

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
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
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

/// `n` karakter için (ölçek, aralık): dış çizgiyle birlikte genişliğe sığan en büyük ölçek.
fn layout(n: usize) -> (usize, usize) {
    let fits = |s: usize, gap: usize| n * 3 * s + n.saturating_sub(1) * gap + 2 <= S;
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

/// `SIZE*SIZE*4` baytlık RGBA görüntü döndürür.
pub fn render_icon(letter: char, text: &str, color: Rgb) -> Vec<u8> {
    let mut mask = vec![false; S * S];
    stamp(&mut mask, letter, LETTER_X, LETTER_Y, LETTER_SCALE);

    let chars: Vec<char> = text.chars().take(3).collect();
    if !chars.is_empty() {
        let (scale, gap) = layout(chars.len());
        let width = chars.len() * 3 * scale + (chars.len() - 1) * gap;
        let x0 = (S - width) / 2;
        let y0 = S - 1 - 5 * scale; // altta 1 satır dış çizgiye kalır
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
            } else {
                let near = (x > 0 && mask[y * S + x - 1])
                    || (x + 1 < S && mask[y * S + x + 1])
                    || (y > 0 && mask[(y - 1) * S + x])
                    || (y + 1 < S && mask[(y + 1) * S + x]);
                if near {
                    px.copy_from_slice(&OUTLINE);
                }
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

    /// Rakam bölgesinde, tam opak (glif) piksel içeren sütunlar.
    fn digit_cols(buf: &[u8]) -> Vec<usize> {
        (0..S)
            .filter(|&x| (DIGIT_TOP..S).any(|y| px(buf, x, y)[3] == 255))
            .collect()
    }

    fn runs(cols: &[usize]) -> usize {
        cols.windows(2).filter(|w| w[1] != w[0] + 1).count() + usize::from(!cols.is_empty())
    }

    #[test]
    fn render_size() {
        assert_eq!(render_icon('F', "82", WHITE).len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn render_100_fits() {
        let cols = digit_cols(&render_icon('F', "100", WHITE));
        assert!(cols[0] >= 1, "sol dış çizgiye yer kalmalı");
        assert!(*cols.last().unwrap() <= S - 2, "sağ dış çizgiye yer kalmalı");
        assert_eq!(runs(&cols), 3, "üç ayrı rakam görünmeli");
    }

    #[test]
    fn two_digits_use_max_scale() {
        // 2 hane ölçek 3: 9 + 2 + 9 = 20 px genişlik
        let cols = digit_cols(&render_icon('K', "88", WHITE));
        assert_eq!(cols.last().unwrap() - cols[0] + 1, 20);
    }

    #[test]
    fn single_digit_centered() {
        let cols = digit_cols(&render_icon('F', "5", WHITE));
        let (l, r) = (cols[0] as i32, (S - 1 - cols.last().unwrap()) as i32);
        assert!((l - r).abs() <= 1, "sol {l}, sağ {r}");
    }

    #[test]
    fn digits_do_not_touch_letter() {
        let buf = render_icon('K', "100", WHITE);
        // harfin dış çizgisi DIGIT_TOP'tan önce biter, rakamların glif pikselleri DIGIT_TOP'tan sonra başlar
        let first_digit_row = (DIGIT_TOP..S)
            .find(|&y| (0..S).any(|x| px(&buf, x, y)[3] == 255))
            .unwrap();
        assert!(first_digit_row > DIGIT_TOP);
    }

    #[test]
    fn glyph_has_dark_outline() {
        let buf = render_icon('K', "8", WHITE);
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
        let buf = render_icon('F', "8", RED);
        assert!(buf.chunks(4).any(|p| p == [0xE5, 0x39, 0x35, 255]));
    }

    #[test]
    fn dash_renders() {
        assert!(!digit_cols(&render_icon('F', "–", GRAY)).is_empty());
    }
}
